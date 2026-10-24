#![cfg(feature = "gallery-tests")]

//! Rotating overflow must composite identically on scrolling and retained frames.
use super::*;

#[test]
#[ignore = "requires a native Windows GPU surface; run explicitly with --ignored --nocapture"]
fn rotating_rectangle_native_frames_match_full_repaint_after_scroll() {
    native_animation_frames(false, false);
}

#[test]
#[cfg(feature = "bundled-lucide")]
#[ignore = "requires a native Windows GPU surface; run explicitly with --ignored --nocapture"]
fn shimmer_spin_rows_native_frames_match_full_repaint_after_scroll() {
    native_animation_frames(true, false);
}

#[test]
#[cfg(feature = "bundled-lucide")]
#[ignore = "requires a native Windows GPU surface; run explicitly with --ignored --nocapture"]
fn shimmer_retained_ink_matches_single_pass_during_unrelated_damage() {
    native_animation_frames(true, true);
}

fn native_animation_frames(markers: bool, interference: bool) {
    use winit::application::ApplicationHandler;
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, EventLoop};
    use winit::platform::windows::EventLoopBuilderExtWindows;
    use winit::window::{WindowAttributes, WindowId};

    struct Harness(bool, bool);
    impl ApplicationHandler for Harness {
        fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
            let options = crate::app::WindowOptions::new().follow_theme(self.1);
            let attributes = if self.1 {
                options.to_window_attributes()
            } else {
                WindowAttributes::default()
            };
            let window = Arc::from(
                event_loop
                    .create_window(
                        attributes
                            .with_title("Jadix rotation regression")
                            .with_surface_size(if self.1 {
                                winit::dpi::LogicalSize::new(800.0, 650.0)
                            } else {
                                winit::dpi::LogicalSize::new(320.0, 240.0)
                            })
                            .with_visible(false),
                    )
                    .unwrap(),
            );
            run_frames(window, self.0, self.1);
            event_loop.exit();
        }
        fn window_event(&mut self, _: &dyn ActiveEventLoop, _: WindowId, _: WindowEvent) {}
    }
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    builder
        .build()
        .unwrap()
        .run_app(Harness(markers, interference))
        .unwrap();
}

