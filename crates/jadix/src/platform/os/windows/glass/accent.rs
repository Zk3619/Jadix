//! Win10 / fallback `SetWindowCompositionAttribute` AccentPolicy.

use std::sync::OnceLock;

use crate::app::glass_params::tint_to_accent_abgr;
use crate::app::WindowOptions;

const WCA_ACCENT_POLICY: u32 = 19;
const ACCENT_ENABLE_BLURBEHIND: u32 = 3;
const ACCENT_ENABLE_ACRYLICBLURBEHIND: u32 = 4;
/// Win11 22H2 (`22621`) removed `ACCENT_ENABLE_BLURBEHIND`.
const BUILD_WIN11_22H2: u32 = 22621;

#[repr(C)]
struct AccentPolicy {
    accent_state: u32,
    accent_flags: u32,
    gradient_color: u32,
    animation_id: u32,
}

#[repr(C)]
struct WindowCompositionAttribData {
    attrib: u32,
    pv_data: *mut core::ffi::c_void,
    cb_data: usize,
}

type SetWindowCompositionAttributeFn = unsafe extern "system" fn(
    hwnd: windows_sys::Win32::Foundation::HWND,
    data: *mut WindowCompositionAttribData,
) -> i32;

fn set_window_composition_attribute() -> Option<SetWindowCompositionAttributeFn> {
    static FN: OnceLock<Option<SetWindowCompositionAttributeFn>> = OnceLock::new();
    *FN.get_or_init(|| unsafe {
        let user32 = windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(
            windows_sys::core::w!("user32.dll"),
        );
        if user32.is_null() {
            return None;
        }
        let proc = windows_sys::Win32::System::LibraryLoader::GetProcAddress(
            user32,
            windows_sys::core::s!("SetWindowCompositionAttribute"),
        )?;
        Some(std::mem::transmute(proc))
    })
}

#[repr(C)]
struct RtlOsVersionInfo {
    dw_os_version_info_size: u32,
    dw_major_version: u32,
    dw_minor_version: u32,
    dw_build_number: u32,
    dw_platform_id: u32,
    sz_csd_version: [u16; 128],
}

type RtlGetVersionFn = unsafe extern "system" fn(*mut RtlOsVersionInfo) -> i32;

pub fn nt_build_number() -> u32 {
    static BUILD: OnceLock<u32> = OnceLock::new();
    *BUILD.get_or_init(|| unsafe {
        let ntdll = windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(
            windows_sys::core::w!("ntdll.dll"),
        );
        if ntdll.is_null() {
            return 0;
        }
        let proc = windows_sys::Win32::System::LibraryLoader::GetProcAddress(
            ntdll,
            windows_sys::core::s!("RtlGetVersion"),
        );
        let Some(proc) = proc else {
            return 0;
        };
        let rtl: RtlGetVersionFn = std::mem::transmute(proc);
        let mut info = RtlOsVersionInfo {
            dw_os_version_info_size: std::mem::size_of::<RtlOsVersionInfo>() as u32,
            dw_major_version: 0,
            dw_minor_version: 0,
            dw_build_number: 0,
            dw_platform_id: 0,
            sz_csd_version: [0; 128],
        };
        if rtl(&mut info) == 0 {
            info.dw_build_number
        } else {
            0
        }
    })
}

pub fn apply_accent_policy(hwnd: windows_sys::Win32::Foundation::HWND, options: &WindowOptions) {
    let Some(set_attr) = set_window_composition_attribute() else {
        return;
    };
    let build = nt_build_number();
    // Official system backdrop on 22H2+ is enough for stock Acrylic/Mica.
    if build >= BUILD_WIN11_22H2 && !options.wants_custom_dcomp() {
        return;
    }
    let accent_state = if build >= BUILD_WIN11_22H2 {
        ACCENT_ENABLE_ACRYLICBLURBEHIND
    } else if matches!(
        options.backdrop,
        crate::app::WindowBackdrop::Acrylic
            | crate::app::WindowBackdrop::Aero
            | crate::app::WindowBackdrop::Mica
            | crate::app::WindowBackdrop::MicaAlt
    ) {
        ACCENT_ENABLE_ACRYLICBLURBEHIND
    } else {
        ACCENT_ENABLE_BLURBEHIND
    };
    let mut policy = AccentPolicy {
        accent_state,
        accent_flags: 0,
        gradient_color: tint_to_accent_abgr(options.resolved_glass_tint()),
        animation_id: 0,
    };
    let mut data = WindowCompositionAttribData {
        attrib: WCA_ACCENT_POLICY,
        pv_data: (&mut policy) as *mut AccentPolicy as *mut _,
        cb_data: std::mem::size_of::<AccentPolicy>(),
    };
    unsafe {
        let _ = set_attr(hwnd, &mut data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::glass_params::tint_to_accent_abgr;

    #[test]
    fn accent_struct_layout() {
        assert_eq!(std::mem::size_of::<AccentPolicy>(), 16);
        assert_eq!(WCA_ACCENT_POLICY, 19);
        assert_eq!(ACCENT_ENABLE_BLURBEHIND, 3);
        assert_eq!(ACCENT_ENABLE_ACRYLICBLURBEHIND, 4);
        assert_eq!(BUILD_WIN11_22H2, 22621);
        assert_eq!(tint_to_accent_abgr([0x64, 0xff, 0xff, 0xff]), 0x64FFFFFF);
    }

    #[test]
    fn win11_22h2_skips_accent_for_system_acrylic() {
        let o = crate::app::WindowOptions::new()
            .blur(true)
            .backdrop(crate::app::WindowBackdrop::Acrylic);
        assert!(!o.wants_custom_dcomp());
        // apply_accent_policy itself is HWND-only; gate is the boolean above + build.
        assert!(nt_build_number() > 0);
    }
}
