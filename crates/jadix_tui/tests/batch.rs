use jadix_core::presentation::{style::NodeStyle, *};
use jadix_tui::{
    crossterm::event::*,
    ratatui::{buffer::Buffer, layout::Rect as Cells, widgets::Widget},
    CellSurface,
};
fn descriptor(content: NodeContent, y: f64) -> NodeDescriptor {
    NodeDescriptor {
        content,
        frame: Rect {
            x: 0.,
            y,
            width: 24.,
            height: 1.,
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
        interaction: Default::default(),
        keyboard: Default::default(),
        navigation_keys: Vec::new(),
        accessibility: Default::default(),
        accessibility_relations: Default::default(),
    }
}
fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}
fn paint(s: &mut CellSurface) -> Buffer {
    let a = Cells::new(0, 0, 40, 12);
    let mut b = Buffer::empty(a);
    s.render(a, &mut b);
    b
}
#[test]
fn batches_preserve_editor_history_and_failed_structural_changes_are_atomic() {
    let root = RootId::new();
    let life = RootLifetime::new(root);
    let mut s = CellSurface::new(life.clone(), [1., 1.]).unwrap();
    let a = NodeId::new(root);
    let b = NodeId::new(root);
    let c = NodeId::new(root);
    let input = descriptor(
        NodeContent::Textarea {
            max_length: None,
            value: String::new(),
            placeholder: String::new(),
            enabled: true,
            revision: 0,
        },
        0.,
    );
    let button = descriptor(
        NodeContent::Button {
            label: "Button".into(),
            enabled: true,
        },
        2.,
    );
    let initial = CommitBatch {
        root,
        previous: 0,
        revision: 10,
        operations: vec![
            Operation::Create {
                node: a,
                parent: None,
                index: 0,
                descriptor: input.clone(),
            },
            Operation::Create {
                node: b,
                parent: None,
                index: 1,
                descriptor: button.clone(),
            },
            Operation::Focus { node: Some(a) },
        ],
    };
    assert_eq!(
        s.apply_batch(&initial).unwrap(),
        ApplyReceipt::for_batch(&initial)
    );
    paint(&mut s);
    let event = s.handle(Event::Paste("中文 🦀".into())).unwrap().unwrap();
    let EventPayload::Edit {
        value, revision, ..
    } = event.payload
    else {
        panic!()
    };
    assert_eq!(revision, 1);
    let mut echo = input.clone();
    if let NodeContent::Textarea {
        value: v,
        revision: r,
        ..
    } = &mut echo.content
    {
        *v = value;
        *r = revision;
    }
    s.apply_batch(&CommitBatch {
        root,
        previous: 10,
        revision: 11,
        operations: vec![
            Operation::Update {
                node: a,
                descriptor: echo.clone(),
            },
            Operation::Move {
                node: b,
                parent: None,
                index: 0,
            },
        ],
    })
    .unwrap();
    assert_eq!(s.nodes().map(|(id, _)| id).collect::<Vec<_>>(), [b, a]);
    s.handle(Event::Key(KeyEvent::new(
        KeyCode::Char('z'),
        KeyModifiers::CONTROL,
    )))
    .unwrap();
    assert_eq!(s.field(a).unwrap().document().text(), "");
    let before = paint(&mut s);
    let rev = s.revision();
    let bad = CommitBatch {
        root,
        previous: rev,
        revision: rev + 1,
        operations: vec![
            Operation::Create {
                node: c,
                parent: None,
                index: 2,
                descriptor: button.clone(),
            },
            Operation::Move {
                node: a,
                parent: Some(a),
                index: 0,
            },
        ],
    };
    assert!(s.apply_batch(&bad).is_err());
    assert_eq!(s.revision(), rev);
    assert_eq!(before, paint(&mut s));
    assert!(s.nodes().all(|(id, _)| id != c));
    let bad_focus = CommitBatch {
        root,
        previous: rev,
        revision: rev + 1,
        operations: vec![
            Operation::Remove { node: b },
            Operation::Focus { node: Some(b) },
        ],
    };
    assert!(s.apply_batch(&bad_focus).is_err());
    assert!(s.nodes().any(|(id, _)| id == b));
    s.apply_batch(&CommitBatch {
        root,
        previous: rev,
        revision: rev + 2,
        operations: vec![Operation::Remove { node: b }],
    })
    .unwrap();
    assert!(s
        .apply_batch(&CommitBatch {
            root,
            previous: rev + 2,
            revision: rev + 3,
            operations: vec![Operation::Create {
                node: b,
                parent: None,
                index: 0,
                descriptor: button
            }]
        })
        .is_err());
    assert!(s.apply_batch(&initial).is_err());
    life.dispose();
    assert!(s.apply_batch(&initial).is_err());
}
#[test]
fn auto_batches_measure_new_focus_before_publication_and_reject_modal_escape() {
    let root = RootId::new();
    let mut s = CellSurface::new(RootLifetime::new(root), [8., 16.]).unwrap();
    let parent = NodeId::new(root);
    let field = NodeId::new(root);
    let modal = NodeId::new(root);
    let close = NodeId::new(root);
    let mut container = descriptor(NodeContent::Container, 0.);
    container.frame = Rect::default();
    container.style.axis = style::Axis::Column;
    let mut child = descriptor(
        NodeContent::Button {
            label: "Focusable".into(),
            enabled: true,
        },
        0.,
    );
    child.frame = Rect::default();
    let batch = CommitBatch {
        root,
        previous: 0,
        revision: 1,
        operations: vec![
            Operation::Create {
                node: parent,
                parent: None,
                index: 0,
                descriptor: container,
            },
            Operation::Create {
                node: field,
                parent: Some(parent),
                index: 0,
                descriptor: child.clone(),
            },
            Operation::Focus { node: Some(field) },
        ],
    };
    s.apply_batch_auto(&batch, Cells::new(3, 2, 60, 20), 0)
        .unwrap();
    assert_eq!(s.focused(), Some(field));
    assert!(s.frame(field).unwrap().width > 0.);
    let mut panel = descriptor(
        NodeContent::Dialog {
            open: true,
            close_on_overlay_click: true,
            revision: 0,
        },
        0.,
    );
    panel.style.width = Some(200.);
    panel.style.height = Some(80.);
    let bad = CommitBatch {
        root,
        previous: 1,
        revision: 2,
        operations: vec![
            Operation::Create {
                node: modal,
                parent: None,
                index: 1,
                descriptor: panel.clone(),
            },
            Operation::Create {
                node: close,
                parent: Some(modal),
                index: 0,
                descriptor: child,
            },
            Operation::Focus { node: Some(field) },
        ],
    };
    assert!(s
        .apply_batch_auto(&bad, Cells::new(3, 2, 60, 20), 0)
        .is_err());
    assert_eq!(s.revision(), 1);
    assert_eq!(s.nodes().count(), 2);
    let good = CommitBatch {
        operations: bad
            .operations
            .into_iter()
            .take(2)
            .chain([Operation::Focus { node: Some(close) }])
            .collect(),
        ..bad
    };
    s.apply_batch_auto(&good, Cells::new(3, 2, 60, 20), 0)
        .unwrap();
    assert_eq!(s.focused(), Some(close));
    let e = s.handle(key(KeyCode::Esc)).unwrap().unwrap();
    assert!(matches!(
        e.payload,
        EventPayload::Dismiss { revision: 1, .. }
    ));
}

