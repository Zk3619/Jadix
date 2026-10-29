use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use winit::event::{ElementState, KeyEvent};

use crate::core::tree::{
    find_in_tree, find_in_tree_mut, pointer_events_targets_children, pointer_events_targets_self,
    CursorKind, CursorValue, Element, PointerEvents,
};
use crate::layout::layout_tree::portal_hit_passthrough;

type HoverCb = Rc<RefCell<dyn FnMut(bool)>>;

/// Resolve hover against freshly laid out bounds after a page structure change.
/// Never synthesize a leave with a sentinel pointer during a reactive rebuild.
pub(crate) fn sync_pointer_chrome_after_layout(
    root: &mut Element,
    layout_tree: &mut crate::layout::layout_tree::LayoutTree,
    mouse: [f32; 2],
    width: f32,
    height: f32,
) -> crate::core::tree::LayoutStatus {
    let mut impact =
        sync_pointer_component_chrome_after_layout(root, layout_tree, mouse, width, height);
    let focused =
        crate::core::reactivity::use_signal("__jadix_button_focus_id", String::new).peek();
    crate::style::sync_class_variants_subtree(
        root,
        mouse,
        &focused,
        crate::style::is_dark_theme(),
        width,
        height,
        layout_tree,
        &mut impact,
    );
    impact
}

/// Resolve component paint against final animated bounds without ticking CSS again.
pub(crate) fn sync_pointer_component_chrome_after_layout(
    root: &mut Element,
    layout_tree: &mut crate::layout::layout_tree::LayoutTree,
    mouse: [f32; 2],
    width: f32,
    height: f32,
) -> crate::core::tree::LayoutStatus {
    apply_pointer_hover_reconcile(root, mouse[0], mouse[1], true);
    let mut impact = crate::core::tree::LayoutStatus::Clean;

    impact = impact.merge(crate::authoring::presentation::sync_tree(
        root,
        layout_tree,
        crate::authoring::presentation::Context {
            phase: crate::authoring::presentation::Phase::Hover,
            viewport: [width, height],
            width_budget: None,
        },
    ));
    impact
}

/// Opt-in containers receive hover while an interactive descendant is targeted.
pub const ATTR_HOVER_REGION: &str = "jadix_hover_region";

/// Stable identity for exclusive hover tracking across tree rebuilds that replace handler `Rc`s.
#[derive(Clone, Debug, PartialEq, Eq)]
enum HoverKey {
    Focus(String),
    Template(String),
    /// No focus_id / template_id — fall back to handler pointer (rebuild still looks like a change).
    Anon(usize),
}

#[derive(Clone)]
struct HoverSlot {
    key: HoverKey,
    cb: HoverCb,
    /// Visual client box of the hoverable (scroll already applied). Chart tooltip
    /// samples datum from this + the pointer without requiring a mousemove.
    bounds: [f32; 4],
    regions: Vec<HoverSlot>,
}

fn push_exclusive_hover(slot: &HoverSlot, hovered: bool, cbs: &mut Vec<(HoverCb, bool)>) {
    if !slot.regions.iter().any(|region| region.key == slot.key) {
        cbs.push((slot.cb.clone(), hovered));
    }
}

fn hover_key_for(el: &Element, cb: &HoverCb) -> HoverKey {
    if let Some(fid) = el.focus_id.as_ref() {
        if !fid.is_empty() {
            return HoverKey::Focus(fid.clone());
        }
    }
    if let Some(tid) = el.template_id.as_ref() {
        if !tid.is_empty() {
            return HoverKey::Template(tid.clone());
        }
    }
    HoverKey::Anon(Rc::as_ptr(cb) as *const () as usize)
}

fn hover_slot_for(el: &Element, bounds: [f32; 4]) -> Option<HoverSlot> {
    let cb = el.on_hover.clone()?;
    Some(HoverSlot {
        key: hover_key_for(el, &cb),
        cb,
        bounds,
        regions: Vec::new(),
    })
}

thread_local! {
    /// Last exclusive `on_hover` target per window signal-key scope.
    /// Diff-based enter/leave avoids O(n) false callbacks every redraw (DevTools tree rows).
    static LAST_HOVER_BY_SCOPE: RefCell<HashMap<u64, Option<HoverSlot>>> =
        RefCell::new(HashMap::new());
    /// Set by [`invalidate_hover_handlers`]: next [`update_hovers`] must not assume `Rc` identity.
    static HOVER_NEEDS_RESYNC: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Consecutive hover misses while a previous target was active (1-frame hit-test jitter).
    static HOVER_MISS_STREAK: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
}

#[inline]
fn pointer_events_blocked(el: &Element) -> bool {
    !pointer_events_targets_children(el)
}

#[inline]
fn claims_own_bounds(el: &Element) -> bool {
    pointer_events_targets_self(el) && !portal_hit_passthrough(el)
}

/// 自身指针命中：paint AABB 粗筛，再按 `pointer-events` 细分。
///
/// HTML 盒（默认 `auto`）与无 path 的纹理 Icon 走变换后的局部 border box
///（[`crate::core::tree::pointer_in_paint_border_box`]），不是旋转后 AABB 的菱形空角。
/// SVG `bounding-box` 停在 AABB。Path fill/stroke / `clip-path` 逆 VS 仿射后测局部几何。
#[cfg(test)]
#[inline]
pub(crate) fn element_self_pointer_hit(el: &Element, x: f32, y: f32, scroll_acc: [f32; 2]) -> bool {
    element_self_pointer_hit_with(
        el,
        x,
        y,
        scroll_acc,
        el.paint_ancestor_linear,
        el.paint_ancestor_scale,
    )
}

#[inline]
pub(crate) fn element_self_pointer_hit_with(
    el: &Element,
    x: f32,
    y: f32,
    scroll_acc: [f32; 2],
    ancestor_linear: crate::style::Mat2d,
    ancestor_scale: [f32; 2],
) -> bool {
    if !claims_own_bounds(el) {
        return false;
    }
    let vis = crate::core::tree::visual_paint_aabb_with_ancestor(
        el,
        scroll_acc,
        ancestor_linear,
        ancestor_scale,
    );
    if x < vis[0] || x > vis[0] + vis[2] || y < vis[1] || y > vis[1] + vis[3] {
        return false;
    }
    let b = el.layout_bounds;
    if !crate::core::tree::clip_path_contains_pointer(
        el,
        [x, y],
        scroll_acc,
        ancestor_linear,
        ancestor_scale,
    ) {
        return false;
    }
    let mode = el.pointer_events_mode();
    if mode == PointerEvents::BoundingBox {
        if crate::core::tree::css_spin_locks_pointer_to_layout_box(el) {
            return crate::core::tree::pointer_in_paint_border_box(
                el,
                [x, y],
                scroll_acc,
                ancestor_linear,
                ancestor_scale,
            );
        }
        return true;
    }
    let has_path = crate::primitives::path::element_has_path_geometry(&el.attrs);
    // SVG leaf on CSS `auto` ≡ `visiblePainted` (MDN).
    let geom_mode = if mode == PointerEvents::Auto && has_path {
        PointerEvents::VisiblePainted
    } else if mode.uses_path_geometry() {
        mode
    } else {
        return crate::core::tree::pointer_in_paint_border_box(
            el,
            [x, y],
            scroll_acc,
            ancestor_linear,
            ancestor_scale,
        );
    };
    if geom_mode.requires_visibility() && el.style.opacity <= 0.0 {
        return false;
    }
    let Some(policy) = geom_mode.path_hit_policy() else {
        return crate::core::tree::pointer_in_paint_border_box(
            el,
            [x, y],
            scroll_acc,
            ancestor_linear,
            ancestor_scale,
        );
    };
    if !has_path {
        // 纯纹理 Icon / AnimatedIcon / MorphIcon：无 path 层时回退变换后的 border box。
        return crate::core::tree::pointer_in_paint_border_box(
            el,
            [x, y],
            scroll_acc,
            ancestor_linear,
            ancestor_scale,
        );
    }
    let local = crate::core::tree::pointer_to_layout_local(
        el,
        [x, y],
        scroll_acc,
        ancestor_linear,
        ancestor_scale,
    );
    crate::primitives::path::path_pointer_hit(&el.attrs, [b[2], b[3]], (local[0], local[1]), policy)
}

/// Preserve `input:` / `textarea:` focus through inner scroll viewports (`scroll:`).
#[inline]
fn inherit_text_edit_focus_id<'a>(el: &'a Element, inherited: Option<&'a str>) -> Option<&'a str> {
    if matches!(
        el.attrs.get("data-slot").map(String::as_str),
        Some("input-adornment-addon" | "input-adornment-text" | "input-adornment-button")
    ) {
        // The shell delegates focus, not editable text ownership. Otherwise a
        // trailing Kbd "K" wins the reverse walk and drag-select measures the
        // input value against the addon, selecting everything on a plain click.
        return None;
    }
    if let Some(id) = el.focus_id.as_deref() {
        if id.starts_with(crate::utils::text_field::INPUT_FOCUS_PREFIX)
            || id.starts_with(crate::utils::text_field::TEXTAREA_FOCUS_PREFIX)
        {
            return Some(id);
        }
    }
    inherited
}

pub(super) fn hit_test_child_indices(el: &Element) -> Vec<usize> {
    // Topmost child wins.
    // Order rule:
    // - body portal layer is above page children, regardless of authored z-index
    // - higher z_index is on top within the same layer
    // - within same z_index, later DOM siblings are on top
    let mut order: Vec<usize> = (0..el.children.len()).collect();
    order.sort_by(|&a, &b| {
        crate::layout::layout_tree::compare_sibling_paint_order(&el.children[a], &el.children[b])
            .then(a.cmp(&b))
    });
    order
}

#[inline]
pub(super) fn children_share_same_z_index(el: &Element) -> bool {
    let Some(first) = el.children.first() else {
        return true;
    };
    let z0 = first.style.z_index;
    el.children.iter().enumerate().all(|(i, c)| {
        c.style.z_index == z0
            && (i + 1 == el.children.len()
                || c.template_id.as_deref() != Some(crate::layout::layout_tree::PORTAL_LAYER_ID))
    })
}

enum HitPhase {
    Enter,
    After,
}

struct HitFrame<'a, Ctx> {
    el: &'a Element,
    scroll_acc: [f32; 2],
    ctx: Ctx,
    ancestor_linear: crate::style::Mat2d,
    ancestor_scale: [f32; 2],
    phase: HitPhase,
}

