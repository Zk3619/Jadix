//! macOS edit-shortcut modifier is Cmd (Super). Text I/O shares desktop `jadix_clipboard`.

use winit::keyboard::ModifiersState;

pub(crate) use crate::platform::desktop::{clipboard_get_text, clipboard_set_text};

pub(crate) fn edit_shortcut_modifiers(m: &ModifiersState) -> bool {
    m.meta_key()
}

#[cfg(test)]
#[path = "tests/clipboard.rs"]
mod tests;
