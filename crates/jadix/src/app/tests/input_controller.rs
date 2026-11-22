use super::*;

#[cfg(target_os = "windows")]
#[test]
#[ignore = "native event loop; run in a separate process"]
fn pointer_cancellation_ends_text_gestures_without_editing_selection() {
    use crate::core::reactivity::{create_root, use_signal};
    use crate::utils::{text_field, text_sel_move};
    use winit::platform::windows::EventLoopBuilderExtWindows;

    let mut builder = winit::event_loop::EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let (_sender, receiver) = std::sync::mpsc::channel();
    let mut app = crate::app::lifecycle::create_app(
        crate::app::WindowOptions::new(),
        crate::core::tree::Element::new,
        event_loop.create_proxy(),
        receiver,
    );
    let (_owner, ()) = create_root(|| {
        let rest = "cancel-text-gesture";
        let value = use_signal(&text_field::input_value_signal_key(rest), || {
            "seed".to_string()
        });
        let selection = use_signal(&text_field::input_selection_signal_key(rest), || {
            Some((0, 4))
        });
        for cancellation in 0..5 {
            app.bind_pointer_window(WindowId::from_raw(10));
            app.input_drag_rest = Some(rest.into());
            app.input_drag_multi = true;
            app.input_drag_multi_base = vec![(0, 4)];
            app.label_drag_key = Some("label-selection".into());
            app.left_button_down = true;
            app.mouse_buttons = 1;
            text_field::set_text_drag_select_active(true);
            text_sel_move::begin_potential(
                rest,
                (0, 4),
                "seed".into(),
                2,
                (10.0, 10.0),
                14.0,
                400,
                [0; 4],
                20.0,
            );
            crate::app::set_pointer_capture(format!("input:{rest}"));
            match cancellation {
                0 => app.cancel_pointer_session(),
                1 => app.bind_pointer_window(WindowId::from_raw(11)),
                _ => {
                    use crate::app::pointer_events::{
                        EventPhase, MouseButton, PointerEvent, PointerKind, dispatch_pointer,
                        hit_path,
                    };
                    use std::cell::Cell;
                    use std::rc::Rc;
                    let handoff_phase = match cancellation {
                        2 => EventPhase::Capture,
                        3 => EventPhase::Target,
                        _ => EventPhase::Bubble,
                    };
                    let calls = Rc::new(Cell::new(0));
                    let armed = Rc::new(Cell::new(true));
                    let handler = || {
                        let calls = calls.clone();
                        let armed = armed.clone();
                        move |event: &PointerEvent| {
                            calls.set(calls.get() + 1);
                            if armed.get() && event.phase == handoff_phase {
                                crate::app::pointer_events::begin_native_pointer_handoff();
                                assert!(!text_field::text_drag_select_active());
                                assert!(!text_sel_move::is_active());
                                assert!(crate::app::pointer_capture_id().is_none());
                            }
                        }
                    };
                    let mut root = crate::core::tree::Element::new().on_mousedown(handler());
                    root.layout_bounds = [0.0, 0.0, 100.0, 100.0];
                    let mut child = crate::core::tree::Element::new().on_mousedown(handler());
                    child.layout_bounds = root.layout_bounds;
                    root.children.push(child);
                    let path = hit_path(&root, 10.0, 10.0);
                    app.click_press_path = Some(path.clone());
                    let mut event = PointerEvent::new(10.0, 10.0, MouseButton::Left, 1);
                    dispatch_pointer(&path, &mut event, PointerKind::Down);
                    assert!(event.is_consumed());
                    assert!(event.default_prevented());
                    assert_eq!(calls.get(), cancellation - 1);
                    let mut blocked = PointerEvent::new(10.0, 10.0, MouseButton::Left, 1);
                    dispatch_pointer(&path, &mut blocked, PointerKind::Down);
                    assert_eq!(calls.get(), cancellation - 1);
                    assert!(app.finish_native_pointer_handoff());
                    assert!(!app.finish_native_pointer_handoff());
                    assert!(app.click_press_path.is_none());
                    armed.set(false);
                    let mut fresh = PointerEvent::new(10.0, 10.0, MouseButton::Left, 1);
                    dispatch_pointer(&path, &mut fresh, PointerKind::Down);
                    assert!(!fresh.is_consumed());
                    assert!(!fresh.default_prevented());
                    assert_eq!(calls.get(), cancellation + 2);
                }
            }
            app.cancel_pointer_session();
            assert!(app.input_drag_rest.is_none());
            assert!(!app.input_drag_multi);
            assert!(app.input_drag_multi_base.is_empty());
            assert!(app.label_drag_key.is_none());
            assert!(!app.left_button_down);
            assert_eq!(app.mouse_buttons, 0);
            assert!(!text_field::text_drag_select_active());
            assert!(!text_sel_move::is_active());
            assert!(crate::app::pointer_capture_id().is_none());
            assert_eq!(value.peek(), "seed");
            assert_eq!(selection.peek(), Some((0, 4)));
        }
    });
}

