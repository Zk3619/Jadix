#![cfg(not(feature = "gpu"))]
use jadix::{authoring::native::ElementProjection, portable::*, presentation::*};
use jadix_components::prelude::*;
use jadix::presentation::{interaction::*, keyboard::*};

#[test]
fn split_handles_clamp_keyboard_and_accessibility_updates_without_recreating_content() {
    let (_owner, (sizes, mut element)) = create_root(|| {
        let sizes = create_signal(vec![50., 50.]);
        let element = resizable("split", sizes).limits(vec![(20., 80.), (30., 70.)])
            .children(vec![input("draft").into(), Element::new()]).unwrap();
        (sizes, element)
    });
    let projection = ElementProjection::new(RootId::new());
    let tree = projection.snapshot(&mut element).unwrap();
    let handle = projection.node_by_template(&element, "split__handle_0").unwrap();
    let draft = projection.node_by_template(&element, "draft").unwrap();
    let key = |name: &str| EventPayload::Key(KeyEvent { phase: KeyPhase::Down, key: name.into(), code: 79,
        modifiers: Default::default(), repeat: false });
    projection.dispatch(&mut element, &PlatformEvent { node: handle, payload: key("End") }).unwrap();
    assert_eq!(sizes.peek(), [70., 30.]);
    projection.dispatch(&mut element, &PlatformEvent { node: handle, payload: key("Home") }).unwrap();
    assert_eq!(sizes.peek(), [30., 70.]);
    projection.dispatch(&mut element, &PlatformEvent { node: handle, payload: EventPayload::AccessibilityAction {
        id: "setValue".into(), value: Some(55.),
    }}).unwrap();
    assert_eq!(sizes.peek(), [55., 45.]);
    projection.apply_layout(&mut element, |id| (id == tree.id).then_some(Rect {x: 0., y: 0., width: 212., height: 100.}));
    let pan = |phase, x| EventPayload::Interaction(InteractionEvent {kind: InteractionKind::Pan, phase,
        position: [0., 0.], local_position: None, timestamp: 1., modifiers: Default::default(),
        translation: Some([x, 0.]), velocity: Some([0., 0.]), value: None, rate: None,
        touches: vec![], active_touches: vec![]});
    projection.dispatch(&mut element, &PlatformEvent {node: handle, payload: pan(InteractionPhase::Started, 0.)}).unwrap();
    projection.dispatch(&mut element, &PlatformEvent {node: handle, payload: pan(InteractionPhase::Changed, 20.)}).unwrap();
    assert_eq!(sizes.peek(), [65., 35.]);
    projection.dispatch(&mut element, &PlatformEvent {node: handle, payload: pan(InteractionPhase::Cancelled, 20.)}).unwrap();
    assert_eq!(sizes.peek(), [55., 45.]);
    projection.snapshot(&mut element).unwrap();
    assert_eq!(projection.node_by_template(&element, "draft"), Some(draft));
}

#[test]
fn toolbar_skips_disabled_and_hidden_descendants_and_preserves_editor_keys() {
    let (_owner, (hidden, disabled, mut element)) = create_root(|| {
        let hidden = create_signal(false); let disabled = create_signal(true);
        let mut middle: Element = button("middle").text("Middle").into();
        middle.descriptor_binding = Some(std::rc::Rc::new(move |node| {
            if let NodeContent::Button {enabled, ..} = &mut node.content { *enabled = !disabled.get(); }
        }));
        let mut last: Element = button("last").text("Last").into();
        last.visibility = Some(std::rc::Rc::new(move || !hidden.get()));
        let element = toolbar("tools").children(vec![button("first").text("First").into(), middle,
            Element::new().child(last), input("editor").into()]);
        (hidden, disabled, element)
    });
    let projection = ElementProjection::new(RootId::new()); projection.snapshot(&mut element).unwrap();
    let first = projection.node_by_template(&element, "first").unwrap();
    let last = projection.node_by_template(&element, "last").unwrap();
    let navigate = |key| EventPayload::Navigate {key, shift: false, control: false};
    projection.dispatch(&mut element, &PlatformEvent {node: first, payload: navigate(NavigationKey::Right)}).unwrap();
    assert_eq!(projection.take_focus_request(&element), Some(last));
    hidden.set(true); disabled.set(false); projection.snapshot(&mut element).unwrap();
    let middle = projection.node_by_template(&element, "middle").unwrap();
    projection.dispatch(&mut element, &PlatformEvent {node: first, payload: navigate(NavigationKey::End)}).unwrap();
    assert_eq!(projection.take_focus_request(&element), Some(middle));
    let editor = projection.node_by_template(&element, "editor").unwrap();
    assert!(projection.dispatch(&mut element, &PlatformEvent {node: editor, payload: navigate(NavigationKey::Right)}).is_err());
    assert!(projection.take_focus_request(&element).is_none());
}

