pub use jadix_renderer::wgpu_compat::*;

pub(crate) fn configure_surface(
    surface: &wgpu::Surface<'_>,
    device: &wgpu::Device,
    config: &wgpu::SurfaceConfiguration,
) {
    #[cfg(target_arch = "wasm32")]
    surface.configure(device, &canvas_configuration(config));
    #[cfg(not(target_arch = "wasm32"))]
    surface.configure(device, config);
}

#[cfg(any(target_arch = "wasm32", test))]
fn canvas_configuration(config: &wgpu::SurfaceConfiguration) -> wgpu::SurfaceConfiguration {
    let mut canvas = config.clone();
    canvas.format = config.format.remove_srgb_suffix();
    if canvas.format != config.format && !canvas.view_formats.contains(&config.format) {
        canvas.view_formats.push(config.format);
    }
    canvas
}

#[cfg(test)]
#[test]
fn canvas_preserves_srgb_render_view_with_unorm_swapchain() {
    let render = wgpu::SurfaceConfiguration {
        format: wgpu::TextureFormat::Bgra8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        width: 64,
        height: 64,
        present_mode: wgpu::PresentMode::Fifo,
        desired_maximum_frame_latency: 2,
        alpha_mode: wgpu::CompositeAlphaMode::Opaque,
        color_space: wgpu::SurfaceColorSpace::Auto,
        view_formats: vec![],
    };
    let canvas = canvas_configuration(&render);
    assert_eq!(canvas.format, wgpu::TextureFormat::Bgra8Unorm);
    assert_eq!(canvas.view_formats, [wgpu::TextureFormat::Bgra8UnormSrgb]);
    assert_eq!(render.format, wgpu::TextureFormat::Bgra8UnormSrgb);
    assert!(render.view_formats.is_empty());
}
