use super::*;

#[test]
fn normalize_accepts_smooth_and_jump_only() {
    assert_eq!(normalize_scroll_behavior("smooth"), SCROLL_BEHAVIOR_SMOOTH);
    assert_eq!(normalize_scroll_behavior("SMOOTH"), SCROLL_BEHAVIOR_SMOOTH);
    assert_eq!(normalize_scroll_behavior("jump"), SCROLL_BEHAVIOR_JUMP);
    assert_eq!(normalize_scroll_behavior(""), DEFAULT_SCROLL_BEHAVIOR);
    assert_eq!(normalize_scroll_behavior("nope"), DEFAULT_SCROLL_BEHAVIOR);
    assert!(scroll_behavior_is_smooth("smooth"));
    assert!(scroll_behavior_is_jump("jump"));
    assert!(!scroll_behavior_is_jump("nope"));
}

#[test]
fn stale_zero_max_detects_cull_not_expand_to_fit() {
    let prev = ([0.0, 100.0, 320.0, 384.0], [0.0, 100.0], [320.0, 484.0]);
    assert!(is_stale_zero_max_sample(
        [0.0, 0.0],
        [0.0, 80.0, 320.0, 384.0],
        [320.0, 384.0],
        Some(&prev),
    ));
    assert!(!is_stale_zero_max_sample(
        [0.0, 0.0],
        [0.0, 48.0, 208.0, 600.0],
        [208.0, 500.0],
        Some(&([0.0, 48.0, 208.0, 300.0], [0.0, 200.0], [208.0, 500.0])),
    ));
}
