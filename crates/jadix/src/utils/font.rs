use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use glyphon::fontdb::Database;
use glyphon::Family;

thread_local! {
    static LOCAL_MATCH_GEN: Cell<u64> = const { Cell::new(0) };

    /// (family_hash, requested_weight) -> attrs weight passed to cosmic-text
    static WEIGHT_NORMALIZE_CACHE: RefCell<HashMap<(u64, u16), ResolvedFaceWeight>> =
        RefCell::new(HashMap::new());

    /// family_hash -> discrete face weights + optional `wght` axis range
    static FAMILY_PROFILE_CACHE: RefCell<HashMap<u64, FamilyWeightProfile>> =
        RefCell::new(HashMap::new());
}

static FONT_MATCH_GEN: AtomicU64 = AtomicU64::new(1);

fn bump_font_match_caches() {
    FONT_MATCH_GEN.fetch_add(1, Ordering::Relaxed);
}

fn ensure_local_caches_current() {
    let gen = FONT_MATCH_GEN.load(Ordering::Relaxed);
    LOCAL_MATCH_GEN.with(|local| {
        if local.get() == gen {
            return;
        }
        WEIGHT_NORMALIZE_CACHE.with(|c| c.borrow_mut().clear());
        FAMILY_PROFILE_CACHE.with(|c| c.borrow_mut().clear());
        RESOLVE_FAMILY_CACHE.with(|c| c.borrow_mut().clear());
        local.set(gen);
    });
}

fn family_name_hash(name: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    name.hash(&mut h);
    h.finish()
}

/// Process-wide default type: family stack, size, weight, and `font-synthesis: weight`.
///
/// Override with [`set_text_defaults`], [`crate::app::WindowOptions`] (`default_font*`),
/// or env (`JADIX_FONT_FAMILY` / `JADIX_FONT_SIZE` / `JADIX_FONT_WEIGHT` / `JADIX_SYNTHETIC_BOLD`).
#[derive(Clone, Debug, PartialEq)]
pub struct TextDefaults {
    /// CSS `font-family` list. `None` = `JADIX_FONT_FAMILY` then the built-in UI stack.
    pub family: Option<String>,
    pub size_px: f32,
    pub weight: u16,
    /// When no Medium (or requested) face exists, thicken Regular (CSS `font-synthesis: weight`).
    pub synthetic_bold: bool,
}

impl Default for TextDefaults {
    fn default() -> Self {
        Self::from_env()
    }
}

impl TextDefaults {
    fn from_env() -> Self {
        let size_px = std::env::var("JADIX_FONT_SIZE")
            .ok()
            .and_then(|s| s.trim().parse::<f32>().ok())
            .filter(|v| v.is_finite() && *v > 0.0)
            .unwrap_or(16.0);
        let weight = std::env::var("JADIX_FONT_WEIGHT")
            .ok()
            .and_then(|s| s.trim().parse::<u16>().ok())
            .map(|w| w.clamp(100, 900))
            .unwrap_or(400);
        let synthetic_bold = match std::env::var("JADIX_SYNTHETIC_BOLD") {
            Ok(raw) => {
                let v = raw.trim().to_ascii_lowercase();
                !matches!(v.as_str(), "0" | "false" | "off" | "no")
            }
            Err(_) => true,
        };
        Self {
            family: None,
            size_px,
            weight,
            synthetic_bold,
        }
    }
}

static TEXT_DEFAULTS: Mutex<Option<TextDefaults>> = Mutex::new(None);
static FAMILY_LIST_CACHE: Mutex<FamilyListCache> = Mutex::new(FamilyListCache {
    sans: None,
    serif: None,
    mono: None,
});

struct FamilyListCache {
    sans: Option<&'static str>,
    serif: Option<&'static str>,
    mono: Option<&'static str>,
}

/// Current process-wide text defaults (initialized from env on first access).
pub fn text_defaults() -> TextDefaults {
    let mut g = TEXT_DEFAULTS.lock().unwrap_or_else(|e| e.into_inner());
    if g.is_none() {
        let d = TextDefaults::from_env();
        crate::layout::dimension::sync_root_font_size_px(d.size_px);
        *g = Some(d);
    }
    g.clone().unwrap_or_else(TextDefaults::from_env)
}

