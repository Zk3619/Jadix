use jadix_2d::*;

#[test]
fn incremental_lru_eviction_and_frozen_snapshots() {
    let limits = AtlasLimits {
        page_size: 16,
        max_pages: 1,
        max_entries: 2,
    };
    let mut atlas = AtlasCache::new(limits).unwrap();
    let red = Image2D::from_rgba8(2, 2, &[255, 0, 0, 255].repeat(4)).unwrap();
    let blue = Image2D::from_rgba8(2, 2, &[0, 0, 255, 255].repeat(4)).unwrap();
    let first = atlas.insert("first", &red).unwrap();
    atlas.insert("second", &blue).unwrap();
    atlas.get(&"first").unwrap();
    atlas.insert("third", &blue).unwrap();
    assert!(atlas.get(&"second").is_none());
    assert!(atlas.get(&"first").is_some());
    let mut c = RasterCanvas::new_linear(4, 4);
    first.draw(&mut c, [0., 0., 4., 4.], ImageSampling::Nearest);
    assert_eq!(&c.get_image_data(1, 1, 1, 1).unwrap(), &[255, 0, 0, 255]);
    let stats = atlas.stats();
    assert_eq!((stats.entries, stats.pages, stats.evictions), (2, 1, 1));
    assert_eq!(stats.page_bytes, 1024);
    let float = Image2D::from_linear_rgba32f(1, 1, &[2., 0., 0., 1.]).unwrap();
    assert_eq!(
        atlas.insert("float", &float).unwrap_err(),
        AtlasError::UnsupportedFormat
    );
    assert_eq!(atlas.stats().entries, 2);
    atlas.clear();
    assert_eq!(atlas.stats().page_bytes, 0);
    // Atlas mutation/clear cannot alter an already returned frozen entry.
    assert_eq!(
        first.image.rgba8()[(first.rect[1] * 16 + first.rect[0]) as usize * 4],
        255
    );
}

#[test]
fn offcanvas_filter_input_is_retained_by_explicit_halo() {
    let scene = Scene2D::record([128, 128], |c| {
        c.set_fill("red");
        c.fill_rect(-6., 3., 6., 10.);
    })
    .unwrap();
    let region = SceneRegion {
        origin: [0, 0],
        size: [16, 16],
        halo: 8,
        filters: vec![LayerFilter::Gaussian { sigma: [2.; 2] }],
    };
    let image = region.rasterize(&scene).unwrap();
    assert_eq!((image.image.width(), image.image.height()), (32, 32));
    let mut canvas = RasterCanvas::new_hdr(16, 16);
    image.draw(&mut canvas);
    let pixels = canvas.into_linear_rgba32f();
    assert!(
        pixels[(8 * 16) * 4 + 3] > 0.3,
        "halo must preserve negative-x source before blur"
    );
    assert!(pixels[(8 * 16 + 10) * 4 + 3] < 0.001);
    assert!(SceneRegion {
        halo: u32::MAX,
        ..region.clone()
    }
    .padded_size()
    .is_none());
    assert!(SceneRegion {
        size: [8192; 2],
        ..region
    }
    .padded_size()
    .is_none());
}

#[cfg(feature = "text-layout")]
#[test]
fn incremental_font_pages_share_identity_and_preserve_prepared_text() {
    use jadix_2d::text_layout::cosmic_text::*;
    let mut fonts = FontSystem::new_with_locale_and_db("en-US".into(), fontdb::Database::new());
    fonts
        .db_mut()
        .load_font_data(include_bytes!("fonts/axes-mini.ttf").to_vec());
    let mut buffer = Buffer::new(&mut fonts, Metrics::new(16., 20.));
    buffer.set_size(Some(96.), Some(32.));
    buffer.set_text(
        "AA fi",
        &Attrs::new().family(Family::Name("Jadix Axes Fixture")),
        Shaping::Advanced,
        None,
    );
    let mut swash = SwashCache::new();
    let mut atlas = GlyphAtlasCache::new(AtlasLimits::default()).unwrap();
    let options = TextPrepareOptions {
        mode: TextRenderMode::Atlas,
        ..Default::default()
    };
    let first = PreparedText2D::new_with_atlas_cache(
        &mut fonts,
        &mut buffer,
        &mut swash,
        options,
        &mut atlas,
        42,
    )
    .unwrap();
    let second = PreparedText2D::new_with_atlas_cache(
        &mut fonts,
        &mut buffer,
        &mut swash,
        options,
        &mut atlas,
        42,
    )
    .unwrap();
    assert_eq!(first.atlas().pages(), second.atlas().pages());
    let count = atlas.stats().insertions;
    assert!(count > 0);
    assert_eq!(atlas.stats().evictions, 0);
    atlas.clear();
    drop(fonts);
    drop(swash);
    let paint = |text: &PreparedText2D| {
        let mut c = RasterCanvas::new_linear(96, 32);
        text.draw(&mut c, [0., 0.]);
        c.get_image_data(0, 0, 96, 32).unwrap()
    };
    assert_eq!(paint(&first), paint(&second));
}
