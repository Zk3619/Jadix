use super::*;

#[cfg(all(debug_assertions, feature = "devtools"))]
#[test]
#[ignore = "requires a native Windows GPU surface"]
fn modal_backdrop_survives_devtools_docking() {
    use crate::app::devtools::DevToolsState;
    use winit::application::ApplicationHandler;
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, EventLoop};
    use winit::platform::windows::EventLoopBuilderExtWindows;
    use winit::window::{WindowAttributes, WindowId};

    struct Harness;
    impl ApplicationHandler for Harness {
        fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
            let window = Arc::from(
                event_loop
                    .create_window(
                        WindowAttributes::default()
                            .with_surface_size(winit::dpi::LogicalSize::new(1200.0, 600.0))
                            .with_visible(false),
                    )
                    .unwrap(),
            );
            let mut state = crate::utils::block_on::block_on(AppState::new_bootstrap(
                window,
                crate::app::WindowOptions::new(),
            ));
            let (_owner, ()) = crate::core::reactivity::create_root(|| {
                DevToolsState::enable_for_window();
                DevToolsState::set_open(false);
                DevToolsState::set_panel_width(600.0);
                let underlay: Element = crate::primitives::overlay::overlay(1000)
                    .color([0.0, 0.0, 0.0, 0.2])
                    .backdrop_blur(12.0)
                    .into();
                let panel = crate::utils::popup::panel_block_clicks(
                    Element::new()
                        .absolute()
                        .left(240.0)
                        .top(260.0)
                        .w(100.0)
                        .h(80.0)
                        .bg("#888888"),
                    1001,
                );
                state.rebuild_ui(crate::app::devtools::wrap_app(
                    Element::new()
                        .w("100%")
                        .h("100%")
                        .bg("#000000")
                        .child(
                            Element::new()
                                .absolute()
                                .left(100.0)
                                .top(40.0)
                                .w(100.0)
                                .h(180.0)
                                .bg("#ffffff"),
                        )
                        .child(underlay)
                        .child(panel),
                ));
                for docked in [false, true, false] {
                    DevToolsState::set_open(docked);
                    DevToolsState::bump();
                    for _ in 0..3 {
                        state
                            .render(
                                [-1.0; 2],
                                ScrollbarUpdateMode::ViewportsOnly,
                                None,
                                false,
                                false,
                                false,
                            )
                            .unwrap();
                    }
                    let frame = readback_texture(
                        &state.device,
                        &state.queue,
                        state.present_retain.color_texture_for_test().unwrap(),
                        state.config.width,
                        state.config.height,
                        state.device.create_command_encoder(&Default::default()),
                    )
                    .unwrap();
                    let scale = state.last_text_ui_scale;
                    let sample = |x: f32| {
                        let column = (x * scale) as usize;
                        let row = (120.0 * scale) as usize;
                        frame.pixels[(row * frame.width as usize + column) * 4]
                    };
                    assert!(
                        sample(94.0) > 10 && sample(106.0) < sample(150.0) - 10,
                        "modal backdrop must remain blurred when docked={docked}: {:?}",
                        [sample(94.0), sample(106.0), sample(150.0)]
                    );
                    assert!(sample(150.0) < 240, "modal scrim must dim its source");
                    if docked {
                        assert!(
                            state.modal_blur_radius.is_none(),
                            "docked modal must use a local backdrop, leaving DevTools sharp"
                        );
                        assert!(state.local_backdrop_capture_count > 0);
                    }
                    state.mark_full_redraw();
                }
                DevToolsState::set_open(false);
            });
            event_loop.exit();
        }
        fn window_event(&mut self, _: &dyn ActiveEventLoop, _: WindowId, _: WindowEvent) {}
    }
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    builder.build().unwrap().run_app(Harness).unwrap();
}