#[cfg(target_os = "windows")]
#[test]
#[ignore = "native event loop; run in a separate process"]
fn typed_keyboard_and_ime_preserve_controlled_input() {
    use crate::core::reactivity::{create_root, use_signal};
    use crate::utils::text_field::{
        bind_controlled_value, input_caret_signal_key, input_preedit_signal_key,
        input_selection_signal_key, input_value_signal_key,
    };
    use winit::platform::windows::EventLoopBuilderExtWindows;

    let mut builder = winit::event_loop::EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let (_sender, receiver) = std::sync::mpsc::channel();
    let mut app = crate::app::lifecycle::create_app(
        crate::app::WindowOptions::new(),
        crate::core::tree::Element::new,
        event_loop.create_proxy(),
        receiver,
    );
    let (_owner, ()) = create_root(|| {
        let focus = use_signal("__jadix_button_focus_id", String::new);
        let window = WindowId::from_raw(10);
        for (input_type, text) in [
            (InputType::Number, "1e3"),
            (InputType::Password, "pāss字👩‍💻"),
            (InputType::Tel, "#123"),
            (InputType::Email, "o'hara@example.com"),
            (InputType::Url, "https://例子.测试"),
        ] {
            let rest = format!("typed-data-{}", input_type.as_str());
            let external = use_signal(&format!("external-{rest}"), || "seed".to_string());
            bind_controlled_value(&rest, external);
            let value = use_signal(&input_value_signal_key(&rest), String::new);
            let caret = use_signal(&input_caret_signal_key(&rest), || None::<usize>);
            let selection = use_signal(
                &input_selection_signal_key(&rest),
                || None::<(usize, usize)>,
            );
            use_signal(&input_type_signal_key(&rest), String::new).set(input_type.as_str().into());
            focus.set(format!("input:{rest}"));
            value.set("seed".into());
            caret.set(Some(4));
            selection.set(Some((0, 4)));
            let key = Key::Character(text.into());
            app.handle_keyboard_text_signals(KeyEvent {
                physical_key: winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyA),
                logical_key: key.clone(),
                text: Some(text.into()),
                location: winit::keyboard::KeyLocation::Standard,
                state: ElementState::Pressed,
                repeat: false,
                text_with_all_modifiers: Some(text.into()),
                key_without_modifiers: key,
            });
            assert_eq!(value.peek(), text, "keyboard {input_type:?}");
            assert_eq!(external.peek(), text);
            assert_eq!(caret.peek(), Some(text.len()));
            assert!(selection.peek().is_none());

            assert_eq!(
                filter_typed_insert(&rest, "seed", &sanitize_pasted_line(text)),
                text,
                "paste filtering {input_type:?}"
            );
            value.set("seed".into());
            caret.set(Some(4));
            selection.set(Some((0, 4)));
            app.handle_ime_event(Ime::Enabled, window);
            app.handle_ime_event(Ime::Preedit("组".into(), Some((3, 3))), window);
            app.handle_ime_event(Ime::Commit(text.into()), window);
            assert_eq!(value.peek(), text, "IME {input_type:?}");
            assert_eq!(external.peek(), text);
            assert_eq!(caret.peek(), Some(text.len()));
            assert!(
                use_signal(&input_preedit_signal_key(&rest), String::new)
                    .peek()
                    .is_empty()
            );
            app.handle_ime_event(Ime::Disabled, window);
        }
        focus.set(String::new());
    });
}

