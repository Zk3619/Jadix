use super::*;

#[test]
fn parse_none() {
    assert!(parse_transform("none").is_none());
}

#[test]
fn parse_rotate_deg() {
    let list = parse_transform("rotate(45deg)").unwrap();
    assert_eq!(list.len(), 1);
    assert!(
        matches!(list[0], TransformFunction::Rotate(CssAngle::Deg(d)) if (d - 45.0).abs() < 1e-6)
    );
}

#[test]
fn parse_multi_functions() {
    let list = parse_transform("translateX(10px) rotate(90deg) scale(2, 0.5)").unwrap();
    assert_eq!(list.len(), 3);
    assert!(
        matches!(&list[0], TransformFunction::TranslateX(CssDim::Px(v)) if (*v - 10.0).abs() < 1e-6)
    );
    assert!(
        matches!(&list[1], TransformFunction::Rotate(CssAngle::Deg(d)) if (*d - 90.0).abs() < 1e-6)
    );
    assert!(
        matches!(&list[2], TransformFunction::Scale(sx, Some(sy)) if (*sx - 2.0).abs() < 1e-6 && (*sy - 0.5).abs() < 1e-6)
    );
}

#[test]
fn parse_matrix() {
    let list = parse_transform("matrix(1, 0, 0, 1, 10, 20)").unwrap();
    assert_eq!(list.len(), 1);
    if let TransformFunction::Matrix(v) = &list[0] {
        assert!((v[4] - 10.0).abs() < 1e-6);
        assert!((v[5] - 20.0).abs() < 1e-6);
    } else {
        panic!("expected Matrix");
    }
}

#[test]
fn parse_skew() {
    let list = parse_transform("skewX(30deg)").unwrap();
    assert!(
        matches!(&list[0], TransformFunction::SkewX(CssAngle::Deg(d)) if (*d - 30.0).abs() < 1e-6)
    );
}

#[test]
fn resolve_translate_origin() {
    let list = vec![TransformFunction::TranslateX(CssDim::Px(10.0))];
    let origin = TransformOrigin::default(); // center
    let m = resolve_transform_matrix(&list, &origin, 100.0, 100.0);
    // With center origin, translateX should just add tx
    assert!((m[4] - 10.0).abs() < 1e-3);
    assert!(m[5].abs() < 1e-3);
}

#[test]
fn resolve_rotate_45() {
    let list = vec![TransformFunction::Rotate(CssAngle::Deg(45.0))];
    let origin = TransformOrigin {
        x: TransformOriginX::Length(CssDim::Px(0.0)),
        y: TransformOriginY::Length(CssDim::Px(0.0)),
        z: 0.0,
    };
    let m = resolve_transform_matrix(&list, &origin, 100.0, 100.0);
    let cos45 = 45.0f32.to_radians().cos();
    assert!((m[0] - cos45).abs() < 1e-3);
}

#[test]
fn center_pivot_origin_tx_ty_is_zero_at_center() {
    let r = mat2d_rotate(45.0f32.to_radians());
    let txty = mat2d_center_pivot_origin_tx_ty(&r, [50.0, 50.0], [100.0, 100.0]);
    assert!(
        txty[0].abs() < 1e-4 && txty[1].abs() < 1e-4,
        "center origin must not add shader translation, got {txty:?}"
    );
}

#[test]
fn center_pivot_origin_tx_ty_left_top_rotate_90() {
    // y-down rotate 90°: (x,y) → (-y, x). About top-left, box center (50,50)
    // maps to (-50, 50). Shader: center + txty = (50,50)+txty must match.
    let r = mat2d_rotate(90.0f32.to_radians());
    let txty = mat2d_center_pivot_origin_tx_ty(&r, [0.0, 0.0], [100.0, 100.0]);
    assert!(
        (txty[0] + 100.0).abs() < 1e-3 && txty[1].abs() < 1e-3,
        "left-top 90° on 100×100 → txty (-100, 0), got {txty:?}"
    );
    let world = mat2d_world_center_pivot(&MAT2D_IDENTITY, &r, [0.0, 0.0], [100.0, 100.0]);
    assert!((world[4] + 100.0).abs() < 1e-3 && world[5].abs() < 1e-3);
    let identity_txty =
        mat2d_center_pivot_origin_tx_ty(&MAT2D_IDENTITY, [0.0, 0.0], [100.0, 100.0]);
    assert!(
        identity_txty[0].abs() < 1e-4 && identity_txty[1].abs() < 1e-4,
        "identity linear must not shift a non-center origin, got {identity_txty:?}"
    );
}

