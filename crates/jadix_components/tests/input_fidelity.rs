use jadix_components::components::input::{
    filter_insert_for_type, focus_id_for_input, input, input_preedit_signal_key,
    input_type_signal_key, input_value_signal_key, InputType,
};
use jadix_components::core::reactivity::{create_root, use_signal};
use jadix_components::core::tree::Element;

fn input_text_leaf(element: &Element) -> Option<&Element> {
    if element.text.is_some() {
        return Some(element);
    }
    element.children.iter().find_map(input_text_leaf)
}

#[test]
fn typed_input_retains_value_and_paints_preserved_text() {
    let (_owner, ()) = create_root(|| {
        for (input_type, text) in [
            (InputType::Number, "1e3"),
            (InputType::Password, "pāss字👩‍💻"),
            (InputType::Tel, "#123"),
            (InputType::Email, "o'hara@example.com"),
            (InputType::Url, "https://例子.测试"),
        ] {
            let id = format!("input-fidelity-{}", input_type.as_str());
            let external = use_signal(&format!("external-{id}"), || {
                filter_insert_for_type(input_type, "", text)
            });
            use_signal("__jadix_button_focus_id", String::new).set(focus_id_for_input(&id));
            let element: Element = input(&id)
                .input_type(input_type)
                .value_signal(external)
                .into();
            assert_eq!(external.peek(), text);
            assert_eq!(
                use_signal(&input_value_signal_key(&id), String::new).peek(),
                text
            );
            assert_eq!(
                use_signal(&input_type_signal_key(&id), String::new).peek(),
                input_type.as_str()
            );
            let leaf = input_text_leaf(&element).expect("input text leaf");
            let displayed = if input_type.is_password() {
                "•".repeat(text.chars().count())
            } else {
                text.to_string()
            };
            assert_eq!(leaf.text.as_deref(), Some(displayed.as_str()));
        }
    });
}

#[test]
fn password_masks_unicode_preedit_without_mutating_committed_value() {
    let (_owner, ()) = create_root(|| {
        let id = "password-preedit-fidelity";
        let text = "pāss字";
        let preedit = "拼👩‍💻";
        let external = use_signal("password-preedit-external", || text.to_string());
        use_signal("__jadix_button_focus_id", String::new).set(focus_id_for_input(id));
        use_signal(&input_preedit_signal_key(id), String::new).set(preedit.into());
        let element: Element = input(id)
            .input_type(InputType::Password)
            .value_signal(external)
            .into();
        let leaf = input_text_leaf(&element).expect("password text leaf");
        let displayed = "•".repeat(text.chars().count() + preedit.chars().count());
        assert_eq!(leaf.text.as_deref(), Some(displayed.as_str()));
        assert_eq!(external.peek(), text);
        assert_eq!(
            use_signal(&input_value_signal_key(id), String::new).peek(),
            text
        );
    });
}

#[test]
fn submit_is_opt_in_and_multiline_modifier_return_keeps_editing() {
    use jadix_components::components::textarea::textarea;
    use std::{cell::Cell, rc::Rc};
    use winit::{
        event::{ElementState, KeyEvent},
        keyboard::{Key, KeyCode, KeyLocation, ModifiersState, NamedKey, PhysicalKey},
    };
    let (_owner, ()) = create_root(|| {
        let calls = Rc::new(Cell::new(0));
        let submit = || {
            let calls = calls.clone();
            move || calls.set(calls.get() + 1)
        };
        let default: Element = textarea("ordinary-newline").into();
        assert!(default.text_edit_key_handler.is_none());
        let disabled: Element = textarea("disabled-submit")
            .disabled(true)
            .on_submit(submit())
            .into();
        assert!(disabled.text_edit_key_handler.is_none());
        let draft = use_signal("submit-draft", || "draft中文😀".to_string());
        let multi: Element = textarea("chat-submit")
            .value_signal(draft)
            .on_submit(submit())
            .into();
        let single: Element = input("single-submit").on_submit(submit()).into();
        let mut enter = KeyEvent {
            physical_key: PhysicalKey::Code(KeyCode::Enter),
            logical_key: Key::Named(NamedKey::Enter),
            text: Some("\r".into()),
            location: KeyLocation::Standard,
            state: ElementState::Pressed,
            repeat: false,
            text_with_all_modifiers: Some("\r".into()),
            key_without_modifiers: Key::Named(NamedKey::Enter),
        };
        let multi = multi.text_edit_key_handler.as_ref().unwrap();
        let single = single.text_edit_key_handler.as_ref().unwrap();
        assert!(multi(&enter, ModifiersState::empty()));
        assert_eq!(calls.get(), 1);
        for modifiers in [
            ModifiersState::SHIFT,
            ModifiersState::ALT,
            ModifiersState::CONTROL,
            ModifiersState::META,
        ] {
            assert!(!multi(&enter, modifiers));
        }
        assert!(single(&enter, ModifiersState::SHIFT));
        assert_eq!(calls.get(), 2);
        enter.repeat = true;
        assert!(multi(&enter, ModifiersState::empty()));
        assert_eq!(calls.get(), 2);
        enter.repeat = false;
        enter.state = ElementState::Released;
        assert!(!multi(&enter, ModifiersState::empty()));
        assert_eq!(draft.peek(), "draft中文😀");
    });
}

#[cfg(feature = "macros")]
#[test]
fn submit_callbacks_are_available_through_rsx_aliases() {
    use jadix_components::components::{Input, InputProps, Textarea, TextareaProps};
    use jadix_components::ui;

    let (_owner, ()) = create_root(|| {
        let single = ui! { <Input id="rsx-submit-input" onSubmit={|| {}} /> };
        let multi = ui! { <Textarea id="rsx-submit-textarea" on_submit={|| {}} /> };
        assert!(single.text_edit_key_handler.is_some());
        assert!(multi.text_edit_key_handler.is_some());
    });
}
