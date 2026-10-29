use super::*;

#[test]
fn polygon_evenodd_hit_and_miss() {
    let clip = parse_clip_path("polygon(50% 0%, 100% 100%, 0% 100%)").unwrap();
    assert!(clip.contains_local([100.0, 100.0], [50.0, 60.0]));
    assert!(!clip.contains_local([100.0, 100.0], [10.0, 10.0]));
}

#[test]
fn polygon_equal_vertex_lerp() {
    let a = parse_clip_path("polygon(0% 0%, 100% 0%, 100% 100%, 0% 100%)").unwrap();
    let b = parse_clip_path("polygon(25% 25%, 75% 25%, 75% 75%, 25% 75%)").unwrap();
    let mid = a.interpolate(&b, 0.5);
    match mid {
        ClipPath::Polygon { points, .. } => {
            assert_eq!(points.len(), 4);
            match points[0][0] {
                ClipCoord::Percent(p) => assert!((p - 0.125).abs() < 1e-4),
                other => panic!("{other:?}"),
            }
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn inset_aabb_and_contains() {
    let clip = parse_clip_path("inset(10px 20px 10px 20px)").unwrap();
    let aabb = clip.aabb([0.0, 0.0, 100.0, 80.0]).unwrap();
    assert!((aabb[0] - 20.0).abs() < 1e-3);
    assert!((aabb[2] - 60.0).abs() < 1e-3);
    assert!(clip.contains_point([0.0, 0.0, 100.0, 80.0], [50.0, 40.0]));
    assert!(!clip.contains_point([0.0, 0.0, 100.0, 80.0], [5.0, 5.0]));
}

#[test]
fn circle_contains() {
    let clip = parse_clip_path("circle(50% at 50% 50%)").unwrap();
    assert!(clip.contains_local([100.0, 100.0], [50.0, 50.0]));
    assert!(!clip.contains_local([100.0, 100.0], [99.0, 99.0]));
}

#[test]
fn none_parses() {
    assert!(matches!(parse_clip_path("none"), Some(ClipPath::None)));
}

#[test]
fn rounded_rect_contains_cuts_corners() {
    let box_xywh = [0.0, 0.0, 80.0, 80.0];
    let r = [40.0; 4];
    assert!(rounded_rect_contains(box_xywh, r, [40.0, 40.0]));
    assert!(!rounded_rect_contains(box_xywh, r, [1.0, 1.0]));
    assert!(rounded_rect_contains(box_xywh, r, [40.0, 1.0]));
    assert!(rounded_rect_contains(box_xywh, [0.0; 4], [1.0, 1.0]));
}

#[test]
fn rounded_rect_gpu_poly_is_active() {
    let poly = GpuClipPoly::from_rounded_rect([10.0, 20.0, 80.0, 80.0], [24.0; 4]);
    assert!(poly.is_active());
    assert!(poly.round_rect[2] > 0.5 && poly.round_rect[3] > 0.5);
    assert_eq!(poly.count, 0, "overflow radii must not tessellate into pts");
    let none = GpuClipPoly::from_rounded_rect([0.0, 0.0, 80.0, 80.0], [0.0; 4]);
    assert!(!none.is_active());
}

#[test]
fn rounded_rect_gpu_poly_matches_sdf_contains() {
    let box_xywh = [0.0, 0.0, 80.0, 80.0];
    let r = [32.0; 4];
    let poly = GpuClipPoly::from_rounded_rect(box_xywh, r);
    assert!(poly.contains_point([40.0, 40.0]));
    assert!(poly.contains_point([40.0, 1.0]));
    assert!(!poly.contains_point([1.0, 1.0]));
    assert_eq!(
        poly.contains_point([8.0, 8.0]),
        rounded_rect_contains(box_xywh, r, [8.0, 8.0])
    );
    assert_eq!(std::mem::size_of::<GpuClipPoly>(), 336);
}

#[test]
fn inset_tessellates_when_asked() {
    let clip = parse_clip_path("inset(10px)").unwrap();
    let poly = clip.to_gpu_tessellated([0.0, 0.0, 100.0, 80.0]);
    assert!(poly.is_active());
    assert!(poly.contains_point([50.0, 40.0]));
    assert!(!poly.contains_point([5.0, 5.0]));
    assert!(
        !clip.to_gpu([0.0, 0.0, 100.0, 80.0]).is_active(),
        "identity inset still uses AABB clip_pos, not a poly"
    );
}

#[test]
fn scale_physical_maps_round_rect_into_instance_space() {
    let mut poly = GpuClipPoly::from_rounded_rect([10.0, 20.0, 80.0, 60.0], [12.0; 4]);
    assert!(poly.contains_point([50.0, 50.0]));
    poly.scale_physical(2.0);
    assert!((poly.round_rect[0] - 20.0).abs() < 1e-4);
    assert!((poly.round_rect[2] - 160.0).abs() < 1e-4);
    assert!((poly.round_radii[0] - 24.0).abs() < 1e-4);
    assert!(
        poly.contains_point([100.0, 100.0]),
        "center in physical px must stay inside"
    );
    assert!(
        !poly.contains_point([15.0, 50.0]),
        "a logical-space interior point left of the scaled origin must miss"
    );
    assert!(!poly.contains_point([22.0, 42.0]));
}

#[test]
fn two_round_rects_and() {
    let outer = GpuClipPoly::from_rounded_rect([0.0, 0.0, 100.0, 100.0], [20.0; 4]);
    let inner = GpuClipPoly::from_rounded_rect([10.0, 10.0, 40.0, 40.0], [8.0; 4]);
    let both = GpuClipPoly::and_layers(outer, inner);
    assert!(both.contains_point([30.0, 30.0]));
    assert!(!both.contains_point([50.0, 50.0]));
    assert!(!both.contains_point([11.0, 11.0]));
}

#[test]
fn path_fn_tessellates_to_polygon() {
    let clip = parse_clip_path(r#"path("M 0 0 L 100 0 L 100 100 L 0 100 Z")"#).unwrap();
    assert!(clip.contains_local([100.0, 100.0], [50.0, 50.0]));
    assert!(!clip.contains_local([100.0, 100.0], [150.0, 50.0]));
}

#[test]
fn url_clip_lookup() {
    begin_named_clip_pass();
    register_clip_path(
        "hole",
        parse_clip_path("polygon(25% 25%, 75% 25%, 75% 75%, 25% 75%)").unwrap(),
    );
    let clip = parse_clip_path("url(#hole)").unwrap();
    assert!(clip.contains_local([100.0, 100.0], [50.0, 50.0]));
    assert!(!clip.contains_local([100.0, 100.0], [5.0, 5.0]));
}

#[test]
fn two_layer_gpu_and() {
    let a = GpuClipPoly::from_world_points(
        &[[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]],
        ClipFillRule::EvenOdd,
    );
    let b = GpuClipPoly::from_world_points(
        &[[40.0, 40.0], [60.0, 40.0], [60.0, 60.0], [40.0, 60.0]],
        ClipFillRule::EvenOdd,
    );
    let both = GpuClipPoly::and_layers(a, b);
    assert!(both.contains_point([50.0, 50.0]));
    assert!(!both.contains_point([10.0, 10.0]));
}

#[test]
fn overflow_box_axis_rect_is_active_without_radii() {
    let poly = GpuClipPoly::from_overflow_box(
        [10.0, 20.0, 200.0, 120.0],
        [0.0; 4],
        &super::super::transform::MAT2D_IDENTITY,
    );
    assert!(poly.is_active());
    assert!(poly.contains_point([11.0, 21.0]));
    assert!(poly.contains_point([209.0, 139.0]));
    assert!(!poly.contains_point([9.0, 80.0]));
    assert!(!poly.contains_point([100.0, 141.0]));
    assert_eq!(poly.count, 0, "axis overflow must use the round-rect slot");
}

#[test]
fn overflow_box_rotated_keeps_diamond_tip() {
    let m = super::super::transform::mat2d_linear(&super::super::transform::mat2d_rotate(
        std::f32::consts::FRAC_PI_4,
    ));
    let poly = GpuClipPoly::from_overflow_box([0.0, 0.0, 80.0, 80.0], [0.0; 4], &m);
    assert!(poly.is_active());
    assert!(poly.count >= 3, "rotated overflow must tessellate a quad");
    let c = [40.0, 40.0];
    let d = [4.0 - c[0], 4.0 - c[1]];
    let (rx, ry) = super::super::transform::mat2d_transform_vec(&m, d[0], d[1]);
    let tip = [c[0] + rx, c[1] + ry];
    assert!(
        poly.contains_point(tip),
        "diamond vertex must stay, tip={tip:?}"
    );
    assert!(
        !poly.contains_point([80.0, 0.0]),
        "layout AABB corner must drop"
    );
}
