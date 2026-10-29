use super::*;
use crate::renderer::offscreen::{readback_texture, OffscreenGpu};

#[test]
#[ignore = "native GPU capture pixel comparison; run explicitly"]
fn image_capture_batch_matches_separate_sizes_dependencies_and_pending_sources() {
    let backend = if std::env::var("JADIX_GPU_BACKEND").as_deref() == Ok("dx12") {
        wgpu::Backends::DX12
    } else {
        wgpu::Backends::GL
    };
    let gpu = OffscreenGpu::with_backends(backend).expect("native GPU");
    let mut rgba_reference = None;
    for format in [
        wgpu::TextureFormat::Rgba8UnormSrgb,
        wgpu::TextureFormat::Bgra8UnormSrgb,
    ] {
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: 64,
            height: 64,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 1,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            color_space: Default::default(),
            view_formats: vec![],
        };
        let textured = TexturedQuadPipeline::new(&gpu.device, &config);
        let rect = super::super::JadixPipeline::new(&gpu.device, &config);
        let run = |batch: bool| {
            let mut pass = EffectPassGpu::new(&gpu.device, config.format);
            let mut images = HashMap::new();
            let mut rgba = vec![0; 8 * 8 * 4];
            for y in 0..8 {
                for x in 0..8 {
                    let i = (y * 8 + x) * 4;
                    rgba[i..i + 4].copy_from_slice(&[
                        (x * 31) as u8,
                        (y * 31) as u8,
                        180,
                        if x < 4 { 255 } else { 96 },
                    ]);
                }
            }
            images.insert(
                "source".into(),
                crate::app::image_registry::upload_rgba8(
                    &gpu.device,
                    &gpu.queue,
                    &textured,
                    &rgba,
                    8,
                    8,
                ),
            );
            let capture = |key: &str, source: &str, bounds: [f32; 4], alpha: f32| {
                let mut draw = super::super::TexturedDraw::clipped(
                    [
                        bounds[0] + 2.0,
                        bounds[1] + 1.0,
                        bounds[2] - 4.0,
                        bounds[3] - 2.0,
                    ],
                    None,
                    source,
                    alpha,
                )
                .unwrap();
                draw.tint_rgb = [0.75, 1.0, 0.5];
                CaptureJob {
                    key: key.into(),
                    bounds,
                    instances: vec![],
                    textured_draws: vec![draw],
                    text_nodes: vec![],
                }
            };
            let mut sdf = <InstanceInput as bytemuck::Zeroable>::zeroed();
            sdf.rect_pos = [7.0, 11.0];
            sdf.rect_size = [23.0, 17.0];
            sdf.color = [0.1, 0.8, 0.3, 1.0];
            sdf.transform = crate::style::transform::MAT2D_IDENTITY;
            sdf.clip_size = InstanceInput::NO_CLIP_SIZE;
            let captures = vec![
                capture(
                    "jadix_filter_cap:a",
                    "source",
                    [10.5, 20.5, 19.0, 13.0],
                    1.0,
                ),
                capture(
                    "jadix_filter_cap:b",
                    "jadix_filter_cap:a",
                    [30.5, 40.5, 33.0, 29.0],
                    0.65,
                ),
                CaptureJob {
                    key: "sdf".into(),
                    bounds: [7.0, 11.0, 23.0, 17.0],
                    instances: vec![sdf],
                    textured_draws: vec![],
                    text_nodes: vec![],
                },
                capture("jadix_filter_cap:c", "sdf", [3.5, 5.5, 41.0, 17.0], 0.8),
                capture(
                    "jadix_filter_cap:pending",
                    "late",
                    [2.0, 4.0, 27.0, 21.0],
                    1.0,
                ),
            ];
            let mut flush = |jobs: &[CaptureJob], images: &mut HashMap<String, GpuImageTexture>| {
                pass.flush(
                    &gpu.device,
                    &gpu.queue,
                    &textured,
                    images,
                    jobs,
                    &[],
                    0.0,
                    [0.0; 2],
                    1.5,
                    &rect,
                    None,
                    None,
                )
            };
            if batch {
                assert!(flush(&captures, &mut images));
            } else {
                for cap in &captures {
                    let ran = flush(std::slice::from_ref(cap), &mut images);
                    assert_eq!(ran, cap.key != "jadix_filter_cap:pending");
                }
            }
            assert!(!images.contains_key("jadix_filter_cap:pending"));
            let late = images.remove("source").unwrap();
            images.insert("late".into(), late);
            assert!(flush(&captures[4..], &mut images));
            drop(flush);
            let mut result = vec![];
            for cap in &captures {
                assert_eq!(images[&cap.key]._texture.format(), format);
                let w = (cap.bounds[2] * 1.5).round() as u32;
                let h = (cap.bounds[3] * 1.5).round() as u32;
                let mut output = readback_texture(
                    &gpu.device,
                    &gpu.queue,
                    &images[&cap.key]._texture,
                    w,
                    h,
                    gpu.device.create_command_encoder(&Default::default()),
                )
                .unwrap();
                if format == wgpu::TextureFormat::Bgra8UnormSrgb {
                    for pixel in output.pixels.chunks_exact_mut(4) {
                        pixel.swap(0, 2);
                    }
                }
                assert!(
                    output.pixels.chunks_exact(4).any(|p| p[3] > 20),
                    "{} must paint",
                    cap.key
                );
                if batch && cap.key != "sdf" {
                    assert!(
                        output.pixels.chunks_exact(4).any(|p| p[3] == 0),
                        "{} keeps its transparent margins",
                        cap.key
                    );
                }
                if batch {
                    if let Ok(dir) = std::env::var("JADIX_CAPTURE_TEST_OUTPUT") {
                        let name = cap.key.replace(':', "-");
                        image::save_buffer(
                            std::path::Path::new(&dir).join(format!("{name}.png")),
                            &output.pixels,
                            w,
                            h,
                            image::ColorType::Rgba8,
                        )
                        .unwrap();
                    }
                }
                result.push(output.pixels);
            }
            result
        };
        let batched = run(true);
        assert_eq!(
            batched,
            run(false),
            "batched captures must preserve every pixel and command dependency"
        );
        if let Some(reference) = &rgba_reference {
            assert_eq!(
                &batched, reference,
                "capture colors must match across target formats"
            );
        } else {
            rgba_reference = Some(batched);
        }
    }
}
