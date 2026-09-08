//! Per-field undo / redo for Input and Textarea.
//!
//! User edits push a snapshot of `(value, caret, selection)`. Consecutive Insert or
//! Delete bursts within [`COALESCE`] merge into one undo step. Programmatic
//! `commit_value` paths should not call this (or should [`reset`] after).

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::core::reactivity::use_signal;
use crate::utils::text_field::{
    commit_value_sig, input_caret_signal_key, input_selection_signal_key, input_value_signal_key,
};

const COALESCE: Duration = Duration::from_millis(800);
const MAX_PAST: usize = 100;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextEditSnapshot {
    pub value: String,
    pub caret: usize,
    pub selection: Option<(usize, usize)>,
}

impl TextEditSnapshot {
    fn clamp(mut self) -> Self {
        let len = self.value.len();
        self.caret = self.caret.min(len);
        if let Some((a, b)) = self.selection {
            let lo = a.min(b).min(len);
            let hi = a.max(b).min(len);
            self.selection = if lo < hi { Some((lo, hi)) } else { None };
        }
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditKind {
    Insert,
    Delete,
    Replace,
    /// Number steppers / hold-repeat: consecutive steps merge.
    Step,
}

struct FieldHistory {
    past: Vec<TextEditSnapshot>,
    current: TextEditSnapshot,
    future: Vec<TextEditSnapshot>,
    last_kind: Option<EditKind>,
    last_at: Instant,
}

thread_local! {
    static HISTORIES: RefCell<HashMap<String, FieldHistory>> = RefCell::new(HashMap::new());
}

fn with_map<R>(f: impl FnOnce(&mut HashMap<String, FieldHistory>) -> R) -> R {
    HISTORIES.with(|h| f(&mut h.borrow_mut()))
}

pub fn capture_field(field_id: &str) -> TextEditSnapshot {
    let value = use_signal(&input_value_signal_key(field_id), String::new).peek();
    let caret = use_signal(&input_caret_signal_key(field_id), || None::<usize>)
        .peek()
        .unwrap_or(value.len())
        .min(value.len());
    let selection = use_signal(&input_selection_signal_key(field_id), || {
        None::<(usize, usize)>
    })
    .peek();
    TextEditSnapshot {
        value,
        caret,
        selection,
    }
    .clamp()
}

pub fn apply_snapshot(field_id: &str, snap: &TextEditSnapshot) {
    let snap = snap.clone().clamp();
    let store = use_signal(&input_value_signal_key(field_id), String::new);
    commit_value_sig(field_id, store, snap.value.clone());
    use_signal(&input_caret_signal_key(field_id), || None::<usize>).set(Some(snap.caret));
    use_signal(&input_selection_signal_key(field_id), || {
        None::<(usize, usize)>
    })
    .set(snap.selection);
}

pub fn will_user_edit_field(field_id: &str, kind: EditKind) {
    will_user_edit(field_id, capture_field(field_id), kind);
}

pub fn note_field_after_edit(field_id: &str) {
    note_after_edit(field_id, capture_field(field_id));
}

pub fn will_user_edit(field_id: &str, before: TextEditSnapshot, kind: EditKind) {
    let before = before.clamp();
    with_map(|map| {
        let st = map
            .entry(field_id.to_string())
            .or_insert_with(|| FieldHistory {
                past: Vec::new(),
                current: before.clone(),
                future: Vec::new(),
                last_kind: None,
                last_at: Instant::now(),
            });
        if st.current != before {
            st.current = before.clone();
        }
        let coalesce = st.last_kind == Some(kind)
            && matches!(kind, EditKind::Insert | EditKind::Delete | EditKind::Step)
            && st.last_at.elapsed() < COALESCE
            && st.future.is_empty();
        if coalesce {
            st.last_at = Instant::now();
            return;
        }
        st.past.push(st.current.clone());
        if st.past.len() > MAX_PAST {
            st.past.remove(0);
        }
        st.future.clear();
        st.last_kind = Some(kind);
        st.last_at = Instant::now();
    });
}

pub fn note_after_edit(field_id: &str, after: TextEditSnapshot) {
    let after = after.clamp();
    with_map(|map| {
        if let Some(st) = map.get_mut(field_id) {
            st.current = after;
        }
    });
}

pub fn undo(field_id: &str) -> Option<TextEditSnapshot> {
    with_map(|map| {
        let st = map.get_mut(field_id)?;
        let prev = st.past.pop()?;
        st.future.push(st.current.clone());
        st.current = prev.clone();
        st.last_kind = None;
        Some(prev)
    })
}

pub fn redo(field_id: &str) -> Option<TextEditSnapshot> {
    with_map(|map| {
        let st = map.get_mut(field_id)?;
        let next = st.future.pop()?;
        st.past.push(st.current.clone());
        st.current = next.clone();
        st.last_kind = None;
        Some(next)
    })
}

pub fn undo_field(field_id: &str) -> bool {
    let Some(snap) = undo(field_id) else {
        return false;
    };
    apply_snapshot(field_id, &snap);
    true
}

pub fn redo_field(field_id: &str) -> bool {
    let Some(snap) = redo(field_id) else {
        return false;
    };
    apply_snapshot(field_id, &snap);
    true
}

/// Drop the field's undo stack (programmatic `commit_value`, unmount).
pub fn reset(field_id: &str) {
    with_map(|map| {
        map.remove(field_id);
    });
}

#[cfg(test)]
pub fn clear_all_for_tests() {
    with_map(|map| map.clear());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(value: &str, caret: usize) -> TextEditSnapshot {
        TextEditSnapshot {
            value: value.into(),
            caret,
            selection: None,
        }
    }

    #[test]
    fn insert_coalesces_then_undo_restores_start() {
        clear_all_for_tests();
        let id = "h1";
        will_user_edit(id, snap("", 0), EditKind::Insert);
        note_after_edit(id, snap("a", 1));
        will_user_edit(id, snap("a", 1), EditKind::Insert);
        note_after_edit(id, snap("ab", 2));
        will_user_edit(id, snap("ab", 2), EditKind::Insert);
        note_after_edit(id, snap("abc", 3));
        let undone = undo(id).expect("undo");
        assert_eq!(undone.value, "");
        assert_eq!(undo(id), None);
        let redone = redo(id).expect("redo");
        assert_eq!(redone.value, "abc");
    }

    #[test]
    fn replace_does_not_coalesce_with_insert() {
        clear_all_for_tests();
        let id = "h2";
        will_user_edit(id, snap("", 0), EditKind::Insert);
        note_after_edit(id, snap("a", 1));
        will_user_edit(id, snap("a", 1), EditKind::Replace);
        note_after_edit(id, snap("hello", 5));
        assert_eq!(undo(id).unwrap().value, "a");
        assert_eq!(undo(id).unwrap().value, "");
    }

    #[test]
    fn delete_coalesces() {
        clear_all_for_tests();
        let id = "h3";
        will_user_edit(id, snap("ab", 2), EditKind::Delete);
        note_after_edit(id, snap("a", 1));
        will_user_edit(id, snap("a", 1), EditKind::Delete);
        note_after_edit(id, snap("", 0));
        assert_eq!(undo(id).unwrap().value, "ab");
        assert_eq!(undo(id), None);
    }

    #[test]
    fn redo_cleared_by_new_edit() {
        clear_all_for_tests();
        let id = "h4";
        will_user_edit(id, snap("", 0), EditKind::Insert);
        note_after_edit(id, snap("a", 1));
        let _ = undo(id);
        will_user_edit(id, snap("", 0), EditKind::Insert);
        note_after_edit(id, snap("b", 1));
        assert_eq!(redo(id), None);
        assert_eq!(undo(id).unwrap().value, "");
    }

    #[test]
    fn step_coalesces() {
        clear_all_for_tests();
        let id = "h5";
        will_user_edit(id, snap("1", 1), EditKind::Step);
        note_after_edit(id, snap("2", 1));
        will_user_edit(id, snap("2", 1), EditKind::Step);
        note_after_edit(id, snap("3", 1));
        assert_eq!(undo(id).unwrap().value, "1");
        assert_eq!(undo(id), None);
    }

    #[test]
    fn undo_field_restores_value_and_caret() {
        clear_all_for_tests();
        let (_owner, ()) = crate::core::reactivity::create_root(|| {
            let id = "h6";
            use_signal(&input_value_signal_key(id), String::new).set("ab".into());
            use_signal(&input_caret_signal_key(id), || None::<usize>).set(Some(2));
            will_user_edit_field(id, EditKind::Insert);
            use_signal(&input_value_signal_key(id), String::new).set("abc".into());
            use_signal(&input_caret_signal_key(id), || None::<usize>).set(Some(3));
            note_field_after_edit(id);
            assert!(undo_field(id));
            assert_eq!(
                use_signal(&input_value_signal_key(id), String::new).peek(),
                "ab"
            );
            assert_eq!(
                use_signal(&input_caret_signal_key(id), || None::<usize>).peek(),
                Some(2)
            );
            assert!(redo_field(id));
            assert_eq!(
                use_signal(&input_value_signal_key(id), String::new).peek(),
                "abc"
            );
        });
    }
}
