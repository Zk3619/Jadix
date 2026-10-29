//! interact.js **dropzone** 面 — 映射到 solid-dnd [`Droppable`](super::builder::droppable)。
//!
//! Per-zone `overlap` / `checker` / lifecycle hooks (`dropactivate` … `drop`) live
//! here so [`super::session`] stays a thin tick loop.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use super::geometry::Rect;
use super::session::DragEvent;

fn slot_key(scope: &str, id: &str) -> String {
    format!("{scope}\0{id}")
}

/// interact `overlap`: `pointer` | `center` | `0..1` ratio of draggable area.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum DropOverlap {
    #[default]
    Pointer,
    Center,
    /// `intersection_area / draggable_area` must be ≥ this value.
    Ratio(f32),
}

impl DropOverlap {
    pub fn parse(s: &str) -> Self {
        let t = s.trim();
        match t.to_ascii_lowercase().as_str() {
            "center" | "centre" => Self::Center,
            "pointer" => Self::Pointer,
            _ => t
                .parse::<f32>()
                .ok()
                .filter(|r| (0.0..=1.0).contains(r))
                .map(Self::Ratio)
                .unwrap_or(Self::Pointer),
        }
    }

    pub fn passes(&self, pointer: [f32; 2], dragged: Rect, zone: Rect) -> bool {
        match self {
            Self::Pointer => zone.contains_point(pointer),
            Self::Center => zone.contains_point(dragged.center()),
            Self::Ratio(min) => dragged.overlap_fraction_in(zone) >= *min,
        }
    }
}

pub type DropzoneHandler = Rc<RefCell<dyn FnMut(&DragEvent)>>;
pub type DropCheckerFn = Rc<dyn Fn(&DragEvent, bool) -> bool>;

#[derive(Clone, Default)]
pub struct DropzoneHooks {
    pub on_activate: Option<DropzoneHandler>,
    pub on_deactivate: Option<DropzoneHandler>,
    pub on_enter: Option<DropzoneHandler>,
    pub on_leave: Option<DropzoneHandler>,
    pub on_move: Option<DropzoneHandler>,
    pub on_drop: Option<DropzoneHandler>,
}

#[derive(Clone, Default)]
pub struct DropzoneConfig {
    pub overlap: DropOverlap,
    pub checker: Option<DropCheckerFn>,
    pub hooks: DropzoneHooks,
}

crate::authoring::window::window_local! {
    static CONFIG: RefCell<HashMap<String, DropzoneConfig>> = RefCell::new(HashMap::new());
    static ACTIVATED: RefCell<HashMap<String, Vec<String>>> = RefCell::new(HashMap::new());
    static ENTERED: RefCell<HashMap<String, Option<String>>> = RefCell::new(HashMap::new());
}

pub fn set_dropzone_config(scope: &str, id: &str, cfg: DropzoneConfig) {
    CONFIG.with(|m| m.borrow_mut().insert(slot_key(scope, id), cfg));
}

pub fn overlap_for(scope: &str, id: &str) -> DropOverlap {
    CONFIG.with(|m| {
        m.borrow()
            .get(&slot_key(scope, id))
            .map(|c| c.overlap)
            .unwrap_or_default()
    })
}

pub fn checker_for(scope: &str, id: &str) -> Option<DropCheckerFn> {
    CONFIG.with(|m| {
        m.borrow()
            .get(&slot_key(scope, id))
            .and_then(|c| c.checker.clone())
    })
}

fn fire_hook(
    scope: &str,
    id: &str,
    pick: impl Fn(&DropzoneHooks) -> &Option<DropzoneHandler>,
    ev: &DragEvent,
) {
    CONFIG.with(|m| {
        let key = slot_key(scope, id);
        let borrowed = m.borrow();
        let Some(cfg) = borrowed.get(&key) else {
            return;
        };
        let Some(h) = pick(&cfg.hooks).clone() else {
            return;
        };
        h.borrow_mut()(ev);
    });
}

/// interact `dropactivate` — acceptable drag crossed the activation threshold.
pub fn on_drag_activated(scope: &str, accepting: &[String], ev: &DragEvent) {
    for id in accepting {
        fire_hook(scope, id, |h| &h.on_activate, ev);
    }
    ACTIVATED.with(|m| m.borrow_mut().insert(scope.to_string(), accepting.to_vec()));
    ENTERED.with(|m| m.borrow_mut().insert(scope.to_string(), None));
}

/// interact `dropdeactivate` on end / cancel.
pub fn on_drag_deactivated(scope: &str, ev: &DragEvent) {
    let ids = ACTIVATED.with(|m| m.borrow_mut().remove(scope).unwrap_or_default());
    for id in ids {
        fire_hook(scope, &id, |h| &h.on_deactivate, ev);
    }
    ENTERED.with(|m| m.borrow_mut().insert(scope.to_string(), None));
}

/// `dragenter` / `dragleave` / `dropmove` while active.
pub fn on_hover_change(scope: &str, prev: Option<&str>, next: Option<&str>, ev: &DragEvent) {
    let prev_entered = ENTERED.with(|m| m.borrow().get(scope).cloned().flatten());
    let prev_id = prev_entered.as_deref().or(prev);
    if prev_id != next {
        if let Some(p) = prev_id {
            fire_hook(scope, p, |h| &h.on_leave, ev);
        }
        if let Some(n) = next {
            fire_hook(scope, n, |h| &h.on_enter, ev);
        }
        ENTERED.with(|m| {
            m.borrow_mut()
                .insert(scope.to_string(), next.map(str::to_string))
        });
    }
    if let Some(n) = next {
        fire_hook(scope, n, |h| &h.on_move, ev);
    }
}

/// interact `drop` on the active zone (before scope `on_drag_end`).
pub fn on_drop(scope: &str, droppable_id: &str, ev: &DragEvent) {
    fire_hook(scope, droppable_id, |h| &h.on_drop, ev);
}

#[cfg(all(test, feature = "product-tests"))]
#[path = "tests/dropzone.rs"]
mod tests;

pub(super) fn take_scope(scope: &str) -> Vec<DropzoneConfig> {
    let prefix = format!("{scope}\0");
    let removed = CONFIG.with(|slots| {
        let mut slots = slots.borrow_mut();
        let keys: Vec<_> = slots
            .keys()
            .filter(|key| key.starts_with(&prefix))
            .cloned()
            .collect();
        keys.into_iter()
            .filter_map(|key| slots.remove(&key))
            .collect()
    });
    ACTIVATED.with(|slots| slots.borrow_mut().remove(scope));
    ENTERED.with(|slots| slots.borrow_mut().remove(scope));
    removed
}