#[test]
fn leaf_patches_match_full_present_and_reject_late_invalid_changes_atomically() {
    let root = RootId::new();
    let mut patched = CellSurface::new(RootLifetime::new(root), [1., 1.]).unwrap();
    let mut full = CellSurface::new(RootLifetime::new(root), [1., 1.]).unwrap();
    let mut tree: Vec<_> = (0..100)
        .map(|i| PresentationNode {
            id: NodeId::new(root),
            descriptor: descriptor(NodeContent::Text(format!("row {i}")), i as f64),
            children: vec![],
        })
        .collect();
    patched.present(0, tree.clone()).unwrap();
    full.present(0, tree.clone()).unwrap();
    paint(&mut patched);
    paint(&mut full);
    for step in 0..20 {
        let i = step % 8;
        tree[i].descriptor.content = NodeContent::Text(format!("changed {step} 中文"));
        patched
            .apply_batch(&CommitBatch {
                root,
                previous: patched.revision(),
                revision: patched.revision() + 1,
                operations: vec![Operation::Update {
                    node: tree[i].id,
                    descriptor: tree[i].descriptor.clone(),
                }],
            })
            .unwrap();
        full.present(full.revision(), tree.clone()).unwrap();
        assert_eq!(paint(&mut patched), paint(&mut full));
    }
    let before = paint(&mut patched);
    let revision = patched.revision();
    let mut valid = tree[0].descriptor.clone();
    valid.content = NodeContent::Text("must not publish".into());
    let mut invalid = tree[1].descriptor.clone();
    invalid.content = NodeContent::Text("x".repeat(16 * 1024 * 1024));
    assert!(patched
        .apply_batch(&CommitBatch {
            root,
            previous: revision,
            revision: revision + 1,
            operations: vec![
                Operation::Update {
                    node: tree[0].id,
                    descriptor: valid
                },
                Operation::Update {
                    node: tree[1].id,
                    descriptor: invalid
                }
            ]
        })
        .is_err());
    assert_eq!(patched.revision(), revision);
    assert_eq!(paint(&mut patched), before);
    // Geometry changes must leave the sparse path and publish measured frames.
    tree[0].descriptor.frame.y = 4.;
    patched
        .apply_batch(&CommitBatch {
            root,
            previous: revision,
            revision: revision + 1,
            operations: vec![Operation::Update {
                node: tree[0].id,
                descriptor: tree[0].descriptor.clone(),
            }],
        })
        .unwrap();
    full.present(full.revision(), tree).unwrap();
    assert_eq!(paint(&mut patched), paint(&mut full));
}

