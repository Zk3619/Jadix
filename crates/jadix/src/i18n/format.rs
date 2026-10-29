//! Locale-aware number / date / list / relative-time formatting.
//!
//! Prefers ICU4X compiled (or blob) data; falls back to compact tables so chrome
//! still works if a constructor fails. Timezone is not read here.

use super::locale_id::Locale;
use super::message::{format_message, Arg};
use super::runtime;

/// Number grouping / decimal separators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumberStyle {
    /// `1,234.56`
    Western,
    /// `1.234,56`
    European,
    /// `1 234,56`
    French,
}

impl NumberStyle {
    pub fn for_locale(locale: &Locale) -> Self {
        match locale.language() {
            "fr" | "ru" | "uk" | "be" | "sv" | "nb" | "nn" | "fi" => Self::French,
            "de" | "es" | "it" | "nl" | "pt" | "pl" | "cs" | "sk" | "hu" | "tr" | "da" => {
                if locale.region() == Some("CH") {
                    Self::Western
                } else {
                    Self::European
                }
            }
            _ => Self::Western,
        }
    }
}

/// Format a number for the current (or given) locale.
pub fn format_number_locale(locale: &Locale, value: f64, fraction_digits: Option<u8>) -> String {
    if let Some(s) = super::icu_data::format_decimal(locale, value, fraction_digits) {
        return s;
    }
    let style = NumberStyle::for_locale(locale);
    let (int_sep, frac_sep) = match style {
        NumberStyle::Western => (',', '.'),
        NumberStyle::European => ('.', ','),
        NumberStyle::French => ('\u{00a0}', ','),
    };
    let negative = value.is_sign_negative() && value != 0.0;
    let abs = value.abs();
    let digits = fraction_digits.unwrap_or(if abs.fract() == 0.0 { 0 } else { 2 });
    let factor = 10f64.powi(digits as i32);
    let rounded = (abs * factor).round() / factor;
    let int_part = rounded.trunc() as u64;
    let mut int_s = int_part.to_string();
    let mut grouped = String::new();
    for (i, c) in int_s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            grouped.push(int_sep);
        }
        grouped.push(c);
    }
    int_s = grouped.chars().rev().collect();
    let mut out = if negative { format!("-{int_s}") } else { int_s };
    if digits > 0 {
        let frac = (rounded.fract() * factor).round() as u64;
        out.push(frac_sep);
        out.push_str(&format!("{frac:0width$}", width = digits as usize));
    }
    out
}

pub fn format_percent_locale(locale: &Locale, ratio: f64) -> String {
    if let Some(s) = super::icu_data::format_percent(locale, ratio) {
        return s;
    }
    let n = format_number_locale(locale, ratio * 100.0, Some(0));
    match locale.language() {
        "tr" => format!("%{n}"),
        "fr" | "ru" | "uk" | "be" | "sv" | "nb" | "nn" | "fi" | "de" => format!("{n} %"),
        _ => format!("{n}%"),
    }
}

/// Compact decimal (`1.2K` / `1.2万`).
pub fn format_compact_locale(locale: &Locale, value: f64) -> String {
    if let Some(s) = super::icu_data::format_compact(locale, value) {
        return s;
    }
    format_number_locale(locale, value, None)
}

/// ISO 4217 currency amount (`USD 1,234.00` / `¥1,234.00`).
pub fn format_currency_locale(locale: &Locale, amount: f64, currency_code: &str) -> String {
    if let Some(s) = super::icu_data::format_currency(locale, amount, currency_code) {
        return s;
    }
    format!(
        "{} {}",
        currency_code,
        format_number_locale(locale, amount, Some(2))
    )
}

/// Date presentation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DateStyle {
    Short,
    #[default]
    Medium,
    Long,
    Full,
}

