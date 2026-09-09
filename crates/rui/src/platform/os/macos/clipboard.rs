//! macOS edit-shortcut modifier is Cmd (Super). Text I/O shares desktop `clipboard-rs`.

use winit::keyboard::ModifiersState;

pub(crate) use crate::platform::desktop::{clipboard_get_text, clipboard_set_text};

pub(crate) fn edit_shortcut_modifiers(m: &ModifiersState) -> bool {
    m.super_key()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_edit_shortcut_is_super_not_ctrl() {
        assert!(edit_shortcut_modifiers(&ModifiersState::SUPER));
        assert!(!edit_shortcut_modifiers(&ModifiersState::CONTROL));
    }
}
