use super::parse_box_shadow;

fn approx_eq(a: f32, b: f32) {
    assert!((a - b).abs() < 0.001, "expected {a} ~= {b}");
}

#[test]
fn box_shadow_unsuffixed_defaults_to_rem() {
    let sh = parse_box_shadow("1 0.5 0.25 0.125");
    approx_eq(sh.offset[0], 16.0);
    approx_eq(sh.offset[1], 8.0);
    approx_eq(sh.blur, 4.0);
    approx_eq(sh.spread, 2.0);
}

#[test]
fn box_shadow_supports_px_rem_em_conversion() {
    let sh = parse_box_shadow("4px 0.5rem 0.25em 2px");
    approx_eq(sh.offset[0], 4.0);
    approx_eq(sh.offset[1], 8.0);
    approx_eq(sh.blur, 4.0);
    approx_eq(sh.spread, 2.0);
}

#[test]
fn box_shadows_split_on_comma() {
    let list = super::parse_box_shadows("0 1px 2px rgb(0,0,0), 0 4px 8px #00000080");
    assert_eq!(list.len(), 2);
    approx_eq(list[0].blur, 2.0);
    approx_eq(list[1].blur, 8.0);
}

#[test]
fn line_style_shader_codes_match_wgsl_branches() {
    use super::LineStyle;
    assert_eq!(LineStyle::None.to_shader_f32(), 0.0);
    assert_eq!(LineStyle::Hidden.to_shader_f32(), 0.0);
    assert_eq!(LineStyle::Solid.to_shader_f32(), 1.0);
    assert_eq!(LineStyle::Dotted.to_shader_f32(), 2.0);
    assert_eq!(LineStyle::Dashed.to_shader_f32(), 3.0);
    assert_eq!(LineStyle::Double.to_shader_f32(), 4.0);
    assert_eq!(LineStyle::Groove.to_shader_f32(), 5.0);
    assert_eq!(LineStyle::Ridge.to_shader_f32(), 6.0);
    assert_eq!(LineStyle::Inset.to_shader_f32(), 7.0);
    assert_eq!(LineStyle::Outset.to_shader_f32(), 8.0);
}

#[test]
fn corner_shape_shader_param_is_zero_except_superellipse() {
    use super::CornerShape;
    assert_eq!(CornerShape::Round.shader_param(), 0.0);
    assert_eq!(CornerShape::Angle.shader_param(), 0.0);
    assert_eq!(CornerShape::Notch.shader_param(), 0.0);
    assert_eq!(CornerShape::Scoop.shader_param(), 0.0);
    assert_eq!(CornerShape::Round.to_shader_f32(), 0.0);
    assert_eq!(CornerShape::Angle.to_shader_f32(), 1.0);
    assert_eq!(CornerShape::Superellipse(4.0).to_shader_f32(), 2.0);
    assert_eq!(CornerShape::Notch.to_shader_f32(), 3.0);
    assert_eq!(CornerShape::Scoop.to_shader_f32(), 4.0);
    approx_eq(CornerShape::Superellipse(4.0).shader_param(), 4.0);
    approx_eq(CornerShape::Superellipse(8.0).shader_param(), 8.0);
}
