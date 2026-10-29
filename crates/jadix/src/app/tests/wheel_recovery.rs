#[cfg(feature = "product-tests")]
use super::*;
#[cfg(feature = "product-tests")]
use crate::app::runtime::{clear_app_runtime, set_app_runtime, AppRuntime};
#[cfg(feature = "product-tests")]
use crate::core::reactivity::{create_root, use_signal};
#[cfg(feature = "product-tests")]
use crate::core::tree::Element;
#[cfg(feature = "product-tests")]
use winit::application::ApplicationHandler;
#[cfg(feature = "product-tests")]
use winit::event::WindowEvent;
#[cfg(feature = "product-tests")]
use winit::event_loop::{ActiveEventLoop, EventLoop};
#[cfg(feature = "product-tests")]
use winit::platform::windows::EventLoopBuilderExtWindows;

#[test]
#[ignore = "native wheel delivery and redraw regression; run in a separate process"]
#[cfg(feature = "product-tests")]
fn queued_wheel_events_do_not_replay_screens_after_stall() {
    struct Harness;
    impl ApplicationHandler for Harness {
        #[cfg(feature = "product-tests")]
        fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
            crate::init();
            let window: std::sync::Arc<dyn winit::window::Window> = std::sync::Arc::from(
                event_loop
                    .create_window(
                        winit::window::WindowAttributes::default()
                            .with_visible(false)
                            .with_surface_size(winit::dpi::PhysicalSize::new(960, 600)),
                    )
                    .unwrap(),
            );
            let id = window.id();
            let scale = window.scale_factor() as f32;
            let (_, rx) = std::sync::mpsc::channel();
            let mut app = crate::app::lifecycle::create_app(
                Default::default(),
                Element::new,
                event_loop.create_proxy(),
                rx,
            );
            *app.state.borrow_mut() = Some(crate::utils::block_on::block_on(
                crate::app::state::AppState::new_bootstrap(window, Default::default()),
            ));
            app.state
                .borrow()
                .as_ref()
                .unwrap()
                .device
                .on_uncaptured_error(std::sync::Arc::new(|err| {
                    panic!("wheel regression GPU error: {err}")
                }));
            set_app_runtime(AppRuntime::new(
                std::rc::Rc::downgrade(&app.state),
                std::rc::Rc::downgrade(&app.secondaries),
            ));
            let (_owner, root) = create_root(|| {
                let mut row = Element::new().w("100%").h("100%").flex_row();
                for (rest, behavior) in [("wheel-smooth", "smooth"), ("wheel-jump", "jump")] {
                    row = row.child(
                        crate::components::scroll_area(rest)
                            .w("50%")
                            .h("100%")
                            .scroll_behavior(behavior)
                            .child(
                                Element::new()
                                    .w("100%")
                                    .h(5000.0)
                                    .min_h(5000.0)
                                    .bg([0.1, 0.2, 0.3, 1.0]),
                            ),
                    );
                }
                row
            });
            app.state.borrow_mut().as_mut().unwrap().rebuild_ui(root);
            for _ in 0..3 {
                app.handle_redraw_requested(id, event_loop);
            }
            for rest in ["wheel-smooth", "wheel-jump"] {
                let keys = crate::core::scroll_viewport::ScrollViewportSignalKeys::new(rest);
                let offset = use_signal(&keys.offset_y, || 0.0f32);
                let target = use_signal(&keys.target_y, || 0.0f32);
                offset.set_silent(1000.0);
                target.set_silent(1000.0);
                app.state
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .update_scroll_offset_in_place(rest, [0.0, 1000.0]);
                let (bounds, max, _) = app
                    .scroll_metrics_cache
                    .get(rest)
                    .copied()
                    .expect("live viewport metrics");
                assert!(max[1] > 1500.0);
                app.cursor_pos = winit::dpi::PhysicalPosition::new(
                    ((bounds[0] + bounds[2] * 0.5) * scale) as f64,
                    ((bounds[1] + bounds[3] * 0.5) * scale) as f64,
                );
                if rest == "wheel-smooth" {
                    app.last_frame_time =
                        std::time::Instant::now() - std::time::Duration::from_secs(1);
                    app.handle_mouse_wheel(id, MouseScrollDelta::LineDelta(0.0, -1.0));
                    app.handle_redraw_requested(id, event_loop);
                    assert!(
                        offset.peek() - 1000.0 <= 3.0,
                        "idle time leaked into the first wheel frame: {}px",
                        offset.peek() - 1000.0
                    );
                    offset.set_silent(1000.0);
                    target.set_silent(1000.0);
                    app.smooth_scroll_motion.remove(rest);
                    app.state
                        .borrow_mut()
                        .as_mut()
                        .unwrap()
                        .update_scroll_offset_in_place(rest, [0.0, 1000.0]);
                }
                app.scroll_velocity.insert(rest.to_string(), [0.0, 5000.0]);
                // A blocked UI thread receives a batch before any redraw.
                for _ in 0..15 {
                    app.handle_mouse_wheel(id, MouseScrollDelta::LineDelta(0.0, -3.0));
                }
                assert!(
                    !app.scroll_velocity.contains_key(rest),
                    "wheel must cancel old fling"
                );
                if rest == "wheel-smooth" {
                    assert!(
                        (target.peek() - 1225.0).abs() < 0.01,
                        "queued target {}",
                        target.peek()
                    );
                } else {
                    assert!(
                        (app.scroll_accum[rest][1] + 225.0).abs() < 0.01,
                        "jump debt {:?}",
                        app.scroll_accum[rest]
                    );
                }
                for _ in 0..4 {
                    app.last_frame_time =
                        std::time::Instant::now() - std::time::Duration::from_millis(16);
                    if let Some((_, last_tick)) = app.smooth_scroll_motion.get_mut(rest) {
                        *last_tick = app.last_frame_time;
                    }
                    app.handle_redraw_requested(id, event_loop);
                    let guard = app.state.borrow();
                    assert!(
                        guard
                            .as_ref()
                            .unwrap()
                            .last_render_scroll_metrics
                            .iter()
                            .any(|(id, _, _, _)| id == rest),
                        "native redraw consumed clips needed by the next scroll frame"
                    );
                    let mut found = false;
                    crate::core::tree::walk_elements(&guard.as_ref().unwrap().root_element, |el| {
                        if el.focus_id.as_deref() == Some(format!("scroll:{rest}").as_str()) {
                            assert!(
                                (el.scroll_offset[1] - offset.peek()).abs() < 0.01,
                                "painted tree and offset signal diverged"
                            );
                            found = true;
                        }
                    });
                    assert!(found);
                }
                let moved = offset.peek() - 1000.0;
                assert!(
                    moved > 0.0
                        && moved
                            <= if rest == "wheel-smooth" {
                                105.0
                            } else {
                                225.01
                            },
                    "{rest} recovery jumped {moved}px"
                );
                app.handle_mouse_wheel(id, MouseScrollDelta::LineDelta(0.0, 3.0));
                let next = if rest == "wheel-smooth" {
                    target.peek()
                } else {
                    offset.peek() - app.scroll_accum[rest][1]
                };
                assert!(next < offset.peek(), "reverse must take effect immediately");
                app.scroll_accum.remove(rest);
                // Page navigation is intentional, not accumulated wheel debt.
                app.commit_wheel_delta(rest.to_string(), 0.0, -600.0, max[0], max[1], false);
                let keyboard_next = if rest == "wheel-smooth" {
                    target.peek()
                } else {
                    target.peek() - app.scroll_accum[rest][1]
                };
                assert!(
                    keyboard_next - offset.peek() > 500.0,
                    "page navigation was wheel-capped"
                );
                target.set_silent(offset.peek());
                app.scroll_accum.remove(rest);
                // Programmatic navigation may have a target distinct from the
                // painted offset before this wheel batch is delivered.
                target.set_silent(offset.peek() + 500.0);
                app.commit_wheel_delta(rest.to_string(), 0.0, -75.0, max[0], max[1], true);
                let next = if rest == "wheel-smooth" {
                    target.peek()
                } else {
                    target.peek() - app.scroll_accum[rest][1]
                };
                assert!(
                    next - offset.peek() <= 225.01,
                    "stale navigation target escaped wheel cap"
                );
                target.set_silent(offset.peek());
                app.scroll_accum.remove(rest);
            }
            clear_app_runtime();
            crate::app::reactive::reset_registered_anchors();
            event_loop.exit();
        }
        fn window_event(&mut self, _: &dyn ActiveEventLoop, _: WindowId, _: WindowEvent) {}
    }
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    crate::app::with_ui_build_stack(|| builder.build().unwrap().run_app(Harness).unwrap());
}
