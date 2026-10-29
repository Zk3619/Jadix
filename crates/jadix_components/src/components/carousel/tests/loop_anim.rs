use super::*;

#[test]
fn loop_reached_epsilon() {
    let a = LoopAnim {
        target_offset: -100.0,
        settle_index: 0,
        settle_offset: 0.0,
    };
    assert!(loop_reached(-100.2, a.target_offset, 0.5));
    assert!(!loop_reached(-110.0, a.target_offset, 0.5));
}
