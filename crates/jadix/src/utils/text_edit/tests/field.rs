use super::*;

#[test]
fn typed_insert_preserves_user_data() {
    for (input_type, text) in [
        (InputType::Number, "1e3"),
        (InputType::Number, "-1.5E+2"),
        (InputType::Number, "1e-"),
        (InputType::Number, "1x2"),
        (InputType::Password, "pāss字👩‍💻 '"),
        (InputType::Tel, "#123;ext=4"),
        (InputType::Email, "o'hara@example.com"),
        (InputType::Url, "https://例子.测试/路径?q=✓"),
    ] {
        assert_eq!(filter_insert_for_type(input_type, "", text), text);
        assert_eq!(filter_insert_for_type(input_type, "prefix", text), text);
    }
}

#[test]
fn typed_insert_removes_controls_without_changing_printable_text() {
    for input_type in [
        InputType::Number,
        InputType::Password,
        InputType::Tel,
        InputType::Email,
        InputType::Url,
    ] {
        assert_eq!(
            filter_insert_for_type(input_type, "", "字\0\t\n\r\u{1b}\u{7f}1"),
            "字1"
        );
    }
}

#[test]
fn password_accepts_ime_and_keeps_unicode_mask_mapping() {
    assert!(InputType::Password.allows_ime());
    let value = "pāss字👩‍💻";
    let masked = mask_password(value);
    assert_eq!(masked.chars().count(), value.chars().count());
    for real_index in value
        .char_indices()
        .map(|(index, _)| index)
        .chain([value.len()])
    {
        let masked_index = map_byte_index_to_masked(value, real_index, &masked);
        assert_eq!(
            map_masked_byte_index_to_real(value, masked_index),
            real_index
        );
    }
}

#[test]
fn validation_accepts_preserved_input_and_rejects_incomplete_numbers() {
    assert!(InputType::Number.validate_value("1e3"));
    assert!(InputType::Number.validate_value("-1.5E+2"));
    for value in [".", "-.", "1e-", "1x2", "NaN", "inf", "1e9999"] {
        assert!(!InputType::Number.validate_value(value), "{value}");
    }
    assert!(InputType::Email.validate_value("o'hara@example.com"));
    assert!(InputType::Tel.validate_value("#123"));
    assert!(InputType::Url.validate_value("https://例子.测试"));
}