fn push_z_hit_children<'a, Ctx: Copy>(
    stack: &mut Vec<HitFrame<'a, Ctx>>,
    el: &'a Element,
    next_acc: [f32; 2],
    child_ctx: Ctx,
    ancestor_linear: crate::style::Mat2d,
    ancestor_scale: [f32; 2],
) {
    let (child_lin, child_sc) =
        crate::core::tree::child_paint_ancestor(el, ancestor_linear, ancestor_scale);
    if children_share_same_z_index(el) {
        for child in &el.children {
            stack.push(HitFrame {
                el: child,
                scroll_acc: next_acc,
                ctx: child_ctx,
                ancestor_linear: child_lin,
                ancestor_scale: child_sc,
                phase: HitPhase::Enter,
            });
        }
    } else {
        for i in hit_test_child_indices(el) {
            stack.push(HitFrame {
                el: &el.children[i],
                scroll_acc: next_acc,
                ctx: child_ctx,
                ancestor_linear: child_lin,
                ancestor_scale: child_sc,
                phase: HitPhase::Enter,
            });
        }
    }
}

/// Heap DFS hit-test. `enter` returns `(escaped_acc, next_acc, child_ctx, descend)` or `None`
/// to skip the node. After children, `after` sees the first decisive child hit.
fn hit_walk_layer<'a, T, Ctx: Copy>(
    root: &'a Element,
    scroll_acc: [f32; 2],
    ctx: Ctx,
    mut enter: impl FnMut(
        &'a Element,
        [f32; 2],
        Ctx,
        crate::style::Mat2d,
        [f32; 2],
    ) -> Option<([f32; 2], [f32; 2], Ctx, bool)>,
    mut after: impl FnMut(
        &'a Element,
        [f32; 2],
        Ctx,
        crate::style::Mat2d,
        [f32; 2],
        LayerHit<T>,
    ) -> LayerHit<T>,
) -> LayerHit<T> {
    let mut stack = vec![HitFrame {
        el: root,
        scroll_acc,
        ctx,
        ancestor_linear: crate::style::MAT2D_IDENTITY,
        ancestor_scale: [1.0, 1.0],
        phase: HitPhase::Enter,
    }];
    let mut result = LayerHit::Miss;
    while let Some(frame) = stack.pop() {
        match frame.phase {
            HitPhase::Enter => {
                if !matches!(result, LayerHit::Miss) {
                    continue;
                }
                let Some((escaped, next_acc, child_ctx, descend)) = enter(
                    frame.el,
                    frame.scroll_acc,
                    frame.ctx,
                    frame.ancestor_linear,
                    frame.ancestor_scale,
                ) else {
                    continue;
                };
                stack.push(HitFrame {
                    el: frame.el,
                    scroll_acc: escaped,
                    ctx: frame.ctx,
                    ancestor_linear: frame.ancestor_linear,
                    ancestor_scale: frame.ancestor_scale,
                    phase: HitPhase::After,
                });
                if descend {
                    push_z_hit_children(
                        &mut stack,
                        frame.el,
                        next_acc,
                        child_ctx,
                        frame.ancestor_linear,
                        frame.ancestor_scale,
                    );
                }
            }
            HitPhase::After => {
                result = after(
                    frame.el,
                    frame.scroll_acc,
                    frame.ctx,
                    frame.ancestor_linear,
                    frame.ancestor_scale,
                    result,
                );
            }
        }
    }
    result
}

fn hit_walk_first<'a, T, Ctx: Copy>(
    root: &'a Element,
    scroll_acc: [f32; 2],
    ctx: Ctx,
    enter: impl FnMut(
        &'a Element,
        [f32; 2],
        Ctx,
        crate::style::Mat2d,
        [f32; 2],
    ) -> Option<([f32; 2], [f32; 2], Ctx)>,
    after: impl FnMut(&'a Element, [f32; 2], Ctx, crate::style::Mat2d, [f32; 2], Option<T>) -> Option<T>,
) -> Option<T> {
    hit_walk_first_from(
        root,
        scroll_acc,
        ctx,
        crate::style::MAT2D_IDENTITY,
        [1.0, 1.0],
        enter,
        after,
    )
}

fn hit_walk_first_from<'a, T, Ctx: Copy>(
    root: &'a Element,
    scroll_acc: [f32; 2],
    ctx: Ctx,
    ancestor_linear: crate::style::Mat2d,
    ancestor_scale: [f32; 2],
    mut enter: impl FnMut(
        &'a Element,
        [f32; 2],
        Ctx,
        crate::style::Mat2d,
        [f32; 2],
    ) -> Option<([f32; 2], [f32; 2], Ctx)>,
    mut after: impl FnMut(
        &'a Element,
        [f32; 2],
        Ctx,
        crate::style::Mat2d,
        [f32; 2],
        Option<T>,
    ) -> Option<T>,
) -> Option<T> {
    let mut stack = vec![HitFrame {
        el: root,
        scroll_acc,
        ctx,
        ancestor_linear,
        ancestor_scale,
        phase: HitPhase::Enter,
    }];
    let mut result = None;
    while let Some(frame) = stack.pop() {
        match frame.phase {
            HitPhase::Enter => {
                if result.is_some() {
                    continue;
                }
                let Some((escaped, next_acc, child_ctx)) = enter(
                    frame.el,
                    frame.scroll_acc,
                    frame.ctx,
                    frame.ancestor_linear,
                    frame.ancestor_scale,
                ) else {
                    continue;
                };
                stack.push(HitFrame {
                    el: frame.el,
                    scroll_acc: escaped,
                    ctx: frame.ctx,
                    ancestor_linear: frame.ancestor_linear,
                    ancestor_scale: frame.ancestor_scale,
                    phase: HitPhase::After,
                });
                push_z_hit_children(
                    &mut stack,
                    frame.el,
                    next_acc,
                    child_ctx,
                    frame.ancestor_linear,
                    frame.ancestor_scale,
                );
            }
            HitPhase::After => {
                result = after(
                    frame.el,
                    frame.scroll_acc,
                    frame.ctx,
                    frame.ancestor_linear,
                    frame.ancestor_scale,
                    result,
                );
            }
        }
    }
    result
}

fn hit_enter_clipped(
    el: &Element,
    x: f32,
    y: f32,
    scroll_acc: [f32; 2],
    ancestor_linear: crate::style::Mat2d,
    ancestor_scale: [f32; 2],
) -> Option<([f32; 2], [f32; 2])> {
    if pointer_events_blocked(el) {
        return None;
    }
    let escaped = escape_scroll_acc(el, scroll_acc);
    let is_body_portal =
        el.template_id.as_deref() == Some(crate::layout::layout_tree::PORTAL_LAYER_ID);
    if !is_body_portal
        && !crate::core::tree::overflow_clip_contains_pointer(
            el,
            [x, y],
            escaped,
            ancestor_linear,
            ancestor_scale,
        )
    {
        return None;
    }
    if !crate::core::tree::clip_path_contains_pointer(
        el,
        [x, y],
        escaped,
        ancestor_linear,
        ancestor_scale,
    ) {
        return None;
    }
    let next_acc = if is_body_portal {
        escaped
    } else {
        next_hit_scroll_acc(el, escaped)
    };
    Some((escaped, next_acc))
}

/// Consistent with the string shown in [`crate::components::input`]: `preedit` is inserted at each live caret.
#[cfg(test)]
pub(super) fn input_display_text(
    committed: &str,
    preedit: &str,
    ime_anchor: Option<usize>,
) -> String {
    if preedit.is_empty() {
        return committed.to_string();
    }
    let a = ime_anchor.unwrap_or(committed.len());
    crate::utils::text_multi_cursor::splice_preedit(committed, &[a], preedit)
}

/// Pointer hit on a stacking layer.
///
/// - [`LayerHit::Miss`]: pointer missed — callers may try siblings.
/// - [`LayerHit::Hit`]: pointer hit this layer. `None` payload means an occluder
///   (e.g. modal underlay) with no focus/cursor/`on_hover` of its own; ancestors may
///   still bubble a payload, but lower siblings must not be searched.
#[derive(Clone, Debug)]
enum LayerHit<T> {
    Miss,
    Hit(Option<T>),
}

impl<T> LayerHit<T> {
    fn bubble(self, parent: Option<T>) -> Self {
        match self {
            Self::Miss => Self::Miss,
            Self::Hit(v) => Self::Hit(v.or(parent)),
        }
    }

    fn into_option(self) -> Option<T> {
        match self {
            Self::Miss => None,
            Self::Hit(v) => v,
        }
    }
}

#[inline]
pub(crate) fn escape_scroll_acc(el: &Element, scroll_acc: [f32; 2]) -> [f32; 2] {
    if el.template_id.as_deref() == Some(crate::layout::layout_tree::PORTAL_LAYER_ID)
        || el.style.position_fixed
    {
        [0.0_f32, 0.0_f32]
    } else {
        scroll_acc
    }
}

/// Compute the visual top-left of an element for hit testing, accounting for sticky offset
/// and CSS translate / offset-path (same contributions as layout collect `render_*`).
#[inline]
pub(crate) fn visual_xy(el: &Element, scroll_acc: [f32; 2]) -> [f32; 2] {
    let b = el.layout_bounds;
    let offset = crate::style::offset_path::sample(&el.style, [b[2], b[3]]);
    let tx = el.style.translate[0] + b[2] * el.style.translate_pct[0] + offset.translate[0];
    let ty = el.style.translate[1] + b[3] * el.style.translate_pct[1] + offset.translate[1];
    [
        b[0] - scroll_acc[0] + el.sticky_visual_offset[0] + tx,
        b[1] - scroll_acc[1] + el.sticky_visual_offset[1] + ty,
    ]
}

/// Paint-space hit box after CSS scale / rotate / origin / offset-path.
///
/// Same formula as [`crate::core::tree::visual_paint_aabb`] so click / `on_hover`
/// stay on the ink `:hover` already uses.
#[cfg(test)]
#[inline]
pub(crate) fn visual_hit_box(el: &Element, scroll_acc: [f32; 2]) -> [f32; 4] {
    crate::core::tree::visual_paint_aabb(el, scroll_acc)
}

/// Pointer-space **layout slot**: scroll + sticky, no CSS `translate` / `scale`.
///
/// Drag collision and sortable insert-index must use this. Preview shift writes
/// `translate` on siblings; if hits follow the moving paint boxes, `hover_index`
/// chases them and live reordering collapses.
#[inline]
pub(crate) fn collision_hit_box(el: &Element, scroll_acc: [f32; 2]) -> [f32; 4] {
    let b = el.layout_bounds;
    [
        b[0] - scroll_acc[0] + el.sticky_visual_offset[0],
        b[1] - scroll_acc[1] + el.sticky_visual_offset[1],
        b[2],
        b[3],
    ]
}

