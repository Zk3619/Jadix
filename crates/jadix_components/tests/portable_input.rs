#![cfg(not(feature = "gpu"))]
use jadix::{authoring::native::ElementProjection, presentation::{*, interaction::*}};
use jadix_components::prelude::*;
use std::{cell::RefCell, rc::Rc};

fn press(phase: InteractionPhase) -> InteractionEvent {
    InteractionEvent { kind: InteractionKind::Press, phase, position: [1.0, 2.0], local_position: None, timestamp: 1.0,
        modifiers: Default::default(), translation: None, velocity: None, value: None, rate: None, touches: vec![], active_touches: vec![] }
}

#[test]
fn generic_component_rsx_and_builders_deliver_shared_input_contracts() {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let observed = seen.clone();
    let policy = InteractionPolicy { press: true, ..Default::default() };
    let mut button: Element = button("input-builder").interaction(policy.clone())
        .on_interaction(move |event| observed.borrow_mut().push(event.phase)).into();
    let observed = seen.clone();
    let mut rsx = ui! { <Button id="input-rsx" interaction={policy}
        onInteraction={move |event: &InteractionEvent| observed.borrow_mut().push(event.phase)}
        keyboard={keyboard::KeyboardPolicy { focusable: true, key_down: true, ..Default::default() }}
        onKey={|_: &keyboard::KeyEvent| {}} onShortcut={|_: &str| {}} /> };
    let projection = ElementProjection::new(RootId::new());
    for element in [&mut button, &mut rsx] {
        let node = projection.snapshot(element).unwrap();
        assert!(node.descriptor.interaction.press);
        projection.dispatch(element, &PlatformEvent { node: node.id, payload: EventPayload::Interaction(press(InteractionPhase::Ended)) }).unwrap();
        assert!(projection.dispatch(element, &PlatformEvent { node: node.id, payload: EventPayload::Interaction(press(InteractionPhase::Changed)) }).is_err());
    }
    assert_eq!(*seen.borrow(), [InteractionPhase::Ended, InteractionPhase::Ended]);
    projection.dispose();
    assert!(projection.snapshot(&mut rsx).is_err());
}
