//! Lucide-animated recipes ([pqoqubbw/icons](https://github.com/pqoqubbw/icons)) → SVG frame bake.
//!
//! Recipes are extracted offline (`scripts/extract_lucide_animated.mjs`) into
//! `assets/lucide_animated.json`. Playback is hover / imperative; frames go through resvg
//! (or CPU path) like public `<svg>` / Lucide static icons — no React/Motion runtime.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use serde::Deserialize;

use std::sync::atomic::{AtomicU64, Ordering};

pub use super::icon_provider::sync_icon_hover as sync_animated_icon_hover;
pub(crate) use super::icon_provider::with_icon_hover as with_animated_icon_hover;
use crate::core::tree::{Element, IntoElementList};

pub const ATTR_ANIMATED_ICON: &str = "jadix_animated_icon";
pub const ATTR_ANIMATED_ICON_STATE: &str = super::icon_provider::ATTR_ICON_ANIMATION_STATE;
pub const ATTR_ANIMATED_ICON_CONTROLLED: &str = "jadix_animated_icon_controlled";
pub const ATTR_ANIMATED_ICON_SIZE: &str = "jadix_animated_icon_size";

#[derive(Debug, Clone, Deserialize, Default)]
pub struct AnimatedIconRecipe {
    pub id: String,
    #[serde(default)]
    pub lucide: String,
    #[serde(default = "default_vb", alias = "viewBox")]
    pub view_box: String,
    #[serde(default = "default_sw", alias = "strokeWidth")]
    pub stroke_width: f32,
    #[serde(default = "default_cap", alias = "strokeLinecap")]
    pub stroke_linecap: String,
    #[serde(default = "default_join", alias = "strokeLinejoin")]
    pub stroke_linejoin: String,
    #[serde(default)]
    pub root: RecipeNodeGroup,
    #[serde(default)]
    pub nodes: Vec<RecipeNode>,
    #[serde(default)]
    pub animated: bool,
}

