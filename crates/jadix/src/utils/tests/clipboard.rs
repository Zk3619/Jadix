use super::*;

fn host_edit_mods() -> ModifiersState {
    if cfg!(target_os = "macos") {
        ModifiersState::META
    } else {
        ModifiersState::CONTROL
    }
}

#[test]
fn ctrl_or_cmd_z_y_are_consumed_not_typed() {
    let mods = host_edit_mods();
    assert!(consumes_undo_redo_letter(Some('z'), &mods));
    assert!(consumes_undo_redo_letter(Some('y'), &mods));
    assert!(consumes_undo_redo_letter(
        Some('z'),
        &(mods | ModifiersState::SHIFT)
    ));
    assert!(!consumes_undo_redo_letter(
        Some('z'),
        &ModifiersState::empty()
    ));
    assert!(!consumes_undo_redo_letter(Some('a'), &mods));
    assert!(!consumes_undo_redo_letter(
        Some('y'),
        &(mods | ModifiersState::SHIFT)
    ));
}
