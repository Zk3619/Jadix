use super::glyph_node_visible_this_frame;

#[test]
fn scrolled_caption_split_keeps_both_scripts_visible() {
    // Visual origin already subtracted scroll_x. The left (CJK) wipe ends
    // before the scroll-restored leaf, so the leaf test drops it and the
    // non-ASCII warmup draws it a frame after the Latin wipe.
    let viewport = [0.0, 0.0, 800.0, 600.0];
    let origin = [100.0, 10.0];
    let size = [124.0, 21.0];
    let scroll_x = 80.0;
    let cjk = Some([100.0, 10.0, 36.0, 21.0]);
    let latin = Some([136.0, 10.0, 88.0, 21.0]);
    assert!(
        glyph_node_visible_this_frame(origin, size, scroll_x, cjk, viewport),
        "CJK wipe is on screen and must prepare with the Latin wipe"
    );
    assert!(glyph_node_visible_this_frame(
        origin, size, scroll_x, latin, viewport
    ));
}

#[test]
fn offscreen_caption_wipe_stays_deferred() {
    let viewport = [0.0, 0.0, 800.0, 600.0];
    assert!(!glyph_node_visible_this_frame(
        [100.0, 10.0],
        [124.0, 21.0],
        0.0,
        Some([900.0, 10.0, 20.0, 21.0]),
        viewport,
    ));
}
