use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::OnceLock;

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

fn family_present(db: &Database, name: &str) -> bool {
    db.faces()
        .any(|face| face.families.iter().any(|(f, _)| f == name))
}

fn first_present_family<'a>(db: &Database, names: &[&'a str]) -> Option<&'a str> {
    names.iter().copied().find(|n| family_present(db, n))
}

/// Built-in UI sans priority when `RUI_FONT_FAMILY` is unset: project → platform → CJK fallback.
const BUILT_IN_UI_SANS_CANDIDATES: &[&str] = &[
    "Inter",
    "Inter Variable",
    "Inter Display",
    "Segoe UI",
    "San Francisco",
    "Helvetica Neue",
    "Arial",
    "Microsoft YaHei UI",
    "Microsoft YaHei",
    "Malgun Gothic",
    "PingFang SC",
    "Hiragino Sans GB",
    "Apple SD Gothic Neo",
    "Leelawadee UI",
    "Noto Sans CJK SC",
    "Noto Sans CJK JP",
    "Noto Sans CJK KR",
    "Noto Sans CJK TC",
    "Source Han Sans SC",
    "WenQuanYi Micro Hei",
];

fn user_font_family_list() -> Option<String> {
    std::env::var("RUI_FONT_FAMILY")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn first_concrete_family_in_list(db: &Database, family_list: &str) -> Option<&'static str> {
    for raw_name in family_list.split(',') {
        let name = raw_name.trim().trim_matches(|c| c == '\'' || c == '"');
        if name.is_empty() {
            continue;
        }
        let lower = name.to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "sans-serif" | "sans" | "serif" | "monospace" | "mono" | "cursive" | "fantasy"
        ) {
            continue;
        }
        if family_present(db, name) {
            return Some(leak_family_name(name));
        }
    }
    None
}

/// CSS-style default UI sans stack: user config first, then project/platform faces, then CJK fallback.
///
/// Override the primary face(s) with `RUI_FONT_FAMILY` (comma-separated, CSS `font-family` syntax).
/// Faces are loaded from `RUI_FONT_DIR` / `assets/` / `fonts/` beside the executable.
pub fn default_ui_sans_family_list() -> &'static str {
    static LIST: OnceLock<String> = OnceLock::new();
    LIST.get_or_init(|| {
        let mut parts: Vec<String> = Vec::new();
        if let Some(user) = user_font_family_list() {
            for raw in user.split(',') {
                let name = raw.trim();
                if !name.is_empty() {
                    parts.push(name.to_string());
                }
            }
        }
        parts.extend(BUILT_IN_UI_SANS_CANDIDATES.iter().map(|s| (*s).to_string()));
        parts.push("sans-serif".to_string());
        parts.join(", ")
    })
}

/// First concrete sans-serif family for `fontdb` defaults and `Family::SansSerif` mapping.
pub fn resolved_default_sans_family(db: &Database) -> &'static str {
    if let Some(user) = user_font_family_list() {
        if let Some(name) = first_concrete_family_in_list(db, &user) {
            return name;
        }
    }
    first_present_family(db, BUILT_IN_UI_SANS_CANDIDATES).unwrap_or("Segoe UI")
}

fn resolved_noto_sans_cjk_family(db: &Database) -> Option<&'static str> {
    if let Some(locale) = cjk_locale_from_env() {
        let preferred = noto_sans_family_for_locale(locale);
        if family_present(db, preferred) {
            return Some(preferred);
        }
    }
    const CJK: &[&str] = &[
        "Noto Sans CJK SC",
        "Noto Sans CJK JP",
        "Noto Sans CJK KR",
        "Noto Sans CJK TC",
        "Source Han Sans SC",
        "WenQuanYi Micro Hei",
    ];
    first_present_family(db, CJK)
}

fn leak_family_name(name: &str) -> &'static str {
    thread_local! {
        static LEAK_CACHE: RefCell<HashMap<String, &'static str>> = RefCell::new(HashMap::new());
    }
    LEAK_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(&leaked) = cache.get(name) {
            return leaked;
        }
        let leaked: &'static str = Box::leak(name.to_string().into_boxed_str());
        cache.insert(name.to_string(), leaked);
        leaked
    })
}

fn resolved_default_serif_family(db: &Database) -> &'static str {
    if let Some(locale) = cjk_locale_from_env() {
        let preferred = noto_serif_family_for_locale(locale);
        if family_present(db, preferred) {
            return preferred;
        }
    }
    first_present_family(
        db,
        &[
            "Noto Serif CJK SC",
            "Noto Serif CJK JP",
            "Noto Serif CJK TC",
            "Noto Serif",
            "Times New Roman",
            "Georgia",
        ],
    )
    .unwrap_or("Times New Roman")
}