/// Visual border box for hit-testing / inspector pick.
/// Prefers the last collect paint rect when it is still current (translate/sticky/scale).
#[cfg(all(debug_assertions, feature = "devtools"))]
#[inline]
pub(super) fn visual_border_box(el: &Element, scroll_acc: [f32; 2]) -> [f32; 4] {
    if el.paint_visual_gen != 0
        && el.paint_visual_gen == crate::app::devtools::highlight_paint_gen()
    {
        return el.paint_visual_bounds;
    }
    crate::core::tree::visual_paint_aabb(el, scroll_acc)
}

/// Same scroll contribution rule as layout collect: `scroll:` viewports always
/// add offset even when overflow was cleared to Visible.
/// `overflow: hidden`/`clip` do **not** scroll — only Scroll/Auto (and `scroll:` ids).
#[inline]
pub(crate) fn next_hit_scroll_acc(el: &Element, scroll_acc: [f32; 2]) -> [f32; 2] {
    let is_scroll_viewport = el
        .focus_id
        .as_deref()
        .is_some_and(|f| f.starts_with(crate::core::scroll_viewport::SCROLL_VIEWPORT_FOCUS_PREFIX));
    let mut next = scroll_acc;
    let ox = el.style.overflow.x;
    let oy = el.style.overflow.y;
    let scroll_x = is_scroll_viewport || matches!(ox, taffy::style::Overflow::Scroll);
    let scroll_y = is_scroll_viewport || matches!(oy, taffy::style::Overflow::Scroll);
    if scroll_x {
        next[0] +=
            crate::core::scroll_viewport::scroll_viewport_axis_offset(el, is_scroll_viewport, true);
    }
    if scroll_y {
        next[1] += crate::core::scroll_viewport::scroll_viewport_axis_offset(
            el,
            is_scroll_viewport,
            false,
        );
    }
    next
}

pub(super) fn pick_cursor(
    el: &Element,
    x: f32,
    y: f32,
    scroll_acc: [f32; 2],
) -> Option<CursorValue> {
    pick_cursor_hit(el, x, y, scroll_acc).into_option()
}

fn pick_cursor_hit(el: &Element, x: f32, y: f32, scroll_acc: [f32; 2]) -> LayerHit<CursorValue> {
    hit_walk_layer(
        el,
        scroll_acc,
        (),
        |el, scroll_acc, (), lin, sc| {
            let (escaped, next_acc) = hit_enter_clipped(el, x, y, scroll_acc, lin, sc)?;
            Some((escaped, next_acc, (), true))
        },
        |el, scroll_acc, (), lin, sc, child_hit| {
            let bubbled = child_hit.bubble(el.cursor.clone());
            if !matches!(bubbled, LayerHit::Miss) {
                return bubbled;
            }
            if element_self_pointer_hit_with(el, x, y, scroll_acc, lin, sc) {
                LayerHit::Hit(el.cursor.clone())
            } else {
                LayerHit::Miss
            }
        },
    )
}

pub(super) fn apply_cursor(
    window: &dyn winit::window::Window,
    event_loop: Option<&dyn winit::event_loop::ActiveEventLoop>,
    cache: &mut crate::style::CustomCursorCache,
    value: Option<CursorValue>,
) {
    crate::style::cursor_image::apply_cursor(
        window,
        event_loop,
        cache,
        crate::utils::text_sel_move::active_cursor()
            .map(CursorValue::from)
            .or(value),
    );
}

/// Hit-test cursor unless a pointer session owns the OS icon.
///
/// Drag overlay uses `pointer-events: none`, so `pick_cursor` misses (Default) while
/// the pointer is over empty chrome. Redraw used to apply that miss every frame,
/// fighting `Grabbing` on mousemove — Windows shows that as 移动指针 ↔ 默认指针 flicker.
pub(super) fn session_owned_cursor(
    hit: Option<CursorValue>,
    nwse_resize: bool,
    grabbing: bool,
) -> Option<CursorValue> {
    if nwse_resize {
        Some(CursorKind::NwseResize.into())
    } else if let Some(cursor) = crate::utils::pointer_action::active_resize_cursor() {
        Some(cursor)
    } else if grabbing {
        Some(CursorKind::Grabbing.into())
    } else {
        hit
    }
}

pub(super) fn apply_session_cursor(
    window: &dyn winit::window::Window,
    event_loop: Option<&dyn winit::event_loop::ActiveEventLoop>,
    cache: &mut crate::style::CustomCursorCache,
    hit: Option<CursorValue>,
    nwse_resize: bool,
    grabbing: bool,
) {
    apply_cursor(
        window,
        event_loop,
        cache,
        session_owned_cursor(hit, nwse_resize, grabbing),
    );
}

pub(super) fn find_bounds_by_focus_id(
    el: &Element,
    focus_id: &str,
    scroll_acc: [f32; 2],
) -> Option<([f32; 4], [f32; 2])> {
    // Prefer the match under the largest ancestor scroll (duplicate focus_id ghosts outside
    // ScrollArea would otherwise win DFS with scroll_acc=0 and freeze IME candidates).
    let mut best: Option<([f32; 4], [f32; 2])> = None;
    find_bounds_by_focus_id_best(el, focus_id, scroll_acc, &mut best);
    best
}

fn find_bounds_by_focus_id_best(
    el: &Element,
    focus_id: &str,
    scroll_acc: [f32; 2],
    best: &mut Option<([f32; 4], [f32; 2])>,
) {
    let mut stack: Vec<(&Element, [f32; 2])> = vec![(el, scroll_acc)];
    while let Some((el, scroll_acc)) = stack.pop() {
        if el.focus_id.as_deref() == Some(focus_id) {
            let score = scroll_acc[0].abs() + scroll_acc[1].abs();
            let replace = match best {
                None => true,
                Some((_, sa)) => score > sa[0].abs() + sa[1].abs() + 0.5,
            };
            if replace {
                *best = Some((el.layout_bounds, scroll_acc));
            }
        }
        let next_acc = next_hit_scroll_acc(el, scroll_acc);
        for c in el.children.iter().rev() {
            stack.push((c, next_acc));
        }
    }
}

/// World-space IME caret origin on the painted field. `caret_local` is in the
/// element's layout box (not the paint AABB). Do not `bind_caret`.
pub(super) fn find_paint_ime_caret(
    el: &Element,
    focus_id: &str,
    caret_local: [f32; 2],
    scroll_acc: [f32; 2],
) -> Option<[f32; 2]> {
    let mut best: Option<[f32; 2]> = None;
    let mut best_score = -1.0_f32;
    let mut stack: Vec<(&Element, [f32; 2], crate::style::Mat2d, [f32; 2])> =
        vec![(el, scroll_acc, crate::style::MAT2D_IDENTITY, [1.0, 1.0])];
    while let Some((el, scroll_acc, lin, sc)) = stack.pop() {
        if el.focus_id.as_deref() == Some(focus_id) {
            let score = scroll_acc[0].abs() + scroll_acc[1].abs();
            if best.is_none() || score > best_score + 0.5 {
                best_score = score;
                best = Some(crate::core::tree::layout_local_to_paint_world(
                    el,
                    caret_local,
                    scroll_acc,
                    lin,
                    sc,
                ));
            }
        }
        let next_acc = next_hit_scroll_acc(el, scroll_acc);
        let (child_lin, child_sc) = crate::core::tree::child_paint_ancestor(el, lin, sc);
        for c in el.children.iter().rev() {
            stack.push((c, next_acc, child_lin, child_sc));
        }
    }
    best
}

/// World-space paint AABB + `scroll_acc` for `focus_id` (IME fallback / keep-focus).
/// The AABB already subtracts ancestor scroll — do not subtract `scroll_acc` again.
pub fn find_paint_aabb_by_focus_id(
    el: &Element,
    focus_id: &str,
    scroll_acc: [f32; 2],
) -> Option<([f32; 4], [f32; 2])> {
    let mut best: Option<([f32; 4], [f32; 2])> = None;
    let mut best_score = -1.0_f32;
    let mut stack: Vec<(&Element, [f32; 2], crate::style::Mat2d, [f32; 2], bool)> = vec![(
        el,
        scroll_acc,
        crate::style::MAT2D_IDENTITY,
        [1.0, 1.0],
        false,
    )];
    while let Some((el, scroll_acc, lin, sc, escaped_page)) = stack.pop() {
        // Match hit_path: portal / `position: fixed` escape ancestor scroll so a
        // hoisted ColorPicker SV is not mapped with page offset (ty→1 / V→0).
        let is_body_portal =
            el.template_id.as_deref() == Some(crate::layout::layout_tree::PORTAL_LAYER_ID);
        let escaped_page = escaped_page || is_body_portal || el.style.position_fixed;
        let scroll_acc = if is_body_portal || el.style.position_fixed {
            [0.0_f32, 0.0_f32]
        } else {
            scroll_acc
        };
        if el.focus_id.as_deref() == Some(focus_id) {
            // Overlay copies sit under the portal / fixed ancestor (scroll_acc=0).
            // Prefer them over in-flow ghosts that still carry page scroll.
            let score = if escaped_page {
                1.0e9_f32
            } else {
                scroll_acc[0].abs() + scroll_acc[1].abs()
            };
            if best.is_none() || score > best_score + 0.5 {
                best_score = score;
                let aabb =
                    crate::core::tree::visual_paint_aabb_with_ancestor(el, scroll_acc, lin, sc);
                best = Some((aabb, scroll_acc));
            }
        }
        let next_acc = if is_body_portal {
            scroll_acc
        } else {
            next_hit_scroll_acc(el, scroll_acc)
        };
        let (child_lin, child_sc) = crate::core::tree::child_paint_ancestor(el, lin, sc);
        for c in el.children.iter().rev() {
            stack.push((c, next_acc, child_lin, child_sc, escaped_page));
        }
    }
    best
}

/// Hit entry for a captured `focus_id` using the painted window-space AABB.
/// `layout_bounds` are document space and glue ColorPicker SV mapping (V→0 / V→1).
pub(super) fn captured_pointer_hit(
    root: &Element,
    focus_id: &str,
) -> Option<crate::app::pointer_events::HitPathEntry> {
    let el = crate::app::pointer_events::find_by_focus_id(root, focus_id)?;
    let aabb = find_paint_aabb_by_focus_id(root, focus_id, [0.0, 0.0])
        .map(|(a, _)| a)
        .unwrap_or(el.layout_bounds);
    Some(crate::app::pointer_events::HitPathEntry::from_element(
        el, aabb,
    ))
}

/// True when the world pointer sits in the painted border box of `focus_id`.
pub(super) fn pointer_hits_focus_paint(
    root: &Element,
    focus_id: &str,
    x: f32,
    y: f32,
    scroll_acc: [f32; 2],
) -> bool {
    hit_walk_first(
        root,
        scroll_acc,
        (),
        |el, scroll_acc, (), _lin, _sc| Some((scroll_acc, next_hit_scroll_acc(el, scroll_acc), ())),
        |el, scroll_acc, (), lin, sc, child_hit| {
            if child_hit == Some(true) {
                return Some(true);
            }
            if el.focus_id.as_deref() == Some(focus_id)
                && crate::core::tree::pointer_in_paint_border_box(el, [x, y], scroll_acc, lin, sc)
            {
                Some(true)
            } else {
                child_hit
            }
        },
    )
    .unwrap_or(false)
}

