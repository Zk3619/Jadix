use super::*;

#[test]
#[cfg(feature = "animation-base")]
fn underdamped_overshoots_then_settles() {
    let spring = Spring::from_physics(
        0.0,
        1.0,
        0.0,
        SpringPhysics {
            stiffness: 100.0,
            damping: 10.0,
            mass: 1.0,
        },
    );
    let mut peak = 0.0_f32;
    for i in 0..40 {
        peak = peak.max(spring.next(i as f32 * 20.0).value);
    }
    assert!(peak > 1.0, "underdamped must overshoot, peak={peak}");
    let mut settled = false;
    for i in 0..80 {
        if spring.next(i as f32 * 50.0).done {
            settled = true;
            break;
        }
    }
    assert!(settled);
    assert!((spring.next(20_000.0).value - 1.0).abs() < 1e-3);
}

#[test]
fn critically_damped_does_not_overshoot() {
    // ζ = 1 ⇒ d = 2√(k m). k=100, m=1 ⇒ d=20.
    let spring = Spring::from_physics(
        0.0,
        1.0,
        0.0,
        SpringPhysics {
            stiffness: 100.0,
            damping: 20.0,
            mass: 1.0,
        },
    );
    let mut max_v = 0.0_f32;
    for i in 0..40 {
        max_v = max_v.max(spring.next(i as f32 * 20.0).value);
    }
    assert!(max_v <= 1.0 + 0.02, "critical must not bounce, max={max_v}");
}

#[test]
#[cfg(feature = "animation-base")]
fn overdamped_is_slower_than_critical() {
    let critical = Spring::from_physics(
        0.0,
        1.0,
        0.0,
        SpringPhysics {
            stiffness: 100.0,
            damping: 20.0,
            mass: 1.0,
        },
    );
    let over = Spring::from_physics(
        0.0,
        1.0,
        0.0,
        SpringPhysics {
            stiffness: 100.0,
            damping: 40.0,
            mass: 1.0,
        },
    );
    let tc = critical.next(200.0).value;
    let to = over.next(200.0).value;
    assert!(to < tc, "overdamped {to} should lag critical {tc}");
}

#[test]
fn duration_bounce_resolves_to_physics() {
    let p = find_spring(800.0, 0.3, 1.0);
    assert!(p.stiffness > 10.0 && p.stiffness < 500.0, "{p:?}");
    assert!(p.damping > 1.0, "{p:?}");
    let spring = Spring::new(0.0, 100.0, 0.0, SpringOptions::duration_bounce(800.0, 0.3));
    let end = spring.next(800.0);
    assert!(end.done, "duration-resolved spring finishes at duration");
    assert!((end.value - 100.0).abs() < 1e-3);
}

#[test]
fn visual_duration_is_snappy() {
    let p = SpringOptions::visual(0.3, 0.25).resolve();
    assert!(p.stiffness > 50.0, "{p:?}");
    let s = Spring::new(0.0, 1.0, 0.0, SpringOptions::visual(0.3, 0.25));
    let mid = s.next(180.0);
    assert!(
        mid.value > 0.4,
        "visual 0.3s should be well along, {}",
        mid.value
    );
}

#[test]
fn retarget_keeps_velocity() {
    let mut s = Spring::from_physics(
        0.0,
        100.0,
        0.0,
        SpringPhysics {
            stiffness: 100.0,
            damping: 10.0,
            mass: 1.0,
        },
    );
    let a = s.next(80.0);
    s.retarget(a.value, 40.0, a.velocity);
    let b = s.next(16.0);
    assert!(
        (b.value - a.value).abs() > 0.5,
        "retargeted spring must keep moving"
    );
}

#[test]
fn inertia_coasts_toward_amplitude() {
    let mut n = Inertia::new(
        0.0,
        InertiaOptions {
            velocity: 400.0,
            power: 0.8,
            ..InertiaOptions::default()
        },
    );
    let far = n.next(800.0);
    assert!(far.value > 200.0, "got {}", far.value);
    assert!(far.done || n.next(4000.0).done);
}

#[test]
fn inertia_bounces_at_max() {
    let mut n = Inertia::new(
        0.0,
        InertiaOptions {
            velocity: 800.0,
            power: 1.0,
            max: Some(50.0),
            ..InertiaOptions::default()
        },
    );
    let mut hit_bound = false;
    for i in 0..40 {
        let s = n.next(i as f32 * 16.0);
        if s.value <= 55.0 && s.value >= 40.0 {
            hit_bound = true;
        }
        if s.done {
            assert!((s.value - 50.0).abs() < 1.0, "settle at max, {}", s.value);
            break;
        }
    }
    assert!(hit_bound);
}

