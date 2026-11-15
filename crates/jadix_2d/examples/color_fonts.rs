//! cargo run -p jadix_2d --example color_fonts -- target/color-fonts.png
use jadix_2d::*;

fn positions(font: &Font2D, text: &str, size: f32, x: f32, y: f32) -> Vec<PositionedGlyph> {
    let mut x = x;
    text.chars()
        .map(|ch| {
            let id = font.glyph_id(ch).unwrap();
            let glyph = PositionedGlyph { id, x, y };
            x += font.advance(id, size).unwrap();
            glyph
        })
        .collect()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args()
        .nth(1)
        .unwrap_or("target/color-fonts.png".into());
    let font = Font2D::new(&include_bytes!("../tests/fonts/Nabla.ttf")[..], 0)?;
    let labels = Font2D::new(
        &include_bytes!("../../jadix_cosmic_text/fonts/Inter-Regular.ttf")[..],
        0,
    )?;
    let mut c = RasterCanvas::new_linear(960, 560);
    c.clear("#101a30");
    let label = |c: &mut RasterCanvas, text: &str, x, y| {
        c.set_fill("#e0e8ff");
        c.fill_glyph_run(
            &GlyphRun::new(&labels, 18., &positions(&labels, text, 18., x, y)).unwrap(),
        );
    };
    label(&mut c, "SCALABLE COLRv1 / CPAL / VARIABLE AXES", 28., 34.);
    for (i, (depth, highlight, palette)) in [(0., 0., 0), (200., 24., 0), (100., 12., 2)]
        .into_iter()
        .enumerate()
    {
        let y = 132. + i as f32 * 112.;
        let varied = font.with_variations(&[(*b"EDPT", depth), (*b"EHLT", highlight)])?;
        let run = ColorGlyphRun::new(
            &varied,
            64.,
            &positions(&varied, "NABLA 2D", 64., 28., y),
            ColorGlyphOptions {
                palette,
                ..Default::default()
            },
        )?;
        run.draw(&mut c)?;
        label(
            &mut c,
            &format!("EDPT {depth:.0} / EHLT {highlight:.0} / palette {palette}"),
            590.,
            y - 20.,
        );
        assert_eq!(run.bitmap_bytes(), 0);
    }
    let stops = GradientStops::new(&[(0., [1., 0., 0., 1.]), (1., [0., 0., 1., 1.])])?;
    for (i, (space, text)) in [
        (GradientSpace::LinearSrgb, "LINEAR SRGB"),
        (GradientSpace::Srgb, "SRGB"),
        (GradientSpace::Oklab, "OKLAB"),
    ]
    .into_iter()
    .enumerate()
    {
        let x = 28. + i as f32 * 312.;
        label(&mut c, text, x, 432.);
        c.set_fill_brush(Brush2D::vector(
            VectorGradient::linear([x, 0.], [x + 280., 0.], stops.clone())?.with_color_space(space),
        ));
        c.fill_rect(x, 450., 280., 70.);
    }
    let path = std::path::Path::new(&output);
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    image::save_buffer(
        path,
        &c.get_image_data(0, 0, 960, 560).unwrap(),
        960,
        560,
        image::ColorType::Rgba8,
    )?;
    println!("{}", path.display());
    Ok(())
}
