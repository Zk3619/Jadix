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
    assert!(out.unwrap().starts_with("jadix_fx:"));
    assert_eq!(take_capture_jobs().len(), 1);
    let fx = take_effect_jobs();
    assert_eq!(fx.len(), 1);
    assert_eq!(fx[0].kind, EffectKind::Blur);
    assert!((fx[0].strength - 0.8).abs() < 1e-4);
}

#[test]
#[cfg(feature = "compute")]
fn blur_effect_enriches_compute_pass() {
    let _ = take_effect_jobs();
    let _ = take_capture_jobs();
    let mut attrs = std::collections::HashMap::new();
    attrs.insert(ATTR_EFFECT.into(), "blur".into());
    queue_from_attrs(&attrs, [0.0, 0.0, 32.0, 16.0], None);
    let mut graph = crate::renderer::RenderGraph::default();
    enrich_render_graph(&mut graph);
    assert!(
        graph
            .nodes
            .iter()
            .any(|n| n.kind == crate::renderer::PassKind::Compute),
        "blur effect must dump PassKind::Compute"
    );
    let _ = take_effect_jobs();
    let _ = take_capture_jobs();
}

#[test]
#[cfg(not(feature = "compute"))]
fn blur_effect_skips_compute_dump_without_feature() {
    let _ = take_effect_jobs();
    let _ = take_capture_jobs();
    let mut attrs = std::collections::HashMap::new();
    attrs.insert(ATTR_EFFECT.into(), "blur".into());
    queue_from_attrs(&attrs, [0.0, 0.0, 32.0, 16.0], None);
    let mut graph = crate::renderer::RenderGraph::default();
    enrich_render_graph(&mut graph);
    assert!(
        graph
            .nodes
            .iter()
            .all(|n| n.kind != crate::renderer::PassKind::Compute),
        "default build must not dump Compute"
    );
    let _ = take_effect_jobs();
    let _ = take_capture_jobs();
}

#[test]
#[cfg(feature = "compute")]
fn particle_effect_enriches_compute_pass() {
    let _ = take_effect_jobs();
    let _ = take_capture_jobs();
    let mut attrs = std::collections::HashMap::new();
    attrs.insert(ATTR_EFFECT.into(), "particle".into());
    queue_from_attrs(&attrs, [0.0, 0.0, 32.0, 16.0], None);
    let mut graph = crate::renderer::RenderGraph::default();
    enrich_render_graph(&mut graph);
    let compute_n = graph
        .nodes
        .iter()
        .filter(|n| n.kind == crate::renderer::PassKind::Compute)
        .count();
    assert!(compute_n >= 2, "particle must dump hist + particle Compute");
    let _ = take_effect_jobs();
    let _ = take_capture_jobs();
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
    assert!(a.starts_with("jadix_zoom_cap:"));
    assert_eq!(a, b, "scroll/origin must not bust the zoom capture cache");
}

fn sample_zoom_text(color: Option<[u8; 4]>) -> crate::layout::layout_tree::TextLayoutInfo {
    crate::layout::layout_tree::TextLayoutInfo {
        text: "You can change this later in Settings.".into(),
        shape_suffix: None,
        spans: None,
        template_id: None,
        x: 0.0,
        y: 0.0,
        width: 240.0,
        height: 16.0,
        clip_bounds: None,
        style: crate::style::TextStyle {
            color,
            ..Default::default()
        },
        prepared: None,
        glyph_fx: None,
        shimmer: None,
        paint_scale: 1.0,
        paint_transform: crate::style::MAT2D_IDENTITY,
        paint_origin: [0.0, 0.0],
        glyph_raster: crate::renderer::GlyphRasterMode::Coverage,
    }
}

#[test]
fn queue_zoom_layer_wrap_includes_text_color() {
    let _ = take_capture_jobs();
    let bounds = [0.0, 0.0, 64.0, 32.0];
    let a = queue_zoom_layer_wrap(bounds, Vec::new(), Vec::new(), vec![sample_zoom_text(None)]);
    let _ = take_capture_jobs();
    let b = queue_zoom_layer_wrap(
        bounds,
        Vec::new(),
        Vec::new(),
        vec![sample_zoom_text(Some([34, 197, 94, 255]))],
    );
    let _ = take_capture_jobs();
    assert_ne!(
        a, b,
        "variant description color must bust the zoom capture cache"
    );
}