fn default_vb() -> String {
    "0 0 24 24".into()
}
fn default_sw() -> f32 {
    2.0
}
fn default_cap() -> String {
    "round".into()
}
fn default_join() -> String {
    "round".into()
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RecipeNodeGroup {
    #[serde(default)]
    pub tracks: Vec<RecipeTrack>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RecipeNode {
    #[serde(default)]
    pub id: String,
    #[serde(default = "default_path_tag")]
    pub tag: String,
    #[serde(default)]
    pub attrs: HashMap<String, String>,
    #[serde(default)]
    pub tracks: Vec<RecipeTrack>,
    /// Nested geometry under a Motion `<g>` (e.g. download arrow group).
    #[serde(default)]
    pub children: Vec<RecipeNode>,
}

fn default_path_tag() -> String {
    "path".into()
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RecipeTrack {
    pub prop: String,
    #[serde(default)]
    pub keyframes: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub transition: Option<RecipeTransition>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RecipeTransition {
    #[serde(default)]
    pub duration: Option<f32>,
    #[serde(default)]
    pub delay: Option<f32>,
    #[serde(default)]
    pub ease: Option<String>,
    /// `-1` = infinite
    #[serde(default)]
    pub repeat: Option<f32>,
    #[serde(default, rename = "type")]
    pub ty: Option<String>,
    #[serde(default)]
    pub stiffness: Option<f32>,
    #[serde(default)]
    pub damping: Option<f32>,
    /// Motion `times` — absolute keyframe offsets in [0,1], same length as value array.
    #[serde(default)]
    pub times: Option<Vec<f32>>,
    #[serde(default, rename = "perProp")]
    pub per_prop: Option<HashMap<String, PerPropTransition>>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PerPropTransition {
    #[serde(default)]
    pub duration: Option<f32>,
}

fn recipes() -> &'static HashMap<String, AnimatedIconRecipe> {
    static MAP: OnceLock<HashMap<String, AnimatedIconRecipe>> = OnceLock::new();
    MAP.get_or_init(|| {
        serde_json::from_str(include_str!("../../assets/lucide_animated.json")).unwrap_or_default()
    })
}

pub fn animated_icon_recipe(name: &str) -> Option<&'static AnimatedIconRecipe> {
    recipes().get(name)
}

pub fn animated_icon_names() -> impl Iterator<Item = &'static str> {
    recipes().keys().map(|s| s.as_str())
}

#[derive(Clone, Debug)]
struct Playback {
    state: String,
    elapsed: f32,
    /// Wall time when current state started.
    started_secs: f32,
    /// Cross-lerp from the other variant. Set only on explicit state changes
    /// (`set_animated_icon_state`). Cold-start idle must NOT bridge — otherwise
    /// `normal` at t=0 samples the `animate` pose (menu→X, copy→split).
    bridging: bool,
}

fn play_store() -> std::rc::Rc<Mutex<HashMap<u64, Playback>>> {
    crate::authoring::window::window_local! {
        static STORE: std::rc::Rc<Mutex<HashMap<u64, Playback>>> = std::rc::Rc::new(Mutex::new(HashMap::new()));
    }
    STORE.with(std::rc::Rc::clone)
}

fn now_secs() -> f32 {
    static START: OnceLock<std::time::Instant> = OnceLock::new();
    START
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_secs_f32()
}

fn instance_key(el: &Element) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    if let Some(id) = el.template_id.as_deref() {
        id.hash(&mut h);
    } else if let Some(n) = el.get_attr(ATTR_ANIMATED_ICON) {
        n.hash(&mut h);
        el.layout_bounds[0].to_bits().hash(&mut h);
        el.layout_bounds[1].to_bits().hash(&mut h);
    }
    h.finish()
}

fn value_as_f32(v: &serde_json::Value) -> Option<f32> {
    match v {
        serde_json::Value::Number(n) => n.as_f64().map(|x| x as f32),
        serde_json::Value::String(s) => {
            let s = s.trim();
            if let Some(d) = s.strip_suffix("deg") {
                return d.trim().parse().ok();
            }
            s.parse().ok()
        }
        serde_json::Value::Array(arr) if arr.len() == 1 => value_as_f32(&arr[0]),
        _ => None,
    }
}

fn keyframe_series(v: &serde_json::Value) -> Vec<f32> {
    match v {
        serde_json::Value::Array(arr) => arr.iter().filter_map(value_as_f32).collect(),
        other => value_as_f32(other).into_iter().collect(),
    }
}

fn ease_fn(name: Option<&str>, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    match name.unwrap_or("easeOut") {
        "linear" => t,
        "easeIn" => t * t,
        "easeInOut" => {
            if t < 0.5 {
                2.0 * t * t
            } else {
                1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
            }
        }
        // easeOut / default
        _ => 1.0 - (1.0 - t).powi(2),
    }
}

fn sample_series_timed(series: &[f32], times: Option<&[f32]>, t: f32) -> f32 {
    if series.is_empty() {
        return 0.0;
    }
    if series.len() == 1 || t <= 0.0 {
        return series[0];
    }
    if t >= 1.0 {
        return *series.last().unwrap();
    }
    if let Some(times) = times {
        if times.len() == series.len() && times.len() >= 2 {
            // Find segment where times[i] <= t <= times[i+1]
            for i in 0..times.len() - 1 {
                let t0 = times[i];
                let t1 = times[i + 1];
                if t >= t0 && t <= t1 + 1e-6 {
                    let span = (t1 - t0).max(1e-6);
                    let f = ((t - t0) / span).clamp(0.0, 1.0);
                    return series[i] + (series[i + 1] - series[i]) * f;
                }
            }
            return *series.last().unwrap();
        }
    }
    let n = series.len() - 1;
    let x = t * n as f32;
    let i = (x.floor() as usize).min(n - 1);
    let f = x - i as f32;
    series[i] + (series[i + 1] - series[i]) * f
}

fn track_duration(track: &RecipeTrack, prop: &str) -> f32 {
    let tr = track.transition.as_ref();
    if let Some(tr) = tr {
        if let Some(per) = tr.per_prop.as_ref().and_then(|m| m.get(prop)) {
            if let Some(d) = per.duration {
                return d.max(0.01);
            }
        }
        if tr.ty.as_deref() == Some("spring") {
            // Approximate spring settle time from stiffness/damping.
            let k = tr.stiffness.unwrap_or(200.0);
            return (12.0 / k.sqrt()).clamp(0.2, 1.2);
        }
        return tr.duration.unwrap_or(0.4).max(0.01);
    }
    0.4
}

fn sample_track(track: &RecipeTrack, state: &str, elapsed: f32, bridging: bool) -> Option<f32> {
    let kf = track.keyframes.get(state)?;
    let mut series = keyframe_series(kf);
    if series.is_empty() {
        return None;
    }
    // Framer Motion: a single target value tweens from the previous variant.
    // Multi-value arrays are a full timeline for this state (no bridge).
    // Bridging is only for explicit state changes — idle `normal` must snap to
    // its own keyframe (not reverse-lerp from `animate`).
    if series.len() == 1 && bridging {
        let other = if state == "animate" {
            "normal"
        } else {
            "animate"
        };
        if let Some(from_v) = track
            .keyframes
            .get(other)
            .map(keyframe_series)
            .filter(|s| !s.is_empty())
            .map(|s| *s.last().unwrap())
        {
            if (from_v - series[0]).abs() > 1e-5 {
                series = vec![from_v, series[0]];
            }
        }
    }
    let delay = track
        .transition
        .as_ref()
        .and_then(|t| t.delay)
        .unwrap_or(0.0);
    let dur = track_duration(track, &track.prop);
    let repeat = track
        .transition
        .as_ref()
        .and_then(|t| t.repeat)
        .unwrap_or(0.0);
    let local = (elapsed - delay).max(0.0);
    let ease = track.transition.as_ref().and_then(|t| t.ease.as_deref());

    let mut t = if dur <= 1e-6 { 1.0 } else { local / dur };
    if repeat < 0.0 {
        t = t.fract();
    } else if repeat > 0.0 {
        let total = 1.0 + repeat;
        if t >= total {
            t = 1.0;
        } else {
            t = (t % 1.0).abs();
            if t < 1e-6 && local > 1e-4 {
                t = 1.0;
            }
        }
    } else {
        t = t.clamp(0.0, 1.0);
    }
    let times = track.transition.as_ref().and_then(|tr| tr.times.as_deref());
    // Motion: when `times` is set, keyframe offsets are absolute on the duration
    // timeline (typically linear). Ease applies within segments only — do not
    // ease the global t before looking up `times`, or stagger blinks skew.
    let value = if times.is_some() {
        sample_series_timed(&series, times, t)
    } else {
        sample_series_timed(&series, None, ease_fn(ease, t))
    };
    Some(value)
}

#[derive(Default, Clone)]
struct SampledProps {
    opacity: Option<f32>,
    path_length: Option<f32>,
    path_offset: Option<f32>,
    scale: Option<f32>,
    scale_x: Option<f32>,
    scale_y: Option<f32>,
    rotate: Option<f32>,
    x: Option<f32>,
    y: Option<f32>,
    x1: Option<f32>,
    y1: Option<f32>,
    x2: Option<f32>,
    y2: Option<f32>,
    cx: Option<f32>,
    cy: Option<f32>,
    r: Option<f32>,
    width: Option<f32>,
    height: Option<f32>,
    stroke_width: Option<f32>,
    /// Optional path `d` morph target (string keyframe; snapped by progress).
    d: Option<String>,
}

fn keyframe_string_series(v: &serde_json::Value) -> Vec<String> {
    match v {
        serde_json::Value::Array(arr) => arr
            .iter()
            .filter_map(|x| x.as_str().map(|s| s.to_string()))
            .collect(),
        serde_json::Value::String(s) => vec![s.clone()],
        _ => Vec::new(),
    }
}

fn sample_tracks(
    tracks: &[RecipeTrack],
    state: &str,
    elapsed: f32,
    bridging: bool,
) -> SampledProps {
    let mut out = SampledProps::default();
    for track in tracks {
        if track.prop == "d" {
            if let Some(kf) = track.keyframes.get(state) {
                let series = keyframe_string_series(kf);
                if !series.is_empty() {
                    // Snap: before midpoint keep first, after keep last — fallback if
                    // polar interp cannot lower the pair (fill-drawn / mismatched d).
                    let delay = track
                        .transition
                        .as_ref()
                        .and_then(|t| t.delay)
                        .unwrap_or(0.0);
                    let dur = track_duration(track, "d");
                    let local = (elapsed - delay).max(0.0);
                    let t = if dur <= 1e-6 {
                        1.0
                    } else {
                        (local / dur).clamp(0.0, 1.0)
                    };
                    let ease = track.transition.as_ref().and_then(|t| t.ease.as_deref());
                    let te = ease_fn(ease, t);
                    if series.len() == 1 {
                        out.d = Some(series[0].clone());
                    } else {
                        let n = series.len() - 1;
                        let x = te * n as f32;
                        let i = (x.floor() as usize).min(n - 1);
                        let f = (x - i as f32) as f64;
                        let snap = || {
                            let idx = if te < 0.5 { 0 } else { series.len() - 1 };
                            Some(series[idx].clone())
                        };
                        #[cfg(feature = "svg")]
                        {
                            out.d = crate::utils::icon_morph::morph_src_at(
                                &series[i],
                                &series[i + 1],
                                f,
                            )
                            .or_else(snap);
                        }
                        #[cfg(not(feature = "svg"))]
                        {
                            let _ = (i, f);
                            out.d = snap();
                        }
                    }
                }
            }
            continue;
        }
        let Some(v) = sample_track(track, state, elapsed, bridging) else {
            continue;
        };
        match track.prop.as_str() {
            "opacity" => out.opacity = Some(v),
            "pathLength" => out.path_length = Some(v),
            "pathOffset" => out.path_offset = Some(v),
            "scale" => out.scale = Some(v),
            "scaleX" => out.scale_x = Some(v),
            "scaleY" => out.scale_y = Some(v),
            "rotate" => out.rotate = Some(v),
            "x" => out.x = Some(v),
            "y" => out.y = Some(v),
            "x1" => out.x1 = Some(v),
            "y1" => out.y1 = Some(v),
            "x2" => out.x2 = Some(v),
            "y2" => out.y2 = Some(v),
            "cx" => out.cx = Some(v),
            "cy" => out.cy = Some(v),
            "r" => out.r = Some(v),
            "width" => out.width = Some(v),
            "height" => out.height = Some(v),
            "strokeWidth" => out.stroke_width = Some(v),
            _ => {}
        }
    }
    out
}

fn xml_escape_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
}

/// Approximate SVG path length in user units (curves flattened via [`decode_path_d`]).
///
/// Motion encodes draw progress as unitless `pathLength`/`pathOffset` with SVG
/// `pathLength="1"`. resvg ignores that attribute
/// (https://github.com/linebender/resvg/issues/879), so `stroke-dasharray="0.5 1"`
/// is ~0.5px on a ~20–30ux Lucide path — no visible left-to-right draw. Scale by
/// the real length instead.
fn svg_path_d_length(d: &str) -> f32 {
    use crate::primitives::path::PathCommand;
    let cmds = crate::primitives::path::flatten_path(&crate::primitives::path::decode_path_d(d));
    let mut len = 0.0f32;
    let mut cx = 0.0f32;
    let mut cy = 0.0f32;
    let mut sx = 0.0f32;
    let mut sy = 0.0f32;
    for cmd in cmds {
        match cmd {
            PathCommand::MoveTo((x, y)) => {
                cx = x;
                cy = y;
                sx = x;
                sy = y;
            }
            PathCommand::LineTo((x, y)) => {
                len += (x - cx).hypot(y - cy);
                cx = x;
                cy = y;
            }
            PathCommand::Close => {
                len += (sx - cx).hypot(sy - cy);
                cx = sx;
                cy = sy;
            }
            PathCommand::QuadTo { .. } | PathCommand::CubicTo { .. } | PathCommand::Arc { .. } => {}
        }
    }
    len.max(1e-3)
}

