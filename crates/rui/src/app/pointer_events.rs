//! Pointer event model: hit path, capture/bubble phases, and pointer capture.
//!
//! Aligns with the DOM [`MouseEvent`](https://developer.mozilla.org/zh-CN/docs/Web/API/MouseEvent)
//! surface used by `click` / `dblclick` / `mousedown` / `mouseup` / `mousemove` /
//! `mouseover` / `mouseout`. Existing leaf-only [`super::tree_hit::find_click`] remains
//! for Label-`for` resolution. Handlers participate in capture → target → bubble
//! via [`dispatch_pointer`].

use std::cell::Cell;
use std::cell::RefCell;
use std::rc::Rc;

use winit::event::MouseButton;

use crate::core::tree::{pointer_events_targets_children, pointer_events_targets_self, Element};
use crate::layout::layout_tree::portal_hit_passthrough;

use super::tree_hit;

/// DOM-like event phase ([`Event.eventPhase`](https://developer.mozilla.org/zh-CN/docs/Web/API/Event/eventPhase)).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventPhase {
    Capture,
    Target,
    Bubble,
}

/// DOM `MouseEvent.button` (0 left / 1 middle / 2 right / 3 back / 4 forward).
pub fn dom_button(button: MouseButton) -> i16 {
    match button {
        MouseButton::Left => 0,
        MouseButton::Middle => 1,
        MouseButton::Right => 2,
        MouseButton::Back => 3,
        MouseButton::Forward => 4,
        MouseButton::Other(n) => i16::try_from(n).unwrap_or(0),
    }
}

/// DOM `MouseEvent.buttons` bit for a winit button (1 left / 2 right / 4 middle / 8 back / 16 forward).
pub fn buttons_mask_bit(button: MouseButton) -> u16 {
    match button {
        MouseButton::Left => 0x01,
        MouseButton::Right => 0x02,
        MouseButton::Middle => 0x04,
        MouseButton::Back => 0x08,
        MouseButton::Forward => 0x10,
        MouseButton::Other(_) => 0,
    }
}

/// DOM [`MouseEvent`](https://developer.mozilla.org/zh-CN/docs/Web/API/MouseEvent).
///
/// `x` / `y` are viewport (client) coordinates in logical pixels — the same space as layout.
/// [`PointerEvent`] is a compatibility alias.
#[derive(Debug)]
pub struct MouseEvent {
    /// [`MouseEvent.clientX`](https://developer.mozilla.org/zh-CN/docs/Web/API/MouseEvent/clientX) (logical px).
    pub x: f32,
    /// [`MouseEvent.clientY`](https://developer.mozilla.org/zh-CN/docs/Web/API/MouseEvent/clientY) (logical px).
    pub y: f32,
    /// winit button that changed (`mousedown` / `mouseup` / `click`). `mousemove` uses the last changed button.
    pub button: MouseButton,
    pub phase: EventPhase,
    /// [`UIEvent.detail`](https://developer.mozilla.org/zh-CN/docs/Web/API/UIEvent/detail) — click count.
    pub detail: u8,
    /// 当前正在分发的节点**可视**盒（窗口逻辑像素 `[x,y,w,h]`，已扣祖先 scroll）。
    pub current_bounds: [f32; 4],
    /// [`Event.type`](https://developer.mozilla.org/zh-CN/docs/Web/API/Event/type): `click`, `mousedown`, …
    pub event_type: &'static str,
    /// [`MouseEvent.buttons`](https://developer.mozilla.org/zh-CN/docs/Web/API/MouseEvent/buttons) bitmask.
    pub buttons: u16,
    pub alt_key: bool,
    pub ctrl_key: bool,
    pub meta_key: bool,
    pub shift_key: bool,
    /// [`MouseEvent.movementX`](https://developer.mozilla.org/zh-CN/docs/Web/API/MouseEvent/movementX)
    pub movement_x: f32,
    /// [`MouseEvent.movementY`](https://developer.mozilla.org/zh-CN/docs/Web/API/MouseEvent/movementY)
    pub movement_y: f32,
    /// [`MouseEvent.screenX`](https://developer.mozilla.org/zh-CN/docs/Web/API/MouseEvent/screenX) (physical px).
    pub screen_x: f32,
    /// [`MouseEvent.screenY`](https://developer.mozilla.org/zh-CN/docs/Web/API/MouseEvent/screenY) (physical px).
    pub screen_y: f32,
    /// [`MouseEvent.pageX`](https://developer.mozilla.org/zh-CN/docs/Web/API/MouseEvent/pageX) — same as client in a single window.
    pub page_x: f32,
    /// [`MouseEvent.pageY`](https://developer.mozilla.org/zh-CN/docs/Web/API/MouseEvent/pageY)
    pub page_y: f32,
    /// [`MouseEvent.region`](https://developer.mozilla.org/zh-CN/docs/Web/API/MouseEvent/region) (`id` / `region` attr).
    pub region: Option<String>,
    /// [`MouseEvent.relatedTarget`](https://developer.mozilla.org/zh-CN/docs/Web/API/MouseEvent/relatedTarget) (`focus_id`).
    pub related_target: Option<String>,
    consumed: Cell<bool>,
    default_prevented: Cell<bool>,
}

