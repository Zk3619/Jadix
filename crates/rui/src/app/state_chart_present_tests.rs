//! Compare moving chart overlays against the production full compositor.
use super::*;

#[test]
#[ignore = "requires a native Windows GPU surface; run explicitly with --ignored --nocapture"]
fn chart_tooltip_native_movement_matches_full_repaint() {
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
                            .with_title("RUI chart overlay regression")
                            .with_surface_size(winit::dpi::LogicalSize::new(640.0, 400.0))
                            .with_visible(false),
                    )
                    .unwrap(),
            );
            run_chart_frames(window);
            event_loop.exit();
        }
        fn window_event(&mut self, _: &dyn ActiveEventLoop, _: WindowId, _: WindowEvent) {}
    }
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    builder.build().unwrap().run_app(Harness).unwrap();
}

fn run_chart_frames(window: Arc<dyn Window>) {
    use crate::app::pointer_events::{PointerEvent, PointerHandler};
    use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
    use crate::app::runtime::{clear_app_runtime, set_app_runtime, AppRuntime};
    use crate::components::chart::*;
    use crate::core::reactivity::{create_effect, create_root};
    use std::{cell::RefCell, rc::Rc};

    fn plot(el: &Element) -> Option<&Element> {
        if el.on_mousemove.is_some() {
            return Some(el);
        }
        el.children.iter().find_map(plot)
    }
    fn frame(
        state: &Rc<RefCell<Option<AppState>>>,
        mouse: [f32; 2],
        full: bool,
    ) -> crate::renderer::offscreen::RgbaImage {
        drain_deferred_view_remounts();
        let mut guard = state.borrow_mut();
        let s = guard.as_mut().unwrap();
        if full {
            s.mark_full_redraw();
        }
        s.render(
            mouse,
            ScrollbarUpdateMode::ViewportsOnly,
            None,
            false,
            false,
            false,
        )
        .unwrap();
        let texture = s.present_retain.color_texture_for_test().unwrap();
        readback_texture(
            &s.device,
            &s.queue,
            texture,
            s.config.width,
            s.config.height,
            s.device.create_command_encoder(&Default::default()),
        )
        .unwrap()
    }

    reset_registered_anchors();
    crate::style::utility::set_frame_dt(0.0);
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    let state = Rc::new(RefCell::new(Some(crate::utils::block_on::block_on(
        AppState::new_bootstrap(
            window,
            super::super::super::window_chrome::WindowOptions::default(),
        ),
    ))));
    let secondaries = Rc::new(RefCell::new(std::collections::HashMap::new()));
    set_app_runtime(AppRuntime::new(
        Rc::downgrade(&state),
        Rc::downgrade(&secondaries),
    ));
    let mut failures = Vec::new();
    for kind in 0..5 {
        let (owner, ()) = create_root(|| {
            let live = state.clone();
            crate::core::reactivity::mark_next_effect_as_ui_root();
            create_effect(move || {
                let rows = [("N", 8.0), ("E", 5.0), ("S", 6.0), ("W", 7.0)];
                let mut chart = define_chart()
                    .id(format!("native-chart-{kind}"))
                    .tooltip(true)
                    .tooltip_indicator("line");
                chart = match kind {
                    0 => chart
                        .mark(area_y(rows))
                        .mark(line_y(rows))
                        .mark(dot(rows))
                        .crosshair(true)
                        .plot_h(200.0),
                    1 => chart.mark(pie(rows)).size(240.0),
                    2 => chart.mark(pie(rows).inner_radius(0.55)).size(240.0),
                    3 => chart
                        .mark(radial_area(rows))
                        .mark(radial_line(rows))
                        .mark(radial_dot(rows))
                        .size(240.0),
                    _ => chart
                        .mark(radial_dot(rows))
                        .mark(radial_bar(rows))
                        .mark(radial_line(rows))
                        .size(240.0),
                };
                let root = Element::new().w("100%").h("100%").bg([1.0; 4]).child(
                    Element::new()
                        .w(560.0)
                        .h(280.0)
                        .absolute()
                        .left(24.0)
                        .top(24.0)
                        .child(Element::from(chart)),
                );
                live.borrow_mut().as_mut().unwrap().rebuild_ui(root);
            });
        });
        for _ in 0..4 {
            frame(&state, [-1.0, -1.0], false);
        }
        let (handler, bounds): (PointerHandler, [f32; 4]) = {
            let guard = state.borrow();
            let p = plot(&guard.as_ref().unwrap().root_element).unwrap();
            (p.on_mousemove.clone().unwrap(), p.layout_bounds)
        };
        // Stay within each datum long enough to exercise retained translate,
        // then cross categories and flip the bubble near the chart edge.
        for (step, [fx, fy]) in [
            [0.65, 0.3],
            [0.66, 0.31],
            [0.68, 0.32],
            [0.72, 0.33],
            [0.9, 0.5],
            [0.88, 0.51],
            [0.3, 0.65],
            [0.31, 0.66],
        ]
        .into_iter()
        .enumerate()
        {
            let mouse = [bounds[0] + fx * bounds[2], bounds[1] + fy * bounds[3]];
            let mut event =
                PointerEvent::new(mouse[0], mouse[1], winit::event::MouseButton::Left, 0);
            event.current_bounds = bounds;
            handler.borrow_mut()(&event);
            let partial = frame(&state, mouse, false);
            let full = frame(&state, mouse, true);
            let changed = partial
                .pixels
                .chunks_exact(4)
                .zip(full.pixels.chunks_exact(4))
                .filter(|(a, b)| a.iter().zip(b.iter()).any(|(a, b)| a.abs_diff(*b) > 8))
                .count();
            if changed > 0 {
                failures.push((kind, step, changed));
            }
        }
        owner.dispose();
        reset_registered_anchors();
    }
    clear_app_runtime();
    crate::style::set_css_transition_virtual_timeline(false);
    assert!(
        failures.is_empty(),
        "chart overlay frames differ from full repaint: {failures:?}"
    );
}
