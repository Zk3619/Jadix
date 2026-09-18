//! Internationalization: locale, catalogs, ICU4X format, TOML overlays, RTL.
//!
//! Compiled only with crate feature `i18n`. App chrome uses [`crate::tr_default`]
//! (English source only). RUI components use [`crate::tr_default_zh`] to ship
//! **en-US** + **zh-CN**. Without the feature both return the English source string.
//!
//! Core holds the **contract** only — no product chrome catalog (same rule as
//! `assets_and_icons_architecture.md`: core does not ship Lucide/Twemoji/copy).
//! Extra locales (ja-JP, fr, …) are app TOML overlays, not component arguments.
//!
//! Lookup: app overlay (active locale) → overlay fallback locale → component
//! `register` / `tr_default` / `tr_default_zh` (locale fallback chain, so `en`
//! matches `en-US`) → missing key returns the key (debug log + `en-XA`/`en-XB`
//! pseudo-locales).
//!
//! **TOML is the app-facing overlay** (same policy as `theme.toml` / `styles.toml`).
//! `i18n.toml` is watched for mtime like stylesheets. `timezone` in that file is
//! a separate axis — not a catalog message.
//!
//! Locale format (number grouping, month names, date order, currency, compact)
//! uses ICU4X compiled data, with an optional runtime blob via [`load_icu_blob`].

pub mod catalog;
pub mod direction;
pub mod format;
mod icu_data;
mod load;
#[path = "locale.rs"]
mod locale_id;
pub mod message;
pub mod plural;
mod pseudo;
mod runtime;
mod watch;

pub use catalog::{Catalog, MessageBundle};
pub use direction::WritingDirection;
pub use format::{
    format_compact_locale, format_currency_locale, format_date_locale, format_datetime_locale,
    format_list_locale, format_number_locale, format_percent_locale, format_relative_locale,
    format_time_locale, format_time_locale_for, format_year_month_locale, DateStyle, ListStyle,
    NumberStyle, RelativeUnit,
};
pub use icu_data::load_blob as load_icu_blob;
pub use load::{load, load_dir, load_str};
pub use locale_id::{system_locale, Locale};
pub use message::{format_message, Arg};
pub use plural::PluralCategory;
pub use runtime::{EN_US, ZH_CN};

/// `true` when catalog / locale overlay machinery is compiled in.
pub const fn i18n_enabled() -> bool {
    cfg!(feature = "i18n")
}

/// Active locale (`en` until [`set_locale`] / `i18n.toml`).
pub fn locale() -> Locale {
    runtime::current_locale()
}

/// Set the active locale. Triggers a UI rebuild when the tag actually changes.
pub fn set_locale(tag: impl Into<Locale>) {
    runtime::set_locale_inner(tag.into());
}

/// Overlay fallback locale (default `en`).
pub fn fallback() -> Locale {
    runtime::current_fallback()
}

/// Overlay fallback locale (default `en`). Used when the active locale misses a key.
pub fn set_fallback(tag: impl Into<Locale>) {
    runtime::set_fallback_inner(tag.into());
}

/// Process / window IANA timezone (`Asia/Shanghai`). Not part of the message catalog.
pub fn timezone() -> Option<String> {
    runtime::current_timezone()
}

/// Set the process (or current-window) timezone. Does not convert civil dates.
pub fn set_timezone(tz: impl Into<String>) {
    runtime::set_timezone_inner(Some(tz.into()));
}

/// Clear the process / window timezone override.
pub fn clear_timezone() {
    runtime::set_timezone_inner(None);
}

/// Detect the OS locale and [`set_locale`] it.
pub fn use_system_locale() {
    if let Some(loc) = system_locale() {
        set_locale(loc);
    }
}

/// Merge a bundle into the process overlay catalog.
pub fn add_bundle(bundle: MessageBundle) {
    runtime::add_bundle_inner(bundle);
}

