use jadix_tui::{
    crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEventKind},
    ratatui::layout::Rect,
    InputDecoder, KittyReply, KittyTerminal, RgbaFrame, TerminalEditor, TerminalViewport,
    WireEvent,
};
use std::{
    cell::RefCell,
    io::{self, Write},
    num::NonZeroU32,
    rc::Rc,
    time::{Duration, Instant},
};

fn key(code: KeyCode, mods: KeyModifiers) -> Event {
    Event::Key(KeyEvent::new(code, mods))
}

#[test]
fn fragmented_input_preserves_paste_and_separates_graphics_without_spoofing() {
    let stream="中\x1b[200~paste👩‍💻\x1b_Gi=100;OK\x1b\\\x1b[201~\x1b_Gi=100;OK\x1b\\\x1b[97;5:3u\x1b[<0;10;4M\x1b[O\x1b[8;30;90t";
    for size in 1..=stream.len() {
        let mut decoder = InputDecoder::default();
        let events: Vec<_> = stream
            .as_bytes()
            .chunks(size)
            .flat_map(|chunk| decoder.feed(chunk).unwrap())
            .collect();
        assert_eq!(events.len(), 7);
        assert_eq!(
            events[0],
            WireEvent::Input(key(KeyCode::Char('中'), KeyModifiers::NONE))
        );
        assert_eq!(
            events[1],
            WireEvent::Input(Event::Paste("paste👩‍💻\x1b_Gi=100;OK\x1b\\".into()))
        );
        assert!(matches!(&events[2],WireEvent::Graphics(r) if r.id.get()==100));
        assert!(
            matches!(events[3],WireEvent::Input(Event::Key(k)) if k.kind==KeyEventKind::Release&&k.modifiers==KeyModifiers::CONTROL)
        );
        assert!(
            matches!(events[4],WireEvent::Input(Event::Mouse(m)) if m.column==9&&m.row==3&&m.kind==MouseEventKind::Down(jadix_tui::crossterm::event::MouseButton::Left))
        );
        assert_eq!(events[5], WireEvent::Input(Event::FocusLost));
        assert_eq!(events[6], WireEvent::Input(Event::Resize(90, 30)));
        assert!(!decoder.is_pending());
    }
    let mut decoder = InputDecoder::default();
    assert!(decoder.feed(b"\x1b").unwrap().is_empty());
    assert_eq!(
        decoder.flush_escape(),
        vec![WireEvent::Input(key(KeyCode::Esc, KeyModifiers::NONE))]
    );
    for string in [b"\x1b]52;c;YWJj\x07".as_slice(), b"\x1bPunknown\x1b\\"] {
        let mut events = Vec::new();
        for byte in string {
            events.extend(decoder.feed(&[*byte]).unwrap());
        }
        assert_eq!(events, vec![WireEvent::Unknown(string.to_vec())]);
    }
    assert_eq!(
        decoder.feed(b"\x1b[12~").unwrap(),
        vec![WireEvent::Input(key(KeyCode::F(2), KeyModifiers::NONE))]
    );
    assert_eq!(
        decoder.feed(b"\x1b[97:65;2u").unwrap(),
        vec![WireEvent::Input(key(
            KeyCode::Char('A'),
            KeyModifiers::SHIFT
        ))]
    );
    let native = "\x1b_Gi=101;OK\x1b\\";
    let events: Vec<_> = native
        .chars()
        .flat_map(|c| {
            decoder
                .feed_native(key(
                    if c == '\x1b' {
                        KeyCode::Esc
                    } else {
                        KeyCode::Char(c)
                    },
                    KeyModifiers::NONE,
                ))
                .unwrap()
        })
        .collect();
    assert!(matches!(&events[..],[WireEvent::Graphics(r)] if r.id.get()==101));
    assert_eq!(
        decoder
            .feed_native(key(KeyCode::Char('q'), KeyModifiers::CONTROL))
            .unwrap(),
        vec![WireEvent::Input(key(
            KeyCode::Char('q'),
            KeyModifiers::CONTROL
        ))]
    );
    assert!(decoder.feed(b"\x1b[200~").unwrap().is_empty());
    assert!(decoder
        .feed(&vec![b'x'; InputDecoder::MAX_PASTE_BYTES + 1])
        .is_err());
}

