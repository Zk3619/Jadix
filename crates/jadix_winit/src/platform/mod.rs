//! # Platform implementations
//!
//! This module re-exports the platform-specific implementation crates that are used by default in
//! Winit.
//!
//! Only the crates / modules corresponding to the platform you're compiling to will be available.
//!
//! | Platform | Crate | Module |
//! | -------- | ----- | ------ |
#![doc = concat!("| Android | [`winit-android`](https://docs.rs/winit-android/", "0.31.0-beta.3", "/) | `jadix_winit::platform::android` |")]
#![doc = concat!("| macOS | [`winit-appkit`](https://docs.rs/winit-appkit/", "0.31.0-beta.3", "/) | `jadix_winit::platform::macos` |")]
//! | OpenHarmony | [`winit-ohos`](https://github.com/ohos-rs/winit-ohos) | `jadix_winit::platform::ohos` |
#![doc = concat!("| Redox | [`winit-orbital`](https://docs.rs/winit-orbital/", "0.31.0-beta.3", "/) | `jadix_winit::platform::orbital` |")]
#![doc = concat!("| iOS/visionOS/tvOS/Mac Catalyst | [`winit-uikit`](https://docs.rs/winit-uikit/", "0.31.0-beta.3", "/) | `jadix_winit::platform::ios` |")]
#![doc = concat!("| Wayland | [`winit-wayland`](https://docs.rs/winit-wayland/", "0.31.0-beta.3", "/) | `jadix_winit::platform::wayland` |")]
#![doc = concat!("| Web | [`winit-web`](https://docs.rs/winit-web/", "0.31.0-beta.3", "/) | `jadix_winit::platform::web` |")]
#![doc = concat!("| Windows | [`winit-win32`](https://docs.rs/winit-win32/", "0.31.0-beta.3", "/) | `jadix_winit::platform::windows` |")]
#![doc = concat!("| X11 | [`winit-x11`](https://docs.rs/winit-x11/", "0.31.0-beta.3", "/) | `jadix_winit::platform::x11` |")]
//! ## Common modules
//!
//! Furthermore, we provide two modules for common functionality:
//! - `scancode`, available on Windows, macOS, Wayland and X11.
//! - `startup_notify`, available on Wayland and X11.

#[cfg(android_platform)]
pub use winit_android as android;
#[cfg(macos_platform)]
pub mod macos;
#[cfg(ohos_platform)]
pub use winit_ohos as ohos;
#[cfg(orbital_platform)]
pub use winit_orbital as orbital;
#[cfg(ios_platform)]
pub use winit_uikit as ios;
#[cfg(wayland_platform)]
pub use winit_wayland as wayland;
#[cfg(web_platform)]
pub use winit_web as web;
#[cfg(windows_platform)]
pub mod windows;
#[cfg(x11_platform)]
pub use winit_x11 as x11;

#[cfg(any(
    windows_platform,
    macos_platform,
    x11_platform,
    wayland_platform,
    docsrs
))]
pub mod scancode;
#[cfg(any(x11_platform, wayland_platform, docsrs))]
pub mod startup_notify;
