use super::*;
use std::{cell::Cell, rc::Rc};

fn readback_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
    encoder: wgpu::CommandEncoder,
) -> Option<crate::renderer::offscreen::RgbaImage> {
    let mut image = crate::renderer::offscreen::readback_texture(
        device, queue, texture, width, height, encoder,
    )?;
    if matches!(
        texture.format(),
        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
    ) {
        for pixel in image.pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
    }
    Some(image)
}

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
#[cfg(feature = "product-tests")]
fn shader_keeps_its_frame_clock_after_other_animations_settle() {
    native_window(|window| {
        crate::core::reactivity::create_root(|| {
            let mut state = crate::utils::block_on::block_on(AppState::new_bootstrap(
                window,
                crate::app::WindowOptions::new(),
            ));
            let leaf: Element = crate::primitives::shader::shader().w(80.0).h(60.0).wgsl(
                "@fragment fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> { return vec4<f32>(pow(u.time, 2.2), 0.0, 0.0, 1.0); }",
            ).into();
            state.rebuild_ui(
                Element::new().w("100%").h("100%").bg("#000000").child(
                    crate::components::scroll_area::scroll_area("shader-fading-scrollbar")
                        .w(220.0)
                        .h(140.0)
                        .scrollbar_visibility("auto")
                        .child(
                            Element::new()
                                .flex_col()
                                .w("100%")
                                .child(leaf)
                                .child(Element::new().w(160.0).h(40.0).bg("#35b968"))
                                .child(Element::new().h(500.0)),
                        ),
                ),
            );
            state
                .render(
                    [-1.0; 2],
                    ScrollbarUpdateMode::Full,
                    None,
                    false,
                    false,
                    false,
                )
                .unwrap();
            let scrollbar = crate::style::use_ui_transition_cubic(
                "scrollbar_spring_shader-fading-scrollbar:y",
                0.0,
            );
            scrollbar.set(1.0);
            for _ in 0..4 {
                crate::style::tick_animations(0.08);
            }
            assert!(scrollbar.peek() > 0.9);
            scrollbar.set(0.0);
            let mut previous = None;
            let mut previous_pixels: Option<Vec<u8>> = None;
            let mut partial_frames = 0;
            let mut fading_frames = 0;
            for frame in 0..120 {
                crate::style::tick_animations(1.0 / 120.0);
                let alpha = scrollbar.peek();
                fading_frames += usize::from(alpha > 0.01 && alpha < 0.99);
                crate::style::set_global_time_for_test(Some(0.05 + frame as f32 / 150.0));
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
                partial_frames += usize::from(state.last_frame_partial);
                assert!(
                    state.shader_frame_active,
                    "shader clock stopped at frame {frame}"
                );
                assert!(
                    crate::app::lifecycle::about_to_wait_force_immediate(
                        false,
                        false,
                        false,
                        false,
                        state.shader_frame_active,
                        false,
                        false,
                        false,
                    ),
                    "idle caret or scrollbar timer must not delay shader frame {frame}"
                );
                let image = readback_texture(
                    &state.device,
                    &state.queue,
                    state.present_retain.color_texture_for_test().unwrap(),
                    state.config.width,
                    state.config.height,
                    state.device.create_command_encoder(&Default::default()),
                )
                .unwrap();
                crate::core::tree::walk_elements(&state.root_element, |element| {
                    if crate::core::scrollbar::auto_scrollbar_live_paint_alpha(element).is_some() {
                        crate::app::damage::push_frame_paint_element(
                            element,
                            element.paint_scroll_acc,
                        );
                    }
                });
                let rails = crate::app::damage::take_frame_paint_nodes();
                assert!(!rails.is_empty());
                for rail in &rails {
                    assert_eq!(rail.kind, crate::app::damage::PaintDirtyKind::Region);
                    assert!(
                        crate::app::damage::intersect_rects(rail.bounds, [0.0, 0.0, 80.0, 60.0])
                            .is_none(),
                        "scrollbar damage reaches the shader: {rail:?}"
                    );
                }
                let viewport = [0.0, 0.0, state.logical_size().0, state.logical_size().1];
                assert_eq!(
                    crate::app::damage::frame_clear_policy_for_nodes(true, false, &rails, viewport),
                    crate::app::damage::FrameClearPolicy::PartialLoad
                );
                if let Some(before) = &previous_pixels {
                    for (index, (before, after)) in before
                        .chunks_exact(4)
                        .zip(image.pixels.chunks_exact(4))
                        .enumerate()
                    {
                        let point = [
                            (index as u32 % state.config.width) as f32 / state.ui_scale(),
                            (index as u32 / state.config.width) as f32 / state.ui_scale(),
                        ];
                        let inside = |bounds: [f32; 4]| {
                            point[0] >= bounds[0] - 4.0
                                && point[0] <= bounds[0] + bounds[2] + 4.0
                                && point[1] >= bounds[1] - 4.0
                                && point[1] <= bounds[1] + bounds[3] + 4.0
                        };
                        if !inside([0.0, 0.0, 80.0, 60.0])
                            && !rails.iter().any(|rail| inside(rail.bounds))
                        {
                            assert_eq!(before, after, "scrollbar exit changed unrelated pixel at {point:?}, frame={frame}");
                        }
                    }
                }
                previous_pixels = Some(image.pixels.clone());
                let red = image
                    .pixel(
                        (20.0 * state.ui_scale()) as u32,
                        (20.0 * state.ui_scale()) as u32,
                    )
                    .unwrap()[0];
                if let Some(previous) = previous {
                    assert!(
                        red > previous && red - previous < 8,
                        "shader jumped or froze at frame {frame}: {previous} -> {red}"
                    );
                }
                previous = Some(red);
            }
            assert!(
                partial_frames > 100,
                "shader updates must stay local: {partial_frames}"
            );
            assert!(
                fading_frames > 3,
                "must exercise real scrollbar exit frames: {fading_frames}"
            );
            state.rebuild_ui(Element::new().w("100%").h("100%"));
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
            assert!(
                !state.shader_frame_active,
                "removing the shader must let the window idle"
            );
            crate::style::set_global_time_for_test(None);
        });
    });
}

