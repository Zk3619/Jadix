use std::cell::RefCell;

use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{Key, NamedKey};
use winit::window::CursorIcon;

use crate::core::tree::{
    pointer_events_targets_children, pointer_events_targets_self, CursorKind, Element,
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

fn hit_test_child_indices(el: &Element) -> Vec<usize> {
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
fn children_share_same_z_index(el: &Element) -> bool {
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

pub(super) fn pick_cursor(
    el: &Element,
    x: f32,
    y: f32,
    scroll_acc: [f32; 2],
) -> Option<CursorKind> {
    if pointer_events_blocked(el) {
        return None;
    }
    let b = el.layout_bounds;
    // `layout_bounds` is stable unscrolled absolute space.
    // To match screen coordinates (x, y), we must subtract parent scroll accumulation.
    let vx = b[0] - scroll_acc[0];
    let vy = b[1] - scroll_acc[1];

    // Overflow Clipping (Check if point is within container visual bounds)
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
            if let Some(c) = pick_cursor(child, x, y, next_acc) {
                return Some(c);
            }
        }
    } else {
        let order = hit_test_child_indices(el);
        for i in order.into_iter().rev() {
            let child = &el.children[i];
            if let Some(c) = pick_cursor(child, x, y, next_acc) {
                return Some(c);
            }
        }
    }

    if x >= vx && x <= vx + b[2] && y >= vy && y <= vy + b[3] {
        if !claims_own_bounds(el) {
            return None;
        }
        el.cursor
    } else {
        None
    }
}

pub(super) fn cursor_kind_to_winit(c: CursorKind) -> CursorIcon {
    match c {
        CursorKind::Default => CursorIcon::Default,
        CursorKind::Pointer => CursorIcon::Pointer,
        CursorKind::Text => CursorIcon::Text,
        CursorKind::NotAllowed => CursorIcon::NotAllowed,
    }
}

pub(super) fn find_bounds_by_focus_id(
    el: &Element,
    focus_id: &str,
    scroll_acc: [f32; 2],
) -> Option<([f32; 4], [f32; 2])> {
    if el.focus_id.as_deref() == Some(focus_id) {
        return Some((el.layout_bounds, scroll_acc));
    }
    let mut next_acc = scroll_acc;
    if el.style.overflow.x != taffy::style::Overflow::Visible {
        next_acc[0] += el.scroll_offset[0];
    }
    if el.style.overflow.y != taffy::style::Overflow::Visible {
        next_acc[1] += el.scroll_offset[1];
    }
    for c in &el.children {
        if let Some(res) = find_bounds_by_focus_id(c, focus_id, next_acc) {
            return Some(res);
        }
    }
    None
}

pub(super) fn find_focus_id_at(
    el: &Element,
    x: f32,
    y: f32,
    scroll_acc: [f32; 2],
) -> Option<String> {
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
            if let Some(id) = find_focus_id_at(child, x, y, next_acc) {
                return Some(id);
            }
        }
    } else {
        let order = hit_test_child_indices(el);
        for i in order.into_iter().rev() {
            let child = &el.children[i];
            if let Some(id) = find_focus_id_at(child, x, y, next_acc) {
                return Some(id);
            }
        }
    }

    if x >= vx && x <= vx + b[2] && y >= vy && y <= vy + b[3] {
        if !claims_own_bounds(el) {
            return None;
        }
        return el.focus_id.clone();
    }
    None
}

/// 命中最上层含 `input:` 继承焦点的文本叶（用于拖选时按 `rest` 查找布局与样式）。
pub(super) fn find_input_text_leaf_for_rest(
    el: &Element,
    inherited_fid: Option<&str>,
    rest: &str,
) -> Option<([f32; 4], String, crate::style::TextStyle, [f32; 2])> {
    let fid = el.focus_id.as_deref().or(inherited_fid);
    if children_share_same_z_index(el) {
        for child in el.children.iter().rev() {
            if let Some(r) = find_input_text_leaf_for_rest(child, fid, rest) {
                return Some(r);
            }
        }
    } else {
        let order = hit_test_child_indices(el);
        for i in order.into_iter().rev() {
            let child = &el.children[i];
            if let Some(r) = find_input_text_leaf_for_rest(child, fid, rest) {
                return Some(r);
            }
        }
    }
    if let Some(text) = &el.text {
        if let Some(id) = fid {
            if id == format!("{}{}", crate::components::INPUT_FOCUS_PREFIX, rest)
                || id == format!("{}{}", crate::components::TEXTAREA_FOCUS_PREFIX, rest)
            {
                return Some((
                    el.layout_bounds,
                    text.clone(),
                    el.text_style.clone(),
                    el.scroll_offset,
                ));
            }
        }
    }
    None
}

