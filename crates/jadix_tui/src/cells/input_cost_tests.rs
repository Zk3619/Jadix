use super::*;
use jadix_core::presentation::RootId;
thread_local! {
    pub(super) static REVISION_VISITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
pub(super) fn field(id: NodeId, text: &str, revision: u64, y: f64) -> PresentationNode {
    PresentationNode {
        id,
        descriptor: NodeDescriptor {
            content: NodeContent::Input {
                value: text.into(),
                placeholder: String::new(),
                enabled: true,
                password: false,
                revision,
                max_length: None,
            },
            style: Default::default(),
            frame: jadix_core::presentation::Rect {
                x: 0.,
                y,
                width: 32.,
                height: 1.,
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
pub(super) fn editor_surface(rows: usize) -> (CellSurface, Vec<PresentationNode>) {
    let root = RootId::new();
    let nodes = (0..rows)
        .map(|i| field(NodeId::new(root), "seed", 0, i as f64 * 2.))
        .collect::<Vec<_>>();
    let mut surface = CellSurface::new(RootLifetime::new(root), [1., 1.]).unwrap();
    surface.present(0, nodes.clone()).unwrap();
    surface.layout(Rect::new(0, 0, 40, 12));
    (surface, nodes)
}
#[test]
fn one_editor_input_budget_queries_do_not_visit_other_mounted_editors() {
    for rows in [32, 256, 1024, 3072] {
        let (mut surface, nodes) = editor_surface(rows);
        surface.focus_node(nodes[0].id).unwrap();
        let mut reference =
            CellSurface::new(RootLifetime::new(nodes[0].id.root), [1., 1.]).unwrap();
        reference.present(0, nodes.clone()).unwrap();
        let area = Rect::new(0, 0, 40, 12);
        reference.layout(area);
        reference.focus_node(nodes[0].id).unwrap();
        for round in 0..6 {
            crate::editor::RETAINED_QUERIES.with(|n| n.set(0));
            let start = std::time::Instant::now();
            let event = surface.handle(Event::Paste("🦀".into())).unwrap().unwrap();
            let mut image = Buffer::empty(area);
            (&mut surface).render(area, &mut image);
            let nanos = start.elapsed().as_nanos();
            let queries = crate::editor::RETAINED_QUERIES.with(std::cell::Cell::get);
            let expected = reference
                .handle(Event::Paste("🦀".into()))
                .unwrap()
                .unwrap();
            reference
                .present(reference.revision(), nodes.clone())
                .unwrap();
            let mut expected_image = Buffer::empty(area);
            (&mut reference).render(area, &mut expected_image);
            assert_eq!(
                image, expected_image,
                "full-publication reference image rows={rows} round={round}"
            );
            assert_eq!(event, expected);
            println!("input rows={rows} round={round} retained_queries={queries} nanos={nanos}");
            assert!(
                queries <= 4,
                "one editor input visited unrelated editor budgets: rows={rows} queries={queries}"
            );
            assert_eq!(event.node, nodes[0].id);
            let EventPayload::Edit {
                value, revision, ..
            } = event.payload
            else {
                panic!("expected actual edit event")
            };
            assert_eq!(value, format!("{}seed", "🦀".repeat(round + 1)));
            assert_eq!(revision, round as u64 + 1);
            assert_eq!(
                surface.field(nodes[rows - 1].id).unwrap().document().text(),
                "seed"
            );
        }
    }
}

#[test]
fn event_admission_does_not_rescan_revisions_of_other_interacted_controls() {
    for rows in [32, 256, 1024, 3072] {
        let root = RootId::new();
        let nodes = (0..rows)
            .map(|i| {
                let mut node = field(NodeId::new(root), "", 0, i as f64 * 2.);
                node.descriptor.content = NodeContent::Checkbox {
                    label: format!("Choice {i}"),
                    checked: false,
                    enabled: true,
                    revision: 0,
                };
                node
            })
            .collect::<Vec<_>>();
        let mut surface = CellSurface::new(RootLifetime::new(root), [1., 1.]).unwrap();
        surface.present(0, nodes.clone()).unwrap();
        surface.layout(Rect::new(0, 0, 40, 12));
        for node in &nodes {
            surface.focus_node(node.id).unwrap();
            assert!(matches!(
                surface
                    .handle(Event::Key(crossterm::event::KeyEvent::new(
                        KeyCode::Char(' '),
                        KeyModifiers::NONE
                    )))
                    .unwrap()
                    .unwrap()
                    .payload,
                EventPayload::Toggle { revision: 1, .. }
            ));
        }
        surface.focus_node(nodes[0].id).unwrap();
        for round in 0..6 {
            REVISION_VISITS.with(|n| n.set(0));
            let start = std::time::Instant::now();
            let event = surface
                .handle(Event::Key(crossterm::event::KeyEvent::new(
                    KeyCode::Char(' '),
                    KeyModifiers::NONE,
                )))
                .unwrap()
                .unwrap();
            let nanos = start.elapsed().as_nanos();
            let visits = REVISION_VISITS.with(std::cell::Cell::get);
            println!("admission rows={rows} round={round} revision_entries={visits} nanos={nanos}");
            assert_eq!(
                visits, 0,
                "input admission revisited unrelated event revisions"
            );
            assert!(
                matches!(event.payload,EventPayload::Toggle{revision,..} if revision == round+2)
            );
        }
    }
}

#[cfg(feature = "portable")]
#[test]
fn one_checkpoint_restore_budget_does_not_visit_untouched_editors() {
    for rows in [32, 256, 1024, 3072] {
        let (mut surface, nodes) = editor_surface(rows);
        let saved = String::from_utf8(surface.checkpoint_field(nodes[0].id).unwrap()).unwrap();
        for round in 0..6 {
            crate::editor::RETAINED_QUERIES.with(|n| n.set(0));
            let start = std::time::Instant::now();
            let events = surface
                .restore_checkpoint_fields(&[(nodes[0].id, saved.clone())])
                .unwrap();
            let nanos = start.elapsed().as_nanos();
            let queries = crate::editor::RETAINED_QUERIES.with(std::cell::Cell::get);
            println!("restore rows={rows} round={round} retained_queries={queries} nanos={nanos}");
            assert!(
                queries <= 4,
                "one checkpoint restore scanned unrelated budgets: {rows} {queries}"
            );
            assert_eq!(events.len(), 1);
            assert!(
                matches!(events[0].payload,EventPayload::Edit{revision,..} if revision == round+1)
            );
            assert_eq!(
                surface.field(nodes[rows - 1].id).unwrap().document().text(),
                "seed"
            );
        }
    }
}
