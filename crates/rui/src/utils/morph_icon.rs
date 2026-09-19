//! `<MorphIcon>` — any two stroke icons morph with spring physics.
//!
//! Gated on the `svg` feature: frames bake through resvg (same path as AnimatedIcon).

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use crate::core::tree::{Element, IntoElementList, LayoutStatus};
use crate::style::{GeneratorSample, Spring, SpringPhysics};
use crate::utils::icon_morph::{
    build_plan, canonical_d, fit_icon, icon_input_from_src, interp_plan_d_kind, morph_d_at_kind,
    resample_icon, sampled_from_outputs, IconInput, MorphPlan, Sampled, SAMPLE_N,
};

pub const ATTR_MORPH_ICON: &str = "rui_morph_icon";
pub const ATTR_MORPH_FROM: &str = "rui_morph_from";
pub const ATTR_MORPH_TO: &str = "rui_morph_to";
pub const ATTR_MORPH_PROGRESS: &str = "rui_morph_progress";
pub const ATTR_MORPH_SIZE: &str = "rui_morph_size";
pub const ATTR_MORPH_STROKE: &str = "rui_morph_stroke";
pub const ATTR_MORPH_SPRING: &str = "rui_morph_spring";
pub const ATTR_MORPH_INTERP: &str = "rui_morph_interp";
pub const ATTR_MORPH_REDUCED: &str = "rui_morph_reduced";
pub const ATTR_MORPH_VIEWBOX: &str = "rui_morph_viewbox";
pub const ATTR_MORPH_ABS_STROKE: &str = "rui_morph_abs_stroke";

/// Anything that can seed a MorphIcon: Lucide name, raw `d`, SVG markup, or [`IconInput`].
pub trait IntoMorphSrc {
    fn into_morph_src(self) -> String;
}

impl IntoMorphSrc for String {
    fn into_morph_src(self) -> String {
        self
    }
}

impl IntoMorphSrc for &str {
    fn into_morph_src(self) -> String {
        self.to_string()
    }
}

impl IntoMorphSrc for IconInput {
    fn into_morph_src(self) -> String {
        match self {
            IconInput::D(d) => d,
            nodes @ IconInput::Nodes(_) => canonical_d(&nodes).unwrap_or_default(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct SpringCfg {
    k: f32,
    c: f32,
}

fn spring_cfg(spec: &str) -> SpringCfg {
    let t = spec.trim();
    if t.eq_ignore_ascii_case("smooth") {
        return SpringCfg { k: 170.0, c: 26.0 };
    }
    if t.eq_ignore_ascii_case("bouncy") {
        return SpringCfg { k: 300.0, c: 14.0 };
    }
    if t.eq_ignore_ascii_case("snappy") || t.is_empty() {
        return SpringCfg { k: 420.0, c: 30.0 };
    }
    parse_spring_constants(t).unwrap_or(SpringCfg { k: 420.0, c: 30.0 })
}

/// `"420,30"` / `"k:420,c:30"` / `"stiffness:170,damping:26"`.
fn parse_spring_constants(s: &str) -> Option<SpringCfg> {
    let lower = s.to_ascii_lowercase().replace(' ', "");
    if lower.contains('k') || lower.contains("stiff") || lower.contains("damp") {
        let mut k = None;
        let mut c = None;
        for part in lower.split([',', ';']) {
            if part.is_empty() {
                continue;
            }
            if let Some(rest) = part
                .strip_prefix("k:")
                .or_else(|| part.strip_prefix("stiffness:"))
            {
                k = rest.parse().ok();
            } else if let Some(rest) = part
                .strip_prefix("c:")
                .or_else(|| part.strip_prefix("damping:"))
            {
                c = rest.parse().ok();
            }
        }
        return Some(SpringCfg { k: k?, c: c? });
    }
    let mut nums = s.split(',').filter_map(|p| p.trim().parse::<f32>().ok());
    Some(SpringCfg {
        k: nums.next()?,
        c: nums.next()?,
    })
}

fn spring_cfg_from_el(el: &Element) -> SpringCfg {
    spring_cfg(
        el.get_attr(ATTR_MORPH_SPRING)
            .map(|s| s.as_str())
            .unwrap_or("snappy"),
    )
}

fn uses_linear(el: &Element) -> bool {
    el.get_attr(ATTR_MORPH_INTERP)
        .map(|s| s.eq_ignore_ascii_case("linear"))
        .unwrap_or(false)
}

fn should_skip_spring(el: &Element) -> bool {
    match el
        .get_attr(ATTR_MORPH_REDUCED)
        .map(|s| s.as_str())
        .unwrap_or("never")
    {
        "always" => true,
        "user" => crate::style::prefers_reduced_motion(),
        _ => false,
    }
}

struct MorphPlay {
    from_key: String,
    to_key: String,
    plan: Option<MorphPlan>,
    live: Option<Vec<Sampled>>,
    rest_d: String,
    x: f32,
    v: f32,
    k: f32,
    c: f32,
    gen: Option<Spring>,
    gen_t_ms: f32,
    controlled: bool,
    settled: bool,
    /// morphicons `based`: mid-flight seek without re-basing onto `from`.
    based: bool,
    pair_from: Option<String>,
    pair_to: Option<String>,
    refs: u32,
}

fn new_play(
    from: String,
    to: String,
    rest_d: String,
    x: f32,
    cfg: SpringCfg,
    controlled: bool,
    settled: bool,
) -> MorphPlay {
    MorphPlay {
        from_key: from,
        to_key: to,
        plan: None,
        live: None,
        rest_d,
        x,
        v: 0.0,
        k: cfg.k,
        c: cfg.c,
        gen: None,
        gen_t_ms: 0.0,
        controlled,
        settled,
        based: false,
        pair_from: None,
        pair_to: None,
        refs: 0,
    }
}

fn invalidate_pair(play: &mut MorphPlay) {
    play.based = false;
    play.pair_from = None;
    play.pair_to = None;
}

fn play_store() -> &'static Mutex<HashMap<u64, MorphPlay>> {
    static S: OnceLock<Mutex<HashMap<u64, MorphPlay>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashMap::new()))
}

fn morph_template_id(id: &str) -> String {
    if id.starts_with("morph-icon:") {
        id.to_string()
    } else {
        format!("morph-icon:{id}")
    }
}

fn instance_key(el: &Element) -> u64 {
    key_for_template(el.template_id.as_deref().unwrap_or_else(|| {
        el.get_attr(ATTR_MORPH_ICON)
            .map(String::as_str)
            .unwrap_or("")
    }))
}

fn key_for_template(id: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    id.hash(&mut h);
    h.finish()
}

/// Playhead for pause/scrub. `id` is the builder/`<MorphIcon id>` value.
pub fn morph_progress_by_id(id: &str) -> f32 {
    let key = key_for_template(&morph_template_id(id));
    if let Ok(map) = play_store().lock() {
        if let Some(play) = map.get(&key) {
            return play.x.clamp(0.0, 1.0);
        }
    }
    1.0
}

pub(crate) fn hash_morph_icon_paint(el: &Element, h: &mut impl Hasher) {
    if el.get_attr(ATTR_MORPH_ICON).is_none() && el.get_attr(ATTR_MORPH_FROM).is_none() {
        return;
    }
    el.get_attr(ATTR_MORPH_FROM).hash(h);
    el.get_attr(ATTR_MORPH_TO).hash(h);
    el.get_attr(ATTR_MORPH_PROGRESS).hash(h);
    el.get_attr(ATTR_MORPH_STROKE).hash(h);
    el.get_attr(ATTR_MORPH_SPRING).hash(h);
    el.get_attr(ATTR_MORPH_INTERP).hash(h);
    el.get_attr(ATTR_MORPH_REDUCED).hash(h);
    el.get_attr(ATTR_MORPH_VIEWBOX).hash(h);
    el.get_attr(ATTR_MORPH_ABS_STROKE).hash(h);
    el.get_attr(ATTR_MORPH_SIZE).hash(h);
    if let Ok(map) = play_store().lock() {
        if let Some(play) = map.get(&instance_key(el)) {
            play.x.to_bits().hash(h);
            play.settled.hash(h);
        }
    }
}

fn lucide_seed(el: &mut Element, name: &str) {
    let glyph = crate::utils::lucide::lucide_glyph(name).unwrap_or_else(|| "\u{ea0e}".into());
    el.text = Some(glyph);
    el.set_attr(crate::utils::ATTR_ICON_NAME, name);
}

fn color_css(rgba: [u8; 4]) -> String {
    if rgba[3] >= 250 {
        format!("rgb({},{},{})", rgba[0], rgba[1], rgba[2])
    } else {
        format!(
            "rgba({},{},{},{})",
            rgba[0],
            rgba[1],
            rgba[2],
            rgba[3] as f32 / 255.0
        )
    }
}

fn emit_doc(d: &str, color: [u8; 4], stroke_width: f32) -> String {
    let stroke = color_css(color);
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="{stroke}" stroke-width="{stroke_width}" stroke-linecap="round" stroke-linejoin="round"><path d="{d}"/></svg>"#
    )
}

