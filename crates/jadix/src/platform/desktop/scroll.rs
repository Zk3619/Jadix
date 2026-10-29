//! Wheel "lines" fallback. Windows overrides via SPI; others use the toolkit default of 3.

/// GTK / Cocoa / Qt conventional default when the OS has no SPI-style setting.
pub(crate) const DEFAULT_SCROLL_LINES: f32 = 3.0;

pub(crate) fn system_scroll_lines() -> f32 {
    #[cfg(target_os = "linux")]
    {
        if let Some(n) = linux_user_scroll_lines() {
            return n;
        }
    }
    DEFAULT_SCROLL_LINES
}

#[cfg(target_os = "linux")]
fn linux_user_scroll_lines() -> Option<f32> {
    let path = kdeglobals_path()?;
    let text = std::fs::read_to_string(path).ok()?;
    parse_kde_wheel_scroll_lines(&text)
}

#[cfg(target_os = "linux")]
fn kdeglobals_path() -> Option<std::path::PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        if !xdg.is_empty() {
            let p = std::path::PathBuf::from(xdg).join("kdeglobals");
            if p.is_file() {
                return Some(p);
            }
        }
    }
    let home = std::env::var("HOME").ok()?;
    let p = std::path::PathBuf::from(home).join(".config/kdeglobals");
    p.is_file().then_some(p)
}

/// KDE `kdeglobals` `[KDE] WheelScrollLines`. `None` when missing or non-positive.
pub(crate) fn parse_kde_wheel_scroll_lines(text: &str) -> Option<f32> {
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
            continue;
        }
        let Some(rest) = line.strip_prefix("WheelScrollLines") else {
            continue;
        };
        let Some(rest) = rest.trim_start().strip_prefix('=') else {
            continue;
        };
        let n: f32 = rest.trim().parse().ok()?;
        if n > 0.0 {
            return Some(n);
        }
    }
    None
}

#[cfg(test)]
#[path = "tests/scroll.rs"]
mod tests;