pub(crate) fn find_focus_id_at(
    el: &Element,
    x: f32,
    y: f32,
    scroll_acc: [f32; 2],
) -> Option<String> {
    find_focus_id_at_hit(el, x, y, scroll_acc).into_option()
}

fn find_focus_id_at_hit(el: &Element, x: f32, y: f32, scroll_acc: [f32; 2]) -> LayerHit<String> {
    hit_walk_layer(
        el,
        scroll_acc,
        (),
        |el, scroll_acc, (), lin, sc| {
            let (escaped, next_acc) = hit_enter_clipped(el, x, y, scroll_acc, lin, sc)?;
            Some((escaped, next_acc, (), true))
        },
        |el, scroll_acc, (), lin, sc, child_hit| {
            let bubbled = child_hit.bubble(el.focus_id.clone());
            if !matches!(bubbled, LayerHit::Miss) {
                return bubbled;
            }
            if element_self_pointer_hit_with(el, x, y, scroll_acc, lin, sc) {
                LayerHit::Hit(el.focus_id.clone())
            } else {
                LayerHit::Miss
            }
        },
    )
}

/// 命中最上层含 `input:` 继承焦点的文本叶（用于拖选时按 `rest` 查找布局与样式）。
///
/// 返回的 bounds 为**屏幕/可视坐标**（已扣祖先 `scroll_acc`），与
/// [`find_input_text_at`] 及指针 `(x,y)` 同一空间。切勿返回未滚动的
/// `layout_bounds`，否则 ScrollArea 内拖选会把 `my` clamp 到错误行，表现为
/// "Click one word but select across lines."
pub(super) fn find_input_text_leaf_for_rest(
    el: &Element,
    inherited_fid: Option<&str>,
    rest: &str,
) -> Option<([f32; 4], String, crate::style::TextStyle, [f32; 2])> {
    find_input_text_leaf_for_rest_acc(el, inherited_fid, rest, [0.0, 0.0])
}

fn find_input_text_leaf_for_rest_acc(
    el: &Element,
    text_edit_fid: Option<&str>,
    rest: &str,
    scroll_acc: [f32; 2],
) -> Option<([f32; 4], String, crate::style::TextStyle, [f32; 2])> {
    hit_walk_first(
        el,
        scroll_acc,
        text_edit_fid,
        |el, scroll_acc, text_edit_fid, _lin, _sc| {
            let next_acc = next_hit_scroll_acc(el, scroll_acc);
            let child_fid = inherit_text_edit_focus_id(el, text_edit_fid);
            Some((scroll_acc, next_acc, child_fid))
        },
        |el, scroll_acc, text_edit_fid, _lin, _sc, child_hit| {
            if child_hit.is_some() {
                return child_hit;
            }
            let child_fid = inherit_text_edit_focus_id(el, text_edit_fid);
            if let Some(text) = &el.text {
                if el.text_style.font_family.as_deref() == Some("lucide") {
                    return None;
                }
                if let Some(id) = child_fid {
                    if id == format!("{}{}", crate::utils::text_field::INPUT_FOCUS_PREFIX, rest)
                        || id
                            == format!(
                                "{}{}",
                                crate::utils::text_field::TEXTAREA_FOCUS_PREFIX,
                                rest
                            )
                    {
                        let b = el.layout_bounds;
                        let [vx, vy] = visual_xy(el, scroll_acc);
                        return Some((
                            [vx, vy, b[2], b[3]],
                            text.clone(),
                            el.text_style.as_ref().clone(),
                            el.scroll_offset,
                        ));
                    }
                }
            }
            None
        },
    )
}

/// 指针 `(x,y)` 是否落在某 `input:` / `textarea:` 文本叶上。
/// 返回 `(rest, bounds, display, style, scroll, focus_id)`。
pub(super) fn find_input_text_at(
    el: &Element,
    text_edit_fid: Option<&str>,
    x: f32,
    y: f32,
    scroll_acc: [f32; 2],
) -> Option<(
    String,
    [f32; 4],
    String,
    crate::style::TextStyle,
    [f32; 2],
    String,
)> {
    find_input_text_at_hit(el, text_edit_fid, x, y, scroll_acc).into_option()
}

fn find_input_text_at_hit(
    el: &Element,
    text_edit_fid: Option<&str>,
    x: f32,
    y: f32,
    scroll_acc: [f32; 2],
) -> LayerHit<(
    String,
    [f32; 4],
    String,
    crate::style::TextStyle,
    [f32; 2],
    String,
)> {
    type Hit = (
        String,
        [f32; 4],
        String,
        crate::style::TextStyle,
        [f32; 2],
        String,
    );
    hit_walk_layer(
        el,
        scroll_acc,
        text_edit_fid,
        |el, scroll_acc, text_edit_fid, lin, sc| {
            let (escaped, next_acc) = hit_enter_clipped(el, x, y, scroll_acc, lin, sc)?;
            let child_fid = inherit_text_edit_focus_id(el, text_edit_fid);
            Some((escaped, next_acc, child_fid, true))
        },
        |el, scroll_acc, text_edit_fid, lin, sc, child_hit: LayerHit<Hit>| {
            if !matches!(child_hit, LayerHit::Miss) {
                return child_hit;
            }
            let child_text_edit_fid = inherit_text_edit_focus_id(el, text_edit_fid);
            let b = el.layout_bounds;
            let [vx, vy] = visual_xy(el, scroll_acc);
            let inside =
                crate::core::tree::pointer_in_paint_border_box(el, [x, y], scroll_acc, lin, sc);
            if let Some(text) = &el.text {
                if el.text_style.font_family.as_deref() != Some("lucide") {
                    if let Some(id) = child_text_edit_fid {
                        let rest = id
                            .strip_prefix(crate::utils::text_field::INPUT_FOCUS_PREFIX)
                            .or_else(|| {
                                id.strip_prefix(crate::utils::text_field::TEXTAREA_FOCUS_PREFIX)
                            });
                        if let Some(rest) = rest {
                            if inside {
                                return LayerHit::Hit(Some((
                                    rest.to_string(),
                                    [vx, vy, b[2], b[3]],
                                    text.clone(),
                                    el.text_style.as_ref().clone(),
                                    el.scroll_offset,
                                    id.to_string(),
                                )));
                            }
                        }
                    }
                }
            }
            let field_id = el
                .focus_id
                .as_deref()
                .filter(|id| is_text_edit_focus_id(id))
                .or(text_edit_fid.filter(|id| is_text_edit_focus_id(id)));
            if let Some(id) = field_id {
                let rest = id
                    .strip_prefix(crate::utils::text_field::INPUT_FOCUS_PREFIX)
                    .or_else(|| id.strip_prefix(crate::utils::text_field::TEXTAREA_FOCUS_PREFIX));
                if let Some(rest) = rest {
                    if inside && !pointer_on_text_edit_adornment(el, x, y, scroll_acc, lin, sc) {
                        if let Some((bounds, text, style, scroll)) =
                            find_input_text_leaf_for_rest_acc(el, Some(id), rest, scroll_acc)
                        {
                            return LayerHit::Hit(Some((
                                rest.to_string(),
                                bounds,
                                text,
                                style,
                                scroll,
                                id.to_string(),
                            )));
                        }
                    }
                }
            }
            if element_self_pointer_hit_with(el, x, y, scroll_acc, lin, sc) {
                LayerHit::Hit(None)
            } else {
                LayerHit::Miss
            }
        },
    )
}

fn is_text_edit_focus_id(id: &str) -> bool {
    id.starts_with(crate::utils::text_field::INPUT_FOCUS_PREFIX)
        || id.starts_with(crate::utils::text_field::TEXTAREA_FOCUS_PREFIX)
}

/// True when `(x,y)` is on a clear/reveal control inside a text-edit chrome.
fn pointer_on_text_edit_adornment(
    el: &Element,
    x: f32,
    y: f32,
    scroll_acc: [f32; 2],
    ancestor_linear: crate::style::Mat2d,
    ancestor_scale: [f32; 2],
) -> bool {
    hit_walk_first_from(
        el,
        scroll_acc,
        (),
        ancestor_linear,
        ancestor_scale,
        |el, scroll_acc, (), _lin, _sc| {
            if pointer_events_blocked(el) {
                return None;
            }
            Some((scroll_acc, next_hit_scroll_acc(el, scroll_acc), ()))
        },
        |el, scroll_acc, (), lin, sc, child_hit| {
            if child_hit == Some(true) {
                return Some(true);
            }
            if el.focus_id.as_deref().is_some_and(is_text_edit_focus_id) {
                return child_hit;
            }
            let is_adornment = el.on_click.is_some()
                || el.text_style.font_family.as_deref() == Some("lucide")
                || el
                    .cursor
                    .as_ref()
                    .is_some_and(|c| c.kind == CursorKind::Pointer);
            if is_adornment
                && claims_own_bounds(el)
                && crate::core::tree::pointer_in_paint_border_box(el, [x, y], scroll_acc, lin, sc)
            {
                Some(true)
            } else {
                child_hit
            }
        },
    )
    .unwrap_or(false)
}

fn matches_input_text_leaf(el: &Element, child_fid: Option<&str>, rest: &str) -> bool {
    if el.text.is_none() || el.text_style.font_family.as_deref() == Some("lucide") {
        return false;
    }
    let Some(id) = child_fid else {
        return false;
    };
    id == format!("{}{}", crate::utils::text_field::INPUT_FOCUS_PREFIX, rest)
        || id
            == format!(
                "{}{}",
                crate::utils::text_field::TEXTAREA_FOCUS_PREFIX,
                rest
            )
}

/// World pointer → glyphon layout-box xy on the input/textarea leaf for `rest`.
pub(super) fn world_to_input_text_layout_pointer(
    root: &Element,
    rest: &str,
    world: [f32; 2],
    scroll_acc: [f32; 2],
) -> [f32; 2] {
    hit_walk_first(
        root,
        scroll_acc,
        None,
        |el, scroll_acc, fid, _lin, _sc| {
            Some((
                scroll_acc,
                next_hit_scroll_acc(el, scroll_acc),
                inherit_text_edit_focus_id(el, fid),
            ))
        },
        |el, scroll_acc, fid, lin, sc, child_hit| {
            if child_hit.is_some() {
                return child_hit;
            }
            let child_fid = inherit_text_edit_focus_id(el, fid);
            if matches_input_text_leaf(el, child_fid, rest) {
                Some(crate::core::tree::pointer_as_layout_box_xy(
                    el, world, scroll_acc, lin, sc,
                ))
            } else {
                None
            }
        },
    )
    .unwrap_or(world)
}