pub fn format_date_locale(
    locale: &Locale,
    year: i32,
    month: u32,
    day: u32,
    style: DateStyle,
) -> String {
    let long = matches!(style, DateStyle::Long | DateStyle::Full | DateStyle::Medium);
    if let Some(s) = super::icu_data::format_ymd(locale, year, month, day, long) {
        if !matches!(style, DateStyle::Full) {
            return s;
        }
        if let Some(w) = weekday_index(year, month, day) {
            return format!("{} {}", weekday_name_for(locale, w), s);
        }
        return s;
    }
    let lang = locale.language();
    let month_long = month_name_for(locale, month, MonthNameStyle::Long);
    let month_short = month_name_for(locale, month, MonthNameStyle::Short);
    let weekday = weekday_index(year, month, day).and_then(|w| Some(weekday_name_for(locale, w)));
    match (lang, style) {
        ("zh" | "ja", DateStyle::Short) => format!("{year}/{month}/{day}"),
        ("zh" | "ja", DateStyle::Medium | DateStyle::Long) => {
            format!("{year}年{month}月{day}日")
        }
        ("zh", DateStyle::Full) => {
            let wd = weekday.unwrap_or_default();
            format!("{year}年{month}月{day}日{wd}")
        }
        ("ja", DateStyle::Full) => {
            let wd = weekday.unwrap_or_default();
            format!("{year}年{month}月{day}日{wd}")
        }
        ("ko", DateStyle::Short) => format!("{year}. {month}. {day}."),
        ("ko", _) => format!("{year}년 {month}월 {day}일"),
        ("de", DateStyle::Short) => format!("{day:02}.{month:02}.{year}"),
        ("de", DateStyle::Medium) => format!("{day}. {month_short} {year}"),
        ("de", DateStyle::Long | DateStyle::Full) => format!("{day}. {month_long} {year}"),
        ("fr", DateStyle::Short) => format!("{day:02}/{month:02}/{year}"),
        ("fr", DateStyle::Medium) => format!("{day} {month_short} {year}"),
        ("fr", DateStyle::Long | DateStyle::Full) => format!("{day} {month_long} {year}"),
        (_, DateStyle::Short) => format!("{month}/{day}/{year}"),
        (_, DateStyle::Medium) => format!("{month_short} {day}, {year}"),
        (_, DateStyle::Long) => format!("{month_long} {day}, {year}"),
        (_, DateStyle::Full) => {
            let wd = weekday.unwrap_or_default();
            if wd.is_empty() {
                format!("{month_long} {day}, {year}")
            } else {
                format!("{wd}, {month_long} {day}, {year}")
            }
        }
    }
}

#[derive(Clone, Copy)]
enum MonthNameStyle {
    Long,
    Short,
}

const MONTH_LONG: [(&str, &str, &str); 12] = [
    ("jadix.calendar.month.january", "January", "一月"),
    ("jadix.calendar.month.february", "February", "二月"),
    ("jadix.calendar.month.march", "March", "三月"),
    ("jadix.calendar.month.april", "April", "四月"),
    ("jadix.calendar.month.may", "May", "五月"),
    ("jadix.calendar.month.june", "June", "六月"),
    ("jadix.calendar.month.july", "July", "七月"),
    ("jadix.calendar.month.august", "August", "八月"),
    ("jadix.calendar.month.september", "September", "九月"),
    ("jadix.calendar.month.october", "October", "十月"),
    ("jadix.calendar.month.november", "November", "十一月"),
    ("jadix.calendar.month.december", "December", "十二月"),
];

