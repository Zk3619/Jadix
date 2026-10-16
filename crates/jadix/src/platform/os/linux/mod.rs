//! Linux overrides. Chrome currently matches desktop (winit `set_blur`);
//! process memory is `/proc` private RSS; wheel lines honor KDE when present.
//! IME uses winit cursor area only (no Imm). Font paths forward Unix desktop
//! tables (same `/usr/share/fonts` as *BSD — see `docs/engine/platform.md` §9.3).

pub(crate) use crate::platform::desktop::{
    apply_created_window, apply_ime_cursor_area, cancel_active_composition, clipboard_get_text,
    clipboard_set_text, decorate_window_attributes, eager_ui_font_files, edit_shortcut_modifiers,
    end_composition_ui, gpu_backend_order, ime_disallow, lazy_script_font_files,
    process_private_mb, release_ui_control, sync_focus_owner, system_font_dirs,
    system_scroll_lines,
};
#[cfg(any(debug_assertions, test))]
pub(crate) use crate::platform::desktop::{enable_ansi_stderr, local_hms};