fn arm_play_gen(play: &mut MorphPlay) {
    play.gen = Some(Spring::from_physics(
        play.x,
        1.0,
        play.v,
        SpringPhysics {
            stiffness: play.k,
            damping: play.c,
            mass: 1.0,
        },
    ));
    play.gen_t_ms = 0.0;
}

fn spring_step(play: &mut MorphPlay, dt: f32) -> bool {
    if play.gen.is_none() {
        arm_play_gen(play);
    }
    play.gen_t_ms += dt.max(0.0) * 1000.0;
    let sample = play
        .gen
        .as_ref()
        .map(|s| s.next(play.gen_t_ms))
        .unwrap_or(GeneratorSample {
            value: 1.0,
            velocity: 0.0,
            done: true,
        });
    play.x = sample.value;
    play.v = sample.velocity;
    sample.done || ((1.0 - play.x).abs() < 0.001 && play.v.abs() < 0.02)
}

fn ensure_play(el: &Element) -> Option<u64> {
    if el.template_id.as_deref().is_some_and(|s| !s.is_empty())
        || el.get_attr(ATTR_MORPH_ICON).is_some()
    {
        Some(instance_key(el))
    } else {
        None
    }
}

fn play_exists(key: u64) -> bool {
    play_store()
        .lock()
        .ok()
        .is_some_and(|m| m.contains_key(&key))
}

fn release_play(key: u64) {
    let Ok(mut map) = play_store().lock() else {
        return;
    };
    let remove = match map.get_mut(&key) {
        Some(p) => {
            p.refs = p.refs.saturating_sub(1);
            p.refs == 0
        }
        None => false,
    };
    if remove {
        map.remove(&key);
    }
}

/// Unregister this instance (morphicons `destroy`). Later [`morph_to`] births a
/// fresh driver (first paint, no spring).
pub fn morph_destroy(el: &Element) {
    let key = instance_key(el);
    if let Ok(mut map) = play_store().lock() {
        map.remove(&key);
    }
}

fn resolve_input(el: &Element, src: &str) -> Option<IconInput> {
    let input = icon_input_from_src(src)?;
    let Some(vb) = el
        .get_attr(ATTR_MORPH_VIEWBOX)
        .filter(|s| !s.trim().is_empty())
    else {
        return Some(input);
    };
    fit_icon(&input, vb, 24.0).ok().map(IconInput::D)
}

fn rest_d_for(src: &str) -> String {
    icon_input_from_src(src)
        .and_then(|i| canonical_d(&i).ok())
        .unwrap_or_default()
}

fn rest_d_el(el: &Element, src: &str) -> String {
    resolve_input(el, src)
        .and_then(|i| canonical_d(&i).ok())
        .unwrap_or_default()
}

fn sampled_src(el: &Element, src: &str) -> Option<Vec<Sampled>> {
    resolve_input(el, src).and_then(|i| resample_icon(&i, SAMPLE_N).ok())
}

fn is_first_icon(el: &Element, key: u64) -> bool {
    !play_exists(key)
        || el
            .get_attr(ATTR_MORPH_ICON)
            .map(|s| s.trim().is_empty())
            .unwrap_or(true)
}

fn start_morph(el: &Element, play: &mut MorphPlay, from_src: &str, to_src: &str, cfg: SpringCfg) {
    let dest = match sampled_src(el, to_src).filter(|d| !d.is_empty()) {
        Some(d) => d,
        None => {
            play.to_key = to_src.to_string();
            play.rest_d = rest_d_el(el, to_src);
            play.settled = true;
            play.x = 1.0;
            play.v = 0.0;
            play.plan = None;
            play.live = None;
            return;
        }
    };
    let src_sampled = if let Some(live) = play.live.take() {
        live
    } else {
        sampled_src(el, from_src).unwrap_or_default()
    };
    if src_sampled.is_empty() {
        play.to_key = to_src.to_string();
        play.from_key = from_src.to_string();
        play.rest_d = rest_d_el(el, to_src);
        play.settled = true;
        play.x = 1.0;
        play.plan = None;
        return;
    }
    match build_plan(&src_sampled, &dest) {
        Ok(plan) => {
            play.plan = Some(plan);
            play.from_key = from_src.to_string();
            play.to_key = to_src.to_string();
            play.x = 0.0;
            play.v = play.v.clamp(-14.0, 14.0);
            play.k = cfg.k;
            play.c = cfg.c;
            play.settled = false;
            play.controlled = false;
            arm_play_gen(play);
            invalidate_pair(play);
        }
        Err(_) => {
            play.to_key = to_src.to_string();
            play.rest_d = rest_d_el(el, to_src);
            play.settled = true;
            play.x = 1.0;
            play.plan = None;
        }
    }
}

fn attr_src(el: &Element, key: &str) -> Option<String> {
    el.get_attr(key).cloned().filter(|s| !s.trim().is_empty())
}

/// morphicons: the pair owns the path while **both** `from` and `to` are present.
fn is_controlled(el: &Element) -> bool {
    attr_src(el, ATTR_MORPH_FROM).is_some() && attr_src(el, ATTR_MORPH_TO).is_some()
}

fn seed_icon(el: &mut Element, icon: &str) {
    el.set_attr(ATTR_MORPH_ICON, icon);
    lucide_seed(el, icon);
}

/// Jump to `icon` with no spring (morphicons `set`). Invalidates a frozen pair
/// so the next from/to bind re-bases; does not drop the pair attrs.
pub fn morph_set(el: &mut Element, icon: impl IntoMorphSrc) {
    let icon = icon.into_morph_src();
    morph_set_src(el, &icon);
}

fn morph_set_src(el: &mut Element, icon: &str) {
    let Some(key) = ensure_play(el) else {
        return;
    };
    let cfg = spring_cfg_from_el(el);
    seed_icon(el, icon);
    if !is_controlled(el) {
        el.set_attr(ATTR_MORPH_TO, icon);
    }
    if let Ok(mut map) = play_store().lock() {
        let play = map.entry(key).or_insert_with(|| {
            let mut p = new_play(
                icon.to_string(),
                icon.to_string(),
                rest_d_el(el, icon),
                1.0,
                cfg,
                false,
                true,
            );
            p.refs = 1;
            p
        });
        play.from_key = icon.to_string();
        play.to_key = icon.to_string();
        play.rest_d = rest_d_el(el, icon);
        play.x = 1.0;
        play.v = 0.0;
        play.k = cfg.k;
        play.c = cfg.c;
        play.controlled = false;
        play.settled = true;
        play.plan = None;
        play.live = None;
        invalidate_pair(play);
    }
    crate::app::runtime::request_redraw();
}

