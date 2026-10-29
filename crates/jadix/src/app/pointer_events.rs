//! Pointer event model: hit path, capture/bubble phases, and pointer capture.
//!
//! Aligns with the DOM [`MouseEvent`](https://developer.mozilla.org/zh-CN/docs/Web/API/MouseEvent)
//! surface used by `click` / `dblclick` / `mousedown` / `mouseup` / `mousemove` /
//! `mouseover` / `mouseout`. Existing leaf-only [`super::tree_hit::find_click`] remains
//! for Label-`for` resolution. Handlers participate in capture → target → bubble
//! via [`dispatch_pointer`].

use std::cell::Cell;
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use winit::event::MouseButton;

use crate::core::tree::{pointer_events_targets_children, Element};

use super::tree_hit;

/// Composite option boxes forward activation without becoming accessible labels.
pub const ATTR_ACTIVATE_FOR: &str = "data-jadix-activate-for";

thread_local! {
    static LAST_POINTER_CLIENT: Cell<[f32; 2]> = const { Cell::new([-1.0, -1.0]) };
    static LAST_HOVER_TARGET_BOUNDS: Cell<[f32; 4]> = const { Cell::new([0.0; 4]) };
    static LAST_HOVER_SAMPLE_VALID: Cell<bool> = const { Cell::new(false) };
}

pub(crate) fn note_pointer_client(x: f32, y: f32) {
    LAST_POINTER_CLIENT.set([x, y]);
}

pub(crate) fn note_hover_target_bounds(bounds: [f32; 4]) {
    LAST_HOVER_TARGET_BOUNDS.set(bounds);
    LAST_HOVER_SAMPLE_VALID.set(bounds[2] > 0.5 && bounds[3] > 0.5);
}

pub(crate) fn clear_hover_target_sample() {
    LAST_HOVER_SAMPLE_VALID.set(false);
}

/// Last exclusive-hover visual box + client pointer (updated on scroll with a still cursor).
pub fn last_hover_sample() -> Option<([f32; 2], [f32; 4])> {
    if !LAST_HOVER_SAMPLE_VALID.get() {
        return None;
    }
    Some((LAST_POINTER_CLIENT.get(), LAST_HOVER_TARGET_BOUNDS.get()))
}

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
        _ => 0,
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
        _ => 0,
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
    /// The **visual** box of the node currently being distributed (window logical pixel `[x,y,w,h]`, ancestor scroll has been deducted).
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
    /// Deepest focus identity on the original hit path; unchanged during propagation.
    pub target_focus_id: Option<String>,
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
            target_focus_id: None,
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
    /// Element was marked `jadix_hoisted_portal` (Solid body overlay surface).
    pub hoisted_portal: bool,
    /// `jadix_portal_key` when present (stable portal identity after hoist).
    pub portal_key: Option<String>,
    /// `jadix_floating` id when present (Select / Popover / Menu panel).
    pub floating_id: Option<String>,
    /// Canvas-like [`MouseEvent.region`](https://developer.mozilla.org/zh-CN/docs/Web/API/MouseEvent/region).
    pub region: Option<String>,
    /// WP-3.12 paint id hex（`jadix_canvas_paint`）；按下/拖线时标 dirty。
    pub canvas_paint: Option<String>,
    /// 该节点可视盒（窗口像素，已扣祖先 `scroll_acc`）。与命中判定同一套坐标，
    /// 供 handler 把指针映射到局部逻辑坐标（Chart / ScrollArea 内控件）。
    pub bounds: [f32; 4],
    /// `hover:` / `group-hover:` / `peer-hover:` utilities on this node.
    pub class_pointer_hover: bool,
    /// Tailwind `group` / `peer` marker (`jadix_group` / `jadix_peer`).
    pub class_group_or_peer: bool,
    /// This node's `cursor`, if set. Deepest `Some` is the resolved pointer icon.
    pub cursor: Option<crate::style::CursorValue>,
    /// `on_hover` is installed. CursorMoved skips the hover walk when neither
    /// the previous nor the next chain has one.
    pub has_on_hover: bool,
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
                .get(crate::utils::text_selection::ATTR_LABEL_FOR)
                .or_else(|| el.attrs.get(ATTR_ACTIVATE_FOR))
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
            class_pointer_hover: el
                .class_set
                .as_ref()
                .is_some_and(|cs| cs.has_pointer_hover_variants()),
            class_group_or_peer: el.attrs.contains_key("jadix_group")
                || el.attrs.contains_key("jadix_peer"),
            cursor: el.cursor.clone(),
            has_on_hover: el.on_hover.is_some(),
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
    if let (Some(x), Some(y)) = (&a.portal_key, &b.portal_key) {
        return x == y;
    }
    if let (Some(x), Some(y)) = (&a.floating_id, &b.floating_id) {
        return x == y;
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

/// Same widget chain (empty padding, sliding inside one box). Used to skip the
/// gallery-wide `class_variant_hover_needs_frame` walk on CursorMoved.
pub fn hit_path_identity_eq(a: &[HitPathEntry], b: &[HitPathEntry]) -> bool {
    a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| same_hit_node(x, y))
}