fn run_frames(window: Arc<dyn Window>, markers: bool, interference: bool) {
    use crate::components::scroll_viewport::scroll_viewport;
    use crate::core::reactivity::{create_root, use_signal};
    use crate::core::scroll_viewport::{scroll_offset_y_signal_key, scroll_target_y_signal_key};

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

    fn spinning_icon_bounds(element: &Element) -> Option<[f32; 4]> {
        if element
            .attrs
            .get(crate::utils::ATTR_ICON_NAME)
            .map(String::as_str)
            == Some("loader-circle")
        {
            Some(element.layout_bounds)
        } else {
            element.children.iter().find_map(spinning_icon_bounds)
        }
    }

    crate::style::utility::set_frame_dt(0.0);
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    crate::style::set_global_time_for_test(Some(0.0));
    if interference {
        crate::style::set_theme_mode(crate::style::ThemeMode::Dark);
    }
    let mut state = crate::utils::block_on::block_on(AppState::new_bootstrap(
        window,
        crate::app::WindowOptions::new().follow_theme(interference),
    ));
    let (_owner, ()) = create_root(|| {
        let content = if markers {
            let mut column = Element::new()
                .flex_col()
                .w(320.0)
                .h(400.0)
                .gap(12.0)
                .p(12.0)
                .bg([0.96, 0.96, 0.96, 1.0])
                .flex_shrink(0.0)
                .child(
                    Element::new()
                        .text("Switched to a new branch")
                        .text_size(14.0),
                );
            for label in [
                "Thinking...",
                "Generating response...",
                "Three sweeps then settle",
            ] {
                let icon = crate::utils::text_node::build_named_icon_text_node(
                    "loader-circle",
                    crate::utils::lucide_glyph("loader-circle").unwrap(),
                )
                .text_size(16.0)
                .class("animate-spin");
                let row: Element = crate::components::marker::marker()
                    .icon(icon)
                    .content_with(|content| {
                        content
                            .class(if interference && label == "Three sweeps then settle" {
                                "shimmer shimmer-count-3 shimmer-duration-900"
                            } else {
                                "shimmer"
                            })
                            .text(label)
                    })
                    .into();
                column = column.child(row);
                column = column.child(Element::from(
                    crate::components::marker::marker()
                        .variant("separator")
                        .content_with(|content| content.class("shimmer").text("Reading 4 files")),
                ));
            }
            let column = column.child(
                Element::new()
                    .p(8.0)
                    .bg([0.88, 0.88, 0.88, 1.0])
                    .class("shimmer-box")
                    .child(Element::new().text("shimmer-box overlay")),
            );
            Element::from(crate::components::card::card().child(column))
        } else {
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
                )
        };
        #[cfg(feature = "svg")]
        let content = if interference {
            crate::pages::basics::MarkerShowcase(Default::default())
        } else {
            content
        };
        let viewport: Element = scroll_viewport("rotation-scroll")
            .w(if interference { 800.0 } else { 320.0 })
            .h(if interference { 600.0 } else { 200.0 })
            .overflow_y("scroll")
            .scroll_behavior("jump")
            .child(content)
            .into();
        let mut root = Element::new()
            .w("100%")
            .h("100%")
            .bg([1.0; 4])
            .child(viewport);
        if markers {
            root = root.child(crate::utils::viewport_portal_stage(
                "shimmer-performance-overlay",
                Element::new()
                    .absolute()
                    .right(4.0)
                    .bottom(4.0)
                    .w(96.0)
                    .h(72.0)
                    .p(8.0)
                    .bg([0.07, 0.09, 0.15, 0.93])
                    .rounded(6.0)
                    .child(
                        Element::new()
                            .text("Performance / FPS")
                            .text_size(12.0)
                            .text_color([255, 255, 255, 255]),
                    ),
                1_000_000,
            ));
        }
        state.rebuild_ui(root);
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
    let scroll_id = "rotation-scroll";
    let mut retained_frames = Vec::new();
    let partial_frames_before = crate::app::damage::partial_load_frame_count();
    for step in 0..if markers { 32 } else { 16 } {
        crate::style::set_global_time_for_test(Some((step + 1) as f32 * 0.125));
        let mouse = if markers && step % 2 == 0 {
            [28.0, 40.0]
        } else {
            [-1.0; 2]
        };
        let scrolling =
            !interference && (matches!(step, 3 | 4) || (markers && matches!(step, 7 | 15 | 23)));
        let scroll = match step {
            _ if interference => 0.0,
            0..=2 => 0.0,
            3 => 8.0,
            7..=14 if markers => 80.0,
            15..=22 if markers => 176.0,
            _ => 16.0,
        };
        use_signal(&scroll_offset_y_signal_key(scroll_id), || 0.0_f32).set_silent(scroll);
        use_signal(&scroll_target_y_signal_key(scroll_id), || 0.0_f32).set_silent(scroll);
        crate::style::utility::set_frame_dt(0.125);
        crate::style::advance_css_transition_timeline_ms(125.0);
        if interference && step >= 8 && step % 4 == 0 {
            crate::app::damage::push_frame_region_paint(123456, [300.0, 4.0, 8.0, 188.0]);
        } else if interference && step >= 8 && step % 2 == 0 {
            crate::app::damage::push_frame_overlay_damage(
                crate::layout::layout_tree::overlay_portal_stamp_key(
                    &state.root_element.children[1],
                ),
                [220.0, 164.0, 96.0, 72.0],
            );
        }
        state
            .render(
                mouse,
                if interference && step % 4 == 0 {
                    ScrollbarUpdateMode::Full
                } else {
                    ScrollbarUpdateMode::ViewportsOnly
                },
                None,
                false,
                false,
                scrolling,
            )
            .unwrap();
        if interference {
            let captions: Vec<_> = state
                .text_nodes
                .iter()
                .filter(|node| node.glyph_fx.as_deref() == Some("shimmer"))
                .map(|node| (node.text.clone(), [node.x, node.y, node.width, node.height]))
                .collect();
            assert!(captions.len() >= 3);
            retained_frames.push((step, captions, read_frame(&state)));
            continue;
        }
        // Keep consecutive retained frames between references so a full draw
        // cannot hide residue accumulating after the scroll has stopped.
        if step != 0 && step % 4 != 3 {
            continue;
        }
        let partial = read_frame(&state);
        if markers && step == 0 {
            assert!(state
                .text_nodes
                .iter()
                .any(|node| node.text == "Thinking..."));
            let icon = spinning_icon_bounds(&state.root_element).unwrap();
            let scale = state.ui_scale();
            let left = (icon[0] * scale).max(0.0) as u32;
            let top = ((icon[1] - scroll) * scale).max(0.0) as u32;
            let right = ((icon[0] + icon[2]) * scale).ceil() as u32;
            let bottom = ((icon[1] + icon[3] - scroll) * scale).ceil() as u32;
            assert!(
                (top..bottom.min(partial.height)).any(|row| (left..right.min(partial.width)).any(
                    |column| partial
                        .pixel(column, row)
                        .is_some_and(|pixel| pixel[..3].iter().all(|channel| *channel < 200))
                )),
                "the spinning icon must actually render before comparing retained frames"
            );
        }
        if markers {
            if let Some(directory) = std::env::var_os("JADIX_ANIMATION_EVIDENCE") {
                std::fs::create_dir_all(&directory).unwrap();
                ::image::save_buffer(
                    std::path::Path::new(&directory).join(format!("shimmer-spin-{step}.png")),
                    &partial.pixels,
                    partial.width,
                    partial.height,
                    ::image::ColorType::Rgba8,
                )
                .unwrap();
            }
        }
        let angle = state.root_element.children[0].children[0].children[0]
            .style
            .rotate_deg;
        crate::style::utility::set_frame_dt(0.0);
        state.mark_full_redraw();
        state
            .render(
                mouse,
                ScrollbarUpdateMode::ViewportsOnly,
                None,
                false,
                false,
                false,
            )
            .unwrap();
        let full = read_frame(&state);
        if markers {
            if let Some(directory) = std::env::var_os("JADIX_ANIMATION_EVIDENCE") {
                ::image::save_buffer(
                    std::path::Path::new(&directory).join(format!("shimmer-spin-{step}-full.png")),
                    &full.pixels,
                    full.width,
                    full.height,
                    ::image::ColorType::Rgba8,
                )
                .unwrap();
            }
        }
        let differences = partial
            .pixels
            .chunks_exact(4)
            .zip(full.pixels.chunks_exact(4))
            .filter(|(a, b)| a.iter().zip(b.iter()).any(|(a, b)| a.abs_diff(*b) > 3))
            .count();
        assert_eq!(
            differences, 0,
            "frame={step}, angle={angle}, scroll={scroll}, scrolling={scrolling}: clipped rotation or stale pixels differ from full repaint"
        );
    }
    if interference {
        let partial_frames = crate::app::damage::partial_load_frame_count() - partial_frames_before;
        assert!(
            partial_frames >= 16,
            "expected retained frames, got {partial_frames}"
        );
    }
    for (step, captions, partial) in retained_frames {
        crate::style::set_global_time_for_test(Some((step + 1) as f32 * 0.125));
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
        let scale = state.ui_scale();
        for (caption, bounds) in captions {
            let left = (bounds[0] * scale).ceil() as u32;
            let top = (bounds[1] * scale).ceil() as u32;
            let right = ((bounds[0] + bounds[2]) * scale).floor() as u32;
            let bottom = ((bounds[1] + bounds[3]) * scale).floor() as u32;
            let differences = (top..bottom.min(partial.height))
                .flat_map(|row| (left..right.min(partial.width)).map(move |column| (column, row)))
                .filter(|&(column, row)| {
                    partial.pixel(column, row).unwrap()[..3]
                        .iter()
                        .zip(&full.pixel(column, row).unwrap()[..3])
                        .any(|(actual, expected)| actual.abs_diff(*expected) > 3)
                })
                .count();
            assert_eq!(
                differences, 0,
                "retained shimmer ink differs at frame {step}: {caption}"
            );
        }
    }
    crate::style::set_css_transition_virtual_timeline(false);
    crate::style::set_global_time_for_test(None);
    crate::style::utility::set_frame_dt(0.016);
}
