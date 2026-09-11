//! Debounced mtime poll for theme / stylesheet files (no `notify` in the lib).
//!
//! Pair with [`super::apply_style_file_batch`] on the UI thread, then a full
//! `rebuild_ui` so baked token colors and `apply_stylesheet_rules` re-run.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime};

const POLL_MS: u64 = 400;

static WATCH_PATHS: OnceLock<Mutex<Vec<PathBuf>>> = OnceLock::new();
static NEED_REBUILD: AtomicBool = AtomicBool::new(false);
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
        .name("rui-style-watch".into())
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
    NEED_REBUILD.store(true, Ordering::SeqCst);
    crate::app::runtime::request_redraw();
}

/// Consume the “need full rebuild after style/theme file change” flag.
pub(crate) fn take_style_hot_rebuild() -> bool {
    NEED_REBUILD.swap(false, Ordering::SeqCst)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_names() {
        assert!(is_theme_override_file(Path::new("theme.toml")));
        assert!(!is_theme_override_file(Path::new("/app/globals.css")));
        assert!(!is_theme_override_file(Path::new("styles.toml")));
    }
}
