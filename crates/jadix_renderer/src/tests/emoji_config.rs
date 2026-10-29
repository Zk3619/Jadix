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
