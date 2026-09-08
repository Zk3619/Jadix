//! Linux overrides. Chrome currently matches desktop (winit `set_blur`);
//! process memory is `/proc` private RSS; wheel lines honor KDE when present.

pub(crate) use crate::platform::desktop::{
    apply_created_window, decorate_window_attributes, process_private_mb, system_scroll_lines,
};
