use jadix_lucide::{glyph, names, svg_inner};

#[test]
fn unified_catalog_resolves_names_without_a_data_package() {
    assert!(names().count() > 1000);
    assert_eq!(glyph("check").unwrap().chars().count(), 1);
    assert!(glyph("not-a-lucide-icon").is_none());
}

#[test]
fn svg_paths_follow_the_selected_feature() {
    assert_eq!(svg_inner("sun").is_some(), cfg!(feature = "paths"));
    assert!(svg_inner("not-a-lucide-icon").is_none());
}

#[cfg(feature = "font")]
#[test]
fn font_bytes_are_part_of_the_unified_package() {
    assert!(jadix_lucide::FONT_DATA.len() > 100_000);
    assert_eq!(&jadix_lucide::FONT_DATA[..4], &[0, 1, 0, 0]);
}
