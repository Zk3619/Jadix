use super::AppState;
use crate::core::tree::TextLayoutInfo;
use crate::renderer::GlyphRasterMode;
use crate::style::TextStyle;

fn node_at_y(y: f32) -> TextLayoutInfo {
    TextLayoutInfo {
        text: "hello".into(),
        shape_suffix: None,
        spans: None,
        template_id: None,
        x: 8.0,
        y,
        width: 40.0,
        height: 16.0,
        clip_bounds: Some([0.0, 0.0, 400.0, 300.0]),
        style: TextStyle::default(),
        prepared: None,
        glyph_fx: None,
        shimmer: None,
        paint_scale: 1.0,
        paint_transform: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        paint_origin: [8.0, y],
        glyph_raster: GlyphRasterMode::Coverage,
    }
}

#[test]
fn idle_cache_tolerates_tiny_pos_noise() {
    let a = [node_at_y(100.0)];
    let b = [node_at_y(100.01)];
    assert!(AppState::text_nodes_equal(&a, &b));
}

#[test]
fn position_drift_quarter_pixel_busts_cache() {
    let a = [node_at_y(100.0)];
    let b = [node_at_y(100.25)];
    assert!(
        !AppState::text_nodes_equal(&a, &b),
        "0.25px scroll must not reuse glyphs parked behind rects (avatar label offset)"
    );
}
