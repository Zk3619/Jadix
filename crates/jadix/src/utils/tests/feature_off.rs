use super::*;

#[test]
fn history_apis_are_nops_without_feature() {
    will_user_edit_field("off", EditKind::Insert);
    note_field_after_edit("off");
    assert!(!undo_field("off"));
    assert!(!redo_field("off"));
    reset("off");
    install_field_config("off", TextUndoConfig::input_default());
    drop(edit_group("off", EditKind::Replace));
}
