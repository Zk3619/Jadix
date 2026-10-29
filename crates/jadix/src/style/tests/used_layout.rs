use super::*;

#[test]
fn clip_does_not_erase_intrinsic_height() {
    let mut u = UsedLayout::default();
    u.stamp_border_box([0.0, 0.0, 100.0, 80.0], false);
    assert!((u.anim_height() - 80.0).abs() < 0.1);
    u.stamp_border_box([0.0, 0.0, 100.0, 0.0], true);
    assert!(
        u.anim_height() >= 80.0,
        "clipped 0-height frame must keep intrinsic, got {}",
        u.anim_height()
    );
}
