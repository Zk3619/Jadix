use super::*;

#[test]
fn glyph_asset_is_valid_and_every_entry_is_resolvable() {
    let parsed: HashMap<String, String> =
        serde_json::from_str(include_str!("../../assets/lucide.json")).unwrap();
    assert!(parsed.len() > 1000);
    assert_eq!(names().count(), parsed.len());
    for name in parsed.keys() {
        assert_eq!(glyph(name).unwrap().chars().count(), 1);
    }
    assert!(glyph("not-a-lucide-icon").is_none());
}

#[test]
fn path_asset_tracks_feature_and_legacy_alias() {
    #[cfg(any(test, feature = "paths"))]
    {
        assert!(svg_inner("sun").is_some());
        assert_eq!(svg_inner("unlock"), svg_inner("lock-open"));
    }
    #[cfg(not(any(test, feature = "paths")))]
    assert!(svg_inner("sun").is_none());
    assert!(svg_inner("not-a-lucide-icon").is_none());
}

#[cfg(any(test, feature = "font"))]
#[test]
fn font_is_shipped_as_a_complete_sfnt() {
    assert!(FONT_DATA.len() > 100_000);
    assert_eq!(&FONT_DATA[..4], &[0, 1, 0, 0]);
}
