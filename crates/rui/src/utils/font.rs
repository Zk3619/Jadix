use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use glyphon::fontdb::Database;
use glyphon::Family;

thread_local! {
    /// (family_hash, requested_weight) -> effective_weight
    static WEIGHT_NORMALIZE_CACHE: RefCell<HashMap<(u64, u16), u16>> = RefCell::new(HashMap::new());

    /// family_hash -> sorted unique face weights reported by fontdb
    static FAMILY_FACE_WEIGHTS_CACHE: RefCell<HashMap<u64, Vec<u16>>> = RefCell::new(HashMap::new());
}

fn family_name_hash(name: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    name.hash(&mut h);
    h.finish()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CjkLocale {
    Sc,
    Tc,
    Hk,
    Jp,
    Kr,
}

fn cjk_locale_from_env() -> Option<CjkLocale> {
    let raw = std::env::var("RUI_TEXT_LOCALE").ok()?;
    let v = raw.trim().to_ascii_lowercase();
    match v.as_str() {
        "zh" | "zh-cn" | "zh_hans" | "hans" | "sc" => Some(CjkLocale::Sc),
        "zh-tw" | "zh_hant" | "hant" | "tc" => Some(CjkLocale::Tc),
        "zh-hk" | "hk" => Some(CjkLocale::Hk),
        "ja" | "jp" => Some(CjkLocale::Jp),
        "ko" | "kr" => Some(CjkLocale::Kr),
        _ => None,
    }
}

fn noto_sans_family_for_locale(locale: CjkLocale) -> &'static str {
    match locale {
        CjkLocale::Sc => "Noto Sans CJK SC",
        CjkLocale::Tc => "Noto Sans CJK TC",
        CjkLocale::Hk => "Noto Sans CJK HK",
        CjkLocale::Jp => "Noto Sans CJK JP",
        CjkLocale::Kr => "Noto Sans CJK KR",
    }
}

fn noto_serif_family_for_locale(locale: CjkLocale) -> &'static str {
    match locale {
        CjkLocale::Sc => "Noto Serif CJK SC",
        CjkLocale::Tc => "Noto Serif CJK TC",
        CjkLocale::Hk => "Noto Serif CJK HK",
        CjkLocale::Jp => "Noto Serif CJK JP",
        CjkLocale::Kr => "Noto Serif CJK KR",
    }
}

fn resolved_default_sans_family(db: &Database) -> &'static str {
    if let Some(locale) = cjk_locale_from_env() {
        let preferred = noto_sans_family_for_locale(locale);
        if db.faces().any(|face| face.families.iter().any(|(f, _)| f == preferred)) {
            return preferred;
        }
    }
    "Noto Sans CJK SC"
}

fn resolved_default_serif_family(db: &Database) -> &'static str {
    if let Some(locale) = cjk_locale_from_env() {
        let preferred = noto_serif_family_for_locale(locale);
        if db.faces().any(|face| face.families.iter().any(|(f, _)| f == preferred)) {
            return preferred;
        }
    }
    "Noto Serif CJK SC"
}

fn resolved_emoji_family(db: &Database) -> Option<&'static str> {
    // Prefer bundled Noto Color Emoji family names.
    const CANDIDATES: &[&str] = &[
        "Noto Color Emoji",
        "Noto Color Emoji Windows Compatible",
        "NotoColorEmoji",
    ];
    for &name in CANDIDATES {
        if db.faces().any(|face| face.families.iter().any(|(f, _)| f == name)) {
            return Some(name);
        }
    }
    // Fallback: pick any family containing "emoji" and "noto".
    None
}