#[test]
#[cfg(feature = "animation-ex")]
fn stagger_from_ends() {
    assert!((stagger_ms(0, 4, 50.0, StaggerFrom::First) - 0.0).abs() < 1e-3);
    assert!((stagger_ms(3, 4, 50.0, StaggerFrom::First) - 150.0).abs() < 1e-3);
    assert!((stagger_ms(3, 4, 50.0, StaggerFrom::Last) - 0.0).abs() < 1e-3);
    assert!((stagger_ms(0, 4, 50.0, StaggerFrom::Last) - 150.0).abs() < 1e-3);
    assert!((stagger_ms(1, 3, 40.0, StaggerFrom::Center) - 0.0).abs() < 1e-3);
    assert!((stagger_ms(0, 3, 40.0, StaggerFrom::Center) - 40.0).abs() < 1e-3);
}

#[test]
fn parse_positional_and_duration() {
    let k = parse_spring_options("90").unwrap();
    assert!((k.stiffness.unwrap() - 90.0).abs() < 1e-3);
    let dur = parse_spring_options("800ms, 0.25").unwrap();
    assert!((dur.duration_ms.unwrap() - 800.0).abs() < 1e-3);
    assert!((dur.bounce.unwrap() - 0.25).abs() < 1e-3);
    let named = parse_spring_options("duration 400ms, bounce 0.2").unwrap();
    assert!((named.duration_ms.unwrap() - 400.0).abs() < 1e-3);
    let visual = parse_spring_options("visual 0.3").unwrap();
    assert!((visual.visual_duration_s.unwrap() - 0.3).abs() < 1e-3);
}

#[test]
#[cfg(feature = "animation-base")]
fn animate_handle_ticks_to_target() {
    let n = animate(0.0, 1.0);
    assert!(n.is_active());
    let mut done = false;
    for _ in 0..120 {
        let _ = tick_value_generators(1.0 / 60.0);
        if !n.is_active() {
            done = true;
            break;
        }
    }
    assert!(done, "animate() must settle");
    assert!((n.peek() - 1.0).abs() < 1e-2);
}

#[test]
#[cfg(feature = "animation-base")]
fn tween_driver_respects_duration() {
    let n = AnimatedNumber::tween(0.0, 10.0, 100.0, TimingFunction::Linear);
    let _ = tick_value_generators(0.05);
    let mid = n.peek();
    assert!(mid > 4.0 && mid < 6.5, "linear 50ms of 100ms, got {mid}");
    let _ = tick_value_generators(0.06);
    assert!(!n.is_active());
    assert!((n.peek() - 10.0).abs() < 1e-3);
}

#[test]
#[cfg(feature = "animation-base")]
fn set_retargets_from_live_value() {
    let n = AnimatedNumber::tween(0.0, 10.0, 200.0, TimingFunction::Linear);
    let _ = tick_value_generators(0.05);
    let mid = n.peek();
    assert!(mid > 1.5 && mid < 3.5, "got {mid}");
    n.set(0.0);
    let _ = tick_value_generators(1.0 / 60.0);
    let after = n.peek();
    assert!(
        (after - mid).abs() < 1.5,
        "retarget must start near live mid {mid}, got {after}"
    );
    assert!(n.is_active());
}

#[test]
#[cfg(feature = "animation-base")]
fn pause_freezes_play_resumes() {
    let n = AnimatedNumber::tween(0.0, 10.0, 200.0, TimingFunction::Linear);
    let _ = tick_value_generators(0.05);
    let mid = n.peek();
    n.pause();
    assert!(n.is_paused());
    let _ = tick_value_generators(0.05);
    assert!((n.peek() - mid).abs() < 1e-4, "pause must freeze");
    n.play();
    let _ = tick_value_generators(0.05);
    assert!(n.peek() > mid + 1.0, "play must resume, {}", n.peek());
}

#[test]
#[cfg(feature = "animation-base")]
fn speed_scales_elapsed() {
    let n = AnimatedNumber::tween(0.0, 10.0, 200.0, TimingFunction::Linear).speed(2.0);
    let _ = tick_value_generators(0.05);
    let v = n.peek();
    assert!(v > 4.0 && v < 6.5, "2x of 50ms on 200ms → ~5, got {v}");
}

#[test]
#[cfg(feature = "animation-base")]
fn pingpong_reverses() {
    let n = AnimatedNumber::tween(0.0, 10.0, 100.0, TimingFunction::Linear)
        .repeat(RepeatMode::PingPong);
    let _ = tick_value_generators(0.10);
    assert!((n.peek() - 10.0).abs() < 0.6, "end of first, {}", n.peek());
    let _ = tick_value_generators(0.05);
    let back = n.peek();
    assert!(back < 8.0, "pingpong should reverse, got {back}");
    assert!(n.is_active());
}

