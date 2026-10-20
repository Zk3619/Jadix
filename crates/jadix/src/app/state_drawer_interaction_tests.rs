//! Replay actual modal enter/exit frames and compare cached captures with fresh ones.
use super::*;
use crate::core::reactivity::{create_effect, create_root, use_signal};
use std::{cell::RefCell, rc::Rc};

#[test]
#[ignore = "requires a native Windows GPU surface"]
fn drawer_native_drag_resize_scroll_and_style() {
    use winit::application::ApplicationHandler;
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, EventLoop};
    use winit::platform::windows::EventLoopBuilderExtWindows;
    use winit::window::{WindowAttributes, WindowId};
    struct Harness;
    impl ApplicationHandler for Harness {
        fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
            for direction in [
                crate::components::drawer::DrawerDirection::Down,
                crate::components::drawer::DrawerDirection::Up,
                crate::components::drawer::DrawerDirection::Left,
                crate::components::drawer::DrawerDirection::Right,
            ] {
                let window = Arc::from(
                    event_loop
                        .create_window(
                            WindowAttributes::default()
                                .with_surface_size(winit::dpi::LogicalSize::new(960.0, 640.0))
                                .with_visible(false),
                        )
                        .unwrap(),
                );
                run_drawer(window, direction);
            }
            event_loop.exit();
        }
        fn window_event(&mut self, _: &dyn ActiveEventLoop, _: WindowId, _: WindowEvent) {}
    }
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    builder.build().unwrap().run_app(Harness).unwrap();
}

fn find<'a>(el: &'a Element, id: &str) -> Option<&'a Element> {
    if el.template_id.as_deref() == Some(id) || el.focus_id.as_deref() == Some(id) {
        return Some(el);
    }
    el.children.iter().find_map(|c| find(c, id))
}

fn frame(state: &Rc<RefCell<Option<AppState>>>) {
    thread_local! { static FRAME_MS: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) }; }
    FRAME_MS.with(|clock| {
        clock.set(clock.get() + 1000.0 / 60.0);
        crate::style::set_css_transition_timeline_ms(clock.get());
    });
    crate::components::drawer::tick_drawer_snaps();
    crate::components::drawer::drain_drawer_layout_bumps();
    crate::app::reactive::drain_deferred_view_remounts();
    crate::style::utility::set_frame_dt(1.0 / 60.0);
    state
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

fn compare_full(state: &Rc<RefCell<Option<AppState>>>) {
    let mut guard = state.borrow_mut();
    let s = guard.as_mut().unwrap();
    let read = |s: &AppState| {
        readback_texture(
            &s.device,
            &s.queue,
            s.present_retain.color_texture_for_test().unwrap(),
            s.config.width,
            s.config.height,
            s.device.create_command_encoder(&Default::default()),
        )
        .unwrap()
    };
    let cached = read(s);
    s.modal_source_fp = None;
    s.mark_full_redraw();
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
    let full = read(s);
    let different = cached
        .pixels
        .chunks_exact(4)
        .zip(full.pixels.chunks_exact(4))
        .filter(|(a, b)| a.iter().zip(*b).any(|(a, b)| a.abs_diff(*b) > 3))
        .count();
    assert_eq!(
        different, 0,
        "Drawer retained rendering differs from full repaint"
    );
}

