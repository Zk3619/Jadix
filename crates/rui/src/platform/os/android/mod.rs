//! Android overrides. Event loop needs `install_android_app` (winit 0.30).
//! Clipboard forwards desktop (`jadix_clipboard` already has an Android backend).

mod font_paths;
mod gpu;
mod window;

pub(crate) use crate::platform::desktop::{
    apply_ime_cursor_area, cancel_active_composition, clipboard_get_text, clipboard_set_text,
    edit_shortcut_modifiers, enable_ansi_stderr, end_composition_ui, ime_disallow, local_hms,
    process_private_mb, release_ui_control, sync_focus_owner, system_scroll_lines,
};
pub(crate) use font_paths::{eager_ui_font_files, lazy_script_font_files, system_font_dirs};
pub(crate) use gpu::gpu_backend_order;
pub(crate) use window::{apply_created_window, decorate_window_attributes};
