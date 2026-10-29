//! OpenHarmony Ability handle (gpui-template / ohos-rs contract).
//!
//! Template entry:
//! ```ignore
//! #[openharmony_ability_derive::ability]
//! fn openharmony_app(app: OpenHarmonyApp) {
//!     jadix::run_ohos(app, WindowOptions::default(), || ui! { <App/> });
//! }
//! ```
//!
//! ArkTS: `EntryAbility extends NativeAbility` with `moduleName` = the Rust cdylib.

use std::sync::Mutex;

/// Latest Ability content rectangle (`OpenHarmonyApp::content_rect`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OhosContentRect {
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
}

impl OhosContentRect {
    pub const fn new(width: i32, height: i32) -> Self {
        Self {
            left: 0,
            top: 0,
            width,
            height,
        }
    }

    pub fn is_usable(self) -> bool {
        self.width > 0 && self.height > 0
    }
}

fn rect_slot() -> &'static Mutex<OhosContentRect> {
    static SLOT: Mutex<OhosContentRect> = Mutex::new(OhosContentRect {
        left: 0,
        top: 0,
        width: 0,
        height: 0,
    });
    &SLOT
}

fn installed_slot() -> &'static Mutex<bool> {
    static SLOT: Mutex<bool> = Mutex::new(false);
    &SLOT
}

/// Record the Ability window size (tests / `WindowResize` / `content_rect`).
pub fn install_ohos_content_rect(rect: OhosContentRect) {
    *rect_slot().lock().unwrap_or_else(|p| p.into_inner()) = rect;
    if rect.is_usable() {
        *installed_slot().lock().unwrap_or_else(|p| p.into_inner()) = true;
    }
}

pub fn ohos_content_rect() -> OhosContentRect {
    *rect_slot().lock().unwrap_or_else(|p| p.into_inner())
}

pub fn ohos_ability_installed() -> bool {
    *installed_slot().lock().unwrap_or_else(|p| p.into_inner())
}

#[cfg(test)]
pub(crate) fn reset_ohos_ability_for_tests() {
    *rect_slot().lock().unwrap_or_else(|p| p.into_inner()) = OhosContentRect::default();
    *installed_slot().lock().unwrap_or_else(|p| p.into_inner()) = false;
}

#[cfg(test)]
#[path = "tests/ability.rs"]
mod tests;
