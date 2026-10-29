use super::scrollbar_track_content_inset_px;

#[test]
fn track_content_inset_is_border_when_padding_is_zero() {
    assert!((scrollbar_track_content_inset_px(0.0, 1.0, true) - 1.0).abs() < f32::EPSILON);
    assert!((scrollbar_track_content_inset_px(1.0, 1.0, true) - 2.0).abs() < f32::EPSILON);
    assert!((scrollbar_track_content_inset_px(1.0, 1.0, false) - 1.0).abs() < f32::EPSILON);
    assert!((scrollbar_track_content_inset_px(0.0, 0.0, true) - 0.0).abs() < f32::EPSILON);
}
