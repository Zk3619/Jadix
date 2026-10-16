//! HarmonyOS host example — same Ability contract as `reference/gpui-template-main`.
//!
//! ```text
//! ohrs build --arch aarch
//! ```
//!
//! ArkTS (`ohos/entry/.../EntryAbility.ets`):
//! `export default class EntryAbility extends NativeAbility { moduleName = "harmonyos_host" }`

#[cfg(target_env = "ohos")]
mod ohos_main {
    use jadix::prelude::*;
    use openharmony_ability::OpenHarmonyApp;

    #[openharmony_ability_derive::ability]
    pub fn openharmony_app(app: OpenHarmonyApp) {
        jadix::run_ohos(
            app,
            WindowOptions::default().title("Jadix HarmonyOS"),
            || {
                ui! {
                    <App title="Jadix HarmonyOS">
                        <Label>"HarmonyOS Ability host"</Label>
                    </App>
                }
            },
        );
    }
}

/// Desktop / CI: the Ability entry only exists on `target_env = "ohos"`.
#[cfg(not(target_env = "ohos"))]
pub fn ohos_target_only() -> &'static str {
    "compile this crate with an OpenHarmony rustc target (see gpui-template + ohrs)"
}

#[cfg(all(test, not(target_env = "ohos")))]
mod tests {
    #[test]
    fn stub_documents_ohos_target() {
        assert!(super::ohos_target_only().contains("ohrs"));
    }
}
