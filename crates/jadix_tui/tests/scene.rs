#![cfg(feature = "jadix-gpu")]
use jadix::{
    core::tree::Element,
    renderer::offscreen::{OffscreenGpu, SceneRenderer},
};
use jadix_tui::crossterm::event::{KeyModifiers, MouseButton, MouseEventKind};
use jadix_tui::{SceneInput, TerminalInput};
use std::{cell::Cell, rc::Rc};

fn pointer(kind: MouseEventKind, x: f64, y: f64) -> TerminalInput {
    TerminalInput::Pointer {
        kind,
        logical: [x, y],
        modifiers: KeyModifiers::NONE,
    }
}

#[test]
fn production_hit_paths_clip_gestures_and_cancel_on_focus_loss() {
    let clicks = Rc::new(Cell::new(0));
    let count = clicks.clone();
    let mut root = Element::new().w(100.0).h(80.0).overflow("hidden").child(
        Element::new()
            .w(150.0)
            .h(80.0)
            .focus_id("button")
            .on_click(move || count.set(count.get() + 1)),
    );
    let mut tree = jadix::layout::layout_tree::LayoutTree::new();
    tree.build_and_layout(&mut root, 100.0, 80.0, [-1.0; 2]);
    let mut input = SceneInput::default();
    assert!(!input.dispatch(
        &root,
        &pointer(MouseEventKind::Down(MouseButton::Left), 120.0, 20.0)
    ));
    input.dispatch(
        &root,
        &pointer(MouseEventKind::Up(MouseButton::Left), 20.0, 20.0),
    );
    assert_eq!(clicks.get(), 0);
    input.dispatch(
        &root,
        &pointer(MouseEventKind::Down(MouseButton::Left), 20.0, 20.0),
    );
    input.dispatch(&root, &TerminalInput::Focus(false));
    input.dispatch(
        &root,
        &pointer(MouseEventKind::Up(MouseButton::Left), 20.0, 20.0),
    );
    assert_eq!(clicks.get(), 0);
    input.dispatch(
        &root,
        &pointer(MouseEventKind::Down(MouseButton::Left), 20.0, 20.0),
    );
    input.dispatch(
        &root,
        &pointer(MouseEventKind::Up(MouseButton::Left), 20.0, 20.0),
    );
    assert_eq!(clicks.get(), 1);
    let count = clicks.clone();
    let mut prevented = Element::new()
        .w(100.0)
        .h(80.0)
        .on_mousedown(|event| event.prevent_default())
        .on_click(move || count.set(count.get() + 1));
    tree.build_and_layout(&mut prevented, 100.0, 80.0, [-1.0; 2]);
    input.cancel();
    input.dispatch(
        &prevented,
        &pointer(MouseEventKind::Down(MouseButton::Left), 20.0, 20.0),
    );
    input.dispatch(
        &prevented,
        &pointer(MouseEventKind::Up(MouseButton::Left), 20.0, 20.0),
    );
    assert_eq!(
        clicks.get(),
        1,
        "prevented down must not activate on release"
    );
}

#[test]
fn focus_redraw_retains_named_press_and_uses_current_callbacks() {
    use jadix_tui::crossterm::event::{KeyCode, KeyEvent};
    let clicks = Rc::new(Cell::new(0));
    let view = |id: &str, amount| {
        let clicks = clicks.clone();
        Element::new()
            .w(100.0)
            .h(80.0)
            .flex_col()
            .child(
                Element::new()
                    .w(100.0)
                    .h(0.0)
                    .focus_id("zero-size")
                    .focusable(true),
            )
            .child(
                Element::new()
                    .w(100.0)
                    .h(60.0)
                    .focus_id(id)
                    .focusable(true)
                    .on_click(move || clicks.set(clicks.get() + amount)),
            )
    };
    let mut tree = jadix::layout::layout_tree::LayoutTree::new();
    let mut root = view("button", 1);
    tree.build_and_layout(&mut root, 100.0, 80.0, [-1.0; 2]);
    let mut input = SceneInput::default();
    input.dispatch(
        &root,
        &TerminalInput::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
    );
    assert_eq!(
        input.focused_id(),
        Some("button"),
        "zero-size target must not take focus"
    );
    input.dispatch(
        &root,
        &pointer(MouseEventKind::Down(MouseButton::Left), 20.0, 20.0),
    );
    root = view("button", 10);
    tree.build_and_layout(&mut root, 100.0, 80.0, [-1.0; 2]);
    input.refresh_gesture(&root);
    input.dispatch(
        &root,
        &pointer(MouseEventKind::Up(MouseButton::Left), 20.0, 20.0),
    );
    assert_eq!(clicks.get(), 10, "callback from rebuilt tree must run");
    input.dispatch(
        &root,
        &pointer(MouseEventKind::Down(MouseButton::Left), 20.0, 20.0),
    );
    root = view("replacement", 100);
    tree.build_and_layout(&mut root, 100.0, 80.0, [-1.0; 2]);
    input.refresh_gesture(&root);
    input.dispatch(
        &root,
        &pointer(MouseEventKind::Up(MouseButton::Left), 20.0, 20.0),
    );
    assert_eq!(clicks.get(), 10, "replacement cannot inherit a press");
}

