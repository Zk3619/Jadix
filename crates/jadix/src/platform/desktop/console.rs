//! Desktop-default stderr ANSI and local wall-clock.
//!
//! Unix (including *BSD) uses `localtime_r`. Windows overrides VT + `GetLocalTime`
//! in `os/windows/console.rs`. Enabling VT is a Windows console quirk; Unix
//! terminals already understand ANSI.

pub(crate) fn enable_ansi_stderr() {}

pub(crate) fn local_hms() -> Option<(u16, u16, u16)> {
    local_hms_os()
}

#[cfg(unix)]
fn local_hms_os() -> Option<(u16, u16, u16)> {
    #[repr(C)]
    struct Tm {
        tm_sec: i32,
        tm_min: i32,
        tm_hour: i32,
        tm_mday: i32,
        tm_mon: i32,
        tm_year: i32,
        tm_wday: i32,
        tm_yday: i32,
        tm_isdst: i32,
        tm_gmtoff: i64,
        tm_zone: *const std::ffi::c_char,
    }
    extern "C" {
        fn time(tloc: *mut i64) -> i64;
        fn localtime_r(timer: *const i64, buf: *mut Tm) -> *mut Tm;
    }
    unsafe {
        let mut t: i64 = 0;
        time(&mut t);
        let mut tm: Tm = std::mem::zeroed();
        if localtime_r(&t, &mut tm).is_null() {
            return None;
        }
        Some((tm.tm_hour as u16, tm.tm_min as u16, tm.tm_sec as u16))
    }
}

#[cfg(not(unix))]
fn local_hms_os() -> Option<(u16, u16, u16)> {
    None
}

#[cfg(test)]
#[path = "tests/console.rs"]
mod tests;