#[cfg(target_os = "windows")]
#[test]
#[ignore = "native event loop; run in a separate process"]
fn ime_events_publish_atomic_unicode_edit_snapshots() {
    use crate::core::reactivity::{create_effect, use_signal};
    use winit::platform::windows::EventLoopBuilderExtWindows;
    let mut builder = winit::event_loop::EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let (_sender, receiver) = std::sync::mpsc::channel();
    let mut app = crate::app::lifecycle::create_app(
        crate::app::WindowOptions::new(),
        crate::core::tree::Element::new,
        event_loop.create_proxy(),
        receiver,
    );
    let rest = "ime-atomic-unicode";
    let value = use_signal(
        &crate::utils::text_field::input_value_signal_key(rest),
        String::new,
    );
    let preedit = use_signal(
        &crate::utils::text_field::input_preedit_signal_key(rest),
        String::new,
    );
    let caret = use_signal(
        &crate::utils::text_field::input_caret_signal_key(rest),
        || None::<usize>,
    );
    let selection = use_signal(
        &crate::utils::text_field::input_selection_signal_key(rest),
        || None::<(usize, usize)>,
    );
    let focus = use_signal("__jadix_button_focus_id", String::new);
    focus.set(format!("input:{rest}"));
    value.set("ab中文z".into());
    caret.set(Some(8));
    selection.set(Some((2, 8)));
    let observed = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let output = observed.clone();
    create_effect(move || {
        output
            .borrow_mut()
            .push((value.get(), preedit.get(), caret.get()))
    });
    let window = WindowId::from_raw(10);
    app.handle_ime_event(Ime::Preedit("拼".into(), Some((3, 3))), window);
    assert_eq!(
        observed.borrow().len(),
        2,
        "one settled notification per OS event"
    );
    assert_eq!(
        observed.borrow().last().unwrap(),
        &("abz".into(), "拼".into(), Some(2))
    );
    app.handle_ime_event(Ime::Preedit(String::new(), None), window);
    let before_commit = observed.borrow().len();
    app.handle_ime_event(Ime::Commit("👩‍💻".into()), window);
    assert_eq!(observed.borrow().len(), before_commit + 1);
    assert_eq!(value.peek(), "ab👩‍💻z");
    assert_eq!(caret.peek(), Some(13));
    assert!(preedit.peek().is_empty());
    app.handle_ime_event(Ime::Commit(String::new()), window);
    app.handle_ime_event(Ime::Disabled, window);
    assert_eq!(
        value.peek(),
        "ab👩‍💻z",
        "teardown must not duplicate or delete committed text"
    );
    focus.set(String::new());
}

#[cfg(target_os = "windows")]
#[test]
#[ignore = "native event loop; run in a separate process"]
fn enabled_ime_without_preedit_accepts_keyboard_text() {
    use winit::platform::windows::EventLoopBuilderExtWindows;
    let mut builder = winit::event_loop::EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let (_tx, rx) = std::sync::mpsc::channel();
    let mut app = crate::app::lifecycle::create_app(
        crate::app::WindowOptions::new(),
        crate::core::tree::Element::new,
        event_loop.create_proxy(),
        rx,
    );
    let rest = "ime-enabled-keyboard";
    let focus = crate::core::reactivity::use_signal("__jadix_button_focus_id", String::new);
    focus.set(format!("input:{rest}"));
    crate::core::reactivity::use_signal(crate::utils::text_edit::field::IME_ACTIVE_SIGNAL, || {
        false
    })
    .set(true);
    let key = Key::Character("a".into());
    let event = KeyEvent {
        physical_key: winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyA),
        logical_key: key.clone(),
        text: Some("a".into()),
        location: winit::keyboard::KeyLocation::Standard,
        state: ElementState::Pressed,
        repeat: false,
        text_with_all_modifiers: Some("a".into()),
        key_without_modifiers: key,
    };
    app.handle_keyboard_text_signals(event.clone());
    let value = crate::core::reactivity::use_signal(
        &crate::utils::text_field::input_value_signal_key(rest),
        String::new,
    );
    assert_eq!(
        value.peek(),
        "a",
        "IME Enabled is not an active composition"
    );
    crate::core::reactivity::use_signal(
        &crate::utils::text_field::input_preedit_signal_key(rest),
        String::new,
    )
    .set("拼".into());
    app.handle_keyboard_text_signals(event);
    assert_eq!(value.peek(), "a", "composition key must not insert twice");
    focus.set(String::new());
    crate::core::reactivity::use_signal(crate::utils::text_edit::field::IME_ACTIVE_SIGNAL, || {
        false
    })
    .set(false);
}

