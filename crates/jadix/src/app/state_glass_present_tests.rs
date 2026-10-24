use super::*;
use crate::renderer::offscreen::RgbaImage;
use std::{cell::RefCell, rc::Rc};

#[test]
fn liquid_indicator_survives_reactive_tree_reconciliation() {
    let previous = crate::style::get_theme();
    crate::style::set_theme_preset(crate::style::ThemePreset::LiquidGlass);
    let build = || {
        let mut group = Element::new()
            .template_id("flow-reconcile")
            .w(240.0)
            .h(32.0);
        group.set_attr("data-slot", "tabs-list");
        for index in 0..3 {
            let mut item = Element::new().w(80.0).h(32.0);
            item.set_attr("data-slot", "tabs-trigger");
            item.set_attr("data-state", if index == 0 { "active" } else { "inactive" });
            group.children.push(item);
        }
        group
    };
    let mut old = build();
    let mut layout = crate::layout::LayoutTree::new();
    layout.build_and_layout(&mut old, 320.0, 120.0, [-1.0; 2]);
    let hits = std::collections::HashSet::from([&old.children[0] as *const Element]);
    crate::style::liquid_flow::sync(&mut old, &hits);
    crate::style::liquid_flow::measure(&mut old, &layout.taffy);
    crate::style::tick_animations(0.016);
    let sample = old.liquid_flow.as_ref().unwrap().sample();
    let mut rebuilt = build();
    AppState::reconcile_elements_for_test(&mut rebuilt, &mut old);
    assert_eq!(rebuilt.liquid_flow.as_ref().unwrap().sample(), sample);
    assert!(rebuilt.liquid_flow.as_ref().unwrap().is_active());
    crate::style::set_theme(previous);
}

#[cfg(feature = "svg")]
#[test]
#[ignore = "requires a native Windows GPU surface; run explicitly with --ignored --nocapture"]
fn gallery_theme_and_motion_first_frames_keep_visible_stationary_labels() {
    with_native_window_options(
        run_first_text_frames,
        crate::app::WindowOptions::new().size(1280.0, 1600.0),
    );
}

#[cfg(feature = "svg")]
fn run_first_text_frames(window: Arc<dyn Window>) {
    let previous = crate::style::get_theme();
    let overrides = crate::style::theme_overrides();
    crate::style::clear_theme_overrides();
    let mut state = crate::utils::block_on::block_on(AppState::new_bootstrap(
        window,
        crate::app::WindowOptions::default(),
    ));
    for preset in [
        crate::style::ThemePreset::Fluent,
        crate::style::ThemePreset::LiquidGlass,
    ] {
        for motion in [false, true] {
            crate::style::set_theme_preset(preset);
            crate::app::reactive::reset_registered_anchors();
            let (owner, tree) = crate::core::reactivity::create_root(|| {
                Element::new()
                    .w("100%")
                    .h("100%")
                    .bg("background")
                    .child(if motion {
                        crate::pages::motion::build_layout_motion_showcase()
                    } else {
                        crate::pages::theme::ThemeStyleShowcase(Default::default())
                    })
            });
            state.rebuild_ui(tree);
            crate::style::utility::set_frame_dt(0.0);
            let first = capture(&mut state, [-1.0; 2], false);
            let needles: &[&str] = if motion {
                &["Card 1", "Card 2", "Card 3"]
            } else {
                &["Fluent", "Liquid Glass"]
            };
            let labels: Vec<_> = needles
                .iter()
                .map(|needle| {
                    state
                        .text_nodes
                        .iter()
                        .find(|text| text.text.contains(needle))
                        .unwrap_or_else(|| panic!("missing first-frame label {needle}"))
                        .clone()
                })
                .collect();
            let reference = capture(&mut state, [-1.0; 2], true);
            let scale = state.window.scale_factor() as f32;
            let bgra = matches!(
                state.config.format,
                wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
            );
            for label in labels {
                let current = state
                    .text_nodes
                    .iter()
                    .find(|text| text.text == label.text)
                    .unwrap();
                assert!(
                    (label.x - current.x).abs() < 0.1 && (label.y - current.y).abs() < 0.1,
                    "first-frame geometry shifted: {}",
                    label.text
                );
                let ink = label.style.color_resolved();
                let mut visible = 0;
                let mut changed = 0;
                for row in (label.y * scale).max(0.0) as u32
                    ..((label.y + label.height) * scale)
                        .ceil()
                        .min(first.height as f32) as u32
                {
                    for column in (label.x * scale).max(0.0) as u32
                        ..((label.x + label.width) * scale)
                            .ceil()
                            .min(first.width as f32) as u32
                    {
                        let offset = ((row * first.width + column) * 4) as usize;
                        let pixel = &first.pixels[offset..offset + 4];
                        let channel = |index: usize| if bgra { 2 - index } else { index };
                        visible += usize::from(
                            (0..3).all(|index| pixel[channel(index)].abs_diff(ink[index]) < 36),
                        );
                        changed += usize::from(
                            pixel
                                .iter()
                                .zip(&reference.pixels[offset..offset + 4])
                                .any(|(left, right)| left.abs_diff(*right) > 2),
                        );
                    }
                }
                assert!(
                    visible > 8,
                    "first-frame label has no visible glyphs: {}",
                    label.text
                );
                assert_eq!(
                    changed, 0,
                    "first-frame label differs from full composite: {}",
                    label.text
                );
            }
            snapshot(
                &first,
                state.config.format,
                &format!("first-text-{}-{motion}", preset.name()),
            );
            owner.dispose();
        }
    }
    crate::app::reactive::reset_registered_anchors();
    crate::style::install_theme_overrides(overrides).unwrap();
    crate::style::set_theme(previous);
}