#[test]
fn escape_deadline_does_not_split_recognized_control_packets_or_utf8() {
    for packet in [
        b"\x1b]10;rgb:1234/abcd/ffff\x1b\\".as_slice(),
        b"\x1b]11;rgb:f/0/a\x07",
        b"\x1bPunknown reply\x1b\\",
        b"\x1b^opaque\x1b\\",
        b"\x1bXopaque\x1b\\",
        b"\x1b_Gi=101;OK\x1b\\",
        b"\x1b[?1;2c",
        b"\x1b[97;5:3u",
        b"\x1bOP",
        "\x1b中".as_bytes(),
        "中👩‍💻".as_bytes(),
    ] {
        let expected = InputDecoder::default().feed(packet).unwrap();
        for split in if packet[0] == 27 { 2 } else { 1 }..packet.len() {
            let mut decoder = InputDecoder::default();
            let mut events = decoder.feed(&packet[..split]).unwrap();
            for _ in 0..3 {
                events.extend(decoder.flush_escape());
            }
            events.extend(decoder.feed(&packet[split..]).unwrap());
            assert_eq!(events, expected, "packet={packet:?}, split={split}");
            assert!(!decoder.is_pending());
            assert_eq!(
                decoder.feed(b"x").unwrap(),
                vec![WireEvent::Input(key(
                    KeyCode::Char('x'),
                    KeyModifiers::NONE
                ))]
            );
        }
    }
}

#[test]
fn native_fragmented_replies_survive_deadlines_and_non_text_records() {
    let mut decoder = InputDecoder::default();
    for ch in "\x1b]".chars() {
        assert!(decoder
            .feed_native(key(
                if ch == '\x1b' {
                    KeyCode::Esc
                } else {
                    KeyCode::Char(ch)
                },
                KeyModifiers::NONE
            ))
            .unwrap()
            .is_empty());
    }
    assert!(decoder.flush_escape().is_empty());
    assert_eq!(
        decoder.feed_native(Event::Resize(91, 27)).unwrap(),
        vec![WireEvent::Input(Event::Resize(91, 27))]
    );
    for ch in "11;rgb:f/f/f\x1b".chars() {
        assert!(decoder
            .feed_native(key(
                if ch == '\x1b' {
                    KeyCode::Esc
                } else {
                    KeyCode::Char(ch)
                },
                KeyModifiers::NONE
            ))
            .unwrap()
            .is_empty());
        assert!(decoder.flush_escape().is_empty());
    }
    assert_eq!(
        decoder
            .feed_native(key(KeyCode::Char('\\'), KeyModifiers::NONE))
            .unwrap(),
        vec![WireEvent::Unknown(b"\x1b]11;rgb:f/f/f\x1b\\".to_vec())]
    );
    assert_eq!(
        decoder
            .feed_native(key(KeyCode::Char('q'), KeyModifiers::CONTROL))
            .unwrap(),
        vec![WireEvent::Input(key(
            KeyCode::Char('q'),
            KeyModifiers::CONTROL
        ))]
    );
}

#[test]
fn incomplete_control_packets_keep_their_existing_byte_limits() {
    for prefix in [b"\x1b]".as_slice(), b"\x1bP", b"\x1b_", b"\x1b^", b"\x1bX"] {
        let mut decoder = InputDecoder::default();
        assert!(decoder.feed(prefix).unwrap().is_empty());
        assert!(decoder.flush_escape().is_empty());
        assert!(decoder.feed(&[b'a'; 1022]).unwrap().is_empty());
        assert!(decoder.feed(b"a").is_err());
    }
    let mut decoder = InputDecoder::default();
    assert!(decoder.feed(b"\x1b[").unwrap().is_empty());
    assert!(decoder.flush_escape().is_empty());
    assert!(decoder.feed(&[b'1'; 126]).unwrap().is_empty());
    assert!(decoder.feed(b"1").is_err());
}

