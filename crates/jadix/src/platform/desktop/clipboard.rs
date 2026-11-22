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

#[cfg(feature = "native-clipboard")]
pub(crate) fn clipboard_get_html() -> Option<String> {
    ClipboardContext::new().ok()?.get_html().ok()
}

#[cfg(feature = "native-clipboard")]
pub(crate) fn clipboard_set_html(html: &str) {
    if let Ok(ctx) = ClipboardContext::new() {
        let _ = ctx.set_html(html.to_string());
    }
}

#[cfg(feature = "native-clipboard")]
pub(crate) fn clipboard_set_text_and_html(text: &str, html: Option<&str>) -> bool {
    use jadix_clipboard::ClipboardContent;
    let Ok(ctx) = ClipboardContext::new() else {
        return false;
    };
    let mut contents = vec![ClipboardContent::Text(text.to_string())];
    if let Some(html) = html {
        contents.push(ClipboardContent::Html(html.to_string()));
    }
    ctx.set(contents).is_ok()
}

#[cfg(not(feature = "native-clipboard"))]
pub(crate) fn clipboard_set_text_and_html(_text: &str, _html: Option<&str>) -> bool {
    false
}

#[cfg(not(feature = "native-clipboard"))]
pub(crate) fn clipboard_get_html() -> Option<String> {
    None
}

#[cfg(not(feature = "native-clipboard"))]
pub(crate) fn clipboard_set_html(_html: &str) {}

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
