use super::*;
use crate::core::tree::Element;
use crate::primitives::path::{
    attach_path, path_gpu_from_element_attrs, polyline, request_path_rt_readback,
    take_path_rt_readback, PathStyle, StrokeLinecap,
};
use crate::renderer::offscreen::OffscreenGpu;

fn setup() -> (OffscreenGpu, TexturedQuadPipeline) {
    let gpu = OffscreenGpu::new().expect("headless GPU adapter");
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        width: 96,
        height: 96,
        present_mode: wgpu::PresentMode::Fifo,
        desired_maximum_frame_latency: 2,
        alpha_mode: wgpu::CompositeAlphaMode::Auto,
        color_space: wgpu::SurfaceColorSpace::Auto,
        view_formats: vec![],
    };
    let textured = TexturedQuadPipeline::new(&gpu.device, &config);
    (gpu, textured)
}

#[test]
#[ignore = "requires a headless GPU adapter"]
fn prepared_text_layout_and_color_atlas_reach_framework_callback() {
    use crate::prelude::{PreparedText2D, TextPrepareOptions, TextRenderMode};
    use crate::primitives::canvas::{canvas, canvas_raster_from_element_attrs};
    use jadix_2d::text_layout::cosmic_text::{
        self as cosmic, Attrs, Buffer, FontSystem, Metrics, Shaping, SwashCache,
    };
    let (gpu, textured) = setup();
    let mut fonts =
        FontSystem::new_with_locale_and_db("en-US".into(), cosmic::fontdb::Database::new());
    fonts.db_mut().load_font_data(
        include_bytes!("../../../../../jadix_cosmic_text/fonts/Inter-Regular.ttf").to_vec(),
    );
    fonts.db_mut().load_font_data(
        include_bytes!("../../../../../jadix_2d/tests/fonts/colr-mini.ttf").to_vec(),
    );
    fonts
        .db_mut()
        .load_font_data(include_bytes!("../../../../../jadix_2d/tests/fonts/colr-v1.ttf").to_vec());
    #[cfg(feature = "svg")]
    fonts.db_mut().load_font_data(
        include_bytes!("../../../../../jadix_2d/tests/fonts/svg-mini.ttf").to_vec(),
    );
    fonts.db_mut().load_font_data(
        include_bytes!("../../../../../jadix_2d/tests/fonts/axes-mini.ttf").to_vec(),
    );
    fonts.db_mut().set_sans_serif_family("Inter");
    let mut cache = SwashCache::new();
    for mode in [TextRenderMode::Outline, TextRenderMode::Atlas] {
        let mut buffer = Buffer::new(&mut fonts, Metrics::new(18., 26.));
        buffer.set_size(Some(90.), Some(90.));
        let spans = vec![
            (
                "AA fi\n",
                Attrs::new()
                    .family(cosmic::Family::Name("Jadix Axes Fixture"))
                    .font_variations(
                        cosmic::FontVariations::new(&[(*b"wdth", 150.), (*b"wght", 700.)]).unwrap(),
                    ),
            ),
            (
                "\u{E000}",
                Attrs::new().family(cosmic::Family::Name("Jadix COLR Fixture")),
            ),
            (
                "\n\u{e102}\u{e103}",
                Attrs::new().family(cosmic::Family::Name("Jadix COLRv1 Fixture")),
            ),
        ];
        #[cfg(feature = "svg")]
        let spans = {
            let mut spans = spans;
            spans.push((
                "\n\u{e100}\u{e101}",
                Attrs::new().family(cosmic::Family::Name("Jadix SVG Fixture")),
            ));
            spans
        };
        buffer.set_rich_text(spans, &Attrs::new(), Shaping::Advanced, None);
        let prepared = PreparedText2D::new(
            &mut fonts,
            &mut buffer,
            &mut cache,
            TextPrepareOptions {
                mode,
                ..Default::default()
            },
        )
        .unwrap();
        // Freeze prepared vector/color text once; callback reuses nested scene
        // snapshots after the font and layout preparation phase has finished.
        let child = crate::prelude::Scene2D::record([96, 96], |c| {
            c.draw_text_layout(&prepared, [3., 2.]);
        })
        .unwrap();
        let scene = crate::prelude::Scene2D::record([96, 96], |c| {
            child
                .draw(c, crate::prelude::SceneOptions::default())
                .unwrap();
        })
        .unwrap();
        let expected = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let capture = expected.clone();
        let el: Element = canvas()
            .w(96.)
            .h(96.)
            .paint(move |ctx, _| {
                ctx.push_layer_with_options(jadix_2d::LayerOptions {
                    filters: vec![
                        jadix_2d::LayerFilter::ComponentTransfer {
                            channels: [
                                jadix_2d::TransferFunction::Gamma {
                                    amplitude: 1.,
                                    exponent: 0.8,
                                    offset: 0.05,
                                },
                                jadix_2d::TransferFunction::Identity,
                                jadix_2d::TransferFunction::Identity,
                                jadix_2d::TransferFunction::Identity,
                            ],
                            color_space: jadix_2d::FilterColorSpace::Srgb,
                        },
                        jadix_2d::LayerFilter::ConvolveMatrix(
                            jadix_2d::Convolution::new([3, 1], &[0.2, 0.6, 0.2]).unwrap(),
                        ),
                        jadix_2d::LayerFilter::DropShadow {
                            sigma: [1., 1.5],
                            offset: [1.5, 2.],
                            color: [0.2, 0.1, 0.8, 0.6],
                        },
                    ],
                    ..Default::default()
                });
                ctx.draw_scene(&scene, crate::prelude::SceneOptions::default())
                    .unwrap();
                ctx.pop_layer();
                *capture.borrow_mut() = ctx.get_image_data(0, 0, 96, 96).unwrap();
                crate::tree::LayoutStatus::Clean
            })
            .into();
        let _ = take_path_jobs();
        let (key, _, _) = canvas_raster_from_element_attrs(&el.attrs, 96., 96.).unwrap();
        request_path_rt_readback(&key);
        let mut renderer = PathGpu::try_new(&gpu.device, 4).unwrap();
        renderer
            .core
            .set_aa_quality(jadix_2d::gpu::AaQuality::Reference);
        assert!(renderer.flush(&gpu.device, &gpu.queue, &textured, &mut HashMap::new()));
        let (_, _, actual) = take_path_rt_readback(&key).unwrap();
        let reference = expected.borrow();
        let linear = jadix_theme::color_space::linearize_display_srgb_component;
        let error: f64 = actual
            .chunks_exact(4)
            .zip(reference.chunks_exact(4))
            .map(|(a, b)| {
                f64::from(a[3].abs_diff(b[3]))
                    + (0..3)
                        .map(|k| {
                            f64::from(
                                (linear(a[k] as f32 / 255.) * a[3] as f32
                                    - linear(b[k] as f32 / 255.) * b[3] as f32)
                                    .abs(),
                            )
                        })
                        .sum::<f64>()
            })
            .sum();
        assert!(
            error / (actual.len() as f64) < 1.,
            "{mode:?}: {}",
            error / actual.len() as f64
        );
    }
}