/// CSS root font-size (`1rem`), same as [`TextDefaults::size_px`].
#[inline]
pub fn root_font_size_px() -> f32 {
    crate::layout::dimension::css_root_font_size_px()
}

/// Set CSS root font-size (`1rem`) and default cascade text size together.
///
/// Rem lengths, utility spacing (`p-4` → `1rem`), and unspecified text size all follow this value.
pub fn set_root_font_size(px: f32) {
    let mut d = text_defaults();
    d.size_px = px.max(1.0);
    set_text_defaults(d);
}

/// Replace process-wide text defaults (family stack, size, weight, synthetic bold).
///
/// Call before the first layout, or from [`crate::app::WindowOptions`] at `run_with`.
/// Invalidates family / weight match caches. Also updates [`crate::layout::dimension::css_root_font_size_px`].
pub fn set_text_defaults(defaults: TextDefaults) {
    let mut defaults = defaults;
    defaults.size_px = defaults.size_px.max(1.0);
    defaults.weight = defaults.weight.clamp(100, 900);
    crate::layout::dimension::sync_root_font_size_px(defaults.size_px);
    *TEXT_DEFAULTS.lock().unwrap_or_else(|e| e.into_inner()) = Some(defaults);
    *FAMILY_LIST_CACHE.lock().unwrap_or_else(|e| e.into_inner()) = FamilyListCache {
        sans: None,
        serif: None,
        mono: None,
    };
    bump_font_match_caches();
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
    let raw = std::env::var("JADIX_TEXT_LOCALE").ok()?;
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

fn is_css_generic_family(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "sans-serif"
            | "sans"
            | "serif"
            | "monospace"
            | "mono"
            | "cursive"
            | "fantasy"
            | "system-ui"
            | "ui-sans-serif"
            | "ui-serif"
            | "ui-monospace"
            | "ui-rounded"
    )
}

/// Latin / platform UI faces. CJK must not win [`resolved_default_sans_family`] when these exist.
const BUILT_IN_UI_SANS_LATIN: &[&str] = &[
    "Inter",
    "Inter Variable",
    "Inter Display",
    "Segoe UI",
    "SF Pro Text",
    "SF Pro Display",
    "San Francisco",
    "Helvetica Neue",
    ".AppleSystemUIFont",
    "Arial",
];

/// CJK / locale UI faces — later CSS-stack fallbacks; lazy-loaded, not the chrome family.
const BUILT_IN_UI_SANS_CJK: &[&str] = &[
    "PingFang SC",
    "Hiragino Sans GB",
    "Microsoft YaHei UI",
    "Microsoft YaHei",
    "Malgun Gothic",
    "Apple SD Gothic Neo",
    "Leelawadee UI",
    "Noto Sans CJK SC",
    "Noto Sans CJK JP",
    "Noto Sans CJK KR",
    "Noto Sans CJK TC",
    "Source Han Sans SC",
    "WenQuanYi Micro Hei",
    "Noto Sans",
];

fn ui_sans_stack_candidates() -> Vec<&'static str> {
    let mut v = Vec::with_capacity(BUILT_IN_UI_SANS_LATIN.len() + BUILT_IN_UI_SANS_CJK.len());
    v.extend_from_slice(BUILT_IN_UI_SANS_LATIN);
    v.extend_from_slice(BUILT_IN_UI_SANS_CJK);
    v
}

const BUILT_IN_UI_SERIF_CANDIDATES: &[&str] = &[
    "Iowan Old Style",
    "Palatino Linotype",
    "Palatino",
    "Noto Serif CJK SC",
    "Noto Serif CJK JP",
    "Noto Serif CJK TC",
    "Noto Serif",
    "Liberation Serif",
    "DejaVu Serif",
    "Times New Roman",
    "Georgia",
    "Times",
];

const BUILT_IN_UI_MONO_CANDIDATES: &[&str] = &[
    "Cascadia Code",
    "Cascadia Mono",
    "Consolas",
    "SF Mono",
    "Menlo",
    "Monaco",
    "Courier New",
];

