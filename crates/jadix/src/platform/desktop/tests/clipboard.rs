use super::*;

#[cfg(not(feature = "native-clipboard"))]
#[test]
fn disabled_clipboard_does_not_access_the_system() {
    clipboard_set_text("not sent to the system clipboard");
    assert_eq!(clipboard_get_text(), None);
}

#[test]
fn desktop_edit_shortcut_is_ctrl_not_super() {
    assert!(edit_shortcut_modifiers(&ModifiersState::CONTROL));
    assert!(!edit_shortcut_modifiers(&ModifiersState::META));
    assert!(!edit_shortcut_modifiers(&ModifiersState::empty()));
}
