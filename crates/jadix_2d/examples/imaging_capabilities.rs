//! cargo run -p jadix_2d --features text-layout --example imaging_capabilities -- target/imaging.png
use jadix_2d::text_layout::cosmic_text::{self as cosmic, *};
use jadix_2d::*;
fn label(c: &mut RasterCanvas, font: &Font2D, text: &str, x: f32, y: f32) {
    let mut px = x;
    let glyphs = text
        .chars()
        .map(|ch| {
            let id = font.glyph_id(ch).unwrap();
            let g = PositionedGlyph { id, x: px, y };
            px += font.advance(id, 15.).unwrap();
            g
        })
        .collect::<Vec<_>>();
    c.set_fill("#e2e8ff");
    c.fill_glyph_run(&GlyphRun::new(font, 15., &glyphs).unwrap());
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args()
        .nth(1)
        .unwrap_or("target/imaging.png".into());
    let mut fonts =
        FontSystem::new_with_locale_and_db("en-US".into(), cosmic::fontdb::Database::new());
    fonts
        .db_mut()
        .load_font_data(include_bytes!("../tests/fonts/Nabla.ttf").to_vec());
    fonts.db_mut().set_sans_serif_family("Nabla");
    let labels = Font2D::new(
        &include_bytes!("../../jadix_cosmic_text/fonts/Inter-Regular.ttf")[..],
        0,
    )?;
    let mut cache = SwashCache::new();
    let mut c = RasterCanvas::new_linear(960, 620);
    c.clear("#101a30");
    label(
        &mut c,
        &labels,
        "SHAPED VARIABLE COLOR FONTS / NATIVE SCENES",
        24.,
        30.,
    );
    for (i, depth) in [0., 100., 200.].into_iter().enumerate() {
        let mut b = Buffer::new(&mut fonts, Metrics::new(40., 56.));
        b.set_size(Some(280.), Some(100.));
        b.set_text(
            "NABLA",
            &Attrs::new()
                .family(Family::Name("Nabla"))
                .font_variations(FontVariations::new(&[(*b"EDPT", depth), (*b"EHLT", 24.)])?),
            Shaping::Advanced,
            None,
        );
        let p = PreparedText2D::new(
            &mut fonts,
            &mut b,
            &mut cache,
            TextPrepareOptions {
                palette: if i == 2 { 2 } else { 0 },
                ..Default::default()
            },
        )?;
        let x = 24. + i as f32 * 312.;
        p.try_draw(&mut c, [x, 50.])?;
        label(
            &mut c,
            &labels,
            &format!("EDPT {depth:.0} / EHLT 24 / Outline"),
            x,
            126.,
        );
    }
    label(
        &mut c,
        &labels,
        "ISOLATED GROUPS / STRAIGHT COMPONENT TRANSFER / CONVOLUTION",
        24.,
        170.,
    );
    let rgba = (0..96)
        .flat_map(|y| {
            (0..96).flat_map(move |x| {
                [
                    ((x / 12) * 30) as u8,
                    ((y / 12) * 30) as u8,
                    if (x / 12 + y / 12) % 2 == 0 { 240 } else { 32 },
                    if x > 20 && x < 75 && y > 20 && y < 75 {
                        128
                    } else {
                        255
                    },
                ]
            })
        })
        .collect::<Vec<_>>();
    let image = Image2D::from_rgba8(96, 96, &rgba).unwrap();
    for i in 0..6 {
        let (name, filter) = match i {
            0 => ("Original", None),
            1 => (
                "Invert + gamma",
                Some(LayerFilter::ComponentTransfer {
                    channels: [
                        TransferFunction::Linear {
                            slope: -1.,
                            intercept: 1.,
                        },
                        TransferFunction::Gamma {
                            amplitude: 1.,
                            exponent: 0.6,
                            offset: 0.,
                        },
                        TransferFunction::Identity,
                        TransferFunction::Identity,
                    ],
                    color_space: FilterColorSpace::Srgb,
                }),
            ),
            2 => (
                "Posterize",
                Some(LayerFilter::ComponentTransfer {
                    channels: [
                        TransferFunction::discrete(&[0., 0.5, 1.])?,
                        TransferFunction::discrete(&[0., 0.5, 1.])?,
                        TransferFunction::discrete(&[0., 0.5, 1.])?,
                        TransferFunction::Identity,
                    ],
                    color_space: FilterColorSpace::Srgb,
                }),
            ),
            _ => {
                let (name, kernel) = match i {
                    3 => ("Sharpen", [0., -1., 0., -1., 5., -1., 0., -1., 0.]),
                    4 => ("Edges", [-1., -1., -1., -1., 8., -1., -1., -1., -1.]),
                    _ => ("Emboss", [-2., -1., 0., -1., 1., 1., 0., 1., 2.]),
                };
                let mut f = Convolution::new([3, 3], &kernel)?;
                f.preserve_alpha = true;
                f.color_space = FilterColorSpace::Srgb;
                if i == 5 {
                    f.bias = 0.25;
                }
                (name, Some(LayerFilter::ConvolveMatrix(f)))
            }
        };
        // Filter at native image resolution, then display the immutable result.
        let mut tile = RasterCanvas::new_linear(96, 96);
        tile.push_layer_with_options(LayerOptions {
            filters: filter.into_iter().collect(),
            ..Default::default()
        });
        tile.draw_image(&CanvasImageSource::Image(&image), 0., 0., 96., 96.);
        tile.pop_layer();
        let bitmap =
            Image2D::from_rgba8(96, 96, &tile.get_image_data(0, 0, 96, 96).unwrap()).unwrap();
        let x = 24. + i as f32 * 156.;
        c.draw_image(&CanvasImageSource::Image(&bitmap), x, 190., 132., 132.);
        label(&mut c, &labels, name, x, 346.);
    }
    label(
        &mut c,
        &labels,
        "POLAR GRADIENTS / HUE PATHS / ALPHA-AWARE INTERPOLATION",
        24.,
        392.,
    );
    let stops = GradientStops::new(&[(0., [1., 0., 0., 1.]), (1., [0., 0., 1., 1.])])?;
    for (row, space) in [GradientSpace::Hsl, GradientSpace::Oklch]
        .into_iter()
        .enumerate()
    {
        for (i, hue) in [
            HueDirection::Shorter,
            HueDirection::Longer,
            HueDirection::Increasing,
            HueDirection::Decreasing,
        ]
        .into_iter()
        .enumerate()
        {
            let x = 24. + i as f32 * 234.;
            let y = 426. + row as f32 * 92.;
            label(&mut c, &labels, &format!("{space:?} {hue:?}"), x, y);
            c.set_fill_brush(Brush2D::vector(
                VectorGradient::linear([x, 0.], [x + 210., 0.], stops.clone())?
                    .with_color_space(space)
                    .with_hue_direction(hue),
            ));
            c.fill_rect(x, y + 12., 210., 44.);
        }
    }
    let path = std::path::Path::new(&output);
    if let Some(p) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(p)?;
    }
    image::save_buffer(
        path,
        &c.get_image_data(0, 0, 960, 620).unwrap(),
        960,
        620,
        image::ColorType::Rgba8,
    )?;
    println!("{}", path.display());
    Ok(())
}