/// World pointer → glyphon layout-box xy on the selectable text leaf for `key`.
pub(super) fn world_to_selectable_text_layout_pointer(
    root: &Element,
    key: &str,
    world: [f32; 2],
    scroll_acc: [f32; 2],
) -> [f32; 2] {
    hit_walk_first(
        root,
        scroll_acc,
        (),
        |el, scroll_acc, (), _lin, _sc| Some((scroll_acc, next_hit_scroll_acc(el, scroll_acc), ())),
        |el, scroll_acc, (), lin, sc, child_hit| {
            if child_hit.is_some() {
                return child_hit;
            }
            if is_selectable_static_text(el)
                && crate::utils::text_selection::selectable_text_key(el).as_deref() == Some(key)
            {
                Some(crate::core::tree::pointer_as_layout_box_xy(
                    el, world, scroll_acc, lin, sc,
                ))
            } else {
                None
            }
        },
    )
    .unwrap_or(world)
}

fn is_selectable_static_text(el: &Element) -> bool {
    crate::utils::text_selection::selectable_text_key(el).is_some()
}

/// 拖选进行中按稳定键查找可选静态文本叶（布局、展示串与样式）。
/// Bounds 为可视坐标（扣祖先 scroll），与指针坐标同空间。
pub(super) fn find_selectable_text_leaf_for_key(
    el: &Element,
    key: &str,
) -> Option<([f32; 4], String, crate::style::TextStyle, [f32; 2])> {
    find_selectable_text_leaf_for_key_acc(el, key, [0.0, 0.0])
}

fn find_selectable_text_leaf_for_key_acc(
    el: &Element,
    key: &str,
    scroll_acc: [f32; 2],
) -> Option<([f32; 4], String, crate::style::TextStyle, [f32; 2])> {
    hit_walk_first(
        el,
        scroll_acc,
        (),
        |el, scroll_acc, (), _lin, _sc| {
            let next_acc = next_hit_scroll_acc(el, scroll_acc);
            Some((scroll_acc, next_acc, ()))
        },
        |el, scroll_acc, (), _lin, _sc, child_hit| {
            if child_hit.is_some() {
                return child_hit;
            }
            if is_selectable_static_text(el)
                && crate::utils::text_selection::selectable_text_key(el).as_deref() == Some(key)
            {
                if let Some(text) = &el.text {
                    let b = el.layout_bounds;
                    let [vx, vy] = visual_xy(el, scroll_acc);
                    return Some((
                        [vx, vy, b[2], b[3]],
                        text.clone(),
                        el.text_style.as_ref().clone(),
                        el.scroll_offset,
                    ));
                }
            }
            None
        },
    )
}

/// 指针 `(x,y)` 是否落在可选静态文本叶上。
pub(super) fn find_selectable_text_at(
    el: &Element,
    x: f32,
    y: f32,
    scroll_acc: [f32; 2],
) -> Option<(String, [f32; 4], String, crate::style::TextStyle, [f32; 2])> {
    find_selectable_text_at_hit(el, x, y, scroll_acc).into_option()
}

fn find_selectable_text_at_hit(
    el: &Element,
    x: f32,
    y: f32,
    scroll_acc: [f32; 2],
) -> LayerHit<(String, [f32; 4], String, crate::style::TextStyle, [f32; 2])> {
    hit_walk_layer(
        el,
        scroll_acc,
        (),
        |el, scroll_acc, (), lin, sc| {
            let (escaped, next_acc) = hit_enter_clipped(el, x, y, scroll_acc, lin, sc)?;
            Some((escaped, next_acc, (), true))
        },
        |el, scroll_acc, (), lin, sc, child_hit| {
            if !matches!(child_hit, LayerHit::Miss) {
                return child_hit;
            }
            let b = el.layout_bounds;
            let [vx, vy] = visual_xy(el, scroll_acc);
            let inside =
                crate::core::tree::pointer_in_paint_border_box(el, [x, y], scroll_acc, lin, sc);
            if is_selectable_static_text(el) && inside {
                if let Some(key) = crate::utils::text_selection::selectable_text_key(el) {
                    if let Some(text) = &el.text {
                        return LayerHit::Hit(Some((
                            key,
                            [vx, vy, b[2], b[3]],
                            text.clone(),
                            el.text_style.as_ref().clone(),
                            el.scroll_offset,
                        )));
                    }
                }
            }
            if element_self_pointer_hit_with(el, x, y, scroll_acc, lin, sc) {
                LayerHit::Hit(None)
            } else {
                LayerHit::Miss
            }
        },
    )
}

#[inline]
pub(crate) fn update_hovers(
    el: &Element,
    x: f32,
    y: f32,
    scroll_offset: [f32; 2],
    cbs: &mut Vec<(HoverCb, bool)>,
) {
    reconcile_pointer_hovers(el, x, y, scroll_offset, cbs, false);
}

/// Diff pointer hover against the current layout tree.
///
/// `immediate_miss_leave`: when content slides under a stationary cursor (smooth scroll),
/// leave on the first miss instead of the 2-frame jitter deferral used for spring/layout noise.
pub(crate) fn reconcile_pointer_hovers(
    el: &Element,
    x: f32,
    y: f32,
    scroll_offset: [f32; 2],
    cbs: &mut Vec<(HoverCb, bool)>,
    immediate_miss_leave: bool,
) {
    let _ = reconcile_pointer_hovers_and_cursor(el, x, y, scroll_offset, cbs, immediate_miss_leave);
}

/// Resolve hover and cursor from the same topmost hit path after layout.
pub(crate) fn reconcile_pointer_hovers_and_cursor(
    el: &Element,
    x: f32,
    y: f32,
    scroll_offset: [f32; 2],
    cbs: &mut Vec<(HoverCb, bool)>,
    immediate_miss_leave: bool,
) -> Option<CursorValue> {
    let hit = pick_hover_and_cursor(el, x, y, scroll_offset).into_option();
    let cursor = hit.as_ref().and_then(|hit| hit.cursor.clone());
    let dragging = crate::utils::drag::runtime::is_active();
    let target = if dragging {
        None
    } else {
        hit.and_then(|hit| hit.hover)
    };
    reconcile_hover_target(target, x, y, cbs, immediate_miss_leave || dragging);
    cursor
}

fn reconcile_hover_target(
    target: Option<HoverSlot>,
    x: f32,
    y: f32,
    cbs: &mut Vec<(HoverCb, bool)>,
    immediate_miss_leave: bool,
) {
    crate::app::pointer_events::note_pointer_client(x, y);
    if let Some(slot) = target.as_ref() {
        crate::app::pointer_events::note_hover_target_bounds(slot.bounds);
    } else {
        crate::app::pointer_events::clear_hover_target_sample();
    }
    let force = HOVER_NEEDS_RESYNC.replace(false);
    let scope = crate::core::reactivity::signal_key_scope();
    let prev =
        LAST_HOVER_BY_SCOPE.with(|map| map.borrow_mut().entry(scope).or_insert(None).clone());
    let same_identity = match (&prev, &target) {
        (Some(a), Some(b)) => a.key == b.key,
        (None, None) => true,
        _ => false,
    };
    if same_identity {
        reconcile_hover_regions(prev.as_ref(), target.as_ref(), force, cbs);
        if let Some(next) = target {
            let rc_changed = prev.as_ref().is_some_and(|p| !Rc::ptr_eq(&p.cb, &next.cb));
            // Quiet Rc refresh after rebuild. Re-fire enter when soft-invalidated, or when the
            // handler Rc was replaced while `{focus_id}_hover` is still false (desync after
            // jank/rebuild — otherwise chrome stays dead until a click forces a leave/enter).
            let mut reenter = force;
            if !reenter && rc_changed {
                if let HoverKey::Focus(fid) = &next.key {
                    let key = format!("{fid}_hover");
                    let sig = crate::core::reactivity::use_signal(&key, || false);
                    if sig.is_alive() && !sig.peek() {
                        reenter = true;
                    }
                }
            }
            if rc_changed
                || reenter
                || !next.regions.is_empty()
                || prev.as_ref().is_some_and(|s| !s.regions.is_empty())
            {
                LAST_HOVER_BY_SCOPE.with(|map| {
                    *map.borrow_mut().entry(scope).or_insert(None) = Some(next.clone());
                });
            }
            if reenter {
                push_exclusive_hover(&next, true, cbs);
            }
        }
        HOVER_MISS_STREAK.set(0);
        return;
    }
    // Different hoverable under the pointer — leave/enter immediately.
    if let Some(new) = target.clone() {
        HOVER_MISS_STREAK.set(0);
        reconcile_hover_regions(prev.as_ref(), Some(&new), force, cbs);
        if let Some(old) = prev {
            push_exclusive_hover(&old, false, cbs);
        }
        push_exclusive_hover(&new, true, cbs);
        LAST_HOVER_BY_SCOPE.with(|map| {
            *map.borrow_mut().entry(scope).or_insert(None) = target;
        });
        return;
    }
    // Miss while we had a target: tolerate one-frame hit-test jitter (spring/layout chrome).
    // Without this, leave→spring(0) + next-frame enter→spring(1) flashes sidebar items.
    let streak = HOVER_MISS_STREAK.get().saturating_add(1);
    HOVER_MISS_STREAK.set(streak);
    if streak < 2 && !immediate_miss_leave {
        // The deferred leave MUST pump its own frame. `update_hovers` only runs from
        // `CursorMoved` and the post-layout redraw pass, so if the pointer leaves the
        // element and then stops, nothing produces miss #2 and the leave is lost forever
        // (Toast's hover stack then stays expanded with every auto-dismiss timer frozen).
        crate::primitives::interactive::mark_hover_paint_dirty();
        return;
    }
    HOVER_MISS_STREAK.set(0);
    reconcile_hover_regions(prev.as_ref(), None, force, cbs);
    if let Some(old) = prev {
        push_exclusive_hover(&old, false, cbs);
    }
    LAST_HOVER_BY_SCOPE.with(|map| {
        *map.borrow_mut().entry(scope).or_insert(None) = None;
    });
}

fn reconcile_hover_regions(
    prev: Option<&HoverSlot>,
    next: Option<&HoverSlot>,
    force: bool,
    cbs: &mut Vec<(HoverCb, bool)>,
) {
    // Include a region when it is the exclusive target too, so moving between
    // its background and descendants never emits a spurious leave/enter.
    let old = prev.map(|s| s.regions.as_slice()).unwrap_or_default();
    let new = next.map(|s| s.regions.as_slice()).unwrap_or_default();
    for region in old {
        if !new.iter().any(|s| s.key == region.key) {
            cbs.push((region.cb.clone(), false));
        }
    }
    for region in new {
        if force || !old.iter().any(|s| s.key == region.key) {
            cbs.push((region.cb.clone(), true));
        }
    }
}

