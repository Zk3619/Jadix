use jadix::renderer::{
    InstanceInput, JadixPipeline,
    offscreen::{OffscreenGpu, RgbaImage},
};
use wgpu::util::DeviceExt;

fn render_glass(gpu: &OffscreenGpu, params: [f32; 4]) -> RgbaImage {
    render_glass_pattern(gpu, params, false)
}

fn render_glass_pattern(gpu: &OffscreenGpu, params: [f32; 4], ramp: bool) -> RgbaImage {
    render_glass_capture(gpu, params, ramp, 255, 2.0)
}

fn render_glass_capture(
    gpu: &OffscreenGpu,
    params: [f32; 4],
    ramp: bool,
    capture_alpha: u8,
    blur: f32,
) -> RgbaImage {
    render_glass_sources(gpu, params, ramp, capture_alpha, blur, false)
}

fn render_glass_sources(
    gpu: &OffscreenGpu,
    params: [f32; 4],
    ramp: bool,
    capture_alpha: u8,
    blur: f32,
    sharp_edges: bool,
) -> RgbaImage {
    render_glass_surface(gpu, params, ramp, capture_alpha, blur, sharp_edges, 1.0)
}

fn render_glass_surface(
    gpu: &OffscreenGpu,
    params: [f32; 4],
    ramp: bool,
    capture_alpha: u8,
    blur: f32,
    sharp_edges: bool,
    opacity: f32,
) -> RgbaImage {
    let (width, height) = (160, 120);
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("glass-test-target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let backdrop = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("glass-test-pattern"),
        size: texture.size(),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let mut pixels = Vec::new();
    for vertical in 0..height {
        for horizontal in 0..width {
            let value = if (horizontal / 7 + vertical / 9) % 2 == 0 {
                220
            } else {
                35
            };
            if ramp {
                pixels.extend_from_slice(&[
                    (horizontal * 255 / (width - 1)) as u8,
                    (vertical * 255 / (height - 1)) as u8,
                    0,
                    capture_alpha,
                ]);
            } else {
                pixels.extend_from_slice(&[value, value, value, capture_alpha]);
            }
        }
    }
    gpu.queue.write_texture(
        backdrop.as_image_copy(),
        &pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 4),
            rows_per_image: Some(height),
        },
        backdrop.size(),
    );
    let pipeline = JadixPipeline::from_format(&gpu.device, format, width, height);
    let backdrop_view = backdrop.create_view(&Default::default());
    let sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let defocused = sharp_edges.then(|| {
        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("glass-test-defocused-capture"),
            size: backdrop.size(),
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        gpu.queue.write_texture(
            texture.as_image_copy(),
            &[110, 110, 110, capture_alpha].repeat((width * height) as usize),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            texture.size(),
        );
        texture
    });
    let backdrop_group = if let Some(defocused) = &defocused {
        let defocused_view = defocused.create_view(&Default::default());
        pipeline.make_optical_backdrop_bind_group(
            &gpu.device,
            &defocused_view,
            &backdrop_view,
            &sampler,
        )
    } else {
        pipeline.make_backdrop_bind_group(&gpu.device, &backdrop_view, &sampler)
    };
    let instance = InstanceInput {
        rect_pos: [20.0, 16.0],
        rect_size: [120.0, 88.0],
        color: [1.0, 1.0, 1.0, if ramp { 0.0 } else { 0.05 * opacity }],
        border_color: [0.0; 4],
        radii: [22.0; 4],
        border_width: 0.0,
        shadow_color: [0.0; 4],
        shadow_offset: [0.0; 2],
        shadow_blur: 0.0,
        shadow_spread: 0.0,
        shadow_inset: 0.0,
        backdrop_fade: 1.0 - opacity,
        transform: jadix::style::transform::MAT2D_IDENTITY,
        clip_pos: [0.0; 2],
        clip_size: InstanceInput::NO_CLIP_SIZE,
        border_extra: 0.0,
        backdrop_blur: blur,
        glass: params,
    };
    let instances = [instance];
    let buffer = gpu
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("glass-test-instance"),
            contents: bytemuck::cast_slice(&instances),
            usage: wgpu::BufferUsages::VERTEX,
        });
    let view = texture.create_view(&Default::default());
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&pipeline.render_pipeline);
        pass.set_bind_group(0, &pipeline.bind_group, &[]);
        pass.set_bind_group(1, &backdrop_group, &[]);
        pass.set_vertex_buffer(0, pipeline.vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, buffer.slice(..));
        pass.set_index_buffer(pipeline.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        pass.draw_indexed(0..pipeline.num_indices, 0, 0..1);
    }
    let bytes_per_row = (width * 4).div_ceil(256) * 256;
    let readback = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("glass-test-readback"),
        size: u64::from(bytes_per_row * height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        texture.size(),
    );
    gpu.queue.submit([encoder.finish()]);
    let (sender, receiver) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            sender.send(result).unwrap()
        });
    gpu.device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    receiver.recv().unwrap().unwrap();
    let mapped = readback.slice(..).get_mapped_range().unwrap();
    let pixels = mapped
        .chunks_exact(bytes_per_row as usize)
        .flat_map(|row| row[..(width * 4) as usize].iter().copied())
        .collect();
    drop(mapped);
    readback.unmap();
    RgbaImage {
        width,
        height,
        pixels,
    }
}

