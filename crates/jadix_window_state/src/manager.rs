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

    #[cfg(feature = "jadix")]
    pub(crate) fn initial_state(&self, label: &str) -> Option<WindowState> {
        let inner = self.lock();
        let label = inner.effective_label(label)?;
        if inner.skip_initial.contains(&label) {
            return None;
        }
        inner
            .windows
            .get(&label)
            .copied()
            .filter(|state| !state.is_placeholder())
    }

    /// Restore `label` onto `window`. Unknown labels insert a live snapshot instead.
    pub fn restore(&self, label: &str, window: &(impl WindowView + ?Sized)) -> Result<()> {
        self.restore_with_flags(label, window, self.flags)
    }

    /// Jadix creates hidden windows; its renderer owns the first show/focus.
    #[cfg(any(feature = "jadix", test))]
    pub(crate) fn restore_before_show(
        &self,
        label: &str,
        window: &(impl WindowView + ?Sized),
    ) -> Result<()> {
        self.restore_with_flags(label, window, self.flags.difference(StateFlags::VISIBLE))
    }

    fn restore_with_flags(
        &self,
        label: &str,
        window: &(impl WindowView + ?Sized),
        flags: StateFlags,
    ) -> Result<()> {
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
            state.apply_to(window, flags);
        } else {
            self.capture(&mapped, window)?;
        }
        Ok(())
    }

    /// Write live fields into the cache (does not flush unless `autosave` was set by caller).
    pub fn capture(&self, label: &str, window: &(impl WindowView + ?Sized)) -> Result<()> {
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
#[path = "tests/manager.rs"]
mod tests;
