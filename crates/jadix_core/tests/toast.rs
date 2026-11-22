use jadix_core::{clock::{self, ManualClock}, reactivity::create_root, toast::*};
use std::time::Duration;

#[test]
fn independent_holds_preserve_remaining_time_and_owner_cancels_expiration() {
    let clock = ManualClock::default();
    clock::with_clock(clock.clock(), || {
        let (owner, queue) = create_root(|| ToastQueue::new().unwrap());
        let id = queue.push("Saved", ToastOptions { duration: Some(Duration::from_secs(5)), ..Default::default() }).unwrap();
        clock.advance(Duration::from_secs(2)).unwrap();
        queue.hold(id, ToastHold::Hover, true);
        queue.hold(id, ToastHold::Focus, true);
        queue.hold(id, ToastHold::Hover, false);
        clock.advance(Duration::from_secs(20)).unwrap(); clock::flush();
        assert_eq!(queue.items().len(), 1);
        assert!(clock::next_wake_at().is_none());
        queue.hold(id, ToastHold::Focus, false);
        clock.advance(Duration::from_millis(2999)).unwrap(); clock::flush();
        assert_eq!(queue.items().len(), 1);
        clock.advance(Duration::from_millis(1)).unwrap(); clock::flush();
        assert!(queue.items().is_empty());
        queue.push("Sticky", ToastOptions { duration: None, ..Default::default() }).unwrap();
        queue.push("Later", Default::default()).unwrap();
        owner.dispose();
        assert!(clock::next_wake_at().is_none());
        assert!(queue.push("Disposed", Default::default()).is_err());
    });
}