/// Either chain samples a pointer-dependent class variant (or a `group`/`peer`
/// marker whose descendants might). Crossing `sm:`/`dark:`-only boxes must not
/// pay the gallery-wide `class_variant_hover_needs_frame` walk.
pub fn class_hover_path_may_change_samples(prev: &[HitPathEntry], next: &[HitPathEntry]) -> bool {
    prev.iter()
        .chain(next.iter())
        .any(|e| e.class_pointer_hover || e.class_group_or_peer)
}

/// Deepest `focus_id` on an exclusive hit chain. Same bubble as `find_focus_id_at`.
pub fn path_deepest_focus_id(path: &[HitPathEntry]) -> Option<&str> {
    path.iter().rev().find_map(|e| e.focus_id.as_deref())
}

/// Deepest `scroll:` viewport rest on the chain. Replaces `collect_scroll_chain_at`
/// for area-hover, which only kept this string.
pub fn path_deepest_scroll_area_rest(path: &[HitPathEntry]) -> Option<&str> {
    path.iter().rev().find_map(|e| {
        e.focus_id
            .as_deref()?
            .strip_prefix(crate::core::scroll_viewport::SCROLL_AREA_FOCUS_PREFIX)
    })
}

/// Deepest explicit `cursor`. Same bubble as `pick_cursor`.
pub fn path_resolved_cursor(path: &[HitPathEntry]) -> Option<crate::style::CursorValue> {
    path.iter().rev().find_map(|e| e.cursor.clone())
}

