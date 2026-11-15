use super::input_cost_tests::field;
use super::*;
use jadix_core::presentation::{CommitBatch, Operation, RootId};
fn paint(surface: &mut CellSurface, area: Rect) -> Buffer {
    let mut b = Buffer::empty(area);
    surface.render(area, &mut b);
    b
}

// Preserve the former whole-tree projection as a correctness oracle. This does
// not use the optimized subtree boundary or its unchanged-shift shortcut.
fn full_overlay_geometry(surface: &mut CellSurface, area: Rect) {
    let mut offsets = HashMap::new();
    surface.layout_geometry(area, &offsets, 0..surface.nodes.len());
    for index in surface.paint_order.clone() {
        let node = &surface.nodes[index];
        let NodeContent::Popover {
            anchor,
            placement,
            gap,
            open: true,
            ..
        } = node.descriptor.content
        else {
            continue;
        };
        let g = surface.geometry[surface.node_indices[&anchor]].frame;
        let frame = node.descriptor.frame;
        let at = jadix_layout::presentation_overlay::position(
            jadix_core::presentation::Rect {
                x: (g[0] - i64::from(area.x)) as f64 * surface.cell_size[0],
                y: (g[1] - i64::from(area.y)) as f64 * surface.cell_size[1],
                width: (g[2] - g[0]) as f64 * surface.cell_size[0],
                height: (g[3] - g[1]) as f64 * surface.cell_size[1],
            },
            [frame.width, frame.height],
            [
                f64::from(area.width) * surface.cell_size[0],
                f64::from(area.height) * surface.cell_size[1],
            ]
            .map(|v| v as f32),
            placement,
            gap,
        );
        offsets.insert(node.id, [at[0] - frame.x, at[1] - frame.y]);
        surface.layout_geometry(area, &offsets, 0..surface.nodes.len());
    }
}