/// Compatibility alias — same type as [`MouseEvent`].
pub type PointerEvent = MouseEvent;

impl MouseEvent {
    pub fn new(x: f32, y: f32, button: MouseButton, detail: u8) -> Self {
        Self {
            x,
            y,
            button,
            phase: EventPhase::Target,
            detail,
            current_bounds: [0.0; 4],
            event_type: "click",
            buttons: 0,
            alt_key: false,
            ctrl_key: false,
            meta_key: false,
            shift_key: false,
            movement_x: 0.0,
            movement_y: 0.0,
            screen_x: x,
            screen_y: y,
            page_x: x,
            page_y: y,
            region: None,
            related_target: None,
            consumed: Cell::new(false),
            default_prevented: Cell::new(false),
        }
    }

    /// [`MouseEvent.clientX`](https://developer.mozilla.org/zh-CN/docs/Web/API/MouseEvent/clientX)
    #[inline]
    pub fn client_x(&self) -> f32 {
        self.x
    }
    /// [`MouseEvent.clientY`](https://developer.mozilla.org/zh-CN/docs/Web/API/MouseEvent/clientY)
    #[inline]
    pub fn client_y(&self) -> f32 {
        self.y
    }
    /// [`MouseEvent.offsetX`](https://developer.mozilla.org/zh-CN/docs/Web/API/MouseEvent/offsetX) — relative to the current target padding edge.
    #[inline]
    pub fn offset_x(&self) -> f32 {
        self.x - self.current_bounds[0]
    }
    /// [`MouseEvent.offsetY`](https://developer.mozilla.org/zh-CN/docs/Web/API/MouseEvent/offsetY)
    #[inline]
    pub fn offset_y(&self) -> f32 {
        self.y - self.current_bounds[1]
    }
    /// [`MouseEvent.button`](https://developer.mozilla.org/zh-CN/docs/Web/API/MouseEvent/button) DOM index.
    #[inline]
    pub fn button_index(&self) -> i16 {
        dom_button(self.button)
    }
    /// [`MouseEvent.which`](https://developer.mozilla.org/zh-CN/docs/Web/API/MouseEvent/which) (1-based; deprecated on the Web).
    #[inline]
    pub fn which(&self) -> i16 {
        self.button_index() + 1
    }
    /// [`MouseEvent.getModifierState()`](https://developer.mozilla.org/zh-CN/docs/Web/API/MouseEvent/getModifierState)
    pub fn get_modifier_state(&self, key: &str) -> bool {
        match key {
            "Alt" | "AltGraph" => self.alt_key,
            "Control" => self.ctrl_key,
            "Meta" | "OS" => self.meta_key,
            "Shift" => self.shift_key,
            "Accel" => self.ctrl_key || self.meta_key,
            _ => false,
        }
    }

    pub fn stop_propagation(&self) {
        self.consumed.set(true);
    }

    pub fn prevent_default(&self) {
        self.default_prevented.set(true);
    }

    pub fn is_consumed(&self) -> bool {
        self.consumed.get()
    }

    pub fn default_prevented(&self) -> bool {
        self.default_prevented.get()
    }
}

pub type PointerHandler = Rc<RefCell<dyn FnMut(&PointerEvent)>>;

