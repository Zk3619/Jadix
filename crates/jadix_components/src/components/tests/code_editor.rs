use super::*;
use crate::utils::document_registry::DocumentRegistry;
use crate::utils::text_edit_history::{
    note_after_edit, undo, undo_slot_exists, will_user_edit, EditKind, TextEditSnapshot,
};

fn snap(value: &str, caret: usize) -> TextEditSnapshot {
    TextEditSnapshot {
        value: value.into(),
        caret,
        ..Default::default()
    }
}

#[test]
fn unmount_keeps_history_until_unregister() {
    let reg = DocumentRegistry::new();
    let uri = "file:///tab.rs";
    let field_id = document_field_id(uri);
    let el: Element = code_editor(uri).registry(reg.clone()).initial("x").into();
    will_user_edit(&field_id, snap("x", 1), EditKind::Insert);
    note_after_edit(&field_id, snap("xy", 2));
    drop(el);
    assert!(
        undo_slot_exists(&field_id),
        "tab unmount must not drop L3 history"
    );
    assert_eq!(undo(&field_id).unwrap().value, "x");
    assert!(reg.unregister(uri));
    assert!(!undo_slot_exists(&field_id));
}