fn user_font_family_list() -> Option<String> {
    if let Some(family) = text_defaults().family {
        let t = family.trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
        return None;
    }
    std::env::var("JADIX_FONT_FAMILY")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn first_concrete_family_in_list(db: &Database, family_list: &str) -> Option<&'static str> {
    for raw_name in family_list.split(',') {
        let name = raw_name.trim().trim_matches(|c| c == '\'' || c == '"');
        if name.is_empty() || is_css_generic_family(name) {
            continue;
        }
        if family_present(db, name) {
            return Some(leak_family_name(name));
        }
    }
    None
}

fn join_css_stack(user: Option<String>, candidates: &[&str], generics: &[&str]) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(user) = user {
        for raw in user.split(',') {
            let name = raw.trim();
            if !name.is_empty() {
                parts.push(name.to_string());
            }
        }
    }
    for name in candidates {
        if !parts.iter().any(|p| p == name) {
            parts.push((*name).to_string());
        }
    }
    for g in generics {
        if !parts.iter().any(|p| p.eq_ignore_ascii_case(g)) {
            parts.push((*g).to_string());
        }
    }
    parts.join(", ")
}

fn intern_list(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

/// CSS-style default UI sans stack: user config first, then project/platform faces, then CJK.
///
/// Override the primary face(s) with `JADIX_FONT_FAMILY` / [`set_text_defaults`] /
/// [`crate::app::WindowOptions::default_font`].
pub fn default_ui_sans_family_list() -> &'static str {
    let mut cache = FAMILY_LIST_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(s) = cache.sans {
        return s;
    }
    let candidates = ui_sans_stack_candidates();
    let s = intern_list(join_css_stack(
        user_font_family_list(),
        &candidates,
        &["system-ui", "ui-sans-serif", "sans-serif"],
    ));
    cache.sans = Some(s);
    s
}

/// Default serif stack for `font-serif` / CSS `serif`.
pub fn default_ui_serif_family_list() -> &'static str {
    let mut cache = FAMILY_LIST_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(s) = cache.serif {
        return s;
    }
    let s = intern_list(join_css_stack(
        None,
        BUILT_IN_UI_SERIF_CANDIDATES,
        &["ui-serif", "serif"],
    ));
    cache.serif = Some(s);
    s
}

/// Default monospace stack for `font-mono` / CSS `monospace`.
pub fn default_ui_mono_family_list() -> &'static str {
    let mut cache = FAMILY_LIST_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(s) = cache.mono {
        return s;
    }
    let s = intern_list(join_css_stack(
        None,
        BUILT_IN_UI_MONO_CANDIDATES,
        &["ui-monospace", "monospace"],
    ));
    cache.mono = Some(s);
    s
}

/// First concrete sans-serif family for `fontdb` defaults and `Family::SansSerif` mapping.
///
/// Prefers Latin/platform UI faces so lazy CJK coverage cannot steal the chrome family.
pub fn resolved_default_sans_family(db: &Database) -> &'static str {
    if let Some(user) = user_font_family_list() {
        if let Some(name) = first_concrete_family_in_list(db, &user) {
            return name;
        }
    }
    if let Some(name) = first_present_family(db, BUILT_IN_UI_SANS_LATIN) {
        return name;
    }
    if let Some(name) = first_present_family(db, BUILT_IN_UI_SANS_CJK) {
        return name;
    }
    db.faces()
        .find_map(|face| {
            face.families.iter().find_map(|(f, _)| {
                if f.eq_ignore_ascii_case("lucide") {
                    None
                } else {
                    Some(leak_family_name(f))
                }
            })
        })
        .unwrap_or("Segoe UI")
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

pub fn resolved_default_serif_family(db: &Database) -> &'static str {
    if let Some(locale) = cjk_locale_from_env() {
        let preferred = noto_serif_family_for_locale(locale);
        if family_present(db, preferred) {
            return preferred;
        }
    }
    first_present_family(db, BUILT_IN_UI_SERIF_CANDIDATES).unwrap_or("Times New Roman")
}

/// First concrete monospace family for `fontdb` `Family::Monospace`.
pub fn resolved_default_mono_family(db: &Database) -> &'static str {
    first_present_family(
        db,
        &[
            "Cascadia Code",
            "Cascadia Mono",
            "Consolas",
            "SF Mono",
            "Menlo",
            "Monaco",
            "Courier New",
        ],
    )
    .unwrap_or("Consolas")
}

fn resolved_emoji_family(db: &Database) -> Option<&'static str> {
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