pub fn path_has_on_hover(path: &[HitPathEntry]) -> bool {
    path.iter().any(|e| e.has_on_hover)
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

/// Physical-pixel slop matching primary double-click; below this the pointer is still.
pub const CLICK_STILL_SLOP_PX: f32 = 6.0;

pub fn click_pointer_moved(press_physical: [f32; 2], release_physical: [f32; 2]) -> bool {
    (release_physical[0] - press_physical[0]).hypot(release_physical[1] - press_physical[1])
        > CLICK_STILL_SLOP_PX
}

/// Press+release click.
///
/// A still pointer confirms the press path when the engine moved content (smooth
/// chase already in flight). User-initiated scroll after press cancels `click`.
/// User pointer motion uses the DOM LCA.
pub fn click_gesture_path(
    down: &[HitPathEntry],
    up: &[HitPathEntry],
    pointer_moved: bool,
    scrolled_since_press: bool,
) -> Vec<HitPathEntry> {
    if scrolled_since_press {
        return Vec::new();
    }
    if !pointer_moved && !down.is_empty() {
        return down.to_vec();
    }
    click_dispatch_path(down, up)
}

struct HitNode {
    ptr: *const Element,
    bounds: [f32; 4],
}

fn hit_path_nodes(el: &Element, x: f32, y: f32) -> Vec<HitNode> {
    let mut path = Vec::new();
    hit_path_walk(el, x, y, &mut path);
    path
}

/// Collect the hit path from root to the deepest hit node (inclusive).
pub fn hit_path(el: &Element, x: f32, y: f32) -> Vec<HitPathEntry> {
    hit_path_nodes(el, x, y)
        .into_iter()
        .map(|n| {
            // SAFETY: `n.ptr` is a node in `el`'s tree for the duration of this call.
            HitPathEntry::from_element(unsafe { &*n.ptr }, n.bounds)
        })
        .collect()
}

pub(crate) fn captured_path(root: &Element) -> Option<Vec<HitPathEntry>> {
    super::pointer_capture_id()
        .and_then(|id| super::tree_hit::captured_pointer_hit(root, id.as_str()))
        .map(|entry| vec![entry])
}

/// Exclusive pointer chain (root → leaf), same walk as [`hit_path`].
///
/// CSS `:hover` must use this set: geometric AABB alone paints covered siblings
/// (ColorPicker popover over a DatePicker trigger).
pub(crate) fn pointer_hit_node_set(el: &Element, x: f32, y: f32) -> HashSet<*const Element> {
    if crate::utils::drag::runtime::is_active() {
        return HashSet::new();
    }
    hit_path_nodes(el, x, y)
        .into_iter()
        .map(|n| n.ptr)
        .collect()
}

fn pointer_events_blocked(el: &Element) -> bool {
    !pointer_events_targets_children(el)
}

/// Exclusive hit path (root → leaf). Heap walk: a 4k parent-chain overflows the
/// default 1 MiB thread if this maps tree depth onto the OS stack (`pointer_hit_node_set`
/// is on the class-variant sync path).
fn hit_path_walk(root: &Element, x: f32, y: f32, out: &mut Vec<HitNode>) {
    struct Frame {
        ptr: *const Element,
        scroll_acc: [f32; 2],
        ancestor_linear: crate::style::Mat2d,
        ancestor_scale: [f32; 2],
        order: Vec<usize>,
        next_child: usize,
        entry_index: usize,
        child_hit: bool,
        started: bool,
    }
    let mut stack = vec![Frame {
        ptr: root as *const Element,
        scroll_acc: [0.0, 0.0],
        ancestor_linear: crate::style::MAT2D_IDENTITY,
        ancestor_scale: [1.0, 1.0],
        order: Vec::new(),
        next_child: 0,
        entry_index: 0,
        child_hit: false,
        started: false,
    }];
    while let Some(mut frame) = stack.pop() {
        // SAFETY: `ptr` is a node in `root`'s tree for the duration of this call.
        let el = unsafe { &*frame.ptr };
        if !frame.started {
            if pointer_events_blocked(el) {
                continue;
            }

            // Solid body portal layer: escape ancestor scroll offsets (same as paint collect).
            let scroll_acc = if el.template_id.as_deref()
                == Some(crate::layout::layout_tree::PORTAL_LAYER_ID)
                || el.style.position_fixed
            {
                [0.0_f32, 0.0_f32]
            } else {
                frame.scroll_acc
            };

            let is_body_portal =
                el.template_id.as_deref() == Some(crate::layout::layout_tree::PORTAL_LAYER_ID);
            if !is_body_portal
                && !crate::core::tree::overflow_clip_contains_pointer(
                    el,
                    [x, y],
                    scroll_acc,
                    frame.ancestor_linear,
                    frame.ancestor_scale,
                )
            {
                continue;
            }
            if !crate::core::tree::clip_path_contains_pointer(
                el,
                [x, y],
                scroll_acc,
                frame.ancestor_linear,
                frame.ancestor_scale,
            ) {
                continue;
            }

            // Record this node before descending (root → leaf order).
            let entry_index = out.len();
            let paint_bounds = crate::core::tree::visual_paint_aabb_with_ancestor(
                el,
                scroll_acc,
                frame.ancestor_linear,
                frame.ancestor_scale,
            );
            out.push(HitNode {
                ptr: el as *const Element,
                bounds: paint_bounds,
            });

            // Highest z_index / later DOM sibling first (same as `order.rev()`).
            let mut order: Vec<usize> = (0..el.children.len()).collect();
            order.sort_by(|&a, &b| {
                let za = el.children[a].style.z_index;
                let zb = el.children[b].style.z_index;
                za.cmp(&zb).then(a.cmp(&b))
            });
            order.reverse();
            frame.scroll_acc = scroll_acc;
            frame.order = order;
            frame.next_child = 0;
            frame.entry_index = entry_index;
            frame.child_hit = false;
            frame.started = true;
        }

        if frame.next_child > 0 && out.len() > frame.entry_index + 1 {
            frame.child_hit = true;
        }

        if !frame.child_hit && frame.next_child < frame.order.len() {
            let i = frame.order[frame.next_child];
            frame.next_child += 1;
            let is_body_portal =
                el.template_id.as_deref() == Some(crate::layout::layout_tree::PORTAL_LAYER_ID);
            let next_acc = if is_body_portal {
                frame.scroll_acc
            } else {
                super::tree_hit::next_hit_scroll_acc(el, frame.scroll_acc)
            };
            let (child_lin, child_sc) = crate::core::tree::child_paint_ancestor(
                el,
                frame.ancestor_linear,
                frame.ancestor_scale,
            );
            let child_ptr = std::ptr::addr_of!(el.children[i]);
            stack.push(frame);
            stack.push(Frame {
                ptr: child_ptr,
                scroll_acc: next_acc,
                ancestor_linear: child_lin,
                ancestor_scale: child_sc,
                order: Vec::new(),
                next_child: 0,
                entry_index: 0,
                child_hit: false,
                started: false,
            });
            continue;
        }

        let self_hit = tree_hit::element_self_pointer_hit_with(
            el,
            x,
            y,
            frame.scroll_acc,
            frame.ancestor_linear,
            frame.ancestor_scale,
        );
        if !(frame.child_hit || self_hit) {
            out.truncate(frame.entry_index);
        }
    }
}

/// Find element by `focus_id`, falling back to `template_id`.
///
/// **Must prefer `focus_id`**: The combined Checkbox outer layer and control surface may share the same `template_id`,
/// 若先匹配 `template_id` 会命中无 `on_click` 的外层行，导致 Label `for` 点击无法切换。
pub fn find_by_focus_id<'a>(el: &'a Element, id: &str) -> Option<&'a Element> {
    if let Some(found) = find_by_focus_id_exact(el, id) {
        return Some(found);
    }
    find_by_template_id_exact(el, id)
}

