//! UIKit window extras (orientations). Chrome / blur stay on desktop defaults.

use winit::window::{Window, WindowAttributes};

use crate::app::WindowOptions;

pub(crate) fn decorate_window_attributes(
    attrs: WindowAttributes,
    _options: &WindowOptions,
    _composition_swapchain: bool,
) -> WindowAttributes {
    use winit::platform::ios::{ValidOrientations, WindowAttributesExtIOS};
    attrs.with_valid_orientations(ValidOrientations::LandscapeAndPortrait)
}

pub(crate) fn apply_created_window(
    window: &dyn Window,
    options: &WindowOptions,
    composition_swapchain: bool,
) {
    crate::platform::desktop::apply_created_window(window, options, composition_swapchain);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decorate_keeps_title() {
        let o = WindowOptions::new().title("iOS");
        let attrs = decorate_window_attributes(o.to_window_attributes(), &o, false);
        assert_eq!(attrs.title, "iOS");
    }
}
