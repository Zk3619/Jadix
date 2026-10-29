use super::*;

#[test]
fn toast_swipe_clamps_and_threshold() {
    let tr = ToastSwipeDirs::for_edge(true, false, false, true);
    assert_eq!(clamp_swipe_offset(40.0, -30.0, tr), [40.0, -30.0]);
    assert_eq!(clamp_swipe_offset(-40.0, 30.0, tr), [0.0, 0.0]);
    assert!(!should_dismiss([40.0, -30.0]));
    assert!(should_dismiss([SWIPE_DISMISS_PX, 0.0]));
}
