//! Ability-hosted window: no OS chrome; size from [`super::OhosContentRect`].

use winit::dpi::LogicalSize;
use winit::window::{Window, WindowAttributes};

use crate::app::WindowOptions;

use super::ohos_content_rect;

/// Fill unset `WindowOptions` size from the Ability `content_rect`.
///
/// No-op on desktop unless an Ability rect was installed (tests / `run_ohos`).
pub fn apply_window_options(mut options: WindowOptions) -> WindowOptions {
    let hosted = cfg!(target_env = "ohos") || super::ohos_ability_installed();
    if !hosted {
        return options;
    }
    let r = ohos_content_rect();
    if options.inner_width.is_none() && options.inner_height.is_none() && r.is_usable() {
        options.inner_width = Some(r.width as f64);
        options.inner_height = Some(r.height as f64);
    }
    options.decorations = false;
    options
}

#[allow(dead_code)]
pub(crate) fn decorate_window_attributes(
    mut attrs: WindowAttributes,
    options: &WindowOptions,
    composition_swapchain: bool,
) -> WindowAttributes {
    let _ = composition_swapchain;
    attrs = attrs.with_decorations(options.decorations);
    let r = ohos_content_rect();
    if options.inner_width.is_none() && r.is_usable() {
        attrs = attrs.with_surface_size(LogicalSize::new(r.width as f64, r.height as f64));
    }
    attrs
}

#[allow(dead_code)]
pub(crate) fn apply_created_window(
    window: &dyn Window,
    options: &WindowOptions,
    composition_swapchain: bool,
) {
    crate::platform::desktop::apply_created_window(window, options, composition_swapchain);
}

#[cfg(test)]
#[path = "tests/window.rs"]
mod tests;