/// Begin a spring morph to `icon`. Reduced-motion `always` / (`user` + OS)
/// jumps via [`morph_set`]. Imperative call invalidates a frozen pair.
/// The first icon on a lazy mount paints without flying (morphicons lazy driver).
pub fn morph_to(el: &mut Element, icon: impl IntoMorphSrc) {
    let icon = icon.into_morph_src();
    morph_to_cfg(el, &icon, None);
}

/// morphicons `morphTo(icon, spring)`: this flight uses `spring` (preset or
/// `"k,c"`). The instance `spring` attr is unchanged.
pub fn morph_to_with(el: &mut Element, icon: impl IntoMorphSrc, spring: impl AsRef<str>) {
    let icon = icon.into_morph_src();
    morph_to_cfg(el, &icon, Some(spring_cfg(spring.as_ref())));
}

fn morph_to_src(el: &mut Element, icon: &str) {
    morph_to_cfg(el, icon, None);
}

fn morph_to_cfg(el: &mut Element, icon: &str, spring: Option<SpringCfg>) {
    let Some(key) = ensure_play(el) else {
        return;
    };
    if is_first_icon(el, key) || should_skip_spring(el) {
        morph_set_src(el, icon);
        return;
    }
    let cfg = spring.unwrap_or_else(|| spring_cfg_from_el(el));
    let prev = el
        .get_attr(ATTR_MORPH_TO)
        .cloned()
        .unwrap_or_else(|| el.get_attr(ATTR_MORPH_ICON).cloned().unwrap_or_default());
    seed_icon(el, icon);
    if !is_controlled(el) {
        el.set_attr(ATTR_MORPH_TO, icon);
    }
    if let Ok(mut map) = play_store().lock() {
        let play = map.entry(key).or_insert_with(|| {
            let mut p = new_play(
                prev.clone(),
                icon.to_string(),
                rest_d_el(el, &prev),
                1.0,
                cfg,
                false,
                true,
            );
            p.refs = 1;
            p
        });
        if play.to_key != icon || !play.settled {
            let from = if play.settled {
                play.to_key.clone()
            } else {
                play.from_key.clone()
            };
            start_morph(el, play, &from, icon, cfg);
        }
        play.controlled = false;
        invalidate_pair(play);
    }
    crate::app::runtime::request_redraw();
}

/// Frozen morph toward `icon` at `t` (morphicons `seek`). No spring; a later
/// [`morph_to`] takes off from this intermediate shape.
pub fn morph_seek(el: &mut Element, icon: impl IntoMorphSrc, t: f32) {
    let icon = icon.into_morph_src();
    morph_seek_src(el, &icon, t);
}

fn morph_seek_src(el: &mut Element, icon: &str, t: f32) {
    let t = t.clamp(0.0, 1.0);
    let Some(key) = ensure_play(el) else {
        return;
    };
    let cfg = spring_cfg_from_el(el);
    seed_icon(el, icon);
    if let Ok(mut map) = play_store().lock() {
        let play = map.entry(key).or_insert_with(|| {
            let mut p = new_play(
                icon.to_string(),
                icon.to_string(),
                rest_d_el(el, icon),
                1.0,
                cfg,
                true,
                true,
            );
            p.refs = 1;
            p
        });
        let reuse = !play.settled && play.plan.is_some() && play.to_key == icon;
        play.v = 0.0;
        if !reuse {
            let from = if play.settled {
                play.to_key.clone()
            } else {
                play.from_key.clone()
            };
            start_morph(el, play, &from, icon, cfg);
        }
        play.controlled = true;
        play.x = t;
        if t <= 0.0 {
            play.settled = true;
            play.rest_d = rest_d_el(el, &play.from_key);
            play.live = None;
            play.plan = None;
        } else if t >= 1.0 - 1e-3 {
            play.x = 1.0;
            play.settled = true;
            play.rest_d = rest_d_el(el, icon);
            play.live = None;
            play.plan = None;
        } else {
            play.settled = false;
            if let Some(plan) = play.plan.as_ref() {
                play.live = Some(sampled_from_outputs(plan, t as f64));
            }
        }
    }
    crate::app::runtime::request_redraw();
}

/// `progress = t` sugar: [`morph_seek`] on the active target.
pub fn morph_seek_progress(el: &mut Element, t: f32) {
    let target = el
        .get_attr(ATTR_MORPH_TO)
        .cloned()
        .or_else(|| el.get_attr(ATTR_MORPH_ICON).cloned())
        .unwrap_or_default();
    if target.is_empty() {
        return;
    }
    morph_seek_src(el, &target, t);
}

pub fn morph_progress(el: &Element) -> f32 {
    let key = instance_key(el);
    if let Ok(map) = play_store().lock() {
        if let Some(play) = map.get(&key) {
            return play.x.clamp(0.0, 1.0);
        }
    }
    el.get_attr(ATTR_MORPH_PROGRESS)
        .and_then(|s| s.parse().ok())
        .unwrap_or(1.0)
}

/// morphicons `MorphHandle`: `morphTo` / `set` / `seek` / `progress`.
pub struct MorphHandle<'a> {
    el: &'a mut Element,
}

pub fn morph_handle(el: &mut Element) -> MorphHandle<'_> {
    MorphHandle { el }
}

impl MorphHandle<'_> {
    pub fn morph_to(&mut self, icon: impl IntoMorphSrc) {
        morph_to(self.el, icon);
    }
    /// morphicons `morphTo(icon, spring)`.
    pub fn morph_to_with(&mut self, icon: impl IntoMorphSrc, spring: impl AsRef<str>) {
        morph_to_with(self.el, icon, spring);
    }
    pub fn set(&mut self, icon: impl IntoMorphSrc) {
        morph_set(self.el, icon);
    }
    pub fn seek(&mut self, icon: impl IntoMorphSrc, t: f32) {
        morph_seek(self.el, icon, t);
    }
    pub fn progress(&self) -> f32 {
        morph_progress(self.el)
    }
    pub fn set_progress(&mut self, t: f32) {
        morph_seek_progress(self.el, t);
    }
    pub fn destroy(&mut self) {
        morph_destroy(self.el);
    }
}

/// Solid-style `<MorphIcon icon={…}>`.
pub fn apply_bound_morph_icon(el: &mut Element, icon: impl AsRef<str>) -> LayoutStatus {
    let icon = icon.as_ref();
    let cur = el
        .get_attr(ATTR_MORPH_ICON)
        .map(|s| s.as_str())
        .unwrap_or("");
    if cur == icon {
        return LayoutStatus::Clean;
    }
    // Controlled wins: pair owns the path; `icon` is ignored until the pair drops.
    if is_controlled(el) {
        el.set_attr(ATTR_MORPH_ICON, icon);
        lucide_seed(el, icon);
        return LayoutStatus::Clean;
    }
    let Some(key) = ensure_play(el) else {
        morph_set(el, icon);
        return LayoutStatus::StyleChanged;
    };
    if is_first_icon(el, key) {
        morph_set(el, icon);
    } else {
        morph_to(el, icon);
    }
    LayoutStatus::StyleChanged
}

pub fn apply_bound_morph_from(el: &mut Element, src: impl AsRef<str>) -> LayoutStatus {
    let src = src.as_ref().trim();
    let cur = el
        .get_attr(ATTR_MORPH_FROM)
        .map(|s| s.as_str())
        .unwrap_or("");
    if cur == src {
        return LayoutStatus::Clean;
    }
    let was = is_controlled(el);
    if src.is_empty() {
        el.attrs.remove(ATTR_MORPH_FROM);
        if was && !is_controlled(el) {
            leave_controlled(el);
        }
        crate::app::runtime::request_redraw();
        return LayoutStatus::StyleChanged;
    }
    el.set_attr(ATTR_MORPH_FROM, src);
    if is_controlled(el) {
        apply_controlled_pair(el);
    }
    crate::app::runtime::request_redraw();
    LayoutStatus::StyleChanged
}

