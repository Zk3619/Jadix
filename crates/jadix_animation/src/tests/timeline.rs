use super::*;

#[test]
#[cfg(feature = "animation-base")]
fn once_timeline_reaches_one() {
    let tl = Timeline::new(0.5);
    assert!(tick_timelines(0.6));
    assert!((tl.linear() - 1.0).abs() < 1e-4);
    assert!(!tl.0.borrow().playing);
}

#[test]
fn keyframe_lerps() {
    let v = keyframe(&[0.0, 1.0], &[0.0, 10.0], 0.5);
    assert!((v - 5.0).abs() < 1e-4);
}
