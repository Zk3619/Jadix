use super::input_cost_tests::{editor_surface, field};
use super::*;
use jadix_core::presentation::{CommitBatch, Operation, RootId};
thread_local! {
    static MATCH_VISITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
pub(super) fn match_visits(count: usize) {
    MATCH_VISITS.with(|n| n.set(n.get() + count));
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
fn replace(
    view: &mut CellSurface,
    nodes: &[PresentationNode],
) -> io::Result<jadix_core::presentation::ApplyReceipt> {
    view.apply_batch(&CommitBatch {
        root: nodes[0].id.root,
        previous: view.revision(),
        revision: view.revision() + 1,
        operations: nodes
            .iter()
            .map(|n| Operation::Update {
                node: n.id,
                descriptor: n.descriptor.clone(),
            })
            .collect(),
    })
}
fn value(node: &mut PresentationNode, text: &str, revision: u64) {
    if let NodeContent::Input {
        value, revision: r, ..
    } = &mut node.descriptor.content
    {
        *value = text.into();
        *r = revision;
    } else {
        panic!("not an input")
    }
}
#[test]
fn changed_search_visits_one_source_and_matches_full_publication_in_sparse_and_dense_cases() {
    let mut nonlocal = Vec::new();
    for dense in [false, true] {
        for rows in [32, 256, 1024, 3072] {
            let (mut surface, mut nodes) = editor_surface(rows);
            if !dense {
                value(&mut nodes[0], "needle", 1);
                replace(&mut surface, &nodes[..1]).unwrap();
            }
            let query = if dense { "seed" } else { "needle" };
            surface.find(query).unwrap();
            let slots = surface.search.matches.as_ptr();
            let mut full = reference(&nodes);
            full.find(query).unwrap();
            for round in 0..6 {
                value(
                    &mut nodes[0],
                    &format!("{}{} 🦀", "x".repeat(round + 1), query),
                    round as u64 + 2,
                );
                interaction_tests::SEARCH_VISITS.with(|n| n.set(0));
                MATCH_VISITS.with(|n| n.set(0));
                let start = std::time::Instant::now();
                replace(&mut surface, &nodes[..1]).unwrap();
                let buffer = paint(&mut surface);
                let nanos = start.elapsed().as_nanos();
                let visits = interaction_tests::SEARCH_VISITS.with(|n| n.get());
                let match_visits = MATCH_VISITS.with(|n| n.get());
                eprintln!("delta dense={dense} rows={rows} round={round} source_visits={visits} match_visits={match_visits} nanos={nanos}");
                if match_visits != 1 || surface.search.matches.as_ptr() != slots {
                    nonlocal.push((dense, rows, round, match_visits));
                }
                assert_eq!(
                    visits, 1,
                    "one changed field searched unrelated source text"
                );
                interaction_tests::SEARCH_VISITS.with(|n| n.set(0));
                MATCH_VISITS.with(|n| n.set(0));
                let full_start = std::time::Instant::now();
                full.present(full.revision(), nodes.clone()).unwrap();
                let expected = paint(&mut full);
                let full_nanos = full_start.elapsed().as_nanos();
                let full_visits = interaction_tests::SEARCH_VISITS.with(|n| n.get());
                let full_match_visits = MATCH_VISITS.with(|n| n.get());
                eprintln!("fullscan dense={dense} rows={rows} round={round} source_visits={full_visits} match_visits={full_match_visits} nanos={full_nanos}");
                assert_eq!(full_match_visits, 2 * if dense { rows } else { 1 });
                assert_eq!(full_visits, rows);
                assert_eq!(surface.search_summary(), full.search_summary());
                assert_eq!(surface.search_matches(), full.search_matches());
                assert_eq!(buffer, expected);
                assert_eq!(
                    surface.search_summary().matches,
                    if dense { rows } else { 1 }
                );
            }
        }
    }
    assert!(
        nonlocal.is_empty(),
        "unchanged search slots were revisited: {nonlocal:?}"
    );
}
#[test]
fn successive_updates_coalesce_changed_sources_and_rejected_patch_keeps_current_search() {
    let (mut surface, mut nodes) = editor_surface(32);
    surface.find("seed").unwrap();
    value(&mut nodes[0], "seed one", 1);
    replace(&mut surface, &nodes[..1]).unwrap();
    value(&mut nodes[0], "seed two", 2);
    replace(&mut surface, &nodes[..1]).unwrap();
    value(&mut nodes[1], "different", 1);
    replace(&mut surface, &nodes[1..2]).unwrap();
    interaction_tests::SEARCH_VISITS.with(|n| n.set(0));
    paint(&mut surface);
    assert_eq!(interaction_tests::SEARCH_VISITS.with(|n| n.get()), 2);
    let mut full = reference(&nodes);
    full.find("seed").unwrap();
    assert_eq!(surface.search_matches(), full.search_matches());
    let mut bad = nodes[0].clone();
    bad.descriptor.frame.width = f64::NAN;
    assert!(replace(&mut surface, &[bad]).is_err());
    interaction_tests::SEARCH_VISITS.with(|n| n.set(0));
    paint(&mut surface);
    assert_eq!(interaction_tests::SEARCH_VISITS.with(|n| n.get()), 0);
}
#[test]
fn local_edit_and_checkpoint_refresh_only_changed_search_source() {
    let (mut surface, nodes) = editor_surface(32);
    surface.find("seed").unwrap();
    surface.focus_node(nodes[0].id).unwrap();
    surface.handle(Event::Paste("x".into())).unwrap();
    interaction_tests::SEARCH_VISITS.with(|n| n.set(0));
    paint(&mut surface);
    assert_eq!(interaction_tests::SEARCH_VISITS.with(|n| n.get()), 1);
    assert_eq!(surface.search_matches()[0].bytes, 1..5);
    surface
        .handle(Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Char('z'),
            KeyModifiers::CONTROL,
        )))
        .unwrap();
    paint(&mut surface);
    assert_eq!(surface.search_matches()[0].bytes, 0..4);
    #[cfg(feature = "portable")]
    {
        let saved = TerminalEditor::new("seed restored").unwrap();
        surface
            .restore_checkpoint_fields(&[(
                nodes[3].id,
                String::from_utf8(saved.checkpoint_bytes().unwrap()).unwrap(),
            )])
            .unwrap();
        interaction_tests::SEARCH_VISITS.with(|n| n.set(0));
        paint(&mut surface);
        assert_eq!(interaction_tests::SEARCH_VISITS.with(|n| n.get()), 1);
    }
}

