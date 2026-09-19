//! Legacy Ability `run_loop`. The live path is `winit-ohos` via
//! [`crate::platform::install_ohos_app`] + [`crate::platform::build_event_loop`].

use crate::core::tree::Element;

#[allow(dead_code)]
pub fn run_ability_loop<F: FnMut() -> Element + 'static>(
    _window_options: crate::app::WindowOptions,
    _app_builder: F,
) {
    log::error!(
        "HarmonyOS now uses winit-ohos (rui::run_ohos → EventLoop + ApplicationHandler). \
         This Ability run_loop helper is unused."
    );
}