#[test]
fn scale_box_about_origin_center_matches_old_inset() {
    let center = scale_box_about_origin([10.0, 20.0, 100.0, 40.0], [50.0, 20.0], [2.0, 2.0]);
    assert!((center[0] - (10.0 - 50.0)).abs() < 1e-4);
    assert!((center[1] - (20.0 - 20.0)).abs() < 1e-4);
    assert!((center[2] - 200.0).abs() < 1e-4 && (center[3] - 80.0).abs() < 1e-4);
    let left_top = scale_box_about_origin([10.0, 20.0, 100.0, 40.0], [0.0, 0.0], [2.0, 2.0]);
    assert!(
        (left_top[0] - 10.0).abs() < 1e-4 && (left_top[1] - 20.0).abs() < 1e-4,
        "left-top origin must keep the top-left fixed, got {left_top:?}"
    );
    assert!((left_top[2] - 200.0).abs() < 1e-4);
    assert!(
        (left_top[0] - center[0]).abs() > 1.0,
        "left-top and center scale must place the box differently"
    );
}

#[test]
fn aabb_of_centered_transform_identity_keeps_rect() {
    let r = [10.0, 20.0, 100.0, 40.0];
    let out = aabb_of_centered_transform(r, &MAT2D_IDENTITY);
    for i in 0..4 {
        assert!(
            (out[i] - r[i]).abs() < 1e-4,
            "identity AABB {out:?} vs {r:?}"
        );
    }
}

#[test]
fn aabb_of_centered_transform_left_top_rotate_90() {
    let r90 = mat2d_rotate(90.0f32.to_radians());
    let m = mat2d_world_center_pivot(&MAT2D_IDENTITY, &r90, [0.0, 0.0], [100.0, 100.0]);
    let aabb = aabb_of_centered_transform([0.0, 0.0, 100.0, 100.0], &m);
    assert!(
        (aabb[0] + 100.0).abs() < 1e-3 && aabb[1].abs() < 1e-3,
        "left-top 90° AABB should sit at (-100, 0), got {aabb:?}"
    );
    assert!((aabb[2] - 100.0).abs() < 1e-3 && (aabb[3] - 100.0).abs() < 1e-3);
}

#[test]
fn cascade_linear_multiply_composes_rotates() {
    let a = mat2d_rotate(45.0f32.to_radians());
    let b = mat2d_rotate(45.0f32.to_radians());
    let c = mat2d_multiply(&a, &b);
    let expected = mat2d_rotate(90.0f32.to_radians());
    for i in 0..4 {
        assert!(
            (c[i] - expected[i]).abs() < 1e-4,
            "cascade linear mismatch at {i}: {} vs {}",
            c[i],
            expected[i]
        );
    }
    assert!(!mat2d_is_axis_aligned(&a));
    assert!(mat2d_is_axis_aligned(&MAT2D_IDENTITY));
    let (vx, vy) = mat2d_transform_vec(&mat2d_rotate(90.0f32.to_radians()), 1.0, 0.0);
    assert!((vx - 0.0).abs() < 1e-4 && (vy - 1.0).abs() < 1e-4);
}

#[test]
fn parse_origin_keywords() {
    let o = parse_transform_origin("left top");
    assert_eq!(o.x, TransformOriginX::Left);
    assert_eq!(o.y, TransformOriginY::Top);

    let o2 = parse_transform_origin("center");
    assert_eq!(o2.x, TransformOriginX::Center);
    assert_eq!(o2.y, TransformOriginY::Center);
}

#[test]
fn parse_origin_lengths() {
    let o = parse_transform_origin("50% 50%");
    assert!(matches!(o.x, TransformOriginX::Length(CssDim::Percent(p)) if (p - 0.5).abs() < 1e-6));
    assert!(matches!(o.y, TransformOriginY::Length(CssDim::Percent(p)) if (p - 0.5).abs() < 1e-6));
}

#[test]
fn mat2d_inverse_identity() {
    let inv = mat2d_inverse(&MAT2D_IDENTITY);
    assert!((inv[0] - 1.0).abs() < 1e-6);
}

#[test]
fn combined_translate_rotate_scale() {
    let m = compute_combined_transform(
        &[10.0, 20.0],
        45.0,
        &[2.0, 2.0],
        None,
        &TransformOrigin::default(),
        100.0,
        100.0,
    );
    // Should not be identity
    assert!(!mat2d_is_identity(&m));
}

#[test]
fn rotate_and_list_keeps_translate() {
    let list = parse_transform("translate(40px, 10px)").unwrap();
    let m = rotate_and_list_mat2d(0.0, Some(&list), 100.0, 80.0);
    assert!((m[4] - 40.0).abs() < 1e-4 && (m[5] - 10.0).abs() < 1e-4);
    assert!(mat2d_is_identity(&mat2d_linear(&m)));
}
