use jadix_core::presentation::{
    style::{NodeStyle, TextOverflow},
    EventPayload, NodeContent, NodeDescriptor, NodeId, PresentationNode, Rect as Frame, RootId,
    RootLifetime,
};
use jadix_tui::{
    crossterm::event::*,
    ratatui::{buffer::Buffer, layout::Rect, widgets::Widget},
    CellSurface,
};
fn node(id: NodeId, content: NodeContent, style: NodeStyle) -> PresentationNode {
    PresentationNode {
        id,
        descriptor: NodeDescriptor {
            content,
            style,
            frame: Frame {
                x: 0.,
                y: 0.,
                width: 8.,
                height: 3.,
            },
            accessible_description: None,
            accessible_name: None,
            accessible_role: None,
            accessible_selected: None,
            accessible_expanded: None,
            interaction: Default::default(),
            keyboard: Default::default(),
            navigation_keys: Vec::new(),
            accessibility: Default::default(),
            accessibility_relations: Default::default(),
        },
        children: vec![],
    }
}
fn key(code: KeyCode, modifiers: KeyModifiers) -> Event {
    Event::Key(KeyEvent::new(code, modifiers))
}
#[test]
fn bidi_paint_hit_selection_visual_edges_wrap_and_ellipsis_share_geometry() {
    let root = RootId::new();
    let id = NodeId::new(root);
    let mut surface = CellSurface::new(RootLifetime::new(root), [1., 1.]).unwrap();
    let area = Rect::new(0, 0, 8, 3);
    let value = "abc אבג";
    surface
        .present(
            0,
            vec![node(
                id,
                NodeContent::Textarea {
                    max_length: None,
                    value: value.into(),
                    placeholder: String::new(),
                    enabled: true,
                    revision: 0,
                },
                Default::default(),
            )],
        )
        .unwrap();
    let mut buffer = Buffer::empty(area);
    surface.render(area, &mut buffer);
    surface.focus_node(id).unwrap();
    assert_eq!(
        (0..7).map(|x| buffer[(x, 0)].symbol()).collect::<String>(),
        "abc גבא"
    );
    let event = surface
        .handle(key(KeyCode::End, KeyModifiers::NONE))
        .unwrap();
    let edit = event
        .iter()
        .find_map(|e| {
            if let EventPayload::Edit {
                selection_utf16, ..
            } = e.payload
            {
                Some(selection_utf16)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(
        edit,
        [4, 4],
        "visual right edge is the logical start of RTL run"
    );
    let event = surface
        .handle(key(KeyCode::Home, KeyModifiers::SHIFT))
        .unwrap();
    assert!(event.iter().any(|e| matches!(
        e.payload,
        EventPayload::Edit {
            selection_utf16: [0, 4],
            ..
        }
    )));
    // Reuse the same logical document and byte offsets across wrapping/resize.
    let second = NodeId::new(root);
    surface
        .present(
            surface.revision(),
            vec![node(
                second,
                NodeContent::Text("ab👨‍👩‍👧‍👦cdefghijk".into()),
                NodeStyle {
                    text_overflow: TextOverflow::Ellipsis,
                    max_lines: Some(1),
                    ..Default::default()
                },
            )],
        )
        .unwrap();
    let mut buffer = Buffer::empty(area);
    surface.render(area, &mut buffer);
    assert_eq!(buffer[(7, 0)].symbol(), "…");
    assert_eq!(buffer[(2, 0)].symbol(), "👨‍👩‍👧‍👦");
    assert_eq!(surface.find("ijk").unwrap().matches, 1);
}

#[test]
fn exact_fit_is_not_ellipsized_and_multiline_caps_preserve_graphemes() {
    for (text, lines, expected) in [
        ("12345678", 1, "12345678"),
        ("123456789", 1, "1234567…"),
        ("1234567812345678", 2, "1234567812345678"),
        ("12345678123456789", 2, "123456781234567…"),
        ("אבגדהוזחט", 1, "…זוהדגבא"),
        ("abcdefgh\nאבגדהוזחט", 2, "abcdefgh…זוהדגבא"),
    ] {
        let root = RootId::new();
        let mut surface = CellSurface::new(RootLifetime::new(root), [1., 1.]).unwrap();
        let area = Rect::new(0, 0, 8, 3);
        surface
            .present(
                0,
                vec![node(
                    NodeId::new(root),
                    NodeContent::Text(text.into()),
                    NodeStyle {
                        text_overflow: TextOverflow::Ellipsis,
                        max_lines: Some(lines),
                        ..Default::default()
                    },
                )],
            )
            .unwrap();
        let mut buffer = Buffer::empty(area);
        surface.render(area, &mut buffer);
        assert_eq!(
            (0..lines)
                .flat_map(|y| (0..8).map(move |x| (x, y)))
                .map(|(x, y)| buffer[(x, y)].symbol())
                .collect::<String>(),
            expected,
            "{text}"
        );
    }
}
#[test]
fn visual_wrap_up_down_home_end_and_password_do_not_reverse_logical_text() {
    let root = RootId::new();
    let id = NodeId::new(root);
    let mut surface = CellSurface::new(RootLifetime::new(root), [1., 1.]).unwrap();
    let area = Rect::new(0, 0, 8, 3);
    surface
        .present(
            0,
            vec![node(
                id,
                NodeContent::Textarea {
                    max_length: None,
                    value: "abcdefghij".into(),
                    placeholder: String::new(),
                    enabled: true,
                    revision: 0,
                },
                Default::default(),
            )],
        )
        .unwrap();
    let mut buffer = Buffer::empty(area);
    surface.render(area, &mut buffer);
    surface.focus_node(id).unwrap();
    let selection = |event: Option<jadix_core::presentation::PlatformEvent>| {
        event.and_then(|event| {
            if let EventPayload::Edit {
                selection_utf16, ..
            } = event.payload
            {
                Some(selection_utf16)
            } else {
                None
            }
        })
    };
    assert_eq!(
        selection(
            surface
                .handle(key(KeyCode::Down, KeyModifiers::NONE))
                .unwrap()
        ),
        Some([8, 8])
    );
    assert_eq!(
        selection(
            surface
                .handle(key(KeyCode::End, KeyModifiers::NONE))
                .unwrap()
        ),
        Some([10, 10])
    );
    assert_eq!(
        selection(
            surface
                .handle(key(KeyCode::Up, KeyModifiers::NONE))
                .unwrap()
        ),
        Some([2, 2])
    );
    let password = NodeId::new(root);
    surface
        .present(
            surface.revision(),
            vec![node(
                password,
                NodeContent::Input {
                    max_length: None,
                    value: "אבג".into(),
                    placeholder: String::new(),
                    enabled: true,
                    password: true,
                    revision: 0,
                },
                Default::default(),
            )],
        )
        .unwrap();
    surface.render(area, &mut buffer);
    surface.focus_node(password).unwrap();
    assert_eq!(
        selection(
            surface
                .handle(key(KeyCode::Right, KeyModifiers::NONE))
                .unwrap()
        ),
        Some([1, 1])
    );
}