const MONTH_SHORT: [(&str, &str, &str); 12] = [
    ("jadix.calendar.month_short.jan", "Jan", "1月"),
    ("jadix.calendar.month_short.feb", "Feb", "2月"),
    ("jadix.calendar.month_short.mar", "Mar", "3月"),
    ("jadix.calendar.month_short.apr", "Apr", "4月"),
    ("jadix.calendar.month_short.may", "May", "5月"),
    ("jadix.calendar.month_short.jun", "Jun", "6月"),
    ("jadix.calendar.month_short.jul", "Jul", "7月"),
    ("jadix.calendar.month_short.aug", "Aug", "8月"),
    ("jadix.calendar.month_short.sep", "Sep", "9月"),
    ("jadix.calendar.month_short.oct", "Oct", "10月"),
    ("jadix.calendar.month_short.nov", "Nov", "11月"),
    ("jadix.calendar.month_short.dec", "Dec", "12月"),
];

const WEEKDAY_LONG: [(&str, &str, &str); 7] = [
    ("jadix.calendar.weekday.sun", "Sunday", "星期日"),
    ("jadix.calendar.weekday.mon", "Monday", "星期一"),
    ("jadix.calendar.weekday.tue", "Tuesday", "星期二"),
    ("jadix.calendar.weekday.wed", "Wednesday", "星期三"),
    ("jadix.calendar.weekday.thu", "Thursday", "星期四"),
    ("jadix.calendar.weekday.fri", "Friday", "星期五"),
    ("jadix.calendar.weekday.sat", "Saturday", "星期六"),
];

fn named(triple: (&str, &str, &str), locale: &Locale) -> String {
    runtime::register_pair(triple.0, triple.1, triple.2);
    let t = runtime::translate_in(locale, triple.0, &[]);
    if t == triple.0 {
        triple.1.to_string()
    } else {
        t
    }
}

fn month_name_for(locale: &Locale, month: u32, style: MonthNameStyle) -> String {
    let idx = month.saturating_sub(1).min(11) as usize;
    match style {
        MonthNameStyle::Long => named(MONTH_LONG[idx], locale),
        MonthNameStyle::Short => named(MONTH_SHORT[idx], locale),
    }
}

fn weekday_name_for(locale: &Locale, weekday: u32) -> String {
    let idx = (weekday as usize) % 7;
    named(WEEKDAY_LONG[idx], locale)
}

fn tr_in(locale: &Locale, key: &str, en_us: &str, zh_cn: &str) -> String {
    runtime::register_pair(key, en_us, zh_cn);
    let t = runtime::translate_in(locale, key, &[]);
    if t == key {
        en_us.to_string()
    } else {
        t
    }
}

/// Sakamoto: 0 = Sunday.
fn weekday_index(y: i32, m: u32, d: u32) -> Option<u32> {
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let mut y = y;
    let m = m as i32;
    let d = d as i32;
    let t = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    if m < 3 {
        y -= 1;
    }
    let w = (y + y / 4 - y / 100 + y / 400 + t[(m as usize) - 1] + d) % 7;
    Some(w.rem_euclid(7) as u32)
}

pub fn format_year_month_locale(locale: &Locale, year: i32, month: u32) -> String {
    if let Some(s) = super::icu_data::format_year_month(locale, year, month) {
        return s;
    }
    let month_long = month_name_for(locale, month, MonthNameStyle::Long);
    match locale.language() {
        "zh" | "ja" => format!("{year}年{month}月"),
        "ko" => format!("{year}년 {month}월"),
        _ => format!("{month_long} {year}"),
    }
}

fn day_period(locale: &Locale, am: bool) -> String {
    runtime::register_pair("jadix.time.am", "AM", "上午");
    runtime::register_pair("jadix.time.pm", "PM", "下午");
    let key = if am { "jadix.time.am" } else { "jadix.time.pm" };
    let t = runtime::translate_in(locale, key, &[]);
    if t == key {
        match locale.language() {
            "ja" => {
                if am {
                    "午前".into()
                } else {
                    "午後".into()
                }
            }
            "ko" => {
                if am {
                    "오전".into()
                } else {
                    "오후".into()
                }
            }
            _ => {
                if am {
                    "AM".into()
                } else {
                    "PM".into()
                }
            }
        }
    } else {
        t
    }
}

