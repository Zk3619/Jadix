//! UIKit window extras (orientations). Chrome / blur stay on desktop defaults.

use winit::window::{Window, WindowAttributes};

use crate::app::{ScreenOrientations, WindowOptions};

pub(crate) fn decorate_window_attributes(
    attrs: WindowAttributes,
    options: &WindowOptions,
    _composition_swapchain: bool,
) -> WindowAttributes {
    use winit::platform::ios::WindowAttributesIos;
    attrs.with_platform_attributes(Box::new(
        WindowAttributesIos::default()
            .with_valid_orientations(ios_valid_orientations(options.screen_orientations)),
    ))
}

fn ios_valid_orientations(
    orientations: ScreenOrientations,
) -> winit::platform::ios::ValidOrientations {
    use winit::platform::ios::ValidOrientations;
    match orientations {
        ScreenOrientations::LandscapeAndPortrait => ValidOrientations::LandscapeAndPortrait,
        ScreenOrientations::Landscape => ValidOrientations::Landscape,
        ScreenOrientations::Portrait => ValidOrientations::Portrait,
    }
}

pub(crate) fn apply_created_window(
    window: &dyn Window,
    options: &WindowOptions,
    composition_swapchain: bool,
) {
    use winit::platform::ios::WindowExtIOS;
    window.set_valid_orientations(ios_valid_orientations(options.screen_orientations));
    crate::platform::desktop::apply_created_window(window, options, composition_swapchain);
}

#[cfg(test)]
#[path = "tests/window.rs"]
mod tests;
