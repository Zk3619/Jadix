//! GPU differential tests: retained partial updates must match a fresh full draw.
use super::*;
use crate::app::{damage::*, draw_cull::*};
use crate::renderer::offscreen::{images_match, readback_texture, OffscreenGpu};

#[path = "state_textarea_present_tests.rs"]
mod textarea_present_tests;

#[cfg(target_os = "windows")]
#[path = "state_rotation_present_tests.rs"]
mod rotation_present_tests;

#[cfg(target_os = "windows")]
#[path = "state_pagination_present_tests.rs"]
mod pagination_present_tests;

#[cfg(target_os = "windows")]
#[path = "state_tree_view_present_tests.rs"]
mod tree_view_present_tests;

#[cfg(target_os = "windows")]
#[path = "state_drag_present_tests.rs"]
mod drag_present_tests;

#[cfg(target_os = "windows")]
#[path = "state_carousel_present_tests.rs"]
mod carousel_present_tests;

#[cfg(all(target_os = "windows", feature = "svg"))]
#[path = "state_snap_grid_present_tests.rs"]
mod snap_grid_present_tests;

#[cfg(all(target_os = "windows", feature = "svg"))]
#[path = "state_kanban_present_tests.rs"]
mod kanban_present_tests;

/// Exercise the production frame planner, reactive portal remounts and native
/// presentation, rather than reconstructing node-present calls in a test.
#[cfg(target_os = "windows")]
#[test]
#[ignore = "requires a native Windows GPU surface; run explicitly with --ignored --nocapture"]
fn color_picker_native_frames_keep_page_and_hover_stable() {
    native_popup_frames(false);
}

#[cfg(target_os = "windows")]
#[test]
#[ignore = "requires a native Windows GPU surface; run explicitly with --ignored --nocapture"]
fn calendar_select_native_frames_have_no_shadow_residue() {
    native_popup_frames(true);
}

#[cfg(target_os = "windows")]
fn native_popup_frames(calendar: bool) {
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
                            .with_title("RUI popup frame regression")
                            .with_surface_size(winit::dpi::LogicalSize::new(640.0, 560.0))
                            .with_visible(false),
                    )
                    .expect("hidden test window"),
            );
            run_color_picker_native_frames(window, self.0);
            event_loop.exit();
        }
        fn window_event(&mut self, _: &dyn ActiveEventLoop, _: WindowId, _: WindowEvent) {}
    }
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    builder
        .build()
        .expect("test event loop")
        .run_app(Harness(calendar))
        .unwrap();
}

#[cfg(target_os = "windows")]
fn run_color_picker_native_frames(window: Arc<dyn Window>, calendar: bool) {
    use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
    use crate::app::runtime::{clear_app_runtime, set_app_runtime, AppRuntime};
    use crate::core::reactivity::{create_effect, create_root};
    use crate::renderer::offscreen::RgbaImage;
    use crate::utils::floating_motion::{drain_pending_floating_unmounts, tick_floating_presence};
    use std::{cell::RefCell, rc::Rc};

    fn trigger(el: &Element) -> Option<&Element> {
        if el.focus_id.as_deref() == Some("native-cp:trigger") {
            return Some(el);
        }
        el.children.iter().find_map(trigger)
    }
    fn focus_suffix<'a>(el: &'a Element, suffix: &str) -> Option<&'a Element> {
        if el
            .focus_id
            .as_deref()
            .is_some_and(|id| id.ends_with(suffix))
        {
            return Some(el);
        }
        el.children
            .iter()
            .find_map(|child| focus_suffix(child, suffix))
    }
    fn frame(state: &Rc<RefCell<Option<AppState>>>, mouse: [f32; 2]) -> RgbaImage {
        std::thread::sleep(std::time::Duration::from_millis(16));
        crate::style::advance_css_transition_timeline_ms(16.0);
        crate::style::tick_animations(0.016);
        tick_floating_presence();
        drain_pending_floating_unmounts();
        drain_deferred_view_remounts();
        let mut opt = state.borrow_mut();
        let s = opt.as_mut().unwrap();
        if std::env::var_os("RUI_NATIVE_CP_FULL").is_some() {
            s.mark_full_redraw();
        }
        s.render(
            mouse,
            ScrollbarUpdateMode::ViewportsOnly,
            None,
            false,
            false,
            false,
        )
        .unwrap();
        assert!(
            s.present_retain.contents_valid(),
            "native test needs retained output"
        );
        let texture = s.present_retain.color_texture_for_test().unwrap();
        let mut image = readback_texture(
            &s.device,
            &s.queue,
            texture,
            s.config.width,
            s.config.height,
            s.device.create_command_encoder(&Default::default()),
        )
        .unwrap();
        if matches!(
            texture.format(),
            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
        ) {
            for pixel in image.pixels.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
        }
        if std::env::var_os("RUI_NATIVE_CP_PARTIAL_ONLY").is_none()
            && focus_suffix(&s.root_element, "native-cal:cap_y:trigger").is_none()
        {
            // Freeze both clocks: zero frame dt alone still advances CSS from
            // elapsed wall time while GPU readback is running.
            crate::style::utility::set_frame_dt(0.0);
            s.mark_full_redraw();
            s.render(
                mouse,
                ScrollbarUpdateMode::ViewportsOnly,
                None,
                false,
                false,
                false,
            )
            .unwrap();
            let texture = s.present_retain.color_texture_for_test().unwrap();
            let mut full = readback_texture(
                &s.device,
                &s.queue,
                texture,
                s.config.width,
                s.config.height,
                s.device.create_command_encoder(&Default::default()),
            )
            .unwrap();
            if matches!(
                texture.format(),
                wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
            ) {
                for pixel in full.pixels.chunks_exact_mut(4) {
                    pixel.swap(0, 2);
                }
            }
            crate::style::utility::set_frame_dt(0.016);
            let changed = changed_outside(&image, &full, &[], s.ui_scale());
            if changed > 0 {
                snapshot(&image, "composite-partial");
                snapshot(&full, "composite-full");
            }
            assert_eq!(
                changed, 0,
                "native animation frame must match a full composite at the same progress"
            );
        }
        image
    }
    fn snapshot(image: &RgbaImage, name: &str) {
        let Some(dir) = std::env::var_os("RUI_NATIVE_CP_OUTPUT") else {
            return;
        };
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        ::image::save_buffer(
            dir.join(format!("{name}.png")),
            &image.pixels,
            image.width,
            image.height,
            ::image::ColorType::Rgba8,
        )
        .unwrap();
    }
    fn changed_outside(a: &RgbaImage, b: &RgbaImage, boxes: &[[f32; 4]], scale: f32) -> usize {
        changed_outside_threshold(a, b, boxes, scale, 6)
    }
    fn changed_outside_threshold(
        a: &RgbaImage,
        b: &RgbaImage,
        boxes: &[[f32; 4]],
        scale: f32,
        threshold: u8,
    ) -> usize {
        a.pixels
            .chunks_exact(4)
            .zip(b.pixels.chunks_exact(4))
            .enumerate()
            .filter(|(i, (p, q))| {
                let x = (*i as u32 % a.width) as f32 / scale;
                let y = (*i as u32 / a.width) as f32 / scale;
                !boxes
                    .iter()
                    .any(|r| x >= r[0] && y >= r[1] && x < r[0] + r[2] && y < r[1] + r[3])
                    // Native sRGB edge blending can quantize a one-pixel
                    // antialiased border by a few levels between retained and
                    // full composition. Treat that as equivalent; visible
                    // seams and stale content differ by much more.
                    && p.iter()
                        .zip(q.iter())
                        .any(|(v, w)| v.abs_diff(*w) > threshold)
            })
            .count()
    }
    fn full_composite(state: &Rc<RefCell<Option<AppState>>>, mouse: [f32; 2]) -> RgbaImage {
        let mut opt = state.borrow_mut();
        let s = opt.as_mut().unwrap();
        crate::style::utility::set_frame_dt(0.0);
        s.mark_full_redraw();
        s.render(
            mouse,
            ScrollbarUpdateMode::ViewportsOnly,
            None,
            false,
            false,
            false,
        )
        .unwrap();
        let texture = s.present_retain.color_texture_for_test().unwrap();
        let mut image = readback_texture(
            &s.device,
            &s.queue,
            texture,
            s.config.width,
            s.config.height,
            s.device.create_command_encoder(&Default::default()),
        )
        .unwrap();
        if matches!(
            texture.format(),
            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
        ) {
            for pixel in image.pixels.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
        }
        crate::style::utility::set_frame_dt(0.016);
        image
    }

    crate::app::portal_overlay::clear_overlay_stack();
    reset_registered_anchors();
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    let _ = take_frame_paint_nodes();
    let state = Rc::new(RefCell::new(Some(crate::utils::block_on::block_on(
        AppState::new_bootstrap(
            window,
            super::super::window_chrome::WindowOptions::default(),
        ),
    ))));
    let secondaries = Rc::new(RefCell::new(std::collections::HashMap::new()));
    set_app_runtime(AppRuntime::new(
        Rc::downgrade(&state),
        Rc::downgrade(&secondaries),
    ));
    let (owner, ()) = create_root(|| {
        let live = Rc::clone(&state);
        crate::core::reactivity::mark_next_effect_as_ui_root();
        create_effect(move || {
            let picker: Element = if calendar {
                crate::components::calendar("native-cal")
                    .caption_layout(crate::components::CalendarCaption::Dropdown)
                    .default_value("2026-10-08")
                    .into()
            } else if std::env::var_os("RUI_NATIVE_CP_DATE").is_some() {
                let mut date = crate::components::date_picker("native-cp");
                if std::env::var_os("RUI_NATIVE_CP_DATE_DROPDOWN").is_some() {
                    date = date.caption_layout(crate::components::CalendarCaption::Dropdown);
                }
                date.into()
            } else {
                crate::components::color_picker("native-cp")
                    .show_alpha(false)
                    .into()
            };
            let root = if calendar {
                Element::new().w("100%").h("100%").child(
                    Element::new()
                        .absolute()
                        .left(100.0)
                        .top(44.0)
                        .child(picker),
                )
            } else {
                Element::new()
                    .w("100%")
                    .h("100%")
                    // Real page chrome beneath the fading panel: a flat clear
                    // backdrop misses strips left at borders and calendar rows.
                    .children((0..8).map(|row| {
                        Element::new()
                            .absolute()
                            .left(0.0)
                            .top(100.0 + row as f32 * 44.0)
                            .w(560.0)
                            .h(40.0)
                            .bg(if row % 2 == 0 {
                                [0.96, 0.96, 0.96, 1.0]
                            } else {
                                [1.0, 1.0, 1.0, 1.0]
                            })
                            .border([0.9, 0.9, 0.9, 1.0], 1.0)
                            .rounded(4.0)
                    }))
                    .child(
                        Element::new()
                            .absolute()
                            .left(0.0)
                            .top(0.0)
                            .w(600.0)
                            .h(28.0)
                            .text("Color · date/time 周围文字必须保持稳定")
                            .text_size(20.0),
                    )
                    .child(
                        Element::new()
                            .absolute()
                            .left(100.0)
                            .top(44.0)
                            .child(picker),
                    )
                    .child(
                        Element::new()
                            .absolute()
                            .left(0.0)
                            .top(212.0)
                            .w(600.0)
                            .h(28.0)
                            .text("Calendar · ColorPicker 标题：打开 / 关闭")
                            .text_size(20.0),
                    )
                    .child(
                        Element::new()
                            .absolute()
                            .left(0.0)
                            .top(242.0)
                            .w(600.0)
                            .h(24.0)
                            .text("内置 Calendar / 时间选择，预设颜色与输入框")
                            .text_size(12.0),
                    )
            };
            live.borrow_mut().as_mut().unwrap().rebuild_ui(root);
        });
    });
    crate::style::utility::set_frame_dt(0.016);
    for _ in 0..4 {
        frame(&state, [-1.0, -1.0]);
    }
    if calendar {
        let suffix = ":cap_y:trigger";
        let mouse = {
            let s = state.borrow();
            let b = focus_suffix(&s.as_ref().unwrap().root_element, suffix)
                .unwrap()
                .layout_bounds;
            [b[0] + b[2] * 0.5, b[1] + b[3] * 0.5]
        };
        for _ in 0..16 {
            frame(&state, mouse);
        }
        let mut failures = Vec::new();
        for opening in [true, false] {
            let click = {
                let s = state.borrow();
                focus_suffix(&s.as_ref().unwrap().root_element, suffix)
                    .and_then(|el| el.on_click.clone())
                    .unwrap()
            };
            click.borrow_mut()();
            for tick in 0..24 {
                let image = frame(&state, mouse);
                snapshot(&image, &format!("calendar-{opening}-{tick}-partial"));
                if tick == 23 {
                    let full = full_composite(&state, mouse);
                    let scale = state.borrow().as_ref().unwrap().ui_scale();
                    let changed = changed_outside_threshold(&image, &full, &[], scale, 20);
                    snapshot(&full, &format!("calendar-{opening}-{tick}-full"));
                    if changed > 0 {
                        failures.push((opening, tick, changed));
                    }
                }
            }
        }
        clear_app_runtime();
        owner.dispose();
        crate::style::utility::set_frame_dt(0.0);
        crate::style::set_css_transition_virtual_timeline(false);
        crate::app::portal_overlay::clear_overlay_stack();
        reset_registered_anchors();
        assert!(
            failures.is_empty(),
            "calendar select differs from full composite: {failures:?}"
        );
        return;
    }
    let (mouse, scale) = {
        let s = state.borrow();
        let s = s.as_ref().unwrap();
        let r = trigger(&s.root_element)
            .expect("picker trigger")
            .layout_bounds;
        ([r[0] + r[2] * 0.5, r[1] + r[3] * 0.5], s.ui_scale())
    };
    crate::utils::roving::focus_id_only("native-cp:trigger");
    let mut baseline = frame(&state, mouse);
    for _ in 0..16 {
        baseline = frame(&state, mouse);
    }
    snapshot(&baseline, "baseline");
    let mut failures = Vec::new();
    let mut hover_failures = Vec::new();
    for cycle in 0..2 {
        for opening in [true, false] {
            let click = {
                let s = state.borrow();
                trigger(&s.as_ref().unwrap().root_element)
                    .unwrap()
                    .on_click
                    .clone()
                    .unwrap()
            };
            click.borrow_mut()();
            for tick in 0..14 {
                let previous = { state.borrow().as_ref().unwrap().last_overlay_boxes.clone() };
                let image = frame(&state, mouse);
                let mut excluded = previous;
                let current = { state.borrow().as_ref().unwrap().last_overlay_boxes.clone() };
                excluded.extend(current);
                // Shadows may lie beyond the portal's occupied content box.
                for r in &mut excluded {
                    r[0] -= 16.0;
                    r[1] -= 16.0;
                    r[2] += 32.0;
                    r[3] += 32.0;
                }
                let changed = changed_outside(&baseline, &image, &excluded, scale);
                let hovered = {
                    let opt = state.borrow();
                    trigger(&opt.as_ref().unwrap().root_element)
                        .unwrap()
                        .class_set
                        .as_ref()
                        .and_then(|c| c.last_pointer_hover)
                };
                if hovered != Some(true) {
                    hover_failures.push((cycle, opening, tick, hovered));
                }
                eprintln!("native cp cycle={cycle} opening={opening} tick={tick} outside={changed} boxes={excluded:?}");
                if cycle == 0 {
                    snapshot(&image, &format!("open-{opening}-tick-{tick}"));
                }
                if std::env::var_os("RUI_NATIVE_CP_TRACE").is_some() {
                    let opt = state.borrow();
                    let s = opt.as_ref().unwrap();
                    let t = trigger(&s.root_element).unwrap();
                    eprintln!(
                        "trigger bounds={:?} hover={:?} text={} damage={:?}",
                        t.layout_bounds,
                        t.class_set.as_ref().and_then(|c| c.last_pointer_hover),
                        s.text_nodes.len(),
                        s.damage_nodes
                    );
                }
                if changed > 0 {
                    failures.push((cycle, opening, tick, changed));
                }
            }
        }
        if cycle == 0 && std::env::var_os("RUI_NATIVE_CP_DATE_DROPDOWN").is_some() {
            // Re-open the outer DatePicker; the nested caption Select only
            // exists while that portal is mounted.
            let outer_click = {
                let s = state.borrow();
                trigger(&s.as_ref().unwrap().root_element)
                    .and_then(|el| el.on_click.clone())
                    .expect("DatePicker outer trigger reopen")
            };
            outer_click.borrow_mut()();
            for _ in 0..14 {
                let _ = frame(&state, mouse);
            }
            let year_suffix = ":cap_y:trigger";
            let (year_mouse, nested_scale) = {
                let s = state.borrow();
                let year = focus_suffix(&s.as_ref().unwrap().root_element, year_suffix)
                    .unwrap_or_else(|| panic!("DatePicker year trigger *{year_suffix} not found"));
                let b = year.layout_bounds;
                (
                    [b[0] + b[2] * 0.5, b[1] + b[3] * 0.5],
                    s.as_ref().unwrap().ui_scale(),
                )
            };
            let _ = frame(&state, year_mouse);
            let year_click = {
                let s = state.borrow();
                focus_suffix(&s.as_ref().unwrap().root_element, year_suffix)
                    .and_then(|el| el.on_click.clone())
                    .expect("DatePicker year trigger click")
            };
            year_click.borrow_mut()();
            for tick in 0..14 {
                let image = frame(&state, year_mouse);
                let full = full_composite(&state, year_mouse);
                let same_frame = changed_outside_threshold(&image, &full, &[], nested_scale, 20);
                assert_eq!(
                    same_frame, 0,
                    "nested year menu opening retained frame differs from full composite at tick {tick}: {same_frame}"
                );
            }
            let year_close = {
                let s = state.borrow();
                focus_suffix(&s.as_ref().unwrap().root_element, year_suffix)
                    .and_then(|el| el.on_click.clone())
                    .expect("DatePicker year trigger close")
            };
            year_close.borrow_mut()();
            // Nested Select expand-up is 200ms and may be seeded one frame
            // after the click; keep sampling past that before the parent settles.
            for tick in 0..24 {
                let image = frame(&state, year_mouse);
                let full = full_composite(&state, year_mouse);
                let same_frame = changed_outside_threshold(&image, &full, &[], nested_scale, 20);
                assert_eq!(
                    same_frame, 0,
                    "nested year menu closing retained frame differs from full composite at tick {tick}: {same_frame}"
                );
            }
            let outer_close = {
                let s = state.borrow();
                trigger(&s.as_ref().unwrap().root_element)
                    .and_then(|el| el.on_click.clone())
                    .expect("DatePicker outer trigger close")
            };
            outer_close.borrow_mut()();
            for _ in 0..14 {
                let _ = frame(&state, mouse);
            }
        }
        // A settled closed frame must match a fresh full render, including hover caption.
        let partial = frame(&state, mouse);
        state.borrow_mut().as_mut().unwrap().mark_full_redraw();
        let full = frame(&state, mouse);
        let changed = changed_outside(&partial, &full, &[], scale);
        eprintln!("native cp closed full diff cycle={cycle}: {changed}");
        if changed > 0 {
            failures.push((cycle, false, 99, changed));
        }
        baseline = full;
    }
    clear_app_runtime();
    owner.dispose();
    crate::style::utility::set_frame_dt(0.0);
    crate::style::set_css_transition_virtual_timeline(false);
    crate::app::portal_overlay::clear_overlay_stack();
    reset_registered_anchors();
    assert!(
        hover_failures.is_empty(),
        "popup remount changed stationary trigger hover: {hover_failures:?}"
    );
    assert!(
        failures.is_empty(),
        "native popup frames changed retained page/trigger pixels: {failures:?}"
    );
}

