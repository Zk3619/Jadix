use jadix_tui::{crossterm::event::*, TerminalEditor};
#[test]
fn maxlength_counts_utf16_and_preserves_graphemes_selection_history_and_external_values() {
    let mut editor = TerminalEditor::new("A").unwrap();
    editor.set_selection(1, 1).unwrap();
    editor.set_max_length(Some(3)).unwrap();
    assert!(editor.insert("🦀Z").unwrap());
    assert_eq!(editor.document().text(), "A🦀");
    assert!(!editor.insert("x").unwrap());
    editor.set_selection(1, editor.document().len()).unwrap();
    assert!(editor.insert("e\u{301}Z").unwrap());
    assert_eq!(editor.document().text(), "Ae\u{301}");
    editor
        .handle(&Event::Key(KeyEvent::new(
            KeyCode::Char('z'),
            KeyModifiers::CONTROL,
        )))
        .unwrap();
    assert_eq!(editor.document().text(), "A🦀");
    editor.set_max_length(Some(1)).unwrap();
    editor.set_selection(1, editor.document().len()).unwrap();
    assert!(!editor.insert("🦀").unwrap());
    assert_eq!(editor.selected_text(), "🦀");
    editor
        .handle(&Event::Key(KeyEvent::new(
            KeyCode::Delete,
            KeyModifiers::NONE,
        )))
        .unwrap();
    assert_eq!(editor.document().text(), "A");
    editor
        .apply_external(editor.document().revision(), "controlled longer")
        .unwrap();
    assert_eq!(editor.document().text(), "controlled longer");
    assert!(!editor.insert("x").unwrap());
    assert!(editor
        .set_max_length(Some(TerminalEditor::MAX_BYTES + 1))
        .is_err());
    assert_eq!(editor.max_length(), Some(1));
}
#[cfg(feature = "portable")]
#[test]
fn portable_input_limit_and_shared_menu_callback_preserve_focus_and_suspend_on_blur() {
    use jadix::portable::{Element, Signal};
    use jadix_components::portable::*;
    use jadix_tui::{
        ratatui::{buffer::Buffer, layout::Rect, widgets::Widget},
        CellApplication, MenuItem,
    };
    use std::{cell::RefCell, rc::Rc};
    let saved = Rc::new(RefCell::new(None));
    let state = saved.clone();
    let mut app = CellApplication::new([1., 1.], move || {
        let text = Signal::new(String::new());
        let count = Signal::new(0);
        *state.borrow_mut() = Some((text, count));
        Element::new()
            .flex_col()
            .child(textarea("draft").value_signal(text).max_length(3))
            .child(button("action").text("Action"))
    })
    .unwrap();
    let (text, count) = saved.borrow().unwrap();
    app.set_menubar(vec![MenuItem::submenu(
        "File",
        vec![MenuItem::command("count", "Count", move || {
            count.set(count.get() + 1)
        })],
    )])
    .unwrap();
    let area = Rect::new(0, 0, 30, 8);
    app.present(area, 0).unwrap();
    (&mut app).render(area, &mut Buffer::empty(area));
    let edit = app
        .surface()
        .nodes()
        .find(|(_, d)| {
            matches!(
                d.content,
                jadix_core::presentation::NodeContent::Textarea { .. }
            )
        })
        .unwrap()
        .0;
    app.surface_mut().focus_node(edit).unwrap();
    app.handle(Event::Paste("🦀AB".into())).unwrap();
    assert_eq!(text.get(), "🦀A");
    let key = |code| Event::Key(KeyEvent::new(code, KeyModifiers::NONE));
    app.handle(key(KeyCode::F(10))).unwrap();
    assert!(app.needs_frame());
    app.handle(Event::Paste("must not edit".into())).unwrap();
    assert_eq!(text.get(), "🦀A");
    app.handle(key(KeyCode::Enter)).unwrap();
    assert_eq!(count.get(), 1);
    assert_eq!(app.menus().last_command(), Some("count"));
    assert_eq!(app.surface().focused(), Some(edit));
    app.handle(key(KeyCode::F(10))).unwrap();
    app.handle(Event::FocusLost).unwrap();
    assert!(!app.menus().is_open());
    app.handle(key(KeyCode::F(10))).unwrap();
    assert!(!app.menus().is_open());
}

#[cfg(feature = "portable")]
#[test]
fn publishing_a_modal_closes_open_global_menus_and_blocks_their_callbacks() {
    use jadix::portable::{Element, Signal};
    use jadix_components::portable::button;
    use jadix_core::presentation::NodeContent;
    use jadix_tui::{ratatui::layout::Rect, CellApplication, MenuItem};
    use std::{cell::RefCell, rc::Rc};
    let saved = Rc::new(RefCell::new(None));
    let handles = saved.clone();
    let mut app = CellApplication::new([8., 16.], move || {
        let open = Signal::new(false);
        let count = Signal::new(0);
        *handles.borrow_mut() = Some((open, count));
        let mut modal = Element::new()
            .id("modal")
            .w(120.)
            .h(48.)
            .child(button("inside").text("Inside"));
        modal.descriptor.content = NodeContent::Dialog {
            open: false,
            close_on_overlay_click: true,
            revision: 0,
        };
        modal.dialog_open = Some(open);
        Element::new()
            .child(button("outside").text("Outside"))
            .child(modal)
    })
    .unwrap();
    let (open, count) = saved.borrow().unwrap();
    app.set_menubar(vec![MenuItem::submenu(
        "File",
        vec![
            MenuItem::command("count", "Count", move || count.set(count.get() + 1))
                .accelerator("Ctrl+Alt+K"),
        ],
    )])
    .unwrap();
    let area = Rect::new(0, 0, 40, 12);
    app.present(area, 0).unwrap();
    let key = |code, modifiers| Event::Key(KeyEvent::new(code, modifiers));
    app.handle(key(KeyCode::F(10), KeyModifiers::NONE)).unwrap();
    assert!(app.menus().is_open());
    open.set(true);
    app.present(area, 0).unwrap();
    assert!(!app.menus().is_open());
    app.handle(key(KeyCode::F(10), KeyModifiers::NONE)).unwrap();
    app.handle(key(
        KeyCode::Char('k'),
        KeyModifiers::CONTROL | KeyModifiers::ALT,
    ))
    .unwrap();
    assert!(!app.menus().is_open());
    assert_eq!(count.get(), 0);
    open.set(false);
    app.present(area, 0).unwrap();
    app.handle(key(KeyCode::F(10), KeyModifiers::NONE)).unwrap();
    app.handle(key(KeyCode::Enter, KeyModifiers::NONE)).unwrap();
    assert_eq!(count.get(), 1);
}
