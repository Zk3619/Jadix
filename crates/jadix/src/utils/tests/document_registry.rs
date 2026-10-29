use super::*;

#[cfg(feature = "text-undo")]
use crate::utils::text_edit_history::{undo, undo_slot_exists};

#[test]
fn empty_uri_is_rejected() {
    let reg = DocumentRegistry::new();
    assert!(reg.register("", None).is_none());
    assert!(reg.is_empty());
}

#[cfg(feature = "text-undo")]
fn snap(value: &str, caret: usize) -> crate::utils::text_edit_history::TextEditSnapshot {
    crate::utils::text_edit_history::TextEditSnapshot {
        value: value.into(),
        caret,
        ..Default::default()
    }
}

#[cfg(feature = "text-undo")]
fn edit_insert(field_id: &str, before: &str, after: &str) {
    use crate::utils::text_edit_history::{note_after_edit, will_user_edit, EditKind};
    will_user_edit(field_id, snap(before, before.len()), EditKind::Insert);
    note_after_edit(field_id, snap(after, after.len()));
}

#[test]
#[cfg(feature = "text-undo")]
fn two_files_undo_only_hits_current() {
    let reg = DocumentRegistry::new();
    let a = reg.register("file:///a.rs", Some("a".into())).expect("a");
    let b = reg.register("file:///b.rs", Some("b".into())).expect("b");
    edit_insert(a.field_id(), "a", "aa");
    edit_insert(b.field_id(), "b", "bb");
    assert_eq!(undo(a.field_id()).unwrap().value, "a");
    assert!(b.can_undo());
    assert_eq!(undo(b.field_id()).unwrap().value, "b");
    reg.unregister(a.uri());
    reg.unregister(b.uri());
}

#[test]
#[cfg(feature = "text-undo")]
fn close_b_does_not_affect_a() {
    let reg = DocumentRegistry::new();
    let a = reg
        .register("file:///keep.rs", Some("keep".into()))
        .expect("a");
    let b = reg
        .register("file:///gone.rs", Some("gone".into()))
        .expect("b");
    edit_insert(a.field_id(), "keep", "keep!");
    edit_insert(b.field_id(), "gone", "gone!");
    assert!(reg.unregister(b.uri()));
    assert!(!reg.contains("file:///gone.rs"));
    assert!(!undo_slot_exists(b.field_id()));
    assert!(reg.contains("file:///keep.rs"));
    assert_eq!(undo(a.field_id()).unwrap().value, "keep");
    reg.unregister(a.uri());
}

#[test]
#[cfg(feature = "text-undo")]
fn save_does_not_clear_undo() {
    let reg = DocumentRegistry::new();
    let a = reg
        .register("file:///saved.rs", Some("x".into()))
        .expect("a");
    assert!(!a.is_dirty());
    edit_insert(a.field_id(), "x", "xy");
    crate::utils::text_field::commit_value(a.field_id(), "xy".into());
    assert!(a.is_dirty());
    assert!(a.save());
    assert!(!a.is_dirty());
    assert!(a.can_undo());
    assert_eq!(undo(a.field_id()).unwrap().value, "x");
    reg.unregister(a.uri());
}

#[test]
#[cfg(feature = "text-undo")]
fn unregister_releases_undo_slot() {
    let reg = DocumentRegistry::new();
    let a = reg.register("file:///mem.rs", Some("m".into())).expect("a");
    edit_insert(a.field_id(), "m", "mm");
    assert!(undo_slot_exists(a.field_id()));
    assert!(reg.unregister(a.uri()));
    assert!(!undo_slot_exists(a.field_id()));
    assert_eq!(reg.len(), 0);
}

#[test]
#[cfg(not(feature = "text-undo"))]
fn register_tracks_uris_without_history() {
    let reg = DocumentRegistry::new();
    let a = reg
        .register("file:///off.rs", Some("hi".into()))
        .expect("a");
    assert!(reg.contains("file:///off.rs"));
    assert!(!a.can_undo());
    assert!(!a.undo());
    assert!(reg.unregister(a.uri()));
    assert!(reg.is_empty());
}
