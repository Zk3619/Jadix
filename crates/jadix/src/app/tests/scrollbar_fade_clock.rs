use super::{content_scroll_paint_only, retarget_scrollbar_fade};
use crate::style::{tick_animations, CubicTween};

#[test]
#[cfg(feature = "animation-base")]
fn scrollbar_hide_after_idle_keeps_its_first_frame_and_full_duration() {
    let fade = CubicTween::new_ui_transition(0.0);
    fade.set(1.0);
    tick_animations(0.08);
    tick_animations(0.08);
    assert_eq!(fade.peek(), 1.0);
    // Hover has settled, focus/caret wakeups run, then the hide deadline
    // retargets before tick. None of the preceding idle interval is fade time.
    tick_animations(0.52);
    retarget_scrollbar_fade(&fade, 0.0);
    assert!(tick_animations(0.52).any);
    assert_eq!(fade.peek(), 1.0, "idle time consumed the start of the fade");
    for _ in 0..8 {
        retarget_scrollbar_fade(&fade, 0.0);
        tick_animations(0.016);
    }
    assert!(fade.is_active(), "a 150ms fade must still run at 128ms");
    assert!(fade.peek() > 0.0 && fade.peek() < 1.0);
    for _ in 0..2 {
        retarget_scrollbar_fade(&fade, 0.0);
        tick_animations(0.016);
    }
    assert!(!fade.is_active());
    assert_eq!(fade.peek(), 0.0);
}

#[test]
#[cfg(feature = "animation-base")]
fn scrollbar_start_does_not_pause_other_cubics_or_jump_on_reversal() {
    let fade = CubicTween::new_ui_transition(1.0);
    let other = CubicTween::new_ui_transition(0.0);
    other.set(1.0);
    retarget_scrollbar_fade(&fade, 0.0);
    tick_animations(0.08);
    assert_eq!(fade.peek(), 1.0);
    assert!(
        other.peek() > 0.5,
        "an existing animation must keep advancing"
    );
    tick_animations(0.04);
    let mid = fade.peek();
    assert!(mid > 0.0 && mid < 1.0);
    retarget_scrollbar_fade(&fade, 1.0);
    tick_animations(0.08);
    assert_eq!(
        fade.peek(),
        mid,
        "reversal reused the preceding frame interval"
    );
    tick_animations(0.016);
    assert!(fade.peek() > mid);
}

#[test]
#[cfg(feature = "animation-base")]
fn scrollbar_fade_does_not_enter_content_paint_only() {
    assert!(!content_scroll_paint_only(false, false, false));
    assert!(content_scroll_paint_only(true, false, false));
    assert!(content_scroll_paint_only(false, true, false));
    assert!(content_scroll_paint_only(false, false, true));
}
