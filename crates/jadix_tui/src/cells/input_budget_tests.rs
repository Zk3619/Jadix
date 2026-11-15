use super::input_cost_tests::{editor_surface, field};
use super::*;
use crossterm::event::KeyEvent;
use jadix_core::presentation::RootId;
fn key(code: KeyCode, modifiers: KeyModifiers) -> Event {
    Event::Key(KeyEvent::new(code, modifiers))
}
fn exact_budget(surface: &CellSurface) {
    assert_eq!(
        surface.editor_bytes,
        surface
            .fields
            .values()
            .map(|field| field.editor.retained_bytes())
            .sum::<usize>()
    );
    assert!(surface.editor_bytes <= CellSurface::EDITOR_BYTES);
}
#[test]
fn cached_editor_budget_follows_clipboard_history_controlled_replacement_removal_and_disposal() {
    let (mut surface, mut nodes) = editor_surface(2);
    let id = nodes[0].id;
    surface.focus_node(id).unwrap();
    for event in [
        key(KeyCode::Char('a'), KeyModifiers::CONTROL),
        key(KeyCode::Char('c'), KeyModifiers::CONTROL),
        key(KeyCode::Char('x'), KeyModifiers::CONTROL),
        key(KeyCode::Char('v'), KeyModifiers::CONTROL),
        Event::Paste("🦀".into()),
        key(KeyCode::Char('z'), KeyModifiers::CONTROL),
        key(KeyCode::Char('y'), KeyModifiers::CONTROL),
    ] {
        surface.handle(event).unwrap();
        exact_budget(&surface);
    }
    let saved = String::from_utf8(surface.checkpoint_field(id).unwrap()).unwrap();
    let before = surface.editor_bytes;
    assert!(surface
        .restore_checkpoint_fields(&[(id, saved.clone()), (id, saved.clone())])
        .is_err());
    assert_eq!(surface.editor_bytes, before);
    assert!(surface
        .restore_checkpoint_fields(&[(id, "broken".into())])
        .is_err());
    exact_budget(&surface);
    surface.restore_checkpoint_fields(&[(id, saved)]).unwrap();
    exact_budget(&surface);
    nodes[0] = field(id, "external", 100, 0.);
    surface.present(surface.revision(), nodes.clone()).unwrap();
    exact_budget(&surface);
    assert!(!surface.field(id).unwrap().can_undo());
    surface
        .present(surface.revision(), vec![nodes[0].clone()])
        .unwrap();
    exact_budget(&surface);
    nodes[0].descriptor.content = NodeContent::Text("static".into());
    surface
        .present(surface.revision(), vec![nodes[0].clone()])
        .unwrap();
    exact_budget(&surface);
    assert_eq!(surface.editor_bytes, 0);
    nodes[0] = field(id, "new", 101, 0.);
    surface
        .present(surface.revision(), vec![nodes[0].clone()])
        .unwrap();
    exact_budget(&surface);
    let before = surface.editor_bytes;
    assert!(surface
        .present(surface.revision(), vec![field(id, "stale", 100, 0.)])
        .is_err());
    assert_eq!(surface.editor_bytes, before);
    surface.lifetime.dispose();
    let area = Rect::new(0, 0, 40, 12);
    (&mut surface).render(area, &mut Buffer::empty(area));
    assert_eq!(surface.editor_bytes, 0);
    assert!(!surface.event_revision_exhausted);
    assert!(surface.fields.is_empty());
}
#[test]
fn checkpoint_event_revisions_follow_unacknowledged_local_events_and_cancel_after_exhaustion() {
    let (mut surface, mut nodes) = editor_surface(1);
    let id = nodes[0].id;
    surface.focus_node(id).unwrap();
    for _ in 0..3 {
        surface.handle(Event::Paste("a".into())).unwrap();
    }
    let saved = String::from_utf8(surface.checkpoint_field(id).unwrap()).unwrap();
    let events = surface
        .restore_checkpoint_fields(&[(id, saved.clone())])
        .unwrap();
    assert!(
        matches!(events[0].payload, EventPayload::Edit { revision: 4, .. }),
        "restored event must follow the last emitted event: {:?}",
        events[0]
    );
    exact_budget(&surface);
    nodes[0] = field(id, "near exhaustion", u64::MAX - 2, 0.);
    surface.present(surface.revision(), nodes.clone()).unwrap();
    let edit = surface.handle(Event::Paste("x".into())).unwrap().unwrap();
    assert!(matches!(edit.payload,EventPayload::Edit{revision,..} if revision==u64::MAX-1));
    let events = surface.restore_checkpoint_fields(&[(id, saved)]).unwrap();
    assert!(matches!(
        events[0].payload,
        EventPayload::Edit {
            revision: u64::MAX,
            ..
        }
    ));
    let before = (
        surface.editor_bytes,
        surface.field(id).unwrap().document().clone(),
        surface.focused(),
    );
    assert!(surface.handle(Event::Paste("rejected".into())).is_err());
    assert!(surface.handle(Event::FocusLost).is_err());
    assert!(surface.active);
    assert_eq!(surface.editor_bytes, before.0);
    assert_eq!(*surface.field(id).unwrap().document(), before.1);
    assert_eq!(surface.focused(), before.2);
    let next = field(NodeId::new(id.root), "fresh", 0, 0.);
    surface
        .present(surface.revision(), vec![next.clone()])
        .unwrap();
    surface.focus_node(next.id).unwrap();
    let edit = surface.handle(Event::Paste("ok".into())).unwrap().unwrap();
    assert!(matches!(
        edit.payload,
        EventPayload::Edit { revision: 1, .. }
    ));
    exact_budget(&surface);
}
#[test]
fn exhausted_toggle_owner_rejects_before_an_unrelated_editor_mutates_then_retirement_recovers() {
    let root = RootId::new();
    let id = NodeId::new(root);
    let toggle = NodeId::new(root);
    let mut checkbox = field(toggle, "", 0, 2.);
    checkbox.descriptor.content = NodeContent::Checkbox {
        label: "last event".into(),
        checked: false,
        enabled: true,
        revision: u64::MAX - 1,
    };
    let node = field(id, "safe", 0, 0.);
    let mut surface = CellSurface::new(RootLifetime::new(root), [1., 1.]).unwrap();
    surface.present(0, vec![node.clone(), checkbox]).unwrap();
    surface.layout(Rect::new(0, 0, 40, 12));
    surface.focus_node(toggle).unwrap();
    let event = surface
        .handle(key(KeyCode::Char(' '), KeyModifiers::NONE))
        .unwrap()
        .unwrap();
    assert!(matches!(
        event.payload,
        EventPayload::Toggle {
            revision: u64::MAX,
            ..
        }
    ));
    surface.focus_node(id).unwrap();
    assert!(surface.handle(Event::Paste("blocked".into())).is_err());
    assert_eq!(surface.field(id).unwrap().document().text(), "safe");
    exact_budget(&surface);
    surface.present(surface.revision(), vec![node]).unwrap();
    let event = surface
        .handle(Event::Paste("accepted".into()))
        .unwrap()
        .unwrap();
    assert!(matches!(
        event.payload,
        EventPayload::Edit { revision: 1, .. }
    ));
    exact_budget(&surface);
}

