use super::*;

#[test]
fn registry_lookup() {
    register_cursor_rgba8("test-cur", 1, 1, vec![10, 20, 30, 40]);
    let d = lookup_registered("test-cur").unwrap();
    assert_eq!(d.rgba, vec![10, 20, 30, 40]);
}

#[test]
fn hotspot_out_of_bounds_skips() {
    let d = DecodedCursor {
        width: 8,
        height: 8,
        rgba: vec![0; 8 * 8 * 4],
        intrinsic_hotspot: None,
        animation: None,
    };
    let img = CursorUrl {
        src: "x".into(),
        hotspot: Some((8, 0)),
    };
    assert!(resolve_hotspot(&img, &d).is_none());
    let img = CursorUrl {
        src: "x".into(),
        hotspot: Some((0, 0)),
    };
    assert_eq!(resolve_hotspot(&img, &d), Some((0, 0)));
}