/// Diff pointer hover and fire `on_hover` immediately so `{id}_hover` and CSS `:hover`
/// leave (and enter) on the same chrome frame.
///
/// `immediate_miss_leave`: smooth-scroll, pointer off-window (`[-1,-1]`), or a
/// StructureChanged sentinel that already forces CSS `:hover` off. Post-render
/// [`reconcile_pointer_hovers`] stays as an idempotent Rc-refresh after layout.
pub(crate) fn apply_pointer_hover_reconcile(
    el: &Element,
    x: f32,
    y: f32,
    immediate_miss_leave: bool,
) {
    let mut cbs = Vec::new();
    reconcile_pointer_hovers(el, x, y, [0.0, 0.0], &mut cbs, immediate_miss_leave);
    for (cb, is_hover) in cbs {
        (*cb.borrow_mut())(is_hover);
    }
}

/// Soft-invalidate after handler `Rc`s are replaced (DevTools dock / `reactive_view`).
///
/// Does **not** fire leave. The next [`update_hovers`] re-resolves by [`HoverKey`]: same
/// `focus_id` only re-asserts enter (no spring 0→1 thrash); pointer moved away emits leave.
pub fn invalidate_hover_handlers() {
    HOVER_NEEDS_RESYNC.set(true);
}

/// Hard reset: fire leave and drop the cached slot (tests / teardown).
///
/// Prefer [`invalidate_hover_handlers`] on live rebuild paths — hard leave here races with
/// same-frame re-enter and spring ticks, which flickered gallery sidebar hover.
#[cfg(test)]
pub(crate) fn clear_hover_tracking() {
    HOVER_NEEDS_RESYNC.set(true);
    HOVER_MISS_STREAK.set(0);
    let scope = crate::core::reactivity::signal_key_scope();
    let prev = LAST_HOVER_BY_SCOPE.with(|map| map.borrow_mut().insert(scope, None).flatten());
    if let Some(old) = prev {
        let mut cbs = Vec::new();
        reconcile_hover_regions(Some(&old), None, false, &mut cbs);
        push_exclusive_hover(&old, false, &mut cbs);
        for (cb, hovered) in cbs {
            (*cb.borrow_mut())(hovered);
        }
    }
}

/// A geometric hit can absorb lower siblings even without either handler.
#[derive(Default)]
struct HoverAndCursor {
    hover: Option<HoverSlot>,
    cursor: Option<CursorValue>,
}

fn pick_hover_and_cursor(
    el: &Element,
    x: f32,
    y: f32,
    scroll_acc: [f32; 2],
) -> LayerHit<HoverAndCursor> {
    hit_walk_layer(
        el,
        scroll_acc,
        (),
        |el, scroll_acc, (), lin, sc| {
            let (escaped, next_acc) = hit_enter_clipped(el, x, y, scroll_acc, lin, sc)?;
            // A hit inside a handler-free subtree can only bubble empty payloads.
            // Do not skip on a miss: overflow-visible descendants may extend out.
            // Zero hashes fall through after structure/handler invalidation.
            let absorbed = el.collect_paint_hash != 0
                && !el.subtree_hover_cursor
                && element_self_pointer_hit_with(el, x, y, escaped, lin, sc);
            Some((escaped, next_acc, (), !absorbed))
        },
        |el, scroll_acc, (), lin, sc, child_hit| {
            // Most list leaves have no hover handler. Only compute handler bounds
            // when one exists; the geometric hit test already computes its own box.
            let hover = || {
                el.on_hover.as_ref()?;
                hover_slot_for(
                    el,
                    crate::core::tree::visual_paint_aabb_with_ancestor(el, scroll_acc, lin, sc),
                )
            };
            let mut hit = match child_hit {
                LayerHit::Hit(child) => {
                    let child: HoverAndCursor = child.unwrap_or_default();
                    HoverAndCursor {
                        hover: child.hover.or_else(hover),
                        cursor: child.cursor.or_else(|| el.cursor.clone()),
                    }
                }
                LayerHit::Miss => {
                    if !element_self_pointer_hit_with(el, x, y, scroll_acc, lin, sc) {
                        return LayerHit::Miss;
                    }
                    HoverAndCursor {
                        hover: hover(),
                        cursor: el.cursor.clone(),
                    }
                }
            };
            if el.attrs.get(ATTR_HOVER_REGION).map(String::as_str) == Some("1") {
                if let Some(slot) = &mut hit.hover {
                    if let Some(region) = hover() {
                        slot.regions.push(region);
                    }
                }
            }
            LayerHit::Hit(Some(hit))
        },
    )
}

pub(crate) fn find_click(
    el: &Element,
    x: f32,
    y: f32,
    scroll_acc: [f32; 2],
) -> Option<std::rc::Rc<std::cell::RefCell<dyn FnMut()>>> {
    hit_walk_first(
        el,
        scroll_acc,
        (),
        |el, scroll_acc, (), lin, sc| {
            if pointer_events_blocked(el) {
                return None;
            }
            if !crate::core::tree::overflow_clip_contains_pointer(el, [x, y], scroll_acc, lin, sc) {
                return None;
            }
            if !crate::core::tree::clip_path_contains_pointer(el, [x, y], scroll_acc, lin, sc) {
                return None;
            }
            let next_acc = next_hit_scroll_acc(el, scroll_acc);
            Some((scroll_acc, next_acc, ()))
        },
        |el, scroll_acc, (), lin, sc, child_hit| {
            if child_hit.is_some() {
                return child_hit;
            }
            if element_self_pointer_hit_with(el, x, y, scroll_acc, lin, sc) {
                el.on_click.clone()
            } else {
                None
            }
        },
    )
}

pub(super) fn find_element_by_focus_id<'a>(el: &'a Element, focus_id: &str) -> Option<&'a Element> {
    find_in_tree(el, |n| n.focus_id.as_deref() == Some(focus_id))
}

pub(super) fn find_element_by_focus_id_mut<'a>(
    el: &'a mut Element,
    focus_id: &str,
) -> Option<&'a mut Element> {
    find_in_tree_mut(el, |n| n.focus_id.as_deref() == Some(focus_id))
}

/// Result of edge auto-scroll while drag-selecting `wrap=off` text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TextDragHScrollTick {
    /// Pointer inside the clip — no auto-scroll.
    Idle,
    /// Scrolled this frame; keep requesting redraws while the pointer stays past the edge.
    Scrolled,
    /// Pointer still past the edge but scroll is already at min/max.
    AtLimit,
}

/// Thin inner belt (px): start a crawl before the pointer fully leaves the clip.
const TEXT_DRAG_EDGE_SLOP: f32 = 2.0;
/// Past this many px outside the clip, one frame pans to the start/end.
const TEXT_DRAG_JUMP_OVERSHOOT: f32 = 100.0;
/// Near-edge crawl: characters (or line-boxes, vertically) per second.
const TEXT_DRAG_NEAR_UNITS_PER_SEC: f32 = 8.0;

/// Distance past the **clip edge** (not the inner belt). `None` = not in the autoscroll zone.
/// `toward_max` is true when scrolling toward higher offset (right / bottom).
fn text_drag_edge_overshoot(pointer: f32, min: f32, max: f32) -> Option<(bool, f32)> {
    if pointer > max - TEXT_DRAG_EDGE_SLOP {
        Some((true, (pointer - max).max(0.0)))
    } else if pointer < min + TEXT_DRAG_EDGE_SLOP {
        Some((false, (min - pointer).max(0.0)))
    } else {
        None
    }
}

/// Browser-like `<input>` drag-select: ~1 glyph/line at the edge, jump to the end when far.
///
/// `unit_px` is one character (horizontal) or one line box (vertical).
#[expect(
    clippy::manual_clamp,
    reason = "max/min deliberately maps NaN to the lower bound; clamp would preserve NaN"
)]
pub(super) fn text_drag_edge_scroll_step(
    overshoot_px: f32,
    unit_px: f32,
    remaining: f32,
    dt: f32,
) -> f32 {
    let remaining = remaining.max(0.0);
    if remaining < 0.05 {
        return 0.0;
    }
    let over = overshoot_px.max(0.0);
    if over >= TEXT_DRAG_JUMP_OVERSHOOT {
        return remaining;
    }
    let dt = dt.max(0.0).min(0.05);
    if dt < 1.0e-6 {
        return 0.0;
    }
    let unit = unit_px.clamp(6.0, 32.0);
    let t = (over / TEXT_DRAG_JUMP_OVERSHOOT).clamp(0.0, 1.0);
    // Quadratic: stays near `NEAR` for the first ~20px, then sprints into the jump.
    let units_per_sec = TEXT_DRAG_NEAR_UNITS_PER_SEC + 200.0 * t * t;
    let step = units_per_sec * unit * dt;
    step.min(remaining)
}

/// Visual clip of a textarea scrollport: `[x, y, w, h]` in logical screen space.
///
/// Used to clamp drag-select hit-testing to the visible edge (avoids selection
/// jumping ahead along a wide nowrap leaf when the pointer is outside the clip).
pub(super) fn textarea_scrollport_visual_clip(root: &Element, field_id: &str) -> Option<[f32; 4]> {
    use crate::core::scroll_viewport::focus_id_for_scroll_viewport;
    use crate::utils::text_field::textarea_scroll_rest;

    let scroll_rest = textarea_scroll_rest(field_id);
    let vp_fid = focus_id_for_scroll_viewport(&scroll_rest);
    let (vp_layout, vp_scroll) = find_bounds_by_focus_id(root, &vp_fid, [0.0, 0.0])?;
    let left = vp_layout[0] - vp_scroll[0];
    let top = vp_layout[1] - vp_scroll[1];
    Some([left, top, vp_layout[2].max(1.0), vp_layout[3].max(1.0)])
}

/// Clamp a pointer into the textarea scrollport so drag-select tracks the visible
/// edge while edge auto-scroll runs (wrap=off horizontal / soft vertical).
pub(super) fn clamp_pointer_to_textarea_scrollport(
    root: &Element,
    field_id: &str,
    x: f32,
    y: f32,
) -> (f32, f32) {
    let Some(clip) = textarea_scrollport_visual_clip(root, field_id) else {
        return (x, y);
    };
    (
        x.clamp(clip[0], clip[0] + clip[2]),
        y.clamp(clip[1], clip[1] + clip[3]),
    )
}