#[test]
#[ignore = "requires a native Windows GPU surface"]
#[cfg(feature = "product-tests")]
fn dark_progress_track_has_uniform_gpu_coverage() {
    native_window(|window| {
        crate::core::reactivity::create_root(|| {
            crate::style::set_theme_preset(crate::style::ThemePreset::Shadcn);
            crate::style::set_theme_mode(crate::style::ThemeMode::Dark);
            let mut state = crate::utils::block_on::block_on(AppState::new_bootstrap(
                window,
                crate::app::WindowOptions::new(),
            ));
            state.rebuild_ui(Element::new().w("100%").h("100%").bg("background").child(
                crate::components::progress::progress_circle_svg(
                    0.0,
                    crate::components::progress::ProgressCircleSize::Md,
                    None,
                    None,
                    false,
                ),
            ));
            state
                .render(
                    [-1.0; 2],
                    ScrollbarUpdateMode::Full,
                    None,
                    false,
                    false,
                    false,
                )
                .unwrap();
            assert!(
                state.path2d.is_some(),
                "coverage must exercise the GPU path renderer"
            );
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
            let center = image
                .pixel((32.0 * scale) as u32, (32.0 * scale) as u32)
                .unwrap();
            let background = image
                .pixel((100.0 * scale) as u32, (100.0 * scale) as u32)
                .unwrap();
            assert_eq!(
                center, background,
                "track must leave the center transparent"
            );
            let mut values = Vec::new();
            for sample in 0..360 {
                let angle = sample as f32 * std::f32::consts::TAU / 360.0;
                let x = ((32.0 + 29.0 * angle.cos()) * scale) as u32;
                let y = ((32.0 + 29.0 * angle.sin()) * scale) as u32;
                values.push(image.pixel(x, y).unwrap()[0]);
            }
            assert!(
                values.iter().max().unwrap() - values.iter().min().unwrap() <= 2,
                "GPU track has repeated alpha overdraw: {values:?}"
            );
            assert!(
                *values.iter().min().unwrap() > background[0],
                "track must be visible"
            );
        });
    });
}

#[test]
#[ignore = "requires a native Windows GPU surface"]
#[cfg(feature = "product-tests")]
fn rebuilt_hovered_button_presents_without_color_flash() {
    native_window(run_button_frames);
}

#[test]
#[ignore = "requires a native Windows GPU surface"]
#[cfg(feature = "product-tests")]
fn fluent_hover_exit_continuous_frames_match_full_repaint() {
    fluent_hover_frames(false, false);
}

#[test]
#[ignore = "requires a native Windows GPU surface"]
#[cfg(feature = "product-tests")]
fn fluent_hover_on_transparent_material_matches_full_repaint() {
    fluent_hover_frames(true, false);
}

