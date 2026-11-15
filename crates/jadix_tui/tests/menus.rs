use jadix_tui::{
    crossterm::event::*,
    ratatui::{buffer::Buffer, layout::Rect},
    MenuInput, MenuItem, TerminalMenu,
};
use std::{cell::Cell, rc::Rc};
fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}
fn run(menu: &mut TerminalMenu, event: Event) -> Option<String> {
    match menu.handle(&event, true, false) {
        MenuInput::Command { id, run } => {
            run();
            Some(id)
        }
        _ => None,
    }
}

#[test]
fn locale_changes_refresh_both_menu_surfaces_and_keep_command_identity() {
    use jadix_core::menu::MenuRole;
    let called = Rc::new(Cell::new(0));
    let count = called.clone();
    let mut menu = TerminalMenu::default();
    menu.set_locale("zh-TW");
    menu.set_bar(vec![MenuItem::submenu(
        "Edit",
        vec![
            MenuItem::localized_role("copy", MenuRole::Copy, move || count.set(count.get() + 1))
                .accelerator("Ctrl+C"),
            MenuItem::command("custom", "Custom", || {}).role(MenuRole::Paste),
        ],
    )])
    .unwrap();
    menu.set_context(vec![MenuItem::localized_role(
        "undo",
        MenuRole::Undo,
        || {},
    )])
    .unwrap();
    let area = Rect::new(0, 0, 40, 10);
    let text = |menu: &mut TerminalMenu| {
        let mut buffer = Buffer::empty(area);
        menu.render(area, &mut buffer);
        buffer
            .content
            .iter()
            .map(|cell| cell.symbol())
            .filter(|symbol| *symbol != " ")
            .collect::<String>()
    };
    run(&mut menu, key(KeyCode::F(10)));
    assert!(text(&mut menu).contains("複製"));
    assert!(text(&mut menu).contains("Custom"));
    menu.set_locale("ja-JP");
    assert!(!menu.is_open());
    run(&mut menu, key(KeyCode::F(10)));
    assert!(text(&mut menu).contains("コピー"));
    run(&mut menu, key(KeyCode::Esc));
    assert_eq!(
        run(
            &mut menu,
            Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL))
        )
        .as_deref(),
        Some("copy")
    );
    assert_eq!(called.get(), 1);
    run(
        &mut menu,
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Right),
            column: 2,
            row: 2,
            modifiers: KeyModifiers::NONE,
        }),
    );
    assert!(text(&mut menu).contains("取り消す"));
    menu.set_locale("en");
    run(
        &mut menu,
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Right),
            column: 2,
            row: 2,
            modifiers: KeyModifiers::NONE,
        }),
    );
    assert!(text(&mut menu).contains("Undo"));
}
#[test]
fn shared_menubar_submenus_disabled_items_accelerators_and_atomic_replacement() {
    let value = Rc::new(Cell::new(0));
    let count = value.clone();
    let nested = value.clone();
    let mut menu = TerminalMenu::default();
    menu.set_bar(vec![
        MenuItem::submenu(
            "文件",
            vec![
                MenuItem::command("disabled", "Disabled", || panic!("disabled")).enabled(false),
                MenuItem::Separator,
                MenuItem::command("save", "Save", move || count.set(count.get() + 1))
                    .accelerator("Ctrl+S"),
                MenuItem::submenu(
                    "More",
                    vec![MenuItem::command("nested", "Nested", move || {
                        nested.set(nested.get() + 10)
                    })],
                ),
            ],
        ),
        MenuItem::submenu("Edit", vec![MenuItem::command("other", "Other", || {})]),
    ])
    .unwrap();
    let area = Rect::new(3, 2, 25, 10);
    let mut buffer = Buffer::empty(area);
    menu.render(area, &mut buffer);
    assert_eq!(menu.content_area(area), Rect::new(3, 3, 25, 9));
    run(&mut menu, key(KeyCode::F(10)));
    assert!(menu.is_open());
    menu.render(area, &mut buffer);
    assert!(buffer.content.iter().any(|c| c.symbol() == "S"));
    assert!(run(&mut menu, key(KeyCode::Down)).is_none());
    run(&mut menu, key(KeyCode::Right));
    assert_eq!(
        run(&mut menu, key(KeyCode::Enter)).as_deref(),
        Some("nested")
    );
    assert_eq!(value.get(), 10);
    assert!(!menu.is_open());
    let shortcut = Event::Key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
    assert!(matches!(
        menu.handle(&shortcut, true, true),
        MenuInput::Pass
    ));
    assert_eq!(run(&mut menu, shortcut.clone()).as_deref(), Some("save"));
    assert_eq!(value.get(), 11);
    assert!(menu
        .set_bar(vec![MenuItem::command("invalid", "root command", || {})])
        .is_err());
    assert_eq!(run(&mut menu, shortcut).as_deref(), Some("save"));
    assert_eq!(value.get(), 12);
    run(&mut menu, key(KeyCode::F(10)));
    menu.handle(&Event::FocusLost, true, false);
    assert!(!menu.is_open());
    assert!(matches!(
        menu.handle(&key(KeyCode::F(10)), false, false),
        MenuInput::Pass
    ));
    assert!(!menu.is_open());
}
#[test]
fn context_pointer_capture_unicode_typeahead_and_small_viewports() {
    let called = Rc::new(Cell::new(0));
    let count = called.clone();
    let mut menu = TerminalMenu::default();
    menu.set_context(vec![
        MenuItem::command("one", "Alpha", || {}),
        MenuItem::Separator,
        MenuItem::command("two", "中文", move || count.set(count.get() + 1)),
    ])
    .unwrap();
    let mouse = |kind, x, y| {
        Event::Mouse(MouseEvent {
            kind,
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        })
    };
    run(
        &mut menu,
        mouse(MouseEventKind::Down(MouseButton::Right), 28, 18),
    );
    run(&mut menu, key(KeyCode::Char('中')));
    assert_eq!(run(&mut menu, key(KeyCode::Enter)).as_deref(), Some("two"));
    assert_eq!(called.get(), 1);
    for area in [
        Rect::new(5, 4, 1, 1),
        Rect::new(5, 4, 8, 5),
        Rect::new(5, 4, 30, 20),
    ] {
        run(
            &mut menu,
            mouse(MouseEventKind::Down(MouseButton::Right), 60, 50),
        );
        let mut buffer = Buffer::empty(area);
        menu.render(area, &mut buffer);
        run(&mut menu, key(KeyCode::Down));
        run(&mut menu, key(KeyCode::Esc));
    }
    // A release outside cancels a press; returning later must not activate it.
    let area = Rect::new(0, 0, 30, 20);
    run(
        &mut menu,
        mouse(MouseEventKind::Down(MouseButton::Right), 0, 0),
    );
    menu.render(area, &mut Buffer::empty(area));
    run(
        &mut menu,
        mouse(MouseEventKind::Down(MouseButton::Left), 1, 3),
    );
    run(
        &mut menu,
        mouse(MouseEventKind::Up(MouseButton::Left), 29, 19),
    );
    assert!(run(
        &mut menu,
        mouse(MouseEventKind::Up(MouseButton::Left), 1, 3)
    )
    .is_none());
    assert_eq!(called.get(), 1);
}
#[test]
fn checked_radio_and_role_items_paint_controlled_state_and_dispatch_shared_commands() {
    use jadix_core::menu::MenuRole;
    let hits = Rc::new(Cell::new(0));
    let selected = hits.clone();
    let mut menu = TerminalMenu::default();
    menu.set_bar(vec![MenuItem::submenu(
        "State",
        vec![
            MenuItem::command("check", "Checked", move || selected.set(selected.get() + 1))
                .checked(true),
            MenuItem::command("radio", "Radio", || {}).radio("group", true),
            MenuItem::command("other", "Other", || {}).radio("group", false),
            MenuItem::command("copy", "Copy", || {}).role(MenuRole::Copy),
        ],
    )])
    .unwrap();
    let area = Rect::new(0, 0, 30, 10);
    let mut buffer = Buffer::empty(area);
    menu.render(area, &mut buffer);
    run(&mut menu, key(KeyCode::F(10)));
    menu.render(area, &mut buffer);
    let text = buffer
        .content
        .iter()
        .map(|c| c.symbol())
        .collect::<String>();
    assert!(text.contains("[x] Checked"), "{text}");
    assert!(text.contains("(●) Radio"), "{text}");
    assert!(text.contains("( ) Other"), "{text}");
    assert_eq!(
        run(&mut menu, key(KeyCode::Enter)).as_deref(),
        Some("check")
    );
    assert_eq!(hits.get(), 1);
    run(&mut menu, key(KeyCode::F(10)));
    menu.render(area, &mut buffer);
    assert!(buffer
        .content
        .iter()
        .map(|c| c.symbol())
        .collect::<String>()
        .contains("[x] Checked"));
}

