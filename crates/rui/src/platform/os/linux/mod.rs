//! Linux overrides. Chrome currently matches desktop (winit `set_blur`);
//! process memory is `/proc` private RSS; wheel lines honor KDE when present.
//! IME uses winit cursor area only (no Imm).

pub(crate) use crate::platform::desktop::{
    apply_created_window, apply_ime_cursor_area, cancel_active_composition,
    decorate_window_attributes, end_composition_ui, ime_disallow, process_private_mb,
    release_ui_control, sync_focus_owner, system_scroll_lines,
};