/// One node on the hit path (root → … → leaf).
#[derive(Clone, Default)]
pub struct HitPathEntry {
    pub focus_id: Option<String>,
    /// Label `attrs["for"]`（Solid `for`）→ 转发点击到匹配 `focus_id` 的控件。
    pub label_for: Option<String>,
    pub on_mousedown: Option<PointerHandler>,
    pub on_mouseup: Option<PointerHandler>,
    pub on_click: Option<Rc<RefCell<dyn FnMut()>>>,
    pub on_context_menu: Option<PointerHandler>,
    pub on_double_click: Option<PointerHandler>,
    pub on_mousemove: Option<PointerHandler>,
    pub on_mouseover: Option<PointerHandler>,
    pub on_mouseout: Option<PointerHandler>,
    /// Element was marked `rui_hoisted_portal` (Solid body overlay surface).
    pub hoisted_portal: bool,
    /// `rui_portal_key` when present (stable portal identity after hoist).
    pub portal_key: Option<String>,
    /// `rui_floating` id when present (Select / Popover / Menu panel).
    pub floating_id: Option<String>,
    /// Canvas-like [`MouseEvent.region`](https://developer.mozilla.org/zh-CN/docs/Web/API/MouseEvent/region).
    pub region: Option<String>,
    /// WP-3.12 paint id hex（`rui_canvas_paint`）；按下/拖线时标 dirty。
    pub canvas_paint: Option<String>,
    /// 该节点可视盒（窗口像素，已扣祖先 `scroll_acc`）。与命中判定同一套坐标，
    /// 供 handler 把指针映射到局部逻辑坐标（Chart / ScrollArea 内控件）。
    pub bounds: [f32; 4],
}

impl HitPathEntry {
    pub fn from_element(el: &Element, bounds: [f32; 4]) -> Self {
        let region = el
            .attrs
            .get("region")
            .or_else(|| el.attrs.get("id"))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        Self {
            focus_id: el.focus_id.clone(),
            label_for: el
                .attrs
                .get(crate::components::label::ATTR_LABEL_FOR)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            on_mousedown: el.on_mousedown.clone(),
            on_mouseup: el.on_mouseup.clone(),
            on_click: el.on_click.clone(),
            on_context_menu: el.on_context_menu.clone(),
            on_double_click: el.on_double_click.clone(),
            on_mousemove: el.on_mousemove.clone(),
            on_mouseover: el.on_mouseover.clone(),
            on_mouseout: el.on_mouseout.clone(),
            hoisted_portal: el
                .attrs
                .get(crate::layout::layout_tree::ATTR_HOISTED_PORTAL)
                .map(|s| s.as_str())
                == Some("1"),
            portal_key: el
                .attrs
                .get(crate::layout::layout_tree::ATTR_PORTAL_KEY)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            floating_id: el
                .attrs
                .get(crate::utils::popup::ATTR_FLOATING)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            region,
            canvas_paint: el
                .attrs
                .get(crate::primitives::canvas::ATTR_CANVAS_PAINT)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            bounds,
        }
    }

    pub fn target_key(&self) -> Option<String> {
        self.focus_id
            .clone()
            .or_else(|| self.portal_key.clone())
            .or_else(|| self.floating_id.clone())
    }
}

fn bounds_close(a: [f32; 4], b: [f32; 4]) -> bool {
    a.iter().zip(b.iter()).all(|(x, y)| (*x - *y).abs() < 0.5)
}

/// Identity for LCA: `focus_id` / portal / floating, then handler `Rc`, then geometry.
pub fn same_hit_node(a: &HitPathEntry, b: &HitPathEntry) -> bool {
    match (&a.focus_id, &b.focus_id) {
        (Some(x), Some(y)) => return x == y,
        (Some(_), None) | (None, Some(_)) => {}
        (None, None) => {}
    }
    match (&a.portal_key, &b.portal_key) {
        (Some(x), Some(y)) => return x == y,
        _ => {}
    }
    match (&a.floating_id, &b.floating_id) {
        (Some(x), Some(y)) => return x == y,
        _ => {}
    }
    if let (Some(ha), Some(hb)) = (&a.on_click, &b.on_click) {
        if Rc::ptr_eq(ha, hb) {
            return true;
        }
    }
    if let (Some(ha), Some(hb)) = (&a.on_mousedown, &b.on_mousedown) {
        if Rc::ptr_eq(ha, hb) {
            return true;
        }
    }
    bounds_close(a.bounds, b.bounds)
        && a.label_for == b.label_for
        && a.region == b.region
        && a.hoisted_portal == b.hoisted_portal
        && a.on_click.is_none() == b.on_click.is_none()
        && a.on_mousedown.is_none() == b.on_mousedown.is_none()
}