#[test]
fn monochrome_menu_keeps_selection_and_disabled_semantics_at_nonzero_origin() {
    use jadix_tui::{
        ratatui::style::{Color, Modifier},
        CellColors,
    };
    let mut menu = TerminalMenu::default();
    menu.set_bar(vec![MenuItem::submenu(
        "File",
        vec![
            MenuItem::command("disabled", "Disabled", || panic!("disabled")).enabled(false),
            MenuItem::command("ok", "Enabled", || {}),
        ],
    )])
    .unwrap();
    menu.set_colors(CellColors::Monochrome);
    run(&mut menu, key(KeyCode::F(10)));
    let area = Rect::new(4, 3, 24, 8);
    let mut buffer = Buffer::empty(Rect::new(0, 0, 40, 20));
    buffer[(0, 0)].set_fg(Color::Red);
    menu.render(area, &mut buffer);
    let drawn: Vec<_> = buffer
        .content
        .iter()
        .filter(|c| c.symbol() != " ")
        .collect();
    assert!(drawn
        .iter()
        .all(|c| c.fg == Color::Reset && c.bg == Color::Reset));
    assert!(drawn.iter().any(|c| c.modifier.contains(Modifier::DIM)));
    assert!(drawn
        .iter()
        .any(|c| c.modifier.contains(Modifier::REVERSED)));
    assert_eq!(buffer[(0, 0)].fg, Color::Red);
    assert_eq!(run(&mut menu, key(KeyCode::Enter)).as_deref(), Some("ok"));
    menu.set_colors(CellColors::Palette16);
    assert!(menu.needs_frame());
    menu.render(area, &mut buffer);
    assert!(buffer.content.iter().any(|c| c.fg == Color::White));
}
