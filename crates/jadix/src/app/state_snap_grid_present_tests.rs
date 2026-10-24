#![cfg(feature = "gallery-tests")]

use super::*;

#[test]
#[ignore = "requires a native Windows GPU surface"]
fn snap_grid_toggle_native_frames_keep_color_stable() {
    use winit::application::ApplicationHandler;
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, EventLoop};
    use winit::platform::windows::EventLoopBuilderExtWindows;
    use winit::window::{WindowAttributes, WindowId};

    struct Harness;
    impl ApplicationHandler for Harness {
        fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
            let window: Arc<dyn Window> = Arc::from(
                event_loop
                    .create_window(
                        WindowAttributes::default()
                            .with_title("Jadix snap grid regression")
                            .with_surface_size(winit::dpi::LogicalSize::new(640.0, 400.0))
                            .with_visible(false),
                    )
                    .unwrap(),
            );
            run_frames(window);
            event_loop.exit();
        }
        fn window_event(&mut self, _: &dyn ActiveEventLoop, _: WindowId, _: WindowEvent) {}
    }
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    builder.build().unwrap().run_app(Harness).unwrap();
}

fn run_frames(window: Arc<dyn Window>) {
    use crate::core::reactivity::{create_root, use_signal};
    use crate::utils::drag::SnapGrid;

    fn frame(state: &mut AppState) -> crate::renderer::offscreen::RgbaImage {
        state
            .render(
                [-1.0, -1.0],
                ScrollbarUpdateMode::ViewportsOnly,
                None,
                false,
                false,
                false,
            )
            .unwrap();
        let texture = state.present_retain.color_texture_for_test().unwrap();
        let mut image = readback_texture(
            &state.device,
            &state.queue,
            texture,
            state.config.width,
            state.config.height,
            state.device.create_command_encoder(&Default::default()),
        )
        .unwrap();
        if matches!(
            texture.format(),
            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
        ) {
            for p in image.pixels.chunks_exact_mut(4) {
                p.swap(0, 2);
            }
        }
        image
    }

    let mut state = crate::utils::block_on::block_on(AppState::new_bootstrap(
        window,
        super::super::super::window_chrome::WindowOptions::default(),
    ));
    let (owner, visible) = create_root(|| {
        let visible = use_signal("native-snap-grid-visible", || false);
        let coarse = SnapGrid::new(32.0, 32.0).offset([16.0, 16.0]);
        let fine = SnapGrid::new(8.0, 8.0);
        let board = |top, grids: &[(SnapGrid, f32)]| {
            Element::new()
                .absolute()
                .left(20.0)
                .top(top)
                .w(600.0)
                .h(160.0)
                .bg([1.0; 4])
                .rounded(12.0)
                .overflow("hidden")
                .border_width(1.0)
                .border_color("border")
                .border_style("dashed")
                .child(crate::gallery_drag::snap_grid_lines(grids, visible))
                .child(
                    Element::new()
                        .absolute()
                        .left(16.0)
                        .top(16.0)
                        .w(176.0)
                        .h(92.0)
                        .rounded(12.0)
                        .bg("muted")
                        .border("destructive", 3.0)
                        .overflow("hidden")
                        .child(Element::new().text("S").text_size(44.0)),
                )
        };
        state.rebuild_ui(
            Element::new()
                .w(640.0)
                .h(400.0)
                .bg([1.0; 4])
                .child(board(20.0, &[(coarse, 0.16)]))
                .child(board(210.0, &[(fine, 0.06), (coarse, 0.16)])),
        );
        visible
    });
    for _ in 0..3 {
        frame(&mut state);
    }
    for shown in [true, false, true] {
        visible.set(shown);
        let frames: Vec<_> = (0..4).map(|_| frame(&mut state)).collect();
        state.mark_full_redraw();
        let full = frame(&mut state);
        for (tick, image) in frames.iter().enumerate() {
            let diffs: Vec<_> = image
                .pixels
                .chunks_exact(4)
                .zip(full.pixels.chunks_exact(4))
                .enumerate()
                .filter(|(i, (a, b))| {
                    // Compare grid ink and foreground card chrome, independently of the
                    // dashed border's antialiased coverage at fractional DPI.
                    let scale = state.ui_scale();
                    let x = (*i as u32 % image.width) as f32 / scale;
                    let y = (*i as u32 / image.width) as f32 / scale;
                    x > 24.0
                        && x < 616.0
                        && ((y > 24.0 && y < 176.0) || (y > 214.0 && y < 366.0))
                        && a.iter().zip(b.iter()).any(|(a, b)| a.abs_diff(*b) > 2)
                })
                .take(8)
                .map(|(i, (a, b))| (i % image.width as usize, i / image.width as usize, a, b))
                .collect();
            assert!(
                diffs.is_empty(),
                "grid shown={shown}, tick={tick}: initial/retained color differs from full composite: {diffs:?}"
            );
        }
    }
    drop(owner);
}
