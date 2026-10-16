//! ICU4X compiled-data constructors, optional blob overlay, locale conversion.

use std::sync::{Mutex, OnceLock};

use icu::locale::Locale as IcuLocale;
use writeable::Writeable;

use super::locale_id::Locale;

static BLOB: OnceLock<Mutex<Option<icu_provider_blob::BlobDataProvider>>> = OnceLock::new();

fn blob_slot() -> &'static Mutex<Option<icu_provider_blob::BlobDataProvider>> {
    BLOB.get_or_init(|| Mutex::new(None))
}

/// Install a runtime ICU4X data blob (`icu4x-datagen --format blob`).
/// Subsequent formatters prefer this provider over compiled data.
pub fn load_blob(bytes: impl Into<Box<[u8]>>) -> Result<(), String> {
    let provider = icu_provider_blob::BlobDataProvider::try_new_from_blob(bytes.into())
        .map_err(|e| e.to_string())?;
    *blob_slot().lock().unwrap_or_else(|e| e.into_inner()) = Some(provider);
    super::runtime::bump();
    Ok(())
}

fn with_blob<T>(f: impl FnOnce(&icu_provider_blob::BlobDataProvider) -> Option<T>) -> Option<T> {
    let g = blob_slot().lock().unwrap_or_else(|e| e.into_inner());
    f(g.as_ref()?)
}

pub fn icu_locale(loc: &Locale) -> IcuLocale {
    let tag = if loc.is_pseudo() {
        loc.language().to_string()
    } else {
        loc.tag()
    };
    tag.parse::<IcuLocale>()
        .unwrap_or_else(|_| icu::locale::locale!("en").into())
}

pub fn writeable_to_string(w: impl Writeable) -> String {
    w.write_to_string().into_owned()
}

fn decimal_from_f64(value: f64, fraction_digits: u8) -> Option<icu::decimal::input::Decimal> {
    use icu::decimal::input::Decimal;
    if !value.is_finite() {
        return None;
    }
    let s = if fraction_digits == 0 {
        format!("{:.0}", value)
    } else {
        format!("{value:.prec$}", prec = fraction_digits as usize)
    };
    s.parse::<Decimal>().ok()
}

/// 0 = Sunday … 6 = Saturday. Prefers CLDR-ish region rules (ICU week data when available).
pub fn first_weekday(locale: &Locale) -> u32 {
    if let Some(icu) = first_weekday_icu(locale) {
        return icu;
    }
    first_weekday_table(locale)
}

fn first_weekday_icu(locale: &Locale) -> Option<u32> {
    let info = icu::calendar::week::WeekInformation::try_new(icu_locale(locale).into()).ok()?;
    Some(match info.first_weekday {
        icu::calendar::types::Weekday::Sunday => 0,
        icu::calendar::types::Weekday::Monday => 1,
        icu::calendar::types::Weekday::Tuesday => 2,
        icu::calendar::types::Weekday::Wednesday => 3,
        icu::calendar::types::Weekday::Thursday => 4,
        icu::calendar::types::Weekday::Friday => 5,
        icu::calendar::types::Weekday::Saturday => 6,
    })
}

fn first_weekday_table(locale: &Locale) -> u32 {
    if let Some(r) = locale.region() {
        match r {
            "US" | "CA" | "JP" | "KR" | "IL" | "BR" | "MX" | "TW" | "HK" | "MO" | "PH" | "TH"
            | "ZA" | "AU" => return 0,
            "AE" | "SA" | "EG" | "IQ" | "JO" | "KW" | "QA" | "IR" | "DZ" | "MA" | "TN" | "LY" => {
                return 6
            }
            _ => {}
        }
    }
    match locale.language() {
        "ja" | "ko" => 0,
        "ar" | "fa" => 6,
        "en" if locale.region().is_none() => 0,
        _ => 1,
    }
}

pub fn format_decimal(locale: &Locale, value: f64, fraction_digits: Option<u8>) -> Option<String> {
    use icu::decimal::DecimalFormatter;
    let prefs = icu_locale(locale).into();
    let fmt = with_blob(|p| {
        DecimalFormatter::try_new_with_buffer_provider(p, prefs, Default::default()).ok()
    })
    .or_else(|| DecimalFormatter::try_new(prefs, Default::default()).ok())?;
    let digits = fraction_digits.unwrap_or(if value.fract() == 0.0 { 0 } else { 2 });
    let dec = decimal_from_f64(value, digits)?;
    Some(writeable_to_string(fmt.format(&dec)))
}

pub fn format_percent(locale: &Locale, ratio: f64) -> Option<String> {
    use icu::experimental::dimension::percent::formatter::PercentFormatter;
    let prefs = icu_locale(locale).into();
    let fmt = PercentFormatter::try_new(prefs, Default::default()).ok()?;
    let digits = if (ratio * 100.0).fract() == 0.0 { 0 } else { 1 };
    let dec = decimal_from_f64(ratio * 100.0, digits)?;
    Some(writeable_to_string(fmt.format(&dec)))
}

pub fn format_compact(locale: &Locale, value: f64) -> Option<String> {
    use icu::decimal::CompactDecimalFormatter;
    let prefs = icu_locale(locale).into();
    let fmt = CompactDecimalFormatter::try_new_short(prefs, Default::default()).ok()?;
    let digits = if value.abs() >= 1000.0 { 0 } else { 1 };
    let dec = decimal_from_f64(value, digits)?;
    Some(writeable_to_string(fmt.format(&dec)))
}

