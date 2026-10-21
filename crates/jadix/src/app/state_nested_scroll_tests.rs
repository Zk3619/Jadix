//! Nested scroll pixels must agree with a full composite at every offset.
use super::*;
use crate::core::scroll_viewport::{scroll_offset_y_signal_key, SCROLL_VIEWPORT_FOCUS_PREFIX};
use crate::renderer::offscreen::RgbaImage;

fn viewport(id: &str, height: f32, child: Element) -> Element {
    let mut el = Element::new()
        .w(300.0)
        .h(height)
        .flex_shrink(0.0)
        .overflow_y("scroll")
        .overflow_x("hidden")
        .child(child);
    el.focus_id = Some(format!("{SCROLL_VIEWPORT_FOCUS_PREFIX}{id}"));
    use_signal(&scroll_offset_y_signal_key(id), || 0.0_f32).set_silent(0.0);
    el
}

fn fixture() -> Element {
    let scene = Element::new()
        .w(300.0)
        .h(1200.0)
        .bg("#f6f3e9")
        .children((0..40).map(|row| {
            Element::new()
                .absolute()
                .left(8.0)
                .top(row as f32 * 30.0)
                .w(280.0)
                .h(24.0)
                .text(format!("Absolute text row {row}"))
                .white_space("nowrap")
                .text_size(15.0)
                .text_line_height(24.0)
                .text_color("#234f42")
        }));
    Element::new()
        .w("100%")
        .h("100%")
        .bg("#ffffff")
        .child(viewport(
            "nested-pixel-outer",
            240.0,
            Element::new()
                .w(300.0)
                .flex_col()
                .child(Element::new().w(300.0).h(100.0).flex_shrink(0.0))
                .child(viewport("nested-pixel-inner", 180.0, scene))
                .child(Element::new().w(300.0).h(600.0).flex_shrink(0.0)),
        ))
}

