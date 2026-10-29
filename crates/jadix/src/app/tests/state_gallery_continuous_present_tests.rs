use super::*;
use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
use crate::app::runtime::{clear_app_runtime, set_app_runtime, AppRuntime};
use crate::core::reactivity::{create_effect, create_root, use_signal};
use std::{cell::RefCell, rc::Rc};

#[test]
#[ignore = "requires native Windows GPU presentation"]
fn message_scroller_latest_exit_fades_through_complete_native_frames() {
    use winit::application::ApplicationHandler;
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, EventLoop};
    use winit::platform::windows::EventLoopBuilderExtWindows;
    use winit::window::{WindowAttributes, WindowId};

    fn jump_button(element: &Element) -> Option<&Element> {
        if element.get_attr("data-slot").map(String::as_str) == Some("message-scroller-button") {
            return Some(element);
        }
        element.children.iter().find_map(jump_button)
    }

    struct Harness;
    impl ApplicationHandler for Harness {
        fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
            crate::init();
            clear_app_runtime();
            reset_registered_anchors();
            crate::style::set_theme_mode(crate::style::ThemeMode::Dark);
            crate::style::set_css_transition_virtual_timeline(true);
            crate::style::set_css_transition_timeline_ms(0.0);
            let window: Arc<dyn Window> = Arc::from(
                event_loop
                    .create_window(
                        WindowAttributes::default()
                            .with_title("Latest exit regression")
                            .with_surface_size(winit::dpi::LogicalSize::new(720.0, 900.0))
                            .with_visible(false),
                    )
                    .unwrap(),
            );
            let mut state = crate::utils::block_on::block_on(AppState::new_bootstrap(
                window,
                crate::app::WindowOptions::new(),
            ));
            let (owner, root) = create_root(|| {
                crate::authoring::mount::with_inline_mount(|| {
                    Element::new()
                        .w("100%")
                        .h("100%")
                        .p(16.0)
                        .bg(crate::style::parse_color("background"))
                        .child(crate::pages::basics::MessageScrollerShowcase(
                            Default::default(),
                        ))
                })
            });
            state.rebuild_ui(root);
            for _ in 0..4 {
                super::toggle_motion_present_tests::render(&mut state);
            }
            let viewport = "gal-ms-showcase:viewport";
            let offset = use_signal(
                &crate::core::scroll_viewport::scroll_offset_y_signal_key(viewport),
                || 0.0_f32,
            );
            let max = use_signal(
                &crate::core::scroll_viewport::scroll_content_h_signal_key(viewport),
                || 0.0_f32,
            )
            .peek()
                - use_signal(
                    &crate::core::scroll_viewport::scroll_viewport_h_signal_key(viewport),
                    || 0.0_f32,
                )
                .peek();
            assert!(max > 100.0, "Gallery conversation must overflow");
            use_signal("gal-ms-showcase:auto_scroll", || true).set(false);
            offset.set_silent(0.0);
            state.update_scroll_offset_in_place(viewport, [0.0, 0.0]);
            super::toggle_motion_present_tests::render(&mut state);
            crate::style::set_css_transition_timeline_ms(220.0);
            super::toggle_motion_present_tests::render(&mut state);
            let button = jump_button(&state.root_element).unwrap();
            assert!(button.style.opacity > 0.99, "Latest must finish entering");
            let bounds = button.viewport_layout_bounds();
            offset.set_silent(max);
            state.update_scroll_offset_in_place(viewport, [0.0, max]);
            let mut previous_opacity = 1.0;
            for elapsed in (0..=208).step_by(16) {
                crate::style::set_css_transition_timeline_ms(300.0 + elapsed as f64);
                super::toggle_motion_present_tests::render(&mut state);
                let opacity = jump_button(&state.root_element).unwrap().style.opacity;
                assert!(
                    opacity <= previous_opacity + 0.001,
                    "exit must not restart at {elapsed}ms"
                );
                if elapsed > 0 && elapsed < 160 {
                    assert!(
                        opacity > 0.0 && opacity < previous_opacity,
                        "exit stopped fading at {elapsed}ms: {opacity}"
                    );
                }
                previous_opacity = opacity;
                let partial = super::toggle_motion_present_tests::read_frame(&state);
                state.mark_full_redraw();
                super::toggle_motion_present_tests::render(&mut state);
                let full = super::toggle_motion_present_tests::read_frame(&state);
                let scale = state.window.scale_factor() as f32;
                let differences = partial
                    .pixels
                    .chunks_exact(4)
                    .zip(full.pixels.chunks_exact(4))
                    .enumerate()
                    .filter(|(index, (actual, expected))| {
                        let horizontal = (*index % full.width as usize) as f32 / scale;
                        let vertical = (*index / full.width as usize) as f32 / scale;
                        horizontal >= bounds[0] - 12.0
                            && horizontal <= bounds[0] + bounds[2] + 12.0
                            && vertical >= bounds[1] - 12.0
                            && vertical <= bounds[1] + bounds[3] + 20.0
                            && actual
                                .iter()
                                .zip(*expected)
                                .any(|(actual, expected)| actual.abs_diff(*expected) > 3)
                    })
                    .count();
                assert_eq!(
                    differences, 0,
                    "Latest exit frame differs from full repaint at {elapsed}ms"
                );
            }
            assert!(previous_opacity < 0.001, "exit must finish hidden");
            drop(owner);
            clear_app_runtime();
            crate::style::set_css_transition_virtual_timeline(false);
            event_loop.exit();
        }
        fn window_event(&mut self, _: &dyn ActiveEventLoop, _: WindowId, _: WindowEvent) {}
    }
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    builder.build().unwrap().run_app(Harness).unwrap();
}

