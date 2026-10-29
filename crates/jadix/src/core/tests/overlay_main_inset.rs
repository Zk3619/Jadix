use super::scrollbar_overlay_main_insets;

#[test]
fn vertical_overlay_spans_full_height() {
    let (top, bottom) = scrollbar_overlay_main_insets("y", false, true, false, 2.0, 10.0);
    assert_eq!((top, bottom), (2.0, 2.0));
}

#[test]
fn horizontal_yields_to_vertical_on_right() {
    let (left, right) = scrollbar_overlay_main_insets("x", false, true, false, 2.0, 10.0);
    assert_eq!((left, right), (2.0, 12.0));
}

#[test]
fn horizontal_yields_to_vertical_on_left() {
    let (left, right) = scrollbar_overlay_main_insets("x", false, true, true, 2.0, 10.0);
    assert_eq!((left, right), (12.0, 2.0));
}

#[test]
fn outside_track_does_not_self_inset() {
    let (a, b) = scrollbar_overlay_main_insets("x", true, true, false, 2.0, 10.0);
    assert_eq!((a, b), (0.0, 0.0));
}

#[test]
fn horizontal_lane_side_keeps_the_other_inset() {
    use super::{horizontal_lane_side_inset, vertical_scrollbar_occupies_lane};
    assert!(!vertical_scrollbar_occupies_lane(true, "auto", 80.0, 40.0));
    assert!(vertical_scrollbar_occupies_lane(true, "always", 80.0, 40.0));
    // Unmeasured auto must not reserve a gutter for a bar that may never show.
    assert!(!vertical_scrollbar_occupies_lane(true, "auto", 0.0, 0.0));
    assert!(vertical_scrollbar_occupies_lane(true, "auto", 80.0, 120.0));
    // Edge baseline + v_cross when the vertical bar owns the corner.
    assert!((horizontal_lane_side_inset(2.0, 10.0, false) - 2.0).abs() < 0.01);
    assert!((horizontal_lane_side_inset(2.0, 10.0, true) - 12.0).abs() < 0.01);
    // Textarea authors left(4) / right(2) with an 8px vertical bar.
    assert!((horizontal_lane_side_inset(2.0, 8.0, false) - 2.0).abs() < 0.01);
    assert!((horizontal_lane_side_inset(2.0, 8.0, true) - 10.0).abs() < 0.01);
}

#[test]
fn unlaid_main_axis_stays_inside_the_insets() {
    use super::overlay_main_length_when_unlaid;
    // Textarea viewport 240, left 4, right 2 once the vertical lane is gone.
    assert!((overlay_main_length_when_unlaid(240.0, 4.0, 2.0).unwrap() - 234.0).abs() < 0.01);
    // Unmeasured viewport must not invent a bar.
    assert!(overlay_main_length_when_unlaid(0.0, 4.0, 2.0).is_none());
}
