use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::error::Result;
use crate::flags::StateFlags;
use crate::path::{resolve_storage_path, StorageLocation, DEFAULT_FILENAME, ENV_PATH};
use crate::state::WindowState;
use crate::store::WindowStateFile;
use crate::view::WindowView;

/// Default persistence identity for a primary window.
pub const DEFAULT_LABEL: &str = "main";

type LabelMapFn = dyn Fn(&str) -> String + Send + Sync;
type FilterFn = dyn Fn(&str) -> bool + Send + Sync;

struct Inner {
    path: PathBuf,
    filename: String,
    windows: HashMap<String, WindowState>,
    denylist: HashSet<String>,
    skip_initial: HashSet<String>,
    filter: Option<Arc<FilterFn>>,
    map_label: Option<Arc<LabelMapFn>>,
}

/// In-memory cache + TOML writer.
pub struct WindowStateManager {
    inner: Mutex<Inner>,
    restoring: Mutex<()>,
    flags: StateFlags,
    autosave: bool,
}

impl WindowStateManager {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn flags(&self) -> StateFlags {
        self.flags
    }

    pub fn path(&self) -> PathBuf {
        self.lock().path.clone()
    }

    pub fn filename(&self) -> String {
        self.lock().filename.clone()
    }

    /// Map a raw window label through `map_label` / denylist / filter.
    /// `None` means this window is not tracked.
    pub fn effective_label(&self, raw: &str) -> Option<String> {
        self.lock().effective_label(raw)
    }

    pub fn get(&self, label: &str) -> Option<WindowState> {
        let g = self.lock();
        let mapped = g.effective_label(label)?;
        g.windows.get(&mapped).copied()
    }

    /// Restore `label` onto `window`. Unknown labels insert a live snapshot instead.
    pub fn restore(&self, label: &str, window: &dyn WindowView) -> Result<()> {
        let Some(mapped) = self.effective_label(label) else {
            return Ok(());
        };
        let _restoring = self.restoring.lock().unwrap_or_else(|p| p.into_inner());
        let skip = {
            let g = self.lock();
            g.skip_initial.contains(&mapped)
        };
        if skip {
            self.capture(&mapped, window)?;
            return Ok(());
        }
        let snapshot = {
            let g = self.lock();
            g.windows.get(&mapped).copied()
        };
        if let Some(state) = snapshot.filter(|s| !s.is_placeholder()) {
            state.apply_to(window, self.flags);
        } else {
            self.capture(&mapped, window)?;
        }
        Ok(())
    }

    /// Write live fields into the cache (does not flush unless `autosave` was set by caller).
    pub fn capture(&self, label: &str, window: &dyn WindowView) -> Result<()> {
        let Some(mapped) = self.effective_label(label) else {
            return Ok(());
        };
        let mut g = self.lock();
        let entry = g.windows.entry(mapped).or_default();
        entry.capture_from(window, self.flags);
        Ok(())
    }

    pub fn note_moved(&self, label: &str, x: i32, y: i32) {
        if !self.flags.contains(StateFlags::POSITION) {
            return;
        }
        if self.restoring.try_lock().is_err() {
            return;
        }
        let Some(mapped) = self.effective_label(label) else {
            return;
        };
        let mut g = self.lock();
        let entry = g.windows.entry(mapped).or_default();
        entry.note_moved(x, y);
    }

    pub fn note_resized(
        &self,
        label: &str,
        width: u32,
        height: u32,
        maximized: bool,
        minimized: bool,
    ) {
        if !self.flags.contains(StateFlags::SIZE) {
            return;
        }
        if self.restoring.try_lock().is_err() {
            return;
        }
        if maximized || minimized {
            return;
        }
        let Some(mapped) = self.effective_label(label) else {
            return;
        };
        let mut g = self.lock();
        let entry = g.windows.entry(mapped).or_default();
        entry.note_resized(width, height);
    }

    /// Flush the cache to TOML.
    pub fn save(&self) -> Result<()> {
        let g = self.lock();
        let mut doc = WindowStateFile::default();
        for (k, v) in &g.windows {
            doc.windows.insert(k.clone(), *v);
        }
        let path = g.path.clone();
        drop(g);
        doc.save(&path)
    }

    pub fn autosave(&self) -> bool {
        self.autosave
    }

    pub fn save_if_autosave(&self) -> Result<()> {
        if self.autosave {
            self.save()
        } else {
            Ok(())
        }
    }

    /// True while [`Self::restore`] holds the restoring lock (ignore live move/resize).
    pub fn is_restoring(&self) -> bool {
        self.restoring.try_lock().is_err()
    }
}

