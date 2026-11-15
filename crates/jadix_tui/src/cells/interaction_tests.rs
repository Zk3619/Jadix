use super::input_cost_tests::editor_surface;
use super::*;
use jadix_core::presentation::{AccessibleRole, CommitBatch, Operation, RootId};
thread_local! {
    pub(super) static GEOMETRY_VISITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    pub(super) static TAB_QUERIES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    pub(super) static SEARCH_VISITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
fn key(code: KeyCode) -> Event {
    Event::Key(crossterm::event::KeyEvent::new(code, KeyModifiers::NONE))
}
fn paint(surface: &mut CellSurface) -> Buffer {
    let mut b = Buffer::empty(Rect::new(0, 0, 40, 12));
    surface.render(b.area, &mut b);
    b
}
fn reference(nodes: &[PresentationNode]) -> CellSurface {
    let mut s = CellSurface::new(RootLifetime::new(nodes[0].id.root), [1., 1.]).unwrap();
    s.present(0, nodes.to_vec()).unwrap();
    s.layout(Rect::new(0, 0, 40, 12));
    s
}
fn budget(surface: &CellSurface) {
    assert_eq!(
        surface.editor_bytes,
        surface
            .fields
            .values()
            .map(|f| f.editor.retained_bytes())
            .sum::<usize>()
    );
}
fn update(
    surface: &mut CellSurface,
    nodes: &[PresentationNode],
) -> io::Result<jadix_core::presentation::ApplyReceipt> {
    surface.apply_batch(&CommitBatch {
        root: nodes[0].id.root,
        previous: surface.revision(),
        revision: surface.revision() + 1,
        operations: nodes
            .iter()
            .map(|n| Operation::Update {
                node: n.id,
                descriptor: n.descriptor.clone(),
            })
            .collect(),
    })
}
#[test]
fn patch_updates_editor_total_before_the_next_input_and_stages_failures_atomically() {
    let (mut surface, mut nodes) = editor_surface(3);
    nodes[0].descriptor.content = NodeContent::Input {
        value: "longer accepted value".into(),
        placeholder: String::new(),
        enabled: true,
        password: false,
        max_length: None,
        revision: 1,
    };
    update(&mut surface, &nodes[..1]).unwrap();
    budget(&surface);
    surface.focus_node(nodes[0].id).unwrap();
    surface.handle(Event::Paste("x".into())).unwrap();
    budget(&surface);
    let before = (
        surface.editor_bytes,
        surface.revision(),
        surface
            .field(nodes[0].id)
            .unwrap()
            .document()
            .text()
            .to_owned(),
    );
    nodes[0].descriptor.content = NodeContent::Input {
        value: "staged".into(),
        placeholder: String::new(),
        enabled: true,
        password: false,
        max_length: None,
        revision: 2,
    };
    nodes[1].descriptor.content = NodeContent::Input {
        value: "stale".into(),
        placeholder: String::new(),
        enabled: true,
        password: false,
        max_length: None,
        revision: 0,
    };
    assert!(update(&mut surface, &nodes[..2]).is_err());
    assert_eq!(
        (
            surface.editor_bytes,
            surface.revision(),
            surface
                .field(nodes[0].id)
                .unwrap()
                .document()
                .text()
                .to_owned()
        ),
        before
    );
    budget(&surface);
    nodes[0].descriptor.content = NodeContent::Input {
        value: "".into(),
        placeholder: String::new(),
        enabled: true,
        password: false,
        max_length: None,
        revision: 2,
    };
    update(&mut surface, &nodes[..1]).unwrap();
    budget(&surface);
    if let NodeContent::Input { value, .. } = &mut nodes[1].descriptor.content {
        *value = "seed".into();
    }
    let id = nodes[0].id;
    nodes[0].descriptor.content = NodeContent::Textarea {
        value: "long line repeated long line repeated".into(),
        placeholder: String::new(),
        enabled: true,
        max_length: None,
        revision: 3,
    };
    nodes[0].descriptor.frame.width = 8.;
    nodes[0].descriptor.frame.height = 2.;
    surface.present(surface.revision(), nodes.clone()).unwrap();
    let mut full = reference(&nodes);
    for view in [&mut surface, &mut full] {
        view.focus_node(id).unwrap();
        view.handle(Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::End,
            KeyModifiers::CONTROL,
        )))
        .unwrap();
    }
    for round in 0..4 {
        if let NodeContent::Textarea {
            value,
            enabled,
            revision,
            ..
        } = &mut nodes[0].descriptor.content
        {
            *value = format!("短文{round}");
            *enabled = round != 1;
            *revision = round + 4;
        }
        nodes[0].descriptor.accessible_role = (round == 2).then_some(AccessibleRole::TabItem);
        nodes[0].descriptor.accessible_selected = (round == 2).then_some(false);
        update(&mut surface, &nodes[..1]).unwrap();
        full.present(full.revision(), nodes.clone()).unwrap();
        assert_eq!(paint(&mut surface), paint(&mut full));
        assert_eq!(surface.focused(), full.focused());
        assert_eq!(surface.fields[&id].top_row, full.fields[&id].top_row);
        budget(&surface);
    }
    for view in [&mut surface, &mut full] {
        view.apply_batch(&CommitBatch {
            root: id.root,
            previous: view.revision(),
            revision: view.revision() + 1,
            operations: vec![Operation::Focus { node: None }],
        })
        .unwrap();
        assert_eq!(view.focused(), None);
    }
    if let NodeContent::Textarea {
        value, revision, ..
    } = &mut nodes[0].descriptor.content
    {
        *value = "refocus".into();
        *revision = 8;
    }
    update(&mut surface, &nodes[..1]).unwrap();
    full.present(full.revision(), nodes.clone()).unwrap();
    assert_eq!(surface.focused(), full.focused());
    assert_eq!(paint(&mut surface), paint(&mut full));
}
#[test]
fn warm_tab_uses_accepted_order_instead_of_retesting_all_nodes() {
    for rows in [32, 256, 1024, 3072] {
        let (mut surface, nodes) = editor_surface(rows);
        let mut full = reference(&nodes);
        for round in 0..6 {
            let old_order: Vec<_> = (0..full.nodes.len())
                .filter(|&i| full.tabbable(i))
                .collect();
            TAB_QUERIES.with(|n| n.set(0));
            let start = std::time::Instant::now();
            let event = surface.handle(key(KeyCode::Tab)).unwrap().unwrap();
            let buffer = paint(&mut surface);
            let nanos = start.elapsed().as_nanos();
            let visits = TAB_QUERIES.with(|n| n.get());
            eprintln!("tab rows={rows} round={round} record_visits={visits} nanos={nanos}");
            assert_eq!(event.node, nodes[round + 1].id);
            assert_eq!(visits, 0, "warm Tab retested unrelated eligibility");
            assert_eq!(event.node, full.nodes[old_order[round + 1]].id);
            full.layout(Rect::new(0, 0, 40, 12));
            assert_eq!(Some(event), full.handle(key(KeyCode::Tab)).unwrap());
            assert_eq!(buffer, paint(&mut full));
        }
    }
}
#[test]
fn unchanged_search_reuses_matches_on_render_and_next_without_hiding_offscreen_results() {
    for rows in [32, 256, 1024, 3072] {
        let (mut surface, nodes) = editor_surface(rows);
        assert_eq!(surface.find("seed").unwrap().matches, rows);
        let mut full = reference(&nodes);
        for round in 0..6 {
            SEARCH_VISITS.with(|n| n.set(0));
            let start = std::time::Instant::now();
            let result = surface.find_next(false).unwrap();
            let mut buffer = Buffer::empty(Rect::new(0, 0, 40, 12));
            (&mut surface).render(buffer.area, &mut buffer);
            let nanos = start.elapsed().as_nanos();
            let visits = SEARCH_VISITS.with(|n| n.get());
            eprintln!("search rows={rows} round={round} record_visits={visits} nanos={nanos}");
            assert_eq!(result.matches, rows);
            assert_eq!(result.current, Some(round + 1));
            assert_eq!(
                surface.search_matches()[round + 1].node,
                nodes[round + 1].id
            );
            assert_eq!(visits, 0, "unchanged search rescanned mounted content");
            full.clear_search();
            full.find("seed").unwrap();
            for _ in 0..=round {
                full.find_next(false).unwrap();
            }
            assert_eq!(surface.search_summary(), full.search_summary());
            assert_eq!(surface.search_matches(), full.search_matches());
            assert_eq!(buffer, paint(&mut full));
        }
    }
}
#[test]
fn one_patch_budget_does_not_rescan_untouched_editors() {
    for rows in [32, 256, 1024, 3072] {
        let (mut surface, mut nodes) = editor_surface(rows);
        let mut full = reference(&nodes);
        for round in 0..6 {
            if let NodeContent::Input {
                value, revision, ..
            } = &mut nodes[0].descriptor.content
            {
                *value = format!("seed{round}");
                *revision = round + 1;
            }
            crate::editor::RETAINED_QUERIES.with(|n| n.set(0));
            GEOMETRY_VISITS.with(|n| n.set(0));
            let start = std::time::Instant::now();
            update(&mut surface, &nodes[..1]).unwrap();
            let buffer = paint(&mut surface);
            let nanos = start.elapsed().as_nanos();
            let queries = crate::editor::RETAINED_QUERIES.with(|n| n.get());
            let geometry = GEOMETRY_VISITS.with(|n| n.get());
            eprintln!("patch rows={rows} round={round} record_visits={queries} nanos={nanos} geometry_visits={geometry}");
            assert_eq!(queries, 2, "one patch summed unrelated editors");
            assert_eq!(geometry, 0, "stable editor patch rebuilt all geometry");
            budget(&surface);
            full.present(full.revision(), nodes.clone()).unwrap();
            assert_eq!(buffer, paint(&mut full));
        }
    }
}

