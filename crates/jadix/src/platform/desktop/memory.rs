//! Process private-ish memory for diagnostics (`JADIX_TREE_STATS` etc.).
//!
//! Windows uses `GetProcessMemoryInfo` PrivateUsage in `os/windows`.
//! Here: Linux `/proc` private RSS; macOS `resident_size` (RSS, not phys_footprint).

#[cfg(target_os = "linux")]
pub(crate) fn process_private_mb() -> f64 {
    linux_process_private_mb()
}

#[cfg(target_os = "macos")]
pub(crate) fn process_private_mb() -> f64 {
    macos_process_private_mb()
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub(crate) fn process_private_mb() -> f64 {
    0.0
}

#[cfg(target_os = "linux")]
fn linux_process_private_mb() -> f64 {
    if let Ok(text) = std::fs::read_to_string("/proc/self/smaps_rollup") {
        if let Some(kb) = parse_smaps_rollup_private_kb(&text) {
            return kb as f64 / 1024.0;
        }
    }
    if let Ok(text) = std::fs::read_to_string("/proc/self/status") {
        if let Some(kb) = parse_status_rss_anon_kb(&text) {
            return kb as f64 / 1024.0;
        }
    }
    0.0
}

/// `Private_Dirty` + `Private_Clean` from `smaps_rollup` (kB). Closest Linux analog to
/// Windows Private Bytes among cheap `/proc` reads.
pub(crate) fn parse_smaps_rollup_private_kb(text: &str) -> Option<u64> {
    let mut dirty = None;
    let mut clean = None;
    for line in text.lines() {
        if let Some(v) = kb_field(line, "Private_Dirty:") {
            dirty = Some(v);
        } else if let Some(v) = kb_field(line, "Private_Clean:") {
            clean = Some(v);
        }
    }
    match (dirty, clean) {
        (Some(d), Some(c)) => Some(d.saturating_add(c)),
        (Some(d), None) => Some(d),
        (None, Some(c)) => Some(c),
        _ => None,
    }
}

/// `RssAnon` from `/proc/self/status` (kB). Fallback when `smaps_rollup` is missing.
pub(crate) fn parse_status_rss_anon_kb(text: &str) -> Option<u64> {
    for line in text.lines() {
        if let Some(v) = kb_field(line, "RssAnon:") {
            return Some(v);
        }
    }
    None
}

fn kb_field(line: &str, prefix: &str) -> Option<u64> {
    let rest = line.strip_prefix(prefix)?.trim();
    let num = rest.split_whitespace().next()?;
    num.parse().ok()
}

#[cfg(target_os = "macos")]
fn macos_process_private_mb() -> f64 {
    unsafe {
        let mut info: libc::mach_task_basic_info = std::mem::zeroed();
        let mut count = libc::MACH_TASK_BASIC_INFO_COUNT;
        let kr = libc::task_info(
            libc::mach_task_self(),
            libc::MACH_TASK_BASIC_INFO,
            (&mut info as *mut libc::mach_task_basic_info).cast(),
            &mut count,
        );
        if kr == libc::KERN_SUCCESS {
            return info.resident_size as f64 / (1024.0 * 1024.0);
        }
    }
    0.0
}

#[cfg(test)]
#[path = "tests/memory.rs"]
mod tests;