#[test]
#[ignore = "requires a native Windows GPU surface"]
#[cfg(feature = "product-tests")]
fn fluent_translucent_hover_frames_match_full_repaint() {
    fluent_hover_frames(true, true);
}

#[cfg(feature = "product-tests")]
fn fluent_hover_frames(transparent: bool, translucent: bool) {
    native_window(move |window| {
        crate::core::reactivity::create_root(|| {
            crate::style::set_theme_preset(crate::style::ThemePreset::Fluent);
            crate::style::set_theme_mode(crate::style::ThemeMode::Dark);
            crate::style::set_css_transition_virtual_timeline(true);
            let mut samples = Vec::<crate::renderer::offscreen::RgbaImage>::new();
            for full_repaint in [false, true] {
                crate::style::set_css_transition_timeline_ms(0.0);
                let mut state = crate::utils::block_on::block_on(AppState::new_bootstrap(
                    window.clone(),
                    crate::app::WindowOptions::new().transparent(transparent),
                ));
                let mut root = Element::new()
                    .w(208.0)
                    .h(160.0)
                    .p(12.0)
                    .bg("card")
                    .rounded(8.0)
                    .flex_col();
                root.set_attr("data-slot", "card");
                for variant in ["secondary", "outline", "ghost", "sidebar"] {
                    crate::core::reactivity::use_signal(
                        &format!("native-fluent-{variant}_hover"),
                        || false,
                    )
                    .set_silent(false);
                    let control = if variant == "sidebar" {
                        Element::from(
                            crate::components::sidebar::sidebar_menu_button(format!(
                                "native-fluent-{variant}"
                            ))
                            .text(variant),
                        )
                    } else {
                        Element::from(
                            crate::components::button::button(format!("native-fluent-{variant}"))
                                .variant(variant)
                                .class("transition-colors duration-150")
                                .text(variant),
                        )
                    };
                    root = root.child(control.w(160.0).h(32.0));
                }
                root = crate::style::apply_stylesheet_rules(root);
                if translucent {
                    root.style.bg_color[3] = 0.18;
                }
                let mut page = Element::new().w("100%").h("100%").child(root);
                if !transparent {
                    page = page.bg("background");
                }
                state.rebuild_ui(page);
                let mut partial_frames = 0;
                let mut partial_hover_frames = 0;
                for frame in 0..720 {
                    crate::style::set_css_transition_timeline_ms(frame as f64 * 1000.0 / 120.0);
                    crate::style::utility::set_frame_dt(1.0 / 120.0);
                    let phase = frame % 90;
                    let variant = ["secondary", "outline", "ghost", "sidebar"][(frame / 90) % 4];
                    let hovered = (20..50).contains(&phase);
                    if matches!(phase, 20 | 50) {
                        crate::primitives::interactive::mark_hover_paint_dirty();
                    }
                    let pointer = if hovered {
                        let button = crate::core::tree::find_by_template_id(
                            &state.root_element,
                            &format!("native-fluent-{variant}"),
                        )
                        .unwrap();
                        let bounds = button.viewport_layout_bounds();
                        if matches!(phase, 20 | 49) {
                            [bounds[0] + 0.25, bounds[1] + 0.25]
                        } else {
                            [bounds[0] + bounds[2] * 0.5, bounds[1] + bounds[3] * 0.5]
                        }
                    } else {
                        [-1.0; 2]
                    };
                    if full_repaint {
                        state.mark_full_redraw();
                    }
                    state
                        .render(
                            pointer,
                            ScrollbarUpdateMode::ViewportsOnly,
                            None,
                            false,
                            false,
                            frame >= 360,
                        )
                        .unwrap();
                    partial_frames += usize::from(state.last_frame_partial);
                    partial_hover_frames +=
                        usize::from(state.last_frame_partial && (21..65).contains(&phase));
                    let current = readback_texture(
                        &state.device,
                        &state.queue,
                        state.present_retain.color_texture_for_test().unwrap(),
                        state.config.width,
                        state.config.height,
                        state.device.create_command_encoder(&Default::default()),
                    )
                    .unwrap();
                    if full_repaint {
                        let partial = &samples[frame];
                        let changed = partial
                            .pixels
                            .chunks_exact(4)
                            .zip(current.pixels.chunks_exact(4))
                            .filter(|(left, right)| {
                                left.iter()
                                    .zip(*right)
                                    .any(|(left, right)| left.abs_diff(*right) > 3)
                            })
                            .count();
                        if changed > 0 {
                            for (name, image) in [
                                ("jadix-fluent-partial.png", partial),
                                ("jadix-fluent-full.png", &current),
                            ] {
                                image::save_buffer(
                                    std::env::temp_dir().join(name),
                                    &image.pixels,
                                    image.width,
                                    image.height,
                                    image::ColorType::Rgba8,
                                )
                                .unwrap();
                            }
                        }
                        assert_eq!(changed, 0, "Fluent repaint diverged at frame {frame}");
                    } else {
                        samples.push(current);
                    }
                    let mut buttons = 0;
                    crate::core::tree::walk_elements(&state.root_element, |button| {
                        if button.attrs.get("data-slot").map(String::as_str) == Some("button") {
                            buttons += 1;
                            if phase == 47
                                && button.template_id.as_deref()
                                    == Some(format!("native-fluent-{variant}").as_str())
                            {
                                assert_eq!(
                                    button
                                        .class_set
                                        .as_ref()
                                        .and_then(|classes| classes.last_pointer_hover),
                                    Some(true)
                                );
                            }
                            assert_eq!(
                                button.style.radii, [4.0; 4],
                                "Fluent radius changed at frame {frame}"
                            );
                            assert_eq!(
                                crate::app::damage::paint_kind_for_element(button),
                                crate::app::damage::PaintDirtyKind::Node
                            );
                        }
                    });
                    assert_eq!(buttons, 3);
                }
                if !full_repaint {
                    assert!(
                        partial_frames > 30,
                        "sampling must exercise granular repaint, got {partial_frames}"
                    );
                    assert!(partial_hover_frames > 100,
                        "hover transitions must retain granular repaint, got {partial_hover_frames}");
                }
            }
            crate::style::set_css_transition_virtual_timeline(false);
        });
    });
}

