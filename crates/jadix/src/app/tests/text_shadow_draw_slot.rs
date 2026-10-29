use super::{text_shadow_draw_slot, TextShadowDrawSlot};

#[test]
fn page_stack_bands_require_per_band_shadow_slot() {
    // Regression: drawing all text-shadows before page_stack band fills makes them
    // invisible under opaque card backgrounds (gallery Text-shadow section).
    assert_eq!(
        text_shadow_draw_slot(true),
        TextShadowDrawSlot::PerBandAfterRects
    );
    assert_eq!(
        text_shadow_draw_slot(false),
        TextShadowDrawSlot::AfterPageFills
    );
}
