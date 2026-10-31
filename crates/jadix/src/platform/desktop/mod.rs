//! Desktop defaults used when an OS module has no override.
//!
//! Windows only needs [`system_scroll_lines`] as an SPI fallback; window/memory
//! stay compiled for macOS/Linux and non-desktop triples.
#![allow(dead_code)]

mod clipboard;
#[cfg(any(debug_assertions, test))]
mod console;
mod font_paths;
mod gpu;
mod ime;
mod memory;
mod scroll;
mod window;

pub(crate) use clipboard::{clipboard_get_text, clipboard_set_text, edit_shortcut_modifiers};
#[cfg_attr(target_os = "windows", allow(unused_imports))]
#[cfg(any(debug_assertions, test))]
pub(crate) use console::{enable_ansi_stderr, local_hms};
#[cfg_attr(any(target_os = "windows", target_os = "macos"), allow(unused_imports))]
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use font_paths::{eager_ui_font_files, lazy_script_font_files, system_font_dirs};
#[cfg_attr(target_os = "macos", allow(unused_imports))]
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use gpu::gpu_backend_order;
pub(crate) use gpu::with_requested_backend_front;
#[cfg_attr(target_os = "windows", allow(unused_imports))]
pub(crate) use ime::{
    apply_ime_cursor_area, cancel_active_composition, end_composition_ui, ime_disallow,
    release_ui_control, sync_focus_owner,
};
#[cfg_attr(target_os = "windows", allow(unused_imports))]
pub(crate) use memory::process_private_mb;
pub(crate) use scroll::system_scroll_lines;
pub(crate) use window::apply_created_window;
#[cfg_attr(
    any(
        target_os = "windows",
        target_os = "ios",
        target_os = "android",
        target_arch = "wasm32",
        target_env = "ohos",
    ),
    allow(unused_imports)
)]
pub(crate) use window::decorate_window_attributes;
