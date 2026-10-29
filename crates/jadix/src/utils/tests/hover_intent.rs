use super::*;
use crate::core::reactivity::{create_root, use_signal};

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
        Element::new()
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
        Element::new()
    });
}
