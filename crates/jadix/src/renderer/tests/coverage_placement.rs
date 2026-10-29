use super::{coverage_line_height, coverage_nowrap_glyph_dy};
use crate::style::{tailwind_default_leading_scale, TextStyle, WhiteSpace};

/// The deleted capture `Buffer::set_text` recipe (Tailwind leading, no lucide 1em).
fn naive_capture_line_height(st: &TextStyle, ui_scale: f32) -> f32 {
    let size = (st.size_resolved() * ui_scale).max(1.0);
    if let Some(px) = st.line_height_px {
        px * ui_scale
    } else if let Some(sc) = st.line_height_scale {
        size * sc
    } else {
        size * tailwind_default_leading_scale(st.size_resolved())
    }
}

#[test]
fn lucide_line_height_is_em_not_tailwind_leading() {
    let st = TextStyle {
        font_family: Some("lucide".into()),
        size: Some(16.0),
        ..Default::default()
    };
    let coverage = coverage_line_height(&st, 1.0, false);
    assert!(
        (coverage - 16.0).abs() < 1e-4,
        "lucide Coverage line box must be 1em; got {coverage}"
    );
    let naive = naive_capture_line_height(&st, 1.0);
    assert!(
        (naive - coverage).abs() > 1.0,
        "Select chevron sat low because capture used Tailwind leading ({naive}) instead of 1em"
    );
}

#[test]
fn nowrap_single_line_centers_in_tall_leaf() {
    // SelectTrigger is h-8 (32px) with items-center; a nowrap caption leaf can
    // still be taller than its line box if padding wins. Coverage centers it.
    let lh = 14.0 * 1.25;
    let dy = coverage_nowrap_glyph_dy(false, true, 1, 32.0, lh);
    assert!((dy - (32.0 - lh) * 0.5).abs() < 1e-4, "got dy={dy}");
    let naive_top = 0.0;
    assert!(
        (dy - naive_top).abs() > 4.0,
        "capture-without-dy would sit at the leaf top"
    );
}

#[test]
fn wrapping_cjk_label_does_not_invent_dy() {
    assert_eq!(coverage_nowrap_glyph_dy(false, false, 1, 32.0, 14.0), 0.0);
}

#[test]
fn label_leading_none_agrees_with_naive_capture_lh() {
    // Gallery Dialog rows use Label defaults (leading-none). Line-height matched;
    // remaining jitter was wrap/weight/shaping — which only unify via prepare_text_frame.
    let st = TextStyle {
        size: Some(14.0),
        line_height_scale: Some(1.0),
        white_space: WhiteSpace::Normal,
        ..Default::default()
    };
    let coverage = coverage_line_height(&st, 1.0, false);
    let naive = naive_capture_line_height(&st, 1.0);
    assert!((coverage - naive).abs() < 1e-4);
    assert!((coverage - 14.0).abs() < 1e-4);
}
