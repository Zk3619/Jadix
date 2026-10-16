use super::*;

#[test]
#[ignore = "requires a native Windows GPU surface"]
fn kanban_drop_first_frame_restores_target_border() {
    native_sortable_frames(0);
}

#[test]
#[ignore = "requires a native Windows GPU surface"]
fn nested_sortable_preview_keeps_auto_height_cards_in_sync() {
    native_sortable_frames(1);
}

#[test]
#[ignore = "requires a native Windows GPU surface"]
fn delayed_drag_activates_after_small_move_without_another_pointer_event() {
    native_sortable_frames(2);
}

#[test]
#[ignore = "requires a native Windows GPU surface"]
fn empty_column_roundtrip_to_first_slot_preserves_row_gap() {
    native_sortable_frames(3);
}

#[test]
#[ignore = "requires a native Windows GPU surface"]
fn kanban_first_item_roundtrip_to_tail_keeps_preview_order() {
    native_sortable_frames(4);
}

#[test]
#[ignore = "requires a native Windows GPU surface"]
fn reordered_first_item_cross_list_keeps_remaining_row_stationary() {
    native_sortable_frames(5);
}

#[test]
#[ignore = "requires a native Windows GPU surface"]
fn committed_empty_column_transfer_back_preserves_preview_gap() {
    native_sortable_frames(6);
}

#[test]
#[ignore = "requires a native Windows GPU surface"]
fn sensor_button_paints_overlay_on_first_frame_after_scroll() {
    native_sortable_frames(7);
}

fn native_sortable_frames(scenario: u8) {
    use winit::application::ApplicationHandler;
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, EventLoop};
    use winit::platform::windows::EventLoopBuilderExtWindows;
    use winit::window::{WindowAttributes, WindowId};

    struct Harness(u8);
    impl ApplicationHandler for Harness {
        fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
            let width = std::env::var("JADIX_KANBAN_WIDTH")
                .ok()
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(1000.0);
            let height = std::env::var("JADIX_KANBAN_HEIGHT")
                .ok()
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(480.0);
            let window = Arc::from(
                event_loop
                    .create_window(
                        WindowAttributes::default()
                            .with_title("Jadix kanban drop regression")
                            .with_surface_size(winit::dpi::PhysicalSize::new(
                                width as u32,
                                height as u32,
                            ))
                            .with_visible(false),
                    )
                    .unwrap(),
            );
            run_kanban_drop_frames(window, event_loop.create_proxy(), event_loop, self.0);
            event_loop.exit();
        }
        fn window_event(&mut self, _: &dyn ActiveEventLoop, _: WindowId, _: WindowEvent) {}
    }
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    builder.build().unwrap().run_app(Harness(scenario)).unwrap();
}

