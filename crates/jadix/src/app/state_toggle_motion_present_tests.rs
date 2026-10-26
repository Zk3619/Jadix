use super::*;
use crate::core::reactivity::{create_root, use_signal};
use crate::core::tree::LayoutStatus;

#[test]
#[ignore = "requires a native Windows GPU surface"]
fn checkbox_checked_native_frames_match_full_repaint() {
    native_frames(false);
}

#[test]
#[ignore = "requires a native Windows GPU surface"]
fn translated_scaled_native_frames_match_full_repaint() {
    native_frames(true);
}

#[test]
fn checkbox_checked_sync_keeps_layout_and_invalidates_its_paint_spine() {
    for use_protocol in [false, true] {
        let (_owner, ()) = create_root(|| {
            let checked = use_signal("checkbox-paint-only", || false);
            checked.set(false);
            let mut root = Element::new()
                .w(240.0)
                .h(80.0)
                .child(Element::from(
                    crate::components::checkbox::checkbox("checkbox-paint-only")
                        .checked_signal(checked),
                ))
                .child(Element::new().text("Unchanged sibling"));
            let mut tree = crate::layout::layout_tree::LayoutTree::new();
            tree.build_and_layout(&mut root, 240.0, 80.0, [-1.0; 2]);
            let bounds = root.children[0].layout_bounds;
            let sibling_hash = root.children[1].collect_paint_hash;
            assert_ne!(sibling_hash, 0);
            checked.set(true);
            let mut impact = LayoutStatus::Clean;
            if use_protocol {
                impact = crate::authoring::presentation::sync_tree(
                    &mut root,
                    &mut tree,
                    crate::authoring::presentation::Context {
                        phase: crate::authoring::presentation::Phase::State,
                        viewport: [240.0, 80.0],
                    width_budget: None,
},
                );
            } else {
                crate::components::checkbox::sync_checkbox_checked_subtree(
                    &mut root,
                    &mut tree,
                    &mut impact,
                );
            }
            assert_eq!(
                impact,
                LayoutStatus::StyleChanged,
                "use_protocol={use_protocol}"
            );
            assert_eq!(root.children[0].layout_bounds, bounds);
            assert_eq!(root.collect_paint_hash, 0);
            assert_eq!(root.children[0].children[0].collect_paint_hash, 0);
            assert_eq!(root.children[1].collect_paint_hash, sibling_hash);
        });
    }
}

fn native_frames(motion: bool) {
    use winit::application::ApplicationHandler;
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, EventLoop};
    use winit::platform::windows::EventLoopBuilderExtWindows;
    use winit::window::{WindowAttributes, WindowId};

    struct Harness(bool);
    impl ApplicationHandler for Harness {
        fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
            let window: Arc<dyn Window> = Arc::from(
                event_loop
                    .create_window(
                        WindowAttributes::default()
                            .with_title("Jadix toggle and motion regression")
                            .with_surface_size(winit::dpi::LogicalSize::new(360.0, 240.0))
                            .with_visible(false),
                    )
                    .unwrap(),
            );
            for mode in [
                crate::style::ThemeMode::Light,
                crate::style::ThemeMode::Dark,
            ] {
                for scroll in if self.0 {
                    vec![64.0, 112.0]
                } else {
                    vec![64.0]
                } {
                    let partial = run_frames(window.clone(), self.0, scroll, mode, false);
                    let full = run_frames(window.clone(), self.0, scroll, mode, true);
                    for (step, (partial, full)) in partial.iter().zip(&full).enumerate() {
                        let differences = partial
                            .pixels
                            .chunks_exact(4)
                            .zip(full.pixels.chunks_exact(4))
                            .filter(|(actual, expected)| {
                                actual
                                    .iter()
                                    .zip(expected.iter())
                                    .any(|(actual, expected)| actual.abs_diff(*expected) > 3)
                            })
                            .count();
                        if differences > 0 {
                            let directory = std::env::temp_dir().join("jadix-gallery-repaint");
                            std::fs::create_dir_all(&directory).unwrap();
                            for (name, image) in [("partial", partial), ("full", full)] {
                                ::image::save_buffer(
                                    directory.join(format!(
                                        "continuous-{}-{mode:?}-{scroll}-{step}-{name}.png",
                                        self.0
                                    )),
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
                            "motion={}, mode={mode:?}, scroll={scroll}, frame={step}",
                            self.0
                        );
                    }
                }
            }
            event_loop.exit();
        }

        fn window_event(&mut self, _: &dyn ActiveEventLoop, _: WindowId, _: WindowEvent) {}
    }
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    builder.build().unwrap().run_app(Harness(motion)).unwrap();
}

