#![cfg(not(feature = "gpu"))]
use jadix::{authoring::native::ElementProjection, portable::*, presentation::*};
use jadix_components::prelude::*;
use std::{cell::Cell, rc::Rc};

#[test]
fn palette_filters_live_commands_skips_disabled_and_clears_query_before_closing() {
    let calls = Rc::new(Cell::new(0)); let called = calls.clone();
    let lifetime = RootLifetime::new(RootId::new());
    let registry = commands::CommandRegistry::new(lifetime);
    registry.register("disabled", "Disabled", schema::Schema::Null,
        || commands::CommandState { enabled: false, ..Default::default() }, |_| panic!("disabled")).unwrap();
    registry.register("save", "保存 Save", schema::Schema::Null, commands::CommandState::default,
        move |_| { called.set(called.get() + 1); Ok(()) }).unwrap();
    let (_owner, (open, query, mut element)) = create_root(|| {
        let open = use_signal("palette-open", || true); let query = use_signal("palette-query", String::new);
        (open, query, command_palette("palette", registry.clone()).open_signal(open).query_signal(query).build())
    });
    let projection = ElementProjection::new(RootId::new());
    projection.snapshot(&mut element).unwrap();
    let search = projection.node_by_template(&element, "palette__search").unwrap();
    let nav = |key| EventPayload::Navigate { key, shift: false, control: false };
    projection.dispatch(&mut element, &PlatformEvent { node: search, payload: nav(NavigationKey::Down) }).unwrap();
    let save = projection.node_by_template(&element, "palette__command_save").unwrap();
    projection.dispatch(&mut element, &PlatformEvent { node: save, payload: nav(NavigationKey::Enter) }).unwrap();
    assert_eq!(calls.get(), 1); assert!(!open.peek());
    open.set(true); query.set("不存在".into());
    projection.snapshot(&mut element).unwrap();
    assert!(projection.node_by_template(&element, "palette__command_save").is_none());
    projection.dispatch(&mut element, &PlatformEvent { node: search, payload: nav(NavigationKey::Escape) }).unwrap();
    assert!(query.peek().is_empty()); assert!(open.peek());
    projection.snapshot(&mut element).unwrap();
    let restored = projection.node_by_template(&element, "palette__command_save").unwrap();
    assert_ne!(restored, save);
    assert!(projection.dispatch(&mut element, &PlatformEvent { node: save, payload: EventPayload::Activate }).is_err());
    registry.unregister("save"); projection.snapshot(&mut element).unwrap();
    assert!(projection.node_by_template(&element, "palette__command_save").is_none());
    projection.dispatch(&mut element, &PlatformEvent { node: search, payload: nav(NavigationKey::Escape) }).unwrap();
    assert!(!open.peek());
}
