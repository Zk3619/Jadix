//! Linux window chrome (kwin / protocol blur via winit `set_blur`).

use winit::window::Window;

use crate::app::WindowOptions;

pub(crate) fn apply_created_window(
    window: &Window,
    options: &WindowOptions,
    _composition_swapchain: bool,
) {
    if options.blur {
        window.set_blur(true);
    }
}