#[test]
#[cfg(feature = "animation-base")]
fn then_to_plays_next_segment() {
    let n = AnimatedNumber::tween(0.0, 10.0, 50.0, TimingFunction::Linear)
        .then_to(0.0, ValueDriver::tween(50.0, TimingFunction::Linear));
    let _ = tick_value_generators(0.05);
    assert!(n.peek() > 8.0, "first segment {}", n.peek());
    let _ = tick_value_generators(0.05);
    assert!(
        n.peek() < 2.0,
        "second segment should return toward 0, {}",
        n.peek()
    );
    assert!(n.finished().peek() || !n.is_active() || n.peek() < 2.0);
}

#[test]
fn follow_signal_chases() {
    let src = Signal::new_unowned(0.0_f32);
    let n = follow(0.0).follow_signal(src);
    src.set(8.0);
    let mut moved = false;
    for _ in 0..20 {
        let _ = tick_value_generators(1.0 / 60.0);
        if n.peek() > 1.0 {
            moved = true;
            break;
        }
    }
    assert!(moved, "follower must chase the signal, {}", n.peek());
}

#[test]
#[cfg(feature = "animation-base")]
fn finished_signal_flips() {
    let n = AnimatedNumber::tween(0.0, 1.0, 50.0, TimingFunction::Linear);
    assert!(!n.finished().peek());
    let _ = tick_value_generators(0.06);
    assert!(n.finished().peek());
    n.set(0.0);
    assert!(!n.finished().peek());
}

#[test]
fn spring_step_closed_settles() {
    let mut pos = 0.0;
    let mut vel = 0.0;
    let p = SpringPhysics::default_css();
    let mut ok = false;
    for _ in 0..240 {
        let (np, nv, settled) =
            spring_step_closed(pos, vel, 1.0, 1.0 / 60.0, p.stiffness, p.damping, p.mass);
        pos = np;
        vel = nv;
        if settled {
            ok = true;
            break;
        }
    }
    assert!(ok);
    assert!((pos - 1.0).abs() < 1e-3);
}

#[test]
#[cfg(feature = "animation-base")]
fn css_mid_is_spring_next_at_elapsed() {
    let p = SpringPhysics::default_css();
    let s = Spring::from_physics(0.0, 1.0, 0.0, p).with_rest(8e-3, 8e-4);
    let mid = s.next(50.0);
    assert!(mid.value > 0.05 && mid.value < 0.95, "{}", mid.value);
    let again = Spring::from_physics(0.0, 1.0, 0.0, p)
        .with_rest(8e-3, 8e-4)
        .next(50.0);
    assert!((again.value - mid.value).abs() < 1e-6);
}

#[test]
#[cfg(feature = "animation-base")]
fn cubic_ui_curve_is_tween_sample() {
    let opts = TweenOptions {
        duration_ms: 150.0,
        easing: TimingFunction::CubicBezier(0.4, 0.0, 0.2, 1.0),
    };
    let a = tween_sample(0.0, 10.0, 75.0, opts);
    let t = 75.0 / 150.0;
    let eased = TimingFunction::CubicBezier(0.4, 0.0, 0.2, 1.0).eval(t);
    assert!((a.value - 10.0 * eased).abs() < 1e-4);
    assert!(!a.done);
    assert!(tween_sample(0.0, 10.0, 150.0, opts).done);
}

#[test]
fn replaced_handle_does_not_move_previous_signal() {
    let first = AnimatedNumber::follow(8.0);
    let bound = first.signal();
    let _second = AnimatedNumber::tween(8.0, 200.0, 40.0, TimingFunction::Linear);
    let _ = tick_value_generators(0.05);
    assert!(
        (bound.peek() - 8.0).abs() < 1e-3,
        "replacing the handle leaves the bound cell idle, {}",
        bound.peek()
    );
}

#[test]
fn restart_drives_original_signal() {
    let n = AnimatedNumber::follow(8.0);
    let sig = n.signal();
    n.restart(200.0, ValueDriver::tween(40.0, TimingFunction::Linear));
    let _ = tick_value_generators(0.05);
    assert!(
        sig.peek() > 50.0,
        "restart must tick the same signal, {}",
        sig.peek()
    );
}

#[test]
fn duration_clock_repeat_modes() {
    let (e, _, playing) = cycle_duration_clock(0.4, 0.5, RepeatMode::Once, false);
    assert!((e - 0.4).abs() < 1e-6 && playing);
    let (e, _, playing) = cycle_duration_clock(0.6, 0.5, RepeatMode::Once, false);
    assert!((e - 0.5).abs() < 1e-6 && !playing);
    let (e, _, playing) = cycle_duration_clock(0.7, 0.5, RepeatMode::Loop, false);
    assert!((e - 0.2).abs() < 1e-6 && playing);
    let (e, rev, playing) = cycle_duration_clock(0.6, 0.5, RepeatMode::PingPong, false);
    assert!((e - 0.1).abs() < 1e-6 && rev && playing);
}