#[test]
fn optical_surface_fades_continuously_to_zero_including_its_backdrop() {
    let gpu = OffscreenGpu::new().expect("offscreen GPU");
    for opacity in [1.0, 0.75, 0.5, 0.25, 0.08, 0.02, 0.0] {
        let image = render_glass_surface(
            &gpu,
            [1.46, 12.0, 0.025, 0.6],
            false,
            255,
            8.0,
            true,
            opacity,
        );
        for (horizontal, vertical) in [(80, 60), (24, 60), (80, 20), (80, 99)] {
            let pixel = image.pixel(horizontal, vertical).unwrap();
            assert!(
                (pixel[3] as f32 - opacity * 255.0).abs() <= 2.0,
                "material must fade with content at ({horizontal},{vertical}), opacity {opacity}: {pixel:?}"
            );
        }
    }
}

#[test]
fn reflected_scene_retains_the_same_diffusion_at_the_bevel() {
    let gpu = OffscreenGpu::new().expect("offscreen GPU");
    let image = render_glass_sources(&gpu, [1.46, 12.0, 0.025, 0.6], false, 255, 8.0, true);
    for (horizontal, vertical) in [(25, 60), (30, 60), (80, 21), (80, 27), (80, 93), (130, 60)] {
        let pixel = image.pixel(horizontal, vertical).unwrap();
        assert!(
            pixel[0].abs_diff(image.pixel(80, 60).unwrap()[0]) <= 12,
            "reflection must not reintroduce the sharp checker or an annular band: ({horizontal},{vertical}) {pixel:?}"
        );
    }
}

