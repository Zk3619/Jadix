//! Desktop defaults used when an OS module has no override.
//!
//! Windows only needs [`system_scroll_lines`] as an SPI fallback; window/memory
//! stay compiled for macOS/Linux and non-desktop triples.
#![allow(dead_code)]

mod ime;
mod memory;
mod scroll;
mod window;

#[cfg(not(target_os = "windows"))]
pub(crate) use ime::{
    apply_ime_cursor_area, cancel_active_composition, end_composition_ui, ime_disallow,
    release_ui_control, sync_focus_owner,
};
#[cfg(not(target_os = "windows"))]
pub(crate) use memory::process_private_mb;
pub(crate) use scroll::system_scroll_lines;
#[cfg(not(target_os = "windows"))]
pub(crate) use window::{apply_created_window, decorate_window_attributes};