#[test]
#[ignore = "requires a GPU adapter; run explicitly with --ignored --nocapture"]
fn avatar_interrupted_expand_restores_caption_each_frame() {
    use crate as rui;
    use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
    use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
    use crate::core::reactivity::{create_effect, create_root, isolate_reads};
    use crate::layout::layout_tree::{hoist_portal_subtrees, hoisted_portal_stamps};
    use crate::utils::floating_motion::{
        drain_pending_floating_unmounts, flush_floating_motion_subtree,
        mark_floating_panels_layout_dirty, tick_floating_presence, tick_hoisted_floating_css,
    };
    use crate::utils::popup::sync_floating_placement;
    use std::{cell::RefCell, rc::Rc};

    crate::app::portal_overlay::clear_overlay_stack();
    reset_registered_anchors();
    clear_test_reconcile_root();
    let live = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&live));
    let (owner, ()) = create_root(|| {
        let live = Rc::clone(&live);
        create_effect(move || {
            *live.borrow_mut() = isolate_reads(|| {
                use crate::components::*;
                let group = crate::ui! {
                    <AvatarGroup>
                        <Avatar><AvatarFallback>"A"</AvatarFallback></Avatar>
                        <Avatar><AvatarFallback>"B"</AvatarFallback></Avatar>
                        <Avatar><AvatarFallback>"C"</AvatarFallback></Avatar>
                        <AvatarGroupCount>
                            "+3"
                            <Avatar><AvatarFallback>"D"</AvatarFallback></Avatar>
                            <Avatar><AvatarFallback>"E"</AvatarFallback></Avatar>
                            <Avatar><AvatarFallback>"F"</AvatarFallback></Avatar>
                        </AvatarGroupCount>
                    </AvatarGroup>
                };
                let mut caption = Element::new().w(280.0).h(32.0).text_size(11.2)
                .text("悬停 / 点击 +N：原地向下盖住展开（expand_placement / expand_class / max_visible 可配）");
                caption.text_style.color = Some([100, 100, 100, 255]);
                Element::new()
                    .w(320.0)
                    .h(200.0)
                    .child(
                        Element::new()
                            .absolute()
                            .left(16.25)
                            .top(30.25)
                            .w(280.0)
                            .h(32.0)
                            .child(caption),
                    )
                    .child(
                        Element::new()
                            .absolute()
                            .left(16.25)
                            .top(64.25)
                            .w(280.0)
                            .child(group.w(104.0).min_w(104.0)),
                    )
            });
        });
    });
    drain_deferred_view_remounts();
    fn find_count(el: &Element) -> Option<Rc<RefCell<dyn FnMut()>>> {
        if el.attrs.get("data-slot").map(String::as_str) == Some("avatar-group-count") {
            if let Some(click) = &el.on_click {
                return Some(Rc::clone(click));
            }
        }
        el.children.iter().find_map(find_count)
    }
    let gpu = OffscreenGpu::new().expect("GPU adapter");
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        width: 320,
        height: 200,
        present_mode: wgpu::PresentMode::Fifo,
        alpha_mode: wgpu::CompositeAlphaMode::Opaque,
        color_space: wgpu::SurfaceColorSpace::Auto,
        view_formats: vec![],
        desired_maximum_frame_latency: 1,
    };
    let pipeline = RuiPipeline::from_format(&gpu.device, config.format, 320, 200);
    pipeline.update_resolution(&gpu.queue, 320, 200);
    let make_target = || {
        gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Interrupted AvatarGroup animation"),
            size: wgpu::Extent3d {
                width: 320,
                height: 200,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
    };
    let retained = make_target();
    let reference = make_target();
    let mut tree = LayoutTree::new();
    let mut previous: Vec<(u64, [f32; 4])> = Vec::new();
    let mut previous_visual = None;
    clear_frame_paint_nodes();
    for frame in 0..24 {
        if frame == 1 {
            let click = find_count(&live.borrow()).expect("+N click");
            click.borrow_mut()();
        }
        if frame == 3 {
            assert!(
                previous.iter().any(|(_, b)| b[2] > 122.0 && b[2] < 230.0),
                "must close during expansion, before full width: {previous:?}"
            );
            assert!(crate::components::avatar::dismiss_avatar_group_expands_on_scroll());
        }
        tick_floating_presence();
        drain_pending_floating_unmounts();
        drain_deferred_view_remounts();
        crate::style::utility::set_frame_dt(0.016);
        let (scene, stamps) = {
            let mut root = live.borrow_mut();
            hoist_portal_subtrees(&mut root);
            let mut scene = tree.build_and_layout(&mut root, 320.0, 200.0, [-1.0, -1.0]);
            if sync_floating_placement(&mut root, 320.0, 200.0) {
                scene = tree.build_and_layout_fast_ex(
                    &mut root,
                    320.0,
                    200.0,
                    [-1.0, -1.0],
                    false,
                    false,
                    None,
                );
            }
            let motion = flush_floating_motion_subtree(&mut root);
            let ticking = tick_hoisted_floating_css(&mut root);
            if motion || ticking {
                mark_floating_panels_layout_dirty(&mut root, &mut tree);
                scene = tree.build_and_layout_fast_ex(
                    &mut root,
                    320.0,
                    200.0,
                    [-1.0, -1.0],
                    false,
                    false,
                    None,
                );
            }
            (scene, hoisted_portal_stamps(&root))
        };
        let visual = AppState::overlay_visual_bounds_logical(
            &scene.instances[scene.overlay_split.instance_start..],
            &[],
            &scene.text_nodes[scene.overlay_split.text_start..],
            &[],
            1.0,
        );
        let mut dirties: Vec<_> = take_frame_paint_nodes()
            .into_iter()
            .filter(|d| d.kind == PaintDirtyKind::Overlay)
            .collect();
        for (key, stamp) in overlay_membership_paint_stamps(&previous, &stamps) {
            dirties.push(PaintDirty {
                key,
                bounds: stamp,
                kind: PaintDirtyKind::Overlay,
            });
        }
        let overlay_dirties = dirties.clone();
        for dirty in &overlay_dirties {
            for hole in
                overlay_mounted_replace_holes(&previous, &stamps, dirty.key, Some(dirty.bounds))
            {
                dirties.push(PaintDirty {
                    key: dirty.key,
                    bounds: hole,
                    kind: PaintDirtyKind::OverlayRestore,
                });
            }
        }
        let replay: Vec<_> = overlay_dirties
            .iter()
            .map(|d| (d.key, overlay_suffix_encode_bounds(d.bounds)))
            .collect();
        for (keys, bounds) in merge_overlay_replay_dirties(&replay, &stamps) {
            let key = keys[0];
            if let Some(hole) = overlay_replay_restore_bounds(key, bounds, &stamps, visual) {
                dirties.push(PaintDirty {
                    key,
                    bounds: hole,
                    kind: PaintDirtyKind::OverlayRestore,
                });
            }
        }
        for (key, hole) in overlay_restore_leftover_coverage(&previous, &stamps, previous_visual) {
            dirties.push(PaintDirty {
                key,
                bounds: hole,
                kind: PaintDirtyKind::OverlayRestore,
            });
        }
        let (runs, flush) = overlay_grazing_caption_replay(
            &dirties,
            &scene.node_paints,
            &stamps,
            visual,
            &scene.text_nodes,
            1.0,
        );
        if frame == 2 {
            assert!(
                !stamps.is_empty(),
                "test must interrupt a visible entering portal"
            );
        }
        let mut holes = Vec::new();
        let mut overlay_holes = Vec::new();
        let mut clear_rects: Vec<_> = dirties
            .iter()
            .filter(|d| d.kind == PaintDirtyKind::OverlayRestore)
            .map(|d| (false, d.bounds, visual.or(Some(d.bounds))))
            .collect();
        for (keys, bounds) in merge_overlay_replay_dirties(&replay, &stamps) {
            let occupancy = overlay_encode_occupancy(keys[0], Some(bounds), &stamps, visual);
            if let Some(hole) = overlay_encode_scissor(Some(bounds), occupancy, None) {
                clear_rects.push((true, hole, occupancy));
            }
        }
        for (is_overlay, hole, shadow) in clear_rects {
            let pieces = scissors_keeping_stable_page_captions(
                vec![hole],
                &scene.node_paints,
                &stamps,
                shadow,
            );
            for piece in pieces {
                let (x, y, w, h) = damage_to_scissor_physical(piece, 0.0, 1.0, 320, 200).unwrap();
                let physical = [x as f32, y as f32, w as f32, h as f32];
                if is_overlay {
                    push_disjoint_rect(&mut overlay_holes, physical);
                } else {
                    push_disjoint_rect(&mut holes, physical);
                }
            }
        }
        let page_end = scene.overlay_split.text_start.min(scene.text_nodes.len());
        let page = &scene.text_nodes[..page_end];
        let make_renderer = |text: &[crate::layout::layout_tree::TextLayoutInfo]| {
            let mut renderer = RuiTextRenderer::new(&gpu.device, &gpu.queue, &config);
            crate::renderer::prepare_text::prepare_text_frame_at(
                &mut renderer,
                &gpu.device,
                &gpu.queue,
                320,
                200,
                text,
                1.0,
                false,
                &Default::default(),
                &Default::default(),
            );
            renderer
        };
        let repair = make_renderer(&runs);
        let page_renderer = make_renderer(&clip_text_nodes_to_cull(
            &page_runs_excluding_caption_replay(page.to_vec(), &runs),
            CullRects::from_rects(holes.iter().copied()).unwrap_or(CullRects::empty()),
        ));
        let full_page = make_renderer(page);
        let overlay = make_renderer(&scene.text_nodes[page_end..]);
        let draw = |target: &wgpu::Texture, full: bool| {
            let view = target.create_view(&Default::default());
            let mut encoder = gpu.device.create_command_encoder(&Default::default());
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: if full {
                                wgpu::LoadOp::Clear(wgpu::Color::WHITE)
                            } else {
                                wgpu::LoadOp::Load
                            },
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                });
                let page_holes = if full {
                    vec![[0.0, 0.0, 320.0, 200.0]]
                } else {
                    holes.clone()
                };
                for hole in &page_holes {
                    let (x, y, w, h) =
                        damage_to_scissor_physical(*hole, 0.0, 1.0, 320, 200).unwrap();
                    pass.set_scissor_rect(x, y, w, h);
                    if !full {
                        draw_scissor_fill(
                            &mut pass,
                            &pipeline,
                            &gpu.device,
                            *hole,
                            wgpu::Color::WHITE,
                        );
                    }
                    draw_sdf_range_with_clip(
                        &mut pass,
                        &pipeline,
                        &gpu.device,
                        pipeline.dummy_backdrop(),
                        &scene.instances,
                        &scene.clip_polys,
                        0,
                        scene.overlay_split.instance_start as u32,
                        None,
                    );
                    if full {
                        full_page.render(&mut pass);
                    } else {
                        page_renderer.render(&mut pass);
                    }
                }
                if !full {
                    let physical_flush: Vec<_> = flush
                        .iter()
                        .filter_map(|h| {
                            damage_to_scissor_physical(*h, 0.0, 1.0, 320, 200)
                                .map(|(x, y, w, h)| [x as f32, y as f32, w as f32, h as f32])
                        })
                        .collect();
                    draw_captured_page_glyphs(&repair, &mut pass, 320, 200, &physical_flush, &[]);
                }
                for hole in &overlay_holes {
                    let (x, y, w, h) =
                        damage_to_scissor_physical(*hole, 0.0, 1.0, 320, 200).unwrap();
                    pass.set_scissor_rect(x, y, w, h);
                    draw_sdf_range_with_clip(
                        &mut pass,
                        &pipeline,
                        &gpu.device,
                        pipeline.dummy_backdrop(),
                        &scene.instances,
                        &scene.clip_polys,
                        scene.overlay_split.instance_start as u32,
                        (scene.instances.len() - scene.overlay_split.instance_start) as u32,
                        None,
                    );
                    overlay.render(&mut pass);
                }
            }
            gpu.queue.submit(Some(encoder.finish()));
        };
        draw(&retained, frame == 0);
        draw(&reference, true);
        let read = |target| {
            readback_texture(
                &gpu.device,
                &gpu.queue,
                target,
                320,
                200,
                gpu.device.create_command_encoder(&Default::default()),
            )
            .unwrap()
        };
        let actual = read(&retained);
        let expected = read(&reference);
        let diffs: Vec<_> = actual
            .pixels
            .chunks_exact(4)
            .zip(expected.pixels.chunks_exact(4))
            .enumerate()
            .filter(|(i, (a, b))| i / 320 < 62 && a.iter().zip(*b).any(|(x, y)| x.abs_diff(*y) > 2))
            .take(12)
            .map(|(i, (a, b))| (i % 320, i / 320, a, b))
            .collect();
        assert!(diffs.is_empty(), "interrupted avatar frame={frame}: caption differs; stamps={stamps:?} holes={holes:?} flush={flush:?} runs={:?} diffs={diffs:?}",runs.iter().map(|n|(&n.text,n.clip_bounds)).collect::<Vec<_>>());
        previous = stamps;
        previous_visual = visual;
    }
    assert!(
        previous.is_empty(),
        "exit must finish within the driven frames"
    );
    crate::style::utility::set_frame_dt(0.0);
    clear_test_reconcile_root();
    drop(live);
    owner.dispose();
    crate::app::portal_overlay::clear_overlay_stack();
    reset_registered_anchors();
}