#[test]
#[ignore = "requires a headless GPU adapter"]
fn canvas_glyphs_gaussian_and_luminance_reach_gpu_readback() {
    use crate::prelude::{
        Brush2D, Font2D, GlyphRun, GradientStops, Morphology, PositionedGlyph, VectorGradient,
    };
    use crate::primitives::canvas::{canvas, canvas_raster_from_element_attrs};
    use crate::tree::LayoutStatus;
    let (gpu, textured) = setup();
    let font = Font2D::new(
        &include_bytes!("../../../../../jadix_cosmic_text/fonts/Inter-Regular.ttf")[..],
        0,
    )
    .unwrap();
    let run = GlyphRun::new(
        &font,
        28.,
        &[PositionedGlyph {
            id: font.glyph_id('B').unwrap(),
            x: 16.,
            y: 44.,
        }],
    )
    .unwrap();
    let expected = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let capture = expected.clone();
    let el: Element = canvas()
        .w(64.)
        .h(64.)
        .paint(move |ctx, _| {
            ctx.push_layer_with_options(jadix_2d::LayerOptions {
                filters: vec![jadix_2d::LayerFilter::DropShadow {
                    sigma: [1., 2.],
                    offset: [2.5, 3.],
                    color: [0.2, 0.1, 0.8, 0.6],
                }],
                ..Default::default()
            });
            ctx.draw_blurred_rounded_rect([12., 12., 40., 32.], 8., 3., [0.2, 0.4, 1., 0.7]);
            ctx.draw_rounded_rect_shadow(
                [4., 4., 52., 48.],
                [0., 16., 4., 8.],
                2.,
                [1., 0.5, 0., 0.6],
                true,
            );
            ctx.set_fill("#ff8000");
            ctx.fill_glyph_run(&run);
            ctx.push_luminance_layer(0.8, true);
            ctx.set_fill("#ffffff");
            ctx.fill_rect(0., 0., 32., 64.);
            ctx.pop_layer();
            ctx.pop_layer();
            ctx.push_layer_with_options(jadix_2d::LayerOptions {
                filters: vec![
                    jadix_2d::LayerFilter::Morphology {
                        radius: [1, 2],
                        operator: Morphology::Dilate,
                    },
                    jadix_2d::LayerFilter::Offset {
                        offset: [0.5, -1.25],
                    },
                ],
                ..Default::default()
            });
            let stops = GradientStops::new(
                &(0..33)
                    .map(|i| {
                        let t = i as f32 / 32.;
                        (t, [t, 0.4, 1. - t, 0.7])
                    })
                    .collect::<Vec<_>>(),
            )
            .unwrap()
            .with_midpoints(&[Some(0.3); 32])
            .unwrap();
            ctx.set_fill_brush(Brush2D::vector(
                VectorGradient::radial([36., 48.], 1., [44., 48.], 15., stops).unwrap(),
            ));
            ctx.fill_rect(28., 40., 28., 16.);
            ctx.pop_layer();
            *capture.borrow_mut() = ctx.get_image_data(0, 0, 64, 64).unwrap();
            LayoutStatus::Clean
        })
        .into();
    let _ = take_path_jobs();
    let (key, _, _) = canvas_raster_from_element_attrs(&el.attrs, 64., 64.).unwrap();
    request_path_rt_readback(&key);
    let mut renderer = PathGpu::try_new(&gpu.device, 4).unwrap();
    renderer
        .core
        .set_aa_quality(jadix_2d::gpu::AaQuality::Reference);
    let mut images = HashMap::new();
    assert!(renderer.flush(&gpu.device, &gpu.queue, &textured, &mut images));
    let (_, _, actual) = take_path_rt_readback(&key).unwrap();
    let reference = expected.borrow();
    let mut error = 0.;
    for (a, b) in actual.chunks_exact(4).zip(reference.chunks_exact(4)) {
        error += f64::from(a[3].abs_diff(b[3]));
        for i in 0..3 {
            let decode = jadix_theme::color_space::linearize_display_srgb_component;
            error += f64::from(
                (decode(a[i] as f32 / 255.) * a[3] as f32
                    - decode(b[i] as f32 / 255.) * b[3] as f32)
                    .abs(),
            );
        }
    }
    assert!(
        error / (actual.len() as f64) < 2.,
        "premultiplied RGBA MAE {}",
        error / actual.len() as f64
    );
}