fn polyline_points_length(points: &str) -> f32 {
    let mut coords = Vec::new();
    for part in points.split(|c: char| c.is_whitespace() || c == ',') {
        if part.is_empty() {
            continue;
        }
        if let Ok(v) = part.parse::<f32>() {
            coords.push(v);
        }
    }
    let mut len = 0.0f32;
    let mut i = 0;
    while i + 3 < coords.len() {
        let x0 = coords[i];
        let y0 = coords[i + 1];
        let x1 = coords[i + 2];
        let y1 = coords[i + 3];
        len += (x1 - x0).hypot(y1 - y0);
        i += 2;
    }
    len.max(1e-3)
}

/// Motion unitless pathLength/pathOffset → resvg-safe dash in user units.
///
/// Matches motion-dom `addSVGPathValue`:
/// - `stroke-dasharray = `${pathLength} ${pathSpacing ?? 1 - pathLength}``
/// - `stroke-dashoffset = ${-pathOffset}`  (negative!)
/// then scaled by real path length because resvg ignores SVG `pathLength`.
fn motion_dash_attrs(pl: f32, po: f32, total_len: f32) -> String {
    motion_dash_attrs_ex(pl, po, None, total_len)
}

fn motion_dash_attrs_ex(pl: f32, po: f32, path_spacing: Option<f32>, total_len: f32) -> String {
    let l = total_len.max(1e-3);
    let pl = pl.clamp(0.0, 1.0);
    // Motion default spacing is `1 - pathLength`. With SVG `pathLength="1"` the
    // dash period equals the path exactly. resvg ignores that attribute, and our
    // flattened length can undershoot the true stroke length — a short gap then
    // lets the *next* dash period leak onto the path (flicker / ghost residue,
    // e.g. activity drawing L→R while a stump flashes on the right).
    // Floor spacing at 1 so gap ≥ path length → at most one visible dash segment.
    let spacing = path_spacing.unwrap_or((1.0 - pl).max(0.0)).max(1.0);
    let dash = pl * l;
    let gap = spacing * l;
    let offset = -po * l;
    format!(" stroke-dasharray=\"{dash} {gap}\" stroke-dashoffset=\"{offset}\"")
}

/// Build one SVG document frame for `state` at `elapsed` seconds since state start.
pub fn emit_animated_icon_svg(
    recipe: &AnimatedIconRecipe,
    state: &str,
    elapsed: f32,
    color_css: &str,
) -> String {
    emit_animated_icon_svg_ex(recipe, state, elapsed, color_css, false)
}

fn emit_animated_icon_svg_ex(
    recipe: &AnimatedIconRecipe,
    state: &str,
    elapsed: f32,
    color_css: &str,
    bridging: bool,
) -> String {
    let root = sample_tracks(&recipe.root.tracks, state, elapsed, bridging);
    let scale = root.scale.unwrap_or(1.0);
    let sx = root.scale_x.unwrap_or(scale);
    let sy = root.scale_y.unwrap_or(scale);
    let rotate = root.rotate.unwrap_or(0.0);
    let tx = root.x.unwrap_or(0.0);
    let ty = root.y.unwrap_or(0.0);
    let root_op = root.opacity.unwrap_or(1.0);

    // If root animates strokeWidth (smile), temporarily override recipe stroke for children.
    let mut recipe_stroke = recipe.clone();
    if let Some(sw) = root.stroke_width {
        recipe_stroke.stroke_width = sw;
    }
    let recipe = &recipe_stroke;

    let mut inner = String::new();
    for node in &recipe.nodes {
        emit_recipe_node(
            &mut inner, node, recipe, state, elapsed, color_css, root_op, bridging,
        );
    }

    // Fallback: recipe has no drawable nodes — use lucide static inner if available.
    if inner.is_empty() {
        if let Some(raw) = crate::utils::lucide::lucide_svg_inner(&recipe.lucide) {
            inner = format!(
                "<g fill=\"none\" stroke=\"{color_css}\" stroke-width=\"{}\" stroke-linecap=\"{}\" stroke-linejoin=\"{}\">{raw}</g>",
                recipe.stroke_width, recipe.stroke_linecap, recipe.stroke_linejoin
            );
        }
    }

    // Keep root motion on an inner `<g>` — transform on the root `<svg>` is unreliable in resvg.
    let root_xf = xf_attr_xy(tx, ty, rotate, sx, sy);
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{vb}" fill="none"><g opacity="{root_op}"{root_xf}>{inner}</g></svg>"#,
        vb = recipe.view_box,
    )
}

fn xf_attr_xy(tx: f32, ty: f32, rotate: f32, sx: f32, sy: f32) -> String {
    if (sx - 1.0).abs() < 1e-4
        && (sy - 1.0).abs() < 1e-4
        && rotate.abs() < 1e-4
        && tx.abs() < 1e-4
        && ty.abs() < 1e-4
    {
        return String::new();
    }
    format!(
        " transform=\"translate({tx} {ty}) translate(12 12) rotate({rotate}) scale({sx} {sy}) translate(-12 -12)\""
    )
}

