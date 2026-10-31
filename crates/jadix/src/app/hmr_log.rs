//! Vite-style HMR console lines: dim timestamp, colored tag, one line per event.
//!
//! OS console VT and local clock live in [`crate::platform`]; this module only
//! formats the 12-hour stamp and honors `NO_COLOR` / `FORCE_COLOR`.

use std::io::{self, IsTerminal, Write};

const RESET: &str = "\x1b[0m";
const DIM: &str = "\x1b[2m";
const BOLD: &str = "\x1b[1m";
const CYAN: &str = "\x1b[36m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const RED: &str = "\x1b[31m";

fn color_enabled() -> bool {
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    if std::env::var_os("FORCE_COLOR").is_some() || std::env::var_os("JADIX_FORCE_COLOR").is_some()
    {
        return true;
    }
    io::stderr().is_terminal()
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
    crate::platform::local_hms()
        .map(|(h, m, s)| format_12h(h, m, s))
        .unwrap_or_else(utc_fallback)
}

fn utc_fallback() -> String {
    use web_time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let s = secs % 86_400;
    format_12h((s / 3600) as u16, ((s % 3600) / 60) as u16, (s % 60) as u16)
}

fn emit(action_color: &str, action: &str, detail: &str) {
    crate::platform::enable_ansi_stderr();
    let time = local_timestamp();
    let mut err = io::stderr().lock();
    let _ = if color_enabled() {
        writeln!(
            err,
            "{DIM}{time}{RESET} {CYAN}{BOLD}[jadix]{RESET} {action_color}{action}{RESET} {DIM}{detail}{RESET}"
        )
    } else {
        writeln!(err, "{time} [jadix] {action} {detail}")
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

pub(super) fn error(detail: &str) {
    emit(RED, "hmr error", detail);
}

#[cfg(test)]
#[path = "tests/hmr_log.rs"]
mod tests;
