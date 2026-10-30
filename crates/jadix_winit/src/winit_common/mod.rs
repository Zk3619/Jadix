//! Winit implementation helpers.

#![warn(clippy::exhaustive_enums)]

#[cfg(target_vendor = "apple")]
pub mod core_foundation;
pub mod event_handler;
#[cfg(target_vendor = "apple")]
pub mod foundation;
pub mod positioner;
#[cfg(all(
    unix,
    not(any(
        target_os = "redox",
        target_family = "wasm",
        target_os = "android",
        target_vendor = "apple",
        target_env = "ohos"
    ))
))]
pub mod xkb;