#[test]
fn document_graphemes_selection_undo_redo_and_budgets_survive_host_events() {
    let mut editor = TerminalEditor::new("中e\u{301}👩‍💻\nnext").unwrap();
    let end = "中e\u{301}👩‍💻".len();
    editor.set_selection(end, end).unwrap();
    editor
        .handle(&key(KeyCode::Backspace, KeyModifiers::NONE))
        .unwrap();
    assert_eq!(editor.document().text(), "中e\u{301}\nnext");
    editor
        .handle(&key(KeyCode::Backspace, KeyModifiers::NONE))
        .unwrap();
    assert_eq!(editor.document().text(), "中\nnext");
    editor
        .handle(&key(KeyCode::Char('z'), KeyModifiers::CONTROL))
        .unwrap();
    assert_eq!(editor.document().text(), "中e\u{301}\nnext");
    editor
        .handle(&key(KeyCode::Char('y'), KeyModifiers::CONTROL))
        .unwrap();
    editor
        .handle(&key(KeyCode::Char('a'), KeyModifiers::CONTROL))
        .unwrap();
    editor.handle(&Event::Paste("新\r\n文本".into())).unwrap();
    assert_eq!(editor.document().text(), "新\n文本");
    assert_eq!(editor.caret(), editor.document().len());
    let snapshot = editor.document().clone();
    editor.handle(&Event::FocusLost).unwrap();
    editor.handle(&Event::Resize(1, 1)).unwrap();
    assert_eq!(editor.document(), &snapshot);
    assert!(editor.set_selection(1, 1).is_err());
    assert!(editor
        .insert(&"x".repeat(TerminalEditor::MAX_BYTES))
        .is_err());
    assert_eq!(editor.document(), &snapshot, "budget failure is atomic");
    let release = Event::Key(KeyEvent::new_with_kind(
        KeyCode::Char('x'),
        KeyModifiers::NONE,
        KeyEventKind::Release,
    ));
    assert!(!editor.handle(&release).unwrap());
    for _ in 0..150 {
        editor.insert("a").unwrap();
    }
    assert!(editor.history_bytes() <= 8 * 1024 * 1024);
}

#[derive(Clone, Default)]
struct Output(Rc<RefCell<Vec<u8>>>);
impl Write for Output {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn actual_terminal_host_probes_fences_coalesces_idle_resize_and_unwind_cleanup() {
    let output = Output::default();
    let vp = TerminalViewport::new(Rect::new(0, 0, 4, 3), [4, 3], 1.0).unwrap();
    let now = Instant::now();
    let id = |n| NonZeroU32::new(n).unwrap();
    let reply = |n| KittyReply {
        id: id(n),
        message: "OK".into(),
    };
    let mut host =
        KittyTerminal::new(output.clone(), vp, id(1), id(100), Duration::from_secs(1)).unwrap();
    host.start(now).unwrap();
    assert!(!host
        .present(now, |_| panic!("unconfirmed terminal must not render"))
        .unwrap());
    host.acknowledge(&reply(100), now).unwrap();
    for value in 0..2 {
        host.invalidate();
        assert!(host
            .present(now, |_| RgbaFrame::new(4, 3, vec![value; 48]))
            .unwrap());
    }
    host.invalidate();
    assert!(!host
        .present(now, |_| panic!("no third in-flight render"))
        .unwrap());
    assert_eq!(host.in_flight(), 2);
    host.acknowledge(&reply(101), now).unwrap();
    assert!(host
        .present(now, |_| RgbaFrame::new(4, 3, vec![3; 48]))
        .unwrap());
    let bytes = output.0.borrow().len();
    assert!(!host
        .present(now, |_| panic!("idle must not render"))
        .unwrap());
    assert_eq!(bytes, output.0.borrow().len());
    host.acknowledge(&reply(102), now).unwrap();
    host.acknowledge(&reply(103), now).unwrap();
    host.resize(TerminalViewport::new(Rect::new(0, 0, 5, 4), [4, 3], 1.0).unwrap())
        .unwrap();
    assert!(host
        .present(now, |_| RgbaFrame::new(4, 3, vec![4; 48]))
        .unwrap());
    assert!(host.check(now + Duration::from_secs(2)).is_err());
    host.close().unwrap();
    assert!(output.0.borrow().ends_with(b"\x1b_Ga=d,d=I,i=1,q=2;\x1b\\"));
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _host =
            KittyTerminal::new(output.clone(), vp, id(2), id(200), Duration::from_secs(1)).unwrap();
        panic!("exercise scoped cleanup");
    }));
    assert!(output.0.borrow().ends_with(b"\x1b_Ga=d,d=I,i=2,q=2;\x1b\\"));
}

