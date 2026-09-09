//! Desktop-default clipboard: `clipboard-rs` text I/O + Ctrl as the edit modifier.
//!
//! macOS overrides the modifier (Cmd / Super). Do not swap crates here
//! (`docs/engine/platform.md` §9.4).

use clipboard_rs::{Clipboard, ClipboardContext};
use winit::keyboard::ModifiersState;

pub(crate) fn clipboard_set_text(text: &str) {
    if let Ok(ctx) = ClipboardContext::new() {
        let _ = ctx.set_text(text.to_string());
    }
}

pub(crate) fn clipboard_get_text() -> Option<String> {
    let ctx = ClipboardContext::new().ok()?;
    ctx.get_text().ok()
}

pub(crate) fn edit_shortcut_modifiers(m: &ModifiersState) -> bool {
    m.control_key()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_edit_shortcut_is_ctrl_not_super() {
        assert!(edit_shortcut_modifiers(&ModifiersState::CONTROL));
        assert!(!edit_shortcut_modifiers(&ModifiersState::SUPER));
        assert!(!edit_shortcut_modifiers(&ModifiersState::empty()));
    }
}
