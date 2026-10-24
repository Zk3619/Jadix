use super::*;
use std::{cell::Cell, rc::Rc};

#[test]
#[ignore = "requires a native Windows GPU surface"]
fn canvas_dirty_pixels_present_without_scrolling() {
    native_texture_frames(true);
}

#[test]
#[ignore = "requires a native Windows GPU surface"]
fn same_key_media_frames_present_without_geometry_changes() {
    native_texture_frames(false);
}

#[test]
#[ignore = "requires a native Windows GPU surface"]
fn rebuilt_hovered_button_presents_without_color_flash() {
    native_window(run_button_frames);
}

#[test]
#[ignore = "requires a native Windows GPU surface"]
fn anchored_close_button_fades_without_chrome_flash() {
    native_window(|window| {
        crate::core::reactivity::create_root(|| {
            use crate::app::portal_overlay::{OverlayKind, OverlayMode, ScrollFreeze};
            use crate::utils::floating_motion::{
                apply_floating_panel_motion, floating_portal_phase, use_floating_presence,
                FloatingMotionKind,
            };
            crate::app::portal_overlay::clear_overlay_stack();
            crate::style::set_theme_preset(crate::style::ThemePreset::Shadcn);
            crate::style::set_theme_mode(crate::style::ThemeMode::Dark);
            crate::style::set_css_transition_virtual_timeline(true);
            crate::style::set_css_transition_timeline_ms(0.0);
            let open = crate::core::reactivity::Signal::new(true);
            let presence = use_floating_presence("native-close", FloatingMotionKind::Fade);
            let build = || {
                let (phase, z) = floating_portal_phase(
                    "native-close",
                    open,
                    &presence,
                    OverlayKind::Popover,
                    OverlayMode::Stacked,
                    ScrollFreeze::None,
                )
                .unwrap();
                let mut panel = Element::new()
                    .w(200.0)
                    .h(72.0)
                    .p(8.0)
                    .flex_col()
                    .bg("popover")
                    .child(
                        Element::from(
                            crate::components::button::button("native-overlay-close")
                                .text("Close")
                                .on_click(move || open.set(false)),
                        )
                        .w(160.0)
                        .h(32.0),
                    );
                crate::utils::popup::mark_floating_at_point(
                    &mut panel,
                    "native-close",
                    "",
                    8.0,
                    8.0,
                    0.0,
                );
                apply_floating_panel_motion(
                    "native-close",
                    &mut panel,
                    FloatingMotionKind::Fade,
                    phase,
                    &presence,
                    open,
                    OverlayKind::Popover,
                    OverlayMode::Stacked,
                    None,
                );
                Element::new()
                    .template_id("native-overlay-island")
                    .child(crate::utils::popup::mount_anchored_overlay(panel, z.panel))
            };
            let mut state = crate::utils::block_on::block_on(AppState::new_bootstrap(
                window,
                crate::app::WindowOptions::new(),
            ));
            state.rebuild_ui(
                Element::new()
                    .w("100%")
                    .h("100%")
                    .bg("background")
                    .child(build()),
            );
            let mut hovered = None;
            let mut previous_pixel: Option<[u8; 4]> = None;
            for frame in 0..12 {
                crate::style::set_css_transition_timeline_ms(if frame < 4 {
                    frame as f64 * 200.0
                } else {
                    600.0 + (frame - 3) as f64 * 16.0
                });
                if frame == 4 {
                    let button = crate::app::pointer_events::find_by_focus_id(
                        &state.root_element,
                        "native-overlay-close",
                    )
                    .unwrap();
                    button.on_click.as_ref().unwrap().borrow_mut()();
                    assert!(!open.peek());
                    state
                        .reconcile_subtree("native-overlay-island", build())
                        .unwrap();
                }
                state
                    .render(
                        [30.0, 25.0],
                        ScrollbarUpdateMode::Full,
                        None,
                        false,
                        false,
                        false,
                    )
                    .unwrap();
                let button = crate::app::pointer_events::find_by_focus_id(
                    &state.root_element,
                    "native-overlay-close",
                )
                .unwrap();
                if frame == 3 {
                    hovered = Some(button.style.bg_color);
                } else if frame >= 4 {
                    assert_eq!(
                        Some(button.style.bg_color),
                        hovered,
                        "close chrome changed on frame {frame}"
                    );
                }
                let image = readback_texture(
                    &state.device,
                    &state.queue,
                    state.present_retain.color_texture_for_test().unwrap(),
                    state.config.width,
                    state.config.height,
                    state.device.create_command_encoder(&Default::default()),
                )
                .unwrap();
                let scale = state.ui_scale();
                let pixel = image
                    .pixel((22.0 * scale) as u32, (25.0 * scale) as u32)
                    .unwrap();
                if frame >= 4 {
                    let previous = previous_pixel.unwrap();
                    assert!(
                        pixel[..3]
                            .iter()
                            .zip(previous)
                            .all(|(current, previous)| *current <= previous.saturating_add(2)),
                        "exit brightened on frame {frame}: {previous:?} -> {pixel:?}"
                    );
                }
                previous_pixel = Some(pixel);
            }
            crate::style::set_css_transition_virtual_timeline(false);
            crate::app::portal_overlay::clear_overlay_stack();
        });
    });
}

fn native_texture_frames(canvas: bool) {
    native_window(move |window| run_texture_frames(window, canvas));
}

