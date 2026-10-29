use super::*;
use crate::style::animation::{tick_element_animations, AnimationSnapshot};

#[test]
#[cfg(feature = "animation-base")]
fn play_pause_current_time_and_finished_signal() {
    let anim = Animation::from_effect(KeyframeEffect::named("spin").duration(1000.0));
    let mut style = crate::style::Style::default();
    style.animations.push(anim.to_entry());
    let mut snap = AnimationSnapshot::default();
    assert!(tick_element_animations(&mut style, &mut snap, 0.08));
    let t0 = anim.current_time();
    assert!(t0 > 0.0, "got {t0}");
    anim.pause();
    let _ = tick_element_animations(&mut style, &mut snap, 0.08);
    let paused = anim.current_time();
    assert!(
        (paused - t0).abs() < 1.0,
        "pause must freeze, {paused} vs {t0}"
    );
    anim.set_current_time(500.0);
    anim.play();
    let _ = tick_element_animations(&mut style, &mut snap, 0.0);
    assert!((anim.current_time() - 500.0).abs() < 1.0);
    anim.finish();
    let _ = tick_element_animations(&mut style, &mut snap, 0.0);
    assert!(anim.finished().get());
}

#[test]
fn cancel_does_not_finish_and_fires_on_cancel() {
    let anim = Animation::from_effect(KeyframeEffect::named("spin").duration(1000.0));
    let fired = std::rc::Rc::new(std::cell::Cell::new(false));
    let flag = fired.clone();
    anim.on_cancel(move || flag.set(true));
    anim.on_finish(|| panic!("cancel must not fire onfinish"));
    anim.cancel();
    assert!(anim.cancelled().get());
    assert!(!anim.finished().get());
    assert!(fired.get());
    assert!(get_animations().iter().all(|a| a.id() != anim.id()));
}

#[test]
fn reverse_flips_playback_rate() {
    let anim = Animation::from_effect(KeyframeEffect::named("spin").duration(1000.0));
    assert!((anim.playback_rate() - 1.0).abs() < 1e-6);
    anim.reverse();
    assert!((anim.playback_rate() + 1.0).abs() < 1e-6);
    anim.update_playback_rate(2.0);
    assert!((anim.playback_rate() - 2.0).abs() < 1e-6);
}

#[test]
fn computed_timing_active_duration() {
    let e = KeyframeEffect::named("spin")
        .duration(8000.0)
        .iterations(2.0)
        .end_delay(100.0);
    let t = e.model();
    assert!((t.duration_ms - 8000.0).abs() < 1e-3);
    assert!((t.active_duration_ms() - 16000.0).abs() < 1e-3);
    assert!((t.end_delay_ms - 100.0).abs() < 1e-3);
    let inf = KeyframeEffect::named("spin")
        .duration(3000.0)
        .iterations(f32::INFINITY)
        .model();
    assert!(matches!(
        inf.iterations,
        crate::style::AnimationIterationCount::Infinite
    ));
    assert!(inf.active_duration_ms().is_infinite());
}

#[test]
fn run_from_timing_handle() {
    let anim = Animation::run(
        vec![Keyframe::new().opacity(0.0), Keyframe::new().opacity(1.0)],
        crate::style::Timing::default().duration(200.0),
    );
    anim.set_rate(0.5);
    anim.seek(20.0);
    let mut style = crate::style::Style::default();
    style.animations.push(anim.to_entry());
    let mut snap = AnimationSnapshot::default();
    let _ = tick_element_animations(&mut style, &mut snap, 0.0);
    assert!((anim.current_time() - 20.0).abs() < 1.0);
}

#[test]
#[cfg(feature = "animation-base")]
fn keyframe_objects_offset_and_default_duration_zero() {
    let anim = Animation::animate(
        vec![
            Keyframe::new().opacity(0.0),
            Keyframe::new().opacity(1.0).offset(0.3),
            Keyframe::new().opacity(0.0),
        ],
        KeyframeEffect::new().duration(1000.0),
    );
    assert!(anim.effect().name.starts_with("__waapi_"));
    assert!((KeyframeEffect::new().duration_ms).abs() < 1e-6);
    let mut style = crate::style::Style::default();
    style.opacity = 1.0;
    style.animations.push(anim.to_entry());
    let mut snap = AnimationSnapshot::default();
    let _ = tick_element_animations(&mut style, &mut snap, 0.15);
    assert!(
        style.opacity > 0.05 && style.opacity < 0.95,
        "offset 0.3 midpoint should be mid opacity, got {}",
        style.opacity
    );
    assert!(get_animations().iter().any(|a| a.id() == anim.id()));
}

#[test]
#[cfg(feature = "animation-base")]
fn negative_rate_rewinds_current_time() {
    let anim = Animation::from_effect(KeyframeEffect::named("spin").duration(1000.0));
    let mut style = crate::style::Style::default();
    style.animations.push(anim.to_entry());
    let mut snap = AnimationSnapshot::default();
    for _ in 0..8 {
        let _ = tick_element_animations(&mut style, &mut snap, 0.08);
    }
    let t0 = anim.current_time();
    assert!(t0 > 400.0, "got {t0}");
    anim.update_playback_rate(-1.0);
    let _ = tick_element_animations(&mut style, &mut snap, 0.08);
    let t1 = anim.current_time();
    assert!(t1 < t0, "rate -1 must rewind, {t1} vs {t0}");
}
