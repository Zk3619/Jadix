//! OS-specific overrides of [`crate::platform::desktop`] defaults.

#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(target_os = "windows")]
pub(crate) use windows as current;

#[cfg(target_os = "macos")]
pub(crate) use macos as current;

#[cfg(target_os = "linux")]
pub(crate) use linux as current;