#[test]
fn optical_bevel_and_interior_share_diffusion_and_tint_without_a_material_band() {
    let Some(gpu) = OffscreenGpu::new() else {
        return;
    };
    let lens = render_glass_sources(&gpu, [1.46, 12.0, 0.025, 0.0], false, 255, 2.0, true);
    let center = lens.pixel(80, 60).unwrap();
    for horizontal in [24, 28, 36, 44, 80, 115, 123, 131, 135] {
        for vertical in 42..78 {
            let edge = lens.pixel(horizontal, vertical).unwrap();
            assert!(
                edge.iter()
                    .zip(center)
                    .all(|(before, after)| before.abs_diff(after) <= 1),
                "uniform diffused capture must stay uniform across the bevel: ({horizontal}, {vertical}) {edge:?} != {center:?}"
            );
        }
    }
    for vertical in 50..70 {
        assert_eq!(
            lens.pixel(80, vertical),
            lens.pixel(80, 60),
            "the center retains the defocused capture"
        );
    }
    if let Some(directory) = std::env::var_os("JADIX_GLASS_EVIDENCE") {
        std::fs::create_dir_all(&directory).unwrap();
        image::save_buffer(
            std::path::Path::new(&directory).join("uniform-glass-diffusion.png"),
            &lens.pixels,
            lens.width,
            lens.height,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
}

#[test]
fn unavailable_backdrop_does_not_flash_dark_or_change_the_surface_alpha() {
    let Some(gpu) = OffscreenGpu::new() else {
        return;
    };
    let fallback = render_glass_capture(&gpu, [0.0; 4], false, 0, 0.0);
    for params in [[0.0; 4], [1.46, 12.0, 0.025, 0.6]] {
        let missing = render_glass_capture(&gpu, params, false, 0, 2.0);
        assert!(
            missing.pixels == fallback.pixels,
            "an unavailable capture must draw the unchanged surface, not black or a reduced-alpha rim"
        );
    }
}

#[test]
fn liquid_glass_has_a_continuous_magnifying_bevel_not_just_a_one_pixel_rim() {
    let Some(gpu) = OffscreenGpu::new() else {
        return;
    };
    let plain = render_glass_pattern(&gpu, [0.0; 4], true);
    let lens = render_glass_pattern(&gpu, [1.46, 12.0, 0.0, 0.0], true);
    for horizontal in [28, 80, 131] {
        assert!(
            plain.pixel(horizontal, 60).unwrap()[0].abs_diff((horizontal * 255 / 159) as u8) <= 1,
            "backdrop must not undergo a second sRGB decode"
        );
    }
    let displacement = |horizontal, vertical, channel: usize| {
        i16::from(lens.pixel(horizontal, vertical).unwrap()[channel])
            - i16::from(plain.pixel(horizontal, vertical).unwrap()[channel])
    };
    assert!(
        displacement(28, 60, 0) >= 5,
        "bevel must magnify detail at least eight pixels inside the edge: {}",
        displacement(28, 60, 0)
    );
    assert!(
        displacement(131, 60, 0) <= -5,
        "opposite edges must bend symmetrically inward"
    );
    assert!(
        displacement(80, 24, 1) >= 5,
        "the top edge must magnify vertically"
    );
    assert!(
        displacement(80, 95, 1) <= -5,
        "the bottom edge must magnify vertically in the opposite direction"
    );
    assert_eq!(displacement(80, 24, 0), 0);
    assert_eq!(displacement(28, 60, 1), 0);
    assert!(
        displacement(28, 60, 0) > displacement(36, 60, 0),
        "magnification must taper continuously toward the flat interior"
    );
    assert_eq!(lens.pixel(80, 60), plain.pixel(80, 60));
    assert!(
        displacement(27, 28, 0) > 0 && displacement(27, 28, 1) > 0,
        "rounded corners must bend on both axes"
    );
}

#[test]
fn liquid_glass_gpu_refraction_dispersion_and_highlights_are_independent() {
    let Some(gpu) = OffscreenGpu::new() else {
        eprintln!("SKIP liquid glass GPU test: no adapter available");
        return;
    };
    eprintln!("liquid glass adapter: {:?}", gpu.adapter_info);
    gpu.device.on_uncaptured_error(std::sync::Arc::new(|error| {
        panic!("glass GPU validation: {error}")
    }));
    let plain = render_glass(&gpu, [0.0; 4]);
    let identity = render_glass(&gpu, [1.0, 12.0, 0.0, 0.0]);
    assert_eq!(plain.pixels, identity.pixels, "index 1 must not bend light");
    let zero_thickness = render_glass(&gpu, [1.46, 0.0, 0.12, 0.6]);
    assert_eq!(
        plain.pixels, zero_thickness.pixels,
        "zero thickness disables optics"
    );
    let refracted = render_glass(&gpu, [1.46, 12.0, 0.0, 0.0]);
    let dispersed = render_glass(&gpu, [1.46, 12.0, 0.12, 0.0]);
    let highlighted = render_glass(&gpu, [1.46, 12.0, 0.12, 0.6]);
    let changed = plain
        .pixels
        .chunks_exact(4)
        .zip(refracted.pixels.chunks_exact(4))
        .filter(|(before, after)| before != after)
        .count();
    assert!(
        changed > 300,
        "refraction must displace the actual backdrop: {changed}"
    );
    assert!(
        refracted
            .pixels
            .chunks_exact(4)
            .all(|pixel| pixel[0] == pixel[1] && pixel[1] == pixel[2])
    );
    let colored = dispersed
        .pixels
        .chunks_exact(4)
        .filter(|pixel| pixel[0].abs_diff(pixel[2]) > 4)
        .count();
    assert!(
        colored > 100,
        "dispersion must split RGB samples: {colored}"
    );
    for (name, horizontal, vertical) in [
        ("left", 24..34, 42..78),
        ("right", 126..136, 42..78),
        ("top", 48..112, 20..30),
        ("bottom", 48..112, 90..100),
    ] {
        let mut colored = 0;
        for horizontal in horizontal {
            for vertical in vertical.clone() {
                let pixel = dispersed.pixel(horizontal, vertical).unwrap();
                colored += usize::from(pixel[0].abs_diff(pixel[2]) > 4);
            }
        }
        assert!(
            colored > 8,
            "{name} edge must disperse the background: {colored}"
        );
    }
    assert_ne!(
        dispersed.pixels, highlighted.pixels,
        "reflection/highlights must be independent of refraction"
    );
    assert_eq!(
        refracted.pixel(80, 60),
        plain.pixel(80, 60),
        "flat center keeps backdrop detail"
    );
    assert_eq!(
        highlighted.pixel(20, 16),
        Some([0, 0, 0, 0]),
        "rounded corners stay clipped"
    );
    if let Some(directory) = std::env::var_os("JADIX_GLASS_EVIDENCE") {
        std::fs::create_dir_all(&directory).unwrap();
        for (name, image) in [
            ("plain", plain),
            ("refracted", refracted),
            ("dispersed", dispersed),
            ("highlighted", highlighted),
        ] {
            image::save_buffer(
                std::path::Path::new(&directory).join(format!("{name}.png")),
                &image.pixels,
                image.width,
                image.height,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
    }
}