pub fn apply_bound_morph_to(el: &mut Element, src: impl AsRef<str>) -> LayoutStatus {
    let src = src.as_ref().trim();
    let cur = el.get_attr(ATTR_MORPH_TO).map(|s| s.as_str()).unwrap_or("");
    if cur == src {
        return LayoutStatus::Clean;
    }
    let was = is_controlled(el);
    if src.is_empty() {
        el.attrs.remove(ATTR_MORPH_TO);
        if was && !is_controlled(el) {
            leave_controlled(el);
        }
        crate::app::runtime::request_redraw();
        return LayoutStatus::StyleChanged;
    }
    el.set_attr(ATTR_MORPH_TO, src);
    if is_controlled(el) {
        apply_controlled_pair(el);
        crate::app::runtime::request_redraw();
        return LayoutStatus::StyleChanged;
    }
    morph_to(el, src);
    LayoutStatus::StyleChanged
}

pub fn apply_bound_morph_spring(el: &mut Element, name: impl AsRef<str>) -> LayoutStatus {
    let name = name.as_ref();
    let cur = el
        .get_attr(ATTR_MORPH_SPRING)
        .map(|s| s.as_str())
        .unwrap_or("");
    if cur == name {
        return LayoutStatus::Clean;
    }
    el.set_attr(ATTR_MORPH_SPRING, name);
    let cfg = spring_cfg(name);
    if let Some(key) = ensure_play(el) {
        if let Ok(mut map) = play_store().lock() {
            if let Some(play) = map.get_mut(&key) {
                play.k = cfg.k;
                play.c = cfg.c;
            }
        }
    }
    crate::app::runtime::request_redraw();
    LayoutStatus::StyleChanged
}

pub fn apply_bound_morph_stroke(el: &mut Element, width: f32) -> LayoutStatus {
    let w = width.max(0.25);
    let prev = el
        .get_attr(ATTR_MORPH_STROKE)
        .and_then(|s| s.parse::<f32>().ok())
        .unwrap_or(-1.0);
    if (prev - w).abs() < 1e-4 {
        return LayoutStatus::Clean;
    }
    el.set_attr(ATTR_MORPH_STROKE, format!("{w}"));
    crate::app::runtime::request_redraw();
    LayoutStatus::StyleChanged
}

pub fn apply_bound_morph_interp(el: &mut Element, kind: impl AsRef<str>) -> LayoutStatus {
    let kind = kind.as_ref().trim();
    let kind = if kind.eq_ignore_ascii_case("linear") {
        "linear"
    } else {
        "polar"
    };
    let cur = el
        .get_attr(ATTR_MORPH_INTERP)
        .map(|s| s.as_str())
        .unwrap_or("polar");
    if cur == kind {
        return LayoutStatus::Clean;
    }
    el.set_attr(ATTR_MORPH_INTERP, kind);
    crate::app::runtime::request_redraw();
    LayoutStatus::StyleChanged
}

pub fn apply_bound_morph_reduced(el: &mut Element, mode: impl AsRef<str>) -> LayoutStatus {
    let mode = mode.as_ref().trim();
    let mode = match mode {
        "always" | "user" | "never" => mode,
        _ => "never",
    };
    let cur = el
        .get_attr(ATTR_MORPH_REDUCED)
        .map(|s| s.as_str())
        .unwrap_or("never");
    if cur == mode {
        return LayoutStatus::Clean;
    }
    el.set_attr(ATTR_MORPH_REDUCED, mode);
    crate::app::runtime::request_redraw();
    LayoutStatus::StyleChanged
}

pub fn apply_bound_morph_view_box(el: &mut Element, vb: impl AsRef<str>) -> LayoutStatus {
    let vb = vb.as_ref().trim();
    let cur = el
        .get_attr(ATTR_MORPH_VIEWBOX)
        .map(|s| s.as_str())
        .unwrap_or("");
    if cur == vb {
        return LayoutStatus::Clean;
    }
    if vb.is_empty() {
        el.attrs.remove(ATTR_MORPH_VIEWBOX);
    } else {
        el.set_attr(ATTR_MORPH_VIEWBOX, vb);
    }
    crate::app::runtime::request_redraw();
    LayoutStatus::StyleChanged
}

pub fn apply_bound_morph_abs_stroke(el: &mut Element, on: bool) -> LayoutStatus {
    let cur = el
        .get_attr(ATTR_MORPH_ABS_STROKE)
        .is_some_and(|s| s == "1" || s.eq_ignore_ascii_case("true"));
    if cur == on {
        return LayoutStatus::Clean;
    }
    if on {
        el.set_attr(ATTR_MORPH_ABS_STROKE, "1");
    } else {
        el.attrs.remove(ATTR_MORPH_ABS_STROKE);
    }
    crate::app::runtime::request_redraw();
    LayoutStatus::StyleChanged
}

pub fn apply_bound_morph_label(el: &mut Element, label: impl AsRef<str>) -> LayoutStatus {
    let name = label.as_ref().trim();
    let cur = el
        .attrs
        .get("accesskit_name")
        .map(|s| s.as_str())
        .unwrap_or("");
    if cur == name {
        return LayoutStatus::Clean;
    }
    *el = paint_morph_a11y(
        std::mem::take(el),
        if name.is_empty() { None } else { Some(name) },
    );
    LayoutStatus::StyleChanged
}

fn pair_from_to(el: &Element) -> (String, String) {
    let to = el
        .get_attr(ATTR_MORPH_TO)
        .cloned()
        .or_else(|| el.get_attr(ATTR_MORPH_ICON).cloned())
        .unwrap_or_default();
    let from = el
        .get_attr(ATTR_MORPH_FROM)
        .cloned()
        .unwrap_or_else(|| to.clone());
    (from, to)
}

fn ensure_pair_plan(el: &Element, play: &mut MorphPlay, from: &str, to: &str) -> bool {
    if play.plan.is_some()
        && play.pair_from.as_deref() == Some(from)
        && play.pair_to.as_deref() == Some(to)
    {
        return true;
    }
    let (Some(a), Some(b)) = (resolve_input(el, from), resolve_input(el, to)) else {
        play.plan = None;
        return false;
    };
    let (Ok(sa), Ok(sb)) = (resample_icon(&a, SAMPLE_N), resample_icon(&b, SAMPLE_N)) else {
        play.plan = None;
        return false;
    };
    match build_plan(&sa, &sb) {
        Ok(plan) => {
            play.plan = Some(plan);
            play.from_key = from.to_string();
            play.to_key = to.to_string();
            true
        }
        Err(_) => {
            play.plan = None;
            false
        }
    }
}

/// morphicons `applyPair`: freeze from→to at `progress` (no spring). Missing
/// progress seeks at 0. Mid t re-bases on `from` once, then seeks.
fn apply_controlled_pair(el: &mut Element) {
    if !is_controlled(el) {
        return;
    }
    let Some(key) = ensure_play(el) else {
        return;
    };
    let t: f32 = el
        .get_attr(ATTR_MORPH_PROGRESS)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0_f32)
        .clamp(0.0, 1.0);
    let (from, to) = pair_from_to(el);
    let cfg = spring_cfg_from_el(el);
    if let Ok(mut map) = play_store().lock() {
        let play = map.entry(key).or_insert_with(|| {
            let mut p = new_play(
                from.clone(),
                to.clone(),
                rest_d_el(el, &to),
                t,
                cfg,
                true,
                false,
            );
            p.refs = 1;
            p
        });
        play.controlled = true;
        play.k = cfg.k;
        play.c = cfg.c;
        let changed = play.pair_from.as_deref() != Some(from.as_str())
            || play.pair_to.as_deref() != Some(to.as_str());
        if changed {
            play.pair_from = Some(from.clone());
            play.pair_to = Some(to.clone());
            play.based = false;
            play.plan = None;
            play.live = None;
        }
        if t <= 0.0 {
            play.from_key = from.clone();
            play.to_key = from.clone();
            play.rest_d = rest_d_el(el, &from);
            play.x = 0.0;
            play.v = 0.0;
            play.settled = true;
            play.based = false;
            play.plan = None;
            play.live = None;
        } else if t >= 1.0 {
            play.from_key = to.clone();
            play.to_key = to.clone();
            play.rest_d = rest_d_el(el, &to);
            play.x = 1.0;
            play.v = 0.0;
            play.settled = true;
            play.based = false;
            play.plan = None;
            play.live = None;
        } else {
            if !play.based {
                play.x = 0.0;
                play.v = 0.0;
                play.live = None;
                play.based = ensure_pair_plan(el, play, &from, &to);
            }
            play.x = t;
            play.settled = false;
        }
    }
}

