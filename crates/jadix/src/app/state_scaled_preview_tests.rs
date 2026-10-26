#![cfg(feature = "scale-preview")]

use super::*;

#[test]
fn scaled_scroll_snapshot_and_collect_share_the_physical_grid() {
    use crate::core::reactivity::{create_root, use_signal};
    use crate::core::scroll_viewport::scroll_offset_y_signal_key;
    create_root(|| {
        for zoom in [0.35, 0.57, 1.0, 1.5] {
            let mut viewport =
                Element::new().focus_id(format!("{SCROLL_VIEWPORT_FOCUS_PREFIX}scaled-grid"));
            viewport.paint_ancestor_scale = [zoom; 2];
            let mut layout = crate::layout::layout_tree::LayoutTree::new();
            layout.scroll_paint_scale = Some(1.5);
            for raw in [8.0, 25.0, 50.0, 0.0] {
                viewport.scroll_offset = [0.0, raw];
                let signal = use_signal(&scroll_offset_y_signal_key("scaled-grid"), || 0.0_f32);
                signal.set_silent(raw);
                let snapshot = AppState::scroll_offsets_map(&mut viewport, 1.5);
                let world = f32::from_bits(snapshot["scaled-grid"][1]);
                let collected = layout.scroll_paint_axis_offset(&viewport, true, false);
                assert!((world - collected).abs() < 0.0001);
                assert!((world * 1.5 - (world * 1.5).round()).abs() < 0.0001);
                assert_eq!(signal.peek(), raw);
            }
        }
    });
}

#[test]
#[ignore = "requires a native Windows GPU surface; run explicitly with --ignored --nocapture"]
#[cfg(feature = "components")]
fn scaled_nested_scrollbars_match_full_repaint_on_every_frame() {
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
                            .with_title("Jadix scaled scrollbar regression")
                            .with_surface_size(winit::dpi::LogicalSize::new(1000.0, 600.0))
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

#[cfg(all(debug_assertions, feature = "devtools"))]
#[test]
#[ignore = "requires a native Windows GPU surface; run explicitly with --ignored --nocapture"]
fn devtools_box_model_edges_match_full_repaint_during_scrollbar_fade() {
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
                            .with_title("Jadix DevTools border regression")
                            .with_surface_size(winit::dpi::LogicalSize::new(1400.0, 900.0))
                            .with_visible(false),
                    )
                    .unwrap(),
            );
            run_devtools_fade_frames(window);
            event_loop.exit();
        }

        fn window_event(&mut self, _: &dyn ActiveEventLoop, _: WindowId, _: WindowEvent) {}
    }
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    builder.build().unwrap().run_app(Harness).unwrap();
}

