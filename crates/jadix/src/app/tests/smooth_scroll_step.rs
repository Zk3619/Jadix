use super::{snap_scroll_offset_to_device_px, wheel_scroll_target, SmoothScrollAxis};

#[test]
fn gentle_wheel_notch_accelerates_from_rest() {
    let mut motion = SmoothScrollAxis::new(0.0);
    let first = motion.step(0.0, 75.0, 0.016, 1.5).0;
    let second = motion.step(first, 75.0, 0.016, 1.5).0 - first;
    assert!(
        first > 0.0 && first <= 3.0,
        "gentle notch starts with a {first}px jump"
    );
    assert!(second > first, "start must ease in: {first} then {second}");
}

#[test]
fn smooth_chase_is_independent_of_frame_rate() {
    let chase = |dt, frames| {
        let mut motion = SmoothScrollAxis::new(0.0);
        let mut y = 0.0;
        for _ in 0..frames {
            y = motion.step(y, 1000.0, dt, 1.5).0;
        }
        y
    };
    assert!((chase(0.016, 20) - chase(0.08, 4)).abs() <= 1.0);
    assert!((chase(1.0 / 240.0, 72) - chase(1.0 / 60.0, 18)).abs() <= 1.0);
}

#[test]
fn rhythmic_notches_preserve_momentum_and_reverse_immediately() {
    let mut motion = SmoothScrollAxis::new(500.0);
    let mut y = 500.0;
    let mut target = y;
    for frame in 0..60 {
        if frame % 12 == 0 {
            target = wheel_scroll_target(y, target, -75.0, 10000.0);
        }
        let next = motion.step(y, target, 1.0 / 60.0, 1.5).0;
        assert!(next >= y && next <= target);
        assert!(next - y < 20.0, "rhythmic notch jumped {}px", next - y);
        y = next;
    }
    target = wheel_scroll_target(y, target, 75.0, 10000.0);
    let next = motion.step(y, target, 0.016, 1.5).0;
    assert!(next < y, "reverse input must cancel forward velocity");
}

#[test]
fn quantized_chase_settles_without_overshoot_or_long_tail() {
    for scale in [1.0, 1.25, 1.5, 2.0] {
        for distance in [0.4, 2.0, 16.0, 75.0, 240.0, -75.0] {
            let mut motion = SmoothScrollAxis::new(500.0);
            let target = snap_scroll_offset_to_device_px(500.0 + distance, scale);
            let mut y = 500.0;
            let mut done = false;
            for _ in 0..120 {
                let (next, anim) = motion.step(y, target, 1.0 / 120.0, scale);
                assert!((next * scale - (next * scale).round()).abs() < 0.001);
                assert!((next - y) * distance >= -0.001);
                assert!((target - next) * distance >= -0.001);
                y = next;
                if !anim {
                    done = true;
                    break;
                }
            }
            assert!(done && y == target, "did not settle: {y} target={target}");
        }
    }
}

#[test]
fn zero_elapsed_time_does_not_advance_and_external_jump_resets_motion() {
    let mut motion = SmoothScrollAxis::new(0.0);
    assert_eq!(motion.step(0.0, 75.0, 0.0, 1.5), (0.0, true));
    let y = motion.step(0.0, 75.0, 0.016, 1.5).0;
    assert!(y > 0.0);
    let jumped = motion.step(500.0, 575.0, 0.016, 1.5).0;
    assert!(jumped > 500.0 && jumped <= 503.0);
}

#[test]
fn wheel_burst_during_stall_has_bounded_recovery_distance() {
    let current = 1000.0;
    let mut target = current;
    for _ in 0..15 {
        target = wheel_scroll_target(current, target, -75.0, 10000.0);
    }
    let mut motion = SmoothScrollAxis::new(current);
    let mut recovered = current;
    for _ in 0..4 {
        recovered = motion.step(recovered, target, 1.0 / 60.0, 1.5).0;
    }
    assert!(target - current <= 225.01);
    assert!(
        recovered - current <= 110.0,
        "recovery jumped {}px",
        recovered - current
    );
    assert_eq!(
        wheel_scroll_target(current, target, 75.0, 10000.0),
        current - 75.0
    );
}

#[test]
fn wheel_target_preserves_normal_input_and_one_large_event() {
    assert_eq!(wheel_scroll_target(1000.0, 1000.0, -75.0, 10000.0), 1075.0);
    assert_eq!(wheel_scroll_target(1000.0, 1075.0, 0.0, 10000.0), 1075.0);
    assert_eq!(wheel_scroll_target(1000.0, 1000.0, -600.0, 10000.0), 1600.0);
    assert_eq!(wheel_scroll_target(50.0, 50.0, 75.0, 10000.0), 0.0);
    assert_eq!(wheel_scroll_target(9950.0, 9950.0, -75.0, 10000.0), 10000.0);
}
