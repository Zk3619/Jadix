use super::*;
use std::time::Duration;

#[test]
fn autoplay_mutex_and_advance() {
    let now = Instant::now();
    assert_eq!(
        decide_autoplay(1000, false, true, false, now, Some(now)),
        AutoplayAction::Skip
    );
    assert_eq!(
        decide_autoplay(1000, false, false, true, now, Some(now)),
        AutoplayAction::FinishLoop
    );
    assert_eq!(
        decide_autoplay(1000, false, false, false, now, None),
        AutoplayAction::ArmDeadline
    );
    assert_eq!(
        decide_autoplay(
            1000,
            false,
            false,
            false,
            now,
            Some(now - Duration::from_millis(1))
        ),
        AutoplayAction::Advance
    );
}

#[test]
fn visibility_gate_freezes_offscreen_and_swallows_enter_advance() {
    assert_eq!(
        gate_autoplay_visibility(false, false, AutoplayAction::Advance),
        AutoplayAction::Skip
    );
    assert_eq!(
        gate_autoplay_visibility(true, true, AutoplayAction::Advance),
        AutoplayAction::ArmDeadline
    );
    assert_eq!(
        gate_autoplay_visibility(true, false, AutoplayAction::Advance),
        AutoplayAction::Advance
    );
    assert_eq!(
        gate_autoplay_visibility(true, false, AutoplayAction::Skip),
        AutoplayAction::Skip
    );
}
