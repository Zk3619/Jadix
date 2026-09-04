use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{Key, NamedKey};

use crate::core::tree::{
    find_in_tree, find_in_tree_mut, pointer_events_targets_children, pointer_events_targets_self,
    CursorKind, CursorValue, Element, PointerEvents,
};
use crate::layout::layout_tree::portal_hit_passthrough;

type HoverCb = Rc<RefCell<dyn FnMut(bool)>>;

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

/// 自身指针命中：AABB +（若适用）SVG path fill/stroke 几何（MDN `pointer-events`）。
#[inline]
pub(crate) fn element_self_pointer_hit(el: &Element, x: f32, y: f32, vx: f32, vy: f32) -> bool {
    if !claims_own_bounds(el) {
        return false;
    }
    let b = el.layout_bounds;
    if x < vx || x > vx + b[2] || y < vy || y > vy + b[3] {
        return false;
    }
    let mode = el.pointer_events_mode();
    if mode == PointerEvents::BoundingBox {
        return true;
    }
    // SVG leaf on CSS `auto` ≡ `visiblePainted` (MDN).
    let geom_mode = if mode == PointerEvents::Auto
        && crate::primitives::path::element_has_path_geometry(&el.attrs)
    {
        PointerEvents::VisiblePainted
    } else if mode.uses_path_geometry() {
        mode
    } else {
        return true;
    };
    if geom_mode.requires_visibility() && el.style.opacity <= 0.0 {
        return false;
    }
    let Some(policy) = geom_mode.path_hit_policy() else {
        return true;
    };
    if !crate::primitives::path::element_has_path_geometry(&el.attrs) {
        // 纯纹理 Icon / AnimatedIcon：无 path 层时回退边界盒。
        return true;
    }
    let local = (x - vx, y - vy);
    crate::primitives::path::path_pointer_hit(&el.attrs, [b[2], b[3]], local, policy)
}

/// Preserve `input:` / `textarea:` focus through inner scroll viewports (`scroll:`).
#[inline]
fn inherit_text_edit_focus_id<'a>(el: &'a Element, inherited: Option<&'a str>) -> Option<&'a str> {
    if let Some(id) = el.focus_id.as_deref() {
        if id.starts_with(crate::components::INPUT_FOCUS_PREFIX)
            || id.starts_with(crate::components::TEXTAREA_FOCUS_PREFIX)
        {
            return Some(id);
        }
    }
    inherited
}

pub(super) fn hit_test_child_indices(el: &Element) -> Vec<usize> {
    // Topmost child wins.
    // Order rule:
    // - higher z_index is always on top
    // - within same z_index, later DOM siblings are on top
    let mut order: Vec<usize> = (0..el.children.len()).collect();
    order.sort_by(|&a, &b| {
        let za = el.children[a].style.z_index;
        let zb = el.children[b].style.z_index;
        za.cmp(&zb).then(a.cmp(&b))
    });
    order
}

#[inline]
pub(super) fn children_share_same_z_index(el: &Element) -> bool {
    let Some(first) = el.children.first() else {
        return true;
    };
    let z0 = first.style.z_index;
    el.children.iter().all(|c| c.style.z_index == z0)
}

enum HitPhase {
    Enter,
    After,
}

struct HitFrame<'a, Ctx> {
    el: &'a Element,
    scroll_acc: [f32; 2],
    ctx: Ctx,
    phase: HitPhase,
}

fn push_z_hit_children<'a, Ctx: Copy>(
    stack: &mut Vec<HitFrame<'a, Ctx>>,
    el: &'a Element,
    next_acc: [f32; 2],
    child_ctx: Ctx,
) {
    if children_share_same_z_index(el) {
        for child in &el.children {
            stack.push(HitFrame {
                el: child,
                scroll_acc: next_acc,
                ctx: child_ctx,
                phase: HitPhase::Enter,
            });
        }
    } else {
        for i in hit_test_child_indices(el) {
            stack.push(HitFrame {
                el: &el.children[i],
                scroll_acc: next_acc,
                ctx: child_ctx,
                phase: HitPhase::Enter,
            });
        }
    }
}

