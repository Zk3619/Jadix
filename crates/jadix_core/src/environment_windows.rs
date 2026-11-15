use super::*;
use windows_sys::Win32::{
    Globalization::*,
    System::{Registry::*, Time::*},
    UI::{Accessibility::*, WindowsAndMessaging::*},
};

fn locale_value(locale: &[u16], kind: u32) -> Option<String> {
    let mut buf = [0u16; 256];
    let len = unsafe { GetLocaleInfoEx(locale.as_ptr(), kind, buf.as_mut_ptr(), buf.len() as i32) };
    (len > 1).then(|| String::from_utf16_lossy(&buf[..len as usize - 1]))
}
fn appearance() -> Option<ColorScheme> {
    let path: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize\0"
        .encode_utf16()
        .collect();
    let key: Vec<u16> = "AppsUseLightTheme\0".encode_utf16().collect();
    let mut value = 0u32;
    let mut size = 4u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            path.as_ptr(),
            key.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&mut value as *mut u32).cast(),
            &mut size,
        )
    };
    if status != 0 {
        return None;
    }
    match value {
        0 => Some(ColorScheme::Dark),
        1 => Some(ColorScheme::Light),
        _ => None,
    }
}
fn high_contrast() -> Option<bool> {
    let mut settings = HIGHCONTRASTW {
        cbSize: std::mem::size_of::<HIGHCONTRASTW>() as u32,
        ..Default::default()
    };
    (unsafe {
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            settings.cbSize,
            (&mut settings as *mut HIGHCONTRASTW).cast(),
            0,
        )
    } != 0)
        .then_some(settings.dwFlags & HCF_HIGHCONTRASTON != 0)
}
pub(super) fn snapshot() -> io::Result<EnvironmentSnapshot> {
    let mut locale = [0u16; 85];
    let len = unsafe { GetUserDefaultLocaleName(locale.as_mut_ptr(), locale.len() as i32) };
    if len <= 1 {
        return Err(io::Error::last_os_error());
    }
    let name = String::from_utf16_lossy(&locale[..len as usize - 1]);
    let mut count = 0u32;
    let mut size = 0u32;
    let ok = unsafe {
        GetUserPreferredUILanguages(
            MUI_LANGUAGE_NAME,
            &mut count,
            std::ptr::null_mut(),
            &mut size,
        )
    };
    if ok == 0 || size > 4096 || count > 16 {
        return Err(io::Error::other(
            "preferred languages unavailable or exceed budget",
        ));
    }
    let mut buffer = vec![0u16; size as usize];
    if unsafe {
        GetUserPreferredUILanguages(
            MUI_LANGUAGE_NAME,
            &mut count,
            buffer.as_mut_ptr(),
            &mut size,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    if size as usize > buffer.len() {
        return Err(io::Error::other("preferred language buffer changed"));
    }
    let mut languages: Vec<String> = buffer[..size as usize]
        .split(|v| *v == 0)
        .filter(|s| !s.is_empty())
        .map(String::from_utf16_lossy)
        .collect();
    if languages.is_empty() {
        languages.push(name.clone());
    }
    let mut zone: DYNAMIC_TIME_ZONE_INFORMATION = unsafe { std::mem::zeroed() };
    let zone_status = unsafe { GetDynamicTimeZoneInformation(&mut zone) };
    let end = zone
        .TimeZoneKeyName
        .iter()
        .position(|v| *v == 0)
        .unwrap_or(zone.TimeZoneKeyName.len());
    let time_zone = (zone_status != u32::MAX && end > 0).then(|| TimeZone {
        kind: TimeZoneKind::Windows,
        id: String::from_utf16_lossy(&zone.TimeZoneKeyName[..end]),
    });
    let mut animation = 0i32;
    let motion_ok = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            (&mut animation as *mut i32).cast(),
            0,
        )
    };
    let value = EnvironmentSnapshot {
        platform: Some(PlatformInfo {
            name: "windows".into(),
            os: Some(OperatingSystem::Windows),
        }),
        locale: LocaleSettings {
            locale: name,
            languages,
            time_zone,
            hour_cycle: match locale_value(&locale, LOCALE_ITIME).as_deref() {
                Some("0") => Some(HourCycle::H12),
                Some("1") => Some(HourCycle::H23),
                _ => None,
            },
            first_day_of_week: locale_value(&locale, LOCALE_IFIRSTDAYOFWEEK)
                .and_then(|v| v.parse::<u8>().ok())
                .filter(|v| *v < 7)
                .map(|v| v + 1),
            measurement_system: match locale_value(&locale, LOCALE_IMEASURE).as_deref() {
                Some("0") => Some(MeasurementSystem::Metric),
                Some("1") => Some(MeasurementSystem::Us),
                _ => None,
            },
            // Win32 NLS does not report an independent temperature preference.
            temperature_unit: None,
        },
        color_scheme: appearance(),
        layout_direction: None,
        high_contrast: high_contrast(),
        reduced_motion: (motion_ok != 0).then_some(animation == 0),
        font_scale: 1.0,
        lifecycle: Lifecycle::Active,
    };
    value.validate()?;
    Ok(value)
}

const STOP: u32 = WM_APP + 0x431;
const CHANGED: u32 = WM_APP + 0x432;
unsafe extern "system" fn watch_proc(
    window: windows_sys::Win32::Foundation::HWND,
    message: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    if matches!(
        message,
        WM_SETTINGCHANGE | WM_THEMECHANGED | WM_TIMECHANGE | WM_DISPLAYCHANGE
    ) {
        // Keep user callbacks and snapshot I/O outside the OS callback/FFI.
        unsafe {
            PostMessageW(window, CHANGED, 0, 0);
        }
        return 0;
    }
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}
pub(super) fn run_watch(
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ready: std::sync::mpsc::SyncSender<io::Result<usize>>,
    mut changed: impl FnMut(),
) {
    use std::sync::{atomic::Ordering, OnceLock};
    static CLASS: OnceLock<u16> = OnceLock::new();
    let instance =
        unsafe { windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(std::ptr::null()) };
    let name: Vec<u16> = "JadixSystemEnvironmentV1\0".encode_utf16().collect();
    let atom = *CLASS.get_or_init(|| unsafe {
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(watch_proc),
            hInstance: instance,
            lpszClassName: name.as_ptr(),
            ..std::mem::zeroed()
        })
    });
    if atom == 0 {
        let _ = ready.send(Err(io::Error::last_os_error()));
        return;
    }
    // Broadcast settings messages exclude message-only windows. This window is
    // top-level but never shown, focused or added to the taskbar.
    let window = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            name.as_ptr(),
            name.as_ptr(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        )
    };
    if window.is_null() {
        let _ = ready.send(Err(io::Error::last_os_error()));
        return;
    }
    let _ = ready.send(Ok(window as usize));
    changed();
    let mut message: MSG = unsafe { std::mem::zeroed() };
    while !stop.load(Ordering::Acquire)
        && unsafe { GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) } > 0
    {
        if message.message == STOP {
            break;
        }
        if message.message == CHANGED {
            let mut redundant: MSG = unsafe { std::mem::zeroed() };
            unsafe {
                while PeekMessageW(&mut redundant, window, CHANGED, CHANGED, PM_REMOVE) != 0 {}
            }
            changed();
            continue;
        }
        unsafe {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    unsafe {
        DestroyWindow(window);
    }
}
pub(super) fn stop_watch(token: usize) {
    if token != 0 {
        unsafe {
            PostMessageW(token as _, STOP, 0, 0);
        }
    }
}
