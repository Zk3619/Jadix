//! Stroke-icon morph core (morphicons): parse → cubics → resample → plan → polar interp → `d`.
//!
//! Compiled only with the `animation-path` feature. Stroke-centerline icons only
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

pub fn interp_plan_d_kind(plan: &MorphPlan, t: f64, linear: bool) -> String {
    serialize_plan(plan, t, linear)
}

pub fn sampled_from_outputs(plan: &MorphPlan, t: f64) -> Vec<Sampled> {
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
#[path = "tests/mod.rs"]
mod tests;
