use super::*;
use ratatui::widgets::Widget;

#[test]
fn pixel_validation_and_premultiplied_canvas_conversion() {
    assert!(RgbaFrame::new(0, 1, vec![]).is_err());
    assert!(RgbaFrame::new(u32::MAX, u32::MAX, vec![]).is_err());
    assert!(RgbaFrame::new(2, 1, vec![0; 4]).is_err());
    use jadix_2d::Canvas2D;
    let mut canvas = jadix_2d::RasterCanvas::new(1, 1);
    canvas.clear("rgba(255, 0, 0, 0.5)");
    let frame = RgbaFrame::from_canvas(canvas).unwrap();
    assert_eq!(frame.size(), (1, 1));
    assert_eq!(frame.pixels(), &[255, 0, 0, 128]);
}

#[test]
fn retained_kitty_stream_chunks_pixels_once_and_deletes_only_owned_id() {
    let id = NonZeroU32::new(42).unwrap();
    let frame = RgbaFrame::new(40, 40, vec![137; 40 * 40 * 4]).unwrap();
    let image = KittyView::new(id, frame, Size::new(5, 3)).unwrap();
    let area = Rect::new(2, 4, 5, 3);
    let mut buffer = Buffer::empty(Rect::new(0, 0, 10, 10));
    (&image).render(area, &mut buffer);
    let stream: String = buffer.content.iter().map(|c| c.symbol()).collect();
    assert!(stream.contains("f=32"));
    assert!(stream.contains("i=42"));
    assert!(stream.contains("m=1"));
    assert!(stream.contains("m=0"));
    assert!(stream.contains("U=1"));
    assert!(stream.contains("U=1,c=5,r=3,"));
    let mut encoded = String::new();
    for sequence in stream.split("\x1b_G").skip(1) {
        if let Some((_, data)) = sequence.split_once(';') {
            let payload = data.split("\x1b\\").next().unwrap();
            encoded.push_str(payload);
            assert!(payload.len() <= 4096);
            assert_eq!(payload.len() % 4, 0);
        }
    }
    use base64::Engine;
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .unwrap(),
        vec![137; 40 * 40 * 4]
    );
    let mut next = Buffer::empty(buffer.area);
    (&image).render(area, &mut next);
    let next: String = next.content.iter().map(|c| c.symbol()).collect();
    assert!(
        !next.contains("\x1b_G"),
        "unchanged frame must not upload again"
    );
    assert!(next.contains('\u{10EEEE}'));
    let mut explicit = Buffer::empty(area);
    (&image).render(area, &mut explicit);
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            let symbols: Vec<_> = explicit[(x, y)].symbol().chars().collect();
            assert_eq!(
                symbols,
                vec![
                    '\u{10eeee}',
                    kitty_diacritics()[(y - area.y) as usize],
                    kitty_diacritics()[(x - area.x) as usize],
                    kitty_diacritics()[0]
                ]
            );
        }
    }
    let mut deletion = Vec::new();
    image.delete(&mut deletion).unwrap();
    assert_eq!(deletion, b"\x1b_Ga=d,d=I,i=42,q=2;\x1b\\");
}

#[test]
fn oversized_placeholder_grid_is_rejected_instead_of_silently_truncated() {
    for size in [
        Size::new(0, 1),
        Size::new(1, 0),
        Size::new(298, 1),
        Size::new(1, 298),
    ] {
        let frame = RgbaFrame::new(1, 1, vec![255; 4]).unwrap();
        assert!(KittyView::new(NonZeroU32::new(1).unwrap(), frame, size).is_err());
    }
}

fn viewport() -> TerminalViewport {
    TerminalViewport::new(Rect::new(7, 3, 10, 5), [400, 200], 2.0).unwrap()
}
fn pixels(size: [u32; 2], value: u8) -> RgbaFrame {
    RgbaFrame::new(
        size[0],
        size[1],
        vec![value; (size[0] * size[1] * 4) as usize],
    )
    .unwrap()
}

#[test]
fn mouse_uses_source_logical_coordinates_and_half_open_region_bounds() {
    let v = viewport();
    assert_eq!(v.logical_size(), [200.0, 100.0]);
    assert_eq!(v.logical_position(7, 3), Some([10.0, 10.0]));
    assert_eq!(v.logical_position(16, 7), Some([190.0, 90.0]));
    for (x, y) in [(6, 3), (7, 2), (17, 3), (7, 8)] {
        assert_eq!(v.logical_position(x, y), None);
    }
    for scale in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::from_bits(1)] {
        assert!(TerminalViewport::new(v.cells(), v.pixels(), scale).is_err());
    }
    assert!(TerminalViewport::new(Rect::new(u16::MAX, 0, 2, 1), [1, 1], 1.0).is_err());
    let extreme = TerminalViewport::new(v.cells(), [400, 200], 4e-306).unwrap();
    assert!(extreme
        .logical_position(16, 7)
        .unwrap()
        .iter()
        .all(|n| n.is_finite()));
}