#[test]
#[ignore = "requires a GPU adapter; run explicitly with --ignored --nocapture"]
fn avatar_caption_open_close_matches_full_composite() {
    avatar_caption_composite(false);
}

#[test]
#[ignore = "requires a GPU adapter; run explicitly with --ignored --nocapture"]
fn avatar_shrinking_overlay_restore_matches_full_composite() {
    avatar_caption_composite(true);
}

fn avatar_caption_composite(shrink: bool) {
    let gpu = OffscreenGpu::new().expect("GPU adapter");
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        width: 320,
        height: 200,
        present_mode: wgpu::PresentMode::Fifo,
        alpha_mode: wgpu::CompositeAlphaMode::Opaque,
        color_space: wgpu::SurfaceColorSpace::Auto,
        view_formats: vec![],
        desired_maximum_frame_latency: 1,
    };
    let pipeline = RuiPipeline::from_format(&gpu.device, config.format, 320, 200);
    pipeline.update_resolution(&gpu.queue, 320, 200);
    let make_target = || {
        gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Avatar caption composite"),
            size: wgpu::Extent3d {
                width: 320,
                height: 200,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
    };
    let retained = make_target();
    let reference = make_target();
    // Both the caption wrapper and its leaf own the same text range, as in
    // the gallery. Its last rows overlap the expanded avatar panel.
    let mut root = Element::new().w(320.0).h(200.0).child(
        Element::new()
            .absolute()
            .left(16.0)
            .top(40.0)
            .w(280.0)
            .h(28.0)
            .child(
                Element::new()
                    .w(280.0)
                    .h(28.0)
                    .text_size(18.0)
                    .text(if shrink {
                        "原地向下 expand_placement"
                    } else {
                        "原地向下盖住展开 expand_placement"
                    }),
            ),
    );
    root.children[0].children[0].text_style.color = Some([0, 0, 0, 255]);
    let scene = LayoutTree::new().build_and_layout(&mut root, 320.0, 200.0, [-1.0, -1.0]);
    let stamp = [16.0, 54.0, 220.0, 40.0];
    let hole = [0.0, 30.0, 280.0, 90.0];
    let mut panel_root = Element::new().w(320.0).h(200.0).child(
        Element::new()
            .absolute()
            .left(stamp[0])
            .top(stamp[1])
            .w(stamp[2])
            .h(stamp[3])
            .bg([1.0; 4]),
    );
    let panel = LayoutTree::new().build_and_layout(&mut panel_root, 320.0, 200.0, [-1.0, -1.0]);
    let draw = |target: &wgpu::Texture,
                panel: &crate::layout::layout_tree::RenderResult,
                open: bool,
                clear: bool,
                caption: Option<&RuiTextRenderer>,
                flush: &[[f32; 4]]| {
        let view = target.create_view(&Default::default());
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            if clear {
                let (x, y, w, h) = damage_to_scissor_physical(hole, 0.0, 1.0, 320, 200).unwrap();
                pass.set_scissor_rect(x, y, w, h);
                draw_scissor_fill(&mut pass, &pipeline, &gpu.device, hole, wgpu::Color::WHITE);
            }
            if let Some(caption) = caption {
                let physical_flush: Vec<[f32; 4]> = flush
                    .iter()
                    .filter_map(|logical| {
                        damage_to_scissor_physical(*logical, 0.0, 1.0, 320, 200)
                            .map(|(x, y, w, h)| [x as f32, y as f32, w as f32, h as f32])
                    })
                    .collect();
                draw_captured_page_glyphs(caption, &mut pass, 320, 200, &physical_flush, &[]);
            }
            if open {
                // Production resets the scissor for each overlay suffix piece;
                // the last disjoint caption repair must not clip the panel.
                pass.set_scissor_rect(0, 0, 320, 200);
                draw_sdf_range_with_clip(
                    &mut pass,
                    &pipeline,
                    &gpu.device,
                    pipeline.dummy_backdrop(),
                    &panel.instances,
                    &panel.clip_polys,
                    0,
                    panel.instances.len() as u32,
                    None,
                );
            }
        }
        gpu.queue.submit(Some(encoder.finish()));
    };
    draw_latest_scene_for_test(&gpu, &config, &pipeline, &retained, &scene, None);
    let mut caption = RuiTextRenderer::new(&gpu.device, &gpu.queue, &config);
    for (frame, open) in [true, true, false, true, false].into_iter().enumerate() {
        let shrinking = shrink && open && frame > 0;
        let live_stamp = if shrinking {
            [16.0, 54.0, 118.0, 40.0]
        } else {
            stamp
        };
        let stamps = if open { vec![(9, live_stamp)] } else { vec![] };
        let live_hole = if shrinking {
            [0.0, 30.0, 160.0, 90.0]
        } else {
            hole
        };
        let dirty = PaintDirty {
            key: 9,
            bounds: live_hole,
            kind: if open {
                PaintDirtyKind::Overlay
            } else {
                PaintDirtyKind::OverlayRestore
            },
        };
        // ExpandStacked's closing animation still encodes the smaller live
        // portal while OverlayRestore clears its previous, wider footprint.
        // Production drops this caption from the ordinary page/capture mesh
        // once it is selected for grazing replay, so that replay must repair
        // both clear regions, including the strip past the live panel.
        let mut dirties = vec![dirty];
        if shrinking {
            dirties.push(PaintDirty {
                key: 9,
                bounds: hole,
                kind: PaintDirtyKind::OverlayRestore,
            });
        }
        let (runs, flush) = overlay_grazing_caption_replay(
            &dirties,
            &scene.node_paints,
            &stamps,
            Some(live_hole),
            &scene.text_nodes,
            1.0,
        );
        assert!(
            !runs.is_empty(),
            "fixture must exercise grazing caption replay"
        );
        crate::renderer::prepare_text::prepare_text_frame_at(
            &mut caption,
            &gpu.device,
            &gpu.queue,
            320,
            200,
            &runs,
            1.0,
            false,
            &Default::default(),
            &Default::default(),
        );
        let mut shrinking_panel_root = Element::new().w(320.0).h(200.0).child(
            Element::new()
                .absolute()
                .left(live_stamp[0])
                .top(live_stamp[1])
                .w(live_stamp[2])
                .h(live_stamp[3])
                .bg([1.0; 4]),
        );
        let shrinking_panel = LayoutTree::new().build_and_layout(
            &mut shrinking_panel_root,
            320.0,
            200.0,
            [-1.0, -1.0],
        );
        let live_panel = if shrinking { &shrinking_panel } else { &panel };
        draw(&retained, live_panel, open, true, Some(&caption), &flush);
        draw_latest_scene_for_test(&gpu, &config, &pipeline, &reference, &scene, None);
        draw(&reference, live_panel, open, false, None, &[]);
        let read = |t| {
            readback_texture(
                &gpu.device,
                &gpu.queue,
                t,
                320,
                200,
                gpu.device.create_command_encoder(&Default::default()),
            )
            .unwrap()
        };
        let actual = read(&retained);
        let expected = read(&reference);
        if shrinking {
            assert!(
                expected
                    .pixels
                    .chunks_exact(4)
                    .enumerate()
                    .any(|(i, p)| i % 320 > 180
                        && i % 320 < 270
                        && i / 320 >= 40
                        && i / 320 < 68
                        && p[0] < 200),
                "fixture must contain visible caption ink beyond the shrinking portal"
            );
        }
        if !images_match(&actual, &expected, 2) {
            let mut mismatches = 0usize;
            let mut bounds = [320u32, 200u32, 0u32, 0u32];
            let mut first = None;
            for (index, (a, b)) in actual
                .pixels
                .chunks_exact(4)
                .zip(expected.pixels.chunks_exact(4))
                .enumerate()
            {
                if a.iter().zip(b).any(|(x, y)| x.abs_diff(*y) > 2) {
                    mismatches += 1;
                    let x = (index as u32) % 320;
                    let y = (index as u32) / 320;
                    bounds[0] = bounds[0].min(x);
                    bounds[1] = bounds[1].min(y);
                    bounds[2] = bounds[2].max(x + 1);
                    bounds[3] = bounds[3].max(y + 1);
                    first.get_or_insert((x, y, a.to_vec(), b.to_vec()));
                }
            }
            panic!(
                "avatar caption frame={frame} open={open}: repeated or occluding page glyphs; runs={}, mismatches={}, bounds={bounds:?}, first={first:?}",
                runs.len(),
                mismatches
            );
        }
    }
}

