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
    window_fallbacks: HashMap<u64, Locale>,
    system_locales: HashMap<u64, Locale>,
    system_timezones: HashMap<u64, String>,
    surface_catalogs: HashMap<u64, Catalog>,
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
            window_fallbacks: HashMap::new(),
            system_locales: HashMap::new(),
            system_timezones: HashMap::new(),
            surface_catalogs: HashMap::new(),
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
    crate::reactivity::signal_key_scope()
}

fn touch_generation_signal() {
    let g = generation();
    let sig = crate::reactivity::use_signal("__jadix_i18n_generation", || 0u64);
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
    super::hooks::changed();
    if crate::reactivity::runtime_tls_alive() {
        let g = generation();
        crate::reactivity::use_signal("__jadix_i18n_generation", || 0u64).set(g);
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
        .or_else(|| g.system_locales.get(&tok))
        .cloned()
        .unwrap_or_else(|| g.locale.clone())
}

pub fn current_fallback() -> Locale {
    let g = read_state();
    g.window_fallbacks.get(&scope_token()).cloned().unwrap_or_else(|| g.fallback.clone())
}

pub fn current_timezone() -> Option<String> {
    let tok = scope_token();
    let g = read_state();
    g.window_timezones
        .get(&tok)
        .or_else(|| g.system_timezones.get(&tok))
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
    let token = scope_token();
    let changed = if token == 0 {
        if g.fallback == locale { false } else { g.fallback = locale; true }
    } else if g.window_fallbacks.get(&token) == Some(&locale) { false }
    else { g.window_fallbacks.insert(token, locale); true };
    drop(g);
    if changed { bump(); }
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
    let mut state = write_state();
    let token = scope_token();
    if token == 0 { state.catalog.add(bundle); }
    else { state.surface_catalogs.entry(token).or_default().add(bundle); }
    drop(state);
    bump();
}

pub fn clear_overlays_inner() {
    let mut state = write_state();
    let token = scope_token();
    if token == 0 { state.catalog.clear(); }
    else { state.surface_catalogs.remove(&token); }
    drop(state);
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
    let fallback = g.window_fallbacks.get(&scope_token()).unwrap_or(&g.fallback);
    let surface = g.surface_catalogs.get(&scope_token());
    for candidate in [&lookup_loc, fallback, &Locale::en()] {
        if let Some(s) = surface.and_then(|c| c.get_language(candidate, key))
            .or_else(|| g.catalog.get_language(candidate, key))
            .or_else(|| g.defaults.get_language(candidate, key)) {
            return Some(s.to_string());
        }
    }
    None
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
    message::format_message(template, &args.iter().map(|(n,v)| Arg::new(n,v)).collect::<Vec<_>>(), current_locale().language())
}

#[cfg(test)]
pub(crate) fn reset_for_test() {
    let mut g = write_state();
    g.locale = Locale::en();
    g.fallback = Locale::en();
    g.timezone = None;
    g.window_locales.clear();
    g.window_timezones.clear();
    g.window_fallbacks.clear();
    g.system_locales.clear();
    g.system_timezones.clear();
    g.surface_catalogs.clear();
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

/// Locale, catalog and timezone ownership for one surface, independent of its renderer.
#[derive(Debug)]
pub struct LocaleContext { token: u64, _ui: std::marker::PhantomData<std::rc::Rc<()>> }
impl Default for LocaleContext { fn default() -> Self { Self::new() } }
impl LocaleContext {
    pub fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1 << 63);
        let token = NEXT.try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .expect("locale context IDs exhausted");
        Self { token, _ui: std::marker::PhantomData }
    }
    /// Enter for translation, formatting and reactive tree construction. Panic-safe and nestable.
    pub fn with<R>(&self, body: impl FnOnce() -> R) -> R {
        struct Restore(u64);
        impl Drop for Restore { fn drop(&mut self) { crate::reactivity::set_signal_key_scope(self.0); } }
        let _restore = Restore(crate::reactivity::set_signal_key_scope(self.token));
        body()
    }
    /// Host preference updates retain explicit application overrides.
    pub fn update_environment(&self, locale: &str, timezone: Option<&str>) -> Result<(), String> {
        let locale = Locale::parse(locale).ok_or("invalid locale")?;
        let mut g = write_state();
        let timezone = timezone.map(str::to_owned);
        let changed = g.system_locales.get(&self.token) != Some(&locale)
            || g.system_timezones.get(&self.token) != timezone.as_ref();
        g.system_locales.insert(self.token, locale);
        if let Some(timezone) = timezone { g.system_timezones.insert(self.token, timezone); }
        else { g.system_timezones.remove(&self.token); }
        drop(g);
        if changed { self.with(bump); }
        Ok(())
    }
    pub fn reset_overrides(&self) {
        let mut g = write_state();
        let changed = g.window_locales.remove(&self.token).is_some()
            | g.window_timezones.remove(&self.token).is_some()
            | g.window_fallbacks.remove(&self.token).is_some();
        drop(g);
        if changed { self.with(bump); }
    }
    pub fn dispose(&self) {
        let mut g = write_state();
        g.window_locales.remove(&self.token);
        g.window_timezones.remove(&self.token);
        g.window_fallbacks.remove(&self.token);
        g.system_locales.remove(&self.token);
        g.system_timezones.remove(&self.token);
        g.surface_catalogs.remove(&self.token);
    }
}
impl Drop for LocaleContext { fn drop(&mut self) { self.dispose(); } }

pub fn available_locales() -> Vec<String> {
    let g = read_state();
    let mut locales = g.catalog.locales();
    if let Some(catalog) = g.surface_catalogs.get(&scope_token()) { locales.extend(catalog.locales()); }
    locales.extend(g.defaults.locales());
    locales.sort(); locales.dedup(); locales
}