#[test]
fn modal_scopes_trap_tab_block_background_restore_and_unwind_when_removed() {
    use jadix_tui::crossterm::event::{KeyCode, KeyEvent};
    let count = Rc::new(Cell::new(0));
    let mut label = Element::new().w(100.0).h(20.0);
    label.attrs.insert("for".into(), "body".into());
    let body = || {
        let clicked = count.clone();
        Element::new()
            .absolute()
            .left(0.0)
            .top(0.0)
            .w(100.0)
            .h(80.0)
            .focus_id("body")
            .focusable(true)
            .on_click(move || clicked.set(clicked.get() + 1))
    };
    let mut root = Element::new()
        .w(300.0)
        .h(200.0)
        .child(body())
        .child(
            Element::new()
                .absolute()
                .left(120.0)
                .top(0.0)
                .w(120.0)
                .h(80.0)
                .flex_col()
                .focus_id("modal")
                .child(
                    Element::new()
                        .w(100.0)
                        .h(30.0)
                        .focus_id("one")
                        .focusable(true),
                )
                .child(
                    Element::new()
                        .w(100.0)
                        .h(30.0)
                        .focus_id("two")
                        .focusable(true),
                )
                .child(label),
        )
        .child(
            Element::new()
                .absolute()
                .left(120.0)
                .top(100.0)
                .w(120.0)
                .h(80.0)
                .focus_id("inner")
                .child(
                    Element::new()
                        .w(100.0)
                        .h(30.0)
                        .focus_id("inner-ok")
                        .focusable(true),
                ),
        );
    let mut tree = jadix::layout::layout_tree::LayoutTree::new();
    tree.build_and_layout(&mut root, 300.0, 200.0, [-1.0; 2]);
    let mut input = SceneInput::default();
    input.focus(Some("body".into()));
    input.push_modal(&root, "modal").unwrap();
    assert_eq!(input.focused_id(), Some("one"));
    assert!(input.push_modal(&root, "modal").is_err());
    let key = |code, mods| TerminalInput::Key(KeyEvent::new(code, mods));
    input.dispatch(&root, &key(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(input.focused_id(), Some("two"));
    input.dispatch(&root, &key(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(input.focused_id(), Some("one"));
    input.dispatch(&root, &key(KeyCode::BackTab, KeyModifiers::SHIFT));
    assert_eq!(input.focused_id(), Some("two"));
    input.dispatch(
        &root,
        &pointer(MouseEventKind::Down(MouseButton::Left), 20.0, 20.0),
    );
    input.dispatch(
        &root,
        &pointer(MouseEventKind::Up(MouseButton::Left), 20.0, 20.0),
    );
    assert_eq!(count.get(), 0);
    input.dispatch(
        &root,
        &pointer(MouseEventKind::Down(MouseButton::Left), 130.0, 70.0),
    );
    input.dispatch(
        &root,
        &pointer(MouseEventKind::Up(MouseButton::Left), 130.0, 70.0),
    );
    assert_eq!(
        count.get(),
        0,
        "modal label cannot forward activation to background"
    );
    input.focus(Some("body".into()));
    input.dispatch(&root, &key(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(
        count.get(),
        0,
        "out-of-scope programmatic focus cannot activate background"
    );
    input.focus(Some("two".into()));
    input.push_modal(&root, "inner").unwrap();
    assert_eq!(input.modal_depth(), 2);
    input.dispatch(&root, &key(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(input.focused_id(), Some("two"));
    input.dispatch(&root, &key(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(input.focused_id(), Some("body"));
    input.dispatch(&root, &key(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(count.get(), 1);
    input.push_modal(&root, "modal").unwrap();
    root = Element::new().w(300.0).h(200.0).child(body());
    tree.build_and_layout(&mut root, 300.0, 200.0, [-1.0; 2]);
    input.refresh_gesture(&root);
    assert!(!input.has_modal());
    assert_eq!(input.focused_id(), Some("body"));
}

#[test]
#[ignore = "requires an actual GPU adapter; execute explicitly, never silently skip"]
fn real_gpu_scene_glyphs_images_canvas_clips_resize_and_reuse() {
    let gpu = OffscreenGpu::new().expect("real GPU adapter required");
    println!("GPU adapter: {:?}", gpu.adapter_info);
    let clear = gpu
        .clear_readback(3, 2, wgpu::Color::RED)
        .expect("shared legacy readback");
    assert_eq!(
        clear.pixels,
        [255, 0, 0, 255].repeat(6),
        "unaligned row unpadding"
    );
    let mut renderer = SceneRenderer::new(gpu);
    let mut root = Element::new()
        .w(240.0)
        .h(160.0)
        .bg("#102030")
        .child(
            Element::new()
                .absolute()
                .left(10.0)
                .top(5.0)
                .w(220.0)
                .h(35.0)
                .text("Jadix glyphs 123")
                .text_size(22.0)
                .text_color("#ffffff")
                .text_decoration("underline"),
        )
        .child(
            Element::new()
                .absolute()
                .left(10.0)
                .top(50.0)
                .w(40.0)
                .h(30.0)
                .image_texture("tui-scene-red"),
        )
        .child(
            Element::new()
                .absolute()
                .left(60.0)
                .top(50.0)
                .w(40.0)
                .h(30.0)
                .overflow("hidden")
                .child(Element::new().w(100.0).h(80.0).bg("#00ff00")),
        )
        .child(jadix::primitives::path::attach_path(
            Element::new()
                .absolute()
                .left(120.0)
                .top(50.0)
                .w(40.0)
                .h(40.0),
            &jadix::primitives::path::filled_circle(20.0, 20.0, 18.0, 32),
            &jadix::primitives::path::PathStyle {
                fill: Some([1.0, 0.0, 1.0, 1.0]),
                ..Default::default()
            },
        ))
        .child(
            Element::new()
                .absolute()
                .left(175.0)
                .top(50.0)
                .w(40.0)
                .h(40.0)
                .bg("#ffff00")
                .clip_path("polygon(0% 0%, 100% 0%, 0% 100%)"),
        )
        .child(
            jadix::primitives::canvas::canvas()
                .w(40.0)
                .h(30.0)
                .paint(|ctx, _| {
                    ctx.clear("#0000ff");
                    jadix::core::tree::LayoutStatus::Clean
                }),
        );
    jadix::queue_image_rgba8("tui-scene-red", 2, 2, [255, 0, 0, 255].repeat(4));
    let before = jadix::text::shared_font_system_ptr();
    let a = renderer
        .render(&mut root, [240, 160], 1.0, [-1.0; 2], wgpu::Color::BLACK)
        .unwrap();
    assert_eq!(
        jadix::text::shared_font_system_ptr(),
        before,
        "restore layout font ownership"
    );
    assert!(a.pixels.chunks_exact(4).all(|p| p[3] == 255));
    assert_eq!(a.pixel(25, 65), Some([255, 0, 0, 255]));
    assert_eq!(a.pixel(75, 65), Some([0, 255, 0, 255]));
    assert_eq!(a.pixel(110, 65), Some([16, 32, 48, 255]), "overflow clip");
    assert_eq!(a.pixel(140, 70), Some([255, 0, 255, 255]), "real path fill");
    assert_eq!(a.pixel(180, 55), Some([255, 255, 0, 255]), "polygon inside");
    assert_eq!(a.pixel(210, 85), Some([16, 32, 48, 255]), "polygon outside");
    assert!(
        a.pixels
            .chunks_exact(4)
            .filter(|p| p[0] > 220 && p[1] > 220 && p[2] > 220)
            .count()
            > 120,
        "real glyph ink"
    );
    assert!(
        a.pixels
            .chunks_exact(4)
            .filter(|p| p[2] > 240 && p[0] < 10 && p[1] < 10)
            .count()
            > 500,
        "Canvas ink"
    );
    let b = renderer
        .render(&mut root, [240, 160], 1.0, [-1.0; 2], wgpu::Color::BLACK)
        .unwrap();
    assert_eq!(a.pixels, b.pixels, "retained resources preserve pixels");
    let scaled = renderer
        .render(&mut root, [480, 320], 2.0, [-1.0; 2], wgpu::Color::BLACK)
        .unwrap();
    assert_eq!(scaled.pixel(50, 130), Some([255, 0, 0, 255]));
    assert_eq!(renderer.rendered_frames(), 3);
    let invalid = renderer.render(&mut root, [0, 160], 1.0, [-1.0; 2], wgpu::Color::BLACK);
    assert_eq!(
        invalid.unwrap_err().kind(),
        std::io::ErrorKind::InvalidInput
    );
    assert_eq!(renderer.rendered_frames(), 3);
    let mut missing = Element::new()
        .w(20.0)
        .h(20.0)
        .image_texture("missing-headless-image");
    assert_eq!(
        renderer
            .render(&mut missing, [240, 160], 1.0, [-1.0; 2], wgpu::Color::BLACK)
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::WouldBlock
    );
    let mut glass = Element::new()
        .w(20.0)
        .h(20.0)
        .bg("#ffffff80")
        .backdrop_blur(8.0);
    let blurred = renderer
        .render(&mut glass, [240, 160], 1.0, [-1.0; 2], wgpu::Color::BLACK)
        .expect("ordinary backdrop blur now uses the supported offscreen path");
    assert!(blurred
        .pixel(10, 10)
        .is_some_and(|p| p[0] > 100 && p[3] == 255));
    assert_eq!(renderer.rendered_frames(), 4);
    let mut unsupported = Element::new().w(20.0).h(20.0).filter("blur(8px)");
    assert_eq!(
        renderer
            .render(
                &mut unsupported,
                [240, 160],
                1.0,
                [-1.0; 2],
                wgpu::Color::BLACK
            )
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::Unsupported
    );
    assert_eq!(
        renderer.rendered_frames(),
        4,
        "rejected frames do not advance presentation"
    );
    let mut covered = Element::new()
        .w(160.0)
        .h(40.0)
        .child(
            Element::new()
                .absolute()
                .w(160.0)
                .h(40.0)
                .text("must stay covered")
                .text_color("#ffffff")
                .text_size(22.0),
        )
        .child(
            Element::new()
                .absolute()
                .w(160.0)
                .h(40.0)
                .bg("#ff0000")
                .z_index(2),
        );
    let cover = renderer
        .render(&mut covered, [160, 40], 1.0, [-1.0; 2], wgpu::Color::BLACK)
        .unwrap();
    assert!(
        cover.pixels.chunks_exact(4).all(|p| p == [255, 0, 0, 255]),
        "later paint band covers earlier glyph mesh"
    );
    let mut portal = Element::new()
        .w(160.0)
        .h(40.0)
        .absolute()
        .child(Element::new().w(160.0).h(40.0).bg("#0000ff"));
    portal.template_id = Some(jadix::layout::layout_tree::PORTAL_LAYER_ID.into());
    covered.children.push(portal);
    let overlay = renderer
        .render(&mut covered, [160, 40], 1.0, [-1.0; 2], wgpu::Color::BLACK)
        .unwrap();
    assert!(
        overlay
            .pixels
            .chunks_exact(4)
            .all(|p| p == [0, 0, 255, 255]),
        "portal suffix remains above page bands"
    );
}
#[test]
fn synthetic_page_prefix_preserves_paint_and_keeps_authored_band_opacity() {
    use jadix::layout::layout_tree::{OverlayPaintBand, OverlayPaintSplit};
    let mut split = OverlayPaintSplit::default();
    split.instance_start = 3;
    split.text_start = 4;
    split.page_stack_bands.push(OverlayPaintBand {
        instance_start: 1,
        text_start: 2,
        underlay_opacity: 0.4,
        ..Default::default()
    });
    let bands = split.page_stack_band_ranges();
    assert_eq!(bands.len(), 2);
    assert_eq!(bands[0].instance, (0, 1));
    assert_eq!(bands[0].text, (0, 2));
    assert_eq!(bands[0].underlay_opacity, 1.0);
    assert_eq!(bands[1].instance, (1, 3));
    assert_eq!(bands[1].underlay_opacity, 0.4);
}
