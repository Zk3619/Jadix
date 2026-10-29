//! A foreground HUD must neither recapture nor corrupt its local blur source.
use super::*;

#[test]
#[ignore = "requires a native Windows GPU surface; run explicitly with --ignored"]
fn hud_damage_keeps_local_frost_and_partial_present() {
    use winit::application::ApplicationHandler;
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, EventLoop};
    use winit::platform::windows::EventLoopBuilderExtWindows;
    use winit::window::WindowAttributes;
    struct Harness;
    impl ApplicationHandler for Harness {
        fn can_create_surfaces(&mut self, events: &dyn ActiveEventLoop) {
            let window: Arc<dyn Window> = Arc::from(
                events
                    .create_window(
                        WindowAttributes::default()
                            .with_visible(false)
                            .with_surface_size(winit::dpi::LogicalSize::new(640.0, 560.0)),
                    )
                    .unwrap(),
            );
            run(window);
            events.exit();
        }
        fn window_event(
            &mut self,
            _: &dyn ActiveEventLoop,
            _: winit::window::WindowId,
            _: WindowEvent,
        ) {
        }
    }
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    builder.build().unwrap().run_app(Harness).unwrap();
}

fn pixels(s: &AppState) -> crate::renderer::offscreen::RgbaImage {
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

fn frame(s: &mut AppState) {
    s.render(
        [-1.0, -1.0],
        ScrollbarUpdateMode::ViewportsOnly,
        None,
        false,
        false,
        false,
    )
    .unwrap();
}

fn run(window: Arc<dyn Window>) {
    use crate::layout::layout_tree::{ATTR_PORTAL_KEY, ATTR_PORTAL_MOUNT};
    crate::app::portal_overlay::clear_overlay_stack();
    let _ = take_frame_paint_nodes();
    let mut s = crate::utils::block_on::block_on(AppState::new_bootstrap(
        window,
        crate::app::window_chrome::WindowOptions::default(),
    ));
    let source = Element::new()
        .absolute()
        .inset(0.0)
        .z_index(0)
        .children((0..7).map(|row| {
            Element::new()
                .absolute()
                .left(12.0)
                .top(24.0 + row as f32 * 32.0)
                .text("YELLOW WHITE ABCXYZ 1234567890")
                .text_size(20.0)
                .text_color([255, 255, 0, 255])
                .template_id(format!("source-{row}"))
        }));
    let mut scrim = Element::new()
        .absolute()
        .inset(0.0)
        .z_index(2)
        .flex_items("center")
        .justify("center")
        .child(
            Element::new()
                .w("75%")
                .h("75%")
                .rounded(12.0)
                .bg([1.0, 1.0, 1.0, 0.55])
                .backdrop_blur(12.0),
        );
    scrim.set_attr("jadix_portal", "1");
    scrim.set_attr(ATTR_PORTAL_MOUNT, "source-host");
    scrim.set_attr(ATTR_PORTAL_KEY, "local-frost");
    let host = Element::new()
        .relative()
        .w(400.0)
        .h(300.0)
        .top(90.0)
        .left(30.0)
        .bg([0.05, 0.2, 0.9, 1.0])
        .overflow_x("hidden")
        .overflow_y("hidden")
        .template_id("source-host")
        .children([source, scrim]);
    let mut hud = Element::new()
        .absolute()
        .left(260.0)
        .top(260.0)
        .w(280.0)
        .h(160.0)
        .z_index(1000000)
        .bg([0.1, 0.1, 0.15, 0.75])
        .p(12.0)
        .child(
            Element::new()
                .text("HUD 00")
                .text_size(18.0)
                .text_color([255; 4])
                .template_id("hud-label"),
        );
    hud.set_attr("jadix_portal", "1");
    hud.set_attr(ATTR_PORTAL_KEY, "hud");
    s.rebuild_ui(Element::new().w("100%").h("100%").children([host, hud]));
    for _ in 0..5 {
        frame(&mut s);
    }
    assert!(s.local_backdrop_valid);
    let baseline = pixels(&s);
    let captures = s.local_backdrop_capture_count;
    for tick in 1..5 {
        s.update_element_by_template_id("hud-label", |el| {
            el.text = Some(format!("HUD {tick:02}"));
            crate::core::tree::LayoutStatus::StyleChanged
        });
        frame(&mut s);
        assert!(s.last_frame_partial, "HUD update escalated to FullClear");
        assert_eq!(
            s.local_backdrop_capture_count, captures,
            "foreground HUD invalidated the source"
        );
        let current = pixels(&s);
        let scale = s.ui_scale();
        let changed = baseline
            .pixels
            .chunks_exact(4)
            .zip(current.pixels.chunks_exact(4))
            .enumerate()
            .filter(|(index, (a, b))| {
                let x = (*index as u32 % current.width) as f32 / scale;
                let y = (*index as u32 / current.width) as f32 / scale;
                (50.0..240.0).contains(&x)
                    && (140.0..350.0).contains(&y)
                    && a.iter().zip(b.iter()).any(|(x, y)| x.abs_diff(*y) > 6)
            })
            .count();
        assert_eq!(
            changed, 0,
            "HUD tick introduced a seam outside its footprint"
        );
    }
    s.update_element_by_template_id("source-2", |el| {
        el.text = Some("CHANGED SOURCE BEHIND FROST".to_owned());
        crate::core::tree::LayoutStatus::StyleChanged
    });
    frame(&mut s);
    assert!(
        s.last_frame_partial,
        "background edit must retain partial presentation"
    );
    assert!(
        s.local_backdrop_capture_count > captures,
        "background source edit did not invalidate frost"
    );
    let partial = pixels(&s);
    s.mark_full_redraw();
    frame(&mut s);
    let full = pixels(&s);
    let changed = partial
        .pixels
        .chunks_exact(4)
        .zip(full.pixels.chunks_exact(4))
        .filter(|(a, b)| a.iter().zip(b.iter()).any(|(x, y)| x.abs_diff(*y) > 6))
        .count();
    assert_eq!(
        changed, 0,
        "partial source recapture differs from complete composition"
    );
    crate::app::portal_overlay::clear_overlay_stack();
}