fn run_frames(
    window: Arc<dyn Window>,
    motion: bool,
    scroll: f32,
    mode: crate::style::ThemeMode,
    full: bool,
) -> Vec<crate::renderer::offscreen::RgbaImage> {
    crate::init();
    crate::style::set_theme_mode(mode);
    crate::style::utility::set_frame_dt(0.0);
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    let mut state = crate::utils::block_on::block_on(AppState::new_bootstrap(
        window,
        crate::app::WindowOptions::new(),
    ));
    let (_owner, ()) = create_root(|| {
        let checked = use_signal("present-checkbox", || true);
        let checkbox: Element = crate::components::checkbox::checkbox("present-checkbox")
            .checked_signal(checked)
            .child(Element::new().text("Keep form state"))
            .into();
        let track = Element::new()
            .w(256.0)
            .h(40.0)
            .rounded(20.0)
            .bg("muted")
            .flex_items("center")
            .overflow("hidden")
            .child(
                Element::new()
                    .template_id("moving-ball")
                    .w(28.0)
                    .h(28.0)
                    .flex_shrink(0.0)
                    .rounded(14.0)
                    .bg("destructive"),
            );
        let bar = Element::new()
            .template_id("scaling-bar")
            .w(20.0)
            .h(56.0)
            .rounded(6.0)
            .bg("primary")
            .transform_origin("center bottom");
        let content = Element::new()
            .w(320.0)
            .h(560.0)
            .flex_col()
            .gap(16.0)
            .p(24.5)
            .bg("card")
            .child(Element::new().h(72.0).flex_shrink(0.0))
            .child(checkbox)
            .child(Element::from(
                crate::components::switch::switch("present-switch").checked_signal(checked),
            ))
            .child(track)
            .child(bar);
        let viewport: Element =
            crate::components::scroll_viewport::scroll_viewport("present-scroll")
                .w(320.0)
                .h(210.0)
                .overflow_y("scroll")
                .scroll_behavior("jump")
                .child(content)
                .into();
        state.rebuild_ui(
            Element::new()
                .w("100%")
                .h("100%")
                .bg("background")
                .child(viewport),
        );
    });
    use_signal(
        &crate::core::scroll_viewport::scroll_offset_y_signal_key("present-scroll"),
        || 0.0_f32,
    )
    .set_silent(scroll);
    use_signal(
        &crate::core::scroll_viewport::scroll_target_y_signal_key("present-scroll"),
        || 0.0_f32,
    )
    .set_silent(scroll);
    for _ in 0..3 {
        render(&mut state);
    }
    let partial_before = partial_load_frame_count();
    let full_before = full_clear_frame_count();
    let mut frames = Vec::new();
    for step in 0..360 {
        let pose = step % 48;
        if motion {
            let offset = if pose < 24 {
                pose as f32 * 0.47
            } else {
                (47 - pose) as f32 * 9.35
            };
            let scale = if pose < 24 {
                0.2 + pose as f32 * 0.011
            } else {
                1.0 - (pose - 24) as f32 * 0.025
            };
            state.update_element_by_template_id("moving-ball", |element| {
                element.style.translate[0] = offset;
                LayoutStatus::StyleChanged
            });
            state.update_element_by_template_id("scaling-bar", |element| {
                element.style.scale[1] = scale;
                LayoutStatus::StyleChanged
            });
        } else if step % 12 == 0 {
            let checkbox =
                crate::core::tree::find_by_template_id(&state.root_element, "present-checkbox")
                    .unwrap();
            checkbox.children[0].on_click.as_ref().unwrap().borrow_mut()();
        }
        if full {
            state.mark_full_redraw();
        }
        render(&mut state);
        frames.push(read_frame(&state));
    }
    if full {
        assert_eq!(full_clear_frame_count() - full_before, 360);
    } else {
        assert!(partial_load_frame_count() - partial_before >= if motion { 300 } else { 30 });
        assert_eq!(full_clear_frame_count() - full_before, 0);
    }
    crate::style::set_css_transition_virtual_timeline(false);
    frames
}

pub(super) fn render(state: &mut AppState) {
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

pub(super) fn read_frame(state: &AppState) -> crate::renderer::offscreen::RgbaImage {
    readback_texture(
        &state.device,
        &state.queue,
        state.present_retain.color_texture_for_test().unwrap(),
        state.config.width,
        state.config.height,
        state.device.create_command_encoder(&Default::default()),
    )
    .unwrap()
}
