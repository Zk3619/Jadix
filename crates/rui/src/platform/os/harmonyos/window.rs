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
mod tests {
    use super::*;
    use crate::platform::os::harmonyos::ability::{
        install_ohos_content_rect, reset_ohos_ability_for_tests, OhosContentRect,
    };

    #[test]
    fn apply_options_takes_ability_size_and_drops_chrome() {
        reset_ohos_ability_for_tests();
        install_ohos_content_rect(OhosContentRect::new(1080, 1920));
        let o = apply_window_options(WindowOptions::new());
        assert!(!o.decorations);
        assert_eq!(o.inner_width, Some(1080.0));
        assert_eq!(o.inner_height, Some(1920.0));
        reset_ohos_ability_for_tests();
    }
}