fn find_by_focus_id_exact<'a>(el: &'a Element, id: &str) -> Option<&'a Element> {
    crate::core::tree::find_in_tree(el, |n| n.focus_id.as_deref() == Some(id))
}

fn find_by_template_id_exact<'a>(el: &'a Element, id: &str) -> Option<&'a Element> {
    crate::core::tree::find_in_tree(el, |n| n.template_id.as_deref() == Some(id))
}

/// Dispatch pointer event along `path` (capture → target → bubble).
///
/// - `on_mousedown` / `on_mouseup` / `on_context_menu` / `on_double_click` participate fully.
/// - Legacy `on_click` fires once at the deepest node that has it on pointer **up**
///   (DOM-like click), unless a capture/target handler already called
///   [`PointerEvent::stop_propagation`] on that gesture.
pub fn dispatch_pointer(path: &[HitPathEntry], event: &mut PointerEvent, kind: PointerKind) {
    // The same hit path skips hover reconciliation. Keep the client sample
    // current so reactive chart sync cannot restore the previous frame's pose.
    if matches!(kind, PointerKind::Move) {
        note_pointer_client(event.x, event.y);
    }
    if path.is_empty() {
        return;
    }

    event.target_focus_id = path_deepest_focus_id(path).map(str::to_owned);
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
#[path = "tests/pointer_events.rs"]
mod tests;