fn run_kanban_drop_frames(
    window: Arc<dyn Window>,
    proxy: winit::event_loop::EventLoopProxy,
    event_loop: &dyn winit::event_loop::ActiveEventLoop,
    scenario: u8,
) {
    let nested = scenario == 1;
    if nested || matches!(scenario, 3..=6) {
        crate::style::set_css_transition_virtual_timeline(true);
        crate::style::set_css_transition_timeline_ms(0.0);
    }
    use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
    use crate::app::runtime::{clear_app_runtime, set_app_runtime, AppRuntime};
    use crate::core::reactivity::{create_effect, create_root, isolate_reads, use_signal};
    use crate::prelude::*;
    use crate::utils::drag::{begin, collect_hits, tick, BeginDrag};
    use std::{cell::RefCell, rc::Rc};

    fn frame(
        state: &Rc<RefCell<Option<AppState>>>,
        pointer: [f32; 2],
    ) -> crate::renderer::offscreen::RgbaImage {
        drain_deferred_view_remounts();
        let mut opt = state.borrow_mut();
        let s = opt.as_mut().unwrap();
        s.render_prof_last = std::time::Instant::now();
        let before = [
            s.render_prof_pre_ms,
            s.render_prof_layout_ms,
            s.render_prof_text_ms,
            s.render_prof_encode_ms,
            s.render_prof_acquire_ms,
            s.render_prof_present_ms,
            s.render_prof_layout_rebuild_ms,
            s.render_prof_layout_setup_ms,
            s.render_prof_layout_taffy_ms,
            s.render_prof_layout_collect_ms,
        ];
        let started = std::time::Instant::now();
        s.render(
            pointer,
            ScrollbarUpdateMode::ViewportsOnly,
            None,
            false,
            false,
            false,
        )
        .unwrap();
        let render_ms = started.elapsed().as_secs_f64() * 1000.0;
        let after = [
            s.render_prof_pre_ms,
            s.render_prof_layout_ms,
            s.render_prof_text_ms,
            s.render_prof_encode_ms,
            s.render_prof_acquire_ms,
            s.render_prof_present_ms,
            s.render_prof_layout_rebuild_ms,
            s.render_prof_layout_setup_ms,
            s.render_prof_layout_taffy_ms,
            s.render_prof_layout_collect_ms,
        ];
        let times: Vec<_> = after.iter().zip(before).map(|(a, b)| a - b).collect();
        eprintln!(
            "breakdown pre/layout/text/encode/acquire/present/rebuild/setup/taffy/collect={times:.2?}"
        );
        eprintln!(
            "text measures: probes={} misses={}",
            s.layout_tree.text_measure_calls.get(),
            s.layout_tree.text_measure_misses.get()
        );
        let readback_started = std::time::Instant::now();
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
        eprintln!(
            "frame {}x{}: render={render_ms:.2}ms readback={:.2}ms",
            s.config.width,
            s.config.height,
            readback_started.elapsed().as_secs_f64() * 1000.0
        );
        if matches!(
            texture.format(),
            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
        ) {
            for pixel in image.pixels.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
        }
        image
    }

    fn dark_top_edge_pixels(
        image: &crate::renderer::offscreen::RgbaImage,
        bounds: [f32; 4],
        scale: f32,
    ) -> usize {
        let x0 = ((bounds[0] + 12.0) * scale) as u32;
        let x1 = ((bounds[0] + bounds[2] - 12.0) * scale) as u32;
        let y0 = (bounds[1] * scale) as u32;
        let y1 = ((bounds[1] + 2.0) * scale).ceil() as u32;
        (y0..y1.min(image.height))
            .flat_map(|y| (x0..x1.min(image.width)).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                let offset = ((y * image.width + x) * 4) as usize;
                image.pixels[offset..offset + 3].iter().all(|v| *v < 180)
            })
            .count()
    }

    reset_registered_anchors();
    let _ = take_frame_paint_nodes();
    let state = Rc::new(RefCell::new(Some(crate::utils::block_on::block_on(
        AppState::new_bootstrap(
            Arc::clone(&window),
            super::super::super::window_chrome::WindowOptions::default(),
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
            let drag = isolate_reads(|| {
                crate::gallery_drag::DragShowcase(crate::gallery_drag::DragShowcaseProps::default())
            });
            let scroll: Element = crate::components::scroll_area("native-kanban-scroll")
                .w("100%")
                .h("100%")
                .child(Element::new().flex_col().child(drag))
                .into();
            live.borrow_mut().as_mut().unwrap().rebuild_ui(
                Element::new()
                    .w("100%")
                    .h("100%")
                    .relative()
                    .bg([1.0; 4])
                    .child(scroll)
                    .child(crate::ui! {
                        <Portal id="native-kanban-empty-portal">
                            <Show when={false}><Label>"Hidden"</Label></Show>
                        </Portal>
                    }),
            );
        });
    });
    for _ in 0..3 {
        let _ = frame(&state, [-1.0, -1.0]);
    }
    let hits = collect_hits(&state.borrow().as_ref().unwrap().root_element);
    if scenario == 7 {
        fn ghost_count(root: &Element) -> usize {
            let mut count = 0;
            crate::core::tree::walk_elements(root, |el| {
                if el.attrs.contains_key(crate::utils::drag::ATTR_DRAG_OVERLAY) {
                    count += 1;
                }
            });
            count
        }
        let idle_ghosts = ghost_count(&state.borrow().as_ref().unwrap().root_element);
        eprintln!("idle drag ghost count: {idle_ghosts}");
        let source = hits.iter().find(|h| h.id == "sensor-chip").unwrap();
        let scroll_y = source.rect.y - 160.0;
        for key in [
            crate::core::scroll_viewport::scroll_offset_y_signal_key("native-kanban-scroll"),
            crate::core::scroll_viewport::scroll_target_y_signal_key("native-kanban-scroll"),
        ] {
            use_signal(&key, || 0.0_f32).set_silent(scroll_y);
        }
        state
            .borrow_mut()
            .as_mut()
            .unwrap()
            .update_scroll_offset_in_place("native-kanban-scroll", [0.0, scroll_y]);
        for _ in 0..3 {
            let _ = frame(&state, [-1.0, -1.0]);
        }
        let mut source_rect = source.rect;
        source_rect.y -= scroll_y;
        let (button_bounds, button_id) = {
            let opt = state.borrow();
            let root = &opt.as_ref().unwrap().root_element;
            let button = crate::core::tree::find_in_tree(root, |el| {
                el.on_click.is_some()
                    && crate::core::tree::find_in_tree(el, |child| {
                        child.text.as_deref() == Some("sensor 启动")
                    })
                    .is_some()
            })
            .expect("sensor button");
            (button.layout_bounds, button.focus_id.clone().unwrap())
        };
        let pointer = [
            button_bounds[0] + button_bounds[2] * 0.5,
            button_bounds[1] + button_bounds[3] * 0.5 - scroll_y,
        ];
        let (_, user_rx) = std::sync::mpsc::channel();
        let mut app =
            crate::app::lifecycle::create_app(Default::default(), Element::new, proxy, user_rx);
        app.state = Rc::clone(&state);
        let scale = window.scale_factor() as f32;
        app.cursor_pos = winit::dpi::PhysicalPosition::new(
            (pointer[0] * scale) as f64,
            (pointer[1] * scale) as f64,
        );
        let _ = frame(&state, pointer);
        let hovered = use_signal(&format!("{button_id}_hover"), || false);
        assert!(hovered.peek(), "button must hover before sensor activation");
        app.handle_mouse_button(
            window.id(),
            winit::event::ElementState::Pressed,
            winit::event::MouseButton::Left,
            None,
        );
        let _ = frame(&state, pointer);
        app.handle_mouse_button(
            window.id(),
            winit::event::ElementState::Released,
            winit::event::MouseButton::Left,
            None,
        );
        assert!(
            crate::utils::drag::is_active(),
            "button must activate the sensor"
        );
        let image = frame(&state, pointer);
        assert!(
            !hovered.peek(),
            "active ghost must clear the button hover immediately"
        );
        assert!(
            crate::app::pointer_events::pointer_hit_node_set(
                &state.borrow().as_ref().unwrap().root_element,
                pointer[0],
                pointer[1]
            )
            .is_empty(),
            "CSS hover must not hit through an active ghost"
        );
        assert_eq!(idle_ghosts, 0, "idle providers must not mount empty ghosts");
        assert_eq!(
            ghost_count(&state.borrow().as_ref().unwrap().root_element),
            1
        );
        let opt = state.borrow();
        let ghost = crate::core::tree::find_in_tree(&opt.as_ref().unwrap().root_element, |el| {
            el.attrs
                .get(crate::utils::popup::ATTR_FLOATING)
                .map(String::as_str)
                == Some("gal-drag-sensor__ghost")
        })
        .expect("sensor overlay must mount on first frame");
        let bounds = ghost.layout_bounds;
        assert!(
            (bounds[0] - source_rect.x).abs() < 1.0 && (bounds[1] - source_rect.y).abs() < 1.0,
            "sensor overlay must appear over the visible source: {bounds:?}"
        );
        let dark_ink = (0..image.height)
            .flat_map(|y| (0..image.width).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                x as f32 >= bounds[0] * scale
                    && (x as f32) < (bounds[0] + bounds[2]) * scale
                    && y as f32 >= bounds[1] * scale
                    && (y as f32) < (bounds[1] + bounds[3]) * scale
            })
            .filter(|&(x, y)| {
                let offset = ((y * image.width + x) * 4) as usize;
                image.pixels[offset..offset + 3].iter().all(|v| *v < 100)
            })
            .count();
        assert!(
            dark_ink > 10,
            "sensor overlay must paint opaque text/border on its first frame"
        );
        drop(opt);
        crate::utils::drag::cancel();
        let _ = frame(&state, pointer);
        assert_eq!(
            ghost_count(&state.borrow().as_ref().unwrap().root_element),
            0
        );
        assert!(hovered.peek(), "hover must resume after the drag ends");
        owner.dispose();
        clear_app_runtime();
        reset_registered_anchors();
        return;
    }
    if matches!(scenario, 2 | 3 | 5 | 6) {
        use winit::application::ApplicationHandler;
        let source_id = if scenario == 2 {
            "act-or"
        } else {
            "veto-open:Move me"
        };
        let source = hits.iter().find(|h| h.id == source_id).unwrap();
        let scroll_y = source.rect.y - 120.0;
        for key in [
            crate::core::scroll_viewport::scroll_offset_y_signal_key("native-kanban-scroll"),
            crate::core::scroll_viewport::scroll_target_y_signal_key("native-kanban-scroll"),
        ] {
            use_signal(&key, || 0.0_f32).set_silent(scroll_y);
        }
        state
            .borrow_mut()
            .as_mut()
            .unwrap()
            .update_scroll_offset_in_place("native-kanban-scroll", [0.0, scroll_y]);
        for _ in 0..3 {
            let _ = frame(&state, [-1.0, -1.0]);
        }
        let hits = collect_hits(&state.borrow().as_ref().unwrap().root_element);
        let source = hits.iter().find(|h| h.id == source_id).unwrap();
        let start = [source.rect.x + 10.0, source.rect.y + 8.0];
        let original_gap = hits
            .iter()
            .find(|h| h.id == "veto-open:Locked")
            .map(|locked| locked.rect.y - source.rect.y - source.rect.height);
        begin(BeginDrag {
            scope_id: source.scope_id.clone(),
            draggable_id: source.id.clone(),
            pointer: start,
            grab_offset: [10.0, 8.0],
            start_bounds: source.rect.to_bounds(),
            from_list: source.list_id.clone(),
            from_index: source.index,
            item_type: source.item_type.clone(),
            group: source.group.clone(),
            capture_id: "hold-activation-test".into(),
        });
        let pointer = [start[0] + 10.0, start[1]];
        let viewport = state.borrow().as_ref().unwrap().logical_size();
        if scenario == 6 {
            assert!(tick(
                pointer[0],
                pointer[1],
                &hits,
                [viewport.0, viewport.1]
            ));
            let _ = frame(&state, pointer);
            let empty = hits.iter().find(|h| h.id == "veto-empty").unwrap();
            let dest = [empty.rect.x + 20.0, source.rect.y + 2.0];
            assert!(tick(dest[0], dest[1], &hits, [viewport.0, viewport.1]));
            let _ = frame(&state, dest);
            crate::style::set_css_transition_timeline_ms(200.0);
            let _ = frame(&state, dest);
            let hits = collect_hits(&state.borrow().as_ref().unwrap().root_element);
            assert!(crate::utils::drag::end(&hits));
            for _ in 0..3 {
                let _ = frame(&state, [-1.0, -1.0]);
            }
            assert_eq!(
                use_signal("gal_drag_veto_open", Vec::<String>::new).peek(),
                vec!["Locked".to_string()]
            );
            assert_eq!(
                use_signal("gal_drag_veto_empty", Vec::<String>::new).peek(),
                vec!["Move me".to_string()]
            );
            let hits = collect_hits(&state.borrow().as_ref().unwrap().root_element);
            let source = hits.iter().find(|h| h.id == "veto-empty:Move me").unwrap();
            let locked = hits.iter().find(|h| h.id == "veto-open:Locked").unwrap();
            let start = [source.rect.x + 10.0, source.rect.y + 8.0];
            begin(BeginDrag {
                scope_id: source.scope_id.clone(),
                draggable_id: source.id.clone(),
                pointer: start,
                grab_offset: [10.0, 8.0],
                start_bounds: source.rect.to_bounds(),
                from_list: source.list_id.clone(),
                from_index: source.index,
                item_type: source.item_type.clone(),
                group: source.group.clone(),
                capture_id: "transfer-back".into(),
            });
            let back = [
                locked.rect.x + 10.0,
                locked.rect.y + locked.rect.height + 14.0,
            ];
            assert!(tick(back[0], back[1], &hits, [viewport.0, viewport.1]));
            for time in [200.0, 275.0, 400.0] {
                crate::style::set_css_transition_timeline_ms(time);
                let _ = frame(&state, back);
                let opt = state.borrow();
                let root = &opt.as_ref().unwrap().root_element;
                let locked = crate::core::tree::find_in_tree(root, |e| {
                    e.attrs
                        .get(crate::utils::drag::ATTR_DRAG_DRAGGABLE)
                        .map(String::as_str)
                        == Some("veto-open:Locked")
                })
                .unwrap();
                let preview = crate::core::tree::find_in_tree(root, |e| {
                    e.attrs
                        .contains_key(crate::utils::drag::ATTR_DRAG_HOLE_PREVIEW)
                })
                .expect("dimmed target preview");
                let gap = preview.layout_bounds[1] + preview.style.translate[1]
                    - locked.layout_bounds[1]
                    - locked.style.translate[1]
                    - locked.layout_bounds[3];
                assert!(
                    (gap - original_gap.unwrap()).abs() <= 1.0,
                    "second drag at {time}ms must keep author gap: expected={original_gap:?}, actual={gap}"
                );
            }
            crate::utils::drag::cancel();
            owner.dispose();
            clear_app_runtime();
            reset_registered_anchors();
            crate::style::set_css_transition_virtual_timeline(false);
            return;
        }
        if scenario == 5 {
            assert!(tick(
                pointer[0],
                pointer[1],
                &hits,
                [viewport.0, viewport.1]
            ));
            let _ = frame(&state, pointer);
            let locked = hits.iter().find(|h| h.id == "veto-open:Locked").unwrap();
            let tail = [start[0], locked.rect.y + locked.rect.height + 8.0];
            assert!(tick(tail[0], tail[1], &hits, [viewport.0, viewport.1]));
            assert_eq!(
                use_signal(
                    &format!("{}__drag_hover_index", source.scope_id),
                    || None::<usize>
                )
                .peek(),
                Some(1),
                "first row must be previewed at the tail before crossing"
            );
            let _ = frame(&state, tail);
            crate::style::set_css_transition_timeline_ms(200.0);
            let _ = frame(&state, tail);
            crate::style::set_css_transition_timeline_ms(400.0);
            let _ = frame(&state, tail);
            let visual_y = || {
                let opt = state.borrow();
                let locked =
                    crate::core::tree::find_in_tree(&opt.as_ref().unwrap().root_element, |e| {
                        e.attrs
                            .get(crate::utils::drag::ATTR_DRAG_DRAGGABLE)
                            .map(String::as_str)
                            == Some("veto-open:Locked")
                    })
                    .unwrap();
                locked.layout_bounds[1] + locked.style.translate[1] - scroll_y
            };
            let before_cross = visual_y();
            assert!(
                (before_cross - source.rect.y).abs() <= 1.0,
                "Locked must already occupy the first slot: locked={before_cross}, first={}",
                source.rect.y
            );
            let hits = collect_hits(&state.borrow().as_ref().unwrap().root_element);
            let empty = hits.iter().find(|h| h.id == "veto-empty").unwrap();
            let dest = [empty.rect.x + 20.0, source.rect.y + 2.0];
            assert!(tick(dest[0], dest[1], &hits, [viewport.0, viewport.1]));
            for time in [400.0, 440.0, 480.0, 560.0, 750.0] {
                crate::style::set_css_transition_timeline_ms(time);
                let _ = frame(&state, dest);
                let actual = visual_y();
                assert!(
                    (actual - before_cross).abs() <= 1.0,
                    "cross {time}ms: already-packed Locked must stay stationary; before={before_cross}, actual={actual}"
                );
            }
            crate::utils::drag::cancel();
            owner.dispose();
            clear_app_runtime();
            reset_registered_anchors();
            crate::style::set_css_transition_virtual_timeline(false);
            return;
        }
        if scenario == 3 {
            let empty = hits.iter().find(|h| h.id == "veto-empty").unwrap();
            let dest = [empty.rect.x + 20.0, source.rect.y + 2.0];
            assert!(tick(dest[0], dest[1], &hits, [viewport.0, viewport.1]));
            let _ = frame(&state, dest);
            crate::style::set_css_transition_timeline_ms(75.0);
            let _ = frame(&state, dest);
            let mid_hits = collect_hits(&state.borrow().as_ref().unwrap().root_element);
            let locked_mid = mid_hits
                .iter()
                .find(|h| h.id == "veto-open:Locked")
                .unwrap();
            assert!(
                (locked_mid.rect.y - source.rect.y).abs() <= 1.0,
                "cross-list vacate must close the layout slot once; y={} packed={}",
                locked_mid.rect.y,
                source.rect.y
            );
            let hits = collect_hits(&state.borrow().as_ref().unwrap().root_element);
            let back = [start[0], start[1] - 6.0];
            assert!(tick(back[0], back[1], &hits, [viewport.0, viewport.1]));
            for (index, time) in [75.0, 110.0, 150.0, 250.0].into_iter().enumerate() {
                crate::style::set_css_transition_timeline_ms(time);
                let _ = frame(&state, back);
                let opt = state.borrow();
                let root = &opt.as_ref().unwrap().root_element;
                let row = |id: &str| {
                    crate::core::tree::find_in_tree(root, |e| {
                        e.attrs
                            .get(crate::utils::drag::ATTR_DRAG_DRAGGABLE)
                            .map(String::as_str)
                            == Some(id)
                    })
                    .unwrap()
                };
                let source = row(source_id);
                let locked = row("veto-open:Locked");
                let gap = locked.layout_bounds[1] + locked.style.translate[1]
                    - source.layout_bounds[1]
                    - source.style.translate[1]
                    - source.layout_bounds[3];
                assert!(
                    gap <= original_gap.unwrap() + 1.0
                        && (index < 3 || (gap - original_gap.unwrap()).abs() < 1.0),
                    "return frame {index}: original gap={:?} got={gap}; source={:?} translate={:?}, locked={:?} translate={:?}",
                    original_gap,
                    source.layout_bounds,
                    source.style.translate,
                    locked.layout_bounds,
                    locked.style.translate
                );
            }
            crate::utils::drag::cancel();
            owner.dispose();
            clear_app_runtime();
            reset_registered_anchors();
            crate::style::set_css_transition_virtual_timeline(false);
            return;
        }
        assert!(!tick(
            pointer[0],
            pointer[1],
            &hits,
            [viewport.0, viewport.1]
        ));
        let (_, user_rx) = std::sync::mpsc::channel();
        let mut app =
            crate::app::lifecycle::create_app(Default::default(), Element::new, proxy, user_rx);
        app.state = Rc::clone(&state);
        app.left_button_down = true;
        app.about_to_wait(event_loop);
        assert!(
            matches!(
                event_loop.control_flow(),
                winit::event_loop::ControlFlow::WaitUntil(_)
            ),
            "pending hold must schedule a wake without pointer events"
        );
        std::thread::sleep(std::time::Duration::from_millis(450));
        app.about_to_wait(event_loop);
        assert!(
            crate::utils::drag::is_active(),
            "400ms hold after a 10px move must activate without another pointer event"
        );
        assert!(
            app.force_redraw_count > 0,
            "timer activation must pump the first overlay frame"
        );
        let _ = frame(&state, pointer);
        assert!(
            crate::core::tree::find_in_tree(&state.borrow().as_ref().unwrap().root_element, |e| e
                .attrs
                .contains_key(crate::utils::drag::ATTR_DRAG_OVERLAY),)
            .is_some(),
            "timer activation must mount the drag overlay"
        );
        crate::utils::drag::cancel();
        owner.dispose();
        clear_app_runtime();
        reset_registered_anchors();
        return;
    }
    let todo = hits
        .iter()
        .find(|h| h.id == if nested { "tree-inbox" } else { "todo" } && h.index.is_none())
        .unwrap();
    let scroll_y = todo.rect.y - 120.0;
    for key in [
        crate::core::scroll_viewport::scroll_offset_y_signal_key("native-kanban-scroll"),
        crate::core::scroll_viewport::scroll_target_y_signal_key("native-kanban-scroll"),
    ] {
        use_signal(&key, || 0.0_f32).set_silent(scroll_y);
    }
    state
        .borrow_mut()
        .as_mut()
        .unwrap()
        .update_scroll_offset_in_place("native-kanban-scroll", [0.0, scroll_y]);
    let mut idle = frame(&state, [-1.0, -1.0]);
    for _ in 0..2 {
        idle = frame(&state, [-1.0, -1.0]);
    }
    let hits = collect_hits(&state.borrow().as_ref().unwrap().root_element);
    let source = hits
        .iter()
        .find(|h| {
            h.id == if nested {
                "tree-inbox:Beta"
            } else if scenario == 4 {
                "todo:Draft RFC"
            } else {
                "doing:Write tests"
            }
        })
        .unwrap();
    let todo = hits
        .iter()
        .find(|h| {
            h.id == if nested {
                "tree-later"
            } else if scenario == 4 {
                "doing"
            } else {
                "todo"
            } && h.index.is_none()
        })
        .unwrap();
    let target_bounds = todo.rect.to_bounds();
    let scale = state.borrow().as_ref().unwrap().window.scale_factor() as f32;
    let idle_edge = dark_top_edge_pixels(&idle, target_bounds, scale);
    let drop_x = std::env::var("JADIX_KANBAN_DROP_X")
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(0.5);
    let drop_y = std::env::var("JADIX_KANBAN_DROP_Y")
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(90.0);
    let pointer = [
        todo.rect.x + todo.rect.width * drop_x,
        todo.rect.y + if nested { 2.0 } else { drop_y },
    ];
    begin(BeginDrag {
        scope_id: source.scope_id.clone(),
        draggable_id: source.id.clone(),
        pointer: [source.rect.x + 10.0, source.rect.y + 8.0],
        grab_offset: [10.0, 8.0],
        start_bounds: source.rect.to_bounds(),
        from_list: source.list_id.clone(),
        from_index: source.index,
        item_type: source.item_type.clone(),
        group: source.group.clone(),
        capture_id: "kanban-test".into(),
    });
    let (width, height) = state.borrow().as_ref().unwrap().logical_size();
    assert!(tick(pointer[0], pointer[1], &hits, [width, height]));
    let mut hovered = frame(&state, pointer);
    if scenario == 4 {
        crate::style::set_css_transition_timeline_ms(200.0);
        let _ = frame(&state, pointer);
        let hits = collect_hits(&state.borrow().as_ref().unwrap().root_element);
        let origin = hits
            .iter()
            .find(|h| h.id == "todo" && h.index.is_none())
            .unwrap();
        let review = hits.iter().find(|h| h.id == "todo:Review PR").unwrap();
        let back = [
            origin.rect.x + 30.0,
            review.rect.y + review.rect.height * 0.8,
        ];
        assert!(tick(back[0], back[1], &hits, [width, height]));
        assert_eq!(
            use_signal(&format!("{}__drag_hover_list", source.scope_id), || None::<
                String,
            >)
            .peek()
            .as_deref(),
            Some("todo")
        );
        assert_eq!(
            use_signal(
                &format!("{}__drag_hover_index", source.scope_id),
                || None::<usize>
            )
            .peek(),
            Some(1),
            "returning below the remaining row must select the tail, even while the source slot is collapsed"
        );
        for time in [200.0, 240.0, 280.0, 360.0, 550.0] {
            crate::style::set_css_transition_timeline_ms(time);
            let _ = frame(&state, back);
            let opt = state.borrow();
            let root = &opt.as_ref().unwrap().root_element;
            let row = |id: &str| {
                crate::core::tree::find_in_tree(root, |e| {
                    e.attrs
                        .get(crate::utils::drag::ATTR_DRAG_DRAGGABLE)
                        .map(String::as_str)
                        == Some(id)
                })
                .unwrap()
            };
            let draft = row("todo:Draft RFC");
            let review = row("todo:Review PR");
            let gap = draft.layout_bounds[1] + draft.style.translate[1]
                - review.layout_bounds[1]
                - review.style.translate[1]
                - review.layout_bounds[3];
            assert!(
                (gap - 6.0).abs() <= 1.0,
                "return {time}ms must preserve the row gap, got {gap}; draft={:?} translate={:?}, review={:?} translate={:?}",
                draft.layout_bounds,
                draft.style.translate,
                review.layout_bounds,
                review.style.translate
            );
        }
        {
            let opt = state.borrow();
            let root = &opt.as_ref().unwrap().root_element;
            let row = |id: &str| {
                crate::core::tree::find_in_tree(root, |e| {
                    e.attrs
                        .get(crate::utils::drag::ATTR_DRAG_DRAGGABLE)
                        .map(String::as_str)
                        == Some(id)
                })
                .unwrap()
            };
            let draft = row("todo:Draft RFC");
            let review = row("todo:Review PR");
            let draft_y = draft.layout_bounds[1] + draft.style.translate[1];
            let review_y = review.layout_bounds[1] + review.style.translate[1];
            assert!(
                draft_y > review_y + review.layout_bounds[3],
                "after returning at tail, dimmed Draft must stay below Review without another pointer event: draft={:?} translate={:?}, review={:?} translate={:?}",
                draft.layout_bounds,
                draft.style.translate,
                review.layout_bounds,
                review.style.translate
            );
        }
        let hits = collect_hits(&state.borrow().as_ref().unwrap().root_element);
        assert!(crate::utils::drag::end(&hits));
        assert_eq!(
            use_signal("gal_drag_todo", Vec::<String>::new).peek(),
            vec!["Review PR".to_string(), "Draft RFC".to_string()],
            "release must commit the previewed tail slot"
        );
        owner.dispose();
        clear_app_runtime();
        reset_registered_anchors();
        crate::style::set_css_transition_virtual_timeline(false);
        return;
    }
    if nested {
        crate::style::set_css_transition_timeline_ms(75.0);
        hovered = frame(&state, pointer);
        let mut opt = state.borrow_mut();
        let s = opt.as_mut().unwrap();
        assert!(
            s.layout_tree.text_measure_misses.get() < 256,
            "nested height animation must reuse Gallery text measures; misses={}",
            s.layout_tree.text_measure_misses.get()
        );
        let gamma = crate::core::tree::find_in_tree(&s.root_element, |e| {
            e.attrs
                .get(crate::utils::drag::ATTR_DRAG_DRAGGABLE)
                .map(String::as_str)
                == Some("tree-later:Gamma")
        })
        .unwrap();
        assert!(
            gamma.style.translate[1] > 1.0 && gamma.style.translate[1] < 21.0,
            "cross-list target must have an animated middle position: {:?}",
            gamma.style.translate
        );
        s.mark_full_redraw();
        drop(opt);
        let full_hover = frame(&state, pointer);
        let different = hovered
            .pixels
            .chunks_exact(4)
            .zip(full_hover.pixels.chunks_exact(4))
            .filter(|(a, b)| a[..3].iter().zip(&b[..3]).any(|(a, b)| a.abs_diff(*b) > 16))
            .count();
        if different > 0 {
            for (image, name) in [
                (&hovered, "nested-preview-partial.png"),
                (&full_hover, "nested-preview-full.png"),
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
        assert_eq!(
            different, 0,
            "first nested preview frame must match a full repaint"
        );
        let hits = collect_hits(&state.borrow().as_ref().unwrap().root_element);
        let alpha = hits.iter().find(|h| h.id == "tree-inbox:Alpha").unwrap();
        let back = [alpha.rect.x + 8.0, alpha.rect.y + 2.0];
        let viewport = state.borrow().as_ref().unwrap().logical_size();
        tick(back[0], back[1], &hits, [viewport.0, viewport.1]);
        for time in [75.0, 110.0, 150.0, 250.0] {
            crate::style::set_css_transition_timeline_ms(time);
            let _ = frame(&state, back);
            let opt = state.borrow();
            let s = opt.as_ref().unwrap();
            let card = crate::core::tree::find_in_tree(&s.root_element, |e| {
                e.attrs
                    .get(crate::utils::drag::ATTR_DRAG_DRAGGABLE)
                    .map(String::as_str)
                    == Some("folder:Later")
            })
            .unwrap();
            let row = crate::core::tree::find_in_tree(&s.root_element, |e| {
                e.attrs
                    .get(crate::utils::drag::ATTR_DRAG_DRAGGABLE)
                    .map(String::as_str)
                    == Some("tree-later:Gamma")
            })
            .unwrap();
            let row_bottom = row.layout_bounds[1] + row.style.translate[1] + row.layout_bounds[3];
            assert!(
                row_bottom <= card.layout_bounds[1] + card.layout_bounds[3] + 1.0,
                "return at {time}ms: Gamma must stay inside shrinking Later: row={:?} translate={:?} card={:?}",
                row.layout_bounds,
                row.style.translate,
                card.layout_bounds
            );
        }
        let hits = collect_hits(&state.borrow().as_ref().unwrap().root_element);
        tick(pointer[0], pointer[1], &hits, [viewport.0, viewport.1]);
        let _ = frame(&state, pointer);
    } else {
        for _ in 0..2 {
            hovered = frame(&state, pointer);
        }
    }
    if !nested && drop_y > 30.0 {
        let hovered_edge = dark_top_edge_pixels(&hovered, target_bounds, scale);
        assert!(
            hovered_edge > idle_edge + (target_bounds[2] * scale * 0.5) as usize,
            "test must paint Todo's active border before releasing: idle={idle_edge}, hovered={hovered_edge}"
        );
    }
    let (_, user_rx) = std::sync::mpsc::channel();
    let mut app =
        crate::app::lifecycle::create_app(Default::default(), Element::new, proxy, user_rx);
    app.state = Rc::clone(&state);
    let window_id = state.borrow().as_ref().unwrap().window.id();
    app.cursor_pos =
        winit::dpi::PhysicalPosition::new((pointer[0] * scale) as f64, (pointer[1] * scale) as f64);
    app.left_button_down = true;
    let commit_started = std::time::Instant::now();
    app.handle_mouse_button(
        window_id,
        winit::event::ElementState::Released,
        winit::event::MouseButton::Left,
        Some(event_loop),
    );
    eprintln!(
        "commit={:.2}ms",
        commit_started.elapsed().as_secs_f64() * 1000.0
    );
    assert!(!crate::utils::drag::is_dragging());
    assert!(use_signal(
        if nested {
            "gal_drag_nest_later"
        } else {
            "gal_drag_todo"
        },
        Vec::<String>::new
    )
    .peek()
    .iter()
    .any(|s| s == if nested { "Beta" } else { "Write tests" }));
    let source_items = use_signal(
        if nested {
            "gal_drag_nest_inbox"
        } else {
            "gal_drag_doing"
        },
        Vec::<String>::new,
    )
    .peek();
    assert!(!source_items
        .iter()
        .any(|s| s == if nested { "Beta" } else { "Write tests" }));
    let partial = frame(&state, pointer);
    let restored_edge = if nested {
        idle_edge
    } else {
        dark_top_edge_pixels(&partial, target_bounds, scale)
    };
    state.borrow_mut().as_mut().unwrap().mark_full_redraw();
    let full = frame(&state, pointer);
    let different = partial
        .pixels
        .chunks_exact(4)
        .zip(full.pixels.chunks_exact(4))
        .filter(|(a, b)| a[..3].iter().zip(&b[..3]).any(|(a, b)| a.abs_diff(*b) > 16))
        .count();
    if different > 0 || restored_edge != idle_edge {
        for (image, name) in [
            (&partial, "kanban-drop-partial.png"),
            (&full, "kanban-drop-full.png"),
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
    owner.dispose();
    clear_app_runtime();
    reset_registered_anchors();
    crate::style::set_css_transition_virtual_timeline(false);
    assert_eq!(
        restored_edge, idle_edge,
        "the first committed frame must restore Todo's entire top border"
    );
    assert_eq!(
        different, 0,
        "first committed frame must match a full repaint"
    );
}
