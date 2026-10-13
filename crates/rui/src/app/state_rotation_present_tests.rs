//! Rotating overflow must composite identically on scrolling and retained frames.
use super::*;

#[test]
#[ignore = "requires a native Windows GPU surface; run explicitly with --ignored --nocapture"]
fn rotating_rectangle_native_frames_match_full_repaint_after_scroll() {
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
                            .with_title("RUI rotation regression")
                            .with_surface_size(winit::dpi::LogicalSize::new(320.0, 240.0))
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
    use crate::core::scroll_viewport::{
        scroll_offset_y_signal_key, scroll_target_y_signal_key, scroll_viewport,
    };

    fn read_frame(s: &AppState) -> crate::renderer::offscreen::RgbaImage {
        readback_texture(
            &s.device,
            &s.queue,
            s.present_retain.color_texture_for_test().unwrap(),
            s.config.width,
            s.config.height,
            s.device.create_command_encoder(&Default::default()),
        )
        .unwrap()
    }

    crate::style::utility::set_frame_dt(0.0);
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    let mut state = crate::utils::block_on::block_on(AppState::new_bootstrap(
        window,
        super::super::super::window_chrome::WindowOptions::default(),
    ));
    let (_owner, ()) = create_root(|| {
        state.rebuild_ui(
            Element::new().w("100%").h("100%").bg([1.0; 4]).child(
                scroll_viewport("rotation-scroll")
                    .w(320.0)
                    .h(200.0)
                    .overflow_y("scroll")
                    .scroll_behavior("jump")
                    .child(
                        Element::new()
                            .flex_col()
                            .w(320.0)
                            .h(400.0)
                            .flex_shrink(0.0)
                            .child(
                                Element::new()
                                    .w(88.0)
                                    .h(56.0)
                                    .flex_shrink(0.0)
                                    .bg([0.2, 0.5, 1.0, 1.0])
                                    .class("animate-spin")
                                    .child(Element::new().text("Rotate").w(88.0).h(20.0)),
                            )
                            .child(
                                Element::new()
                                    .w(160.0)
                                    .h(20.0)
                                    .flex_shrink(0.0)
                                    .text("Title below rotating rectangle"),
                            ),
                    ),
            ),
        );
    });
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
    for step in 0..16 {
        let scrolling = step == 3 || step == 4;
        let scroll = match step {
            0..=2 => 0.0,
            3 => 8.0,
            _ => 16.0,
        };
        use_signal(&scroll_offset_y_signal_key("rotation-scroll"), || 0.0_f32).set_silent(scroll);
        use_signal(&scroll_target_y_signal_key("rotation-scroll"), || 0.0_f32).set_silent(scroll);
        crate::style::utility::set_frame_dt(0.125);
        crate::style::advance_css_transition_timeline_ms(125.0);
        state
            .render(
                [-1.0; 2],
                ScrollbarUpdateMode::ViewportsOnly,
                None,
                false,
                false,
                scrolling,
            )
            .unwrap();
        // Keep consecutive retained frames between references so a full draw
        // cannot hide residue accumulating after the scroll has stopped.
        if step != 0 && step % 4 != 3 {
            continue;
        }
        let partial = read_frame(&state);
        let angle = state.root_element.children[0].children[0].children[0]
            .style
            .rotate_deg;
        crate::style::utility::set_frame_dt(0.0);
        state.mark_full_redraw();
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
        let full = read_frame(&state);
        let differences = partial
            .pixels
            .chunks_exact(4)
            .zip(full.pixels.chunks_exact(4))
            .filter(|(a, b)| a.iter().zip(b.iter()).any(|(a, b)| a.abs_diff(*b) > 3))
            .count();
        assert_eq!(differences, 0,
            "frame={step}, angle={angle}, scroll={scroll}, scrolling={scrolling}: clipped rotation or stale pixels differ from full repaint");
    }
    crate::style::set_css_transition_virtual_timeline(false);
    crate::style::utility::set_frame_dt(0.016);
}
