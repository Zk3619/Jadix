//! Item metadata registry: typed `data` bags, per-item flags, idle hit tables.
//!
//! Solid-dnd keeps droppables in context store; RUI normally walks the laid-out
//! tree ([`super::collect_hits`]). This module mirrors registered slots so
//! [`super::DragCtx::hits`] works while idle without holding the app root.

use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use super::session::DragHit;

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

thread_local! {
    static ITEM_DATA: RefCell<HashMap<String, Rc<dyn Any>>> = RefCell::new(HashMap::new());
    static ITEM_FLAGS: RefCell<HashMap<String, ItemFlags>> = RefCell::new(HashMap::new());
    static REGISTERED: RefCell<HashMap<String, DragHit>> = RefCell::new(HashMap::new());
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
pub fn register_slot(hit: DragHit) {
    let key = slot_key(&hit.scope_id, &hit.id);
    REGISTERED.with(|m| {
        m.borrow_mut().insert(key, hit);
    });
}

/// Merge a layout walk into the scope registry (idle + debugger HUD).
pub fn sync_registered_from_collect(hits: &[DragHit]) {
    for h in hits {
        register_slot(h.clone());
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::drag::geometry::Rect;

    #[test]
    fn registered_hits_filter_by_scope() {
        register_slot(DragHit {
            id: "a".into(),
            scope_id: "s1".into(),
            is_draggable: true,
            is_droppable: false,
            rect: Rect::default(),
            item_type: String::new(),
            accept: String::new(),
            list_id: None,
            index: None,
            group: String::new(),
            disabled: false,
            list_axis: None,
            skip_shift: false,
            skip_follow: false,
            pull_clone: false,
        });
        register_slot(DragHit {
            id: "b".into(),
            scope_id: "s2".into(),
            is_draggable: true,
            is_droppable: false,
            rect: Rect::default(),
            item_type: String::new(),
            accept: String::new(),
            list_id: None,
            index: None,
            group: String::new(),
            disabled: false,
            list_axis: None,
            skip_shift: false,
            skip_follow: false,
            pull_clone: false,
        });
        assert_eq!(registered_hits_in_scope("s1").len(), 1);
        assert_eq!(registered_hits_in_scope("s1")[0].id, "a");
    }
}
