//! Pluggable shader effect pass + subtree-capture job queue (B16).
//!
//! Effects consume a texture (`rui_capture:` / any pooled key) and write `rui_fx:`.
//! Hit-testing stays on the Element tree — this module never swallows pointer targets.

use std::cell::RefCell;

use bytemuck::{Pod, Zeroable};

/// Public attr: wrap children with a named effect (`blur` / `displacement` / `vhs` / `glitch` / `glass`).
pub const ATTR_EFFECT: &str = "rui_fx";
/// Optional strength 0..=1.
pub const ATTR_EFFECT_STRENGTH: &str = "rui_fx_strength";
/// When `1`, children are captured to offscreen instead of painting to the main pass.
pub const ATTR_EFFECT_CAPTURE_CHILDREN: &str = "rui_fx_capture_children";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectKind {
    Passthrough,
    Displacement,
    Blur,
    Glass,
    Ripple,
    Chromatic,
    Vhs,
    Glitch,
    Particle,
}

impl EffectKind {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "displacement" | "liquid" => Self::Displacement,
            "blur" | "frost" => Self::Blur,
            "glass" => Self::Glass,
            "ripple" => Self::Ripple,
            "chromatic" => Self::Chromatic,
            "vhs" => Self::Vhs,
            "glitch" => Self::Glitch,
            "particle" => Self::Particle,
            _ => Self::Passthrough,
        }
    }

    pub fn as_mode(self) -> u32 {
        match self {
            Self::Passthrough => 0,
            Self::Displacement => 1,
            Self::Blur => 2,
            Self::Glass => 3,
            Self::Ripple => 4,
            Self::Chromatic => 5,
            Self::Vhs => 6,
            Self::Glitch => 7,
            Self::Particle => 8,
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct EffectUniforms {
    pub mode: f32,
    pub time: f32,
    pub pointer: [f32; 2],
    pub strength: f32,
    pub texel: [f32; 2],
    pub _pad: f32,
}

#[derive(Clone, Debug)]
pub struct EffectJob {
    pub kind: EffectKind,
    pub input_key: String,
    pub output_key: String,
    pub strength: f32,
    pub bounds: [f32; 4],
}

thread_local! {
    static JOBS: RefCell<Vec<EffectJob>> = const { RefCell::new(Vec::new()) };
    static CAPTURES: RefCell<Vec<CaptureJob>> = const { RefCell::new(Vec::new()) };
}

#[derive(Clone, Debug)]
pub struct CaptureJob {
    pub key: String,
    pub bounds: [f32; 4],
    /// Child subtree geometry (logical px), replayed into the capture RT.
    pub instances: Vec<crate::renderer::InstanceInput>,
    pub textured_draws: Vec<crate::renderer::TexturedDraw>,
}

pub fn queue_effect(job: EffectJob) {
    JOBS.with(|j| j.borrow_mut().push(job));
}

pub fn take_effect_jobs() -> Vec<EffectJob> {
    JOBS.with(|j| std::mem::take(&mut *j.borrow_mut()))
}

pub fn queue_capture(job: CaptureJob) {
    CAPTURES.with(|c| c.borrow_mut().push(job));
}

pub fn take_capture_jobs() -> Vec<CaptureJob> {
    CAPTURES.with(|c| std::mem::take(&mut *c.borrow_mut()))
}

/// Layout helper: if the leaf has `rui_fx`, queue capture + effect around its bounds.
pub fn queue_from_attrs(
    attrs: &std::collections::HashMap<String, String>,
    bounds: [f32; 4],
    input_key: Option<&str>,
) -> Option<String> {
    let raw = attrs.get(ATTR_EFFECT)?;
    let kind = EffectKind::parse(raw);
    if matches!(kind, EffectKind::Passthrough) && raw.trim().is_empty() {
        return None;
    }
    let strength = attrs
        .get(ATTR_EFFECT_STRENGTH)
        .and_then(|s| s.parse::<f32>().ok())
        .unwrap_or(0.45)
        .clamp(0.0, 1.0);
    let cap_key = format!(
        "rui_capture:{:.0}x{:.0}:{:.0}x{:.0}",
        bounds[0], bounds[1], bounds[2], bounds[3]
    );
    queue_capture(CaptureJob {
        key: cap_key.clone(),
        bounds,
        instances: Vec::new(),
        textured_draws: Vec::new(),
    });
    let in_key = input_key.unwrap_or(cap_key.as_str()).to_string();
    let out_key = format!("rui_fx:{}:{:.0}x{:.0}", kind.as_mode(), bounds[2], bounds[3]);
    queue_effect(EffectJob {
        kind,
        input_key: in_key,
        output_key: out_key.clone(),
        strength,
        bounds,
    });
    Some(out_key)
}

/// Append effect / capture passes to the per-frame graph (does not consume the job queues).
pub fn enrich_render_graph(graph: &mut crate::renderer::RenderGraph) {
    CAPTURES.with(|c| {
        for cap in c.borrow().iter() {
            graph.push(
                crate::renderer::RenderPassNode::new(
                    crate::renderer::PassKind::SubtreeCapture,
                    format!("capture-{}", cap.key),
                )
                .write(cap.key.clone()),
            );
        }
    });
    JOBS.with(|j| {
        for fx in j.borrow().iter() {
            graph.push(
                crate::renderer::RenderPassNode::new(
                    crate::renderer::PassKind::Effect,
                    format!("fx-{:?}", fx.kind),
                )
                .read(fx.input_key.clone())
                .write(fx.output_key.clone()),
            );
        }
    });
}

/// Queue subtree capture + effect for a wrap container (children already collected).
pub fn queue_subtree_wrap(
    attrs: &std::collections::HashMap<String, String>,
    bounds: [f32; 4],
    instances: Vec<crate::renderer::InstanceInput>,
    textured_draws: Vec<crate::renderer::TexturedDraw>,
) -> Option<String> {
    let raw = attrs.get(ATTR_EFFECT)?;
    let kind = EffectKind::parse(raw);
    if matches!(kind, EffectKind::Passthrough) && raw.trim().is_empty() {
        return None;
    }
    let strength = attrs
        .get(ATTR_EFFECT_STRENGTH)
        .and_then(|s| s.parse::<f32>().ok())
        .unwrap_or(0.45)
        .clamp(0.0, 1.0);
    let cap_key = format!(
        "rui_capture:{:.0}x{:.0}:{:.0}x{:.0}",
        bounds[0], bounds[1], bounds[2], bounds[3]
    );
    queue_capture(CaptureJob {
        key: cap_key.clone(),
        bounds,
        instances,
        textured_draws,
    });
    let out_key = format!("rui_fx:{}:{:.0}x{:.0}", kind.as_mode(), bounds[2], bounds[3]);
    queue_effect(EffectJob {
        kind,
        input_key: cap_key,
        output_key: out_key.clone(),
        strength,
        bounds,
    });
    Some(out_key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_kinds() {
        assert_eq!(EffectKind::parse("liquid"), EffectKind::Displacement);
        assert_eq!(EffectKind::parse("VHS"), EffectKind::Vhs);
        assert_eq!(EffectKind::parse("glass"), EffectKind::Glass);
    }

    #[test]
    fn queue_from_attrs_emits_jobs() {
        let _ = take_effect_jobs();
        let _ = take_capture_jobs();
        let mut attrs = std::collections::HashMap::new();
        attrs.insert(ATTR_EFFECT.into(), "blur".into());
        attrs.insert(ATTR_EFFECT_STRENGTH.into(), "0.8".into());
        let out = queue_from_attrs(&attrs, [0.0, 0.0, 64.0, 32.0], None);
        assert!(out.unwrap().starts_with("rui_fx:"));
        assert_eq!(take_capture_jobs().len(), 1);
        let fx = take_effect_jobs();
        assert_eq!(fx.len(), 1);
        assert_eq!(fx[0].kind, EffectKind::Blur);
        assert!((fx[0].strength - 0.8).abs() < 1e-4);
    }
}
