use super::*;

#[test]
fn native_wheel_restores_detents_before_browser_applies_system_speed() {
    assert_eq!(wheel_detent_data(3.0, 3), 120.0);
    assert_eq!(wheel_detent_data(-5.0, 5), -120.0);
    assert_eq!(wheel_detent_data(1.5, 3), 60.0);
    assert_eq!(wheel_detent_data(0.0, 0), 0.0);
    assert_eq!(wheel_detent_data(3.0, u32::MAX), 120.0);
}

#[test]
#[ignore = "requires native WebView2 and a Windows GPU"]
fn loading_native_document_keeps_opaque_pixels_until_navigation_completes() {
    use winit::application::ApplicationHandler;
    use winit::event_loop::{ActiveEventLoop, EventLoop};
    use winit::platform::windows::EventLoopBuilderExtWindows;
    use winit::window::WindowId;

    fn aperture_alpha(host: &NativeWebViews, gpu: &crate::renderer::offscreen::OffscreenGpu) -> u8 {
        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("WebView loading alpha"),
            size: wgpu::Extent3d {
                width: 32,
                height: 32,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::GREEN),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            host.clear_apertures(&mut pass, 32, 32);
        }
        let image = crate::renderer::offscreen::readback_texture(
            &gpu.device,
            &gpu.queue,
            &texture,
            32,
            32,
            encoder,
        )
        .unwrap();
        image.pixel(16, 16).unwrap()[3]
    }

    struct Harness;
    impl ApplicationHandler for Harness {
        fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
            let window: Arc<dyn Window> = Arc::from(
                event_loop
                    .create_window(
                        crate::app::WindowOptions::new()
                            .size(64.0, 64.0)
                            .to_window_attributes(),
                    )
                    .unwrap(),
            );
            let gpu = crate::renderer::offscreen::OffscreenGpu::with_backends(wgpu::Backends::DX12)
                .unwrap();
            let mut host = NativeWebViews::new(window.as_ref()).unwrap();
            host.configure(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb);
            let (_owner, ()) = crate::core::reactivity::create_root(|| {
                let mut root = super::super::surface("loading-alpha".into(), String::new(), Some(
                        "<html><script>const t=Date.now();while(Date.now()-t<1800){}</script><body>Ready</body></html>".into()));
                root.layout_bounds = [0.0, 0.0, 32.0, 32.0];
                host.sync(&window, &root, 1.0, [0.0, 0.0, 32.0, 32.0]);
                assert_eq!(aperture_alpha(&host, &gpu), 255);
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
                let mut saw_loading_browser = false;
                while !host.entries["loading-alpha"].ready.get() {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "WebView navigation timed out"
                    );
                    let entry = host.entries["loading-alpha"].clone();
                    if entry.browser.borrow().is_some() && !entry.ready.get() {
                        host.sync(&window, &root, 1.0, [0.0, 0.0, 32.0, 32.0]);
                        let mut visible = windows::core::BOOL(0);
                        unsafe {
                            entry
                                .browser
                                .borrow()
                                .as_ref()
                                .unwrap()
                                .controller
                                .IsVisible(&mut visible)
                                .unwrap();
                        }
                        assert!(!visible.as_bool());
                        assert_eq!(aperture_alpha(&host, &gpu), 255);
                        saw_loading_browser = true;
                    }
                    unsafe {
                        use windows_sys::Win32::UI::WindowsAndMessaging::{
                            DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE,
                        };
                        let mut message = MSG::default();
                        while PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0
                        {
                            TranslateMessage(&message);
                            DispatchMessageW(&message);
                        }
                    }
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                assert!(saw_loading_browser);
                host.sync(&window, &root, 1.0, [0.0, 0.0, 32.0, 32.0]);
                assert_eq!(aperture_alpha(&host, &gpu), 0);
                host.entries["loading-alpha"].snapshot.set(true);
                assert_eq!(aperture_alpha(&host, &gpu), 255);
                root.set_attr("jadix_webview_html", "<html><body>Second</body></html>");
                host.sync(&window, &root, 1.0, [0.0, 0.0, 32.0, 32.0]);
                assert!(!host.entries["loading-alpha"].ready.get());
                assert_eq!(aperture_alpha(&host, &gpu), 255);
            });
            drop(host);
            event_loop.exit();
        }
        fn window_event(&mut self, _: &dyn ActiveEventLoop, _: WindowId, _: WindowEvent) {}
    }
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    builder.build().unwrap().run_app(Harness).unwrap();
}