#[test]
fn queued_results_revalidate_revision_identity_cancellation_and_teardown() {
    use jadix_tui::{Mailbox, RequestSlot};
    use std::num::NonZeroUsize;
    let mailbox = Mailbox::new(NonZeroUsize::new(4).unwrap()).unwrap();
    let slot = RequestSlot::default();
    let first = slot.replace().unwrap();
    assert!(first.publish(&mailbox.sink(), "old queued").is_ok());
    let second = slot.replace().unwrap();
    assert!(first.cancellation().is_cancelled());
    assert!(!slot.accepts(&mailbox.try_recv().unwrap().ticket));
    assert!(first.publish(&mailbox.sink(), "late").is_err());
    assert!(second.publish(&mailbox.sink(), "new").is_ok());
    let update = mailbox.try_recv().unwrap();
    assert!(slot.accepts(&update.ticket));
    assert_eq!(update.value, "new");
    let foreign_slot = RequestSlot::default();
    let foreign = foreign_slot.replace().unwrap();
    assert!(foreign.is_current());
    assert!(!slot.accepts(&foreign));
    slot.cancel();
    assert!(!slot.accepts(&second));
    let third = slot.replace().unwrap();
    let cancelled = third.cancellation();
    drop(slot);
    assert!(cancelled.is_cancelled());
    assert!(third.publish(&mailbox.sink(), "disposed").is_err());
}

#[test]
fn compression_negotiates_before_pixels_and_rejection_falls_back() {
    for accepted in [true, false] {
        let output = Output::default();
        let vp = TerminalViewport::new(Rect::new(0, 0, 4, 3), [32, 32], 1.0).unwrap();
        let id = |n| NonZeroU32::new(n).unwrap();
        let now = Instant::now();
        let mut host =
            KittyTerminal::new(output.clone(), vp, id(1), id(100), Duration::from_secs(1)).unwrap();
        host.request_compression().unwrap();
        host.start(now).unwrap();
        host.acknowledge(
            &KittyReply {
                id: id(100),
                message: "OK".into(),
            },
            now,
        )
        .unwrap();
        assert!(!host
            .present(now, |_| panic!("compression probe pending"))
            .unwrap());
        assert!(String::from_utf8_lossy(&output.0.borrow()).contains("o=z,i=101"));
        host.acknowledge(
            &KittyReply {
                id: id(101),
                message: if accepted { "OK" } else { "ENOTSUP" }.into(),
            },
            now,
        )
        .unwrap();
        assert_eq!(host.compressed(), accepted);
        assert!(host
            .present(now, |_| RgbaFrame::new(
                32,
                32,
                [30, 80, 120, 255].repeat(32 * 32)
            ))
            .unwrap());
        let bytes = String::from_utf8_lossy(&output.0.borrow()).into_owned();
        let upload = bytes
            .split("\x1b_G")
            .find(|part| part.split(';').next().unwrap().contains("a=T"))
            .unwrap();
        assert_eq!(upload.split(';').next().unwrap().contains("o=z"), accepted);
        use base64::Engine;
        use std::io::Read;
        let mut encoded = String::new();
        for part in bytes
            .split("\x1b_G")
            .skip_while(|part| !part.split(';').next().unwrap().contains("a=T"))
        {
            let (header, payload) = part.split_once(';').unwrap();
            encoded.push_str(payload.split("\x1b\\").next().unwrap());
            if header.contains("m=0") {
                break;
            }
        }
        let payload = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .unwrap();
        let raw = if accepted {
            let mut raw = Vec::new();
            flate2::read::ZlibDecoder::new(payload.as_slice())
                .take(4097)
                .read_to_end(&mut raw)
                .unwrap();
            assert!(payload.len() < raw.len());
            raw
        } else {
            payload
        };
        assert_eq!(raw, [30, 80, 120, 255].repeat(32 * 32));
        assert!(host.request_compression().is_err());
    }
}