#[test]
#[ignore = "requires a headless GPU adapter"]
fn canvas_explicit_brushes_reach_framework_gpu_readback() {
    use crate::prelude::{Brush2D, ExtendMode, Image2D, ImageSampling};
    use crate::primitives::canvas::{canvas, canvas_raster_from_element_attrs};
    use crate::tree::LayoutStatus;
    let (gpu, textured) = setup();
    let expected = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let capture = expected.clone();
    let image = Image2D::from_rgba8(2, 1, &[255, 0, 0, 255, 0, 0, 255, 128]).unwrap();
    let el: Element = canvas()
        .w(32.)
        .h(32.)
        .paint(move |ctx, _| {
            ctx.set_fill_brush(
                Brush2D::gradient(
                    crate::style::parse_gradient("linear-gradient(to right,red,blue)").unwrap(),
                    [0., 0., 32., 32.],
                )
                .with_transform([0., 1., -1., 0., 32., 0.]),
            );
            ctx.fill_rect(0., 0., 16., 32.);
            ctx.fill_rect(16., 0., 16., 32.);
            ctx.push_layer(0.75, crate::prelude::BlendMode::Multiply);
            ctx.set_stroke_brush(
                Brush2D::image(image.clone())
                    .with_transform([4., 0., 0., 4., 0., 0.])
                    .with_extend(ExtendMode::Reflect, ExtendMode::Pad)
                    .with_sampling(ImageSampling::Bilinear),
            );
            ctx.set_line_width(8.);
            ctx.stroke_path(&polyline(&[(0., 16.), (32., 16.)]));
            ctx.pop_layer();
            *capture.borrow_mut() = ctx.get_image_data(0, 0, 32, 32).unwrap();
            LayoutStatus::Clean
        })
        .into();
    let _ = take_path_jobs();
    let (key, _, _) = canvas_raster_from_element_attrs(&el.attrs, 32., 32.).unwrap();
    request_path_rt_readback(&key);
    let mut renderer = PathGpu::try_new(&gpu.device, 4).unwrap();
    let mut images = HashMap::new();
    assert!(renderer.flush(&gpu.device, &gpu.queue, &textured, &mut images));
    let (_, _, actual) = take_path_rt_readback(&key).unwrap();
    assert!(actual
        .iter()
        .zip(expected.borrow().iter())
        .all(|(a, b)| a.abs_diff(*b) <= 4));
}

