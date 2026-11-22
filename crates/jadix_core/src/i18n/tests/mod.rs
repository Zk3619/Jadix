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
        assert_eq!(tr_with("greet", &[("name", "Ada")]), "Hello Ada");
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
        let prev = crate::reactivity::set_signal_key_scope(3);
        set_locale("ja");
        assert_eq!(locale().language(), "ja");
        crate::reactivity::set_signal_key_scope(0);
        assert_eq!(locale().language(), "en");
        crate::reactivity::set_signal_key_scope(3);
        assert_eq!(locale().language(), "ja");
        crate::reactivity::set_signal_key_scope(prev);
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

#[test]
fn surface_catalogs_and_preferences_are_isolated_and_follow_system_until_overridden() {
    with_i18n(|| {
        use request::{load_packaged, LocaleRequest};
        let first = LocaleContext::new();
        let second = LocaleContext::new();
        first.update_environment("fr-FR", Some("Europe/Paris")).unwrap();
        second.update_environment("ja-JP", Some("Asia/Tokyo")).unwrap();
        load_packaged(&first, &[("locales/fr.json".into(), br#"{"greeting":"Bonjour %{name}"}"#.to_vec())].into()).unwrap();
        load_packaged(&second, &[("locales/ja.json".into(), "{\"greeting\":\"こんにちは {name}\"}".as_bytes().to_vec())].into()).unwrap();
        assert_eq!(first.with(|| tr_with("greeting", &[("name", "Ada")])), "Bonjour Ada");
        assert_eq!(second.with(|| tr_with("greeting", &[("name", "Ada")])), "こんにちは Ada");
        assert_eq!(first.with(available_locales), ["fr"]);
        assert_eq!(second.with(available_locales), ["ja"]);
        assert_eq!(tr("greeting"), "greeting");
        first.with(|| { set_locale("ar"); set_timezone("Asia/Riyadh"); });
        first.update_environment("de-DE", Some("Europe/Berlin")).unwrap();
        assert_eq!(first.with(|| locale().language().to_string()), "ar");
        assert_eq!(first.with(timezone).as_deref(), Some("Asia/Riyadh"));
        let state = LocaleRequest::Reset.execute(&first).unwrap();
        assert_eq!(state["locale"], "de-DE");
        assert_eq!(state["timezone"], "Europe/Berlin");
        assert_eq!(second.with(timezone).as_deref(), Some("Asia/Tokyo"));
        assert_eq!(first.with(available_locales), ["fr"]);
    });
}

#[test]
fn explicit_fallback_precedes_english_and_rejected_updates_are_atomic() {
    with_i18n(|| {
        use request::{load_packaged, LocaleRequest};
        let context = LocaleContext::new();
        context.with(|| {
            add_json("en", r#"{"label":"English"}"#).unwrap();
            add_json("fr", r#"{"label":"Français"}"#).unwrap();
            set_locale("ja"); set_fallback("fr");
            assert_eq!(tr("label"), "Français");
        });
        let previous = context.with(locale);
        assert!(LocaleRequest::Configure { locale: Some("de".into()), fallback: None,
            timezone: Some("".into()) }.execute(&context).is_err());
        assert_eq!(context.with(locale), previous);
        assert!(load_packaged(&context, &[
            ("locales/de.json".into(), br#"{"new":"Neu"}"#.to_vec()),
            ("locales/zh.json".into(), b"{".to_vec()),
        ].into()).is_err());
        assert_eq!(context.with(|| tr("new")), "new");
        for invalid in [
            LocaleRequest::Number { value: f64::NAN, digits: None },
            LocaleRequest::Date { year: 2025, month: 2, day: 29, short: true },
            LocaleRequest::Time { hour: 24, minute: 0, second: None, hour12: false },
            LocaleRequest::Currency { value: 1.0, code: "bad".into() },
        ] { assert!(invalid.execute(&context).is_err()); }
        assert!(LocaleRequest::Date { year: 2024, month: 2, day: 29, short: true }.execute(&context).is_ok());
    });
}

#[test]
fn locale_context_restores_outer_scope_after_unwind() {
    with_i18n(|| {
        let context = LocaleContext::new();
        let outer = crate::reactivity::signal_key_scope();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| context.with(|| {
            set_locale("ja");
            panic!("test nested scope cleanup");
        })));
        assert_eq!(crate::reactivity::signal_key_scope(), outer);
        assert_eq!(locale().language(), "en");
    });
}
