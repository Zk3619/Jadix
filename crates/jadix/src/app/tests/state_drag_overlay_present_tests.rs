use super::*;
use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
use crate::app::runtime::{clear_app_runtime, set_app_runtime, AppRuntime};
use crate::core::reactivity::{create_effect, create_root, use_signal};
use crate::utils::drag::{self, BeginDrag};
use std::{cell::RefCell, rc::Rc};

#[test]
#[ignore = "requires a native Windows GPU surface"]
fn gallery_handle_overlay_continuous_frames_match_full_repaint() {
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
                            .with_title("Gallery drag overlay continuous regression")
                            .with_surface_size(winit::dpi::LogicalSize::new(1000.0, 640.0))
                            .with_visible(false),
                    )
                    .unwrap(),
            );
            for debug in [false, true] {
                let directory = std::env::temp_dir()
                    .join("jadix-drag-continuous")
                    .join(format!("{}-{debug}", std::process::id()));
                std::fs::create_dir_all(&directory).unwrap();
                run_frames(window.clone(), debug, false, &directory);
                run_frames(window.clone(), debug, true, &directory);
            }
            event_loop.exit();
        }
        fn window_event(&mut self, _: &dyn ActiveEventLoop, _: WindowId, _: WindowEvent) {}
    }
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    builder.build().unwrap().run_app(Harness).unwrap();
}