/// DOM-like click target: nearest common ancestor of mousedown and mouseup hit paths.
///
/// `mouseup` handlers still use the live pointer path; only `click` / `dblclick` /
/// legacy `on_click` are truncated to this prefix. A capture-only up path (single
/// node) is matched against the press path so `set_pointer_capture` still clicks.
pub fn click_dispatch_path(down: &[HitPathEntry], up: &[HitPathEntry]) -> Vec<HitPathEntry> {
    let n = down.len().min(up.len());
    let mut i = 0;
    while i < n && same_hit_node(&down[i], &up[i]) {
        i += 1;
    }
    if i > 0 {
        return up[..i].to_vec();
    }
    for u in up.iter().rev() {
        if let Some(idx) = down.iter().position(|d| same_hit_node(d, u)) {
            let mut path = down[..=idx].to_vec();
            path[idx] = u.clone();
            return path;
        }
    }
    Vec::new()
}

/// Collect the hit path from root to the deepest hit node (inclusive).
pub fn hit_path(el: &Element, x: f32, y: f32) -> Vec<HitPathEntry> {
    let mut path = Vec::new();
    hit_path_rec(el, x, y, [0.0, 0.0], &mut path);
    path
}

fn pointer_events_blocked(el: &Element) -> bool {
    !pointer_events_targets_children(el)
}

#[inline]
fn claims_own_bounds(el: &Element) -> bool {
    pointer_events_targets_self(el) && !portal_hit_passthrough(el)
}

fn hit_path_rec(
    el: &Element,
    x: f32,
    y: f32,
    scroll_acc: [f32; 2],
    out: &mut Vec<HitPathEntry>,
) -> bool {
    if pointer_events_blocked(el) {
        return false;
    }

    // Solid body portal layer: escape ancestor scroll offsets (same as paint collect).
    let scroll_acc = if el.template_id.as_deref()
        == Some(crate::layout::layout_tree::PORTAL_LAYER_ID)
        || el.style.position_fixed
    {
        [0.0_f32, 0.0_f32]
    } else {
        scroll_acc
    };

    let b = el.layout_bounds;
    let [vx, vy] = super::tree_hit::visual_xy(el, scroll_acc);

    // Root portal layer must not reject hits via its own box when children extend
    // visually (layer is a stacking shell; children carry geometry).
    let is_body_portal =
        el.template_id.as_deref() == Some(crate::layout::layout_tree::PORTAL_LAYER_ID);
    if !is_body_portal
        && (el.style.overflow.x != taffy::style::Overflow::Visible
            || el.style.overflow.y != taffy::style::Overflow::Visible)
        && (x < vx || x > vx + b[2] || y < vy || y > vy + b[3])
    {
        return false;
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

    // Record this node before descending (root → leaf order).
    let entry_index = out.len();
    out.push(HitPathEntry::from_element(el, [vx, vy, b[2], b[3]]));

    let mut child_hit = false;
    let order = {
        let mut order: Vec<usize> = (0..el.children.len()).collect();
        order.sort_by(|&a, &b| {
            let za = el.children[a].style.z_index;
            let zb = el.children[b].style.z_index;
            za.cmp(&zb).then(a.cmp(&b))
        });
        order
    };
    for i in order.into_iter().rev() {
        if hit_path_rec(&el.children[i], x, y, next_acc, out) {
            child_hit = true;
            break;
        }
    }

    let inside = x >= vx && x <= vx + b[2] && y >= vy && y <= vy + b[3];
    let hit = if claims_own_bounds(el) {
        child_hit || inside
    } else {
        child_hit
    };
    if hit {
        true
    } else {
        out.truncate(entry_index);
        false
    }
}

/// Find element by `focus_id`, falling back to `template_id`.
///
/// **Must prefer `focus_id`**：组合 Checkbox 外层与控制面可能共用同一 `template_id`，
/// 若先匹配 `template_id` 会命中无 `on_click` 的外层行，导致 Label `for` 点击无法切换。
pub fn find_by_focus_id<'a>(el: &'a Element, id: &str) -> Option<&'a Element> {
    if let Some(found) = find_by_focus_id_exact(el, id) {
        return Some(found);
    }
    find_by_template_id_exact(el, id)
}