fn run_drawer(window: Arc<dyn Window>, direction: crate::components::drawer::DrawerDirection) {
    use crate::app::pointer_events::{dispatch_pointer, hit_path, PointerEvent, PointerKind};
    use crate::components::button::{Button, ButtonProps};
    use crate::components::drawer::*;
    use crate::components::label::{Label, LabelProps};
    crate::init();
    crate::style::set_css_transition_virtual_timeline(true);
    crate::app::reactive::reset_registered_anchors();
    crate::app::portal_overlay::clear_overlay_stack();
    let state = Rc::new(RefCell::new(Some(crate::utils::block_on::block_on(
        AppState::new_bootstrap(window, crate::app::WindowOptions::new()),
    ))));
    let secondary = Rc::new(RefCell::new(HashMap::new()));
    crate::app::runtime::set_app_runtime(crate::app::runtime::AppRuntime::new(
        Rc::downgrade(&state),
        Rc::downgrade(&secondary),
    ));
    let (owner, (open, tip_open, card_open, hud_caption)) = create_root(|| {
        let open = use_signal("drawer-native-open", || false);
        let tip_open = use_signal("drawer-native-tooltip", || false);
        let card_open = use_signal("drawer-native-hovercard", || false);
        let hud_caption = use_signal("drawer-native-hud", || "FPS 60".to_string());
        let live = state.clone();
        crate::core::reactivity::mark_next_effect_as_ui_root();
        create_effect(move || {
            let drawer = drawer_scope("native-drawer", Some(open), || {
                drawer_content(
                    DrawerContentOpts {
                        direction,
                        height: (direction == DrawerDirection::Up).then(|| "400px".into()),
                        width: (direction == DrawerDirection::Right).then(|| "280px".into()),
                        show_swipe_handle: true,
                        snap_points: vec![
                            DrawerSnapPoint::Fraction(0.5),
                            DrawerSnapPoint::Fraction(1.0),
                        ],
                        snap_to_sequential_point: true,
                        ..Default::default()
                    },
                    || {
                        let mut rows = Element::new().flex_col().w("100%");
                        for i in 0..30 {
                            rows = rows.child(
                                Element::new()
                                    .h(32.0)
                                    .flex_shrink(0.0)
                                    .text(format!("Row {i}")),
                            );
                        }
                        Element::new()
                            .child(DrawerHeader(DrawerHeaderProps {
                                children: Some(Element::new().text("Drawer title")),
                                ..Default::default()
                            }))
                            .child(DrawerBody(DrawerBodyProps {
                                children: Some(rows),
                                ..Default::default()
                            }))
                            .child(DrawerFooter(DrawerFooterProps {
                                children: Some(Element::new().h(40.0).text("Sticky footer")),
                                ..Default::default()
                            }))
                    },
                )
            });
            let mut background = Element::new().absolute().inset(0.0).flex_col();
            for i in 0..32 {
                background = background.child(
                    Element::new()
                        .h(20.0)
                        .text(format!("Backdrop row {i} ABCDEFG 123456")),
                );
            }
            let tip = crate::components::tooltip::tooltip("native-tip", "Tooltip content")
                .open_signal(tip_open)
                .trigger(|_| Element::new().w(100.0).h(30.0).text("Hover"));
            let card = crate::components::hover_card::hover_card_scope(
                "native-card",
                Some(card_open),
                || {
                    crate::components::hover_card::hover_card_content(Default::default(), || {
                        Element::new().w(160.0).h(100.0).text("HoverCard")
                    })
                },
            );
            let hud = crate::utils::mount_reactive_anchor("gallery:performance-host", move || {
                let hud_panel = crate::ui! {
                    <VStack id="perf-panel" absolute={true} right="12px" bottom="12px" z_index={1000000} gap="4px" items="end">
                        <Button id="perf-toggle" size="sm" variant="secondary">"性能 / FPS"</Button>
                        <Show when={true} scope="gallery:performance">
                            <Box w="280px" h="82px" p="8px" bg="#111827dd" rounded="6px" pointer_events="none">
                                <Label w="264px" h="66px" text_size="12px" color="#ffffff" leading="1.5">{hud_caption.get()}</Label>
                            </Box>
                        </Show>
                    </VStack>
                };
                Element::new()
                    .w(0.0)
                    .h(0.0)
                    .child(crate::utils::viewport_portal_stage(
                        "gallery-performance",
                        hud_panel,
                        1_000_000,
                    ))
            });
            live.borrow_mut().as_mut().unwrap().rebuild_ui(
                Element::new()
                    .w("100%")
                    .h("100%")
                    .bg("#2563eb")
                    .child(background)
                    .child(tip)
                    .child(card)
                    .child(drawer)
                    .child(Element::new().w(0.0).h(0.0).child(hud)),
            );
        });
        (open, tip_open, card_open, hud_caption)
    });
    frame(&state);
    for i in 0..4 {
        // A floating-only compute must not freeze the portal root at its old
        // viewport when the real surface is maximized, then restored.
        if i == 1 || i == 3 {
            let mut guard = state.borrow_mut();
            let s = guard.as_mut().unwrap();
            let logical = if i == 1 {
                [1706.6666, 978.6667]
            } else {
                [960.0, 640.0]
            };
            let scale = s.ui_scale();
            let size = winit::dpi::PhysicalSize::new(
                (logical[0] * scale).round() as u32,
                (logical[1] * scale).round() as u32,
            );
            let _ = s.window.request_surface_size(size.into());
            s.resize(size);
        }
        tip_open.set(i % 2 == 0);
        card_open.set(i % 2 != 0);
        hud_caption.set(format!("FPS {}", 60 - i));
        for _ in 0..40 {
            std::thread::sleep(std::time::Duration::from_millis(16));
            frame(&state);
        }
        assert!(
            find(
                &state.borrow().as_ref().unwrap().root_element,
                "perf-toggle"
            )
            .is_some(),
            "HUD lost during floating open"
        );
        let guard = state.borrow();
        let s = guard.as_ref().unwrap();
        let hud = find(&s.root_element, "perf-panel").unwrap();
        let width = s.config.width as f32 / s.ui_scale();
        let height = s.config.height as f32 / s.ui_scale();
        assert!(
            (width - if i == 1 || i == 2 { 1706.6666 } else { 960.0 }).abs() < 1.0,
            "native viewport must actually resize: {width}x{height}"
        );
        assert!(
            (width - hud.layout_bounds[0] - hud.layout_bounds[2] - 12.0).abs() < 1.0
                && (height - hud.layout_bounds[1] - hud.layout_bounds[3] - 12.0).abs() < 1.0,
            "HUD must stay at viewport bottom-right after floating layout: {:?}",
            hud.layout_bounds
        );
        drop(guard);
        compare_full(&state);
    }
    tip_open.set(false);
    card_open.set(false);
    open.set(true);
    for _ in 0..40 {
        frame(&state);
    }
    let bounds = |id: &str| {
        let guard = state.borrow();
        let el = find(&guard.as_ref().unwrap().root_element, id).expect(id);
        let mut b = el.layout_bounds;
        b[0] += el.style.translate[0] + el.style.translate_pct[0] * b[2];
        b[1] += el.style.translate[1] + el.style.translate_pct[1] * b[3];
        b
    };
    let full = bounds("native-drawer__modal_panel");
    if direction == DrawerDirection::Up {
        assert!(
            (full[3] - 400.0).abs() < 1.0,
            "custom height must be respected: {full:?}"
        );
    }
    if direction == DrawerDirection::Right {
        assert!(
            (full[2] - 280.0).abs() < 1.0,
            "custom width must be respected"
        );
    }
    let handle = bounds("native-drawer__drag");
    let x = handle[0] + handle[2] / 2.0;
    let y = handle[1] + handle[3] / 2.0;
    let path = {
        let guard = state.borrow();
        hit_path(&guard.as_ref().unwrap().root_element, x, y)
    };
    assert!(
        path.iter()
            .any(|e| e.focus_id.as_deref() == Some("native-drawer__drag")),
        "handle must be hittable"
    );
    let mut event = PointerEvent::new(x, y, winit::event::MouseButton::Left, 1);
    dispatch_pointer(&path, &mut event, PointerKind::Down);
    let vertical = matches!(direction, DrawerDirection::Down | DrawerDirection::Up);
    let axis = if vertical { full[3] } else { full[2] };
    let sign = if matches!(direction, DrawerDirection::Down | DrawerDirection::Right) {
        1.0
    } else {
        -1.0
    };
    let moved_x = x + if vertical { 0.0 } else { sign * axis * 0.3 };
    let moved_y = y + if vertical { sign * axis * 0.3 } else { 0.0 };
    {
        let mut guard = state.borrow_mut();
        let s = guard.as_mut().unwrap();
        assert!(tick_drawer_drag(&mut s.root_element, moved_x, moved_y));
        s.mark_full_redraw();
    }
    frame(&state);
    let dragged = bounds("native-drawer__modal_panel");
    let drag_alpha = {
        let guard = state.borrow();
        let overlay = find(
            &guard.as_ref().unwrap().root_element,
            "native-drawer__modal_overlay",
        )
        .unwrap();
        overlay.style.bg_color[3] * overlay.style.opacity
    };
    if vertical {
        assert!(
            (dragged[3] - full[3] * 0.7).abs() < 2.0,
            "height follows pointer: full={full:?} dragged={dragged:?}"
        );
    } else {
        assert!(
            (dragged[0] - full[0] - sign * axis * 0.3).abs() < 2.0,
            "horizontal drag must follow pointer: {full:?} -> {dragged:?}"
        );
    }
    compare_full(&state);
    clear_drawer_drag_if_any();
    frame(&state);
    let release = bounds("native-drawer__modal_panel");
    assert!(
        (release[3] - dragged[3]).abs() < 10.0,
        "release must not jump: {dragged:?} -> {release:?}"
    );
    let mut heights = vec![release[3]];
    for _ in 0..40 {
        std::thread::sleep(std::time::Duration::from_millis(16));
        frame(&state);
        heights.push(bounds("native-drawer__modal_panel")[3]);
        compare_full(&state);
    }
    let half = bounds("native-drawer__modal_panel");
    let rest_alpha = {
        let guard = state.borrow();
        let overlay = find(
            &guard.as_ref().unwrap().root_element,
            "native-drawer__modal_overlay",
        )
        .unwrap();
        overlay.style.bg_color[3] * overlay.style.opacity
    };
    assert!(
        rest_alpha < drag_alpha,
        "smaller resting drawer must have lighter scrim: drag={drag_alpha} rest={rest_alpha}"
    );
    assert_eq!(drawer_active_snap_point("native-drawer"), Some(0));
    if vertical {
        assert!(
            (half[3] - full[3] * 0.5).abs() < 2.0,
            "half snap: full={full:?} half={half:?}"
        );
        assert!(
            heights
                .iter()
                .any(|h| *h > half[3] + 4.0 && *h < release[3] - 4.0),
            "snap must include intermediate heights"
        );
    } else {
        assert!(
            (half[0] - full[0] - sign * axis * 0.5).abs() < 2.0,
            "horizontal half snap must leave half the panel visible: {full:?} -> {half:?}"
        );
    }
    let guard = state.borrow();
    let s = guard.as_ref().unwrap();
    let body = find(&s.root_element, "native-drawer__body").expect("body");
    assert!(
        body.layout_bounds[3] > 10.0 && body.layout_bounds[3] < half[3],
        "body must get remaining space"
    );
    let footer = s
        .text_nodes
        .iter()
        .find(|n| n.text == "Sticky footer")
        .expect("footer text");
    assert!(
        footer.y >= half[1] && footer.y < half[1] + half[3],
        "footer remains visible"
    );
    let metrics = s
        .last_render_scroll_metrics
        .iter()
        .find(|(id, ..)| id == "native-drawer__body")
        .expect("scroll metrics");
    eprintln!(
        "Drawer {direction:?} full={full:?}, drag={dragged:?}, half={half:?}, body={:?}, scroll={metrics:?}",
        body.layout_bounds
    );
    let max_scroll = metrics.2[1];
    drop(guard);
    use_signal(
        &crate::core::scroll_viewport::scroll_offset_y_signal_key("native-drawer__body"),
        || 0.0_f32,
    )
    .set(max_scroll * 0.5);
    frame(&state);
    assert!(
        find(
            &state.borrow().as_ref().unwrap().root_element,
            "scroll:native-drawer__body"
        )
        .unwrap()
        .scroll_offset[1]
            > 1.0,
        "half-open Body must actually scroll"
    );
    compare_full(&state);
    drawer_set_active_snap_point("native-drawer", 1);
    for _ in 0..40 {
        // CSS transition completion follows elapsed time. DX12 can render these
        // frames faster than its duration when the normal event-loop pace is absent.
        std::thread::sleep(std::time::Duration::from_millis(16));
        frame(&state);
    }
    let restored = bounds("native-drawer__modal_panel");
    assert!(
        (restored[0] - full[0]).abs() < 2.0 && (restored[3] - full[3]).abs() < 2.0,
        "programmatic snap after drag must refresh layout: {restored:?}"
    );
    // Drag below the dismiss threshold and verify exit keeps the current size.
    let handle = bounds("native-drawer__drag");
    let x = handle[0] + handle[2] / 2.0;
    let y = handle[1] + handle[3] / 2.0;
    let path = {
        let guard = state.borrow();
        hit_path(&guard.as_ref().unwrap().root_element, x, y)
    };
    dispatch_pointer(
        &path,
        &mut PointerEvent::new(x, y, winit::event::MouseButton::Left, 1),
        PointerKind::Down,
    );
    {
        let mut guard = state.borrow_mut();
        let s = guard.as_mut().unwrap();
        tick_drawer_drag(
            &mut s.root_element,
            x + if vertical { 0.0 } else { sign * axis * 0.9 },
            y + if vertical { sign * axis * 0.9 } else { 0.0 },
        );
        s.mark_full_redraw();
    }
    frame(&state);
    let closing = bounds("native-drawer__modal_panel");
    clear_drawer_drag_if_any();
    frame(&state);
    assert!(!open.peek(), "drag below threshold must dismiss");
    if vertical {
        assert!(
            (bounds("native-drawer__modal_panel")[3] - closing[3]).abs() < 2.0,
            "dismiss must not restore full height before exit"
        );
    }
    for _ in 0..40 {
        frame(&state);
    }
    std::thread::sleep(std::time::Duration::from_millis(500));
    crate::utils::modal_motion::drain_pending_modal_unmounts();
    frame(&state);
    open.set(true);
    for _ in 0..40 {
        frame(&state);
    }
    let reopened = bounds("native-drawer__modal_panel");
    assert!(
        (reopened[3] - full[3]).abs() < 2.0 && (reopened[0] - full[0]).abs() < 2.0,
        "reopen must reset live drag pose: {full:?} -> {reopened:?}"
    );
    open.set(false);
    for _ in 0..40 {
        frame(&state);
    }
    std::thread::sleep(std::time::Duration::from_millis(500));
    crate::utils::modal_motion::drain_pending_modal_unmounts();
    frame(&state);
    crate::app::runtime::clear_app_runtime();
    owner.dispose();
    crate::app::reactive::reset_registered_anchors();
    crate::app::portal_overlay::clear_overlay_stack();
    crate::style::set_css_transition_virtual_timeline(false);
}