#[test]
fn moved_popovers_project_only_their_subtrees_and_match_full_tree_oracle() {
    use jadix_core::presentation::PopoverPlacement;
    for unrelated in [0, 256, 1024, 4096] {
        let root = RootId::new();
        let anchor = field(NodeId::new(root), "anchor", 0, 2.);
        let mut roots = vec![anchor.clone()];
        // Four pairs include nested anchors and Scrolls inside moved portals.
        for pair in 0..4 {
            let mut outer = field(NodeId::new(root), "", 0, 0.);
            outer.descriptor.frame.width = 18.;
            outer.descriptor.frame.height = 7.;
            outer.descriptor.content = NodeContent::Popover {
                anchor: anchor.id,
                placement: PopoverPlacement::BottomStart,
                gap: pair,
                open: true,
                close_on_outside_click: true,
                revision: 0,
            };
            let mut nested_anchor = field(NodeId::new(root), "nested anchor", 0, 1.);
            nested_anchor.descriptor.frame.x = 3.;
            let mut inner = field(NodeId::new(root), "", 0, 0.);
            inner.descriptor.frame.width = 10.;
            inner.descriptor.frame.height = 4.;
            inner.descriptor.content = NodeContent::Popover {
                anchor: nested_anchor.id,
                placement: PopoverPlacement::RightStart,
                gap: 1,
                open: true,
                close_on_outside_click: true,
                revision: 0,
            };
            let mut scroll = field(NodeId::new(root), "", 0, 0.);
            scroll.descriptor.content = NodeContent::Scroll;
            scroll.descriptor.frame.width = 9.;
            scroll.descriptor.frame.height = 2.;
            scroll
                .children
                .push(field(NodeId::new(root), "popup text", 0, 3.));
            inner.children.push(scroll);
            outer.children = vec![nested_anchor, inner];
            roots.push(outer);
        }
        for i in 0..unrelated {
            let mut text = field(NodeId::new(root), "", 0, 100. + i as f64);
            text.descriptor.content = NodeContent::Text("unrelated".into());
            roots.push(text);
        }
        let mut surface = CellSurface::new(RootLifetime::new(root), [1., 1.]).unwrap();
        surface.present(0, roots).unwrap();
        surface.automatic = true;
        for area in [Rect::new(3, 2, 80, 25), Rect::new(7, 4, 24, 10)] {
            interaction_tests::GEOMETRY_VISITS.with(|n| n.set(0));
            surface.layout(area);
            let visits = interaction_tests::GEOMETRY_VISITS.with(std::cell::Cell::get);
            let optimized = surface.geometry.clone();
            let cells = paint(&mut surface, area);
            let hits: Vec<_> = (area.y..area.bottom())
                .flat_map(|y| (area.x..area.right()).map(move |x| (x, y)))
                .map(|(x, y)| surface.hit(x, y))
                .collect();
            full_overlay_geometry(&mut surface, area);
            for (index, (actual, expected)) in optimized.iter().zip(&surface.geometry).enumerate() {
                assert_eq!(actual.frame, expected.frame, "frame node {index}");
                assert_eq!(actual.clip, expected.clip, "clip node {index}");
                assert_eq!(actual.live, expected.live, "live node {index}");
                assert_eq!(actual.scroll, expected.scroll, "scroll node {index}");
                assert_eq!(actual.shift, expected.shift, "shift node {index}");
            }
            assert_eq!(cells, paint(&mut surface, area));
            let reference_hits: Vec<_> = (area.y..area.bottom())
                .flat_map(|y| (area.x..area.right()).map(move |x| (x, y)))
                .map(|(x, y)| surface.hit(x, y))
                .collect();
            assert_eq!(hits, reference_hits);
            // Each outer projects 5 nodes, each inner projects 3: at most 32
            // extra visits regardless of the unrelated business tree size.
            assert!(visits <= surface.nodes.len() + 32, "{visits} visits");
            println!(
                "popover unrelated={unrelated} nodes={} geometry_visits={visits} former_visits={}",
                surface.nodes.len(),
                9 * surface.nodes.len()
            );
        }
    }
}
#[test]
fn changed_geometry_clamps_scroll_and_preserves_other_editor_draft_history_and_focus() {
    let root = RootId::new();
    let life = RootLifetime::new(root);
    let mut text = field(NodeId::new(root), "", 0, 0.);
    text.descriptor.content = NodeContent::Text("seed\n".repeat(24));
    text.descriptor.style.width = Some(32.);
    text.descriptor.style.shrink = 0.;
    let mut input = field(NodeId::new(root), "draft", 0, 0.);
    input.descriptor.style.width = Some(32.);
    input.descriptor.style.height = Some(1.);
    input.descriptor.style.shrink = 0.;
    let mut scroll = field(NodeId::new(root), "", 0, 0.);
    scroll.descriptor.content = NodeContent::Scroll;
    scroll.descriptor.style.width = Some(32.);
    scroll.descriptor.style.height = Some(3.);
    scroll.descriptor.style.shrink = 0.;
    scroll.children = vec![text.clone(), input.clone()];
    let mut button = field(NodeId::new(root), "", 0, 0.);
    button.descriptor.content = NodeContent::Button {
        label: "outside".into(),
        enabled: true,
    };
    button.descriptor.style.width = Some(32.);
    button.descriptor.style.height = Some(1.);
    let mut nodes = vec![scroll, button.clone()];
    let mut operations = vec![];
    fn create(
        n: &PresentationNode,
        parent: Option<NodeId>,
        index: usize,
        operations: &mut Vec<Operation>,
    ) {
        operations.push(Operation::Create {
            node: n.id,
            parent,
            index,
            descriptor: n.descriptor.clone(),
        });
        for (i, c) in n.children.iter().enumerate() {
            create(c, Some(n.id), i, operations);
        }
    }
    for (i, n) in nodes.iter().enumerate() {
        create(n, None, i, &mut operations);
    }
    let area = Rect::new(3, 2, 40, 12);
    let mut s = CellSurface::new(life.clone(), [1., 1.]).unwrap();
    let mut full = CellSurface::new(life, [1., 1.]).unwrap();
    s.commit_auto(
        &CommitBatch {
            root,
            previous: 0,
            revision: 1,
            operations,
        },
        area,
        0,
    )
    .unwrap();
    full.present_auto(0, nodes.clone(), area, 0).unwrap();
    for host in [&mut s, &mut full] {
        host.focus_node(input.id).unwrap();
        host.handle(Event::Paste("中文 🦀 ".into())).unwrap();
        host.focus_node(button.id).unwrap();
        let at = host.node_indices[&nodes[0].id];
        host.scroll_node(at, 0., 1000.);
        assert!(host.scroll[&nodes[0].id][1] > 0.);
    }
    assert_eq!(paint(&mut s, area), paint(&mut full, area));
    let editor = s.fields[&input.id].editor.checkpoint_bytes().unwrap();
    nodes[0].children[0].descriptor.content = NodeContent::Text("seed short".into());
    let change = CommitBatch {
        root,
        previous: s.revision(),
        revision: s.revision() + 1,
        operations: vec![Operation::Update {
            node: text.id,
            descriptor: nodes[0].children[0].descriptor.clone(),
        }],
    };
    let stats = s.commit_auto(&change, area, 0).unwrap();
    assert!(!stats.geometry_reused);
    full.present_auto(full.revision(), nodes.clone(), area, 0)
        .unwrap();
    assert_eq!(s.scroll, full.scroll);
    assert_eq!(s.scroll[&nodes[0].id][1], 0.);
    assert_eq!(
        s.fields[&input.id].editor.checkpoint_bytes().unwrap(),
        editor
    );
    assert_eq!(s.focused(), Some(button.id));
    assert_eq!(s.focused(), full.focused());
    assert_eq!(paint(&mut s, area), paint(&mut full, area));
    let mut bad = nodes[0].children[0].descriptor.clone();
    bad.style.height = Some(f32::NAN);
    let revision = s.revision();
    let image = paint(&mut s, area);
    assert!(s
        .commit_auto(
            &CommitBatch {
                root,
                previous: revision,
                revision: revision + 1,
                operations: vec![Operation::Update {
                    node: text.id,
                    descriptor: bad
                }]
            },
            area,
            0
        )
        .is_err());
    assert_eq!(s.revision(), revision);
    assert_eq!(paint(&mut s, area), image);
    assert_eq!(
        s.fields[&input.id].editor.checkpoint_bytes().unwrap(),
        editor
    );
    for host in [&mut s, &mut full] {
        host.focus_node(input.id).unwrap();
    }
    let undo = Event::Key(crossterm::event::KeyEvent::new(
        KeyCode::Char('z'),
        KeyModifiers::CONTROL,
    ));
    assert_eq!(s.handle(undo.clone()).unwrap(), full.handle(undo).unwrap());
    assert_eq!(s.fields[&input.id].editor.document().text(), "draft");
    assert_eq!(paint(&mut s, area), paint(&mut full, area));
}