#[test]
#[ignore = "requires a GPU adapter; run explicitly with --ignored --nocapture"]
fn avatar_wrapped_caption_edge_open_close_matches_full_composite() {
    let gpu = OffscreenGpu::new().expect("GPU adapter");
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        width: 320,
        height: 200,
        present_mode: wgpu::PresentMode::Fifo,
        alpha_mode: wgpu::CompositeAlphaMode::Opaque,
        color_space: wgpu::SurfaceColorSpace::Auto,
        view_formats: vec![],
        desired_maximum_frame_latency: 1,
    };
    let pipeline = RuiPipeline::from_format(&gpu.device, config.format, 320, 200);
    pipeline.update_resolution(&gpu.queue, 320, 200);
    let make_target = || {
        gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Avatar wrapped caption composite"),
            size: wgpu::Extent3d {
                width: 320,
                height: 200,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
    };
    let retained = make_target();
    let reference = make_target();
    let text = "悬停 / 点击 +N：原地向下盖住展开";
    let mut root = Element::new().w(320.0).h(200.0).child(
        Element::new()
            .absolute()
            .left(16.0)
            .top(44.25)
            .w(280.0)
            .h(20.0)
            .child(Element::new().w(280.0).h(20.0).text_size(18.0).text(text)),
    );
    let scene = LayoutTree::new().build_and_layout(&mut root, 320.0, 200.0, [-1.0, -1.0]);
    let stamp = [16.25, 51.35, 220.0, 20.2];
    let hole = overlay_encode_scissor(Some(stamp), Some(stamp), None).unwrap();
    let mut panel_root = Element::new().w(320.0).h(200.0).child(
        Element::new()
            .absolute()
            .left(stamp[0])
            .top(stamp[1])
            .w(stamp[2])
            .h(stamp[3])
            .bg([1.0; 4]),
    );
    let panel = LayoutTree::new().build_and_layout(&mut panel_root, 320.0, 200.0, [-1.0, -1.0]);
    let draw = |target: &wgpu::Texture,
                open: bool,
                clear: bool,
                caption: Option<&RuiTextRenderer>,
                flush: &[[f32; 4]]| {
        let view = target.create_view(&Default::default());
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            if clear {
                // Open encoding clears the padded overlay suffix; closing
                // OverlayRestore clears the raw visual bounds.
                let clear_rect = if open { hole } else { stamp };
                let (x, y, w, h) =
                    damage_to_scissor_physical(clear_rect, 0.0, 1.0, 320, 200).unwrap();
                pass.set_scissor_rect(x, y, w, h);
                draw_scissor_fill(
                    &mut pass,
                    &pipeline,
                    &gpu.device,
                    clear_rect,
                    wgpu::Color::WHITE,
                );
            }
            if let Some(caption) = caption {
                let physical_flush: Vec<[f32; 4]> = flush
                    .iter()
                    .filter_map(|logical| {
                        damage_to_scissor_physical(*logical, 0.0, 1.0, 320, 200)
                            .map(|(x, y, w, h)| [x as f32, y as f32, w as f32, h as f32])
                    })
                    .collect();
                draw_captured_page_glyphs(caption, &mut pass, 320, 200, &physical_flush, &[]);
            }
            if open {
                draw_sdf_range_with_clip(
                    &mut pass,
                    &pipeline,
                    &gpu.device,
                    pipeline.dummy_backdrop(),
                    &panel.instances,
                    &panel.clip_polys,
                    0,
                    panel.instances.len() as u32,
                    None,
                );
            }
        }
        gpu.queue.submit(Some(encoder.finish()));
    };
    draw_latest_scene_for_test(&gpu, &config, &pipeline, &retained, &scene, None);
    let mut caption = RuiTextRenderer::new(&gpu.device, &gpu.queue, &config);
    for (frame, open) in [true, true, false, true, false].into_iter().enumerate() {
        let stamps = if open { vec![(9, stamp)] } else { vec![] };
        let dirty = PaintDirty {
            key: 9,
            bounds: stamp,
            kind: if open {
                PaintDirtyKind::Overlay
            } else {
                PaintDirtyKind::OverlayRestore
            },
        };
        let (runs, flush) = overlay_grazing_caption_replay(
            &[dirty],
            &scene.node_paints,
            &stamps,
            Some(stamp),
            &scene.text_nodes,
            1.0,
        );
        assert!(
            !runs.is_empty(),
            "wrapped caption must be replayed: paints={:?}, text={:?}",
            scene
                .node_paints
                .iter()
                .map(|p| (p.bounds, p.text, p.overlay))
                .collect::<Vec<_>>(),
            scene
                .text_nodes
                .iter()
                .map(|n| (n.x, n.y, n.width, n.height, n.text.clone()))
                .collect::<Vec<_>>()
        );
        crate::renderer::prepare_text::prepare_text_frame_at(
            &mut caption,
            &gpu.device,
            &gpu.queue,
            320,
            200,
            &runs,
            1.0,
            false,
            &Default::default(),
            &Default::default(),
        );
        draw(&retained, open, true, Some(&caption), &flush);
        draw_latest_scene_for_test(&gpu, &config, &pipeline, &reference, &scene, None);
        draw(&reference, open, false, None, &[]);
        let read = |target| {
            readback_texture(
                &gpu.device,
                &gpu.queue,
                target,
                320,
                200,
                gpu.device.create_command_encoder(&Default::default()),
            )
            .unwrap()
        };
        let actual = read(&retained);
        let expected = read(&reference);
        if !images_match(&actual, &expected, 2) {
            let mut mismatches = 0usize;
            let mut bounds = [320u32, 200u32, 0u32, 0u32];
            let mut first = None;
            for (index, (a, b)) in actual
                .pixels
                .chunks_exact(4)
                .zip(expected.pixels.chunks_exact(4))
                .enumerate()
            {
                if a.iter().zip(b).any(|(x, y)| x.abs_diff(*y) > 2) {
                    mismatches += 1;
                    let x = (index as u32) % 320;
                    let y = (index as u32) / 320;
                    bounds[0] = bounds[0].min(x);
                    bounds[1] = bounds[1].min(y);
                    bounds[2] = bounds[2].max(x + 1);
                    bounds[3] = bounds[3].max(y + 1);
                    first.get_or_insert((x, y, a.to_vec(), b.to_vec()));
                }
            }
            panic!(
                "wrapped avatar caption frame={frame} open={open} differs: runs={:?}, flush={flush:?}, mismatches={mismatches}, bounds={bounds:?}, first={first:?}",
                runs.iter().map(|n| n.clip_bounds).collect::<Vec<_>>()
            );
        }
    }
}