#[cfg(all(debug_assertions, feature = "devtools"))]
fn run_devtools_fade_frames(window: Arc<dyn Window>) {
    use crate::app::devtools::{DevToolsState, DevToolsTab};
    use crate::core::reactivity::{create_root, use_signal};
    use crate::core::scroll_viewport::{scroll_offset_y_signal_key, scroll_target_y_signal_key};

    let mut state = crate::utils::block_on::block_on(AppState::new_bootstrap(
        window,
        super::super::super::window_chrome::WindowOptions::default(),
    ));
    let (_owner, ()) = create_root(|| {
        crate::style::set_theme_preset(crate::style::ThemePreset::Shadcn);
        crate::style::set_theme_mode(crate::style::ThemeMode::Dark);
        crate::style::set_css_transition_virtual_timeline(true);
        DevToolsState::enable_for_window();
        DevToolsState::set_open(true);
        DevToolsState::set_tab(DevToolsTab::Elements);
        DevToolsState::set_panel_width(900.0);
        DevToolsState::set_computed(false);
        DevToolsState::set_selected(vec![0, 0, 0], "fade-target", vec![], (0.0, 0.0));
        DevToolsState::bump();
        state.rebuild_ui(crate::app::devtools::wrap_app(
            Element::new().w("100%").h("100%").bg("background").child(
                Element::new()
                    .template_id("fade-target")
                    .text("Selected text")
                    .text_size(14.0),
            ),
        ));
        for _ in 0..4 {
            state
                .render(
                    [-1.0; 2],
                    ScrollbarUpdateMode::Full,
                    None,
                    false,
                    false,
                    false,
                )
                .unwrap();
        }
        let fade = crate::style::use_ui_transition_cubic(
            "scrollbar_spring_devtools-card-properties:y",
            0.0,
        );
        fade.set(1.0);
        for _ in 0..4 {
            crate::style::tick_animations(0.08);
            state
                .render(
                    [-1.0; 2],
                    ScrollbarUpdateMode::Full,
                    None,
                    false,
                    false,
                    false,
                )
                .unwrap();
        }
        let mut diagram = None;
        crate::core::tree::walk_elements(&state.root_element, |node| {
            if node.template_id.as_deref() == Some("dt-bm-ring-position") {
                diagram = Some(crate::core::tree::visual_paint_aabb(
                    node,
                    node.paint_scroll_acc,
                ));
            }
        });
        let diagram = diagram.expect("selected node must have a box-model diagram");
        assert!(diagram[2] > 100.0 && diagram[3] > 100.0);
        for step in 0..14 {
            if step == 0 {
                fade.set(0.0);
            }
            crate::style::tick_animations(0.016);
            if step == 0 {
                use_signal(
                    &scroll_offset_y_signal_key("devtools-card-properties"),
                    || 0.0_f32,
                )
                .set_silent(24.0);
                use_signal(
                    &scroll_target_y_signal_key("devtools-card-properties"),
                    || 0.0_f32,
                )
                .set_silent(24.0);
            }
            state
                .render(
                    [-1.0; 2],
                    ScrollbarUpdateMode::ViewportsOnly,
                    None,
                    false,
                    false,
                    step == 0,
                )
                .unwrap();
            let read = |state: &AppState| {
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
            let partial = read(&state);
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
            let full = read(&state);
            let differences = partial
                .pixels
                .chunks_exact(4)
                .zip(full.pixels.chunks_exact(4))
                .filter(|(left, right)| {
                    left.iter()
                        .zip(right.iter())
                        .any(|(left, right)| left.abs_diff(*right) > 3)
                })
                .count();
            if differences > 0 {
                let directory = std::path::Path::new(".tmp/devtools-box-border-evidence");
                std::fs::create_dir_all(directory).unwrap();
                for (kind, image) in [("partial", &partial), ("full", &full)] {
                    ::image::save_buffer(
                        directory.join(format!("{step}-{kind}.png")),
                        &image.pixels,
                        image.width,
                        image.height,
                        ::image::ColorType::Rgba8,
                    )
                    .unwrap();
                }
            }
            assert_eq!(
                differences, 0,
                "box-model scrollbar fade step={step}, diagram={diagram:?}"
            );
        }
        DevToolsState::set_open(false);
        assert!(!fade.is_active() && fade.peek() < 0.0001);
        crate::style::set_css_transition_virtual_timeline(false);
    });
}

#[cfg(feature = "components")]
fn run_frames(window: Arc<dyn Window>) {
    use crate::core::reactivity::{create_root, use_signal};
    use crate::core::scroll_viewport::{
        scroll_content_h_signal_key, scroll_content_w_signal_key, scroll_offset_y_signal_key,
        scroll_target_y_signal_key, scroll_viewport_h_signal_key, scroll_viewport_w_signal_key,
    };

    let mut state = crate::utils::block_on::block_on(AppState::new_bootstrap(
        window,
        super::super::super::window_chrome::WindowOptions::default(),
    ));
    let (_owner, ()) = create_root(|| {
        let inner: Element = crate::components::scroll_area("scaled-inner")
            .w(240.0)
            .h(180.0)
            .overflow_x("hidden")
            .scroll_behavior("jump")
            .scrollbar_visibility("always")
            .scrollbar_thumb_color("#000000")
            .child(
                Element::new()
                    .w("100%")
                    .h(900.0)
                    .flex_shrink(0.0)
                    .bg([0.9, 0.95, 1.0, 1.0]),
            )
            .into();
        let content = Element::new()
            .w("100%")
            .flex_col()
            .gap(12.0)
            .child(inner.flex_shrink(0.0))
            .children((0..20).map(|index| {
                Element::new()
                    .w("100%")
                    .h(32.0)
                    .flex_shrink(0.0)
                    .bg([0.93, 0.94, 0.96, 1.0])
                    .text(format!("Row {index}"))
                    .text_color([0, 0, 0, 255])
            }));
        let outer: Element = crate::components::scroll_area("scaled-outer")
            .w(320.0)
            .h(500.0)
            .overflow_x("hidden")
            .scroll_behavior("jump")
            .scrollbar_visibility("always")
            .scrollbar_thumb_color("#000000")
            .child(content)
            .into();
        let inset = crate::components::sidebar::SidebarInset(
            crate::components::sidebar::SidebarInsetProps {
                variant: Some("inset".into()),
                children: Some(outer),
                base: None,
            },
        )
        .transition("none");
        let mut preview = Element::new().absolute().left(30.0).top(20.0).child(inset);
        crate::core::scaled_viewport::configure_viewport(
            &mut preview,
            [320.0, 500.0],
            0.57,
            "scaled-scroll-portals",
        );
        state.rebuild_ui(
            Element::new()
                .w("100%")
                .h("100%")
                .bg([1.0; 4])
                .child(preview),
        );
    });
    for _ in 0..3 {
        state
            .render(
                [-1.0; 2],
                ScrollbarUpdateMode::Full,
                None,
                false,
                false,
                false,
            )
            .unwrap();
        for (rest, bounds, _, content) in &state.last_render_scroll_metrics {
            for (key, value) in [
                (scroll_viewport_w_signal_key(rest), bounds[2]),
                (scroll_viewport_h_signal_key(rest), bounds[3]),
                (scroll_content_w_signal_key(rest), content[0]),
                (scroll_content_h_signal_key(rest), content[1]),
            ] {
                use_signal(&key, || 0.0_f32).set_silent(value);
            }
        }
    }
    for step in 0..16 {
        for (rest, offset) in [
            ("scaled-outer", (step % 5) as f32 * 8.0),
            ("scaled-inner", (step % 7) as f32 * 25.0),
        ] {
            use_signal(&scroll_offset_y_signal_key(rest), || 0.0_f32).set_silent(offset);
            use_signal(&scroll_target_y_signal_key(rest), || 0.0_f32).set_silent(offset);
        }
        state
            .render(
                if step % 2 == 0 {
                    [100.0, 100.0]
                } else {
                    [-1.0; 2]
                },
                if step % 4 < 2 {
                    ScrollbarUpdateMode::ViewportsOnly
                } else {
                    ScrollbarUpdateMode::Full
                },
                if step % 4 < 2 {
                    None
                } else {
                    Some("scaled-outer")
                },
                false,
                step % 4 >= 2,
                true,
            )
            .unwrap();
        let read = |state: &AppState| {
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
        let partial = read(&state);
        let mut partial_bars = Vec::new();
        crate::core::tree::walk_elements(&state.root_element, |node| {
            if node
                .focus_id
                .as_deref()
                .is_some_and(|id| id.starts_with("scrollbar-"))
            {
                partial_bars.push((
                    node.focus_id.clone(),
                    node.layout_bounds,
                    crate::core::tree::visual_paint_aabb(node, node.paint_scroll_acc),
                ));
            }
        });
        state.mark_full_redraw();
        state
            .render(
                [100.0, 100.0],
                ScrollbarUpdateMode::ViewportsOnly,
                None,
                false,
                false,
                false,
            )
            .unwrap();
        state
            .render(
                [-1.0; 2],
                ScrollbarUpdateMode::Full,
                None,
                false,
                false,
                false,
            )
            .unwrap();
        let full = read(&state);
        let differences = partial
            .pixels
            .chunks_exact(4)
            .zip(full.pixels.chunks_exact(4))
            .filter(|(left, right)| {
                left.iter()
                    .zip(right.iter())
                    .any(|(left, right)| left.abs_diff(*right) > 3)
            })
            .count();
        if differences > 0 {
            eprintln!(
                "partial bars={partial_bars:?}, metrics={:?}",
                state.last_render_scroll_metrics
            );
            crate::core::tree::walk_elements(&state.root_element, |node| {
                if node
                    .focus_id
                    .as_deref()
                    .is_some_and(|id| id.starts_with("scrollbar-"))
                {
                    eprintln!(
                        "full bar={:?}, layout={:?}, visual={:?}",
                        node.focus_id,
                        node.layout_bounds,
                        crate::core::tree::visual_paint_aabb(node, node.paint_scroll_acc)
                    );
                }
            });
            let directory = std::path::Path::new(".tmp/scaled-nested-scroll-evidence");
            std::fs::create_dir_all(directory).unwrap();
            for (kind, image) in [("partial", &partial), ("full", &full)] {
                ::image::save_buffer(
                    directory.join(format!("{step}-{kind}.png")),
                    &image.pixels,
                    image.width,
                    image.height,
                    ::image::ColorType::Rgba8,
                )
                .unwrap();
            }
        }
        assert_eq!(
            differences, 0,
            "scaled nested scroll frame={step} differs from full repaint"
        );
    }
}
