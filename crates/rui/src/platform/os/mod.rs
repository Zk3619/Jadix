//! OS-specific overrides of [`crate::platform::desktop`] defaults.
//!
//! `current` is the compiled host. OpenHarmony is `target_env = "ohos"` (often
//! also `target_os = "linux"`) and must win over the Linux module.

#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(all(target_os = "linux", not(target_env = "ohos")))]
pub mod linux;

#[cfg(target_os = "ios")]
pub mod ios;

#[cfg(target_os = "android")]
pub mod android;

#[cfg(target_arch = "wasm32")]
pub mod web;

/// Always compiled so the HarmonyOS contract is testable on desktop.
pub mod harmonyos;

#[cfg(target_os = "windows")]
pub(crate) use windows as current;

#[cfg(target_os = "macos")]
pub(crate) use macos as current;

#[cfg(all(target_os = "linux", not(target_env = "ohos")))]
pub(crate) use linux as current;

#[cfg(target_os = "ios")]
pub(crate) use ios as current;

#[cfg(target_os = "android")]
pub(crate) use android as current;

#[cfg(target_arch = "wasm32")]
pub(crate) use web as current;

#[cfg(target_env = "ohos")]
pub(crate) use harmonyos as current;
