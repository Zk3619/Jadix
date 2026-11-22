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

#[test]
fn end_intent_retargets_without_losing_smooth_anchor_and_yields_to_navigation() {
    let (_owner, _) = crate::core::reactivity::create_root(|| {
        for horizontal in [false, true] {
            let id = if horizontal {
                "end-intent-x"
            } else {
                "end-intent-y"
            };
            let offset_key = if horizontal {
                scroll_offset_x_signal_key
            } else {
                scroll_offset_y_signal_key
            };
            let target_key = if horizontal {
                scroll_target_x_signal_key
            } else {
                scroll_target_y_signal_key
            };
            let request = if horizontal {
                request_scroll_end_x
            } else {
                request_scroll_end_y
            };
            let reconcile = if horizontal {
                reconcile_scroll_end_x
            } else {
                reconcile_scroll_end_y
            };
            let cancel = if horizontal {
                cancel_scroll_end_x
            } else {
                cancel_scroll_end_y
            };
            let offset = use_signal(&offset_key(id), || 100.0_f32);
            let target = use_signal(&target_key(id), || 500.0_f32);
            request(id, 500.0);
            assert_eq!(reconcile(id, 900.0, 180.0), Some(180.0));
            assert_eq!(target.peek(), 900.0);
            // Repetition does not reset the chase or create a second request.
            request(id, 900.0);
            assert_eq!(reconcile(id, 1100.0, 240.0), Some(240.0));
            offset.set_silent(1100.0);
            assert_eq!(reconcile(id, 1300.0, 1100.0), Some(1300.0));
            target.set_silent(300.0); // application/focus/scrollbar wins
            assert_eq!(reconcile(id, 1500.0, 1300.0), None);
            assert_eq!(target.peek(), 300.0);
            request(id, 300.0);
            cancel(id);
            assert_eq!(reconcile(id, 1800.0, 300.0), None);
            request(id, 300.0);
            remove_scroll_viewport_signals(id);
            assert_eq!(reconcile(id, 2000.0, 0.0), None);
        }
    });
}