#[test]
fn changed_match_counts_keep_unaffected_selection_and_rebuild_order_after_reorder() {
    let (mut surface, mut nodes) = editor_surface(4);
    surface.find("seed").unwrap();
    surface.find_next(false).unwrap();
    let selected = nodes[1].id;
    value(&mut nodes[0], "seed seed", 1);
    replace(&mut surface, &nodes[..1]).unwrap();
    paint(&mut surface);
    assert_eq!(
        surface.search_matches()[surface.search_summary().current.unwrap()].node,
        selected
    );
    assert_eq!(surface.search_summary().current, Some(2));
    nodes.reverse();
    surface.present(surface.revision(), nodes.clone()).unwrap();
    paint(&mut surface);
    value(&mut nodes[0], "xseed", 1);
    replace(&mut surface, &nodes[..1]).unwrap();
    interaction_tests::SEARCH_VISITS.with(|n| n.set(0));
    let buffer = paint(&mut surface);
    assert_eq!(interaction_tests::SEARCH_VISITS.with(|n| n.get()), 1);
    let mut full = reference(&nodes);
    full.find("seed").unwrap();
    assert_eq!(surface.search_matches(), full.search_matches());
    // Align independent reference to the retained selected node before paint comparison.
    while full.search_matches()[full.search_summary().current.unwrap()].node != selected {
        full.find_next(false).unwrap();
    }
    full.focus_node(surface.focused().unwrap()).unwrap();
    assert_eq!(buffer, paint(&mut full));
}
#[test]
fn truncation_falls_back_then_recovers_and_grapheme_boundaries_stay_atomic() {
    let (mut surface, mut nodes) = editor_surface(3);
    value(&mut nodes[0], &"seed ".repeat(4096), 1);
    replace(&mut surface, &nodes[..1]).unwrap();
    assert!(surface.find("seed").unwrap().truncated);
    value(&mut nodes[0], "no match", 2);
    replace(&mut surface, &nodes[..1]).unwrap();
    paint(&mut surface);
    assert_eq!(surface.search_summary().matches, 2);
    assert!(!surface.search_summary().truncated);
    value(&mut nodes[1], &"seed ".repeat(4097), 1);
    replace(&mut surface, &nodes[1..2]).unwrap();
    paint(&mut surface);
    assert_eq!(surface.search_summary().matches, 4096);
    assert!(surface.search_summary().truncated);
    value(&mut nodes[1], "á 👩‍🔬 🦀", 2);
    replace(&mut surface, &nodes[1..2]).unwrap();
    surface.find("a").unwrap();
    assert!(!surface
        .search_matches()
        .iter()
        .any(|m| m.node == nodes[1].id));
    assert_eq!(surface.find("á").unwrap().matches, 1);
    assert_eq!(surface.find("👩").unwrap().matches, 0);
    assert_eq!(surface.find("👩‍🔬").unwrap().matches, 1);
    assert_eq!(surface.find("🦀").unwrap().matches, 1);
    value(&mut nodes[1], "🦀 🦀", 3);
    replace(&mut surface, &nodes[1..2]).unwrap();
    paint(&mut surface);
    assert_eq!(surface.search_summary().matches, 2);
}
#[test]
fn stable_structural_leaf_and_modal_scope_changes_preserve_search_order() {
    let root = RootId::new();
    let mut nodes = (0..32)
        .map(|i| {
            let mut n = field(NodeId::new(root), "", 0, i as f64);
            n.descriptor.content = NodeContent::Text(format!("seed {i}"));
            n.descriptor.style.width = Some(32.);
            n.descriptor.style.height = Some(1.);
            n.descriptor.style.shrink = 0.;
            n
        })
        .collect::<Vec<_>>();
    let mut surface = CellSurface::new(RootLifetime::new(root), [1., 1.]).unwrap();
    surface
        .commit_auto(
            &CommitBatch {
                root,
                previous: 0,
                revision: 1,
                operations: nodes
                    .iter()
                    .enumerate()
                    .map(|(index, n)| Operation::Create {
                        node: n.id,
                        parent: None,
                        index,
                        descriptor: n.descriptor.clone(),
                    })
                    .collect(),
            },
            Rect::new(0, 0, 40, 12),
            0,
        )
        .unwrap();
    surface.find("seed").unwrap();
    nodes[0].descriptor.content = NodeContent::Text("xseed".into());
    surface
        .commit_auto(
            &CommitBatch {
                root,
                previous: 1,
                revision: 2,
                operations: vec![Operation::Update {
                    node: nodes[0].id,
                    descriptor: nodes[0].descriptor.clone(),
                }],
            },
            Rect::new(0, 0, 40, 12),
            0,
        )
        .unwrap();
    interaction_tests::SEARCH_VISITS.with(|n| n.set(0));
    paint(&mut surface);
    assert_eq!(interaction_tests::SEARCH_VISITS.with(|n| n.get()), 1);
    assert_eq!(
        surface
            .search_matches()
            .iter()
            .find(|m| m.node == nodes[0].id)
            .unwrap()
            .bytes,
        1..5
    );
    let computed = surface.structural_layout.as_ref().unwrap().roots().to_vec();
    let mut full = reference(&computed);
    full.find("seed").unwrap();
    assert_eq!(surface.search_matches(), full.search_matches());
    let mut modal = field(NodeId::new(root), "", 0, 0.);
    modal.descriptor.content = NodeContent::Dialog {
        open: true,
        close_on_overlay_click: true,
        revision: 0,
    };
    modal.descriptor.frame.height = 10.;
    modal.children = vec![field(NodeId::new(root), "seed", 0, 1.)];
    let mut view = reference(&[nodes[0].clone(), modal.clone()]);
    view.find("seed").unwrap();
    assert_eq!(view.search_matches().len(), 1);
    assert_eq!(view.search_matches()[0].node, modal.children[0].id);
    value(&mut modal.children[0], "seed seed", 1);
    replace(&mut view, &modal.children).unwrap();
    paint(&mut view);
    assert_eq!(view.search_matches().len(), 2);
}
#[cfg(feature = "shell")]
#[test]
fn actual_sdk_mouse_commits_with_search_match_full_mount_and_visit_one_source() {
    use crate::ShellApplication;
    use jadix_core::presentation::component::ComponentSchema;
    use jadix_shell::{application::ScriptApplication, wire::ComponentRegistry};
    fn app(rows: usize, sparse: bool) -> ShellApplication {
        let mut registry = ComponentRegistry::default();
        for schema in serde_json::from_str::<Vec<ComponentSchema>>(include_str!(
            "../../../jadix_components/schema/portable-v1.json"
        ))
        .unwrap()
        {
            registry.register(schema).unwrap();
        }
        let source = format!(
            "globalThis.scaleRows={rows};globalThis.leafPatches={sparse};globalThis.searchOffsets=true;\n{}",
            include_str!("../../../jadix_shell/tests/fixtures/scaling.js")
        );
        ShellApplication::new(
            ScriptApplication::development(
                "tests.search.delta",
                &source,
                registry,
                jadix_shell::runtime::ResourceLimits {
                    turn_milliseconds: 1000,
                    memory_bytes: 128 * 1024 * 1024,
                    ..Default::default()
                },
            )
            .unwrap(),
            [8., 16.],
        )
        .unwrap()
    }
    for rows in [32, 256, 1024, 3072] {
        let mut apps = [app(rows, true), app(rows, false)];
        for a in &mut apps {
            a.publish(Rect::new(0, 0, 40, 12), 0).unwrap();
            paint(a.surface_mut());
            a.surface_mut().find("Row").unwrap();
        }
        for round in 0..6 {
            let mut images = [None, None];
            for mode in if round % 2 == 0 { [0, 1] } else { [1, 0] } {
                let a = &mut apps[mode];
                interaction_tests::SEARCH_VISITS.with(|n| n.set(0));
                MATCH_VISITS.with(|n| n.set(0));
                let start = std::time::Instant::now();
                for kind in [
                    MouseEventKind::Down(MouseButton::Left),
                    MouseEventKind::Up(MouseButton::Left),
                ] {
                    a.handle(Event::Mouse(crossterm::event::MouseEvent {
                        kind,
                        column: 2,
                        row: 0,
                        modifiers: KeyModifiers::NONE,
                    }))
                    .unwrap();
                }
                assert_eq!(a.publish(Rect::new(0, 0, 40, 12), 0).unwrap(), 1);
                let image = paint(a.surface_mut());
                let nanos = start.elapsed().as_nanos();
                let visits = interaction_tests::SEARCH_VISITS.with(|n| n.get());
                let match_visits = MATCH_VISITS.with(|n| n.get());
                eprintln!("sdksearch rows={rows} round={round} sparse={} source_visits={visits} match_visits={match_visits} nanos={nanos}",mode==0);
                assert_eq!(a.surface().search_summary().matches, rows);
                if mode == 0 {
                    assert_eq!(visits, 1);
                    assert_eq!(match_visits, 1);
                }
                let visible: String = image.content.iter().map(|c| c.symbol()).collect();
                assert!(visible.contains(&format!("Row 0: {}", round + 1)));
                images[mode] = Some(image);
            }
            assert_eq!(images[0], images[1]);
        }
    }
}

