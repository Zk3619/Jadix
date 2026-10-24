use super::*;
use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
use crate::app::runtime::{AppRuntime, clear_app_runtime, set_app_runtime};
use crate::core::reactivity::{create_effect, create_root, use_signal};
use std::{cell::RefCell, rc::Rc};

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