pub fn format_currency(locale: &Locale, amount: f64, currency_code: &str) -> Option<String> {
    use icu::experimental::dimension::currency::formatter::CurrencyFormatter;
    use icu::locale::preferences::extensions::unicode::keywords::CurrencyType;
    let prefs = icu_locale(locale).into();
    let code = CurrencyType::try_from_str(&currency_code.to_ascii_lowercase()).ok()?;
    let fmt = CurrencyFormatter::try_new_symbol(prefs, code, Default::default()).ok()?;
    let dec = decimal_from_f64(amount, 2)?;
    Some(writeable_to_string(fmt.format_fixed_decimal(&dec)))
}

pub fn format_ymd(locale: &Locale, year: i32, month: u32, day: u32, long: bool) -> Option<String> {
    use icu::calendar::Date;
    use icu::datetime::fieldsets::YMD;
    use icu::datetime::DateTimeFormatter;
    let date = Date::try_new_iso(year, month as u8, day as u8).ok()?;
    let prefs = icu_locale(locale).into();
    let set = if long { YMD::long() } else { YMD::medium() };
    let fmt = DateTimeFormatter::try_new(prefs, set).ok()?;
    Some(writeable_to_string(fmt.format(&date)))
}

pub fn format_year_month(locale: &Locale, year: i32, month: u32) -> Option<String> {
    use icu::calendar::Date;
    use icu::datetime::fieldsets::YM;
    use icu::datetime::DateTimeFormatter;
    let date = Date::try_new_iso(year, month as u8, 1).ok()?;
    let prefs = icu_locale(locale).into();
    let fmt = DateTimeFormatter::try_new(prefs, YM::long()).ok()?;
    Some(writeable_to_string(fmt.format(&date)))
}

pub fn format_hms(
    locale: &Locale,
    hour: u32,
    minute: u32,
    second: Option<u32>,
    hour12: bool,
) -> Option<String> {
    use icu::datetime::fieldsets::T;
    use icu::datetime::DateTimeFormatter;
    use icu::time::Time;
    let s = second.unwrap_or(0) as u8;
    let time = Time::try_new(hour as u8, minute as u8, s, 0).ok()?;
    let prefs = icu_locale(locale).into();
    let mut set = if second.is_some() {
        T::medium()
    } else {
        T::short()
    };
    if hour12 {
        set = set.with_time_precision(icu::datetime::options::TimePrecision::Minute);
    }
    let fmt = DateTimeFormatter::try_new(prefs, set).ok()?;
    Some(writeable_to_string(fmt.format(&time)))
}

pub fn format_ymd_hms(
    locale: &Locale,
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
) -> Option<String> {
    use icu::calendar::Date;
    use icu::datetime::fieldsets::YMDT;
    use icu::datetime::DateTimeFormatter;
    use icu::time::{DateTime, Time};
    let date = Date::try_new_iso(year, month as u8, day as u8).ok()?;
    let time = Time::try_new(hour as u8, minute as u8, second as u8, 0).ok()?;
    let dt = DateTime { date, time };
    let prefs = icu_locale(locale).into();
    let fmt = DateTimeFormatter::try_new(prefs, YMDT::medium()).ok()?;
    Some(writeable_to_string(fmt.format(&dt)))
}

pub fn format_list(locale: &Locale, items: &[&str], and: bool) -> Option<String> {
    use icu::list::options::ListFormatterOptions;
    use icu::list::ListFormatter;
    let prefs = icu_locale(locale).into();
    let fmt = if and {
        ListFormatter::try_new_and(prefs, ListFormatterOptions::default()).ok()?
    } else {
        ListFormatter::try_new_or(prefs, ListFormatterOptions::default()).ok()?
    };
    Some(writeable_to_string(fmt.format(items.iter().copied())))
}

pub fn language_display_name(locale: &Locale, of_language: &str) -> Option<String> {
    use icu::experimental::displaynames::{
        multi::LocaleDisplayNamesFormatter, DisplayNamesOptions,
    };
    let prefs = icu_locale(locale).into();
    let fmt = LocaleDisplayNamesFormatter::try_new(prefs, DisplayNamesOptions::default()).ok()?;
    let target: IcuLocale = of_language.parse().ok()?;
    Some(fmt.of(&target).into_owned())
}

pub fn plural_category(language_tag: &str, n: i64) -> Option<super::plural::PluralCategory> {
    use icu::plurals::{PluralOperands, PluralRules};
    let loc: IcuLocale = language_tag.parse().ok()?;
    let rules = PluralRules::try_new_cardinal((&loc).into()).ok()?;
    let cat = rules.category_for(PluralOperands::from(n.unsigned_abs()));
    Some(match cat {
        icu::plurals::PluralCategory::Zero => super::plural::PluralCategory::Zero,
        icu::plurals::PluralCategory::One => super::plural::PluralCategory::One,
        icu::plurals::PluralCategory::Two => super::plural::PluralCategory::Two,
        icu::plurals::PluralCategory::Few => super::plural::PluralCategory::Few,
        icu::plurals::PluralCategory::Many => super::plural::PluralCategory::Many,
        icu::plurals::PluralCategory::Other => super::plural::PluralCategory::Other,
    })
}
