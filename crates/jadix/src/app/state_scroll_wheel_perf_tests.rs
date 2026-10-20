//! Wheel replay yields to the native event loop after every frame.
use super::*;
use crate::core::reactivity::{OwnerHandle, Signal};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::platform::windows::EventLoopBuilderExtWindows;
use winit::window::{WindowAttributes, WindowId};

pub(super) fn run() {
    struct Harness {
        app: Option<crate::app::App>,
        owner: Option<OwnerHandle>,
        sections: std::collections::VecDeque<String>,
        section: String,
        warm: usize,
        mount_ms: f32,
        scan: Option<Scan>,
        period: std::time::Duration,
        next_frame: Instant,
        complete: std::rc::Rc<std::cell::Cell<bool>>,
        worst_ms: std::rc::Rc<std::cell::Cell<f32>>,
    }
    impl ApplicationHandler for Harness {
        fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
            crate::init();
            let gallery =
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/gallery");
            crate::prelude::load_and_install_theme_overrides(&gallery.join("theme.toml")).unwrap();
            crate::prelude::watch_stylesheets([
                gallery.join("theme.toml"),
                gallery.join("styles.toml"),
            ]);
            let logo = super::super::super::pending_images::texture_key_for_src(
                crate::pages::layout::FILTER_DEMO_LOGO,
            );
            assert!(super::super::super::pending_images::queue_image_from_src(
                logo,
                gallery.join("assets/firefox-logo.svg").to_str().unwrap()
            ));
            let (rgba, w, h) = crate::shared::checker_rgba_texture();
            crate::queue_image_rgba8("demo-checker", w, h, rgba);
            let width: u32 = std::env::var("JADIX_PERF_WIDTH")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(3840);
            let window: Arc<dyn Window> = Arc::from(
                event_loop
                    .create_window(
                        WindowAttributes::default()
                            .with_title("Jadix native wheel performance")
                            .with_visible(std::env::var_os("JADIX_PERF_VISIBLE").is_some())
                            .with_surface_size(winit::dpi::PhysicalSize::new(
                                width,
                                width * 9 / 16,
                            )),
                    )
                    .unwrap(),
            );
            let live = std::rc::Rc::new(std::cell::RefCell::new(Some(
                crate::utils::block_on::block_on(AppState::new_bootstrap(
                    window,
                    crate::app::WindowOptions::new(),
                )),
            )));
            let secondaries = std::rc::Rc::new(std::cell::RefCell::new(HashMap::new()));
            crate::app::runtime::set_app_runtime(crate::app::runtime::AppRuntime::new(
                std::rc::Rc::downgrade(&live),
                std::rc::Rc::downgrade(&secondaries),
            ));
            {
                let guard = live.borrow();
                let state = guard.as_ref().unwrap();
                eprintln!(
                    "native adapter={:?} scale={} target={}x{} present={:?} event-loop=pumped period_ms={:.3}",
                    state.adapter.get_info(),
                    state.ui_scale(),
                    state.config.width,
                    state.config.height,
                    state.config.present_mode,
                    self.period.as_secs_f64() * 1000.0
                );
                assert_eq!(
                    (state.config.width, state.config.height),
                    (width, width * 9 / 16),
                    "scan target must match the requested physical resolution"
                );
                state.device.on_uncaptured_error(std::sync::Arc::new(|err| {
                    panic!("native performance GPU error: {err}")
                }));
            }
            let (_tx, rx) = std::sync::mpsc::channel();
            let mut app = crate::app::lifecycle::create_app(
                crate::app::WindowOptions::new(),
                || {
                    Element::new()
                        .w("100%")
                        .h("100%")
                        .child(crate::gallery_app::build_gallery_app())
                },
                event_loop.create_proxy(),
                rx,
            );
            app.state = live;
            app.secondaries = secondaries;
            self.app = Some(app);
        }

        fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
            if self.app.is_none() {
                return;
            }
            if Instant::now() < self.next_frame {
                event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame));
                return;
            }
            self.next_frame = Instant::now() + self.period;
            event_loop.set_control_flow(ControlFlow::Poll);
            let app = self.app.as_mut().unwrap();
            if self.section.is_empty() {
                let Some(section) = self.sections.pop_front() else {
                    if let Some(owner) = self.owner.take() {
                        owner.dispose();
                    }
                    crate::app::runtime::clear_app_runtime();
                    self.complete.set(true);
                    event_loop.exit();
                    return;
                };
                if let Some(owner) = self.owner.take() {
                    owner.dispose();
                }
                let mount_begin = Instant::now();
                crate::app::reactive::reset_registered_anchors();
                let rest = format!("scroll-{section}");
                use_signal(
                    &crate::core::scroll_viewport::scroll_offset_y_signal_key(&rest),
                    || 0.0_f32,
                )
                .set_silent(0.0);
                use_signal("gal_nav_section", || section.clone()).set_silent(section.clone());
                let live = app.state.clone();
                let (owner, ()) = create_root(|| {
                    crate::core::reactivity::mark_next_effect_as_ui_root();
                    create_effect(move || {
                        live.borrow_mut().as_mut().unwrap().rebuild_ui(
                            Element::new()
                                .w("100%")
                                .h("100%")
                                .child(crate::gallery_app::build_gallery_app()),
                        )
                    });
                });
                self.owner = Some(owner);
                self.section = section;
                self.warm = 3;
                self.mount_ms = mount_begin.elapsed().as_secs_f32() * 1000.0;
            }
            if self.warm > 0 {
                let warm_begin = Instant::now();
                crate::app::reactive::drain_deferred_view_remounts();
                app.state
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .render(
                        [-1.0; 2],
                        ScrollbarUpdateMode::ViewportsOnly,
                        None,
                        false,
                        false,
                        false,
                    )
                    .unwrap();
                self.warm -= 1;
                self.mount_ms += warm_begin.elapsed().as_secs_f32() * 1000.0;
                return;
            }
            if self.scan.is_none() {
                eprintln!(
                    "native-mount/{}: three_frames_ms={:.2}",
                    self.section, self.mount_ms
                );
                self.scan = Some(Scan::new(event_loop, app, &self.section));
                return;
            }
            if self.scan.as_mut().unwrap().step(event_loop, app) {
                self.worst_ms.set(
                    self.worst_ms
                        .get()
                        .max(*self.scan.as_ref().unwrap().times.last().unwrap()),
                );
                self.scan = None;
                self.section.clear();
            }
        }

        fn window_event(
            &mut self,
            _event_loop: &dyn ActiveEventLoop,
            _: WindowId,
            event: WindowEvent,
        ) {
            if matches!(event, WindowEvent::CloseRequested) {
                panic!("native performance window closed before scan completion");
            }
            // Native messages are dispatched between every replayed frame.
            // Redraw is measured by Scan::step; OS-requested extra frames do
            // not get a second animation tick or alter the pinned trajectory.
        }
    }
    let selected = std::env::var("JADIX_PERF_SECTION").ok();
    let sections: std::collections::VecDeque<String> = [
        "basics",
        "forms",
        "overlays",
        "data",
        "draw",
        "navigation",
        "layout",
        "text",
        "events",
        "theme",
        "about",
    ]
    .into_iter()
    .filter(|section| {
        selected
            .as_ref()
            .is_none_or(|s| s.split(',').any(|x| x == *section))
    })
    .map(str::to_string)
    .collect();
    assert!(!sections.is_empty(), "no matching performance section");
    let period = std::env::var("JADIX_PERF_FRAME_PERIOD_MS")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|v| v.is_finite() && *v > 0.0)
        .map(|ms| std::time::Duration::from_secs_f64(ms / 1000.0))
        .unwrap_or_default();
    let complete = std::rc::Rc::new(std::cell::Cell::new(false));
    let worst_ms = std::rc::Rc::new(std::cell::Cell::new(0.0_f32));
    let harness = Harness {
        app: None,
        owner: None,
        sections,
        section: String::new(),
        warm: 0,
        mount_ms: 0.0,
        scan: None,
        period,
        next_frame: Instant::now(),
        complete: complete.clone(),
        worst_ms: worst_ms.clone(),
    };
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    crate::app::with_ui_build_stack(|| builder.build().unwrap().run_app(harness).unwrap());
    assert!(
        complete.get(),
        "native event loop exited before completing the wheel scan"
    );
    crate::app::reactive::reset_registered_anchors();
    if let Ok(budget) = std::env::var("JADIX_PERF_BUDGET_MS") {
        let budget: f32 = budget.parse().unwrap();
        assert!(
            worst_ms.get() <= budget,
            "native-wheel/all worst={:.2} exceeds {budget}ms",
            worst_ms.get()
        );
    }
}