#[test]
fn key_paste_focus_resize_and_wheel_keep_reported_semantics() {
    use crossterm::event::{
        Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind,
    };
    let v = viewport();
    for kind in [
        KeyEventKind::Press,
        KeyEventKind::Repeat,
        KeyEventKind::Release,
    ] {
        let key = KeyEvent::new_with_kind(KeyCode::Char('中'), KeyModifiers::ALT, kind);
        assert_eq!(v.input(Event::Key(key)), Some(TerminalInput::Key(key)));
    }
    let text = "中文\r\n👩‍💻".to_owned();
    assert_eq!(
        v.input(Event::Paste(text.clone())),
        Some(TerminalInput::Paste(text))
    );
    assert_eq!(v.input(Event::FocusLost), Some(TerminalInput::Focus(false)));
    assert_eq!(
        v.input(Event::FocusGained),
        Some(TerminalInput::Focus(true))
    );
    assert_eq!(
        v.input(Event::Resize(0, 0)),
        Some(TerminalInput::Resize(Size::new(0, 0)))
    );
    let mouse = MouseEvent {
        column: 7,
        row: 3,
        kind: MouseEventKind::ScrollLeft,
        modifiers: KeyModifiers::CONTROL,
    };
    assert_eq!(
        v.input(Event::Mouse(mouse)),
        Some(TerminalInput::Pointer {
            logical: [10.0, 10.0],
            kind: mouse.kind,
            modifiers: mouse.modifiers,
        })
    );
    assert_eq!(
        v.input(Event::Mouse(MouseEvent { column: 6, ..mouse })),
        None
    );
    assert_eq!(TerminalInput::from_event(Event::Mouse(mouse), None), None);
    assert_eq!(
        TerminalInput::from_event(Event::FocusGained, None),
        Some(TerminalInput::Focus(true))
    );
    assert_eq!(
        TerminalInput::from_event(Event::Paste("42".into()), None),
        Some(TerminalInput::Paste("42".into()))
    );
}

#[test]
fn out_of_order_workers_and_duplicate_results_cannot_replace_the_latest_frame() {
    let v = viewport();
    let mut surface = KittySurface::new(NonZeroU32::new(42).unwrap(), v);
    let old = surface.request_frame(v).unwrap();
    let current = surface.request_frame(v).unwrap();
    let (old_tx, old_rx) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || old_tx.send((old, pixels([400, 200], 11))).unwrap());
    assert!(surface.submit(&current, pixels(v.pixels(), 137)).unwrap());
    let (old, late_frame) = old_rx.recv().unwrap();
    worker.join().unwrap();
    assert!(!surface.submit(&old, late_frame).unwrap());
    assert!(!surface.submit(&current, pixels(v.pixels(), 9)).unwrap());
    assert!(surface.prepare().unwrap());
    assert!(!surface.prepare().unwrap());
    let mut buffer = Buffer::empty(Rect::new(0, 0, 30, 20));
    surface.view().unwrap().render(v.cells(), &mut buffer);
    let stream: String = buffer.content.iter().map(|c| c.symbol()).collect();
    let mut encoded = String::new();
    for sequence in stream.split("\x1b_G").skip(1) {
        if let Some((_, payload)) = sequence.split_once(';') {
            encoded.push_str(payload.split("\x1b\\").next().unwrap());
        }
    }
    use base64::Engine;
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .unwrap(),
        vec![137; 400 * 200 * 4]
    );
}

#[test]
fn geometry_change_invalidates_pending_pixels_and_foreign_surface_tickets() {
    let v = viewport();
    let mut surface = KittySurface::new(NonZeroU32::new(42).unwrap(), v);
    let first = surface.request_frame(v).unwrap();
    let too_wide = TerminalViewport::new(Rect::new(0, 0, 298, 10), [400, 200], 1.0).unwrap();
    assert!(surface.request_frame(too_wide).is_err());
    surface.submit(&first, pixels(v.pixels(), 255)).unwrap();
    surface.prepare().unwrap();
    let pending = surface.request_frame(v).unwrap();
    surface.submit(&pending, pixels(v.pixels(), 1)).unwrap();
    let resized = TerminalViewport::new(v.cells(), [200, 100], 1.0).unwrap();
    let new = surface.request_frame(resized).unwrap();
    assert!(surface.view().is_none());
    assert!(!surface.prepare().unwrap());
    assert!(!surface.submit(&pending, pixels(v.pixels(), 1)).unwrap());
    assert!(surface.submit(&new, pixels(v.pixels(), 1)).is_err());
    let mut other = KittySurface::new(NonZeroU32::new(42).unwrap(), resized);
    let foreign = other.request_frame(resized).unwrap();
    assert!(!surface
        .submit(&foreign, pixels(resized.pixels(), 1))
        .unwrap());
    assert!(surface.submit(&new, pixels(resized.pixels(), 2)).unwrap());
}

#[test]
fn failed_cleanup_still_disposes_surface_before_late_work_arrives() {
    struct Broken;
    impl std::io::Write for Broken {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let v = viewport();
    let mut surface = KittySurface::new(NonZeroU32::new(42).unwrap(), v);
    let ticket = surface.request_frame(v).unwrap();
    assert!(surface.close(&mut Broken).is_err());
    assert!(!surface.submit(&ticket, pixels(v.pixels(), 1)).unwrap());
    assert!(surface.request_frame(v).is_err());
    assert!(!surface.prepare().unwrap());
    assert!(surface.view().is_none());
    let mut bytes = Vec::new();
    surface.close(&mut bytes).unwrap();
    assert_eq!(bytes, b"\x1b_Ga=d,d=I,i=42,q=2;\x1b\\");
}

#[test]
fn empty_or_controls_only_paste_preserves_selected_text_and_history() {
    let mut editor = TerminalEditor::new("keep 中文 🦀").unwrap();
    let end = editor.document().len();
    editor.set_selection(0, end).unwrap();
    let before = editor.retained_bytes();
    for text in ["", "\u{1b}\0\u{7}"] {
        assert!(!editor
            .handle(&crossterm::event::Event::Paste(text.into()))
            .unwrap());
        assert_eq!(editor.document().text(), "keep 中文 🦀");
        assert_eq!(editor.selection(), 0..end);
        assert_eq!(editor.retained_bytes(), before);
    }
}