#[test]
fn tab_order_rebuilds_for_roving_hidden_disabled_modal_reorder_and_disposal() {
    use super::input_cost_tests::field;
    let root = RootId::new();
    let ids = (0..7).map(|_| NodeId::new(root)).collect::<Vec<_>>();
    let mut nodes = ids
        .iter()
        .enumerate()
        .map(|(i, id)| field(*id, "seed", 0, (i * 2) as f64))
        .collect::<Vec<_>>();
    nodes[1].descriptor.accessible_role = Some(AccessibleRole::RadioItem { tab_stop: false });
    nodes[2].descriptor.accessible_role = Some(AccessibleRole::TabItem);
    nodes[2].descriptor.accessible_selected = Some(false);
    if let NodeContent::Input { enabled, .. } = &mut nodes[3].descriptor.content {
        *enabled = false;
    }
    nodes[4].descriptor.frame.height = 0.;
    let mut surface = reference(&nodes);
    assert_eq!(surface.focused(), Some(ids[0]));
    assert_eq!(
        surface.handle(key(KeyCode::Tab)).unwrap().unwrap().node,
        ids[5]
    );
    // Programmatic roving focus may be excluded from Tab: preserve legacy wrap semantics.
    surface.focus_node(ids[1]).unwrap();
    assert_eq!(
        surface.handle(key(KeyCode::BackTab)).unwrap().unwrap().node,
        ids[6]
    );
    nodes.swap(0, 6);
    surface.present(surface.revision(), nodes.clone()).unwrap();
    assert_eq!(
        surface.handle(key(KeyCode::Tab)).unwrap().unwrap().node,
        ids[5]
    );
    let mut modal = field(NodeId::new(root), "", 0, 0.);
    modal.descriptor.content = NodeContent::Dialog {
        open: true,
        close_on_overlay_click: true,
        revision: 0,
    };
    modal.descriptor.frame.height = 10.;
    modal.children = vec![nodes[0].clone(), nodes[6].clone()];
    surface
        .present(surface.revision(), vec![modal.clone(), nodes[5].clone()])
        .unwrap();
    for code in [KeyCode::Tab, KeyCode::BackTab, KeyCode::Tab] {
        let event = surface.handle(key(code)).unwrap().unwrap();
        assert!([ids[0], ids[6]].contains(&event.node));
    }
    modal.descriptor.frame.height = 0.;
    surface
        .present(surface.revision(), vec![modal, nodes[5].clone()])
        .unwrap();
    assert_eq!(
        surface.handle(key(KeyCode::Tab)).unwrap().unwrap().node,
        ids[5]
    );
    surface.lifetime.dispose();
    paint(&mut surface);
    assert!(surface.handle(key(KeyCode::Tab)).is_err());
}
#[test]
fn search_invalidates_for_local_edit_patch_full_publish_password_query_and_removal() {
    let (mut surface, mut nodes) = editor_surface(3);
    assert_eq!(surface.find("seed").unwrap().matches, 3);
    surface.focus_node(nodes[0].id).unwrap();
    surface.handle(Event::Paste("changed".into())).unwrap();
    assert_eq!(surface.find("changed").unwrap().matches, 1);
    // Selection/cursor operations and scrolling do not change searchable text.
    SEARCH_VISITS.with(|n| n.set(0));
    surface.handle(key(KeyCode::Left)).unwrap();
    paint(&mut surface);
    assert_eq!(SEARCH_VISITS.with(|n| n.get()), 0);
    if let NodeContent::Input {
        value, revision, ..
    } = &mut nodes[1].descriptor.content
    {
        *value = "changed".into();
        *revision = 1;
    }
    update(&mut surface, &nodes[1..2]).unwrap();
    paint(&mut surface);
    assert_eq!(surface.search_summary().matches, 2);
    // Failed acceptance cannot replace valid search results.
    let accepted = surface.search_matches().to_vec();
    let mut bad = nodes[1].clone();
    bad.descriptor.frame.width = f64::NAN;
    assert!(update(&mut surface, &[bad]).is_err());
    paint(&mut surface);
    assert_eq!(surface.search_matches(), accepted);
    if let NodeContent::Input { password, .. } = &mut nodes[1].descriptor.content {
        *password = true;
    }
    update(&mut surface, &nodes[1..2]).unwrap();
    paint(&mut surface);
    assert_eq!(surface.search_summary().matches, 1);
    surface
        .present(surface.revision(), vec![nodes[2].clone()])
        .unwrap();
    paint(&mut surface);
    assert_eq!(surface.search_summary().matches, 0);
    assert_eq!(surface.find("seed").unwrap().matches, 1);
    surface.clear_search();
    assert!(surface.search_matches().is_empty());
}
#[test]
fn search_scope_truncation_and_scroll_reveal_match_fresh_search() {
    use super::input_cost_tests::field;
    let root = RootId::new();
    let id = NodeId::new(root);
    let mut value = field(id, &"seed ".repeat(4200), 0, 0.);
    value.descriptor.frame.width = 35.;
    let mut surface = reference(&[value.clone()]);
    let result = surface.find("seed").unwrap();
    assert_eq!(result.matches, 4096);
    assert!(result.truncated);
    if let NodeContent::Input {
        value, revision, ..
    } = &mut value.descriptor.content
    {
        *value = "seed".into();
        *revision = 1;
    }
    update(&mut surface, &[value.clone()]).unwrap();
    paint(&mut surface);
    assert_eq!(surface.search_summary().matches, 1);
    assert!(!surface.search_summary().truncated);
    let mut modal = field(NodeId::new(root), "", 0, 0.);
    modal.descriptor.content = NodeContent::Dialog {
        open: true,
        close_on_overlay_click: true,
        revision: 0,
    };
    modal.descriptor.frame.height = 10.;
    let inside = field(NodeId::new(root), "seed", 0, 2.);
    modal.children = vec![inside.clone()];
    surface
        .present(surface.revision(), vec![value, modal])
        .unwrap();
    surface.find("seed").unwrap();
    assert_eq!(surface.search_matches()[0].node, inside.id);
    assert_eq!(surface.search_summary().matches, 1);
}
#[cfg(feature = "portable")]
#[test]
fn checkpoint_restoration_invalidates_search_without_losing_restored_selection() {
    let (mut surface, nodes) = editor_surface(2);
    surface.find("seed").unwrap();
    let mut saved = TerminalEditor::new("restored").unwrap();
    saved.set_selection(1, 4).unwrap();
    surface
        .restore_checkpoint_fields(&[(
            nodes[1].id,
            String::from_utf8(saved.checkpoint_bytes().unwrap()).unwrap(),
        )])
        .unwrap();
    paint(&mut surface);
    assert_eq!(surface.search_summary().matches, 1);
    assert_eq!(surface.find("restored").unwrap().matches, 1);
    assert_eq!(surface.field(nodes[1].id).unwrap().selection(), 1..4);
}

