use super::*;

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
            assert!(use_signal(&input_preedit_signal_key(&rest), String::new)
                .peek()
                .is_empty());
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
