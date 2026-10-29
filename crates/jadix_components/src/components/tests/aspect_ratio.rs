use super::*;
use crate::core::tree::parse_css_aspect_ratio;

#[test]
fn parse_mdn_aspect_ratio_values() {
    assert_eq!(parse_css_aspect_ratio("auto"), None);
    assert!((parse_css_aspect_ratio("1 / 1").unwrap() - 1.0).abs() < f32::EPSILON);
    assert!((parse_css_aspect_ratio("16/9").unwrap() - 16.0 / 9.0).abs() < 0.001);
    assert!((parse_css_aspect_ratio("0.5").unwrap() - 0.5).abs() < f32::EPSILON);
    assert!((parse_css_aspect_ratio("auto 3/4").unwrap() - 0.75).abs() < f32::EPSILON);
    assert!((parse_css_aspect_ratio("9/6 auto").unwrap() - 1.5).abs() < f32::EPSILON);
}

#[test]
fn builder_sets_slot_and_ratio() {
    let el: Element = aspect_ratio().ratio(16.0 / 9.0).into();
    assert_eq!(
        el.attrs.get(ATTR_SLOT).map(|s| s.as_str()),
        Some(SLOT_ASPECT_RATIO)
    );
    assert!((el.style.aspect_ratio.unwrap() - 16.0 / 9.0).abs() < 0.001);
}

#[test]
fn dual_surface_compiles_shape() {
    let _b: Element = aspect_ratio().ratio("16/9").into();
    let el = AspectRatio(AspectRatioProps {
        id: String::new(),
        ratio: Some(AspectRatioValue::from("1/1")),
        children: None,
        base: None,
        ..Default::default()
    });
    let _: Element = el;
}
