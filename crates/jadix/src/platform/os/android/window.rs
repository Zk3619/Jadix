//! Android: winit creates the native window on `resumed`. No extra attributes in 0.30.

use winit::window::{Window, WindowAttributes};

use crate::app::WindowOptions;

pub(crate) fn decorate_window_attributes(
    attrs: WindowAttributes,
    _options: &WindowOptions,
    _composition_swapchain: bool,
) -> WindowAttributes {
    attrs
}

pub(crate) fn apply_created_window(
    window: &dyn Window,
    options: &WindowOptions,
    composition_swapchain: bool,
) {
    crate::platform::desktop::apply_created_window(window, options, composition_swapchain);
}