fn leave_controlled(el: &mut Element) {
    let icon = el
        .get_attr(ATTR_MORPH_ICON)
        .cloned()
        .or_else(|| el.get_attr(ATTR_MORPH_TO).cloned())
        .unwrap_or_default();
    if let Some(key) = ensure_play(el) {
        if let Ok(mut map) = play_store().lock() {
            if let Some(play) = map.get_mut(&key) {
                play.controlled = false;
                invalidate_pair(play);
            }
        }
    }
    if !icon.is_empty() {
        morph_to_src(el, &icon);
    }
}

/// Solid-style `progress`. Does **not** enter/leave controlled mode (`from` &&
/// `to` do). `progress < 0` drops the attr; a live pair seeks at 0.
pub fn apply_bound_morph_progress(el: &mut Element, progress: f32) -> LayoutStatus {
    if progress < 0.0 {
        let had = el.get_attr(ATTR_MORPH_PROGRESS).is_some();
        if !had && !is_controlled(el) {
            return LayoutStatus::Clean;
        }
        el.attrs.remove(ATTR_MORPH_PROGRESS);
        if is_controlled(el) {
            apply_controlled_pair(el);
        }
        crate::app::runtime::request_redraw();
        return LayoutStatus::StyleChanged;
    }
    let p = progress.clamp(0.0, 1.0);
    let prev = el
        .get_attr(ATTR_MORPH_PROGRESS)
        .and_then(|s| s.parse::<f32>().ok())
        .unwrap_or(-1.0);
    if (prev - p).abs() < 1e-4 {
        return LayoutStatus::Clean;
    }
    el.set_attr(ATTR_MORPH_PROGRESS, format!("{p}"));
    if is_controlled(el) {
        apply_controlled_pair(el);
    } else {
        morph_seek_progress(el, p);
    }
    crate::app::runtime::request_redraw();
    LayoutStatus::StyleChanged
}

pub fn tick_morph_icon(el: &mut Element, dt: f32) -> bool {
    if el.get_attr(ATTR_MORPH_ICON).is_none() {
        return false;
    }
    let key = instance_key(el);
    let mut store = match play_store().lock() {
        Ok(g) => g,
        Err(_) => return false,
    };
    let Some(play) = store.get_mut(&key) else {
        return false;
    };
    if play.controlled || play.settled || play.plan.is_none() {
        return false;
    }
    let settled = spring_step(play, dt.max(0.0));
    if let Some(plan) = play.plan.as_ref() {
        play.live = Some(sampled_from_outputs(plan, play.x as f64));
    }
    if settled {
        play.x = 1.0;
        play.v = 0.0;
        play.settled = true;
        play.rest_d = rest_d_el(el, &play.to_key);
        play.live = None;
        play.plan = None;
        return false;
    }
    true
}

pub fn tick_morph_icons_subtree(el: &mut Element, dt: f32) -> bool {
    let mut any = tick_morph_icon(el, dt);
    for c in &mut el.children {
        if tick_morph_icons_subtree(c, dt) {
            any = true;
        }
    }
    any
}

pub fn subtree_needs_morph_icon_frame(el: &Element) -> bool {
    if el.get_attr(ATTR_MORPH_ICON).is_some() {
        let key = instance_key(el);
        if let Ok(map) = play_store().lock() {
            if let Some(play) = map.get(&key) {
                if !play.settled && !play.controlled && play.plan.is_some() {
                    return true;
                }
            }
        }
    }
    el.children.iter().any(subtree_needs_morph_icon_frame)
}

fn current_d(el: &Element) -> String {
    let key = instance_key(el);
    let linear = uses_linear(el);
    if let Ok(map) = play_store().lock() {
        if let Some(play) = map.get(&key) {
            if !play.settled {
                if let Some(plan) = play.plan.as_ref() {
                    return interp_plan_d_kind(plan, play.x as f64, linear);
                }
            }
            if !play.rest_d.is_empty() {
                return play.rest_d.clone();
            }
            return rest_d_el(el, &play.to_key);
        }
    }
    let to = el
        .get_attr(ATTR_MORPH_TO)
        .cloned()
        .or_else(|| el.get_attr(ATTR_MORPH_ICON).cloned())
        .unwrap_or_default();
    if is_controlled(el) {
        let (from, to) = pair_from_to(el);
        let p: f32 = el
            .get_attr(ATTR_MORPH_PROGRESS)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.0);
        if let (Some(a), Some(b)) = (resolve_input(el, &from), resolve_input(el, &to)) {
            if let Ok(d) = morph_d_at_kind(&a, &b, p as f64, linear) {
                return d;
            }
        }
        return rest_d_el(el, &to);
    }
    rest_d_el(el, &to)
}

fn stroke_for_paint(el: &Element, size: f32) -> f32 {
    let sw = el
        .get_attr(ATTR_MORPH_STROKE)
        .and_then(|s| s.parse().ok())
        .unwrap_or(2.0_f32);
    let abs = el
        .get_attr(ATTR_MORPH_ABS_STROKE)
        .is_some_and(|s| s == "1" || s.eq_ignore_ascii_case("true"));
    if abs {
        (sw * 24.0 / size.max(1.0)).max(0.25)
    } else {
        sw
    }
}

fn paint_morph_a11y(mut el: Element, label: Option<&str>) -> Element {
    if let Some(name) = label.map(str::trim).filter(|s| !s.is_empty()) {
        el.attrs.remove("aria-hidden");
        el.accesskit_role("image")
            .accesskit_name(name)
            .aria_label(name)
    } else {
        el.set_attr("aria-hidden", "true");
        el.attrs.remove("accesskit_role");
        el.attrs.remove("accesskit_name");
        el.attrs.remove("aria-label");
        el
    }
}

/// Rasterize current frame; returns texture key + logical size.
pub fn ensure_morph_icon_texture(
    el: &Element,
    size_px: f32,
    color: [u8; 4],
    dpr: f32,
) -> Option<(String, f32)> {
    let d = current_d(el);
    if d.is_empty() {
        return None;
    }
    let size = size_px.clamp(8.0, 256.0);
    let sw = stroke_for_paint(el, size);
    let doc = emit_doc(&d, color, sw);
    let dpr = dpr.clamp(1.0, 3.0);
    let side = (size * dpr).ceil().clamp(16.0, 512.0) as u32;

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    d.hash(&mut hasher);
    side.hash(&mut hasher);
    color.hash(&mut hasher);
    sw.to_bits().hash(&mut hasher);
    let tex = format!("rui_morph_icon:{:016x}", hasher.finish());
    let rgba = crate::media::svg_raster::rasterize_svg_str(&doc, side, side)?;
    crate::app::pending_images::queue_image_rgba8(&tex, side, side, rgba);
    Some((tex, side as f32))
}

pub struct MorphIconBuilder {
    icon: String,
    from: Option<String>,
    to: Option<String>,
    progress: Option<f32>,
    size: f32,
    stroke_width: f32,
    absolute_stroke_width: bool,
    spring: String,
    interp: String,
    reduced_motion: String,
    view_box: Option<String>,
    label: Option<String>,
    color: Option<[u8; 4]>,
    id: Option<String>,
}

pub fn morph_icon(icon: impl IntoMorphSrc) -> MorphIconBuilder {
    MorphIconBuilder {
        icon: icon.into_morph_src(),
        from: None,
        to: None,
        progress: None,
        size: 24.0,
        stroke_width: 2.0,
        absolute_stroke_width: false,
        spring: "snappy".into(),
        interp: "polar".into(),
        reduced_motion: "never".into(),
        view_box: None,
        label: None,
        color: None,
        id: None,
    }
}

