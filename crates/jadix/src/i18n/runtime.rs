//! Process-wide catalogs plus per-window locale / timezone.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{OnceLock, RwLock};

use super::catalog::{Catalog, MessageBundle};
use super::locale_id::Locale;
use super::message::{self, Arg};
use super::pseudo;

pub const EN_US: &str = "en-US";
pub const ZH_CN: &str = "zh-CN";

struct State {
    /// Default locale for windows that have not overridden.
    locale: Locale,
    fallback: Locale,
    timezone: Option<String>,
    window_locales: HashMap<u64, Locale>,
    window_timezones: HashMap<u64, String>,
    catalog: Catalog,
    defaults: Catalog,
    registered: HashSet<String>,
}

fn state() -> &'static RwLock<State> {
    static STATE: OnceLock<RwLock<State>> = OnceLock::new();
    STATE.get_or_init(|| {
        RwLock::new(State {
            locale: Locale::en(),
            fallback: Locale::en(),
            timezone: None,
            window_locales: HashMap::new(),
            window_timezones: HashMap::new(),
            catalog: Catalog::new(),
            defaults: Catalog::new(),
            registered: HashSet::new(),
        })
    })
}

fn read_state() -> std::sync::RwLockReadGuard<'static, State> {
    state().read().unwrap_or_else(|e| e.into_inner())
}

fn write_state() -> std::sync::RwLockWriteGuard<'static, State> {
    state().write().unwrap_or_else(|e| e.into_inner())
}

fn scope_token() -> u64 {
    crate::core::reactivity::signal_key_scope()
}

fn touch_generation_signal() {
    let g = generation();
    let sig = crate::core::reactivity::use_signal("__jadix_i18n_generation", || 0u64);
    let _ = sig.get();
    if sig.peek() != g {
        sig.set(g);
    }
}

static GENERATION: AtomicU64 = AtomicU64::new(0);
static NEED_REBUILD: AtomicBool = AtomicBool::new(false);

pub fn generation() -> u64 {
    GENERATION.load(Ordering::SeqCst)
}

pub(crate) fn bump() {
    GENERATION.fetch_add(1, Ordering::SeqCst);
    NEED_REBUILD.store(true, Ordering::SeqCst);
    crate::app::runtime::request_redraw_all();
    if crate::core::reactivity::runtime_tls_alive() {
        let g = generation();
        crate::core::reactivity::use_signal("__jadix_i18n_generation", || 0u64).set(g);
    }
}

pub(crate) fn take_locale_rebuild() -> bool {
    NEED_REBUILD.swap(false, Ordering::SeqCst)
}

pub fn current_locale() -> Locale {
    touch_generation_signal();
    let tok = scope_token();
    let g = read_state();
    g.window_locales
        .get(&tok)
        .cloned()
        .unwrap_or_else(|| g.locale.clone())
}

pub fn current_fallback() -> Locale {
    read_state().fallback.clone()
}

pub fn current_timezone() -> Option<String> {
    let tok = scope_token();
    let g = read_state();
    g.window_timezones
        .get(&tok)
        .cloned()
        .or_else(|| g.timezone.clone())
}

pub fn set_locale_inner(locale: Locale) {
    let tok = scope_token();
    let mut g = write_state();
    if tok == 0 {
        if g.locale != locale {
            g.locale = locale;
            drop(g);
            bump();
        }
    } else if g.window_locales.get(&tok) != Some(&locale) {
        g.window_locales.insert(tok, locale);
        drop(g);
        bump();
    }
}

pub fn set_global_locale_inner(locale: Locale) {
    let mut g = write_state();
    if g.locale != locale {
        g.locale = locale;
        drop(g);
        bump();
    }
}

pub fn set_fallback_inner(locale: Locale) {
    let mut g = write_state();
    if g.fallback != locale {
        g.fallback = locale;
        drop(g);
        bump();
    }
}

pub fn set_timezone_inner(tz: Option<String>) {
    let tok = scope_token();
    let mut g = write_state();
    if tok == 0 {
        if g.timezone != tz {
            g.timezone = tz;
            drop(g);
            bump();
        }
    } else {
        match tz {
            Some(s) if g.window_timezones.get(&tok) != Some(&s) => {
                g.window_timezones.insert(tok, s);
                drop(g);
                bump();
            }
            None if g.window_timezones.contains_key(&tok) => {
                g.window_timezones.remove(&tok);
                drop(g);
                bump();
            }
            _ => {}
        }
    }
}

pub fn add_bundle_inner(bundle: MessageBundle) {
    write_state().catalog.add(bundle);
    bump();
}