#[test]
fn resize_recomputes_scroll_range_without_changing_editor_or_protocol_revision() {
    use jadix_core::presentation::style::TextOverflow;
    let root = RootId::new();
    let life = RootLifetime::new(root);
    let mut text = field(NodeId::new(root), "", 0, 0.);
    text.descriptor.content = NodeContent::Text("wrap 中文 🦀 words ".repeat(5));
    text.descriptor.style.width = None;
    text.descriptor.style.shrink = 0.;
    text.descriptor.style.text_overflow = TextOverflow::Wrap;
    let mut scroll = field(NodeId::new(root), "", 0, 0.);
    scroll.descriptor.content = NodeContent::Scroll;
    scroll.descriptor.style.width = None;
    scroll.descriptor.style.height = Some(3.);
    scroll.descriptor.style.shrink = 0.;
    scroll.children.push(text.clone());
    let mut input = field(NodeId::new(root), "draft", 0, 0.);
    input.descriptor.style.height = Some(1.);
    input.descriptor.style.shrink = 0.;
    let nodes = vec![scroll.clone(), input.clone()];
    let operations = vec![
        Operation::Create {
            node: scroll.id,
            parent: None,
            index: 0,
            descriptor: scroll.descriptor.clone(),
        },
        Operation::Create {
            node: text.id,
            parent: Some(scroll.id),
            index: 0,
            descriptor: text.descriptor.clone(),
        },
        Operation::Create {
            node: input.id,
            parent: None,
            index: 1,
            descriptor: input.descriptor.clone(),
        },
    ];
    let narrow = Rect::new(3, 2, 12, 10);
    let wide = Rect::new(3, 2, 100, 10);
    let mut sparse = CellSurface::new(life.clone(), [1., 1.]).unwrap();
    let mut full = CellSurface::new(life, [1., 1.]).unwrap();
    sparse
        .commit_auto(
            &CommitBatch {
                root,
                previous: 0,
                revision: 1,
                operations,
            },
            narrow,
            0,
        )
        .unwrap();
    full.present_auto(0, nodes.clone(), narrow, 0).unwrap();
    for host in [&mut sparse, &mut full] {
        host.set_scrollbars(true);
        host.focus_node(input.id).unwrap();
        host.handle(Event::Paste("中文 🦀 ".into())).unwrap();
        let at = host.node_indices[&scroll.id];
        host.scroll_node(at, 0., 1000.);
        assert!(host.scroll[&scroll.id][1] > 0.);
    }
    assert_eq!(paint(&mut sparse, narrow), paint(&mut full, narrow));
    for host in [&mut sparse, &mut full] {
        host.handle(Event::Mouse(crossterm::event::MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: narrow.right() - 1,
            row: narrow.y + 2,
            modifiers: KeyModifiers::NONE,
        }))
        .unwrap();
        assert!(host.scrollbar_capture.is_some());
    }
    let checkpoint = sparse.fields[&input.id].editor.checkpoint_bytes().unwrap();
    let revision = sparse.revision();
    let event_revisions = sparse.event_revisions.clone();
    for (round, (area, environment)) in [(wide, 0), (narrow, 1), (wide, 1), (wide, 2)]
        .into_iter()
        .enumerate()
    {
        sparse.relayout_structural(area, environment).unwrap();
        full.present_auto(full.revision(), nodes.clone(), area, environment)
            .unwrap();
        assert_eq!(
            sparse.revision(),
            revision,
            "viewport does not publish a protocol revision"
        );
        assert_eq!(sparse.event_revisions, event_revisions);
        assert_eq!(
            sparse.scroll_limits, full.scroll_limits,
            "new range round={round}"
        );
        assert_eq!(sparse.scroll, full.scroll, "clamped offset round={round}");
        assert_eq!(
            sparse.fields[&input.id].editor.checkpoint_bytes().unwrap(),
            checkpoint
        );
        assert_eq!(sparse.focused(), Some(input.id));
        assert_eq!(
            paint(&mut sparse, area),
            paint(&mut full, area),
            "cells and scrollbar round={round}"
        );
        if area == wide {
            assert_eq!(sparse.scroll[&scroll.id][1], 0.);
            assert!(sparse.scrollbar_capture.is_none());
            assert!(full.scrollbar_capture.is_none());
        } else {
            for host in [&mut sparse, &mut full] {
                let at = host.node_indices[&scroll.id];
                host.scroll_node(at, 0., 1000.);
            }
        }
    }
    let undo = Event::Key(crossterm::event::KeyEvent::new(
        KeyCode::Char('z'),
        KeyModifiers::CONTROL,
    ));
    assert_eq!(
        sparse.handle(undo.clone()).unwrap(),
        full.handle(undo).unwrap()
    );
    assert_eq!(sparse.fields[&input.id].editor.document().text(), "draft");
    assert_eq!(paint(&mut sparse, wide), paint(&mut full, wide));
}

