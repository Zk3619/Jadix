//! Item metadata registry: typed `data` bags, per-item flags, idle hit tables.
//!
//! Solid-dnd keeps droppables in context store; Jadix normally walks the laid-out
//! tree ([`super::collect_hits`]). This module mirrors registered slots so
//! [`super::DragCtx::hits`] works while idle without holding the app root.

use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use super::session::DragHit;
use crate::core::reactivity::Signal;

fn slot_key(scope: &str, id: &str) -> String {
    format!("{scope}\0{id}")
}

/// Per-draggable chrome overrides ([`super::builder`] / solid-dnd `skipTransform`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ItemFlags {
    pub skip_shift: bool,
    pub skip_follow: bool,
    /// SortableJS `pull: 'clone'` — source stays; drop inserts a copy.
    pub pull_clone: bool,
}

crate::authoring::window::window_local! {
    static ITEM_DATA: RefCell<HashMap<String, Rc<dyn Any>>> = RefCell::new(HashMap::new());
    static ITEM_FLAGS: RefCell<HashMap<String, ItemFlags>> = RefCell::new(HashMap::new());
    static REGISTERED: RefCell<HashMap<String, DragHit>> = RefCell::new(HashMap::new());
    static REGISTRY_EPOCHS: RefCell<HashMap<String, Signal<u64>>> = RefCell::new(HashMap::new());
}

pub fn bind_epoch(scope: &str, epoch: Signal<u64>) {
    REGISTRY_EPOCHS.with(|m| {
        m.borrow_mut().insert(scope.to_string(), epoch);
    });
}

pub fn set_item_data(scope: &str, id: &str, data: Rc<dyn Any>) {
    ITEM_DATA.with(|m| {
        m.borrow_mut().insert(slot_key(scope, id), data);
    });
}

pub fn item_data(scope: &str, id: &str) -> Option<Rc<dyn Any>> {
    ITEM_DATA.with(|m| m.borrow().get(&slot_key(scope, id)).cloned())
}

pub fn set_item_flags(scope: &str, id: &str, flags: ItemFlags) {
    ITEM_FLAGS.with(|m| {
        m.borrow_mut().insert(slot_key(scope, id), flags);
    });
}

pub fn item_flags(scope: &str, id: &str) -> ItemFlags {
    ITEM_FLAGS.with(|m| {
        m.borrow()
            .get(&slot_key(scope, id))
            .copied()
            .unwrap_or_default()
    })
}

/// Register slot metadata before the first layout walk (rect filled on collect).
pub fn register_slot_meta(hit: DragHit) {
    let key = slot_key(&hit.scope_id, &hit.id);
    REGISTERED.with(|m| {
        let rect = m.borrow().get(&key).map(|h| h.rect).unwrap_or_default();
        let mut h = hit;
        h.rect = rect;
        m.borrow_mut().insert(key, h);
    });
}

/// Register or refresh slot metadata (rect updated on each [`super::collect_hits`]).
pub fn register_slot(hit: DragHit) -> bool {
    let key = slot_key(&hit.scope_id, &hit.id);
    REGISTERED.with(|m| {
        let mut slots = m.borrow_mut();
        let changed = slots.get(&key) != Some(&hit);
        slots.insert(key, hit);
        changed
    })
}

/// Merge a layout walk into the scope registry (idle + debugger HUD).
pub fn sync_registered_from_collect(hits: &[DragHit]) {
    let mut changed_scopes = std::collections::HashSet::new();
    for h in hits {
        if register_slot(h.clone()) {
            changed_scopes.insert(h.scope_id.clone());
        }
    }
    REGISTRY_EPOCHS.with(|epochs| {
        let epochs = epochs.borrow();
        for scope in changed_scopes {
            if let Some(epoch) = epochs.get(&scope).cloned() {
                epoch.set(epoch.peek().wrapping_add(1));
            }
        }
    });
}

/// Idle hit table for one provider (solid-dnd context store analog).
pub fn registered_hits_in_scope(scope_id: &str) -> Vec<DragHit> {
    REGISTERED.with(|m| {
        let mut out: Vec<DragHit> = m
            .borrow()
            .values()
            .filter(|h| h.scope_id == scope_id)
            .cloned()
            .collect();
        out.sort_by(|a, b| a.id.cmp(&b.id));
        out
    })
}

#[cfg(all(test, feature = "product-tests"))]
#[path = "tests/registry.rs"]
mod tests;

pub(super) fn take_scope(scope: &str) -> Vec<Rc<dyn Any>> {
    let prefix = format!("{scope}\0");
    let removed = ITEM_DATA.with(|slots| {
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
    ITEM_FLAGS.with(|slots| {
        slots
            .borrow_mut()
            .retain(|key, _| !key.starts_with(&prefix))
    });
    REGISTERED.with(|slots| {
        slots
            .borrow_mut()
            .retain(|key, _| !key.starts_with(&prefix))
    });
    REGISTRY_EPOCHS.with(|epochs| {
        epochs.borrow_mut().remove(scope);
    });
    removed
}