/// Result of CSS-like family + weight matching for one shaping request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolvedFaceWeight {
    /// Value passed to `Attrs::weight` (VF: requested; static: CSS-matched face).
    pub attrs_weight: u16,
    /// Family has a `wght` axis covering the request — cosmic interpolates.
    pub variable: bool,
    /// No matching bolder face; paint may thicken Regular (`font-synthesis: weight`).
    pub synthetic_bold: bool,
}

/// Coverage glyphon / Bake halo for [`ResolvedFaceWeight::synthetic_bold`] (logical px at scale 1).
pub const SYNTHETIC_BOLD_OFFSET_X: f32 = 0.18;
pub const SYNTHETIC_BOLD_OFFSET_Y: f32 = 0.0;
pub const SYNTHETIC_BOLD_BLUR: f32 = 0.32;

/// Same fake-bold stamp Coverage uses: extra 0-blur-ish shadow in text color.
pub fn synthetic_bold_shadow(color: [u8; 4], scale: f32) -> ([u8; 4], [f32; 2], f32) {
    (
        color,
        [
            SYNTHETIC_BOLD_OFFSET_X * scale,
            SYNTHETIC_BOLD_OFFSET_Y * scale,
        ],
        SYNTHETIC_BOLD_BLUR * scale,
    )
}

#[derive(Clone, Debug)]
struct FamilyWeightProfile {
    discrete: Vec<u16>,
    wght: Option<(f32, f32)>,
}

const WGHT_TAG: ttf_parser::Tag = ttf_parser::Tag::from_bytes(b"wght");

fn face_wght_range(db: &Database, id: glyphon::fontdb::ID) -> Option<(f32, f32)> {
    db.with_face_data(id, |data, index| {
        let face = ttf_parser::Face::parse(data, index).ok()?;
        let axis = face
            .variation_axes()
            .into_iter()
            .find(|a| a.tag == WGHT_TAG)?;
        Some((axis.min_value, axis.max_value))
    })
    .flatten()
}

fn family_weight_profile(db: &Database, name: &str) -> FamilyWeightProfile {
    let family_h = family_name_hash(name);
    ensure_local_caches_current();
    if let Some(hit) = FAMILY_PROFILE_CACHE.with(|c| c.borrow().get(&family_h).cloned()) {
        return hit;
    }

    let mut ids_weights: Vec<(glyphon::fontdb::ID, u16)> = Vec::new();
    for face in db.faces() {
        if face.families.iter().any(|(n, _)| *n == name) {
            ids_weights.push((face.id, face.weight.0));
        }
    }

    let mut discrete: Vec<u16> = ids_weights.iter().map(|(_, w)| *w).collect();
    discrete.sort_unstable();
    discrete.dedup();

    let mut wght_min = f32::MAX;
    let mut wght_max = f32::MIN;
    let mut found_vf = false;
    for (id, _) in ids_weights {
        if let Some((a, b)) = face_wght_range(db, id) {
            found_vf = true;
            wght_min = wght_min.min(a);
            wght_max = wght_max.max(b);
        }
    }

    let profile = FamilyWeightProfile {
        discrete,
        wght: found_vf.then_some((wght_min, wght_max)),
    };
    FAMILY_PROFILE_CACHE.with(|c| {
        c.borrow_mut().insert(family_h, profile.clone());
    });
    profile
}

/// CSS Fonts 4 font-style matching for the weight axis (discrete faces).
pub fn css_fonts4_match_weight(desired: u16, available: &[u16]) -> Option<u16> {
    if available.is_empty() {
        return None;
    }
    if available.binary_search(&desired).is_ok() {
        return Some(desired);
    }
    let desired_i = desired as i32;
    if (400..=500).contains(&desired) {
        if let Some(w) = available
            .iter()
            .copied()
            .filter(|w| (*w as i32) >= desired_i && *w <= 500)
            .min()
        {
            return Some(w);
        }
        if let Some(w) = available
            .iter()
            .copied()
            .filter(|w| (*w as i32) < desired_i)
            .max()
        {
            return Some(w);
        }
        return available.iter().copied().filter(|w| *w > 500).min();
    }
    if desired < 400 {
        if let Some(w) = available
            .iter()
            .copied()
            .filter(|w| (*w as i32) <= desired_i)
            .max()
        {
            return Some(w);
        }
        return available
            .iter()
            .copied()
            .filter(|w| (*w as i32) > desired_i)
            .min();
    }
    if let Some(w) = available
        .iter()
        .copied()
        .filter(|w| (*w as i32) >= desired_i)
        .min()
    {
        return Some(w);
    }
    available
        .iter()
        .copied()
        .filter(|w| (*w as i32) < desired_i)
        .max()
}

