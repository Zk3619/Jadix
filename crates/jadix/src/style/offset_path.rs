//! CSS `offset-path` / `offset-distance` / `offset-rotate` (motion path).
//!
//! Sampled at paint time so animation can write `offset_distance` without baking
//! extra translate into the CSS baseline.

use super::element_style::Style;

/// Author `offset-path`. Geometry is sampled each frame.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum OffsetPath {
    #[default]
    None,
    /// SVG `d` in border-box px (same convention as `clip-path: path()`).
    Path(String),
    /// `url(#id)` → named clip-path polygon vertices as the motion path.
    Url(String),
}

impl OffsetPath {
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }
}

/// Parse `none` / `path("…")` / `url(#id)`.
pub fn parse_offset_path(raw: &str) -> Option<OffsetPath> {
    let s = raw.trim();
    if s.eq_ignore_ascii_case("none") {
        return Some(OffsetPath::None);
    }
    let lower = s.to_ascii_lowercase();
    if lower.starts_with("path(") {
        let inner = strip_fn_ci(s, "path")?;
        let d = unquote(inner.trim())?;
        if d.is_empty() {
            return None;
        }
        return Some(OffsetPath::Path(d.to_string()));
    }
    if lower.starts_with("url(") {
        let inner = strip_fn_ci(s, "url")?;
        let id = super::clip_path::parse_url_hash(inner)?;
        return Some(OffsetPath::Url(id));
    }
    None
}

fn strip_fn_ci<'a>(s: &'a str, name: &str) -> Option<&'a str> {
    let s = s.trim();
    if s.len() < name.len() + 2 {
        return None;
    }
    if !s.get(..name.len())?.eq_ignore_ascii_case(name) {
        return None;
    }
    s[name.len()..]
        .trim_start()
        .strip_prefix('(')?
        .strip_suffix(')')
        .map(str::trim)
}

fn unquote(s: &str) -> Option<&str> {
    let s = s.trim();
    if let Some(inner) = s.strip_prefix('"').and_then(|r| r.strip_suffix('"')) {
        return Some(inner);
    }
    if let Some(inner) = s.strip_prefix('\'').and_then(|r| r.strip_suffix('\'')) {
        return Some(inner);
    }
    Some(s)
}

/// Extra translate (px) + tangent degrees to compose on top of independent transform.
#[derive(Clone, Copy, Debug, Default)]
pub struct OffsetSample {
    pub translate: [f32; 2],
    pub rotate_deg: f32,
}

/// Sample `offset-path` at `style.offset_distance` (0–1 of path length).
pub fn sample(style: &Style, box_wh: [f32; 2]) -> OffsetSample {
    let pts = match &style.offset_path {
        OffsetPath::None => return OffsetSample::default(),
        OffsetPath::Path(d) => polyline_from_d(d),
        OffsetPath::Url(id) => super::clip_path::lookup_clip_path(id)
            .map(|c| c.to_world_polygon([0.0, 0.0, box_wh[0], box_wh[1]]))
            .unwrap_or_default(),
    };
    if pts.len() < 2 {
        return OffsetSample::default();
    }
    let (pos, tangent) = sample_polyline(&pts, style.offset_distance.clamp(0.0, 1.0));
    let auto_deg = tangent.to_degrees();
    let rotate_deg = style.offset_rotate_deg.unwrap_or(auto_deg);
    OffsetSample {
        translate: pos,
        rotate_deg,
    }
}

fn polyline_from_d(d: &str) -> Vec<[f32; 2]> {
    crate::primitives::path::points_from_commands(&crate::primitives::path::decode_path_d(d))
        .into_iter()
        .map(|(x, y)| [x, y])
        .collect()
}

/// Bound the same polyline used by `sample`, including every intermediate point.
pub(super) fn max_translate(style: &Style, box_wh: [f32; 2]) -> Option<f32> {
    let points = match &style.offset_path {
        OffsetPath::None => return Some(0.0),
        OffsetPath::Path(d) => polyline_from_d(d),
        OffsetPath::Url(id) => super::clip_path::lookup_clip_path(id)?
            .to_world_polygon([0.0, 0.0, box_wh[0], box_wh[1]]),
    };
    points.into_iter().try_fold(0.0_f32, |radius, [x, y]| {
        (x.is_finite() && y.is_finite()).then(|| radius.max(x.hypot(y)))
    })
}

fn sample_polyline(pts: &[[f32; 2]], t: f32) -> ([f32; 2], f32) {
    let mut lens = Vec::with_capacity(pts.len());
    lens.push(0.0f32);
    let mut total = 0.0f32;
    for w in pts.windows(2) {
        let dx = w[1][0] - w[0][0];
        let dy = w[1][1] - w[0][1];
        total += (dx * dx + dy * dy).sqrt();
        lens.push(total);
    }
    if total < 1e-6 {
        return (pts[0], 0.0);
    }
    let target = t * total;
    for i in 0..pts.len() - 1 {
        if lens[i + 1] + 1e-6 >= target {
            let seg = (lens[i + 1] - lens[i]).max(1e-6);
            let u = ((target - lens[i]) / seg).clamp(0.0, 1.0);
            let x = pts[i][0] + (pts[i + 1][0] - pts[i][0]) * u;
            let y = pts[i][1] + (pts[i + 1][1] - pts[i][1]) * u;
            let ang = (pts[i + 1][1] - pts[i][1]).atan2(pts[i + 1][0] - pts[i][0]);
            return ([x, y], ang);
        }
    }
    let last = pts.len() - 1;
    let ang = (pts[last][1] - pts[last - 1][1]).atan2(pts[last][0] - pts[last - 1][0]);
    (pts[last], ang)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_midpoint_is_halfway() {
        let mut style = Style::default();
        style.offset_path = parse_offset_path(r#"path("M 0 0 L 100 0")"#).unwrap();
        style.offset_distance = 0.5;
        let s = sample(&style, [100.0, 20.0]);
        assert!((s.translate[0] - 50.0).abs() < 1e-3, "{:?}", s.translate);
        assert!(s.translate[1].abs() < 1e-3);
        assert!(s.rotate_deg.abs() < 1.0);
    }
}