#[test]
#[ignore = "requires a native Windows GPU surface"]
#[cfg(feature = "product-tests")]
fn themed_popover_frost_does_not_blur_pixels_outside_the_panel() {
    native_window(|window| {
        crate::core::reactivity::create_root(|| {
            for preset in [
                crate::style::ThemePreset::Fluent,
                crate::style::ThemePreset::LiquidGlass,
            ] {
                crate::style::set_theme_preset(preset);
                crate::style::set_theme_mode(crate::style::ThemeMode::Dark);
                let mut state = crate::utils::block_on::block_on(AppState::new_bootstrap(
                    window.clone(),
                    crate::app::WindowOptions::new(),
                ));
                let page = || {
                    let mut page = Element::new().w("100%").h("100%").bg("#102040");
                    for stripe in 0..26 {
                        page = page.child(
                            Element::new()
                                .absolute()
                                .left(stripe as f32 * 8.0)
                                .top(0.0)
                                .w(4.0)
                                .h("100%")
                                .bg("#d0d0d0"),
                        );
                    }
                    page
                };
                state.rebuild_ui(page());
                state
                    .render(
                        [-1.0; 2],
                        ScrollbarUpdateMode::Full,
                        None,
                        false,
                        false,
                        false,
                    )
                    .unwrap();
                let read = |state: &AppState| {
                    readback_texture(
                        &state.device,
                        &state.queue,
                        state.present_retain.color_texture_for_test().unwrap(),
                        state.config.width,
                        state.config.height,
                        state.device.create_command_encoder(&Default::default()),
                    )
                    .unwrap()
                };
                let before = read(&state);
                let mut panel = crate::components::popover::popover_surface(Element::new(), None)
                    .absolute()
                    .left(80.0)
                    .top(65.0)
                    .w(100.0)
                    .h(65.0);
                panel.set_attr(
                    crate::layout::layout_tree::ATTR_PORTAL_KEY,
                    "panel:optical-test",
                );
                let panel = crate::utils::popup::panel_block_clicks(panel, 1000);
                state.rebuild_ui(page().child(panel));
                for _ in 0..2 {
                    state
                        .render(
                            [-1.0; 2],
                            ScrollbarUpdateMode::Full,
                            None,
                            false,
                            false,
                            false,
                        )
                        .unwrap();
                    assert_eq!(
                        state.modal_blur_radius, None,
                        "{preset:?} popover requested viewport blur"
                    );
                    let after = read(&state);
                    let scale = state.ui_scale();
                    for y in 0..(30.0 * scale) as u32 {
                        for x in 0..after.width {
                            assert_eq!(
                                before.pixel(x, y),
                                after.pixel(x, y),
                                "{preset:?} blurred page pixel ({x}, {y})"
                            );
                        }
                    }
                    assert_ne!(
                        before.pixel((128.0 * scale) as u32, (90.0 * scale) as u32),
                        after.pixel((128.0 * scale) as u32, (90.0 * scale) as u32)
                    );
                }
                assert!(
                    state.local_backdrop_capture_count > 0,
                    "{preset:?} must retain local material sampling"
                );
            }
        });
    });
}

