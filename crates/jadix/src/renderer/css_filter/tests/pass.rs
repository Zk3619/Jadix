use super::*;
use crate::renderer::offscreen::{readback_texture, OffscreenGpu};

#[test]
#[ignore = "native GPU all filter modes versus dynamic shader; run explicitly"]
fn specialized_filter_modes_match_dynamic_shader() {
    let backend = if std::env::var("JADIX_UPLOAD_BACKEND").as_deref() == Ok("dx11") {
        wgpu::Backends::DX11
    } else {
        wgpu::Backends::GL
    };
    let gpu = OffscreenGpu::with_backends(backend).expect("requested GPU");
    eprintln!("filter specialization adapter={:?}", gpu.adapter_info);
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        width: 16,
        height: 16,
        present_mode: wgpu::PresentMode::Fifo,
        desired_maximum_frame_latency: 1,
        alpha_mode: wgpu::CompositeAlphaMode::Auto,
        color_space: Default::default(),
        view_formats: vec![],
    };
    let textured = TexturedQuadPipeline::new(&gpu.device, &config);
    let pixels: Vec<u8> = (0..16 * 16 * 4)
        .map(|i| (((i * 73) ^ (i / 17)) & 255) as u8)
        .collect();
    let input = crate::app::image_registry::upload_rgba8(
        &gpu.device,
        &gpu.queue,
        &textured,
        &pixels,
        16,
        16,
    );
    let output = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("filter mode comparison"),
        size: wgpu::Extent3d {
            width: 16,
            height: 16,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: config.format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let images = HashMap::from([
        ("source".into(), input),
        (
            "output".into(),
            GpuImageTexture {
                bind_group: textured.create_texture_bind_group(
                    &gpu.device,
                    &output.create_view(&Default::default()),
                ),
                _texture: output,
            },
        ),
    ]);
    let mut specialized = CssFilterPassGpu::new(&gpu.device, config.format);
    let mut dynamic = CssFilterPassGpu::new(&gpu.device, config.format);
    dynamic.force_dynamic_pipeline = true;
    assert!(
        specialized.pipelines.is_empty(),
        "unused filters must not compile a pipeline"
    );
    assert!(!specialized.flush_filter_jobs(
        &gpu.device,
        &gpu.queue,
        &textured,
        &mut HashMap::new(),
        &[],
        1.0,
        None
    ));
    assert!(specialized.pipelines.is_empty());
    specialized.ensure_fallback(&gpu.device, &textured);
    dynamic.ensure_fallback(&gpu.device, &textured);
    for mode in (0..18).chain((0..18).rev()) {
        let mut u = FilterUniforms {
            mode: mode as f32,
            p0: 1.0,
            p1: 1.0,
            texel: [1.0 / 16.0; 2],
            color: [0.2, 0.4, 0.8, 0.7],
            m0: [1.0, 0.0, 0.0, 0.0],
            m1: [0.0, 1.0, 0.0, 0.0],
            m2: [0.0, 0.0, 1.0, 0.0],
            m3: [0.0, 0.0, 0.0, 1.0],
            ..Default::default()
        };
        match mode {
            2 | 3 => u.set_gaussian(1.2),
            10 => {
                u.p0 = 3.0;
                u.p1 = 3.0;
                u.p2 = 1.0;
                u.p3 = 1.0;
                u.p4 = 9.0;
                u.k0 = [1.0; 4];
                u.k1 = [1.0; 4];
                u.k2 = [1.0, 0.0, 0.0, 0.0];
            }
            13 => {
                u.p0 = 0.04;
                u.p1 = 0.05;
                u.p2 = 2.0;
                u.p3 = 0.123;
            }
            15 | 16 => {
                u.p2 = 0.5;
                u.p3 = 1.0;
                u.p4 = 0.4;
                u.p5 = 3.0;
            }
            _ => {}
        }
        let run = |filter: &mut CssFilterPassGpu| {
            assert!(filter.draw(
                &gpu.device,
                &gpu.queue,
                &textured,
                &images,
                "source",
                "source",
                "output",
                &u
            ));
            filter.submit_pending(&gpu.queue);
            readback_texture(
                &gpu.device,
                &gpu.queue,
                &images["output"]._texture,
                16,
                16,
                gpu.device.create_command_encoder(&Default::default()),
            )
            .unwrap()
            .pixels
        };
        let actual = run(&mut specialized);
        let expected = run(&mut dynamic);
        let delta = actual
            .iter()
            .zip(&expected)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        assert!(delta <= 1, "mode={mode} max byte delta={delta}");
    }
    assert_eq!(specialized.pipelines.len(), 18);
    assert_eq!(dynamic.pipelines.len(), 1);
}