pub fn format_time_locale_for(
    locale: &Locale,
    hour: u32,
    minute: u32,
    second: Option<u32>,
    hour12: bool,
) -> String {
    if !hour12 {
        if let Some(s) = super::icu_data::format_hms(locale, hour, minute, second, false) {
            return s;
        }
        return match second {
            Some(s) => format!("{hour:02}:{minute:02}:{s:02}"),
            None => format!("{hour:02}:{minute:02}"),
        };
    }
    if let Some(s) = super::icu_data::format_hms(locale, hour, minute, second, true) {
        if has_day_period(&s) {
            return s;
        }
    }
    let am = hour < 12;
    let h12 = match hour {
        0 => 12,
        13..=23 => hour - 12,
        _ => hour,
    };
    let suffix = day_period(locale, am);
    match second {
        Some(s) => format!("{h12}:{minute:02}:{s:02} {suffix}"),
        None => format!("{h12}:{minute:02} {suffix}"),
    }
}

fn has_day_period(s: &str) -> bool {
    [
        "AM", "PM", "am", "pm", "a.m.", "p.m.", "上午", "下午", "午前", "午後", "오전", "오후",
    ]
    .iter()
    .any(|p| s.contains(p))
}

pub fn format_time_locale(hour: u32, minute: u32, second: Option<u32>, hour12: bool) -> String {
    format_time_locale_for(&runtime::current_locale(), hour, minute, second, hour12)
}

/// Date + time in one locale-typical string (no timezone conversion).
pub fn format_datetime_locale(
    locale: &Locale,
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
) -> String {
    if let Some(s) = super::icu_data::format_ymd_hms(locale, year, month, day, hour, minute, second)
    {
        return s;
    }
    let date = format_date_locale(locale, year, month, day, DateStyle::Long);
    let time = format_time_locale_for(locale, hour, minute, Some(second), false);
    match locale.language() {
        "zh" | "ja" | "ko" => format!("{date} {time}"),
        _ => format!("{date}, {time}"),
    }
}

/// Relative unit for [`format_relative_locale`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelativeUnit {
    Second,
    Minute,
    Hour,
    Day,
    Week,
    Month,
    Year,
}