#[test]
#[ignore = "requires a native Windows GPU surface"]
#[cfg(feature = "product-tests")]
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

#[test]
#[ignore = "requires a native Windows GPU surface"]
#[cfg(feature = "product-tests")]
fn sheet_edge_keeps_coverage_at_animation_settle() {
    native_window(|window| {
        use std::{cell::RefCell, rc::Rc};
        crate::init();
        crate::app::reactive::reset_registered_anchors();
        crate::app::portal_overlay::clear_overlay_stack();
        crate::style::set_css_transition_virtual_timeline(true);
        crate::style::set_css_transition_timeline_ms(0.0);
        crate::style::set_global_time_for_test(Some(0.0));
        let live = Rc::new(RefCell::new(Some(crate::utils::block_on::block_on(
            AppState::new_bootstrap(window, crate::app::WindowOptions::new()),
        ))));
        let secondary = Rc::new(RefCell::new(HashMap::new()));
        crate::app::runtime::set_app_runtime(crate::app::runtime::AppRuntime::new(
            Rc::downgrade(&live),
            Rc::downgrade(&secondary),
        ));
        crate::core::reactivity::create_root(|| {
            crate::style::set_theme_preset(crate::style::ThemePreset::Shadcn);
            crate::style::set_theme_mode(crate::style::ThemeMode::Dark);
            crate::style::set_css_transition_virtual_timeline(true);
            let open = crate::core::reactivity::Signal::new(false);
            let state = live.clone();
            crate::core::reactivity::mark_next_effect_as_ui_root();
            crate::core::reactivity::create_effect(move || {
                let sheet = crate::components::sheet::sheet_scope("edge-sheet", Some(open), || {
                    crate::components::sheet::sheet_content(
                        crate::components::sheet::SheetContentOpts {
                            show_close: false,
                            ..Default::default()
                        },
                        || Element::new().h(40.0),
                    )
                });
                state.borrow_mut().as_mut().unwrap().rebuild_ui(
                    Element::new()
                        .w("100%")
                        .h("100%")
                        .bg("background")
                        .child(sheet),
                );
            });
            crate::app::reactive::drain_deferred_view_remounts();
            live.borrow_mut()
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
            open.set(true);
            let mut previous: Option<Vec<u8>> = None;
            let mut sampled = 0;
            for frame in 0..65 {
                crate::style::set_css_transition_timeline_ms(frame as f64 * 1000.0 / 120.0);
                crate::style::set_global_time_for_test(Some(frame as f32 / 120.0));
                crate::style::utility::set_frame_dt(1.0 / 120.0);
                crate::app::reactive::drain_deferred_view_remounts();
                let mut guard = live.borrow_mut();
                let state = guard.as_mut().unwrap();
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
                let Some(panel) = crate::core::tree::find_by_template_id(
                    &state.root_element,
                    "edge-sheet__modal_panel",
                ) else {
                    continue;
                };
                let edge = (panel.layout_bounds[0] * state.ui_scale()).round() as u32;
                let image = readback_texture(
                    &state.device,
                    &state.queue,
                    state.present_retain.color_texture_for_test().unwrap(),
                    state.config.width,
                    state.config.height,
                    state.device.create_command_encoder(&Default::default()),
                )
                .unwrap();
                let values = (edge.saturating_sub(3)..edge + 5)
                    .map(|column| image.pixel(column, image.height / 2).unwrap()[0])
                    .collect::<Vec<_>>();
                if frame >= 45 {
                    sampled += 1;
                    if let Some(previous) = previous {
                        let delta = values
                            .iter()
                            .zip(previous)
                            .map(|(current, before)| current.abs_diff(before))
                            .max()
                            .unwrap();
                        assert!(delta <= 5, "sheet edge changed after settling: {delta}");
                    }
                    previous = Some(values);
                }
            }
            assert!(sampled > 10, "sheet panel must be mounted during sampling");
            crate::style::set_css_transition_virtual_timeline(false);
            crate::style::set_global_time_for_test(None);
        });
    });
}

