//! Stroke-icon morph core (morphicons): parse → cubics → resample → plan → polar interp → `d`.
//!
//! Compiled only with the `svg` feature. Stroke-centerline icons only
//! (Lucide / Tabler / Heroicons outline, or a raw `d`) — not a general SVG morpher.

mod interpolate;
mod markup;
mod normalize;
mod parse;
mod plan;
mod resample;
mod serialize;
mod types;

pub use interpolate::{alloc_outputs, interp_linear, interp_polar};
pub use markup::markup_to_icon;
pub use normalize::{canonical_d, fit_icon, icon_to_cubics};
pub use plan::{build_plan, MorphPlan, PlanItem};
pub use resample::{resample_icon, resample_path, SAMPLE_N};
pub use serialize::{cubics_to_path_d, serialize};
pub use types::{CubicPath, IconInput, IconNode, MorphError, Sampled};

/// morphicons `svgToIcon`: strict stroke-centerline scan + viewBox fit onto 24.
pub fn svg_to_icon(markup: &str) -> Result<IconInput, MorphError> {
    markup_to_icon(markup, false)
}

/// Resolve a Lucide name, raw `d`, or SVG markup into morph input.
pub fn icon_input_from_src(src: &str) -> Option<IconInput> {
    let t = src.trim();
    if t.is_empty() {
        return None;
    }
    if t.starts_with('<') {
        return markup_to_icon(t, true).ok();
    }
    if looks_like_path_d(t) {
        return Some(IconInput::D(t.to_string()));
    }
    crate::utils::icon_provider::resolve_icon_svg(t).and_then(|inner| markup_to_icon(&inner, true).ok())
}

fn looks_like_path_d(s: &str) -> bool {
    let t = s.trim_start();
    let b = t.as_bytes();
    if b.is_empty() || (b[0] != b'M' && b[0] != b'm') {
        return false;
    }
    let rest = t[1..].trim_start().as_bytes();
    rest.first()
        .is_some_and(|c| c.is_ascii_digit() || *c == b'+' || *c == b'-' || *c == b'.')
}

/// Polar-interp `from` → `to` at `t` ∈ ℝ (exact snap at 0 and 1).
pub fn morph_d_at(from: &IconInput, to: &IconInput, t: f64) -> Result<String, MorphError> {
    morph_d_at_kind(from, to, t, false)
}

/// `linear` uses raw coordinate lerp (playground polar-vs-linear).
pub fn morph_d_at_kind(
    from: &IconInput,
    to: &IconInput,
    t: f64,
    linear: bool,
) -> Result<String, MorphError> {
    if t <= 0.0 {
        return canonical_d(from);
    }
    if t >= 1.0 {
        return canonical_d(to);
    }
    let a = resample_icon(from, SAMPLE_N)?;
    let b = resample_icon(to, SAMPLE_N)?;
    let plan = build_plan(&a, &b)?;
    Ok(serialize_plan(&plan, t, linear))
}

fn serialize_plan(plan: &MorphPlan, t: f64, linear: bool) -> String {
    let mut out = alloc_outputs(plan);
    if linear {
        interp_linear(plan, t, &mut out);
    } else {
        interp_polar(plan, t, &mut out);
    }
    let closed: Vec<bool> = plan.items.iter().map(|it| it.closed).collect();
    serialize(&out, &closed)
}

/// Morph two `d` strings (or Lucide names) at progress `t`.
pub fn morph_src_at(from: &str, to: &str, t: f64) -> Option<String> {
    let a = icon_input_from_src(from)?;
    let b = icon_input_from_src(to)?;
    morph_d_at(&a, &b, t).ok()
}

pub(crate) fn interp_plan_d_kind(plan: &MorphPlan, t: f64, linear: bool) -> String {
    serialize_plan(plan, t, linear)
}

