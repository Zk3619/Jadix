use super::*;

#[test]
fn virtual_clock_seek_advance_and_cap_are_shared() {
    set_css_transition_virtual_timeline(true);
    set_css_transition_timeline_ms(12.0);
    advance_css_transition_timeline_ms(8.0);
    assert_eq!(css_timeline_now_ms(), 20.0);
    assert_eq!(css_transition_max_step_ms(), None);
    set_css_transition_max_step_ms(Some(-4.0));
    assert_eq!(css_transition_max_step_ms(), Some(0.0));
    set_css_transition_synthetic_clock(true);
    assert_eq!(css_timeline_now_ms(), 0.0);
    set_css_transition_virtual_timeline(false);
    assert_eq!(css_transition_max_step_ms(), Some(1000.0 / 60.0));
}

#[test]
fn reduced_motion_override_respects_disabled_animation() {
    set_prefers_reduced_motion(Some(false));
    assert_eq!(prefers_reduced_motion(), !cfg!(feature = "animation-base"));
    set_prefers_reduced_motion(Some(true));
    assert!(prefers_reduced_motion());
    set_prefers_reduced_motion(None);
}
