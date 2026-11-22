#![cfg(not(feature = "gpu"))]
use jadix::{authoring::native::ElementProjection, portable::*, presentation::*};
use jadix_components::prelude::{dock, input, Dock, DockBounds, DockEdge, DockLimits, DockPane, DockState, DropLocation};
use std::{cell::Cell, rc::Rc};

#[test]
fn moving_and_undoing_panes_preserves_composition_identity_and_content_scopes() {
    let builds = Rc::new(Cell::new(0)); let cleanups = Rc::new(Cell::new(0));
    let built = builds.clone(); let cleaned = cleanups.clone();
    let (_owner, (state, mut element)) = create_root(|| {
        let model = Dock::new((1..=3).map(|key| DockPane {key, title: format!("Pane {key}"), closable: key != 1}).collect(),
            DockBounds {origin: [0., 0.], size: [960., 640.]}, DockLimits::default()).unwrap();
        let state = DockState::new(model);
        let element = dock("work", state.clone()).pane(move |key| {
            built.set(built.get() + 1);
            let cleaned = cleaned.clone(); on_cleanup(move || cleaned.set(cleaned.get() + 1));
            input(&format!("draft_{key}")).into()
        }).build();
        (state, element)
    });
    let projection = ElementProjection::new(RootId::new());
    projection.snapshot(&mut element).unwrap();
    let input = projection.node_by_template(&element, "draft_1").unwrap();
    projection.dispatch(&mut element, &PlatformEvent {node: input, payload: EventPayload::Edit {
        value: "ni".into(), revision: 1, selection_utf16: [2, 2], composing_utf16: Some([0, 2]),
    }}).unwrap();
    state.update(|model| model.relocate(2, DropLocation::Split {stack: 1, edge: DockEdge::Right})).unwrap();
    projection.snapshot(&mut element).unwrap();
    assert_eq!(projection.node_by_template(&element, "draft_1"), Some(input));
    assert_eq!(builds.get(), 3); assert_eq!(cleanups.get(), 0);
    projection.dispatch(&mut element, &PlatformEvent {node: input, payload: EventPayload::Edit {
        value: "你好😀".into(), revision: 2, selection_utf16: [4, 4], composing_utf16: None,
    }}).unwrap();
    state.update(|model| model.undo()).unwrap(); projection.snapshot(&mut element).unwrap();
    assert_eq!(projection.node_by_template(&element, "draft_1"), Some(input));
    assert_eq!(builds.get(), 3); assert_eq!(cleanups.get(), 0);
    state.update(|model| model.activate(3)).unwrap();
    projection.snapshot(&mut element).unwrap();
    let closed = projection.node_by_template(&element, "draft_3").unwrap();
    state.update(|model| model.close(3)).unwrap(); projection.snapshot(&mut element).unwrap();
    assert_eq!(cleanups.get(), 1);
    assert!(projection.node_by_template(&element, "draft_3").is_none());
    assert!(projection.dispatch(&mut element, &PlatformEvent {node: closed, payload: EventPayload::Focus}).is_err());
    drop(element); assert_eq!(cleanups.get(), 3);
}

#[test]
fn cancelling_or_unmounting_a_tab_releases_the_shared_drag() {
    use jadix::presentation::interaction::*;
    let (_owner, (state, mut element)) = create_root(|| {
        let state = DockState::new(Dock::new((1..=2).map(|key| DockPane {key, title: key.to_string(), closable: true}).collect(),
            DockBounds {origin: [0., 0.], size: [800., 600.]}, DockLimits::default()).unwrap());
        (state.clone(), dock("work", state).build())
    });
    let projection = ElementProjection::new(RootId::new()); projection.snapshot(&mut element).unwrap();
    let tab = projection.node_by_template(&element, "work__tab_1").unwrap();
    let pan = InteractionEvent {kind: InteractionKind::Pan, phase: InteractionPhase::Started,
        position: [100., 20.], local_position: Some([30., 20.]), translation: Some([0., 0.]), velocity: Some([0., 0.]),
        timestamp: 1., modifiers: Default::default(), value: None, rate: None, touches: vec![], active_touches: vec![]};
    projection.dispatch(&mut element, &PlatformEvent {node: tab, payload: EventPayload::Interaction(pan.clone())}).unwrap();
    assert_eq!(state.read(Dock::dragging), Some(1));
    projection.dispatch(&mut element, &PlatformEvent {node: tab, payload: EventPayload::Navigate {
        key: NavigationKey::Escape, shift: false, control: false,
    }}).unwrap();
    assert_eq!(state.read(Dock::dragging), None);
    projection.dispatch(&mut element, &PlatformEvent {node: tab, payload: EventPayload::Interaction(pan)}).unwrap();
    drop(element); assert_eq!(state.read(Dock::dragging), None);
}
