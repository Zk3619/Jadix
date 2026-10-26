#![cfg(feature = "components")]
//! Moving carousel frames must retain the same pixels as a full compositor draw.
use super::*;

#[test]
#[ignore = "requires a native Windows GPU surface; run explicitly with --ignored --nocapture"]
fn carousel_native_motion_keeps_text_and_matches_full_repaint() {
    use winit::application::ApplicationHandler;
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, EventLoop};
    use winit::platform::windows::EventLoopBuilderExtWindows;
    use winit::window::{WindowAttributes, WindowId};

    struct Harness;
    impl ApplicationHandler for Harness {
        fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
            for (variant, page_flush) in [
                (crate::components::carousel::CarouselVariant::Single, false),
                (crate::components::carousel::CarouselVariant::Multi, false),
                (crate::components::carousel::CarouselVariant::Stack, false),
                (crate::components::carousel::CarouselVariant::Multi, true),
            ] {
                let window = Arc::from(
                    event_loop
                        .create_window(
                            WindowAttributes::default()
                                .with_title("Jadix carousel regression")
                                .with_surface_size(winit::dpi::LogicalSize::new(640.0, 300.0))
                                .with_visible(false),
                        )
                        .unwrap(),
                );
                run_carousel_frames(window, variant, page_flush);
            }
            event_loop.exit();
        }
        fn window_event(&mut self, _: &dyn ActiveEventLoop, _: WindowId, _: WindowEvent) {}
    }
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    builder.build().unwrap().run_app(Harness).unwrap();
}
fn run_carousel_frames(
    window: Arc<dyn Window>,
    variant: crate::components::carousel::CarouselVariant,
    page_flush: bool,
) {
    use crate::app::pointer_events::{dispatch_pointer, hit_path, PointerEvent, PointerKind};
    use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
    use crate::app::runtime::{clear_app_runtime, set_app_runtime, AppRuntime};
    use crate::components::carousel::*;
    use crate::core::reactivity::{create_effect, create_root, use_signal};
    use std::{cell::RefCell, rc::Rc};

    fn frame(
        state: &Rc<RefCell<Option<AppState>>>,
        full: bool,
    ) -> crate::renderer::offscreen::RgbaImage {
        drain_deferred_view_remounts();
        let mut guard = state.borrow_mut();
        let s = guard.as_mut().unwrap();
        if full {
            s.mark_full_redraw();
        }
        s.render(
            [-1.0, -1.0],
            ScrollbarUpdateMode::ViewportsOnly,
            None,
            false,
            false,
            false,
        )
        .unwrap();
        readback_texture(
            &s.device,
            &s.queue,
            s.present_retain.color_texture_for_test().unwrap(),
            s.config.width,
            s.config.height,
            s.device.create_command_encoder(&Default::default()),
        )
        .unwrap()
    }

    reset_registered_anchors();
    crate::style::utility::set_frame_dt(0.016);
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
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
    let (owner, index) = create_root(|| {
        let index = use_signal("native-carousel-index", || 0usize);
        index.set_silent(0);
        let live = state.clone();
        crate::core::reactivity::mark_next_effect_as_ui_root();
        create_effect(move || {
            let mut builder = carousel_single("native-carousel")
                .variant(variant)
                .per_view(variant.default_per_view())
                .index_signal(index)
                .autoplay_ms(4000);
            if page_flush {
                builder = builder
                    .loop_slides(false)
                    .scroll_by(CarouselScrollBy::Page)
                    .page_snap(CarouselPageSnap::Viewport)
                    .end_align(CarouselEndAlign::Flush)
                    .autoplay_ms(0);
            }
            let labels = if page_flush || variant == CarouselVariant::Stack {
                &["Slide A", "Slide B", "Slide C", "Slide D", "Slide E"][..]
            } else {
                &["Slide A", "Slide B", "Slide C"][..]
            };
            for &label in labels {
                builder = builder.slide(move || {
                    crate::primitives::r#box::r#box()
                        .w("100%")
                        .h(128.0)
                        .bg([0.9, 0.9, 0.9, 1.0])
                        .items("center")
                        .justify("center")
                        .class("rounded-md")
                        .child(crate::primitives::text::text(label).color([0.0, 0.0, 0.0, 1.0]))
                        .into()
                });
            }
            live.borrow_mut().as_mut().unwrap().rebuild_ui(
                Element::new()
                    .w("100%")
                    .h("100%")
                    .bg([1.0; 4])
                    .p(24.0)
                    .child(Element::from(builder)),
            );
        });
        index
    });
    for _ in 0..4 {
        frame(&state, false);
    }
    let viewport = {
        let guard = state.borrow();
        crate::core::tree::find_by_attr(
            &guard.as_ref().unwrap().root_element,
            "data-slot",
            "carousel-content",
        )
        .unwrap()
        .layout_bounds
    };
    let scale = state.borrow().as_ref().unwrap().window.scale_factor() as f32;
    let mut failures = Vec::new();
    for step in 0..100 {
        if step == 0 || step == 30 || step == 60 {
            let next = {
                let guard = state.borrow();
                crate::core::tree::find_by_attr(
                    &guard.as_ref().unwrap().root_element,
                    "data-slot",
                    "carousel-next",
                )
                .unwrap()
                .on_click
                .clone()
                .unwrap()
            };
            (next.borrow_mut())();
            assert_eq!(
                index.peek(),
                if page_flush {
                    if step == 0 {
                        2
                    } else {
                        3
                    }
                } else {
                    (step / 30 + 1)
                        % if variant == CarouselVariant::Stack {
                            5
                        } else {
                            3
                        }
                }
            );
        }
        if step == 70 {
            let x = viewport[0] + viewport[2] * 0.7;
            let y = viewport[1] + viewport[3] * 0.5;
            let path = hit_path(&state.borrow().as_ref().unwrap().root_element, x, y);
            dispatch_pointer(
                &path,
                &mut PointerEvent::new(x, y, winit::event::MouseButton::Left, 0),
                PointerKind::Down,
            );
        }
        if (71..80).contains(&step) {
            crate::components::carousel::tick_carousel_drag(
                viewport[0]
                    + viewport[2]
                        * (0.7
                            - (step - 70) as f32
                                * if variant == CarouselVariant::Stack {
                                    0.002
                                } else {
                                    0.06
                                }),
                viewport[1] + viewport[3] * 0.5,
            );
        }
        if step == 80 {
            crate::components::carousel::clear_carousel_drag_if_any();
        }
        crate::style::advance_css_transition_timeline_ms(16.0);
        let partial = frame(&state, false);
        if variant == CarouselVariant::Stack && (71..80).contains(&step) {
            let guard = state.borrow();
            let content = crate::core::tree::find_by_attr(
                &guard.as_ref().unwrap().root_element,
                "data-slot",
                "carousel-content",
            )
            .unwrap();
            let cards: Vec<_> = content
                .children
                .iter()
                .filter(|c| c.attrs.get("data-carousel-stack-measure").is_none())
                .collect();
            let previous = (index.peek() + cards.len() - 1) % cards.len();
            assert!(
                cards[previous].style.opacity > 0.5,
                "previous stack card disappeared during a small drag"
            );
            // The exposed left part of the previous card is outside the
            // centered card. Check actual retained pixels, not just its style.
            let x = viewport[0] + viewport[2] * 0.18;
            let y = viewport[1] + viewport[3] * 0.5;
            let pixel = ((y * scale) as usize * partial.width as usize + (x * scale) as usize) * 4;
            assert!(
                partial.pixels[pixel] < 250,
                "previous stack card has no painted pixels"
            );
        }
        crate::style::utility::set_frame_dt(0.0);
        let full = frame(&state, true);
        crate::style::utility::set_frame_dt(0.016);
        if page_flush && step == 59 {
            let guard = state.borrow();
            let track = crate::core::tree::find_by_attr(
                &guard.as_ref().unwrap().root_element,
                "data-carousel-track",
                "1",
            )
            .unwrap();
            let expected = -(5.0 - 2.2) * viewport[2] / 2.2;
            assert!(
                (track.style.translate[0] - expected).abs() < 1.0,
                "the final card must reach the viewport's right edge"
            );
            let text_x = viewport[0] + viewport[2] - viewport[2] / 2.2 * 0.5;
            let text_y = viewport[1] + viewport[3] * 0.5;
            let last_label_ink = partial
                .pixels
                .chunks_exact(4)
                .enumerate()
                .filter(|(i, p)| {
                    let x = (*i % partial.width as usize) as f32 / scale;
                    let y = (*i / partial.width as usize) as f32 / scale;
                    (x - text_x).abs() < 45.0
                        && (y - text_y).abs() < 16.0
                        && p[0] < 80
                        && p[1] < 80
                        && p[2] < 80
                })
                .count();
            assert!(
                last_label_ink > 0,
                "the last card label must be painted inside the viewport"
            );
        }
        let changed = partial
            .pixels
            .chunks_exact(4)
            .zip(full.pixels.chunks_exact(4))
            .filter(|(a, b)| a.iter().zip(b.iter()).any(|(a, b)| a.abs_diff(*b) > 8))
            .count();
        let center_ink = partial
            .pixels
            .chunks_exact(4)
            .enumerate()
            .filter(|(i, p)| {
                let x = (*i % partial.width as usize) as f32 / scale;
                let y = (*i / partial.width as usize) as f32 / scale;
                x > viewport[0] + 8.0
                    && x < viewport[0] + viewport[2] - 8.0
                    && y > viewport[1] + viewport[3] * 0.25
                    && y < viewport[1] + viewport[3] * 0.75
                    && p[0] < 200
                    && p[1] < 200
                    && p[2] < 200
            })
            .count();
        if changed > 0 || center_ink == 0 {
            failures.push((step, changed, center_ink));
            if failures.len() == 1 {
                let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../target/carousel-present");
                std::fs::create_dir_all(&dir).unwrap();
                for (name, image) in [("partial", &partial), ("full", &full)] {
                    ::image::save_buffer(
                        dir.join(format!("{}-{step}-{name}.png", variant.as_str())),
                        &image.pixels,
                        image.width,
                        image.height,
                        ::image::ColorType::Rgba8,
                    )
                    .unwrap();
                }
            }
        }
    }
    clear_app_runtime();
    state.borrow_mut().take();
    owner.dispose();
    crate::style::set_css_transition_virtual_timeline(false);
    assert!(
        failures.is_empty(),
        "{variant:?} carousel frames (step, differing pixels, text pixels): {failures:?}"
    );
}
