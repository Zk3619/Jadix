#![cfg(feature = "components")]
//! Replay actual modal enter/exit frames and compare cached captures with fresh ones.
use super::*;
use crate::core::reactivity::{create_effect, create_root, use_signal};
use std::{cell::RefCell, rc::Rc};

#[test]
#[ignore = "requires a native Windows GPU surface"]
fn modal_animation_reuses_background_and_matches_fresh_capture() {
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
                            .with_surface_size(winit::dpi::LogicalSize::new(960.0, 640.0))
                            .with_visible(false),
                    )
                    .unwrap(),
            );
            run(window);
            event_loop.exit();
        }
        fn window_event(&mut self, _: &dyn ActiveEventLoop, _: WindowId, _: WindowEvent) {}
    }
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    builder.build().unwrap().run_app(Harness).unwrap();
}

fn render(s: &mut AppState, ms: f64) {
    crate::style::set_css_transition_timeline_ms(ms);
    crate::style::set_global_time_for_test(Some((ms / 1000.0) as f32));
    crate::style::utility::set_frame_dt(1.0 / 60.0);
    crate::app::reactive::drain_deferred_view_remounts();
    s.render(
        [-1.0; 2],
        ScrollbarUpdateMode::ViewportsOnly,
        None,
        false,
        false,
        false,
    )
    .unwrap();
}