fn emit_recipe_node(
    out: &mut String,
    node: &RecipeNode,
    recipe: &AnimatedIconRecipe,
    state: &str,
    elapsed: f32,
    color_css: &str,
    parent_op: f32,
    bridging: bool,
) {
    let sp = sample_tracks(&node.tracks, state, elapsed, bridging);
    let op = sp.opacity.unwrap_or(1.0) * parent_op;
    let pl = sp.path_length.unwrap_or(1.0).clamp(0.0, 1.0);
    let po = sp.path_offset.unwrap_or(0.0);
    let ns = sp.scale.unwrap_or(1.0);
    let nsx = sp.scale_x.unwrap_or(ns);
    let nsy = sp.scale_y.unwrap_or(ns);
    let nr = sp.rotate.unwrap_or(0.0);
    let nx = sp.x.unwrap_or(0.0);
    let ny = sp.y.unwrap_or(0.0);
    let transform = xf_attr_xy(nx, ny, nr, nsx, nsy);
    let wants_dash = sp.path_length.is_some() || sp.path_offset.is_some();

    let fill_attr = node.attrs.get("fill").map(|s| s.as_str()).unwrap_or("none");
    let stroke_attr = node.attrs.get("stroke").map(|s| s.as_str());
    let fill_paint = if fill_attr == "none" || fill_attr.is_empty() {
        "none".to_string()
    } else if fill_attr == "currentColor" || fill_attr == "currentcolor" {
        color_css.to_string()
    } else {
        fill_attr.to_string()
    };
    let stroke_paint = match stroke_attr {
        Some("none") => "none".to_string(),
        Some(s) if s == "currentColor" || s == "currentcolor" => color_css.to_string(),
        Some(s) if !s.is_empty() => s.to_string(),
        _ => color_css.to_string(),
    };
    let sw = sp.stroke_width.unwrap_or(recipe.stroke_width).max(0.0);
    let stroke_w = if stroke_paint == "none" { 0.0 } else { sw };

    let stroke_common = format!(
        "fill=\"{fill_paint}\" stroke=\"{stroke_paint}\" stroke-width=\"{stroke_w}\" stroke-linecap=\"{}\" stroke-linejoin=\"{}\"",
        recipe.stroke_linecap, recipe.stroke_linejoin
    );

    match node.tag.as_str() {
        "g" => {
            out.push_str(&format!("<g opacity=\"{op}\"{transform}>"));
            for c in &node.children {
                emit_recipe_node(out, c, recipe, state, elapsed, color_css, op, bridging);
            }
            out.push_str("</g>");
        }
        "path" => {
            let d =
                sp.d.as_deref()
                    .or_else(|| node.attrs.get("d").map(|s| s.as_str()))
                    .unwrap_or("");
            if d.is_empty() {
                return;
            }
            let dash = if wants_dash {
                motion_dash_attrs(pl, po, svg_path_d_length(d))
            } else {
                String::new()
            };
            out.push_str(&format!(
                "<path d=\"{}\" {stroke_common} opacity=\"{op}\"{dash}{transform}/>",
                xml_escape_attr(d),
            ));
        }
        "circle" => {
            let cx = sp
                .cx
                .map(|v| format!("{v}"))
                .or_else(|| node.attrs.get("cx").cloned())
                .unwrap_or_else(|| "0".into());
            let cy = sp
                .cy
                .map(|v| format!("{v}"))
                .or_else(|| node.attrs.get("cy").cloned())
                .unwrap_or_else(|| "0".into());
            let r =
                sp.r.map(|v| format!("{v}"))
                    .or_else(|| node.attrs.get("r").cloned())
                    .unwrap_or_else(|| "0".into());
            let dash = if wants_dash {
                let rv = r.parse::<f32>().unwrap_or(1.0).abs().max(1e-3);
                motion_dash_attrs(pl, po, std::f32::consts::TAU * rv)
            } else {
                String::new()
            };
            out.push_str(&format!(
                "<circle cx=\"{cx}\" cy=\"{cy}\" r=\"{r}\" {stroke_common} opacity=\"{op}\"{dash}{transform}/>"
            ));
        }
        "line" => {
            let x1 = sp
                .x1
                .map(|v| format!("{v}"))
                .or_else(|| node.attrs.get("x1").cloned())
                .unwrap_or_else(|| "0".into());
            let y1 = sp
                .y1
                .map(|v| format!("{v}"))
                .or_else(|| node.attrs.get("y1").cloned())
                .unwrap_or_else(|| "0".into());
            let x2 = sp
                .x2
                .map(|v| format!("{v}"))
                .or_else(|| node.attrs.get("x2").cloned())
                .unwrap_or_else(|| "0".into());
            let y2 = sp
                .y2
                .map(|v| format!("{v}"))
                .or_else(|| node.attrs.get("y2").cloned())
                .unwrap_or_else(|| "0".into());
            let dash = if wants_dash {
                let ax = x1.parse::<f32>().unwrap_or(0.0);
                let ay = y1.parse::<f32>().unwrap_or(0.0);
                let bx = x2.parse::<f32>().unwrap_or(0.0);
                let by = y2.parse::<f32>().unwrap_or(0.0);
                motion_dash_attrs(pl, po, (bx - ax).hypot(by - ay).max(1e-3))
            } else {
                String::new()
            };
            out.push_str(&format!(
                "<line x1=\"{x1}\" y1=\"{y1}\" x2=\"{x2}\" y2=\"{y2}\" {stroke_common} opacity=\"{op}\"{dash}{transform}/>"
            ));
        }
        "polyline" => {
            let pts = node.attrs.get("points").map(|s| s.as_str()).unwrap_or("");
            if pts.is_empty() {
                return;
            }
            let dash = if wants_dash {
                motion_dash_attrs(pl, po, polyline_points_length(pts))
            } else {
                String::new()
            };
            out.push_str(&format!(
                "<polyline points=\"{}\" {stroke_common} opacity=\"{op}\"{dash}{transform}/>",
                xml_escape_attr(pts),
            ));
        }
        "polygon" => {
            let pts = node.attrs.get("points").map(|s| s.as_str()).unwrap_or("");
            if pts.is_empty() {
                return;
            }
            let dash = if wants_dash {
                motion_dash_attrs(pl, po, polyline_points_length(pts))
            } else {
                String::new()
            };
            out.push_str(&format!(
                "<polygon points=\"{}\" {stroke_common} opacity=\"{op}\"{dash}{transform}/>",
                xml_escape_attr(pts),
            ));
        }
        "rect" => {
            let x = node.attrs.get("x").map(|s| s.as_str()).unwrap_or("0");
            let y = node.attrs.get("y").map(|s| s.as_str()).unwrap_or("0");
            let w = if let Some(aw) = sp.width {
                format!("{aw}")
            } else {
                node.attrs
                    .get("width")
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "0".into())
            };
            let h = if let Some(ah) = sp.height {
                format!("{ah}")
            } else {
                node.attrs
                    .get("height")
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "0".into())
            };
            let rx = node.attrs.get("rx").map(|s| s.as_str()).unwrap_or("");
            let ry = node.attrs.get("ry").map(|s| s.as_str()).unwrap_or("");
            let rxa = if rx.is_empty() {
                String::new()
            } else {
                format!(" rx=\"{rx}\"")
            };
            let rya = if ry.is_empty() {
                String::new()
            } else {
                format!(" ry=\"{ry}\"")
            };
            // Prefer explicit stroke="none" from recipe for filled bars (battery).
            let rect_stroke = if stroke_attr == Some("none") {
                format!("fill=\"{fill_paint}\" stroke=\"none\" stroke-width=\"0\"")
            } else {
                stroke_common.clone()
            };
            out.push_str(&format!(
                "<rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\"{rxa}{rya} {rect_stroke} opacity=\"{op}\"{transform}/>"
            ));
        }
        _ => {}
    }
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

fn max_duration(recipe: &AnimatedIconRecipe, state: &str) -> f32 {
    let mut d = 0.4_f32;
    fn walk(tracks: &[RecipeTrack], nodes: &[RecipeNode], state: &str, d: &mut f32) -> bool {
        for t in tracks {
            if !t.keyframes.contains_key(state) {
                continue;
            }
            let dur = track_duration(t, &t.prop);
            let delay = t.transition.as_ref().and_then(|x| x.delay).unwrap_or(0.0);
            let repeat = t.transition.as_ref().and_then(|x| x.repeat).unwrap_or(0.0);
            let total = if repeat < 0.0 {
                return true; // infinity
            } else {
                delay + dur * (1.0 + repeat.max(0.0))
            };
            *d = d.max(total);
        }
        for n in nodes {
            if walk(&n.tracks, &n.children, state, d) {
                return true;
            }
        }
        false
    }
    if walk(&recipe.root.tracks, &recipe.nodes, state, &mut d) {
        return f32::INFINITY;
    }
    d
}

/// Advance playback; returns true if a redraw is still needed.
pub fn tick_animated_icon(el: &mut Element, _dt: f32) -> bool {
    #[cfg(feature = "svg")]
    let morph = el
        .get_attr(crate::utils::morph_icon::ATTR_MORPH_ICON)
        .is_some();
    #[cfg(not(feature = "svg"))]
    let morph = false;
    #[cfg(feature = "svg")]
    if morph && crate::utils::morph_icon::blocks_hover_animation(el) {
        reset_animated_icon_playback(el);
        return false;
    }
    let Some(name) = el.get_attr(ATTR_ANIMATED_ICON).cloned() else {
        return false;
    };
    let Some(recipe) = animated_icon_recipe(&name) else {
        return false;
    };
    if !recipe.animated {
        return false;
    }
    let state = el
        .get_attr(ATTR_ANIMATED_ICON_STATE)
        .map(|s| s.as_str())
        .unwrap_or("normal");
    let key = instance_key(el);
    let playback_store = play_store();
    let mut store = playback_store.lock().ok();
    let Some(ref mut map) = store else {
        return false;
    };
    let pb = map.entry(key).or_insert_with(|| Playback {
        state: state.to_string(),
        elapsed: 0.0,
        started_secs: now_secs(),
        bridging: morph && state != "normal",
    });
    if pb.state != state {
        // Attr changed without `set_animated_icon_state` — still treat as a bridge.
        pb.state = state.to_string();
        pb.started_secs = now_secs();
        pb.elapsed = 0.0;
        pb.bridging = true;
    }
    // Idle settled: no pump.
    if state == "normal" && !pb.bridging {
        return false;
    }
    pb.elapsed = (now_secs() - pb.started_secs).max(0.0);
    let max_d = max_duration(recipe, state);
    if max_d.is_finite() && pb.elapsed >= max_d {
        if pb.bridging {
            pb.bridging = false;
            pb.elapsed = max_d;
        }
        // Finite animate keeps last frame while still hovering; stop pump.
        return false;
    }
    true
}

pub fn set_animated_icon_state(el: &mut Element, state: &str) {
    el.set_attr(ATTR_ANIMATED_ICON_STATE, state);
    let key = instance_key(el);
    if let Ok(mut map) = play_store().lock() {
        map.insert(
            key,
            Playback {
                state: state.to_string(),
                elapsed: 0.0,
                started_secs: now_secs(),
                bridging: true,
            },
        );
    }
}

pub fn start_animated_icon(el: &mut Element) {
    set_animated_icon_state(el, "animate");
}

pub fn stop_animated_icon(el: &mut Element) {
    set_animated_icon_state(el, "normal");
}

pub fn reset_animated_icon_playback(el: &Element) {
    if let Ok(mut map) = play_store().lock() {
        map.remove(&instance_key(el));
    }
}