#[test]
#[ignore = "requires a headless GPU adapter"]
fn canvas_gradient_strokes_and_bounded_porter_duff_reach_gpu_readback() {
    use crate::prelude::{BlendMode, CompositeMode, LayerOptions};
    use crate::primitives::canvas::{canvas, canvas_raster_from_element_attrs};
    use crate::tree::LayoutStatus;
    let (gpu, textured) = setup();
    let expected = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let capture = expected.clone();
    let el: Element = canvas()
        .w(32.)
        .h(32.)
        .paint(move |ctx, _| {
            ctx.clear("#8040c0");
            ctx.push_layer_with_options(LayerOptions {
                compose: CompositeMode::Copy,
                blend: BlendMode::SoftLight,
                clip: Some([8., 8., 16., 16.]),
                ..Default::default()
            });
            ctx.set_line_width(8.);
            ctx.set_stroke_gradient(
                crate::style::gradient::parse_gradient("linear-gradient(to right,#ff0000,#0000ff)")
                    .unwrap(),
            );
            ctx.stroke_path(&polyline(&[(4., 16.), (28., 16.)]));
            ctx.pop_layer();
            *capture.borrow_mut() = ctx.get_image_data(0, 0, 32, 32).unwrap();
            LayoutStatus::Clean
        })
        .into();
    let _ = take_path_jobs();
    let (key, _, _) = canvas_raster_from_element_attrs(&el.attrs, 32., 32.).unwrap();
    request_path_rt_readback(&key);
    let mut renderer = PathGpu::try_new(&gpu.device, 4).unwrap();
    let mut images = HashMap::new();
    assert!(renderer.flush(&gpu.device, &gpu.queue, &textured, &mut images));
    let (_, _, actual) = take_path_rt_readback(&key).unwrap();
    for (x, y) in [(2, 2), (10, 10), (12, 16), (20, 16), (28, 28)] {
        let at = (y * 32 + x) * 4;
        assert!(actual[at..at + 4]
            .iter()
            .zip(&expected.borrow()[at..at + 4])
            .all(|(a, b)| a.abs_diff(*b) <= 4));
    }
}

#[test]
#[ignore = "requires a headless GPU adapter"]
fn canvas_nested_groups_flush_through_framework_gpu_pipeline() {
    use crate::primitives::canvas::{canvas, canvas_raster_from_element_attrs, BlendMode};
    use crate::primitives::path::{request_path_rt_readback, take_path_rt_readback};
    use crate::tree::LayoutStatus;
    let (gpu, textured) = setup();
    let el: Element = canvas()
        .w(32.)
        .h(32.)
        .paint(|ctx, _| {
            ctx.set_fill("#0000ff");
            ctx.fill_rect(0., 0., 32., 32.);
            ctx.push_layer(0.5, BlendMode::Normal);
            ctx.push_layer(1., BlendMode::Normal);
            ctx.set_fill("#ff0000");
            ctx.fill_rect(0., 0., 32., 32.);
            ctx.set_fill("#00ff00");
            ctx.fill_rect(8., 8., 16., 16.);
            ctx.pop_layer();
            ctx.pop_layer();
            LayoutStatus::Clean
        })
        .into();
    let _ = take_path_jobs();
    let (key, _, _) = canvas_raster_from_element_attrs(&el.attrs, 32., 32.).unwrap();
    request_path_rt_readback(&key);
    let mut path = PathGpu::try_new(&gpu.device, 4).unwrap();
    let mut images = HashMap::new();
    assert!(path.flush(&gpu.device, &gpu.queue, &textured, &mut images));
    let (_, _, rgba) = take_path_rt_readback(&key).expect("groups must stay on GPU");
    for (at, expected) in [
        (0, [188, 0, 188, 255]),
        ((16 * 32 + 16) * 4, [0, 188, 188, 255]),
    ] {
        assert!(rgba[at..][..4]
            .iter()
            .zip(expected)
            .all(|(a, b)| a.abs_diff(b) <= 2));
    }
}

#[test]
#[ignore = "requires a headless GPU adapter"]
fn canvas_images_and_path_masks_flush_through_framework_gpu_pipeline() {
    use crate::core::tree::LayoutStatus;
    use crate::primitives::canvas::{
        canvas, canvas_raster_from_element_attrs, CanvasImageSource, Image2D, ImageSampling,
    };
    let (gpu, textured) = setup();
    let image = Image2D::from_rgba8(1, 1, &[0, 0, 255, 128]).unwrap();
    let el: Element = canvas()
        .w(96.0)
        .h(96.0)
        .paint(move |ctx, _| {
            ctx.clip_path(
                &crate::primitives::path::filled_circle(48.0, 48.0, 28.0, 32),
                crate::primitives::path::FillRule::NonZero,
            );
            ctx.set_image_sampling(ImageSampling::Bilinear);
            ctx.draw_image(&CanvasImageSource::Image(&image), 0.0, 0.0, 96.0, 96.0);
            LayoutStatus::Clean
        })
        .into();
    let _ = take_path_jobs();
    let (key, _, _) = canvas_raster_from_element_attrs(&el.attrs, 96.0, 96.0).unwrap();
    request_path_rt_readback(&key);
    let mut path = PathGpu::try_new(&gpu.device, 4).unwrap();
    let mut images = HashMap::new();
    assert!(path.flush(&gpu.device, &gpu.queue, &textured, &mut images));
    let (w, h, rgba) = take_path_rt_readback(&key).expect("image/mask must remain a GPU job");
    assert_eq!([w, h], [96, 96]);
    let p = &rgba[((48 * w + 48) * 4) as usize..][..4];
    assert!(
        p[0] == 0 && p[1] == 0 && p[2] >= 254 && p[3].abs_diff(128) <= 1,
        "{p:?}"
    );
    assert_eq!(&rgba[..4], &[0, 0, 0, 0]);
}

