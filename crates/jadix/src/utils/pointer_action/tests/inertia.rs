use super::*;
use std::time::Duration;

#[test]
fn fling_requires_min_speed() {
    let opts = InertiaOpts {
        enabled: true,
        ..Default::default()
    };
    assert!(Inertia::try_fling([10.0, 0.0], opts).is_none());
    assert!(Inertia::try_fling([120.0, 0.0], opts).is_some());
    assert!(Inertia::try_fling([120.0, 0.0], InertiaOpts::default()).is_none());
}

#[test]
fn fling_decays_and_stops() {
    let opts = InertiaOpts {
        enabled: true,
        resistance: 10.0,
        min_speed: 50.0,
        end_speed: 10.0,
        ..Default::default()
    };
    let mut i = Inertia::try_fling([400.0, 0.0], opts).unwrap();
    let mut travelled = 0.0;
    let mut frames = 0;
    while let Some(d) = i.step(1.0 / 60.0) {
        travelled += d[0];
        frames += 1;
        if frames > 600 {
            panic!("did not stop");
        }
    }
    assert!(travelled > 10.0, "{travelled}");
    assert!(frames > 5);
}

#[test]
fn smooth_end_reaches_target() {
    let opts = InertiaOpts {
        enabled: true,
        smooth_end_duration: 0.3,
        ..Default::default()
    };
    let mut i = Inertia::try_smooth_end([0.0, 0.0], [30.0, 0.0], opts).unwrap();
    let mut x = 0.0;
    let mut frames = 0;
    while let Some(d) = i.step(1.0 / 60.0) {
        x += d[0];
        frames += 1;
        if frames > 120 {
            break;
        }
    }
    assert!((x - 30.0).abs() < 0.6, "{x}");
}

#[test]
fn velocity_window_ignores_old_samples() {
    let mut t = VelocityTracker::default();
    let t0 = Instant::now();
    t.sample([0.0, 0.0], t0);
    t.sample([100.0, 0.0], t0 + Duration::from_millis(20));
    let v = t.velocity();
    assert!(v[0] > 1000.0, "{v:?}");
    t.sample([100.0, 0.0], t0 + Duration::from_millis(200));
    t.sample([100.0, 0.0], t0 + Duration::from_millis(210));
    assert!(t.speed() < 50.0, "{}", t.speed());
}

#[test]
fn tiny_dt_does_not_explode() {
    let mut t = VelocityTracker::default();
    let t0 = Instant::now();
    t.sample([0.0, 0.0], t0);
    t.sample([8.0, 0.0], t0);
    assert_eq!(t.velocity(), [0.0, 0.0]);
}
