use super::*;
use gpu::{GpuImageAlpha as Alpha, GpuImageColor as Color};
fn output(device: &wgpu::Device) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("gpu-mips-out"),
        size: wgpu::Extent3d {
            width: 32,
            height: 32,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}
#[test]
#[ignore = "requires native GPU"]
fn imported_region_mips_gpu_only_npot_alpha_dirty_and_prepared_scenes() -> Result<(), Box<dyn Error>>
{
    let TestGpu { device, queue, .. } = test_gpu();
    let target = output(&device);
    let mut rows = vec![];
    for copy_only in [false, true] {
        for format_case in 0..3 {
            let format = if format_case == 0 {
                wgpu::TextureFormat::Rgba8Unorm
            } else {
                wgpu::TextureFormat::Rgba16Float
            };
            let alpha = if format_case == 2 {
                Alpha::Premultiplied
            } else {
                Alpha::Straight
            };
            let color = if format_case == 0 {
                Color::Srgb
            } else {
                Color::LinearSrgb
            };
            let external = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("external-atlas"),
                size: wgpu::Extent3d {
                    width: 39,
                    height: 23,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::COPY_DST
                    | if copy_only {
                        wgpu::TextureUsages::COPY_SRC
                    } else {
                        wgpu::TextureUsages::TEXTURE_BINDING
                    },
                view_formats: &[],
            });
            let mut expected = vec![];
            let mut raw = vec![];
            for y in 0..23 {
                for x in 0..39 {
                    let inside = (3..34).contains(&x) && (2..19).contains(&y);
                    let i = (y - 2i32).max(0) * 31 + (x - 3i32).max(0);
                    let a = if inside { (i % 3) as f32 * 0.5 } else { 1. };
                    let mut p = if inside {
                        [if i % 2 == 0 { 1. } else { 0. }, 0.25, 0.75, a]
                    } else {
                        [0., 1., 0., 1.]
                    };
                    if format_case > 0 {
                        p[0] *= 3.;
                        p[1] = -0.25;
                        p[2] = 1.2;
                    }
                    if format_case == 2 {
                        for v in &mut p[..3] {
                            *v *= a;
                        }
                    }
                    let decoded = if format_case == 0 {
                        let bytes = p.map(|v| (v * 255.).round() as u8);
                        raw.extend(bytes);
                        let mut p = bytes.map(|v| v as f32 / 255.);
                        for v in &mut p[..3] {
                            *v = jadix_theme::color_space::linearize_display_srgb_component(*v);
                        }
                        p
                    } else {
                        let v = p.map(half::f16::from_f32);
                        raw.extend(v.iter().flat_map(|v| v.to_le_bytes()));
                        v.map(|v| v.to_f32())
                    };
                    if inside {
                        let mut p = decoded;
                        if alpha == Alpha::Premultiplied {
                            if p[3] > 0. {
                                let a = p[3];
                                for v in &mut p[..3] {
                                    *v /= a;
                                }
                            } else {
                                p = [0.; 4];
                            }
                        }
                        expected.extend(p);
                    }
                }
            }
            queue.write_texture(
                external.as_image_copy(),
                &raw,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(39 * if format_case == 0 { 4 } else { 8 }),
                    rows_per_image: Some(23),
                },
                external.size(),
            );
            let expected = Image2D::from_linear_rgba32f(31, 17, &expected)
                .unwrap()
                .with_mipmaps(Default::default())?;
            let mut renderer =
                PathGpu::try_new_with_format(&device, 1, wgpu::TextureFormat::Rgba16Float).unwrap();
            renderer.set_aa_quality(gpu::AaQuality::Area);
            let image = renderer.register_texture_region(
                external.clone(),
                [3, 2, 31, 17],
                color,
                alpha,
                Some(Default::default()),
            )?;
            assert_eq!([image.width(), image.height()], [31, 17]);
            assert!(image.rgba8().iter().all(|v| *v == 0));
            for quality in [
                ImageSampling::Trilinear,
                ImageSampling::Anisotropic,
                ImageSampling::Ewa,
            ] {
                let mut canvas = GpuCanvas::new_linear(32, 32, None);
                canvas.set_fill_brush(
                    Brush2D::image(image.clone())
                        .with_sampling(quality)
                        .with_transform([0.17, 0., 0., 0.51, 0., 0.])
                        .with_extend(ExtendMode::Repeat, ExtendMode::Reflect),
                );
                canvas.fill_rect(0., 0., 32., 32.);
                let ops = canvas.into_ops();
                assert!(renderer.render_premultiplied(
                    &device,
                    &queue,
                    &target.create_view(&Default::default()),
                    [32, 32],
                    &ops
                ));
                let cold = renderer.last_stats();
                if quality == ImageSampling::Trilinear {
                    assert_eq!(cold.mip_generation_passes, 4);
                    let limits = device.limits();
                    let compute = cfg!(feature = "compute")
                        && limits.max_compute_workgroup_size_x >= 8
                        && limits.max_compute_workgroup_size_y >= 8
                        && limits.max_compute_invocations_per_workgroup >= 64
                        && limits.max_storage_textures_per_shader_stage > 0;
                    assert_eq!(cold.compute_dispatches, if compute { 4 } else { 0 });
                    assert!(
                        cold.image_uploaded_bytes <= 1,
                        "only the renderer white mask may upload"
                    );
                }
                let actual = advanced_tests::read_float(&device, &queue, &target, [32, 32]);
                let mut max = 0f32;
                for y in 0..32 {
                    for x in 0..32 {
                        let e = expected.sample_linear_with_footprint(
                            [(x as f32 + 0.5) / 0.17, (y as f32 + 0.5) / 0.51],
                            [1. / 0.17, 0.],
                            [0., 1. / 0.51],
                            quality,
                            ExtendMode::Repeat,
                            ExtendMode::Reflect,
                        );
                        for k in 0..4 {
                            max = max.max((actual[(y * 32 + x) * 4 + k] - e[k]).abs());
                        }
                    }
                }
                assert!(
                    max < 0.012,
                    "copy{copy_only} format{format_case} {quality:?}: {max}"
                );
                assert!(renderer.render_premultiplied(
                    &device,
                    &queue,
                    &target.create_view(&Default::default()),
                    [32, 32],
                    &ops
                ));
                assert_eq!(renderer.last_stats().mip_generation_passes, 0);
                assert_eq!(renderer.last_stats().external_image_conversions, 0);
                rows.push(json!({"copy_only":copy_only,"format_case":format_case,"quality":format!("{quality:?}"),"max_error":max,"cold_compute_dispatches":cold.compute_dispatches,"warm_mip_passes":renderer.last_stats().mip_generation_passes}));
            }
            // Explicitly force the last (1x1) level: uniform mass and atlas isolation.
            let mut c = GpuCanvas::new_linear(32, 32, None);
            c.set_fill_brush(
                Brush2D::image(image.clone())
                    .with_sampling(ImageSampling::Trilinear)
                    .with_transform([0.0001, 0., 0., 0.0001, 0., 0.]),
            );
            c.fill_rect(0., 0., 32., 32.);
            assert!(renderer.render_premultiplied(
                &device,
                &queue,
                &target.create_view(&Default::default()),
                [32, 32],
                &c.into_ops()
            ));
            let coarse = advanced_tests::read_float(&device, &queue, &target, [32, 32]);
            let mass = expected
                .mip_level_premultiplied(expected.mip_level_count() - 1)
                .unwrap();
            assert!(coarse
                .chunks_exact(4)
                .all(|p| p.iter().zip(mass).all(|(a, b)| (a - b).abs() < 0.003)));
            // Sample all the way down to 1x1: NPOT chain must conserve premultiplied mass.
            let scene = Scene2D::record([32, 32], |c| {
                c.set_image_sampling(ImageSampling::Trilinear);
                c.draw_image(&CanvasImageSource::Image(&image), 0., 0., 2., 2.);
            })?;
            let sampled = scene.sampled(
                [32, 32],
                Default::default(),
                SceneSampling {
                    axis_samples: 2,
                    ..Default::default()
                },
            )?;
            let mut prepared = renderer.prepare_sampled_scene(
                &device,
                &sampled,
                wgpu::TextureFormat::Rgba16Float,
            )?;
            assert!(prepared.render(
                &device,
                &queue,
                &target.create_view(&Default::default()),
                true
            ));
            assert_eq!(prepared.last_stats().queue_submissions, 1);
            // Failed budgets do not replace a valid registry entry.
            assert!(renderer
                .set_registered_texture_mipmaps(
                    &image,
                    Some(MipmapLimits {
                        max_bytes: 1,
                        ..Default::default()
                    })
                )
                .is_err());
            assert_eq!(renderer.registered_texture_count(), 1);
            renderer.set_registered_texture_mipmaps(&image, None)?;
            let mut c = GpuCanvas::new_linear(32, 32, None);
            c.set_image_sampling(ImageSampling::Ewa);
            c.draw_image(&CanvasImageSource::Image(&image), 0., 0., 32., 32.);
            assert!(renderer.render_premultiplied(
                &device,
                &queue,
                &target.create_view(&Default::default()),
                [32, 32],
                &c.into_ops()
            ));
            assert_eq!(renderer.last_stats().mip_generation_passes, 0);
            // Update GPU region to a constant HDR/sRGB value; prepared snapshot dirtied separately.
            let bytes = if format_case == 0 {
                [255, 0, 0, 255].repeat(31 * 17)
            } else {
                [2f32, 0., 0., 1.]
                    .repeat(31 * 17)
                    .iter()
                    .flat_map(|v| half::f16::from_f32(*v).to_le_bytes())
                    .collect()
            };
            let mut dest = external.as_image_copy();
            dest.origin = wgpu::Origin3d { x: 3, y: 2, z: 0 };
            queue.write_texture(
                dest,
                &bytes,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(31 * if format_case == 0 { 4 } else { 8 }),
                    rows_per_image: Some(17),
                },
                wgpu::Extent3d {
                    width: 31,
                    height: 17,
                    depth_or_array_layers: 1,
                },
            );
            prepared.mark_override_image_dirty(&image)?;
            assert!(prepared.render(
                &device,
                &queue,
                &target.create_view(&Default::default()),
                true
            ));
            assert!(prepared.last_stats().mip_generation_passes > 0);
            let actual = advanced_tests::read_float(&device, &queue, &target, [32, 32]);
            assert!((actual[0] - if format_case == 0 { 1. } else { 2. }).abs() < 0.003);
            assert!(actual[1].abs() < 0.003);
        }
    }
    let out = std::env::var_os("JADIX_2D_EVIDENCE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    std::fs::create_dir_all(&out)?;
    std::fs::write(
        out.join("gpu-mipmap-matrix.json"),
        serde_json::to_vec_pretty(&json!({"cases":rows}))?,
    )?;
    Ok(())
}

