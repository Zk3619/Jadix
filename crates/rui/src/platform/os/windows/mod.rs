//! Windows-specific window chrome, IME, process metrics, PE `/STACK` opt-in, fonts.

mod console;
mod font_paths;
mod ime;
mod process;
mod scroll;
mod stack;
mod window;

pub(crate) use crate::platform::desktop::{
    clipboard_get_text, clipboard_set_text, edit_shortcut_modifiers, gpu_backend_order,
};
pub(crate) use console::{enable_ansi_stderr, local_hms};
pub(crate) use font_paths::{eager_ui_font_files, lazy_script_font_files, system_font_dirs};
pub(crate) use ime::{
    apply_ime_cursor_area, cancel_active_composition, end_composition_ui, ime_disallow,
    release_ui_control, sync_focus_owner,
};
pub(crate) use process::process_private_mb;
pub(crate) use scroll::system_scroll_lines;
pub use stack::{gnu_link_arg, msvc_link_arg, EXE_STACK_RESERVE};
pub(crate) use window::{apply_created_window, decorate_window_attributes};
