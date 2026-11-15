#![cfg(feature = "portable")]
use jadix::portable::{on_cleanup, Element, Signal};
use jadix_components::portable::*;
use jadix_core::presentation::{EventPayload, NodeContent, NodeId, PlatformEvent};
use jadix_tui::{
    crossterm::event::*,
    ratatui::{buffer::Buffer, layout::Rect, widgets::Widget},
    CellApplication,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}
fn named(app: &CellApplication, label: &str) -> NodeId {
    app.surface()
        .nodes()
        .find(|(_, d)| {
            matches!(&d.content, NodeContent::Button {label:l,..} if l==label)
                || d.accessible_name.as_deref() == Some(label)
        })
        .unwrap()
        .0
}
fn paint(app: &mut CellApplication) {
    let area = Rect::new(0, 0, 80, 24);
    let mut b = Buffer::empty(area);
    app.render(area, &mut b);
}

#[test]
fn request_exit_stops_dispatch_before_dispose_and_cleanup_is_idempotent() {
    let cleaned = Rc::new(Cell::new(0));
    let cleanup = cleaned.clone();
    let mut app = CellApplication::new([8., 16.], move || {
        on_cleanup(move || cleanup.set(cleanup.get() + 1));
        Element::new().child(button("ok").text("OK"))
    })
    .unwrap();
    app.present(Rect::new(0, 0, 80, 24), 0).unwrap();
    assert!(!app.exit_requested());
    assert_eq!(cleaned.get(), 0);

    app.request_exit();
    assert!(app.exit_requested());
    assert!(app.present(Rect::new(0, 0, 80, 24), 1).is_err());
    assert!(app.handle(key(KeyCode::Enter)).is_err());
    assert_eq!(cleaned.get(), 0);

    app.dispose();
    app.dispose();
    assert_eq!(cleaned.get(), 1);
}

#[test]
fn shared_callbacks_edit_revisions_echo_undo_modal_and_owner_cleanup() {
    let handles = Rc::new(RefCell::new(None));
    let saved = handles.clone();
    let cleaned = Rc::new(Cell::new(0));
    let cleanup = cleaned.clone();
    let mut app = CellApplication::new([8., 16.], move || {
        on_cleanup(move || cleanup.set(cleanup.get() + 1));
        let text = Signal::new(String::new());
        let checked = Signal::new(false);
        let open = Signal::new(false);
        let count = Signal::new(0);
        *saved.borrow_mut() = Some((text, checked, open, count));
        let mut editor: Element = textarea("edit").value_signal(text).into();
        editor.descriptor.accessible_name = Some("Editor".into());
        let mut panel = Element::new().id("modal").w(240.).h(80.).child(
            button("close")
                .text("Close")
                .on_click(move || open.set(false)),
        );
        panel.descriptor.content = NodeContent::Dialog {
            open: false,
            close_on_overlay_click: true,
            revision: 0,
        };
        panel.dialog_open = Some(open);
        Element::new().flex_col().gap(8.).children([
            editor.h(32.).flex_shrink(0.),
            button("inc")
                .text("Increment")
                .on_click(move || count.set(count.get() + 1))
                .into(),
            checkbox("check")
                .checked_signal(checked)
                .child(Element::new().text("Checked"))
                .into(),
            button("open")
                .text("Open")
                .on_click(move || open.set(true))
                .into(),
            panel,
        ])
    })
    .unwrap();
    app.present(Rect::new(0, 0, 80, 24), 0).unwrap();
    paint(&mut app);
    let (text, checked, open, count) = handles.borrow().unwrap();
    let editor = named(&app, "Editor");
    app.surface_mut().focus_node(editor).unwrap();
    let edit = app.handle(Event::Paste("中文 🦀".into())).unwrap().unwrap();
    assert_eq!(text.get(), "中文 🦀");
    assert!(matches!(
        edit.payload,
        EventPayload::Edit { revision: 1, .. }
    ));
    app.handle(Event::Key(KeyEvent::new(
        KeyCode::Char('z'),
        KeyModifiers::CONTROL,
    )))
    .unwrap();
    assert_eq!(text.get(), "");
    app.handle(Event::Key(KeyEvent::new(
        KeyCode::Char('y'),
        KeyModifiers::CONTROL,
    )))
    .unwrap();
    assert_eq!(text.get(), "中文 🦀");
    let check = app
        .surface()
        .nodes()
        .find(|(_, d)| matches!(d.content, NodeContent::Checkbox { .. }))
        .unwrap()
        .0;
    app.surface_mut().focus_node(check).unwrap();
    app.handle(key(KeyCode::Char(' '))).unwrap();
    assert!(checked.get());
    app.handle(key(KeyCode::Char(' '))).unwrap();
    assert!(!checked.get());
    let inc = named(&app, "Increment");
    app.surface_mut().focus_node(inc).unwrap();
    app.handle(key(KeyCode::Enter)).unwrap();
    assert_eq!(count.get(), 1);
    let opener = named(&app, "Open");
    app.surface_mut().focus_node(opener).unwrap();
    app.handle(key(KeyCode::Enter)).unwrap();
    assert!(open.get());
    assert!(app
        .dispatch(&PlatformEvent {
            node: inc,
            payload: EventPayload::Activate
        })
        .is_err());
    app.handle(key(KeyCode::Esc)).unwrap();
    assert!(!open.get());
    assert_eq!(app.surface().focused(), Some(opener));
    assert!(app.dispatch(&edit).is_err());
    app.dispose();
    assert_eq!(cleaned.get(), 1);
    assert!(text.try_peek().is_none());
    assert!(app.handle(key(KeyCode::Enter)).is_err());
    drop(app);
    assert_eq!(cleaned.get(), 1);
}

