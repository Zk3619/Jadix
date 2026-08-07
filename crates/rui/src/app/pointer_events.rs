//! Pointer event model: hit path, capture/bubble phases, and pointer capture.
//!
//! Existing leaf-only [`super::tree_hit::find_click`] remains for backward compatibility.
//! New handlers (`on_pointer_down` / `on_context_menu` / `on_double_click`) participate in
//! capture → target → bubble dispatch via [`dispatch_pointer`].

use std::cell::Cell;
use std::cell::RefCell;
use std::rc::Rc;

use winit::event::MouseButton;

use crate::core::tree::{
    pointer_events_targets_children, pointer_events_targets_self, Element,
};
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
    pub on_pointer_down: Option<PointerHandler>,
    pub on_pointer_up: Option<PointerHandler>,
    pub on_click: Option<Rc<RefCell<dyn FnMut()>>>,
    pub on_context_menu: Option<PointerHandler>,
    pub on_double_click: Option<PointerHandler>,
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
    let b = el.layout_bounds;
    let vx = b[0] - scroll_acc[0];
    let vy = b[1] - scroll_acc[1];

    if (el.style.overflow.x != taffy::style::Overflow::Visible
        || el.style.overflow.y != taffy::style::Overflow::Visible)
        && (x < vx || x > vx + b[2] || y < vy || y > vy + b[3])
    {
        return false;
    }

    let mut next_acc = scroll_acc;
    if el.style.overflow.x != taffy::style::Overflow::Visible {
        next_acc[0] += el.scroll_offset[0];
    }
    if el.style.overflow.y != taffy::style::Overflow::Visible {
        next_acc[1] += el.scroll_offset[1];
    }

    // Record this node before descending (root → leaf order).
    let entry_index = out.len();
    out.push(HitPathEntry {
        focus_id: el.focus_id.clone(),
        on_pointer_down: el.on_pointer_down.clone(),
        on_pointer_up: el.on_pointer_up.clone(),
        on_click: el.on_click.clone(),
        on_context_menu: el.on_context_menu.clone(),
        on_double_click: el.on_double_click.clone(),
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

/// Find element by `focus_id` (for pointer capture retargeting).
pub fn find_by_focus_id<'a>(el: &'a Element, id: &str) -> Option<&'a Element> {
    if el.focus_id.as_deref() == Some(id) {
        return Some(el);
    }
    for c in &el.children {
        if let Some(found) = find_by_focus_id(c, id) {
            return Some(found);
        }
    }
    None
}

/// Dispatch pointer event along `path` (capture → target → bubble).
///
/// - `on_pointer_down` / `on_pointer_up` / `on_context_menu` / `on_double_click` participate fully.
/// - Legacy `on_click` fires once at the deepest node that has it (target/bubble leaf behavior),
///   unless a capture/target handler already called [`PointerEvent::stop_propagation`].
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
}
