use super::*;

#[test]
fn english_one_other() {
    assert_eq!(category("en", 1), PluralCategory::One);
    assert_eq!(category("en", 0), PluralCategory::Other);
    assert_eq!(category("en", 2), PluralCategory::Other);
}

#[test]
fn chinese_always_other() {
    assert_eq!(category("zh", 0), PluralCategory::Other);
    assert_eq!(category("zh", 1), PluralCategory::Other);
    assert_eq!(category("zh", 8), PluralCategory::Other);
}

#[test]
fn russian_few() {
    assert_eq!(category("ru", 1), PluralCategory::One);
    assert_eq!(category("ru", 2), PluralCategory::Few);
    assert_eq!(category("ru", 5), PluralCategory::Many);
    assert_eq!(category("ru", 11), PluralCategory::Many);
    assert_eq!(category("ru", 21), PluralCategory::One);
}