#[test]
fn controlled_patches_keep_history_and_failed_focus_does_not_consume_editor_state() {
    let root = RootId::new();
    let id = NodeId::new(root);
    let mut s = CellSurface::new(RootLifetime::new(root), [1., 1.]).unwrap();
    let mut d = descriptor(
        NodeContent::Textarea {
            max_length: None,
            value: String::new(),
            placeholder: String::new(),
            enabled: true,
            revision: 0,
        },
        0.,
    );
    s.present(
        0,
        vec![PresentationNode {
            id,
            descriptor: d.clone(),
            children: vec![],
        }],
    )
    .unwrap();
    paint(&mut s);
    let event = s.handle(Event::Paste("中文 🦀".into())).unwrap().unwrap();
    let EventPayload::Edit {
        value, revision, ..
    } = event.payload
    else {
        panic!()
    };
    d.content = NodeContent::Textarea {
        max_length: None,
        value,
        placeholder: String::new(),
        enabled: true,
        revision,
    };
    let rev = s.revision();
    let foreign = NodeId::new(RootId::new());
    assert!(s
        .apply_batch(&CommitBatch {
            root,
            previous: rev,
            revision: rev + 1,
            operations: vec![
                Operation::Update {
                    node: id,
                    descriptor: d.clone()
                },
                Operation::Focus {
                    node: Some(foreign)
                }
            ]
        })
        .is_err());
    assert_eq!(s.revision(), rev);
    assert_eq!(s.field(id).unwrap().document().text(), "中文 🦀");
    s.apply_batch(&CommitBatch {
        root,
        previous: rev,
        revision: rev + 1,
        operations: vec![Operation::Update {
            node: id,
            descriptor: d.clone(),
        }],
    })
    .unwrap();
    s.handle(Event::Key(KeyEvent::new(
        KeyCode::Char('z'),
        KeyModifiers::CONTROL,
    )))
    .unwrap();
    assert_eq!(s.field(id).unwrap().document().text(), "");
    let before = paint(&mut s);
    if let NodeContent::Textarea { value, .. } = &mut d.content {
        *value = "stale overwrite".into();
    }
    assert!(s
        .apply_batch(&CommitBatch {
            root,
            previous: s.revision(),
            revision: s.revision() + 1,
            operations: vec![Operation::Update {
                node: id,
                descriptor: d
            }]
        })
        .is_err());
    assert_eq!(paint(&mut s), before);
}
