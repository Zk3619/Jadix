#![cfg(feature = "text-layout")]
use jadix_2d::text_layout::cosmic_text::{self as cosmic, *};
use jadix_2d::*;

fn fonts() -> FontSystem {
    let mut f = FontSystem::new_with_locale_and_db("en-US".into(), cosmic::fontdb::Database::new());
    f.db_mut()
        .load_font_data(include_bytes!("fonts/axes-mini.ttf").to_vec());
    f.db_mut()
        .load_font_data(include_bytes!("fonts/Nabla.ttf").to_vec());
    f.db_mut().set_sans_serif_family("Jadix Axes Fixture");
    f
}
fn ink(text: &PreparedText2D) -> Vec<u8> {
    let mut c = RasterCanvas::new_linear(192, 96);
    text.try_draw(&mut c, [8., 4.]).unwrap();
    let expected = c.get_image_data(0, 0, 192, 96).unwrap();
    let mut g = GpuCanvas::new_linear(192, 96, None);
    text.try_draw(&mut g, [8., 4.]).unwrap();
    assert_eq!(g.get_image_data(0, 0, 192, 96).unwrap(), expected);
    expected
}
#[test]
fn canonical_axes_and_cache_identity() {
    let a = FontVariations::new(&[(*b"wdth", 50.), (*b"wght", 900.)]).unwrap();
    assert_eq!(
        a,
        FontVariations::new(&[(*b"wght", 900.), (*b"wdth", 50.)]).unwrap()
    );
    assert_eq!(
        FontVariations::new(&[(*b"opsz", -0.)]),
        FontVariations::new(&[(*b"opsz", 0.)])
    );
    assert_eq!(
        FontVariations::new(&[(*b"wdth", f32::NAN)]),
        Err(VariationError::InvalidValue)
    );
    assert_eq!(
        FontVariations::new(&[(*b"wdth", 1.), (*b"wdth", 2.)]),
        Err(VariationError::DuplicateTag)
    );
    assert_eq!(
        FontVariations::new(&[(*b"a\0bc", 1.)]),
        Err(VariationError::InvalidTag)
    );
    assert_eq!(
        FontVariations::new(&[(*b"aaaa", 0.); 17]),
        Err(VariationError::TooManyAxes)
    );
    let mut f = fonts();
    let id = f
        .db()
        .faces()
        .find(|face| face.families[0].0 == "Jadix Axes Fixture")
        .unwrap()
        .id;
    let narrow = f.get_font_with_variations(id, Weight(400), a).unwrap();
    let wide = f
        .get_font_with_variations(
            id,
            Weight(400),
            FontVariations::new(&[(*b"wdth", 200.)]).unwrap(),
        )
        .unwrap();
    assert_ne!(narrow.normalized_coords(), wide.normalized_coords());
    assert!(std::sync::Arc::ptr_eq(
        &narrow,
        &f.get_font_with_variations(id, Weight(400), a).unwrap()
    ));
}
#[test]
fn shaping_metrics_outlines_atlas_and_cached_locations_stay_in_sync() {
    let mut f = fonts();
    let mut cache = SwashCache::new();
    let mut b = Buffer::new(&mut f, Metrics::new(40., 52.));
    b.set_size(Some(192.), Some(96.));
    let mut retained = Vec::new();
    for shaping in [Shaping::Advanced, Shaping::Basic] {
        for mode in [TextRenderMode::Outline, TextRenderMode::Atlas] {
            let mut results = Vec::new();
            for width in [50., 200., 50.] {
                let axes =
                    FontVariations::new(&[(*b"wdth", width), (*b"wght", 900.), (*b"NONE", 123.)])
                        .unwrap();
                b.set_text(
                    "AA",
                    &Attrs::new()
                        .family(Family::Name("Jadix Axes Fixture"))
                        .weight(Weight(100))
                        .font_variations(axes),
                    shaping,
                    None,
                );
                let p = PreparedText2D::new(
                    &mut f,
                    &mut b,
                    &mut cache,
                    TextPrepareOptions {
                        mode,
                        raster_scale: 2.,
                        ..Default::default()
                    },
                )
                .unwrap();
                assert_eq!(p.glyphs().len(), 2);
                assert!(p
                    .glyphs()
                    .iter()
                    .all(|g| (g.advance - width / 5.).abs() < 0.01 && g.font_variations == axes));
                results.push(ink(&p));
                retained.push(p);
            }
            assert_ne!(results[0], results[1]);
            assert_eq!(results[0], results[2]);
        }
    }
    drop(cache);
    drop(b);
    drop(f);
    assert!(retained.iter().all(|p| ink(p).iter().any(|v| *v > 0)));
}
#[test]
fn rich_spans_split_features_and_axes_before_shaping() {
    let mut f = fonts();
    let mut cache = SwashCache::new();
    let mut b = Buffer::new(&mut f, Metrics::new(40., 52.));
    b.set_size(Some(192.), Some(96.));
    let base = Attrs::new().family(Family::Name("Jadix Axes Fixture"));
    let mut disabled = FontFeatures::new();
    disabled.disable(FeatureTag::new(b"liga"));
    b.set_rich_text(
        [
            ("fi", base.clone()),
            ("fi", base.clone().font_features(disabled)),
        ],
        &base,
        Shaping::Advanced,
        None,
    );
    let p = PreparedText2D::new(&mut f, &mut b, &mut cache, TextPrepareOptions::default()).unwrap();
    assert_eq!(p.glyphs().len(), 3);
    assert_eq!(p.glyphs()[0].cluster, 0..2);
    b.set_rich_text(
        [
            (
                "AA",
                base.clone()
                    .font_variations(FontVariations::new(&[(*b"wdth", 50.)]).unwrap()),
            ),
            (
                "AA",
                base.clone()
                    .font_variations(FontVariations::new(&[(*b"wdth", 200.)]).unwrap()),
            ),
        ],
        &base,
        Shaping::Advanced,
        None,
    );
    let p = PreparedText2D::new(&mut f, &mut b, &mut cache, TextPrepareOptions::default()).unwrap();
    let advances = p.glyphs().iter().map(|g| g.advance).collect::<Vec<_>>();
    assert_eq!(advances, [10., 10., 40., 40.]);
}
#[test]
fn varied_color_fonts_use_the_shaped_instance_in_both_modes() {
    let mut f = fonts();
    let mut cache = SwashCache::new();
    let mut b = Buffer::new(&mut f, Metrics::new(40., 52.));
    b.set_size(Some(192.), Some(96.));
    for mode in [TextRenderMode::Outline, TextRenderMode::Atlas] {
        let mut images = Vec::new();
        for depth in [0., 200., 0.] {
            b.set_text(
                "AB",
                &Attrs::new().family(Family::Name("Nabla")).font_variations(
                    FontVariations::new(&[(*b"EDPT", depth), (*b"EHLT", 24.)]).unwrap(),
                ),
                Shaping::Advanced,
                None,
            );
            let p = PreparedText2D::new(
                &mut f,
                &mut b,
                &mut cache,
                TextPrepareOptions {
                    mode,
                    ..Default::default()
                },
            )
            .unwrap();
            images.push(ink(&p));
        }
        assert_ne!(images[0], images[1]);
        assert_eq!(images[0], images[2]);
    }
}