fn resolve_face_weight_uncached(
    db: &Database,
    name: &str,
    requested_weight: u16,
) -> ResolvedFaceWeight {
    let profile = family_weight_profile(db, name);
    if let Some((min, max)) = profile.wght {
        let req = requested_weight as f32;
        if req >= min - 0.5 && req <= max + 0.5 {
            return ResolvedFaceWeight {
                attrs_weight: requested_weight,
                variable: true,
                synthetic_bold: false,
            };
        }
    }

    let matched =
        css_fonts4_match_weight(requested_weight, &profile.discrete).unwrap_or(requested_weight);

    let synth_enabled = text_defaults().synthetic_bold;
    let needs_synth = synth_enabled
        && requested_weight >= 500
        && matched < requested_weight
        && profile.wght.is_none();

    ResolvedFaceWeight {
        attrs_weight: matched,
        variable: false,
        synthetic_bold: needs_synth,
    }
}

/// CSS Fonts 4 + variable `wght` matching for `family` at `requested_weight`.
pub fn resolve_face_weight<'a>(
    db: &Database,
    family: Family<'a>,
    requested_weight: u16,
) -> ResolvedFaceWeight {
    let name = match family {
        Family::Name(n) => n,
        Family::SansSerif => resolved_default_sans_family(db),
        Family::Serif => resolved_default_serif_family(db),
        Family::Monospace => resolved_default_mono_family(db),
        Family::Cursive | Family::Fantasy => {
            return ResolvedFaceWeight {
                attrs_weight: requested_weight,
                variable: false,
                synthetic_bold: false,
            };
        }
    };

    if requested_weight == 0 || name.eq_ignore_ascii_case("lucide") {
        return ResolvedFaceWeight {
            attrs_weight: requested_weight,
            variable: false,
            synthetic_bold: false,
        };
    }

    ensure_local_caches_current();
    let family_h = family_name_hash(name);
    if let Some(hit) =
        WEIGHT_NORMALIZE_CACHE.with(|c| c.borrow().get(&(family_h, requested_weight)).copied())
    {
        return hit;
    }

    let resolved = resolve_face_weight_uncached(db, name, requested_weight);
    WEIGHT_NORMALIZE_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() > 4096 {
            cache.clear();
        }
        cache.insert((family_h, requested_weight), resolved);
    });
    resolved
}

/// Weight passed to `Attrs` after family matching (VF interpolates; static uses CSS Fonts 4).
///
/// Prefer [`resolve_face_weight`] when the caller needs the synthetic-bold flag.
pub fn normalize_weight_for_family<'a>(
    db: &Database,
    family: Family<'a>,
    requested_weight: u16,
) -> u16 {
    resolve_face_weight(db, family, requested_weight).attrs_weight
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

/// Clears the process-local [`resolve_family`] / weight caches (e.g. after bundled fonts load).
pub fn clear_resolve_family_cache() {
    bump_font_match_caches();
    ensure_local_caches_current();
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
        !is_css_generic_family(name)
    })
}

/// Parses a comma-separated font family list (e.g., "Noto Sans CJK SC, Segoe UI, sans-serif")
/// and returns the first available font family in the database.
///
/// If no font in the list is found, it returns `Family::SansSerif` by default.
pub fn resolve_family<'a>(db: &Database, family_list: &'a str) -> Family<'a> {
    ensure_local_caches_current();
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

fn map_generic_sans<'a>(db: &Database) -> Family<'a> {
    let mapped = resolved_default_sans_family(db);
    if db
        .faces()
        .any(|face| face.families.iter().any(|(f, _)| f == mapped))
    {
        return Family::Name(mapped);
    }
    Family::SansSerif
}

fn map_generic_serif<'a>(db: &Database) -> Family<'a> {
    let mapped = resolved_default_serif_family(db);
    if db
        .faces()
        .any(|face| face.families.iter().any(|(f, _)| f == mapped))
    {
        return Family::Name(mapped);
    }
    Family::Serif
}