impl Inner {
    fn effective_label(&self, raw: &str) -> Option<String> {
        let mapped = self
            .map_label
            .as_ref()
            .map(|f| f(raw))
            .unwrap_or_else(|| raw.to_string());
        let mapped = mapped.trim();
        if mapped.is_empty() {
            return None;
        }
        if self.denylist.contains(mapped) {
            return None;
        }
        if let Some(filter) = &self.filter {
            if !filter(mapped) {
                return None;
            }
        }
        Some(mapped.to_string())
    }
}

/// Fluent constructor (tauri-plugin-window-state `Builder` analogue).
pub struct Builder {
    denylist: HashSet<String>,
    filter: Option<Arc<FilterFn>>,
    skip_initial: HashSet<String>,
    flags: StateFlags,
    map_label: Option<Arc<LabelMapFn>>,
    filename: Option<String>,
    location: StorageLocation,
    explicit_path: bool,
    autosave: bool,
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}

impl Builder {
    pub fn new() -> Self {
        Self {
            denylist: HashSet::new(),
            filter: None,
            skip_initial: HashSet::new(),
            flags: StateFlags::all(),
            map_label: None,
            filename: None,
            location: StorageLocation::CurrentDir,
            explicit_path: false,
            autosave: true,
        }
    }

    pub fn with_state_flags(mut self, flags: StateFlags) -> Self {
        self.flags = flags;
        self
    }

    pub fn with_filename(mut self, filename: impl Into<String>) -> Self {
        self.filename = Some(filename.into());
        self
    }

    /// Exact file, or a directory (filename is joined). Overrides env and [`Self::with_storage`].
    pub fn with_path(mut self, path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        self.location = if path.is_dir() {
            StorageLocation::Directory(path)
        } else {
            StorageLocation::File(path)
        };
        self.explicit_path = true;
        self
    }

    pub fn with_storage(mut self, location: StorageLocation) -> Self {
        let explicit = matches!(
            location,
            StorageLocation::File(_) | StorageLocation::Directory(_)
        );
        self.location = location;
        self.explicit_path = explicit;
        self
    }

    pub fn with_denylist(mut self, labels: &[&str]) -> Self {
        self.denylist = labels.iter().map(|l| (*l).to_string()).collect();
        self
    }

    pub fn with_filter<F>(mut self, filter: F) -> Self
    where
        F: Fn(&str) -> bool + Send + Sync + 'static,
    {
        self.filter = Some(Arc::new(filter));
        self
    }

    pub fn skip_initial_state(mut self, label: &str) -> Self {
        self.skip_initial.insert(label.into());
        self
    }

    pub fn map_label<F>(mut self, map: F) -> Self
    where
        F: Fn(&str) -> String + Send + Sync + 'static,
    {
        self.map_label = Some(Arc::new(map));
        self
    }

    /// Write TOML on each window close as well as process exit (default `true`).
    pub fn with_autosave(mut self, enabled: bool) -> Self {
        self.autosave = enabled;
        self
    }

    pub fn build(self) -> Result<WindowStateManager> {
        let filename = self
            .filename
            .unwrap_or_else(|| DEFAULT_FILENAME.to_string());
        let env_override = if self.explicit_path {
            None
        } else {
            std::env::var(ENV_PATH).ok()
        };
        let path = resolve_storage_path(&self.location, &filename, env_override.as_deref())?;
        let doc = WindowStateFile::load(&path)?;
        let windows = doc.windows.into_iter().collect();
        Ok(WindowStateManager {
            inner: Mutex::new(Inner {
                path,
                filename,
                windows,
                denylist: self.denylist,
                skip_initial: self.skip_initial,
                filter: self.filter,
                map_label: self.map_label,
            }),
            restoring: Mutex::new(()),
            flags: self.flags,
            autosave: self.autosave,
        })
    }