#[test]
#[ignore = "requires a headless GPU adapter"]
fn dynamic_gradient_frames_reuse_gpu_resources_without_stale_pixels() {
    use crate::primitives::path::{filled_rect, register_path_scene, ATTR_PATH_SCENE};
    let (gpu, textured) = setup();
    let mut renderer = PathGpu::try_new(&gpu.device, 4).unwrap();
    for frame in 0..4 {
        let mut style = PathStyle::fill([1.0; 4]);
        style.fill_gradient = Some(
            crate::style::parse_gradient(if frame % 2 == 0 {
                "linear-gradient(to right, #ff0000, #ff0000)"
            } else {
                "linear-gradient(to right, #0000ff, #0000ff)"
            })
            .unwrap(),
        );
        let scene = register_path_scene(
            vec![(filled_rect(8.0, 8.0, 80.0, 80.0), style)],
            Vec::new(),
            0.0,
            96.0,
            96.0,
        );
        let mut el = Element::new();
        el.set_attr(ATTR_PATH_SCENE, format!("{scene:016x}"));
        let actual = render(&gpu, &textured, &mut renderer, &el, 1.0);
        let color = actual.pixel(48, 48).unwrap();
        assert_eq!(
            color,
            if frame % 2 == 0 {
                [255, 0, 0, 255]
            } else {
                [0, 0, 255, 255]
            }
        );
        let stats = renderer.core.last_stats();
        assert_eq!(stats.draw_calls, 2);
        assert_eq!(stats.uniform_uploads, 1);
        if frame > 0 {
            assert_eq!(stats.buffer_allocations, 0);
            assert_eq!(stats.bind_group_allocations, 0);
            assert_eq!(stats.texture_allocations, 0);
        }
    }
}

#[test]
#[ignore = "requires a headless GPU adapter"]
fn conic_repeating_and_linear_gradients_match_cpu_samples() {
    let (gpu, textured) = setup();
    for samples in [1, 4] {
        let mut renderer = PathGpu::try_new(&gpu.device, samples).unwrap();
        for css in [
            "linear-gradient(to right, #ff0000, #0000ff)",
            "conic-gradient(from 20deg, #ff0000, #0000ff, #ff0000)",
            "repeating-linear-gradient(to right, #ff0000 10%, #0000ff 40%)",
            "repeating-radial-gradient(#ff0000 10%, #0000ff 40%)",
            "repeating-conic-gradient(#ff0000 20deg, #0000ff 120deg)",
        ] {
            let gradient = crate::style::parse_gradient(css).unwrap();
            let mut style = PathStyle::fill([1.0; 4]);
            style.fill_gradient = Some(gradient.clone());
            let scene = crate::primitives::path::register_path_scene(
                vec![(
                    crate::primitives::path::filled_rect(8.0, 8.0, 80.0, 80.0),
                    style,
                )],
                Vec::new(),
                0.0,
                96.0,
                96.0,
            );
            let mut el = Element::new();
            el.set_attr(
                crate::primitives::path::ATTR_PATH_SCENE,
                format!("{scene:016x}"),
            );
            let image = render(&gpu, &textured, &mut renderer, &el, 1.0);
            for (x, y) in [(24, 29), (36, 43), (67, 60), (75, 78)] {
                let expected = crate::style::sample_gradient_in_box(
                    &gradient,
                    (x as f32 + 0.5 - 8.0) / 80.0,
                    (y as f32 + 0.5 - 8.0) / 80.0,
                    80.0,
                    80.0,
                );
                let actual = image.pixel(x, y).unwrap();
                for c in 0..4 {
                    assert!(
                        (actual[c] as f32 - expected[c] * 255.0).abs() < 4.0,
                        "{css} samples={samples} ({x},{y}): {actual:?}, expected {expected:?}"
                    );
                }
            }
        }
    }
}

