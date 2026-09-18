//! Debounced mtime poll for `i18n.toml` / locale files (same loop shape as stylesheets).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime};

const POLL_MS: u64 = 400;

static WATCH_PATHS: OnceLock<Mutex<Vec<PathBuf>>> = OnceLock::new();
static ROOT: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();
static LOOP_STARTED: AtomicBool = AtomicBool::new(false);

fn watch_paths() -> &'static Mutex<Vec<PathBuf>> {
    WATCH_PATHS.get_or_init(|| Mutex::new(Vec::new()))
}

fn root_slot() -> &'static Mutex<Option<PathBuf>> {
    ROOT.get_or_init(|| Mutex::new(None))
}

/// Watch `config` and any extra locale files. On mtime change, reload `config`.
pub fn watch_i18n_root(config: impl AsRef<Path>, extras: &[PathBuf]) {
    let config = config.as_ref().to_path_buf();
    *root_slot().lock().unwrap_or_else(|e| e.into_inner()) = Some(config.clone());
    let mut all = vec![config];
    all.extend(extras.iter().cloned());
    watch_i18n_files(all);
}

/// Remember i18n overlay files and start a background poll loop (idempotent).
pub fn watch_i18n_files<I, P>(paths: I)
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
        .name("rui-i18n-watch".into())
        .spawn(i18n_watch_loop);
}

fn i18n_watch_loop() {
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
        let mut changed = false;
        for p in &paths {
            let m = std::fs::metadata(p).and_then(|m| m.modified()).ok();
            let prev = stamps.get(p);
            if prev != Some(&m) {
                if seeded && prev.is_some() {
                    changed = true;
                }
                stamps.insert(p.clone(), m);
            }
        }
        if !seeded {
            seeded = true;
            continue;
        }
        if !changed {
            continue;
        }
        let reload = root_slot()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        crate::app::ui_bridge::run_on_ui_thread(move || {
            let Some(root) = reload else {
                return;
            };
            super::runtime::clear_overlays_inner();
            if let Err(err) = super::load::reload(&root) {
                log::warn!("i18n watch reload failed for {}: {err}", root.display());
            }
        });
    }
}
