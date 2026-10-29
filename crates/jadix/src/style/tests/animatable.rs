use super::*;
use crate::layout::track_fr;

#[test]
fn grid_template_rows_fr_lerps() {
    let mut style = Style::default();
    style.grid_template_rows = vec![track_fr(0.0)];
    let text = TextStyle::default();
    let a = read_value(&style, &text, PropertyId::GridTemplateRows);
    style.grid_template_rows = vec![track_fr(1.0)];
    let b = read_value(&style, &text, PropertyId::GridTemplateRows);
    let mid = interpolate(&a, &b, 0.5).expect("fr tracks interpolate");
    match mid {
        AnimatableValue::Scalar(x) => assert!((x - 0.5).abs() < 1e-4, "got {x}"),
        other => panic!("expected scalar, got {other:?}"),
    }
    let mut out = Style::default();
    let mut t = TextStyle::default();
    write_value(&mut out, &mut t, PropertyId::GridTemplateRows, &mid);
    assert!((first_track_fr(&out.grid_template_rows).unwrap() - 0.5).abs() < 1e-4);
}

#[test]
fn grid_template_equal_length_fr_list_lerps() {
    let mut style = Style::default();
    style.grid_template_rows = vec![track_fr(0.0), track_fr(0.0)];
    let text = TextStyle::default();
    let a = read_value(&style, &text, PropertyId::GridTemplateRows);
    style.grid_template_rows = vec![track_fr(1.0), track_fr(1.0)];
    let b = read_value(&style, &text, PropertyId::GridTemplateRows);
    let mid = interpolate(&a, &b, 0.5).expect("equal-length fr lists interpolate");
    match mid {
        AnimatableValue::FrList(ref v) => {
            assert_eq!(v.len(), 2);
            assert!((v[0] - 0.5).abs() < 1e-4 && (v[1] - 0.5).abs() < 1e-4);
        }
        other => panic!("expected FrList, got {other:?}"),
    }
    let mut out = Style::default();
    let mut t = TextStyle::default();
    write_value(&mut out, &mut t, PropertyId::GridTemplateRows, &mid);
    assert_eq!(
        tracks_fr_list(&out.grid_template_rows).as_deref(),
        Some(&[0.5, 0.5][..])
    );
}

#[test]
fn compose_add_opacity_and_transform_scale_multiplies() {
    let o = compose(
        AnimationComposition::Add,
        &AnimatableValue::Scalar(0.3),
        &AnimatableValue::Scalar(0.2),
    );
    match o {
        AnimatableValue::Scalar(x) => assert!((x - 0.5).abs() < 1e-4),
        other => panic!("{other:?}"),
    }
    let t = compose(
        AnimationComposition::Accumulate,
        &AnimatableValue::Transform([10.0, 0.0, 2.0, 2.0, 15.0]),
        &AnimatableValue::Transform([5.0, 1.0, 0.5, 0.5, 5.0]),
    );
    match t {
        AnimatableValue::Transform(v) => {
            assert!((v[0] - 15.0).abs() < 1e-4);
            assert!((v[2] - 1.0).abs() < 1e-4);
            assert!((v[4] - 20.0).abs() < 1e-4);
        }
        other => panic!("{other:?}"),
    }
}