/// A node capture contains the whole caption, even when the actual repair has
/// separate holes. Retained antialiased edges must never be blended again.
#[test]
#[ignore = "requires a GPU adapter; run explicitly with --ignored --nocapture"]
fn trigger_capture_only_repaints_cleared_pixels() {
    let gpu = OffscreenGpu::new().expect("GPU adapter");
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        width: 320,
        height: 200,
        present_mode: wgpu::PresentMode::Fifo,
        alpha_mode: wgpu::CompositeAlphaMode::Opaque,
        color_space: wgpu::SurfaceColorSpace::Auto,
        view_formats: vec![],
        desired_maximum_frame_latency: 1,
    };
    let pipeline = RuiPipeline::from_format(&gpu.device, config.format, 320, 200);
    pipeline.update_resolution(&gpu.queue, 320, 200);
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("trigger capture retain"),
        size: wgpu::Extent3d {
            width: 320,
            height: 200,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: config.format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let mut renderer = RuiTextRenderer::new(&gpu.device, &gpu.queue, &config);
    for (label, color, clear) in [
        ("关闭 RSX Trigger", [255, 255, 255, 255], wgpu::Color::BLACK),
        ("关闭 Builder Trigger", [0, 0, 0, 255], wgpu::Color::WHITE),
    ] {
        let mut root = Element::new().w(320.0).h(200.0).child(
            Element::new()
                .absolute()
                .left(18.25)
                .top(46.25)
                .text(label)
                .text_size(16.0)
                .text_color(color),
        );
        let scene = LayoutTree::new().build_and_layout(&mut root, 320.0, 200.0, [-1.0, -1.0]);
        let prep = crate::renderer::prepare_text::prepare_text_frame_at(
            &mut renderer,
            &gpu.device,
            &gpu.queue,
            320,
            200,
            &scene.text_nodes,
            1.0,
            false,
            &Default::default(),
            &Default::default(),
        );
        assert!(!prep.culled_skip_prepare);
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            renderer.render(&mut pass);
        }
        let full = readback_texture(&gpu.device, &gpu.queue, &texture, 320, 200, encoder).unwrap();
        assert!(
            full.pixels
                .chunks_exact(4)
                .any(|p| p[0] != (clear.r * 255.0) as u8),
            "fixture needs glyph ink"
        );
        // Two repairs overlap by a device pixel after outward rounding. The
        // rest of the caption is retained, as on a split portal-halo repair.
        let logical = [[18.25, 46.25, 21.0, 25.0], [39.25, 46.25, 18.0, 25.0]];
        let cleared: Vec<_> = logical
            .into_iter()
            .map(|r| {
                let (x, y, w, h) = damage_to_scissor_physical(r, 0.0, 1.0, 320, 200).unwrap();
                [x as f32, y as f32, w as f32, h as f32]
            })
            .collect();
        for frame in 0..3 {
            let mut encoder = gpu.device.create_command_encoder(&Default::default());
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                });
                for rect in &cleared {
                    let (x, y, w, h) = physical_xywh_to_scissor(*rect, 320, 200).unwrap();
                    pass.set_scissor_rect(x, y, w, h);
                    draw_scissor_fill(&mut pass, &pipeline, &gpu.device, *rect, clear);
                }
                // Model the present pass split: an earlier capture may already
                // have painted part of the same glyphs before the late node
                // capture runs. The late scissors must subtract that ownership
                // after device-pixel rounding, otherwise the retained edge is
                // blended a second time.
                let pre_replay = vec![[18.0, 46.0, 12.0, 25.0]];
                let late_repair =
                    capture_glyph_scissors_after_replay_physical(&cleared, &pre_replay, 320, 200);
                draw_captured_page_glyphs(&renderer, &mut pass, 320, 200, &pre_replay, &[]);
                draw_captured_page_glyphs(&renderer, &mut pass, 320, 200, &late_repair, &[]);
            }
            let partial =
                readback_texture(&gpu.device, &gpu.queue, &texture, 320, 200, encoder).unwrap();
            let changed = partial
                .pixels
                .chunks_exact(4)
                .zip(full.pixels.chunks_exact(4))
                .filter(|(a, b)| a.iter().zip(b.iter()).any(|(x, y)| x.abs_diff(*y) > 2))
                .count();
            assert_eq!(
                changed, 0,
                "{label} frame {frame}: retained glyph edges changed brightness"
            );
        }
    }
}

#[test]
#[ignore = "requires a GPU adapter; run explicitly with --ignored --nocapture"]
fn trigger_hover_region_repair_matches_full_composite() {
    let gpu = OffscreenGpu::new().expect("GPU adapter");
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        width: 320,
        height: 200,
        present_mode: wgpu::PresentMode::Fifo,
        alpha_mode: wgpu::CompositeAlphaMode::Opaque,
        color_space: wgpu::SurfaceColorSpace::Auto,
        view_formats: vec![],
        desired_maximum_frame_latency: 1,
    };
    let pipeline = RuiPipeline::from_format(&gpu.device, config.format, 320, 200);
    pipeline.update_resolution(&gpu.queue, 320, 200);
    let make_target = || {
        gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ColorPicker trigger hover repair"),
            size: wgpu::Extent3d {
                width: 320,
                height: 200,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
    };
    let retained = make_target();
    let reference = make_target();
    let mut root = Element::new()
        .w(320.0)
        .h(200.0)
        .child(
            Element::new()
                .absolute()
                .left(18.25)
                .top(18.25)
                .text("ColorPicker"),
        )
        .child(
            Element::new()
                .absolute()
                .left(18.25)
                .top(46.25)
                .w(114.0)
                .h(32.0)
                .rounded(6.0)
                .bg([0.97, 0.97, 0.97, 1.0])
                .child(
                    Element::new()
                        .absolute()
                        .left(12.0)
                        .top(7.0)
                        .text("#3b82f6"),
                ),
        );
    let mut tree = LayoutTree::new();
    let initial = tree.build_and_layout(&mut root, 320.0, 200.0, [-1.0, -1.0]);
    draw_latest_scene_for_test(&gpu, &config, &pipeline, &retained, &initial, None);
    let key = paint_element_key(&root.children[1]);
    for (frame, gray) in [0.91, 0.97, 0.91].into_iter().enumerate() {
        root.children[1].style.bg_color = [gray, gray, gray, 1.0];
        let scene = tree.build_and_layout(&mut root, 320.0, 200.0, [-1.0, -1.0]);
        let region = PaintDirty {
            key: 0,
            bounds: [8.0, 8.0, 280.0, 140.0],
            kind: PaintDirtyKind::Region,
        };
        let node = PaintDirty {
            key,
            bounds: root.children[1].layout_bounds,
            kind: PaintDirtyKind::Node,
        };
        let scissors = region_scissors_keeping_stable_page_captions(
            vec![region.bounds],
            &region,
            &[region, node],
            &scene.node_paints,
            &[(9, [180.0, 90.0, 80.0, 40.0])],
            None,
        );
        for clip in scissors {
            draw_latest_scene_for_test(&gpu, &config, &pipeline, &retained, &scene, Some(clip));
        }
        draw_latest_scene_for_test(&gpu, &config, &pipeline, &reference, &scene, None);
        let read = |target| {
            readback_texture(
                &gpu.device,
                &gpu.queue,
                target,
                320,
                200,
                gpu.device.create_command_encoder(&Default::default()),
            )
            .unwrap()
        };
        assert!(
            images_match(&read(&retained), &read(&reference), 2),
            "hover repair frame {frame} must match a fresh composite"
        );
    }
}

#[test]
#[ignore = "requires a GPU adapter; run explicitly with --ignored --nocapture"]
fn fractional_overlay_restore_repaints_rounded_scissor_edge() {
    let gpu = OffscreenGpu::new().expect("GPU adapter");
    let pipeline =
        RuiPipeline::from_format(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb, 80, 80);
    pipeline.update_resolution(&gpu.queue, 80, 80);
    let mut root = Element::new().w(80.0).h(80.0).child(
        Element::new()
            .absolute()
            .left(10.0)
            .top(55.3)
            .w(60.0)
            .h(20.0)
            .bg([0.0, 0.6, 0.0, 1.0]),
    );
    let mut scene = LayoutTree::new().build_and_layout(&mut root, 80.0, 80.0, [-1.0, -1.0]);
    for instance in &mut scene.instances {
        instance.clip_pos = [10.0, 55.3];
        instance.clip_size = [60.0, 20.0];
    }
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("fractional overlay restore"),
        size: wgpu::Extent3d {
            width: 80,
            height: 80,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let hole = [10.0, 50.0, 60.0, 5.2];
    let (x, y, w, h) = damage_to_scissor_physical(hole, 0.0, 1.0, 80, 80).unwrap();
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        draw_cpu_sdf_instances(
            &mut pass,
            &pipeline,
            &gpu.device,
            pipeline.dummy_backdrop(),
            &scene.instances,
        );
        pass.set_scissor_rect(x, y, w, h);
        let cull_rect = node_present_repair_cull_rect((x, y, w, h));
        draw_scissor_fill(
            &mut pass,
            &pipeline,
            &gpu.device,
            cull_rect,
            wgpu::Color::WHITE,
        );
        draw_sdf_range_with_clip(
            &mut pass,
            &pipeline,
            &gpu.device,
            pipeline.dummy_backdrop(),
            &scene.instances,
            &[],
            0,
            scene.instances.len() as u32,
            Some(CullRects::one(cull_rect)),
        );
    }
    let partial = readback_texture(&gpu.device, &gpu.queue, &texture, 80, 80, encoder).unwrap();
    let full = gpu
        .render_instances_readback(80, 80, wgpu::Color::WHITE, &scene.instances)
        .unwrap();
    assert!(
        images_match(&partial, &full, 2),
        "overlay restore left a white horizontal edge: partial={:?}, full={:?}",
        partial.pixel(40, 55),
        full.pixel(40, 55)
    );
}

