//! Concurrent pointers + interact `max` / `maxPerElement`.
//!
//! interact allows several `Interaction`s at once, capped per action and per
//! element. Jadix keeps the same two knobs and adds an explicit [`ActionKind`] so
//! a second finger on the **same** target prefers [`ActionKind::Gesture`] over
//! a second drag (interact auto-start). Two fingers on **different** targets
//! may both drag when `max` allows it.

use std::collections::HashMap;

use super::gesture::{GestureSession, GestureState};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ActionKind {
    Drag,
    Resize,
    Gesture,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BeginError {
    /// Global `max` already reached.
    MaxReached,
    /// This element already has `max_per_element` live actions.
    MaxPerElement,
}

#[derive(Clone, Copy, Debug)]
pub struct Quota {
    /// interact `max` (Infinity → `usize::MAX`).
    pub max: usize,
    /// interact `maxPerElement` (default 1).
    pub max_per_element: usize,
}

impl Default for Quota {
    fn default() -> Self {
        Self {
            max: usize::MAX,
            max_per_element: 1,
        }
    }
}

impl Quota {
    pub fn max(mut self, n: usize) -> Self {
        self.max = n.max(1);
        self
    }
    pub fn max_per_element(mut self, n: usize) -> Self {
        self.max_per_element = n.max(1);
        self
    }
}

#[derive(Clone, Debug)]
struct Live {
    pointer_ids: Vec<u64>,
    element_id: String,
    kind: ActionKind,
}

/// Pointer positions (all fingers) plus live action table.
#[derive(Clone, Debug, Default)]
pub struct PointerSet {
    points: HashMap<u64, [f32; 2]>,
    live: Vec<Live>,
    quota: Quota,
}

impl PointerSet {
    pub fn new(quota: Quota) -> Self {
        Self {
            quota,
            ..Self::default()
        }
    }

    pub fn quota(&self) -> Quota {
        self.quota
    }

    pub fn set_quota(&mut self, quota: Quota) {
        self.quota = quota;
    }

    pub fn upsert(&mut self, pointer_id: u64, pos: [f32; 2]) {
        self.points.insert(pointer_id, pos);
    }

    pub fn remove(&mut self, pointer_id: u64) {
        self.points.remove(&pointer_id);
        for l in &mut self.live {
            l.pointer_ids.retain(|id| *id != pointer_id);
        }
        self.live.retain(|l| {
            if l.kind == ActionKind::Gesture {
                l.pointer_ids.len() >= 2
            } else {
                !l.pointer_ids.is_empty()
            }
        });
    }

    pub fn get(&self, pointer_id: u64) -> Option<[f32; 2]> {
        self.points.get(&pointer_id).copied()
    }

    pub fn len(&self) -> usize {
        self.points.len()
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    pub fn positions(&self) -> Vec<(u64, [f32; 2])> {
        let mut v: Vec<_> = self.points.iter().map(|(k, p)| (*k, *p)).collect();
        v.sort_by_key(|(id, _)| *id);
        v
    }

    pub fn action_count(&self) -> usize {
        self.live.len()
    }

    pub fn count_on(&self, element_id: &str) -> usize {
        self.live
            .iter()
            .filter(|l| l.element_id == element_id)
            .count()
    }

    /// Two or more live pointers (pinch candidate).
    pub fn pair(&self) -> Option<[[f32; 2]; 2]> {
        let mut it = self.positions().into_iter();
        let a = it.next()?.1;
        let b = it.next()?.1;
        Some([a, b])
    }

    pub fn try_begin(
        &mut self,
        pointer_id: u64,
        element_id: impl Into<String>,
        kind: ActionKind,
    ) -> Result<(), BeginError> {
        if self.live.len() >= self.quota.max {
            return Err(BeginError::MaxReached);
        }
        let element_id = element_id.into();
        if self.count_on(&element_id) >= self.quota.max_per_element && kind != ActionKind::Gesture {
            return Err(BeginError::MaxPerElement);
        }
        if kind == ActionKind::Gesture {
            if self.count_on(&element_id) >= self.quota.max_per_element {
                // Gesture replaces a same-element drag when only one slot is allowed.
                self.live.retain(|l| l.element_id != element_id);
            }
            let ids: Vec<u64> = self.points.keys().copied().take(2).collect();
            if ids.len() < 2 {
                return Err(BeginError::MaxPerElement);
            }
            self.live.push(Live {
                pointer_ids: ids,
                element_id,
                kind,
            });
            return Ok(());
        }
        self.live.push(Live {
            pointer_ids: vec![pointer_id],
            element_id,
            kind,
        });
        Ok(())
    }

    pub fn end_pointer(&mut self, pointer_id: u64) {
        self.live.retain(|l| !l.pointer_ids.contains(&pointer_id));
    }

    pub fn end_element(&mut self, element_id: &str) {
        self.live.retain(|l| l.element_id != element_id);
    }

    pub fn clear(&mut self) {
        self.points.clear();
        self.live.clear();
    }
}

thread_local! {
    static GLOBAL: std::cell::RefCell<PointerSet> = std::cell::RefCell::new(PointerSet::default());
    static GESTURE: std::cell::RefCell<Option<GestureSession>> = const { std::cell::RefCell::new(None) };
    static LAST_GESTURE: std::cell::RefCell<Option<GestureState>> = const { std::cell::RefCell::new(None) };
}

/// Replace the live touch map and keep a two-finger [`GestureSession`] in sync.
///
/// Call from the window Touch handler with **logical** pixels. When fewer than
/// two fingers remain, the gesture ends (last state frozen).
pub fn sync_touches(points: impl IntoIterator<Item = (u64, [f32; 2])>) {
    GLOBAL.with(|g| {
        let mut set = g.borrow_mut();
        set.points.clear();
        for (id, pos) in points {
            set.points.insert(id, pos);
        }
        let pair = set.pair();
        drop(set);
        match pair {
            Some([a, b]) => {
                GESTURE.with(|slot| {
                    let mut g = slot.borrow_mut();
                    let state = if let Some(sess) = g.as_mut() {
                        sess.tick(a, b)
                    } else {
                        let mut sess = GestureSession::begin(a, b);
                        let state = sess.tick(a, b);
                        *g = Some(sess);
                        state
                    };
                    LAST_GESTURE.with(|s| *s.borrow_mut() = Some(state));
                });
            }
            None => {
                GESTURE.with(|slot| {
                    if let Some(sess) = slot.borrow_mut().take() {
                        LAST_GESTURE.with(|s| *s.borrow_mut() = Some(sess.end()));
                    }
                });
            }
        }
    });
}

pub fn with_pointers<R>(f: impl FnOnce(&PointerSet) -> R) -> R {
    GLOBAL.with(|g| f(&g.borrow()))
}

pub fn with_pointers_mut<R>(f: impl FnOnce(&mut PointerSet) -> R) -> R {
    GLOBAL.with(|g| f(&mut g.borrow_mut()))
}

pub fn last_gesture() -> Option<GestureState> {
    LAST_GESTURE.with(|s| *s.borrow())
}

pub fn gesture_is_active() -> bool {
    GESTURE.with(|s| s.borrow().is_some())
}

pub fn reset_global() {
    GLOBAL.with(|g| g.borrow_mut().clear());
    GESTURE.with(|s| *s.borrow_mut() = None);
    LAST_GESTURE.with(|s| *s.borrow_mut() = None);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_per_element_blocks_second_drag() {
        let mut set = PointerSet::new(Quota::default().max_per_element(1));
        set.upsert(1, [0.0, 0.0]);
        set.upsert(2, [10.0, 0.0]);
        assert!(set.try_begin(1, "card", ActionKind::Drag).is_ok());
        assert_eq!(
            set.try_begin(2, "card", ActionKind::Drag),
            Err(BeginError::MaxPerElement)
        );
        assert!(set.try_begin(2, "other", ActionKind::Drag).is_ok());
    }

    #[test]
    fn gesture_replaces_same_element_drag() {
        let mut set = PointerSet::new(Quota::default().max_per_element(1));
        set.upsert(1, [0.0, 0.0]);
        set.upsert(2, [10.0, 0.0]);
        assert!(set.try_begin(1, "card", ActionKind::Drag).is_ok());
        assert!(set.try_begin(1, "card", ActionKind::Gesture).is_ok());
        assert_eq!(set.count_on("card"), 1);
        assert_eq!(set.action_count(), 1);
    }

    #[test]
    fn global_max() {
        let mut set = PointerSet::new(Quota::default().max(1));
        set.upsert(1, [0.0, 0.0]);
        set.upsert(2, [1.0, 0.0]);
        assert!(set.try_begin(1, "a", ActionKind::Drag).is_ok());
        assert_eq!(
            set.try_begin(2, "b", ActionKind::Drag),
            Err(BeginError::MaxReached)
        );
    }

    #[test]
    fn two_elements_concurrent_when_max_allows() {
        let mut set = PointerSet::new(Quota::default().max(8).max_per_element(1));
        set.upsert(1, [0.0, 0.0]);
        set.upsert(2, [40.0, 0.0]);
        assert!(set.try_begin(1, "a", ActionKind::Drag).is_ok());
        assert!(set.try_begin(2, "b", ActionKind::Drag).is_ok());
        assert_eq!(set.action_count(), 2);
    }

    #[test]
    fn sync_touches_drives_gesture() {
        reset_global();
        sync_touches([(1, [0.0, 0.0]), (2, [10.0, 0.0])]);
        assert!(gesture_is_active());
        sync_touches([(1, [0.0, 0.0]), (2, [20.0, 0.0])]);
        let g = last_gesture().unwrap();
        assert!((g.scale - 2.0).abs() < 1e-3);
        sync_touches([(1, [0.0, 0.0])]);
        assert!(!gesture_is_active());
        reset_global();
    }
}
