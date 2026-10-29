use super::*;
use crate::i18n::{register, set_locale, tr};

#[test]
fn config_sets_locale_and_messages() {
    crate::i18n::with_lock(|| {
        register("jadix.pagination.previous", "Previous");
        load_str(
            "i18n.toml",
            r#"
locale = "zh-CN"
[jadix.pagination]
previous = "上一页"
"#,
        )
        .unwrap();
        assert_eq!(crate::i18n::locale().language(), "zh");
        assert_eq!(tr("jadix.pagination.previous"), "上一页");
    });
}

#[test]
fn missing_overlay_falls_back_to_register() {
    crate::i18n::with_lock(|| {
        register("jadix.pagination.next", "Next");
        set_locale("zh-CN");
        assert_eq!(tr("jadix.pagination.next"), "Next");
    });
}
