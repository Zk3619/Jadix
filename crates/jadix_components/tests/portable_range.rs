#![cfg(not(feature = "gpu"))]
use jadix::{authoring::native::ElementProjection, portable::{create_root, use_signal}, presentation::{*, interaction::*, keyboard::*}};
use jadix_components::prelude::*;

#[test]
fn range_input_uses_local_coordinates_and_toggle_cancel_keeps_selection() {
    let (_owner, (value, selected, pressed, mut surface)) = create_root(|| {
        let value = use_signal("range-test-value", || 0.); let selected = use_signal("range-test-selected", || false); let pressed = use_signal("range-test-pressed", || false);
        let surface = Element::new().child(slider("range", value).range(-5., 5., 3.).unwrap().build())
            .child(progress("progress", value).range(-5., 5.).unwrap().build())
            .child(toggle("toggle", selected).pressed_signal(pressed).text("Bold").build());
        (value, selected, pressed, surface)
    });
    let projection = ElementProjection::new(RootId::new());
    projection.snapshot(&mut surface).unwrap();
    let slider = projection.node_by_template(&surface, "range").unwrap();
    projection.apply_layout(&mut surface, |id| (id == slider).then_some(Rect { x: 400., y: 300., width: 160., height: 32. }));
    let mut event = InteractionEvent { kind: InteractionKind::Press, phase: InteractionPhase::Started, position: [10., 10.], local_position: Some([40., 16.]), timestamp: 1., modifiers: Default::default(), translation: None, velocity: None, value: None, rate: None, touches: vec![], active_touches: vec![] };
    projection.dispatch(&mut surface, &PlatformEvent { node: slider, payload: EventPayload::Interaction(event.clone()) }).unwrap();
    assert_eq!(value.peek(), -2.5);
    projection.dispatch(&mut surface, &PlatformEvent { node: slider, payload: EventPayload::Key(KeyEvent { phase: KeyPhase::Down, key: "End".into(), code: 77, modifiers: Default::default(), repeat: false }) }).unwrap();
    assert_eq!(value.peek(), 5.);
    let node = projection.node_by_template(&surface, "toggle").unwrap();
    projection.dispatch(&mut surface, &PlatformEvent { node, payload: EventPayload::Interaction(event.clone()) }).unwrap();
    assert!(pressed.peek());
    event.phase = InteractionPhase::Cancelled;
    projection.dispatch(&mut surface, &PlatformEvent { node, payload: EventPayload::Interaction(event) }).unwrap();
    assert!(!pressed.peek()); assert!(!selected.peek());
    projection.dispatch(&mut surface, &PlatformEvent { node, payload: EventPayload::Activate }).unwrap();
    assert!(selected.peek());
    projection.snapshot(&mut surface).unwrap();
    let disabled = projection.node_by_template(&surface, "progress").unwrap();
    assert!(projection.dispatch(&mut surface, &PlatformEvent { node: disabled, payload: EventPayload::Activate }).is_err());
}
