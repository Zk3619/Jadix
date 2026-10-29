//! Lucide-animated recipes ([pqoqubbw/icons](https://github.com/pqoqubbw/icons)) → SVG frame bake.
//!
//! Recipes are extracted offline (`scripts/assets/extract_lucide_animated.mjs`) into
//! `assets/lucide_animated.json`. Playback is hover / imperative; frames go through resvg
//! (or CPU path) like public `<svg>` / Lucide static icons — no React/Motion runtime.

use crate::catalog::serde_json;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use std::sync::atomic::{AtomicU64, Ordering};

pub use super::icon_provider::sync_icon_hover as sync_animated_icon_hover;
pub(crate) use super::icon_provider::with_icon_hover as with_animated_icon_hover;
use crate::core::tree::{Element, IntoElementList};

pub const ATTR_ANIMATED_ICON: &str = "jadix_animated_icon";
pub const ATTR_ANIMATED_ICON_STATE: &str = super::icon_provider::ATTR_ICON_ANIMATION_STATE;
pub const ATTR_ANIMATED_ICON_CONTROLLED: &str = "jadix_animated_icon_controlled";
pub const ATTR_ANIMATED_ICON_SIZE: &str = "jadix_animated_icon_size";

pub use crate::recipes::*;

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
                        #[cfg(feature = "animation-path")]
                        {
                            out.d = crate::utils::icon_morph::morph_src_at(
                                &series[i],
                                &series[i + 1],
                                f,
                            )
                            .or_else(snap);
                        }
                        #[cfg(not(feature = "animation-path"))]
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
///
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
        if let Some(raw) = crate::utils::lucide_svg_inner(&recipe.lucide) {
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

#[expect(
    clippy::too_many_arguments,
    reason = "The existing rendering adapter keeps its independent geometry, state and callback parameters"
)]
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
    #[cfg(feature = "animation-path")]
    let morph = el
        .get_attr(crate::utils::morph_icon::ATTR_MORPH_ICON)
        .is_some();
    #[cfg(not(feature = "animation-path"))]
    let morph = false;
    #[cfg(feature = "animation-path")]
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
    #[cfg(feature = "animation-path")]
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
    #[cfg(feature = "animation-path")]
    let morph_recipe = el
        .get_attr(crate::utils::morph_icon::ATTR_MORPH_ICON)
        .map(|_| {
            let mut recipe = recipe.clone();
            recipe.stroke_width = crate::utils::morph_icon::stroke_for_paint(el, size_px);
            recipe
        });
    #[cfg(feature = "animation-path")]
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
        Some((tex, side as f32))
    }
    #[cfg(not(feature = "svg"))]
    {
        let _ = (doc, tex);
        // Fall back to static lucide texture.
        let glyph = crate::utils::lucide_glyph(name).unwrap_or_else(|| "\u{ea0e}".into());
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
    #[cfg(feature = "animation-path")]
    let blocked = el
        .get_attr(crate::utils::morph_icon::ATTR_MORPH_ICON)
        .is_some()
        && crate::utils::morph_icon::blocks_hover_animation(el);
    #[cfg(not(feature = "animation-path"))]
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
            if let Some(previous) = previous {
                previous.fire();
            }
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
        let glyph = crate::utils::lucide_glyph(&name).unwrap_or_else(|| "\u{ea0e}".into());
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

#[cfg(all(test, feature = "product-tests"))]
#[path = "tests/lucide_animated.rs"]
mod tests;
