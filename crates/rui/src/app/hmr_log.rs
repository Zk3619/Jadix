//! Vite-style HMR console lines: dim timestamp, colored tag, one line per event.

use std::io::{self, IsTerminal, Write};

const RESET: &str = "\x1b[0m";
const DIM: &str = "\x1b[2m";
const BOLD: &str = "\x1b[1m";
const CYAN: &str = "\x1b[36m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
#[cfg(feature = "hot-reload")]
const RED: &str = "\x1b[31m";

fn color_enabled() -> bool {
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    if std::env::var_os("FORCE_COLOR").is_some() || std::env::var_os("RUI_FORCE_COLOR").is_some() {
        return true;
    }
    io::stderr().is_terminal()
}

fn enable_vt() {
    #[cfg(windows)]
    {
        use std::sync::Once;
        static ONCE: Once = Once::new();
        ONCE.call_once(|| unsafe {
            #[link(name = "kernel32")]
            extern "system" {
                fn GetStdHandle(nStdHandle: i32) -> *mut std::ffi::c_void;
                fn GetConsoleMode(hConsoleHandle: *mut std::ffi::c_void, lpMode: *mut u32) -> i32;
                fn SetConsoleMode(hConsoleHandle: *mut std::ffi::c_void, dwMode: u32) -> i32;
            }
            const STD_OUTPUT_HANDLE: i32 = -11;
            const STD_ERROR_HANDLE: i32 = -12;
            const ENABLE_VIRTUAL_TERMINAL_PROCESSING: u32 = 0x0004;
            for handle in [STD_ERROR_HANDLE, STD_OUTPUT_HANDLE] {
                let h = GetStdHandle(handle);
                let mut mode = 0u32;
                if GetConsoleMode(h, &mut mode) != 0 {
                    let _ = SetConsoleMode(h, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
                }
            }
        });
    }
}

fn format_12h(hour: u16, min: u16, sec: u16) -> String {
    let am = hour < 12;
    let h12 = match hour {
        0 => 12,
        13..=23 => hour - 12,
        _ => hour,
    };
    let suffix = if am { "AM" } else { "PM" };
    format!("{h12}:{min:02}:{sec:02} {suffix}")
}

fn local_timestamp() -> String {
    local_timestamp_os().unwrap_or_else(utc_fallback)
}

#[cfg(windows)]
fn local_timestamp_os() -> Option<String> {
    #[repr(C)]
    struct SystemTime {
        w_year: u16,
        w_month: u16,
        w_day_of_week: u16,
        w_day: u16,
        w_hour: u16,
        w_minute: u16,
        w_second: u16,
        w_milliseconds: u16,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetLocalTime(lp_system_time: *mut SystemTime);
    }
    let mut st = SystemTime {
        w_year: 0,
        w_month: 0,
        w_day_of_week: 0,
        w_day: 0,
        w_hour: 0,
        w_minute: 0,
        w_second: 0,
        w_milliseconds: 0,
    };
    unsafe {
        GetLocalTime(&mut st);
    }
    Some(format_12h(st.w_hour, st.w_minute, st.w_second))
}

#[cfg(unix)]
fn local_timestamp_os() -> Option<String> {
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
        Some(format_12h(
            tm.tm_hour as u16,
            tm.tm_min as u16,
            tm.tm_sec as u16,
        ))
    }
}

#[cfg(not(any(windows, unix)))]
fn local_timestamp_os() -> Option<String> {
    None
}

fn utc_fallback() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let s = secs % 86_400;
    format_12h((s / 3600) as u16, ((s % 3600) / 60) as u16, (s % 60) as u16)
}

fn emit(action_color: &str, action: &str, detail: &str) {
    enable_vt();
    let time = local_timestamp();
    let mut err = io::stderr().lock();
    let _ = if color_enabled() {
        writeln!(
            err,
            "{DIM}{time}{RESET} {CYAN}{BOLD}[rui]{RESET} {action_color}{action}{RESET} {DIM}{detail}{RESET}"
        )
    } else {
        writeln!(err, "{time} [rui] {action} {detail}")
    };
}

pub(super) fn display_path(p: &str) -> String {
    p.replace('\\', "/")
}

#[cfg(feature = "hot-reload")]
pub(super) fn ready(detail: &str) {
    emit(CYAN, "ready", detail);
}

pub(super) fn update(detail: &str) {
    emit(GREEN, "hmr update", detail);
}

#[allow(dead_code)]
pub(super) fn skip(detail: &str) {
    emit(YELLOW, "hmr skip", detail);
}

#[cfg(feature = "hot-reload")]
pub(super) fn error(detail: &str) {
    emit(RED, "hmr error", detail);
}
