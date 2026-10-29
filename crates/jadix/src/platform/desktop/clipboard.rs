//! Desktop-default clipboard: `jadix_clipboard` text I/O + Ctrl as the edit modifier.
//!
//! macOS overrides the modifier (Cmd / Super).

#[cfg(feature = "native-clipboard")]
use jadix_clipboard::{Clipboard, ClipboardContext};
use winit::keyboard::ModifiersState;

#[cfg(feature = "native-clipboard")]
pub(crate) fn clipboard_set_text(text: &str) {
    if let Ok(ctx) = ClipboardContext::new() {
        let _ = ctx.set_text(text.to_string());
    }
}

#[cfg(feature = "native-clipboard")]
pub(crate) fn clipboard_get_text() -> Option<String> {
    let ctx = ClipboardContext::new().ok()?;
    ctx.get_text().ok()
}

#[cfg(not(feature = "native-clipboard"))]
pub(crate) fn clipboard_set_text(_text: &str) {}

#[cfg(not(feature = "native-clipboard"))]
pub(crate) fn clipboard_get_text() -> Option<String> {
    None
}

pub(crate) fn edit_shortcut_modifiers(m: &ModifiersState) -> bool {
    m.control_key()
}

#[cfg(test)]
#[path = "tests/clipboard.rs"]
mod tests;
