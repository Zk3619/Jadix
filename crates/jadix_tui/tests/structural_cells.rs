use jadix_core::presentation::{style::NodeStyle, *};
use jadix_tui::{
    crossterm::event::*,
    ratatui::{buffer::Buffer, layout::Rect as Area, widgets::Widget},
    CellSurface,
};
fn descriptor(content: NodeContent) -> NodeDescriptor {
    NodeDescriptor {
        content,
        style: NodeStyle {
            shrink: 0.,
            ..Default::default()
        },
        frame: Rect::default(),
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
#[test]
fn structural_mount_layout_and_cell_publication_keep_editor_state_and_visit_delta() {
    let life = RootLifetime::new(RootId::new());
    let root = NodeId::new(life.id());
    let field = NodeId::new(life.id());
    let mut surface = CellSurface::new(life.clone(), [8., 16.]).unwrap();
    let area = Area::new(0, 0, 40, 12);
    surface
        .commit_auto(
            &CommitBatch {
                root: life.id(),
                previous: 0,
                revision: 1,
                operations: vec![
                    Operation::Create {
                        node: root,
                        parent: None,
                        index: 0,
                        descriptor: descriptor(NodeContent::Container),
                    },
                    Operation::Create {
                        node: field,
                        parent: Some(root),
                        index: 0,
                        descriptor: descriptor(NodeContent::Input {
                            max_length: None,
                            value: "hello".into(),
                            placeholder: "".into(),
                            enabled: true,
                            password: false,
                            revision: 0,
                        }),
                    },
                ],
            },
            area,
            0,
        )
        .unwrap();
    let mut buffer = Buffer::empty(area);
    (&mut surface).render(area, &mut buffer);
    surface
        .handle(Event::Key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE)))
        .unwrap();
    surface
        .handle(Event::Key(KeyEvent::new(
            KeyCode::Char('!'),
            KeyModifiers::NONE,
        )))
        .unwrap();
    let selection = surface.field(field).unwrap().selection();
    let text = NodeId::new(life.id());
    let stats = surface
        .commit_auto(
            &CommitBatch {
                root: life.id(),
                previous: 1,
                revision: 2,
                operations: vec![Operation::Create {
                    node: text,
                    parent: Some(root),
                    index: 0,
                    descriptor: descriptor(NodeContent::Text("inserted".into())),
                }],
            },
            area,
            0,
        )
        .unwrap();
    assert!(stats.incremental_sync);
    assert_eq!(stats.sync_nodes, 2);
    assert_eq!(surface.field(field).unwrap().document().text(), "hello!");
    assert_eq!(surface.field(field).unwrap().selection(), selection);
    let before = surface.revision();
    assert!(surface
        .commit_auto(
            &CommitBatch {
                root: life.id(),
                previous: 2,
                revision: 3,
                operations: vec![Operation::Move {
                    node: root,
                    parent: Some(root),
                    index: 0
                }]
            },
            area,
            0
        )
        .is_err());
    assert_eq!(surface.revision(), before);
    assert_eq!(surface.field(field).unwrap().document().text(), "hello!");
}