fn read(s: &AppState) -> crate::renderer::offscreen::RgbaImage {
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

fn compare(s: &mut AppState, ms: f64) {
    let cached = read(s);
    let mut before_pose = Vec::new();
    crate::core::tree::walk_elements(&s.root_element, |el| {
        if el.template_id.as_deref().is_some_and(|id| id.ends_with("__modal_panel")) {
            before_pose.push((el.template_id.clone(), el.style.opacity, el.style.translate_pct, el.layout_bounds));
        }
    });
    s.modal_source_fp = None;
    s.mark_full_redraw();
    // Modal keyframes advance by frame_dt, independent of the CSS test clock.
    // Replay the same pose, rather than comparing two consecutive animation frames.
    crate::style::utility::set_frame_dt(0.0);
    s.render(
        [-1.0; 2],
        ScrollbarUpdateMode::ViewportsOnly,
        None,
        false,
        false,
        false,
    )
    .unwrap();
    let fresh = read(s);
    let changed = cached
        .pixels
        .chunks_exact(4)
        .zip(fresh.pixels.chunks_exact(4))
        .filter(|(a, b)| a.iter().zip(*b).any(|(a, b)| a.abs_diff(*b) > 3))
        .count();
    if changed > 0 {
        let folder = std::env::temp_dir();
        for (name, image) in [("jadix-modal-cached.png", &cached), ("jadix-modal-fresh.png", &fresh)] {
            image::save_buffer(folder.join(name), &image.pixels, image.width, image.height, image::ColorType::Rgba8).unwrap();
        }
        eprintln!("modal mismatch partial={}, source={:?}, blur={:?}", s.last_frame_partial, s.modal_source_fp, s.modal_blur_radius);
        eprintln!("before pose={before_pose:?}");
        crate::core::tree::walk_elements(&s.root_element, |el| {
            if el.template_id.as_deref().is_some_and(|id| id.ends_with("__modal_panel")) {
                eprintln!("after pose={:?} {:?} {:?}", el.template_id, el.style.opacity, el.style.translate_pct);
            }
        });
    }
    assert_eq!(
        changed, 0,
        "cached modal differs from fresh capture at {ms} ms"
    );
}
fn run(window: Arc<dyn Window>) {
    crate::init();
    crate::app::reactive::reset_registered_anchors();
    crate::app::portal_overlay::clear_overlay_stack();
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    crate::style::set_global_time_for_test(Some(0.0));
    let state = Rc::new(RefCell::new(Some(crate::utils::block_on::block_on(
        AppState::new_bootstrap(window, crate::app::WindowOptions::new()),
    ))));
    let secondary = Rc::new(RefCell::new(HashMap::new()));
    crate::app::runtime::set_app_runtime(crate::app::runtime::AppRuntime::new(
        Rc::downgrade(&state),
        Rc::downgrade(&secondary),
    ));
    let (owner, (open, kind, caption)) = create_root(|| {
        let open = use_signal("modal-perf-open", || false);
        let kind = use_signal("modal-perf-kind", || 0);
        let caption = use_signal("modal-perf-caption", || "Background A".to_string());
        let live = state.clone();
        crate::core::reactivity::mark_next_effect_as_ui_root();
        create_effect(move || {
            let body = || {
                Element::new()
                    .w("100%")
                    .h(180.0)
                    .child(Element::new().text("Modal body 中文文字"))
            };
            let modal = match kind.get() {
                0 => crate::components::dialog::dialog_scope("perf-dialog", Some(open), || {
                    crate::components::dialog::dialog_content(Default::default(), body)
                }),
                1 => crate::components::sheet::sheet_scope("perf-sheet", Some(open), || {
                    crate::components::sheet::sheet_content(Default::default(), body)
                }),
                _ => crate::components::drawer::drawer_scope("perf-drawer", Some(open), || {
                    crate::components::drawer::drawer_content(Default::default(), body)
                }),
            };
            let mut page = Element::new()
                .w("100%")
                .h("100%")
                .bg("#2563eb")
                .child(Element::new().text(caption.get()));
            page = page.child(
                Element::new()
                    .relative()
                    .w(320.0)
                    .h(80.0)
                    .overflow("hidden")
                    .child(Element::new().text("Local frost stays below modal"))
                    .child(
                        Element::new()
                            .absolute()
                            .inset(0.0)
                            .bg([1.0, 1.0, 1.0, 0.3])
                            .backdrop_blur(8.0),
                    ),
            );
            for i in 0..24 {
                page = page.child(Element::new().h(20.0).text(format!("Background row {i}")));
            }
            live.borrow_mut()
                .as_mut()
                .unwrap()
                .rebuild_ui(Element::new().w("100%").h("100%").child(page).child(modal));
        });
        (open, kind, caption)
    });
    let mut base = 0.0;
    for k in 0..3 {
        kind.set(k);
        render(state.borrow_mut().as_mut().unwrap(), base);
        open.set(true);
        render(state.borrow_mut().as_mut().unwrap(), base);
        render(state.borrow_mut().as_mut().unwrap(), base + 16.0);
        let work = state
            .borrow()
            .as_ref()
            .unwrap()
            .modal_backdrop
            .as_ref()
            .unwrap()
            .work_size();
        let start = Instant::now();
        for frame in 2..=40 {
            let mut guard = state.borrow_mut();
            let s = guard.as_mut().unwrap();
            let before_blurs = s.modal_blur_count;
            let before_captures = s.modal_capture_count;
            let time = base + frame as f64 * 16.0;
            render(s, time);
            assert_eq!(
                s.modal_backdrop.as_ref().unwrap().work_size(),
                work,
                "fade must not reallocate blur work targets"
            );
            assert!(work.0 <= s.config.width / 2 && work.1 <= s.config.height / 2);
            assert_eq!(s.modal_blur_count, before_blurs, "enter blur changed at frame {frame}, kind {k}");
            assert_eq!(s.modal_capture_count, before_captures, "enter recaptured background at frame {frame}, kind {k}");
            compare(s, time);
        }
        let mut guard = state.borrow_mut();
        let s = guard.as_mut().unwrap();
        eprintln!(
            "modal kind={k} continuous enter frames={:?}",
            start.elapsed()
        );
        compare(s, base + 640.0);
        drop(guard);
        caption.set(format!("Background changed {k}"));
        let before = state.borrow().as_ref().unwrap().modal_capture_count;
        render(state.borrow_mut().as_mut().unwrap(), base + 656.0);
        assert!(
            state.borrow().as_ref().unwrap().modal_capture_count > before,
            "page edit must refresh capture"
        );
        for frame in 1..=36 {
            render(
                state.borrow_mut().as_mut().unwrap(),
                base + 656.0 + frame as f64 * 16.0,
            );
        }
        let blurred = state.borrow().as_ref().unwrap().modal_blur_count;
        render(state.borrow_mut().as_mut().unwrap(), base + 1248.0);
        assert_eq!(
            state.borrow().as_ref().unwrap().modal_blur_count,
            blurred,
            "settled modal must reuse the completed blur"
        );
        open.set(false);
        render(state.borrow_mut().as_mut().unwrap(), base + 1264.0);
        for frame in 1..=40 {
            let mut guard = state.borrow_mut();
            let s = guard.as_mut().unwrap();
            let time = base + 1264.0 + frame as f64 * 16.0;
            render(s, time);
            compare(s, time);
        }
        // Finish presence wall-clock gate for Sheet/Drawer, then establish a closed page.
        std::thread::sleep(std::time::Duration::from_millis(500));
        render(state.borrow_mut().as_mut().unwrap(), base + 1920.0);
        crate::utils::modal_motion::drain_pending_modal_unmounts();
        base += 2000.0;
    }
    crate::app::runtime::clear_app_runtime();
    owner.dispose();
    crate::app::reactive::reset_registered_anchors();
    crate::app::portal_overlay::clear_overlay_stack();
    crate::style::set_css_transition_virtual_timeline(false);
    crate::style::set_global_time_for_test(None);
}