impl MorphIconBuilder {
    pub fn from_icon(mut self, src: impl IntoMorphSrc) -> Self {
        self.from = Some(src.into_morph_src());
        self
    }
    pub fn to_icon(mut self, src: impl IntoMorphSrc) -> Self {
        self.to = Some(src.into_morph_src());
        self
    }
    pub fn icon_input(mut self, input: IconInput) -> Self {
        self.icon = input.into_morph_src();
        self
    }
    pub fn from_input(self, input: IconInput) -> Self {
        self.from_icon(input)
    }
    pub fn to_input(self, input: IconInput) -> Self {
        self.to_icon(input)
    }
    pub fn progress(mut self, p: f32) -> Self {
        if p < 0.0 {
            self.progress = None;
        } else {
            self.progress = Some(p.clamp(0.0, 1.0));
        }
        self
    }
    pub fn size(mut self, px: f32) -> Self {
        self.size = px.clamp(8.0, 256.0);
        self
    }
    pub fn size_px(self, px: f32) -> Self {
        self.size(px)
    }
    pub fn stroke_width(mut self, w: f32) -> Self {
        self.stroke_width = w.max(0.25);
        self
    }
    /// Lucide `absoluteStrokeWidth`: stroke stays `stroke_width` CSS px as `size` grows.
    pub fn absolute_stroke_width(mut self, on: bool) -> Self {
        self.absolute_stroke_width = on;
        self
    }
    /// Source grid for off-24 packs; geometry is `fit_icon`'d onto 24.
    pub fn view_box(mut self, vb: impl Into<String>) -> Self {
        let vb = vb.into();
        self.view_box = if vb.trim().is_empty() { None } else { Some(vb) };
        self
    }
    /// Accessible name. Empty / omitted → decorative (`aria-hidden`).
    pub fn label(mut self, label: impl Into<String>) -> Self {
        let label = label.into();
        self.label = if label.trim().is_empty() {
            None
        } else {
            Some(label)
        };
        self
    }
    pub fn spring(mut self, name: impl Into<String>) -> Self {
        self.spring = name.into();
        self
    }
    pub fn spring_constants(mut self, k: f32, c: f32) -> Self {
        self.spring = format!("{k},{c}");
        self
    }
    pub fn interp(mut self, kind: impl Into<String>) -> Self {
        let k = kind.into();
        self.interp = if k.eq_ignore_ascii_case("linear") {
            "linear".into()
        } else {
            "polar".into()
        };
        self
    }
    pub fn reduced_motion(mut self, mode: impl Into<String>) -> Self {
        let m = mode.into();
        self.reduced_motion = match m.as_str() {
            "always" | "user" | "never" => m,
            _ => "never".into(),
        };
        self
    }
    pub fn color(mut self, c: [u8; 4]) -> Self {
        self.color = Some(c);
        self
    }
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }
}

impl From<MorphIconBuilder> for Element {
    fn from(b: MorphIconBuilder) -> Self {
        static SEQ: AtomicU64 = AtomicU64::new(1);
        let display = b.to.clone().unwrap_or_else(|| b.icon.clone());
        let pair = b.from.as_ref().is_some_and(|s| !s.is_empty())
            && b.to.as_ref().is_some_and(|s| !s.is_empty());
        let empty = display.trim().is_empty() && !pair;
        let t0 = if pair {
            b.progress.unwrap_or(0.0)
        } else {
            b.progress.unwrap_or(1.0)
        };
        let controlled = pair;
        let size = b.size;
        let mut el = Element::new()
            .w(size)
            .h(size)
            .min_w(size)
            .min_h(size)
            .text_size(size);
        el.set_attr(ATTR_MORPH_SIZE, format!("{size}"));
        el.set_attr(ATTR_MORPH_STROKE, format!("{}", b.stroke_width));
        el.set_attr(ATTR_MORPH_SPRING, &b.spring);
        el.set_attr(ATTR_MORPH_INTERP, &b.interp);
        el.set_attr(ATTR_MORPH_REDUCED, &b.reduced_motion);
        if b.absolute_stroke_width {
            el.set_attr(ATTR_MORPH_ABS_STROKE, "1");
        }
        if let Some(vb) = &b.view_box {
            el.set_attr(ATTR_MORPH_VIEWBOX, vb);
        }
        if let Some(from) = &b.from {
            el.set_attr(ATTR_MORPH_FROM, from);
        }
        if let Some(p) = b.progress {
            el.set_attr(ATTR_MORPH_PROGRESS, format!("{p}"));
        }
        let fg = b
            .color
            .unwrap_or_else(|| crate::style::parse_text_color("foreground"));
        el = el.text_color(fg);
        if !empty {
            el.set_attr(ATTR_MORPH_ICON, &display);
            el.set_attr(ATTR_MORPH_TO, &display);
            lucide_seed(&mut el, &display);
        }
        el.text_style.font_family = Some("lucide".into());
        let tid = if let Some(id) = &b.id {
            morph_template_id(id)
        } else {
            let seq = SEQ.fetch_add(1, Ordering::Relaxed);
            let tag = if display.is_empty() {
                "lazy"
            } else {
                display.as_str()
            };
            format!("morph-icon:{tag}:{seq}")
        };
        el.template_id = Some(tid);
        // Button parents need hits; MorphIcon itself is paint-only.
        el = el.pointer_events("none");
        el = paint_morph_a11y(el, b.label.as_deref());

        let key = instance_key(&el);
        let cfg = spring_cfg(&b.spring);
        let mut inserted = false;
        if !empty {
            if let Ok(mut map) = play_store().lock() {
                if let Some(play) = map.get_mut(&key) {
                    play.refs = play.refs.saturating_add(1);
                } else {
                    let mut play = new_play(
                        b.from.clone().unwrap_or_else(|| display.clone()),
                        display.clone(),
                        rest_d_el(&el, &display),
                        t0,
                        cfg,
                        controlled,
                        !controlled || t0 >= 1.0 - 1e-3 || t0 <= 0.0,
                    );
                    play.refs = 1;
                    map.insert(key, play);
                    inserted = true;
                }
            }
        }
        el = el.on_unmount(move || release_play(key));
        if pair {
            apply_controlled_pair(&mut el);
        } else if inserted && b.progress.is_some() {
            morph_seek_src(&mut el, &display, t0);
        }
        el
    }
}

impl IntoElementList for MorphIconBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

