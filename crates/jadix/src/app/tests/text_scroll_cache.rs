use super::AppState;
use crate::core::tree::TextLayoutInfo;
use crate::renderer::GlyphRasterMode;
use crate::style::TextStyle;

fn node_at_y(y: f32) -> TextLayoutInfo {
    TextLayoutInfo {
        node_id: None,
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

#[test]
fn rich_text_paint_edits_invalidate_retained_page_and_backdrop_sources() {
    use crate::core::tree::{Element, LayoutTree};
    use crate::style::TextSpan;

    let collect = |span: TextSpan| {
        let mut root = Element::new()
            .w(240.0)
            .h(40.0)
            .child(Element::new().text_spans(vec![span]).text_size(16.0));
        LayoutTree::new()
            .build_and_layout(&mut root, 240.0, 40.0, [-1.0, -1.0])
            .text_nodes
    };
    let original = TextSpan::new("same glyphs");
    let before = collect(original.clone());
    assert_eq!(before.len(), 1);
    assert!(AppState::text_nodes_equal(
        &before,
        &collect(original.clone())
    ));
    for (name, edit) in [
        ("foreground", 0),
        ("underline", 1),
        ("strikethrough", 2),
        ("background", 3),
    ] {
        let mut span = original.clone();
        match edit {
            0 => span.color = Some([240, 30, 20, 255]),
            1 => span.underline = true,
            2 => span.strikethrough = true,
            _ => span.background = Some([20, 120, 240, 255]),
        }
        let after = collect(span);
        assert_eq!(before[0].text, after[0].text);
        assert_eq!(before[0].style, after[0].style);
        assert_eq!(
            [before[0].x, before[0].y, before[0].width, before[0].height],
            [after[0].x, after[0].y, after[0].width, after[0].height],
            "{name} must exercise a paint-only difference"
        );
        assert!(
            !AppState::text_nodes_equal(&before, &after),
            "{name}: retained page/backdrop must not reuse the old text ink"
        );
    }
}

#[test]
fn text_effect_and_raster_changes_invalidate_retained_sources() {
    let before = [node_at_y(100.0)];
    let mut after = before.clone();
    after[0].glyph_fx = Some("shimmer".into());
    assert!(!AppState::text_nodes_equal(&before, &after));
    let mut before = after.clone();
    before[0].shimmer = Some(crate::renderer::ShimmerParams::default());
    after[0].shimmer = Some(crate::renderer::ShimmerParams {
        color: Some([255, 0, 0, 255]),
        ..Default::default()
    });
    assert!(!AppState::text_nodes_equal(&before, &after));
    let mut after = before.clone();
    after[0].glyph_raster = GlyphRasterMode::Msdf;
    assert!(!AppState::text_nodes_equal(&before, &after));
    assert!(AppState::text_nodes_equal(&before, &before.clone()));
}