#[test]
fn escape_and_function_keys_do_not_insert_text() {
    assert!(named_key_blocks_text_insert(&Key::Named(NamedKey::Escape)));
    assert!(named_key_blocks_text_insert(&Key::Named(NamedKey::F1)));
    assert!(named_key_blocks_text_insert(&Key::Named(NamedKey::Tab)));
    assert!(named_key_blocks_text_insert(&Key::Character("\t".into())));
    assert!(is_tab_key(&Key::Named(NamedKey::Tab)));
    assert!(is_tab_key(&Key::Character("\t".into())));
    assert!(!is_tab_key(&Key::Character("a".into())));
    assert!(!named_key_blocks_text_insert(&Key::Character(" ".into())));
    assert!(!named_key_blocks_text_insert(&Key::Named(NamedKey::Enter)));
    assert!(!named_key_blocks_text_insert(&Key::Character("a".into())));
    // Character(U+001B) may still appear; discarded by strip_control_insert, not in the Named guard.
    assert!(!named_key_blocks_text_insert(&Key::Character(
        "\u{1b}".into()
    )));
}

#[test]
fn hold_repeat_insert_and_delete_skip_visual_metrics() {
    assert!(!text_edit_key_needs_visual_metrics(&Key::Character(
        "a".into()
    )));
    assert!(!text_edit_key_needs_visual_metrics(&Key::Named(
        NamedKey::Backspace
    )));
    assert!(!text_edit_key_needs_visual_metrics(&Key::Named(
        NamedKey::Delete
    )));
    assert!(!text_edit_key_is_caret_or_delete(&Key::Character(
        "a".into()
    )));
    assert!(!text_edit_key_is_caret_or_delete(&Key::Named(
        NamedKey::Backspace
    )));
    assert!(text_edit_key_is_caret_or_delete(&Key::Named(
        NamedKey::Delete
    )));
    assert!(text_edit_key_needs_visual_metrics(&Key::Named(
        NamedKey::ArrowLeft
    )));
    assert!(text_edit_key_needs_visual_metrics(&Key::Named(
        NamedKey::ArrowDown
    )));
    assert!(text_edit_key_needs_visual_metrics(&Key::Named(
        NamedKey::Home
    )));
    assert!(text_edit_key_needs_visual_metrics(&Key::Named(
        NamedKey::PageUp
    )));
}

#[test]
fn hold_repeat_coalesces_to_one_mutation_per_frame() {
    clear_text_edit_frame_mutated();
    let insert = Key::Character("a".into());
    let backspace = Key::Named(NamedKey::Backspace);
    assert!(is_text_hold_mutation_key(&insert));
    assert!(is_text_hold_mutation_key(&backspace));
    assert!(!is_text_hold_mutation_key(&Key::Named(NamedKey::ArrowLeft)));

    assert!(
        text_edit_repeat_allow(false, true, false),
        "first keydown (repeat=false) always applies"
    );
    mark_text_edit_frame_mutated();
    let already = TEXT_EDIT_FRAME_MUTATED.with(|c| c.get());
    assert!(already);
    assert!(
        !text_edit_repeat_allow(true, true, already),
        "queued OS repeats in the same pump must not mutate"
    );
    assert!(!text_edit_repeat_allow(true, true, already));
    assert!(
        text_edit_repeat_allow(true, false, already),
        "arrow repeats are not hold-insert mutations"
    );
    clear_text_edit_frame_mutated();
    assert!(
        text_edit_repeat_allow(true, true, TEXT_EDIT_FRAME_MUTATED.with(|c| c.get())),
        "after redraw, the next repeat applies once"
    );
    let _ = take_text_edit_force_immediate();
}