/// 指针 `(x,y)` 是否落在某 `input:` 的文本叶上；返回 `rest`、盒、`display` 与样式（用于按下时命中）。
pub(super) fn find_input_text_at(
    el: &Element,
    inherited_fid: Option<&str>,
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

    let fid = el.focus_id.as_deref().or(inherited_fid);
    if children_share_same_z_index(el) {
        for child in el.children.iter().rev() {
            if let Some(r) = find_input_text_at(child, fid, x, y, next_acc) {
                return Some(r);
            }
        }
    } else {
        let order = hit_test_child_indices(el);
        for i in order.into_iter().rev() {
            let child = &el.children[i];
            if let Some(r) = find_input_text_at(child, fid, x, y, next_acc) {
                return Some(r);
            }
        }
    }
    if let Some(text) = &el.text {
        if let Some(id) = fid {
            if let Some(rest) = id.strip_prefix(crate::components::INPUT_FOCUS_PREFIX) {
                if x >= vx && x <= vx + b[2] && y >= vy && y <= vy + b[3] {
                    // Note: Here we still return el.scroll_offset for specific input metrics.
                    return Some((
                        rest.to_string(),
                        [vx, vy, b[2], b[3]],
                        text.clone(),
                        el.text_style.clone(),
                        el.scroll_offset,
                    ));
                }
            }
        }
    }
    None
}

fn is_selectable_static_text(el: &Element) -> bool {
    crate::components::label::selectable_text_key(el).is_some()
}

/// 拖选进行中按稳定键查找可选静态文本叶（布局、展示串与样式）。
pub(super) fn find_selectable_text_leaf_for_key(
    el: &Element,
    key: &str,
) -> Option<([f32; 4], String, crate::style::TextStyle, [f32; 2])> {
    if children_share_same_z_index(el) {
        for child in el.children.iter().rev() {
            if let Some(r) = find_selectable_text_leaf_for_key(child, key) {
                return Some(r);
            }
        }
    } else {
        let order = hit_test_child_indices(el);
        for i in order.into_iter().rev() {
            let child = &el.children[i];
            if let Some(r) = find_selectable_text_leaf_for_key(child, key) {
                return Some(r);
            }
        }
    }
    if is_selectable_static_text(el)
        && crate::components::label::selectable_text_key(el).as_deref() == Some(key)
    {
        if let Some(text) = &el.text {
            return Some((
                el.layout_bounds,
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
        let target = pick_hover_target(el, x, y, scroll_offset);
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

fn pick_hover_target(
    el: &Element,
    x: f32,
    y: f32,
    scroll_acc: [f32; 2],
) -> Option<std::rc::Rc<std::cell::RefCell<dyn FnMut(bool)>>> {
    if pointer_events_blocked(el) {
        return None;
    }
    let b = el.layout_bounds;
    let vx = b[0] - scroll_acc[0];
    let vy = b[1] - scroll_acc[1];
    let inside = x >= vx && x <= vx + b[2] && y >= vy && y <= vy + b[3];

    // Respect overflow clipping as a hard boundary.
    if (el.style.overflow.x != taffy::style::Overflow::Visible
        || el.style.overflow.y != taffy::style::Overflow::Visible)
        && !inside
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

    // Topmost child wins.
    if children_share_same_z_index(el) {
        for child in el.children.iter().rev() {
            if let Some(cb) = pick_hover_target(child, x, y, next_acc) {
                return Some(cb);
            }
        }
    } else {
        let order = hit_test_child_indices(el);
        for i in order.into_iter().rev() {
            let child = &el.children[i];
            if let Some(cb) = pick_hover_target(child, x, y, next_acc) {
                return Some(cb);
            }
        }
    }

    if inside {
        if !claims_own_bounds(el) {
            return None;
        }
        el.on_hover.clone()
    } else {
        None
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

/// Routes a key to the focused [`Element`] (by `focus_id`): `on_key_down` first, then Space/Enter
/// activates `on_click` when `focusable` (no repeat).
pub(super) fn dispatch_key_for_focused(el: &Element, focus_id: &str, event: &KeyEvent) -> bool {
    let Some(node) = find_element_by_focus_id(el, focus_id) else {
        return false;
    };
    if let Some(cb) = &node.on_key_down {
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
    if activate && node.focusable {
        if let Some(cb) = &node.on_click {
            cb.borrow_mut()();
            return true;
        }
    }
    false
}