#[test]
fn shared_navigation_pages_preserve_signals_and_remount_identities() {
    let handles = Rc::new(RefCell::new(None));
    let saved = handles.clone();
    let mut app = CellApplication::new([8., 16.], move || {
        let history = Signal::new(NavigationHistory::new(["edit", "review"], "edit").unwrap());
        let text = Signal::new(String::new());
        *saved.borrow_mut() = Some((history, text));
        Element::new().flex_col().children([
            navigation_page(
                "edit-page",
                history,
                "edit",
                Element::new().flex_col().children([
                    textarea("draft").value_signal(text).into(),
                    navigation_link("review", history, "review", "Review"),
                ]),
            ),
            navigation_page(
                "review-page",
                history,
                "review",
                Element::new()
                    .flex_col()
                    .child(navigation_back("back", history, "Back")),
            ),
        ])
    })
    .unwrap();
    app.present(Rect::new(0, 0, 80, 24), 0).unwrap();
    let (history, text) = handles.borrow().unwrap();
    let original = app.surface().focused().unwrap();
    app.handle(Event::Paste("retained".into())).unwrap();
    let review = named(&app, "Review");
    app.surface_mut().focus_node(review).unwrap();
    app.handle(key(KeyCode::Enter)).unwrap();
    assert_eq!(history.get().current(), "review");
    let back = named(&app, "Back");
    app.surface_mut().focus_node(back).unwrap();
    app.handle(key(KeyCode::Enter)).unwrap();
    assert_eq!(history.get().current(), "edit");
    assert_eq!(text.get(), "retained");
    assert_ne!(app.surface().focused(), Some(original));
    assert!(app
        .dispatch(&PlatformEvent {
            node: original,
            payload: EventPayload::Edit {
                value: "stale".into(),
                revision: 100,
                selection_utf16: [0, 0],
                composing_utf16: None
            }
        })
        .is_err());
    assert_eq!(text.get(), "retained");
}

#[test]
fn portable_macro_and_roving_navigation_work_beside_gpu_features() {
    let mut app=CellApplication::new([8.,16.],|| {
        let value=Signal::new(String::from("a"));
        let mut element:Element=radio_group_scope("options",Some(value),"a",false,||Element::new().flex_col().children([
            radio_group_item("a").child(Element::new().text("Alpha")).into(),
            radio_group_item("b").child(Element::new().text("Beta")).into(),
        ]));
        let macro_tree: Element=jadix_components::portable_ui!{ <div><Button id="macro-button">"Macro"</Button></div> };
        element.descriptor.style.height=Some(96.);Element::new().flex_col().children([element,macro_tree])
    }).unwrap();
    app.present(Rect::new(0, 0, 80, 24), 0).unwrap();
    paint(&mut app);
    assert!(app
        .surface()
        .nodes()
        .any(|(_, d)| matches!(&d.content,NodeContent::Button{label,..} if label=="Macro")));
    let first = app.surface().focused();
    app.handle(key(KeyCode::Down)).unwrap();
    assert_ne!(app.surface().focused(), first);
    app.handle(key(KeyCode::Home)).unwrap();
    assert_eq!(app.surface().focused(), first);
    app.handle(key(KeyCode::Tab)).unwrap();
    assert_eq!(app.surface().focused(), Some(named(&app, "Macro")));
}

