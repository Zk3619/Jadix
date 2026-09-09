//! macOS overrides. Chrome currently matches desktop (winit `set_blur`);
//! process memory is Mach `resident_size` via [`crate::platform::desktop`].
//! IME uses winit cursor area only (no Imm). Font paths are macOS-specific.

mod clipboard;
mod font_paths;
mod gpu;

pub(crate) use crate::platform::desktop::{
    apply_created_window, apply_ime_cursor_area, cancel_active_composition,
    decorate_window_attributes, enable_ansi_stderr, end_composition_ui, ime_disallow, local_hms,
    process_private_mb, release_ui_control, sync_focus_owner, system_scroll_lines,
};
pub(crate) use clipboard::{clipboard_get_text, clipboard_set_text, edit_shortcut_modifiers};
pub(crate) use font_paths::{eager_ui_font_files, lazy_script_font_files, system_font_dirs};
pub(crate) use gpu::gpu_backend_order;