struct Scan {
    section: String,
    id: WindowId,
    scale: f32,
    max: f32,
    step: f32,
    leg: usize,
    frames: usize,
    offset: Signal<f32>,
    times: Vec<f32>,
    furthest: f32,
}

impl Scan {
    fn new(event_loop: &dyn ActiveEventLoop, app: &mut crate::app::App, section: &str) -> Self {
        let rest = format!("scroll-{section}");
        let (id, scale, viewport, max) = {
            let guard = app.state.borrow();
            let state = guard.as_ref().unwrap();
            let (_, viewport, max, _) = state
                .last_render_scroll_metrics
                .iter()
                .find(|(id, _, _, _)| id == &rest)
                .cloned()
                .expect("gallery viewport");
            (state.window.id(), state.ui_scale(), viewport, max[1])
        };
        app.cursor_pos = winit::dpi::PhysicalPosition::new(
            ((viewport[0] + 8.0) * scale) as f64,
            ((viewport[1] + viewport[3] * 0.5) * scale) as f64,
        );
        app.cursor_inside_window = true;
        app.pointer_window = Some(id);
        app.cursor_move_pending = true;
        app.handle_redraw_requested(id, event_loop);
        assert!(
            app.state
                .borrow()
                .as_ref()
                .unwrap()
                .last_render_scroll_metrics
                .iter()
                .any(|(id, _, _, _)| id == &rest),
            "redraw must retain viewport clips"
        );
        let step = std::env::var("JADIX_PERF_SCROLL_STEP")
            .ok()
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(24.0)
            .abs()
            .max(1.0);
        let leg = (max / step).ceil() as usize + 30;
        let frames = std::env::var("JADIX_PERF_WHEEL_FRAMES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(leg * 2 + 30);
        Self {
            section: section.into(),
            id,
            scale,
            max,
            step,
            leg,
            frames,
            offset: use_signal(
                &crate::core::scroll_viewport::scroll_offset_y_signal_key(&rest),
                || 0.0_f32,
            ),
            times: Vec::new(),
            furthest: 0.0,
        }
    }

    fn step(&mut self, event_loop: &dyn ActiveEventLoop, app: &mut crate::app::App) -> bool {
        let frame = self.times.len();
        app.last_frame_time = Instant::now() - std::time::Duration::from_millis(16);
        let before = {
            let mut guard = app.state.borrow_mut();
            let state = guard.as_mut().unwrap();
            state.render_prof_frames = 0;
            state.render_prof_last = Instant::now();
            [
                state.render_prof_pre_ms,
                state.render_prof_layout_collect_ms,
                state.render_prof_text_ms,
                state.render_prof_encode_ms,
                state.render_prof_acquire_ms,
                state.render_prof_submit_ms,
                state.render_prof_present_ms,
                state.render_prof_layout_ms,
                state.render_prof_layout_setup_ms,
                state.render_prof_layout_stamp_ms,
                state.render_prof_upload_ms,
                state.render_prof_layout_taffy_ms,
            ]
        };
        let path_builds_before = crate::primitives::path::gpu_op_build_count();
        let begin = Instant::now();
        if frame < self.leg * 2 {
            let dy = if frame < self.leg {
                -self.step
            } else {
                self.step
            };
            app.handle_mouse_wheel(
                self.id,
                winit::event::MouseScrollDelta::PixelDelta(winit::dpi::PhysicalPosition::new(
                    0.0,
                    (dy * self.scale) as f64,
                )),
            );
        }
        for (_, last_tick) in app.smooth_scroll_motion.values_mut() {
            *last_tick = app.last_frame_time;
        }
        app.handle_redraw_requested(self.id, event_loop);
        let ms = begin.elapsed().as_secs_f32() * 1000.0;
        self.times.push(ms);
        self.furthest = self.furthest.max(self.offset.peek());
        if frame == 0 || frame + 1 == self.frames || ms > 16.67 {
            let path_builds = crate::primitives::path::gpu_op_build_count() - path_builds_before;
            eprintln!("  wheel-path-builds={path_builds}");
            let guard = app.state.borrow();
            let state = guard.as_ref().unwrap();
            let a = state.native_render_sample;
            eprintln!(
                "wheel/{} frame={frame} offset={:.1} total={ms:.2} images={:.2} paths={:.2} shader={:.2} effects={:.2} glyphs={:.2} band_alloc={:.2} band_new={} bands={} strips={} pins={}",
                self.section,
                self.offset.peek(),
                a[4],
                a[8],
                a[9],
                a[0],
                a[1],
                a[10],
                a[11],
                a[7],
                state.scroll_strip_blit.len(),
                state.scroll_blit_pins.len()
            );
            eprintln!(
                "  wheel-stages pre={:.2} collect={:.2} prepare={:.2} encode={:.2} acquire={:.2} submit={:.2} present={:.2}",
                state.render_prof_pre_ms - before[0],
                state.render_prof_layout_collect_ms - before[1],
                state.render_prof_text_ms - before[2],
                state.render_prof_encode_ms - before[3],
                state.render_prof_acquire_ms - before[4],
                state.render_prof_submit_ms - before[5],
                state.render_prof_present_ms - before[6]
            );
            eprintln!(
                "  wheel-measure calls={} misses={}",
                state.layout_tree.text_measure_calls.get(),
                state.layout_tree.text_measure_misses.get()
            );
            eprintln!(
                "  wheel-layout total={:.2} setup={:.2} stamp={:.2} upload={:.2} taffy={:.2}",
                state.render_prof_layout_ms - before[7],
                state.render_prof_layout_setup_ms - before[8],
                state.render_prof_layout_stamp_ms - before[9],
                state.render_prof_upload_ms - before[10],
                state.render_prof_layout_taffy_ms - before[11]
            );
        }
        if self.times.len() < self.frames {
            return false;
        }
        if std::env::var_os("JADIX_PERF_WHEEL_FRAMES").is_none() {
            assert!(
                self.furthest >= self.max - self.step,
                "{} missed bottom: {}/{}",
                self.section,
                self.furthest,
                self.max
            );
            assert!(
                self.offset.peek() <= self.step,
                "{} did not return to top",
                self.section
            );
        }
        let over = self.times.iter().filter(|&&t| t > 16.67).count();
        self.times.sort_by(f32::total_cmp);
        let worst = *self.times.last().unwrap();
        eprintln!(
            "native-wheel/{}: frames={} median={:.2} worst={worst:.2} over={over} max_offset={:.1}",
            self.section,
            self.frames,
            self.times[self.times.len() / 2],
            self.max
        );
        true
    }
}
