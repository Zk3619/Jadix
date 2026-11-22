//! CoreFoundation adapter. No AppKit objects, main-thread UI or process-global
//! locale changes; Foundation/AppKit hosts may overlay appearance/lifecycle.
use super::*;
use std::{ffi::{c_char, c_void, CString}, ptr, sync::{atomic::AtomicBool, mpsc::SyncSender, Arc}, time::Duration};

type Ref = *const c_void;
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(value: Ref);
    fn CFGetTypeID(value: Ref) -> usize;
    fn CFStringGetTypeID() -> usize;
    fn CFBooleanGetTypeID() -> usize;
    fn CFBooleanGetValue(value: Ref) -> bool;
    fn CFStringCreateWithCString(allocator: Ref, value: *const c_char, encoding: u32) -> Ref;
    fn CFStringGetCString(value: Ref, buffer: *mut c_char, capacity: isize, encoding: u32) -> bool;
    fn CFArrayGetCount(value: Ref) -> isize;
    fn CFArrayGetValueAtIndex(value: Ref, index: isize) -> Ref;
    fn CFLocaleCopyCurrent() -> Ref;
    fn CFLocaleGetIdentifier(locale: Ref) -> Ref;
    fn CFLocaleGetValue(locale: Ref, key: Ref) -> Ref;
    fn CFLocaleCopyPreferredLanguages() -> Ref;
    fn CFTimeZoneCopyDefault() -> Ref;
    fn CFTimeZoneGetName(zone: Ref) -> Ref;
    fn CFTimeZoneResetSystem();
    fn CFCalendarCopyCurrent() -> Ref;
    fn CFCalendarGetFirstWeekday(calendar: Ref) -> isize;
    fn CFDateFormatterCreateDateFormatFromTemplate(allocator: Ref, template: Ref, options: usize, locale: Ref) -> Ref;
    fn CFPreferencesCopyValue(key: Ref, app: Ref, user: Ref, host: Ref) -> Ref;
    fn CFPreferencesSynchronize(app: Ref, user: Ref, host: Ref) -> bool;
    static kCFLocaleMeasurementSystem: Ref;
    static kCFPreferencesAnyApplication: Ref;
    static kCFPreferencesCurrentUser: Ref;
    static kCFPreferencesAnyHost: Ref;
}
const UTF8: u32 = 0x08000100;
struct Owned(Ref);
impl Drop for Owned { fn drop(&mut self) { if !self.0.is_null() { unsafe { CFRelease(self.0) } } } }
fn string(value: Ref) -> Option<String> {
    if value.is_null() || unsafe { CFGetTypeID(value) != CFStringGetTypeID() } { return None; }
    let mut bytes = [0u8; 1024];
    if !unsafe { CFStringGetCString(value, bytes.as_mut_ptr().cast(), bytes.len() as isize, UTF8) } { return None; }
    let end = bytes.iter().position(|v| *v == 0)?;
    String::from_utf8(bytes[..end].to_vec()).ok()
}
fn cf_string(value: &str) -> Owned {
    let value = CString::new(value).expect("constant CF preference key");
    Owned(unsafe { CFStringCreateWithCString(ptr::null(), value.as_ptr(), UTF8) })
}
fn preference(key: &str, app: Ref) -> Owned {
    let key = cf_string(key);
    Owned(unsafe { CFPreferencesCopyValue(key.0, app, kCFPreferencesCurrentUser, kCFPreferencesAnyHost) })
}
fn boolean(value: Ref) -> Option<bool> {
    (!value.is_null() && unsafe { CFGetTypeID(value) == CFBooleanGetTypeID() })
        .then(|| unsafe { CFBooleanGetValue(value) })
}
fn language_id(value: Ref) -> Option<String> {
    let value = string(value)?;
    let value = value.split('@').next()?.replace('_', "-");
    super::language(&value).then_some(value)
}