fn resolved_emoji_family(db: &Database) -> Option<&'static str> {
    // Prefer bundled Noto Color Emoji family names, then OS emoji faces.
    const CANDIDATES: &[&str] = &[
        "Noto Color Emoji",
        "Noto Color Emoji Windows Compatible",
        "NotoColorEmoji",
        "Segoe UI Emoji",
        "Apple Color Emoji",
    ];
    CANDIDATES
        .iter()
        .find(|&&name| family_present(db, name))
        .copied()
        .map(|v| v as _)
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

    if let Some(hit) = WEIGHT_NORMALIZE_CACHE
        .with(|cache| cache.borrow().get(&(family_h, requested_weight)).copied())
    {
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

    let effective = if weights.is_empty() || weights.binary_search(&requested_weight).is_ok() {
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
                let tie_break = if candidate < requested_weight {
                    1u8
                } else {
                    0u8
                };
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

#[derive(Clone, Copy)]
enum ResolvedFamilyCached {
    Named(&'static str),
    SansSerif,
    Serif,
    Monospace,
    Cursive,
    Fantasy,
}

thread_local! {
    static RESOLVE_FAMILY_CACHE: RefCell<HashMap<String, ResolvedFamilyCached>> = RefCell::new(HashMap::new());
}

/// Clears the process-local [`resolve_family`] cache (e.g. after bundled fonts load).
pub fn clear_resolve_family_cache() {
    RESOLVE_FAMILY_CACHE.with(|c| c.borrow_mut().clear());
}

fn family_name_present(db: &Database, name: &str) -> bool {
    db.faces()
        .any(|face| face.families.iter().any(|(f, _)| f == name))
}

/// True when the list names a concrete face (e.g. `"lucide"`), not only CSS generics.
fn is_specific_family_request(family_list: &str) -> bool {
    family_list.split(',').any(|raw| {
        let name = raw.trim().trim_matches(|c| c == '\'' || c == '"');
        if name.is_empty() {
            return false;
        }
        let lower = name.to_ascii_lowercase();
        !matches!(
            lower.as_str(),
            "sans-serif" | "sans" | "serif" | "monospace" | "mono" | "cursive" | "fantasy"
        )
    })
}

/// Parses a comma-separated font family list (e.g., "Noto Sans CJK SC, Segoe UI, sans-serif")
/// and returns the first available font family in the database.
///
/// If no font in the list is found, it returns `Family::SansSerif` by default.
pub fn resolve_family<'a>(db: &Database, family_list: &'a str) -> Family<'a> {
    let cached = RESOLVE_FAMILY_CACHE.with(|c| c.borrow().get(family_list).copied());
    if let Some(entry) = cached {
        match entry {
            ResolvedFamilyCached::Named(s) if family_name_present(db, s) => {
                return Family::Name(s);
            }
            ResolvedFamilyCached::SansSerif if !is_specific_family_request(family_list) => {
                return Family::SansSerif;
            }
            ResolvedFamilyCached::Serif if !is_specific_family_request(family_list) => {
                return Family::Serif;
            }
            ResolvedFamilyCached::Monospace if !is_specific_family_request(family_list) => {
                return Family::Monospace;
            }
            ResolvedFamilyCached::Cursive if !is_specific_family_request(family_list) => {
                return Family::Cursive;
            }
            ResolvedFamilyCached::Fantasy if !is_specific_family_request(family_list) => {
                return Family::Fantasy;
            }
            _ => {}
        }
    }

    let result = resolve_family_uncached(db, family_list);
    let should_cache =
        !matches!(result, Family::SansSerif) || !is_specific_family_request(family_list);
    if should_cache {
        let to_cache = match result {
            Family::Name(name) => {
                ResolvedFamilyCached::Named(Box::leak(name.to_string().into_boxed_str()))
            }
            Family::SansSerif => ResolvedFamilyCached::SansSerif,
            Family::Serif => ResolvedFamilyCached::Serif,
            Family::Monospace => ResolvedFamilyCached::Monospace,
            Family::Cursive => ResolvedFamilyCached::Cursive,
            Family::Fantasy => ResolvedFamilyCached::Fantasy,
        };
        RESOLVE_FAMILY_CACHE.with(|c| {
            let mut cache = c.borrow_mut();
            if cache.len() > 256 {
                cache.clear();
            }
            cache.insert(family_list.to_string(), to_cache);
        });
    }
    result
}

fn resolve_family_uncached<'a>(db: &Database, family_list: &'a str) -> Family<'a> {
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
                if db
                    .faces()
                    .any(|face| face.families.iter().any(|(f, _)| f == mapped))
                {
                    return Family::Name(mapped);
                }
                return Family::SansSerif;
            }
            "serif" => {
                let mapped = resolved_default_serif_family(db);
                if db
                    .faces()
                    .any(|face| face.families.iter().any(|(f, _)| f == mapped))
                {
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
            "noto sans cjk" => resolved_noto_sans_cjk_family(db),
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
            if db
                .faces()
                .any(|face| face.families.iter().any(|(f, _)| f == mapped))
            {
                return Family::Name(mapped);
            }
        }

        // Query the database to see if this specific face exists by checking families directly.
        if db
            .faces()
            .any(|face| face.families.iter().any(|(f, _)| f == name))
        {
            return Family::Name(name);
        }
    }

    Family::SansSerif
}

#[cfg(test)]
mod resolve_family_cache_tests {
    use super::*;
    use glyphon::fontdb::Database;

    use crate::renderer::font_loader::create_font_system;

    #[test]
    fn lucide_resolves_after_font_load_despite_early_miss_cache() {
        clear_resolve_family_cache();
        let empty = Database::new();
        assert!(matches!(
            resolve_family(&empty, "lucide"),
            Family::SansSerif
        ));

        let fs = create_font_system();
        assert!(matches!(
            resolve_family(fs.db(), "lucide"),
            Family::Name("lucide")
        ));
    }

    #[test]
    fn resolved_default_sans_respects_user_config_when_present() {
        clear_resolve_family_cache();
        std::env::set_var("RUI_FONT_FAMILY", "Segoe UI");
        let fs = create_font_system();
        let sans = resolved_default_sans_family(fs.db());
        assert_eq!(sans, "Segoe UI");
        std::env::remove_var("RUI_FONT_FAMILY");
        clear_resolve_family_cache();
    }
}
