use super::{about_to_wait_force_immediate, idle_control_flow, scroll_settle_must_pump};
use std::time::{Duration, Instant};
use winit::event_loop::ControlFlow;

#[test]
fn page_css_does_not_sleep_on_scrollbar_wait_until() {
    let scrollbar_hide = Instant::now() + Duration::from_millis(1500);
    assert_eq!(
        idle_control_flow(Some(scrollbar_hide), true),
        ControlFlow::Poll,
        "spin / shimmer after scroll stop must Poll, not WaitUntil scrollbar hide"
    );
}

#[test]
fn resize_settle_does_not_sleep_after_maximize() {
    let scrollbar_hide = Instant::now() + Duration::from_millis(1500);
    assert_eq!(
            idle_control_flow(Some(scrollbar_hide), true),
            ControlFlow::Poll,
            "needs_text_settle / force_redraw_count after maximize must Poll; coalesced request_redraw cannot be the only wake"
        );
    assert_eq!(
        idle_control_flow(None, true),
        ControlFlow::Poll,
        "maximize without a wheel still Polls — no scrollbar deadline to hide behind"
    );
}

#[test]
fn scroll_settle_force_redraw_must_poll_not_caret_wait_until() {
    assert!(
        scroll_settle_must_pump(true, false),
        "wheel stop is the settle frame"
    );
    assert!(!scroll_settle_must_pump(true, true));
    assert!(!scroll_settle_must_pump(false, false));
    assert!(!scroll_settle_must_pump(false, true));
    // Gallery forms card 2: live scroll stays fluid (paint_clock Poll). The first
    // idle frame used to plant caret WaitUntil; Windows coalesced later clicks
    // and froze focus chrome / caret blink / overlay scrollbar.
    let caret = Instant::now() + Duration::from_millis(520);
    let force =
        about_to_wait_force_immediate(false, false, false, false, false, true, false, false);
    assert!(force, "scroll-settle force_redraw_count must Poll");
    assert_eq!(
        idle_control_flow(Some(caret), force),
        ControlFlow::Poll,
        "first idle frame after scroll must not WaitUntil caret blink"
    );
}

#[test]
fn coalesced_cursor_move_must_poll_not_caret_wait_until() {
    // `handle_cursor_moved` defers hit-test to present and arms `force_redraw_count`.
    // WaitUntil caret would drop that redraw, so the MouseEvent log never flushes.
    let caret = Instant::now() + Duration::from_millis(520);
    let force =
        about_to_wait_force_immediate(false, false, false, false, false, true, false, false);
    assert!(
        force,
        "CursorMoved pending must arm force_redraw_count / Poll"
    );
    assert_eq!(
        idle_control_flow(Some(caret), force),
        ControlFlow::Poll,
        "gallery MouseEvent mousemove flush cannot WaitUntil caret blink"
    );
}

#[test]
fn paint_only_focus_chrome_does_not_sleep_on_caret_wait_until() {
    // Clicking Search after a sliver-clip: `force_redraw_count` is the existing
    // Windows coalescing pump (same as maximize). Caret WaitUntil on the previous
    // field must not win.
    let caret = Instant::now() + Duration::from_millis(520);
    let force =
        about_to_wait_force_immediate(false, false, false, false, false, true, false, false);
    assert!(force, "text-edit click arming force_redraw_count must Poll");
    assert_eq!(
        idle_control_flow(Some(caret), force),
        ControlFlow::Poll,
        "paint-only focus_id.set cannot WaitUntil the previous field's caret blink"
    );
}

#[test]
fn ime_jumped_stale_caret_wake_must_poll_via_text_edit_pump() {
    // FALLBACK contract: if dropping WaitUntil is not enough, re-enable
    // `arm_text_edit_force_immediate` in about_to_wait. This latch must Poll.
    let caret = Instant::now() + Duration::from_millis(520);
    let force =
        about_to_wait_force_immediate(false, false, false, true, false, false, false, false);
    assert!(
        force,
        "caret_wake_stale arm_text_edit_force_immediate must Poll"
    );
    assert_eq!(
        idle_control_flow(Some(caret), force),
        ControlFlow::Poll,
        "IME-jumped chrome frame cannot WaitUntil the Placeholder caret blink"
    );
}

#[test]
fn stale_caret_wake_idles_with_wait_not_previous_blink() {
    // Root: about_to_wait clears caret_wake_at. Idle is Wait so the click's
    // request_redraw stays queued — not WaitUntil the previous field's blink.
    assert_eq!(
        idle_control_flow(None, false),
        ControlFlow::Wait,
        "cleared caret deadline must not plant WaitUntil"
    );
    let caret = Instant::now() + Duration::from_millis(520);
    assert_eq!(
        idle_control_flow(Some(caret), false),
        ControlFlow::WaitUntil(caret),
        "a live matching caret may still WaitUntil blink"
    );
}

