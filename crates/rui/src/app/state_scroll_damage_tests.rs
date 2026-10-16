//! Compare real Gallery incremental frames with a fresh full composite.
use super::*;
use crate::core::reactivity::{create_effect, create_root, use_signal};
use crate::renderer::offscreen::RgbaImage;
use std::{cell::RefCell, rc::Rc};

#[test]
#[ignore = "native Windows GPU differential regression"]
fn gallery_layout_and_scroll_damage_matches_full() {
    use winit::application::ApplicationHandler;
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, EventLoop};
    use winit::platform::windows::EventLoopBuilderExtWindows;
    use winit::window::{WindowAttributes, WindowId};
    struct Harness;
    impl ApplicationHandler for Harness {
        fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
            let window = event_loop
                .create_window(
                    WindowAttributes::default()
                        .with_surface_size(winit::dpi::LogicalSize::new(1600.0, 900.0))
                        .with_visible(false),
                )
                .unwrap();
            run(Arc::from(window));
            event_loop.exit();
        }
        fn window_event(&mut self, _: &dyn ActiveEventLoop, _: WindowId, _: WindowEvent) {}
    }
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    builder.build().unwrap().run_app(Harness).unwrap();
}

fn render(s: &mut AppState, scroll: bool) -> RgbaImage {
    s.render(
        [-1.0; 2],
        ScrollbarUpdateMode::ViewportsOnly,
        None,
        scroll,
        false,
        scroll,
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

fn compare(s: &mut AppState, partial: &RgbaImage, label: &str, scrolling: bool) -> usize {
    s.mark_full_redraw();
    // Compare at the same text shaping mode: scrolling deliberately quantizes
    // layout bounds to reduce cache churn.
    let full = render(s, scrolling);
    let diffs: Vec<_> = partial
        .pixels
        .chunks_exact(4)
        .zip(full.pixels.chunks_exact(4))
        .enumerate()
        .filter(|(_, (a, b))| {
            let delta = a.iter().zip(*b).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
            // One threshold for retained interiors, exposed strips and chrome.
            delta > 6
        })
        .map(|(i, _)| (i as u32 % full.width, i as u32 / full.width))
        .collect();
    if !diffs.is_empty() {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../.tmp/render-regressions");
        std::fs::create_dir_all(&dir).unwrap();
        for (name, image) in [("partial", partial), ("full", &full)] {
            let mut pixels = image.pixels.clone();
            if matches!(
                s.config.format,
                wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
            ) {
                for p in pixels.chunks_exact_mut(4) {
                    p.swap(0, 2);
                }
            }
            ::image::save_buffer(
                dir.join(format!("{label}-{name}.png")),
                &pixels,
                image.width,
                image.height,
                ::image::ColorType::Rgba8,
            )
            .unwrap();
        }
        let min = diffs
            .iter()
            .fold((u32::MAX, u32::MAX), |a, b| (a.0.min(b.0), a.1.min(b.1)));
        let max = diffs
            .iter()
            .fold((0, 0), |a, b| (a.0.max(b.0), a.1.max(b.1)));
        eprintln!(
            "{label}: {} differing pixels, bounds={min:?}..{max:?}",
            diffs.len()
        );
    }
    diffs.len()
}

fn run(window: Arc<dyn Window>) {
    crate::init();
    crate::app::reactive::reset_registered_anchors();
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    crate::style::set_global_time_for_test(Some(0.0));
    crate::style::utility::set_frame_dt(0.0);
    let gallery =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/gallery");
    crate::prelude::load_and_install_theme_overrides(&gallery.join("theme.toml")).unwrap();
    crate::prelude::watch_stylesheets([gallery.join("theme.toml"), gallery.join("styles.toml")]);
    let state = Rc::new(RefCell::new(Some(crate::utils::block_on::block_on(
        AppState::new_bootstrap(window, crate::app::WindowOptions::new()),
    ))));
    // Empty stacking bands carry geometry but must not allocate text atlases.
    {
        let mut guard = state.borrow_mut();
        let s = guard.as_mut().unwrap();
        let band = |key, text| crate::layout::layout_tree::OverlayBandRange {
            instance: (0, 0),
            textured: (0, 0),
            text,
            chrome: (0, 0),
            paint_ops: (0, 0),
            backdrop_blur: 0.0,
            underlay_opacity: 1.0,
            stamp_key: key,
        };
        let mut bands = [band(1, (0, 0)), band(2, (0, 0)), band(3, (0, 0))];
        assert_eq!(
            s.ensure_page_stack_band_text_renderers(&bands),
            0,
            "empty stacking bands must not create text resources"
        );
        bands[2].text = (0, 1);
        assert_eq!(
            s.ensure_page_stack_band_text_renderers(&bands),
            1,
            "first text in an existing empty band must allocate exactly once"
        );
        assert_eq!(s.ensure_page_stack_band_text_renderers(&bands), 0);
        bands.swap(1, 2);
        assert_eq!(
            s.ensure_page_stack_band_text_renderers(&bands),
            0,
            "reordering must retain the host's text resources"
        );
        s.page_stack_band_text_keys.clear();
        s.page_stack_band_text_renderers.clear();
    }
    let secondaries = Rc::new(RefCell::new(HashMap::new()));
    crate::app::runtime::set_app_runtime(crate::app::runtime::AppRuntime::new(
        Rc::downgrade(&state),
        Rc::downgrade(&secondaries),
    ));
    let count = use_signal("gallery_btn_count", || 0);
    count.set_silent(0);
    use_signal("gal_nav_section", || "basics".to_string()).set_silent("basics".to_string());
    let offset = use_signal(
        &crate::core::scroll_viewport::scroll_offset_y_signal_key("scroll-basics"),
        || 0.0f32,
    );
    offset.set_silent(0.0);
    let (owner, ()) = create_root(|| {
        let live = state.clone();
        crate::core::reactivity::mark_next_effect_as_ui_root();
        create_effect(move || {
            live.borrow_mut().as_mut().unwrap().rebuild_ui(
                Element::new()
                    .w("100%")
                    .h("100%")
                    .child(crate::gallery_app::build_gallery_app()),
            );
        });
    });
    for _ in 0..4 {
        crate::app::reactive::drain_deferred_view_remounts();
        render(state.borrow_mut().as_mut().unwrap(), false);
    }
    let mut failures = Vec::new();
    if std::env::var("RUI_DAMAGE_CASE").as_deref() != Ok("scroll") {
        for i in 1..=10 {
            count.set(i);
            crate::app::reactive::drain_deferred_view_remounts();
            let mut opt = state.borrow_mut();
            let s = opt.as_mut().unwrap();
            let partial = render(s, false);
            let label = format!("count-{i}");
            if compare(s, &partial, &label, false) > 0 {
                failures.push(label);
            }
        }
    }
    let mut blits = 0;
    if std::env::var("RUI_DAMAGE_CASE").as_deref() != Ok("layout") {
        let rail = crate::style::use_ui_transition_cubic("scrollbar_spring_scroll-basics:y", 0.0);
        rail.set(1.0);
        crate::style::tick_animations(0.2);
        let mut opt = state.borrow_mut();
        let s = opt.as_mut().unwrap();
        s.mark_full_redraw();
        render(s, false);
        let copies_before = crate::app::damage::scroll_strip_blit_count();
        for (label, start, step, frames) in [
            ("near-top", 0.0, 1.0, 20),
            ("scroll-down", 20.0, 30.0, 80),
            ("scroll-up", 2420.0, -30.0, 80),
        ] {
            let mut partial = None;
            for frame in 1..=frames {
                let y = super::super::super::scroll_support::snap_scroll_offset_to_device_px(
                    start + frame as f32 * step,
                    s.ui_scale(),
                );
                offset.set_silent(y);
                s.update_scroll_offset_in_place("scroll-basics", [0.0, y]);
                partial = Some(render(s, true));
                blits += usize::from(!s.scroll_strip_blit.is_empty());
            }
            if compare(s, &partial.unwrap(), label, true) > 0 {
                failures.push(label.to_string());
            }
        }
        assert!(blits > 0, "regression must exercise optimized strip copies");
        let copies = crate::app::damage::scroll_strip_blit_count() - copies_before;
        eprintln!("Gallery regression: {blits} scroll frames, {copies} GPU strip copies");
        // Bundled SVG icons add texture pins: the repair-cost policy may
        // choose a full frame where the glyph fallback still copies. Require
        // a substantial majority in either feature configuration.
        assert!(
            copies * 3 >= blits as u64 * 2,
            "most scrolling frames must actually retain and copy the overlap: {copies}"
        );
        let settled = render(s, false);
        if compare(s, &settled, "scroll-settled", true) > 0 {
            failures.push("scroll-settled".to_string());
        }
    }
    crate::app::runtime::clear_app_runtime();
    owner.dispose();
    crate::app::reactive::reset_registered_anchors();
    crate::style::set_css_transition_virtual_timeline(false);
    crate::style::set_global_time_for_test(None);
    assert!(
        failures.is_empty(),
        "incremental Gallery differs from full: {failures:?}; optimized scroll frames={blits}"
    );
}