fn publish_pair(forest: &[PresentationNode], area: Area) -> (CellSurface, CellSurface) {
    let life = RootLifetime::new(forest[0].id.root);
    let mut structural = CellSurface::new(life.clone(), [8., 16.]).unwrap();
    let mut snapshot = CellSurface::new(life.clone(), [8., 16.]).unwrap();
    let mut operations = Vec::new();
    fn create(nodes: &[PresentationNode], parent: Option<NodeId>, out: &mut Vec<Operation>) {
        for (index, node) in nodes.iter().enumerate() {
            out.push(Operation::Create {
                node: node.id,
                parent,
                index,
                descriptor: node.descriptor.clone(),
            });
            create(&node.children, Some(node.id), out);
        }
    }
    create(forest, None, &mut operations);
    structural
        .commit_auto(
            &CommitBatch {
                root: life.id(),
                previous: 0,
                revision: 1,
                operations,
            },
            area,
            0,
        )
        .unwrap();
    snapshot.present_auto(0, forest.to_vec(), area, 0).unwrap();
    (structural, snapshot)
}
fn image(surface: &mut CellSurface, area: Area) -> Buffer {
    let mut result = Buffer::empty(area);
    (&mut *surface).render(area, &mut result);
    result
}
fn leaf(root: RootId, content: NodeContent) -> PresentationNode {
    PresentationNode {
        id: NodeId::new(root),
        descriptor: descriptor(content),
        children: vec![],
    }
}
fn update(
    surface: &mut CellSurface,
    node: &PresentationNode,
    area: Area,
) -> std::io::Result<jadix_layout::presentation::LayoutStats> {
    surface.commit_auto(
        &CommitBatch {
            root: node.id.root,
            previous: surface.revision(),
            revision: surface.revision() + 1,
            operations: vec![Operation::Update {
                node: node.id,
                descriptor: node.descriptor.clone(),
            }],
        },
        area,
        0,
    )
}
#[test]
fn stable_text_button_publication_refreshes_search_preserves_focus_and_advances_noop_batches() {
    let root = RootId::new();
    let area = Area::new(0, 0, 40, 12);
    let mut forest = vec![
        leaf(root, NodeContent::Text("before".into())),
        leaf(
            root,
            NodeContent::Button {
                label: "button".into(),
                enabled: true,
            },
        ),
    ];
    for node in &mut forest {
        node.descriptor.style.width = Some(160.);
        node.descriptor.style.height = Some(20.);
    }
    let (mut host, mut full) = publish_pair(&forest, area);
    host.focus_node(forest[1].id).unwrap();
    full.focus_node(forest[1].id).unwrap();
    assert_eq!(host.find("before").unwrap().matches, 1);
    forest[0].descriptor.content = NodeContent::Text("after".into());
    update(&mut host, &forest[0], area).unwrap();
    full.present_auto(full.revision(), forest.clone(), area, 0)
        .unwrap();
    assert_eq!(image(&mut host, area), image(&mut full, area));
    assert_eq!(host.find_next(false).unwrap().matches, 0);
    assert_eq!(host.find("after").unwrap().matches, 1);
    forest[1].descriptor.content = NodeContent::Button {
        label: "changed".into(),
        enabled: true,
    };
    update(&mut host, &forest[1], area).unwrap();
    assert_eq!(host.focused(), Some(forest[1].id));
    let previous = host.revision();
    host.commit_auto(
        &CommitBatch {
            root,
            previous,
            revision: previous + 1,
            operations: vec![],
        },
        area,
        0,
    )
    .unwrap();
    update(&mut host, &forest[1], area).unwrap();
    assert_eq!(host.revision(), previous + 2);
    // Force the full publication fallback to be a no-op as well.
    let changed_area = Area::new(0, 0, 41, 12);
    update(&mut host, &forest[1], changed_area).unwrap();
    update(&mut host, &forest[1], changed_area).unwrap();
    assert_eq!(host.revision(), previous + 4);
}
#[test]
fn frame_style_accessibility_enabled_and_viewport_changes_match_full_publication() {
    let root = RootId::new();
    let mut forest = vec![
        leaf(root, NodeContent::Text("short".into())),
        leaf(
            root,
            NodeContent::Button {
                label: "button".into(),
                enabled: true,
            },
        ),
    ];
    forest[0].descriptor.style.width = Some(80.);
    let mut area = Area::new(0, 0, 40, 12);
    let (mut host, mut full) = publish_pair(&forest, area);
    host.focus_node(forest[1].id).unwrap();
    full.focus_node(forest[1].id).unwrap();
    let old_frame = host.frame(forest[1].id);
    for step in 0..5 {
        let index = match step {
            0 => {
                forest[0].descriptor.content =
                    NodeContent::Text("a label with multiple wrapped rows".into());
                0
            }
            1 => {
                forest[0].descriptor.style.width = Some(160.);
                0
            }
            2 => {
                forest[1].descriptor.accessible_name = Some("renamed action".into());
                1
            }
            3 => {
                forest[1].descriptor.content = NodeContent::Button {
                    label: "disabled".into(),
                    enabled: false,
                };
                1
            }
            _ => {
                area = Area::new(0, 0, 30, 10);
                0
            }
        };
        update(&mut host, &forest[index], area).unwrap();
        full.present_auto(full.revision(), forest.clone(), area, 0)
            .unwrap();
        assert_eq!(
            image(&mut host, area),
            image(&mut full, area),
            "step={step}"
        );
        assert_eq!(host.focused(), full.focused(), "step={step}");
        assert_eq!(host.frame(forest[1].id), full.frame(forest[1].id));
        if step == 0 {
            assert_ne!(host.frame(forest[1].id), old_frame);
        }
    }
}
#[test]
fn final_unique_text_budget_rejection_and_retry_are_atomic() {
    let root = RootId::new();
    let area = Area::new(0, 0, 40, 12);
    let mut forest = vec![leaf(root, NodeContent::Text("safe".into()))];
    forest[0].descriptor.accessible_name = Some("a".repeat(16 * 1024 * 1024 - 1024));
    forest[0].descriptor.style.width = Some(160.);
    forest[0].descriptor.style.height = Some(20.);
    let (mut host, _) = publish_pair(&forest, area);
    let before = image(&mut host, area);
    let previous = host.revision();
    let mut too_big = forest[0].descriptor.clone();
    too_big.content = NodeContent::Text("b".repeat(2048));
    let mut batch = CommitBatch {
        root,
        previous,
        revision: previous + 1,
        operations: vec![Operation::Update {
            node: forest[0].id,
            descriptor: too_big,
        }],
    };
    assert!(host.commit_auto(&batch, area, 0).is_err());
    assert_eq!(host.revision(), previous);
    assert_eq!(image(&mut host, area), before);
    // Only final unique descriptors consume candidate resources.
    forest[0].descriptor.content = NodeContent::Text("fine".into());
    batch.operations.push(Operation::Update {
        node: forest[0].id,
        descriptor: forest[0].descriptor.clone(),
    });
    host.commit_auto(&batch, area, 0).unwrap();
    assert_eq!(host.revision(), previous + 1);
    let visible: String = image(&mut host, area)
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(visible.contains("fine"));
}