#[test]
fn hold_repeat_burst_applies_once_until_redraw() {
    clear_text_edit_frame_mutated();
    let _ = take_text_edit_force_immediate();
    let mut applied = 0_u32;
    for i in 0..12 {
        let repeat = i > 0;
        let already = TEXT_EDIT_FRAME_MUTATED.with(|c| c.get());
        if text_edit_repeat_allow(repeat, true, already) {
            mark_text_edit_frame_mutated();
            applied += 1;
        }
    }
    assert_eq!(
        applied, 1,
        "12 OS repeats in one pump must show as one glyph, not a batch"
    );
    assert!(take_text_edit_force_immediate());
    clear_text_edit_frame_mutated();
    let already = TEXT_EDIT_FRAME_MUTATED.with(|c| c.get());
    assert!(text_edit_repeat_allow(true, true, already));
}

#[test]
fn focus_move_arm_is_a_one_shot_poll_latch() {
    let _ = take_text_edit_force_immediate();
    arm_text_edit_force_immediate();
    assert!(
        take_text_edit_force_immediate(),
        "Placeholder → number click must latch Poll before caret WaitUntil"
    );
    assert!(
        !take_text_edit_force_immediate(),
        "about_to_wait consumes the latch once"
    );
}

#[cfg(target_os = "windows")]
#[test]
#[ignore = "native event loop; run in a separate process"]
fn max_length_keyboard_ime_and_overwrite_preserve_selection_and_suffix() {
    use crate::core::reactivity::{create_root, use_signal};
    use crate::utils::text_field::*;
    use winit::platform::windows::EventLoopBuilderExtWindows;
    let mut builder = winit::event_loop::EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let (_sender, receiver) = std::sync::mpsc::channel();
    let mut app = crate::app::lifecycle::create_app(
        crate::app::WindowOptions::new(),
        crate::core::tree::Element::new,
        event_loop.create_proxy(),
        receiver,
    );
    let (_owner, ()) = create_root(|| {
        let rest = "max-length-controller";
        let value = use_signal(&input_value_signal_key(rest), String::new);
        let caret = use_signal(&input_caret_signal_key(rest), || None::<usize>);
        let selection = use_signal(&input_selection_signal_key(rest), || None::<(usize, usize)>);
        let limit = use_signal(&input_max_length_signal_key(rest), || None::<usize>);
        let external = use_signal("max-length-external", String::new);
        bind_controlled_value(rest, external);
        use_signal("__jadix_button_focus_id", String::new).set(input_focus_id(rest));
        let mut type_text = |text: &str| {
            let key = Key::Character(text.into());
            app.handle_keyboard_text_signals(KeyEvent {
                physical_key: winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyA),
                logical_key: key.clone(),
                text: Some(text.into()),
                location: winit::keyboard::KeyLocation::Standard,
                state: ElementState::Pressed,
                repeat: false,
                text_with_all_modifiers: Some(text.into()),
                key_without_modifiers: key,
            });
        };
        value.set("ab".into());
        caret.set(Some(1));
        limit.set(Some(5));
        type_text("中🦀x");
        assert_eq!(value.peek(), "a中🦀b");
        assert_eq!(external.peek(), value.peek());
        limit.set(Some(2));
        selection.set(Some((0, value.peek().len())));
        let selected = selection.peek();
        type_text("👩‍💻");
        assert_eq!(value.peek(), "a中🦀b");
        assert_eq!(selection.peek(), selected);
        type_text("🦀x");
        assert_eq!(value.peek(), "🦀");
        type_text("x");
        assert_eq!(value.peek(), "🦀");
        value.set("ab".into());
        caret.set(Some(0));
        selection.set(None);
        use_signal(&input_overwrite_signal_key(rest), || false).set(true);
        type_text("🦀");
        assert_eq!(value.peek(), "ab");
        type_text("中");
        assert_eq!(value.peek(), "中b");
        use_signal(&input_overwrite_signal_key(rest), || false).set(false);
        value.set("ab".into());
        caret.set(Some(1));
        limit.set(Some(5));
        let window = WindowId::from_raw(10);
        app.handle_ime_event(Ime::Enabled, window);
        app.handle_ime_event(Ime::Preedit("组".into(), Some((3, 3))), window);
        app.handle_ime_event(Ime::Commit("中🦀x".into()), window);
        assert_eq!(value.peek(), "a中🦀b");
        assert_eq!(external.peek(), value.peek());
        app.handle_ime_event(Ime::Disabled, window);
        value.set("ab".into());
        caret.set(Some(1));
        use_signal(&input_ime_anchor_signal_key(rest), || None::<usize>).set(Some(1));
        use_signal(&input_preedit_signal_key(rest), String::new).set("中🦀x".into());
        ime_support::finish_composition_preserve_text(&input_focus_id(rest));
        assert_eq!(value.peek(), "a中🦀b");
        assert!(
            use_signal(&input_preedit_signal_key(rest), String::new)
                .peek()
                .is_empty()
        );
    });
}