fn find_by_focus_id_exact<'a>(el: &'a Element, id: &str) -> Option<&'a Element> {
    if el.focus_id.as_deref() == Some(id) {
        return Some(el);
    }
    for c in &el.children {
        if let Some(found) = find_by_focus_id_exact(c, id) {
            return Some(found);
        }
    }
    None
}

fn find_by_template_id_exact<'a>(el: &'a Element, id: &str) -> Option<&'a Element> {
    if el.template_id.as_deref() == Some(id) {
        return Some(el);
    }
    for c in &el.children {
        if let Some(found) = find_by_template_id_exact(c, id) {
            return Some(found);
        }
    }
    None
}

/// Dispatch pointer event along `path` (capture → target → bubble).
///
/// - `on_mousedown` / `on_mouseup` / `on_context_menu` / `on_double_click` participate fully.
/// - Legacy `on_click` fires once at the deepest node that has it on pointer **up**
///   (DOM-like click), unless a capture/target handler already called
///   [`PointerEvent::stop_propagation`] on that gesture.
pub fn dispatch_pointer(path: &[HitPathEntry], event: &mut PointerEvent, kind: PointerKind) {
    if path.is_empty() {
        return;
    }

    // Capture: root → parent of target
    event.phase = EventPhase::Capture;
    for entry in path.iter().take(path.len().saturating_sub(1)) {
        event.current_bounds = entry.bounds;
        event.region = entry.region.clone();
        fire_handler(entry, event, kind);
        if event.is_consumed() {
            return;
        }
    }

    // Target
    event.phase = EventPhase::Target;
    let target = &path[path.len() - 1];
    event.current_bounds = target.bounds;
    event.region = target.region.clone();
    fire_handler(target, event, kind);
    if event.is_consumed() {
        return;
    }

    // Bubble: parent → root (skip target)
    event.phase = EventPhase::Bubble;
    for entry in path.iter().rev().skip(1) {
        event.current_bounds = entry.bounds;
        event.region = entry.region.clone();
        fire_handler(entry, event, kind);
        if event.is_consumed() {
            return;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerKind {
    Down,
    Up,
    Click,
    ContextMenu,
    DoubleClick,
    Move,
    Over,
    Out,
}

fn fire_mouse(handler: &Option<PointerHandler>, event: &PointerEvent) {
    if let Some(cb) = handler {
        (*cb.borrow_mut())(event);
    }
}

fn fire_handler(entry: &HitPathEntry, event: &PointerEvent, kind: PointerKind) {
    let canvas_drag = matches!(kind, PointerKind::Down | PointerKind::Up)
        || (kind == PointerKind::Move && event.buttons != 0);
    if canvas_drag {
        if let Some(hex) = entry.canvas_paint.as_deref() {
            crate::primitives::canvas::mark_canvas_dirty_hex(hex);
        }
    }
    match kind {
        PointerKind::Down => {
            fire_mouse(&entry.on_mousedown, event);
        }
        PointerKind::Up => {
            fire_mouse(&entry.on_mouseup, event);
        }
        PointerKind::ContextMenu => {
            fire_mouse(&entry.on_context_menu, event);
        }
        PointerKind::Move => {
            fire_mouse(&entry.on_mousemove, event);
        }
        PointerKind::Over => {
            fire_mouse(&entry.on_mouseover, event);
        }
        PointerKind::Out => {
            fire_mouse(&entry.on_mouseout, event);
        }
        PointerKind::DoubleClick => {
            fire_mouse(&entry.on_double_click, event);
            // Also fire legacy on_click on target/bubble for convenience when detail>=2
            if event.phase != EventPhase::Capture {
                if let Some(cb) = &entry.on_click {
                    (*cb.borrow_mut())();
                }
            }
        }
        PointerKind::Click => {
            // Legacy on_click: only at target (deepest with handler) — fire once when we
            // reach the deepest entry that has on_click during target/bubble from leaf.
            if event.phase == EventPhase::Target {
                if let Some(cb) = &entry.on_click {
                    (*cb.borrow_mut())();
                }
            }
        }
    }
}

/// Resolve click callback with optional pointer capture override.
pub fn resolve_click_with_capture(
    root: &Element,
    x: f32,
    y: f32,
    capture_id: Option<&str>,
) -> Option<Rc<RefCell<dyn FnMut()>>> {
    if let Some(id) = capture_id {
        if let Some(el) = find_by_focus_id(root, id) {
            return el.on_click.clone();
        }
    }
    tree_hit::find_click(root, x, y, [0.0, 0.0])
}

/// 从 hit path 解析最终 `on_click`：优先 Label `for` → 目标控件，否则最深节点自身 handler。
///
/// 返回 `(callback, focus_id_to_set)`；`focus_id_to_set` 在经由 Label `for` 激活时指向目标控件。
pub fn resolve_click_from_path(
    root: &Element,
    path: &[HitPathEntry],
) -> (Option<Rc<RefCell<dyn FnMut()>>>, Option<String>) {
    // 先整 path 扫 Label `for`（叶→根），避免被无 handler 的文本叶抢先返回。
    for entry in path.iter().rev() {
        if let Some(for_id) = entry.label_for.as_deref() {
            if let Some(target) = find_by_focus_id(root, for_id) {
                if target.on_click.is_some() {
                    return (target.on_click.clone(), Some(for_id.to_string()));
                }
            }
        }
    }
    for entry in path.iter().rev() {
        if let Some(cb) = entry.on_click.clone() {
            return (Some(cb), None);
        }
    }
    (None, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::tree::Element;

    #[test]
    fn hit_path_orders_root_to_leaf() {
        let mut leaf = Element::new();
        leaf.layout_bounds = [10.0, 10.0, 20.0, 20.0];
        leaf.focus_id = Some("leaf".into());
        leaf = leaf.on_click(|| {});

        let mut root = Element::new();
        root.layout_bounds = [0.0, 0.0, 100.0, 100.0];
        root.focus_id = Some("root".into());
        root.children.push(leaf);

        let path = hit_path(&root, 15.0, 15.0);
        assert!(path.len() >= 2);
        assert_eq!(path[0].focus_id.as_deref(), Some("root"));
        assert_eq!(path.last().unwrap().focus_id.as_deref(), Some("leaf"));
    }

    #[test]
    fn stop_propagation_blocks_bubble() {
        let fired = Rc::new(Cell::new(0u8));
        let f1 = fired.clone();
        let f2 = fired.clone();

        let mut leaf = Element::new();
        leaf.layout_bounds = [0.0, 0.0, 10.0, 10.0];
        leaf.on_mousedown = Some(Rc::new(RefCell::new(move |e: &PointerEvent| {
            f1.set(f1.get() | 1);
            e.stop_propagation();
        })));

        let mut root = Element::new();
        root.layout_bounds = [0.0, 0.0, 100.0, 100.0];
        root.on_mousedown = Some(Rc::new(RefCell::new(move |_e: &PointerEvent| {
            f2.set(f2.get() | 2);
        })));
        root.children.push(leaf);

        let path = hit_path(&root, 5.0, 5.0);
        let mut ev = PointerEvent::new(5.0, 5.0, MouseButton::Left, 1);
        // Capture runs root first — root sets bit 2, then leaf sets bit 1 and stops
        dispatch_pointer(&path, &mut ev, PointerKind::Down);
        assert_eq!(fired.get() & 1, 1);
        // Root capture still runs before leaf target; bubble must not re-fire root's down
        // (we only fire on_mousedown once per phase walk). Capture already ran root.
        assert!(ev.is_consumed());
    }

    #[test]
    fn hit_path_bounds_are_visual_under_scroll() {
        let mut leaf = Element::new();
        leaf.layout_bounds = [0.0, 200.0, 80.0, 80.0];
        leaf.focus_id = Some("plot".into());
        leaf.on_mousedown = Some(Rc::new(RefCell::new(|_e: &PointerEvent| {})));

        let mut viewport = Element::new().overflow("hidden");
        viewport.layout_bounds = [0.0, 0.0, 100.0, 100.0];
        viewport.scroll_offset = [0.0, 180.0];
        viewport.children.push(leaf);

        // Visual leaf: y = 200 - 180 = 20.
        let path = hit_path(&viewport, 40.0, 40.0);
        let leaf_e = path
            .iter()
            .find(|e| e.focus_id.as_deref() == Some("plot"))
            .expect("scrolled leaf on path");
        assert!(
            (leaf_e.bounds[1] - 20.0).abs() < 0.5,
            "expected visual y=20, got {:?}",
            leaf_e.bounds
        );
        assert!((leaf_e.bounds[0] - 0.0).abs() < 0.5);
        assert!((leaf_e.bounds[2] - 80.0).abs() < 0.5);
    }

    #[test]
    fn label_for_resolves_compound_checkbox_control_not_row() {
        use std::cell::Cell;

        let toggled = Rc::new(Cell::new(0u32));
        let t = toggled.clone();

        // Mirrors `<Checkbox>"caption"</Checkbox>`: outer row shares template_id with control focus_id.
        let mut control = Element::new();
        control.focus_id = Some("terms".into());
        control.template_id = Some("terms".into());
        control = control.on_click(move || {
            t.set(t.get() + 1);
        });

        let mut caption = Element::new().text("Accept");
        caption.attrs.insert("for".to_string(), "terms".to_string());

        let mut row = Element::new().child(control).child(caption);
        row.template_id = Some("terms".into());

        let found = find_by_focus_id(&row, "terms").expect("control");
        assert!(
            found.on_click.is_some(),
            "must resolve focused control, not template_id-only row"
        );
        assert_eq!(found.focus_id.as_deref(), Some("terms"));

        let path = [HitPathEntry {
            label_for: Some("terms".into()),
            ..Default::default()
        }];
        let (cb, fid) = resolve_click_from_path(&row, &path);
        assert_eq!(fid.as_deref(), Some("terms"));
        (*cb.expect("forwarded click").borrow_mut())();
        assert_eq!(toggled.get(), 1);
    }

    #[test]
    fn mouse_event_exposes_dom_fields() {
        let mut ev = MouseEvent::new(40.0, 18.0, MouseButton::Left, 1);
        ev.current_bounds = [10.0, 8.0, 100.0, 32.0];
        ev.alt_key = true;
        ev.buttons = 1;
        ev.screen_x = 80.0;
        ev.movement_x = 3.0;
        assert_eq!(ev.client_x(), 40.0);
        assert_eq!(ev.client_y(), 18.0);
        assert!((ev.offset_x() - 30.0).abs() < 0.01);
        assert!((ev.offset_y() - 10.0).abs() < 0.01);
        assert_eq!(ev.button_index(), 0);
        assert_eq!(ev.which(), 1);
        assert!(ev.get_modifier_state("Alt"));
        assert!(!ev.get_modifier_state("Shift"));
        assert_eq!(ev.buttons, 1);
        ev.prevent_default();
        assert!(ev.default_prevented());
    }

    #[test]
    fn mousedown_and_mouseover_dispatch() {
        let downs = Rc::new(Cell::new(0u8));
        let overs = Rc::new(Cell::new(0u8));
        let d = downs.clone();
        let o = overs.clone();

        let mut leaf = Element::new();
        leaf.layout_bounds = [0.0, 0.0, 20.0, 20.0];
        leaf = leaf
            .on_mousedown(move |e| {
                assert_eq!(e.event_type, "mousedown");
                d.set(d.get() + 1);
            })
            .on_mouseover(move |e| {
                assert_eq!(e.related_target.as_deref(), Some("prev"));
                o.set(o.get() + 1);
            });

        let mut root = Element::new();
        root.layout_bounds = [0.0, 0.0, 100.0, 100.0];
        root.children.push(leaf);

        let path = hit_path(&root, 5.0, 5.0);
        let mut down = MouseEvent::new(5.0, 5.0, MouseButton::Left, 1);
        down.event_type = "mousedown";
        dispatch_pointer(&path, &mut down, PointerKind::Down);
        assert_eq!(downs.get(), 1);

        let mut over = MouseEvent::new(5.0, 5.0, MouseButton::Left, 0);
        over.event_type = "mouseover";
        over.related_target = Some("prev".into());
        dispatch_pointer(&path, &mut over, PointerKind::Over);
        assert_eq!(overs.get(), 1);
    }

    #[test]
    fn mousedown_is_alias_of_pointer_down_single_fire() {
        let n = Rc::new(Cell::new(0u8));
        let a = n.clone();
        let b = n.clone();
        let mut leaf = Element::new();
        leaf.layout_bounds = [0.0, 0.0, 20.0, 20.0];
        leaf = leaf
            .on_pointer_down(move |_e| {
                a.set(a.get() + 1);
            })
            .on_mousedown(move |_e| {
                b.set(b.get() + 10);
            });
        let mut root = Element::new();
        root.layout_bounds = [0.0, 0.0, 100.0, 100.0];
        root.children.push(leaf);
        let path = hit_path(&root, 5.0, 5.0);
        let mut ev = MouseEvent::new(5.0, 5.0, MouseButton::Left, 1);
        ev.event_type = "mousedown";
        dispatch_pointer(&path, &mut ev, PointerKind::Down);
        // Alias: later `on_mousedown` replaces `on_pointer_down`; still one dispatch.
        assert_eq!(n.get(), 10);
    }

    fn click_tree() -> (Element, Rc<Cell<u32>>) {
        let n = Rc::new(Cell::new(0u32));
        let n2 = n.clone();
        let mut btn = Element::new();
        btn.layout_bounds = [50.0, 0.0, 40.0, 40.0];
        btn.focus_id = Some("btn".into());
        btn = btn.on_click(move || n2.set(n2.get() + 1));

        let mut pad = Element::new();
        pad.layout_bounds = [0.0, 0.0, 40.0, 40.0];
        pad.focus_id = Some("pad".into());

        let mut root = Element::new();
        root.layout_bounds = [0.0, 0.0, 100.0, 40.0];
        root.focus_id = Some("root".into());
        root.children.push(pad);
        root.children.push(btn);
        (root, n)
    }

    #[test]
    fn click_ignores_press_outside_release_inside() {
        let (root, n) = click_tree();
        let down = hit_path(&root, 10.0, 10.0);
        let up = hit_path(&root, 60.0, 10.0);
        assert_eq!(down.last().unwrap().focus_id.as_deref(), Some("pad"));
        assert_eq!(up.last().unwrap().focus_id.as_deref(), Some("btn"));
        let click_path = click_dispatch_path(&down, &up);
        let (cb, _) = resolve_click_from_path(&root, &click_path);
        assert!(
            cb.is_none(),
            "DOM click target is the LCA; outer press + inner release must not fire the button"
        );
        assert_eq!(n.get(), 0);
    }

    #[test]
    fn click_fires_when_press_and_release_share_the_button() {
        let (root, n) = click_tree();
        let down = hit_path(&root, 60.0, 10.0);
        let up = hit_path(&root, 70.0, 12.0);
        let click_path = click_dispatch_path(&down, &up);
        let (cb, _) = resolve_click_from_path(&root, &click_path);
        (*cb.expect("button click").borrow_mut())();
        assert_eq!(n.get(), 1);
    }

    #[test]
    fn click_fires_when_release_is_on_child_of_press_target() {
        let n = Rc::new(Cell::new(0u32));
        let n2 = n.clone();
        let mut child = Element::new();
        child.layout_bounds = [55.0, 5.0, 20.0, 20.0];
        child.focus_id = Some("label".into());

        let mut btn = Element::new();
        btn.layout_bounds = [50.0, 0.0, 40.0, 40.0];
        btn.focus_id = Some("btn".into());
        btn = btn.on_click(move || n2.set(n2.get() + 1));
        btn.children.push(child);

        let mut root = Element::new();
        root.layout_bounds = [0.0, 0.0, 100.0, 40.0];
        root.focus_id = Some("root".into());
        root.children.push(btn);

        let down = hit_path(&root, 60.0, 10.0);
        let up = hit_path(&root, 62.0, 12.0);
        assert!(down.iter().any(|e| e.focus_id.as_deref() == Some("btn")));
        let click_path = click_dispatch_path(&down, &up);
        let (cb, _) = resolve_click_from_path(&root, &click_path);
        (*cb.expect("ancestor button").borrow_mut())();
        assert_eq!(n.get(), 1);
    }

    #[test]
    fn click_survives_pointer_capture_short_up_path() {
        let (root, n) = click_tree();
        let down = hit_path(&root, 60.0, 10.0);
        let captured = down
            .iter()
            .find(|e| e.focus_id.as_deref() == Some("btn"))
            .cloned()
            .expect("btn on press path");
        let up = vec![captured];
        let click_path = click_dispatch_path(&down, &up);
        let (cb, _) = resolve_click_from_path(&root, &click_path);
        (*cb.expect("captured button").borrow_mut())();
        assert_eq!(n.get(), 1);
    }
}
