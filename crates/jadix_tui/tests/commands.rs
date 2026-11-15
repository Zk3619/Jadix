use jadix_core::{
    commands::*,
    menu::{MenuItem, MenuRole, MenuState},
    presentation::{RootId, RootLifetime},
    schema::{Schema, Value},
};
use jadix_tui::{crossterm::event::*, CommandBinding, CommandContext, CommandInput, CommandRouter};
use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};
fn key(c: char) -> Event {
    Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL))
}
#[test]
fn commands_are_general_scoped_owner_bound_and_chords_cancel_on_context_changes() {
    let life = RootLifetime::new(RootId::new());
    let registry = CommandRegistry::new(life.clone());
    let count = Rc::new(Cell::new(0));
    let enabled = Rc::new(Cell::new(true));
    registry
        .register(
            "save",
            "Save",
            Schema::Null,
            {
                let enabled = enabled.clone();
                move || CommandState {
                    enabled: enabled.get(),
                    menu: MenuState::Checked(true),
                }
            },
            {
                let count = count.clone();
                move |_| {
                    count.set(count.get() + 1);
                    Ok(())
                }
            },
        )
        .unwrap();
    let stale = registry.menu("save").unwrap();
    let mut router = CommandRouter::new(registry.clone());
    router
        .set_bindings(vec![CommandBinding {
            command: "save".into(),
            chord: "Ctrl+K Ctrl+S".into(),
            scope: None,
            in_editors: false,
        }])
        .unwrap();
    let now = Instant::now();
    let context = CommandContext::default();
    assert_eq!(
        router.handle_at(&key('k'), context.clone(), now).unwrap(),
        CommandInput::Pending
    );
    assert_eq!(
        router.handle_at(&key('s'), context.clone(), now).unwrap(),
        CommandInput::Invoked {
            command: "save".into(),
            executed: true
        }
    );
    assert_eq!(count.get(), 1);
    router.handle_at(&key('k'), context.clone(), now).unwrap();
    assert_eq!(
        router
            .handle_at(&key('s'), context.clone(), now + Duration::from_secs(2))
            .unwrap(),
        CommandInput::Pass
    );
    router.handle_at(&key('k'), context.clone(), now).unwrap();
    assert_eq!(
        router
            .handle_at(
                &key('s'),
                CommandContext {
                    in_editor: true,
                    ..context.clone()
                },
                now
            )
            .unwrap(),
        CommandInput::Pass
    );
    router.handle_at(&key('k'), context.clone(), now).unwrap();
    assert_eq!(
        router
            .handle_at(&Event::FocusLost, context.clone(), now)
            .unwrap(),
        CommandInput::Pass
    );
    assert_eq!(
        router.handle_at(&key('s'), context.clone(), now).unwrap(),
        CommandInput::Pass
    );
    enabled.set(false);
    if let MenuItem::Command { on_select, .. } = stale {
        on_select();
    }
    assert_eq!(count.get(), 1);
    life.dispose();
    assert!(registry.invoke("save", Value::Null).is_err());
}
#[test]
fn menu_radio_validation_and_prefix_ambiguity_fail_closed() {
    let items = vec![
        MenuItem::command("a", "A", || {}).radio("theme", true),
        MenuItem::command("b", "B", || {}).radio("theme", true),
    ];
    assert!(jadix_core::menu::validate(&items).is_err());
    assert!(jadix_core::menu::validate(&[
        MenuItem::command("copy", "Copy", || {}).role(MenuRole::Copy),
        MenuItem::command("check", "Check", || {}).checked(false)
    ])
    .is_ok());
    let registry = CommandRegistry::new(RootLifetime::new(RootId::new()));
    registry
        .register("x", "X", Schema::Null, Default::default, |_| Ok(()))
        .unwrap();
    let mut router = CommandRouter::new(registry);
    assert!(router
        .set_bindings(vec![
            CommandBinding {
                command: "x".into(),
                chord: "Ctrl+K".into(),
                scope: None,
                in_editors: false
            },
            CommandBinding {
                command: "x".into(),
                chord: "Ctrl+K Ctrl+S".into(),
                scope: None,
                in_editors: false
            }
        ])
        .is_err());
}
#[cfg(feature = "portable")]
#[test]
fn application_routes_commands_without_a_button_and_refreshes_reactive_text() {
    use jadix::portable::{Element, Signal};
    use jadix_tui::{
        ratatui::{buffer::Buffer, layout::Rect, widgets::Widget},
        CellApplication,
    };
    let count = Rc::new(Cell::new(0));
    let text = std::rc::Rc::new(std::cell::RefCell::new(None));
    let mut app = CellApplication::new([8., 16.], {
        let text = text.clone();
        move || {
            let signal = Signal::new("Initial".to_string());
            *text.borrow_mut() = Some(signal);
            Element::new().child(jadix::portable::app::reactive::bind_text(
                "command-value",
                move || signal.get(),
            ))
        }
    })
    .unwrap();
    let registry = app.command_registry();
    registry
        .register("save", "Save", Schema::Null, Default::default, {
            let text = text.clone();
            let count = count.clone();
            move |_| {
                count.set(count.get() + 1);
                text.borrow().as_ref().unwrap().set("Saved".into());
                Ok(())
            }
        })
        .unwrap();
    let mut router = CommandRouter::new(registry.clone());
    router
        .set_bindings(vec![CommandBinding {
            command: "save".into(),
            chord: "Ctrl+K Ctrl+S".into(),
            scope: None,
            in_editors: false,
        }])
        .unwrap();
    app.set_command_router(router);
    let area = Rect::new(0, 0, 40, 8);
    app.present(area, 0).unwrap();
    app.handle(key('k')).unwrap();
    app.handle(key('s')).unwrap();
    assert_eq!(count.get(), 1);
    let mut buffer = Buffer::empty(area);
    (&mut app).render(area, &mut buffer);
    assert!(buffer
        .content
        .iter()
        .map(|c| c.symbol())
        .collect::<String>()
        .contains("Saved"));
    app.dispose();
    assert!(registry.invoke("save", Value::Null).is_err());
}
#[test]
fn disjoint_command_scopes_can_reuse_the_same_chord() {
    let registry = CommandRegistry::new(RootLifetime::new(RootId::new()));
    let count = Rc::new(Cell::new(0));
    for (id, increment) in [("left", 1), ("right", 10)] {
        let count = count.clone();
        registry
            .register(id, id, Schema::Null, Default::default, move |_| {
                count.set(count.get() + increment);
                Ok(())
            })
            .unwrap();
    }
    let mut router = CommandRouter::new(registry);
    router
        .set_bindings(
            ["left", "right"]
                .map(|id| CommandBinding {
                    command: id.into(),
                    chord: "Ctrl+K Ctrl+S".into(),
                    scope: Some(id.into()),
                    in_editors: false,
                })
                .to_vec(),
        )
        .unwrap();
    for scope in ["left", "right"] {
        let context = CommandContext {
            scope: Some(scope.into()),
            ..Default::default()
        };
        router.handle(&key('k'), context.clone()).unwrap();
        router.handle(&key('s'), context).unwrap();
    }
    assert_eq!(count.get(), 11);
}