#[test]
#[ignore = "requires native Windows GPU presentation"]
fn multiline_ime_commit_clears_preedit_pixels_immediately() {
    use winit::application::ApplicationHandler;
    use winit::event::{Ime, WindowEvent};
    use winit::event_loop::{ActiveEventLoop, EventLoop};
    use winit::platform::windows::EventLoopBuilderExtWindows;
    use winit::window::WindowId;

    struct Harness;
    impl ApplicationHandler for Harness {
        fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
            crate::init();
            clear_app_runtime();
            reset_registered_anchors();
            crate::style::set_theme_mode(crate::style::ThemeMode::Dark);
            crate::style::set_css_transition_virtual_timeline(true);
            crate::style::set_css_transition_timeline_ms(0.0);
            let (sender, receiver) = std::sync::mpsc::channel();
            crate::app::ui_bridge::install_proxy(event_loop.create_proxy(), sender);
            let mut app = crate::app::lifecycle::create_app(
                crate::app::WindowOptions::new().size(600.0, 700.0),
                || {
                    Element::new()
                        .w("100%")
                        .h("100%")
                        .p(24.0)
                        .bg(crate::style::parse_color("background"))
                        .child(crate::pages::forms::InputAdornmentShowcase(
                            Default::default(),
                        ))
                },
                event_loop.create_proxy(),
                receiver,
            );
            crate::app::with_ui_build_stack(|| app.can_create_surfaces(event_loop));
            let state = app.state.clone();
            let window = state.borrow().as_ref().unwrap().window.clone();
            window.set_visible(false);
            for _ in 0..3 {
                drain_deferred_view_remounts();
                super::toggle_motion_present_tests::render(state.borrow_mut().as_mut().unwrap());
            }
            use_signal("__jadix_button_focus_id", String::new).set("textarea:gal-ia-ta".into());
            for text in ["T", "Th", "Thus"] {
                app.handle_ime_event(
                    Ime::Preedit(text.into(), Some((text.len(), text.len()))),
                    window.id(),
                );
                drain_deferred_view_remounts();
                app.handle_redraw_requested(window.id(), event_loop);
            }
            let composing = super::toggle_motion_present_tests::read_frame(
                state.borrow_mut().as_mut().unwrap(),
            );
            app.handle_ime_event(Ime::Commit("Thus".into()), window.id());
            app.handle_redraw_requested(window.id(), event_loop);
            let mut live = state.borrow_mut();
            let live = live.as_mut().unwrap();
            let partial = super::toggle_motion_present_tests::read_frame(live);
            live.mark_full_redraw();
            super::toggle_motion_present_tests::render(live);
            let full = super::toggle_motion_present_tests::read_frame(live);
            let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../.theme-evidence/ui-regressions");
            std::fs::create_dir_all(&directory).unwrap();
            for (name, image) in [
                ("composing", &composing),
                ("committed", &partial),
                ("full", &full),
            ] {
                ::image::RgbaImage::from_raw(image.width, image.height, image.pixels.clone())
                    .unwrap()
                    .save(directory.join(format!("ime-{name}.png")))
                    .unwrap();
            }
            let differences = partial
                .pixels
                .chunks_exact(4)
                .zip(full.pixels.chunks_exact(4))
                .filter(|(actual, expected)| {
                    actual
                        .iter()
                        .zip(*expected)
                        .any(|(actual, expected)| actual.abs_diff(*expected) > 3)
                })
                .count();
            assert_eq!(differences, 0, "IME commit left stale pixels");
            assert_eq!(
                use_signal(
                    &crate::utils::text_field::input_value_signal_key("gal-ia-ta"),
                    String::new
                )
                .peek(),
                "Thus"
            );
            crate::style::set_css_transition_virtual_timeline(false);
            event_loop.exit();
        }
        fn window_event(&mut self, _: &dyn ActiveEventLoop, _: WindowId, _: WindowEvent) {}
    }
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    builder.build().unwrap().run_app(Harness).unwrap();
}

#[cfg(feature = "performance")]
#[test]
#[ignore = "requires the complete native Gallery and Windows GPU presentation"]
fn real_gallery_hover_card_scroll_keeps_performance_panel_anchored() {
    check_real_gallery_scroll(false);
}

#[cfg(feature = "performance")]
#[test]
#[ignore = "requires the complete native Gallery and Windows GPU presentation"]
fn real_gallery_filter_scroll_keeps_sidebar_text_visible() {
    check_real_gallery_scroll(true);
}