pub(super) fn snapshot() -> io::Result<EnvironmentSnapshot> {
    let locale = Owned(unsafe { CFLocaleCopyCurrent() });
    if locale.0.is_null() { return Err(io::Error::other("CFLocaleCopyCurrent failed")); }
    let identifier = language_id(unsafe { CFLocaleGetIdentifier(locale.0) })
        .ok_or_else(|| io::Error::other("macOS returned an invalid locale identifier"))?;
    let preferred = Owned(unsafe { CFLocaleCopyPreferredLanguages() });
    let mut languages = Vec::new();
    if !preferred.0.is_null() {
        for index in 0..unsafe { CFArrayGetCount(preferred.0) }.min(16) {
            if let Some(value) = language_id(unsafe { CFArrayGetValueAtIndex(preferred.0, index) }) {
                if !languages.iter().any(|old: &String| old.eq_ignore_ascii_case(&value)) { languages.push(value); }
            }
        }
    }
    if languages.is_empty() { languages.push(identifier.clone()); }
    let zone = Owned(unsafe { CFTimeZoneCopyDefault() });
    let time_zone = if zone.0.is_null() { None } else {
        string(unsafe { CFTimeZoneGetName(zone.0) }).map(|id| TimeZone { kind: TimeZoneKind::Iana, id })
    };
    let calendar = Owned(unsafe { CFCalendarCopyCurrent() });
    let weekday = if calendar.0.is_null() { None } else {
        let value = unsafe { CFCalendarGetFirstWeekday(calendar.0) };
        (1..=7).contains(&value).then_some(((value + 5) % 7 + 1) as u8)
    };
    let template = cf_string("j");
    let format = Owned(unsafe { CFDateFormatterCreateDateFormatFromTemplate(ptr::null(), template.0, 0, locale.0) });
    let hour_cycle = string(format.0).and_then(|format| {
        let mut quoted = false;
        let mut chars = format.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\'' {
                if chars.peek() == Some(&'\'') { chars.next(); } else { quoted = !quoted; }
            } else if !quoted {
                match c { 'K' => return Some(HourCycle::H11), 'h' => return Some(HourCycle::H12),
                    'H' => return Some(HourCycle::H23), 'k' => return Some(HourCycle::H24), _ => {} }
            }
        }
        None
    });
    let measurement_system = string(unsafe { CFLocaleGetValue(locale.0, kCFLocaleMeasurementSystem) })
        .and_then(|value| match value.as_str() {
            "Metric" => Some(MeasurementSystem::Metric), "U.S." => Some(MeasurementSystem::Us),
            "U.K." => Some(MeasurementSystem::Uk), _ => None,
        });
    let global = unsafe { kCFPreferencesAnyApplication };
    let temperature = preference("AppleTemperatureUnit", global);
    let temperature_unit = string(temperature.0).and_then(|v| match v.as_str() {
        "Celsius" => Some(TemperatureUnit::Celsius), "Fahrenheit" => Some(TemperatureUnit::Fahrenheit), _ => None,
    });
    let appearance = preference("AppleInterfaceStyle", global);
    let accessibility_app = cf_string("com.apple.universalaccess");
    let motion = preference("reduceMotion", accessibility_app.0);
    let contrast = preference("increaseContrast", accessibility_app.0);
    let direction = languages.first().map(|v| {
        // CFLocale character direction follows the selected language's default.
        let language = cf_string(v);
        unsafe extern "C" { fn CFLocaleGetLanguageCharacterDirection(language: Ref) -> usize; }
        if unsafe { CFLocaleGetLanguageCharacterDirection(language.0) } == 2 {
            LayoutDirection::RightToLeft
        } else { LayoutDirection::LeftToRight }
    });
    let snapshot = EnvironmentSnapshot {
        platform: Some(PlatformInfo { name: "macos".into(), os: Some(OperatingSystem::Macos) }),
        locale: LocaleSettings { locale: identifier, languages, time_zone, hour_cycle,
            first_day_of_week: weekday, measurement_system, temperature_unit },
        color_scheme: Some(if string(appearance.0).as_deref() == Some("Dark") { ColorScheme::Dark } else { ColorScheme::Light }),
        layout_direction: direction, high_contrast: boolean(contrast.0), reduced_motion: boolean(motion.0),
        ..Default::default()
    };
    snapshot.validate()?;
    Ok(snapshot)
}

pub(super) fn run_watch(stop: Arc<AtomicBool>, ready: SyncSender<io::Result<usize>>, mut changed: impl FnMut()) {
    let _ = ready.send(Ok(0));
    while !stop.load(Ordering::Acquire) {
        // CF preferences and the system timezone have independent caches. Refresh
        // both before comparison; only changed snapshots wake the consumer.
        unsafe {
            CFPreferencesSynchronize(kCFPreferencesAnyApplication, kCFPreferencesCurrentUser, kCFPreferencesAnyHost);
            CFTimeZoneResetSystem();
        }
        changed();
        // Bounded shutdown latency, no UI/main-run-loop ownership.
        for _ in 0..10 {
            if stop.load(Ordering::Acquire) { break; }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}