#[test]
#[ignore = "requires a native Windows GPU surface; run explicitly with --ignored --nocapture"]
fn optical_popover_open_hover_close_frames_match_full_composites() {
    with_native_window(run_optical_frames);
}

#[test]
#[ignore = "requires a native Windows GPU surface; run explicitly with --ignored --nocapture"]
fn themed_group_hover_text_frames_match_full_composites() {
    with_native_window(run_group_frames);
}

#[test]
#[ignore = "requires a native Windows GPU surface; run explicitly with --ignored --nocapture"]
fn window_material_follows_presets_without_recreating_the_composition_surface() {
    with_native_window_options(
        run_window_material_frames,
        crate::app::WindowOptions::new().follow_theme(true),
    );
}

fn with_native_window(run: fn(Arc<dyn Window>)) {
    with_native_window_options(run, crate::app::WindowOptions::new().size(640.0, 480.0));
}

fn with_native_window_options(run: fn(Arc<dyn Window>), options: crate::app::WindowOptions) {
    use winit::application::ApplicationHandler;
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, EventLoop};
    use winit::platform::windows::EventLoopBuilderExtWindows;
    use winit::window::WindowId;

    struct Harness(fn(Arc<dyn Window>), crate::app::WindowOptions);
    impl ApplicationHandler for Harness {
        fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
            let window: Arc<dyn Window> = Arc::from(
                event_loop
                    .create_window(
                        self.1
                            .to_window_attributes()
                            .with_title("Jadix optical frame regression")
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
        .run_app(Harness(run, options))
        .unwrap();
}

fn run_window_material_frames(window: Arc<dyn Window>) {
    let previous = crate::style::get_theme();
    let mut state = crate::utils::block_on::block_on(AppState::new_bootstrap(
        window,
        crate::app::WindowOptions::new().follow_theme(true),
    ));
    assert!(state.composition_swapchain);
    let surface = crate::platform::custom_glass_surface_handle(state.window.as_ref())
        .expect("composition surface");
    for preset in crate::style::ThemePreset::ALL {
        for mode in [
            crate::style::ThemeMode::Light,
            crate::style::ThemeMode::Dark,
        ] {
            crate::style::set_theme_preset(preset);
            crate::style::set_theme_mode(mode);
            state.sync_window_theme();
            if crate::platform::windows::glass::system_backdrop_available() {
                use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
                let RawWindowHandle::Win32(handle) = state.window.window_handle().unwrap().as_raw() else {
                    panic!("expected a Win32 window");
                };
                let mut backdrop = 0_i32;
                let result = unsafe {
                    windows_sys::Win32::Graphics::Dwm::DwmGetWindowAttribute(
                        handle.hwnd.get() as _,
                        windows_sys::Win32::Graphics::Dwm::DWMWA_SYSTEMBACKDROP_TYPE as u32,
                        &mut backdrop as *mut i32 as _,
                        std::mem::size_of_val(&backdrop) as u32,
                    )
                };
                assert_eq!(result, 0);
                assert_eq!(backdrop, if preset == crate::style::ThemePreset::Fluent { 2 } else { 1 },
                    "{preset:?} {mode:?} must update the compositor's native material");
            }
            let (_, root) = crate::core::reactivity::create_root(|| {
                Element::new()
                    .w("100%")
                    .h("100%")
                    .child(crate::components::button("native-window-material").text(preset.name()))
                    .child(crate::components::scroll_area("native-material-scroll")
                        .w(200.0)
                        .h(100.0)
                        .scrollbar_visibility("always")
                        .child(Element::new().h(300.0)))
            });
            state.rebuild_ui(root);
            capture(&mut state, [-1.0; 2], true);
            state.render([-1.0; 2], ScrollbarUpdateMode::Full, None, false, false, false).unwrap();
            let scrollbar = &state.scrollbar_base_style["native-material-scroll:y"];
            assert_eq!(scrollbar.track_bg, crate::style::get_theme().colors.scrollbar_track,
                "{preset:?} {mode:?} must discard the previous theme's scrollbar rail");
            assert_eq!(scrollbar.thumb_bg, crate::style::get_theme().colors.scrollbar_thumb);
            assert_eq!(
                crate::platform::custom_glass_surface_handle(state.window.as_ref()),
                Some(surface)
            );
            assert_eq!(
                state.window.theme(),
                Some(super::super::super::window_chrome::native_theme())
            );
        }
    }
    crate::style::set_theme(previous);
}

fn run_group_frames(window: Arc<dyn Window>) {
    use crate::prelude::*;
    let overrides = crate::style::theme_overrides();
    let mode = crate::style::theme_mode();
    crate::style::clear_theme_overrides();
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    let mut state = crate::utils::block_on::block_on(AppState::new_bootstrap(
        window,
        super::super::super::window_chrome::WindowOptions::default(),
    ));
    let mut failures = Vec::new();
    let mut samples = 0;
    for preset in ThemePreset::ALL {
        if std::env::var("JADIX_GROUP_FRAME_PRESET").is_ok_and(|name| name != preset.name()) {
            continue;
        }
        crate::style::set_theme_preset(preset);
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            crate::style::set_theme_mode(mode);
            for vertical in [false, true] {
                for toggle in [false, true] {
                    let (owner, group) = crate::core::reactivity::create_root(|| {
                        let orientation = if vertical { "vertical" } else { "horizontal" };
                        let group: Element = if toggle {
                            toggle_group("native-group-toggle")
                                .variant("outline")
                                .orientation(orientation)
                                .spacing(0)
                                .default_value("middle")
                                .options([
                                    ("top", "Top"),
                                    ("middle", "Middle"),
                                    ("bottom", "Bottom"),
                                ])
                                .into()
                        } else {
                            button_group()
                                .orientation(orientation)
                                .child(button("native-group-top").variant("outline").text("Top"))
                                .child(
                                    button("native-group-middle")
                                        .variant("outline")
                                        .text("Middle"),
                                )
                                .child(
                                    button("native-group-bottom")
                                        .variant("outline")
                                        .text("Bottom"),
                                )
                                .into()
                        };
                        if vertical { group.w(160.0) } else { group }
                    });
                    state.rebuild_ui(crate::ui! {
                        <VStack w="100%" h="100%" class="p-6 gap-4 bg-background">
                            <Text>"Retained text and segment boundaries"</Text>
                            {group}
                            <Text>"Unchanged foreground"</Text>
                        </VStack>
                    });
                    capture(&mut state, [-1.0; 2], true);
                    let mut bounds = Vec::new();
                    crate::core::tree::walk_elements(&state.root_element, |element| {
                        if matches!(
                            element.attrs.get("data-slot").map(String::as_str),
                            Some("button" | "toggle-group-item")
                        ) {
                            bounds.push(element.layout_bounds);
                        }
                    });
                    assert_eq!(bounds.len(), 3);
                    for hovered in [Some(0), Some(1), Some(2), Some(0), None] {
                        let mouse = hovered
                            .map(|index| {
                                let rect = bounds[index];
                                [rect[0] + rect[2] * 0.5, rect[1] + rect[3] * 0.5]
                            })
                            .unwrap_or([-1.0; 2]);
                        for tick in 0..16 {
                            crate::style::advance_css_transition_timeline_ms(16.0);
                            crate::style::tick_animations(0.016);
                            crate::style::utility::set_frame_dt(0.016);
                            crate::app::tree_hit::sync_pointer_chrome_after_layout(
                                &mut state.root_element,
                                &mut state.layout_tree,
                                mouse,
                                640.0,
                                480.0,
                            );
                            let actual = capture(&mut state, mouse, false);
                            crate::style::utility::set_frame_dt(0.0);
                            let expected = capture(&mut state, mouse, true);
                            let changed = actual
                                .pixels
                                .chunks_exact(4)
                                .zip(expected.pixels.chunks_exact(4))
                                .filter(|(before, after)| {
                                    before
                                        .iter()
                                        .zip(after.iter())
                                        .any(|(before, after)| before.abs_diff(*after) > 2)
                                })
                                .count();
                            if changed > 0 || (vertical && hovered == Some(1) && tick == 15) {
                                let name = format!(
                                    "group-{}-{mode:?}-{vertical}-{toggle}-{hovered:?}-{tick}",
                                    preset.name()
                                );
                                snapshot(&actual, state.config.format, &format!("{name}-actual"));
                                snapshot(
                                    &expected,
                                    state.config.format,
                                    &format!("{name}-reference"),
                                );
                                if changed > 0 {
                                    failures.push((name, changed));
                                }
                            }
                            samples += 1;
                        }
                    }
                    owner.dispose();
                    crate::app::tree_hit::clear_hover_tracking();
                }
            }
        }
    }
    crate::style::utility::set_frame_dt(0.0);
    crate::style::set_css_transition_virtual_timeline(false);
    crate::style::install_theme_overrides(overrides).unwrap();
    crate::style::set_theme_mode(mode);
    eprintln!("native group text frames: {samples}, divergent: {failures:?}");
    assert!(samples > 0, "preset filter must select an existing preset");
    assert!(
        failures.is_empty(),
        "group hover must not overpaint text or leave stale chrome"
    );
}

fn capture(state: &mut AppState, mouse: [f32; 2], full: bool) -> RgbaImage {
    if full {
        state.mark_full_redraw();
    }
    state
        .render(
            mouse,
            ScrollbarUpdateMode::ViewportsOnly,
            None,
            false,
            false,
            false,
        )
        .unwrap();
    assert!(state.present_retain.contents_valid());
    let texture = state.present_retain.color_texture_for_test().unwrap();
    readback_texture(
        &state.device,
        &state.queue,
        texture,
        state.config.width,
        state.config.height,
        state.device.create_command_encoder(&Default::default()),
    )
    .unwrap()
}

fn snapshot(image: &RgbaImage, format: wgpu::TextureFormat, name: &str) {
    let Some(directory) = std::env::var_os("JADIX_GLASS_EVIDENCE") else {
        return;
    };
    std::fs::create_dir_all(&directory).unwrap();
    let mut pixels = image.pixels.clone();
    if matches!(
        format,
        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
    ) {
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
    }
    ::image::save_buffer(
        std::path::Path::new(&directory).join(format!("{name}.png")),
        &pixels,
        image.width,
        image.height,
        ::image::ColorType::Rgba8,
    )
    .unwrap();
}

fn run_optical_frames(window: Arc<dyn Window>) {
    use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
    use crate::app::runtime::{AppRuntime, clear_app_runtime, set_app_runtime};
    use crate::core::reactivity::{create_effect, create_root};
    use crate::utils::floating_motion::{drain_pending_floating_unmounts, tick_floating_presence};

    crate::app::portal_overlay::clear_overlay_stack();
    reset_registered_anchors();
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    let overrides = crate::style::theme_overrides();
    let mode = crate::style::theme_mode();
    crate::style::clear_theme_overrides();
    crate::style::set_theme_preset(crate::style::ThemePreset::LiquidGlass);
    let state = Rc::new(RefCell::new(Some(crate::utils::block_on::block_on(
        AppState::new_bootstrap(
            window,
            super::super::super::window_chrome::WindowOptions::default(),
        ),
    ))));
    let secondaries = Rc::new(RefCell::new(std::collections::HashMap::new()));
    set_app_runtime(AppRuntime::new(
        Rc::downgrade(&state),
        Rc::downgrade(&secondaries),
    ));
    let (owner, (open, inactive_modal, modal)) = create_root(|| {
        use crate::prelude::*;
        let open = use_signal("optical-native-open", || false);
        let modal = use_signal("optical-native-modal", || false);
        let live = Rc::clone(&state);
        crate::core::reactivity::mark_next_effect_as_ui_root();
        create_effect(move || {
            let stripes: Vec<Element> = (0..24)
                .map(|stripe| {
                    Element::new()
                        .absolute()
                        .left(stripe as f32 * 24.0)
                        .top(0.0)
                        .w(8.0)
                        .h("100%")
                        .bg(if stripe % 2 == 0 { "primary" } else { "border" })
                })
                .collect();
            let floating = if modal.get() {
                crate::ui! {
                    <Dialog id="optical-native-dialog" open={open}>
                        <DialogContent>
                            <DialogTitle>"Stable optical dialog"</DialogTitle>
                            <DialogBody><Input id="optical-native-input" placeholder="Sharp foreground" /></DialogBody>
                        </DialogContent>
                    </Dialog>
                }
            } else {
                crate::ui! {
                    <Box class="absolute left-8 top-64">
                        <Popover id="optical-native" open={open}>
                            <PopoverTrigger><Button id="optical-native-trigger">"Optical popover"</Button></PopoverTrigger>
                            <PopoverContent side="top" align="start">
                                <PopoverTitle>"Stable optical layer"</PopoverTitle>
                                <Input id="optical-native-input" placeholder="Sharp foreground" />
                            </PopoverContent>
                        </Popover>
                    </Box>
                }
            };
            let root = crate::ui! {
                <Box w="100%" h="100%" bg="background" class="relative">
                    { stripes }
                    { floating }
                </Box>
            };
            live.borrow_mut().as_mut().unwrap().rebuild_ui(root);
        });
        (open, Signal::new(false), modal)
    });
    let mut samples = 0;
    let mut failures = Vec::new();
    let phases = [
        crate::style::ThemeMode::Light,
        crate::style::ThemeMode::Dark,
    ]
    .into_iter()
    .flat_map(|mode| [false, true, false, true, false].map(|opened| (mode, opened)));
    for (phase, (theme_mode, opened)) in phases.enumerate() {
        crate::style::set_theme_mode(theme_mode);
        modal.set(phase % 5 >= 3);
        open.set(opened);
        for tick in 0..20 {
            std::thread::sleep(std::time::Duration::from_millis(16));
            crate::style::set_global_time_for_test(Some((samples + 1) as f32 * 0.016));
            crate::style::advance_css_transition_timeline_ms(16.0);
            crate::style::tick_animations(0.016);
            tick_floating_presence();
            drain_pending_floating_unmounts();
            drain_deferred_view_remounts();
            crate::app::portal_overlay::portal_sync(
                "optical-native-inactive-sidebar",
                inactive_modal,
                crate::app::portal_overlay::OverlayKind::Modal,
                crate::app::portal_overlay::OverlayMode::Stacked,
            );
            assert_eq!(open.peek(), opened);
            crate::core::reactivity::use_signal("__jadix_button_focus_id", String::new).set(
                if opened && (8..16).contains(&tick) {
                    crate::components::input::focus_id_for_input("optical-native-input")
                } else {
                    String::new()
                },
            );
            let mouse = if tick % 2 == 0 {
                [90.0, 200.0]
            } else {
                [-1.0; 2]
            };
            let mut state = state.borrow_mut();
            let state = state.as_mut().unwrap();
            crate::style::utility::set_frame_dt(0.016);
            let incremental = capture(state, mouse, false);
            crate::style::utility::set_frame_dt(0.0);
            let reference = capture(state, mouse, true);
            if opened && tick == 19 {
                assert!(
                    state.local_backdrop_capture_count > 0,
                    "test must exercise real optical backdrop capture"
                );
            }
            let changed = incremental
                .pixels
                .chunks_exact(4)
                .zip(reference.pixels.chunks_exact(4))
                .filter(|(before, after)| {
                    before
                        .iter()
                        .zip(after.iter())
                        .any(|(before, after)| before.abs_diff(*after) > 2)
                })
                .count();
            if changed > 0 || tick == 19 || (opened && tick == 10) {
                snapshot(
                    &incremental,
                    state.config.format,
                    &format!("native-{phase}-{tick}-actual"),
                );
                snapshot(
                    &reference,
                    state.config.format,
                    &format!("native-{phase}-{tick}-reference"),
                );
            }
            if changed > 0 {
                failures.push((opened, tick, changed));
            }
            samples += 1;
        }
    }
    clear_app_runtime();
    owner.dispose();
    crate::style::utility::set_frame_dt(0.0);
    crate::style::set_global_time_for_test(None);
    crate::style::set_css_transition_virtual_timeline(false);
    crate::app::portal_overlay::clear_overlay_stack();
    reset_registered_anchors();
    crate::style::install_theme_overrides(overrides).unwrap();
    crate::style::set_theme_mode(mode);
    eprintln!("optical native frames: {samples}, divergent: {failures:?}");
    assert!(
        failures.is_empty(),
        "optical layer must not flash between incremental and full composites: {failures:?}"
    );
}
