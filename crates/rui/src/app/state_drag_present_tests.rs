use super::*;
use crate as rui;

#[test]
#[ignore = "requires a native Windows GPU surface"]
fn free_drag_native_frames_follow_pointer_before_release() {
    use winit::application::ApplicationHandler;
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, EventLoop};
    use winit::platform::windows::EventLoopBuilderExtWindows;
    use winit::window::{WindowAttributes, WindowId};

    struct Harness {
        proxy: winit::event_loop::EventLoopProxy,
        callback: Option<Box<dyn FnMut(&dyn ActiveEventLoop, Option<WindowEvent>) -> bool>>,
    }
    impl ApplicationHandler for Harness {
        fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
            let window = Arc::from(
                event_loop
                    .create_window(
                        WindowAttributes::default()
                            .with_title("RUI free drag regression")
                            .with_surface_size(winit::dpi::LogicalSize::new(640.0, 480.0))
                            .with_visible(true),
                    )
                    .unwrap(),
            );
            self.callback = Some(run_drag_frames(window, self.proxy.clone()));
        }
        fn window_event(
            &mut self,
            event_loop: &dyn ActiveEventLoop,
            _: WindowId,
            event: WindowEvent,
        ) {
            if let Some(cb) = &mut self.callback {
                if cb(event_loop, Some(event)) {
                    event_loop.exit();
                }
            }
        }
        fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
            if let Some(cb) = &mut self.callback {
                cb(event_loop, None);
            }
        }
        fn proxy_wake_up(&mut self, _: &dyn ActiveEventLoop) {
            panic!("drag must produce a redraw before release");
        }
    }
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let proxy = event_loop.create_proxy();
    event_loop
        .run_app(Harness {
            proxy,
            callback: None,
        })
        .unwrap();
}

fn run_drag_frames(
    window: Arc<dyn Window>,
    proxy: winit::event_loop::EventLoopProxy,
) -> Box<
    dyn FnMut(&dyn winit::event_loop::ActiveEventLoop, Option<winit::event::WindowEvent>) -> bool,