fn render(s: &mut AppState, scroll: bool) -> RgbaImage {
    s.render(
        [-1.0; 2],
        ScrollbarUpdateMode::ViewportsOnly,
        None,
        scroll,
        false,
        scroll,
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

fn run(window: Arc<dyn Window>) {
    crate::init();
    let mut s = crate::utils::block_on::block_on(AppState::new_bootstrap(
        window,
        crate::app::WindowOptions::new(),
    ));
    s.rebuild_ui(fixture());
    for _ in 0..3 {
        render(&mut s, false);
    }
    let mut blits = 0;
    for (frame, (outer, inner)) in [
        (1.0, 0.0),
        (8.0, 0.0),
        (16.0, 0.0),
        (45.0, 0.0),
        (45.0, 25.0),
        (60.0, 25.0),
        (120.0, 25.0),
        (175.0, 25.0),
        (200.0, 300.0),
        (180.0, 300.0),
        (130.0, 300.0),
        (10.0, 300.0),
        (0.0, 0.0),
    ]
    .into_iter()
    .enumerate()
    {
        for (id, y) in [("nested-pixel-outer", outer), ("nested-pixel-inner", inner)] {
            use_signal(&scroll_offset_y_signal_key(id), || 0.0_f32).set_silent(y);
            s.update_scroll_offset_in_place(id, [0.0, y]);
        }
        let partial = render(&mut s, true);
        blits += usize::from(!s.scroll_strip_blit.is_empty());
        s.mark_full_redraw();
        let full = render(&mut s, true);
        let changed = partial
            .pixels
            .chunks_exact(4)
            .zip(full.pixels.chunks_exact(4))
            .filter(|(a, b)| a.iter().zip(*b).any(|(a, b)| a.abs_diff(*b) > 6))
            .count();
        if changed != 0 {
            let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../.tmp/nested-scroll-20261020");
            std::fs::create_dir_all(&dir).unwrap();
            for (label, image) in [("partial", &partial), ("full", &full)] {
                let mut pixels = image.pixels.clone();
                if matches!(
                    s.config.format,
                    wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
                ) {
                    for p in pixels.chunks_exact_mut(4) {
                        p.swap(0, 2);
                    }
                }
                ::image::save_buffer(
                    dir.join(format!("nested-{frame}-{label}.png")),
                    &pixels,
                    image.width,
                    image.height,
                    ::image::ColorType::Rgba8,
                )
                .unwrap();
            }
        }
        assert_eq!(
            changed, 0,
            "nested scroll frame {frame}, outer={outer}, inner={inner}"
        );
    }
    eprintln!("nested scroll optimized frames={blits}");
    if crate::app::damage::scroll_strip_blit_enabled() {
        assert!(blits > 0, "fixture must exercise the optimized scroll path");
    }
}

#[test]
#[ignore = "requires a native Windows GPU surface"]
fn nested_scroll_pixels_match_full() {
    native_window(run);
}

#[cfg(all(debug_assertions, feature = "devtools"))]
#[test]
#[ignore = "requires a native Windows GPU surface"]
fn nested_cssom_textarea_scroll_pixels_match_full() {
    native_window(|window| {
        crate::init();
        let field = "nested-cssom-textarea";
        let outer = "nested-cssom-properties";
        let inner = crate::utils::text_field::textarea_scroll_rest(field);
        use_signal(&crate::utils::text_field::input_value_signal_key(field), String::new)
            .set_silent((0..80).map(|index| format!("--declaration-{index}: solid 1px rgba(20, 40, 60, 0.8);\n")).collect());
        let editor: Element = crate::app::devtools::controls::textarea(field).rows(18).wrap("off").resizable(false).into();
        let content = Element::new().flex_col().w("100%")
            .child(Element::new().h(160.0).flex_shrink(0.0).text("element.style"))
            .child(editor.flex_shrink(0.0))
            .child(Element::new().h(300.0).flex_shrink(0.0).text("Matched rules"));
        let root = Element::new().w("100%").h("100%").bg("#ffffff").child(
            crate::app::devtools::controls::scroll_area(outer).w("100%").h("100%")
                .scrollbar_visibility("always").child(content),
        );
        let mut state = crate::utils::block_on::block_on(AppState::new_bootstrap(
            window, crate::app::WindowOptions::new(),
        ));
        state.rebuild_ui(root);
        for _ in 0..3 { render(&mut state, false); }
        for axis in ["x", "y"] {
            use_signal(&crate::core::scrollbar::scrollbar_visibility_signal_key(&inner, axis), || "auto".to_string()).set_silent("always".to_string());
        }
        for (index, (outer_y, inner_y)) in [(0.0, 15.0), (0.0, 85.0), (90.0, 85.0), (140.0, 180.0), (140.0, 220.0), (60.0, 220.0), (60.0, 220.5), (60.5, 250.0), (0.0, 0.0)].into_iter().enumerate() {
            for (rest, offset) in [(outer, outer_y), (inner.as_str(), inner_y)] {
                use_signal(&scroll_offset_y_signal_key(rest), || 0.0_f32).set_silent(offset);
                let x = if rest == inner { (index % 3) as f32 * 12.0 } else { 0.0 };
                use_signal(&crate::core::scroll_viewport::scroll_offset_x_signal_key(rest), || 0.0_f32).set_silent(x);
                state.update_scroll_offset_in_place(rest, [x, offset]);
            }
            let partial = render(&mut state, true);
            state.mark_full_redraw();
            let full = render(&mut state, true);
            let changed_pixels: Vec<_> = partial.pixels.chunks_exact(4).zip(full.pixels.chunks_exact(4))
                .enumerate()
                .filter_map(|(pixel, (before, after))| before.iter().zip(after).any(|(before, after)| before.abs_diff(*after) > 6).then_some(pixel))
                .collect();
            let changed = changed_pixels.len();
            if changed != 0 {
                let min_x = changed_pixels.iter().map(|pixel| pixel % partial.width as usize).min().unwrap();
                let max_x = changed_pixels.iter().map(|pixel| pixel % partial.width as usize).max().unwrap();
                let min_y = changed_pixels.iter().map(|pixel| pixel / partial.width as usize).min().unwrap();
                let max_y = changed_pixels.iter().map(|pixel| pixel / partial.width as usize).max().unwrap();
                eprintln!("nested CSSOM diff bounds x={min_x}..={max_x}, y={min_y}..={max_y}");
                let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../.tmp/nested-cssom-scroll");
                std::fs::create_dir_all(&dir).unwrap();
                for (label, image) in [("partial", &partial), ("full", &full)] {
                    let mut pixels = image.pixels.clone();
                    if matches!(state.config.format, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb) {
                        for pixel in pixels.chunks_exact_mut(4) { pixel.swap(0, 2); }
                    }
                    ::image::save_buffer(
                        dir.join(format!("nested-cssom-{index}-{label}.png")),
                        &pixels,
                        image.width,
                        image.height,
                        ::image::ColorType::Rgba8,
                    ).unwrap();
                }
            }
            assert_eq!(changed, 0, "nested CSSOM scroll frame {index}, outer={outer_y}, inner={inner_y}");
        }
    });
}

fn native_window(check: fn(Arc<dyn Window>)) {
    use winit::platform::windows::EventLoopBuilderExtWindows;
    use winit::{
        application::ApplicationHandler,
        event::WindowEvent,
        event_loop::{ActiveEventLoop, EventLoop},
        window::{WindowAttributes, WindowId},
    };
    struct Harness(fn(Arc<dyn Window>));
    impl ApplicationHandler for Harness {
        fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
            let window = event_loop
                .create_window(
                    WindowAttributes::default()
                        .with_surface_size(winit::dpi::LogicalSize::new(400.0, 320.0))
                        .with_visible(false),
                )
                .unwrap();
            (self.0)(Arc::from(window));
            event_loop.exit();
        }
        fn window_event(&mut self, _: &dyn ActiveEventLoop, _: WindowId, _: WindowEvent) {}
    }
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    builder.build().unwrap().run_app(Harness(check)).unwrap();
}
