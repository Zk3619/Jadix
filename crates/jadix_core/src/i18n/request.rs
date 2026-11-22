//! Bounded, synchronous locale operations shared by script and native hosts.
use super::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum LocaleRequest {
    Snapshot,
    Configure { locale: Option<String>, fallback: Option<String>, timezone: Option<String> },
    Reset,
    Translate { key: String, #[serde(default)] args: BTreeMap<String, String> },
    Number { value: f64, digits: Option<u8> },
    Percent { value: f64 },
    Compact { value: f64 },
    Currency { value: f64, code: String },
    Date { year: i32, month: u32, day: u32, #[serde(default)] short: bool },
    Time { hour: u32, minute: u32, second: Option<u32>, #[serde(default)] hour12: bool },
    List { items: Vec<String> },
}

impl LocaleRequest {
    /// All configuration is validated before changing the current surface.
    pub fn execute(self, context: &LocaleContext) -> Result<serde_json::Value, String> {
        context.with(|| {
            use serde_json::json;
            let text = match self {
                Self::Snapshot => return Ok(snapshot()),
                Self::Configure { locale, fallback, timezone } => {
                    let parse = |value: Option<String>| -> Result<Option<Locale>, String> {
                        value.map(|s| {
                            if s.len() > 64 || s.contains('\0') { return Err("invalid locale".into()); }
                            Locale::parse(&s).ok_or_else(|| "invalid locale".into())
                        }).transpose()
                    };
                    let locale = parse(locale)?;
                    let fallback = parse(fallback)?;
                    if timezone.as_ref().is_some_and(|s| s.is_empty() || s.len() > 256 || s.contains('\0')) {
                        return Err("invalid timezone".into());
                    }
                    if let Some(locale) = locale { set_locale(locale); }
                    if let Some(fallback) = fallback { set_fallback(fallback); }
                    if let Some(timezone) = timezone { set_timezone(timezone); }
                    return Ok(snapshot());
                }
                Self::Reset => { context.reset_overrides(); return Ok(snapshot()); }
                Self::Translate { key, args } => {
                    if key.len() > 16 * 1024 || args.len() > 128
                        || args.iter().any(|(k,v)| k.len() > 256 || v.len() > 16 * 1024)
                        || args.iter().map(|(k,v)| k.len() + v.len()).sum::<usize>() > 64 * 1024 {
                        return Err("translation limit".into());
                    }
                    tr_with(&key, &args.iter().map(|(k,v)| (k.as_str(), v.as_str())).collect::<Vec<_>>())
                }
                Self::Number { value, digits } if value.is_finite() && digits.is_none_or(|d| d <= 20) => format_number(value, digits),
                Self::Percent { value } if value.is_finite() => format_percent(value),
                Self::Compact { value } if value.is_finite() => format_compact(value),
                Self::Currency { value, code } if value.is_finite() && code.len() == 3 && code.bytes().all(|c| c.is_ascii_uppercase()) => format_currency(value, &code),
                Self::Date { year, month, day, short } if valid_date(year, month, day) => format_date(year, month, day, if short { DateStyle::Short } else { DateStyle::Long }),
                Self::Time { hour, minute, second, hour12 } if hour < 24 && minute < 60 && second.is_none_or(|s| s < 60) => format_time(hour, minute, second, hour12),
                Self::List { items } if items.len() <= 256 && items.iter().map(String::len).sum::<usize>() <= 64 * 1024 => format_list(&items.iter().map(String::as_str).collect::<Vec<_>>(), ListStyle::And),
                _ => return Err("invalid locale format arguments".into()),
            };
            if text.len() > 1024 * 1024 { return Err("formatted message limit".into()); }
            Ok(json!(text))
        })
    }
}

fn snapshot() -> serde_json::Value {
    serde_json::json!({"locale":locale().tag(), "fallback":fallback().tag(), "timezone":timezone(),
        "direction":if writing_direction().is_rtl() { "rtl" } else { "ltr" },
        "availableLocales":available_locales(), "generation":generation().to_string()})
}

fn valid_date(year: i32, month: u32, day: u32) -> bool {
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month { 2 => if leap { 29 } else { 28 }, 4|6|9|11 => 30, 1|3|5|7|8|10|12 => 31, _ => 0 };
    (1..=9999).contains(&year) && day > 0 && day <= days
}

/// Load verified packaged locale files before application code executes.
/// Paths use `locales/<BCP47>.json` or `.toml`; no filesystem or network IO occurs.
pub fn load_packaged(context: &LocaleContext, resources: &BTreeMap<String, Vec<u8>>) -> Result<(), String> {
    let mut bundles = Vec::new();
    let mut total = 0usize;
    for (path, bytes) in resources {
        let Some(name) = path.strip_prefix("locales/") else { continue; };
        if name.contains('/') || name.contains('\\') { continue; }
        let Some((tag, extension)) = name.rsplit_once('.') else { continue; };
        if !matches!(extension, "json"|"toml") { continue; }
        total = total.checked_add(bytes.len()).ok_or("translation byte limit")?;
        if bundles.len() >= 128 || total > 4 * 1024 * 1024 || bytes.len() > 1024 * 1024 { return Err("translation byte limit".into()); }
        let locale = Locale::parse(tag).ok_or("invalid packaged locale")?;
        let source = std::str::from_utf8(bytes).map_err(|_| "invalid translation UTF-8")?;
        bundles.push(if extension == "json" { MessageBundle::from_json(locale, source)? } else { MessageBundle::from_toml(locale, source)? });
    }
    context.with(|| { for bundle in bundles { add_bundle(bundle); } });
    Ok(())
}