#[cfg(feature = "performance")]
fn check_real_gallery_scroll(filters: bool) {
    use winit::application::ApplicationHandler;
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, EventLoop};
    use winit::platform::windows::EventLoopBuilderExtWindows;
    use winit::window::WindowId;

    struct Harness(bool);
    impl ApplicationHandler for Harness {
        fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
            let filters = self.0;
            let scroll_id = if filters {
                "scroll-layout"
            } else {
                "scroll-overlays"
            };
            crate::init();
            crate::style::load_and_install_theme_overrides(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../examples/gallery/theme.toml"),
            )
            .unwrap();
            clear_app_runtime();
            reset_registered_anchors();
            crate::style::set_theme_mode(crate::style::ThemeMode::Dark);
            crate::style::set_css_transition_virtual_timeline(true);
            crate::style::set_css_transition_timeline_ms(0.0);
            use_signal("gal_nav_section", String::new)
                .set_silent(if filters { "layout" } else { "overlays" }.into());
            use_signal("gal-perf-visible", || true).set_silent(true);
            crate::shared::install_performance_overlay();
            let (tx, rx) = std::sync::mpsc::channel();
            crate::app::ui_bridge::install_proxy(event_loop.create_proxy(), tx);
            let mut app = crate::app::lifecycle::create_app(
                crate::app::WindowOptions::new().size(1000.0, 700.0),
                crate::gallery_app::build_gallery_app,
                event_loop.create_proxy(),
                rx,
            );
            crate::app::with_ui_build_stack(|| app.can_create_surfaces(event_loop));
            let state = app.state.clone();
            let window = state.borrow().as_ref().unwrap().window.clone();
            window.set_visible(false);
            for _ in 0..4 {
                drain_deferred_view_remounts();
                super::toggle_motion_present_tests::render(state.borrow_mut().as_mut().unwrap());
            }
            let scroll = {
                let live = state.borrow();
                fn first_filter(element: &Element) -> Option<&Element> {
                    if element.style.filter.is_some() {
                        return Some(element);
                    }
                    element.children.iter().find_map(first_filter)
                }
                let root = &live.as_ref().unwrap().root_element;
                let target = if filters {
                    first_filter(root).expect("Gallery filter")
                } else {
                    crate::core::tree::find_by_template_id(root, "gallery:hover_card_builder_btn")
                        .unwrap()
                };
                (target.layout_bounds[1] - 300.0).max(0.0)
            };
            let sidebar = super::toggle_motion_present_tests::read_frame(
                state.borrow_mut().as_mut().unwrap(),
            );
            let mut mismatched_frames = 0;
            for frame in 0_i32..90 {
                let offset = if filters {
                    scroll + (45 - (frame % 90 - 45).abs()) as f32 * 20.0
                } else {
                    scroll + (frame % 12) as f32 * 0.3
                };
                for key in [
                    crate::core::scroll_viewport::scroll_offset_y_signal_key(scroll_id),
                    crate::core::scroll_viewport::scroll_target_y_signal_key(scroll_id),
                ] {
                    use_signal(&key, || 0.0_f32).set_silent(offset);
                }
                if !filters && frame % 6 == 0 {
                    use_signal("gal_hover_card_builder_open", || false).set(frame % 12 == 0);
                }
                if frame % 9 == 0 {
                    use_signal("gal-perf-text", String::new).set(format!(
                        "Window 109 FPS\nCPU 2.1%\nGPU 36.6%\nRAM {} MiB",
                        518 + frame,
                    ));
                }
                drain_deferred_view_remounts();
                {
                    let mut live = state.borrow_mut();
                    live.as_mut()
                        .unwrap()
                        .update_scroll_offset_in_place(scroll_id, [0.0, offset]);
                }
                let pointer = {
                    let live = state.borrow();
                    let bounds = if filters {
                        [650.0, 350.0, 1.0, 1.0]
                    } else {
                        crate::core::tree::find_by_template_id(
                            &live.as_ref().unwrap().root_element,
                            "gallery:hover_card_builder_btn",
                        )
                        .unwrap()
                        .viewport_layout_bounds()
                    };
                    if frame % 12 < 6 {
                        [bounds[0] + bounds[2] * 0.5, bounds[1] + bounds[3] * 0.5]
                    } else {
                        [450.0, 350.0]
                    }
                };
                app.with_primary_input(window.id(), |app| {
                    let scale = window.scale_factor();
                    app.handle_cursor_moved(
                        window.id(),
                        winit::dpi::PhysicalPosition::new(
                            pointer[0] as f64 * scale,
                            pointer[1] as f64 * scale,
                        ),
                        event_loop,
                    );
                    app.flush_pending_cursor_moved(window.id(), event_loop);
                    app.handle_mouse_wheel(
                        window.id(),
                        winit::event::MouseScrollDelta::PixelDelta(
                            winit::dpi::PhysicalPosition::new(0.0, -0.45),
                        ),
                    );
                });
                app.last_frame_time =
                    std::time::Instant::now() - std::time::Duration::from_millis(16);
                for motion in app.smooth_scroll_motion.values_mut() {
                    motion.1 = app.last_frame_time;
                }
                app.handle_redraw_requested(window.id(), event_loop);
                let mut live = state.borrow_mut();
                let live = live.as_mut().unwrap();
                if !filters {
                    live.update_scroll_offset_in_place(
                        scroll_id,
                        [0.0, scroll + (frame % 12) as f32 * 0.3],
                    );
                    live.scroll_layer_clear = true;
                    super::toggle_motion_present_tests::render(live);
                }
                let panel =
                    crate::core::tree::find_by_template_id(&live.root_element, "gal-perf-panel")
                        .expect("performance panel")
                        .viewport_layout_bounds();
                let viewport = [
                    live.config.width as f32 / live.window.scale_factor() as f32,
                    live.config.height as f32 / live.window.scale_factor() as f32,
                ];
                if (viewport[0] - panel[0] - panel[2] - 12.0).abs() >= 1.0
                    || (viewport[1] - panel[1] - panel[3] - 12.0).abs() >= 1.0
                {
                    for id in ["gal-perf-panel", "gal-perf-toggle", "__jadix_portal_layer"] {
                        if let Some(element) =
                            crate::core::tree::find_by_template_id(&live.root_element, id)
                        {
                            eprintln!(
                                "HUD FAILURE {id}: node={:?} bounds={:?} style={:?}",
                                element.layout_node, element.layout_bounds, element.style
                            );
                        }
                    }
                }
                assert!(
                    (viewport[0] - panel[0] - panel[2] - 12.0).abs() < 1.0
                        && (viewport[1] - panel[1] - panel[3] - 12.0).abs() < 1.0,
                    "performance panel detached at frame {frame}: {panel:?}, viewport={viewport:?}"
                );
                let toggle =
                    crate::core::tree::find_by_template_id(&live.root_element, "gal-perf-toggle")
                        .expect("performance toggle")
                        .viewport_layout_bounds();
                assert!(
                    toggle[2] > 40.0
                        && toggle[3] > 20.0
                        && (toggle[0] + toggle[2] - panel[0] - panel[2]).abs() < 1.0,
                    "performance toggle lost layout at frame {frame}: {toggle:?}"
                );
                let partial = super::toggle_motion_present_tests::read_frame(live);
                if filters {
                    let sidebar_width = (220.0 * window.scale_factor()) as usize;
                    let differences = partial
                        .pixels
                        .chunks_exact(4)
                        .zip(sidebar.pixels.chunks_exact(4))
                        .enumerate()
                        .filter(|(index, (actual, expected))| {
                            index % (sidebar.width as usize) < sidebar_width
                                && actual
                                    .iter()
                                    .zip(*expected)
                                    .any(|(actual, expected)| actual.abs_diff(*expected) > 3)
                        })
                        .count();
                    if differences > 0 {
                        eprintln!("FAILED text nodes: {}", live.text_nodes.len());
                        for text in live.text_nodes.iter().filter(|text| text.x < 240.0) {
                            eprintln!(
                                "SIDEBAR {} {:?} clip={:?}",
                                text.text,
                                [text.x, text.y, text.width, text.height],
                                text.clip_bounds
                            );
                        }
                        let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                            .join("../../.theme-evidence/ui-regressions");
                        std::fs::create_dir_all(&directory).unwrap();
                        for (name, image) in [("partial", &partial), ("baseline", &sidebar)] {
                            ::image::RgbaImage::from_raw(
                                image.width,
                                image.height,
                                image.pixels.clone(),
                            )
                            .unwrap()
                            .save(directory.join(format!("filter-{name}-{frame}.png")))
                            .unwrap();
                        }
                        live.mark_full_redraw();
                        super::toggle_motion_present_tests::render(live);
                        let full = super::toggle_motion_present_tests::read_frame(live);
                        ::image::RgbaImage::from_raw(full.width, full.height, full.pixels)
                            .unwrap()
                            .save(directory.join(format!("filter-full-{frame}.png")))
                            .unwrap();
                    }
                    assert_eq!(
                        differences, 0,
                        "filter scroll erased Gallery sidebar at frame {frame}"
                    );
                    continue;
                }
                live.mark_full_redraw();
                super::toggle_motion_present_tests::render(live);
                let full = super::toggle_motion_present_tests::read_frame(live);
                let scale = live.window.scale_factor() as f32;
                let left = ((panel[0] + 2.0) * scale) as usize;
                let top = ((panel[1] + 2.0) * scale) as usize;
                let right = ((panel[0] + panel[2] - 2.0) * scale) as usize;
                let bottom = ((panel[1] + panel[3] - 2.0) * scale) as usize;
                let differences = (top..bottom)
                    .flat_map(|vertical| {
                        (left..right).map(move |horizontal| (horizontal, vertical))
                    })
                    .filter(|(horizontal, vertical)| {
                        let logical = [*horizontal as f32 / scale, *vertical as f32 / scale];
                        if logical[1] < toggle[1] + toggle[3] + 4.0
                            && !(logical[0] > toggle[0] + 2.0
                                && logical[0] < toggle[0] + toggle[2] - 2.0
                                && logical[1] > toggle[1] + 2.0
                                && logical[1] < toggle[1] + toggle[3] - 2.0)
                        {
                            return false;
                        }
                        let pixel = (vertical * full.width as usize + horizontal) * 4;
                        partial.pixels[pixel..pixel + 4]
                            .iter()
                            .zip(&full.pixels[pixel..pixel + 4])
                            .any(|(actual, expected)| actual.abs_diff(*expected) > 3)
                    })
                    .count();
                if differences > 0 {
                    mismatched_frames += 1;
                    eprintln!("performance panel frame {frame}: {differences} differing pixels, bounds={panel:?}");
                    if mismatched_frames <= 4 {
                        let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                            .join("../../.theme-evidence/ui-regressions");
                        std::fs::create_dir_all(&directory).unwrap();
                        for (name, image) in [("partial", partial), ("full", full)] {
                            ::image::RgbaImage::from_raw(image.width, image.height, image.pixels)
                                .unwrap()
                                .save(directory.join(format!("hover-{name}-{frame}.png")))
                                .unwrap();
                        }
                    }
                }
            }
            assert_eq!(
                mismatched_frames, 0,
                "performance panel paint diverged from full repaint"
            );
            drop(app);
            clear_app_runtime();
            crate::style::set_css_transition_virtual_timeline(false);
            event_loop.exit();
        }
        fn window_event(&mut self, _: &dyn ActiveEventLoop, _: WindowId, _: WindowEvent) {}
    }
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    builder.build().unwrap().run_app(Harness(filters)).unwrap();
}

