use super::*;
use crate::renderer::offscreen::{readback_texture, OffscreenGpu};
use crate::style::*;

fn frame(
    gpu: &OffscreenGpu,
    fade: &mut ThemeTransition,
    content: &mut PresentRetainPipeline,
    color: wgpu::Color,
) -> ([u8; 4], bool) {
    let raw = content.color_texture().unwrap();
    let output = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("theme-test-output"),
        size: raw.size(),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: raw.format(),
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = output.create_view(&Default::default());
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    fade.prepare(&gpu.device, &mut encoder, content);
    {
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: content.color_view().unwrap(),
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(color),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    }
    let running = fade.present(&gpu.queue, &mut encoder, content, &view, color, |_| {});
    content.mark_contents_valid();
    let image = readback_texture(
        &gpu.device,
        &gpu.queue,
        &output,
        output.width(),
        output.height(),
        encoder,
    )
    .unwrap();
    (image.pixel(2, 2).unwrap(), running)
}

fn check_crossfade(gpu: &OffscreenGpu) {
    gpu.device.on_uncaptured_error(std::sync::Arc::new(|err| {
        panic!("theme transition GPU validation: {err}");
    }));
    let prior_theme = get_theme();
    let prior_config = theme_transition_config();
    set_theme_mode(ThemeMode::Light);
    set_theme_transition_config(ThemeTransitionConfig {
        enabled: true,
        duration_ms: 180.0,
        easing: "linear".into(),
    });
    set_css_transition_virtual_timeline(true);
    set_css_transition_timeline_ms(0.0);
    let mut content = PresentRetainPipeline::new(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb);
    content.ensure_size(&gpu.device, 8, 8);
    let mut fade = ThemeTransition::new(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb);
    assert_eq!(
        frame(gpu, &mut fade, &mut content, wgpu::Color::WHITE),
        ([255; 4], false)
    );

    set_theme_mode(ThemeMode::Dark);
    let (start, running) = frame(gpu, &mut fade, &mut content, wgpu::Color::BLACK);
    assert_eq!(start, [255; 4]);
    assert!(running);
    set_css_transition_timeline_ms(90.0);
    let (mid, running) = frame(gpu, &mut fade, &mut content, wgpu::Color::BLACK);
    // The retained target is sRGB; the shader blends in linear light.
    assert!(
        (184..=191).contains(&mid[0]),
        "linear-light midpoint {mid:?}"
    );
    assert!(running);

    // Rapid reversal starts from the actually presented mixture, not the dark target.
    set_theme_mode(ThemeMode::Light);
    let (reversed, running) = frame(gpu, &mut fade, &mut content, wgpu::Color::WHITE);
    assert!(
        (0..4).all(|i| reversed[i].abs_diff(mid[i]) <= 1),
        "{mid:?} → {reversed:?}"
    );
    assert!(running);
    set_css_transition_timeline_ms(270.0);
    assert_eq!(
        frame(gpu, &mut fade, &mut content, wgpu::Color::WHITE),
        ([255; 4], false)
    );
    assert!(!fade.needs_frame());

    // Disabled, zero-duration and reduced-motion switches settle in one frame.
    for (enabled, duration_ms, reduced) in [
        (false, 180.0, false),
        (true, 0.0, false),
        (true, 180.0, true),
    ] {
        set_theme_transition_config(ThemeTransitionConfig {
            enabled,
            duration_ms,
            easing: "linear".into(),
        });
        set_prefers_reduced_motion(Some(reduced));
        toggle_theme_mode();
        assert_eq!(
            frame(gpu, &mut fade, &mut content, wgpu::Color::BLACK),
            ([0, 0, 0, 255], false)
        );
        assert!(!fade.needs_frame());
    }
    set_prefers_reduced_motion(Some(false));
    set_theme_transition_config(ThemeTransitionConfig::default());
    toggle_theme_mode();
    assert!(frame(gpu, &mut fade, &mut content, wgpu::Color::WHITE).1);
    content.ensure_size(&gpu.device, 16, 16);
    assert_eq!(
        frame(gpu, &mut fade, &mut content, wgpu::Color::WHITE),
        ([255; 4], false)
    );
    assert!(!fade.needs_frame(), "resize cancels the old-size snapshot");

    set_css_transition_virtual_timeline(false);
    set_prefers_reduced_motion(None);
    set_theme(prior_theme);
    set_theme_transition_config(prior_config);
}