> {
    use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
    use crate::app::runtime::{clear_app_runtime, set_app_runtime, AppRuntime};
    use crate::core::reactivity::{create_effect, create_root};
    use crate::prelude::*;
    use crate::utils::drag::collect_hits;
    use std::{cell::RefCell, rc::Rc};

    fn frame(
        state: &Rc<RefCell<Option<AppState>>>,
        pointer: [f32; 2],
        render: bool,
    ) -> crate::renderer::offscreen::RgbaImage {
        drain_deferred_view_remounts();
        let mut opt = state.borrow_mut();
        let s = opt.as_mut().unwrap();
        if render {
            s.render(
                pointer,
                ScrollbarUpdateMode::ViewportsOnly,
                None,
                false,
                false,
                false,
            )
            .unwrap();
        }
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
            for p in image.pixels.chunks_exact_mut(4) {
                p.swap(0, 2);
            }
        }
        image
    }

    reset_registered_anchors();
    let _ = take_frame_paint_nodes();
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
    let (owner, ()) = create_root(|| {
        let a_pos = use_signal("native-free-a", || [59.0_f32, 600.0]);
        let b_pos = use_signal("native-free-b", || [131.0_f32, 632.0]);
        let live = Rc::clone(&state);
        crate::core::reactivity::mark_next_effect_as_ui_root();
        create_effect(move || {
            let drag = crate::core::reactivity::isolate_reads(|| {
                crate::ui! {
                    <DragProvider id="native-free-drag" threshold={0.0}
                        onDragEnd={move |ev: &crate::utils::drag::DragEvent| {
                            let pos = if ev.draggable_id == "arb-auto-1" { a_pos } else { b_pos };
                            let p = pos.peek();
                            pos.set([p[0] + ev.delta[0], p[1] + ev.delta[1]]);
                        }}>
                        <Box w="100%" h="1000px" relative={true} overflow="hidden">
                            <Draggable id="arb-auto-1" stack="auto">
                                <Box absolute={true} left={a_pos.get()[0]} top={a_pos.get()[1]} w="176px" h="92px"
                                    bg="#ffffff" border_style="solid" border_width="3px"
                                    border_color="#000000" rounded="12px"
                                    items="center" justify="center" overflow="hidden">
                                    <Label text_size="44px" font_weight="800">"A"</Label>
                                </Box>
                            </Draggable>
                            <Draggable id="arb-auto-2" stack="auto">
                                <Box absolute={true} left={b_pos.get()[0]} top={b_pos.get()[1]} w="176px" h="92px"
                                    bg="#ffffff" border_style="solid" border_width="3px"
                                    border_color="#000000" rounded="12px"
                                    items="center" justify="center" overflow="hidden">
                                    <Label text_size="44px" font_weight="800">"B"</Label>
                                </Box>
                            </Draggable>
                        </Box>
                    </DragProvider>
                }
            });
            let scroll: Element = crate::components::scroll_area("native-free-scroll")
                .w("100%")
                .h("100%")
                .child(Element::new().flex_col().children([drag]))
                .into();
            let root = Element::new()
                .w("100%")
                .h("100%")
                .relative()
                .bg([1.0; 4])
                .child(scroll)
                // Gallery's root Portal remains mounted with an empty Show.
                // Its full-size host must not occlude page repaint scissors.
                .child(crate::ui! {
                    <Portal id="native-empty-portal">
                        <Show when={false}><Label>"Hidden"</Label></Show>
                    </Portal>
                })
                .child(
                    Element::new()
                        .absolute()
                        .right(0.0)
                        .w(8.0)
                        .h(80.0)
                        .z_index(1000)
                        .bg([0.5, 0.5, 0.5, 1.0]),
                )
                .child(crate::utils::popup::mount_anchored_overlay(
                    Element::new()
                        .absolute()
                        .left(86.0)
                        .top(422.0)
                        .w(264.0)
                        .h(168.0)
                        .bg([1.0; 4])
                        .text("Pinned panel"),
                    900,
                ));
            live.borrow_mut().as_mut().unwrap().rebuild_ui(root);
        });
    });
    for _ in 0..3 {
        let _ = frame(&state, [-1.0, -1.0], true);
    }
    let initial = crate::core::tree::find_by_attr(
        &state.borrow().as_ref().unwrap().root_element,
        crate::utils::drag::ATTR_DRAG_DRAGGABLE,
        "arb-auto-1",
    )
    .unwrap()
    .layout_bounds;
    let scroll_y = initial[1] - 100.0;
    assert!(scroll_y > 400.0);
    crate::core::reactivity::use_signal(
        &crate::core::scroll_viewport::scroll_offset_y_signal_key("native-free-scroll"),
        || 0.0_f32,
    )
    .set_silent(scroll_y);
    crate::core::reactivity::use_signal(
        &crate::core::scroll_viewport::scroll_target_y_signal_key("native-free-scroll"),
        || 0.0_f32,
    )
    .set_silent(scroll_y);
    state
        .borrow_mut()
        .as_mut()
        .unwrap()
        .update_scroll_offset_in_place("native-free-scroll", [0.0, scroll_y]);
    for _ in 0..3 {
        let _ = frame(&state, [-1.0, -1.0], true);
    }
    let (_, user_rx) = std::sync::mpsc::channel();
    let mut app =
        crate::app::lifecycle::create_app(Default::default(), Element::new, proxy.clone(), user_rx);
    app.state = Rc::clone(&state);
    let window_id = state.borrow().as_ref().unwrap().window.id();
    app.pointer_window = Some(window_id);
    let hits = collect_hits(&state.borrow().as_ref().unwrap().root_element);
    let cases = ["arb-auto-1", "arb-auto-2"].map(|id| {
        hits.iter()
            .find(|h| h.id == id && h.is_draggable)
            .unwrap()
            .rect
            .to_bounds()
    });
    let mut source = 0;
    let mut bounds = cases[source];
    let mut grab = [10.0, 8.0];
    let scale = state.borrow().as_ref().unwrap().window.scale_factor() as f32;
    app.cursor_pos = winit::dpi::PhysicalPosition::new(
        ((bounds[0] + grab[0]) * scale) as f64,
        ((bounds[1] + grab[1]) * scale) as f64,
    );
    let mut idle_until = std::time::Instant::now() + std::time::Duration::from_millis(300);
    let mut started = false;
    // First move A over B and commit it. Then move B over A and away again
    // while the pointer is still down; the covered sibling must be restored
    // before mouseup (this is where the gallery used to leave a lower-left
    // retained rectangle and erase the sibling glyph).
    let mut deltas = [[100.0, 60.0], [200.0, 100.0], [160.0, 80.0]];
    let mut sample = 0usize;
    let mut pending = false;
    let mut owner = Some(owner);
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(5));
        proxy.wake_up();
    });
    Box::new(move |event_loop, event| {
        if sample >= deltas.len() {
            return true;
        }
        if matches!(event, Some(winit::event::WindowEvent::RedrawRequested)) {
            app.handle_redraw_requested(window_id, event_loop);
            if pending {
                let delta = deltas[sample];
                let pointer = [
                    bounds[0] + grab[0] + delta[0],
                    bounds[1] + grab[1] + delta[1],
                ];
                let image = frame(&state, pointer, false);
                let x = ((bounds[0] + delta[0] + 1.0) * scale) as u32;
                let y = ((bounds[1] + delta[1] + 46.0) * scale) as u32;
                let i = ((y * image.width + x) * 4) as usize;
                let pixel = &image.pixels[i..i + 4];
                assert!(
                    pixel[0] < 80 && pixel[1] < 80 && pixel[2] < 80,
                    "drag must paint before release: delta={delta:?} pixel={pixel:?}"
                );
                let x = ((bounds[0] + 1.0) * scale) as u32;
                let y = ((bounds[1] + 75.0) * scale) as u32;
                let i = ((y * image.width + x) * 4) as usize;
                let old = &image.pixels[i..i + 4];
                assert!(
                    old[..3].iter().all(|c| *c > 220),
                    "old body must clear before release: {old:?}"
                );
                let mut ink = 0;
                for ly in 15..62 {
                    for lx in 65..110 {
                        let x = ((bounds[0] + delta[0] + lx as f32) * scale) as u32;
                        let y = ((bounds[1] + delta[1] + ly as f32) * scale) as u32;
                        let i = ((y * image.width + x) * 4) as usize;
                        if image.pixels[i..i + 3].iter().all(|c| *c < 80) {
                            ink += 1;
                        }
                    }
                }
                assert!(
                    ink > 150,
                    "letter must follow the body before release: ink={ink}"
                );
                if source == 1 && sample == 1 {
                    // B has just moved away from A. A's committed position must
                    // be repainted during the drag, before B is released.
                    let a_origin = [cases[0][0] + 160.0, cases[0][1] + 80.0];
                    let mut sibling_ink = 0;
                    for ly in 15..62 {
                        for lx in 65..110 {
                            let x = ((a_origin[0] + lx as f32) * scale) as u32;
                            let y = ((a_origin[1] + ly as f32) * scale) as u32;
                            let i = ((y * image.width + x) * 4) as usize;
                            if image.pixels[i..i + 3].iter().all(|c| *c < 80) {
                                sibling_ink += 1;
                            }
                        }
                    }
                    assert!(
                        sibling_ink > 150,
                        "covered sibling must reappear before release: ink={sibling_ink}, a_origin={a_origin:?}, bounds={bounds:?}"
                    );
                }
                if source == 2 && sample == deltas.len() - 1 {
                    state.borrow_mut().as_mut().unwrap().mark_full_redraw();
                    let reference = frame(&state, pointer, true);
                    let different = image
                        .pixels
                        .chunks_exact(4)
                        .zip(reference.pixels.chunks_exact(4))
                        .filter(|(partial, full)| {
                            partial[..3]
                                .iter()
                                .zip(&full[..3])
                                .any(|(a, b)| a.abs_diff(*b) > 16)
                        })
                        .count();
                    assert_eq!(
                        different, 0,
                        "repeated partial drags must match a full repaint"
                    );
                }
                sample += 1;
                pending = false;
                if sample == deltas.len() {
                    app.handle_mouse_button(
                        window_id,
                        winit::event::ElementState::Released,
                        winit::event::MouseButton::Left,
                        Some(event_loop),
                    );
                    // Let the drop callback rebuild the committed left/top before
                    // starting the next drag. This is the real gallery sequence.
                    let _ = frame(&state, [-1.0, -1.0], true);
                    let _ = frame(&state, [-1.0, -1.0], true);
                    source += 1;
                    if source == 3 {
                        owner.take().unwrap().dispose();
                        clear_app_runtime();
                        reset_registered_anchors();
                        return true;
                    }
                    bounds = crate::utils::drag::collect_hits(
                        &state.borrow().as_ref().unwrap().root_element,
                    )
                    .into_iter()
                    .find(|h| {
                        h.id == if source == 1 {
                            "arb-auto-2"
                        } else {
                            "arb-auto-1"
                        } && h.is_draggable
                    })
                    .unwrap()
                    .rect
                    .to_bounds();
                    // A was committed to the right; B's top-left is exposed.
                    grab = [10.0, 8.0];
                    deltas = if source == 1 {
                        [[80.0, 60.0], [300.0, 200.0], [320.0, 220.0]]
                    } else {
                        [[-180.0, 120.0], [-120.0, 40.0], [100.0, -40.0]]
                    };
                    app.cursor_pos = winit::dpi::PhysicalPosition::new(
                        ((bounds[0] + grab[0]) * scale) as f64,
                        ((bounds[1] + grab[1]) * scale) as f64,
                    );
                    sample = 0;
                    started = false;
                    idle_until = std::time::Instant::now() + std::time::Duration::from_millis(300);
                }
            }
        }
        if event.is_none() {
            if !started {
                if std::time::Instant::now() < idle_until {
                    winit::application::ApplicationHandler::about_to_wait(&mut app, event_loop);
                    event_loop
                        .set_control_flow(winit::event_loop::ControlFlow::WaitUntil(idle_until));
                    return false;
                }
                assert!(!app.paint_clock_active);
                app.handle_mouse_button(
                    window_id,
                    winit::event::ElementState::Pressed,
                    winit::event::MouseButton::Left,
                    Some(event_loop),
                );
                started = true;
            }
            if !pending {
                let delta = deltas[sample];
                let pointer = [
                    bounds[0] + grab[0] + delta[0],
                    bounds[1] + grab[1] + delta[1],
                ];
                app.handle_cursor_moved(
                    window_id,
                    winit::dpi::PhysicalPosition::new(
                        (pointer[0] * scale) as f64,
                        (pointer[1] * scale) as f64,
                    ),
                    event_loop,
                );
                pending = true;
            }
            winit::application::ApplicationHandler::about_to_wait(&mut app, event_loop);
        }
        false
    })
}