#[test]
fn scrolling_and_focus_observations_do_not_reproject_the_application() {
    let refreshes = Rc::new(Cell::new(0));
    let observed = refreshes.clone();
    let mut app = CellApplication::new([1., 1.], || {
        let mut scroll = Element::new().flex_col().h(20.).w(40.);
        scroll.descriptor.content = NodeContent::Scroll;
        scroll
            .children
            .push(jadix::portable::app::reactive::bind_text(
                "live",
                move || {
                    observed.set(observed.get() + 1);
                    "observed".into()
                },
            ));
        scroll.children.push(
            Element::from(button("focus").text("Focus"))
                .h(1.)
                .flex_shrink(0.),
        );
        scroll.children.extend((0..100).map(|i| {
            Element::new()
                .text(format!("row {i}"))
                .h(1.)
                .flex_shrink(0.)
        }));
        scroll
    })
    .unwrap();
    app.present(Rect::new(0, 0, 80, 24), 0).unwrap();
    paint(&mut app);
    let before = refreshes.get();
    let revision = app.surface().revision();
    let target = named(&app, "Focus");
    app.dispatch(&PlatformEvent {
        node: target,
        payload: EventPayload::Focus,
    })
    .unwrap();
    let event = app
        .handle(Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 10,
            row: 10,
            modifiers: KeyModifiers::NONE,
        }))
        .unwrap()
        .unwrap();
    assert!(matches!(event.payload, EventPayload::Scroll { y: 3., .. }));
    assert_eq!(refreshes.get(), before);
    assert_eq!(app.surface().revision(), revision);
    assert!(app
        .dispatch(&PlatformEvent {
            node: target,
            payload: EventPayload::Scroll { x: 0., y: 1. }
        })
        .is_err());
}

#[test]
fn named_shortcuts_resolve_scopes_repeats_editing_modal_eligibility_and_remounts() {
    use jadix_tui::Shortcut;
    let handles = Rc::new(RefCell::new(None));
    let saved = handles.clone();
    let mut app = CellApplication::new([8., 16.], || {
        let count = Signal::new(0);
        let local = Signal::new(0);
        let text = Signal::new(String::new());
        let open = Signal::new(false);
        let visible = Signal::new(true);
        *saved.borrow_mut() = Some((count, local, text, open, visible));
        let mut page = Element::new().id("scope").flex_col().children([
            textarea("editor").value_signal(text).into(),
            button("local")
                .text("Local")
                .on_click(move || local.set(local.get() + 1))
                .into(),
        ]);
        page.visibility = Some(Rc::new(move || visible.get()));
        let mut modal = Element::new().id("modal").w(160.).h(64.).child(
            button("close")
                .text("Close")
                .on_click(move || open.set(false)),
        );
        modal.descriptor.content = NodeContent::Dialog {
            open: false,
            close_on_overlay_click: true,
            revision: 0,
        };
        modal.dialog_open = Some(open);
        Element::new().flex_col().children([
            page,
            button("global")
                .text("Global")
                .on_click(move || count.set(count.get() + 1))
                .into(),
            modal,
        ])
    })
    .unwrap();
    app.present(Rect::new(0, 0, 80, 24), 0).unwrap();
    paint(&mut app);
    let (count, local, _, open, visible) = handles.borrow().unwrap();
    let binding = |target: &str, scope: Option<&str>, in_editors| Shortcut {
        code: KeyCode::Char('k'),
        modifiers: KeyModifiers::CONTROL,
        target: target.into(),
        scope: scope.map(str::to_owned),
        in_editors,
    };
    app.set_shortcuts(vec![
        binding("global", None, false),
        binding("local", Some("scope"), false),
    ])
    .unwrap();
    let shortcut = Event::Key(KeyEvent::new(KeyCode::Char('K'), KeyModifiers::CONTROL));
    // Editors own their chords unless the application explicitly overrides them.
    assert!(app.handle(shortcut.clone()).unwrap().is_none());
    assert_eq!(count.get(), 0);
    let target = named(&app, "Local");
    app.surface_mut().focus_node(target).unwrap();
    app.handle(shortcut.clone()).unwrap();
    assert_eq!(local.get(), 1);
    assert_eq!(count.get(), 0);
    app.handle(Event::Key(KeyEvent::new_with_kind(
        KeyCode::Char('k'),
        KeyModifiers::CONTROL,
        KeyEventKind::Repeat,
    )))
    .unwrap();
    assert_eq!(local.get(), 1);
    // Invalid replacement is atomic; the previous map keeps working.
    assert!(app
        .set_shortcuts(vec![
            binding("global", None, false),
            binding("local", None, false)
        ])
        .is_err());
    app.handle(shortcut.clone()).unwrap();
    assert_eq!(local.get(), 2);
    visible.set(false);
    app.present(Rect::new(0, 0, 80, 24), 0).unwrap();
    app.handle(shortcut.clone()).unwrap();
    assert_eq!(count.get(), 1);
    visible.set(true);
    app.present(Rect::new(0, 0, 80, 24), 0).unwrap();
    let new = named(&app, "Local");
    assert_ne!(new, target);
    app.surface_mut().focus_node(new).unwrap();
    app.handle(shortcut.clone()).unwrap();
    assert_eq!(local.get(), 3);
    open.set(true);
    app.present(Rect::new(0, 0, 80, 24), 0).unwrap();
    app.handle(shortcut.clone()).unwrap();
    assert_eq!(count.get(), 1);
    assert_eq!(local.get(), 3);
    app.set_shortcuts(vec![binding("close", None, false)])
        .unwrap();
    app.handle(shortcut).unwrap();
    assert!(!open.get());
    app.handle(Event::FocusLost).unwrap();
    let before = count.get();
    app.set_shortcuts(vec![binding("global", None, true)])
        .unwrap();
    app.handle(Event::Key(KeyEvent::new(
        KeyCode::Char('k'),
        KeyModifiers::CONTROL,
    )))
    .unwrap();
    assert_eq!(count.get(), before);
}

