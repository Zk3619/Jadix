//! Exercise the production render path, including reactive sync and GPU work.
use super::*;
use crate::core::reactivity::{create_effect, create_root};

#[path = "state_scroll_wheel_perf_tests.rs"]
mod wheel_event_loop;

#[test]
#[ignore = "native Windows GPU performance scan; run explicitly with --ignored --nocapture"]
fn gallery_native_scroll_segment_cost() {
    struct TextPerfLog;
    impl log::Log for TextPerfLog {
        fn enabled(&self, metadata: &log::Metadata) -> bool {
            (metadata.target().contains("prepare_text") || metadata.target().contains("app::state"))
                && metadata.level() <= log::Level::Warn
        }
        fn log(&self, record: &log::Record) {
            if self.enabled(record.metadata()) {
                eprintln!("text-perf: {}", record.args());
            }
        }
        fn flush(&self) {}
    }
    if std::env::var_os("RUI_PERF_TEXT_DETAIL").is_some() {
        static LOGGER: TextPerfLog = TextPerfLog;
        let _ = log::set_logger(&LOGGER);
        log::set_max_level(log::LevelFilter::Warn);
    }
    if std::env::var_os("RUI_PERF_WHEEL").is_some() {
        wheel_event_loop::run();
        return;
    }
    use winit::application::ApplicationHandler;
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, EventLoop};
    use winit::platform::windows::EventLoopBuilderExtWindows;
    use winit::window::{WindowAttributes, WindowId};

    struct Harness;
    impl ApplicationHandler for Harness {
        fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
            crate::init();
            let gallery_root =
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/gallery");
            crate::prelude::load_and_install_theme_overrides(&gallery_root.join("theme.toml"))
                .unwrap();
            crate::prelude::watch_stylesheets([
                gallery_root.join("theme.toml"),
                gallery_root.join("styles.toml"),
            ]);
            // Gallery source is compiled into rui for this harness, so its
            // CARGO_MANIFEST_DIR image constant points at rui/assets. Seed the
            // same source key from the real Gallery fixture before it mounts.
            let logo_key = super::super::pending_images::texture_key_for_src(
                crate::pages::layout::FILTER_DEMO_LOGO,
            );
            assert!(super::super::pending_images::queue_image_from_src(
                logo_key,
                gallery_root
                    .join("assets/firefox-logo.svg")
                    .to_str()
                    .unwrap(),
            ));
            let (rgba, w, h) = crate::shared::checker_rgba_texture();
            crate::queue_image_rgba8("demo-checker", w, h, rgba);
            let sizes = if std::env::var_os("RUI_PERF_DENSE").is_some() {
                vec![(1920, 1080), (3840, 2160)]
            } else {
                vec![(800, 600), (1920, 1080)]
            };
            for size in sizes {
                if let Ok(width) = std::env::var("RUI_PERF_WIDTH") {
                    if width.parse::<u32>().unwrap() != size.0 {
                        continue;
                    }
                }
                let window: Arc<dyn Window> = Arc::from(
                    event_loop
                        .create_window(
                            WindowAttributes::default()
                                .with_visible(std::env::var_os("RUI_PERF_VISIBLE").is_some())
                                .with_surface_size(winit::dpi::PhysicalSize::new(size.0, size.1)),
                        )
                        .unwrap(),
                );
                let live_state = std::rc::Rc::new(std::cell::RefCell::new(Some(
                    crate::utils::block_on::block_on(AppState::new_bootstrap(
                        window,
                        crate::app::WindowOptions::new(),
                    )),
                )));
                let secondaries = std::rc::Rc::new(std::cell::RefCell::new(HashMap::new()));
                crate::app::runtime::set_app_runtime(crate::app::runtime::AppRuntime::new(
                    std::rc::Rc::downgrade(&live_state),
                    std::rc::Rc::downgrade(&secondaries),
                ));
                eprintln!(
                    "native adapter={:?} scale={}",
                    live_state.borrow().as_ref().unwrap().adapter.get_info(),
                    live_state.borrow().as_ref().unwrap().ui_scale()
                );
                live_state
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .device
                    .on_uncaptured_error(std::sync::Arc::new(|err| {
                        panic!("native performance GPU error: {err}")
                    }));
                if let Ok(path) = std::env::var("RUI_PERF_SHADER") {
                    let mut guard = live_state.borrow_mut();
                    let state = guard.as_mut().unwrap();
                    assert!(state.pipeline.rebuild_wgsl(
                        &state.device,
                        &state.config,
                        &std::fs::read_to_string(path).unwrap()
                    ));
                }
                for section in [
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
                ] {
                    if let Ok(selected) = std::env::var("RUI_PERF_SECTION") {
                        if !selected.split(',').any(|s| s == section) {
                            continue;
                        }
                    }
                    crate::app::reactive::reset_registered_anchors();
                    let rest = format!("scroll-{section}");
                    let offset = use_signal(
                        &crate::core::scroll_viewport::scroll_offset_y_signal_key(&rest),
                        || 0.0_f32,
                    );
                    offset.set_silent(0.0);
                    let live = live_state.clone();
                    let shell = std::env::var_os("RUI_PERF_FULL_SHELL").is_some();
                    if shell {
                        use_signal("gal_nav_section", || section.to_string())
                            .set_silent(section.to_string());
                    }
                    let (_owner, _) = create_root(|| {
                        crate::core::reactivity::mark_next_effect_as_ui_root();
                        create_effect(move || {
                            use crate::gallery_app::*;
                            let page = if shell {
                                build_gallery_app()
                            } else {
                                match section {
                                    "forms" => {
                                        GallerySectionForms(GallerySectionFormsProps::default())
                                    }
                                    "overlays" => GallerySectionOverlays(
                                        GallerySectionOverlaysProps::default(),
                                    ),
                                    "data" => {
                                        GallerySectionData(GallerySectionDataProps::default())
                                    }
                                    "draw" => {
                                        GallerySectionDraw(GallerySectionDrawProps::default())
                                    }
                                    "navigation" => GallerySectionNavigation(
                                        GallerySectionNavigationProps::default(),
                                    ),
                                    "layout" => {
                                        GallerySectionLayout(GallerySectionLayoutProps::default())
                                    }
                                    "text" => {
                                        GallerySectionText(GallerySectionTextProps::default())
                                    }
                                    "events" => {
                                        GallerySectionEvents(GallerySectionEventsProps::default())
                                    }
                                    "theme" => crate::pages::GallerySectionTheme(
                                        crate::pages::GallerySectionThemeProps::default(),
                                    ),
                                    "about" => {
                                        GallerySectionAbout(GallerySectionAboutProps::default())
                                    }
                                    _ => GallerySectionBasics(GallerySectionBasicsProps::default()),
                                }
                            };
                            live.borrow_mut()
                                .as_mut()
                                .unwrap()
                                .rebuild_ui(Element::new().w("100%").h("100%").child(page));
                        });
                    });
                    for _ in 0..3 {
                        crate::app::reactive::drain_deferred_view_remounts();
                        live_state
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
                    }
                    let mut state_guard = live_state.borrow_mut();
                    let state = state_guard.as_mut().unwrap();
                    let (_, viewport, max_offset, content) = state
                        .last_render_scroll_metrics
                        .iter()
                        .find(|(id, _, _, _)| id == &rest)
                        .cloned()
                        .expect("real page viewport");
                    let max = max_offset[1];
                    assert!(
                        max > 0.0,
                        "{section} must exercise scrollable content: {content:?} / {viewport:?}"
                    );
                    if std::env::var_os("RUI_PERF_SCROLL_SYNC").is_some() {
                        let mut offsets = HashMap::new();
                        crate::core::tree::walk_elements(&state.root_element, |el| {
                            if let Some(id) = el.focus_id.as_deref().and_then(|id| {
                                id.strip_prefix(
                                    crate::core::scroll_viewport::SCROLL_VIEWPORT_FOCUS_PREFIX,
                                )
                            }) {
                                offsets.insert(id.to_string(), el.scroll_offset);
                            }
                        });
                        // Replay the old per-redraw work on this real Gallery tree,
                        // then the changed page viewport only. No GPU work in this timing.
                        let begin = Instant::now();
                        for _ in 0..100 {
                            for (id, offset) in &offsets {
                                state.update_scroll_offset_in_place(id, *offset);
                            }
                        }
                        let all_ms = begin.elapsed().as_secs_f64() * 10.0;
                        let page_offset = offsets[&rest];
                        let begin = Instant::now();
                        for _ in 0..100 {
                            state.update_scroll_offset_in_place(&rest, page_offset);
                        }
                        let changed_ms = begin.elapsed().as_secs_f64() * 10.0;
                        eprintln!(
                            "scroll-sync/{section} areas={} all={all_ms:.3}ms changed={changed_ms:.3}ms",
                            offsets.len()
                        );
                    }
                    let continuous = std::env::var_os("RUI_PERF_CONTINUOUS").is_some();
                    let positions: Vec<f32> = if continuous {
                        vec![0.0]
                    } else if std::env::var_os("RUI_PERF_DENSE").is_some() {
                        (0..=(max / 240.0).ceil() as usize)
                            .map(|step| (step as f32 * 240.0).min(max))
                            .collect()
                    } else {
                        [0.0, 0.25, 0.5, 0.75, 1.0].map(|f| max * f).to_vec()
                    };
                    let scroll_step: f32 = std::env::var("RUI_PERF_SCROLL_STEP")
                        .ok()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(1.0);
                    for y in positions {
                        if let Ok(selected) = std::env::var("RUI_PERF_Y") {
                            let targets: Vec<f32> = selected
                                .split(',')
                                .filter_map(|value| value.trim().parse().ok())
                                .collect();
                            if !targets.iter().any(|target| (y - *target).abs() < 0.5) {
                                continue;
                            }
                        }
                        let mut times = Vec::new();
                        let mut first_ms = 0.0;
                        let leg_frames = (max / scroll_step.abs().max(1.0)).ceil() as usize;
                        let frames = if continuous {
                            leg_frames * 2 + 1
                        } else {
                            std::env::var("RUI_PERF_FRAMES")
                                .ok()
                                .and_then(|s| s.parse::<usize>().ok())
                                .unwrap_or(8)
                                .max(2)
                        };
                        for frame in 0..frames {
                            let frame_y =
                                super::super::scroll_support::snap_scroll_offset_to_device_px(
                                    if continuous {
                                        let step = frame.min(leg_frames * 2 - frame);
                                        (step as f32 * scroll_step.abs()).min(max)
                                    } else {
                                        (y + frame as f32 * scroll_step).clamp(0.0, max)
                                    },
                                    state.ui_scale(),
                                );
                            offset.set_silent(frame_y);
                            state.update_scroll_offset_in_place(&rest, [0.0, frame_y]);
                            state.scroll_layer_clear = true;
                            // Counters are reset periodically by production profiling;
                            // reset here so the sampled stages always describe this frame.
                            state.render_prof_frames = 0;
                            state.render_prof_last = Instant::now();
                            let before = [
                                state.render_prof_draw_ms,
                                state.render_prof_layout_collect_ms,
                                state.render_prof_text_ms,
                                state.render_prof_encode_ms,
                                state.render_prof_upload_ms,
                                state.render_prof_present_ms,
                                state.render_prof_pre_ms,
                                state.render_prof_layout_ms,
                            ];
                            let extra_before = [
                                state.render_prof_acquire_ms,
                                state.render_prof_submit_ms,
                                state.render_prof_layout_taffy_ms,
                                state.render_prof_layout_setup_ms,
                                state.render_prof_layout_stamp_ms,
                            ];
                            let t = Instant::now();
                            state
                                .render(
                                    [-1.0; 2],
                                    ScrollbarUpdateMode::ViewportsOnly,
                                    None,
                                    true,
                                    true,
                                    true,
                                )
                                .unwrap();
                            let render_ms = t.elapsed().as_secs_f32() * 1000.0;
                            let _ = state.device.poll(wgpu::PollType::wait_indefinitely());
                            let ms = t.elapsed().as_secs_f32() * 1000.0;
                            if frame == 0 {
                                first_ms = ms;
                            }
                            if frame > 0 {
                                times.push(ms);
                            }
                            if frame == 0
                                || frame + 1 == frames
                                || (ms > 16.67
                                    && std::env::var_os("RUI_PERF_FRAME_DETAIL").is_some())
                            {
                                eprintln!(
                                    "  sample/{section} y={y:.0} frame={frame} offset={frame_y:.1} total={ms:.2}"
                                );
                                eprintln!(
                                    "  cpu={render_ms:.2} wait={:.2} acquire={:.2} submit={:.2} taffy={:.2} setup={:.2} stamp={:.2} strips={} pins={}",
                                    ms - render_ms,
                                    state.render_prof_acquire_ms - extra_before[0],
                                    state.render_prof_submit_ms - extra_before[1],
                                    state.render_prof_layout_taffy_ms - extra_before[2],
                                    state.render_prof_layout_setup_ms - extra_before[3],
                                    state.render_prof_layout_stamp_ms - extra_before[4],
                                    state.scroll_strip_blit.len(),
                                    state.scroll_blit_pins.len()
                                );
                                eprintln!(
                                    "  effects={:.2} glyphs={:.2} captures={} fx_dirty={}",
                                    state.native_render_sample[0],
                                    state.native_render_sample[1],
                                    state.native_render_sample[2],
                                    state.native_render_sample[3]
                                );
                                eprintln!(
                                    "  images={:.2} paths={:.2} uploads={} bands={}",
                                    state.native_render_sample[4],
                                    state.native_render_sample[5],
                                    state.native_render_sample[6],
                                    state.native_render_sample[7]
                                );
                                if std::env::var_os("RUI_PERF_REPAIR_DETAIL").is_some() {
                                    eprintln!(
                                        "  repairs={:?}",
                                        super::super::draw_cull::scroll_repair_rects_physical(
                                            &state.scroll_strip_blit,
                                            &state.scroll_blit_pins,
                                            state.ui_scale()
                                        )
                                        .map(|c| c.strips().collect::<Vec<_>>())
                                    );
                                }
                                eprintln!(
                                    "  path_gpu={:.2} shader_gpu={:.2}",
                                    state.native_render_sample[8], state.native_render_sample[9]
                                );
                                eprintln!(
                                    "  stages frame={frame} draw={:.2} collect={:.2} text={:.2} encode={:.2} upload={:.2} present={:.2} pre={:.2} layout={:.2}",
                                    state.render_prof_draw_ms - before[0],
                                    state.render_prof_layout_collect_ms - before[1],
                                    state.render_prof_text_ms - before[2],
                                    state.render_prof_encode_ms - before[3],
                                    state.render_prof_upload_ms - before[4],
                                    state.render_prof_present_ms - before[5],
                                    state.render_prof_pre_ms - before[6],
                                    state.render_prof_layout_ms - before[7]
                                );
                            }
                        }
                        times.sort_by(f32::total_cmp);
                        eprintln!(
                            "native/{section} {}x{} y={y:.0}/{max:.0}: frame={:.2}ms first={first_ms:.2}ms worst={:.2}ms warm_worst={:.2}ms pending_dirty={}",
                            size.0,
                            size.1,
                            times[times.len() / 2],
                            times.last().unwrap().max(first_ms),
                            times.last().unwrap(),
                            state.pending_layout_dirty.len()
                        );
                        if let Ok(budget) = std::env::var("RUI_PERF_BUDGET_MS") {
                            let budget: f32 = budget.parse().unwrap();
                            assert!(
                                *times.last().unwrap() <= budget,
                                "{section} {size:?} y={y}: warm worst {}ms exceeds {budget}ms",
                                times.last().unwrap()
                            );
                        }
                    }
                }
                crate::app::runtime::clear_app_runtime();
            }
            crate::app::reactive::reset_registered_anchors();
            event_loop.exit();
        }
        fn window_event(&mut self, _: &dyn ActiveEventLoop, _: WindowId, _: WindowEvent) {}
    }
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    crate::app::with_ui_build_stack(|| builder.build().unwrap().run_app(Harness).unwrap());
}