/// Map pointer + leaf metadata into the coordinate space expected by
/// [`crate::renderer::hit_test_input_text_index`] during an active drag-select.
///
/// Pointer space for input/textarea hit-testing: for `wrap=off` textarea the viewport
/// `sx` is already baked into [`visual_xy`] leaf bounds (same as layout collect
/// `text_x`); hit bounds stay on the leaf with `scroll_x = 0`. Pointer X/Y may still
/// clamp to the scrollport clip so edge drag-select tracks the visible edge.
pub(super) fn input_text_edit_pointer_space(
    root: &Element,
    rest: &str,
    x: f32,
    y: f32,
    leaf_bounds: [f32; 4],
    style: &crate::style::TextStyle,
    leaf_scroll: [f32; 2],
) -> (f32, f32, [f32; 4], crate::style::TextStyle) {
    input_text_edit_pointer_space_impl(root, rest, x, y, leaf_bounds, style, leaf_scroll)
}

/// While drag-selecting in a `wrap=off` textarea, prefer [`input_text_edit_pointer_space`].
pub(super) fn input_drag_select_hit_space(
    root: &Element,
    rest: &str,
    x: f32,
    y: f32,
    leaf_bounds: [f32; 4],
    style: &crate::style::TextStyle,
    leaf_scroll: [f32; 2],
) -> (f32, f32, [f32; 4], crate::style::TextStyle) {
    input_text_edit_pointer_space_impl(root, rest, x, y, leaf_bounds, style, leaf_scroll)
}

fn input_text_edit_pointer_space_impl(
    root: &Element,
    rest: &str,
    x: f32,
    y: f32,
    leaf_bounds: [f32; 4],
    style: &crate::style::TextStyle,
    leaf_scroll: [f32; 2],
) -> (f32, f32, [f32; 4], crate::style::TextStyle) {
    use crate::style::WhiteSpace;

    let (hx, hy) = clamp_pointer_to_textarea_scrollport(root, rest, x, y);
    let mx_base = hx + leaf_scroll[0];
    let my = hy + leaf_scroll[1];

    if style.white_space == WhiteSpace::Nowrap && !style.overflow.allows_caret_scroll() {
        if let Some(clip) = textarea_scrollport_visual_clip(root, rest) {
            let mut hit_style = style.clone();
            // Viewport pan is in leaf visual x via `next_hit_scroll_acc`; do not also
            // add `scroll_x` or use clip origin (misses region padding → caret偏右).
            hit_style.scroll_x = 0.0;
            let mx = mx_base.clamp(clip[0], clip[0] + clip[2]);
            let my = my.clamp(clip[1], clip[1] + clip[3]);
            let mapped = world_to_input_text_layout_pointer(
                root,
                rest,
                [mx - leaf_scroll[0], my - leaf_scroll[1]],
                [0.0, 0.0],
            );
            return (
                mapped[0] + leaf_scroll[0],
                mapped[1] + leaf_scroll[1],
                leaf_bounds,
                hit_style,
            );
        }
    }

    let mapped = world_to_input_text_layout_pointer(root, rest, [hx, hy], [0.0, 0.0]);
    let mx_base = mapped[0] + leaf_scroll[0];
    let my = mapped[1] + leaf_scroll[1];
    let mx = if style.overflow.allows_caret_scroll() {
        // Native `<input>`: hit-test stays on the visible clip so drag-select can
        // edge-scroll instead of jumping to the first/last glyph.
        mx_base.clamp(leaf_bounds[0], leaf_bounds[0] + leaf_bounds[2])
    } else if style.white_space == WhiteSpace::Nowrap {
        mx_base
    } else {
        mx_base.clamp(leaf_bounds[0], leaf_bounds[0] + leaf_bounds[2])
    };
    (
        mx,
        my.clamp(leaf_bounds[1], leaf_bounds[1] + leaf_bounds[3]),
        leaf_bounds,
        style.clone(),
    )
}

/// While drag-selecting in a `wrap=off` textarea, scroll horizontally when the pointer
/// is near/past the scrollport on X. `dt` is frame delta so holding still still scrolls.
///
/// Follow-scroll stops at the **current** hard line's end (caret fully visible). A longer
/// sibling line must not keep panning into empty space beside this line.
///
/// Paint must not run `ensure_caret_visible_scroll_x` during text drag (see
/// [`crate::utils::text_field::text_drag_select_active`]).
pub(super) fn nudge_textarea_hscroll_for_drag(
    root: &mut Element,
    field_id: &str,
    pointer_x: f32,
    pointer_y: f32,
    dt: f32,
) -> TextDragHScrollTick {
    use crate::core::reactivity::use_signal;
    use crate::core::scroll_viewport::{
        focus_id_for_scroll_viewport, scroll_offset_x_signal_key, scroll_offset_y_signal_key,
        scroll_viewport_w_signal_key,
    };
    use crate::style::WhiteSpace;
    use crate::utils::text_field::{
        hard_line_body_at_y, nowrap_line_end_max_scroll_x, text_edit_line_box_px,
        textarea_scroll_rest, textarea_scrollport_padding_x, TEXTAREA_SCROLLPORT_PAD_X,
    };

    let Some((leaf_bounds, display, style, _)) =
        find_input_text_leaf_for_rest(root, None, field_id)
    else {
        return TextDragHScrollTick::Idle;
    };
    if style.white_space != WhiteSpace::Nowrap {
        return TextDragHScrollTick::Idle;
    }

    let scroll_rest = textarea_scroll_rest(field_id);
    let vp_fid = focus_id_for_scroll_viewport(&scroll_rest);
    let Some((vp_layout, vp_scroll)) = find_bounds_by_focus_id(root, &vp_fid, [0.0, 0.0]) else {
        return TextDragHScrollTick::Idle;
    };
    // Viewport chrome visual rect (layout − ancestor scroll). Own scroll does not move the clip.
    let left = vp_layout[0] - vp_scroll[0];
    let right = left + vp_layout[2];
    let vw_layout = vp_layout[2].max(1.0);

    let pad_left = find_element_by_focus_id(root, &vp_fid)
        .map(|vp| textarea_scrollport_padding_x(vp, vp_layout[0], vw_layout).0)
        .unwrap_or(TEXTAREA_SCROLLPORT_PAD_X);

    let lh = text_edit_line_box_px(
        style.size_resolved(),
        style.line_height_px,
        style.line_height_scale,
    );
    let rel_y = (pointer_y - leaf_bounds[1]).clamp(0.0, leaf_bounds[3].max(0.0));
    let line_body = hard_line_body_at_y(&display, rel_y, lh);
    let measured = crate::text::measure_prefix_advance(
        line_body,
        style.size_resolved(),
        style.weight_resolved(),
        lh,
    );
    let line_end_x = if line_body.is_empty() {
        0.0
    } else if measured > 1.0 {
        measured
    } else {
        leaf_bounds[2]
    };

    let sx_key = scroll_offset_x_signal_key(&scroll_rest);
    let sy_key = scroll_offset_y_signal_key(&scroll_rest);
    let vw_key = scroll_viewport_w_signal_key(&scroll_rest);
    let sx_sig = use_signal(&sx_key, || 0.0_f32);
    let sy = use_signal(&sy_key, || 0.0_f32).peek();
    let vw = use_signal(&vw_key, || 0.0_f32)
        .peek()
        .max(vw_layout)
        .max(1.0);
    let max_sx = nowrap_line_end_max_scroll_x(line_end_x, vw, pad_left);
    let sx_raw = sx_sig.peek().max(0.0);
    // Dragging onto a shorter line while still panned from a longer sibling: snap back
    // so the current line-end caret is fully visible.
    if sx_raw > max_sx + 0.05 {
        super::scroll_support::set_scroll_offset_with_behavior(&scroll_rest, max_sx, sy, true);
        if let Some(vp) = find_element_by_focus_id_mut(root, &vp_fid) {
            vp.scroll_offset[0] = max_sx;
        }
        return TextDragHScrollTick::Scrolled;
    }
    if max_sx < 0.5 {
        return TextDragHScrollTick::AtLimit;
    }
    // Paint ensure is skipped during text drag (that pair caused bounce before).
    let Some((toward_max, over)) = text_drag_edge_overshoot(pointer_x, left, right) else {
        return TextDragHScrollTick::Idle;
    };

    let sx = sx_raw.clamp(0.0, max_sx);
    let remaining = if toward_max { max_sx - sx } else { sx };
    let unit = style.size_resolved().max(1.0) * 0.55;
    let step = text_drag_edge_scroll_step(over, unit, remaining, dt);
    let next = if toward_max {
        (sx + step).min(max_sx)
    } else {
        (sx - step).max(0.0)
    };
    if (next - sx).abs() < 0.05 {
        return TextDragHScrollTick::AtLimit;
    }
    // Silent + sync target: avoid reactive rebuild / spring fighting the drag.
    super::scroll_support::set_scroll_offset_with_behavior(&scroll_rest, next, sy, true);
    if let Some(vp) = find_element_by_focus_id_mut(root, &vp_fid) {
        vp.scroll_offset[0] = next;
    }
    TextDragHScrollTick::Scrolled
}

/// Single-line Input (`text-overflow: hidden` + caret pan): edge auto-scroll while drag-selecting.
///
/// Unlike wrap=off textarea this uses [`crate::utils::text_field::input_scroll_x_signal_key`]
/// (leaf `text_style.scroll_x`), not a scroll viewport.
pub(super) fn nudge_input_hscroll_for_drag(
    root: &mut Element,
    field_id: &str,
    pointer_x: f32,
    dt: f32,
) -> TextDragHScrollTick {
    use crate::core::reactivity::use_signal;
    use crate::utils::text_field::{
        clamp_caret_scroll_x, input_scroll_x_signal_key, max_caret_scroll_x, text_edit_line_box_px,
    };

    let Some((leaf_bounds, display, style, _)) =
        find_input_text_leaf_for_rest(root, None, field_id)
    else {
        return TextDragHScrollTick::Idle;
    };
    if !style.overflow.allows_caret_scroll() {
        return TextDragHScrollTick::Idle;
    }

    let left = leaf_bounds[0];
    let right = left + leaf_bounds[2];
    let vw = leaf_bounds[2].max(1.0);

    let lh = text_edit_line_box_px(
        style.size_resolved(),
        style.line_height_px,
        style.line_height_scale,
    );
    let content_w = crate::text::measure_prefix_advance(
        &display,
        style.size_resolved(),
        style.weight_resolved(),
        lh,
    )
    .max(vw);
    let max_sx = max_caret_scroll_x(content_w, vw);
    let sx_sig = use_signal(&input_scroll_x_signal_key(field_id), || 0.0_f32);
    let sx_raw = sx_sig.peek().max(0.0);
    if sx_raw > max_sx + 0.05 {
        let next = clamp_caret_scroll_x(content_w, vw, sx_raw);
        sx_sig.set_silent(next);
        write_input_leaf_scroll_x(root, field_id, next);
        return TextDragHScrollTick::Scrolled;
    }
    if max_sx < 0.5 {
        return TextDragHScrollTick::AtLimit;
    }
    let Some((toward_max, over)) = text_drag_edge_overshoot(pointer_x, left, right) else {
        return TextDragHScrollTick::Idle;
    };

    let sx = sx_raw.clamp(0.0, max_sx);
    let remaining = if toward_max { max_sx - sx } else { sx };
    let unit = style.size_resolved().max(1.0) * 0.55;
    let step = text_drag_edge_scroll_step(over, unit, remaining, dt);
    let next = if toward_max {
        (sx + step).min(max_sx)
    } else {
        (sx - step).max(0.0)
    };
    if (next - sx).abs() < 0.05 {
        return TextDragHScrollTick::AtLimit;
    }
    sx_sig.set_silent(next);
    write_input_leaf_scroll_x(root, field_id, next);
    TextDragHScrollTick::Scrolled
}

