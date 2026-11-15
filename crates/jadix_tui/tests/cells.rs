use jadix_core::presentation::{
    style::NodeStyle, EventPayload, NodeContent, NodeDescriptor, NodeId, PresentationNode,
    Rect as Frame, RootId, RootLifetime,
};
use jadix_tui::{
    crossterm::event::*,
    ratatui::{buffer::Buffer, layout::Rect, widgets::Widget},
    CellColors, CellSurface,
};

fn node(id: NodeId, content: NodeContent, frame: [f64; 4]) -> PresentationNode {
    PresentationNode {
        id,
        descriptor: NodeDescriptor {
            content,
            frame: Frame {
                x: frame[0],
                y: frame[1],
                width: frame[2],
                height: frame[3],
            },
            style: NodeStyle {
                foreground: [255; 4],
                ..Default::default()
            },
            accessible_description: None,
            accessible_name: None,
            accessible_role: None,
            accessible_selected: None,
            accessible_expanded: None,
            accessibility_relations: Default::default(),
        },
        children: vec![],
    }
}
fn button(id: NodeId, y: f64) -> PresentationNode {
    node(
        id,
        NodeContent::Button {
            label: "go".into(),
            enabled: true,
        },
        [0., y, 12., 1.],
    )
}
fn input(id: NodeId, value: &str, revision: u64, password: bool) -> PresentationNode {
    node(
        id,
        NodeContent::Input {
            max_length: None,
            value: value.into(),
            placeholder: "type".into(),
            enabled: true,
            password,
            revision,
        },
        [0., 0., 12., 2.],
    )
}
fn paint(surface: &mut CellSurface, area: Rect) -> Buffer {
    let mut buffer = Buffer::empty(Rect::new(0, 0, 40, 20));
    (&mut *surface).render(area, &mut buffer);
    buffer
}
fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}
fn mouse(kind: MouseEventKind, x: u16, y: u16) -> Event {
    Event::Mouse(MouseEvent {
        kind,
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    })
}

#[test]
fn explicit_color_reduction_invalidates_paint_without_resetting_editor_state() {
    use jadix_tui::ratatui::style::Color;
    let root = RootId::new();
    let id = NodeId::new(root);
    let mut view = CellSurface::new(RootLifetime::new(root), [1., 1.]).unwrap();
    let mut field = input(id, "draft", 0, false);
    field.descriptor.style.foreground = [128, 0, 0, 255];
    field.descriptor.style.background = [12, 34, 56, 255];
    view.present(0, vec![field]).unwrap();
    let area = Rect::new(0, 0, 12, 2);
    let rgb = paint(&mut view, area);
    assert_eq!(rgb[(0, 0)].fg, Color::Rgb(128, 0, 0));
    view.handle(key(KeyCode::End)).unwrap();
    view.handle(key(KeyCode::Char('!'))).unwrap();
    let selection = view.field(id).unwrap().selection();
    view.set_colors(CellColors::Palette16);
    assert!(view.needs_frame());
    let ansi = paint(&mut view, area);
    assert_eq!(ansi[(0, 0)].fg, Color::Red);
    assert!(!ansi
        .content
        .iter()
        .any(|cell| matches!(cell.bg, Color::Rgb(..))));
    assert_eq!(view.field(id).unwrap().document().text(), "draft!");
    assert_eq!(view.field(id).unwrap().selection(), selection);
    assert_eq!(view.focused(), Some(id));
    view.set_colors(CellColors::Palette16);
    assert!(!view.needs_frame());
    view.set_colors(CellColors::Indexed256);
    let indexed = paint(&mut view, area);
    assert_eq!(indexed[(0, 0)].bg, Color::Indexed(235));
    assert_eq!(view.colors(), CellColors::Indexed256);
    view.set_colors(CellColors::Monochrome);
    let mono = paint(&mut view, area);
    assert!(mono
        .content
        .iter()
        .all(|cell| cell.fg == Color::Reset && cell.bg == Color::Reset));
    assert_eq!(view.field(id).unwrap().document().text(), "draft!");
    assert_eq!(view.field(id).unwrap().selection(), selection);
    assert_eq!(view.focused(), Some(id));
    view.set_colors(CellColors::Monochrome);
    assert!(!view.needs_frame());
    assert_eq!(
        "mono".parse::<CellColors>().unwrap(),
        CellColors::Monochrome
    );
    view.handle(Event::Key(KeyEvent::new(
        KeyCode::Left,
        KeyModifiers::SHIFT,
    )))
    .unwrap();
    let selected = paint(&mut view, area);
    assert!(selected
        .content
        .iter()
        .all(|cell| cell.fg == Color::Reset && cell.bg == Color::Reset));
    assert!(selected.content.iter().any(|cell| {
        cell.modifier
            .contains(jadix_tui::ratatui::style::Modifier::REVERSED)
    }));
    use jadix_tui::ratatui::backend::{Backend, CrosstermBackend};
    let mut output = Vec::new();
    {
        let mut backend = CrosstermBackend::new(&mut output);
        backend
            .draw(
                selected
                    .content
                    .iter()
                    .enumerate()
                    .map(|(index, cell)| ((index % 40) as u16, (index / 40) as u16, cell)),
            )
            .unwrap();
    }
    let output = String::from_utf8(output).unwrap();
    assert!(!output.contains("\x1b[38;") && !output.contains("\x1b[48;"));
    assert!(output.contains("draft"));
}

