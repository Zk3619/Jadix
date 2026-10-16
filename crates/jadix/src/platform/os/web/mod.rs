//! Web / wasm32 host. Event loop uses `EventLoopExtWebSys::spawn_app`.
//! Fonts come from bundled assets (`system_font_dirs` is empty on the web).

mod clipboard;
mod gpu;
mod window;

use std::path::PathBuf;

use crate::platform::SystemFontScript;

#[cfg(any(debug_assertions, test))]
pub(crate) use crate::platform::desktop::enable_ansi_stderr;
pub(crate) use crate::platform::desktop::{
    apply_ime_cursor_area, cancel_active_composition, end_composition_ui, ime_disallow,
    process_private_mb, release_ui_control, sync_focus_owner, system_scroll_lines,
};
pub(crate) use clipboard::{clipboard_get_text, clipboard_set_text, edit_shortcut_modifiers};
pub(crate) use gpu::gpu_backend_order;
pub(crate) use window::{apply_created_window, decorate_window_attributes};

#[cfg(any(debug_assertions, test))]
pub(crate) fn local_hms() -> Option<(u16, u16, u16)> {
    None
}

pub(crate) fn system_font_dirs() -> Vec<PathBuf> {
    Vec::new()
}

pub(crate) fn eager_ui_font_files() -> &'static [&'static str] {
    // Bundled faces still register via `font_loader`; this name keeps the facade nonempty.
    &["Roboto-Regular.ttf"]
}

pub(crate) fn lazy_script_font_files(_script: SystemFontScript) -> &'static [&'static str] {
    &[]
}
