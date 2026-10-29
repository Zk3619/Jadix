use super::*;

#[test]
fn stack_collapsed_peek_and_scale() {
    let cfg = SlotListConfig::toast_stack(72.0, 14.0, 8.0, true);
    let p0 = slot_list_pose(0, false, &cfg);
    assert!((p0.offset - 0.0).abs() < f32::EPSILON);
    assert!((p0.scale - 1.0).abs() < f32::EPSILON);
    assert!(!p0.dim);

    let p1 = slot_list_pose(1, false, &cfg);
    assert!((p1.offset - 14.0).abs() < f32::EPSILON);
    assert!((p1.scale - 0.95).abs() < f32::EPSILON);
    assert!(p1.dim);
}

#[test]
fn stack_expanded_uses_item_plus_gap() {
    let cfg = SlotListConfig::toast_stack(72.0, 14.0, 8.0, true);
    let p2 = slot_list_pose(2, true, &cfg);
    assert!((p2.offset - 2.0 * (72.0 + 8.0)).abs() < f32::EPSILON);
    assert!((p2.scale - 1.0).abs() < f32::EPSILON);
    assert!(!p2.dim);
}

#[test]
fn bottom_axis_is_negative() {
    let cfg = SlotListConfig::toast_stack(72.0, 14.0, 8.0, false);
    let p = slot_list_pose(2, false, &cfg);
    assert!((p.offset - (-28.0)).abs() < f32::EPSILON);
}

#[test]
fn visual_offset_keeps_leading_edge() {
    let raw = 14.0;
    let scale = 0.95;
    let item = 72.0;
    let v = slot_visual_offset(raw, scale, item, true);
    let expected = raw + item * (1.0 - scale) * 0.5;
    assert!((v - expected).abs() < 0.01);
    let v_bot = slot_visual_offset(raw, scale, item, false);
    assert!((v_bot - (raw - item * (1.0 - scale) * 0.5)).abs() < 0.01);
}

#[test]
fn linear_list_offsets() {
    assert!((linear_slot_offset(2, 80.0, true) - 160.0).abs() < f32::EPSILON);
    assert!((linear_slot_offset(2, 80.0, false) - (-160.0)).abs() < f32::EPSILON);
    let g = LayoutGroup::new(SlotListConfig::linear(72.0, 8.0, true));
    let p = g.pose(1, true);
    assert!((p.offset - 80.0).abs() < f32::EPSILON);
    assert!((p.scale - 1.0).abs() < f32::EPSILON);
}
