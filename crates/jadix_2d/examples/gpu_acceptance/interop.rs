use super::*;
use gpu::{GpuImageAlpha as Alpha, GpuImageColor as Color};
fn texture(device: &wgpu::Device, format: wgpu::TextureFormat) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("interop"),
        size: wgpu::Extent3d {
            width: 32,
            height: 32,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}
fn clear(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture, rgba: [f64; 4]) {
    let view = texture.create_view(&Default::default());
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: rgba[0],
                        g: rgba[1],
                        b: rgba[2],
                        a: rgba[3],
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    }
    queue.submit([encoder.finish()]);
}
#[test]
#[ignore = "requires native GPU"]
fn external_texture_color_alpha_nested_layers_dirty_and_fallback() -> Result<(), Box<dyn Error>> {
    let TestGpu {
        device,
        queue,
        backend,
        ..
    } = test_gpu();
    let mut rows = Vec::new();
    for format in [
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        wgpu::TextureFormat::Bgra8Unorm,
        wgpu::TextureFormat::Rgba16Float,
    ] {
        for alpha in [Alpha::Straight, Alpha::Premultiplied] {
            for mask in [false, true] {
                for samples in [1, 4] {
                    let color = if format == wgpu::TextureFormat::Rgba8UnormSrgb {
                        Color::LinearSrgb
                    } else {
                        Color::Srgb
                    };
                    let external = texture(&device, format);
                    let decode = jadix_theme::color_space::linearize_display_srgb_component;
                    clear(&device, &queue, &external, [0.4, 0.2, 0.1, 0.5]);
                    let fallback =
                        Image2D::from_rgba8(32, 32, &[0, 255, 0, 255].repeat(32 * 32)).unwrap();
                    let mut renderer = PathGpu::try_new(&device, samples).unwrap();
                    renderer.override_image(&fallback, external.clone(), color, alpha)?;
                    let target = texture(&device, wgpu::TextureFormat::Rgba8UnormSrgb);
                    let view = target.create_view(&Default::default());
                    let make_ops = || {
                        let mut canvas = GpuCanvas::new_linear(32, 32, None);
                        canvas.push_layer(0.8, BlendMode::Normal);
                        canvas.push_layer_with_options(LayerOptions {
                            mask: mask.then(|| fallback.clone()),
                            filters: vec![LayerFilter::Gaussian { sigma: [0.5; 2] }],
                            ..Default::default()
                        });
                        if mask {
                            canvas.set_fill("white");
                            canvas.fill_rect(0., 0., 32., 32.);
                        } else {
                            canvas.set_fill_brush(Brush2D::image(fallback.clone()));
                            canvas.fill_rect(0., 0., 32., 32.);
                        }
                        canvas.pop_layer();
                        canvas.pop_layer();
                        canvas.into_ops()
                    };
                    let ops = make_ops();
                    assert!(renderer.render_premultiplied(&device, &queue, &view, [32, 32], &ops));
                    let first = renderer.last_stats();
                    assert!(first.external_image_conversions > 0);
                    assert!(first.image_uploaded_bytes < 4096);
                    let actual = readback(&device, &queue, &target, 32)?;
                    let p = &actual[(16 * 32 + 16) * 4..][..4];
                    let a = 128f32 / 255.;
                    let expected = if mask {
                        [a * 0.8; 4]
                    } else {
                        // Clear on sRGB format encodes the supplied linear channels; Unorm retains them.
                        let rgb = if format == wgpu::TextureFormat::Rgba8UnormSrgb {
                            [0.4, 0.2, 0.1]
                        } else {
                            [decode(0.4), decode(0.2), decode(0.1)]
                        };
                        let factor = if alpha == Alpha::Straight { a } else { 1. };
                        [
                            rgb[0] * factor * 0.8,
                            rgb[1] * factor * 0.8,
                            rgb[2] * factor * 0.8,
                            a * 0.8,
                        ]
                    };
                    let linear = [
                        decode(p[0] as f32 / 255.),
                        decode(p[1] as f32 / 255.),
                        decode(p[2] as f32 / 255.),
                        p[3] as f32 / 255.,
                    ];
                    assert!(
                        linear
                            .iter()
                            .zip(expected)
                            .all(|(a, b)| (a - b).abs() < 0.012),
                        "{format:?}/{alpha:?}/{mask}/{samples} {linear:?} {expected:?}"
                    );
                    assert!(renderer.render_premultiplied(&device, &queue, &view, [32, 32], &ops));
                    assert_eq!(renderer.last_stats().external_image_conversions, 0);
                    clear(&device, &queue, &external, [0.1, 0.2, 0.9, 0.25]);
                    renderer.mark_override_image_dirty(&fallback)?;
                    assert!(renderer.render_premultiplied(&device, &queue, &view, [32, 32], &ops));
                    let changed = readback(&device, &queue, &target, 32)?;
                    assert_ne!(&changed[(16 * 32 + 16) * 4..][..4], p);
                    assert!(renderer.unregister_texture(&fallback));
                    assert_eq!(renderer.registered_texture_count(), 0);
                    assert!(renderer.mark_override_image_dirty(&fallback).is_err());
                    assert!(renderer.render_premultiplied(&device, &queue, &view, [32, 32], &ops));
                    let restored = readback(&device, &queue, &target, 32)?;
                    let p = &restored[(16 * 32 + 16) * 4..][..4];
                    if mask {
                        assert!(p[0] > 225 && p[3] > 200);
                    } else {
                        assert!(p[0] == 0 && p[1] > 225 && p[2] == 0 && p[3] > 200);
                    }
                    rows.push(json!({"format":format!("{format:?}"),"alpha":format!("{alpha:?}"),"mask":mask,"samples":samples,"status":"pass"}));
                }
            }
        }
    }
    if let Some(dir) = std::env::var_os("JADIX_2D_EVIDENCE") {
        std::fs::create_dir_all(&dir)?;
        std::fs::write(
            Path::new(&dir).join("external-textures.json"),
            serde_json::to_vec_pretty(&json!({"backend":format!("{backend:?}"),"cases":rows}))?,
        )?;
    }
    Ok(())
}
#[test]
#[ignore = "requires native GPU"]
fn gpu_region_float_texture_roundtrip_and_registration_validation() -> Result<(), Box<dyn Error>> {
    let TestGpu { device, queue, .. } = test_gpu();
    let scene = Scene2D::record([32, 32], |c| {
        c.set_fill_brush(
            Color2D::new(ColorSpaceTag::LinearSrgb, [4., -0.25, 0.5, 0.5])
                .unwrap()
                .brush()
                .unwrap(),
        );
        c.fill_rect(0., 0., 32., 32.);
    })?;
    let region = SceneRegion {
        origin: [0, 0],
        size: [16, 16],
        halo: 8,
        filters: vec![LayerFilter::Gaussian { sigma: [1.; 2] }],
    };
    // COPY_SRC-only sources do not need TEXTURE_BINDING; import stages them on GPU.
    let copy_only = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("copy-only"),
        size: wgpu::Extent3d {
            width: 32,
            height: 32,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        copy_only.as_image_copy(),
        &[255, 0, 0, 255].repeat(1024),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(128),
            rows_per_image: Some(32),
        },
        copy_only.size(),
    );
    let mut copy_renderer = PathGpu::try_new(&device, 4).unwrap();
    let copy_image = copy_renderer.register_texture(copy_only, Color::Srgb, Alpha::Straight)?;
    let mut copy_canvas = GpuCanvas::new_linear(32, 32, None);
    copy_canvas.draw_image(&CanvasImageSource::Image(&copy_image), 0., 0., 32., 32.);
    let copy_target = texture(&device, wgpu::TextureFormat::Rgba8UnormSrgb);
    assert!(copy_renderer.render(
        &device,
        &queue,
        &copy_target.create_view(&Default::default()),
        [32, 32],
        &copy_canvas.into_ops()
    ));
    let bytes = readback(&device, &queue, &copy_target, 32)?;
    assert_eq!(&bytes[(16 * 32 + 16) * 4..][..4], &[255, 0, 0, 255]);
    let prepared = region.prepare_gpu(&scene)?;
    let output = texture(&device, wgpu::TextureFormat::Rgba16Float);
    let mut source =
        PathGpu::try_new_with_format(&device, 4, wgpu::TextureFormat::Rgba16Float).unwrap();
    assert!(source.render_premultiplied(
        &device,
        &queue,
        &output.create_view(&Default::default()),
        [32, 32],
        prepared.ops()
    ));
    let mut destination =
        PathGpu::try_new_with_format(&device, 4, wgpu::TextureFormat::Rgba16Float).unwrap();
    let image = destination.register_texture(output, Color::LinearSrgb, Alpha::Premultiplied)?;
    assert_eq!(image.byte_len(), 4096);
    let mut c = GpuCanvas::new_linear(32, 32, None);
    c.draw_image(
        &CanvasImageSource::Region {
            image: &image,
            rect: [8, 8, 16, 16],
        },
        8.,
        8.,
        16.,
        16.,
    );
    let target = texture(&device, wgpu::TextureFormat::Rgba16Float);
    assert!(destination.render_premultiplied(
        &device,
        &queue,
        &target.create_view(&Default::default()),
        [32, 32],
        &c.into_ops()
    ));
    let pixels = super::advanced_tests::read_float(&device, &queue, &target, [32, 32]);
    let p = &pixels[(16 * 32 + 16) * 4..][..4];
    assert!(
        (p[0] - 2.).abs() < 0.02 && (p[1] + 0.125).abs() < 0.01 && (p[3] - 0.5).abs() < 0.01,
        "{p:?}"
    );
    let invalid = texture(&device, wgpu::TextureFormat::Rgba8UnormSrgb);
    assert!(destination
        .register_texture(invalid, Color::Srgb, Alpha::Straight)
        .is_err());
    let mut cpu = RasterCanvas::new_hdr(32, 32);
    cpu.draw_image(&CanvasImageSource::Image(&image), 0., 0., 32., 32.);
    assert!(cpu.into_linear_rgba32f().iter().all(|v| *v == 0.));
    assert!(destination.unregister_texture(&image));
    let compact = region.render_gpu(&scene, &mut destination, &device, &queue)?;
    let mut c = GpuCanvas::new_linear(32, 32, None);
    compact.draw(&mut c);
    assert!(destination.render_premultiplied(
        &device,
        &queue,
        &target.create_view(&Default::default()),
        [32, 32],
        &c.into_ops()
    ));
    let data = super::advanced_tests::read_float(&device, &queue, &target, [32, 32]);
    assert!((data[(8 * 32 + 8) * 4] - 2.).abs() < 0.02);
    drop(compact);
    drop(image);
    assert_eq!(destination.prune_registered_textures(), 1);
    Ok(())
}
#[test]
#[ignore = "requires native GPU"]
fn prepared_outline_controls_gpu_pixels() -> Result<(), Box<dyn Error>> {
    let TestGpu {
        device,
        queue,
        backend,
        ..
    } = test_gpu();
    let font = Font2D::new(
        include_bytes!("../../../jadix_cosmic_text/fonts/Inter-Regular.ttf").as_slice(),
        0,
    )?;
    let glyphs = [PositionedGlyph {
        id: font.glyph_id('H').unwrap(),
        x: 5.,
        y: 26.,
    }];
    let mut rows = Vec::new();
    for hint in [false, true] {
        for amount in [[0., 0.], [0.5, 0.8], [-0.1, -0.1]] {
            let run = GlyphRun::with_options(
                &font,
                19.,
                &glyphs,
                &GlyphRunOptions {
                    hint,
                    embolden: FontEmbolden {
                        amount,
                        ..Default::default()
                    },
                    glyph_transform: [1., 0., 0.15, 1., 0., 0.],
                    ..Default::default()
                },
            )?;
            let mut gpu = GpuCanvas::new_linear(32, 32, None);
            gpu.set_fill("white");
            gpu.fill_glyph_run(&run);
            let ops = gpu.into_ops();
            let mut cpu = RasterCanvas::new_linear(32, 32);
            cpu.set_fill("white");
            cpu.fill_glyph_run(&run);
            let expected = cpu.get_image_data(0, 0, 32, 32).unwrap();
            let target = texture(&device, wgpu::TextureFormat::Rgba8UnormSrgb);
            let mut renderer = PathGpu::try_new(&device, 4).unwrap();
            renderer.set_aa_quality(gpu::AaQuality::Reference);
            assert!(renderer.render(
                &device,
                &queue,
                &target.create_view(&Default::default()),
                [32, 32],
                &ops
            ));
            let actual = readback(&device, &queue, &target, 32)?;
            let error = actual
                .chunks_exact(4)
                .zip(expected.chunks_exact(4))
                .map(|(a, b)| f64::from(a[3].abs_diff(b[3])))
                .sum::<f64>()
                / 1024.;
            assert!(error < 2., "{hint}/{amount:?}: {error}");
            rows.push(json!({"hint":hint,"amount":amount,"alpha_mae":error}));
        }
    }
    if let Some(dir) = std::env::var_os("JADIX_2D_EVIDENCE") {
        std::fs::create_dir_all(&dir)?;
        std::fs::write(
            Path::new(&dir).join("outline-controls.json"),
            serde_json::to_vec_pretty(&json!({"backend":format!("{backend:?}"),"cases":rows}))?,
        )?;
    }
    Ok(())
}
