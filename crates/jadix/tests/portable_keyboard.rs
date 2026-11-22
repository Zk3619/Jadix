use jadix::portable::{authoring::native::ElementProjection, presentation::*, Element};
use std::{cell::RefCell, rc::Rc};

#[test]
fn portable_keyboard_callbacks_obey_policy_and_root_lifetime() {
    let root = RootId::new();
    let projection = ElementProjection::new(root);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let keys = seen.clone();
    let shortcuts = seen.clone();
    let mut element = Element::new().keyboard(keyboard::KeyboardPolicy {
        focusable: true, key_down: true, key_up: false,
        shortcuts: vec![keyboard::KeyShortcut { id: "save".into(), key: "s".into(), modifiers: Default::default() }],
    }).on_key(move |event| keys.borrow_mut().push(event.key.clone()))
      .on_shortcut(move |id| shortcuts.borrow_mut().push(id.into()));
    let node = projection.snapshot(&mut element).unwrap().id;
    let key = keyboard::KeyEvent { phase: keyboard::KeyPhase::Down, key: "Ж".into(), code: 4,
        modifiers: Default::default(), repeat: false };
    projection.dispatch(&mut element, &PlatformEvent { node, payload: EventPayload::Key(key.clone()) }).unwrap();
    projection.dispatch(&mut element, &PlatformEvent { node, payload: EventPayload::Shortcut { id: "save".into() } }).unwrap();
    assert_eq!(*seen.borrow(), ["Ж", "save"]);
    for payload in [EventPayload::Key(keyboard::KeyEvent { phase: keyboard::KeyPhase::Up, ..key }),
        EventPayload::Shortcut { id: "unknown".into() }] {
        assert!(projection.dispatch(&mut element, &PlatformEvent { node, payload }).is_err());
    }
    element.descriptor.keyboard.focusable = false;
    assert!(projection.dispatch(&mut element, &PlatformEvent { node, payload: EventPayload::Shortcut { id: "save".into() } }).is_err());
    projection.dispose();
    assert!(projection.snapshot(&mut element).is_err());
    assert_eq!(*seen.borrow(), ["Ж", "save"]);
}