#[test]
fn max_length_multi_cursor_rejection_does_not_publish_or_move_cursors() {
    use crate::core::reactivity::{create_root, use_signal};
    use crate::utils::text_field::*;
    let (_owner, ()) = create_root(|| {
        let rest = "max-length-multi";
        use_signal(&input_max_length_signal_key(rest), || Some(2usize));
        let value = use_signal(&input_value_signal_key(rest), || "ab".to_string());
        use crate::utils::text_annotations::*;
        set_text_decorations(
            rest,
            vec![TextDecoration {
                range: 1..2,
                value: TextRangeStyle::default(),
            }],
        );
        let decorations = text_decorations(rest);
        let cursors = load_text_cursors(rest, "ab");
        assert!(!commit_multi_edit(
            rest,
            value,
            "axb".into(),
            cursors.clone(),
            vec![(1..1, "x".into())],
            crate::utils::text_edit_history::EditKind::Insert
        ));
        assert_eq!(value.peek(), "ab");
        assert_eq!(load_text_cursors(rest, "ab"), cursors);
        assert_eq!(text_decorations(rest), decorations);
        value.set("external over-limit".into());
        assert!(accepts_input_edit(
            rest,
            &value.peek(),
            "external over-limi"
        ));
        assert!(!accepts_input_edit(
            rest,
            &value.peek(),
            "external over-limit!"
        ));
    });
}

#[cfg(feature = "text-undo")]
#[test]
fn disjoint_cursor_edits_and_history_keep_middle_annotations_precise() {
    use crate::core::reactivity::{create_root, use_signal};
    use crate::utils::{
        text_annotations::*, text_edit_history::*, text_field::*, text_multi_cursor::*,
    };
    let (_owner, ()) = create_root(|| {
        let id = "annotations-multi-history";
        let value = use_signal(&input_value_signal_key(id), || "a中b".to_string());
        let cursors = vec![TextCursor::collapsed(0), TextCursor::collapsed(5)];
        store_text_cursors(id, &cursors);
        set_text_decorations(
            id,
            vec![TextDecoration {
                range: 1..4,
                value: TextRangeStyle {
                    underline: true,
                    ..Default::default()
                },
            }],
        );
        let mut next = value.peek();
        let (cursors, edits) = insert_at_cursors_with_edits(&mut next, cursors, "x");
        assert!(commit_multi_edit(
            id,
            value,
            next,
            cursors,
            edits,
            EditKind::Insert
        ));
        assert_eq!(value.peek(), "xa中bx");
        assert_eq!(text_decorations(id)[0].range, 2..5);
        assert!(undo_field(id));
        assert_eq!(value.peek(), "a中b");
        assert_eq!(text_decorations(id)[0].range, 1..4);
        assert!(redo_field(id));
        assert_eq!(value.peek(), "xa中bx");
        assert_eq!(text_decorations(id)[0].range, 2..5);
    });
}
