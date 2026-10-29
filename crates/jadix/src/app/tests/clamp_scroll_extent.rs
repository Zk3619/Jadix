use super::clamp_scroll_offset_and_target;

#[test]
fn expand_to_fit_clears_stale_target_so_later_shrink_keeps_top() {
    // Scrolled down, then viewport grows until max=0: both must become 0.
    let (o, t) = clamp_scroll_offset_and_target(280.0, 280.0, 0.0);
    assert_eq!((o, t), (0.0, 0.0));
    // Shrink again: with synced target, stay at top (bottom overflows).
    let (o2, t2) = clamp_scroll_offset_and_target(o, t, 200.0);
    assert_eq!((o2, t2), (0.0, 0.0));
}

#[test]
fn snap_targets_cancel_leftover_smooth_chase() {
    use crate::core::reactivity::use_signal;
    use crate::core::scroll_viewport::{scroll_offset_y_signal_key, scroll_target_y_signal_key};
    let rest = "snap-leftover-forms";
    use_signal(&scroll_offset_y_signal_key(rest), || 0.0_f32).set_silent(80.0);
    use_signal(&scroll_target_y_signal_key(rest), || 0.0_f32).set_silent(140.0);
    super::snap_scroll_targets_to_current_offsets([rest]);
    let ty = use_signal(&scroll_target_y_signal_key(rest), || 0.0_f32).peek();
    let sy = use_signal(&scroll_offset_y_signal_key(rest), || 0.0_f32).peek();
    assert!(
        (ty - sy).abs() < 0.01,
        "click must drop leftover target (ty={ty} sy={sy}) so unfreeze does not dump scroll"
    );
}

#[test]
fn shrink_while_scrolled_clamps_to_new_max_not_zero() {
    // Standard browser-like: stay near bottom when max shrinks.
    let (o, t) = clamp_scroll_offset_and_target(350.0, 350.0, 200.0);
    assert_eq!((o, t), (200.0, 200.0));
}