#[test]
#[ignore = "requires a native Windows GPU surface"]
#[cfg(feature = "product-tests")]
fn avatar_group_presence_fades_its_faces() {
    native_window(|window| {
        use std::{cell::RefCell, rc::Rc};
        crate::init();
        crate::app::reactive::reset_registered_anchors();
        crate::app::portal_overlay::clear_overlay_stack();
        crate::style::set_theme_preset(crate::style::ThemePreset::Shadcn);
        crate::style::set_theme_mode(crate::style::ThemeMode::Dark);
        crate::style::set_css_transition_virtual_timeline(true);
        crate::style::set_css_transition_timeline_ms(0.0);
        #[cfg(not(feature = "svg"))]
        let image_src = {
            let image_path = std::env::temp_dir().join("jadix-avatar-presence.png");
            image::save_buffer(
                &image_path,
                &[240, 40, 40, 255].repeat(64),
                8,
                8,
                image::ColorType::Rgba8,
            )
            .unwrap();
            image_path.to_string_lossy().into_owned()
        };
        #[cfg(feature = "svg")]
        let image_src = "data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' width='64' height='64'><rect width='64' height='64' fill='%23ef4444'/></svg>".to_string();
        let live = Rc::new(RefCell::new(Some(crate::utils::block_on::block_on(
            AppState::new_bootstrap(window, crate::app::WindowOptions::new()),
        ))));
        let secondary = Rc::new(RefCell::new(HashMap::new()));
        crate::app::runtime::set_app_runtime(crate::app::runtime::AppRuntime::new(
            Rc::downgrade(&live),
            Rc::downgrade(&secondary),
        ));
        crate::core::reactivity::create_root(|| {
            let state = live.clone();
            crate::core::reactivity::mark_next_effect_as_ui_root();
            crate::core::reactivity::create_effect(move || {
                let mut packed = Element::new();
                packed.set_attr("jadix_fragment", "1");
                packed = packed.children(["A", "B", "C"].map(|label| {
                    let face = crate::components::avatar::avatar();
                    Element::from(if label == "A" {
                        face.image(image_src.clone()).fallback(label)
                    } else {
                        face.fallback(label)
                    })
                }));
                let mut count = Element::new();
                count.set_attr("jadix_fragment", "1");
                count = count
                    .child(Element::new().text("+3"))
                    .children(["D", "E", "F"].map(|label| {
                        Element::from(crate::components::avatar::avatar().fallback(label))
                    }));
                packed = packed.child(crate::components::avatar::AvatarGroupCount(
                    crate::components::avatar::AvatarGroupCountProps {
                        children: Some(count),
                        ..Default::default()
                    },
                ));
                let group = crate::components::avatar::AvatarGroup(
                    crate::components::avatar::AvatarGroupProps {
                        children: Some(packed),
                        ..Default::default()
                    },
                );
                state.borrow_mut().as_mut().unwrap().rebuild_ui(
                    Element::new().w("100%").h("100%").bg("background").child(
                        crate::components::scroll_area::scroll_area("avatar-presence-scroll")
                            .w("100%")
                            .h("100%")
                            .child(
                                Element::new()
                                    .flex_col()
                                    .w("100%")
                                    .child(group)
                                    .child(Element::new().h(500.0)),
                            ),
                    ),
                );
            });
            live.borrow_mut()
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
            let click = {
                let guard = live.borrow();
                crate::core::tree::find_in_tree(&guard.as_ref().unwrap().root_element, |element| {
                    element.attrs.get("data-slot").map(String::as_str) == Some("avatar-group-count")
                })
                .unwrap()
                .on_click
                .clone()
                .unwrap()
            };
            click.borrow_mut()();
            let mut samples = [0; 2];
            let mut face_samples = Vec::new();
            for frame in 0..70 {
                crate::style::set_css_transition_timeline_ms(frame as f64 * 16.0);
                crate::style::utility::set_frame_dt(0.016);
                if frame == 35 {
                    click.borrow_mut()();
                }
                crate::app::reactive::drain_deferred_view_remounts();
                let mut guard = live.borrow_mut();
                let state = guard.as_mut().unwrap();
                state
                    .render(
                        [-1.0; 2],
                        ScrollbarUpdateMode::ViewportsOnly,
                        None,
                        false,
                        false,
                        true,
                    )
                    .unwrap();
                let Some(panel) = crate::core::tree::find_in_tree(&state.root_element, |element| {
                    element.attrs.contains_key("data-avatar-group-popover")
                }) else {
                    continue;
                };
                let alpha = panel.style.opacity;
                if alpha > 0.01 {
                    if alpha < 0.99 {
                        samples[usize::from(frame >= 35)] += 1;
                    }
                    let face = crate::core::tree::find_in_tree(panel, |element| {
                        element.attrs.get("data-slot").map(String::as_str) == Some("avatar")
                    })
                    .unwrap();
                    let image = crate::core::tree::find_in_tree(face, |element| {
                        element.attrs.get("data-slot").map(String::as_str) == Some("avatar-image")
                    })
                    .unwrap();
                    assert_eq!(
                        image.attrs.get("data-load-state").map(String::as_str),
                        Some("ready")
                    );
                    let bounds = face.viewport_layout_bounds();
                    let pixels = readback_texture(
                        &state.device,
                        &state.queue,
                        state.present_retain.color_texture_for_test().unwrap(),
                        state.config.width,
                        state.config.height,
                        state.device.create_command_encoder(&Default::default()),
                    )
                    .unwrap();
                    let pixel = pixels
                        .pixel(
                            ((bounds[0] + bounds[2] * 0.5) * state.ui_scale()) as u32,
                            ((bounds[1] + bounds[3] * 0.5) * state.ui_scale()) as u32,
                        )
                        .unwrap();
                    let mut faces = Vec::new();
                    crate::core::tree::walk_elements(panel, |element| {
                        if element.attrs.get("data-slot").map(String::as_str) == Some("avatar") {
                            faces.push(element.viewport_layout_bounds());
                        }
                    });
                    let fallback = faces[1];
                    assert!(
                        fallback[2] > 0.0,
                        "avatar geometry disappeared at frame {frame}"
                    );
                    let face_pixel = pixels
                        .pixel(
                            ((fallback[0] + fallback[2] * 0.5) * state.ui_scale()) as u32,
                            ((fallback[1] + 5.0) * state.ui_scale()) as u32,
                        )
                        .unwrap();
                    face_samples.push((alpha, face_pixel[0]));
                    if alpha < 0.2 {
                        assert!(
                            pixel[0] < 145,
                            "image did not fade: alpha={alpha} pixel={pixel:?}"
                        );
                    } else if alpha >= 0.99 {
                        assert!(
                            pixel[0] > 200,
                            "open avatar image is not visible: {pixel:?}"
                        );
                    }
                    for text in &state.text_nodes {
                        if text.text == "A" || text.text == "B" || text.text == "C" {
                            if text.y > 32.0 {
                                assert!(
                                    text.style.color_resolved()[3] as f32 <= alpha * 255.0 + 1.0,
                                    "avatar frame={frame} panel={alpha} text={:?}",
                                    text.style.color_resolved()
                                );
                            }
                        }
                    }
                    state.mark_full_redraw();
                    crate::style::utility::set_frame_dt(0.0);
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
                    let fresh = readback_texture(
                        &state.device,
                        &state.queue,
                        state.present_retain.color_texture_for_test().unwrap(),
                        state.config.width,
                        state.config.height,
                        state.device.create_command_encoder(&Default::default()),
                    )
                    .unwrap();
                    let changed = pixels
                        .pixels
                        .chunks_exact(4)
                        .zip(fresh.pixels.chunks_exact(4))
                        .filter(|(before, after)| {
                            before
                                .iter()
                                .zip(*after)
                                .any(|(before, after)| before.abs_diff(*after) > 3)
                        })
                        .count();
                    assert_eq!(
                        changed, 0,
                        "avatar pixels diverged at frame {frame}, alpha={alpha}"
                    );
                }
            }
            assert!(
                samples.iter().all(|samples| *samples > 2),
                "must sample real enter and exit frames: {samples:?}"
            );
            let linear = |value: f32| {
                if value <= 0.04045 {
                    value / 12.92
                } else {
                    ((value + 0.055) / 1.055).powf(2.4)
                }
            };
            let background = linear(crate::style::parse_color("background")[0]);
            let rest = linear(
                face_samples
                    .iter()
                    .find(|(alpha, _)| *alpha >= 0.99)
                    .expect("fully open avatar")
                    .1 as f32
                    / 255.0,
            );
            for (alpha, pixel) in face_samples {
                let progress = (linear(pixel as f32 / 255.0) - background) / (rest - background);
                assert!((progress - alpha).abs() < 0.1,
                    "avatar and panel fade at different rates: panel={alpha}, avatar={progress}, pixel={pixel}");
            }
        });
        crate::style::set_css_transition_virtual_timeline(false);
    });
}