#[test]
#[ignore = "requires native Windows GPU presentation"]
fn real_gallery_svg_filters_retain_pixels_during_scroll() {
    use winit::application::ApplicationHandler;
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, EventLoop};
    use winit::platform::windows::EventLoopBuilderExtWindows;
    use winit::window::{WindowAttributes, WindowId};

    struct Harness;
    impl ApplicationHandler for Harness {
        fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
            crate::init();
            clear_app_runtime();
            reset_registered_anchors();
            crate::style::set_css_transition_virtual_timeline(true);
            crate::style::set_css_transition_timeline_ms(0.0);
            let gallery_root =
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/gallery");
            crate::style::load_and_install_theme_overrides(gallery_root.join("theme.toml"))
                .unwrap();
            let source = format!(
                "{}/assets/firefox-logo.svg",
                crate::shared::gallery_resource_root().display()
            );
            let key = crate::app::pending_images::texture_key_for_src(&source);
            assert!(crate::app::pending_images::queue_image_from_src(
                key,
                gallery_root
                    .join("assets/firefox-logo.svg")
                    .to_str()
                    .unwrap(),
            ));
            let window: Arc<dyn Window> = Arc::from(
                event_loop
                    .create_window(
                        WindowAttributes::default()
                            .with_title("Gallery SVG filter scroll regression")
                            .with_surface_size(winit::dpi::LogicalSize::new(1000.0, 700.0))
                            .with_visible(false),
                    )
                    .unwrap(),
            );
            let mut state = crate::utils::block_on::block_on(AppState::new_bootstrap(
                window,
                crate::app::WindowOptions::new(),
            ));
            let (owner, root) = create_root(|| {
                crate::authoring::mount::with_inline_mount(|| {
                    Element::new()
                        .w("100%")
                        .h("100%")
                        .flex_row()
                        .child(
                            Element::new()
                                .w(200.0)
                                .h("100%")
                                .flex_shrink(0.0)
                                .text("Jadix Gallery\nNavigation\nForms\nLayout\nText\nAnimation"),
                        )
                        .child(
                            crate::components::scroll_area("filter-scroll-regression")
                                .flex_grow(1.0)
                                .min_w(0.0)
                                .h("100%")
                                .child(Element::new().h(4800.0).flex_shrink(0.0))
                                .child(crate::pages::layout::FilterShowcase(Default::default())),
                        )
                })
            });
            state.rebuild_ui(root);
            for _ in 0..4 {
                super::toggle_motion_present_tests::render(&mut state);
            }
            let mut pixels_by_key = std::collections::HashMap::new();
            for frame in 0..60 {
                let offset = 4720.0 + (frame % 30) as f32 * 0.3;
                use_signal(
                    &crate::core::scroll_viewport::scroll_offset_y_signal_key(
                        "filter-scroll-regression",
                    ),
                    || 0.0_f32,
                )
                .set_silent(offset);
                state.update_scroll_offset_in_place("filter-scroll-regression", [0.0, offset]);
                state.scroll_layer_clear = true;
                super::toggle_motion_present_tests::render(&mut state);
                let partial = super::toggle_motion_present_tests::read_frame(&mut state);
                state.mark_full_redraw();
                super::toggle_motion_present_tests::render(&mut state);
                let full = super::toggle_motion_present_tests::read_frame(&mut state);
                let sidebar_width = (195.0 * state.window.scale_factor()) as usize;
                let sidebar_differences = partial
                    .pixels
                    .chunks_exact(4)
                    .zip(full.pixels.chunks_exact(4))
                    .enumerate()
                    .filter(|(index, (actual, expected))| {
                        index % (full.width as usize) < sidebar_width
                            && actual
                                .iter()
                                .zip(*expected)
                                .any(|(actual, expected)| actual.abs_diff(*expected) > 3)
                    })
                    .count();
                assert_eq!(
                    sidebar_differences, 0,
                    "filter scroll erased sidebar text at frame {frame}"
                );
                fn filter_bounds(element: &Element, bounds: &mut Vec<[f32; 4]>) {
                    if element.style.filter.is_some() {
                        bounds.push(element.viewport_layout_bounds());
                    }
                    for child in &element.children {
                        filter_bounds(child, bounds);
                    }
                }
                let mut bounds = Vec::new();
                filter_bounds(&state.root_element, &mut bounds);
                let scale = state.window.scale_factor() as f32;
                for bounds in bounds {
                    let left = ((bounds[0] + 4.0).max(0.0) * scale) as usize;
                    let top = ((bounds[1] + 4.0).max(0.0) * scale) as usize;
                    let right =
                        (((bounds[0] + bounds[2] - 4.0) * scale) as usize).min(full.width as usize);
                    let bottom = (((bounds[1] + bounds[3] - 4.0) * scale) as usize)
                        .min(full.height as usize);
                    let differences = (top..bottom)
                        .flat_map(|vertical| {
                            (left..right).map(move |horizontal| (horizontal, vertical))
                        })
                        .filter(|(horizontal, vertical)| {
                            let pixel = (vertical * full.width as usize + horizontal) * 4;
                            partial.pixels[pixel..pixel + 4]
                                .iter()
                                .zip(&full.pixels[pixel..pixel + 4])
                                .any(|(actual, expected)| actual.abs_diff(*expected) > 3)
                        })
                        .count();
                    if differences > 0 {
                        let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                            .join("../../.theme-evidence/ui-regressions");
                        std::fs::create_dir_all(&directory).unwrap();
                        for (name, image) in [("partial", &partial), ("full", &full)] {
                            ::image::RgbaImage::from_raw(
                                image.width,
                                image.height,
                                image.pixels.clone(),
                            )
                            .unwrap()
                            .save(directory.join(format!("svg-{name}-{frame}.png")))
                            .unwrap();
                        }
                    }
                    assert_eq!(
                        differences, 0,
                        "SVG filter presentation flickered at frame {frame}: {bounds:?}"
                    );
                }
                let outputs: Vec<_> = state
                    .image_textures
                    .keys()
                    .filter(|key| key.starts_with("jadix_filter:"))
                    .cloned()
                    .collect();
                assert!(
                    outputs.len() >= 15,
                    "SVG filter outputs missing at frame {frame}: {}",
                    outputs.len()
                );
                for key in outputs {
                    let texture = &state.image_textures.get(&key).unwrap()._texture;
                    let image = crate::renderer::offscreen::readback_texture(
                        &state.device,
                        &state.queue,
                        texture,
                        texture.width(),
                        texture.height(),
                        state.device.create_command_encoder(&Default::default()),
                    )
                    .unwrap();
                    assert!(
                        image.pixels.chunks_exact(4).any(|pixel| pixel[3] > 2),
                        "SVG filter became blank at frame {frame}: {key}"
                    );
                    if let Some(previous) = pixels_by_key.insert(key.clone(), image.pixels.clone())
                    {
                        assert_eq!(
                            previous, image.pixels,
                            "SVG filter pixels flickered at frame {frame}: {key}"
                        );
                    }
                }
            }
            drop(owner);
            crate::style::set_css_transition_virtual_timeline(false);
            event_loop.exit();
        }
        fn window_event(&mut self, _: &dyn ActiveEventLoop, _: WindowId, _: WindowEvent) {}
    }
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    builder.build().unwrap().run_app(Harness).unwrap();
}