/// `ui!` / component entry: `<MorphIcon icon="menu" size={24.0} />`
#[allow(non_snake_case)]
pub fn MorphIcon(
    icon: impl AsRef<str>,
    from: Option<String>,
    to: Option<String>,
    progress: Option<f32>,
    size: Option<f32>,
    spring: Option<String>,
    stroke_width: Option<f32>,
    absolute_stroke_width: Option<bool>,
    interp: Option<String>,
    reduced_motion: Option<String>,
    view_box: Option<String>,
    label: Option<String>,
    id: Option<String>,
) -> Element {
    let mut b = morph_icon(icon.as_ref()).size_px(size.unwrap_or(24.0));
    if let Some(from) = from {
        b = b.from_icon(from);
    }
    if let Some(to) = to {
        b = b.to_icon(to);
    }
    if let Some(p) = progress {
        b = b.progress(p);
    }
    if let Some(s) = spring {
        b = b.spring(s);
    }
    if let Some(w) = stroke_width {
        b = b.stroke_width(w);
    }
    if let Some(on) = absolute_stroke_width {
        b = b.absolute_stroke_width(on);
    }
    if let Some(kind) = interp {
        b = b.interp(kind);
    }
    if let Some(mode) = reduced_motion {
        b = b.reduced_motion(mode);
    }
    if let Some(vb) = view_box {
        b = b.view_box(vb);
    }
    if let Some(label) = label {
        b = b.label(label);
    }
    if let Some(id) = id {
        b = b.id(id);
    }
    b.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_sets_attrs() {
        let el: Element = morph_icon("menu").size_px(28.0).spring("bouncy").into();
        assert_eq!(
            el.get_attr(ATTR_MORPH_ICON).map(|s| s.as_str()),
            Some("menu")
        );
        assert_eq!(
            el.get_attr(ATTR_MORPH_SPRING).map(|s| s.as_str()),
            Some("bouncy")
        );
        assert!(el
            .template_id
            .as_deref()
            .unwrap()
            .starts_with("morph-icon:"));
    }

    #[cfg(feature = "bundled-lucide")]
    #[test]
    fn morph_to_changes_target() {
        let mut el: Element = morph_icon("menu")
            .id("morph-to-target")
            .size_px(24.0)
            .into();
        morph_to(&mut el, "x");
        assert_eq!(el.get_attr(ATTR_MORPH_TO).map(|s| s.as_str()), Some("x"));
        assert!(tick_morph_icon(&mut el, 1.0 / 60.0));
    }

    #[cfg(feature = "bundled-lucide")]
    #[test]
    fn resvg_morph_frame_nonempty() {
        let el: Element = morph_icon("menu").size_px(32.0).into();
        let fg = crate::style::parse_text_color("foreground");
        let (tex, _) = ensure_morph_icon_texture(&el, 32.0, fg, 2.0).expect("tex");
        assert!(tex.starts_with("rui_morph_icon:"));
    }

    #[cfg(feature = "bundled-lucide")]
    #[test]
    fn stable_id_does_not_reset_play() {
        let mut el: Element = morph_icon("menu").id("keep-flight").into();
        morph_to(&mut el, "x");
        assert!(tick_morph_icon(&mut el, 1.0 / 60.0));
        let _rebuild: Element = morph_icon("menu").id("keep-flight").into();
        assert!(
            tick_morph_icon(&mut el, 1.0 / 60.0),
            "same template id must keep the in-flight spring"
        );
        assert!((morph_progress_by_id("keep-flight") - 1.0).abs() > 0.01);
    }

    #[test]
    fn bound_progress_stroke_spring() {
        let mut el: Element = morph_icon("menu")
            .id("pg")
            .from_icon("menu")
            .to_icon("x")
            .into();
        assert_eq!(
            apply_bound_morph_stroke(&mut el, 2.5),
            LayoutStatus::StyleChanged
        );
        assert_eq!(
            el.get_attr(ATTR_MORPH_STROKE).map(|s| s.as_str()),
            Some("2.5")
        );
        assert_eq!(
            apply_bound_morph_spring(&mut el, "bouncy"),
            LayoutStatus::StyleChanged
        );
        assert_eq!(
            el.get_attr(ATTR_MORPH_SPRING).map(|s| s.as_str()),
            Some("bouncy")
        );
        assert_eq!(
            apply_bound_morph_progress(&mut el, 0.4),
            LayoutStatus::StyleChanged
        );
        assert_eq!(
            el.get_attr(ATTR_MORPH_PROGRESS).map(|s| s.as_str()),
            Some("0.4")
        );
        assert!((morph_progress_by_id("pg") - 0.4).abs() < 1e-3);
        assert!(!tick_morph_icon(&mut el, 1.0 / 60.0));
        assert_eq!(
            apply_bound_morph_progress(&mut el, -1.0),
            LayoutStatus::StyleChanged
        );
        assert!(el.get_attr(ATTR_MORPH_PROGRESS).is_none());
        assert!(
            is_controlled(&el),
            "progress < 0 must not drop a from&&to pair"
        );
        assert!(!tick_morph_icon(&mut el, 1.0 / 60.0));
    }

    #[test]
    fn button_keeps_morph_icon_child() {
        let morph: Element = morph_icon("pause").id("pp").into();
        let el: Element = crate::components::button("gal-pp").child(morph).into();
        fn has_morph(el: &Element) -> bool {
            el.attrs.contains_key(ATTR_MORPH_ICON) || el.children.iter().any(has_morph)
        }
        assert!(
            has_morph(&el),
            "Button must not flatten MorphIcon into a Lucide caption leaf"
        );

        let wrapped = Element::new().child(morph_icon("play").id("pp-wrap"));
        let el: Element = crate::components::button("gal-pp-wrap")
            .child(wrapped)
            .into();
        assert!(
            has_morph(&el),
            "Button must keep MorphIcon even when nested in a wrapper"
        );
    }

    #[test]
    fn spring_cfg_parses_constants() {
        let a = spring_cfg("420,30");
        assert!((a.k - 420.0).abs() < 1e-3 && (a.c - 30.0).abs() < 1e-3);
        let b = spring_cfg("k:170,c:26");
        assert!((b.k - 170.0).abs() < 1e-3 && (b.c - 26.0).abs() < 1e-3);
        let c = spring_cfg("stiffness:300,damping:14");
        assert!((c.k - 300.0).abs() < 1e-3 && (c.c - 14.0).abs() < 1e-3);
        let d = spring_cfg("smooth");
        assert!((d.k - 170.0).abs() < 1e-3);
    }

    #[test]
    fn morph_set_jumps_without_spring() {
        let mut el: Element = morph_icon("menu").id("set-jump").into();
        morph_set(&mut el, "x");
        assert_eq!(el.get_attr(ATTR_MORPH_TO).map(|s| s.as_str()), Some("x"));
        assert!(el.get_attr(ATTR_MORPH_PROGRESS).is_none());
        assert!(!tick_morph_icon(&mut el, 1.0 / 60.0));
        assert!((morph_progress_by_id("set-jump") - 1.0).abs() < 1e-3);
    }

    #[test]
    fn reduced_always_morph_to_is_set() {
        let mut el: Element = morph_icon("menu")
            .id("rm-always")
            .reduced_motion("always")
            .into();
        morph_to(&mut el, "x");
        assert!(!tick_morph_icon(&mut el, 1.0 / 60.0));
        assert!((morph_progress_by_id("rm-always") - 1.0).abs() < 1e-3);
    }

    #[test]
    fn builder_icon_input_and_custom_spring() {
        let input = IconInput::D("M0 12 L24 12".into());
        let el: Element = morph_icon(input)
            .id("ii")
            .spring_constants(200.0, 22.0)
            .interp("linear")
            .into();
        assert_eq!(
            el.get_attr(ATTR_MORPH_SPRING).map(|s| s.as_str()),
            Some("200,22")
        );
        assert_eq!(
            el.get_attr(ATTR_MORPH_INTERP).map(|s| s.as_str()),
            Some("linear")
        );
        let cfg = spring_cfg(el.get_attr(ATTR_MORPH_SPRING).unwrap());
        assert!((cfg.k - 200.0).abs() < 1e-3 && (cfg.c - 22.0).abs() < 1e-3);
    }

    #[cfg(feature = "bundled-lucide")]
    #[test]
    fn controlled_pair_rebases_and_exit_invalidates() {
        let mut el: Element = morph_icon("menu")
            .id("rebase")
            .from_icon("menu")
            .to_icon("x")
            .progress(0.4)
            .into();
        let d_menu_x = current_d(&el);
        assert!(!d_menu_x.is_empty());
        assert_eq!(
            apply_bound_morph_from(&mut el, "plus"),
            LayoutStatus::StyleChanged
        );
        assert_eq!(
            apply_bound_morph_to(&mut el, "minus"),
            LayoutStatus::StyleChanged
        );
        let d_plus_minus = current_d(&el);
        assert_ne!(
            d_menu_x, d_plus_minus,
            "pair change at mid t must rebase onto the new from"
        );
        assert_eq!(
            apply_bound_morph_icon(&mut el, "circle"),
            LayoutStatus::Clean
        );
        assert_eq!(
            el.get_attr(ATTR_MORPH_TO).map(|s| s.as_str()),
            Some("minus"),
            "icon bind ignored while controlled"
        );
        assert_eq!(
            apply_bound_morph_progress(&mut el, -1.0),
            LayoutStatus::StyleChanged
        );
        assert!(el.get_attr(ATTR_MORPH_PROGRESS).is_none());
        let key = instance_key(&el);
        if let Ok(map) = play_store().lock() {
            let play = map.get(&key).expect("play");
            assert!(play.controlled, "dropping progress keeps the from&&to pair");
            assert_eq!(play.pair_from.as_deref(), Some("plus"));
        }
        assert_eq!(
            apply_bound_morph_from(&mut el, ""),
            LayoutStatus::StyleChanged
        );
        assert!(!is_controlled(&el));
        let key = instance_key(&el);
        if let Ok(map) = play_store().lock() {
            let play = map.get(&key).expect("play");
            assert!(!play.based);
            assert!(play.pair_from.is_none());
        }
        assert_eq!(
            apply_bound_morph_from(&mut el, "plus"),
            LayoutStatus::StyleChanged
        );
        assert_eq!(
            apply_bound_morph_to(&mut el, "minus"),
            LayoutStatus::StyleChanged
        );
        assert_eq!(
            apply_bound_morph_progress(&mut el, 0.5),
            LayoutStatus::StyleChanged
        );
        if let Ok(map) = play_store().lock() {
            let play = map.get(&key).expect("play");
            assert!(play.based);
            assert_eq!(play.pair_from.as_deref(), Some("plus"));
            assert_eq!(play.pair_to.as_deref(), Some("minus"));
        }
    }

    #[cfg(feature = "bundled-lucide")]
    #[test]
    fn from_and_to_without_progress_is_controlled() {
        let mut el: Element = morph_icon("menu")
            .id("pair-only")
            .from_icon("menu")
            .to_icon("x")
            .into();
        assert!(is_controlled(&el));
        assert!(!tick_morph_icon(&mut el, 1.0 / 60.0));
        assert!((morph_progress(&el) - 0.0).abs() < 1e-3);
        let from_d = rest_d_for("menu");
        assert_eq!(current_d(&el), from_d);
    }

    #[cfg(feature = "bundled-lucide")]
    #[test]
    fn morph_handle_seek_and_into_morph_src() {
        let mut el: Element = morph_icon("menu").id("morph-handle-seek").into();
        let input = IconInput::from("x");
        {
            let mut h = morph_handle(&mut el);
            h.seek(input, 0.35);
            assert!((h.progress() - 0.35).abs() < 1e-3);
        }
        assert!(!tick_morph_icon(&mut el, 1.0 / 60.0));
        let mut el: Element = morph_icon("menu").id("morph-handle-to").into();
        morph_to(&mut el, IconInput::from("x"));
        assert_eq!(el.get_attr(ATTR_MORPH_ICON).map(|s| s.as_str()), Some("x"));
        assert!(tick_morph_icon(&mut el, 1.0 / 60.0));
    }

    #[test]
    fn fit_icon_is_public() {
        let input = IconInput::D("M0 0 L32 0".into());
        let d = crate::utils::fit_icon(&input, "0 0 32 32", 24.0).expect("fit");
        assert!(d.starts_with('M'), "{d}");
    }

    #[cfg(feature = "bundled-lucide")]
    #[test]
    fn polar_and_linear_mid_frames_differ() {
        let polar: Element = morph_icon("menu")
            .id("interp-p")
            .from_icon("menu")
            .to_icon("x")
            .progress(0.5)
            .interp("polar")
            .into();
        let linear: Element = morph_icon("menu")
            .id("interp-l")
            .from_icon("menu")
            .to_icon("x")
            .progress(0.5)
            .interp("linear")
            .into();
        assert_ne!(current_d(&polar), current_d(&linear));
    }

    fn play_present(id: &str) -> bool {
        let key = key_for_template(&morph_template_id(id));
        play_exists(key)
    }

    #[cfg(feature = "bundled-lucide")]
    #[test]
    fn morph_to_with_overrides_instance_spring() {
        let mut el: Element = morph_icon("menu").id("spring-once").spring("snappy").into();
        morph_to_with(&mut el, "x", "bouncy");
        let key = instance_key(&el);
        let (k, c) = play_store()
            .lock()
            .ok()
            .and_then(|m| m.get(&key).map(|p| (p.k, p.c)))
            .expect("play");
        assert!((k - 300.0).abs() < 1e-3 && (c - 14.0).abs() < 1e-3);
        assert_eq!(
            el.get_attr(ATTR_MORPH_SPRING).map(String::as_str),
            Some("snappy"),
            "per-call spring must not rewrite the instance attr"
        );
    }

    #[test]
    fn destroy_and_drop_clear_play_store() {
        let el: Element = morph_icon("menu").id("destroy-me").into();
        assert!(play_present("destroy-me"));
        morph_destroy(&el);
        assert!(!play_present("destroy-me"));
        drop(el);

        let el: Element = morph_icon("menu").id("unmount-me").into();
        assert!(play_present("unmount-me"));
        drop(el);
        assert!(
            !play_present("unmount-me"),
            "last Element drop must release the play slot"
        );
    }

    #[cfg(feature = "bundled-lucide")]
    #[test]
    fn rebuild_keeps_play_until_last_drop() {
        let mut live: Element = morph_icon("menu").id("keep-refs").into();
        morph_to(&mut live, "x");
        {
            let _rebuild: Element = morph_icon("menu").id("keep-refs").into();
            assert!(play_present("keep-refs"));
        }
        assert!(
            play_present("keep-refs"),
            "temporary rebuild must not drop the live instance"
        );
        drop(live);
        assert!(!play_present("keep-refs"));
    }

    #[cfg(feature = "bundled-lucide")]
    #[test]
    fn lazy_mount_first_icon_does_not_fly() {
        let mut el: Element = morph_icon("").id("lazy-driver").into();
        assert!(el.get_attr(ATTR_MORPH_ICON).is_none());
        morph_to(&mut el, "menu");
        assert_eq!(
            el.get_attr(ATTR_MORPH_ICON).map(String::as_str),
            Some("menu")
        );
        assert!(
            !tick_morph_icon(&mut el, 1.0 / 60.0),
            "first icon on a lazy mount paints, no spring"
        );
        morph_to(&mut el, "x");
        assert!(
            tick_morph_icon(&mut el, 1.0 / 60.0),
            "second morph_to flies"
        );
    }

    #[test]
    fn absolute_stroke_width_locks_pixel_width() {
        let rel: Element = morph_icon("menu").id("sw-rel").size_px(48.0).into();
        let abs: Element = morph_icon("menu")
            .id("sw-abs")
            .size_px(48.0)
            .absolute_stroke_width(true)
            .into();
        assert!((stroke_for_paint(&rel, 48.0) - 2.0).abs() < 1e-3);
        assert!((stroke_for_paint(&abs, 48.0) - 1.0).abs() < 1e-3);
    }

    #[test]
    fn view_box_fits_off_grid_d() {
        let el: Element = morph_icon(IconInput::D("M0 0 L32 0".into()))
            .id("vb-fit")
            .view_box("0 0 32 32")
            .into();
        let d = current_d(&el);
        assert!(d.starts_with('M'), "{d}");
        let fitted =
            crate::utils::fit_icon(&IconInput::D("M0 0 L32 0".into()), "0 0 32 32", 24.0).unwrap();
        assert_eq!(d, fitted);
    }

    #[test]
    fn decorative_by_default_labeled_is_image() {
        let quiet: Element = morph_icon("menu").id("a11y-quiet").into();
        assert_eq!(
            quiet.attrs.get("aria-hidden").map(String::as_str),
            Some("true")
        );
        assert!(!quiet.attrs.contains_key("accesskit_role"));
        let named: Element = morph_icon("menu").id("a11y-named").label("Menu").into();
        assert!(!named.attrs.contains_key("aria-hidden"));
        assert_eq!(
            named.attrs.get("accesskit_role").map(String::as_str),
            Some("image")
        );
        assert_eq!(
            named.attrs.get("accesskit_name").map(String::as_str),
            Some("Menu")
        );
    }
}
