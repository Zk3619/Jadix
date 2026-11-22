#![cfg(not(feature = "gpu"))]
use jadix::{authoring::native::ElementProjection, presentation::*, portable::hover_intent::*};
use jadix_components::prelude::*;

#[test]
fn keyboard_focus_survives_deadlines_and_press_dismissal_does_not_rearm() {
    let mut surface = tooltip("help").open_delay(NEVER_HOVER_DELAY_MS).child(|| {
        Element::new()
            .child(TooltipTrigger(TooltipTriggerProps { children: Some(button("trigger").text("Help").into()), ..Default::default() }))
            .child(tooltip_content(Default::default(), || Element::new().text("Explanation")))
    });
    let projection = ElementProjection::new(RootId::new());
    projection.snapshot(&mut surface).unwrap();
    let node = projection.node_by_template(&surface, "trigger").unwrap();
    let dispatch = |surface: &mut Element, payload| {
        projection.dispatch(surface, &PlatformEvent { node, payload }).unwrap();
        projection.snapshot(surface).unwrap()
    };
    dispatch(&mut surface, EventPayload::FocusVisible);
    assert!(projection.node_by_template(&surface, "help__hover_surface").is_some());
    tick_hover_intent_at(web_time::Instant::now() + std::time::Duration::from_secs(60));
    projection.snapshot(&mut surface).unwrap();
    assert!(projection.node_by_template(&surface, "help__hover_surface").is_some());
    dispatch(&mut surface, EventPayload::Blur);
    assert!(projection.node_by_template(&surface, "help__hover_surface").is_none());
    dispatch(&mut surface, EventPayload::FocusVisible);
    dispatch(&mut surface, EventPayload::Key(keyboard::KeyEvent { phase: keyboard::KeyPhase::Down, key: "Escape".into(), code: 41, modifiers: Default::default(), repeat: false }));
    tick_hover_intent_at(web_time::Instant::now() + std::time::Duration::from_secs(60));
    projection.snapshot(&mut surface).unwrap();
    assert!(projection.node_by_template(&surface, "help__hover_surface").is_none());
}
