use jadix_core::{clock::*, reactivity::{create_root, create_signal}};
use std::{cell::RefCell, rc::Rc, time::Duration};

#[test]
fn virtual_deadlines_order_rearm_and_dispose_without_sleep() {
    let manual = ManualClock::default();
    with_clock(manual.clock(), || {
        let log = Rc::new(RefCell::new(Vec::new()));
        let (owner, (first, second, postponed)) = create_root(|| {
            (Timer::new().unwrap(), Timer::new().unwrap(), Timer::new().unwrap())
        });
        let later = postponed.clone(); let output = log.clone();
        first.arm(Duration::from_millis(10), move || {
            output.borrow_mut().push(1);
            assert_eq!(tick(), 0); // An in-flight timer cannot recursively drain.
            let output = output.clone();
            later.arm(Duration::ZERO, move || output.borrow_mut().push(4)).unwrap();
        }).unwrap();
        let output = log.clone();
        second.arm(Duration::from_millis(10), move || output.borrow_mut().push(2)).unwrap();
        let output = log.clone();
        postponed.arm(Duration::from_millis(10), move || output.borrow_mut().push(3)).unwrap();
        manual.advance(Duration::from_millis(9)).unwrap(); assert_eq!(flush(), 0);
        assert_eq!(next_wake_delay(), Some(Duration::from_millis(1)));
        manual.advance(Duration::from_millis(1)).unwrap(); assert_eq!(flush(), 2);
        assert_eq!(*log.borrow(), [1, 2]);
        assert_eq!(flush(), 1); assert_eq!(*log.borrow(), [1, 2, 4]);
        first.arm(Duration::ZERO, || panic!("disposed timer ran")).unwrap();
        owner.dispose(); assert_eq!(flush(), 0);
        assert_eq!(next_wake_at(), None);
        assert!(first.arm(Duration::ZERO, || {}).is_err());
    });
}

#[test]
fn clock_scope_restores_and_hover_uses_the_same_sample() {
    use jadix_core::hover_intent::*;
    let manual = ManualClock::default();
    with_clock(manual.clock(), || {
        let base = now();
        let nested = ManualClock::new(base + Duration::from_secs(20));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            with_clock(nested.clock(), || { assert_eq!(now(), base + Duration::from_secs(20)); panic!("restore"); });
        }));
        assert!(result.is_err()); assert_eq!(now(), base);
        let (owner, (open, trigger)) = create_root(|| {
            let open = create_signal(false); let trigger = create_signal(false);
            bind_hover_intent_effect("shared-clock", HoverIntent {
                open, trigger_hovered: trigger, content_hovered: create_signal(false),
                open_delay_ms: create_signal(300), close_delay_ms: create_signal(100),
            });
            (open, trigger)
        });
        trigger.set(true); manual.advance(Duration::from_millis(299)).unwrap(); flush(); assert!(!open.peek());
        manual.advance(Duration::from_millis(1)).unwrap(); flush(); assert!(open.peek());
        trigger.set(false); manual.advance(Duration::from_millis(100)).unwrap(); flush(); assert!(!open.peek());
        owner.dispose();
    });
}
