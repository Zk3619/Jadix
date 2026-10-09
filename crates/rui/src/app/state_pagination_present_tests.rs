//! Pagination must retain hover on the first presented frame after a reactive page change.
use super::*;
use crate::renderer::offscreen::RgbaImage;
use std::{cell::RefCell, rc::Rc};

#[test]
#[ignore = "requires a native Windows GPU surface; run explicitly with --ignored --nocapture"]
fn pagination_native_first_page_change_frame_keeps_hover() {
    use winit::application::ApplicationHandler;
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, EventLoop};
    use winit::platform::windows::EventLoopBuilderExtWindows;
    use winit::window::{WindowAttributes, WindowId};

    struct Harness;
    impl ApplicationHandler for Harness {
        fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
            let window: Arc<dyn Window> = Arc::from(
                event_loop
                    .create_window(
                        WindowAttributes::default()
                            .with_title("RUI pagination frame regression")
                            .with_surface_size(winit::dpi::LogicalSize::new(720.0, 180.0))
                            .with_visible(false),
                    )
                    .unwrap(),
            );
            run_frames(window);
            event_loop.exit();
        }
        fn window_event(&mut self, _: &dyn ActiveEventLoop, _: WindowId, _: WindowEvent) {}
    }
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    builder.build().unwrap().run_app(Harness).unwrap();
}

fn run_frames(window: Arc<dyn Window>) {
    use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
    use crate::app::runtime::{clear_app_runtime, set_app_runtime, AppRuntime};
    use crate::core::reactivity::{create_effect, create_root, use_signal};

    fn nav<'a>(root: &'a Element, slot: &str) -> &'a Element {
        crate::core::tree::find_in_tree(root, |el| {
            el.attrs.get("data-slot").map(String::as_str) == Some(slot)
        })
        .unwrap()
    }
    fn frame(state: &Rc<RefCell<Option<AppState>>>, mouse: [f32; 2]) -> RgbaImage {
        crate::style::advance_css_transition_timeline_ms(16.0);
        drain_deferred_view_remounts();
        let mut opt = state.borrow_mut();
        let s = opt.as_mut().unwrap();
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
        readback_texture(
            &s.device,
            &s.queue,
            texture,
            s.size.width,
            s.size.height,
            s.device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor::default()),
        )
        .unwrap()
    }

    reset_registered_anchors();
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    crate::style::utility::set_frame_dt(0.016);
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
        let page = use_signal("native-pagination-page", || 5usize);
        crate::core::reactivity::mark_next_effect_as_ui_root();
        create_effect(move || {
            let pagination: Element = crate::components::pagination("native-pagination")
                .page_signal(page)
                .page_count(12)
                .range_counts(1, 1, 1)
                .auto_fit(false)
                .into();
            let root = Element::new()
                .w("100%")
                .h("100%")
                .bg("background")
                .child(Element::new().w("100%").h(80.0).child(pagination));
            live.borrow_mut().as_mut().unwrap().rebuild_ui(root);
        });
    });
    for _ in 0..4 {
        frame(&state, [-1.0, -1.0]);
    }
    for slot in ["pagination-previous", "pagination-next"] {
        let (mouse, pixel) = {
            let opt = state.borrow();
            let s = opt.as_ref().unwrap();
            let b = nav(&s.root_element, slot).layout_bounds;
            (
                [b[0] + b[2] * 0.5, b[1] + b[3] * 0.5],
                [
                    ((b[0] + 6.0) * s.ui_scale()) as u32,
                    ((b[1] + 6.0) * s.ui_scale()) as u32,
                ],
            )
        };
        let mut before = frame(&state, mouse);
        for _ in 0..16 {
            before = frame(&state, mouse);
        }
        let (bg, click) = {
            let opt = state.borrow();
            let button = nav(&opt.as_ref().unwrap().root_element, slot);
            assert!(button.style.bg_color[3] > 0.9);
            (button.style.bg_color, button.on_click.clone().unwrap())
        };
        click.borrow_mut()();
        let after = frame(&state, mouse);
        assert_eq!(
            nav(&state.borrow().as_ref().unwrap().root_element, slot)
                .style
                .bg_color,
            bg,
            "{slot}: first presented frame must retain hover after paging"
        );
        let at = ((pixel[1] * before.width + pixel[0]) * 4) as usize;
        assert_eq!(
            &before.pixels[at..at + 4],
            &after.pixels[at..at + 4],
            "{slot}: hover background pixels must not flash"
        );
        for _ in 0..16 {
            frame(&state, [-1.0, -1.0]);
        }
        assert!(
            nav(&state.borrow().as_ref().unwrap().root_element, slot)
                .style
                .bg_color[3]
                < 0.01,
            "leaving navigation must still clear hover"
        );
    }
    clear_app_runtime();
    owner.dispose();
    reset_registered_anchors();
    crate::style::utility::set_frame_dt(0.0);
    crate::style::set_css_transition_virtual_timeline(false);
}
