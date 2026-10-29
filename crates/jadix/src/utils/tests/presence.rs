use super::*;

#[test]
#[cfg(feature = "animation-base")]
fn exit_gate_present_exit_absent() {
    let t0 = Instant::now();
    let mut g = ExitGate::new(true, Duration::from_millis(100));
    assert_eq!(g.phase(), PresencePhase::Present);
    assert!(g.set_present(false, t0));
    assert_eq!(g.phase(), PresencePhase::Exiting);
    assert!(!g.tick(t0 + Duration::from_millis(50)));
    assert!(g.is_mounted());
    assert!(g.tick(t0 + Duration::from_millis(100)));
    assert_eq!(g.phase(), PresencePhase::Absent);
    assert!(!g.is_mounted());
}

#[test]
fn exit_gate_reenter_cancels_exit() {
    let t0 = Instant::now();
    let mut g = ExitGate::new(true, Duration::from_millis(200));
    g.set_present(false, t0);
    assert!(g.set_present(true, t0 + Duration::from_millis(10)));
    assert_eq!(g.phase(), PresencePhase::Present);
    assert!(g.exit_started().is_none());
}

#[test]
fn exit_elapsed_helpers() {
    let t0 = Instant::now();
    assert_eq!(
        exit_elapsed_at(t0, Duration::from_millis(50), t0),
        !cfg!(feature = "animation-base")
    );
    assert!(exit_elapsed_at(
        t0,
        Duration::from_millis(50),
        t0 + Duration::from_millis(50)
    ));
}

#[test]
#[cfg(feature = "animation-base")]
fn generic_presence_exits_after_duration() {
    use crate::core::reactivity::{create_root, use_signal};

    create_root(|| {
        let show = use_signal("gp-test-show", || true);
        let _el = presence("gp-demo", show, 50, || Element::new().text("hi"));
        let handle = use_generic_presence("gp-demo", 50);
        assert_eq!(sync_generic_presence(&handle, true), PresencePhase::Present);
        assert_eq!(
            sync_generic_presence(&handle, false),
            PresencePhase::Exiting
        );
        assert!(handle.mounted().get());
        let t0 = Instant::now();
        GENERIC_PRESENCE_REGISTRY.with(|r| {
            for cell in r.borrow().iter() {
                let mut c = cell.borrow_mut();
                if c.gate.phase() == PresencePhase::Exiting {
                    c.gate.exit_started = Some(t0 - Duration::from_millis(60));
                }
            }
        });
        assert!(tick_generic_presence());
        assert_eq!(handle.phase(), PresencePhase::Absent);
        assert!(!handle.mounted().get());
    });
}

#[test]
#[cfg(feature = "animation-base")]
fn generic_presence_tick_notifies_mounted_readers() {
    use crate::core::reactivity::{create_effect, create_root};
    use std::cell::Cell;
    use std::rc::Rc;

    create_root(|| {
        let handle = use_generic_presence("gp-reader", 50);
        let runs = Rc::new(Cell::new(0u32));
        create_effect({
            let handle = handle.clone();
            let runs = Rc::clone(&runs);
            move || {
                let _ = handle.mounted().get();
                runs.set(runs.get() + 1);
            }
        });
        assert_eq!(sync_generic_presence(&handle, true), PresencePhase::Present);
        let before = runs.get();
        assert_eq!(
            sync_generic_presence(&handle, false),
            PresencePhase::Exiting
        );
        let t0 = Instant::now();
        GENERIC_PRESENCE_REGISTRY.with(|r| {
            for cell in r.borrow().iter() {
                let mut c = cell.borrow_mut();
                if c.gate.phase() == PresencePhase::Exiting {
                    c.gate.exit_started = Some(t0 - Duration::from_millis(60));
                }
            }
        });
        assert!(tick_generic_presence());
        assert!(!handle.mounted().get());
        assert!(
            runs.get() > before,
            "mounted.set(false) must flush readers after tick releases cell borrows"
        );
    });
}
