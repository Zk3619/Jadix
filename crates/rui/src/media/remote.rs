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
    if !is_remote_src(url) {
        return Err("not an http(s) URL".into());
    }
    let fetcher = FETCHER
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .ok_or_else(|| {
            "no RemoteAssetFetcher installed (call rui::set_remote_asset_fetcher)".to_string()
        })?;
    fetcher.fetch(url)
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
mod tests {
    use super::*;

    #[test]
    fn detects_http_and_https() {
        assert!(is_remote_src("https://example.com/a.png"));
        assert!(is_remote_src(" HTTP://Example.COM/a.png "));
        assert!(!is_remote_src("data:image/png;base64,xx"));
        assert!(!is_remote_src("assets/a.png"));
        assert!(!is_remote_src("file:///tmp/a.png"));
        assert!(!is_remote_src(""));
    }

    #[test]
    fn feature_flag_matches_cfg() {
        assert_eq!(remote_asset_enabled(), cfg!(feature = "remote-asset"));
    }

    #[test]
    fn fetch_without_fetcher_errors() {
        let _guard = FETCHER_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_remote_asset_fetcher();
        let err = fetch_remote_bytes("https://example.com/a.png").unwrap_err();
        assert!(err.contains("RemoteAssetFetcher"));
    }

    #[test]
    fn installed_closure_is_the_client() {
        with_remote_fetcher(
            |url: &str| {
                assert_eq!(url, "https://cdn.example/a.png");
                Ok(b"PNG".to_vec())
            },
            || {
                assert!(remote_asset_fetcher_installed());
                assert_eq!(
                    fetch_remote_bytes("https://cdn.example/a.png").unwrap(),
                    b"PNG"
                );
            },
        );
        assert!(!remote_asset_fetcher_installed());
    }
}
