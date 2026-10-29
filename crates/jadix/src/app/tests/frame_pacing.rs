use super::*;

#[test]
fn default_is_120_fps_and_can_be_overridden() {
    assert_eq!(frame_interval(), Some(Duration::from_secs_f64(1.0 / 120.0)));
    set_frame_rate_limit(NonZeroU32::new(90));
    assert_eq!(frame_interval(), Some(Duration::from_secs_f64(1.0 / 90.0)));
    set_frame_rate_limit(None);
    assert_eq!(frame_interval(), None);
    set_frame_rate_limit(NonZeroU32::new(120));
}

#[test]
fn late_wakes_do_not_accumulate_into_a_lower_frame_rate() {
    let mut pacer = FramePacer::default();
    let start = Instant::now();
    let period = Duration::from_secs_f64(1.0 / 60.0);
    let interval = Some(period);
    assert!(pacer.begin_frame(start, interval));
    for frame in 1..=120 {
        let wake = pacer.deadline(start, interval) + Duration::from_micros(500);
        assert!(pacer.begin_frame(wake, interval));
        assert_eq!(pacer.deadline(wake, interval), start + period * (frame + 1));
    }
    // After idle or a full missed frame, resume without a burst of catch-up paints.
    let resume = start + Duration::from_secs(10);
    assert!(pacer.begin_frame(resume, interval));
    assert_eq!(pacer.deadline(resume, interval), resume + period);
    assert!(!pacer.begin_frame(resume, interval));
}

#[test]
fn coalesces_until_deadline_without_postponing_it_on_input() {
    let mut pacer = FramePacer::default();
    let start = Instant::now();
    let interval = Some(Duration::from_secs_f64(1.0 / 60.0));
    assert!(pacer.begin_frame(start, interval));
    for ms in 1..16 {
        let now = start + Duration::from_millis(ms);
        assert!(!pacer.begin_frame(now, interval));
        assert_eq!(pacer.deadline(now, interval), start + interval.unwrap());
        assert!(pacer.pending);
    }
    assert!(pacer.begin_frame(start + interval.unwrap(), interval));
    assert!(!pacer.pending);
}

#[test]
fn idle_has_no_wake_and_windows_have_independent_budgets() {
    let now = Instant::now();
    let mut a = FramePacer::default();
    let mut b = FramePacer::default();
    let interval = Some(Duration::from_millis(16));
    assert!(!a.pending);
    assert!(a.begin_frame(now, interval));
    assert!(b.begin_frame(now, interval));
    assert!(!a.pending && !b.pending);
    assert!(a.begin_frame(now, None));
    assert!(!a.begin_frame(now, interval));
    assert!(
        a.begin_frame(now, None),
        "removing the limit releases pending paint"
    );
}
