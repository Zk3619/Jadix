#![cfg(all(not(feature = "gpu"), feature = "macros"))]
use jadix::{authoring::native::ElementProjection, presentation::*};
use jadix_components::prelude::*;
use std::{cell::Cell, rc::Rc};

fn nodes(root: &PresentationNode) -> Vec<&PresentationNode> {
    let mut result = vec![root]; let mut i = 0;
    while i < result.len() { result.extend(&result[i].children); i += 1; }
    result
}
fn named(root: &PresentationNode, name: &str) -> NodeId {
    nodes(root).into_iter().find(|node| node.descriptor.accessible_name.as_deref() == Some(name)).unwrap().id
}

#[test]
fn keyed_table_bounds_both_axes_retains_editors_and_routes_nested_controls() {
    let clicks = Rc::new(Cell::new(0)); let nested = clicks.clone();
    let (_owner, (mut element, state, draft)) = create_root(|| {
        let state = TableState::new((0..10000).map(|i| format!("r{i:05}")).collect(),
            (0..100).map(|i| TableColumn::new(format!("c{i}"), format!("C{i}")).width(100.)
                .pin(if i == 0 { ColumnPin::Left } else if i == 99 { ColumnPin::Right } else { ColumnPin::None })).collect()).unwrap();
        let draft = create_signal("draft".to_string());
        let element = table("grid", state.clone()).h(160.).cell(move |row, column| {
            if row == "r00000" && column == "c1" { input("editor").value_signal(draft).into() }
            else if row == "r00000" && column == "c0" {
                let clicks = nested.clone(); button("nested").text("Action").on_click(move || clicks.set(clicks.get() + 1)).into()
            } else { Element::new().text(format!("{row}/{column}")) }
        }).build();
        (element, state, draft)
    });
    let projection = ElementProjection::new(RootId::new());
    let first = projection.snapshot(&mut element).unwrap();
    assert!(nodes(&first).len() < 800);
    let editor = nodes(&first).into_iter().find(|n| matches!(n.descriptor.content, NodeContent::Input { .. })).unwrap().id;
    let action = nodes(&first).into_iter().find(|n| matches!(&n.descriptor.content, NodeContent::Button {label, ..} if label == "Action")).unwrap().id;
    projection.dispatch(&mut element, &PlatformEvent {node: action, payload: EventPayload::Activate}).unwrap();
    assert_eq!(clicks.get(), 1);
    assert!(state.read(|rows| rows.selected().is_empty()));
    draft.set("edited".into());
    state.update(|rows| rows.query(|_| true, |a,b| b.cmp(a), None)).unwrap();
    projection.snapshot(&mut element).unwrap();
    state.update(|rows| rows.query(|_| true, |a,b| a.cmp(b), None)).unwrap();
    state.reveal("r00000", "c1").unwrap();
    let restored = projection.snapshot(&mut element).unwrap();
    assert!(nodes(&restored).into_iter().any(|n| matches!(&n.descriptor.content, NodeContent::Input {value, ..} if value == "edited")));
    // A column resize and reorder keeps the mounted editor's identity.
    state.set_columns((0..100).map(|i| TableColumn::new(format!("c{i}"), format!("C{i}")).width(if i == 1 { 110. } else {100.})
        .pin(if i == 0 { ColumnPin::Left } else if i == 99 { ColumnPin::Right } else { ColumnPin::None })).collect()).unwrap();
    let resized = projection.snapshot(&mut element).unwrap();
    let resized_editor = nodes(&resized).into_iter().find(|n| matches!(n.descriptor.content, NodeContent::Input {..})).unwrap().id;
    let restored_editor = nodes(&restored).into_iter().find(|n| matches!(n.descriptor.content, NodeContent::Input {..})).unwrap().id;
    assert_eq!(resized_editor, restored_editor);
    let _ = editor; // Scrolled-out editors are remounted from the caller's keyed draft store.
    state.reveal("r09999", "c98").unwrap();
    let last = projection.snapshot(&mut element).unwrap();
    assert!(nodes(&last).len() < 800);
    assert!(state.horizontal_offset() > 9000.);
    let cell = named(&last, "C98: r09999");
    projection.dispatch(&mut element, &PlatformEvent {node: cell,
        payload: EventPayload::Navigate {key: NavigationKey::Left, shift: false, control: false}}).unwrap();
    let after = projection.snapshot(&mut element).unwrap();
    assert_eq!(projection.take_focus_request(&element), Some(named(&after, "C97: r09999")));
}

#[test]
fn thousands_of_pinned_columns_are_virtualized_and_keyboard_revealable() {
    let (_owner, (mut element, state)) = create_root(|| {
        let state = TableState::new(vec!["row".into()], (0..3000).map(|i|
            TableColumn::new(format!("c{i}"), format!("C{i}")).width(100.)
                .pin(if i < 1000 { ColumnPin::Left } else if i < 2000 { ColumnPin::None } else { ColumnPin::Right })).collect()).unwrap();
        (table("pins", state.clone()).h(96.).build(), state)
    });
    let projection = ElementProjection::new(RootId::new());
    let first = projection.snapshot(&mut element).unwrap();
    assert!(nodes(&first).len() < 180);
    state.reveal("row", "c2999").unwrap();
    let last = projection.snapshot(&mut element).unwrap();
    assert!(nodes(&last).len() < 180);
    let last_cell = named(&last, "C2999: row");
    projection.dispatch(&mut element, &PlatformEvent {node: last_cell,
        payload: EventPayload::Navigate { key: NavigationKey::Home, shift: false, control: false }}).unwrap();
    let home = projection.snapshot(&mut element).unwrap();
    assert_eq!(projection.take_focus_request(&element), Some(named(&home, "C0: row")));
}