fn write_input_leaf_scroll_x(root: &mut Element, field_id: &str, sx: f32) {
    let tid = crate::utils::text_field::input_text_template_id(field_id);
    if let Some(leaf) = find_in_tree_mut(root, |el| el.template_id.as_deref() == Some(tid.as_str()))
    {
        leaf.text_style.scroll_x = sx;
    }
}

/// Vertical edge auto-scroll while drag-selecting soft-wrapped textarea text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TextDragVScrollTick {
    Idle,
    Scrolled,
    AtLimit,
}

pub(super) fn nudge_textarea_vscroll_for_drag(
    root: &mut Element,
    field_id: &str,
    pointer_y: f32,
    dt: f32,
) -> TextDragVScrollTick {
    use crate::core::reactivity::use_signal;
    use crate::core::scroll_viewport::{
        focus_id_for_scroll_viewport, scroll_content_h_signal_key, scroll_offset_x_signal_key,
        scroll_offset_y_signal_key, scroll_viewport_h_signal_key,
    };
    use crate::utils::text_field::{text_edit_line_box_px, textarea_scroll_rest};

    let Some((_, _, style, _)) = find_input_text_leaf_for_rest(root, None, field_id) else {
        return TextDragVScrollTick::Idle;
    };
    let line_unit = text_edit_line_box_px(
        style.size_resolved(),
        style.line_height_px,
        style.line_height_scale,
    )
    .max(12.0);

    let scroll_rest = textarea_scroll_rest(field_id);
    let vp_fid = focus_id_for_scroll_viewport(&scroll_rest);
    let Some((vp_layout, vp_scroll)) = find_bounds_by_focus_id(root, &vp_fid, [0.0, 0.0]) else {
        return TextDragVScrollTick::Idle;
    };
    let top = vp_layout[1] - vp_scroll[1];
    let bottom = top + vp_layout[3];
    let vh_layout = vp_layout[3].max(1.0);

    // Match horizontal drag: thin inner belt (paint ensure skipped during text drag).
    let Some((toward_max, over)) = text_drag_edge_overshoot(pointer_y, top, bottom) else {
        return TextDragVScrollTick::Idle;
    };

    let sx_key = scroll_offset_x_signal_key(&scroll_rest);
    let sy_key = scroll_offset_y_signal_key(&scroll_rest);
    let vh_key = scroll_viewport_h_signal_key(&scroll_rest);
    let ch_key = scroll_content_h_signal_key(&scroll_rest);
    let sx = use_signal(&sx_key, || 0.0_f32).peek();
    let sy_sig = use_signal(&sy_key, || 0.0_f32);
    let vh = use_signal(&vh_key, || 0.0_f32)
        .peek()
        .max(vh_layout)
        .max(1.0);
    let ch = use_signal(&ch_key, || 0.0_f32).peek().max(vh);
    let max_sy = (ch - vh).max(0.0);
    if max_sy < 0.5 {
        return TextDragVScrollTick::AtLimit;
    }

    let sy = sy_sig.peek().clamp(0.0, max_sy);
    let remaining = if toward_max { max_sy - sy } else { sy };
    let unit = line_unit;
    let step = text_drag_edge_scroll_step(over, unit, remaining, dt);
    let next = if toward_max {
        (sy + step).min(max_sy)
    } else {
        (sy - step).max(0.0)
    };
    if (next - sy).abs() < 0.05 {
        return TextDragVScrollTick::AtLimit;
    }
    super::scroll_support::set_scroll_offset_with_behavior(&scroll_rest, sx, next, true);
    if let Some(vp) = find_element_by_focus_id_mut(root, &vp_fid) {
        vp.scroll_offset[1] = next;
    }
    TextDragVScrollTick::Scrolled
}

/// Handlers cloned off the focused node so callers can drop `AppState` borrows before running them.
///
/// Running `on_key_down` / `on_click` while `AppState` is borrowed is unsafe: handlers often
/// `signal.set`, which flushes `reactive_view` → [`crate::app::runtime::try_reconcile_subtree`]
/// → `borrow_mut` and panics with `RefCell already borrowed`.
pub(super) struct FocusedKeyHandlers {
    pub on_key_down: Option<std::rc::Rc<std::cell::RefCell<dyn FnMut(KeyEvent) -> bool>>>,
    pub on_click: Option<std::rc::Rc<std::cell::RefCell<dyn FnMut()>>>,
    pub focusable: bool,
}

pub(super) fn focused_key_handlers(el: &Element, focus_id: &str) -> Option<FocusedKeyHandlers> {
    let node = find_element_by_focus_id(el, focus_id)?;
    Some(FocusedKeyHandlers {
        on_key_down: node.on_key_down.clone(),
        on_click: node.on_click.clone(),
        focusable: node.focusable,
    })
}

/// Routes a key through previously extracted [`FocusedKeyHandlers`].
pub(super) fn run_focused_key_handlers(handlers: &FocusedKeyHandlers, event: &KeyEvent) -> bool {
    if run_focused_keydown_only(handlers, event) {
        return true;
    }
    if event.state != ElementState::Pressed || event.repeat {
        return false;
    }
    let activate = crate::utils::keyboard::is_enter_or_space(&event.logical_key);
    if activate && handlers.focusable {
        if let Some(cb) = &handlers.on_click {
            cb.borrow_mut()();
            return true;
        }
    }
    false
}

/// Run `on_key_down` only (no Space/Enter → click). Used while a text field is focused so
/// Command/Combobox can steal arrows/Enter without eating typed Space.
pub(super) fn run_focused_keydown_only(handlers: &FocusedKeyHandlers, event: &KeyEvent) -> bool {
    if let Some(cb) = &handlers.on_key_down {
        return cb.borrow_mut()(event.clone());
    }
    false
}

#[cfg(test)]
#[path = "tests/hover_occlusion.rs"]
mod hover_occlusion_tests;

#[cfg(test)]
#[path = "tests/focused_key_dispatch.rs"]
mod focused_key_dispatch_tests;

#[cfg(test)]
#[path = "tests/ime_scroll_bounds.rs"]
mod ime_scroll_bounds_tests;

#[cfg(test)]
#[path = "tests/textarea_text_hit.rs"]
mod textarea_text_hit_tests;

#[cfg(test)]
#[path = "tests/drag_select_bounds.rs"]
mod drag_select_bounds_tests;

/// Inspector pick: child-index path for the topmost hit target (logical coords).
#[cfg(all(debug_assertions, feature = "devtools"))]
#[allow(dead_code)]
pub(crate) fn pick_element_path(
    el: &Element,
    x: f32,
    y: f32,
    scroll_acc: [f32; 2],
) -> Option<Vec<usize>> {
    pick_element_path_acc(el, x, y, scroll_acc, Vec::new())
}

#[cfg(all(debug_assertions, feature = "devtools"))]
#[allow(dead_code)]
fn pick_element_path_acc(
    el: &Element,
    x: f32,
    y: f32,
    scroll_acc: [f32; 2],
    path: Vec<usize>,
) -> Option<Vec<usize>> {
    enum Phase {
        Enter,
        After,
    }
    struct Frame<'a> {
        el: &'a Element,
        scroll_acc: [f32; 2],
        path: Vec<usize>,
        phase: Phase,
    }
    let mut stack = vec![Frame {
        el,
        scroll_acc,
        path,
        phase: Phase::Enter,
    }];
    let mut result = None;
    while let Some(frame) = stack.pop() {
        match frame.phase {
            Phase::Enter => {
                if result.is_some() {
                    continue;
                }
                if frame.el.template_id.as_deref() == Some(crate::app::devtools::HIGHLIGHT_SLOT_ID)
                    || frame
                        .el
                        .template_id
                        .as_deref()
                        .is_some_and(|id| id.starts_with("__jadix_devtools_hl_"))
                {
                    continue;
                }
                let escaped = escape_scroll_acc(frame.el, frame.scroll_acc);
                let is_body_portal = frame.el.template_id.as_deref()
                    == Some(crate::layout::layout_tree::PORTAL_LAYER_ID);
                if !is_body_portal
                    && !crate::core::tree::overflow_clip_contains_pointer(
                        frame.el,
                        [x, y],
                        escaped,
                        frame.el.paint_ancestor_linear,
                        frame.el.paint_ancestor_scale,
                    )
                {
                    continue;
                }
                if !crate::core::tree::clip_path_contains_pointer(
                    frame.el,
                    [x, y],
                    escaped,
                    frame.el.paint_ancestor_linear,
                    frame.el.paint_ancestor_scale,
                ) {
                    continue;
                }
                let next_acc = if is_body_portal {
                    escaped
                } else {
                    next_hit_scroll_acc(frame.el, escaped)
                };
                let order = if children_share_same_z_index(frame.el) {
                    (0..frame.el.children.len()).collect::<Vec<_>>()
                } else {
                    hit_test_child_indices(frame.el)
                };
                stack.push(Frame {
                    el: frame.el,
                    scroll_acc: escaped,
                    path: frame.path.clone(),
                    phase: Phase::After,
                });
                for i in order {
                    let mut child_path = frame.path.clone();
                    child_path.push(i);
                    stack.push(Frame {
                        el: &frame.el.children[i],
                        scroll_acc: next_acc,
                        path: child_path,
                        phase: Phase::Enter,
                    });
                }
            }
            Phase::After => {
                if result.is_some() {
                    continue;
                }
                let vb = visual_border_box(frame.el, frame.scroll_acc);
                let inside = x >= vb[0] && x <= vb[0] + vb[2] && y >= vb[1] && y <= vb[1] + vb[3];
                if inside && vb[2] > 0.0 && vb[3] > 0.0 && !portal_hit_passthrough(frame.el) {
                    result = Some(frame.path);
                }
            }
        }
    }
    result
}

#[cfg(test)]
#[path = "tests/hit_path_matches_pointer_walks.rs"]
mod hit_path_matches_pointer_walks;

#[cfg(test)]
include!("tests/tree_hit_cases.rs");