#[test]
fn resize_clamps_multiple_scrolls_with_one_final_geometry_projection() {
    use jadix_core::presentation::style::TextOverflow;
    let mut projections = vec![];
    for count in [1, 4, 8] {
        let root = RootId::new();
        let mut roots = vec![];
        let mut operations = vec![];
        for index in 0..count {
            let mut text = field(NodeId::new(root), "", 0, 0.);
            text.descriptor.content = NodeContent::Text("wrap words ".repeat(8));
            text.descriptor.style.width = None;
            text.descriptor.style.text_overflow = TextOverflow::Wrap;
            text.descriptor.style.shrink = 0.;
            let mut scroll = field(NodeId::new(root), "", 0, 0.);
            scroll.descriptor.content = NodeContent::Scroll;
            scroll.descriptor.style.width = None;
            scroll.descriptor.style.height = Some(3.);
            scroll.descriptor.style.shrink = 0.;
            operations.push(Operation::Create {
                node: scroll.id,
                parent: None,
                index,
                descriptor: scroll.descriptor.clone(),
            });
            operations.push(Operation::Create {
                node: text.id,
                parent: Some(scroll.id),
                index: 0,
                descriptor: text.descriptor.clone(),
            });
            scroll.children.push(text);
            roots.push(scroll);
        }
        let narrow = Rect::new(3, 2, 12, 32);
        let wide = Rect::new(3, 2, 100, 32);
        let life = RootLifetime::new(root);
        let mut surface = CellSurface::new(life.clone(), [1., 1.]).unwrap();
        surface
            .commit_auto(
                &CommitBatch {
                    root,
                    previous: 0,
                    revision: 1,
                    operations,
                },
                narrow,
                0,
            )
            .unwrap();
        for node in &roots {
            assert!(surface.scroll_to(node.id, [0., 1e6]).unwrap());
        }
        interaction_tests::GEOMETRY_VISITS.with(|n| n.set(0));
        surface.relayout_structural(wide, 0).unwrap();
        let visits = interaction_tests::GEOMETRY_VISITS.with(std::cell::Cell::get);
        println!(
            "resize scrolls={count} nodes={} geometry_visits={visits}",
            surface.nodes.len()
        );
        assert!(surface.scroll.values().all(|offset| *offset == [0., 0.]));
        let mut full = CellSurface::new(life, [1., 1.]).unwrap();
        full.present_auto(0, roots, wide, 0).unwrap();
        assert_eq!(paint(&mut surface, wide), paint(&mut full, wide));
        projections.push((visits, surface.nodes.len()));
    }
    assert!(
        projections
            .iter()
            .all(|(visits, nodes)| *visits <= 2 * nodes),
        "clamp must not project the entire tree per Scroll: {projections:?}"
    );
}