#[test]
#[ignore = "requires a headless GPU adapter"]
fn compound_gpu_fill_matches_cpu_holes_and_single_alpha_coverage() {
    use crate::primitives::path::{decode_path_d, register_path_scene, FillRule, ATTR_PATH_SCENE};
    let (gpu, textured) = setup();
    let mut renderer = PathGpu::try_new(&gpu.device, 4).unwrap();
    for source in [
        "M8 8 H88 V88 H8 Z M28 28 H68 V68 H28 Z",
        "M8 8 H88 V88 H8 M28 28 V68 H68 V28",
    ] {
        for rule in [FillRule::EvenOdd, FillRule::NonZero] {
            let commands = decode_path_d(source);
            let mut style = PathStyle::fill([1.0, 0.0, 0.0, 0.5]);
            style.fill_rule = rule;
            let cpu =
                crate::primitives::path::rasterize_path_rgba(&commands, &style, &[], 96, 96, 2);
            let scene = register_path_scene(vec![(commands, style)], Vec::new(), 0.0, 96.0, 96.0);
            let mut el = Element::new();
            el.set_attr(ATTR_PATH_SCENE, format!("{scene:016x}"));
            let actual = render(&gpu, &textured, &mut renderer, &el, 1.0);
            for (x, y) in [(16, 16), (48, 48), (75, 75), (2, 2)] {
                let offset = (y * 96 + x) as usize * 4;
                let p = actual.pixel(x, y).unwrap();
                for c in 0..4 {
                    assert!(
                        (p[c] as i32 - cpu[offset + c] as i32).abs() <= 2,
                        "{source} {rule:?}: {p:?}, {:?}",
                        &cpu[offset..offset + 4]
                    );
                }
            }
        }
    }
}

fn render(
    gpu: &OffscreenGpu,
    textured: &TexturedQuadPipeline,
    path: &mut PathGpu,
    el: &Element,
    dpi: f32,
) -> crate::renderer::offscreen::RgbaImage {
    let _ = take_path_jobs();
    crate::style::set_gradient_raster_dpi(dpi);
    let (key, _, _) = path_gpu_from_element_attrs(&el.attrs, 96.0, 96.0).unwrap();
    crate::style::set_gradient_raster_dpi(1.0);
    request_path_rt_readback(&key);
    let mut images = HashMap::new();
    assert!(path.flush(&gpu.device, &gpu.queue, textured, &mut images));
    let (width, height, pixels) = take_path_rt_readback(&key).expect("path pixels");
    crate::renderer::offscreen::RgbaImage {
        width,
        height,
        pixels,
    }
}

#[test]
#[ignore = "requires a headless GPU adapter"]
fn resident_path_recollect_does_not_tessellate_again() {
    use crate::primitives::path::gpu_op_build_count;
    let (gpu, textured) = setup();
    let el = attach_path(
        Element::new(),
        &crate::primitives::path::decode_path_d("M 12 72 C 22 -14 76 118 84 19"),
        &PathStyle::stroke([0.2, 0.5, 0.8, 1.0], 2.5),
    );
    let mut path = PathGpu::try_new(&gpu.device, 1).unwrap();
    assert!(
        !path.core.pipeline_state()[0]
            && !path.core.pipeline_state()[1]
            && !path.core.pipeline_state()[2]
    );
    let mut images = HashMap::new();
    let _ = take_path_jobs();
    crate::style::set_gradient_raster_dpi(1.0);
    let (key, _, _) = path_gpu_from_element_attrs(&el.attrs, 96.0, 96.0).unwrap();
    request_path_rt_readback(&key);
    assert!(path.flush(&gpu.device, &gpu.queue, &textured, &mut images));
    let original = take_path_rt_readback(&key).unwrap();
    assert!(
        !path.core.pipeline_state()[0]
            && !path.core.pipeline_state()[1]
            && !path.core.pipeline_state()[2],
        "stroke jobs must not compile fill programs"
    );
    let built = gpu_op_build_count();
    for _ in 0..4 {
        path_gpu_from_element_attrs(&el.attrs, 96.0, 96.0).unwrap();
        assert!(!path.flush(&gpu.device, &gpu.queue, &textured, &mut images));
    }
    assert_eq!(
        gpu_op_build_count(),
        built,
        "resident recollect must skip tessellation"
    );
    images.remove(&key);
    path_gpu_from_element_attrs(&el.attrs, 96.0, 96.0).unwrap();
    request_path_rt_readback(&key);
    assert!(path.flush(&gpu.device, &gpu.queue, &textured, &mut images));
    assert_eq!(
        gpu_op_build_count(),
        built + 1,
        "eviction must rebuild geometry"
    );
    assert_eq!(
        take_path_rt_readback(&key).unwrap(),
        original,
        "eviction must preserve pixels"
    );

    path_gpu_from_element_attrs(&el.attrs, 96.0, 96.0).unwrap();
    let mut forced = take_path_jobs().pop().unwrap();
    forced.force_redraw = true;
    crate::primitives::path::queue_path_job(forced);
    request_path_rt_readback(&key);
    assert!(path.flush(&gpu.device, &gpu.queue, &textured, &mut images));
    assert_eq!(
        gpu_op_build_count(),
        built + 2,
        "explicit redraw must rebuild"
    );
    assert_eq!(take_path_rt_readback(&key).unwrap(), original);

    let (resized, _, _) = path_gpu_from_element_attrs(&el.attrs, 80.0, 64.0).unwrap();
    assert_ne!(
        resized, key,
        "dimensions must remain part of texture identity"
    );
    assert!(path.flush(&gpu.device, &gpu.queue, &textured, &mut images));
    assert_eq!(
        gpu_op_build_count(),
        built + 3,
        "new dimensions must rebuild"
    );
}