#[cfg(feature = "performance")]
#[test]
#[ignore = "requires the complete native Gallery and Windows GPU presentation"]
fn real_gallery_fluent_hover_leave_scroll_hud_continuous_frames() {
    use winit::application::ApplicationHandler;
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, EventLoop};
    use winit::platform::windows::EventLoopBuilderExtWindows;
    use winit::window::WindowId;

    struct Harness;
    impl ApplicationHandler for Harness {
        fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
            let optical_edge = std::env::var_os("JADIX_OPTICAL_EDGE").is_some();
            let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(if optical_edge {
                    "../../.theme-evidence/optical-edge-continuous"
                } else {
                    "../../.theme-evidence/fluent-gallery-continuous"
                })
                .join(std::process::id().to_string());
            std::fs::create_dir_all(&directory).unwrap();
            let mut mismatched = 0;
            let mut misplaced = 0;
            let frames = std::env::var("JADIX_CONTINUOUS_FRAMES")
                .ok()
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(240);
            for full in [false, true] {
                crate::init();
                clear_app_runtime();
                reset_registered_anchors();
                crate::style::load_and_install_theme_overrides(
                    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("../../examples/gallery/theme.toml"),
                )
                .unwrap();
                crate::style::set_theme_preset(crate::style::ThemePreset::Fluent);
                crate::style::set_theme_mode(crate::style::ThemeMode::Dark);
                crate::style::set_theme_preference(crate::style::ThemePreference::Dark);
                let mut transition = crate::style::theme_transition_config();
                transition.enabled = false;
                crate::style::set_theme_transition_config(transition);
                crate::style::set_css_transition_virtual_timeline(true);
                crate::style::set_css_transition_timeline_ms(0.0);
                use_signal("gal_nav_section", String::new).set("theme".into());
                use_signal("gal-perf-visible", || true).set_silent(true);
                use_signal("gal_theme_preview_popover", || false).set_silent(false);
                for key in [
                    crate::core::scroll_viewport::scroll_offset_y_signal_key("scroll-theme"),
                    crate::core::scroll_viewport::scroll_target_y_signal_key("scroll-theme"),
                ] {
                    use_signal(&key, || 0.0_f32).set_silent(0.0);
                }
                crate::shared::install_performance_overlay();
                let (tx, rx) = std::sync::mpsc::channel();
                crate::app::ui_bridge::install_proxy(event_loop.create_proxy(), tx);
                let mut app = crate::app::lifecycle::create_app(
                    crate::app::WindowOptions::new()
                        .follow_theme(true)
                        .size(1000.0, 700.0),
                    crate::gallery_app::build_gallery_app,
                    event_loop.create_proxy(),
                    rx,
                );
                crate::app::with_ui_build_stack(|| app.can_create_surfaces(event_loop));
                let window = app.state.borrow().as_ref().unwrap().window.clone();
                let mut optical_surroundings = Vec::new();
                for frame in 0..frames {
                    crate::style::set_css_transition_timeline_ms(frame as f64 * 1000.0 / 60.0);
                    crate::style::utility::set_frame_dt(1.0 / 60.0);
                    if frame % 15 == 0 {
                        use_signal("gal-perf-text", String::new).set(format!(
                            "Present {} FPS\nCPU {}%\nGPU {}%\nRAM {} MiB{}",
                            frame % 3,
                            frame % 7,
                            frame % 5,
                            320 + frame % 17,
                            if frame % 30 == 0 {
                                "\nSampling 0.50s\nUI 3.1%\nResources ready"
                            } else {
                                ""
                            },
                        ));
                    }
                    drain_deferred_view_remounts();
                    let phase = frame % 90;
                    if !optical_edge && (frame == 180 || frame == 225) {
                        use_signal("gal_theme_preview_popover", || false).set(frame == 180);
                        drain_deferred_view_remounts();
                    }
                    if optical_edge || frame >= 90 {
                        let offset = if optical_edge {
                            480.0
                        } else if frame >= 180 {
                            480.0
                        } else if frame % 90 < 45 {
                            180.0
                        } else {
                            360.0
                        };
                        for key in [
                            crate::core::scroll_viewport::scroll_offset_y_signal_key(
                                "scroll-theme",
                            ),
                            crate::core::scroll_viewport::scroll_target_y_signal_key(
                                "scroll-theme",
                            ),
                        ] {
                            use_signal(&key, || 0.0_f32).set_silent(offset);
                        }
                    }
                    let target = if optical_edge {
                        "gal-theme-preview-popover"
                    } else {
                        [
                            "gal-nav-forms",
                            "gal-nav-data",
                            "gal-nav-theme",
                            "gal-theme-refraction",
                            "gal-perf-toggle",
                        ][(frame / 12) % 5]
                    };
                    let pointer = {
                        let state = app.state.borrow();
                        let state = state.as_ref().unwrap();
                        let bounds =
                            crate::core::tree::find_by_template_id(&state.root_element, target)
                                .unwrap_or_else(|| panic!("missing full Gallery control {target}"))
                                .viewport_layout_bounds();
                        if optical_edge {
                            let [left, top, width, height] = bounds;
                            let step = frame % 60;
                            let edge = if frame % 2 == 0 { -0.25 } else { 0.25 };
                            match (frame / 60) % 4 {
                                0 => [left + edge, top + height * 0.5],
                                1 => [left + width * 0.5, top + edge],
                                2 => [left + width - edge, top + height * 0.5],
                                _ if step < 30 => [left + width * 0.5, top + height - edge],
                                _ => [left + width + 10.0, top + height + 10.0],
                            }
                        } else if phase >= 75 {
                            [-1.0; 2]
                        } else if phase >= 65 {
                            [999.5, 350.0]
                        } else if frame % 12 < 3 {
                            [bounds[0] + 0.25, bounds[1] + 0.25]
                        } else {
                            [bounds[0] + bounds[2] * 0.5, bounds[1] + bounds[3] * 0.5]
                        }
                    };
                    app.with_primary_input(window.id(), |app| {
                        if pointer[0] < 0.0 {
                            app.handle_cursor_moved(
                                window.id(),
                                winit::dpi::PhysicalPosition::new(
                                    999.5 * window.scale_factor(),
                                    350.0 * window.scale_factor(),
                                ),
                                event_loop,
                            );
                            app.handle_cursor_left(window.id(), event_loop);
                        } else {
                            let scale = window.scale_factor();
                            app.handle_cursor_moved(
                                window.id(),
                                winit::dpi::PhysicalPosition::new(
                                    pointer[0] as f64 * scale,
                                    pointer[1] as f64 * scale,
                                ),
                                event_loop,
                            );
                        }
                        app.flush_pending_cursor_moved(window.id(), event_loop);
                        assert_eq!(app.cursor_inside_window, pointer[0] >= 0.0);
                    });
                    drain_deferred_view_remounts();
                    let mut state = app.state.borrow_mut();
                    let state = state.as_mut().unwrap();
                    if !optical_edge && (frame == 150 || frame == 165) {
                        let scale = window.scale_factor();
                        state.resize(winit::dpi::PhysicalSize::new(
                            ((if frame == 150 { 940.0 } else { 1000.0 }) * scale) as u32,
                            (700.0 * scale) as u32,
                        ));
                    }
                    if full {
                        state.mark_full_redraw();
                    }
                    crate::app::with_ui_build_stack(|| {
                        state.render(
                            pointer,
                            ScrollbarUpdateMode::ViewportsOnly,
                            None,
                            false,
                            false,
                            false,
                        )
                    })
                    .unwrap();
                    let panel = crate::core::tree::find_by_template_id(
                        &state.root_element,
                        "gal-perf-panel",
                    )
                    .expect("complete Gallery must include its real performance portal");
                    let bounds = panel.viewport_layout_bounds();
                    let (width, height) = state.logical_size();
                    if (width - bounds[0] - bounds[2] - 12.0).abs() > 1.0
                        || (height - bounds[1] - bounds[3] - 12.0).abs() > 1.0
                    {
                        misplaced += 1;
                        if misplaced < 5 {
                            eprintln!("HUD misplaced frame={frame} full={full} bounds={bounds:?} viewport={width}x{height}");
                        }
                    }
                    let image = super::toggle_motion_present_tests::read_frame(state);
                    let image =
                        ::image::RgbaImage::from_raw(image.width, image.height, image.pixels)
                            .unwrap();
                    if !optical_edge && full && matches!(frame, 220 | 239) {
                        let samples: Vec<_> = [[400.0, 600.0], [600.0, 600.0]]
                            .map(|point| {
                                *image.get_pixel(
                                    (point[0] * window.scale_factor()) as u32,
                                    (point[1] * window.scale_factor()) as u32,
                                )
                            })
                            .into_iter()
                            .collect();
                        if frame == 220 {
                            optical_surroundings = samples;
                        } else {
                            assert_eq!(
                                samples, optical_surroundings,
                                "opening optical popover must not recolor distant page pixels"
                            );
                        }
                    }
                    let path = directory.join(format!(
                        "{}-{frame:04}.png",
                        if full { "full" } else { "partial" }
                    ));
                    image.save(&path).unwrap();
                    if full {
                        let partial =
                            ::image::open(directory.join(format!("partial-{frame:04}.png")))
                                .unwrap()
                                .into_rgba8();
                        let changed = partial
                            .pixels()
                            .zip(image.pixels())
                            .filter(|(left, right)| {
                                left.0
                                    .iter()
                                    .zip(right.0.iter())
                                    .any(|(left, right)| left.abs_diff(*right) > 3)
                            })
                            .count();
                        if changed > 0 {
                            mismatched += 1;
                            if mismatched <= 8 {
                                eprintln!("Fluent Gallery mismatch frame={frame} pixels={changed}");
                            }
                        }
                    }
                }
                drop(app);
            }
            crate::style::set_css_transition_virtual_timeline(false);
            clear_app_runtime();
            eprintln!("Full Gallery continuous capture: frames={frames} mismatched={mismatched} misplaced={misplaced} evidence={}", directory.display());
            assert_eq!(
                misplaced, 0,
                "real Gallery HUD must remain viewport anchored"
            );
            assert_eq!(
                mismatched, 0,
                "every real Gallery partial frame must match a full repaint"
            );
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
fn real_gallery_motion_continuous_frames() {
    native_gallery(true);
}

#[test]
#[ignore = "requires a native Windows GPU surface"]
fn real_gallery_checkbox_continuous_frames() {
    native_gallery(false);
}

fn native_gallery(motion: bool) {
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
                            .with_title("Gallery continuous repaint regression")
                            .with_surface_size(winit::dpi::LogicalSize::new(1000.0, 640.0))
                            .with_visible(false),
                    )
                    .unwrap(),
            );
            for mode in [
                crate::style::ThemeMode::Dark,
                crate::style::ThemeMode::Light,
            ] {
                let directory = std::env::temp_dir()
                    .join("jadix-gallery-continuous")
                    .join(format!("{}-{}-{mode:?}", std::process::id(), self.0));
                std::fs::create_dir_all(&directory).unwrap();
                gallery_frames(window.clone(), self.0, mode, false, &directory);
                gallery_frames(window.clone(), self.0, mode, true, &directory);
            }
            event_loop.exit();
        }

        fn window_event(&mut self, _: &dyn ActiveEventLoop, _: WindowId, _: WindowEvent) {}
    }
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    builder.build().unwrap().run_app(Harness(motion)).unwrap();
}

