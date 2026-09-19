//! wasm clipboard: no `jadix_clipboard` backend. Async `navigator.clipboard` is later.

use winit::keyboard::ModifiersState;

pub(crate) fn clipboard_set_text(_text: &str) {}

pub(crate) fn clipboard_get_text() -> Option<String> {
    None
}

pub(crate) fn edit_shortcut_modifiers(m: &ModifiersState) -> bool {
    m.control_key()
}
