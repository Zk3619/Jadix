use super::*;

pub(super) fn verify(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    out: &std::path::Path,
) -> Result<(), Box<dyn Error>> {
    use jadix_renderer::compute::{ComputeBlur, ComputeHistogram, ComputeParticles};
    let source = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("compute-effects-source"),
        size: wgpu::Extent3d {
            width: 16,
            height: 16,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = source.create_view(&Default::default());
    let write = |raw: &[u8]| {
        queue.write_texture(
            source.as_image_copy(),
            raw,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(64),
                rows_per_image: Some(16),
            },
            source.size(),
        )
    };
    let mut raw = vec![0; 16 * 16 * 4];
    for p in raw.chunks_exact_mut(4) {
        p[3] = 255;
    }
    raw[(8 * 16 + 8) * 4] = 255;
    write(&raw);
    let mut blur = ComputeBlur::try_new(device).expect("compute blur must compile");
    let blurred = blur.blur_submit(device, queue, &view, 16, 16, 1.5).unwrap();
    let pixels = export(device, queue, blurred)?;
    let weights: Vec<f32> = (-5..=5).map(|i| (-(i * i) as f32 / 4.5).exp()).collect();
    let sum: f32 = weights.iter().sum();
    let mut max_error = 0f32;
    for y in 0..16 {
        for x in 0..16 {
            let wx = if (3..=13).contains(&x) {
                weights[x - 3] / sum
            } else {
                0.
            };
            let wy = if (3..=13).contains(&y) {
                weights[y - 3] / sum
            } else {
                0.
            };
            let expected = ((wx * 255.).round() * wy).round() / 255.;
            let actual = pixels[(y * 32 + x) * 4];
            max_error = max_error.max((actual - expected).abs());
        }
    }
    assert!(
        max_error <= 1. / 255. + 0.001,
        "two-axis Gaussian error {max_error}"
    );
    assert!(pixels[(8 * 32 + 7) * 4] > 0. && pixels[(7 * 32 + 8) * 4] > 0.);
    write(&[255; 16 * 16 * 4]);
    let mut histogram =
        ComputeHistogram::try_new(device).expect("histogram/downsample must compile");
    for _ in 0..2 {
        assert!(histogram.submit(device, queue, &view, 16, 16));
    }
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 1024,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_buffer_to_buffer(histogram.bins_buffer(), 0, &staging, 0, 1024);
    assert!(histogram.downsample_encode(device, queue, &mut encoder, &view, 7, 7));
    queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    staging
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
    device.poll(wgpu::PollType::wait_indefinitely())?;
    rx.recv()??;
    {
        let data = staging.slice(..).get_mapped_range()?;
        for (i, b) in data.chunks_exact(4).enumerate() {
            assert_eq!(
                u32::from_le_bytes(b.try_into().unwrap()),
                if i == 255 { 256 } else { 0 }
            );
        }
    }
    staging.unmap();
    let down = export(device, queue, histogram.result_view().unwrap())?;
    for y in 0..7 {
        for x in 0..7 {
            for c in 0..4 {
                assert!((down[(y * 32 + x) * 4 + c] - 1.).abs() < 0.001);
            }
        }
    }
    let mut particles = ComputeParticles::try_new(device).expect("particle shader must compile");
    let result = particles
        .submit(
            device,
            queue,
            &view,
            Some(histogram.bins_buffer()),
            16,
            16,
            1.,
            0.,
        )
        .unwrap();
    let result = export(device, queue, result)?;
    for y in 0..16 {
        for x in 0..16 {
            for c in 0..4 {
                assert!((result[(y * 32 + x) * 4 + c] - 1.).abs() < 0.001);
            }
        }
    }
    std::fs::write(
        out.join("compute-effects.json"),
        serde_json::to_vec_pretty(
            &json!({"status":"pass","blur_max_error":max_error,"histogram_pixels":256,"histogram_clear_submissions":2,"downsample_size":[7,7],"particle_zero_strength_identity":true}),
        )?,
    )?;
    Ok(())
}

fn export(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    source: &wgpu::TextureView,
) -> Result<Vec<f32>, Box<dyn Error>> {
    let target = initialization_tests::float_target(device);
    let mut renderer =
        PathGpu::try_new_with_format(device, 1, wgpu::TextureFormat::Rgba16Float).unwrap();
    renderer.set_aa_quality(gpu::AaQuality::Aliased);
    let image = renderer.register_texture(
        source.texture().clone(),
        gpu::GpuImageColor::LinearSrgb,
        gpu::GpuImageAlpha::Straight,
    )?;
    let mut canvas = GpuCanvas::new_linear(32, 32, None);
    canvas.draw_image(
        &CanvasImageSource::Image(&image),
        0.,
        0.,
        image.width() as f32,
        image.height() as f32,
    );
    assert!(renderer.render(
        device,
        queue,
        &target.create_view(&Default::default()),
        [32, 32],
        &canvas.into_ops()
    ));
    Ok(advanced_tests::read_float(device, queue, &target, [32, 32]))
}
