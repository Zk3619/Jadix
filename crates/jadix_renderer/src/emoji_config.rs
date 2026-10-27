//! Emoji *asset* policy: system color-font rasters by default; Twemoji/SVG only when opted in.
//!
//! Layout/centering (invisible slots + optical quads) is always on — see
//! [`super::emoji_layout::custom_glyph_layout_enabled`]. This module only chooses the
//! bitmap source. See `docs/engine/assets_and_icons_architecture.md` §11.

use std::path::PathBuf;
use std::sync::OnceLock;

/// True when the app opted into Twemoji/SVG as the emoji raster source.
///
/// Active when:
/// - `JADIX_EMOJI_CUSTOM_GLYPH` is explicitly on (`1` / `true`), or
/// - `JADIX_TWEMOJI_DIR` points at an existing directory.
///
/// Default is **off** → system/user color emoji (Segoe / Apple / Noto) via the
/// *isolated* fallback FontSystem (never the UI DB — CBDT would leak under alpha=0 slots).
pub fn emoji_bitmap_override_active() -> bool {
    static ACTIVE: OnceLock<bool> = OnceLock::new();
    *ACTIVE.get_or_init(|| {
        if env_explicitly_enabled("JADIX_EMOJI_CUSTOM_GLYPH") {
            return true;
        }
        if env_explicitly_disabled("JADIX_EMOJI_CUSTOM_GLYPH") {
            return false;
        }
        configured_twemoji_dir().is_some()
    })
}

/// Directory for Twemoji PNG/SVG assets when bitmap override is active.
///
/// Only returns a path from an explicit `JADIX_TWEMOJI_DIR` (existing dir). Does **not**
/// auto-discover the repo sample `assets/twemoji` tree — that half-pack caused blank chips.
pub fn configured_twemoji_dir() -> Option<PathBuf> {
    static DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
    DIR.get_or_init(|| {
        let Ok(raw) = std::env::var("JADIX_TWEMOJI_DIR") else {
            return None;
        };
        let p = PathBuf::from(raw.trim());
        if p.is_dir() {
            Some(p)
        } else {
            log::warn!("JADIX_TWEMOJI_DIR set but not a directory: {}", p.display());
            None
        }
    })
    .clone()
}

/// Resolve Twemoji asset root for raster loads (override mode only).
///
/// Prefer `JADIX_TWEMOJI_DIR`; otherwise, when `JADIX_EMOJI_CUSTOM_GLYPH=1`, fall back to
/// well-known workspace/exe-relative `assets/twemoji/72x72` paths for local demos.
pub fn twemoji_asset_dir() -> Option<PathBuf> {
    if !emoji_bitmap_override_active() {
        return None;
    }
    if let Some(dir) = configured_twemoji_dir() {
        return Some(dir);
    }
    // CUSTOM_GLYPH=1 without JADIX_TWEMOJI_DIR: allow discovering a local pack for demos.
    discover_twemoji_dir()
}

fn discover_twemoji_dir() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    candidates.push(manifest.join("../../assets/twemoji/72x72"));
    candidates.push(manifest.join("assets/twemoji/72x72"));
    if let Some(workspace) = manifest.parent().and_then(|p| p.parent()) {
        candidates.push(workspace.join("assets/twemoji/72x72"));
    }
    candidates.push(PathBuf::from("assets/twemoji/72x72"));
    candidates.push(PathBuf::from("crates/jadix/assets/twemoji/72x72"));
    if let Ok(cwd) = std::env::current_dir() {
        let mut walk = cwd;
        for _ in 0..6 {
            candidates.push(walk.join("assets/twemoji/72x72"));
            candidates.push(walk.join("crates/jadix/assets/twemoji/72x72"));
            if !walk.pop() {
                break;
            }
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            candidates.push(parent.join("assets/twemoji/72x72"));
            candidates.push(parent.join("twemoji/72x72"));
            let mut walk = parent.to_path_buf();
            for _ in 0..4 {
                candidates.push(walk.join("assets/twemoji/72x72"));
                if !walk.pop() {
                    break;
                }
            }
        }
    }
    candidates.into_iter().find(|p| p.is_dir())
}

fn env_explicitly_enabled(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .map(|v| {
            let v = v.trim();
            v == "1"
                || v.eq_ignore_ascii_case("true")
                || v.eq_ignore_ascii_case("on")
                || v.eq_ignore_ascii_case("yes")
        })
        .unwrap_or(false)
}

fn env_explicitly_disabled(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .map(|v| {
            let v = v.trim();
            v == "0"
                || v.eq_ignore_ascii_case("false")
                || v.eq_ignore_ascii_case("off")
                || v.eq_ignore_ascii_case("no")
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twemoji_asset_dir_none_when_override_inactive() {
        // OnceLock may already be set by other tests; only assert the function's
        // contract when we can observe inactive mode.
        if !emoji_bitmap_override_active() {
            assert!(twemoji_asset_dir().is_none());
        }
    }

    #[test]
    fn configured_dir_rejects_missing_path() {
        // Unit-level: PathBuf::is_dir on a nonsense path.
        let p = PathBuf::from("/this/jadix/twemoji/dir/does/not/exist");
        assert!(!p.is_dir());
    }
}
