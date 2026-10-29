use super::*;
use std::time::Duration;

fn event(presented: bool) -> ActiveEvent {
    ActiveEvent {
        presented,
        ime_ms: 1.0,
        semantics_ms: 2.0,
    }
}

#[test]
fn budgets_include_input_ime_shared_and_redraw_without_double_counting_stages() {
    enable_application_frame_timings(Some(16.0));
    let start = Instant::now();
    let window = WindowId::from_raw(1);
    TIMINGS.with(|timings| {
        let mut timings = timings.borrow_mut();
        let timings = timings.as_mut().unwrap();
        timings.shared_ms = 3.0;
        timings.finish(
            window,
            EventKind::Ime,
            start,
            start + Duration::from_millis(5),
            event(false),
        );
    });
    assert_eq!(take_application_frame_timings()[0].frames, 0);
    TIMINGS.with(|timings| {
        timings.borrow_mut().as_mut().unwrap().finish(
            window,
            EventKind::Redraw,
            start + Duration::from_millis(10),
            start + Duration::from_millis(20),
            event(true),
        )
    });
    let report = take_application_frame_timings().remove(0);
    assert_eq!(report.over_budget, 1);
    assert_eq!(report.p95_ms, 18.0);
    let sample = report.samples[0];
    assert_eq!(sample.ime_ms, 2.0);
    assert_eq!(sample.semantics_ms, 4.0);
    assert_eq!(sample.input_events, 1);
    assert_eq!(sample.ime_events, 1);
    assert_eq!(sample.input_to_present_ms, Some(20.0));
    enable_application_frame_timings(None);
}

#[test]
fn samples_are_bounded_window_scoped_and_skipped_redraws_preserve_input() {
    enable_application_frame_timings(Some(1.0));
    let start = Instant::now();
    TIMINGS.with(|timings| {
        let mut timings = timings.borrow_mut();
        let timings = timings.as_mut().unwrap();
        timings.finish(
            WindowId::from_raw(1),
            EventKind::Input,
            start,
            start,
            event(false),
        );
        timings.finish(
            WindowId::from_raw(1),
            EventKind::Redraw,
            start,
            start,
            event(false),
        );
        for _ in 0..SAMPLE_LIMIT + 4 {
            timings.finish(
                WindowId::from_raw(2),
                EventKind::Redraw,
                start,
                start,
                event(true),
            );
        }
    });
    let reports = take_application_frame_timings();
    assert_eq!(reports[0].skipped_redraws, 1);
    assert_eq!(reports[0].frames, 0);
    assert_eq!(reports[1].samples.len(), SAMPLE_LIMIT);
    assert_eq!(reports[1].dropped_samples, 4);
    assert!(reports[1]
        .samples
        .iter()
        .all(|sample| sample.input_events == 0));
    forget_window(WindowId::from_raw(1));
    assert_eq!(take_application_frame_timings().len(), 1);
    enable_application_frame_timings(Some(f64::NAN));
    assert!(take_application_frame_timings().is_empty());
}