#[test]
fn controlled_echo_patch_keeps_history_clipboard_and_budget_then_external_value_resets_history() {
    let (mut surface, mut nodes) = editor_surface(2);
    surface.handle(Event::Paste("x".into())).unwrap();
    surface
        .handle(Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Char('a'),
            KeyModifiers::CONTROL,
        )))
        .unwrap();
    surface
        .handle(Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
        )))
        .unwrap();
    let retained = surface.editor_bytes;
    let id = nodes[0].id;
    if let NodeContent::Input {
        value, revision, ..
    } = &mut nodes[0].descriptor.content
    {
        *value = surface.field(id).unwrap().document().text().into();
        *revision = 1;
    }
    update(&mut surface, &nodes[..1]).unwrap();
    budget(&surface);
    assert_eq!(surface.editor_bytes, retained);
    surface
        .handle(Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Char('z'),
            KeyModifiers::CONTROL,
        )))
        .unwrap();
    budget(&surface);
    assert_eq!(
        surface.field(nodes[0].id).unwrap().document().text(),
        "seed"
    );
    if let NodeContent::Input {
        value, revision, ..
    } = &mut nodes[0].descriptor.content
    {
        *value = "external".into();
        *revision = 2;
    }
    update(&mut surface, &nodes[..1]).unwrap();
    budget(&surface);
    assert_eq!(surface.field(nodes[0].id).unwrap().history_bytes(), 0);
}