#[test]
fn automatic_checkpoint_recovers_controlled_editors_and_rejects_bad_state_before_changes() {
    use jadix_tui::{ratatui::style::Color, CheckpointStore};
    fn create() -> (CellApplication, Signal<String>) {
        let saved = Rc::new(RefCell::new(None));
        let target = saved.clone();
        let mut app = CellApplication::new([8., 16.], move || {
            let value = Signal::new(String::new());
            *target.borrow_mut() = Some(value);
            let mut secret: Element = input("secret")
                .value_signal(Signal::new(String::from("sensitive")))
                .into();
            if let NodeContent::Input { password, .. } = &mut secret.descriptor.content {
                *password = true;
            }
            Element::new().flex_col().children([
                textarea("draft").value_signal(value).into(),
                textarea("other")
                    .value_signal(Signal::new(String::new()))
                    .into(),
                secret,
            ])
        })
        .unwrap();
        app.present(Rect::new(0, 0, 80, 24), 0).unwrap();
        let value = saved.borrow().unwrap();
        (app, value)
    }
    fn id(app: &CellApplication, n: usize) -> NodeId {
        app.surface()
            .nodes()
            .filter(|(_, d)| matches!(d.content, NodeContent::Textarea { .. }))
            .nth(n)
            .unwrap()
            .0
    }
    let directory = std::env::temp_dir().join(format!(
        "jadix-app-recovery-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("draft.json");
    let (mut first, value) = create();
    assert!(first
        .enable_checkpoint(&path, &["draft", "secret"])
        .is_err());
    assert!(!path.exists());
    assert!(first.enable_checkpoint(&path, &["draft", "draft"]).is_err());
    let first_editor = id(&first, 0);
    first.surface_mut().focus_node(first_editor).unwrap();
    assert!(!first.enable_checkpoint(&path, &["draft", "other"]).unwrap());
    first.handle(Event::Paste("中文 👩‍💻".into())).unwrap();
    first
        .handle(Event::Key(KeyEvent::new(
            KeyCode::Left,
            KeyModifiers::SHIFT,
        )))
        .unwrap();
    let bytes = std::fs::read(&path).unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("sensitive"));
    let generation = first.save_checkpoint().unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    let old_id = id(&first, 0);
    assert_eq!(value.get(), "中文 👩‍💻");
    drop(first);
    let (mut second, recovered) = create();
    assert!(second
        .enable_checkpoint(&path, &["draft", "other"])
        .unwrap());
    assert_eq!(recovered.get(), "中文 👩‍💻");
    assert_ne!(id(&second, 0), old_id);
    assert_eq!(second.surface().focused(), Some(id(&second, 0)));
    second
        .handle(Event::Key(KeyEvent::new(
            KeyCode::Char('z'),
            KeyModifiers::CONTROL,
        )))
        .unwrap();
    assert_eq!(recovered.get(), "");
    second
        .handle(Event::Key(KeyEvent::new(
            KeyCode::Char('z'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        )))
        .unwrap();
    assert_eq!(recovered.get(), "中文 👩‍💻");
    assert!(second.save_checkpoint().unwrap() > generation);
    // A stale second writer returns its conflict after accepting local input.
    let (mut third, third_value) = create();
    assert!(third.enable_checkpoint(&path, &["draft", "other"]).unwrap());
    second.handle(Event::Paste("new".into())).unwrap();
    let newer = std::fs::read(&path).unwrap();
    assert!(third.handle(Event::Paste("local".into())).is_err());
    assert!(third_value.get().contains("local"));
    assert_eq!(std::fs::read(&path).unwrap(), newer);
    assert!(third.save_checkpoint().is_err());
    drop(second);
    drop(third);
    // Correct checksum, bad last editor: no earlier editor or Signal is changed.
    let store = CheckpointStore::new(&path, 16 * 1024 * 1024).unwrap();
    let saved = store.read().unwrap().unwrap();
    let mut payload: serde_json::Value = serde_json::from_str(&saved.payload).unwrap();
    payload["fields"]["other"] = "{bad editor}".into();
    store
        .save(Some(saved.generation), &payload.to_string())
        .unwrap();
    let (mut fourth, untouched) = create();
    let focus = fourth.surface().focused();
    assert!(fourth
        .enable_checkpoint(&path, &["draft", "other"])
        .is_err());
    assert_eq!(untouched.get(), "");
    assert_eq!(fourth.surface().focused(), focus);
    assert_eq!(fourth.save_checkpoint().unwrap(), None);
    // Application-level policy reaches menus as well as content.
    fourth
        .set_menubar(vec![jadix_tui::MenuItem::submenu(
            "File",
            vec![jadix_tui::MenuItem::command("ok", "OK", || {})],
        )])
        .unwrap();
    fourth
        .surface_mut()
        .set_colors(jadix_tui::CellColors::Monochrome);
    fourth.handle(key(KeyCode::F(10))).unwrap();
    let mut buffer = Buffer::empty(Rect::new(0, 0, 80, 24));
    (&mut fourth).render(Rect::new(0, 0, 80, 24), &mut buffer);
    assert!(buffer
        .content
        .iter()
        .all(|c| c.fg == Color::Reset && c.bg == Color::Reset));
    drop(fourth);
    std::fs::remove_file(&path).unwrap();
    std::fs::remove_dir(&directory).unwrap();
}

#[test]
fn portable_canonical_max_length_needs_no_separate_constraint_tree_walk() {
    let saved = Rc::new(RefCell::new(None));
    let capture = saved.clone();
    let mut app = CellApplication::new([8., 16.], move || {
        let text = Signal::new(String::new());
        *capture.borrow_mut() = Some(text);
        let mut field: Element = input("bounded").value_signal(text).max_length(5).into();
        field.descriptor.accessible_name = Some("Bounded".into());
        field
    })
    .unwrap();
    app.present(Rect::new(0, 0, 80, 24), 0).unwrap();
    let text = saved.borrow().unwrap();
    let id = named(&app, "Bounded");
    app.surface_mut().focus_node(id).unwrap();
    app.handle(Event::Paste("ab中🦀x".into())).unwrap();
    assert_eq!(text.get(), "ab中🦀");
    if let NodeContent::Input { max_length, .. } = &mut app.element_mut().descriptor.content {
        *max_length = Some(2);
    }
    app.present(Rect::new(0, 0, 80, 24), 0).unwrap();
    app.handle(Event::Paste("x".into())).unwrap();
    assert_eq!(text.get(), "ab中🦀");
    app.handle(Event::Key(KeyEvent::new(
        KeyCode::Char('a'),
        KeyModifiers::CONTROL,
    )))
    .unwrap();
    app.handle(Event::Paste("🦀x".into())).unwrap();
    assert_eq!(text.get(), "🦀");
}
