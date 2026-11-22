#![cfg(not(feature = "gpu"))]
use jadix::{authoring::native::ElementProjection, portable::*, presentation::*};
use jadix_components::prelude::{button, error_boundary, ErrorBoundary};
use std::{cell::Cell, rc::Rc};

#[test]
fn native_projection_replaces_event_and_binding_failures_and_retries_cleanly() {
    let builds = Rc::new(Cell::new(0)); let built = builds.clone();
    let (_root, (state, fail, mut element)) = create_root(|| {
        let state = ErrorBoundary::new(); let fail = create_signal(false);
        let element = error_boundary("boundary", state.clone(), |message, state| {
            Element::new().child(Element::new().id("error").text(message))
                .child(button("retry").text("Retry").on_click(move || state.reset()))
        }, move || {
            built.set(built.get() + 1);
            Element::new().child(app::reactive::bind_text("healthy", move || { assert!(!fail.get(), "binding failed"); "healthy".into() }))
                .child(button("fail").text("Fail").on_click(|| panic!("event failed")))
        });
        (state, fail, element)
    });
    let projection = ElementProjection::new(RootId::new());
    projection.snapshot(&mut element).unwrap();
    let old = projection.node_by_template(&element, "fail").unwrap();
    projection.dispatch(&mut element, &PlatformEvent { node: old, payload: EventPayload::Activate }).unwrap();
    projection.snapshot(&mut element).unwrap();
    assert_eq!(state.error().as_deref(), Some("event failed"));
    assert!(projection.node_by_template(&element, "error").is_some());
    assert!(projection.dispatch(&mut element, &PlatformEvent { node: old, payload: EventPayload::Activate }).is_err());
    let retry = projection.node_by_template(&element, "retry").unwrap();
    projection.dispatch(&mut element, &PlatformEvent { node: retry, payload: EventPayload::Activate }).unwrap();
    projection.snapshot(&mut element).unwrap();
    assert!(state.error().is_none()); assert_eq!(builds.get(), 2);
    fail.set(true);
    // Binding failure is resolved within this snapshot, not one native frame later.
    projection.snapshot(&mut element).unwrap();
    assert_eq!(state.error().as_deref(), Some("binding failed"));
    assert!(projection.node_by_template(&element, "error").is_some());
}