/// Heap DFS hit-test. `enter` returns `(escaped_acc, next_acc, child_ctx)` or `None`
/// to skip the node. After children, `after` sees the first decisive child hit.
fn hit_walk_layer<'a, T, Ctx: Copy>(
    root: &'a Element,
    scroll_acc: [f32; 2],
    ctx: Ctx,
    mut enter: impl FnMut(&'a Element, [f32; 2], Ctx) -> Option<([f32; 2], [f32; 2], Ctx)>,
    mut after: impl FnMut(&'a Element, [f32; 2], Ctx, LayerHit<T>) -> LayerHit<T>,
) -> LayerHit<T> {
    let mut stack = vec![HitFrame {
        el: root,
        scroll_acc,
        ctx,
        phase: HitPhase::Enter,
    }];
    let mut result = LayerHit::Miss;
    while let Some(frame) = stack.pop() {
        match frame.phase {
            HitPhase::Enter => {
                if !matches!(result, LayerHit::Miss) {
                    continue;
                }
                let Some((escaped, next_acc, child_ctx)) =
                    enter(frame.el, frame.scroll_acc, frame.ctx)
                else {
                    continue;
                };
                stack.push(HitFrame {
                    el: frame.el,
                    scroll_acc: escaped,
                    ctx: frame.ctx,
                    phase: HitPhase::After,
                });
                push_z_hit_children(&mut stack, frame.el, next_acc, child_ctx);
            }
            HitPhase::After => {
                result = after(frame.el, frame.scroll_acc, frame.ctx, result);
            }
        }
    }
    result
}

fn hit_walk_first<'a, T, Ctx: Copy>(
    root: &'a Element,
    scroll_acc: [f32; 2],
    ctx: Ctx,
    mut enter: impl FnMut(&'a Element, [f32; 2], Ctx) -> Option<([f32; 2], [f32; 2], Ctx)>,
    mut after: impl FnMut(&'a Element, [f32; 2], Ctx, Option<T>) -> Option<T>,
) -> Option<T> {
    let mut stack = vec![HitFrame {
        el: root,
        scroll_acc,
        ctx,
        phase: HitPhase::Enter,
    }];
    let mut result = None;
    while let Some(frame) = stack.pop() {
        match frame.phase {
            HitPhase::Enter => {
                if result.is_some() {
                    continue;
                }
                let Some((escaped, next_acc, child_ctx)) =
                    enter(frame.el, frame.scroll_acc, frame.ctx)
                else {
                    continue;
                };
                stack.push(HitFrame {
                    el: frame.el,
                    scroll_acc: escaped,
                    ctx: frame.ctx,
                    phase: HitPhase::After,
                });
                push_z_hit_children(&mut stack, frame.el, next_acc, child_ctx);
            }
            HitPhase::After => {
                result = after(frame.el, frame.scroll_acc, frame.ctx, result);
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
) -> Option<([f32; 2], [f32; 2])> {
    if pointer_events_blocked(el) {
        return None;
    }
    let escaped = escape_scroll_acc(el, scroll_acc);
    let b = el.layout_bounds;
    let [vx, vy] = visual_xy(el, escaped);
    let inside = x >= vx && x <= vx + b[2] && y >= vy && y <= vy + b[3];
    let is_body_portal =
        el.template_id.as_deref() == Some(crate::layout::layout_tree::PORTAL_LAYER_ID);
    if !is_body_portal && crate::core::tree::element_clips_overflow(el) && !inside {
        return None;
    }
    let next_acc = if is_body_portal {
        escaped
    } else {
        next_hit_scroll_acc(el, escaped)
    };
    Some((escaped, next_acc))
}

/// Consistent with the string shown in [`crate::components::input`]: `preedit` is inserted at the `ime_anchor` byte position of `committed`.
pub(super) fn input_display_text(
    committed: &str,
    preedit: &str,
    ime_anchor: Option<usize>,
) -> String {
    if preedit.is_empty() {
        return committed.to_string();
    }
    let mut a = ime_anchor.unwrap_or(committed.len()).min(committed.len());
    a = committed.floor_char_boundary(a);
    format!("{}{}{}", &committed[..a], preedit, &committed[a..])
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
/// and CSS translate (same contributions as layout collect `render_*`).
#[inline]
pub(super) fn visual_xy(el: &Element, scroll_acc: [f32; 2]) -> [f32; 2] {
    let b = el.layout_bounds;
    let tx = el.style.translate[0] + b[2] * el.style.translate_pct[0];
    let ty = el.style.translate[1] + b[3] * el.style.translate_pct[1];
    [
        b[0] - scroll_acc[0] + el.sticky_visual_offset[0] + tx,
        b[1] - scroll_acc[1] + el.sticky_visual_offset[1] + ty,
    ]
}

/// Paint-space hit box after CSS `scale` (center origin), matching layout collect.
///
/// Pointer / `:hover` must use the scaled box. Unscaled layout hit + `hover:scale-*`
/// leaves a ring where enter/leave oscillates and CSS tweens never settle.
#[inline]
pub(super) fn visual_hit_box(el: &Element, scroll_acc: [f32; 2]) -> [f32; 4] {
    let b = el.layout_bounds;
    let [vx, vy] = visual_xy(el, scroll_acc);
    let sx = if el.style.scale[0].is_finite() && el.style.scale[0] > 0.0 {
        el.style.scale[0]
    } else {
        1.0
    };
    let sy = if el.style.scale[1].is_finite() && el.style.scale[1] > 0.0 {
        el.style.scale[1]
    } else {
        1.0
    };
    if (sx - 1.0).abs() <= f32::EPSILON && (sy - 1.0).abs() <= f32::EPSILON {
        return [vx, vy, b[2], b[3]];
    }
    let sw = b[2] * sx;
    let sh = b[3] * sy;
    [vx + (b[2] - sw) * 0.5, vy + (b[3] - sh) * 0.5, sw, sh]
}

/// Visual border box for hit-testing / inspector pick.
/// Prefers the last collect paint rect when it is still current (translate/sticky/scale).
#[cfg(feature = "devtools")]
#[inline]
pub(super) fn visual_border_box(el: &Element, scroll_acc: [f32; 2]) -> [f32; 4] {
    if el.paint_visual_gen != 0
        && el.paint_visual_gen == crate::app::devtools::highlight_paint_gen()
    {
        return el.paint_visual_bounds;
    }
    let [vx, vy] = visual_xy(el, scroll_acc);
    let b = el.layout_bounds;
    [vx, vy, b[2], b[3]]
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
        |el, scroll_acc, ()| {
            let (escaped, next_acc) = hit_enter_clipped(el, x, y, scroll_acc)?;
            Some((escaped, next_acc, ()))
        },
        |el, scroll_acc, (), child_hit| {
            let bubbled = child_hit.bubble(el.cursor.clone());
            if !matches!(bubbled, LayerHit::Miss) {
                return bubbled;
            }
            let [vx, vy] = visual_xy(el, scroll_acc);
            if element_self_pointer_hit(el, x, y, vx, vy) {
                LayerHit::Hit(el.cursor.clone())
            } else {
                LayerHit::Miss
            }
        },
    )
}

pub(super) fn apply_cursor(
    window: &winit::window::Window,
    event_loop: Option<&winit::event_loop::ActiveEventLoop>,
    cache: &mut crate::style::CustomCursorCache,
    value: Option<CursorValue>,
) {
    crate::style::cursor_image::apply_cursor(window, event_loop, cache, value);
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

pub(super) fn find_focus_id_at(
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
        |el, scroll_acc, ()| {
            let (escaped, next_acc) = hit_enter_clipped(el, x, y, scroll_acc)?;
            Some((escaped, next_acc, ()))
        },
        |el, scroll_acc, (), child_hit| {
            let bubbled = child_hit.bubble(el.focus_id.clone());
            if !matches!(bubbled, LayerHit::Miss) {
                return bubbled;
            }
            let [vx, vy] = visual_xy(el, scroll_acc);
            if element_self_pointer_hit(el, x, y, vx, vy) {
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
        |el, scroll_acc, text_edit_fid| {
            let next_acc = next_hit_scroll_acc(el, scroll_acc);
            let child_fid = inherit_text_edit_focus_id(el, text_edit_fid);
            Some((scroll_acc, next_acc, child_fid))
        },
        |el, scroll_acc, text_edit_fid, child_hit| {
            if child_hit.is_some() {
                return child_hit;
            }
            let child_fid = inherit_text_edit_focus_id(el, text_edit_fid);
            if let Some(text) = &el.text {
                if el.text_style.font_family.as_deref() == Some("lucide") {
                    return None;
                }
                if let Some(id) = child_fid {
                    if id == format!("{}{}", crate::components::INPUT_FOCUS_PREFIX, rest)
                        || id == format!("{}{}", crate::components::TEXTAREA_FOCUS_PREFIX, rest)
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
        |el, scroll_acc, text_edit_fid| {
            let (escaped, next_acc) = hit_enter_clipped(el, x, y, scroll_acc)?;
            let child_fid = inherit_text_edit_focus_id(el, text_edit_fid);
            Some((escaped, next_acc, child_fid))
        },
        |el, scroll_acc, text_edit_fid, child_hit: LayerHit<Hit>| {
            if !matches!(child_hit, LayerHit::Miss) {
                return child_hit;
            }
            let child_text_edit_fid = inherit_text_edit_focus_id(el, text_edit_fid);
            let b = el.layout_bounds;
            let [vx, vy] = visual_xy(el, scroll_acc);
            let inside = x >= vx && x <= vx + b[2] && y >= vy && y <= vy + b[3];
            if let Some(text) = &el.text {
                if el.text_style.font_family.as_deref() != Some("lucide") {
                    if let Some(id) = child_text_edit_fid {
                        let rest = id
                            .strip_prefix(crate::components::INPUT_FOCUS_PREFIX)
                            .or_else(|| id.strip_prefix(crate::components::TEXTAREA_FOCUS_PREFIX));
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
                    .strip_prefix(crate::components::INPUT_FOCUS_PREFIX)
                    .or_else(|| id.strip_prefix(crate::components::TEXTAREA_FOCUS_PREFIX));
                if let Some(rest) = rest {
                    if inside && !pointer_on_text_edit_adornment(el, x, y, scroll_acc) {
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
            if element_self_pointer_hit(el, x, y, vx, vy) {
                LayerHit::Hit(None)
            } else {
                LayerHit::Miss
            }
        },
    )
}

fn is_text_edit_focus_id(id: &str) -> bool {
    id.starts_with(crate::components::INPUT_FOCUS_PREFIX)
        || id.starts_with(crate::components::TEXTAREA_FOCUS_PREFIX)
}

/// True when `(x,y)` is on a clear/reveal control inside a text-edit chrome.
fn pointer_on_text_edit_adornment(el: &Element, x: f32, y: f32, scroll_acc: [f32; 2]) -> bool {
    enum Phase {
        Enter,
        After,
    }
    struct Frame<'a> {
        el: &'a Element,
        scroll_acc: [f32; 2],
        phase: Phase,
    }
    let mut stack = vec![Frame {
        el,
        scroll_acc,
        phase: Phase::Enter,
    }];
    let mut hit = false;
    while let Some(frame) = stack.pop() {
        match frame.phase {
            Phase::Enter => {
                if hit {
                    continue;
                }
                if pointer_events_blocked(frame.el) {
                    continue;
                }
                let mut next_acc = frame.scroll_acc;
                if frame.el.style.overflow.x != taffy::style::Overflow::Visible {
                    next_acc[0] += frame.el.scroll_offset[0];
                }
                if frame.el.style.overflow.y != taffy::style::Overflow::Visible {
                    next_acc[1] += frame.el.scroll_offset[1];
                }
                stack.push(Frame {
                    el: frame.el,
                    scroll_acc: frame.scroll_acc,
                    phase: Phase::After,
                });
                for child in frame.el.children.iter().rev() {
                    stack.push(Frame {
                        el: child,
                        scroll_acc: next_acc,
                        phase: Phase::Enter,
                    });
                }
            }
            Phase::After => {
                if hit {
                    continue;
                }
                if frame
                    .el
                    .focus_id
                    .as_deref()
                    .is_some_and(is_text_edit_focus_id)
                {
                    continue;
                }
                let b = frame.el.layout_bounds;
                let [vx, vy] = visual_xy(frame.el, frame.scroll_acc);
                let is_adornment = frame.el.on_click.is_some()
                    || frame.el.text_style.font_family.as_deref() == Some("lucide")
                    || frame
                        .el
                        .cursor
                        .as_ref()
                        .is_some_and(|c| c.kind == CursorKind::Pointer);
                hit = is_adornment
                    && claims_own_bounds(frame.el)
                    && x >= vx
                    && x <= vx + b[2]
                    && y >= vy
                    && y <= vy + b[3];
            }
        }
    }
    hit
}

fn is_selectable_static_text(el: &Element) -> bool {
    crate::components::label::selectable_text_key(el).is_some()
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
        |el, scroll_acc, ()| {
            let mut next_acc = scroll_acc;
            if el.style.overflow.x != taffy::style::Overflow::Visible {
                next_acc[0] += el.scroll_offset[0];
            }
            if el.style.overflow.y != taffy::style::Overflow::Visible {
                next_acc[1] += el.scroll_offset[1];
            }
            Some((scroll_acc, next_acc, ()))
        },
        |el, scroll_acc, (), child_hit| {
            if child_hit.is_some() {
                return child_hit;
            }
            if is_selectable_static_text(el)
                && crate::components::label::selectable_text_key(el).as_deref() == Some(key)
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
        |el, scroll_acc, ()| {
            let (escaped, next_acc) = hit_enter_clipped(el, x, y, scroll_acc)?;
            Some((escaped, next_acc, ()))
        },
        |el, scroll_acc, (), child_hit| {
            if !matches!(child_hit, LayerHit::Miss) {
                return child_hit;
            }
            let b = el.layout_bounds;
            let [vx, vy] = visual_xy(el, scroll_acc);
            let inside = x >= vx && x <= vx + b[2] && y >= vy && y <= vy + b[3];
            if is_selectable_static_text(el) && inside {
                if let Some(key) = crate::components::label::selectable_text_key(el) {
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
            if element_self_pointer_hit(el, x, y, vx, vy) {
                LayerHit::Hit(None)
            } else {
                LayerHit::Miss
            }
        },
    )
}

#[inline]
pub(super) fn update_hovers(
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
pub(super) fn reconcile_pointer_hovers(
    el: &Element,
    x: f32,
    y: f32,
    scroll_offset: [f32; 2],
    cbs: &mut Vec<(HoverCb, bool)>,
    immediate_miss_leave: bool,
) {
    let target = match pick_hover_target(el, x, y, scroll_offset) {
        HoverHit::Hit(slot) => slot,
        HoverHit::Miss => None,
    };
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
            if rc_changed || reenter {
                LAST_HOVER_BY_SCOPE.with(|map| {
                    *map.borrow_mut().entry(scope).or_insert(None) = Some(next.clone());
                });
            }
            if reenter {
                cbs.push((next.cb, true));
            }
        }
        HOVER_MISS_STREAK.set(0);
        return;
    }
    // Different hoverable under the pointer — leave/enter immediately.
    if let Some(new) = target.clone() {
        HOVER_MISS_STREAK.set(0);
        if let Some(old) = prev {
            cbs.push((old.cb, false));
        }
        cbs.push((new.cb, true));
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
    if let Some(old) = prev {
        cbs.push((old.cb, false));
    }
    LAST_HOVER_BY_SCOPE.with(|map| {
        *map.borrow_mut().entry(scope).or_insert(None) = None;
    });
}

/// Soft-invalidate after handler `Rc`s are replaced (DevTools dock / `reactive_view`).
///
/// Does **not** fire leave. The next [`update_hovers`] re-resolves by [`HoverKey`]: same
/// `focus_id` only re-asserts enter (no spring 0→1 thrash); pointer moved away emits leave.
pub(crate) fn invalidate_hover_handlers() {
    HOVER_NEEDS_RESYNC.set(true);
}

/// Hard reset: fire leave and drop the cached slot (tests / teardown).
///
/// Prefer [`invalidate_hover_handlers`] on live rebuild paths — hard leave here races with
/// same-frame re-enter and spring ticks, which flickered gallery sidebar hover.
#[cfg(test)]
pub(super) fn clear_hover_tracking() {
    HOVER_NEEDS_RESYNC.set(true);
    HOVER_MISS_STREAK.set(0);
    let scope = crate::core::reactivity::signal_key_scope();
    let prev = LAST_HOVER_BY_SCOPE.with(|map| map.borrow_mut().insert(scope, None).flatten());
    if let Some(old) = prev {
        (*old.cb.borrow_mut())(false);
    }
}

/// Result of hover hit-testing (distinct from “no handler”).
///
/// - [`HoverHit::Miss`]: pointer missed this subtree — try siblings.
/// - [`HoverHit::Hit`]: pointer hit this subtree. `None` means absorb (block siblings
///   below, e.g. modal underlay) but ancestors may still supply `on_hover`.
enum HoverHit {
    Miss,
    Hit(Option<HoverSlot>),
}

fn pick_hover_target(el: &Element, x: f32, y: f32, scroll_acc: [f32; 2]) -> HoverHit {
    match hit_walk_layer(
        el,
        scroll_acc,
        (),
        |el, scroll_acc, ()| {
            if pointer_events_blocked(el) {
                return None;
            }
            let escaped = if el.template_id.as_deref()
                == Some(crate::layout::layout_tree::PORTAL_LAYER_ID)
                || el.style.position_fixed
            {
                [0.0_f32, 0.0_f32]
            } else {
                scroll_acc
            };
            let b = el.layout_bounds;
            let is_body_portal =
                el.template_id.as_deref() == Some(crate::layout::layout_tree::PORTAL_LAYER_ID);
            if !is_body_portal && crate::core::tree::element_clips_overflow(el) {
                let [vx, vy] = visual_xy(el, escaped);
                let in_layout = x >= vx && x <= vx + b[2] && y >= vy && y <= vy + b[3];
                if !in_layout {
                    return None;
                }
            }
            let next_acc = if is_body_portal {
                escaped
            } else {
                next_hit_scroll_acc(el, escaped)
            };
            Some((escaped, next_acc, ()))
        },
        |el, scroll_acc, (), child_hit| {
            let hit = visual_hit_box(el, scroll_acc);
            let bubbled = match child_hit {
                LayerHit::Miss => LayerHit::Miss,
                LayerHit::Hit(slot) => LayerHit::Hit(slot.or_else(|| hover_slot_for(el, hit))),
            };
            if !matches!(bubbled, LayerHit::Miss) {
                return bubbled;
            }
            let b = el.layout_bounds;
            let inside = x >= hit[0] && x <= hit[0] + hit[2] && y >= hit[1] && y <= hit[1] + hit[3];
            if !inside {
                return LayerHit::Miss;
            }
            let [vx, vy] = visual_xy(el, scroll_acc);
            let in_layout = x >= vx && x <= vx + b[2] && y >= vy && y <= vy + b[3];
            if in_layout {
                if element_self_pointer_hit(el, x, y, vx, vy) {
                    LayerHit::Hit(hover_slot_for(el, hit))
                } else {
                    LayerHit::Miss
                }
            } else if claims_own_bounds(el) {
                LayerHit::Hit(hover_slot_for(el, hit))
            } else {
                LayerHit::Miss
            }
        },
    ) {
        LayerHit::Miss => HoverHit::Miss,
        LayerHit::Hit(slot) => HoverHit::Hit(slot),
    }
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
        |el, scroll_acc, ()| {
            if pointer_events_blocked(el) {
                return None;
            }
            let b = el.layout_bounds;
            let [vx, vy] = visual_xy(el, scroll_acc);
            if crate::core::tree::element_clips_overflow(el)
                && (x < vx || x > vx + b[2] || y < vy || y > vy + b[3])
            {
                return None;
            }
            let next_acc = next_hit_scroll_acc(el, scroll_acc);
            Some((scroll_acc, next_acc, ()))
        },
        |el, scroll_acc, (), child_hit| {
            if child_hit.is_some() {
                return child_hit;
            }
            let [vx, vy] = visual_xy(el, scroll_acc);
            if element_self_pointer_hit(el, x, y, vx, vy) {
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
            return (mx, my, leaf_bounds, hit_style);
        }
    }

    let mx = if style.overflow.allows_caret_scroll() || style.white_space == WhiteSpace::Nowrap {
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

    // Inner hot-zone: paint ensure is skipped during text drag, so this no longer
    // fights `ensure_caret_visible_scroll_x` (that pair caused bounce before).
    const EDGE_SLOP: f32 = 16.0;
    let outside_right = pointer_x > right - EDGE_SLOP;
    let outside_left = pointer_x < left + EDGE_SLOP;

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
    if !outside_left && !outside_right {
        return TextDragHScrollTick::Idle;
    }

    let sx = sx_raw.clamp(0.0, max_sx);
    let dt = dt.clamp(1.0 / 240.0, 0.05);
    // px/s — base rate + boost by how far past the edge the pointer is.
    let over = if outside_right {
        (pointer_x - (right - EDGE_SLOP)).max(0.0)
    } else {
        ((left + EDGE_SLOP) - pointer_x).max(0.0)
    };
    let speed = (280.0 + over * 14.0).min(900.0);
    let step = speed * dt;
    let next = if outside_right {
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
    use crate::utils::text_field::textarea_scroll_rest;

    let Some((_, _, _style, _)) = find_input_text_leaf_for_rest(root, None, field_id) else {
        return TextDragVScrollTick::Idle;
    };

    let scroll_rest = textarea_scroll_rest(field_id);
    let vp_fid = focus_id_for_scroll_viewport(&scroll_rest);
    let Some((vp_layout, vp_scroll)) = find_bounds_by_focus_id(root, &vp_fid, [0.0, 0.0]) else {
        return TextDragVScrollTick::Idle;
    };
    let top = vp_layout[1] - vp_scroll[1];
    let bottom = top + vp_layout[3];
    let vh_layout = vp_layout[3].max(1.0);

    // Match horizontal drag: inner hot-zone (paint ensure skipped during text drag).
    const EDGE_SLOP: f32 = 16.0;
    let outside_bottom = pointer_y > bottom - EDGE_SLOP;
    let outside_top = pointer_y < top + EDGE_SLOP;
    if !outside_top && !outside_bottom {
        return TextDragVScrollTick::Idle;
    }

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
    let dt = dt.clamp(1.0 / 240.0, 0.05);
    let over = if outside_bottom {
        (pointer_y - (bottom - EDGE_SLOP)).max(0.0)
    } else {
        ((top + EDGE_SLOP) - pointer_y).max(0.0)
    };
    let speed = (280.0 + over * 14.0).min(900.0);
    let step = speed * dt;
    let next = if outside_bottom {
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
    let activate = matches!(
        event.logical_key,
        Key::Named(NamedKey::Space) | Key::Named(NamedKey::Enter)
    );
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
mod hover_occlusion_tests {
    use super::*;
    use crate::primitives::interactive::interactive;
    use std::cell::Cell;
    use std::rc::Rc;

    #[test]
    fn claiming_underlay_blocks_hover_on_content_below() {
        clear_hover_tracking();
        let below_hits = Rc::new(Cell::new(0u32));
        let below_hits_cb = below_hits.clone();
        let mut below: Element = interactive(Element::new().w(100.0).h(100.0))
            .on_hover(move |h| {
                if h {
                    below_hits_cb.set(below_hits_cb.get() + 1);
                }
            })
            .into();
        below.layout_bounds = [0.0, 0.0, 100.0, 100.0];

        // Full-screen underlay claims bounds via on_click but has no on_hover.
        let mut underlay = Element::new().w(100.0).h(100.0).on_click(|| {});
        underlay.layout_bounds = [0.0, 0.0, 100.0, 100.0];
        underlay.style.z_index = 1000;

        let mut root = Element::new().child(below).child(underlay);
        root.layout_bounds = [0.0, 0.0, 100.0, 100.0];

        let mut to_run = Vec::new();
        update_hovers(&root, 50.0, 50.0, [0.0, 0.0], &mut to_run);
        for (cb, is_hover) in to_run {
            (*cb.borrow_mut())(is_hover);
        }
        assert_eq!(
            below_hits.get(),
            0,
            "content under a claiming underlay must not receive hover"
        );
    }

    #[test]
    fn claiming_underlay_blocks_scrollbar_focus_and_cursor() {
        use crate::components::scrollbar::SCROLLBAR_TRACK_FOCUS_PREFIX;
        use crate::core::tree::CursorKind;

        let mut track = Element::new()
            .w(12.0)
            .h(100.0)
            .cursor_kind(CursorKind::Pointer);
        track.focus_id = Some(format!("{SCROLLBAR_TRACK_FOCUS_PREFIX}gallery:y"));
        track.layout_bounds = [88.0, 0.0, 12.0, 100.0];

        // Underlay is an earlier sibling with higher z — DOM-order-only hit would miss it.
        let mut underlay = Element::new().w(100.0).h(100.0).on_click(|| {});
        underlay.layout_bounds = [0.0, 0.0, 100.0, 100.0];
        underlay.style.z_index = 1000;

        let mut root = Element::new().child(underlay).child(track);
        root.layout_bounds = [0.0, 0.0, 100.0, 100.0];

        assert!(
            find_focus_id_at(&root, 94.0, 50.0, [0.0, 0.0]).is_none(),
            "scrollbar track under modal underlay must not win focus hit-test"
        );
        assert!(
            pick_cursor(&root, 94.0, 50.0, [0.0, 0.0]).is_none(),
            "scrollbar Pointer cursor must not leak through claiming underlay"
        );
    }

    #[test]
    fn claiming_underlay_blocks_scroll_chain_underneath() {
        use crate::core::scroll_viewport::SCROLL_AREA_FOCUS_PREFIX;

        let mut area = Element::new().w(100.0).h(100.0);
        area.focus_id = Some(format!("{SCROLL_AREA_FOCUS_PREFIX}gallery"));
        area.layout_bounds = [0.0, 0.0, 100.0, 100.0];

        let mut underlay = Element::new().w(100.0).h(100.0).on_click(|| {});
        underlay.layout_bounds = [0.0, 0.0, 100.0, 100.0];
        underlay.style.z_index = 1000;

        let mut root = Element::new().child(underlay).child(area);
        root.layout_bounds = [0.0, 0.0, 100.0, 100.0];

        let mut chain = Vec::new();
        let hit = crate::app::scroll_support::collect_scroll_chain_at(
            &root,
            50.0,
            50.0,
            [0.0, 0.0],
            &mut chain,
        );
        assert!(hit, "underlay must absorb the pointer");
        assert!(
            chain.is_empty(),
            "gallery scroll area under underlay must not enter hover/wheel chain"
        );
    }

    #[test]
    fn claiming_overlay_blocks_input_text_hit_underneath() {
        use crate::components::TEXTAREA_FOCUS_PREFIX;
        use crate::layout::layout_tree::PORTAL_LAYER_ID;

        // Page textarea under a color-picker-like portal panel (claims via on_click).
        let mut leaf = Element::new().w(200.0).h(80.0);
        leaf.layout_bounds = [40.0, 40.0, 200.0, 80.0];
        leaf.text = Some("hello".into());
        leaf.focus_id = Some(format!("{TEXTAREA_FOCUS_PREFIX}under-ta"));

        let mut field = Element::new()
            .w(200.0)
            .h(80.0)
            .on_click(|| {})
            .cursor_text();
        field.layout_bounds = [40.0, 40.0, 200.0, 80.0];
        field.focus_id = Some(format!("{TEXTAREA_FOCUS_PREFIX}under-ta"));
        field = field.child(leaf);

        let mut panel = Element::new().w(180.0).h(160.0).on_click(|| {});
        panel.layout_bounds = [60.0, 20.0, 180.0, 160.0];
        panel.style.z_index = 1001;
        panel.set_attr(crate::layout::layout_tree::ATTR_HOISTED_PORTAL, "1");

        let mut layer = Element::new()
            .w(400.0)
            .h(400.0)
            .pointer_events_box_none()
            .template_id(PORTAL_LAYER_ID)
            .child(panel);
        layer.layout_bounds = [0.0, 0.0, 400.0, 400.0];

        let mut root = Element::new().w(400.0).h(400.0).child(field).child(layer);
        root.layout_bounds = [0.0, 0.0, 400.0, 400.0];

        // Before the LayerHit fix, find_input_text_at walked past the empty panel
        // and focused the textarea under the SV / popover silhouette.
        assert!(
            find_input_text_at(&root, None, 120.0, 80.0, [0.0, 0.0]).is_none(),
            "textarea under a claiming floating panel must not win input hit-test"
        );
        assert_eq!(
            find_focus_id_at(&root, 120.0, 80.0, [0.0, 0.0]),
            None,
            "focus hit-test must also stop on the claiming panel"
        );
    }

    #[test]
    fn button_hover_bubbles_from_child_label() {
        clear_hover_tracking();
        let hovered = Rc::new(Cell::new(false));
        let hovered_cb = hovered.clone();
        let mut label = Element::new().w(80.0).h(20.0);
        label.layout_bounds = [10.0, 10.0, 80.0, 20.0];

        let mut btn: Element = interactive(Element::new().w(100.0).h(40.0).child(label))
            .on_hover(move |h| hovered_cb.set(h))
            .into();
        btn.layout_bounds = [0.0, 0.0, 100.0, 40.0];

        let mut root = Element::new().child(btn);
        root.layout_bounds = [0.0, 0.0, 100.0, 40.0];

        let mut to_run = Vec::new();
        // Pointer over the inner label (not the button padding).
        update_hovers(&root, 40.0, 20.0, [0.0, 0.0], &mut to_run);
        for (cb, is_hover) in to_run {
            (*cb.borrow_mut())(is_hover);
        }
        assert!(
            hovered.get(),
            "Button on_hover must fire when pointer is over a non-hover child"
        );
    }

    #[test]
    fn update_hovers_is_diff_based_not_broadcast() {
        clear_hover_tracking();
        let a_hits = Rc::new(Cell::new(0u32));
        let b_hits = Rc::new(Cell::new(0u32));
        let a_hits_cb = a_hits.clone();
        let b_hits_cb = b_hits.clone();

        let mut a: Element = interactive(Element::new().w(40.0).h(40.0))
            .on_hover(move |h| {
                if h {
                    a_hits_cb.set(a_hits_cb.get() + 1);
                }
            })
            .into();
        a.layout_bounds = [0.0, 0.0, 40.0, 40.0];

        let mut b: Element = interactive(Element::new().w(40.0).h(40.0))
            .on_hover(move |h| {
                if h {
                    b_hits_cb.set(b_hits_cb.get() + 1);
                }
            })
            .into();
        b.layout_bounds = [50.0, 0.0, 40.0, 40.0];

        let mut root = Element::new().child(a).child(b);
        root.layout_bounds = [0.0, 0.0, 100.0, 40.0];

        let mut to_run = Vec::new();
        update_hovers(&root, 20.0, 20.0, [0.0, 0.0], &mut to_run);
        for (cb, is_hover) in to_run.drain(..) {
            (*cb.borrow_mut())(is_hover);
        }
        assert_eq!(a_hits.get(), 1);
        assert_eq!(b_hits.get(), 0);

        // Same target again: no callbacks (diff-based).
        update_hovers(&root, 20.0, 20.0, [0.0, 0.0], &mut to_run);
        assert!(to_run.is_empty());
        assert_eq!(a_hits.get(), 1);

        // Move to B: leave A + enter B only.
        update_hovers(&root, 70.0, 20.0, [0.0, 0.0], &mut to_run);
        assert_eq!(to_run.len(), 2);
        for (cb, is_hover) in to_run.drain(..) {
            (*cb.borrow_mut())(is_hover);
        }
        assert_eq!(a_hits.get(), 1);
        assert_eq!(b_hits.get(), 1);
    }

    #[test]
    fn hover_identity_survives_handler_rc_rebuild() {
        clear_hover_tracking();
        let enters = Rc::new(Cell::new(0u32));
        let leaves = Rc::new(Cell::new(0u32));
        let enters_cb = enters.clone();
        let leaves_cb = leaves.clone();

        let mut btn: Element = interactive(Element::new().w(40.0).h(40.0))
            .focus_id("stable-btn")
            .on_hover(move |h| {
                if h {
                    enters_cb.set(enters_cb.get() + 1);
                } else {
                    leaves_cb.set(leaves_cb.get() + 1);
                }
            })
            .into();
        btn.layout_bounds = [0.0, 0.0, 40.0, 40.0];
        let mut root = Element::new().child(btn);
        root.layout_bounds = [0.0, 0.0, 40.0, 40.0];

        let mut to_run = Vec::new();
        update_hovers(&root, 20.0, 20.0, [0.0, 0.0], &mut to_run);
        for (cb, is_hover) in to_run.drain(..) {
            (*cb.borrow_mut())(is_hover);
        }
        assert_eq!(enters.get(), 1);
        assert_eq!(leaves.get(), 0);

        // Simulate reactive_view rebuild: new on_hover Rc, same focus_id.
        let enters_cb2 = enters.clone();
        let leaves_cb2 = leaves.clone();
        let mut btn2: Element = interactive(Element::new().w(40.0).h(40.0))
            .focus_id("stable-btn")
            .on_hover(move |h| {
                if h {
                    enters_cb2.set(enters_cb2.get() + 1);
                } else {
                    leaves_cb2.set(leaves_cb2.get() + 1);
                }
            })
            .into();
        btn2.layout_bounds = [0.0, 0.0, 40.0, 40.0];
        root.children[0] = btn2;

        invalidate_hover_handlers();
        update_hovers(&root, 20.0, 20.0, [0.0, 0.0], &mut to_run);
        // Soft-invalidate + same focus_id: re-assert enter only (no leave).
        assert_eq!(to_run.len(), 1);
        assert!(to_run[0].1);
        for (cb, is_hover) in to_run.drain(..) {
            (*cb.borrow_mut())(is_hover);
        }
        assert_eq!(leaves.get(), 0);
        assert_eq!(enters.get(), 2);

        // Simulate button chrome: `{focus_id}_hover` stays true while the pointer rests.
        crate::core::reactivity::use_signal("stable-btn_hover", || false).set_silent(true);

        // Quiet Rc swap without invalidate must not re-fire when the hover signal is still true.
        let enters_cb3 = enters.clone();
        let leaves_cb3 = leaves.clone();
        let mut btn3: Element = interactive(Element::new().w(40.0).h(40.0))
            .focus_id("stable-btn")
            .on_hover(move |h| {
                if h {
                    enters_cb3.set(enters_cb3.get() + 1);
                } else {
                    leaves_cb3.set(leaves_cb3.get() + 1);
                }
            })
            .into();
        btn3.layout_bounds = [0.0, 0.0, 40.0, 40.0];
        root.children[0] = btn3;
        update_hovers(&root, 20.0, 20.0, [0.0, 0.0], &mut to_run);
        assert!(to_run.is_empty());
        assert_eq!(enters.get(), 2);
        assert_eq!(leaves.get(), 0);

        // Rc swap + cleared hover signal: re-assert enter (post-jank heal).
        crate::core::reactivity::use_signal("stable-btn_hover", || false).set_silent(false);
        let enters_cb4 = enters.clone();
        let leaves_cb4 = leaves.clone();
        let mut btn4: Element = interactive(Element::new().w(40.0).h(40.0))
            .focus_id("stable-btn")
            .on_hover(move |h| {
                if h {
                    enters_cb4.set(enters_cb4.get() + 1);
                } else {
                    leaves_cb4.set(leaves_cb4.get() + 1);
                }
            })
            .into();
        btn4.layout_bounds = [0.0, 0.0, 40.0, 40.0];
        root.children[0] = btn4;
        update_hovers(&root, 20.0, 20.0, [0.0, 0.0], &mut to_run);
        assert_eq!(to_run.len(), 1);
        assert!(to_run[0].1);
        for (cb, is_hover) in to_run.drain(..) {
            (*cb.borrow_mut())(is_hover);
        }
        assert_eq!(enters.get(), 3);
        assert_eq!(leaves.get(), 0);
    }

    #[test]
    fn hover_miss_uses_one_frame_hysteresis() {
        clear_hover_tracking();
        let leaves = Rc::new(Cell::new(0u32));
        let leaves_cb = leaves.clone();
        let mut btn: Element = interactive(Element::new().w(40.0).h(40.0))
            .focus_id("hyst-btn")
            .on_hover(move |h| {
                if !h {
                    leaves_cb.set(leaves_cb.get() + 1);
                }
            })
            .into();
        btn.layout_bounds = [0.0, 0.0, 40.0, 40.0];
        let mut root = Element::new().child(btn);
        root.layout_bounds = [0.0, 0.0, 40.0, 40.0];

        let mut to_run = Vec::new();
        update_hovers(&root, 20.0, 20.0, [0.0, 0.0], &mut to_run);
        for (cb, is_hover) in to_run.drain(..) {
            (*cb.borrow_mut())(is_hover);
        }

        // First miss: hold hover — but the deferred leave must pump its own frame.
        // `update_hovers` only runs from `CursorMoved` and the redraw pass, so for a pointer
        // that leaves the element and then stops, nothing else produces miss #2 and the leave
        // would be lost forever (Toast's hover stack then stays expanded with every
        // auto-dismiss timer frozen).
        let _ = crate::primitives::interactive::take_hover_paint_dirty();
        update_hovers(&root, 100.0, 100.0, [0.0, 0.0], &mut to_run);
        assert!(to_run.is_empty());
        assert_eq!(leaves.get(), 0);
        assert!(
            crate::primitives::interactive::take_hover_paint_dirty(),
            "first miss must request a frame so miss #2 can arrive under a still pointer"
        );

        // Second miss: leave.
        update_hovers(&root, 100.0, 100.0, [0.0, 0.0], &mut to_run);
        assert_eq!(to_run.len(), 1);
        assert!(!to_run[0].1);
        for (cb, is_hover) in to_run.drain(..) {
            (*cb.borrow_mut())(is_hover);
        }
        assert_eq!(leaves.get(), 1);
    }

    #[test]
    fn scroll_viewport_visible_overflow_does_not_steal_sidebar_hover() {
        // Gallery shell: main ScrollArea is painted after the sidebar (later sibling = topmost).
        // If `scroll:` viewports with overflow cleared to Visible skip clip, a claiming content
        // tile whose abs box overlaps the sidebar coords absorbs the hit → sidebar leave/enter
        // thrash while the main pane scrolls under a still pointer.
        clear_hover_tracking();
        let enters = Rc::new(Cell::new(0u32));
        let leaves = Rc::new(Cell::new(0u32));
        let enters_cb = enters.clone();
        let leaves_cb = leaves.clone();

        let mut side: Element = interactive(Element::new().w(180.0).h(32.0))
            .focus_id("gal-nav-layout")
            .on_hover(move |h| {
                if h {
                    enters_cb.set(enters_cb.get() + 1);
                } else {
                    leaves_cb.set(leaves_cb.get() + 1);
                }
            })
            .into();
        side.layout_bounds = [8.0, 100.0, 180.0, 32.0];
        // Claiming paint so Hit(None) would absorb if tested without clip.
        side.style.bg_color = [0.2, 0.2, 0.2, 1.0];

        let mut tile = Element::new();
        tile.layout_bounds = [8.0, 100.0, 64.0, 64.0];
        tile.style.bg_color = [1.0, 0.0, 0.0, 1.0];

        let mut viewport = Element::new();
        viewport.focus_id = Some("scroll:gal-layout".into());
        viewport.style.overflow.x = taffy::style::Overflow::Visible;
        viewport.style.overflow.y = taffy::style::Overflow::Visible;
        viewport.layout_bounds = [208.0, 0.0, 600.0, 800.0];
        viewport.scroll_offset = [0.0, 120.0];
        viewport = viewport.child(tile);

        let mut root = Element::new().child(side).child(viewport);
        root.layout_bounds = [0.0, 0.0, 808.0, 800.0];

        let mut to_run = Vec::new();
        update_hovers(&root, 40.0, 110.0, [0.0, 0.0], &mut to_run);
        assert_eq!(to_run.len(), 1, "sidebar button must win hover");
        assert!(to_run[0].1);
        for (cb, is_hover) in to_run.drain(..) {
            (*cb.borrow_mut())(is_hover);
        }
        assert_eq!(enters.get(), 1);
        assert_eq!(leaves.get(), 0);

        // Scroll offset changes every frame — hover identity must stay on the sidebar item.
        viewport_scroll_bump(&mut root, 180.0);
        update_hovers(&root, 40.0, 110.0, [0.0, 0.0], &mut to_run);
        assert!(
            to_run.is_empty(),
            "scroll must not leave/enter sidebar hover, got {} cbs",
            to_run.len()
        );
        assert_eq!(enters.get(), 1);
        assert_eq!(leaves.get(), 0);

        viewport_scroll_bump(&mut root, 240.0);
        update_hovers(&root, 40.0, 110.0, [0.0, 0.0], &mut to_run);
        assert!(to_run.is_empty());
        assert_eq!(leaves.get(), 0);
    }

    #[test]
    fn scroll_content_leave_fires_immediately_under_stationary_pointer() {
        clear_hover_tracking();
        let leaves = Rc::new(Cell::new(0u32));
        let leaves_cb = leaves.clone();

        let mut btn: Element = interactive(Element::new().w(80.0).h(32.0))
            .focus_id("scroll-btn")
            .on_hover(move |h| {
                if !h {
                    leaves_cb.set(leaves_cb.get() + 1);
                }
            })
            .into();
        btn.layout_bounds = [20.0, 100.0, 80.0, 32.0];

        let mut viewport = Element::new();
        viewport.focus_id = Some("scroll:main".into());
        viewport.style.overflow.y = taffy::style::Overflow::Visible;
        viewport.layout_bounds = [0.0, 0.0, 200.0, 200.0];
        viewport.scroll_offset = [0.0, 0.0];
        viewport = viewport.child(btn);

        let mut root = Element::new().child(viewport);
        root.layout_bounds = [0.0, 0.0, 200.0, 200.0];

        let mut to_run = Vec::new();
        reconcile_pointer_hovers(&root, 40.0, 110.0, [0.0, 0.0], &mut to_run, false);
        assert_eq!(to_run.len(), 1);
        assert!(to_run[0].1);
        for (cb, is_hover) in to_run.drain(..) {
            (*cb.borrow_mut())(is_hover);
        }

        viewport_scroll_bump(&mut root, 80.0);
        reconcile_pointer_hovers(&root, 40.0, 110.0, [0.0, 0.0], &mut to_run, true);
        assert_eq!(
            to_run.len(),
            1,
            "scroll must leave immediately under still pointer"
        );
        assert!(!to_run[0].1);
        for (cb, is_hover) in to_run.drain(..) {
            (*cb.borrow_mut())(is_hover);
        }
        assert_eq!(leaves.get(), 1);
    }

    fn viewport_scroll_bump(root: &mut Element, sy: f32) {
        fn bump(el: &mut Element, sy: f32) -> bool {
            if el.focus_id.as_deref().is_some_and(|f| {
                f.starts_with(crate::core::scroll_viewport::SCROLL_VIEWPORT_FOCUS_PREFIX)
            }) {
                el.scroll_offset[1] = sy;
                if let Some(rest) = el.focus_id.as_deref().and_then(|f| {
                    f.strip_prefix(crate::core::scroll_viewport::SCROLL_VIEWPORT_FOCUS_PREFIX)
                }) {
                    crate::core::reactivity::use_signal(
                        &crate::core::scroll_viewport::scroll_offset_y_signal_key(rest),
                        || 0.0_f32,
                    )
                    .set_silent(sy);
                }
                return true;
            }
            for child in &mut el.children {
                if bump(child, sy) {
                    return true;
                }
            }
            false
        }
        if !bump(root, sy) {
            // Legacy gallery shell: sidebar + main viewport siblings.
            if let Some(vp) = root.children.get_mut(1) {
                vp.scroll_offset[1] = sy;
            }
        }
    }
}

#[cfg(test)]
mod focused_key_dispatch_tests {
    use super::*;
    use crate::core::reactivity::use_signal;
    use crate::primitives::interactive::interactive;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// Regression for `RefCell already borrowed`: extract handlers under an outer borrow,
    /// release it, then run — handlers may `signal.set` / nest `borrow_mut`.
    #[test]
    fn key_handlers_run_after_outer_borrow_released() {
        let state = Rc::new(RefCell::new(0u32));
        let state_cb = state.clone();
        let open = use_signal("tree_hit_key_reentry_open", || false);
        let root: Element = interactive(Element::new())
            .focus_id("k")
            .focusable(true)
            .on_click(move || {
                *state_cb.borrow_mut() += 1;
                open.set(true);
            })
            .into();

        // Outer immutable borrow (like input_controller before the fix).
        let outer = state.borrow();
        let handlers = focused_key_handlers(&root, "k").expect("handlers");
        assert!(handlers.focusable);
        let click = handlers.on_click.clone().expect("click");
        drop(outer); // must release before running
        click.borrow_mut()();
        assert_eq!(*state.borrow(), 1);
        assert!(open.peek());
    }
}

#[cfg(test)]
mod ime_scroll_bounds_tests {
    use super::*;
    use taffy::style::Overflow;

    #[test]
    fn find_bounds_by_focus_id_accumulates_scroll_for_ime_visual() {
        // Same space as window_events IME path: layout_bounds − scroll_acc.
        let mut input = Element::new().w(200.0).h(32.0);
        input.layout_bounds = [20.0, 300.0, 200.0, 32.0];
        input.focus_id = Some("input:gal-s2-input-a".into());

        let mut viewport = Element::new().w(400.0).h(200.0);
        viewport.style.overflow.y = Overflow::Scroll;
        viewport.scroll_offset = [0.0, 120.0];
        viewport.layout_bounds = [0.0, 0.0, 400.0, 200.0];
        viewport = viewport.child(input);

        let (bounds, scroll_acc) =
            find_bounds_by_focus_id(&viewport, "input:gal-s2-input-a", [0.0, 0.0])
                .expect("input focus");
        let visual_y = bounds[1] - scroll_acc[1];
        assert!(
            (scroll_acc[1] - 120.0).abs() < 0.5,
            "scroll_acc={scroll_acc:?}"
        );
        assert!(
            (visual_y - 180.0).abs() < 0.5,
            "IME visual y must track scroll (300−120), got {visual_y}"
        );

        viewport.scroll_offset = [0.0, 200.0];
        let (bounds2, sa2) =
            find_bounds_by_focus_id(&viewport, "input:gal-s2-input-a", [0.0, 0.0]).unwrap();
        assert!(((bounds2[1] - sa2[1]) - 100.0).abs() < 0.5);
    }

    #[test]
    fn find_bounds_prefers_scrolled_duplicate_focus_id() {
        use crate::core::reactivity::use_signal;
        use crate::core::scroll_viewport::scroll_offset_y_signal_key;

        let mut ghost = Element::new();
        ghost.focus_id = Some("input:dup".into());
        ghost.layout_bounds = [20.0, 400.0, 200.0, 32.0];

        let mut real = Element::new();
        real.focus_id = Some("input:dup".into());
        real.layout_bounds = [20.0, 400.0, 200.0, 32.0];

        let mut viewport = Element::new();
        viewport.focus_id = Some("scroll:root-scroll".into());
        viewport.style.overflow.y = Overflow::Visible; // overflow wrong, but scroll: still counts
        viewport.scroll_offset = [0.0, 220.0];
        use_signal(&scroll_offset_y_signal_key("root-scroll"), || 0.0_f32).set_silent(220.0);
        viewport = viewport.child(real);

        let root = Element::new().child(ghost).child(viewport);
        let (bounds, sa) = find_bounds_by_focus_id(&root, "input:dup", [0.0, 0.0]).unwrap();
        assert!(
            (sa[1] - 220.0).abs() < 0.5,
            "must prefer scrolled match, sa={sa:?}"
        );
        assert!(((bounds[1] - sa[1]) - 180.0).abs() < 0.5);
    }
}

#[cfg(test)]
mod textarea_text_hit_tests {
    use super::*;
    use crate::components::TEXTAREA_FOCUS_PREFIX;
    use crate::core::scroll_viewport::focus_id_for_scroll_viewport;
    use crate::utils::text_field::text_edit_line_box_px;
    use taffy::style::Overflow;

    fn textarea_like_tree() -> (Element, f32) {
        let lh = text_edit_line_box_px(14.0, None, None);
        let field_id = "ta-hit";
        let rest = "ta-hit-scroll";

        let mut text = Element::new().w(300.0);
        text.layout_bounds = [34.0, 58.0, 300.0, lh * 3.0];
        text.text = Some("啊\n啊\n啊".into());
        text.focus_id = None;

        let mut viewport = Element::new().w(368.0).h(98.0);
        viewport.layout_bounds = [0.0, 0.0, 368.0, 98.0];
        viewport.style.overflow.y = Overflow::Hidden;
        viewport.focus_id = Some(focus_id_for_scroll_viewport(rest));
        viewport = viewport.child(text);

        let mut chrome = Element::new().w(400.0).h(100.0);
        chrome.layout_bounds = [0.0, 0.0, 400.0, 100.0];
        chrome.focus_id = Some(format!("{TEXTAREA_FOCUS_PREFIX}{field_id}"));
        chrome = chrome.child(viewport);

        (chrome, lh)
    }

    #[test]
    fn find_input_text_at_hits_multiline_leaf_through_scroll_viewport() {
        let (root, lh) = textarea_like_tree();
        let line2_y = 58.0 + lh * 1.5;
        let hit = find_input_text_at(&root, None, 120.0, line2_y, [0.0, 0.0])
            .expect("line 2 must hit textarea text leaf through scroll viewport");
        assert_eq!(hit.0, "ta-hit");
        assert_eq!(hit.5, format!("{TEXTAREA_FOCUS_PREFIX}ta-hit"));
        assert!(hit.2.contains('\n'), "display must include hard newlines");
    }

    #[test]
    fn find_input_text_at_hits_scrollport_padding_not_only_leaf() {
        let (root, lh) = textarea_like_tree();
        let line0_y = 58.0 + lh * 0.4;
        let left = find_input_text_at(&root, None, 10.0, line0_y, [0.0, 0.0])
            .expect("left region padding must resolve to the textarea leaf");
        assert_eq!(left.0, "ta-hit");
        assert_eq!(
            left.1[0], 34.0,
            "must return leaf bounds, not the scrollport"
        );

        let below = find_input_text_at(&root, None, 120.0, 90.0, [0.0, 0.0])
            .expect("empty clip below a short leaf must still resolve to the textarea");
        assert_eq!(below.0, "ta-hit");
    }

    #[test]
    fn find_input_text_leaf_for_rest_finds_multiline_leaf_through_scroll_viewport() {
        let (root, _) = textarea_like_tree();
        let (bounds, text, _, _) =
            find_input_text_leaf_for_rest(&root, None, "ta-hit").expect("leaf for drag-select");
        assert_eq!(text.matches('\n').count(), 2);
        assert!(bounds[3] > 50.0, "bounds must span multi-line leaf");
    }

    fn input_like_tree_with_padding_and_adornment() -> Element {
        use crate::components::INPUT_FOCUS_PREFIX;
        use crate::core::tree::CursorKind;

        let mut text = Element::new();
        text.layout_bounds = [12.0, 4.0, 152.0, 28.0];
        text.text = Some("hello world".into());

        let mut icon = Element::new();
        icon.layout_bounds = [176.0, 8.0, 20.0, 20.0];
        icon.text = Some("x".into());
        icon.text_style.font_family = Some("lucide".into());

        let mut btn = Element::new()
            .on_click(|| {})
            .cursor_kind(CursorKind::Pointer);
        btn.layout_bounds = [176.0, 8.0, 20.0, 20.0];
        btn = btn.child(icon);

        let mut chrome = Element::new().on_click(|| {}).cursor_text();
        chrome.layout_bounds = [0.0, 0.0, 200.0, 36.0];
        chrome.style.overflow.x = Overflow::Hidden;
        chrome.style.overflow.y = Overflow::Hidden;
        chrome.focus_id = Some(format!("{INPUT_FOCUS_PREFIX}pad-hit"));
        chrome.child(text).child(btn)
    }

    #[test]
    fn find_input_text_at_hits_field_padding_not_adornment() {
        let root = input_like_tree_with_padding_and_adornment();
        let left = find_input_text_at(&root, None, 6.0, 18.0, [0.0, 0.0])
            .expect("left padding must resolve to the text leaf");
        assert_eq!(left.0, "pad-hit");
        assert_eq!(left.1[0], 12.0, "must return the leaf bounds, not chrome");
        assert_eq!(left.2, "hello world");

        let right = find_input_text_at(&root, None, 168.0, 18.0, [0.0, 0.0])
            .expect("right padding between leaf and adornment must resolve to the text leaf");
        assert_eq!(right.0, "pad-hit");
        assert_eq!(right.2, "hello world");

        assert!(
            find_input_text_at(&root, None, 186.0, 18.0, [0.0, 0.0]).is_none(),
            "clear/reveal adornment must not be treated as field text"
        );
    }

    #[test]
    fn find_input_text_leaf_for_rest_skips_lucide_adornment() {
        let root = input_like_tree_with_padding_and_adornment();
        let (_, text, _, _) =
            find_input_text_leaf_for_rest(&root, None, "pad-hit").expect("field leaf");
        assert_eq!(text, "hello world");
    }
}

#[cfg(test)]
mod drag_select_bounds_tests {
    use super::*;
    use taffy::style::Overflow;

    #[test]
    fn input_leaf_for_rest_returns_visual_bounds_under_scroll() {
        // Scroll parent moved content up by 100; leaf abs y=250 → visual y=150.
        let mut leaf = Element::new().w(300.0).h(67.0);
        leaf.layout_bounds = [10.0, 250.0, 300.0, 67.0];
        leaf.text = Some("的\n啊\n啊\n我".into());
        leaf.focus_id = Some("textarea:ta-scroll-sel".into());

        let mut viewport = Element::new().w(400.0).h(200.0);
        // Must be Scroll (not Hidden): `next_hit_scroll_acc` only accumulates
        // scroll offset for Scroll/scroll-viewport ids.
        viewport.style.overflow.y = Overflow::Scroll;
        viewport.scroll_offset = [0.0, 100.0];
        viewport.layout_bounds = [0.0, 0.0, 400.0, 200.0];
        viewport = viewport.child(leaf);

        let (bounds, text, _, _) =
            find_input_text_leaf_for_rest(&viewport, None, "ta-scroll-sel").expect("leaf");
        assert_eq!(text.lines().count(), 4);
        assert!(
            (bounds[1] - 150.0).abs() < 0.5,
            "drag-select bounds must be visual (abs 250 − scroll 100), got y={}",
            bounds[1]
        );
        assert!(
            (bounds[0] - 10.0).abs() < 0.5 && (bounds[2] - 300.0).abs() < 0.5,
            "bounds={bounds:?}"
        );
    }

    #[test]
    fn clamp_pointer_to_textarea_scrollport_pins_outside_hits_to_clip_edge() {
        use crate::core::scroll_viewport::focus_id_for_scroll_viewport;
        use crate::utils::text_field::textarea_scroll_rest;

        let id = "ta-clamp-hit";
        let rest = textarea_scroll_rest(id);
        let vp_fid = focus_id_for_scroll_viewport(&rest);

        let mut leaf = Element::new();
        leaf.layout_bounds = [20.0, 40.0, 800.0, 40.0];
        leaf.text = Some("padding: 8px 16px; background: #ffffff; border-radius: 6px".into());
        leaf.text_style.white_space = crate::style::WhiteSpace::Nowrap;
        leaf.focus_id = Some(format!("textarea:{id}"));

        let mut viewport = Element::new();
        viewport.focus_id = Some(vp_fid);
        viewport.style.overflow.x = Overflow::Scroll;
        viewport.style.overflow.y = Overflow::Scroll;
        viewport.layout_bounds = [20.0, 40.0, 200.0, 120.0];
        viewport.scroll_offset = [0.0, 0.0];
        viewport = viewport.child(leaf);

        let (cx, cy) = clamp_pointer_to_textarea_scrollport(&viewport, id, 500.0, 80.0);
        assert!(
            (cx - 220.0).abs() < 0.5,
            "outside-right pointer must clamp to clip right edge, got {cx}"
        );
        assert!((cy - 80.0).abs() < 0.5);

        let (cx2, _) = clamp_pointer_to_textarea_scrollport(&viewport, id, 0.0, 80.0);
        assert!(
            (cx2 - 20.0).abs() < 0.5,
            "outside-left pointer must clamp to clip left edge, got {cx2}"
        );
    }

    #[test]
    fn nudge_hscroll_uses_inner_edge_hot_zone_and_leaf_width() {
        use crate::core::reactivity::{create_root, use_signal};
        use crate::core::scroll_viewport::{
            focus_id_for_scroll_viewport, scroll_content_w_signal_key, scroll_offset_x_signal_key,
            scroll_offset_y_signal_key, scroll_viewport_w_signal_key,
        };
        use crate::utils::text_field::textarea_scroll_rest;

        create_root(|| {
            let id = "ta-hscroll-edge";
            let rest = textarea_scroll_rest(id);
            let vp_fid = focus_id_for_scroll_viewport(&rest);

            use_signal(&scroll_offset_x_signal_key(&rest), || 0.0_f32).set_silent(0.0);
            use_signal(&scroll_offset_y_signal_key(&rest), || 0.0_f32).set_silent(0.0);
            use_signal(&scroll_viewport_w_signal_key(&rest), || 0.0_f32).set_silent(0.0);
            // Stale/zero content-w: nudge must fall back to leaf layout width.
            use_signal(&scroll_content_w_signal_key(&rest), || 0.0_f32).set_silent(0.0);

            let mut leaf = Element::new();
            leaf.layout_bounds = [0.0, 0.0, 600.0, 40.0];
            leaf.text = Some("x".repeat(80));
            leaf.text_style.white_space = crate::style::WhiteSpace::Nowrap;
            leaf.text_style.size = Some(14.0);
            leaf.text_style.line_height_px = Some(21.0);
            leaf.focus_id = Some(format!("textarea:{id}"));

            let mut viewport = Element::new();
            viewport.focus_id = Some(vp_fid);
            viewport.style.overflow.x = Overflow::Scroll;
            viewport.style.overflow.y = Overflow::Scroll;
            viewport.layout_bounds = [0.0, 0.0, 200.0, 80.0];
            viewport.scroll_offset = [0.0, 0.0];
            viewport = viewport.child(leaf);

            // Pointer inside clip but within 16px of the right edge.
            let tick = nudge_textarea_hscroll_for_drag(&mut viewport, id, 190.0, 20.0, 1.0 / 60.0);
            assert_eq!(tick, TextDragHScrollTick::Scrolled);
            let sx = use_signal(&scroll_offset_x_signal_key(&rest), || 0.0_f32).peek();
            assert!(sx > 0.5, "inner edge hot-zone must advance sx, got {sx}");
        });
    }

    #[test]
    fn input_drag_select_hit_space_uses_leaf_visual_bounds_for_nowrap() {
        use crate::core::reactivity::{create_root, use_signal};
        use crate::core::scroll_viewport::{
            focus_id_for_scroll_viewport, scroll_offset_x_signal_key,
        };
        use crate::style::TextOverflow;
        use crate::utils::text_field::textarea_scroll_rest;

        create_root(|| {
            let id = "ta-drag-hit-sx";
            let rest = textarea_scroll_rest(id);
            let vp_fid = focus_id_for_scroll_viewport(&rest);

            use_signal(&scroll_offset_x_signal_key(&rest), || 0.0_f32).set_silent(120.0);

            let mut leaf = Element::new();
            leaf.layout_bounds = [32.0, 68.0, 800.0, 40.0];
            leaf.text = Some("padding: 8px 16px; background: #ffffff; border-radius: 6px".into());
            leaf.text_style.white_space = crate::style::WhiteSpace::Nowrap;
            leaf.text_style.overflow = TextOverflow::Clip;
            leaf.focus_id = Some(format!("textarea:{id}"));

            let mut viewport = Element::new();
            viewport.focus_id = Some(vp_fid);
            viewport.style.overflow.x = Overflow::Scroll;
            viewport.style.overflow.y = Overflow::Scroll;
            viewport.layout_bounds = [20.0, 40.0, 200.0, 120.0];
            // Tree scroll still zero — signal owns sx during drag edge auto-scroll.
            viewport.scroll_offset = [0.0, 0.0];
            // Leaf visual x = layout_x − sx (padding inset from clip left).
            let leaf_visual = [-88.0, 68.0, 800.0, 40.0];
            let style = leaf.text_style.as_ref().clone();
            viewport = viewport.child(leaf);
            let (mx, _, hit_bounds, hit_style) = input_drag_select_hit_space(
                &viewport,
                id,
                500.0,
                80.0,
                leaf_visual,
                &style,
                [0.0, 0.0],
            );

            assert_eq!(hit_bounds, leaf_visual);
            assert_eq!(hit_style.scroll_x, 0.0);
            assert!(
                (mx - 220.0).abs() < 0.5,
                "pointer must clamp to clip right, got {mx}"
            );
            let content_x = mx - hit_bounds[0];
            assert!(
                (content_x - 308.0).abs() < 0.5,
                "content x must match panned leaf origin at clip edge, got {content_x}"
            );
        });
    }
}

/// Inspector pick: child-index path for the topmost hit target (logical coords).
#[cfg(feature = "devtools")]
#[allow(dead_code)]
pub(crate) fn pick_element_path(
    el: &Element,
    x: f32,
    y: f32,
    scroll_acc: [f32; 2],
) -> Option<Vec<usize>> {
    pick_element_path_acc(el, x, y, scroll_acc, Vec::new())
}

#[cfg(feature = "devtools")]
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
                if pointer_events_blocked(frame.el) {
                    continue;
                }
                if frame.el.template_id.as_deref() == Some(crate::app::devtools::HIGHLIGHT_SLOT_ID)
                    || frame
                        .el
                        .template_id
                        .as_deref()
                        .is_some_and(|id| id.starts_with("__rui_devtools_hl_"))
                {
                    continue;
                }
                let escaped = escape_scroll_acc(frame.el, frame.scroll_acc);
                let vb = visual_border_box(frame.el, escaped);
                let inside = x >= vb[0] && x <= vb[0] + vb[2] && y >= vb[1] && y <= vb[1] + vb[3];
                let is_body_portal = frame.el.template_id.as_deref()
                    == Some(crate::layout::layout_tree::PORTAL_LAYER_ID);
                if !is_body_portal && crate::core::tree::element_clips_overflow(frame.el) && !inside
                {
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
