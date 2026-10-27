#![cfg(feature = "product-tests")]
use super::*;

#[test]
fn clipped_overflow_text_does_not_trim_neighbor_focus_ring() {
    let field = crate::layout::layout_tree::NodePaint {
        key: 1,
        bounds: [13.0, 97.0, 262.0, 126.0],
        rest: Some([16.0, 100.0, 256.0, 120.0]),
        instance: (0, 1),
        textured: (0, 0),
        text: (0, 1),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        overlay: false,
        layered: false,
    };
    let overflow_text = crate::layout::layout_tree::NodePaint {
        key: 2,
        bounds: [-100.0, 108.0, 900.0, 24.0],
        rest: Some([-100.0, 108.0, 900.0, 24.0]),
        instance: (1, 1),
        text: (1, 2),
        overflow_clip: Some([560.0, 100.0, 256.0, 120.0]),
        ..field.clone()
    };
    let rect = crate::app::draw_cull::node_present_scissor_logical(
        crate::app::damage::PaintDirtyKind::Node,
        field.key,
        field.bounds,
        &[],
        &[field.clone(), overflow_text],
        8.0,
    );
    assert!(
        crate::app::damage::rect_contains_rect(rect, field.bounds),
        "invisible overflow text trimmed the focus ring: {rect:?}"
    );
}

#[cfg(target_os = "windows")]
#[test]
#[ignore = "requires a native Windows GPU surface"]
fn textarea_native_focus_ring_matches_full_draw() {
    use winit::application::ApplicationHandler;
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, EventLoop};
    use winit::platform::windows::EventLoopBuilderExtWindows;
    use winit::window::{WindowAttributes, WindowId};

    struct Harness(winit::event_loop::EventLoopProxy);
    impl ApplicationHandler for Harness {
        fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
            let window: Arc<dyn Window> = Arc::from(
                event_loop
                    .create_window(
                        WindowAttributes::default()
                            .with_title("Textarea focus ring regression")
                            .with_surface_size(winit::dpi::LogicalSize::new(1700.0, 1000.0))
                            .with_visible(false),
                    )
                    .unwrap(),
            );
            check_focus_ring(window, self.0.clone(), event_loop);
            event_loop.exit();
        }
        fn window_event(&mut self, _: &dyn ActiveEventLoop, _: WindowId, _: WindowEvent) {}
    }
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let proxy = event_loop.create_proxy();
    event_loop.run_app(Harness(proxy)).unwrap();
}

