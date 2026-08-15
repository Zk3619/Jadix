use std::cell::RefCell;

use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{Key, NamedKey};

use crate::core::tree::{
    pointer_events_targets_children, pointer_events_targets_self, CursorKind, CursorValue, Element,
};
use crate::layout::layout_tree::portal_hit_passthrough;

thread_local! {
    static HOVER_HANDLERS_SCRATCH: RefCell<Vec<std::rc::Rc<std::cell::RefCell<dyn FnMut(bool)>>>> =
        RefCell::new(Vec::new());
}

#[inline]
fn pointer_events_blocked(el: &Element) -> bool {
    !pointer_events_targets_children(el)
}

#[inline]
fn claims_own_bounds(el: &Element) -> bool {
    pointer_events_targets_self(el) && !portal_hit_passthrough(el)
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

/// 与 [`crate::components::input`] 中展示串一致：`preedit` 插在 `committed` 的 `ime_anchor` 字节位置。
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
pub(super) fn escape_scroll_acc(el: &Element, scroll_acc: [f32; 2]) -> [f32; 2] {
    if el.template_id.as_deref() == Some(crate::layout::layout_tree::PORTAL_LAYER_ID)
        || el.style.position_fixed
    {
        [0.0_f32, 0.0_f32]
    } else {
        scroll_acc
    }
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
    if pointer_events_blocked(el) {
        return LayerHit::Miss;
    }
    let scroll_acc = escape_scroll_acc(el, scroll_acc);
    let b = el.layout_bounds;
    let vx = b[0] - scroll_acc[0];
    let vy = b[1] - scroll_acc[1];
    let inside = x >= vx && x <= vx + b[2] && y >= vy && y <= vy + b[3];

    let is_body_portal =
        el.template_id.as_deref() == Some(crate::layout::layout_tree::PORTAL_LAYER_ID);
    if !is_body_portal
        && (el.style.overflow.x != taffy::style::Overflow::Visible
            || el.style.overflow.y != taffy::style::Overflow::Visible)
        && !inside
    {
        return LayerHit::Miss;
    }

    let mut next_acc = scroll_acc;
    if !is_body_portal {
        if el.style.overflow.x != taffy::style::Overflow::Visible {
            next_acc[0] += el.scroll_offset[0];
        }
        if el.style.overflow.y != taffy::style::Overflow::Visible {
            next_acc[1] += el.scroll_offset[1];
        }
    }

    if children_share_same_z_index(el) {
        for child in el.children.iter().rev() {
            match pick_cursor_hit(child, x, y, next_acc).bubble(el.cursor.clone()) {
                LayerHit::Miss => {}
                hit => return hit,
            }
        }
    } else {
        let order = hit_test_child_indices(el);
        for i in order.into_iter().rev() {
            let child = &el.children[i];
            match pick_cursor_hit(child, x, y, next_acc).bubble(el.cursor.clone()) {
                LayerHit::Miss => {}
                hit => return hit,
            }
        }
    }

    if inside {
        if !claims_own_bounds(el) {
            return LayerHit::Miss;
        }
        // Claiming bounds occludes lower siblings even without a cursor kind.
        LayerHit::Hit(el.cursor.clone())
    } else {
        LayerHit::Miss
    }
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
    let mut next_acc = scroll_acc;
    let is_scroll_viewport = el
        .focus_id
        .as_deref()
        .is_some_and(|f| f.starts_with(crate::core::scroll_viewport::SCROLL_VIEWPORT_FOCUS_PREFIX));
    if is_scroll_viewport || el.style.overflow.x != taffy::style::Overflow::Visible {
        next_acc[0] += el.scroll_offset[0];
    }
    if is_scroll_viewport || el.style.overflow.y != taffy::style::Overflow::Visible {
        next_acc[1] += el.scroll_offset[1];
    }
    for c in &el.children {
        find_bounds_by_focus_id_best(c, focus_id, next_acc, best);
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
    if pointer_events_blocked(el) {
        return LayerHit::Miss;
    }
    let scroll_acc = escape_scroll_acc(el, scroll_acc);
    let b = el.layout_bounds;
    let vx = b[0] - scroll_acc[0];
    let vy = b[1] - scroll_acc[1];
    let inside = x >= vx && x <= vx + b[2] && y >= vy && y <= vy + b[3];

    let is_body_portal =
        el.template_id.as_deref() == Some(crate::layout::layout_tree::PORTAL_LAYER_ID);
    if !is_body_portal
        && (el.style.overflow.x != taffy::style::Overflow::Visible
            || el.style.overflow.y != taffy::style::Overflow::Visible)
        && !inside
    {
        return LayerHit::Miss;
    }

    let mut next_acc = scroll_acc;
    if !is_body_portal {
        if el.style.overflow.x != taffy::style::Overflow::Visible {
            next_acc[0] += el.scroll_offset[0];
        }
        if el.style.overflow.y != taffy::style::Overflow::Visible {
            next_acc[1] += el.scroll_offset[1];
        }
    }

    if children_share_same_z_index(el) {
        for child in el.children.iter().rev() {
            match find_focus_id_at_hit(child, x, y, next_acc).bubble(el.focus_id.clone()) {
                LayerHit::Miss => {}
                hit => return hit,
            }
        }
    } else {
        let order = hit_test_child_indices(el);
        for i in order.into_iter().rev() {
            let child = &el.children[i];
            match find_focus_id_at_hit(child, x, y, next_acc).bubble(el.focus_id.clone()) {
                LayerHit::Miss => {}
                hit => return hit,
            }
        }
    }

    if inside {
        if !claims_own_bounds(el) {
            return LayerHit::Miss;
        }
        // Claiming bounds occludes lower siblings (modal underlay vs gallery scrollbars).
        LayerHit::Hit(el.focus_id.clone())
    } else {
        LayerHit::Miss
    }
}

/// 命中最上层含 `input:` 继承焦点的文本叶（用于拖选时按 `rest` 查找布局与样式）。
///
/// 返回的 bounds 为**屏幕/可视坐标**（已扣祖先 `scroll_acc`），与
/// [`find_input_text_at`] 及指针 `(x,y)` 同一空间。切勿返回未滚动的
/// `layout_bounds`，否则 ScrollArea 内拖选会把 `my` clamp 到错误行，表现为
/// 「点选一字却跨行选中」。
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
    let mut next_acc = scroll_acc;
    if el.style.overflow.x != taffy::style::Overflow::Visible {
        next_acc[0] += el.scroll_offset[0];
    }
    if el.style.overflow.y != taffy::style::Overflow::Visible {
        next_acc[1] += el.scroll_offset[1];
    }
    let child_text_edit_fid = inherit_text_edit_focus_id(el, text_edit_fid);
    if children_share_same_z_index(el) {
        for child in el.children.iter().rev() {
            if let Some(r) =
                find_input_text_leaf_for_rest_acc(child, child_text_edit_fid, rest, next_acc)
            {
                return Some(r);
            }
        }
    } else {
        let order = hit_test_child_indices(el);
        for i in order.into_iter().rev() {
            let child = &el.children[i];
            if let Some(r) =
                find_input_text_leaf_for_rest_acc(child, child_text_edit_fid, rest, next_acc)
            {
                return Some(r);
            }
        }
    }
    if let Some(text) = &el.text {
        // Clear / reveal adornments inherit `input:` — they are not the field leaf.
        if el.text_style.font_family.as_deref() == Some("lucide") {
            return None;
        }
        if let Some(id) = child_text_edit_fid {
            if id == format!("{}{}", crate::components::INPUT_FOCUS_PREFIX, rest)
                || id == format!("{}{}", crate::components::TEXTAREA_FOCUS_PREFIX, rest)
            {
                let b = el.layout_bounds;
                let vx = b[0] - scroll_acc[0];
                let vy = b[1] - scroll_acc[1];
                return Some((
                    [vx, vy, b[2], b[3]],
                    text.clone(),
                    el.text_style.clone(),
                    el.scroll_offset,
                ));
            }
        }
    }
    None
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
    if pointer_events_blocked(el) {
        return None;
    }
    let b = el.layout_bounds;
    let vx = b[0] - scroll_acc[0];
    let vy = b[1] - scroll_acc[1];

    if (el.style.overflow.x != taffy::style::Overflow::Visible
        || el.style.overflow.y != taffy::style::Overflow::Visible)
        && (x < vx || x > vx + b[2] || y < vy || y > vy + b[3])
    {
        return None;
    }

    let mut next_acc = scroll_acc;
    if el.style.overflow.x != taffy::style::Overflow::Visible {
        next_acc[0] += el.scroll_offset[0];
    }
    if el.style.overflow.y != taffy::style::Overflow::Visible {
        next_acc[1] += el.scroll_offset[1];
    }

    let child_text_edit_fid = inherit_text_edit_focus_id(el, text_edit_fid);
    if children_share_same_z_index(el) {
        for child in el.children.iter().rev() {
            if let Some(r) = find_input_text_at(child, child_text_edit_fid, x, y, next_acc) {
                return Some(r);
            }
        }
    } else {
        let order = hit_test_child_indices(el);
        for i in order.into_iter().rev() {
            let child = &el.children[i];
            if let Some(r) = find_input_text_at(child, child_text_edit_fid, x, y, next_acc) {
                return Some(r);
            }
        }
    }
    if let Some(text) = &el.text {
        // Clear / reveal adornments use Lucide icon glyphs — not editable field text.
        if el.text_style.font_family.as_deref() == Some("lucide") {
            return None;
        }
        if let Some(id) = child_text_edit_fid {
            let rest = id
                .strip_prefix(crate::components::INPUT_FOCUS_PREFIX)
                .or_else(|| id.strip_prefix(crate::components::TEXTAREA_FOCUS_PREFIX));
            if let Some(rest) = rest {
                if x >= vx && x <= vx + b[2] && y >= vy && y <= vy + b[3] {
                    // Fourth tuple slot carries the full focus_id so callers keep `textarea:` vs `input:`.
                    return Some((
                        rest.to_string(),
                        [vx, vy, b[2], b[3]],
                        text.clone(),
                        el.text_style.clone(),
                        el.scroll_offset,
                        id.to_string(),
                    ));
                }
            }
        }
    }
    // Padding / empty chrome of the field: children missed (pointer is not on the
    // text leaf) but the click is still inside this `input:` / `textarea:` box.
    // Do not steal clear/reveal adornment hits.
    if let Some(id) = el.focus_id.as_deref() {
        let rest = id
            .strip_prefix(crate::components::INPUT_FOCUS_PREFIX)
            .or_else(|| id.strip_prefix(crate::components::TEXTAREA_FOCUS_PREFIX));
        if let Some(rest) = rest {
            if x >= vx
                && x <= vx + b[2]
                && y >= vy
                && y <= vy + b[3]
                && !pointer_on_text_edit_adornment(el, x, y, scroll_acc)
            {
                if let Some((bounds, text, style, scroll)) =
                    find_input_text_leaf_for_rest_acc(el, Some(id), rest, scroll_acc)
                {
                    return Some((
                        rest.to_string(),
                        bounds,
                        text,
                        style,
                        scroll,
                        id.to_string(),
                    ));
                }
            }
        }
    }
    None
}

fn is_text_edit_focus_id(id: &str) -> bool {
    id.starts_with(crate::components::INPUT_FOCUS_PREFIX)
        || id.starts_with(crate::components::TEXTAREA_FOCUS_PREFIX)
}

/// True when `(x,y)` is on a clear/reveal control inside a text-edit chrome.
fn pointer_on_text_edit_adornment(el: &Element, x: f32, y: f32, scroll_acc: [f32; 2]) -> bool {
    if pointer_events_blocked(el) {
        return false;
    }
    let b = el.layout_bounds;
    let vx = b[0] - scroll_acc[0];
    let vy = b[1] - scroll_acc[1];
    let mut next_acc = scroll_acc;
    if el.style.overflow.x != taffy::style::Overflow::Visible {
        next_acc[0] += el.scroll_offset[0];
    }
    if el.style.overflow.y != taffy::style::Overflow::Visible {
        next_acc[1] += el.scroll_offset[1];
    }
    for child in &el.children {
        if pointer_on_text_edit_adornment(child, x, y, next_acc) {
            return true;
        }
    }
    // The field chrome itself has `on_click` (focus) and `cursor_text` — not an adornment.
    if el.focus_id.as_deref().is_some_and(is_text_edit_focus_id) {
        return false;
    }
    let is_adornment = el.on_click.is_some()
        || el.text_style.font_family.as_deref() == Some("lucide")
        || el.cursor.as_ref().is_some_and(|c| c.kind == CursorKind::Pointer);
    is_adornment && claims_own_bounds(el) && x >= vx && x <= vx + b[2] && y >= vy && y <= vy + b[3]
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
    let mut next_acc = scroll_acc;
    if el.style.overflow.x != taffy::style::Overflow::Visible {
        next_acc[0] += el.scroll_offset[0];
    }
    if el.style.overflow.y != taffy::style::Overflow::Visible {
        next_acc[1] += el.scroll_offset[1];
    }
    if children_share_same_z_index(el) {
        for child in el.children.iter().rev() {
            if let Some(r) = find_selectable_text_leaf_for_key_acc(child, key, next_acc) {
                return Some(r);
            }
        }
    } else {
        let order = hit_test_child_indices(el);
        for i in order.into_iter().rev() {
            let child = &el.children[i];
            if let Some(r) = find_selectable_text_leaf_for_key_acc(child, key, next_acc) {
                return Some(r);
            }
        }
    }
    if is_selectable_static_text(el)
        && crate::components::label::selectable_text_key(el).as_deref() == Some(key)
    {
        if let Some(text) = &el.text {
            let b = el.layout_bounds;
            let vx = b[0] - scroll_acc[0];
            let vy = b[1] - scroll_acc[1];
            return Some((
                [vx, vy, b[2], b[3]],
                text.clone(),
                el.text_style.clone(),
                el.scroll_offset,
            ));
        }
    }
    None
}

/// 指针 `(x,y)` 是否落在可选静态文本叶上。
pub(super) fn find_selectable_text_at(
    el: &Element,
    x: f32,
    y: f32,
    scroll_acc: [f32; 2],
) -> Option<(String, [f32; 4], String, crate::style::TextStyle, [f32; 2])> {
    if pointer_events_blocked(el) {
        return None;
    }
    let b = el.layout_bounds;
    let vx = b[0] - scroll_acc[0];
    let vy = b[1] - scroll_acc[1];

    if (el.style.overflow.x != taffy::style::Overflow::Visible
        || el.style.overflow.y != taffy::style::Overflow::Visible)
        && (x < vx || x > vx + b[2] || y < vy || y > vy + b[3])
    {
        return None;
    }

    let mut next_acc = scroll_acc;
    if el.style.overflow.x != taffy::style::Overflow::Visible {
        next_acc[0] += el.scroll_offset[0];
    }
    if el.style.overflow.y != taffy::style::Overflow::Visible {
        next_acc[1] += el.scroll_offset[1];
    }

    if children_share_same_z_index(el) {
        for child in el.children.iter().rev() {
            if let Some(r) = find_selectable_text_at(child, x, y, next_acc) {
                return Some(r);
            }
        }
    } else {
        let order = hit_test_child_indices(el);
        for i in order.into_iter().rev() {
            let child = &el.children[i];
            if let Some(r) = find_selectable_text_at(child, x, y, next_acc) {
                return Some(r);
            }
        }
    }
    if is_selectable_static_text(el) && x >= vx && x <= vx + b[2] && y >= vy && y <= vy + b[3] {
        if let Some(key) = crate::components::label::selectable_text_key(el) {
            if let Some(text) = &el.text {
                return Some((
                    key,
                    [vx, vy, b[2], b[3]],
                    text.clone(),
                    el.text_style.clone(),
                    el.scroll_offset,
                ));
            }
        }
    }
    None
}

pub(super) fn update_hovers(
    el: &Element,
    x: f32,
    y: f32,
    scroll_offset: [f32; 2],
    cbs: &mut Vec<(std::rc::Rc<std::cell::RefCell<dyn FnMut(bool)>>, bool)>,
) {
    HOVER_HANDLERS_SCRATCH.with(|scratch| {
        let mut all = scratch.borrow_mut();
        all.clear();
        collect_hover_handlers(el, &mut all);
        let target = match pick_hover_target(el, x, y, scroll_offset) {
            HoverHit::Hit(cb) => cb,
            HoverHit::Miss => None,
        };
        for cb in all.iter() {
            let is_target = target.as_ref().is_some_and(|t| std::rc::Rc::ptr_eq(t, cb));
            cbs.push((cb.clone(), is_target));
        }
    });
}

fn collect_hover_handlers(
    el: &Element,
    out: &mut Vec<std::rc::Rc<std::cell::RefCell<dyn FnMut(bool)>>>,
) {
    if let Some(cb) = &el.on_hover {
        out.push(cb.clone());
    }
    for child in &el.children {
        collect_hover_handlers(child, out);
    }
}

/// Result of hover hit-testing (distinct from “no handler”).
///
/// - [`HoverHit::Miss`]: pointer missed this subtree — try siblings.
/// - [`HoverHit::Hit`]: pointer hit this subtree. `None` means absorb (block siblings
///   below, e.g. modal underlay) but ancestors may still supply `on_hover`.
enum HoverHit {
    Miss,
    Hit(Option<std::rc::Rc<std::cell::RefCell<dyn FnMut(bool)>>>),
}

fn pick_hover_target(el: &Element, x: f32, y: f32, scroll_acc: [f32; 2]) -> HoverHit {
    if pointer_events_blocked(el) {
        return HoverHit::Miss;
    }

    // Match `pointer_events::hit_path`: body portal / fixed escape ancestor scroll.
    let scroll_acc = if el.template_id.as_deref()
        == Some(crate::layout::layout_tree::PORTAL_LAYER_ID)
        || el.style.position_fixed
    {
        [0.0_f32, 0.0_f32]
    } else {
        scroll_acc
    };

    let b = el.layout_bounds;
    let vx = b[0] - scroll_acc[0];
    let vy = b[1] - scroll_acc[1];
    let inside = x >= vx && x <= vx + b[2] && y >= vy && y <= vy + b[3];

    let is_body_portal =
        el.template_id.as_deref() == Some(crate::layout::layout_tree::PORTAL_LAYER_ID);

    // Respect overflow clipping as a hard boundary (portal layer is a stacking shell).
    if !is_body_portal
        && (el.style.overflow.x != taffy::style::Overflow::Visible
            || el.style.overflow.y != taffy::style::Overflow::Visible)
        && !inside
    {
        return HoverHit::Miss;
    }

    let mut next_acc = scroll_acc;
    if !is_body_portal {
        if el.style.overflow.x != taffy::style::Overflow::Visible {
            next_acc[0] += el.scroll_offset[0];
        }
        if el.style.overflow.y != taffy::style::Overflow::Visible {
            next_acc[1] += el.scroll_offset[1];
        }
    }

    // Topmost child wins; bubble `on_hover` so Button > label still highlights.
    let bubble = |child_hit: HoverHit| -> HoverHit {
        match child_hit {
            HoverHit::Miss => HoverHit::Miss,
            HoverHit::Hit(cb) => HoverHit::Hit(cb.or_else(|| el.on_hover.clone())),
        }
    };

    if children_share_same_z_index(el) {
        for child in el.children.iter().rev() {
            match bubble(pick_hover_target(child, x, y, next_acc)) {
                HoverHit::Miss => {}
                hit => return hit,
            }
        }
    } else {
        let order = hit_test_child_indices(el);
        for i in order.into_iter().rev() {
            let child = &el.children[i];
            match bubble(pick_hover_target(child, x, y, next_acc)) {
                HoverHit::Miss => {}
                hit => return hit,
            }
        }
    }

    if inside {
        if !claims_own_bounds(el) {
            return HoverHit::Miss;
        }
        // Claiming bounds absorbs sibling search even without `on_hover` (modal underlay).
        HoverHit::Hit(el.on_hover.clone())
    } else {
        HoverHit::Miss
    }
}

pub(crate) fn find_click(
    el: &Element,
    x: f32,
    y: f32,
    scroll_acc: [f32; 2],
) -> Option<std::rc::Rc<std::cell::RefCell<dyn FnMut()>>> {
    if pointer_events_blocked(el) {
        return None;
    }
    let b = el.layout_bounds;
    let vx = b[0] - scroll_acc[0];
    let vy = b[1] - scroll_acc[1];

    if (el.style.overflow.x != taffy::style::Overflow::Visible
        || el.style.overflow.y != taffy::style::Overflow::Visible)
        && (x < vx || x > vx + b[2] || y < vy || y > vy + b[3])
    {
        return None;
    }

    let mut next_acc = scroll_acc;
    if el.style.overflow.x != taffy::style::Overflow::Visible {
        next_acc[0] += el.scroll_offset[0];
    }
    if el.style.overflow.y != taffy::style::Overflow::Visible {
        next_acc[1] += el.scroll_offset[1];
    }

    if children_share_same_z_index(el) {
        for child in el.children.iter().rev() {
            if let Some(cb) = find_click(child, x, y, next_acc) {
                return Some(cb);
            }
        }
    } else {
        let order = hit_test_child_indices(el);
        for i in order.into_iter().rev() {
            let child = &el.children[i];
            if let Some(cb) = find_click(child, x, y, next_acc) {
                return Some(cb);
            }
        }
    }
    if x >= vx && x <= vx + b[2] && y >= vy && y <= vy + b[3] {
        if !claims_own_bounds(el) {
            return None;
        }
        if let Some(cb) = &el.on_click {
            return Some(cb.clone());
        }
    }
    None
}

pub(super) fn find_element_by_focus_id<'a>(el: &'a Element, focus_id: &str) -> Option<&'a Element> {
    if el.focus_id.as_deref() == Some(focus_id) {
        return Some(el);
    }
    for c in &el.children {
        if let Some(n) = find_element_by_focus_id(c, focus_id) {
            return Some(n);
        }
    }
    None
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
    if let Some(cb) = &handlers.on_key_down {
        if cb.borrow_mut()(event.clone()) {
            return true;
        }
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

#[cfg(test)]
mod hover_occlusion_tests {
    use super::*;
    use crate::primitives::interactive::interactive;
    use std::cell::Cell;
    use std::rc::Rc;

    #[test]
    fn claiming_underlay_blocks_hover_on_content_below() {
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
    fn button_hover_bubbles_from_child_label() {
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
        viewport.style.overflow.y = Overflow::Hidden;
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
}
