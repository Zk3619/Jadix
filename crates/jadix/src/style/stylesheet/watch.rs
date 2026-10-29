//! Debounced mtime poll for theme / stylesheet files (no `notify` in the lib).
//!
//! Commit [`super::apply_style_file_batch`] on the UI thread. The next redraw
//! refreshes subscribed window roots and islands so baked token colors re-resolve.

use std::cell::Cell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime};

const POLL_MS: u64 = 400;

static WATCH_PATHS: OnceLock<Mutex<Vec<PathBuf>>> = OnceLock::new();
thread_local! {
    static NEED_REBUILD: Cell<bool> = const { Cell::new(false) };
}
static LOOP_STARTED: AtomicBool = AtomicBool::new(false);

fn watch_paths() -> &'static Mutex<Vec<PathBuf>> {
    WATCH_PATHS.get_or_init(|| Mutex::new(Vec::new()))
}

/// True when this path is a theme-only file (`theme.toml`), not a rule sheet.
pub fn is_theme_override_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.eq_ignore_ascii_case("theme.toml"))
        .unwrap_or(false)
}

/// Remember paths and start a background poll loop (idempotent).
pub fn watch_stylesheets<I, P>(paths: I)
where
    I: IntoIterator<Item = P>,
    P: AsRef<Path>,
{
    {
        let mut guard = watch_paths().lock().unwrap_or_else(|e| e.into_inner());
        for p in paths {
            let p = p.as_ref().to_path_buf();
            if !guard.iter().any(|x| x == &p) {
                guard.push(p);
            }
        }
    }
    if LOOP_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("jadix-style-watch".into())
        .spawn(style_watch_loop);
}

fn style_watch_loop() {
    let mut stamps: HashMap<PathBuf, Option<SystemTime>> = HashMap::new();
    let mut seeded = false;
    loop {
        std::thread::sleep(Duration::from_millis(POLL_MS));
        let paths = watch_paths()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if paths.is_empty() {
            continue;
        }
        let mut changed = Vec::new();
        for p in &paths {
            let m = std::fs::metadata(p).and_then(|m| m.modified()).ok();
            let prev = stamps.get(p);
            if prev != Some(&m) {
                if seeded && prev.is_some() {
                    changed.push(p.clone());
                }
                stamps.insert(p.clone(), m);
            }
        }
        if !seeded {
            seeded = true;
            continue;
        }
        if changed.is_empty() {
            continue;
        }
        let reload = paths.clone();
        crate::app::ui_bridge::run_on_ui_thread(move || {
            if let Err(err) = super::apply_style_file_batch(&reload) {
                log::warn!("stylesheet watch reload failed: {err}");
                return;
            }
            mark_style_hot_rebuild();
        });
    }
}

pub(crate) fn mark_style_hot_rebuild() {
    NEED_REBUILD.with(|pending| pending.set(true));
    crate::app::runtime::request_redraw_all();
}

/// Consume the “need full rebuild after style/theme file change” flag.
pub(crate) fn take_style_hot_rebuild() -> bool {
    NEED_REBUILD.with(|pending| pending.replace(false))
}

fn style_generation() -> crate::core::reactivity::Signal<u64> {
    crate::core::reactivity::use_signal("__jadix_style_generation", || 0)
}

/// Subscribe outside isolated component bodies so all window roots and islands refresh.
pub(crate) fn track_style_changes() {
    let _ = style_generation().get();
}

/// Flush only while window trees are free to borrow. Multiple mutations coalesce per frame.
pub(crate) fn flush_style_rebuild() {
    if take_style_hot_rebuild() {
        crate::app::reactive::reset_static_templates();
        let generation = style_generation();
        generation.set(generation.peek().wrapping_add(1));
        crate::app::runtime::sync_window_theme();
    }
}

#[cfg(test)]
#[path = "tests/watch.rs"]
mod tests;
