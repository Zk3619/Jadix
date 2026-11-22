use super::*;

#[test]
fn rtl_languages() {
    assert!(WritingDirection::for_language("ar").is_rtl());
    assert!(WritingDirection::for_language("he").is_rtl());
    assert!(WritingDirection::for_language("fa").is_rtl());
    assert!(!WritingDirection::for_language("zh").is_rtl());
    assert!(!WritingDirection::for_language("en").is_rtl());
}