/// Parse TOML and [`add_bundle`]. Nested tables flatten with `.`.
pub fn add_toml(locale: impl Into<Locale>, src: &str) -> Result<(), String> {
    add_bundle(MessageBundle::from_toml(locale, src)?);
    Ok(())
}

/// Parse JSON and [`add_bundle`]. Nested objects flatten with `.`.
pub fn add_json(locale: impl Into<Locale>, src: &str) -> Result<(), String> {
    add_bundle(MessageBundle::from_json(locale, src)?);
    Ok(())
}

/// Drop app overlay bundles. Component-registered defaults are kept.
pub fn clear_bundles() {
    runtime::clear_overlays_inner();
}

/// Register an English (`en-US`) component source string. First registration wins;
/// overlays still override.
pub fn register(key: &str, default: &str) {
    runtime::register_default(key, default);
}

/// Register `en-US` and `zh-CN` source strings. First registration per locale wins.
pub fn register_locales(key: &str, en_us: &str, zh_cn: &str) {
    runtime::register_pair(key, en_us, zh_cn);
}

/// Register many English (`en-US`) component source strings.
pub fn register_many(pairs: &[(&str, &str)]) {
    runtime::register_defaults(pairs);
}

/// Translate `key`. Missing keys return the key itself.
pub fn tr(key: &str) -> String {
    runtime::translate(key, &[])
}

/// Translate with named `{placeholder}` / plural / select args.
pub fn tr_with(key: &str, args: &[(&str, &str)]) -> String {
    runtime::translate(key, args)
}

/// Translate using `{n, plural, …}` with integer `n`.
pub fn tr_count(key: &str, n: i64) -> String {
    let s = n.to_string();
    tr_with(key, &[("n", &s), ("count", &s)])
}

/// App / custom chrome: register an English (`en-US`) source string, then look up.
pub fn tr_default(key: &str, en_us: &str) -> String {
    register(key, en_us);
    let found = tr(key);
    if found == key {
        en_us.to_string()
    } else {
        found
    }
}

/// RUI component chrome: register `en-US` and `zh-CN`, then look up.
pub fn tr_default_zh(key: &str, en_us: &str, zh_cn: &str) -> String {
    register_locales(key, en_us, zh_cn);
    let found = tr(key);
    if found == key {
        en_us.to_string()
    } else {
        found
    }
}

/// [`tr_default`] plus named args.
pub fn tr_with_default(key: &str, en_us: &str, args: &[(&str, &str)]) -> String {
    register(key, en_us);
    let found = tr_with(key, args);
    if found == key {
        runtime::interpolate_simple(en_us, args)
    } else {
        found
    }
}

/// [`tr_default_zh`] plus named args.
pub fn tr_with_default_zh(key: &str, en_us: &str, zh_cn: &str, args: &[(&str, &str)]) -> String {
    register_locales(key, en_us, zh_cn);
    let found = tr_with(key, args);
    if found == key {
        runtime::interpolate_simple(en_us, args)
    } else {
        found
    }
}

/// [`tr_default`] with `{n, plural}` integer `n`.
pub fn tr_count_default(key: &str, en_us: &str, n: i64) -> String {
    let s = n.to_string();
    tr_with_default(key, en_us, &[("n", &s), ("count", &s)])
}

/// [`tr_default_zh`] with `{n, plural}` integer `n`.
pub fn tr_count_default_zh(key: &str, en_us: &str, zh_cn: &str, n: i64) -> String {
    let s = n.to_string();
    tr_with_default_zh(key, en_us, zh_cn, &[("n", &s), ("count", &s)])
}

/// `ToString` helper for the [`t!`](crate::t) macro.
pub fn arg(v: impl ToString) -> String {
    v.to_string()
}

/// Current writing direction from the active locale.
pub fn writing_direction() -> WritingDirection {
    locale().direction()
}

/// `true` when chrome should flip inline start/end (Arabic, Hebrew, …).
pub fn chrome_rtl() -> bool {
    writing_direction().is_rtl()
}