fn native_window(run: impl FnMut(Arc<dyn Window>) + 'static) {
    use winit::application::ApplicationHandler;
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, EventLoop};
    use winit::platform::windows::EventLoopBuilderExtWindows;
    use winit::window::{WindowAttributes, WindowId};

    struct Harness(Box<dyn FnMut(Arc<dyn Window>)>);
    impl ApplicationHandler for Harness {
        fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
            let window: Arc<dyn Window> = Arc::from(
                event_loop
                    .create_window(
                        WindowAttributes::default()
                            .with_title("Jadix dynamic texture regression")
                            .with_surface_size(winit::dpi::LogicalSize::new(240.0, 160.0))
                            .with_visible(false),
                    )
                    .unwrap(),
            );
            (self.0)(window);
            event_loop.exit();
        }
        fn window_event(&mut self, _: &dyn ActiveEventLoop, _: WindowId, _: WindowEvent) {}
    }
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    builder
        .build()
        .unwrap()
        .run_app(Harness(Box::new(run)))
        .unwrap();
}

fn run_button_frames(window: Arc<dyn Window>) {
    crate::core::reactivity::create_root(|| {
        crate::style::set_theme_preset(crate::style::ThemePreset::Shadcn);
        crate::style::set_theme_mode(crate::style::ThemeMode::Dark);
        crate::style::set_css_transition_virtual_timeline(true);
        let build = |height| {
            Element::new()
                .w("100%")
                .h("100%")
                .bg("background")
                .flex_col()
                .child(
                    Element::from(
                        crate::components::button::button("native-hover-rebuild")
                            .variant("outline")
                            .text("Toggle"),
                    )
                    .w(160.0)
                    .h(32.0),
                )
                .child(Element::new().w(200.0).h(height).bg("muted"))
        };
        let mut state = crate::utils::block_on::block_on(AppState::new_bootstrap(
            window,
            crate::app::WindowOptions::new(),
        ));
        state.rebuild_ui(build(40.0));
        let mut expected = None;
        for frame in 0..12 {
            crate::style::set_css_transition_timeline_ms(if frame < 4 {
                frame as f64 * 200.0
            } else {
                600.0 + (frame - 3) as f64 * 16.0
            });
            if frame >= 4 {
                state.rebuild_ui(build(if frame % 2 == 0 { 60.0 } else { 40.0 }));
            }
            state
                .render(
                    [30.0, 20.0],
                    ScrollbarUpdateMode::Full,
                    None,
                    false,
                    false,
                    false,
                )
                .unwrap();
            let image = readback_texture(
                &state.device,
                &state.queue,
                state.present_retain.color_texture_for_test().unwrap(),
                state.config.width,
                state.config.height,
                state.device.create_command_encoder(&Default::default()),
            )
            .unwrap();
            let scale = state.ui_scale();
            let pixel = image
                .pixel((8.0 * scale) as u32, (16.0 * scale) as u32)
                .unwrap();
            if frame == 3 {
                assert!(
                    state.root_element.children[0].style.bg_color[3] > 0.01,
                    "the reference button must actually be hovered"
                );
                expected = Some(pixel);
            }
            if frame >= 4 {
                assert_eq!(
                    Some(pixel),
                    expected,
                    "hover chrome flashed on rebuilt frame {frame}"
                );
            }
        }
        crate::style::set_css_transition_virtual_timeline(false);
    });
}

fn run_texture_frames(window: Arc<dyn Window>, canvas: bool) {
    crate::core::reactivity::create_root(|| {
        let color = Rc::new(Cell::new("#ff0000"));
        let sample = color.clone();
        let leaf: Element = if canvas {
            crate::primitives::canvas::canvas()
                .w(80.0)
                .h(60.0)
                .paint(move |ctx, _| {
                    ctx.clear(sample.get());
                    crate::core::tree::LayoutStatus::Clean
                })
                .into()
        } else {
            crate::queue_image_rgba8("native-media-regression", 1, 1, vec![255, 0, 0, 255]);
            Element::new()
                .w(80.0)
                .h(60.0)
                .image_texture("native-media-regression")
        };
        let paint = leaf
            .attrs
            .get(crate::primitives::canvas::ATTR_CANVAS_PAINT)
            .cloned();
        let root = Element::new().w("100%").h("100%").bg([1.0; 4]).child(leaf);
        let mut state = crate::utils::block_on::block_on(AppState::new_bootstrap(
            window,
            crate::app::WindowOptions::new(),
        ));
        state.rebuild_ui(root);
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
        let partial_before = crate::app::damage::partial_load_frame_count();
        let full_before = crate::app::damage::full_clear_frame_count();
        for (name, rgba) in [("#00ff00", [0, 255, 0, 255]), ("#0000ff", [0, 0, 255, 255])] {
            color.set(name);
            if let Some(paint) = &paint {
                crate::primitives::canvas::mark_canvas_dirty_hex(paint);
            } else {
                crate::queue_media_frame(
                    "native-media-regression",
                    crate::MediaFrame::rgba8(1, 1, &rgba),
                );
            }
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
            let image = readback_texture(
                &state.device,
                &state.queue,
                state.present_retain.color_texture_for_test().unwrap(),
                state.config.width,
                state.config.height,
                state.device.create_command_encoder(&Default::default()),
            )
            .unwrap();
            let scale = state.ui_scale();
            let pixel = image
                .pixel((30.0 * scale) as u32, (25.0 * scale) as u32)
                .unwrap();
            assert!(pixel.iter().zip(rgba).all(|(actual, expected)| actual.abs_diff(expected) <= 3),
                "updated texture must present in the same retained frame: canvas={canvas} expected={rgba:?} actual={pixel:?}");
        }
        assert!(
            crate::app::damage::partial_load_frame_count() > partial_before,
            "dynamic textures must not require whole-window repaint"
        );
        assert_eq!(
            crate::app::damage::full_clear_frame_count(),
            full_before,
            "dynamic textures must only damage their own paint slots"
        );
    });
}
