use super::*;
use crate::core::reactivity::{create_root, create_signal};

#[test]
fn resource_state_latest_prefers_ready() {
    let s = ResourceState::Loading {
        previous: Some(1_i32),
    };
    assert_eq!(s.latest(), Some(&1));
    assert_eq!(ResourceState::Ready(2).latest(), Some(&2));
}

#[test]
fn suspense_stack_marks_pending() {
    let (_o, _) = create_root(|| {
        let frame = push_suspense_frame();
        assert!(!suspense_frame_is_pending(&frame));
        mark_suspense_pending();
        assert!(suspense_frame_is_pending(&frame));
        pop_suspense_frame();
    });
}

#[test]
fn panic_in_suspense_frame_still_pops() {
    let (_o, _) = create_root(|| {
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _frame = push_suspense_frame();
            let _guard = super::SuspenseFrameGuard;
            panic!("user-render");
        }));
        assert!(panicked.is_err());
        let frame = push_suspense_frame();
        assert!(
            !suspense_frame_is_pending(&frame),
            "leaked frame would leave a stale stack slot"
        );
        pop_suspense_frame();
    });
}

#[test]
fn resource_get_marks_suspense_when_loading() {
    let (_o, _) = create_root(|| {
        let state = create_signal(ResourceState::<i32>::Loading { previous: None });
        let r = Resource { state };
        let frame = push_suspense_frame();
        assert!(r.get().is_none());
        assert!(suspense_frame_is_pending(&frame));
        pop_suspense_frame();
    });
}
