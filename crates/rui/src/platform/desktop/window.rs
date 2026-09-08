//! Desktop-default window chrome: passthrough attributes, optional winit blur.

use winit::window::{Window, WindowAttributes};

use crate::app::WindowOptions;

pub(crate) fn decorate_window_attributes(
    attrs: WindowAttributes,
    options: &WindowOptions,
    composition_swapchain: bool,
) -> WindowAttributes {
    let _ = (options, composition_swapchain);
    attrs
}

pub(crate) fn apply_created_window(
    window: &Window,
    options: &WindowOptions,
    composition_swapchain: bool,
) {
    let _ = composition_swapchain;
    if options.blur {
        window.set_blur(true);
    }
}