#[test]
#[ignore = "requires a native Windows GPU surface"]
#[cfg(feature = "product-tests")]
fn viewport_panel_keeps_its_anchor_when_resources_update_during_scroll() {
    native_window(|window| {
        crate::core::reactivity::create_root(|| {
            let mut state = crate::utils::block_on::block_on(AppState::new_bootstrap(
                window,
                crate::app::WindowOptions::new(),
            ));
            let resources = |frame: usize| {
                Element::new()
                    .template_id("perf-resources")
                    .w(140.0)
                    .bg("#162035")
                    .child(
                        Element::new()
                            .template_id(format!("perf-row-{frame}"))
                            .text(if frame % 2 == 0 {
                                "CPU 1%\nGPU 2%".to_string()
                            } else {
                                "CPU 12%\nGPU 28%\nRAM 123 MiB".to_string()
                            }),
                    )
            };
            let panel = Element::new()
                .template_id("perf-panel")
                .absolute()
                .right(12.0)
                .bottom(12.0)
                .flex_col()
                .flex_items("end")
                .gap(4.0)
                .child(
                    Element::new()
                        .template_id("perf-button")
                        .w(80.0)
                        .h(24.0)
                        .bg("#374f86"),
                )
                .child(resources(0));
            state.rebuild_ui(
                Element::new()
                    .w("100%")
                    .h("100%")
                    .bg("#090909")
                    .child(
                        crate::components::scroll_area::scroll_area("perf-scroll")
                            .w("100%")
                            .h("100%")
                            .child(Element::new().w("100%").h(1800.0)),
                    )
                    .child(Element::new().w(0.0).h(0.0).child(
                        crate::utils::viewport_portal_stage(
                            "performance-position-test",
                            panel,
                            1_000_000,
                        ),
                    )),
            );
            state
                .render(
                    [-1.0; 2],
                    ScrollbarUpdateMode::Full,
                    None,
                    false,
                    false,
                    false,
                )
                .unwrap();
            for frame in 0..120 {
                crate::core::reactivity::use_signal(
                    &crate::core::scroll_viewport::scroll_offset_y_signal_key("perf-scroll"),
                    || 0.0_f32,
                )
                .set_silent((frame * 37 % 900) as f32);
                if frame % 3 == 0 {
                    state
                        .reconcile_subtree("perf-resources", resources(frame))
                        .unwrap();
                }
                state
                    .render(
                        [-1.0; 2],
                        ScrollbarUpdateMode::ViewportsOnly,
                        None,
                        false,
                        false,
                        frame % 4 != 0,
                    )
                    .unwrap();
                let panel = crate::core::tree::find_in_tree(&state.root_element, |element| {
                    element.template_id.as_deref() == Some("perf-panel")
                })
                .unwrap();
                let bounds = panel.viewport_layout_bounds();
                let (width, height) = state.logical_size();
                assert!(
                    (width - bounds[0] - bounds[2] - 12.0).abs() < 1.0
                        && (height - bounds[1] - bounds[3] - 12.0).abs() < 1.0,
                    "performance panel lost its viewport anchor at frame {frame}: {bounds:?}"
                );
                let button = crate::core::tree::find_in_tree(panel, |element| {
                    element.template_id.as_deref() == Some("perf-button")
                })
                .unwrap()
                .viewport_layout_bounds();
                assert!(
                    (button[0] + button[2] - bounds[0] - bounds[2]).abs() < 1.0,
                    "performance button detached at frame {frame}: {button:?} / {bounds:?}"
                );
                let before = readback_texture(
                    &state.device,
                    &state.queue,
                    state.present_retain.color_texture_for_test().unwrap(),
                    state.config.width,
                    state.config.height,
                    state.device.create_command_encoder(&Default::default()),
                )
                .unwrap();
                state.mark_full_redraw();
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
                let after = readback_texture(
                    &state.device,
                    &state.queue,
                    state.present_retain.color_texture_for_test().unwrap(),
                    state.config.width,
                    state.config.height,
                    state.device.create_command_encoder(&Default::default()),
                )
                .unwrap();
                let changed = before
                    .pixels
                    .chunks_exact(4)
                    .zip(after.pixels.chunks_exact(4))
                    .filter(|(before, after)| {
                        before
                            .iter()
                            .zip(*after)
                            .any(|(before, after)| before.abs_diff(*after) > 3)
                    })
                    .count();
                assert_eq!(
                    changed, 0,
                    "performance retained pixels shifted at frame {frame}"
                );
            }
        });
    });
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

#[cfg(feature = "product-tests")]
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
