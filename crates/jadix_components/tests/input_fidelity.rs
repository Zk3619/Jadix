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