#[test]
#[cfg_attr(
    not(any(
        all(windows, feature = "backend-dx12"),
        all(target_vendor = "apple", feature = "backend-metal"),
        feature = "backend-gles",
        feature = "backend-vulkan",
    )),
    ignore = "requires a wgpu backend for this target"
)]
fn crossfade_samples_pixels_and_reverses_without_snapping() {
    let Some(gpu) = OffscreenGpu::new() else {
        return;
    };
    check_crossfade(&gpu);
    if let Some(gl) = OffscreenGpu::with_backends(wgpu::Backends::GL) {
        check_crossfade(&gl);
    }
}

#[cfg(windows)]
#[test]
#[cfg_attr(
    not(any(
        all(windows, feature = "backend-dx12"),
        all(target_vendor = "apple", feature = "backend-metal"),
        feature = "backend-gles",
        feature = "backend-vulkan",
    )),
    ignore = "requires a wgpu backend for this target"
)]
fn compositor_receives_display_premultiplied_pixels_through_theme_fades() {
    let gpu = OffscreenGpu::with_backends(wgpu::Backends::DX12).expect("DX12 adapter");
    let previous_theme = get_theme();
    let previous_preference = theme_preference();
    let previous_config = theme_transition_config();
    set_theme_transition_config(ThemeTransitionConfig {
        enabled: true,
        duration_ms: 180.0,
        easing: "linear".into(),
    });
    set_css_transition_virtual_timeline(true);
    set_prefers_reduced_motion(Some(false));
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        width: 8,
        height: 8,
        present_mode: wgpu::PresentMode::Fifo,
        alpha_mode: wgpu::CompositeAlphaMode::PreMultiplied,
        color_space: wgpu::SurfaceColorSpace::Auto,
        view_formats: vec![],
        desired_maximum_frame_latency: 1,
    };
    for alpha in [0.0, 0.18, 0.35, 1.0] {
        for tint in [1.0, 0.15] {
            set_css_transition_timeline_ms(0.0);
            let mut content = PresentRetainPipeline::new_for_surface(&gpu.device, &config);
            content.ensure_size(&gpu.device, 8, 8);
            let mut fade = ThemeTransition::new_for_surface(&gpu.device, &config);
            let linear = linearize_display_srgb_component(tint) as f64 * alpha;
            let color = wgpu::Color { r: linear, g: linear, b: linear, a: alpha };
            let expected = [(tint as f64 * alpha * 255.0).round() as u8; 3];
            let check = |pixel: [u8; 4]| {
                for channel in 0..3 {
                    assert!(pixel[channel].abs_diff(expected[channel]) <= 3,
                        "alpha={alpha} tint={tint} pixel={pixel:?} expected={expected:?}");
                }
                assert!(pixel[3].abs_diff((alpha * 255.0).round() as u8) <= 1);
            };
            check(frame(&gpu, &mut fade, &mut content, color).0);
            toggle_theme_mode();
            let (pixel, running) = frame(&gpu, &mut fade, &mut content, color);
            check(pixel);
            assert!(running);
            set_css_transition_timeline_ms(90.0);
            check(frame(&gpu, &mut fade, &mut content, color).0);
            toggle_theme_mode();
            check(frame(&gpu, &mut fade, &mut content, color).0);
            set_css_transition_timeline_ms(300.0);
            let (pixel, running) = frame(&gpu, &mut fade, &mut content, color);
            check(pixel);
            assert!(!running);
        }
    }
    set_css_transition_virtual_timeline(false);
    set_prefers_reduced_motion(None);
    set_theme_transition_config(previous_config);
    set_theme_preference(previous_preference);
    set_theme(previous_theme);
}
