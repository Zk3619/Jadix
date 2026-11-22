use jadix_core::{commands::{CommandRegistry, CommandState}, menu::MenuItem,
    presentation::{RootId, RootLifetime}, schema::{Schema, Value}};
use std::{cell::Cell, rc::Rc};

fn select(item: &MenuItem) {
    let MenuItem::Command { on_select, .. } = item else { panic!("expected command") };
    on_select();
}

#[test]
fn stale_menus_obey_live_state_and_cannot_invoke_replacements_or_disposed_roots() {
    let lifetime = RootLifetime::new(RootId::new());
    let registry = CommandRegistry::new(lifetime.clone());
    let enabled = Rc::new(Cell::new(true)); let count = Rc::new(Cell::new(0));
    let state = enabled.clone(); let calls = count.clone();
    registry.register("save", "保存 Save", Schema::Null,
        move || CommandState { enabled: state.get(), ..Default::default() },
        move |_| { calls.set(calls.get() + 1); Ok(()) }).unwrap();
    let menu = registry.menu("save").unwrap();
    select(&menu); assert_eq!(count.get(), 1);
    enabled.set(false); select(&menu); assert_eq!(count.get(), 1);
    assert_eq!(registry.search("保存 SAVE")[0].state.enabled, false);
    registry.unregister("save");
    let calls = count.clone();
    registry.register("save", "Replacement", Schema::Null, CommandState::default,
        move |_| { calls.set(calls.get() + 100); Ok(()) }).unwrap();
    select(&menu); assert_eq!(count.get(), 1);
    let current = registry.menu("save").unwrap();
    lifetime.dispose(); select(&current); assert_eq!(count.get(), 1);
    assert!(registry.search("").is_empty());
    assert!(registry.invoke("save", Value::Null).is_err());
}

#[test]
fn state_and_run_callbacks_can_mutate_the_registry_without_borrowing_or_stale_dispatch() {
    let registry = CommandRegistry::new(RootLifetime::new(RootId::new()));
    let removed = registry.clone();
    registry.register("remove", "Remove", Schema::Null, move || {
        removed.unregister("remove"); CommandState::default()
    }, |_| panic!("removed handler ran")).unwrap();
    assert!(!registry.invoke("remove", Value::Null).unwrap());
    let removed = registry.clone();
    registry.register("search", "Search", Schema::Null, move || {
        removed.unregister("search"); CommandState::default()
    }, |_| panic!("removed handler ran")).unwrap();
    assert!(registry.search("search").is_empty());
    let removed = registry.clone();
    registry.register("menu", "Menu", Schema::Null, move || {
        removed.unregister("menu"); CommandState::default()
    }, |_| panic!("removed handler ran")).unwrap();
    assert!(registry.menu("menu").is_err());
    let removed = registry.clone();
    registry.register("run", "Run", Schema::Null, CommandState::default, move |_| {
        removed.unregister("run"); Ok(())
    }).unwrap();
    assert!(registry.invoke("run", Value::Null).unwrap());
    assert!(!registry.contains("run"));
    registry.register("args", "Arguments", Schema::Bool, CommandState::default, |_| Ok(())).unwrap();
    assert!(!registry.search("args")[0].accepts_null);
    assert!(registry.menu("args").is_err());
    assert!(registry.invoke("args", Value::Null).is_err());
    assert!(registry.invoke("args", Value::Bool(true)).unwrap());
}