/// 将 `requested_weight` 归一到同一字体家族可用的“最接近权重”，以避免
/// `fontdb` 在找不到精确权重时跨家族回退（典型问题：`Segoe UI` 的 500 回退到 `Dubai`）。
pub fn normalize_weight_for_family<'a>(
    db: &Database,
    family: Family<'a>,
    requested_weight: u16,
) -> u16 {
    let name = match family {
        Family::Name(n) => n,
        // Generic families map to our renderer defaults.
        Family::SansSerif => resolved_default_sans_family(db),
        Family::Serif => resolved_default_serif_family(db),
        // For other generics, keep original requested weight.
        Family::Monospace | Family::Cursive | Family::Fantasy => {
            return requested_weight;
        }
    };

    if requested_weight == 0 {
        return requested_weight;
    }

    let family_h = family_name_hash(name);

    if let Some(hit) = WEIGHT_NORMALIZE_CACHE.with(|cache| cache.borrow().get(&(family_h, requested_weight)).copied()) {
        return hit;
    }

    let weights = FAMILY_FACE_WEIGHTS_CACHE.with(|cache| {
        if let Some(v) = cache.borrow().get(&family_h).cloned() {
            return v;
        }

        let mut seen = Vec::new();
        for face in db.faces() {
            if face.families.iter().any(|(n, _)| *n == name) {
                seen.push(face.weight.0);
            }
        }
        seen.sort_unstable();
        seen.dedup();
        let out = seen;

        cache.borrow_mut().insert(family_h, out.clone());
        out
    });

    let effective = if weights.is_empty() {
        requested_weight
    } else if weights.binary_search(&requested_weight).is_ok() {
        requested_weight
    } else {
        // Pick the nearest available face weight within the same family.
        *weights
            .iter()
            .min_by_key(|w| {
                let candidate = **w;
                let dist = (candidate as i32 - requested_weight as i32).abs();
                // When distances are equal (e.g. 600 between 500/700), prefer the heavier one
                // so UI weights remain visually distinguishable.
                let tie_break = if candidate < requested_weight { 1u8 } else { 0u8 };
                (dist, tie_break)
            })
            .unwrap_or(&requested_weight)
    };

    WEIGHT_NORMALIZE_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() > 4096 {
            cache.clear();
        }
        cache.insert((family_h, requested_weight), effective);
    });

    effective
}

/// Parses a comma-separated font family list (e.g., "Noto Sans CJK SC, Segoe UI, sans-serif")
/// and returns the first available font family in the database.
///
/// If no font in the list is found, it returns `Family::SansSerif` by default.
pub fn resolve_family<'a>(db: &Database, family_list: &'a str) -> Family<'a> {
    #[cfg(debug_assertions)]
    if std::env::var("RUI_TEXT_DEBUG").is_ok() {
        // Only print once or twice to avoid spam
    }

    for raw_name in family_list.split(',').map(|s| s.trim()) {
        let name = raw_name.trim_matches(|c| c == '\'' || c == '"');
        if name.is_empty() {
            continue;
        }

        // Handle standard generic families.
        let lower = name.to_ascii_lowercase();
        match lower.as_str() {
            "sans-serif" | "sans" => {
                let mapped = resolved_default_sans_family(db);
                if db.faces().any(|face| face.families.iter().any(|(f, _)| f == mapped)) {
                    return Family::Name(mapped);
                }
                return Family::SansSerif;
            }
            "serif" => {
                let mapped = resolved_default_serif_family(db);
                if db.faces().any(|face| face.families.iter().any(|(f, _)| f == mapped)) {
                    return Family::Name(mapped);
                }
                return Family::Serif;
            }
            "monospace" | "mono" => return Family::Monospace,
            "cursive" => return Family::Cursive,
            "fantasy" => return Family::Fantasy,
            _ => {}
        }

        // Locale-aware aliases within Noto CJK families.
        let alias = match lower.as_str() {
            "noto sans cjk" => Some(resolved_default_sans_family(db)),
            "noto serif cjk" => Some(resolved_default_serif_family(db)),
            "noto emoji" | "noto color emoji" | "emoji" => resolved_emoji_family(db),
            "noto sans cjk sc" => Some("Noto Sans CJK SC"),
            "noto sans cjk tc" => Some("Noto Sans CJK TC"),
            "noto sans cjk hk" => Some("Noto Sans CJK HK"),
            "noto sans cjk jp" => Some("Noto Sans CJK JP"),
            "noto sans cjk kr" => Some("Noto Sans CJK KR"),
            "noto serif sc" => Some("Noto Serif CJK SC"),
            "noto serif tc" => Some("Noto Serif CJK TC"),
            "noto serif hk" => Some("Noto Serif CJK HK"),
            "noto serif jp" => Some("Noto Serif CJK JP"),
            "noto serif kr" => Some("Noto Serif CJK KR"),
            _ => None,
        };
        if let Some(mapped) = alias {
            if db.faces().any(|face| face.families.iter().any(|(f, _)| f == mapped)) {
                return Family::Name(mapped);
            }
        }

        // Query the database to see if this specific face exists by checking families directly.
        if db.faces().any(|face| face.families.iter().any(|(f, _)| f == name)) {
            return Family::Name(name);
        }
    }

    Family::SansSerif
}