#[test]
fn multi_field_restore_budget_rejection_keeps_every_document_clock_and_total_unchanged() {
    let (mut surface, nodes) = editor_surface(4);
    let mut editor = TerminalEditor::new("a".repeat(TerminalEditor::MAX_BYTES)).unwrap();
    for _ in 0..8 {
        editor
            .handle(&key(KeyCode::Delete, KeyModifiers::NONE))
            .unwrap();
    }
    let retained = editor.retained_bytes();
    assert!(retained * 3 < CellSurface::EDITOR_BYTES && retained * 4 > CellSurface::EDITOR_BYTES);
    let saved = String::from_utf8(editor.checkpoint_bytes().unwrap()).unwrap();
    let before = surface.editor_bytes;
    let all = nodes
        .iter()
        .map(|node| (node.id, saved.clone()))
        .collect::<Vec<_>>();
    assert!(surface.restore_checkpoint_fields(&all).is_err());
    assert_eq!(surface.editor_bytes, before);
    assert!(surface.event_revisions.is_empty());
    for node in &nodes {
        assert_eq!(surface.field(node.id).unwrap().document().text(), "seed");
    }
    exact_budget(&surface);
    let events = surface.restore_checkpoint_fields(&all[..3]).unwrap();
    assert_eq!(events.len(), 3);
    exact_budget(&surface);
    let before = surface.editor_bytes;
    let clocks = surface.event_revisions.clone();
    assert!(surface.restore_checkpoint_fields(&all[3..]).is_err());
    assert_eq!(surface.editor_bytes, before);
    assert_eq!(surface.event_revisions, clocks);
    assert_eq!(
        surface.field(nodes[3].id).unwrap().document().text(),
        "seed"
    );
    exact_budget(&surface);
}
