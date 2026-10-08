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
            self.callback = Some(run_drag_frames(window, self.proxy.clone(), event_loop));
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
    event_loop: &dyn winit::event_loop::ActiveEventLoop,
) -> Box<
    dyn FnMut(&dyn winit::event_loop::ActiveEventLoop, Option<winit::event::WindowEvent>) -> bool,
> {
    use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
    use crate::app::runtime::{clear_app_runtime, set_app_runtime, AppRuntime};
    use crate::core::reactivity::{create_effect, create_root};
    use crate::prelude::*;
    use crate::utils::drag::{collect_hits, end};
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
        let live = Rc::clone(&state);
        crate::core::reactivity::mark_next_effect_as_ui_root();
        create_effect(move || {
            let drag = crate::core::reactivity::isolate_reads(|| {
                crate::ui! {
                    <DragProvider id="native-free-drag" threshold={0.0}>
                        <Box w="100%" h="1000px" relative={true} overflow="hidden">
                            <Draggable id="arb-auto-1" stack="auto">
                                <Box absolute={true} left={59.0} top={100.0} w="176px" h="92px"
                                    bg="#ffffff" border_style="solid" border_width="3px"
                                    border_color="#000000" rounded="12px"
                                    items="center" justify="center" overflow="hidden">
                                    <Label text_size="44px" font_weight="800">"A"</Label>
                                </Box>
                            </Draggable>
                            <Draggable id="arb-auto-2" stack="auto">
                                <Box absolute={true} left={131.0} top={132.0} w="176px" h="92px"
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
    let initial_hits = collect_hits(&state.borrow().as_ref().unwrap().root_element);
    let initial = initial_hits
        .iter()
        .find(|h| h.id == "arb-auto-1" && h.is_draggable)
        .unwrap()
        .rect
        .to_bounds();
    let scroll_y = initial[1] - 100.0;
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
    let a = hits
        .iter()
        .find(|h| h.id == "arb-auto-1" && h.is_draggable)
        .unwrap();
    let bounds = a.rect.to_bounds();
    let grab = [10.0, 8.0];
    let scale = state.borrow().as_ref().unwrap().window.scale_factor() as f32;
    app.cursor_pos = winit::dpi::PhysicalPosition::new(
        ((bounds[0] + grab[0]) * scale) as f64,
        ((bounds[1] + grab[1]) * scale) as f64,
    );
    app.handle_mouse_button(
        window_id,
        winit::event::ElementState::Pressed,
        winit::event::MouseButton::Left,
        Some(event_loop),
    );
    let deltas = [[100.0, 60.0], [200.0, 100.0], [160.0, 80.0]];
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
                let y = ((bounds[1] + 46.0) * scale) as u32;
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
                sample += 1;
                pending = false;
                if sample == deltas.len() {
                    let _ = end(&[]);
                    owner.take().unwrap().dispose();
                    clear_app_runtime();
                    reset_registered_anchors();
                    return true;
                }
            }
        }
        if event.is_none() {
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
