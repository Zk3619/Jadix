use super::clip_rect_to_bounds;

#[test]
fn caret_pan_cull_uses_unshifted_origin() {
    let scroll = 80.0_f32;
    let field = [10.0, 0.0, 40.0, 20.0];
    let shifted_x = field[0] - scroll;
    let shifted = clip_rect_to_bounds([shifted_x, 0.0, field[2], field[3]], Some(field));
    assert!(
        shifted.is_none(),
        "shifted viewport-sized rect must miss the field"
    );
    let cull = clip_rect_to_bounds([shifted_x + scroll, 0.0, field[2], field[3]], Some(field));
    assert!(cull.is_some(), "unshifted origin must overlap the field");
}

#[test]
fn ellipsis_buffer_uses_box_width_not_intrinsic() {
    use crate::style::TextOverflow;
    let intrinsic = 240.0_f32;
    let box_w = 80.0_f32;
    let w = super::nowrap_buffer_width(TextOverflow::Ellipsis, true, intrinsic, box_w);
    assert!((w - box_w).abs() < 0.01, "ellipsis buffer w={w}");
    let hidden = super::nowrap_buffer_width(TextOverflow::Hidden, true, intrinsic, box_w);
    assert!(
        (hidden - intrinsic).abs() < 0.01,
        "hidden still shapes the full line, got {hidden}"
    );
}

#[test]
fn overflowing_text_overflow_forces_start_align() {
    assert!(crate::style::TextOverflow::Ellipsis.start_align_when_line_overflows());
    assert!(crate::style::TextOverflow::Hidden.start_align_when_line_overflows());
    assert!(!crate::style::TextOverflow::Newline.start_align_when_line_overflows());
}