pub fn clear_overlays_inner() {
    write_state().catalog.clear();
    bump();
}

fn reg_stamp(locale: &str, key: &str) -> String {
    format!("{locale}\x1f{key}")
}

pub fn register_default(key: &str, default: &str) {
    let stamp = reg_stamp(EN_US, key);
    {
        let g = read_state();
        if g.registered.contains(&stamp) {
            return;
        }
    }
    let mut g = write_state();
    if g.registered.insert(stamp) {
        g.defaults.register_vacant(EN_US, key, default);
    }
}

pub fn register_pair(key: &str, en_us: &str, zh_cn: &str) {
    let a = reg_stamp(EN_US, key);
    let b = reg_stamp(ZH_CN, key);
    {
        let g = read_state();
        if g.registered.contains(&a) && g.registered.contains(&b) {
            return;
        }
    }
    let mut g = write_state();
    if g.registered.insert(a) {
        g.defaults.register_vacant(EN_US, key, en_us);
    }
    if g.registered.insert(b) {
        g.defaults.register_vacant(ZH_CN, key, zh_cn);
    }
}

pub fn register_defaults(pairs: &[(&str, &str)]) {
    for (k, v) in pairs {
        register_default(k, v);
    }
}

fn lookup_in(g: &State, loc: &Locale, key: &str) -> Option<String> {
    let lookup_loc = if loc.is_pseudo() {
        Locale::from(loc.language())
    } else {
        loc.clone()
    };
    if let Some(s) = g.catalog.get(&lookup_loc, key) {
        return Some(s.to_string());
    }
    if g.fallback != lookup_loc {
        if let Some(s) = g.catalog.get(&g.fallback, key) {
            return Some(s.to_string());
        }
    }
    g.defaults.get(&lookup_loc, key).map(str::to_string)
}

fn note_missing(locale: &Locale, key: &str) {
    #[cfg(debug_assertions)]
    {
        use std::sync::Mutex;
        static SEEN: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
        let stamp = format!("{}\x1f{key}", locale.tag());
        let mut g = SEEN
            .get_or_init(|| Mutex::new(HashSet::new()))
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if g.insert(stamp) {
            log::debug!("i18n: missing key `{key}` for {}", locale.tag());
        }
    }
    let _ = (locale, key);
}

fn apply_pseudo(loc: &Locale, text: String) -> String {
    match loc.region() {
        Some("XA") => pseudo::accent_xa(&text),
        Some("XB") => pseudo::wrap_xb(&text),
        _ => text,
    }
}

pub fn translate(key: &str, args: &[(&str, &str)]) -> String {
    let loc = current_locale();
    let template = {
        let g = read_state();
        lookup_in(&g, &loc, key)
    };
    match template {
        Some(t) => apply_pseudo(&loc, format_template(&loc, &t, args)),
        None => {
            note_missing(&loc, key);
            apply_pseudo(&loc, key.to_string())
        }
    }
}

pub fn translate_in(locale: &Locale, key: &str, args: &[Arg<'_>]) -> String {
    let lang = locale.language().to_string();
    let template = {
        let g = read_state();
        lookup_in(&g, locale, key)
    };
    match template {
        Some(t) => {
            let out = if args.is_empty() && !t.contains('{') {
                t
            } else {
                message::format_message(&t, args, &lang)
            };
            apply_pseudo(locale, out)
        }
        None => {
            note_missing(locale, key);
            apply_pseudo(locale, key.to_string())
        }
    }
}

fn format_template(locale: &Locale, template: &str, args: &[(&str, &str)]) -> String {
    if args.is_empty() && !template.contains('{') {
        return template.to_string();
    }
    let owned: Vec<Arg> = args.iter().map(|(n, v)| Arg::new(n, v)).collect();
    message::format_message(template, &owned, locale.language())
}

pub(super) fn interpolate_simple(template: &str, args: &[(&str, &str)]) -> String {
    crate::i18n_interpolate(template, args)
}

#[cfg(test)]
pub(crate) fn reset_for_test() {
    let mut g = write_state();
    g.locale = Locale::en();
    g.fallback = Locale::en();
    g.timezone = None;
    g.window_locales.clear();
    g.window_timezones.clear();
    g.catalog.clear();
    g.defaults.clear();
    g.registered.clear();
    drop(g);
    GENERATION.store(0, Ordering::SeqCst);
    NEED_REBUILD.store(false, Ordering::SeqCst);
}

#[cfg(test)]
pub(crate) fn with_lock<R>(body: impl FnOnce() -> R) -> R {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset_for_test();
    let out = body();
    reset_for_test();
    out
}