#[test]
#[ignore = "requires a native Windows GPU surface"]
fn layered_partial_updates_match_full_frames() {
    use winit::application::ApplicationHandler;
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, EventLoop};
    use winit::platform::windows::EventLoopBuilderExtWindows;
    use winit::window::{WindowAttributes, WindowId};
    struct Harness;
    impl ApplicationHandler for Harness {
        fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
            let window = Arc::from(
                event_loop
                    .create_window(
                        WindowAttributes::default()
                            .with_surface_size(winit::dpi::LogicalSize::new(320.0, 240.0))
                            .with_visible(false),
                    )
                    .unwrap(),
            );
            let mut state = crate::utils::block_on::block_on(AppState::new_bootstrap(
                window,
                crate::app::WindowOptions::new(),
            ));
            state.rebuild_ui(
                Element::new()
                    .w("100%")
                    .h("100%")
                    .bg("#111111")
                    .child(
                        Element::new()
                            .absolute()
                            .left(40.0)
                            .top(40.0)
                            .w(180.0)
                            .h(24.0)
                            .bg("#eeeeee")
                            .rounded(8.0),
                    )
                    .child(
                        Element::new()
                            .absolute()
                            .left(20.0)
                            .top(20.0)
                            .w(200.0)
                            .h(170.0)
                            .bg("#225533")
                            .z_index(100)
                            .child(
                                Element::new()
                                    .absolute()
                                    .left(20.0)
                                    .top(20.0)
                                    .w(140.0)
                                    .h(40.0)
                                    .bg("#999999")
                                    .opacity(0.5),
                            ),
                    ),
            );
            let capture = |state: &mut AppState| {
                state
                    .render(
                        [-1.0; 2],
                        ScrollbarUpdateMode::ViewportsOnly,
                        None,
                        false,
                        false,
                        false,
                    )
                    .unwrap();
                readback_texture(
                    &state.device,
                    &state.queue,
                    state.present_retain.color_texture_for_test().unwrap(),
                    state.config.width,
                    state.config.height,
                    state.device.create_command_encoder(&Default::default()),
                )
                .unwrap()
            };
            let mut previous = capture(&mut state);
            for foreground in [false, true] {
                let element = if foreground {
                    &mut state.root_element.children[1].children[0]
                } else {
                    &mut state.root_element.children[0]
                };
                element.style.opacity = 0.75;
                crate::app::damage::push_frame_paint_element(element, [0.0; 2]);
                let partial = capture(&mut state);
                assert!(
                    state.last_frame_partial,
                    "must exercise retained partial presentation"
                );
                if foreground {
                    assert!(
                        !images_match(&partial, &previous, 2),
                        "hover update must change pixels"
                    );
                }
                state.mark_full_redraw();
                let full = capture(&mut state);
                assert!(
                    images_match(&partial, &full, 2),
                    "layered partial update differs from full frame: foreground={foreground}"
                );
                previous = full;
            }
            event_loop.exit();
        }
        fn window_event(&mut self, _: &dyn ActiveEventLoop, _: WindowId, _: WindowEvent) {}
    }
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    builder.build().unwrap().run_app(Harness).unwrap();
}

#[test]
#[ignore = "requires GPU readback"]
fn backdrop_blur_is_isotropic() {
    let gpu = OffscreenGpu::with_backends(wgpu::Backends::DX12).unwrap();
    let size = 96;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut blur = BackdropBlurPipeline::new(&gpu.device, format);
    blur.ensure_size(&gpu.device, size, size, format);
    let pipeline = JadixPipeline::from_format(&gpu.device, format, size, size);
    for (compute, radius) in [(false, 3.0), (true, 3.0), (true, 8.0), (true, 20.0)] {
        if compute && !cfg!(feature = "compute") {
            continue;
        }
        let mut pipes = jadix_renderer::compute::ComputePipelines {
            blur: if compute {
                Some(jadix_renderer::compute::ComputeBlur::try_new(&gpu.device).unwrap())
            } else {
                None
            },
            histogram: None,
            particles: None,
        };
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        let output = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("blur symmetry result"),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("blur symmetry capture"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: blur.scene_view().unwrap(),
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_scissor_rect(44, 44, 8, 8);
            draw_scissor_fill(
                &mut pass,
                &pipeline,
                &gpu.device,
                [44.0, 44.0, 8.0, 8.0],
                wgpu::Color::WHITE,
            );
        }
        blur.blur(
            &gpu.device,
            &gpu.queue,
            &mut encoder,
            radius,
            if compute { Some(&mut pipes) } else { None },
        );
        blur.blit_to(
            &gpu.queue,
            &mut encoder,
            &output.create_view(&Default::default()),
            1.0,
            wgpu::Color::BLACK,
        );
        let image =
            readback_texture(&gpu.device, &gpu.queue, &output, size, size, encoder).unwrap();
        let mut horizontal = 0u64;
        let mut vertical = 0u64;
        for row in 0..size {
            for column in 0..size {
                let weight = image.pixel(column, row).unwrap()[0] as u64;
                horizontal += weight * (2 * column as i64 - 95).pow(2) as u64;
                vertical += weight * (2 * row as i64 - 95).pow(2) as u64;
            }
        }
        assert!(horizontal > 0 && vertical > 0);
        assert!(horizontal.abs_diff(vertical) < horizontal.max(vertical) / 20,
            "blur must spread equally: compute={compute} radius={radius} horizontal={horizontal} vertical={vertical}");
    }
}