fn run_frames(window: Arc<dyn Window>, debug: bool, full: bool, directory: &std::path::Path) {
    crate::init();
    crate::style::load_and_install_theme_overrides(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/gallery/theme.toml"),
    )
    .unwrap();
    drag::cancel();
    clear_app_runtime();
    reset_registered_anchors();
    crate::style::set_theme_mode(crate::style::ThemeMode::Dark);
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    crate::style::utility::set_frame_dt(0.0);
    use_signal("gal_nav_section", || "overlays".to_string()).set_silent("overlays".to_string());
    use_signal("gal_drag_handle_inbox", || false).set_silent(false);
    use_signal("gal_drag_debug", || false).set_silent(debug);
    for key in [
        crate::core::scroll_viewport::scroll_offset_y_signal_key("scroll-overlays"),
        crate::core::scroll_viewport::scroll_target_y_signal_key("scroll-overlays"),
    ] {
        use_signal(&key, || 0.0_f32).set_silent(0.0);
    }
    drag::set_debug_drag_hits(debug);
    let state = Rc::new(RefCell::new(Some(crate::utils::block_on::block_on(
        AppState::new_bootstrap(window, crate::app::WindowOptions::new()),
    ))));
    let secondaries = Rc::new(RefCell::new(std::collections::HashMap::new()));
    set_app_runtime(AppRuntime::new(
        Rc::downgrade(&state),
        Rc::downgrade(&secondaries),
    ));
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
            )
        });
    });
    for _ in 0..4 {
        super::toggle_motion_present_tests::render(state.borrow_mut().as_mut().unwrap());
    }
    let scroll = {
        let live = state.borrow();
        let source = crate::core::tree::find_by_attr(
            &live.as_ref().unwrap().root_element,
            drag::ATTR_DRAG_DRAGGABLE,
            "handled",
        )
        .unwrap();
        source.layout_bounds[1] - 340.0
    };
    for key in [
        crate::core::scroll_viewport::scroll_offset_y_signal_key("scroll-overlays"),
        crate::core::scroll_viewport::scroll_target_y_signal_key("scroll-overlays"),
    ] {
        use_signal(&key, || 0.0_f32).set_silent(scroll);
    }
    for _ in 0..4 {
        super::toggle_motion_present_tests::render(state.borrow_mut().as_mut().unwrap());
    }
    let source = drag::collect_hits(&state.borrow().as_ref().unwrap().root_element)
        .into_iter()
        .find(|hit| hit.id == "handled")
        .unwrap();
    let start = [source.rect.x + 8.0, source.rect.y + 8.0];
    drag::begin(BeginDrag {
        scope_id: source.scope_id.clone(),
        draggable_id: source.id.clone(),
        pointer: start,
        grab_offset: [8.0, 8.0],
        start_bounds: source.rect.to_bounds(),
        from_list: source.list_id.clone(),
        from_index: source.index,
        item_type: source.item_type.clone(),
        group: source.group.clone(),
        capture_id: String::new(),
    });
    let frames = std::env::var("JADIX_DRAG_FRAMES")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(300);
    let initial_full = full_clear_frame_count();
    let mut failures = 0;
    let mut worst = 0;
    for frame in 0..frames {
        let motion_frame = frame.min(239);
        let phase = (motion_frame % 120) as f32 / 119.0;
        let direction = if motion_frame / 120 % 2 == 0 {
            phase
        } else {
            1.0 - phase
        };
        let pointer = [
            start[0] + 12.0 + direction * 180.0,
            start[1] + 36.0 - direction * 136.0,
        ];
        let hits = drag::collect_hits(&state.borrow().as_ref().unwrap().root_element);
        if frame == 270 {
            drag::end(&hits);
        } else if frame < 270 {
            drag::tick(pointer[0], pointer[1], &hits, [1000.0, 640.0]);
        }
        crate::style::advance_css_transition_timeline_ms(1000.0 / 120.0);
        crate::style::tick_animations(1.0 / 120.0);
        crate::style::utility::set_frame_dt(1.0 / 120.0);
        drain_deferred_view_remounts();
        let image = {
            let mut live = state.borrow_mut();
            let live = live.as_mut().unwrap();
            if full {
                live.mark_full_redraw();
            }
            live.render(
                pointer,
                ScrollbarUpdateMode::ViewportsOnly,
                None,
                false,
                false,
                false,
            )
            .unwrap();
            super::toggle_motion_present_tests::read_frame(live)
        };
        assert_eq!(
            drag::active_draggable_id().as_deref(),
            if frame < 270 { Some("handled") } else { None }
        );
        if frame < 270 && !full {
            assert_eq!(
                full_clear_frame_count() - initial_full,
                0,
                "drag must keep partial redraw at frame {frame}"
            );
        }
        if debug {
            for row in 0..80 {
                for column in 450..image.width - 40 {
                    let offset = ((row * image.width + column) * 4) as usize;
                    let pixel = &image.pixels[offset..offset + 3];
                    assert!(!(pixel[2] > pixel[1].saturating_add(20) && pixel[1] > pixel[0].saturating_add(20)),
                        "debug slot escaped into title: frame={frame} pixel=({column},{row}) color={pixel:?}");
                }
            }
        }
        let path = directory.join(format!("partial-{frame:04}.png"));
        if full {
            let partial = ::image::open(&path).unwrap().to_rgba8();
            let differences = image
                .pixels
                .chunks_exact(4)
                .zip(partial.pixels())
                .filter(|(reference, actual)| {
                    reference[..3]
                        .iter()
                        .zip(&actual.0[..3])
                        .any(|(left, right)| left.abs_diff(*right) > 3)
                })
                .count();
            worst = worst.max(differences);
            if differences > 0 {
                if failures < 4 {
                    ::image::save_buffer(
                        directory.join(format!("reference-{frame:04}.png")),
                        &image.pixels,
                        image.width,
                        image.height,
                        ::image::ColorType::Rgba8,
                    )
                    .unwrap();
                }
                failures += 1;
            }
        } else {
            ::image::save_buffer(
                path,
                &image.pixels,
                image.width,
                image.height,
                ::image::ColorType::Rgba8,
            )
            .unwrap();
        }
    }
    eprintln!("drag-continuous debug={debug} full={full} failures={failures}/{frames} worst={worst} full_frames={} captures={}",
        full_clear_frame_count()-initial_full, directory.display());
    drag::cancel();
    drop(owner);
    clear_app_runtime();
    drag::set_debug_drag_hits(false);
    crate::style::set_css_transition_virtual_timeline(false);
    assert_eq!(failures, 0);
}
