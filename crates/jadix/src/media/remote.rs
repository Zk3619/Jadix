//! Remote `http(s)` bytes for `<Image src>` / Avatar / Icon — **no HTTP client**.
//!
//! Feature `remote-asset` only opens the async Image path. Bytes come from an
//! app-installed [`RemoteAssetFetcher`] (`reqwest` / `ureq` / custom). Sync
//! [`super::load_src`] still refuses `http(s)`.

use std::sync::{Arc, Mutex};

/// True for `http://` / `https://` (after trim; case-insensitive).
pub fn is_remote_src(src: &str) -> bool {
    let lower = src.trim().to_ascii_lowercase();
    lower.starts_with("https://") || lower.starts_with("http://")
}

/// Whether `<Image src="https://…">` will call the installed fetcher (feature on).
pub const fn remote_asset_enabled() -> bool {
    cfg!(feature = "remote-asset")
}

/// App-owned network (or cache) adapter. Install once at startup.
///
/// Blocking is OK: the Image path runs this on a worker via [`crate::app::spawn_blocking`].
pub trait RemoteAssetFetcher: Send + Sync {
    fn fetch(&self, url: &str) -> Result<Vec<u8>, String>;

    /// Share immutable encoded bytes across consumers. Existing adapters inherit
    /// the allocating fallback; cache adapters can return their retained bytes.
    fn fetch_shared(&self, url: &str) -> Result<Arc<[u8]>, String> {
        self.fetch(url).map(Arc::from)
    }
}

impl<F> RemoteAssetFetcher for F
where
    F: Fn(&str) -> Result<Vec<u8>, String> + Send + Sync,
{
    fn fetch(&self, url: &str) -> Result<Vec<u8>, String> {
        (self)(url)
    }
}

static FETCHER: Mutex<Option<Arc<dyn RemoteAssetFetcher>>> = Mutex::new(None);

/// Replace the process-wide fetcher (same pattern as [`crate::set_icon_provider`]).
pub fn set_remote_asset_fetcher(fetcher: impl RemoteAssetFetcher + 'static) {
    *FETCHER.lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(fetcher));
}

pub fn clear_remote_asset_fetcher() {
    *FETCHER.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

/// Whether [`set_remote_asset_fetcher`] has been called.
pub fn remote_asset_fetcher_installed() -> bool {
    FETCHER.lock().unwrap_or_else(|e| e.into_inner()).is_some()
}

/// Shared entry for Image / Icon / custom SVG pipelines.
///
/// Does not perform I/O itself. Errors if the URL is not `http(s)` or no fetcher is installed.
pub fn fetch_remote_bytes(url: &str) -> Result<Vec<u8>, String> {
    let url = url.trim();
    installed_fetcher(url)?.fetch(url)
}

/// Fetch shared encoded bytes on a worker, using the same installed adapter.
pub fn fetch_remote_bytes_shared(url: &str) -> Result<Arc<[u8]>, String> {
    let url = url.trim();
    installed_fetcher(url)?.fetch_shared(url)
}

fn installed_fetcher(url: &str) -> Result<Arc<dyn RemoteAssetFetcher>, String> {
    if !is_remote_src(url) {
        return Err("not an http(s) URL".into());
    }
    FETCHER
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .ok_or_else(|| {
            "no RemoteAssetFetcher installed (call jadix::set_remote_asset_fetcher)".to_string()
        })
}

#[cfg(test)]
pub(crate) static FETCHER_TEST_LOCK: Mutex<()> = Mutex::new(());

#[cfg(test)]
pub(crate) fn with_remote_fetcher<R>(
    fetcher: impl RemoteAssetFetcher + 'static,
    body: impl FnOnce() -> R,
) -> R {
    let _guard = FETCHER_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    set_remote_asset_fetcher(fetcher);
    let out = body();
    clear_remote_asset_fetcher();
    out
}

#[cfg(test)]
#[path = "tests/remote.rs"]
mod tests;
