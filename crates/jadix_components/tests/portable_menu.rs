#![cfg(not(feature = "gpu"))]
use jadix::{authoring::native::ElementProjection, presentation::*};
use jadix_components::prelude::*;
use std::{cell::RefCell, rc::Rc};

fn item(id: &str, children: Vec<HostedMenuItem>) -> HostedMenuItem {
    serde_json::from_value(serde_json::json!({"id":id,"label":id,"enabled":true,"checked":false,"children":children})).unwrap()
}
fn dispatch(projection: &ElementProjection, menu: &mut Element, name: &str, payload: EventPayload) {
    let node = projection.node_by_template(menu, name).unwrap();
    projection.dispatch(menu, &PlatformEvent { node, payload }).unwrap();
    projection.snapshot(menu).unwrap();
}

#[test]
fn portable_menu_keeps_trigger_handlers_navigates_submenus_and_restores_focus() {
    let selected = Rc::new(RefCell::new(Vec::new())); let observed = selected.clone();
    let clicked = Rc::new(RefCell::new(0)); let seen = clicked.clone();
    let mut menu = dropdown_menu("actions", vec![item("parent", vec![item("child", vec![])]), item("last", vec![])])
        .on_select(move |id| observed.borrow_mut().push(id.to_owned()))
        .child(button("trigger").text("Actions").on_click(move || *seen.borrow_mut() += 1));
    let projection = ElementProjection::new(RootId::new());
    projection.snapshot(&mut menu).unwrap();
    dispatch(&projection, &mut menu, "trigger", EventPayload::Activate);
    assert_eq!(*clicked.borrow(), 1);
    let parent = projection.node_by_template(&menu, "actions__menu_item_parent").unwrap();
    assert_eq!(projection.take_focus_request(&menu), Some(parent));
    dispatch(&projection, &mut menu, "actions__menu_item_parent", EventPayload::Navigate { key: NavigationKey::Right, shift: false, control: false });
    let child = projection.node_by_template(&menu, "actions__menu_item_child").unwrap();
    assert_eq!(projection.take_focus_request(&menu), Some(child));
    dispatch(&projection, &mut menu, "actions__menu_item_child", EventPayload::Navigate { key: NavigationKey::Left, shift: false, control: false });
    assert_eq!(projection.take_focus_request(&menu), projection.node_by_template(&menu, "actions__menu_item_parent"));
    dispatch(&projection, &mut menu, "actions__menu_item_parent", EventPayload::Activate);
    dispatch(&projection, &mut menu, "actions__menu_item_child", EventPayload::Activate);
    assert_eq!(*selected.borrow(), ["child"]);
    assert!(projection.node_by_template(&menu, "actions__menu_item_child").is_none());
    assert_eq!(projection.take_focus_request(&menu), projection.node_by_template(&menu, "trigger"));
    dispatch(&projection, &mut menu, "trigger", EventPayload::Activate);
    assert_eq!(projection.take_focus_request(&menu), projection.node_by_template(&menu, "actions__menu_item_parent"));
}

#[test]
fn a_container_trigger_opens_on_completed_press_and_preserves_its_handler() {
    use jadix::presentation::interaction::*;
    let clicks = Rc::new(RefCell::new(0)); let observed = clicks.clone();
    let mut menu = dropdown_menu("container", vec![item("first", vec![])])
        .child(Element::new().id("trigger").text("Actions").on_click(move || *observed.borrow_mut() += 1));
    let projection = ElementProjection::new(RootId::new());
    projection.snapshot(&mut menu).unwrap();
    let event = |phase| EventPayload::Interaction(InteractionEvent {
        kind: InteractionKind::Press, phase, position: [0.0; 2], local_position: None, timestamp: 1.0,
        modifiers: Default::default(), translation: None, velocity: None,
        value: None, rate: None, touches: vec![], active_touches: vec![],
    });
    dispatch(&projection, &mut menu, "trigger", event(InteractionPhase::Cancelled));
    assert_eq!(*clicks.borrow(), 0);
    dispatch(&projection, &mut menu, "trigger", event(InteractionPhase::Ended));
    assert_eq!(*clicks.borrow(), 1);
    assert!(projection.node_by_template(&menu, "container__menu_item_first").is_some());
}
