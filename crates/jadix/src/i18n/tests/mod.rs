use super::*;

fn with_i18n<R>(body: impl FnOnce() -> R) -> R {
    with_lock(body)
}

#[test]
fn feature_flag_matches_cfg() {
    assert_eq!(i18n_enabled(), cfg!(feature = "i18n"));
}

#[test]
fn component_register_is_source_of_truth() {
    with_i18n(|| {
        assert_eq!(
            tr_default("jadix.pagination.previous", "Previous"),
            "Previous"
        );
        assert_eq!(tr("missing.key"), "missing.key");
        set_locale("zh-CN");
        assert_eq!(
            tr_default_zh("jadix.pagination.next", "Next", "下一页"),
            "下一页"
        );
    });
}

#[test]
fn component_zh_cn_without_overlay() {
    with_i18n(|| {
        register_locales("jadix.pagination.previous", "Previous", "上一页");
        assert_eq!(tr("jadix.pagination.previous"), "Previous");
        set_locale("zh-CN");
        assert_eq!(tr("jadix.pagination.previous"), "上一页");
        set_locale("en-US");
        assert_eq!(tr("jadix.pagination.previous"), "Previous");
    });
}

#[test]
fn t_macro_interpolates_registered() {
    with_i18n(|| {
        register("greet", "Hello {name}");
        assert_eq!(crate::t!("greet", name = "Ada"), "Hello Ada");
    });
}

#[test]
fn tr_with_default_interpolates_without_catalog() {
    with_i18n(|| {
        let s = tr_with_default(
            "jadix.date_picker.week_of",
            "Week of {date}",
            &[("date", "August 14, 2026")],
        );
        assert_eq!(s, "Week of August 14, 2026");
        set_locale("zh-CN");
        let zh = tr_with_default_zh(
            "jadix.date_picker.week_of",
            "Week of {date}",
            "{date} 所在周",
            &[("date", "8月14日")],
        );
        assert_eq!(zh, "8月14日 所在周");
    });
}

#[test]
fn set_locale_updates_tag() {
    with_i18n(|| {
        set_locale("ar");
        assert!(writing_direction().is_rtl());
        assert_eq!(locale().language(), "ar");
    });
}

#[test]
fn overlay_overrides_register() {
    with_i18n(|| {
        register("jadix.pagination.previous", "Previous");
        add_toml(
            "zh-CN",
            r#"
[jadix.pagination]
previous = "上一页"
"#,
        )
        .unwrap();
        set_locale("zh-CN");
        assert_eq!(tr("jadix.pagination.previous"), "上一页");
        assert_eq!(
            tr_default_zh("jadix.pagination.previous", "Previous", "上一页"),
            "上一页"
        );
    });
}

#[test]
fn clear_bundles_keeps_register() {
    with_i18n(|| {
        register("jadix.pagination.next", "Next");
        add_toml(
            "en",
            r#"
[jadix.pagination]
next = "Forward"
"#,
        )
        .unwrap();
        assert_eq!(tr("jadix.pagination.next"), "Forward");
        clear_bundles();
        assert_eq!(tr("jadix.pagination.next"), "Next");
    });
}

#[test]
fn short_aliases_and_pseudo_locale() {
    with_i18n(|| {
        register("hello", "Hello");
        set_locale("jp");
        assert_eq!(locale().language(), "ja");
        set_locale("kr");
        assert_eq!(locale().language(), "ko");
        set_locale("iw");
        assert_eq!(locale().language(), "he");
        set_locale("in");
        assert_eq!(locale().language(), "id");
        set_locale("en-XA");
        let accented = tr("hello");
        assert_ne!(accented, "Hello");
        assert!(accented.contains('é') || accented.contains('ø'));
    });
}

#[test]
fn per_window_locale() {
    with_i18n(|| {
        set_locale("en");
        let prev = crate::core::reactivity::set_signal_key_scope(3);
        set_locale("ja");
        assert_eq!(locale().language(), "ja");
        crate::core::reactivity::set_signal_key_scope(0);
        assert_eq!(locale().language(), "en");
        crate::core::reactivity::set_signal_key_scope(3);
        assert_eq!(locale().language(), "ja");
        crate::core::reactivity::set_signal_key_scope(prev);
    });
}

#[test]
fn week_start_follows_region() {
    with_i18n(|| {
        set_locale("en-US");
        assert_eq!(first_weekday(), 0);
        set_locale("zh-CN");
        assert_eq!(first_weekday(), 1);
    });
}