pub fn format_relative_locale(locale: &Locale, amount: i64, unit: RelativeUnit) -> String {
    let n = amount.to_string();
    let (key, en_us, zh_cn) = match (amount, unit) {
        (-1, RelativeUnit::Day) => ("jadix.relative.yesterday", "yesterday", "昨天"),
        (1, RelativeUnit::Day) => ("jadix.relative.tomorrow", "tomorrow", "明天"),
        (0, RelativeUnit::Day) => ("jadix.relative.today", "today", "今天"),
        (n, RelativeUnit::Second) if n < 0 => (
            "jadix.relative.seconds_ago",
            "{n, plural, one {# second ago} other {# seconds ago}}",
            "{n, plural, other {# 秒前}}",
        ),
        (_, RelativeUnit::Second) => (
            "jadix.relative.seconds",
            "{n, plural, one {in # second} other {in # seconds}}",
            "{n, plural, other {# 秒后}}",
        ),
        (n, RelativeUnit::Minute) if n < 0 => (
            "jadix.relative.minutes_ago",
            "{n, plural, one {# minute ago} other {# minutes ago}}",
            "{n, plural, other {# 分钟前}}",
        ),
        (_, RelativeUnit::Minute) => (
            "jadix.relative.minutes",
            "{n, plural, one {in # minute} other {in # minutes}}",
            "{n, plural, other {# 分钟后}}",
        ),
        (n, RelativeUnit::Hour) if n < 0 => (
            "jadix.relative.hours_ago",
            "{n, plural, one {# hour ago} other {# hours ago}}",
            "{n, plural, other {# 小时前}}",
        ),
        (_, RelativeUnit::Hour) => (
            "jadix.relative.hours",
            "{n, plural, one {in # hour} other {in # hours}}",
            "{n, plural, other {# 小时后}}",
        ),
        (n, RelativeUnit::Day) if n < 0 => (
            "jadix.relative.days_ago",
            "{n, plural, one {# day ago} other {# days ago}}",
            "{n, plural, other {# 天前}}",
        ),
        (_, RelativeUnit::Day) => (
            "jadix.relative.days",
            "{n, plural, one {in # day} other {in # days}}",
            "{n, plural, other {# 天后}}",
        ),
        (n, RelativeUnit::Week) if n < 0 => (
            "jadix.relative.weeks_ago",
            "{n, plural, one {# week ago} other {# weeks ago}}",
            "{n, plural, other {# 周前}}",
        ),
        (_, RelativeUnit::Week) => (
            "jadix.relative.weeks",
            "{n, plural, one {in # week} other {in # weeks}}",
            "{n, plural, other {# 周后}}",
        ),
        (n, RelativeUnit::Month) if n < 0 => (
            "jadix.relative.months_ago",
            "{n, plural, one {# month ago} other {# months ago}}",
            "{n, plural, other {# 个月前}}",
        ),
        (_, RelativeUnit::Month) => (
            "jadix.relative.months",
            "{n, plural, one {in # month} other {in # months}}",
            "{n, plural, other {# 个月后}}",
        ),
        (n, RelativeUnit::Year) if n < 0 => (
            "jadix.relative.years_ago",
            "{n, plural, one {# year ago} other {# years ago}}",
            "{n, plural, other {# 年前}}",
        ),
        (_, RelativeUnit::Year) => (
            "jadix.relative.years",
            "{n, plural, one {in # year} other {in # years}}",
            "{n, plural, other {# 年后}}",
        ),
    };
    let abs = amount.abs().to_string();
    let tmpl = tr_in(locale, key, en_us, zh_cn);
    format_message(
        &tmpl,
        &[Arg::new("n", &abs), Arg::new("count", &n)],
        locale.language(),
    )
}

/// List conjunction: `a, b, and c`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ListStyle {
    #[default]
    And,
    Or,
}

pub fn format_list_locale(locale: &Locale, items: &[&str], style: ListStyle) -> String {
    if items.len() >= 2 {
        if let Some(s) =
            super::icu_data::format_list(locale, items, matches!(style, ListStyle::And))
        {
            return s;
        }
    }
    match items {
        [] => String::new(),
        [a] => (*a).to_string(),
        [a, b] => {
            let key = match style {
                ListStyle::And => "jadix.list.and_pair",
                ListStyle::Or => "jadix.list.or_pair",
            };
            let (en_us, zh_cn) = match style {
                ListStyle::And => ("{a} and {b}", "{a} 和 {b}"),
                ListStyle::Or => ("{a} or {b}", "{a} 或 {b}"),
            };
            format_message(
                &tr_in(locale, key, en_us, zh_cn),
                &[Arg::new("a", a), Arg::new("b", b)],
                locale.language(),
            )
        }
        rest => {
            let (sep_key, last_key, last_en, last_zh) = match style {
                ListStyle::And => (
                    "jadix.list.comma",
                    "jadix.list.and_last",
                    "{head}, and {last}",
                    "{head} 和 {last}",
                ),
                ListStyle::Or => (
                    "jadix.list.comma",
                    "jadix.list.or_last",
                    "{head}, or {last}",
                    "{head} 或 {last}",
                ),
            };
            let sep = tr_in(locale, sep_key, ", ", "、");
            let head = rest[..rest.len() - 1].join(&sep);
            let last = rest[rest.len() - 1];
            format_message(
                &tr_in(locale, last_key, last_en, last_zh),
                &[Arg::new("head", &head), Arg::new("last", last)],
                locale.language(),
            )
        }
    }
}

#[cfg(test)]
#[path = "tests/format.rs"]
mod tests;