    /// Load + register on the Jadix event loop (feature `jadix`).
    #[cfg(feature = "jadix")]
    pub fn install(self) -> Result<std::sync::Arc<WindowStateManager>> {
        crate::jadix_hook::install(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::MonitorRect;
    use std::cell::Cell;
    use std::fs;

    #[derive(Default)]
    struct MockWindow {
        size: Cell<(u32, u32)>,
        pos: Cell<(i32, i32)>,
        maximized: Cell<bool>,
        minimized: Cell<bool>,
        visible: Cell<bool>,
        decorated: Cell<bool>,
        fullscreen: Cell<bool>,
        monitors: Vec<MonitorRect>,
    }

    impl MockWindow {
        fn desktop() -> Self {
            Self {
                size: Cell::new((800, 600)),
                pos: Cell::new((40, 50)),
                visible: Cell::new(true),
                decorated: Cell::new(true),
                monitors: vec![MonitorRect {
                    x: 0,
                    y: 0,
                    width: 1920,
                    height: 1080,
                }],
                ..Self::default()
            }
        }
    }

    impl WindowView for MockWindow {
        fn inner_size(&self) -> (u32, u32) {
            self.size.get()
        }
        fn outer_position(&self) -> Option<(i32, i32)> {
            Some(self.pos.get())
        }
        fn is_maximized(&self) -> bool {
            self.maximized.get()
        }
        fn is_minimized(&self) -> bool {
            self.minimized.get()
        }
        fn is_visible(&self) -> bool {
            self.visible.get()
        }
        fn is_decorated(&self) -> bool {
            self.decorated.get()
        }
        fn is_fullscreen(&self) -> bool {
            self.fullscreen.get()
        }
        fn available_monitors(&self) -> Vec<MonitorRect> {
            self.monitors.clone()
        }
        fn set_outer_position(&self, x: i32, y: i32) {
            self.pos.set((x, y));
        }
        fn set_inner_size(&self, width: u32, height: u32) {
            self.size.set((width, height));
        }
        fn set_maximized(&self, maximized: bool) {
            self.maximized.set(maximized);
        }
        fn set_fullscreen(&self, fullscreen: bool) {
            self.fullscreen.set(fullscreen);
        }
        fn set_decorations(&self, decorated: bool) {
            self.decorated.set(decorated);
        }
        fn set_visible(&self, visible: bool) {
            self.visible.set(visible);
        }
        fn focus(&self) {}
    }

    fn unique_path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "jadix-ws-mgr-{}-{}.toml",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn restore_skips_placeholder_then_captures() {
        let path = unique_path();
        let mgr = Builder::new()
            .with_path(&path)
            .with_autosave(false)
            .build()
            .unwrap();
        let w = MockWindow::desktop();
        mgr.restore("main", &w).unwrap();
        let s = mgr.get("main").unwrap();
        assert_eq!((s.width, s.height), (800, 600));
        assert_eq!((s.x, s.y), (40, 50));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn restore_applies_saved_geometry() {
        let path = unique_path();
        let mgr = Builder::new()
            .with_path(&path)
            .with_autosave(false)
            .build()
            .unwrap();
        let src = MockWindow::desktop();
        src.size.set((1440, 900));
        src.pos.set((80, 60));
        mgr.capture("main", &src).unwrap();
        mgr.save().unwrap();

        let mgr2 = Builder::new()
            .with_path(&path)
            .with_autosave(false)
            .build()
            .unwrap();
        let dst = MockWindow::desktop();
        mgr2.restore("main", &dst).unwrap();
        assert_eq!(dst.size.get(), (1440, 900));
        assert_eq!(dst.pos.get(), (80, 60));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn maximized_restore_uses_prev_origin() {
        let path = unique_path();
        let mgr = Builder::new()
            .with_path(&path)
            .with_autosave(false)
            .build()
            .unwrap();
        mgr.note_moved("main", 120, 80);
        mgr.note_moved("main", 0, 0);
        mgr.note_resized("main", 1280, 720, false, false);
        {
            let mut g = mgr.lock();
            if let Some(s) = g.windows.get_mut("main") {
                s.maximized = true;
            }
        }
        let dst = MockWindow::desktop();
        mgr.restore("main", &dst).unwrap();
        assert!(dst.maximized.get());
        assert_eq!(dst.pos.get(), (120, 80));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn denylist_skips_tracking() {
        let path = unique_path();
        let mgr = Builder::new()
            .with_path(&path)
            .with_denylist(&["splash"])
            .with_autosave(false)
            .build()
            .unwrap();
        let w = MockWindow::desktop();
        mgr.restore("splash", &w).unwrap();
        assert!(mgr.get("splash").is_none());
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn map_label_groups_windows() {
        let path = unique_path();
        let mgr = Builder::new()
            .with_path(&path)
            .map_label(|l| {
                if l.starts_with("editor-") {
                    "editor".into()
                } else {
                    l.into()
                }
            })
            .with_autosave(false)
            .build()
            .unwrap();
        let w = MockWindow::desktop();
        w.size.set((100, 100));
        mgr.capture("editor-1", &w).unwrap();
        w.size.set((200, 200));
        mgr.capture("editor-2", &w).unwrap();
        let s = mgr.get("editor-1").unwrap();
        assert_eq!(s.width, 200);
        assert!(mgr.get("editor").is_some());
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn offscreen_position_is_not_applied() {
        let path = unique_path();
        let mgr = Builder::new()
            .with_path(&path)
            .with_autosave(false)
            .build()
            .unwrap();
        let src = MockWindow::desktop();
        src.pos.set((8000, 8000));
        src.size.set((400, 300));
        mgr.capture("main", &src).unwrap();
        let dst = MockWindow::desktop();
        dst.pos.set((10, 10));
        mgr.restore("main", &dst).unwrap();
        assert_eq!(dst.pos.get(), (10, 10));
        assert_eq!(dst.size.get(), (400, 300));
        let _ = fs::remove_file(&path);
    }
}