#[test]
#[ignore = "native GPU paired Gaussian versus original discrete taps; run explicitly"]
fn paired_gaussian_matches_discrete_taps() {
    let source = include_str!("../filter_pass.wgsl");
    let start = source.find("fn gaussian_1d(").unwrap();
    let end = start + source[start + 4..].find("\nfn ").unwrap() + 4;
    let mut reference_source = source.to_string();
    reference_source.replace_range(start..end, include_str!("../gaussian_reference.wgsl"));
    for backend in [wgpu::Backends::GL, wgpu::Backends::DX12] {
        if backend == wgpu::Backends::DX12 && !cfg!(feature = "backend-dx12") {
            continue;
        }
        let gpu = OffscreenGpu::with_backends(backend).expect("native GPU adapter");
        let (w, h) = (67_u32, 49_u32);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            width: w,
            height: h,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            color_space: Default::default(),
            view_formats: vec![],
        };
        let textured = TexturedQuadPipeline::new(&gpu.device, &config);
        let rgba: Vec<u8> = (0..w * h * 4)
            .map(|i| ((i.wrapping_mul(73) ^ (i / 17)) & 255) as u8)
            .collect();
        let input = crate::app::image_registry::upload_rgba8(
            &gpu.device,
            &gpu.queue,
            &textured,
            &rgba,
            w,
            h,
        );
        let mut optimized = CssFilterPassGpu::new(&gpu.device, config.format);
        let mut reference =
            CssFilterPassGpu::with_shader(&gpu.device, config.format, &reference_source);
        for sigma in [0.01_f32, 0.3, 0.5, 1.0, 2.0, 3.0, 5.1, 8.0, 18.0] {
            let graph = crate::style::resolve_filter_list_to_graph(
                &crate::style::parse_filter(&format!("blur({sigma}px)")).unwrap(),
            )
            .unwrap();
            let run = |filter: &mut CssFilterPassGpu| {
                filter.completed_outputs.clear();
                let mut images = HashMap::from([(
                    "source".into(),
                    GpuImageTexture {
                        _texture: input._texture.clone(),
                        bind_group: input.bind_group.clone(),
                    },
                )]);
                assert!(filter.flush_filter_jobs(
                    &gpu.device,
                    &gpu.queue,
                    &textured,
                    &mut images,
                    &[FilterJob {
                        graph: graph.clone(),
                        input_key: "source".into(),
                        output_key: "output".into(),
                        bounds: [0.0, 0.0, w as f32, h as f32],
                    }],
                    1.0,
                    None
                ));
                readback_texture(
                    &gpu.device,
                    &gpu.queue,
                    &images["output"]._texture,
                    w,
                    h,
                    gpu.device.create_command_encoder(&Default::default()),
                )
                .unwrap()
                .pixels
            };
            let a = run(&mut optimized);
            let b = run(&mut reference);
            let delta = a.iter().zip(&b).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
            assert!(
                delta <= 1,
                "{backend:?} sigma={sigma} max byte delta={delta}"
            );
        }
    }
}

