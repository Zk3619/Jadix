//! BiDi selection highlight must follow visual runs, not LTR logical prefixes.

use rui::renderer::font_loader;
use rui::text::{
    install_shared_font_system, measure_prefix_advance, visual_selection_highlights,
    LineBreakSource,
};

#[test]
fn visual_selection_ltr_island_in_rtl_paragraph() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let text = "עברית Hello";
    let lo = text.find("Hello").expect("Hello");
    let hi = lo + "Hello".len();
    let slabs = visual_selection_highlights(
        text,
        14.0,
        400,
        21.0,
        280.0,
        LineBreakSource::SoftWord,
        lo,
        hi,
    );
    assert!(!slabs.is_empty(), "expected a highlight over Hello");
    let hebrew_w = measure_prefix_advance(&text[..lo], 14.0, 400, 21.0);
    let x = slabs[0].1;
    assert!(
        x > hebrew_w + 1.0,
        "Hello glyphs are past the logical Hebrew prefix (LTR box would cover Hebrew); x={x} hebrew_w={hebrew_w}"
    );
}

#[test]
fn gallery_wj_hebrew_cluster_is_not_ltr_prefix() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let text = "Hello עברית\u{2060}789 and العربية — soft wrap keeps WJ pairs.";
    let lo = text.find("עברית").expect("hebrew");
    let hi = text.find(" and").expect(" and");
    let slabs = visual_selection_highlights(
        text,
        14.0,
        400,
        21.0,
        256.0,
        LineBreakSource::SoftWord,
        lo,
        hi,
    );
    assert!(
        !slabs.is_empty(),
        "WJ/Hebrew cluster must have a visual slab"
    );
    let total_w: f32 = slabs.iter().map(|s| s.2).sum();
    let line_w = slabs[0].3.max(1.0);
    assert!(
        total_w < line_w * 0.85,
        "cluster highlight must stay glyph-tight, not a full-line LTR box; total_w={total_w} line_w={line_w} slabs={slabs:?}"
    );
}
