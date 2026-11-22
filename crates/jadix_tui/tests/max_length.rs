use jadix_core::presentation::*;
use jadix_tui::{crossterm::event::*, CellSurface};

fn descriptor(limit: usize) -> NodeDescriptor {
    NodeDescriptor {
        content: NodeContent::Input {
            value: String::new(),
            placeholder: String::new(),
            enabled: true,
            password: false,
            max_length: Some(limit),
            revision: 0,
        },
        frame: Rect {
            x: 0.,
            y: 0.,
            width: 24.,
            height: 2.,
        },
        style: Default::default(),
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
    }
}
fn paint(view: &mut CellSurface) {
    use jadix_tui::ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};
    let area = Rect::new(0, 0, 40, 8);
    view.render(area, &mut Buffer::empty(area));
}
fn key(code: KeyCode, modifiers: KeyModifiers) -> Event {
    Event::Key(KeyEvent::new(code, modifiers))
}

#[test]
fn canonical_limit_reaches_full_and_incremental_publication_without_resetting_drafts() {
    let root = RootId::new();
    let id = NodeId::new(root);
    let mut view = CellSurface::new(RootLifetime::new(root), [1., 1.]).unwrap();
    view.apply_batch(&CommitBatch {
        root,
        previous: 0,
        revision: 1,
        operations: vec![
            Operation::Create {
                node: id,
                parent: None,
                index: 0,
                descriptor: descriptor(5),
            },
            Operation::Focus { node: Some(id) },
        ],
    })
    .unwrap();
    paint(&mut view);
    view.handle(Event::Paste("ab中🦀x".into())).unwrap();
    assert_eq!(view.field(id).unwrap().document().text(), "ab中🦀");
    // Same value revision, new constraint: keep the unacknowledged draft and its history.
    view.apply_batch(&CommitBatch {
        root,
        previous: 1,
        revision: 2,
        operations: vec![Operation::Update {
            node: id,
            descriptor: descriptor(2),
        }],
    })
    .unwrap();
    paint(&mut view);
    view.handle(Event::Paste("Z".into())).unwrap();
    assert_eq!(view.field(id).unwrap().document().text(), "ab中🦀");
    view.handle(key(KeyCode::Char('a'), KeyModifiers::CONTROL))
        .unwrap();
    // A ZWJ grapheme that cannot fit must preserve the selection and its text.
    view.handle(Event::Paste("👩‍💻".into())).unwrap();
    assert_eq!(view.field(id).unwrap().document().text(), "ab中🦀");
    view.handle(Event::Paste("🦀x".into())).unwrap();
    assert_eq!(view.field(id).unwrap().document().text(), "🦀");
    view.handle(key(KeyCode::Char('z'), KeyModifiers::CONTROL))
        .unwrap();
    assert_eq!(view.field(id).unwrap().document().text(), "ab中🦀");
    let before = view.revision();
    assert!(view
        .apply_batch(&CommitBatch {
            root,
            previous: before,
            revision: before + 1,
            operations: vec![Operation::Update {
                node: id,
                descriptor: descriptor(1024 * 1024 + 1)
            }]
        })
        .is_err());
    assert_eq!(view.revision(), before);
    assert_eq!(view.field(id).unwrap().document().text(), "ab中🦀");

    view.present(
        view.revision(),
        vec![PresentationNode {
            id,
            descriptor: descriptor(0),
            children: vec![],
        }],
    )
    .unwrap();
    paint(&mut view);
    view.handle(key(KeyCode::Char('a'), KeyModifiers::CONTROL))
        .unwrap();
    view.handle(Event::Paste("x".into())).unwrap();
    assert_eq!(view.field(id).unwrap().document().text(), "ab中🦀");
    view.handle(key(KeyCode::Backspace, KeyModifiers::NONE))
        .unwrap();
    assert_eq!(view.field(id).unwrap().document().text(), "");
}
