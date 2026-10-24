#![cfg(feature = "gallery-tests")]

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
            let size = if std::env::var("JADIX_DAMAGE_CASE").as_deref() == Ok("nested-text") {
                (810.0, 793.0)
            } else {
                (1600.0, 900.0)
            };
            let window = event_loop
                .create_window(
                    WindowAttributes::default()
                        .with_surface_size(winit::dpi::LogicalSize::new(size.0, size.1))
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
    // Recollect the reference so a stale cached clip cannot corrupt both images.
    s.layout_tree.invalidate_collect_skip();
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

fn assert_sidebar_header_unchanged(s: &AppState, before: &RgbaImage, after: &RgbaImage) {
    let header = crate::core::tree::find_by_template_id(&s.root_element, "gal-workspace")
        .expect("Gallery workspace header")
        .layout_bounds;
    let scale = s.ui_scale();
    let right = ((header[0] + header[2]) * scale).floor() as u32;
    let top = (header[1] * scale).ceil() as u32;
    let bottom = ((header[1] + header[3]) * scale).floor() as u32;
    assert!(right > 0 && bottom > top, "header fixture must be visible");
    // Include the space left of the icon, where the Pi report places the dirt.
    let changed = (top..bottom)
        .flat_map(|y| (0..right).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            before
                .pixel(x, y)
                .unwrap()
                .iter()
                .zip(after.pixel(x, y).unwrap())
                .any(|(a, b)| a.abs_diff(b) > 3)
        })
        .count();
    assert_eq!(
        changed, 0,
        "scrolling must not alter the fixed Gallery header"
    );
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
    let section = std::env::var("JADIX_DAMAGE_SECTION").unwrap_or_else(|_| "basics".into());
    use_signal("gal_nav_section", || section.clone()).set_silent(section.clone());
    let rest = format!("scroll-{section}");
    let offset = use_signal(
        &crate::core::scroll_viewport::scroll_offset_y_signal_key(&rest),
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
    if section == "basics" && std::env::var("JADIX_DAMAGE_CASE").as_deref() != Ok("scroll") {
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
    if !matches!(
        std::env::var("JADIX_DAMAGE_CASE").as_deref(),
        Ok("layout" | "nested-text")
    ) {
        let rail =
            crate::style::use_ui_transition_cubic(&format!("scrollbar_spring_{rest}:y"), 0.0);
        rail.set(1.0);
        crate::style::tick_animations(0.2);
        let mut opt = state.borrow_mut();
        let s = opt.as_mut().unwrap();
        s.mark_full_redraw();
        let header_before = render(s, false);
        let copies_before = crate::app::damage::scroll_strip_blit_count();
        for (label, start, step, frames) in [
            ("near-top", 0.0, 1.0, 20),
            ("scroll-down", 20.0, 30.0, 80),
            ("scroll-up", 2420.0, -30.0, 80),
        ] {
            let mut partial = None;
            for frame in 1..=frames {
                // Exercise raw jump offsets too: snapping the fixture first hid
                // half-device-pixel copy drift at fractional display scales.
                let y = start + frame as f32 * step;
                offset.set_silent(y);
                s.update_scroll_offset_in_place(&rest, [0.0, y]);
                partial = Some(render(s, true));
                assert_sidebar_header_unchanged(s, &header_before, partial.as_ref().unwrap());
                blits += usize::from(!s.scroll_strip_blit.is_empty());
                if std::env::var_os("JADIX_DAMAGE_EVERY_FRAME").is_some() {
                    assert_eq!(
                        compare(
                            s,
                            partial.as_ref().unwrap(),
                            &format!("{label}-{frame}"),
                            true
                        ),
                        0
                    );
                }
            }
            if compare(s, &partial.unwrap(), label, true) > 0 {
                failures.push(label.to_string());
            }
        }
        let copies = crate::app::damage::scroll_strip_blit_count() - copies_before;
        eprintln!("Gallery regression: {blits} scroll frames, {copies} GPU strip copies");
        // Bundled SVG icons add texture pins: the repair-cost policy may
        // choose a full frame where the glyph fallback still copies. Require
        // a substantial majority in either feature configuration.
        if crate::app::damage::scroll_strip_blit_enabled() {
            assert!(blits > 0, "regression must exercise optimized strip copies");
            assert!(
                copies * 3 >= blits as u64 * 2,
                "most scrolling frames must actually retain and copy the overlap: {copies}"
            );
        } else {
            assert_eq!(copies, 0, "disabled strip copies must use viewport clears");
        }
        let settled = render(s, false);
        if compare(s, &settled, "scroll-settled", true) > 0 {
            failures.push("scroll-settled".to_string());
        }
    }
    if std::env::var("JADIX_DAMAGE_CASE").as_deref() == Ok("nested-text") {
        assert_eq!(section, "text");
        let mut opt = state.borrow_mut();
        let s = opt.as_mut().unwrap();
        let inner_id = "gal-flow-preview";
        let inner = s
            .find_viewport_element(inner_id)
            .expect("text flow viewport")
            .layout_bounds;
        let outer = s
            .find_viewport_element(&rest)
            .expect("page viewport")
            .layout_bounds;
        let start = (inner[1] - outer[1] - 40.0).max(0.0);
        eprintln!("nested text: inner={inner:?}, outer={outer:?}, start={start}");
        let inner_offset = use_signal(
            &crate::core::scroll_viewport::scroll_offset_y_signal_key(inner_id),
            || 0.0f32,
        );
        let copies_before = crate::app::damage::scroll_strip_blit_count();
        for (frame, (outer_delta, inner_y)) in [
            (0.0, 0.0),
            (1.0, 0.0),
            (8.0, 25.0),
            (16.0, 25.0),
            (45.0, 25.0),
            (60.0, 25.0),
            (120.0, 150.0),
            (175.0, 150.0),
            (200.0, 300.0),
            (180.0, 300.0),
            (130.0, 300.0),
            (10.0, 300.0),
            (0.0, 0.0),
        ]
        .into_iter()
        .enumerate()
        {
            let outer_y = start + outer_delta;
            offset.set_silent(outer_y);
            inner_offset.set_silent(inner_y);
            s.update_scroll_offset_in_place(&rest, [0.0, outer_y]);
            s.update_scroll_offset_in_place(inner_id, [0.0, inner_y]);
            let partial = render(s, true);
            blits += usize::from(!s.scroll_strip_blit.is_empty());
            let label = format!("nested-text-{frame}");
            if compare(s, &partial, &label, true) > 0 {
                failures.push(label);
            }
        }
        // Keep the retained surface through each entire sequence so a full
        // reference between steps cannot hide accumulating copy errors.
        for (label, outer_start, outer_step, inner_start, inner_step) in [
            ("nested-continuous-down", start, 5.0, 0.0, 0.0),
            ("nested-continuous-up", start + 200.0, -5.0, 0.0, 0.0),
            ("nested-continuous-inner", start, 0.0, 0.0, 7.5),
            ("nested-continuous-both", start, 5.0, 300.0, -7.5),
        ] {
            let mut partial = None;
            for frame in 0..=40 {
                let outer_y = outer_start + frame as f32 * outer_step;
                let inner_y = inner_start + frame as f32 * inner_step;
                offset.set_silent(outer_y);
                inner_offset.set_silent(inner_y);
                s.update_scroll_offset_in_place(&rest, [0.0, outer_y]);
                s.update_scroll_offset_in_place(inner_id, [0.0, inner_y]);
                partial = Some(render(s, true));
                blits += usize::from(!s.scroll_strip_blit.is_empty());
            }
            if compare(s, &partial.unwrap(), label, true) > 0 {
                failures.push(label.to_string());
            }
        }
        let copies = crate::app::damage::scroll_strip_blit_count() - copies_before;
        eprintln!("Gallery nested regression: {blits} optimized frames, {copies} GPU strip copies");
        if crate::app::damage::scroll_strip_blit_enabled() {
            assert!(
                copies > 0,
                "nested regression must execute GPU strip copies"
            );
        } else {
            assert_eq!(copies, 0, "disabled strip copies must use viewport clears");
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

#[test]
#[ignore = "native Windows GPU sidebar clipped-edge regression"]
fn sidebar_clipped_edge_keeps_color_after_hover() {
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
                        .with_surface_size(winit::dpi::LogicalSize::new(800.0, 400.0))
                        .with_visible(false),
                )
                .unwrap();
            run_sidebar_edge(Arc::from(window));
            event_loop.exit();
        }
        fn window_event(&mut self, _: &dyn ActiveEventLoop, _: WindowId, _: WindowEvent) {}
    }
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    builder.build().unwrap().run_app(Harness).unwrap();
}

fn run_sidebar_edge(window: Arc<dyn Window>) {
    crate::init();
    crate::app::reactive::reset_registered_anchors();
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    crate::style::set_global_time_for_test(Some(0.0));
    crate::style::utility::set_frame_dt(0.0);
    let theme =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/gallery/theme.toml");
    crate::style::load_and_install_theme_overrides(theme).unwrap();
    let state = Rc::new(RefCell::new(Some(crate::utils::block_on::block_on(
        AppState::new_bootstrap(window, crate::app::WindowOptions::new()),
    ))));
    let secondary = Rc::new(RefCell::new(HashMap::new()));
    crate::app::runtime::set_app_runtime(crate::app::runtime::AppRuntime::new(
        Rc::downgrade(&state),
        Rc::downgrade(&secondary),
    ));
    use_signal("gal_nav_section", String::new).set_silent("basics".into());
    let (owner, ()) = create_root(|| {
        let live = state.clone();
        crate::core::reactivity::mark_next_effect_as_ui_root();
        create_effect(move || {
            live.borrow_mut()
                .as_mut()
                .unwrap()
                .rebuild_ui(crate::gallery_app::build_gallery_app())
        });
    });
    for _ in 0..4 {
        crate::app::reactive::drain_deferred_view_remounts();
        render(state.borrow_mut().as_mut().unwrap(), false);
    }
    let mut failures = Vec::new();
    let mut clipped_ink = 0;
    let header_before = render(state.borrow_mut().as_mut().unwrap(), false);
    let copies_before = crate::app::damage::scroll_strip_blit_count();
    for offset in [0.0, 0.666667, 1.333333, 2.0, 4.0, 8.0, 16.0, 24.0, 32.0] {
        let offset = super::super::super::scroll_support::snap_scroll_offset_to_device_px(
            offset,
            state.borrow().as_ref().unwrap().ui_scale(),
        );
        let rest = {
            let mut guard = state.borrow_mut();
            let s = guard.as_mut().unwrap();
            // Resolve the actual content scroll ID; it belongs to the provider.
            let vp = crate::core::tree::find_in_tree(&s.root_element, |el| {
                el.attrs.get("data-slot").map(String::as_str) == Some("sidebar-content")
            })
            .unwrap();
            let fid = vp.focus_id.as_ref().unwrap();
            let rest = fid.strip_prefix("scroll:").unwrap().to_owned();
            use_signal(
                &crate::core::scroll_viewport::scroll_offset_y_signal_key(&rest),
                || 0.0f32,
            )
            .set_silent(offset);
            for y in [30.0, 60.0, 90.0, 60.0, 30.0, offset] {
                use_signal(
                    &crate::core::scroll_viewport::scroll_offset_y_signal_key(&rest),
                    || 0.0f32,
                )
                .set_silent(y);
                s.update_scroll_offset_in_place(&rest, [0.0, y]);
                let frame = render(s, true);
                assert_sidebar_header_unchanged(s, &header_before, &frame);
            }
            rest
        };
        let before = render(state.borrow_mut().as_mut().unwrap(), true);
        for target in [
            "gal-nav-events",
            "gal-nav-forms",
            "gal-nav-data",
            "scrollbar-show",
            "scrollbar-hide",
        ] {
            let pointer = {
                let guard = state.borrow();
                let s = guard.as_ref().unwrap();
                let pos = if target.starts_with("scrollbar-") {
                    let rail = crate::style::use_ui_transition_cubic(
                        &format!("scrollbar_spring_{rest}:y"),
                        0.0,
                    );
                    rail.set(if target == "scrollbar-show" { 1.0 } else { 0.0 });
                    [-1.0; 2]
                } else {
                    let b = crate::core::tree::find_by_template_id(&s.root_element, target)
                        .unwrap()
                        .layout_bounds;
                    [b[0] + 30.0, b[1] + b[3] / 2.0 - offset]
                };
                crate::app::tree_hit::apply_pointer_hover_reconcile(
                    &s.root_element,
                    pos[0],
                    pos[1],
                    true,
                );
                pos
            };
            crate::style::tick_animations(0.3);
            crate::app::reactive::drain_deferred_view_remounts();
            let mut guard = state.borrow_mut();
            let s = guard.as_mut().unwrap();
            s.render(
                pointer,
                ScrollbarUpdateMode::ViewportsOnly,
                None,
                false,
                false,
                false,
            )
            .unwrap();
            let partial = readback_texture(
                &s.device,
                &s.queue,
                s.present_retain.color_texture_for_test().unwrap(),
                s.config.width,
                s.config.height,
                s.device.create_command_encoder(&Default::default()),
            )
            .unwrap();
            assert_sidebar_header_unchanged(s, &header_before, &partial);
            let host = crate::core::tree::find_in_tree(&s.root_element, |el| {
                el.attrs.get("data-slot").map(String::as_str) == Some("sidebar-content-host")
            })
            .unwrap()
            .layout_bounds;
            let scale = s.ui_scale();
            let bottom = ((host[1] + host[3]) * scale).round() as u32;
            // Compare the clipped item's text/icon column. The trailing
            // scrollbar gutter intentionally changes when the rail appears.
            let label = s.text_nodes.iter().find(|n| n.text == "布局").unwrap();
            let right = ((label.x + label.width) * scale).floor() as u32;
            let dark = (bottom.saturating_sub(3)..bottom)
                .flat_map(|y| (20..right).map(move |x| (x, y)))
                .filter(|&(x, y)| before.pixel(x, y).unwrap()[0] < 180)
                .count();
            clipped_ink += dark;
            let diff = (bottom.saturating_sub(8)..bottom + 2)
                .flat_map(|y| (20..right).map(move |x| (x, y)))
                .filter(|&(x, y)| {
                    before
                        .pixel(x, y)
                        .unwrap()
                        .iter()
                        .zip(partial.pixel(x, y).unwrap())
                        .any(|(a, b)| a.abs_diff(b) > 3)
                })
                .count();
            eprintln!(
                "offset={offset} target={target} bottom={bottom} dark edge pixels={dark} edge changed pixels={diff}"
            );
            s.mark_full_redraw();
            let full = render(s, false);
            let full_diff = (bottom.saturating_sub(8)..bottom + 2)
                .flat_map(|y| (20..right).map(move |x| (x, y)))
                .filter(|&(x, y)| {
                    partial
                        .pixel(x, y)
                        .unwrap()
                        .iter()
                        .zip(full.pixel(x, y).unwrap())
                        .any(|(a, b)| a.abs_diff(b) > 3)
                })
                .count();
            if diff > 0 || full_diff > 0 {
                let changed: Vec<_> = (bottom.saturating_sub(8)..bottom + 2)
                    .flat_map(|y| (20..right).map(move |x| (x, y)))
                    .filter_map(|(x, y)| {
                        let a = before.pixel(x, y).unwrap();
                        let b = partial.pixel(x, y).unwrap();
                        a.iter()
                            .zip(b)
                            .any(|(a, b)| a.abs_diff(b) > 3)
                            .then_some((x, y, a, b))
                    })
                    .take(20)
                    .collect();
                eprintln!("edge differences: {changed:?}; partial/full={full_diff}");
                failures.push((offset, target, diff, full_diff));
            }
        }
    }
    crate::app::runtime::clear_app_runtime();
    owner.dispose();
    crate::app::reactive::reset_registered_anchors();
    crate::style::set_css_transition_virtual_timeline(false);
    crate::style::set_global_time_for_test(None);
    if crate::app::damage::scroll_strip_blit_enabled() {
        assert!(
            crate::app::damage::scroll_strip_blit_count() > copies_before,
            "regression must execute GPU scroll copies"
        );
    } else {
        assert_eq!(crate::app::damage::scroll_strip_blit_count(), copies_before);
    }
    assert!(
        clipped_ink > 50,
        "fixture must expose actual clipped text/icon ink at the bottom edge"
    );
    assert!(
        failures.is_empty(),
        "Clipped sidebar edge changed on unrelated hover: {failures:?}"
    );
}
