use super::*;

#[test]
fn path_midpoint_is_halfway() {
    let mut style = Style::default();
    style.offset_path = parse_offset_path(r#"path("M 0 0 L 100 0")"#).unwrap();
    style.offset_distance = 0.5;
    let s = sample(&style, [100.0, 20.0]);
    assert!((s.translate[0] - 50.0).abs() < 1e-3, "{:?}", s.translate);
    assert!(s.translate[1].abs() < 1e-3);
    assert!(s.rotate_deg.abs() < 1.0);
}