#[test]
#[ignore = "requires a headless GPU adapter"]
fn shared_gpu_edges_have_subpixel_coverage() {
    let (gpu, textured) = setup();
    let triangle = [
        crate::primitives::path::PathCommand::MoveTo((11.2, 13.4)),
        crate::primitives::path::PathCommand::LineTo((83.7, 29.8)),
        crate::primitives::path::PathCommand::LineTo((24.6, 82.1)),
        crate::primitives::path::PathCommand::Close,
    ];
    let fill = attach_path(
        Element::new(),
        &triangle,
        &PathStyle::fill([0.2, 0.5, 0.8, 1.0]),
    );
    let line = attach_path(
        Element::new(),
        &polyline(&[(9.3, 16.2), (82.4, 72.8)]),
        &PathStyle::stroke([0.2, 0.5, 0.8, 1.0], 1.25).linecap(StrokeLinecap::Butt),
    );
    let curve = attach_path(
        Element::new(),
        &crate::primitives::path::decode_path_d("M 12.3 72.4 C 22.7 -14.6 76.1 118.2 83.6 18.7"),
        &PathStyle::stroke([0.2, 0.5, 0.8, 1.0], 2.5),
    );
    let caps = attach_path(
        Element::new(),
        &polyline(&[(26.3, 34.6), (69.2, 61.8)]),
        &PathStyle::stroke([0.2, 0.5, 0.8, 1.0], 24.0),
    );
    for samples in [1, 4] {
        let mut path = PathGpu::try_new(&gpu.device, samples).expect("path pipeline");
        for dpi in [1.0, 1.5, 2.0] {
            for (name, el) in [
                ("fill", &fill),
                ("thin-stroke", &line),
                ("curve", &curve),
                ("round-caps", &caps),
            ] {
                let img = render(&gpu, &textured, &mut path, el, dpi);
                let partial = img
                    .pixels
                    .chunks_exact(4)
                    .filter(|p| p[3] > 0 && p[3] < 255)
                    .count();
                assert!(
                    partial > 50,
                    "{name} at {dpi}, {samples} samples has aliased edges: {partial} partial pixels"
                );
                if name == "fill" {
                    let p = img.pixel((40.0 * dpi) as u32, (40.0 * dpi) as u32).unwrap();
                    assert_eq!(p[3], 255, "antialiasing must keep interiors opaque");
                    for (channel, expected) in p[..3].iter().zip([51, 128, 204]) {
                        assert!(
                            (*channel as i32 - expected).abs() <= 2,
                            "path must preserve display sRGB color, got {p:?}"
                        );
                    }
                }
                if let Ok(dir) = std::env::var("JADIX_AA_CAPTURE_DIR") {
                    image::save_buffer(
                        std::path::Path::new(&dir).join(format!("{name}-{samples}-{dpi}.png")),
                        &img.pixels,
                        img.width,
                        img.height,
                        image::ColorType::Rgba8,
                    )
                    .unwrap();
                }
            }
        }
    }
}

#[test]
#[ignore = "requires a headless GPU adapter"]
fn shared_gpu_lazy_gradients_match_general_cover() {
    use crate::primitives::path::{filled_rect, register_path_scene, ATTR_PATH_SCENE};
    let (gpu, textured) = setup();
    for samples in [1, 4] {
        let begin = std::time::Instant::now();
        let mut path = PathGpu::try_new(&gpu.device, samples).expect("path pipeline");
        eprintln!(
            "path init samples={samples}: {:.3}ms",
            begin.elapsed().as_secs_f64() * 1000.0
        );
        assert!(!path.core.pipeline_state()[3]);
        let mut reference = PathGpu::try_new(&gpu.device, samples).unwrap();
        reference.core.use_general_cover_for_test(&gpu.device);
        for gradient in [
            None,
            Some("linear-gradient(35deg, #ff000080, #0044ff)"),
            Some("radial-gradient(circle, #ffffff, #00000080)"),
            None,
        ] {
            let mut style = PathStyle::fill([0.3, 0.6, 0.8, 0.7]);
            style.fill_gradient = gradient.map(|css| crate::style::parse_gradient(css).unwrap());
            let scene = register_path_scene(
                vec![(filled_rect(9.3, 12.4, 70.5, 66.2), style)],
                Vec::new(),
                0.0,
                96.0,
                96.0,
            );
            let mut el = Element::new();
            el.set_attr(ATTR_PATH_SCENE, format!("{scene:016x}"));
            for dpi in [1.0, 1.5, 2.0] {
                let actual = render(&gpu, &textured, &mut path, &el, dpi);
                let expected = render(&gpu, &textured, &mut reference, &el, dpi);
                let max_delta = actual
                    .pixels
                    .iter()
                    .zip(&expected.pixels)
                    .map(|(a, b)| a.abs_diff(*b))
                    .max()
                    .unwrap_or(0);
                let differences: Vec<_> = actual
                    .pixels
                    .iter()
                    .zip(&expected.pixels)
                    .enumerate()
                    .filter(|(_, (a, b))| a != b)
                    .take(8)
                    .collect();
                assert!(
                    max_delta <= 1,
                    "gradient={gradient:?} samples={samples} dpi={dpi} max_delta={max_delta} first_differences={differences:?}"
                );
            }
            assert!(
                !path.core.pipeline_state()[1],
                "convex fill skips winding pipeline"
            );
            assert!(
                !path.core.pipeline_state()[0],
                "nonzero fills must not compile even-odd"
            );
            if gradient.is_some() {
                if std::env::var("JADIX_GPU_2D_AA").as_deref() == Ok("area") {
                    assert!(path.core.area_pipeline_state()[1]);
                    assert!(
                        !path.core.pipeline_state()[3],
                        "Area must not compile unused standard gradients"
                    );
                } else {
                    assert!(path.core.pipeline_state()[3]);
                }
            }
        }
    }
}

