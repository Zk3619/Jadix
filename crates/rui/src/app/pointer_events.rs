//! Pointer event model: hit path, capture/bubble phases, and pointer capture.
//!
//! Existing leaf-only [`super::tree_hit::find_click`] remains for backward compatibility.
//! New handlers (`on_pointer_down` / `on_context_menu` / `on_double_click`) participate in
//! capture → target → bubble dispatch via [`dispatch_pointer`].

use std::cell::Cell;
use std::cell::RefCell;
use std::rc::Rc;

use winit::event::MouseButton;

use crate::core::tree::{pointer_events_targets_children, pointer_events_targets_self, Element};
use crate::layout::layout_tree::portal_hit_passthrough;

use super::tree_hit;

/// DOM-like event phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventPhase {
    Capture,
    Target,
    Bubble,
}

/// Pointer event delivered to [`Element`] handlers along the hit path.
#[derive(Debug)]
pub struct PointerEvent {
    pub x: f32,
    pub y: f32,
    pub button: MouseButton,
    pub phase: EventPhase,
    /// Click count (1 = single, 2 = double, …).
    pub detail: u8,
    consumed: Cell<bool>,
}

impl PointerEvent {
    pub fn new(x: f32, y: f32, button: MouseButton, detail: u8) -> Self {
        Self {
            x,
            y,
            button,
            phase: EventPhase::Target,
            detail,
            consumed: Cell::new(false),
        }
    }

    pub fn stop_propagation(&self) {
        self.consumed.set(true);
    }

    pub fn is_consumed(&self) -> bool {
        self.consumed.get()
    }
}

pub type PointerHandler = Rc<RefCell<dyn FnMut(&PointerEvent)>>;

/// One node on the hit path (root → … → leaf).
#[derive(Clone)]
pub struct HitPathEntry {
    pub focus_id: Option<String>,
    /// Label `attrs["for"]`（Solid `for`）→ 转发点击到匹配 `focus_id` 的控件。
    pub label_for: Option<String>,
    pub on_pointer_down: Option<PointerHandler>,
    pub on_pointer_up: Option<PointerHandler>,
    pub on_click: Option<Rc<RefCell<dyn FnMut()>>>,
    pub on_context_menu: Option<PointerHandler>,
    pub on_double_click: Option<PointerHandler>,
    /// Element was marked `rui_hoisted_portal` (Solid body overlay surface).
    pub hoisted_portal: bool,
    /// `rui_portal_key` when present (stable portal identity after hoist).
    pub portal_key: Option<String>,
    /// `rui_floating` id when present (Select / Popover / Menu panel).
    pub floating_id: Option<String>,
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
    let vx = b[0] - scroll_acc[0];
    let vy = b[1] - scroll_acc[1];

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
    let portal_key = el
        .attrs
        .get(crate::layout::layout_tree::ATTR_PORTAL_KEY)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let floating_id = el
        .attrs
        .get(crate::utils::popup::ATTR_FLOATING)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    out.push(HitPathEntry {
        focus_id: el.focus_id.clone(),
        label_for: el
            .attrs
            .get(crate::components::label::ATTR_LABEL_FOR)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        on_pointer_down: el.on_pointer_down.clone(),
        on_pointer_up: el.on_pointer_up.clone(),
        on_click: el.on_click.clone(),
        on_context_menu: el.on_context_menu.clone(),
        on_double_click: el.on_double_click.clone(),
        hoisted_portal: el
            .attrs
            .get(crate::layout::layout_tree::ATTR_HOISTED_PORTAL)
            .map(|s| s.as_str())
            == Some("1"),
        portal_key,
        floating_id,
    });

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
/// - `on_pointer_down` / `on_pointer_up` / `on_context_menu` / `on_double_click` participate fully.
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
        fire_handler(entry, event, kind);
        if event.is_consumed() {
            return;
        }
    }

    // Target
    event.phase = EventPhase::Target;
    let target = &path[path.len() - 1];
    fire_handler(target, event, kind);
    if event.is_consumed() {
        return;
    }

    // Bubble: parent → root (skip target)
    event.phase = EventPhase::Bubble;
    for entry in path.iter().rev().skip(1) {
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
}

fn fire_handler(entry: &HitPathEntry, event: &PointerEvent, kind: PointerKind) {
    match kind {
        PointerKind::Down => {
            if let Some(cb) = &entry.on_pointer_down {
                (*cb.borrow_mut())(event);
            }
        }
        PointerKind::Up => {
            if let Some(cb) = &entry.on_pointer_up {
                (*cb.borrow_mut())(event);
            }
        }
        PointerKind::ContextMenu => {
            if let Some(cb) = &entry.on_context_menu {
                (*cb.borrow_mut())(event);
            }
        }
        PointerKind::DoubleClick => {
            if let Some(cb) = &entry.on_double_click {
                (*cb.borrow_mut())(event);
            }
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
        leaf.on_pointer_down = Some(Rc::new(RefCell::new(move |e: &PointerEvent| {
            f1.set(f1.get() | 1);
            e.stop_propagation();
        })));

        let mut root = Element::new();
        root.layout_bounds = [0.0, 0.0, 100.0, 100.0];
        root.on_pointer_down = Some(Rc::new(RefCell::new(move |_e: &PointerEvent| {
            f2.set(f2.get() | 2);
        })));
        root.children.push(leaf);

        let path = hit_path(&root, 5.0, 5.0);
        let mut ev = PointerEvent::new(5.0, 5.0, MouseButton::Left, 1);
        // Capture runs root first — root sets bit 2, then leaf sets bit 1 and stops
        dispatch_pointer(&path, &mut ev, PointerKind::Down);
        assert_eq!(fired.get() & 1, 1);
        // Root capture still runs before leaf target; bubble must not re-fire root's down
        // (we only fire on_pointer_down once per phase walk). Capture already ran root.
        assert!(ev.is_consumed());
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
            focus_id: None,
            label_for: Some("terms".into()),
            on_pointer_down: None,
            on_pointer_up: None,
            on_click: None,
            on_context_menu: None,
            on_double_click: None,
            hoisted_portal: false,
            portal_key: None,
            floating_id: None,
        }];
        let (cb, fid) = resolve_click_from_path(&row, &path);
        assert_eq!(fid.as_deref(), Some("terms"));
        (*cb.expect("forwarded click").borrow_mut())();
        assert_eq!(toggled.get(), 1);
    }
}