#[cfg(target_os = "windows")]
fn check_focus_ring(
    window: Arc<dyn Window>,
    proxy: winit::event_loop::EventLoopProxy,
    event_loop: &dyn winit::event_loop::ActiveEventLoop,
) {
    use crate::components::textarea::{textarea, textarea_focus_id};
    use crate::core::reactivity::use_signal;
    use crate::renderer::offscreen::RgbaImage;

    fn frame(s: &mut AppState) -> RgbaImage {
        frame_mode(s, false)
    }
    fn frame_mode(s: &mut AppState, fast: bool) -> RgbaImage {
        frame_at(s, fast, [-1.0, -1.0])
    }
    fn frame_at(s: &mut AppState, fast: bool, mouse: [f32; 2]) -> RgbaImage {
        s.render(
            mouse,
            if fast {
                ScrollbarUpdateMode::ViewportsOnly
            } else {
                ScrollbarUpdateMode::Full
            },
            None,
            fast,
            fast,
            fast,
        )
        .unwrap();
        read_frame(s)
    }
    fn read_frame(s: &AppState) -> RgbaImage {
        let texture = s.present_retain.color_texture_for_test().unwrap();
        let mut img = readback_texture(
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
            for pixel in img.pixels.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
        }
        img
    }
    fn snapshot(name: &str, img: &RgbaImage) {
        if let Some(dir) = std::env::var_os("JADIX_TEXTAREA_TEST_OUTPUT") {
            std::fs::create_dir_all(&dir).unwrap();
            ::image::save_buffer(
                std::path::Path::new(&dir).join(format!("textarea-{name}.png")),
                &img.pixels,
                img.width,
                img.height,
                ::image::ColorType::Rgba8,
            )
            .unwrap();
        }
    }
    let focus = use_signal("__jadix_button_focus_id", String::new);
    focus.set(String::new());
    crate::style::set_focus_ring_style(crate::style::FocusRingStyle::Hard);
    let mut s = crate::utils::block_on::block_on(AppState::new_bootstrap(
        window,
        super::super::super::window_chrome::WindowOptions::default(),
    ));
    let mut row = Element::new().flex_row().gap(16.0);
    for (id, wrap) in [
        ("native-ta-word", "word"),
        ("native-ta-char", "char"),
        ("native-ta-off", "off"),
        ("native-ta-bidi", "word"),
    ] {
        let value = use_signal(id, || {
            let text = "padding: 8px 16px; background: #ffffff; border-width: 1px; border-radius: 6px -- soft wrap folds this long hard line.";
            if id == "native-ta-bidi" {
                "Hello 789עברית and العربية — soft wrap keeps WJ pairs.".to_string()
            } else if wrap == "char" {
                "Supercalifragilisticexpialidocious_unbreakable_token_plus_中文emoji😀 — char may mid-break.".to_string()
            } else if wrap == "off" {
                "padding: 8px 16px; background: #ffffff; border-width: 1px; border-radius: 6px — wrap=off scrolls horizontally (no clip).".to_string()
            } else {
                text.to_string()
            }
        });
        row = row.child(
            Element::new()
                .w(256.0)
                .flex_col()
                .gap(5.6)
                .child(Element::new().text(format!("wrap={wrap}")).text_size(12.0))
                .child(
                    textarea(id)
                        .rows(5)
                        .wrap(wrap)
                        .selection_on_blur(if wrap == "char" { "retain" } else { "clear" })
                        .resizable(false)
                        .value_signal(value),
                ),
        );
    }
    let body = Element::new()
        .w("100%")
        .flex_col()
        .flex_shrink(0.0)
        .p(24.0)
        .bg(crate::style::parse_color("background"))
        .child(Element::new().h(1040.0).flex_shrink(0.0))
        .child(
            Element::new()
                .w("100%")
                .min_w(0.0)
                .p(16.0)
                .flex_col()
                .bg(crate::style::parse_color("background"))
                .border_style("solid")
                .border_color(crate::style::parse_color("border"))
                .border_width(0.125)
                .rounded(8.0)
                .flex_shrink(0.0)
                .child(row.flex_shrink(0.0)),
        )
        .child(
            Element::new()
                .flex_row()
                .gap(16.0)
                .mt(16.0)
                .child(
                    Element::new()
                        .w(256.0)
                        .child(crate::components::input::input("native-input-clear")),
                )
                .child(
                    Element::new().w(256.0).child(
                        crate::components::input::input("native-input-retain")
                            .selection_on_blur("retain"),
                    ),
                ),
        )
        .child(Element::new().h(240.0).flex_shrink(0.0));
    s.rebuild_ui(
        crate::components::scroll_area::scroll_area("native-ta-page")
            .w("100%")
            .h("100%")
            .overflow_x("auto")
            .overflow_y("auto")
            .scroll_behavior("smooth")
            .child(body)
            .into(),
    );
    for _ in 0..3 {
        frame(&mut s);
    }
    crate::app::scroll_support::set_scroll_offset_with_behavior("native-ta-page", 0.0, 400.0, true);
    s.update_scroll_offset_in_place("native-ta-page", [0.0, 400.0]);
    for _ in 0..3 {
        frame(&mut s);
    }
    focus.set(textarea_focus_id("native-ta-off"));
    use_signal(
        &crate::utils::text_field::input_caret_signal_key("native-ta-off"),
        || None::<usize>,
    )
    .set(Some(0));
    frame(&mut s);
    let off_len = use_signal("native-ta-off", String::new).peek().len();
    use_signal(
        &crate::utils::text_field::input_caret_signal_key("native-ta-off"),
        || None::<usize>,
    )
    .set(Some(off_len));
    use_signal(
        &crate::utils::text_field::input_selection_signal_key("native-ta-off"),
        || None::<(usize, usize)>,
    )
    .set(Some((0, off_len)));
    frame(&mut s);
    frame(&mut s);
    fn find_field_by_id<'a>(el: &'a Element, id: &str) -> Option<&'a Element> {
        if el.focus_id.as_deref() == Some(id) {
            return Some(el);
        }
        el.children
            .iter()
            .find_map(|child| find_field_by_id(child, id))
    }
    let off = find_field_by_id(&s.root_element, "textarea:native-ta-off").unwrap();
    let off_bounds = crate::core::tree::visual_paint_aabb(off, off.paint_scroll_acc);
    let scale = s.ui_scale();
    for (name, caret) in [("pan-end", off_len), ("pan-start", 0)] {
        use_signal(
            &crate::utils::text_field::input_caret_signal_key("native-ta-off"),
            || None::<usize>,
        )
        .set(Some(caret));
        let image = frame(&mut s);
        snapshot(name, &image);
        for px in [off_bounds[0] + 6.0, off_bounds[0] + off_bounds[2] - 6.0] {
            let pixel = image
                .pixel((px * scale) as u32, ((off_bounds[1] + 12.0) * scale) as u32)
                .unwrap();
            assert!(
                pixel[0] > 240,
                "scrolling text entered fixed padding: {name} {pixel:?}"
            );
        }
    }
    focus.set(textarea_focus_id("native-ta-word"));
    assert_eq!(
        use_signal(
            &crate::utils::text_field::input_selection_signal_key("native-ta-off"),
            || None::<(usize, usize)>
        )
        .peek(),
        None
    );
    let partial = frame(&mut s);
    fn find_field(el: &Element) -> Option<&Element> {
        if el.focus_id.as_deref() == Some("textarea:native-ta-word") {
            return Some(el);
        }
        el.children.iter().find_map(find_field)
    }
    let field = find_field(&s.root_element).unwrap();
    let [x, y, w, h] = crate::core::tree::visual_paint_aabb(field, field.paint_scroll_acc);
    let scale = s.ui_scale();
    s.mark_full_redraw();
    let full = frame(&mut s);
    snapshot("partial", &partial);
    snapshot("full", &full);
    for (edge, px, py) in [
        ("top", x + w * 0.5, y - 2.0),
        ("left upper", x - 2.0, y + h * 0.25),
        ("bottom", x + w * 0.5, y + h + 1.0),
    ] {
        let p = partial
            .pixel((px * scale) as u32, (py * scale) as u32)
            .unwrap();
        let f = full
            .pixel((px * scale) as u32, (py * scale) as u32)
            .unwrap();
        assert!(f[0] < 230, "full {edge} ring missing: {f:?}");
        assert!(
            p[0].abs_diff(f[0]) <= 6,
            "partial {edge} ring missing: partial={p:?}, full={f:?}"
        );
    }
    assert!(
        crate::renderer::offscreen::images_match(&partial, &full, 3),
        "focus transfer changed retained text pixels"
    );
    let selection = use_signal(
        &crate::utils::text_field::input_selection_signal_key("native-ta-word"),
        || None::<(usize, usize)>,
    );
    for (i, range) in [Some((0, 24)), Some((12, 72)), None]
        .into_iter()
        .enumerate()
    {
        selection.set(range);
        let partial = frame(&mut s);
        s.mark_full_redraw();
        let full = frame(&mut s);
        snapshot(&format!("selection-{i}-partial"), &partial);
        snapshot(&format!("selection-{i}-full"), &full);
        assert!(
            crate::renderer::offscreen::images_match(&partial, &full, 3),
            "selection {range:?} changed retained text colors"
        );
    }
    for (id, multiline, retain) in [
        ("native-ta-char", true, true),
        ("native-input-clear", false, false),
        ("native-input-retain", false, true),
    ] {
        if !multiline {
            use_signal(
                &crate::utils::text_field::input_value_signal_key(id),
                String::new,
            )
            .set("selected overflow text ".repeat(16));
        }
        let fid = crate::utils::text_field::focus_id_for_text_edit(id, multiline);
        let selection = use_signal(
            &crate::utils::text_field::input_selection_signal_key(id),
            || None::<(usize, usize)>,
        );
        focus.set(fid.clone());
        use_signal(
            &crate::utils::text_field::input_caret_signal_key(id),
            || None::<usize>,
        )
        .set(Some(0));
        selection.set(Some((0, 24)));
        frame(&mut s);
        for (phase, next_focus) in [
            ("blur", textarea_focus_id("native-ta-word")),
            ("refocus", fid),
        ] {
            focus.set(next_focus);
            let partial = frame(&mut s);
            s.mark_full_redraw();
            let full = frame(&mut s);
            snapshot(&format!("{id}-{phase}-partial"), &partial);
            snapshot(&format!("{id}-{phase}-full"), &full);
            assert!(
                crate::renderer::offscreen::images_match(&partial, &full, 3),
                "{id} {phase} retained selection differs from full draw"
            );
            assert_eq!(selection.peek(), retain.then_some((0, 24)));
        }
    }

    // Retain a wrapped selection beside a horizontally panned full-line selection.
    focus.set(textarea_focus_id("native-ta-char"));
    let char_len = use_signal("native-ta-char", String::new).peek().len();
    use_signal(
        &crate::utils::text_field::input_selection_signal_key("native-ta-char"),
        || None::<(usize, usize)>,
    )
    .set(Some((0, char_len)));
    frame(&mut s);
    focus.set(textarea_focus_id("native-ta-off"));
    use_signal(
        &crate::utils::text_field::input_caret_signal_key("native-ta-off"),
        || None::<usize>,
    )
    .set(Some(off_len));
    use_signal(
        &crate::utils::text_field::input_selection_signal_key("native-ta-off"),
        || None::<(usize, usize)>,
    )
    .set(Some((0, off_len)));
    let partial = frame(&mut s);
    snapshot("neighbor-selected-partial", &partial);
    s.mark_full_redraw();
    let full = frame(&mut s);
    snapshot("neighbor-selected-full", &full);
    let mut selected_ink = 0;
    for py in ((off_bounds[1] + 8.0) * scale) as u32..((off_bounds[1] + 28.0) * scale) as u32 {
        for px in ((off_bounds[0] + 14.0) * scale) as u32
            ..((off_bounds[0] + off_bounds[2] - 14.0) * scale) as u32
        {
            if partial.pixel(px, py).unwrap()[0] > 180 {
                selected_ink += 1;
            }
        }
    }
    assert!(
        selected_ink > 200,
        "selected nowrap glyphs disappeared: {selected_ink} bright pixels"
    );
    assert!(
        crate::renderer::offscreen::images_match(&partial, &full, 3),
        "wrap=off selection erased the retained neighbor's first line"
    );

    let rest = crate::utils::text_field::textarea_scroll_rest("native-ta-off");
    let visibility =
        crate::style::use_ui_transition_cubic(&format!("scrollbar_spring_{rest}:x"), 0.0);
    let selected_image = full;
    for (phase, alpha) in [("enter", 1.0), ("leave", 0.0), ("reenter", 1.0)] {
        visibility.set(alpha);
        let mouse = if alpha > 0.0 {
            [off_bounds[0] + 40.0, off_bounds[1] + 40.0]
        } else {
            [-1.0, -1.0]
        };
        for tick in 0..8 {
            crate::style::tick_animations(0.025);
            let partial = frame_at(&mut s, tick % 2 == 0, mouse);
            snapshot(&format!("bar-{phase}-{tick}-partial"), &partial);
            for py in
                ((off_bounds[1] + 6.0) * scale) as u32..((off_bounds[1] + 30.0) * scale) as u32
            {
                for px in ((off_bounds[0] + 13.0) * scale) as u32
                    ..((off_bounds[0] + off_bounds[2] - 13.0) * scale) as u32
                {
                    let actual = partial.pixel(px, py).unwrap();
                    let expected = selected_image.pixel(px, py).unwrap();
                    assert!(
                        actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 3),
                        "scrollbar {phase} tick {tick} lost selected glyph color at ({px}, {py}): {actual:?} != {expected:?}"
                    );
                }
            }
        }
    }

    // Drive the real pointer-down/up path, rather than only setting the focus signal.
    let window_id = s.window.id();
    let (_, rx) = std::sync::mpsc::channel();
    let mut app = crate::app::lifecycle::create_app(Default::default(), Element::new, proxy, rx);
    *app.state.borrow_mut() = Some(s);
    app.pointer_window = Some(window_id);
    for (id, multiline) in [("native-ta-char", true), ("native-input-retain", false)] {
        let fid = crate::utils::text_field::focus_id_for_text_edit(id, multiline);
        focus.set(fid.clone());
        let selection = use_signal(
            &crate::utils::text_field::input_selection_signal_key(id),
            || None::<(usize, usize)>,
        );
        selection.set(Some((0, 24)));
        frame(app.state.borrow_mut().as_mut().unwrap());
        focus.set(textarea_focus_id("native-ta-word"));
        frame(app.state.borrow_mut().as_mut().unwrap());
        let bounds = {
            let st = app.state.borrow();
            let field = find_field_by_id(&st.as_ref().unwrap().root_element, &fid).unwrap();
            crate::core::tree::visual_paint_aabb(field, field.paint_scroll_acc)
        };
        app.cursor_pos = winit::dpi::PhysicalPosition::new(
            ((bounds[0] + 20.0) * scale) as f64,
            ((bounds[1] + 14.0) * scale) as f64,
        );
        app.last_click_at = None;
        app.handle_mouse_button(
            window_id,
            winit::event::ElementState::Pressed,
            winit::event::MouseButton::Left,
            None,
        );
        assert_eq!(
            selection.peek(),
            Some((0, 24)),
            "{id} refocus press must keep selection"
        );
        assert!(
            app.input_drag_rest.is_none(),
            "refocus must not arm selection drag"
        );
        app.handle_cursor_moved(
            window_id,
            winit::dpi::PhysicalPosition::new(app.cursor_pos.x + 1.0, app.cursor_pos.y),
            event_loop,
        );
        app.flush_pending_cursor_moved(window_id, event_loop);
        let partial = frame(app.state.borrow_mut().as_mut().unwrap());
        app.handle_mouse_button(
            window_id,
            winit::event::ElementState::Released,
            winit::event::MouseButton::Left,
            None,
        );
        assert_eq!(
            selection.peek(),
            Some((0, 24)),
            "{id} refocus release must keep selection"
        );
        let mut st = app.state.borrow_mut();
        let st = st.as_mut().unwrap();
        st.mark_full_redraw();
        let full = frame(st);
        snapshot(&format!("pointer-{id}-partial"), &partial);
        snapshot(&format!("pointer-{id}-full"), &full);
        assert!(
            crate::renderer::offscreen::images_match(&partial, &full, 3),
            "{id} pointer refocus must restore selected glyph colors"
        );
    }

    focus.set(textarea_focus_id("native-ta-off"));
    use_signal(
        &crate::utils::text_field::input_caret_signal_key("native-ta-off"),
        || None::<usize>,
    )
    .set(Some(off_len));
    use_signal(
        &crate::utils::text_field::input_selection_signal_key("native-ta-off"),
        || None::<(usize, usize)>,
    )
    .set(Some((0, off_len)));
    frame(app.state.borrow_mut().as_mut().unwrap());
    crate::app::scroll_support::snap_scroll_targets_to_current_offsets([rest.as_str()]);
    let mut baseline = frame(app.state.borrow_mut().as_mut().unwrap());
    snapshot("mouse-panned-selected-baseline", &baseline);
    let mut bright_ink = 0;
    for py in ((off_bounds[1] + 8.0) * scale) as u32..((off_bounds[1] + 28.0) * scale) as u32 {
        for px in ((off_bounds[0] + 14.0) * scale) as u32
            ..((off_bounds[0] + off_bounds[2] - 14.0) * scale) as u32
        {
            bright_ink += usize::from(baseline.pixel(px, py).unwrap()[0] > 180);
        }
    }
    assert!(
        bright_ink > 200,
        "panned selection must contain visible glyphs"
    );
    let offset = use_signal(
        &crate::core::scroll_viewport::scroll_offset_x_signal_key(&rest),
        || 0.0_f32,
    )
    .peek();
    assert!(offset > 100.0, "selected run must be horizontally panned");
    // Complete fade-out before re-entering, then read back every entering frame.
    // Rapid alternation alone never exercised the hidden -> visible boundary.
    for cycle in 0..16 {
        if cycle == 8 {
            use_signal(
                &crate::utils::text_field::input_caret_signal_key("native-ta-off"),
                || None::<usize>,
            )
            .set(Some(0));
            crate::app::scroll_support::set_scroll_offset_with_behavior(&rest, 0.0, 0.0, true);
            app.state
                .borrow_mut()
                .as_mut()
                .unwrap()
                .update_scroll_offset_in_place(&rest, [0.0, 0.0]);
            baseline = frame(app.state.borrow_mut().as_mut().unwrap());
        }
        for entering in [false, true] {
            let mouse = if entering {
                [off_bounds[0] + 40.0, off_bounds[1] + 14.0]
            } else {
                [off_bounds[0] + off_bounds[2] + 60.0, off_bounds[1] + 14.0]
            };
            app.scrollbar_visible_until.clear();
            app.handle_cursor_moved(
                window_id,
                winit::dpi::PhysicalPosition::new(
                    (mouse[0] * scale) as f64,
                    (mouse[1] * scale) as f64,
                ),
                event_loop,
            );
            for tick in 0..24 {
                crate::style::tick_animations(1.0 / 60.0);
                app.handle_redraw_requested(window_id, event_loop);
                let guard = app.state.borrow();
                let actual = read_frame(guard.as_ref().unwrap());
                for py in
                    ((off_bounds[1] + 8.0) * scale) as u32..((off_bounds[1] + 28.0) * scale) as u32
                {
                    for px in ((off_bounds[0] + 14.0) * scale) as u32
                        ..((off_bounds[0] + off_bounds[2] - 14.0) * scale) as u32
                    {
                        let expected = baseline.pixel(px, py).unwrap();
                        let pixel = actual.pixel(px, py).unwrap();
                        if expected[0] > 220 && pixel[0] < 180 {
                            std::env::set_var("JADIX_TEXTAREA_TEST_OUTPUT", ".tmp/textarea-fade");
                            snapshot("failed-fade-baseline", &baseline);
                            snapshot("failed-fade-frame", &actual);
                            panic!(
                                "fade cycle {cycle} entering={entering} tick={tick} alpha={} erased glyph at ({px},{py}): {pixel:?} != {expected:?}",
                                visibility.peek()
                            );
                        }
                    }
                }
            }
            assert!(
                (visibility.peek() - if entering { 1.0 } else { 0.0 }).abs() < 0.01,
                "fade did not settle: entering={entering} alpha={}",
                visibility.peek()
            );
            if entering && cycle == 0 {
                let actual = read_frame(app.state.borrow().as_ref().unwrap());
                let expected = {
                    let mut guard = app.state.borrow_mut();
                    let s = guard.as_mut().unwrap();
                    s.mark_full_redraw();
                    frame_at(s, false, mouse)
                };
                let guard = app.state.borrow();
                let track = find_field_by_id(
                    &guard.as_ref().unwrap().root_element,
                    &format!("scrollbar-track:{rest}:x"),
                )
                .unwrap();
                let track_bounds =
                    crate::core::tree::visual_paint_aabb(track, track.paint_scroll_acc);
                let center = expected
                    .pixel(
                        ((track_bounds[0] + track_bounds[2] * 0.5) * scale) as u32,
                        ((track_bounds[1] + track_bounds[3] * 0.5) * scale) as u32,
                    )
                    .unwrap();
                eprintln!(
                    "[DEBUG-track] bounds={track_bounds:?} color={:?} opacity={} center={center:?}",
                    track.style.bg_color, track.style.opacity
                );
                std::env::set_var("JADIX_TEXTAREA_TEST_OUTPUT", ".tmp/textarea-track");
                snapshot("track-full", &expected);
                std::env::remove_var("JADIX_TEXTAREA_TEST_OUTPUT");
                drop(guard);
                for py in ((off_bounds[1] + off_bounds[3] - 11.0) * scale) as u32
                    ..((off_bounds[1] + off_bounds[3] - 4.0) * scale) as u32
                {
                    for px in ((off_bounds[0] + 8.0) * scale) as u32
                        ..((off_bounds[0] + off_bounds[2] - 8.0) * scale) as u32
                    {
                        let a = actual.pixel(px, py).unwrap();
                        let e = expected.pixel(px, py).unwrap();
                        if a.iter().zip(e).any(|(a, e)| a.abs_diff(e) > 3) {
                            std::env::set_var("JADIX_TEXTAREA_TEST_OUTPUT", ".tmp/textarea-track");
                            snapshot("track-partial", &actual);
                            snapshot("track-full", &expected);
                            panic!("scrollbar track lost paint at ({px},{py}): {a:?} != {e:?}");
                        }
                    }
                }
            }
        }
    }
    for (phase, mouse) in [
        ("leave", [off_bounds[0] - 100.0, off_bounds[1] + 40.0]),
        ("enter", [off_bounds[0] + 40.0, off_bounds[1] + 40.0]),
        (
            "leave-again",
            [off_bounds[0] + off_bounds[2] + 60.0, off_bounds[1] + 40.0],
        ),
        ("reenter", [off_bounds[0] + 40.0, off_bounds[1] + 40.0]),
    ] {
        app.scrollbar_visible_until.clear();
        app.handle_cursor_moved(
            window_id,
            winit::dpi::PhysicalPosition::new((mouse[0] * scale) as f64, (mouse[1] * scale) as f64),
            event_loop,
        );
        for tick in 0..12 {
            crate::style::tick_animations(0.025);
            app.handle_redraw_requested(window_id, event_loop);
            let st = app.state.borrow();
            let actual = read_frame(st.as_ref().unwrap());
            snapshot(&format!("mouse-{phase}-{tick}"), &actual);
            for py in
                ((off_bounds[1] + 6.0) * scale) as u32..((off_bounds[1] + 30.0) * scale) as u32
            {
                for px in ((off_bounds[0] + 13.0) * scale) as u32
                    ..((off_bounds[0] + off_bounds[2] - 13.0) * scale) as u32
                {
                    let pixel = actual.pixel(px, py).unwrap();
                    let expected = baseline.pixel(px, py).unwrap();
                    assert!(
                        pixel.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 3),
                        "mouse {phase} tick {tick} changed selected glyphs at ({px}, {py}): {pixel:?} != {expected:?}"
                    );
                }
            }
        }
    }
    // Exercise the real drag/release path before hover, without a full redraw.
    use_signal(
        &crate::utils::text_field::input_selection_signal_key("native-ta-off"),
        || None::<(usize, usize)>,
    )
    .set(None);
    frame(app.state.borrow_mut().as_mut().unwrap());
    app.cursor_pos = winit::dpi::PhysicalPosition::new(
        ((off_bounds[0] + 22.0) * scale) as f64,
        ((off_bounds[1] + 14.0) * scale) as f64,
    );
    app.last_click_at = None;
    app.handle_mouse_button(
        window_id,
        winit::event::ElementState::Pressed,
        winit::event::MouseButton::Left,
        Some(event_loop),
    );
    app.handle_cursor_moved(
        window_id,
        winit::dpi::PhysicalPosition::new(
            ((off_bounds[0] + off_bounds[2] - 22.0) * scale) as f64,
            ((off_bounds[1] + 14.0) * scale) as f64,
        ),
        event_loop,
    );
    app.flush_pending_cursor_moved(window_id, event_loop);
    app.handle_mouse_button(
        window_id,
        winit::event::ElementState::Released,
        winit::event::MouseButton::Left,
        Some(event_loop),
    );
    assert!(!app.left_button_down);
    assert!(app.input_drag_rest.is_none());
    assert!(use_signal(
        &crate::utils::text_field::input_selection_signal_key("native-ta-off"),
        || None::<(usize, usize)>,
    )
    .peek()
    .is_some_and(|(a, b)| a != b));
    let baseline = frame(app.state.borrow_mut().as_mut().unwrap());
    snapshot("partial-selected-baseline", &baseline);
    for turn in 0..80 {
        let mouse = match turn % 4 {
            0 => [off_bounds[0] - 100.0, off_bounds[1] + 14.0],
            2 => [off_bounds[0] + off_bounds[2] + 60.0, off_bounds[1] + 14.0],
            _ => [off_bounds[0] + 40.0, off_bounds[1] + 14.0],
        };
        app.scrollbar_visible_until.clear();
        app.handle_cursor_moved(
            window_id,
            winit::dpi::PhysicalPosition::new((mouse[0] * scale) as f64, (mouse[1] * scale) as f64),
            event_loop,
        );
        crate::style::tick_animations(0.01);
        app.handle_redraw_requested(window_id, event_loop);
        let st = app.state.borrow();
        let actual = read_frame(st.as_ref().unwrap());
        snapshot(&format!("rapid-hover-{turn}"), &actual);
        for py in ((off_bounds[1] + 8.0) * scale) as u32..((off_bounds[1] + 28.0) * scale) as u32 {
            for px in ((off_bounds[0] + 14.0) * scale) as u32
                ..((off_bounds[0] + off_bounds[2] - 14.0) * scale) as u32
            {
                let pixel = actual.pixel(px, py).unwrap();
                let expected = baseline.pixel(px, py).unwrap();
                assert!(
                    expected[0] <= 220 || pixel[0] >= 180,
                    "rapid hover {turn} erased glyph ink at ({px}, {py}): {pixel:?} != {expected:?}"
                );
            }
        }
    }
}
