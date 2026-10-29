use super::*;

#[test]
fn macos_edit_shortcut_is_super_not_ctrl() {
    assert!(edit_shortcut_modifiers(&ModifiersState::META));
    assert!(!edit_shortcut_modifiers(&ModifiersState::CONTROL));
}
