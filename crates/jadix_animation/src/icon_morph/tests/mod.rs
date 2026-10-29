use super::plan::procrustes;
use super::*;

fn line(x1: f64, y1: f64, x2: f64, y2: f64) -> IconInput {
    IconInput::D(format!("M{x1} {y1}L{x2} {y2}"))
}

#[test]
fn parse_and_canonical_roundtrip_line() {
    let d = canonical_d(&line(0.0, 0.0, 10.0, 0.0)).unwrap();
    assert!(d.starts_with('M'), "{d}");
    assert!(d.contains('C'), "line lowers to cubic: {d}");
}

#[test]
fn endpoints_snap_to_canonical() {
    let a = line(0.0, 12.0, 24.0, 12.0);
    let b = line(12.0, 0.0, 12.0, 24.0);
    let d0 = morph_d_at(&a, &b, 0.0).unwrap();
    let d1 = morph_d_at(&a, &b, 1.0).unwrap();
    assert_eq!(d0, canonical_d(&a).unwrap());
    assert_eq!(d1, canonical_d(&b).unwrap());
}

#[test]
fn mid_flight_is_polyline() {
    let a = line(0.0, 12.0, 24.0, 12.0);
    let b = line(12.0, 0.0, 12.0, 24.0);
    let mid = morph_d_at(&a, &b, 0.5).unwrap();
    assert!(mid.contains('L'), "in-flight d is polyline: {mid}");
    assert!(!mid.is_empty());
}

#[test]
fn rotated_line_is_pure_rotation() {
    let a = resample_icon(&line(0.0, 12.0, 24.0, 12.0), SAMPLE_N).unwrap();
    let b = resample_icon(&line(12.0, 0.0, 12.0, 24.0), SAMPLE_N).unwrap();
    let plan = build_plan(&a, &b).unwrap();
    assert_eq!(plan.items.len(), 1);
    let it = &plan.items[0];
    assert!(
        it.res < 0.05,
        "congruent lines should have tiny residual, got {}",
        it.res
    );
    let deg = it.theta.abs() * 180.0 / std::f64::consts::PI;
    assert!(
        (deg - 90.0).abs() < 5.0,
        "expected ~90° rotation, got {deg}"
    );
    let _ = procrustes(&it.a, &it.b_o, it.ca, it.cb);
}

#[test]
fn unequal_subpaths_cell_division() {
    let one = IconInput::D("M4 12 L20 12".into());
    let two = IconInput::D("M4 8 L20 8 M4 16 L20 16".into());
    let mid = morph_d_at(&one, &two, 0.5).unwrap();
    // Two subpaths in flight (duplicated source).
    assert!(
        mid.matches('M').count() >= 2,
        "cell division keeps both strokes: {mid}"
    );
}

#[test]
fn unordered_alignment_keeps_the_scale_floor() {
    let source = [-1.0, 0.0, 1.0, 0.0];
    let target = [f64::NAN, 0.0, 1.0, 0.0];
    let alignment = procrustes(&source, &target, [0.0, 0.0], [0.0, 0.0]);
    assert!(alignment.theta.is_nan());
    assert_eq!(alignment.sigma, 1e-6);
}

#[test]
fn markup_lucide_inner() {
    let inner = r#"<path d="M4 6h16"/><path d="M4 12h16"/><path d="M4 18h16"/>"#;
    let input = markup_to_icon(inner, true).unwrap();
    let cubics = icon_to_cubics(&input).unwrap();
    assert_eq!(cubics.len(), 3);
}

#[test]
fn fill_drawn_rejected_when_strict() {
    let svg = r##"<svg viewBox="0 0 24 24"><path d="M0 0h24v24H0z" fill="#000"/></svg>"##;
    assert!(markup_to_icon(svg, false).is_err());
}

#[test]
fn svg_to_icon_fits_viewbox() {
    let svg = r#"<svg viewBox="0 0 24 24"><path d="M4 12h16" fill="none" stroke="black" stroke-width="2"/></svg>"#;
    let input = svg_to_icon(svg).expect("stroke svg");
    let d = canonical_d(&input).expect("d");
    assert!(d.starts_with('M'), "{d}");
}

#[test]
fn linear_mid_differs_from_polar_on_rotation() {
    let a = line(0.0, 12.0, 24.0, 12.0);
    let b = line(12.0, 0.0, 12.0, 24.0);
    let polar = morph_d_at_kind(&a, &b, 0.5, false).unwrap();
    let linear = morph_d_at_kind(&a, &b, 0.5, true).unwrap();
    assert_ne!(polar, linear);
    assert!(polar.contains('L') && linear.contains('L'));
}

#[test]
fn public_canonical_linear_and_cubics_d() {
    let d = cubics_to_path_d(&icon_to_cubics(&line(0.0, 0.0, 10.0, 0.0)).unwrap());
    assert!(d.starts_with('M') && d.contains('C'), "{d}");
    let a = resample_icon(&line(0.0, 12.0, 24.0, 12.0), SAMPLE_N).unwrap();
    let b = resample_icon(&line(12.0, 0.0, 12.0, 24.0), SAMPLE_N).unwrap();
    let plan = build_plan(&a, &b).unwrap();
    let mut out = alloc_outputs(&plan);
    interp_linear(&plan, 0.5, &mut out);
    let closed: Vec<bool> = plan.items.iter().map(|it| it.closed).collect();
    let mid = serialize(&out, &closed);
    assert!(mid.contains('L'), "{mid}");
    assert_eq!(canonical_d(&line(0.0, 0.0, 10.0, 0.0)).unwrap(), d);
}