#[test]
#[ignore = "requires native GPU"]
fn large_compound_area_and_sampled_scene_exceed_old_triangle_limit() -> Result<(), Box<dyn Error>> {
    let TestGpu { device, queue, .. } = test_gpu();
    let mut path = vec![];
    for y in 0..50 {
        for x in 0..64 {
            path.extend(filled_rect(
                x as f32 * 4. + 0.17,
                y as f32 * 4. + 0.23,
                2.71,
                2.63,
            ));
        }
    }
    let scene = Scene2D::record([256, 256], |c| {
        c.set_fill("#ff000080");
        c.fill_path(&path);
    })?;
    let mut canvas = GpuCanvas::new_linear(256, 256, None);
    scene.draw(&mut canvas, Default::default())?;
    let ops = canvas.into_ops();
    let tris = if let PathGpuOp::Fill(v) = &ops[0] {
        v.stencil.len()
    } else {
        0
    };
    assert!(tris > 4096);
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 256,
            height: 256,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let mut renderer =
        PathGpu::try_new_with_format(&device, 1, wgpu::TextureFormat::Rgba16Float).unwrap();
    renderer.set_aa_quality(gpu::AaQuality::Area);
    assert!(renderer.render_premultiplied(
        &device,
        &queue,
        &target.create_view(&Default::default()),
        [256, 256],
        &ops
    ));
    let stats = renderer.last_stats();
    let actual = advanced_tests::read_float(&device, &queue, &target, [256, 256]);
    let mut max = 0f32;
    for y in 0..256 {
        for x in 0..256 {
            let left = (x / 4 * 4) as f64 + 0.17;
            let top = (y / 4 * 4) as f64 + 0.23;
            let a = if y < 200 {
                (x as f64 + 1.).min(left + 2.71).max(left) - (x as f64).max(left)
            } else {
                0.
            };
            let b = (y as f64 + 1.).min(top + 2.63).max(top) - (y as f64).max(top);
            let e = (a.max(0.) * b.max(0.) * 128. / 255.) as f32;
            max = max.max((actual[(y * 256 + x) * 4 + 3] - e).abs());
        }
    }
    assert!(max < 0.001, "area error {max}");
    let sampled = scene.sampled(
        [256, 256],
        Default::default(),
        SceneSampling {
            axis_samples: 2,
            ..Default::default()
        },
    )?;
    let expected = sampled.rasterize()?;
    let mut prepared = sampled.prepare_gpu(&device, wgpu::TextureFormat::Rgba16Float)?;
    assert!(prepared.render(
        &device,
        &queue,
        &target.create_view(&Default::default()),
        true
    ));
    let actual = advanced_tests::read_float(&device, &queue, &target, [256, 256]);
    let sample_error = actual
        .chunks_exact(4)
        .zip(expected.linear_rgba32f().unwrap().chunks_exact(4))
        .map(|(a, e)| (a[3] - e[3]).abs())
        .fold(0f32, f32::max);
    assert!(sample_error < 0.001);
    let out = std::env::var_os("JADIX_2D_EVIDENCE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    std::fs::create_dir_all(&out)?;
    std::fs::write(
        out.join("large-compound.json"),
        serde_json::to_vec_pretty(
            &json!({"original_triangles":tris,"uploaded_tile_triangles":stats.area_triangles,"tile_instances":stats.area_primitives,"max_independent_area_alpha_error":max,"max_sample_alpha_error":sample_error,"single_compound_path":true}),
        )?,
    )?;
    Ok(())
}

#[test]
#[ignore = "requires native GPU"]
fn generated_mips_lifecycle_regions_and_invalid_budgets_are_bounded() -> Result<(), Box<dyn Error>>
{
    let TestGpu { device, queue, .. } = test_gpu();
    let target = output(&device);
    let source = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 35,
            height: 35,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    queue.write_texture(
        source.as_image_copy(),
        &[255, 64, 0, 128].repeat(35 * 35),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(35 * 4),
            rows_per_image: Some(35),
        },
        source.size(),
    );
    let mut renderer =
        PathGpu::try_new_with_format(&device, 1, wgpu::TextureFormat::Rgba16Float).unwrap();
    renderer.set_aa_quality(gpu::AaQuality::Area);
    for region in [[0, 0, 0, 1], [u32::MAX, 0, 2, 2], [34, 0, 2, 1]] {
        assert!(renderer
            .register_texture_region(
                source.clone(),
                region,
                Color::Srgb,
                Alpha::Straight,
                Some(Default::default())
            )
            .is_err());
    }
    for limits in [
        MipmapLimits {
            max_bytes: 0,
            max_levels: 3,
        },
        MipmapLimits {
            max_bytes: usize::MAX,
            max_levels: 3,
        },
        MipmapLimits {
            max_bytes: 65536,
            max_levels: 0,
        },
        MipmapLimits {
            max_bytes: 1,
            max_levels: 1,
        },
    ] {
        assert!(renderer
            .register_texture_region(
                source.clone(),
                [1, 1, 32, 32],
                Color::Srgb,
                Alpha::Straight,
                Some(limits)
            )
            .is_err());
    }
    assert_eq!(renderer.registered_texture_count(), 0);
    let mut peak_cached = 0;
    let mut generations = 0;
    for frame in 0..24 {
        let limits = MipmapLimits {
            max_levels: if frame % 2 == 0 { 2 } else { 32 },
            ..Default::default()
        };
        let handle = renderer.register_texture_region(
            source.clone(),
            [1, 1, 32, 32],
            Color::Srgb,
            Alpha::Straight,
            Some(limits),
        )?;
        // A region-backed alpha mask remains base-level and never builds color mips.
        let mut c = GpuCanvas::new_linear(32, 32, None);
        c.push_layer_with_options(LayerOptions {
            mask: Some(handle.clone()),
            ..Default::default()
        });
        c.set_fill("#ffffff");
        c.fill_rect(0., 0., 32., 32.);
        c.pop_layer();
        assert!(renderer.render_premultiplied(
            &device,
            &queue,
            &target.create_view(&Default::default()),
            [32, 32],
            &c.into_ops()
        ));
        assert_eq!(renderer.last_stats().mip_generation_passes, 0);
        let pixels = advanced_tests::read_float(&device, &queue, &target, [32, 32]);
        assert!((pixels[3] - 128. / 255.).abs() < 0.002);
        let mut c = GpuCanvas::new_linear(32, 32, None);
        c.set_image_sampling(ImageSampling::Ewa);
        c.draw_image(&CanvasImageSource::Image(&handle), 0., 0., 32., 32.);
        assert!(renderer.render_premultiplied(
            &device,
            &queue,
            &target.create_view(&Default::default()),
            [32, 32],
            &c.into_ops()
        ));
        let stats = renderer.last_stats();
        assert_eq!(
            stats.mip_generation_passes,
            if frame % 2 == 0 { 1 } else { 5 }
        );
        generations += stats.mip_generation_passes;
        peak_cached = peak_cached.max(stats.cached_images);
        drop(handle);
        assert_eq!(renderer.prune_registered_textures(), 1);
        assert_eq!(renderer.registered_texture_count(), 0);
    }
    assert!(peak_cached <= 3);
    let out = std::env::var_os("JADIX_2D_EVIDENCE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    std::fs::create_dir_all(&out)?;
    std::fs::write(
        out.join("gpu-mip-lifecycle.json"),
        serde_json::to_vec_pretty(
            &json!({"frames":24,"mip_generation_passes":generations,"peak_cached_images":peak_cached,"live_registrations_at_end":renderer.registered_texture_count(),"scope":"bounded renderer cache counters, not complete RSS/VRAM high water"}),
        )?,
    )?;
    Ok(())
}

#[test]
#[ignore = "requires native GPU"]
fn oblique_elliptical_sampling_reference_images() -> Result<(), Box<dyn Error>> {
    let TestGpu { device, queue, .. } = test_gpu();
    let out = std::env::var_os("JADIX_2D_EVIDENCE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    std::fs::create_dir_all(&out)?;
    let pixels = (0..127 * 65)
        .flat_map(|i| {
            [
                if i % 127 % 2 == 0 { 255 } else { 0 },
                if i / 127 < 32 { 255 } else { 0 },
                ((i % 127) as f32 / 126. * 255.).round() as u8,
                255,
            ]
        })
        .collect::<Vec<u8>>();
    let image = Image2D::from_rgba8(127, 65, &pixels)
        .unwrap()
        .with_mipmaps(Default::default())?;
    let mut renderer = PathGpu::try_new(&device, 1).unwrap();
    renderer.set_aa_quality(gpu::AaQuality::Area);
    for size in [32, 64] {
        let target = scene_tests::texture(&device, size);
        for quality in [
            ImageSampling::Trilinear,
            ImageSampling::Anisotropic,
            ImageSampling::Ewa,
        ] {
            let mut c = GpuCanvas::new_linear(size, size, None);
            c.set_fill_brush(
                Brush2D::image(image.clone())
                    .with_transform([0.13, 0.07, -0.11, 0.4, 3., 4.])
                    .with_extend(ExtendMode::Repeat, ExtendMode::Reflect)
                    .with_sampling(quality),
            );
            c.fill_rect(0., 0., size as f32, size as f32);
            assert!(renderer.render(
                &device,
                &queue,
                &target.create_view(&Default::default()),
                [size, size],
                &c.into_ops()
            ));
            let pixels = readback(&device, &queue, &target, size)?;
            image::save_buffer(
                out.join(format!("sampling-oblique-{quality:?}-{size}-gpu.png")),
                &pixels,
                size,
                size,
                image::ColorType::Rgba8,
            )?;
        }
    }
    Ok(())
}