#[test]
#[ignore = "native GPU named-result lifetime pixel comparison; run explicitly"]
fn transient_filter_results_match_persistent_snapshots() {
    let gpu = OffscreenGpu::with_backends(wgpu::Backends::GL).expect("native GL GPU");
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        width: 32,
        height: 32,
        present_mode: wgpu::PresentMode::Fifo,
        desired_maximum_frame_latency: 1,
        alpha_mode: wgpu::CompositeAlphaMode::Auto,
        color_space: Default::default(),
        view_formats: vec![],
    };
    let textured = TexturedQuadPipeline::new(&gpu.device, &config);
    let mut rgba = vec![0; 32 * 32 * 4];
    for y in 0..32 {
        for x in 0..32 {
            let i = (y * 32 + x) * 4;
            rgba[i..i + 4].copy_from_slice(&[
                (x * 7) as u8,
                (y * 7) as u8,
                120,
                if x < 8 || y < 8 { 0 } else { 160 },
            ]);
        }
    }
    let input =
        crate::app::image_registry::upload_rgba8(&gpu.device, &gpu.queue, &textured, &rgba, 32, 32);
    let graphs = [
        "grayscale(0.4) brightness(0.7) contrast(1.2) saturate(0.8)",
        "drop-shadow(2px 3px 2px #123456) grayscale(0.5) drop-shadow(-2px 1px 1px #abcdef)",
        "blur(2px) sepia(0.8) hue-rotate(45deg) opacity(0.6)",
    ];
    for css in graphs {
        let graph =
            crate::style::resolve_filter_list_to_graph(&crate::style::parse_filter(css).unwrap())
                .unwrap();
        let run = |persist: bool| {
            let mut filters = CssFilterPassGpu::new(&gpu.device, config.format);
            filters.force_named_snapshots = persist;
            let mut images = HashMap::from([(
                "source".into(),
                GpuImageTexture {
                    _texture: input._texture.clone(),
                    bind_group: input.bind_group.clone(),
                },
            )]);
            let job = FilterJob {
                graph: graph.clone(),
                input_key: "source".into(),
                output_key: "output".into(),
                bounds: [0.0, 0.0, 32.0, 32.0],
            };
            assert!(filters.flush_filter_jobs(
                &gpu.device,
                &gpu.queue,
                &textured,
                &mut images,
                &[job],
                1.0,
                None
            ));
            let snapshots = filters
                .target_size
                .keys()
                .filter(|key| key.starts_with("jadix_filter_rt:"))
                .count();
            let output = readback_texture(
                &gpu.device,
                &gpu.queue,
                &images["output"]._texture,
                32,
                32,
                gpu.device.create_command_encoder(&Default::default()),
            )
            .unwrap();
            (output.pixels, snapshots)
        };
        let (optimized, fewer) = run(false);
        let (reference, all) = run(true);
        assert_eq!(optimized, reference, "named dependencies changed: {css}");
        assert!(
            fewer < all,
            "should eliminate temporary snapshots: {css}, {fewer}/{all}"
        );
        if css.starts_with("drop-shadow") {
            assert!(
                fewer > 0,
                "branching drop-shadow must preserve results across reuse"
            );
        }
    }
}

