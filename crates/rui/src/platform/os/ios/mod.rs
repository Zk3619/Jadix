//! iOS / UIKit overrides. Event loop is still upstream winit (`resumed` creates the window).
//! IME / clipboard / clock forward desktop (`jadix_clipboard` already has a UIKit backend).

mod font_paths;
mod gpu;
mod window;

use winit::keyboard::ModifiersState;

pub(crate) use crate::platform::desktop::{
    apply_ime_cursor_area, cancel_active_composition, clipboard_get_text, clipboard_set_text,
    enable_ansi_stderr, end_composition_ui, ime_disallow, local_hms, process_private_mb,
    release_ui_control, sync_focus_owner, system_scroll_lines,
};

/// Hardware-keyboard edit shortcut is Cmd (same as macOS).
pub(crate) fn edit_shortcut_modifiers(m: &ModifiersState) -> bool {
    m.super_key()
}
pub(crate) use font_paths::{eager_ui_font_files, lazy_script_font_files, system_font_dirs};
pub(crate) use gpu::gpu_backend_order;
pub(crate) use window::{apply_created_window, decorate_window_attributes};
