use super::*;

#[test]
fn named_lookup_nearest_wins() {
    let named = vec![
        ("page".into(), 0.2),
        ("page".into(), 0.8),
        ("other".into(), 0.1),
    ];
    assert!((lookup_named(&named, "--page").unwrap() - 0.8).abs() < 1e-4);
    assert!((lookup_named(&named, "PAGE").unwrap() - 0.8).abs() < 1e-4);
    assert!(lookup_named(&named, "--missing").is_none());
}

#[test]
fn animation_range_maps_progress() {
    let t = map_animation_range(0.3, Some("10%"), Some("90%"));
    assert!((t - 0.25).abs() < 1e-4, "got {t}");
    assert!((map_animation_range(0.0, Some("0%"), Some("100%"))).abs() < 1e-4);
    assert!((map_animation_range(1.0, None, None) - 1.0).abs() < 1e-4);
}