fn release_animated_icon(key: u64, template_id: &str) {
    if let Ok(mut map) = play_store().lock() {
        map.remove(&key);
    }
    super::icon_provider::release_icon_hover(template_id);
}

/// Rasterize current frame; returns texture key + logical size.
pub fn ensure_animated_icon_texture(
    el: &Element,
    size_px: f32,
    color: [u8; 4],
    dpr: f32,
) -> Option<(String, f32)> {
    #[cfg(feature = "svg")]
    if el
        .get_attr(crate::utils::morph_icon::ATTR_MORPH_ICON)
        .is_some()
    {
        return crate::utils::morph_icon::ensure_morph_icon_texture(el, size_px, color, dpr);
    }
    ensure_animated_icon_recipe_texture(el, size_px, color, dpr)
}

pub(crate) fn ensure_animated_icon_recipe_texture(
    el: &Element,
    size_px: f32,
    color: [u8; 4],
    dpr: f32,
) -> Option<(String, f32)> {
    let name = el.get_attr(ATTR_ANIMATED_ICON)?;
    let recipe = animated_icon_recipe(name)?;
    #[cfg(feature = "svg")]
    let morph_recipe = el
        .get_attr(crate::utils::morph_icon::ATTR_MORPH_ICON)
        .map(|_| {
            let mut recipe = recipe.clone();
            recipe.stroke_width = crate::utils::morph_icon::stroke_for_paint(el, size_px);
            recipe
        });
    #[cfg(feature = "svg")]
    let recipe = morph_recipe.as_ref().unwrap_or(recipe);
    let state = el
        .get_attr(ATTR_ANIMATED_ICON_STATE)
        .map(|s| s.as_str())
        .unwrap_or("normal");
    let key_id = instance_key(el);
    let (elapsed, bridging) = play_store()
        .lock()
        .ok()
        .and_then(|m| m.get(&key_id).map(|p| (p.elapsed, p.bridging)))
        .unwrap_or((0.0, false));

    let doc = emit_animated_icon_svg_ex(recipe, state, elapsed, &color_css(color), bridging);
    let size = size_px.clamp(8.0, 256.0);
    let dpr = dpr.clamp(1.0, 3.0);
    let side = (size * dpr).ceil().clamp(16.0, 512.0) as u32;

    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    name.hash(&mut hasher);
    state.hash(&mut hasher);
    // A key identifies pixels, including the final settled frame. Time buckets
    // alone can alias two different documents near an animation boundary.
    doc.hash(&mut hasher);
    bridging.hash(&mut hasher);
    side.hash(&mut hasher);
    color.hash(&mut hasher);
    let tex = format!("jadix_lucide_anim:{:016x}", hasher.finish());

    #[cfg(feature = "svg")]
    {
        let tex =
            crate::media::svg_raster::ensure_svg_document_texture_with_key(tex, &doc, side, side)?;
        return Some((tex, side as f32));
    }
    #[cfg(not(feature = "svg"))]
    {
        let _ = (doc, tex);
        // Fall back to static lucide texture.
        let glyph = crate::utils::lucide::lucide_glyph(name).unwrap_or_else(|| "\u{ea0e}".into());
        crate::utils::icon_texture::ensure_lucide_icon_texture(
            Some(name),
            &glyph,
            size_px,
            color,
            dpr,
        )
    }
}