#[test]
fn equal_counts_keep_selected_match_identity_when_it_moves_within_its_group() {
    let (mut surface, mut nodes) = editor_surface(2);
    for n in &mut nodes {
        value(n, "xxxxxseed seed", 1);
    }
    replace(&mut surface, &nodes).unwrap();
    surface.find("seed").unwrap();
    assert_eq!(surface.search_matches()[0].bytes, 5..9);
    value(&mut nodes[0], "seed seed", 2);
    replace(&mut surface, &nodes[..1]).unwrap();
    MATCH_VISITS.with(|n| n.set(0));
    let image = paint(&mut surface);
    assert_eq!(MATCH_VISITS.with(|n| n.get()), 2);
    assert_eq!(surface.search_summary().current, Some(1));
    assert_eq!(surface.search_matches()[1].bytes, 5..9);
    let mut full = reference(&nodes);
    full.find("seed").unwrap();
    full.find_next(false).unwrap();
    assert_eq!(surface.search_summary(), full.search_summary());
    assert_eq!(surface.search_matches(), full.search_matches());
    assert_eq!(image, paint(&mut full));
    assert_eq!(surface.find_next(true).unwrap().current, Some(0));
    assert_eq!(surface.find_next(true).unwrap().current, Some(3));
}
#[test]
fn coalesced_reversed_groups_do_not_move_fallback_selection_a_second_time() {
    let (mut surface, mut nodes) = editor_surface(3);
    let first_id = nodes[0].id;
    for n in &mut nodes {
        value(n, "xxxxxseed seed", 1);
    }
    nodes.reverse();
    surface.present(surface.revision(), nodes.clone()).unwrap();
    paint(&mut surface);
    surface.find("seed").unwrap();
    for _ in 0..5 {
        surface.find_next(false).unwrap();
    }
    assert_eq!(surface.search_matches()[5].node, first_id);
    assert_eq!(surface.search_matches()[5].bytes, 10..14);
    for n in &mut nodes {
        value(n, "seed seed", 2);
    }
    replace(&mut surface, &nodes).unwrap();
    MATCH_VISITS.with(|n| n.set(0));
    let image = paint(&mut surface);
    assert_eq!(MATCH_VISITS.with(|n| n.get()), 6);
    assert_eq!(surface.search_summary().current, Some(0));
    let mut full = reference(&nodes);
    full.find("seed").unwrap();
    assert_eq!(surface.search_summary(), full.search_summary());
    assert_eq!(surface.search_matches(), full.search_matches());
    full.focus_node(surface.focused().unwrap()).unwrap();
    assert_eq!(image, paint(&mut full));
}