#[test]
#[ignore = "requires a GPU adapter; run explicitly with --ignored --nocapture"]
fn finalized_scroll_pin_clear_cannot_touch_header() {
    let gpu = OffscreenGpu::new().expect("GPU adapter");
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("scroll pin header boundary"),
        size: wgpu::Extent3d {
            width: 320,
            height: 200,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let pipeline =
        RuiPipeline::from_format(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb, 320, 200);
    pipeline.update_resolution(&gpu.queue, 320, 200);
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("scroll repair"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        let rects = scroll_repair_rects_physical(&[], &[[120.0, 64.0, 80.0, 32.0]], 1.0).unwrap();
        for rect in rects.strips() {
            let (x, y, w, h) = physical_xywh_to_scissor(rect, 320, 200).unwrap();
            pass.set_scissor_rect(x, y, w, h);
            draw_scissor_fill(&mut pass, &pipeline, &gpu.device, rect, wgpu::Color::WHITE);
        }
    }
    let image = readback_texture(&gpu.device, &gpu.queue, &texture, 320, 200, encoder).unwrap();
    assert!(
        image
            .pixels
            .chunks_exact(4)
            .take(320 * 64)
            .all(|p| p[0] == 0),
        "scroll repair erased header pixels"
    );
    assert_eq!(
        image.pixel(150, 64).unwrap()[0],
        255,
        "first repaired row must be opaque, not an AA fringe"
    );
}

#[test]
#[ignore = "requires a GPU adapter; run explicitly with --ignored --nocapture"]
fn culled_latest_repair_must_preserve_header_pixels() {
    let gpu = OffscreenGpu::new().expect("GPU adapter");
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("scroll repair header regression"),
        size: wgpu::Extent3d {
            width: 320,
            height: 200,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let pipeline =
        RuiPipeline::from_format(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb, 320, 200);
    pipeline.update_resolution(&gpu.queue, 320, 200);
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("scroll pin clear"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        let mut root = Element::new()
            .w(320.0)
            .h(200.0)
            .flex_col()
            .child(Element::new().w(320.0).h(64.0).flex_shrink(0.0))
            .child(
                Element::new()
                    .w(320.0)
                    .h(136.0)
                    .relative()
                    .overflow_y("hidden")
                    .child(
                        Element::new()
                            .absolute()
                            .left(120.0)
                            .top(-22.0)
                            .w(80.0)
                            .h(32.0),
                    ),
            );
        let mut tree = LayoutTree::new();
        let _ = tree.build_and_layout(&mut root, 320.0, 200.0, [-1.0, -1.0]);
        let key = paint_element_key(&root.children[1].children[0]);
        let mut dirties = vec![PaintDirty {
            key,
            bounds: [120.0, 28.0, 80.0, 60.0],
            kind: PaintDirtyKind::Node,
        }];
        let mut paints = Vec::new(); // Real failure: Latest was culled; slots=[].
        restore_culled_dirty_clips(&root, &mut paints, &dirties);
        prepare_node_present_dirties(&mut dirties, &paints, &[], &[], &[], &[], 1.0, None);
        let hole = node_present_scissor_logical(
            dirties[0].kind,
            key,
            dirties[0].bounds,
            &[],
            &paints,
            0.0,
        );
        if let Some((x, y, w, h)) = damage_to_scissor_physical(hole, 0.0, 1.0, 320, 200) {
            pass.set_scissor_rect(x, y, w, h);
            draw_scissor_fill(&mut pass, &pipeline, &gpu.device, hole, wgpu::Color::WHITE);
        }
    }
    let image = readback_texture(&gpu.device, &gpu.queue, &texture, 320, 200, encoder).unwrap();
    let header_damage = image
        .pixels
        .chunks_exact(4)
        .take(320 * 64)
        .filter(|p| p[0] > 2)
        .count();
    assert_eq!(
        header_damage, 0,
        "scroll pin clear changed fixed-header pixels"
    );
    assert!(
        image.pixel(150, 70).unwrap()[0] > 250,
        "repair hole must actually clear"
    );
}

#[test]
#[ignore = "requires a GPU adapter; run explicitly with --ignored --nocapture"]
fn latest_fade_over_wrapping_text_matches_full_composite() {
    let gpu = OffscreenGpu::new().expect("GPU adapter");
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        width: 320,
        height: 200,
        present_mode: wgpu::PresentMode::Fifo,
        alpha_mode: wgpu::CompositeAlphaMode::Opaque,
        color_space: wgpu::SurfaceColorSpace::Auto,
        view_formats: vec![],
        desired_maximum_frame_latency: 1,
    };
    let pipeline = RuiPipeline::from_format(&gpu.device, config.format, 320, 200);
    pipeline.update_resolution(&gpu.queue, 320, 200);
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Latest retained fade"),
        size: wgpu::Extent3d {
            width: 320,
            height: 200,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: config.format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let mut root = Element::new().w(320.0).h(200.0).relative().bg([1.0;4])
        .child(Element::new().w(280.0).text("MessageScrollerContent sets role=log and aria-relevant=additions. The button leaves no ghost focus stops."))
        .child(Element::new().absolute().left(110.0).top(40.0).w(100.0).h(32.0)
            .z_index(20).bg([1.0;4]).rounded(12.0).child(Element::new().text("Latest")));
    let mut tree = LayoutTree::new();
    let mut previous = Vec::new();
    for (frame, alpha) in [0.0, 0.12, 0.45, 0.8, 1.0, 0.45, 0.08, 0.015, 0.0]
        .into_iter()
        .enumerate()
    {
        root.children[1].style.opacity = alpha;
        root.children[1].style.inset.top =
            taffy::style::LengthPercentageAuto::length(40.0 + (1.0 - alpha) * 8.0);
        let scene = tree.build_and_layout(&mut root, 320.0, 200.0, [-1.0, -1.0]);
        let key = paint_element_key(&root.children[1]);
        let mut dirty = vec![PaintDirty {
            key,
            bounds: root.children[1].layout_bounds,
            kind: PaintDirtyKind::Node,
        }];
        prepare_node_present_dirties(
            &mut dirty,
            &scene.node_paints,
            &previous,
            &scene.instances,
            &[],
            &[],
            1.0,
            None,
        );
        let clip = if frame == 0 {
            None
        } else {
            Some(node_present_scissor_logical(
                dirty[0].kind,
                key,
                dirty[0].bounds,
                &previous,
                &scene.node_paints,
                0.0,
            ))
        };
        if frame > 0 {
            assert_eq!(
                dirty[0].kind,
                PaintDirtyKind::Node,
                "compact Latest fade dest-resets the ink wipe"
            );
        }
        draw_latest_scene_for_test(&gpu, &config, &pipeline, &texture, &scene, clip);
        let partial = readback_texture(
            &gpu.device,
            &gpu.queue,
            &texture,
            320,
            200,
            gpu.device.create_command_encoder(&Default::default()),
        )
        .unwrap();
        draw_latest_scene_for_test(&gpu, &config, &pipeline, &texture, &scene, None);
        let full = readback_texture(
            &gpu.device,
            &gpu.queue,
            &texture,
            320,
            200,
            gpu.device.create_command_encoder(&Default::default()),
        )
        .unwrap();
        let diffs: Vec<_> = partial
            .pixels
            .chunks_exact(4)
            .zip(full.pixels.chunks_exact(4))
            .enumerate()
            .filter(|(_, (a, b))| a.iter().zip(b.iter()).any(|(x, y)| x.abs_diff(*y) > 2))
            .take(12)
            .map(|(i, (a, b))| (i % 320, i / 320, a, b))
            .collect();
        assert!(
            images_match(&partial, &full, 2),
            "Latest frame={frame} alpha={alpha}: partial enter/exit composite differs from full draw; clip={clip:?} diffs={diffs:?}"
        );
        previous = last_present_visual_slots(
            &scene.node_paints,
            &scene.instances,
            &[],
            &scene.text_nodes,
            &[],
        );
    }
}

#[test]
#[ignore = "requires a GPU adapter; run explicitly with --ignored --nocapture"]
fn fractional_overlay_restore_recovers_every_cleared_glyph_row() {
    let gpu = OffscreenGpu::new().expect("GPU adapter");
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        width: 320,
        height: 200,
        present_mode: wgpu::PresentMode::Fifo,
        alpha_mode: wgpu::CompositeAlphaMode::Opaque,
        color_space: wgpu::SurfaceColorSpace::Auto,
        view_formats: vec![],
        desired_maximum_frame_latency: 1,
    };
    let pipeline = RuiPipeline::from_format(&gpu.device, config.format, 320, 200);
    pipeline.update_resolution(&gpu.queue, 320, 200);
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Fractional Avatar restore"),
        size: wgpu::Extent3d {
            width: 320,
            height: 200,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: config.format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let mut root = Element::new().w(320.0).h(200.0).relative().child(
        Element::new()
            .absolute()
            .left(40.0)
            .top(40.0)
            .w(200.0)
            .h(32.0)
            .z_index(1)
            .text("Fallback caption"),
    );
    let mut tree = LayoutTree::new();
    let scene = tree.build_and_layout(&mut root, 320.0, 200.0, [-1.0, -1.0]);
    draw_latest_scene_for_test(&gpu, &config, &pipeline, &texture, &scene, None);
    let full = readback_texture(
        &gpu.device,
        &gpu.queue,
        &texture,
        320,
        200,
        gpu.device.create_command_encoder(&Default::default()),
    )
    .unwrap();
    for edge in [44.2, 46.7, 51.2, 54.6, 60.3] {
        let hole = [0.0, 0.0, 320.0, edge];
        draw_latest_scene_for_test(&gpu, &config, &pipeline, &texture, &scene, Some(hole));
        let repaired = readback_texture(
            &gpu.device,
            &gpu.queue,
            &texture,
            320,
            200,
            gpu.device.create_command_encoder(&Default::default()),
        )
        .unwrap();
        assert!(
            images_match(&full, &repaired, 2),
            "fractional restore bottom={edge}: cleared glyph row was not restored"
        );
    }
}

fn draw_latest_scene_for_test(
    gpu: &OffscreenGpu,
    config: &wgpu::SurfaceConfiguration,
    pipeline: &RuiPipeline,
    texture: &wgpu::Texture,
    scene: &crate::layout::layout_tree::RenderResult,
    clip: Option<[f32; 4]>,
) {
    let bands = scene.overlay_split.page_stack_band_ranges();
    let mut renderers = Vec::new();
    if bands.is_empty() {
        let mut renderer = RuiTextRenderer::new(&gpu.device, &gpu.queue, config);
        let text = match clip {
            Some(hole) => clip_text_nodes_to_cull(
                &scene.text_nodes,
                region_repair_text_cull(
                    &[PaintDirty {
                        key: 0,
                        bounds: hole,
                        kind: PaintDirtyKind::OverlayRestore,
                    }],
                    &[],
                    &[],
                    &[],
                    None,
                    1.0,
                    320,
                    200,
                ),
            ),
            None => scene.text_nodes.clone(),
        };
        crate::renderer::prepare_text::prepare_text_frame_at(
            &mut renderer,
            &gpu.device,
            &gpu.queue,
            320,
            200,
            &text,
            1.0,
            false,
            &Default::default(),
            &Default::default(),
        );
        renderers.push(renderer);
    }
    for band in &bands {
        let mut renderer = RuiTextRenderer::new(&gpu.device, &gpu.queue, config);
        let source = &scene.text_nodes[band.text.0..band.text.1];
        let text = match clip {
            Some(hole) => clip_text_nodes_to_cull(
                source,
                region_repair_text_cull(
                    &[PaintDirty {
                        key: 0,
                        bounds: hole,
                        kind: PaintDirtyKind::OverlayRestore,
                    }],
                    &[],
                    &[],
                    &[],
                    None,
                    1.0,
                    320,
                    200,
                ),
            ),
            None => source.to_vec(),
        };
        crate::renderer::prepare_text::prepare_text_frame_at(
            &mut renderer,
            &gpu.device,
            &gpu.queue,
            320,
            200,
            &text,
            1.0,
            false,
            &Default::default(),
            &Default::default(),
        );
        renderers.push(renderer);
    }
    let view = texture.create_view(&Default::default());
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Latest ordered composite"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: if clip.is_some() {
                        wgpu::LoadOp::Load
                    } else {
                        wgpu::LoadOp::Clear(wgpu::Color::WHITE)
                    },
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        if let Some(clip) = clip {
            let (x, y, w, h) = damage_to_scissor_physical(clip, 0.0, 1.0, 320, 200).unwrap();
            pass.set_scissor_rect(x, y, w, h);
            draw_scissor_fill(&mut pass, pipeline, &gpu.device, clip, wgpu::Color::WHITE);
        }
        if bands.is_empty() {
            draw_sdf_range_with_clip(
                &mut pass,
                pipeline,
                &gpu.device,
                pipeline.dummy_backdrop(),
                &scene.instances,
                &scene.clip_polys,
                0,
                scene.instances.len() as u32,
                None,
            );
            renderers[0].render(&mut pass);
        } else {
            for (band, renderer) in bands.iter().zip(&renderers) {
                draw_sdf_range_with_clip(
                    &mut pass,
                    pipeline,
                    &gpu.device,
                    pipeline.dummy_backdrop(),
                    &scene.instances,
                    &scene.clip_polys,
                    band.instance.0 as u32,
                    (band.instance.1 - band.instance.0) as u32,
                    None,
                );
                renderer.render(&mut pass);
            }
        }
    }
    gpu.queue.submit(Some(encoder.finish()));
}

