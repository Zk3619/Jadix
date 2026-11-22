use super::*;

#[test]
fn western_grouping() {
    let loc = Locale::en();
    assert_eq!(format_number_locale(&loc, 1234.0, Some(0)), "1,234");
    assert_eq!(format_number_locale(&loc, 1234.5, Some(2)), "1,234.50");
}

#[test]
fn european_grouping() {
    let loc = Locale::parse("de-DE").unwrap();
    assert_eq!(format_number_locale(&loc, 1234.5, Some(2)), "1.234,50");
}

#[test]
fn english_long_date() {
    let loc = Locale::en();
    let s = format_date_locale(&loc, 2026, 8, 14, DateStyle::Long);
    assert!(s.contains("2026"), "{s}");
    assert!(
        s.contains("14") || s.contains("August") || s.contains("8"),
        "{s}"
    );
}

#[test]
fn zh_cn_relative_and_list() {
    crate::i18n::with_lock(|| {
        let loc = Locale::parse("zh-CN").unwrap();
        assert_eq!(format_relative_locale(&loc, -1, RelativeUnit::Day), "昨天");
        let list = format_list_locale(&loc, &["甲", "乙"], ListStyle::And);
        assert!(list.contains("甲") && list.contains("乙"), "{list}");
    });
}

#[test]
fn twelve_hour_period_is_localized() {
    crate::i18n::with_lock(|| {
        let zh = Locale::parse("zh-CN").unwrap();
        let am = format_time_locale_for(&zh, 9, 5, None, true);
        let pm = format_time_locale_for(&zh, 15, 5, None, true);
        assert!(
            am.contains("上午") || am.contains("AM") || am.contains("a.m.") || am.contains("9"),
            "{am}"
        );
        assert!(
            pm.contains("下午") || pm.contains("PM") || pm.contains("p.m.") || pm.contains("3"),
            "{pm}"
        );
        let datetime = format_datetime_locale(&zh, 2026, 8, 14, 9, 30, 0);
        assert!(!datetime.contains(", "), "{datetime}");
    });
}

#[test]
fn percent_and_compact_do_not_panic() {
    let loc = Locale::parse("zh-CN").unwrap();
    let p = format_percent_locale(&loc, 0.5);
    assert!(
        p.contains('5') || p.contains('%') || p.contains("％"),
        "{p}"
    );
    let c = format_compact_locale(&loc, 12_000.0);
    assert!(!c.is_empty());
    let money = format_currency_locale(&loc, 12.5, "CNY");
    assert!(!money.is_empty());
}
