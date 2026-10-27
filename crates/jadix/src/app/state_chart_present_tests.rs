#![cfg(feature = "product-tests")]
//! Check chart first-display alignment and moving overlays in the native compositor.
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
                            .with_title("Jadix chart overlay regression")
                            .with_surface_size(winit::dpi::LogicalSize::new(2240.0, 400.0))
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
    use crate::app::pointer_events::{dispatch_pointer, hit_path, PointerEvent, PointerKind};
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
        scrolling: bool,
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
            scrolling,
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
    let mut owners = Vec::new();
    let scroll_offsets = [240.0, 280.0, 320.0, 400.0, 576.0, 576.0, 576.0, 576.0];
    for kind in 0..7 {
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
                    4 => chart
                        .mark(radial_dot(rows))
                        .mark(radial_bar(rows))
                        .mark(radial_line(rows))
                        .size(240.0),
                    _ => chart
                        .mark(bar_y([
                            ("Mon", 0.2),
                            ("Tue", 0.5),
                            ("Wed", 0.8),
                            ("Thu", 0.4),
                            ("Fri", 0.9),
                        ]))
                        .x_band()
                        .x_padding(0.2)
                        .axis_labels(true)
                        .plot_h(96.0),
                };
                let mut root = Element::new().w("100%").h("100%").bg([1.0; 4]).child(
                    Element::new()
                        .w(if kind >= 5 { 2000.0 } else { 560.0 })
                        .h(280.0)
                        .absolute()
                        .left(24.0)
                        .top(24.0)
                        .child(Element::from(chart)),
                );
                if kind == 6 {
                    let chart = root.children[0].children.remove(0);
                    root.children.clear();
                    root = root.child(
                        crate::components::scroll_viewport::scroll_viewport("native-bar-scroll")
                            .w("100%")
                            .h("100%")
                            .overflow_y("scroll")
                            .scroll_behavior("jump")
                            .child(
                                Element::new()
                                    .w("100%")
                                    .h(1160.0)
                                    .flex_col()
                                    .flex_shrink(0.0)
                                    .child(Element::new().h(600.0).flex_shrink(0.0))
                                    .child(
                                        Element::new()
                                            .w(2000.0)
                                            .h(160.0)
                                            .flex_shrink(0.0)
                                            .child(chart),
                                    )
                                    .child(Element::new().h(400.0).flex_shrink(0.0)),
                            ),
                    );
                }
                live.borrow_mut().as_mut().unwrap().rebuild_ui(root);
            });
        });
        if kind == 6 {
            for _ in 0..3 {
                frame(&state, [-1.0, -1.0], false, false);
            }
        }
        let initial_frames: Vec<_> = (0..8)
            .map(|i| {
                if kind == 6 {
                    let offset = scroll_offsets[i];
                    crate::app::scroll_support::set_scroll_offset_with_behavior(
                        "native-bar-scroll",
                        0.0,
                        offset,
                        true,
                    );
                    state
                        .borrow_mut()
                        .as_mut()
                        .unwrap()
                        .update_scroll_offset_in_place("native-bar-scroll", [0.0, offset]);
                }
                frame(&state, [-1.0, -1.0], false, kind == 6 && i < 5)
            })
            .collect();
        let bounds = {
            let guard = state.borrow();
            let p = plot(&guard.as_ref().unwrap().root_element).unwrap();
            let mut bounds = p.layout_bounds;
            bounds[1] -= p.paint_scroll_acc[1];
            bounds
        };
        // The first visible raster must already use the final measured plot.
        // A hidden bootstrap frame is allowed, a provisional painted chart is not.
        let ui_scale = state.borrow().as_ref().unwrap().window.scale_factor() as f32;
        let in_plot = |index: usize, width: u32| {
            let x = (index % width as usize) as f32 / ui_scale;
            let y = (index / width as usize) as f32 / ui_scale;
            x >= bounds[0]
                && x < bounds[0] + bounds[2]
                && y >= bounds[1]
                && y < bounds[1] + bounds[3] + if kind >= 5 { 28.0 } else { 0.0 }
        };
        let visible = initial_frames
            .iter()
            .find(|img| {
                img.pixels.chunks_exact(4).enumerate().any(|(i, p)| {
                    in_plot(i, img.width)
                        && p[2] > p[0].saturating_add(10)
                        && p[2] > p[1].saturating_add(2)
                })
            })
            .unwrap_or_else(|| panic!("chart {kind} never became visible"));
        let settled = initial_frames.last().unwrap();
        if kind >= 5 {
            let mut checked_frames = 0;
            for (frame_index, image) in initial_frames.iter().enumerate() {
                let mut bounds = bounds;
                if kind == 6 {
                    bounds[1] = 600.0 - scroll_offsets[frame_index];
                    // Earlier frames have the axis clipped below the viewport.
                    if bounds[1] + bounds[3] + 28.0 > image.height as f32 / ui_scale {
                        continue;
                    }
                } else if !image
                    .pixels
                    .chunks_exact(4)
                    .enumerate()
                    .any(|(i, p)| in_plot(i, image.width) && p[2] > p[0].saturating_add(10))
                {
                    continue;
                }
                checked_frames += 1;
                for category in 0..5 {
                    let x0 = bounds[0] + bounds[2] * category as f32 / 5.0;
                    let x1 = bounds[0] + bounds[2] * (category + 1) as f32 / 5.0;
                    let ink_bounds = |label: bool| {
                        let mut min_x = f32::MAX;
                        let mut max_x = f32::MIN;
                        for (i, p) in image.pixels.chunks_exact(4).enumerate() {
                            let x = (i % image.width as usize) as f32 / ui_scale;
                            let y = (i / image.width as usize) as f32 / ui_scale;
                            let ink = if label {
                                y >= bounds[1] + bounds[3] + 8.0
                                    && y < bounds[1] + bounds[3] + 28.0
                                    && p[0] < 180
                                    && p[1] < 180
                                    && p[2] < 180
                            } else {
                                y >= bounds[1]
                                    && y < bounds[1] + bounds[3]
                                    && p[2] > p[0].saturating_add(10)
                                    && p[2] > p[1].saturating_add(2)
                            };
                            if x >= x0 && x < x1 && ink {
                                min_x = min_x.min(x);
                                max_x = max_x.max(x);
                            }
                        }
                        assert!(
                            min_x <= max_x,
                            "chart {kind} category {category} missing label={label}"
                        );
                        (min_x + max_x) * 0.5
                    };
                    let bar = ink_bounds(false);
                    let label = ink_bounds(true);
                    if (bar - label).abs() > 0.7 {
                        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                            .join("../../target/chart-present");
                        std::fs::create_dir_all(&dir).unwrap();
                        ::image::save_buffer(
                            dir.join("bar-label-misaligned.png"),
                            &image.pixels,
                            image.width,
                            image.height,
                            ::image::ColorType::Rgba8,
                        )
                        .unwrap();
                    }
                    assert!(
                        (bar - label).abs() <= 0.7,
                        "chart {kind} first visible frame {frame_index} category {category}: bar={bar} label={label}"
                    );
                }
            }
            assert!(
                checked_frames >= 4,
                "chart {kind} did not exercise visible bar labels"
            );
        }
        // Scrolling moves the plot between these frames; its alignment was checked above.
        if kind != 6 {
            let changed = visible
                .pixels
                .chunks_exact(4)
                .zip(settled.pixels.chunks_exact(4))
                .enumerate()
                .filter(|(i, (a, b))| {
                    in_plot(*i, visible.width)
                        && a.iter().zip(b.iter()).any(|(a, b)| a.abs_diff(*b) > 8)
                })
                .count();
            assert_eq!(
                changed, 0,
                "chart {kind} changed after its first visible frame"
            );
        }
        crate::app::pointer_events::note_hover_target_bounds(bounds);
        // Stay within each datum long enough to exercise retained translate,
        // then cross categories and flip the bubble near the chart edge.
        let positions = if kind == 0 || kind >= 5 {
            [
                [0.65, 0.3],
                [0.66, 0.31],
                [0.68, 0.32],
                [0.72, 0.33],
                [0.9, 0.5],
                [0.88, 0.51],
                [0.3, 0.65],
                [0.31, 0.66],
            ]
        } else {
            [
                [0.79, 0.21],
                [0.795, 0.215],
                [0.8, 0.22],
                [0.805, 0.225],
                [0.7, 0.7],
                [0.705, 0.705],
                [0.24, 0.24],
                [0.245, 0.245],
            ]
        };
        let mut hovered_frames = 0;
        for (step, [fx, fy]) in positions.into_iter().enumerate() {
            if std::env::var_os("JADIX_CHART_PRESENT_TRACE").is_some() {
                eprintln!("[DEBUG-chart] kind={kind} step={step}");
            }
            let mouse = [bounds[0] + fx * bounds[2], bounds[1] + fy * bounds[3]];
            let path = hit_path(
                &state.borrow().as_ref().unwrap().root_element,
                mouse[0],
                mouse[1],
            );
            let mut event =
                PointerEvent::new(mouse[0], mouse[1], winit::event::MouseButton::Left, 0);
            dispatch_pointer(&path, &mut event, PointerKind::Move);
            let partial = frame(&state, mouse, false, false);
            if crate::core::tree::find_by_attr(
                &state.borrow().as_ref().unwrap().root_element,
                "data-slot",
                "chart-tooltip",
            )
            .is_some()
            {
                hovered_frames += 1;
            }
            let full = frame(&state, mouse, true, false);
            let changed = partial
                .pixels
                .chunks_exact(4)
                .zip(full.pixels.chunks_exact(4))
                .filter(|(a, b)| a.iter().zip(b.iter()).any(|(a, b)| a.abs_diff(*b) > 8))
                .count();
            if changed > 0 {
                failures.push((kind, step, changed));
                if std::env::var_os("JADIX_CHART_PRESENT_TRACE").is_some() {
                    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("../../target/chart-present");
                    std::fs::create_dir_all(&dir).unwrap();
                    for (name, img) in [("partial", &partial), ("full", &full)] {
                        ::image::save_buffer(
                            dir.join(format!("{kind}-{step}-{name}.png")),
                            &img.pixels,
                            img.width,
                            img.height,
                            ::image::ColorType::Rgba8,
                        )
                        .unwrap();
                    }
                }
            }
        }
        assert!(
            hovered_frames >= 4,
            "chart {kind} never exercised tooltip movement"
        );
        owners.push(owner);
        reset_registered_anchors();
    }
    clear_app_runtime();
    state.borrow_mut().take();
    for owner in owners {
        owner.dispose();
    }
    crate::style::set_css_transition_virtual_timeline(false);
    assert!(
        failures.is_empty(),
        "chart overlay frames differ from full repaint: {failures:?}"
    );
}