#[test]
#[ignore = "requires a GPU adapter; run explicitly with --ignored --nocapture"]
fn scrolled_out_message_capture_must_not_replay_previous_glyph_mesh() {
    let gpu = OffscreenGpu::new().expect("GPU adapter");
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        width: 320,
        height: 200,
        present_mode: wgpu::PresentMode::Fifo,
        alpha_mode: wgpu::CompositeAlphaMode::Opaque,
        color_space: wgpu::SurfaceColorSpace::Auto,
        view_formats: vec![],
        desired_maximum_frame_latency: 1,
    };
    let mut renderer = RuiTextRenderer::new(&gpu.device, &gpu.queue, &config);
    let mut root = Element::new()
        .w(320.0)
        .h(200.0)
        .flex_col()
        .child(Element::new().text("Builder"))
        .child(Element::new().text("On it, one-line change."))
        .child(Element::new().text("Delivered"));
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 320.0, 200.0, [-1.0, -1.0]);
    let mut texts = result.text_nodes.clone();
    let prepare = |renderer: &mut RuiTextRenderer,
                   texts: &[crate::layout::layout_tree::TextLayoutInfo]| {
        crate::renderer::prepare_text::prepare_text_frame_at(
            renderer,
            &gpu.device,
            &gpu.queue,
            320,
            200,
            texts,
            1.0,
            false,
            &Default::default(),
            &Default::default(),
        )
    };
    let first = prepare(&mut renderer, &texts);
    assert!(!first.culled_skip_prepare);
    let image = draw_message_capture_for_test(&gpu, &renderer);
    assert!(
        image.pixels.chunks_exact(4).any(|p| p[0] < 200),
        "fixture must rasterize Message glyphs"
    );
    let mut overlay =
        RuiTextRenderer::new_sharing_resources(&gpu.device, &gpu.queue, &config, &renderer);
    let _ = prepare(&mut overlay, &texts);
    overlay.prepare_resolution(&gpu.device, &gpu.queue, 320, 200, std::iter::empty());
    let cleared_overlay = draw_message_capture_for_test(&gpu, &overlay);
    assert!(
        cleared_overlay.pixels.chunks_exact(4).all(|p| p[0] == 255),
        "clearing the overlay must discard its own glyph vertices"
    );
    let page_after_overlay_clear = draw_message_capture_for_test(&gpu, &renderer);
    assert!(
        images_match(&image, &page_after_overlay_clear, 0),
        "sharing GPU resources must keep page and overlay vertex batches independent"
    );
    for text in &mut texts {
        text.y -= 400.0;
        text.clip_bounds = Some([0.0, 50.0, 320.0, 150.0]);
    }
    let hidden = prepare(&mut renderer, &texts);
    assert!(
        hidden.culled_skip_prepare,
        "fixture must reach all-clipped prepare after scrolling"
    );
    let image = draw_message_capture_for_test(&gpu, &renderer);
    let ghosts = image.pixels.chunks_exact(4).filter(|p| p[0] < 250).count();
    assert_eq!(
        ghosts, 0,
        "all-clipped Message prepare must not retain glyph vertices from before scrolling"
    );
    assert!(
        !hidden.needs_warmup,
        "fully culled text must not request endless warmup frames"
    );
    for text in &mut texts {
        text.y += 480.0;
        text.clip_bounds = Some([0.0, 50.0, 320.0, 150.0]);
    }
    let visible_again = prepare(&mut renderer, &texts);
    assert!(
        !visible_again.culled_skip_prepare,
        "clearing draw vertices must preserve reusable shaping/atlas state"
    );
    let image = draw_message_capture_for_test(&gpu, &renderer);
    assert!(
        image
            .pixels
            .chunks_exact(4)
            .enumerate()
            .any(|(i, p)| i / 320 >= 80 && p[0] < 200),
        "Message text must reappear at its new position"
    );
    assert!(
        image
            .pixels
            .chunks_exact(4)
            .enumerate()
            .all(|(i, p)| i / 320 >= 75 || p[0] >= 250),
        "resuming a capture must not revive text at its old position"
    );
}

fn draw_message_capture_for_test(
    gpu: &OffscreenGpu,
    renderer: &RuiTextRenderer,
) -> crate::renderer::offscreen::RgbaImage {
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Message capture stale-mesh regression"),
        size: wgpu::Extent3d {
            width: 320,
            height: 200,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Message capture"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        renderer.render(&mut pass);
    }
    readback_texture(&gpu.device, &gpu.queue, &texture, 320, 200, encoder).unwrap()
}

#[test]
#[ignore = "requires a GPU adapter; run explicitly with --ignored --nocapture"]
fn button_group_hover_first_then_second_matches_full_composite() {
    let gpu = OffscreenGpu::new().expect("GPU adapter");
    for toggle in [false, true] {
        for orientation in ["vertical", "horizontal"] {
            for scale in [1.0, 1.25, 1.5, 2.0] {
                check_group_hover_matches_full_composite(&gpu, toggle, orientation, scale);
            }
        }
    }
}

fn check_group_hover_matches_full_composite(
    gpu: &OffscreenGpu,
    toggle: bool,
    orientation: &str,
    scale: f32,
) {
    use crate::components::button::{button, sync_button_hover_paint_subtree};
    use crate::components::button_group::button_group;
    use crate::components::toggle_group::toggle_group;
    use crate::core::reactivity::create_root;
    use crate::core::tree::LayoutStatus;
    use crate::renderer::scale_instances;

    const W: u32 = 400;
    const H: u32 = 240;
    let logical_w = W as f32 / scale;
    let logical_h = H as f32 / scale;
    crate::style::set_theme_mode(crate::style::ThemeMode::Light);
    let (_handle, group) = create_root(|| {
        let group = if toggle {
            Element::from(
                toggle_group("retained-toggle-group")
                    .variant("outline")
                    .orientation(orientation)
                    .spacing(0)
                    .options([("top", "Top"), ("middle", "Middle"), ("bottom", "Bottom")]),
            )
        } else {
            Element::from(
                button_group()
                    .orientation(orientation)
                    .child(button("retained-group-top").variant("outline").text("Top"))
                    .child(
                        button("retained-group-middle")
                            .variant("outline")
                            .text("Middle"),
                    )
                    .child(
                        button("retained-group-bottom")
                            .variant("outline")
                            .text("Bottom"),
                    ),
            )
        };
        if orientation == "vertical" {
            group.w(160.0)
        } else {
            group
        }
    });
    let mut root = Element::new()
        .w(logical_w)
        .h(logical_h)
        .bg([1.0; 4])
        .child(group);
    let mut tree = LayoutTree::new();
    let initial = tree.build_and_layout(&mut root, logical_w, logical_h, [-1.0; 2]);
    tree.set_collect_skip_allowed(true);
    let _ = tree.build_and_layout_fast(&mut root, logical_w, logical_h, [-1.0; 2], true, false);
    clear_frame_paint_nodes();
    crate::app::tree_hit::clear_hover_tracking();
    let initial_instances = scale_instances(&initial.instances, scale);
    let pipeline = RuiPipeline::from_format(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb, W, H);
    pipeline.update_resolution(&gpu.queue, W, H);
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("group hover retained differential"),
        size: wgpu::Extent3d {
            width: W,
            height: H,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        draw_cpu_sdf_instances(
            &mut pass,
            &pipeline,
            &gpu.device,
            pipeline.dummy_backdrop(),
            &initial_instances,
        );
    }
    let _ = readback_texture(&gpu.device, &gpu.queue, &texture, W, H, encoder).unwrap();
    let mut prev = last_present_visual_slots(
        &initial.node_paints,
        &initial.instances,
        &[],
        &initial.text_nodes,
        &[],
    );
    for index in [Some(0), Some(1), Some(2), Some(0), None] {
        let mouse = index
            .map(|i| {
                let b = root.children[0].children[i].layout_bounds;
                [b[0] + b[2] * 0.5, b[1] + b[3] * 0.5]
            })
            .unwrap_or([-1.0; 2]);
        clear_frame_paint_nodes();
        crate::app::tree_hit::apply_pointer_hover_reconcile(&root, mouse[0], mouse[1], true);
        let mut impact = LayoutStatus::Clean;
        sync_button_hover_paint_subtree(
            &mut root,
            &mut tree,
            &mut impact,
            logical_w,
            logical_h,
            [0.0; 2],
        );
        crate::style::tick_active_css_transitions_subtree(
            &mut root,
            mouse,
            "",
            false,
            logical_w,
            logical_h,
            [0.0; 2],
            &mut tree,
            &mut impact,
        );
        let current =
            tree.build_and_layout_fast(&mut root, logical_w, logical_h, mouse, true, false);
        let current_instances = scale_instances(&current.instances, scale);
        let mut dirties = take_frame_paint_nodes();
        assert!(!dirties.is_empty(), "hover must produce real paint damage");
        prepare_node_present_dirties(
            &mut dirties,
            &current.node_paints,
            &prev,
            &current_instances,
            &[],
            &[],
            scale,
            None,
        );
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        let mut resets = Vec::new();
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            for dirty in &dirties {
                assert_eq!(dirty.kind, PaintDirtyKind::Node);
                let reset = node_present_dest_reset_full(
                    dirty.key,
                    dirty.bounds,
                    &current.node_paints,
                    &current_instances,
                    &[],
                    &[],
                    scale,
                    prev.iter().find(|(k, _)| *k == dirty.key).map(|(_, r)| *r),
                )
                .expect("opaque root backdrop");
                resets.push((dirty.key, dirty.bounds, reset.rect));
                // Dest-reset already returns physical bounds, as in production.
                let (x, y, w, h) = damage_to_scissor_physical(reset.rect, 0.0, 1.0, W, H).unwrap();
                pass.set_scissor_rect(x, y, w, h);
                let cull = node_present_repair_cull_rect((x, y, w, h));
                let hole = cull.map(|v| v / scale);
                let [r, g, b, a] = reset.clear;
                draw_scissor_fill(
                    &mut pass,
                    &pipeline,
                    &gpu.device,
                    cull,
                    wgpu::Color {
                        r: r as f64,
                        g: g as f64,
                        b: b as f64,
                        a: a as f64,
                    },
                );
                {
                    let (start, end) = reset.replay;
                    draw_sdf_range_with_clip(
                        &mut pass,
                        &pipeline,
                        &gpu.device,
                        pipeline.dummy_backdrop(),
                        &current_instances,
                        &current.clip_polys,
                        start as u32,
                        (end - start) as u32,
                        Some(CullRects::one(cull)),
                    );
                }
                let restore =
                    node_present_restore_underlay(dirty.key, dirty.bounds, &current.node_paints);
                let replay_peers = node_present_replays_peer_chrome(
                    dirty.key,
                    dirty.bounds,
                    hole,
                    &current.node_paints,
                    &current_instances,
                );
                // Joined chrome is replayed in scene order; overlay underlays stay separate.
                for own_pass in [false, true] {
                    for p in &current.node_paints {
                        let own =
                            paint_dirty_matches_slot(dirty.key, dirty.bounds, p.key, p.bounds);
                        let underlay = (restore || replay_peers)
                            && !p.overlay
                            && !own
                            && node_present_underlay_paint(
                                dirty.key,
                                dirty.bounds,
                                p,
                                hole,
                                &current.node_paints,
                                &current_instances,
                            );
                        if (own_pass && (own || (replay_peers && underlay)))
                            || (!own_pass && !replay_peers && underlay)
                        {
                            let (start, end) = p.instance;
                            draw_sdf_range_with_clip(
                                &mut pass,
                                &pipeline,
                                &gpu.device,
                                pipeline.dummy_backdrop(),
                                &current_instances,
                                &current.clip_polys,
                                start as u32,
                                (end - start) as u32,
                                underlay.then_some(CullRects::one(cull)),
                            );
                        }
                    }
                }
            }
        }
        let partial = readback_texture(&gpu.device, &gpu.queue, &texture, W, H, encoder).unwrap();
        let full = gpu
            .render_instances_readback(W, H, wgpu::Color::WHITE, &current_instances)
            .unwrap();
        let diffs: Vec<_> = partial
            .pixels
            .chunks_exact(4)
            .zip(full.pixels.chunks_exact(4))
            .enumerate()
            .filter(|(_, (a, b))| a.iter().zip(b.iter()).any(|(x, y)| x.abs_diff(*y) > 2))
            .take(12)
            .map(|(i, (a, b))| ((i as u32 % W, i as u32 / W), a.to_vec(), b.to_vec()))
            .collect();
        assert!(
            images_match(&partial, &full, 2),
            "toggle={toggle}, orientation={orientation}, scale={scale}, hover={index:?}, resets={resets:?}, diffs={diffs:?}"
        );
        prev = last_present_visual_slots(
            &current.node_paints,
            &current.instances,
            &[],
            &current.text_nodes,
            &[],
        );
    }
    crate::app::tree_hit::clear_hover_tracking();
    clear_frame_paint_nodes();
}

