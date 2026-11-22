use super::*;
use crate::reactivity::{create_root, use_signal};

#[test]
fn never_hover_delay_cancels_pending_transitions_and_can_be_reenabled() {
    let (_owner, _) = create_root(|| {
        let open = use_signal("hi-never-open", || false);
        let th = use_signal("hi-never-th", || true);
        let ch = use_signal("hi-never-ch", || false);
        let od = use_signal("hi-never-od", || 500_u64);
        let cd = use_signal("hi-never-cd", || 0_u64);
        bind_hover_intent_effect(
            "hi-never",
            HoverIntent {
                open,
                trigger_hovered: th,
                content_hovered: ch,
                open_delay_ms: od,
                close_delay_ms: cd,
            },
        );
        assert!(TIMERS.with(|timers| timers.borrow().contains_key("hi-never")));
        od.set(NEVER_HOVER_DELAY_MS);
        assert!(!TIMERS.with(|timers| timers.borrow().contains_key("hi-never")));
        assert!(!open.peek());
        od.set(0);
        assert!(open.peek());
        cd.set(NEVER_HOVER_DELAY_MS);
        th.set(false);
        assert!(open.peek());
        assert!(!TIMERS.with(|timers| timers.borrow().contains_key("hi-never")));
        cd.set(0);
        assert!(!open.peek());
        ()
    });
}

#[test]
fn zero_delay_open_closes_via_intent() {
    let (_owner, _) = create_root(|| {
        let open = use_signal("hi-zd-open", || false);
        let th = use_signal("hi-zd-th", || false);
        let ch = use_signal("hi-zd-ch", || false);
        let od = use_signal("hi-zd-od", || 0_u64);
        let cd = use_signal("hi-zd-cd", || 0_u64);
        let intent = HoverIntent {
            open,
            trigger_hovered: th,
            content_hovered: ch,
            open_delay_ms: od,
            close_delay_ms: cd,
        };
        arm_open("hi-zd", intent);
        assert!(!open.peek());
        th.set(true);
        arm_open("hi-zd", intent);
        assert!(open.peek());
        th.set(false);
        arm_close("hi-zd", intent);
        assert!(!open.peek());
        ()
    });
}

#[test]
fn delayed_open_does_not_fire_before_deadline() {
    let (_owner, _) = create_root(|| {
        let open = use_signal("hi-w-open", || false);
        let th = use_signal("hi-w-th", || true);
        let ch = use_signal("hi-w-ch", || false);
        let od = use_signal("hi-w-od", || 500_u64);
        let cd = use_signal("hi-w-cd", || 0_u64);
        let intent = HoverIntent {
            open,
            trigger_hovered: th,
            content_hovered: ch,
            open_delay_ms: od,
            close_delay_ms: cd,
        };
        arm_open("hi-w", intent);
        assert!(!open.peek());
        assert!(!tick_hover_intent());
        assert!(!open.peek());
        ()
    });
}

#[test]
fn pointer_tooltip_handoff_skips_delay_and_keyboard_open_does_not_start_grace() {
    let (_owner, _) = create_root(|| {
        let keyboard_open = use_signal("hi-pointer-tip-keyboard", || false);
        let first = use_signal("hi-pointer-tip-first", || false);
        let second = use_signal("hi-pointer-tip-second", || false);
        let after_leave = use_signal("hi-pointer-tip-after-leave", || false);

        keyboard_open.set(true);
        assert!(!pointer_tooltip_enter("hi-pointer-tip-first", first));
        first.set(true);
        assert!(pointer_tooltip_enter("hi-pointer-tip-second", second));
        second.set(true);
        pointer_tooltip_leave("hi-pointer-tip-first");
        pointer_tooltip_leave("hi-pointer-tip-second");
        assert!(pointer_tooltip_enter(
            "hi-pointer-tip-after-leave",
            after_leave
        ));

        after_leave.set(true);
        pointer_tooltip_leave("hi-pointer-tip-after-leave");
        end_pointer_tooltip_grace();
        assert!(!pointer_tooltip_enter(
            "hi-pointer-tip-after-action",
            keyboard_open
        ));
        clear_pointer_tooltip("hi-pointer-tip-first");
        clear_pointer_tooltip("hi-pointer-tip-second");
        clear_pointer_tooltip("hi-pointer-tip-after-leave");
        clear_pointer_tooltip("hi-pointer-tip-after-action");
        ()
    });
}

#[test]
fn pointer_tooltip_grace_is_active_only_before_its_300ms_deadline() {
    let now = Instant::now();
    let deadline = now + POINTER_TOOLTIP_GRACE;
    assert!(!pointer_tooltip_grace_active(None, now));
    assert!(pointer_tooltip_grace_active(Some(deadline), now));
    assert!(pointer_tooltip_grace_active(
        Some(deadline),
        deadline - Duration::from_nanos(1)
    ));
    assert!(!pointer_tooltip_grace_active(Some(deadline), deadline));
    assert!(!pointer_tooltip_grace_active(
        Some(deadline),
        deadline + Duration::from_nanos(1)
    ));
}
