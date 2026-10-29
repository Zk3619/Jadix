use super::*;

#[test]
fn resolve_delay_active_end_delay_finish() {
    let t = Timing::default()
        .duration(100.0)
        .delay(50.0)
        .end_delay(20.0)
        .iterations(1.0);
    assert_eq!(t.resolve(10.0).phase, TimingPhase::Delay);
    assert_eq!(t.resolve(80.0).phase, TimingPhase::Active);
    assert!((t.resolve(80.0).local_t - 0.3).abs() < 1e-4);
    assert_eq!(t.resolve(160.0).phase, TimingPhase::EndDelay);
    assert!(!t.resolve(160.0).finished);
    assert!(t.resolve(180.0).finished);
}

#[test]
fn spring_step_settles_at_target() {
    let mut pos = 0.0f32;
    let mut vel = 0.0f32;
    let k = TimingFunction::default_spring();
    let TimingFunction::Spring {
        stiffness,
        damping,
        mass,
    } = k
    else {
        panic!("default spring");
    };
    let mut settled = false;
    for _ in 0..240 {
        let (p, v, ok) = spring_step(pos, vel, 1.0, 1.0 / 60.0, stiffness, damping, mass);
        pos = p;
        vel = v;
        if ok {
            settled = true;
            break;
        }
    }
    assert!(settled, "spring must settle, pos={pos}");
    assert!((pos - 1.0).abs() < 1e-3);
}

#[test]
fn parse_spring_variants() {
    assert!(parse_spring_timing("spring").unwrap().is_spring());
    assert!(parse_spring_timing("spring()").unwrap().is_spring());
    let TimingFunction::Spring { stiffness, .. } = parse_spring_timing("spring(90)").unwrap()
    else {
        panic!("spring(90)");
    };
    assert!((stiffness - 90.0).abs() < 1e-3);
    assert!(parse_spring_timing("ease").is_none());
    let TimingFunction::SpringOptions(opts) = parse_spring_timing("spring(800ms, 0.25)").unwrap()
    else {
        panic!("duration spring");
    };
    assert_eq!(opts.duration_ms, Some(800.0));
    let physics = opts.resolve();
    assert!(physics.stiffness > 10.0 && physics.damping > 1.0);
    assert!(parse_spring_timing("spring(visual 0.3)")
        .unwrap()
        .is_spring());
}