#[test]
#[ignore = "requires a GPU adapter; run explicitly with --ignored --nocapture"]
fn retained_node_and_region_match_full_clear_pixels() {
    let gpu = OffscreenGpu::new().expect("GPU differential test requires an adapter");
    eprintln!("GPU differential adapter: {:?}", gpu.adapter_info);
    for (background, portal) in [(false, false), (true, false), (true, true)] {
        check_partial_pixels(&gpu, background, portal);
    }
}

fn check_partial_pixels(gpu: &OffscreenGpu, background: bool, portal: bool) {
    const W: u32 = 128;
    const H: u32 = 96;
    let clear = wgpu::Color::WHITE;
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let pipeline = RuiPipeline::from_format(&gpu.device, format, W, H);
    pipeline.update_resolution(&gpu.queue, W, H);
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("partial/full differential retain"),
        size: wgpu::Extent3d {
            width: W,
            height: H,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut root = Element::new().w(W as f32).h(H as f32).relative().child(
        Element::new()
            .absolute()
            .left(12.0)
            .top(18.0)
            .w(104.0)
            .h(56.0)
            .bg([1.0, 0.0, 0.0, 0.8]),
    );
    if background {
        root.style.bg_color = [1.0; 4];
    }
    let mut tree = LayoutTree::new();
    let initial = tree.build_and_layout(&mut root, W as f32, H as f32, [-1.0, -1.0]);
    // A static portal bisects the Node dirty. Region case has no portal so the
    // test isolates the missing-backdrop fallback and repeated alpha updates.
    let overlay = portal.then_some([54.0, 0.0, 20.0, H as f32]);
    let overlay_inst = overlay.map(|b| {
        let mut i = initial.instances.last().copied().expect("painted child");
        i.rect_pos = [b[0], b[1]];
        i.rect_size = [b[2], b[3]];
        i.color = [0.0, 0.8, 0.0, 0.65];
        i
    });
    let occupancy = overlay_inst.as_ref().map(instance_paint_bounds);
    let mut prev = last_present_visual_slots(
        &initial.node_paints,
        &initial.instances,
        &[],
        &initial.text_nodes,
        &[],
    );
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("initial full draw"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(clear),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        draw_cpu_sdf_instances(
            &mut pass,
            &pipeline,
            &gpu.device,
            pipeline.dummy_backdrop(),
            &initial.instances,
        );
        if let Some(i) = overlay_inst {
            draw_cpu_sdf_instances(
                &mut pass,
                &pipeline,
                &gpu.device,
                pipeline.dummy_backdrop(),
                &[i],
            );
        }
    }
    let mut retained_before =
        readback_texture(&gpu.device, &gpu.queue, &texture, W, H, encoder).unwrap();
    for (frame, opacity) in [0.2, 0.65, 0.0, 0.45].into_iter().enumerate() {
        root.children[0].style.bg_color = [0.0, 0.0, 1.0, opacity];
        tree.set_collect_skip_allowed(false);
        let current =
            tree.build_and_layout_fast(&mut root, W as f32, H as f32, [-1.0, -1.0], false, false);
        let key = paint_element_key(&root.children[0]);
        let bounds = root.children[0].layout_bounds;
        let mut dirty = vec![PaintDirty {
            key,
            bounds,
            kind: PaintDirtyKind::Node,
        }];
        prepare_node_present_dirties(
            &mut dirty,
            &current.node_paints,
            &prev,
            &current.instances,
            &[],
            &[],
            1.0,
            occupancy,
        );
        let reset = node_present_dest_reset_full(
            key,
            bounds,
            &current.node_paints,
            &current.instances,
            &[],
            &[],
            1.0,
            prev.iter().find(|(k, _)| *k == key).map(|(_, r)| *r),
        );
        let hole = if dirty[0].kind == PaintDirtyKind::Region {
            dirty[0].bounds
        } else {
            if reset.is_none() {
                assert!(
                    layered_src_over_skips_dest_reset(
                        key,
                        bounds,
                        &current.node_paints,
                        prev.iter().find(|(k, _)| *k == key).map(|(_, r)| *r)
                    ),
                    "Node must have a backdrop unless this is its first visible paint: frame={frame}"
                );
            }
            reset.as_ref().map(|r| r.rect).unwrap_or_else(|| {
                node_present_scissor_logical(
                    dirty[0].kind,
                    key,
                    bounds,
                    &prev,
                    &current.node_paints,
                    node_present_scissor_pad(dirty[0].kind),
                )
            })
        };
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("retained partial update"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            let clips = if dirty[0].kind == PaintDirtyKind::Region {
                vec![hole]
            } else {
                page_scissors_excluding_overlay(hole, occupancy)
            };
            for clip in clips {
                let (x, y, w, h) = damage_to_scissor_physical(clip, 0.0, 1.0, W, H).unwrap();
                pass.set_scissor_rect(x, y, w, h);
                if dirty[0].kind == PaintDirtyKind::Region || reset.is_some() {
                    draw_scissor_fill(&mut pass, &pipeline, &gpu.device, hole, clear);
                }
                let range = if dirty[0].kind == PaintDirtyKind::Region {
                    (0, current.instances.len())
                } else {
                    current
                        .node_paints
                        .iter()
                        .find(|p| p.key == key)
                        .unwrap()
                        .instance
                };
                draw_sdf_range_with_clip(
                    &mut pass,
                    &pipeline,
                    &gpu.device,
                    pipeline.dummy_backdrop(),
                    &current.instances,
                    &current.clip_polys,
                    range.0 as u32,
                    (range.1 - range.0) as u32,
                    None,
                );
                if dirty[0].kind == PaintDirtyKind::Region {
                    if let Some(i) = overlay_inst {
                        draw_cpu_sdf_instances(
                            &mut pass,
                            &pipeline,
                            &gpu.device,
                            pipeline.dummy_backdrop(),
                            &[i],
                        );
                    }
                }
            }
        }
        let partial = readback_texture(&gpu.device, &gpu.queue, &texture, W, H, encoder).unwrap();
        let mut full_instances = current.instances.clone();
        full_instances.extend(overlay_inst);
        let mut full = gpu
            .render_instances_readback(W, H, clear, &full_instances)
            .unwrap();
        if dirty[0].kind == PaintDirtyKind::Node {
            // Node page updates intentionally preserve a static portal's
            // occupied pixels, including its captured underlay. Compare the
            // repaired page with full draw and protected pixels with retain.
            if let Some(occupied) = occupancy {
                let (x, y, w, h) = damage_to_scissor_physical(occupied, 0.0, 1.0, W, H).unwrap();
                for row in y..y + h {
                    let start = ((row * W + x) * 4) as usize;
                    let end = start + (w * 4) as usize;
                    full.pixels[start..end].copy_from_slice(&retained_before.pixels[start..end]);
                }
            }
        }
        let different: Vec<_> = partial
            .pixels
            .chunks_exact(4)
            .zip(full.pixels.chunks_exact(4))
            .enumerate()
            .filter(|(_, (a, b))| a.iter().zip(b.iter()).any(|(x, y)| x.abs_diff(*y) > 2))
            .take(12)
            .map(|(i, (a, b))| ((i as u32 % W, i as u32 / W), a.to_vec(), b.to_vec()))
            .collect();
        assert!(
            images_match(&partial, &full, 2),
            "retained/full pixels differ: background={background}, portal={portal}, frame={frame}, alpha={opacity}, hole={hole:?}, diffs={different:?}"
        );
        retained_before = partial;
        prev = last_present_visual_slots(
            &current.node_paints,
            &current.instances,
            &[],
            &current.text_nodes,
            &[],
        );
    }
}

/// Instance clip flags must preserve both polygon layers and rounded clips.
#[test]
#[ignore = "requires a GPU adapter; run explicitly with --ignored --nocapture"]
fn sdf_vertex_clip_flags_preserve_every_world_clip_channel() {
    let gpu = OffscreenGpu::new().expect("GPU adapter");
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let pipeline = RuiPipeline::from_format(&gpu.device, format, 96, 64);
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("world clip flags"),
        size: wgpu::Extent3d {
            width: 96,
            height: 64,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let mut root = Element::new().w(96.0).h(64.0).bg([1.0, 0.0, 0.0, 1.0]);
    let scene = LayoutTree::new().build_and_layout(&mut root, 96.0, 64.0, [-1.0, -1.0]);
    let mut clipped = scene.instances[0];
    clipped.color = [0.0, 0.0, 1.0, 1.0];
    let instances = [scene.instances[0], clipped];
    for channel in 0..4 {
        let mut clip = crate::style::GpuClipPoly::NONE;
        let points = [[32.0, 8.0], [64.0, 8.0], [64.0, 56.0], [32.0, 56.0]];
        match channel {
            0 => {
                clip.count = 4;
                clip.pts[..4].copy_from_slice(&points);
            }
            1 => {
                clip.count2 = 4;
                clip.pts2[..4].copy_from_slice(&points);
            }
            2 => {
                clip.round_rect = [32.0, 8.0, 32.0, 48.0];
                clip.round_radii = [8.0; 4];
            }
            _ => {
                clip.round_rect2 = [32.0, 8.0, 32.0, 48.0];
                clip.round_radii2 = [8.0; 4];
            }
        }
        let clips = [crate::style::GpuClipPoly::NONE, clip];
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            draw_sdf_range_with_clip(
                &mut pass,
                &pipeline,
                &gpu.device,
                pipeline.dummy_backdrop(),
                &instances,
                &clips,
                0,
                2,
                None,
            );
        }
        let image = readback_texture(&gpu.device, &gpu.queue, &texture, 96, 64, encoder).unwrap();
        assert_eq!(
            image.pixel(48, 32),
            Some([0, 0, 255, 255]),
            "clip channel {channel} must paint its interior"
        );
        assert_eq!(
            image.pixel(16, 32),
            Some([255, 0, 0, 255]),
            "clip channel {channel} must preserve unclipped predecessor"
        );
        assert_eq!(
            image.pixel(72, 32),
            Some([255, 0, 0, 255]),
            "clip channel {channel} must reject exterior"
        );
        if channel >= 2 {
            assert_eq!(
                image.pixel(33, 9),
                Some([255, 0, 0, 255]),
                "rounded clip corner must remain curved"
            );
        }
    }
}

#[cfg(target_os = "windows")]
#[path = "state_chart_present_tests.rs"]
mod chart_present_tests;
