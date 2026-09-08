//! macOS overrides. Chrome currently matches desktop (winit `set_blur`);
//! process memory is Mach `resident_size` via [`crate::platform::desktop`].

pub(crate) use crate::platform::desktop::{
    apply_created_window, decorate_window_attributes, process_private_mb, system_scroll_lines,
};
