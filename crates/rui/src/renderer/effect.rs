//! Pluggable shader effect pass + subtree-capture job queue (B16).
//!
//! Effects consume a texture (`rui_capture:` / any pooled key) and write `rui_fx:`.
//! Hit-testing stays on the Element tree — this module never swallows pointer targets.

use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

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

/// GPU uniform block. Must match `effect_pass.wgsl` (WGSL uniform: `vec2` align 8,
/// struct size round up to 16). Rust `[f32; 2]` is align-4, so `_pad0` sits before `texel`.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct EffectUniforms {
    pub mode: f32,
    pub time: f32,
    pub pointer: [f32; 2],
    pub strength: f32,
    pub _pad0: f32,
    pub texel: [f32; 2],
    pub _pad1: [f32; 4],
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

#[derive(Clone)]
pub struct CaptureJob {
    pub key: String,
    pub bounds: [f32; 4],
    /// Child subtree geometry (logical px), replayed into the capture RT.
    pub instances: Vec<crate::renderer::InstanceInput>,
    pub textured_draws: Vec<crate::renderer::TexturedDraw>,
    /// Text laid out in page space; remapped into the RT (identity transform inside RT).
    pub text_nodes: Vec<crate::layout::layout_tree::TextLayoutInfo>,
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
        text_nodes: Vec::new(),
    });
    let in_key = input_key.unwrap_or(cap_key.as_str()).to_string();
    let out_key = format!(
        "rui_fx:{}:{:.0}x{:.0}",
        kind.as_mode(),
        bounds[2],
        bounds[3]
    );
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
    queue_subtree_wrap_with_text(attrs, bounds, instances, textured_draws, Vec::new())
}

/// Like [`queue_subtree_wrap`], but also captures text for effect / filter SourceGraphic.
pub fn queue_subtree_wrap_with_text(
    attrs: &std::collections::HashMap<String, String>,
    bounds: [f32; 4],
    instances: Vec<crate::renderer::InstanceInput>,
    textured_draws: Vec<crate::renderer::TexturedDraw>,
    text_nodes: Vec<crate::layout::layout_tree::TextLayoutInfo>,
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
        text_nodes,
    });
    let out_key = format!(
        "rui_fx:{}:{:.0}x{:.0}",
        kind.as_mode(),
        bounds[2],
        bounds[3]
    );
    queue_effect(EffectJob {
        kind,
        input_key: cap_key,
        output_key: out_key.clone(),
        strength,
        bounds,
    });
    Some(out_key)
}

/// Capture a Dialog/Alert `__modal_zoom` subtree at rest, then blit the texture at CSS `scale`.
///
/// Independent `scale` otherwise flows to glyphon `TextArea.scale` (atlas churn) or sticky Bake
/// (Coverage+Bake ghosting). One compositor layer matches Sheet: geometry stays at 1.0, the GPU
/// scales a single quad. Cache id ignores dest scale so enter frames reuse the RT.
pub fn queue_zoom_layer_wrap(
    bounds: [f32; 4],
    instances: Vec<crate::renderer::InstanceInput>,
    textured_draws: Vec<crate::renderer::TexturedDraw>,
    text_nodes: Vec<crate::layout::layout_tree::TextLayoutInfo>,
) -> String {
    let id = zoom_layer_cache_id(bounds, &instances, &textured_draws, &text_nodes);
    let cap_key = format!("rui_zoom_cap:{id:016x}");
    // Keep descendant overflow clips (remapped in `render_capture`). Stripping them
    // let inner shells paint the layout AABB and cover rounded corners / the shadow pad.
    queue_capture(CaptureJob {
        key: cap_key.clone(),
        bounds,
        instances,
        textured_draws,
        text_nodes,
    });
    cap_key
}

fn zoom_layer_cache_id(
    bounds: [f32; 4],
    instances: &[crate::renderer::InstanceInput],
    textured_draws: &[crate::renderer::TexturedDraw],
    text_nodes: &[crate::layout::layout_tree::TextLayoutInfo],
) -> u64 {
    let mut hasher = DefaultHasher::new();
    (bounds[2].round() as i32).hash(&mut hasher);
    (bounds[3].round() as i32).hash(&mut hasher);
    instances.len().hash(&mut hasher);
    for i in instances {
        ((i.rect_pos[0] - bounds[0]).to_bits()).hash(&mut hasher);
        ((i.rect_pos[1] - bounds[1]).to_bits()).hash(&mut hasher);
        i.rect_size[0].to_bits().hash(&mut hasher);
        i.rect_size[1].to_bits().hash(&mut hasher);
        for c in i.color {
            c.to_bits().hash(&mut hasher);
        }
    }
    textured_draws.len().hash(&mut hasher);
    for d in textured_draws {
        d.texture_key.hash(&mut hasher);
        ((d.rect[0] - bounds[0]).to_bits()).hash(&mut hasher);
        ((d.rect[1] - bounds[1]).to_bits()).hash(&mut hasher);
        d.rect[2].to_bits().hash(&mut hasher);
        d.rect[3].to_bits().hash(&mut hasher);
    }
    text_nodes.len().hash(&mut hasher);
    for t in text_nodes {
        t.text.hash(&mut hasher);
        t.width.to_bits().hash(&mut hasher);
        t.height.to_bits().hash(&mut hasher);
    }
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniforms_match_wgsl_uniform_layout() {
        assert_eq!(std::mem::size_of::<EffectUniforms>(), 48);
        assert_eq!(std::mem::offset_of!(EffectUniforms, texel), 24);
    }

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

    #[test]
    fn queue_zoom_layer_wrap_ignores_origin() {
        let _ = take_capture_jobs();
        let a = queue_zoom_layer_wrap([10.0, 20.0, 64.0, 32.0], Vec::new(), Vec::new(), Vec::new());
        let _ = take_capture_jobs();
        let b = queue_zoom_layer_wrap(
            [100.0, 200.0, 64.0, 32.0],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        let _ = take_capture_jobs();
        assert!(a.starts_with("rui_zoom_cap:"));
        assert_eq!(a, b, "scroll/origin must not bust the zoom capture cache");
    }
}