/// Whether any animated icon in the tree needs the frame pump.
fn node_needs_animated_icon_frame(el: &Element) -> bool {
    // Collect already summarizes animated descendants. An invalidated hash must
    // fall through so newly mounted/bound icons are discovered before collection.
    if crate::authoring::presentation::is_cached_static(el) {
        return false;
    }
    #[cfg(feature = "svg")]
    let blocked = el
        .get_attr(crate::utils::morph_icon::ATTR_MORPH_ICON)
        .is_some()
        && crate::utils::morph_icon::blocks_hover_animation(el);
    #[cfg(not(feature = "svg"))]
    let blocked = false;
    if !blocked && el.get_attr(ATTR_ANIMATED_ICON).is_some() {
        let state = el
            .get_attr(ATTR_ANIMATED_ICON_STATE)
            .map(|s| s.as_str())
            .unwrap_or("normal");
        if let Some(name) = el.get_attr(ATTR_ANIMATED_ICON) {
            if let Some(recipe) = animated_icon_recipe(name) {
                if recipe.animated {
                    let key = instance_key(el);
                    let (elapsed, bridging) = play_store()
                        .lock()
                        .ok()
                        .and_then(|m| m.get(&key).map(|p| (p.elapsed, p.bridging)))
                        .unwrap_or((0.0, false));
                    let pumping = state != "normal" || bridging;
                    if pumping {
                        let max_d = max_duration(recipe, state);
                        if !max_d.is_finite() || elapsed < max_d {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

pub fn subtree_needs_animated_icon_frame(el: &Element) -> bool {
    node_needs_animated_icon_frame(el) || el.children.iter().any(subtree_needs_animated_icon_frame)
}

pub fn attach_playback(el: &mut Element) {
    if !el.attrs.contains_key("jadix_animated_icon_cleanup") {
        let previous = el.on_unmount.take();
        let key = instance_key(el);
        let template = el.template_id.clone().unwrap_or_default();
        let window = crate::authoring::window::Scope::capture();
        el.on_unmount = Some(crate::core::tree::UnmountHook::new(move || {
            let _window = window.enter();
            release_animated_icon(key, &template);
            if let Some(previous) = previous { previous.fire(); }
        }));
        el.set_attr("jadix_animated_icon_cleanup", "1");
    }
    crate::authoring::presentation::invalidate_paint(el);
    el.icon_playback = Some(crate::authoring::icon::Playback {
        tick: tick_animated_icon,
        needs_frame: node_needs_animated_icon_frame,
        texture: ensure_animated_icon_texture,
        hover_texture: ensure_animated_icon_recipe_texture,
        set_state: set_animated_icon_state,
        reset: reset_animated_icon_playback,
        detach: |element| {
            reset_animated_icon_playback(element);
            element.attrs.remove(ATTR_ANIMATED_ICON);
            element.attrs.remove(ATTR_ANIMATED_ICON_CONTROLLED);
        },
        controlled: el.get_attr(ATTR_ANIMATED_ICON_CONTROLLED).is_some(),
    });
}

/// Tick all animated icons under `el`.
pub fn tick_animated_icons_subtree(el: &mut Element, dt: f32) -> bool {
    // Collect already summarizes animated descendants. An invalidated hash must
    // fall through so newly mounted/bound icons are discovered before collection.
    if crate::authoring::presentation::is_cached_static(el) {
        return false;
    }
    let mut any = tick_animated_icon(el, dt);
    for c in &mut el.children {
        if tick_animated_icons_subtree(c, dt) {
            any = true;
        }
    }
    any
}

// ---------------------------------------------------------------------------
// Builder / component surface
// ---------------------------------------------------------------------------

pub struct AnimatedIconBuilder {
    name: String,
    size: f32,
    color: Option<[u8; 4]>,
    controlled: bool,
    /// MDN / CSS `pointer-events`（默认 SVG-only `bounding-box`）。
    pointer_events: Option<String>,
}

pub fn animated_icon(name: impl Into<String>) -> AnimatedIconBuilder {
    AnimatedIconBuilder {
        name: name.into(),
        size: 24.0,
        color: None,
        controlled: false,
        pointer_events: None,
    }
}

impl AnimatedIconBuilder {
    pub fn size(mut self, px: f32) -> Self {
        self.size = px.clamp(8.0, 256.0);
        self
    }

    pub fn size_px(mut self, px: f32) -> Self {
        self.size = px.clamp(8.0, 256.0);
        self
    }

    pub fn color(mut self, rgba: [u8; 4]) -> Self {
        self.color = Some(rgba);
        self
    }

    /// Disable auto-hover; drive via [`start_animated_icon`] / [`stop_animated_icon`].
    pub fn controlled(mut self, yes: bool) -> Self {
        self.controlled = yes;
        self
    }

    /// Hit policy（[CSS pointer-events](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/pointer-events)）。
    /// 默认 `bounding-box`（SVG 专用：命中容器）；亦支持通用 `auto` / `none` 与其它 SVG 关键字。
    pub fn pointer_events(mut self, mode: impl Into<String>) -> Self {
        self.pointer_events = Some(mode.into());
        self
    }
}

impl From<AnimatedIconBuilder> for Element {
    fn from(b: AnimatedIconBuilder) -> Self {
        static SEQ: AtomicU64 = AtomicU64::new(1);
        let name = b.name;
        let size = b.size;
        let controlled = b.controlled;
        let pointer_events = b.pointer_events;
        let mut el = Element::new()
            .w(size)
            .h(size)
            .min_w(size)
            .min_h(size)
            .text_size(size);
        el.set_attr(ATTR_ANIMATED_ICON, &name);
        el.set_attr(ATTR_ANIMATED_ICON_STATE, "normal");
        el.set_attr(ATTR_ANIMATED_ICON_SIZE, format!("{size}"));
        if controlled {
            el.set_attr(ATTR_ANIMATED_ICON_CONTROLLED, "1");
        }
        // Must use `.text_color` (sets `text_color` attr) so cascade does not overwrite
        // with parent/`TextStyle::default` white — that made icons invisible on light cards.
        let fg = b
            .color
            .unwrap_or_else(|| crate::style::parse_text_color("foreground"));
        el = el.text_color(fg);
        // Seed glyph/name so layout enters the icon paint path.
        let glyph = crate::utils::lucide::lucide_glyph(&name).unwrap_or_else(|| "\u{ea0e}".into());
        el.text = Some(glyph);
        el.text_style.font_family = Some("lucide".into());
        el.set_attr(crate::utils::ATTR_ICON_NAME, &name);
        let seq = SEQ.fetch_add(1, Ordering::Relaxed);
        let tid = format!("anim-icon:{name}:{seq}");
        el.template_id = Some(tid.clone());
        // Default: SVG `bounding-box` — hit own layout container (MDN SVG-only).
        el = el.pointer_events(pointer_events.as_deref().unwrap_or("bounding-box"));

        attach_playback(&mut el);
        if controlled {
            return el;
        }
        with_animated_icon_hover(el)
    }
}

impl IntoElementList for AnimatedIconBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

/// `ui!` / component entry: `<AnimatedIcon name="heart" size={28.0} />`
#[allow(non_snake_case)]
pub fn AnimatedIcon(name: impl AsRef<str>, size: Option<f32>, controlled: Option<bool>) -> Element {
    let mut b = animated_icon(name.as_ref()).size_px(size.unwrap_or(24.0));
    if controlled.unwrap_or(false) {
        b = b.controlled(true);
    }
    b.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(all(feature = "svg", feature = "bundled-lucide"))]
    use crate::utils::icon_provider::hover_intent_map;

    #[cfg(all(feature = "svg", feature = "bundled-lucide"))]
    #[test]
    fn morph_hover_switches_renderers_and_restarts_after_settling() {
        use crate::utils::morph_icon::{apply_bound_morph_icon, morph_to, tick_morph_icon};
        let mut el: Element = crate::utils::morph_icon("sun")
            .id("hybrid-hover-switch")
            .hover_animation(true)
            .label("Switch theme")
            .into();
        let key = instance_key(&el);
        let color = [25, 45, 65, 255];
        let texture = |el: &Element| {
            ensure_animated_icon_texture(el, 24.0, color, 1.0)
                .unwrap()
                .0
        };
        assert!(!subtree_needs_animated_icon_frame(&el));
        assert_eq!(
            el.get_attr("accesskit_name").map(String::as_str),
            Some("Switch theme")
        );

        (el.on_hover.as_ref().unwrap().borrow_mut())(true);
        sync_animated_icon_hover(&mut el);
        assert!(tick_animated_icon(&mut el, 0.016));
        assert!(texture(&el).starts_with("jadix_lucide_anim:"));
        // A nearly finished old recipe must not carry its playhead to the new icon.
        play_store().lock().unwrap().get_mut(&key).unwrap().elapsed = 9.0;
        apply_bound_morph_icon(&mut el, "moon");
        assert_eq!(
            el.get_attr(ATTR_ANIMATED_ICON).map(String::as_str),
            Some("moon")
        );
        assert!(!tick_animated_icon(&mut el, 0.016));
        assert!(!subtree_needs_animated_icon_frame(&el));
        assert!(texture(&el).starts_with("jadix_morph_icon:"));
        assert!(tick_morph_icon(&mut el, 0.016));
        // Retarget while morphing, then finish while still hovered.
        morph_to(&mut el, "heart");
        assert!(
            tick_morph_icon(&mut el, 10.0),
            "handoff must wake another frame"
        );
        let pb = play_store().lock().unwrap().get(&key).unwrap().clone();
        assert_eq!(pb.state, "animate");
        assert_eq!(pb.elapsed, 0.0);
        assert!(subtree_needs_animated_icon_frame(&el));
        assert!(texture(&el).starts_with("jadix_lucide_anim:"));

        (el.on_hover.as_ref().unwrap().borrow_mut())(false);
        sync_animated_icon_hover(&mut el);
        assert_eq!(
            el.get_attr(ATTR_ANIMATED_ICON_STATE).map(String::as_str),
            Some("normal")
        );
        // Deterministically finish the leave transition without sleeping.
        play_store()
            .lock()
            .unwrap()
            .get_mut(&key)
            .unwrap()
            .started_secs = now_secs() - 10.0;
        assert!(!tick_animated_icon(&mut el, 0.016));
        assert!(!subtree_needs_animated_icon_frame(&el));
        let tid = el.template_id.clone().unwrap();
        drop(el);
        assert!(!play_store().lock().unwrap().contains_key(&key));
        assert!(!hover_intent_map().lock().unwrap().contains_key(&tid));
    }

    #[cfg(all(feature = "svg", feature = "bundled-lucide"))]
    #[test]
    fn morph_hover_respects_controlled_geometry_reduced_motion_and_fallbacks() {
        use crate::utils::morph_icon::{
            apply_bound_morph_from, apply_bound_morph_reduced, morph_to,
        };
        let color = [50, 70, 90, 255];
        let texture = |el: &Element| {
            ensure_animated_icon_texture(el, 32.0, color, 1.0)
                .unwrap()
                .0
        };
        let mut el: Element = crate::utils::morph_icon("sun")
            .id("hybrid-controlled")
            .from_icon("sun")
            .to_icon("moon")
            .progress(0.5)
            .hover_animation(true)
            .into();
        start_animated_icon(&mut el);
        assert!(!tick_animated_icon(&mut el, 0.016));
        assert!(texture(&el).starts_with("jadix_morph_icon:"));
        apply_bound_morph_reduced(&mut el, "always");
        apply_bound_morph_from(&mut el, "");
        morph_to(&mut el, "heart");
        assert!(!tick_animated_icon(&mut el, 0.016));
        assert!(!subtree_needs_animated_icon_frame(&el));
        assert!(texture(&el).starts_with("jadix_morph_icon:"));

        apply_bound_morph_reduced(&mut el, "never");
        morph_to(&mut el, "M4 12 L20 12");
        crate::utils::morph_icon::tick_morph_icon(&mut el, 10.0);
        assert!(!tick_animated_icon(&mut el, 0.016));
        assert!(texture(&el).starts_with("jadix_morph_icon:"));
        let plain: Element = crate::utils::morph_icon("sun").into();
        assert!(plain.on_hover.is_none());
        assert!(plain.get_attr(ATTR_ANIMATED_ICON).is_none());
    }

    #[test]
    fn collect_summary_keeps_cold_and_new_animated_icons_live() {
        use crate::layout::collect_cache::{
            invalidate_collect_paint_hash_path, stamp_subtree_paint_hashes,
        };
        let mut root = Element::new().child(Element::new().text("static"));
        stamp_subtree_paint_hashes(&mut root, false);
        assert!(!subtree_needs_animated_icon_frame(&root));
        assert!(!tick_animated_icons_subtree(&mut root, 0.016));
        let mut icon: Element = animated_icon("menu").controlled(true).into();
        set_animated_icon_state(&mut icon, "animate");
        root.children[0] = icon;
        invalidate_collect_paint_hash_path(&mut root, &[0]);
        assert!(
            subtree_needs_animated_icon_frame(&root),
            "invalid summary must walk new icon"
        );
        stamp_subtree_paint_hashes(&mut root, true);
        assert!(root.collect_skip_live);
        assert!(
            subtree_needs_animated_icon_frame(&root),
            "cached summary must retain icon path"
        );
    }

    #[test]
    fn recipes_loaded() {
        assert!(recipes().len() >= 400, "len={}", recipes().len());
        assert!(animated_icon_recipe("heart").is_some());
        assert!(animated_icon_recipe("check").is_some());
        assert!(animated_icon_recipe("menu").is_some());
        assert!(animated_icon_recipe("download").is_some());
    }

    #[test]
    fn animated_icon_sets_text_color_attr_for_cascade() {
        let el: Element = animated_icon("heart").size_px(28.0).controlled(true).into();
        let fg = crate::style::parse_text_color("foreground");
        assert_eq!(el.text_style.color, Some(fg));
        assert!(
            el.text_style.color.is_some(),
            "explicit color must survive cascade (no parent white overwrite)"
        );
        assert_ne!(el.text_style.color_resolved(), [255, 255, 255, 255]);
    }

    #[test]
    fn heart_scale_keyframes_emit() {
        let r = animated_icon_recipe("heart").unwrap();
        let doc0 = emit_animated_icon_svg(r, "animate", 0.15, "#000");
        assert!(doc0.contains("<svg"), "{doc0}");
        assert!(doc0.contains("scale("), "{doc0}");
        let doc1 = emit_animated_icon_svg(r, "normal", 0.0, "#000");
        assert!(doc1.contains("<path"), "{doc1}");
    }

    #[test]
    fn settings_single_value_rotate_lerps_from_normal() {
        let r = animated_icon_recipe("settings").unwrap();
        // Bridging on: mid-tween between normal(0) and animate(180).
        let mid = emit_animated_icon_svg_ex(r, "animate", 0.2, "#000", true);
        assert!(mid.contains("rotate("), "{mid}");
        assert!(!mid.contains("rotate(0)"), "{mid}");
        // Idle (no bridge): must be identity, not stuck at animate end.
        let idle = emit_animated_icon_svg(r, "normal", 0.0, "#000");
        assert!(
            !idle.contains("rotate(90)") && !idle.contains("rotate(180)"),
            "idle must not show animate end pose: {idle}"
        );
    }

    #[test]
    fn menu_and_copy_idle_not_animate_end() {
        let menu = animated_icon_recipe("menu").unwrap();
        let idle = emit_animated_icon_svg(menu, "normal", 0.0, "#000");
        // Three horizontal lines — no X (45/-45) transform at idle.
        assert!(!idle.contains("rotate(45)"), "{idle}");
        assert!(!idle.contains("rotate(-45)"), "{idle}");
        assert!(!idle.contains("translate(0 6)"), "{idle}");

        let copy = animated_icon_recipe("copy").unwrap();
        let idle_c = emit_animated_icon_svg(copy, "normal", 0.0, "#000");
        // Split offset is animate end; idle must not translate by ±3.
        assert!(!idle_c.contains("translate(-3"), "{idle_c}");
        assert!(!idle_c.contains("translate(3"), "{idle_c}");
    }

    #[test]
    fn download_group_nests_polyline() {
        let r = animated_icon_recipe("download").unwrap();
        let g = r.nodes.iter().find(|n| n.tag == "g").expect("g node");
        assert!(
            g.children.iter().any(|c| c.tag == "polyline"),
            "arrow polyline must be nested under motion.g"
        );
        let doc = emit_animated_icon_svg(r, "animate", 0.3, "#222");
        assert!(doc.contains("<polyline"), "{doc}");
        assert!(doc.contains("<g "), "{doc}");
    }

    #[test]
    fn menu_custom_variants_materialized() {
        let r = animated_icon_recipe("menu").unwrap();
        assert_eq!(r.nodes.len(), 3);
        let top = &r.nodes[0];
        let rot = top
            .tracks
            .iter()
            .find(|t| t.prop == "rotate")
            .expect("rotate");
        assert_eq!(
            rot.keyframes.get("animate").and_then(value_as_f32),
            Some(45.0)
        );
        let mid = &r.nodes[1];
        let op = mid
            .tracks
            .iter()
            .find(|t| t.prop == "opacity")
            .expect("opacity");
        assert_eq!(
            op.keyframes.get("animate").and_then(value_as_f32),
            Some(0.0)
        );
    }

    #[test]
    fn check_path_length_dash() {
        let r = animated_icon_recipe("check").unwrap();
        let d = r.nodes[0].attrs.get("d").unwrap();
        let total = svg_path_d_length(d);
        assert!(
            total > 5.0,
            "check path should be longer than a few ux, got {total}"
        );
        let doc = emit_animated_icon_svg(r, "animate", 0.0, "#111");
        // t=0 → pathLength 0 → dash starts at 0 (hidden), scaled by real length.
        assert!(doc.contains("stroke-dasharray=\"0 "), "{doc}");
        assert!(
            !doc.contains("pathLength=\""),
            "resvg ignores pathLength; omit it: {doc}"
        );
    }

    #[test]
    fn activity_mid_draws_half_path_in_user_units() {
        let r = animated_icon_recipe("activity").unwrap();
        let d = r.nodes[0].attrs.get("d").unwrap();
        let total = svg_path_d_length(d);
        let mid = emit_animated_icon_svg(r, "animate", 0.3, "#000");
        // At t=0.5: pl≈0.5, po≈0.5 → dash≈0.5L, gap≥L (anti-residue), offset≈-0.5L.
        let half = total * 0.5;
        assert!(
            mid.contains(&format!("stroke-dasharray=\"{half} {total}\"")),
            "expected dash={half} gap≥L ({total}), got {mid}"
        );
        assert!(
            mid.contains(&format!("stroke-dashoffset=\"-{half}\"")),
            "pathOffset must negate for Motion parity: {mid}"
        );
        assert!(!mid.contains("stroke-dasharray=\"0.5 1\""), "{mid}");
    }

    /// Activity path is authored right→left; Motion's pl↑ + po↓ must reveal left→right.
    #[cfg(feature = "svg")]
    #[test]
    fn activity_draw_progresses_left_to_right() {
        use crate::media::svg_raster::rasterize_svg_str;
        let r = animated_icon_recipe("activity").unwrap();
        let size = 96u32;
        let ink_left_ratio = |elapsed: f32| -> f32 {
            let doc = emit_animated_icon_svg(r, "animate", elapsed, "rgb(0,0,0)");
            let rgba = rasterize_svg_str(&doc, size, size).expect("raster");
            let mut left = 0u32;
            let mut right = 0u32;
            let mid = size / 2;
            for y in 0..size {
                for x in 0..size {
                    let i = ((y * size + x) * 4) as usize;
                    if rgba[i + 3] > 20 {
                        if x < mid {
                            left += 1;
                        } else {
                            right += 1;
                        }
                    }
                }
            }
            let t = (left + right).max(1) as f32;
            left as f32 / t
        };
        // Early: mostly left half. Late: more balanced / includes right.
        let early = ink_left_ratio(0.12);
        let late = ink_left_ratio(0.48);
        assert!(
            early > 0.7,
            "early draw should start on the left (ratio={early})"
        );
        assert!(
            late < early,
            "as path progresses, right side should gain ink (early={early} late={late})"
        );
    }

    #[test]
    fn check_mid_reveals_partial_stroke() {
        let r = animated_icon_recipe("check").unwrap();
        let d = r.nodes[0].attrs.get("d").unwrap();
        let total = svg_path_d_length(d);
        let mid = emit_animated_icon_svg(r, "animate", 0.2, "#111");
        // pathLength ~0.5 at mid (easeOut may skew); dash must be a sizable fraction of L.
        let dash = mid
            .split("stroke-dasharray=\"")
            .nth(1)
            .and_then(|s| s.split_whitespace().next())
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(0.0);
        assert!(
            dash > total * 0.15 && dash < total * 0.95,
            "mid dash {dash} should be partial of total {total}: {mid}"
        );
        assert!(mid.contains("scale("), "check scales while drawing: {mid}");
        // Anti-residue: gap floored to ≥ L so a second dash period cannot leak.
        let gap = mid
            .split("stroke-dasharray=\"")
            .nth(1)
            .and_then(|s| s.split(|c: char| c == '"' || c.is_whitespace()).nth(1))
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(-1.0);
        assert!(
            gap + 0.5 >= total,
            "gap {gap} should be ≥ total {total}: {mid}"
        );
    }

    #[test]
    fn sun_map_paths_have_staggered_opacity() {
        let r = animated_icon_recipe("sun").unwrap();
        let rays: Vec<_> = r.nodes.iter().filter(|n| n.tag == "path").collect();
        assert!(rays.len() >= 8, "sun rays={}", rays.len());
        assert!(rays.iter().all(|n| n.attrs.get("d").is_some()));
        let delays: Vec<f32> = rays
            .iter()
            .filter_map(|n| {
                n.tracks
                    .iter()
                    .find(|t| t.prop == "opacity")
                    .and_then(|t| t.transition.as_ref())
                    .and_then(|tr| tr.delay)
            })
            .collect();
        assert!(delays.len() >= 8, "{delays:?}");
        assert!(delays.windows(2).all(|w| w[1] > w[0] - 1e-4), "{delays:?}");
        let mid = emit_animated_icon_svg(r, "animate", 0.25, "#000");
        assert!(mid.matches("<path").count() >= 8, "{mid}");
    }

    #[test]
    fn wifi_levels_extracted_with_fade_sequence() {
        let r = animated_icon_recipe("wifi").unwrap();
        assert!(r.animated);
        assert_eq!(r.nodes.len(), 4);
        // Dot stays opaque; rings flash out then in.
        let ring = &r.nodes[1];
        let op = ring
            .tracks
            .iter()
            .find(|t| t.prop == "opacity")
            .expect("opacity");
        let anim = op.keyframes.get("animate").expect("animate");
        let series = keyframe_series(anim);
        assert!(series.len() >= 2, "{series:?}");
        assert!(series.iter().any(|v| *v < 0.5), "must dip: {series:?}");
        let doc = emit_animated_icon_svg(r, "animate", 0.2, "#111");
        assert!(doc.matches("<path").count() >= 4, "{doc}");
    }

    #[test]
    fn battery_width_and_fill_animate() {
        let r = animated_icon_recipe("battery").unwrap();
        let fill = r
            .nodes
            .iter()
            .find(|n| n.tag == "rect" && n.attrs.get("fill").is_some())
            .expect("fill rect");
        assert!(fill.tracks.iter().any(|t| t.prop == "width"));
        let idle = emit_animated_icon_svg(r, "normal", 0.0, "#222");
        assert!(
            idle.contains("width=\"0\"") || idle.contains("opacity=\"0\""),
            "{idle}"
        );
        let end = emit_animated_icon_svg_ex(r, "animate", 0.4, "#222", true);
        assert!(
            end.contains("width=\"12\"") || end.contains("width=\"12"),
            "{end}"
        );
        assert!(
            end.contains("fill=\"#222\"") || end.contains("fill=\"rgb"),
            "{end}"
        );
    }

    #[test]
    fn smile_and_align_and_grip_recipes_animate() {
        let smile = animated_icon_recipe("smile").unwrap();
        assert!(smile.animated);
        assert!(
            smile.root.tracks.iter().any(|t| t.prop == "scale"),
            "face scale on root"
        );
        assert!(
            smile
                .nodes
                .iter()
                .any(|n| n.tracks.iter().any(|t| t.prop == "d")),
            "mouth d morph track"
        );

        let align = animated_icon_recipe("align-left").unwrap();
        let mid = emit_animated_icon_svg_ex(align, "animate", 0.2, "#000", true);
        assert!(mid.contains("x2=\"1"), "x2 should tween toward 19: {mid}");

        let grip = animated_icon_recipe("grip").unwrap();
        assert_eq!(grip.nodes.len(), 9);
        let t0 = &grip.nodes[0].tracks[0];
        assert!(matches!(
            t0.keyframes.get("animate"),
            Some(serde_json::Value::Array(a)) if a.len() >= 3
        ));
        let blink = emit_animated_icon_svg(grip, "animate", 0.3, "#000");
        assert!(blink.matches("<circle").count() >= 9, "{blink}");

        let volume = animated_icon_recipe("volume").unwrap();
        assert!(volume.animated);
        assert!(volume.nodes.iter().any(|n| n.tag == "line"), "mute X lines");
        let v_idle = emit_animated_icon_svg(volume, "normal", 0.0, "#000");
        assert!(v_idle.contains("<line"), "{v_idle}");
        let v_anim = emit_animated_icon_svg(volume, "animate", 0.3, "#000");
        assert!(
            v_anim.contains("M16 9a5") || v_anim.contains("opacity="),
            "{v_anim}"
        );

        let keyboard = animated_icon_recipe("keyboard").unwrap();
        assert!(keyboard.animated);
        assert!(keyboard.nodes.len() >= 8);
        let k = emit_animated_icon_svg(keyboard, "animate", 0.4, "#000");
        assert!(k.matches("<path").count() >= 8, "{k}");
    }

    #[test]
    fn check_draw_and_scale_at_mid() {
        let r = animated_icon_recipe("check").unwrap();
        let d = r.nodes[0].attrs.get("d").unwrap();
        let total = svg_path_d_length(d);
        let mid = emit_animated_icon_svg(r, "animate", 0.2, "#111");
        assert!(mid.contains("stroke-dasharray="), "{mid}");
        assert!(mid.contains("scale("), "check scales while drawing: {mid}");
        let idle = emit_animated_icon_svg(r, "normal", 0.0, "#111");
        // Idle pathLength=1 → dash = L; gap floored to ≥ L (still fully covered).
        assert!(
            idle.contains(&format!("stroke-dasharray=\"{total} {total}\"")),
            "idle should be solid full path: {idle}"
        );
    }

    #[cfg(feature = "svg")]
    #[test]
    fn resvg_anim_frame_nonempty() {
        let mut el: Element = animated_icon("bell").size_px(32.0).controlled(true).into();
        set_animated_icon_state(&mut el, "animate");
        let fg = crate::style::parse_text_color("foreground");
        let (tex, _) = ensure_animated_icon_texture(&el, 32.0, fg, 2.0).expect("tex");
        assert!(tex.starts_with("jadix_lucide_anim:"));
    }

    #[cfg(feature = "svg")]
    #[test]
    fn unchanged_animated_frame_reuses_resident_texture() {
        let el: Element = animated_icon("bell").size_px(31.0).into();
        let color = [13, 37, 91, 255];
        let (key, _) = ensure_animated_icon_texture(&el, 31.0, color, 1.0).unwrap();
        crate::media::svg_raster::flush_svg_textures(|_| false);
        assert!(crate::app::pending_images::take_pending_key(&key).is_some());
        assert_eq!(
            ensure_animated_icon_texture(&el, 31.0, color, 1.0)
                .unwrap()
                .0,
            key
        );
        crate::media::svg_raster::flush_svg_textures(|_| true);
        assert!(
            crate::app::pending_images::take_pending_key(&key).is_none(),
            "an unchanged resident frame must not rasterize/upload on every scroll"
        );
        ensure_animated_icon_texture(&el, 31.0, color, 1.0).unwrap();
        crate::media::svg_raster::flush_svg_textures(|_| false);
        assert!(
            crate::app::pending_images::take_pending_key(&key).is_some(),
            "GPU eviction must still rehydrate the frame"
        );
    }

    #[cfg(feature = "svg")]
    #[test]
    fn animated_frame_key_tracks_pixels_within_one_time_bucket() {
        let mut el: Element = animated_icon("bell").size_px(31.0).into();
        set_animated_icon_state(&mut el, "animate");
        let sample = |elapsed: f32| {
            play_store()
                .lock()
                .unwrap()
                .get_mut(&instance_key(&el))
                .unwrap()
                .elapsed = elapsed;
            ensure_animated_icon_texture(&el, 31.0, [13, 37, 91, 255], 1.0)
                .unwrap()
                .0
        };
        assert_ne!(
            sample(0.101),
            sample(0.102),
            "different SVG pixels in one 60Hz bucket must not alias in the raster cache"
        );
    }
}
