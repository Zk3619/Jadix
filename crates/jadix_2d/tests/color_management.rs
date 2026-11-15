use jadix_2d::*;

#[test]
fn css_literal_spaces_missing_components_and_gamut_mapping() {
    for css in [
        "rebeccapurple",
        "#1234",
        "rgb(255 0 0 / 50%)",
        "hsl(120deg 100% 50%)",
        "hwb(0 0% 0%)",
        "lab(50% 20 -20)",
        "lch(50% 40 300)",
        "oklab(0.6 0.1 -0.1)",
        "oklch(0.6 0.2 120 / none)",
        "color(display-p3 1 0 0)",
        "color(rec2020 1 0 0)",
        "color(a98-rgb .2 .3 .4)",
        "color(prophoto-rgb .2 .3 .4)",
        "color(xyz-d50 .3 .4 .5)",
        "color(srgb-linear 2 -.1 .2)",
    ] {
        assert!(Color2D::parse(css).is_ok(), "{css}");
    }
    assert!(Color2D::parse("rgb(1 2 3) trailing").is_err());
    let red = Color2D::parse("color(display-p3 1 0 0)").unwrap();
    let linear = red.linear_rgba();
    assert!(linear[0] > 1.2 && linear[1] < 0. && linear[2] < 0.);
    let mapped = red
        .map_to(ColorSpaceTag::Srgb, GamutMapping::OklchChroma)
        .unwrap();
    assert!(mapped.0.components.iter().all(|v| (0. ..=1.).contains(v)));
    assert_ne!(
        mapped,
        red.map_to(ColorSpaceTag::Srgb, GamutMapping::Clip).unwrap()
    );
    assert!(red
        .map_to(ColorSpaceTag::Oklab, GamutMapping::Clip)
        .is_none());
}

#[test]
fn hdr_images_brushes_nested_groups_and_float_export_preserve_range() {
    let mut c = RasterCanvas::new_hdr(16, 16);
    c.push_layer_with_options(LayerOptions {
        compose: CompositeMode::Copy,
        filters: vec![LayerFilter::Gaussian { sigma: [0.5; 2] }],
        ..Default::default()
    });
    c.set_fill_brush(
        Color2D::new(ColorSpaceTag::LinearSrgb, [4., -0.25, 0.5, 1.])
            .unwrap()
            .brush()
            .unwrap(),
    );
    c.fill_rect(0., 0., 16., 16.);
    c.pop_layer();
    let image = Image2D::from_linear_rgba32f(1, 1, &[2., -0.1, 0.25, 0.5]).unwrap();
    c.draw_image(&CanvasImageSource::Image(&image), 4., 4., 8., 8.);
    let p = c.into_linear_rgba32f();
    let p = &p[(8 * 16 + 8) * 4..][..4];
    assert!(
        (p[0] - 3.).abs() < 0.002 && (p[1] + 0.175).abs() < 0.002 && (p[3] - 1.).abs() < 0.002,
        "{p:?}"
    );
    assert!(Image2D::from_linear_rgba32f(1, 1, &[f32::NAN, 0., 0., 1.]).is_none());
    let image =
        Image2D::from_color_space(1, 1, &[1., 0., 0., 1.], ColorSpaceTag::DisplayP3).unwrap();
    assert!(image.linear_rgba32f().unwrap()[0] > 1.2);
    let mut c = RasterCanvas::new_hdr(2, 2);
    c.set_fill_brush(
        Color2D::parse("color(display-p3 1 0 0)")
            .unwrap()
            .brush()
            .unwrap(),
    );
    c.fill_rect(0., 0., 2., 2.);
    let output = c.into_color_space(ColorSpaceTag::DisplayP3);
    assert!((output[0] - 1.).abs() < 0.0001 && output[1].abs() < 0.0001);
}

#[cfg(feature = "icc")]
#[test]
fn icc_real_profiles_import_export_and_invalid_input() {
    use jadix_2d::color::moxcms::{ColorProfile, RenderingIntent};
    let p3 = ColorProfile::new_display_p3().encode().unwrap();
    let srgb = ColorProfile::new_srgb().encode().unwrap();
    let transform =
        IccTransform::to_linear_srgb(&p3, RenderingIntent::RelativeColorimetric).unwrap();
    let result = transform.transform(&[1., 0., 0., 0.5]).unwrap();
    assert!(
        result[0] > 1.2 && result[1] < 0. && (result[3] - 0.5).abs() < 0.0001,
        "{result:?}"
    );
    assert!(Image2D::from_linear_rgba32f(1, 1, &result).is_some());
    let export =
        IccTransform::from_linear_srgb(&p3, RenderingIntent::RelativeColorimetric).unwrap();
    let roundtrip = export.transform(&result).unwrap();
    assert!(
        (roundtrip[0] - 1.).abs() < 0.0002
            && roundtrip[1].abs() < 0.0002
            && roundtrip[2].abs() < 0.0002,
        "{roundtrip:?}"
    );
    let identity = IccTransform::new(&srgb, &srgb, RenderingIntent::RelativeColorimetric).unwrap();
    let input = [0.25, 0.5, 0.75, 0.4];
    let output = identity.transform(&input).unwrap();
    assert!(input
        .iter()
        .zip(output)
        .all(|(a, b)| (a - b).abs() < 0.0002));
    assert!(identity.transform(&[f32::NAN; 4]).is_err());
    assert!(identity.transform(&[0.; 3]).is_err());
    assert!(IccTransform::to_linear_srgb(&[0; 128], RenderingIntent::Perceptual).is_err());
}
