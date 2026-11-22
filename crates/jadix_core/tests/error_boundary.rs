use std::{cell::Cell, rc::Rc};
use jadix_core::{reactivity::*, error_boundary::*};

#[test]
fn effects_memos_events_and_timers_recover_without_stopping_siblings() {
    let (root, (source, boundary, owner, sibling, cleanup)) = create_root(|| {
        let source = create_signal(0);
        let sibling = create_signal(0);
        create_effect(move || sibling.set(source.get()));
        let boundary = ErrorBoundary::new();
        let cleanup = Rc::new(Cell::new(0));
        let cleaned = cleanup.clone();
        let owner = boundary.run(|| {
            on_cleanup(move || cleaned.set(cleaned.get() + 1));
            let memo = create_memo(move || { let value = source.get(); assert!(value != 2, "memo failed"); value });
            create_effect(move || { let value = memo.get(); assert!(value != 1, "effect failed"); });
            current_owner().unwrap()
        }).unwrap();
        (source, boundary, owner, sibling, cleanup)
    });
    source.set(1);
    assert_eq!(boundary.error().as_deref(), Some("effect failed"));
    assert_eq!(sibling.peek(), 1);
    assert!(run_owned_callback(owner, || panic!("stale callback")).is_none());
    boundary.reset();
    assert_eq!(cleanup.get(), 1);
    assert!(run_owned_callback(owner, || ()).is_none());
    source.set(0);
    let owner = boundary.run(|| {
        let memo = create_memo(move || { let value = source.get(); assert!(value != 2, "memo failed"); value });
        create_effect(move || { memo.get(); });
        current_owner().unwrap()
    }).unwrap();
    source.set(2);
    assert_eq!(boundary.error().as_deref(), Some("memo failed"));
    assert_eq!(sibling.peek(), 2);
    boundary.reset();
    let owner2 = boundary.run(|| current_owner().unwrap()).unwrap();
    assert_ne!(owner, owner2);
    run_owned_callback(owner2, || panic!("event failed"));
    assert_eq!(boundary.error().as_deref(), Some("event failed"));
    boundary.reset();
    let clock = jadix_core::clock::ManualClock::default();
    jadix_core::clock::with_clock(clock.clock(), || {
        let timer = boundary.run(|| {
            let timer = jadix_core::clock::Timer::new().unwrap();
            timer.arm(Default::default(), || panic!("timer failed")).unwrap(); timer
        }).unwrap();
        jadix_core::clock::flush();
        assert_eq!(boundary.error().as_deref(), Some("timer failed"));
        drop(timer);
    });
    drop(root);
    assert!(boundary.run(|| panic!("dead boundary")).is_none());
}

#[test]
fn construction_and_nested_fallback_failures_restore_tracking() {
    let (_root, (outer, inner, source, observed)) = create_root(|| {
        let outer = ErrorBoundary::new();
        let inner = outer.run(ErrorBoundary::new).unwrap();
        assert!(inner.run(|| isolate_reads(|| untrack(|| {
            create_memo::<usize, _>(|| panic!("initial memo"));
        }))).is_none());
        assert_eq!(inner.error().as_deref(), Some("initial memo"));
        assert!(outer.error().is_none());
        let source = create_signal(1);
        let observed = create_signal(0);
        create_effect(move || observed.set(source.get()));
        (outer, inner, source, observed)
    });
    source.set(4);
    assert_eq!(observed.peek(), 4);
    // A fallback is owned by the enclosing scope, so it reaches the outer boundary.
    let fallback_owner = outer.run(|| current_owner().unwrap()).unwrap();
    run_owned_callback(fallback_owner, || panic!("fallback failed"));
    assert_eq!(outer.error().as_deref(), Some("fallback failed"));
    assert!(inner.run(|| ()).is_none());
}

#[test]
fn unhandled_panics_do_not_poison_the_scheduler_or_owner() {
    let (_root, source) = create_root(|| {
        let source = create_signal(0);
        create_effect(move || { if source.get() == 1 { panic!("unhandled"); } }); source
    });
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| source.set(1))).is_err());
    source.set(2);
    let seen = Rc::new(Cell::new(0)); let result = seen.clone();
    let (_next, ()) = create_root(|| { create_effect(move || result.set(source.get())); });
    assert_eq!(seen.get(), 2);
}

#[test]
fn teardown_finishes_all_descendants_and_reports_to_the_surviving_boundary() {
    let (root, (boundary, retired, cleaned)) = create_root(|| {
        let boundary = ErrorBoundary::new();
        let cleaned = Rc::new(Cell::new(0));
        let retired = boundary.run(|| {
            create_scope(|| {
                let cleaned = cleaned.clone();
                on_cleanup(move || cleaned.set(cleaned.get() + 1));
                create_scope(|| on_cleanup(|| panic!("cleanup failed")))
            })
        }).unwrap();
        (boundary, retired, cleaned)
    });
    drop(retired);
    assert_eq!(cleaned.get(), 1);
    assert_eq!(boundary.error().as_deref(), Some("cleanup failed"));
    drop(root);
}

#[test]
fn deferred_callbacks_keep_their_owner_and_continue_after_a_handled_failure() {
    let (_root, (boundary, seen)) = create_root(|| {
        let boundary = ErrorBoundary::new();
        let seen = Rc::new(Cell::new(0));
        let other = seen.clone();
        batch(|| {
            boundary.run(|| start_transition(|| panic!("deferred failed")));
            start_transition(move || other.set(other.get() + 1));
        });
        (boundary, seen)
    });
    assert_eq!(boundary.error().as_deref(), Some("deferred failed"));
    assert_eq!(seen.get(), 1);
}

