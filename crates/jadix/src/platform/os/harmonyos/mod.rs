//! HarmonyOS / OpenHarmony host.
//!
//! Contract copied from `reference/gpui-template-main`:
//! - Rust: `#[ability] fn openharmony_app(app: OpenHarmonyApp)` → [`crate::app::run_ohos`]
//! - ArkTS: `EntryAbility extends NativeAbility` (`@ohos-rs/ability`)
//! - Build: cdylib + `napi-build-ohos` + `ohrs build --arch aarch`
//!
//! `target_env = "ohos"` is `os::current`. The `jadix_winit` facade selects
//! `winit-ohos` with the same core as desktop. [`HarmonyOsHost`] describes
//! capabilities only — Ability owns the native loop.

use std::sync::Arc;

use crate::platform::host::{set_external_host, HostBackend, HostCapabilities, HostKind};

mod ability;
mod font_paths;
mod gpu;
mod run_loop;
mod window;

pub use ability::{
    install_ohos_content_rect, ohos_ability_installed, ohos_content_rect, OhosContentRect,
};

#[allow(unused_imports)]
pub use run_loop::run_ability_loop;
pub use window::apply_window_options;

#[allow(unused_imports)]
pub(crate) use crate::platform::desktop::{
    apply_ime_cursor_area, cancel_active_composition, clipboard_get_text, clipboard_set_text,
    edit_shortcut_modifiers, end_composition_ui, ime_disallow, process_private_mb,
    release_ui_control, sync_focus_owner, system_scroll_lines,
};
#[cfg(any(debug_assertions, test))]
#[allow(unused_imports)]
pub(crate) use crate::platform::desktop::{enable_ansi_stderr, local_hms};
#[allow(unused_imports)]
pub(crate) use font_paths::{eager_ui_font_files, lazy_script_font_files, system_font_dirs};
#[allow(unused_imports)]
pub(crate) use gpu::gpu_backend_order;
#[allow(unused_imports)]
pub(crate) use window::{apply_created_window, decorate_window_attributes};

/// Capabilities card for HarmonyOS. The loop is `winit-ohos`, not a take-over host.
pub struct HarmonyOsHost;

impl HostBackend for HarmonyOsHost {
    fn kind(&self) -> HostKind {
        HostKind::HarmonyOs
    }

    fn capabilities(&self) -> HostCapabilities {
        HostCapabilities::for_kind(HostKind::HarmonyOs)
    }
}

/// Register [`HarmonyOsHost`] as the process host (gpui-template `with_ohos_app`).
pub fn install_harmonyos_host() {
    set_external_host(Some(Arc::new(HarmonyOsHost)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_matches_gpui_template_contract() {
        let host = HarmonyOsHost;
        assert_eq!(host.kind(), HostKind::HarmonyOs);
        assert!(!host.take_over_event_loop());
        assert!(host.capabilities().suspend_resume);
        assert!(!host.capabilities().blocking_run);
        assert!(host.capabilities().recreate_window_after_destroy);
        assert!(!host.capabilities().multi_window);
    }
}
