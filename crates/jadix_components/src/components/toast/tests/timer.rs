use super::*;

#[test]
fn toast_timer_pause_floor_and_resume() {
    clear(1);
    arm(1, Duration::from_millis(50), true);
    std::thread::sleep(Duration::from_millis(60));
    pause(1);
    TIMERS.with(|m| {
        let t = m.borrow().get(&1).unwrap().clone();
        assert!(t.running_since.is_none());
        assert!(t.remaining >= TOAST_PAUSE_FLOOR);
    });
    resume(1);
    assert!(!expired(1));
    clear(1);
}

#[test]
fn toast_timer_arm_running_expires() {
    clear(2);
    arm(2, Duration::from_millis(1), true);
    std::thread::sleep(Duration::from_millis(5));
    assert!(expired(2));
    clear(2);
}