#[test]
#[ignore = "requires a headless GPU adapter"]
fn shared_gpu_translucent_layers_preserve_color() {
    use crate::primitives::path::{filled_rect, register_path_scene, ATTR_PATH_SCENE};
    let (gpu, textured) = setup();
    let layers = vec![
        (
            filled_rect(10.0, 10.0, 70.0, 70.0),
            PathStyle::fill([1.0, 0.0, 0.0, 0.5]),
        ),
        (
            filled_rect(30.0, 30.0, 60.0, 60.0),
            PathStyle::fill([0.0, 0.0, 1.0, 0.5]),
        ),
    ];
    let scene = register_path_scene(layers, Vec::new(), 0.0, 96.0, 96.0);
    let mut el = Element::new();
    el.set_attr(ATTR_PATH_SCENE, format!("{scene:016x}"));
    let mut path = PathGpu::try_new(&gpu.device, 1).expect("path pipeline");
    let img = render(&gpu, &textured, &mut path, &el, 1.0);
    let p = img.pixel(50, 50).unwrap();
    let linear = |c: u8| {
        let s = c as f32 / 255.0;
        if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    assert!(
        (p[3] as f32 / 255.0 - 0.75).abs() < 0.01,
        "source-over alpha: {p:?}"
    );
    assert!(
        (linear(p[0]) - 1.0 / 3.0).abs() < 0.02 && (linear(p[2]) - 2.0 / 3.0).abs() < 0.02,
        "path textures must export straight color after premultiplied compositing: {p:?}"
    );
}

#[test]
#[ignore = "requires a headless GPU adapter"]
fn shared_gpu_canvas_aa_keeps_clipping_and_cached_paint() {
    use crate::core::tree::LayoutStatus;
    use crate::primitives::canvas::{canvas, canvas_raster_from_element_attrs};
    let (gpu, textured) = setup();
    let el: Element = canvas()
        .w(96.0)
        .h(96.0)
        .paint(|ctx, _| {
            ctx.clip_rect(12.0, 12.0, 72.0, 72.0);
            ctx.set_fill("#336699");
            ctx.fill_path(&crate::primitives::path::filled_circle(
                48.0, 48.0, 39.0, 16,
            ));
            LayoutStatus::Clean
        })
        .into();
    let _ = take_path_jobs();
    let (key, _, _) = canvas_raster_from_element_attrs(&el.attrs, 96.0, 96.0).unwrap();
    request_path_rt_readback(&key);
    let mut path = PathGpu::try_new(&gpu.device, 1).expect("path pipeline");
    let mut images = HashMap::new();
    assert!(path.flush(&gpu.device, &gpu.queue, &textured, &mut images));
    let (w, h, rgba) = take_path_rt_readback(&key).unwrap();
    assert_eq!(
        [w, h],
        [96, 96],
        "SSAA must preserve output texture dimensions"
    );
    let partial = rgba
        .chunks_exact(4)
        .filter(|p| p[3] > 0 && p[3] < 255)
        .count();
    assert!(partial > 50, "Canvas curves must also receive subpixel AA");
    for y in 0..h {
        for x in 0..w {
            if !(12..84).contains(&x) || !(12..84).contains(&y) {
                assert_eq!(
                    rgba[((y * w + x) * 4 + 3) as usize],
                    0,
                    "supersampling must keep Canvas scissor clipping at ({x}, {y})"
                );
            }
        }
    }
    assert_eq!(
        canvas_raster_from_element_attrs(&el.attrs, 96.0, 96.0)
            .unwrap()
            .0,
        key
    );
    assert!(
        !path.flush(&gpu.device, &gpu.queue, &textured, &mut images),
        "static Canvas paint must reuse its resolved texture"
    );
}