#[test]
fn transactional_tree_validation_and_disposal_leave_no_active_handlers() {
    let root = RootId::new();
    let lifetime = RootLifetime::new(root);
    let id = NodeId::new(root);
    let mut view = CellSurface::new(lifetime.clone(), [1., 1.]).unwrap();
    view.present(0, vec![button(id, 0.)]).unwrap();
    let before = paint(&mut view, Rect::new(3, 4, 12, 4));
    assert!(!view.needs_frame());
    for roots in [
        vec![button(id, 0.), button(id, 1.)],
        vec![button(NodeId::new(RootId::new()), 0.)],
    ] {
        assert!(view.present(1, roots).is_err());
        assert_eq!(view.revision(), 1);
        assert_eq!(paint(&mut view, Rect::new(3, 4, 12, 4)), before);
    }
    assert_eq!(
        view.present(0, vec![]).unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert!(!view.present(1, vec![button(id, 0.)]).unwrap());
    lifetime.dispose();
    assert!(view.needs_frame());
    paint(&mut view, Rect::new(3, 4, 12, 4));
    assert!(!view.needs_frame());
    assert_eq!(view.focused(), None);
    assert_eq!(
        view.handle(key(KeyCode::Enter)).unwrap_err().kind(),
        std::io::ErrorKind::BrokenPipe
    );
}

#[test]
fn controlled_unicode_edit_echo_preserves_history_and_utf16_selection() {
    let root = RootId::new();
    let id = NodeId::new(root);
    let mut view = CellSurface::new(RootLifetime::new(root), [1., 1.]).unwrap();
    view.present(0, vec![input(id, "", 4, false)]).unwrap();
    paint(&mut view, Rect::new(2, 3, 20, 4));
    let event = view
        .handle(Event::Paste("中🦀e\u{301}\r\nx".into()))
        .unwrap()
        .unwrap();
    let EventPayload::Edit {
        value,
        selection_utf16,
        revision,
        ..
    } = event.payload
    else {
        panic!()
    };
    assert_eq!(value, "中🦀e\u{301} x");
    assert_eq!(selection_utf16, [7, 7]);
    assert_eq!(revision, 5);
    // Same controlled snapshot is not an acknowledgement and cannot erase local editing.
    assert!(!view.present(1, vec![input(id, "", 4, false)]).unwrap());
    assert_eq!(view.field(id).unwrap().document().text(), value);
    let selection = view
        .handle(Event::Key(KeyEvent::new(
            KeyCode::Home,
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        )))
        .unwrap()
        .unwrap();
    let EventPayload::Edit {
        selection_utf16, ..
    } = selection.payload
    else {
        panic!()
    };
    assert!(jadix_core::presentation::valid_utf16_range(
        &value,
        selection_utf16
    ));
    assert!(view.field(id).unwrap().anchor() > view.field(id).unwrap().caret());
    view.handle(Event::Key(KeyEvent::new(
        KeyCode::End,
        KeyModifiers::CONTROL,
    )))
    .unwrap();
    assert!(view.present(1, vec![input(id, "stale", 4, false)]).is_err());
    view.present(1, vec![input(id, &value, 5, false)]).unwrap();
    assert!(view.field(id).unwrap().can_undo());
    view.handle(Event::Key(KeyEvent::new(
        KeyCode::Char('z'),
        KeyModifiers::CONTROL,
    )))
    .unwrap();
    assert_eq!(view.field(id).unwrap().document().text(), "");
    view.handle(Event::Key(KeyEvent::new(
        KeyCode::Char('y'),
        KeyModifiers::CONTROL,
    )))
    .unwrap();
    assert_eq!(view.field(id).unwrap().document().text(), value);
    view.handle(key(KeyCode::Left)).unwrap();
    view.handle(key(KeyCode::Left)).unwrap();
    view.handle(key(KeyCode::Backspace)).unwrap();
    assert_eq!(view.field(id).unwrap().document().text(), "中🦀 x");
}

#[test]
fn non_origin_clipping_password_and_terminal_controls_are_safe() {
    let root = RootId::new();
    let a = NodeId::new(root);
    let b = NodeId::new(root);
    let mut view = CellSurface::new(RootLifetime::new(root), [1., 1.]).unwrap();
    view.present(
        0,
        vec![
            input(a, "secret🦀", 0, true),
            node(
                b,
                NodeContent::Text("中e\u{301}\x1b[31m".into()),
                [0., 2., 16., 1.],
            ),
        ],
    )
    .unwrap();
    let buffer = paint(&mut view, Rect::new(5, 6, 10, 4));
    let contents: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
    assert!(!contents.contains("secret"));
    assert!(!contents.contains('\x1b'));
    assert!(contents.contains("•••••••"));
    assert_eq!(buffer[(5, 8)].symbol(), "中");
    assert_eq!(buffer[(7, 8)].symbol(), "e\u{301}");
    assert_eq!(buffer[(4, 8)].symbol(), " ");
    assert_eq!(buffer[(15, 8)].symbol(), " ");
    // Pointer coordinates use the same embedded area and grapheme widths as paint.
    view.handle(mouse(MouseEventKind::Down(MouseButton::Left), 7, 6))
        .unwrap();
    view.handle(mouse(MouseEventKind::Drag(MouseButton::Left), 10, 6))
        .unwrap();
    assert_eq!(view.field(a).unwrap().selected_text(), "ret");
    view.handle(Event::FocusLost).unwrap();
    assert!(view
        .handle(Event::Paste("ignored".into()))
        .unwrap()
        .is_none());
}

#[test]
fn hidden_ancestor_modal_is_skipped_and_nested_focus_is_restored() {
    let root = RootId::new();
    let outside = NodeId::new(root);
    let hidden = NodeId::new(root);
    let dialog = NodeId::new(root);
    let inside = NodeId::new(root);
    let nested = NodeId::new(root);
    let inner = NodeId::new(root);
    let mut view = CellSurface::new(RootLifetime::new(root), [1., 1.]).unwrap();
    let make = |height, outer_open, inner_open| {
        let mut top = node(
            dialog,
            NodeContent::Dialog {
                open: outer_open,
                close_on_overlay_click: true,
                revision: 1,
            },
            [0., 1., 18., 8.],
        );
        top.children.push(button(inside, 2.));
        let mut sub = node(
            nested,
            NodeContent::Dialog {
                open: inner_open,
                close_on_overlay_click: true,
                revision: 2,
            },
            [0., 3., 16., 4.],
        );
        sub.children.push(button(inner, 4.));
        top.children.push(sub);
        let mut wrapper = node(hidden, NodeContent::Container, [0., 0., 20., height]);
        wrapper.children.push(top);
        vec![button(outside, 0.), wrapper]
    };
    view.present(0, make(0., true, true)).unwrap();
    paint(&mut view, Rect::new(0, 0, 20, 10));
    assert_eq!(view.focused(), Some(outside));
    view.present(1, make(10., true, false)).unwrap();
    assert_eq!(view.focused(), Some(inside));
    view.handle(key(KeyCode::Tab)).unwrap();
    assert_eq!(view.focused(), Some(inside));
    view.present(2, make(10., true, true)).unwrap();
    assert_eq!(view.focused(), Some(inner));
    assert_eq!(
        view.handle(key(KeyCode::Esc)).unwrap().unwrap().node,
        nested
    );
    view.present(3, make(10., true, false)).unwrap();
    assert_eq!(view.focused(), Some(inside));
    view.present(4, make(0., true, true)).unwrap();
    assert_eq!(view.focused(), Some(outside));
}

#[test]
fn scrolling_clips_paint_and_hit_testing_and_removal_cancels_capture() {
    let root = RootId::new();
    let scroll = NodeId::new(root);
    let a = NodeId::new(root);
    let b = NodeId::new(root);
    let mut view = CellSurface::new(RootLifetime::new(root), [1., 1.]).unwrap();
    let mut tree = node(scroll, NodeContent::Scroll, [0., 0., 12., 3.]);
    tree.children = vec![button(a, 0.), button(b, 4.)];
    view.present(0, vec![tree]).unwrap();
    paint(&mut view, Rect::new(3, 5, 15, 8));
    let event = view
        .handle(mouse(MouseEventKind::ScrollDown, 4, 6))
        .unwrap()
        .unwrap();
    assert_eq!(event.payload, EventPayload::Scroll { x: 0., y: 2. });
    let buffer = paint(&mut view, Rect::new(3, 5, 15, 8));
    assert_eq!(buffer[(3, 7)].symbol(), "[");
    assert_eq!(buffer[(3, 9)].symbol(), " ");
    assert_eq!(
        view.handle(mouse(MouseEventKind::Down(MouseButton::Left), 4, 7))
            .unwrap()
            .unwrap()
            .node,
        b
    );
    let mut shrunk = node(scroll, NodeContent::Scroll, [0., 0., 12., 3.]);
    shrunk.children.push(button(a, 0.));
    view.present(1, vec![shrunk]).unwrap();
    assert_eq!(
        paint(&mut view, Rect::new(3, 5, 15, 8))[(3, 5)].symbol(),
        "["
    );
    view.present(2, vec![button(a, 0.)]).unwrap();
    assert!(view
        .handle(mouse(MouseEventKind::Up(MouseButton::Left), 4, 5))
        .unwrap()
        .is_none());
}

#[test]
fn keyboard_reveal_and_wrapped_editor_caret_remain_visible_after_resize() {
    let root = RootId::new();
    let scroll = NodeId::new(root);
    let first = NodeId::new(root);
    let last = NodeId::new(root);
    let mut view = CellSurface::new(RootLifetime::new(root), [1., 1.]).unwrap();
    let mut tree = node(scroll, NodeContent::Scroll, [0., 0., 12., 3.]);
    tree.children = vec![button(first, 0.), button(last, 12.)];
    view.present(0, vec![tree]).unwrap();
    paint(&mut view, Rect::new(3, 4, 12, 3));
    assert_eq!(view.handle(key(KeyCode::Tab)).unwrap().unwrap().node, last);
    let buffer = paint(&mut view, Rect::new(3, 4, 12, 3));
    assert_eq!(buffer[(3, 6)].symbol(), "[");
    assert!(view.handle(key(KeyCode::Home)).unwrap().is_some());
    let id = NodeId::new(root);
    let mut field = input(id, "", 0, false);
    field.descriptor.frame.width = 4.;
    field.descriptor.frame.height = 2.;
    view.present(view.revision(), vec![field]).unwrap();
    paint(&mut view, Rect::new(3, 4, 12, 3));
    view.handle(Event::Paste("abcdefghijklmnopq".into()))
        .unwrap();
    let buffer = paint(&mut view, Rect::new(3, 4, 12, 3));
    assert_eq!(buffer[(3, 5)].symbol(), "q");
    assert!(buffer[(4, 5)]
        .modifier
        .contains(jadix_tui::ratatui::style::Modifier::REVERSED));
    view.handle(Event::Key(KeyEvent::new(
        KeyCode::Home,
        KeyModifiers::CONTROL,
    )))
    .unwrap();
    let buffer = paint(&mut view, Rect::new(3, 4, 12, 3));
    assert_eq!(buffer[(3, 4)].symbol(), "a");
}

#[test]
fn controlled_echo_keeps_drag_capture_but_a_new_modal_cancels_it() {
    let root = RootId::new();
    let id = NodeId::new(root);
    let modal = NodeId::new(root);
    let close = NodeId::new(root);
    let mut view = CellSurface::new(RootLifetime::new(root), [1., 1.]).unwrap();
    view.present(0, vec![input(id, "abcdef", 0, false)])
        .unwrap();
    paint(&mut view, Rect::new(2, 3, 16, 5));
    view.handle(mouse(MouseEventKind::Down(MouseButton::Left), 2, 3))
        .unwrap();
    view.handle(mouse(MouseEventKind::Drag(MouseButton::Left), 4, 3))
        .unwrap();
    view.present(1, vec![input(id, "abcdef", 1, false)])
        .unwrap();
    view.handle(mouse(MouseEventKind::Drag(MouseButton::Left), 6, 3))
        .unwrap();
    assert_eq!(view.field(id).unwrap().selected_text(), "bcde");
    let mut dialog = node(
        modal,
        NodeContent::Dialog {
            open: true,
            revision: 0,
            close_on_overlay_click: true,
        },
        [0., 1., 16., 3.],
    );
    dialog.children.push(button(close, 2.));
    view.present(2, vec![input(id, "abcdef", 1, false), dialog])
        .unwrap();
    assert!(view
        .handle(mouse(MouseEventKind::Drag(MouseButton::Left), 7, 3))
        .unwrap()
        .is_none());
    assert_eq!(view.field(id).unwrap().selected_text(), "bcde");
    view.present(view.revision(), vec![input(id, "abcdef", 1, false)])
        .unwrap();
    view.handle(mouse(MouseEventKind::Down(MouseButton::Left), 2, 3))
        .unwrap();
    view.present(view.revision(), vec![button(id, 0.)]).unwrap();
    assert!(view
        .handle(mouse(MouseEventKind::Up(MouseButton::Left), 2, 3))
        .unwrap()
        .is_none());
}

#[test]
fn aggregate_editor_budget_rejects_mutation_and_password_copy_cut_are_blocked() {
    let root = RootId::new();
    let ids: Vec<_> = (0..5).map(|_| NodeId::new(root)).collect();
    let mut view = CellSurface::new(RootLifetime::new(root), [1., 1.]).unwrap();
    let value = "a".repeat(jadix_tui::TerminalEditor::MAX_BYTES);
    let trees = ids
        .iter()
        .enumerate()
        .map(|(i, id)| {
            let mut n = input(*id, &value, 0, false);
            n.descriptor.frame.y = i as f64 * 2.;
            n
        })
        .collect();
    view.present(0, trees).unwrap();
    paint(&mut view, Rect::new(0, 0, 12, 12));
    let mut rejected = false;
    for _ in 0..5 {
        view.handle(Event::Key(KeyEvent::new(
            KeyCode::End,
            KeyModifiers::CONTROL,
        )))
        .unwrap();
        for _ in 0..9 {
            let id = view.focused().unwrap();
            let before = view.field(id).unwrap().document().text().to_owned();
            if view.handle(key(KeyCode::Backspace)).is_err() {
                assert_eq!(view.field(id).unwrap().document().text(), before);
                rejected = true;
                break;
            }
        }
        if rejected {
            break;
        }
        view.handle(key(KeyCode::Tab)).unwrap();
    }
    assert!(rejected);
    let secret = NodeId::new(root);
    view.present(view.revision(), vec![input(secret, "secret", 0, true)])
        .unwrap();
    view.handle(Event::Key(KeyEvent::new(
        KeyCode::Char('a'),
        KeyModifiers::CONTROL,
    )))
    .unwrap();
    for c in ['c', 'x'] {
        assert!(view
            .handle(Event::Key(KeyEvent::new(
                KeyCode::Char(c),
                KeyModifiers::CONTROL
            )))
            .unwrap()
            .is_none());
    }
    assert_eq!(view.field(secret).unwrap().document().text(), "secret");
    view.handle(Event::Key(KeyEvent::new(
        KeyCode::End,
        KeyModifiers::CONTROL,
    )))
    .unwrap();
    view.handle(Event::Key(KeyEvent::new(
        KeyCode::Char('v'),
        KeyModifiers::CONTROL,
    )))
    .unwrap();
    assert_eq!(view.field(secret).unwrap().document().text(), "secret");
}

#[test]
fn observed_default_background_composites_alpha_and_keeps_state_and_mono_semantics() {
    use jadix_tui::{ratatui::style::Color, DefaultColors};
    let root = RootId::new();
    let id = NodeId::new(root);
    let mut surface = CellSurface::new(RootLifetime::new(root), [1., 1.]).unwrap();
    let mut text = node(id, NodeContent::Text("X".into()), [0., 0., 4., 1.]);
    text.descriptor.style.foreground = [0, 0, 0, 128];
    surface.present(0, vec![text]).unwrap();
    let area = Rect::new(0, 0, 4, 1);
    let unknown = paint(&mut surface, area);
    assert_eq!(unknown[(0, 0)].fg, Color::Rgb(0, 0, 0));
    let defaults = DefaultColors {
        foreground: Some([10, 20, 30]),
        background: Some([255, 255, 255]),
    };
    surface.set_default_colors(defaults);
    assert!(surface.needs_frame());
    let known = paint(&mut surface, area);
    assert_eq!(known[(0, 0)].fg, Color::Rgb(127, 127, 127));
    assert_eq!(known[(0, 0)].bg, Color::Reset);
    assert_eq!(surface.default_colors(), defaults);
    surface.set_default_colors(defaults);
    assert!(!surface.needs_frame());
    surface.set_colors(CellColors::Monochrome);
    let mono = paint(&mut surface, area);
    assert_eq!(mono[(0, 0)].fg, Color::Reset);
    assert_eq!(mono[(0, 0)].bg, Color::Reset);
}

#[test]
fn focusing_an_oversized_visible_control_preserves_the_scroll_position() {
    for offset in [0.0, 4.0] {
        let root = RootId::new();
        let scroll = NodeId::new(root);
        let first = NodeId::new(root);
        let large = NodeId::new(root);
        let mut view = CellSurface::new(RootLifetime::new(root), [1., 1.]).unwrap();
        let mut tree = node(scroll, NodeContent::Scroll, [0., 0., 12., 3.]);
        let mut target = button(large, 2.0);
        target.descriptor.frame.height = 10.0;
        tree.children = vec![button(first, 0.), target];
        view.present(0, vec![tree]).unwrap();
        paint(&mut view, Rect::new(3, 4, 12, 3));
        view.scroll_to(scroll, [0.0, offset]).unwrap();
        let symbols = |buffer: Buffer| {
            buffer
                .content
                .into_iter()
                .map(|cell| cell.symbol().to_string())
                .collect::<Vec<_>>()
        };
        let before = symbols(paint(&mut view, Rect::new(3, 4, 12, 3)));
        assert_eq!(view.handle(key(KeyCode::Tab)).unwrap().unwrap().node, large);
        let after = symbols(paint(&mut view, Rect::new(3, 4, 12, 3)));
        assert_eq!(
            after, before,
            "focus must not move a partially visible oversized control"
        );
        assert!(!view.scroll_to(scroll, [0.0, offset]).unwrap());
        assert_eq!(view.focused(), Some(large));
    }
}