/// First day of the week: 0 = Sunday … 6 = Saturday (CLDR / ICU).
pub fn first_weekday() -> u32 {
    icu_data::first_weekday(&locale())
}

/// Language display name in the active locale (`ja` → `日本語` when locale is `ja`).
pub fn language_display_name(of: &str) -> String {
    icu_data::language_display_name(&locale(), of).unwrap_or_else(|| of.to_string())
}

/// Monotonic counter; increments when locale or catalogs change.
pub fn generation() -> u64 {
    runtime::generation()
}

pub(crate) fn take_locale_rebuild() -> bool {
    runtime::take_locale_rebuild()
}

/// Format a number in the active locale.
pub fn format_number(value: f64, fraction_digits: Option<u8>) -> String {
    format_number_locale(&locale(), value, fraction_digits)
}

pub fn format_percent(ratio: f64) -> String {
    format_percent_locale(&locale(), ratio)
}

pub fn format_compact(value: f64) -> String {
    format_compact_locale(&locale(), value)
}

pub fn format_currency(amount: f64, currency_code: &str) -> String {
    format_currency_locale(&locale(), amount, currency_code)
}

pub fn format_date(year: i32, month: u32, day: u32, style: DateStyle) -> String {
    format_date_locale(&locale(), year, month, day, style)
}

pub fn format_year_month(year: i32, month: u32) -> String {
    format_year_month_locale(&locale(), year, month)
}

pub fn format_datetime(
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
) -> String {
    format_datetime_locale(&locale(), year, month, day, hour, minute, second)
}

pub fn format_time(hour: u32, minute: u32, second: Option<u32>, hour12: bool) -> String {
    format_time_locale(hour, minute, second, hour12)
}

pub fn format_list(items: &[&str], style: ListStyle) -> String {
    format_list_locale(&locale(), items, style)
}

pub fn format_relative(amount: i64, unit: RelativeUnit) -> String {
    format_relative_locale(&locale(), amount, unit)
}

#[cfg(test)]
pub(crate) fn with_lock<R>(body: impl FnOnce() -> R) -> R {
    runtime::with_lock(body)
}

#[cfg(test)]
mod tests {
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
                tr_default("rui.pagination.previous", "Previous"),
                "Previous"
            );
            assert_eq!(tr("missing.key"), "missing.key");
            set_locale("zh-CN");
            assert_eq!(
                tr_default_zh("rui.pagination.next", "Next", "下一页"),
                "下一页"
            );
        });
    }

    #[test]
    fn component_zh_cn_without_overlay() {
        with_i18n(|| {
            register_locales("rui.pagination.previous", "Previous", "上一页");
            assert_eq!(tr("rui.pagination.previous"), "Previous");
            set_locale("zh-CN");
            assert_eq!(tr("rui.pagination.previous"), "上一页");
            set_locale("en-US");
            assert_eq!(tr("rui.pagination.previous"), "Previous");
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
                "rui.date_picker.week_of",
                "Week of {date}",
                &[("date", "August 14, 2026")],
            );
            assert_eq!(s, "Week of August 14, 2026");
            set_locale("zh-CN");
            let zh = tr_with_default_zh(
                "rui.date_picker.week_of",
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
            register("rui.pagination.previous", "Previous");
            add_toml(
                "zh-CN",
                r#"
[rui.pagination]
previous = "上一页"
"#,
            )
            .unwrap();
            set_locale("zh-CN");
            assert_eq!(tr("rui.pagination.previous"), "上一页");
            assert_eq!(
                tr_default_zh("rui.pagination.previous", "Previous", "上一页"),
                "上一页"
            );
        });
    }

    #[test]
    fn clear_bundles_keeps_register() {
        with_i18n(|| {
            register("rui.pagination.next", "Next");
            add_toml(
                "en",
                r#"
[rui.pagination]
next = "Forward"
"#,
            )
            .unwrap();
            assert_eq!(tr("rui.pagination.next"), "Forward");
            clear_bundles();
            assert_eq!(tr("rui.pagination.next"), "Next");
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
}