#[test]
#[ignore = "native GPU filter batch pixel comparison; run explicitly"]
fn batched_filters_preserve_uniforms_dependencies_and_pending_sources() {
    let gpu = OffscreenGpu::with_backends(wgpu::Backends::GL).expect("native GL GPU");
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        width: 32,
        height: 32,
        present_mode: wgpu::PresentMode::Fifo,
        desired_maximum_frame_latency: 2,
        alpha_mode: wgpu::CompositeAlphaMode::Auto,
        color_space: Default::default(),
        view_formats: vec![],
    };
    let textured = TexturedQuadPipeline::new(&gpu.device, &config);
    let run = |batch: bool| {
        let mut filter = CssFilterPassGpu::new(&gpu.device, config.format);
        let mut images = HashMap::new();
        filter.ensure_fallback(&gpu.device, &textured);
        for key in ["input", "red", "shifted", "blue"] {
            filter.ensure_target(&gpu.device, &textured, &mut images, key, 32, 32);
        }
        let mut flood = FilterUniforms {
            mode: 5.0,
            color: [1.0, 0.0, 0.0, 1.0],
            ..Default::default()
        };
        assert!(filter.draw(
            &gpu.device,
            &gpu.queue,
            &textured,
            &images,
            "input",
            "input",
            "red",
            &flood
        ));
        if !batch {
            filter.submit_pending(&gpu.queue);
        }
        let offset = FilterUniforms {
            mode: 1.0,
            m0: [0.5, 0.0, 0.0, 0.0],
            m1: [0.0, 1.0, 0.0, 0.0],
            m2: [0.0, 0.0, 1.0, 0.0],
            m3: [0.0, 0.0, 0.0, 1.0],
            ..Default::default()
        };
        assert!(filter.draw(
            &gpu.device,
            &gpu.queue,
            &textured,
            &images,
            "red",
            "red",
            "shifted",
            &offset
        ));
        if !batch {
            filter.submit_pending(&gpu.queue);
        }
        flood.color = [0.0, 0.0, 1.0, 0.5];
        assert!(filter.draw(
            &gpu.device,
            &gpu.queue,
            &textured,
            &images,
            "input",
            "input",
            "blue",
            &flood
        ));
        // Cross the initial uniform capacity with distinct snapshots. A
        // growing batch must preserve both its earlier outputs and its tail.
        for step in 0..20 {
            flood.color = [step as f32 / 19.0, 0.0, 1.0, 0.5];
            assert!(filter.draw(
                &gpu.device,
                &gpu.queue,
                &textured,
                &images,
                "input",
                "input",
                "blue",
                &flood
            ));
            if !batch {
                filter.submit_pending(&gpu.queue);
            }
        }
        filter.submit_pending(&gpu.queue);
        let mut result = Vec::new();
        for key in ["red", "shifted", "blue"] {
            let encoder = gpu.device.create_command_encoder(&Default::default());
            result.push(
                readback_texture(
                    &gpu.device,
                    &gpu.queue,
                    &images[key]._texture,
                    32,
                    32,
                    encoder,
                )
                .unwrap()
                .pixels,
            );
        }
        assert!(result[0]
            .chunks_exact(4)
            .any(|px| px[0] > 200 && px[3] > 200));
        assert_ne!(
            result[0], result[1],
            "color matrix must change its preceding source"
        );
        assert_ne!(result[0], result[2], "uniform snapshots must differ");

        let direct_job = FilterJob {
            graph: crate::style::resolve_filter_list_to_graph(
                &crate::style::parse_filter("brightness(0.5)").unwrap(),
            )
            .unwrap(),
            input_key: "red".into(),
            output_key: "direct".into(),
            bounds: [0.0, 0.0, 32.0, 32.0],
        };
        assert!(filter.flush_filter_jobs(
            &gpu.device,
            &gpu.queue,
            &textured,
            &mut images,
            std::slice::from_ref(&direct_job),
            1.0,
            None
        ));
        assert!(
            !filter
                .target_size
                .keys()
                .any(|key| key.starts_with("jadix_filter_tmp:direct:")
                    || key.starts_with("jadix_filter_rt:direct:")),
            "single primitive needs no intermediate targets"
        );
        let encoder = gpu.device.create_command_encoder(&Default::default());
        let direct = readback_texture(
            &gpu.device,
            &gpu.queue,
            &images["direct"]._texture,
            32,
            32,
            encoder,
        )
        .unwrap();
        assert_eq!(
            direct.pixels, result[1],
            "direct brightness must match the separately captured matrix"
        );

        let job = FilterJob {
            graph: Default::default(),
            input_key: "late".into(),
            output_key: "ready".into(),
            bounds: [0.0, 0.0, 32.0, 32.0],
        };
        assert!(!filter.flush_filter_jobs(
            &gpu.device,
            &gpu.queue,
            &textured,
            &mut images,
            std::slice::from_ref(&job),
            1.0,
            None
        ));
        assert!(!images.contains_key("ready"));
        assert!(!filter.completed_outputs.contains("ready"));
        let late = images.remove("red").unwrap();
        images.insert("late".into(), late);
        assert!(filter.flush_filter_jobs(
            &gpu.device,
            &gpu.queue,
            &textured,
            &mut images,
            std::slice::from_ref(&job),
            1.0,
            None
        ));
        let encoder = gpu.device.create_command_encoder(&Default::default());
        let ready = readback_texture(
            &gpu.device,
            &gpu.queue,
            &images["ready"]._texture,
            32,
            32,
            encoder,
        )
        .unwrap();
        assert_eq!(
            ready.pixels, result[0],
            "ready source must replace the pending result"
        );
        result
    };
    assert_eq!(
        run(true),
        run(false),
        "batch must match separate submits byte for byte"
    );
}
