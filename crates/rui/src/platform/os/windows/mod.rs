//! Windows-specific window chrome, IME, process metrics, and PE `/STACK` opt-in.

mod ime;
mod process;
mod scroll;
mod stack;
mod window;

pub(crate) use ime::{
    apply_ime_cursor_area, cancel_active_composition, end_composition_ui, ime_disallow,
    release_ui_control, sync_focus_owner,
};
pub(crate) use process::process_private_mb;
pub(crate) use scroll::system_scroll_lines;
pub use stack::{gnu_link_arg, msvc_link_arg, EXE_STACK_RESERVE};
pub(crate) use window::{apply_created_window, decorate_window_attributes};
