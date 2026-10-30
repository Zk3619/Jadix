#[cfg(android_platform)]
pub(crate) use crate::winit_android as platform;
#[cfg(macos_platform)]
pub(crate) use crate::winit_appkit as platform;
#[cfg(ohos_platform)]
pub(crate) use crate::winit_ohos as platform;
#[cfg(any(x11_platform, wayland_platform))]
mod linux;
#[cfg(orbital_platform)]
pub(crate) use crate::winit_orbital as platform;
#[cfg(ios_platform)]
pub(crate) use crate::winit_uikit as platform;
#[cfg(web_platform)]
pub(crate) use crate::winit_web as platform;
#[cfg(windows_platform)]
pub(crate) use crate::winit_win32 as platform;

#[cfg(any(x11_platform, wayland_platform))]
use self::linux as platform;
#[allow(unused_imports)]
pub use self::platform::*;

#[cfg(all(
    not(ios_platform),
    not(windows_platform),
    not(macos_platform),
    not(android_platform),
    not(ohos_platform),
    not(x11_platform),
    not(wayland_platform),
    not(web_platform),
    not(orbital_platform),
))]
compile_error!("The platform you're compiling for is not supported by winit");