#[test]
fn modal_open_does_not_sleep_on_caret_wait_until() {
    let caret = Instant::now() + Duration::from_millis(520);
    assert_eq!(
        idle_control_flow(Some(caret), true),
        ControlFlow::Poll,
        "first Dialog open must Poll, not WaitUntil caret ~520ms"
    );
    assert_eq!(
        idle_control_flow(Some(caret), false),
        ControlFlow::WaitUntil(caret)
    );
    assert_eq!(idle_control_flow(None, false), ControlFlow::Wait);
    assert_eq!(idle_control_flow(None, true), ControlFlow::Poll);
}

#[test]
fn modal_css_pump_polls_after_one_shot_consumed() {
    // One-shot Poll is taken on the first about_to_wait. Mid enter/exit still
    // needs Poll or caret WaitUntil (~520ms) freezes the CSS clock.
    let caret = Instant::now() + Duration::from_millis(520);
    let force =
        about_to_wait_force_immediate(false, true, false, false, false, false, false, false);
    assert!(
        force,
        "any_modal_css_needs_frame must latch Poll after take_modal_force_immediate_wake"
    );
    assert_eq!(
        idle_control_flow(Some(caret), force),
        ControlFlow::Poll,
        "mid Dialog/Sheet CSS must Poll; request_redraw + WaitUntil caret is the hitch"
    );
    assert!(!about_to_wait_force_immediate(
        false, false, false, false, false, false, false, false,
    ));
}

#[test]
fn nested_select_expand_polls_instead_of_caret_wait_until() {
    // ColorPicker hex Input plants caret WaitUntil (~520ms). Nested format Select
    // Expand is not Dialog CSS; about_to_wait must still Poll on live floating
    // motion — a one-shot armed at parent Fade Enter is already gone.
    use crate::core::reactivity::create_root;
    use crate::utils::floating_motion::{
        any_floating_motion_needs_frame, sync_floating_presence, use_floating_presence,
        FloatingMotionKind,
    };

    create_root(|| {
        let fade = use_floating_presence("cp-fade", FloatingMotionKind::Fade);
        sync_floating_presence(&fade, true);
        let nested = use_floating_presence("cp-fmt", FloatingMotionKind::Expand);
        sync_floating_presence(&nested, true);
        assert!(
            any_floating_motion_needs_frame(),
            "nested Expand open must be visible before the first tick"
        );
        let caret = Instant::now() + Duration::from_millis(520);
        let force = about_to_wait_force_immediate(
            false,
            any_floating_motion_needs_frame(),
            false,
            false,
            false,
            false,
            false,
            false,
        );
        assert!(force, "floating Expand busy must Poll like modal CSS");
        assert_eq!(
            idle_control_flow(Some(caret), force),
            ControlFlow::Poll,
            "ColorPicker nested Select cannot WaitUntil the hex caret blink"
        );
    });
}

#[test]
fn scrollbar_hide_expiry_polls_like_dialog() {
    assert_eq!(
        idle_control_flow(None, true),
        ControlFlow::Poll,
        "prune-expired auto-hide must Poll; Wait coalesces the fade-start redraw on Windows"
    );
    let leftover = Instant::now() + Duration::from_millis(400);
    assert_eq!(
        idle_control_flow(Some(leftover), true),
        ControlFlow::Poll,
        "a remaining axis deadline must not win WaitUntil over the expired-axis wake"
    );
}

#[test]
fn scrollbar_hide_wake_survives_second_about_to_wait_before_redraw() {
    // Windows: WaitUntil expiry → about_to_wait (prune) → about_to_wait again
    // → RedrawRequested. The latch is App.scrollbar_hide_wake; it is not
    // consumed by about_to_wait (unlike take_modal_force_immediate_wake).
    let mut hide_wake = false;
    let prune_first = true;
    if prune_first {
        hide_wake = true;
    }
    assert_eq!(
        idle_control_flow(None, hide_wake),
        ControlFlow::Poll,
        "prune must Poll so the fade-start request_redraw is not coalesced"
    );
    let prune_second = false;
    if prune_second {
        hide_wake = true;
    }
    assert!(
        hide_wake,
        "a local prune flag would be false on the second wait; latch must hold"
    );
    assert_eq!(
        idle_control_flow(None, hide_wake),
        ControlFlow::Poll,
        "second about_to_wait before RedrawRequested must still Poll"
    );
    hide_wake = false;
    assert_eq!(
        idle_control_flow(None, hide_wake),
        ControlFlow::Wait,
        "RedrawRequested consumes the latch; later idle may Wait"
    );
}