fn resolve_family_uncached<'a>(db: &Database, family_list: &'a str) -> Family<'a> {
    for raw_name in family_list.split(',').map(|s| s.trim()) {
        let name = raw_name.trim_matches(|c| c == '\'' || c == '"');
        if name.is_empty() {
            continue;
        }

        let lower = name.to_ascii_lowercase();
        match lower.as_str() {
            "sans-serif" | "sans" | "system-ui" | "ui-sans-serif" | "ui-rounded" => {
                return map_generic_sans(db);
            }
            "serif" | "ui-serif" => {
                return map_generic_serif(db);
            }
            "monospace" | "mono" | "ui-monospace" => return Family::Monospace,
            "cursive" => return Family::Cursive,
            "fantasy" => return Family::Fantasy,
            _ => {}
        }

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
        std::env::set_var("JADIX_FONT_FAMILY", "Segoe UI");
        let fs = create_font_system();
        let sans = resolved_default_sans_family(fs.db());
        assert_eq!(sans, "Segoe UI");
        std::env::remove_var("JADIX_FONT_FAMILY");
        clear_resolve_family_cache();
    }

    #[test]
    fn serif_stack_does_not_collapse_to_sans_when_serif_face_loaded() {
        clear_resolve_family_cache();
        let fs = create_font_system();
        let db = fs.db();
        let sans = resolved_default_sans_family(db);
        let serif = resolved_default_serif_family(db);
        let serif_loaded = db
            .faces()
            .any(|face| face.families.iter().any(|(f, _)| f == serif));
        if !serif_loaded {
            return;
        }
        assert_ne!(
            sans, serif,
            "generic serif must resolve to a different family than UI sans (got {sans})"
        );
        match resolve_family(db, "Times New Roman, Georgia, serif") {
            Family::Name(name) => {
                assert_ne!(name, sans, "Times/Georgia stack must not land on {sans}");
            }
            Family::Serif => {
                assert_ne!(serif, sans);
            }
            other => panic!("expected named serif or Family::Serif, got {other:?}"),
        }
    }

    #[test]
    fn css_fonts4_weight_matching_examples() {
        let segoe = [400_u16, 600, 700];
        assert_eq!(css_fonts4_match_weight(400, &segoe), Some(400));
        assert_eq!(css_fonts4_match_weight(500, &segoe), Some(400));
        assert_eq!(css_fonts4_match_weight(600, &segoe), Some(600));
        assert_eq!(css_fonts4_match_weight(700, &segoe), Some(700));
        assert_eq!(css_fonts4_match_weight(800, &segoe), Some(700));
        assert_eq!(css_fonts4_match_weight(300, &segoe), Some(400));
        let only_reg = [400_u16];
        assert_eq!(css_fonts4_match_weight(500, &only_reg), Some(400));
        assert_eq!(css_fonts4_match_weight(700, &only_reg), Some(400));
        let gap = [400_u16, 700];
        assert_eq!(css_fonts4_match_weight(600, &gap), Some(700));
    }

    #[test]
    fn segoe_medium_uses_regular_plus_synthetic_bold() {
        let fs = create_font_system();
        let family = crate::renderer::text_hit::best_font_family(&fs);
        let resolved = resolve_face_weight(fs.db(), family, 500);
        if resolved.variable {
            assert_eq!(resolved.attrs_weight, 500);
            assert!(!resolved.synthetic_bold);
        } else {
            assert!(
                resolved.attrs_weight <= 500,
                "CSS match for 500 should not jump past Medium (got {})",
                resolved.attrs_weight
            );
            if resolved.attrs_weight < 500 {
                assert!(
                    resolved.synthetic_bold,
                    "no Medium face → font-synthesis: weight"
                );
            }
        }
        let bold = resolve_face_weight(fs.db(), family, 700);
        assert!(
            bold.attrs_weight >= 600,
            "Bold should land on a real bolder face when loaded (got {})",
            bold.attrs_weight
        );
    }

    #[test]
    fn text_defaults_size_matches_css_root_slot() {
        let d = text_defaults();
        assert!((d.size_px - root_font_size_px()).abs() < 1e-6);
        assert!((d.size_px - crate::layout::dimension::css_root_font_size_px()).abs() < 1e-6);
    }
}