#[test]
fn split_rtl_drag_retains_original_transaction_and_rolls_back_on_unmount() {
    let (_owner, (sizes, mut element, scope)) = create_root(|| {
        let sizes = create_signal(vec![50., 50.]);
        let (scope, element) = create_scope(|| resizable("rtl", sizes).rtl(true)
            .children(vec![Element::new(), Element::new()]).unwrap());
        (sizes, element, scope)
    });
    let projection = ElementProjection::new(RootId::new());
    let tree = projection.snapshot(&mut element).unwrap();
    assert_eq!(tree.descriptor.style.direction, Some(jadix::environment::LayoutDirection::RightToLeft));
    let handle = projection.node_by_template(&element, "rtl__handle_0").unwrap();
    projection.apply_layout(&mut element, |id| (id == tree.id).then_some(Rect {x: 0., y: 0., width: 212., height: 100.}));
    let pan = |phase, x| EventPayload::Interaction(InteractionEvent {kind: InteractionKind::Pan, phase,
        position: [0., 0.], local_position: None, timestamp: 1., modifiers: Default::default(),
        translation: Some([x, 0.]), velocity: Some([0., 0.]), value: None, rate: None,
        touches: vec![], active_touches: vec![]});
    for (phase, x) in [(InteractionPhase::Started, 0.), (InteractionPhase::Changed, 20.),
                       (InteractionPhase::Started, 0.), (InteractionPhase::Changed, 40.)] {
        projection.dispatch(&mut element, &PlatformEvent {node: handle, payload: pan(phase, x)}).unwrap();
    }
    assert_eq!(sizes.peek(), [30., 70.]);
    scope.dispose();
    assert_eq!(sizes.peek(), [50., 50.]);
}

#[test]
fn toolbar_navigation_tracks_keyed_insertions_reorders_and_nested_groups() {
    let (_owner, (keys, mut element)) = create_root(|| {
        let keys = create_signal(vec!["a".to_owned()]);
        let dynamic = Element::new().bind_keyed_children(move || keys.get(), |key, _| {
            let mut item = Element::new().id(key);
            item.descriptor.content = NodeContent::Checkbox { label: "Option".into(), checked: false, enabled: true, revision: 0 };
            item
        });
        let nested = toolbar("nested").vertical(true).children(vec![button("inner1").into(), button("inner2").into()]);
        (keys, toolbar("outer").children(vec![button("first").into(), dynamic, nested, button("last").into()]))
    });
    let projection = ElementProjection::new(RootId::new());
    let navigate = |key| EventPayload::Navigate {key, shift: false, control: false};
    projection.snapshot(&mut element).unwrap();
    let a = projection.node_by_template(&element, "a").unwrap();
    keys.set(vec!["b".into(), "a".into()]);
    let tree = projection.snapshot(&mut element).unwrap();
    let b = projection.node_by_template(&element, "b").unwrap();
    assert_eq!(projection.node_by_template(&element, "a"), Some(a));
    assert!(tree.children[1].children[0].descriptor.navigation_keys.contains(&NavigationKey::Right));
    projection.dispatch(&mut element, &PlatformEvent {node: b, payload: navigate(NavigationKey::Right)}).unwrap();
    assert_eq!(projection.take_focus_request(&element), Some(a));
    let last = projection.node_by_template(&element, "last").unwrap();
    projection.dispatch(&mut element, &PlatformEvent {node: a, payload: navigate(NavigationKey::Right)}).unwrap();
    assert_eq!(projection.take_focus_request(&element), Some(last));
    let inner1 = projection.node_by_template(&element, "inner1").unwrap();
    let inner2 = projection.node_by_template(&element, "inner2").unwrap();
    projection.dispatch(&mut element, &PlatformEvent {node: inner1, payload: navigate(NavigationKey::Down)}).unwrap();
    assert_eq!(projection.take_focus_request(&element), Some(inner2));
    assert!(projection.dispatch(&mut element, &PlatformEvent {node: inner1, payload: navigate(NavigationKey::Right)}).is_err());
    keys.set(vec!["b".into()]); projection.snapshot(&mut element).unwrap();
    assert!(projection.dispatch(&mut element, &PlatformEvent {node: a, payload: navigate(NavigationKey::Right)}).is_err());
}
