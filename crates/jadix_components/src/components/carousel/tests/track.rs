use super::*;

#[test]
fn ideal_single_page_snaps_to_index() {
    let off = ideal_offset(IdealOffsetInput {
        index: 2,
        max_index: 4,
        step: 100.0,
        clone_count: 0,
        page_delta: 100.0,
        is_stack: false,
        multi_page_linear: false,
        track_loops: false,
        wraps_nav: false,
        min_offset: -400.0,
    });
    assert!((off - (-200.0)).abs() < 0.01);
}

#[test]
fn stack_step_scales_with_viewport() {
    let s = stack_step_px(400.0);
    assert!(s > 0.0 && s <= 200.0);
}