pub(crate) fn sampled_from_outputs(plan: &MorphPlan, t: f64) -> Vec<Sampled> {
    let mut out = alloc_outputs(plan);
    interp_polar(plan, t, &mut out);
    out.into_iter()
        .zip(plan.items.iter())
        .map(|(pts, it)| Sampled {
            pts,
            closed: it.closed,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::icon_morph::plan::procrustes;

    fn line(x1: f64, y1: f64, x2: f64, y2: f64) -> IconInput {
        IconInput::D(format!("M{x1} {y1}L{x2} {y2}"))
    }

    #[test]
    fn parse_and_canonical_roundtrip_line() {
        let d = canonical_d(&line(0.0, 0.0, 10.0, 0.0)).unwrap();
        assert!(d.starts_with('M'), "{d}");
        assert!(d.contains('C'), "line lowers to cubic: {d}");
    }

    #[test]
    fn endpoints_snap_to_canonical() {
        let a = line(0.0, 12.0, 24.0, 12.0);
        let b = line(12.0, 0.0, 12.0, 24.0);
        let d0 = morph_d_at(&a, &b, 0.0).unwrap();
        let d1 = morph_d_at(&a, &b, 1.0).unwrap();
        assert_eq!(d0, canonical_d(&a).unwrap());
        assert_eq!(d1, canonical_d(&b).unwrap());
    }

    #[test]
    fn mid_flight_is_polyline() {
        let a = line(0.0, 12.0, 24.0, 12.0);
        let b = line(12.0, 0.0, 12.0, 24.0);
        let mid = morph_d_at(&a, &b, 0.5).unwrap();
        assert!(mid.contains('L'), "in-flight d is polyline: {mid}");
        assert!(!mid.is_empty());
    }

    #[test]
    fn rotated_line_is_pure_rotation() {
        let a = resample_icon(&line(0.0, 12.0, 24.0, 12.0), SAMPLE_N).unwrap();
        let b = resample_icon(&line(12.0, 0.0, 12.0, 24.0), SAMPLE_N).unwrap();
        let plan = build_plan(&a, &b).unwrap();
        assert_eq!(plan.items.len(), 1);
        let it = &plan.items[0];
        assert!(
            it.res < 0.05,
            "congruent lines should have tiny residual, got {}",
            it.res
        );
        let deg = it.theta.abs() * 180.0 / std::f64::consts::PI;
        assert!(
            (deg - 90.0).abs() < 5.0,
            "expected ~90° rotation, got {deg}"
        );
        let _ = procrustes(&it.a, &it.b_o, it.ca, it.cb);
    }

    #[test]
    fn unequal_subpaths_cell_division() {
        let one = IconInput::D("M4 12 L20 12".into());
        let two = IconInput::D("M4 8 L20 8 M4 16 L20 16".into());
        let mid = morph_d_at(&one, &two, 0.5).unwrap();
        // Two subpaths in flight (duplicated source).
        assert!(
            mid.matches('M').count() >= 2,
            "cell division keeps both strokes: {mid}"
        );
    }

    #[test]
    fn markup_lucide_inner() {
        let inner = r#"<path d="M4 6h16"/><path d="M4 12h16"/><path d="M4 18h16"/>"#;
        let input = markup_to_icon(&inner, true).unwrap();
        let cubics = icon_to_cubics(&input).unwrap();
        assert_eq!(cubics.len(), 3);
    }

    #[test]
    fn fill_drawn_rejected_when_strict() {
        let svg = r##"<svg viewBox="0 0 24 24"><path d="M0 0h24v24H0z" fill="#000"/></svg>"##;
        assert!(markup_to_icon(svg, false).is_err());
    }

    #[test]
    fn menu_name_is_not_path_d() {
        assert!(looks_like_path_d("M4 6h16"));
        assert!(looks_like_path_d("m14 12 4 4"));
        assert!(!looks_like_path_d("menu"));
        assert!(!looks_like_path_d("moon"));
        assert!(!looks_like_path_d("x"));
    }

    #[cfg(feature = "bundled-lucide")]
    #[test]
    fn unlock_aliases_lock_open_paths() {
        let a = icon_input_from_src("unlock").expect("unlock alias");
        let b = icon_input_from_src("lock-open").expect("lock-open");
        assert_eq!(canonical_d(&a).unwrap(), canonical_d(&b).unwrap());
    }

    #[cfg(feature = "bundled-lucide")]
    #[test]
    fn lucide_menu_to_x_mid() {
        let mid = morph_src_at("menu", "x", 0.5).expect("lucide paths");
        assert!(mid.contains('M') && mid.contains('L'), "{mid}");
        let rest = morph_src_at("menu", "x", 1.0).unwrap();
        let x = canonical_d(&icon_input_from_src("x").unwrap()).unwrap();
        assert_eq!(rest, x);
    }

    #[test]
    fn svg_to_icon_fits_viewbox() {
        let svg = r#"<svg viewBox="0 0 24 24"><path d="M4 12h16" fill="none" stroke="black" stroke-width="2"/></svg>"#;
        let input = svg_to_icon(svg).expect("stroke svg");
        let d = canonical_d(&input).expect("d");
        assert!(d.starts_with('M'), "{d}");
    }

    #[test]
    fn linear_mid_differs_from_polar_on_rotation() {
        let a = line(0.0, 12.0, 24.0, 12.0);
        let b = line(12.0, 0.0, 12.0, 24.0);
        let polar = morph_d_at_kind(&a, &b, 0.5, false).unwrap();
        let linear = morph_d_at_kind(&a, &b, 0.5, true).unwrap();
        assert_ne!(polar, linear);
        assert!(polar.contains('L') && linear.contains('L'));
    }

    #[test]
    fn public_canonical_linear_and_cubics_d() {
        let d = cubics_to_path_d(&icon_to_cubics(&line(0.0, 0.0, 10.0, 0.0)).unwrap());
        assert!(d.starts_with('M') && d.contains('C'), "{d}");
        let a = resample_icon(&line(0.0, 12.0, 24.0, 12.0), SAMPLE_N).unwrap();
        let b = resample_icon(&line(12.0, 0.0, 12.0, 24.0), SAMPLE_N).unwrap();
        let plan = build_plan(&a, &b).unwrap();
        let mut out = alloc_outputs(&plan);
        interp_linear(&plan, 0.5, &mut out);
        let closed: Vec<bool> = plan.items.iter().map(|it| it.closed).collect();
        let mid = serialize(&out, &closed);
        assert!(mid.contains('L'), "{mid}");
        assert_eq!(canonical_d(&line(0.0, 0.0, 10.0, 0.0)).unwrap(), d);
    }
}