fn matching_control<'a>(root: &'a Element, caption: &str) -> Option<&'a Element> {
    if root.on_click.is_some() && has_caption(root, caption) {
        return Some(root);
    }
    root.children
        .iter()
        .find_map(|child| matching_control(child, caption))
}

fn click_caption(state: &Rc<RefCell<Option<AppState>>>, caption: &str) {
    let callback = {
        let live = state.borrow();
        let showcase = crate::core::tree::find_by_template_id(
            &live.as_ref().unwrap().root_element,
            "gal-generator-showcase",
        )
        .unwrap();
        matching_control(showcase, caption)
            .unwrap_or_else(|| panic!("missing gallery action {caption}"))
            .on_click
            .clone()
            .unwrap()
    };
    callback.borrow_mut()();
}

fn has_caption(root: &Element, caption: &str) -> bool {
    root.text.as_deref() == Some(caption)
        || root
            .children
            .iter()
            .any(|child| has_caption(child, caption))
}

fn gallery_frames(
    window: Arc<dyn Window>,
    motion: bool,
    mode: crate::style::ThemeMode,
    full: bool,
    directory: &std::path::Path,
) {
    crate::init();
    crate::style::load_and_install_theme_overrides(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/gallery/theme.toml"),
    )
    .unwrap();
    clear_app_runtime();
    reset_registered_anchors();
    crate::style::set_theme_mode(mode);
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    crate::style::utility::set_frame_dt(0.0);
    let state = Rc::new(RefCell::new(Some(crate::utils::block_on::block_on(
        AppState::new_bootstrap(window, crate::app::WindowOptions::new()),
    ))));
    let secondaries = Rc::new(RefCell::new(std::collections::HashMap::new()));
    set_app_runtime(AppRuntime::new(
        Rc::downgrade(&state),
        Rc::downgrade(&secondaries),
    ));
    let section = if motion { "basics" } else { "theme" };
    use_signal("gal_theme_preview_checked", || true).set_silent(true);
    use_signal("gal_gen_stagger", || false).set_silent(false);
    let wave = crate::style::AnimatedNumber::tween(
        0.0,
        14.25 * crate::layout::dimension::css_root_font_size_px(),
        900.0,
        crate::style::TimingFunction::EaseInOut,
    )
    .repeat(crate::style::RepeatMode::PingPong);
    wave.pause();
    use_signal("gal_gen_wave", || wave.clone()).set_silent(wave.clone());
    let scroll_id = format!("scroll-{section}");
    use_signal("gal_nav_section", || section.to_string()).set_silent(section.to_string());
    for key in [
        crate::core::scroll_viewport::scroll_offset_y_signal_key(&scroll_id),
        crate::core::scroll_viewport::scroll_target_y_signal_key(&scroll_id),
    ] {
        use_signal(&key, || 0.0_f32).set_silent(0.0);
    }
    let (owner, ()) = create_root(|| {
        let live = state.clone();
        crate::core::reactivity::mark_next_effect_as_ui_root();
        create_effect(move || {
            live.borrow_mut().as_mut().unwrap().rebuild_ui(
                Element::new()
                    .w("100%")
                    .h("100%")
                    .child(crate::core::reactivity::isolate_reads(
                        crate::gallery_app::build_gallery_app,
                    )),
            );
        });
    });
    for _ in 0..4 {
        let mut live = state.borrow_mut();
        super::toggle_motion_present_tests::render(live.as_mut().unwrap());
    }
    let target = if motion {
        "gal_gen_tx:wave"
    } else {
        "gal-theme-preview-checkbox"
    };
    let (scroll, capture_left) = {
        let live = state.borrow();
        let element =
            crate::core::tree::find_by_template_id(&live.as_ref().unwrap().root_element, target)
                .unwrap();
        (
            (element.layout_bounds[1] - 140.0).max(0.0),
            ((element.layout_bounds[0] - 16.0).max(0.0)
                * live.as_ref().unwrap().config.width as f32
                / 1000.0) as u32,
        )
    };
    for key in [
        crate::core::scroll_viewport::scroll_offset_y_signal_key(&scroll_id),
        crate::core::scroll_viewport::scroll_target_y_signal_key(&scroll_id),
    ] {
        use_signal(&key, || 0.0_f32).set_silent(scroll);
    }
    for _ in 0..3 {
        let mut live = state.borrow_mut();
        super::toggle_motion_present_tests::render(live.as_mut().unwrap());
    }
    let initial_full = full_clear_frame_count();
    let mut mismatched_frames = 0;
    let mut missing_check_frames = 0;
    let mut worst = (0, 0);
    let mut wave_range = [f32::INFINITY, f32::NEG_INFINITY];
    let frames = std::env::var("JADIX_CONTINUOUS_FRAMES")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(480);
    for step in 0..frames {
        if motion {
            let caption = match step {
                0 | 300 => Some("play"),
                240 => Some("pause"),
                _ => None,
            };
            if let Some(caption) = caption {
                click_caption(&state, caption);
            }
            let caption = match step {
                0 | 240 => Some("Play"),
                120 | 360 => Some("Reset"),
                _ => None,
            };
            if let Some(caption) = caption {
                click_caption(&state, caption);
            }
        } else if step % 30 == 0 {
            let callback = {
                let live = state.borrow();
                let checkbox = crate::core::tree::find_by_template_id(
                    &live.as_ref().unwrap().root_element,
                    target,
                )
                .unwrap();
                checkbox.children[0].on_click.clone().unwrap()
            };
            callback.borrow_mut()();
        }
        crate::style::advance_css_transition_timeline_ms(1000.0 / 120.0);
        crate::style::tick_animations(1.0 / 120.0);
        crate::style::utility::set_frame_dt(1.0 / 120.0);
        drain_deferred_view_remounts();
        wave_range[0] = wave_range[0].min(wave.peek());
        wave_range[1] = wave_range[1].max(wave.peek());
        let (image, checked_control) = {
            let mut live = state.borrow_mut();
            let live = live.as_mut().unwrap();
            if full {
                live.mark_full_redraw();
            }
            let mouse = if !motion && step % 90 < 60 {
                let checkbox =
                    crate::core::tree::find_by_template_id(&live.root_element, target).unwrap();
                let control = &checkbox.children[0];
                [
                    control.layout_bounds[0] - control.paint_scroll_acc[0] + 8.0,
                    control.layout_bounds[1] - control.paint_scroll_acc[1] + 8.0,
                ]
            } else {
                [-1.0; 2]
            };
            live.render(
                mouse,
                ScrollbarUpdateMode::ViewportsOnly,
                None,
                false,
                false,
                false,
            )
            .unwrap();
            if motion && [119, 239, 359, 479].contains(&step) {
                let expected = if step == 119 || step == 359 {
                    1.0
                } else {
                    0.75 / 3.5
                };
                for index in 0..5 {
                    let bar = crate::core::tree::find_by_template_id(
                        &live.root_element,
                        &format!("gal-gen-stagger-{index}"),
                    )
                    .unwrap();
                    assert!(
                        (bar.style.scale[1] - expected).abs() < 1e-5,
                        "stagger must reach both endpoints: step={step} index={index}"
                    );
                }
            }
            let control = if motion {
                None
            } else {
                let checkbox =
                    crate::core::tree::find_by_template_id(&live.root_element, target).unwrap();
                let control = &checkbox.children[0];
                Some((
                    control.checkbox_sync.unwrap().peek(),
                    control.layout_bounds,
                    control.paint_scroll_acc,
                    live.config.width as f32 / 1000.0,
                ))
            };
            let image = super::toggle_motion_present_tests::read_frame(live);
            (
                ::image::RgbaImage::from_raw(image.width, image.height, image.pixels).unwrap(),
                control,
            )
        };
        let cropped = ::image::imageops::crop_imm(
            &image,
            capture_left,
            0,
            (image.width() - capture_left).min(600),
            image.height().min(660),
        )
        .to_image();
        let path = directory.join(format!("partial-{step:04}.png"));
        if let Some((true, bounds, scroll_acc, scale)) = checked_control {
            let foreground = crate::core::tree::parse_text_color("primary-foreground");
            let left = ((bounds[0] - scroll_acc[0] + 3.0) * scale) as u32;
            let top = ((bounds[1] - scroll_acc[1] + 3.0) * scale) as u32;
            let right = ((bounds[0] - scroll_acc[0] + bounds[2] - 3.0) * scale) as u32;
            let bottom = ((bounds[1] - scroll_acc[1] + bounds[3] - 3.0) * scale) as u32;
            let ink = (top..bottom.min(image.height()))
                .flat_map(|y| (left..right.min(image.width())).map(move |x| (x, y)))
                .filter(|(x, y)| {
                    image.get_pixel(*x, *y).0[..3]
                        .iter()
                        .zip(&foreground[..3])
                        .all(|(actual, expected)| actual.abs_diff(*expected) < 48)
                })
                .count();
            if ink == 0 {
                if missing_check_frames < 8 {
                    cropped
                        .save(directory.join(format!("missing-check-{full}-{step:04}.png")))
                        .unwrap();
                    eprintln!(
                        "missing-check frame={step} bounds={bounds:?} scroll={scroll_acc:?} rect={left},{top},{right},{bottom}"
                    );
                }
                missing_check_frames += 1;
            }
        }
        if full {
            let partial = ::image::open(&path).unwrap().into_rgba8();
            let differences = partial
                .pixels()
                .zip(cropped.pixels())
                .filter(|(actual, expected)| {
                    actual
                        .0
                        .iter()
                        .zip(expected.0.iter())
                        .any(|(actual, expected)| actual.abs_diff(*expected) > 3)
                })
                .count();
            if differences > 0 {
                if mismatched_frames < 8 {
                    cropped
                        .save(directory.join(format!("full-{step:04}.png")))
                        .unwrap();
                }
                mismatched_frames += 1;
                if differences > worst.1 {
                    worst = (step, differences);
                }
            }
        } else {
            cropped.save(path).unwrap();
        }
    }
    eprintln!(
        "continuous-gallery motion={motion} mode={mode:?} full={full} mismatched={mismatched_frames}/{frames} missing_check={missing_check_frames} full_frames={} worst={worst:?} captures={}",
        full_clear_frame_count() - initial_full,
        directory.display()
    );
    drop(owner);
    clear_app_runtime();
    crate::style::set_css_transition_virtual_timeline(false);
    if motion && frames >= 120 {
        assert!(
            wave_range[1] - wave_range[0] > 100.0,
            "wave must traverse its track: {wave_range:?}"
        );
    }
    assert_eq!(mismatched_frames, 0);
    assert_eq!(missing_check_frames, 0);
    assert_eq!(
        full_clear_frame_count() - initial_full,
        if full { frames as u64 } else { 0 }
    );
}
