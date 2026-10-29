use super::*;

#[cfg(feature = "performance")]
#[test]
fn window_presentations_exclude_other_windows() {
    let first = winit::window::WindowId::from_raw(1001);
    let second = winit::window::WindowId::from_raw(1002);
    forget_window_presentations(first);
    forget_window_presentations(second);
    note_presented_frame(first);
    note_presented_frame(second);
    note_presented_frame(first);
    assert_eq!(window_presented_frames_count(first), 2);
    assert_eq!(window_presented_frames_count(second), 1);
    forget_window_presentations(first);
    assert_eq!(window_presented_frames_count(first), 0);
    forget_window_presentations(second);
}

#[cfg(feature = "performance")]
#[test]
fn window_fps_drops_history_when_presenting_slows_down() {
    let start = std::time::Instant::now();
    let mut clock = WindowPresentationClock::default();
    for frame in 0..101 {
        clock.note(start + std::time::Duration::from_millis(frame * 10));
    }
    // No animation flag is needed: these are actual surface presents.
    for frame in 1..=2 {
        clock.note(start + std::time::Duration::from_millis(1000 + frame * 500));
    }
    let measured = clock.sample(start + std::time::Duration::from_millis(2000));
    assert_eq!(measured.fps, Some(2.0), "old fast frames must expire");
    assert_eq!(
        clock.sample(start + std::time::Duration::from_secs(4)).fps,
        Some(0.0),
        "no presents means zero rendered frames"
    );
}

#[cfg(feature = "performance")]
#[test]
fn genuinely_slow_animation_is_not_hidden_as_idle() {
    let start = std::time::Instant::now();
    let mut frames = WindowPresentationClock {
        continuous: true,
        ..Default::default()
    };
    for tick in 0..5 {
        frames.note(start + std::time::Duration::from_millis(tick * 500));
    }
    assert_eq!(
        frames
            .sample(start + std::time::Duration::from_millis(2100))
            .fps,
        Some(2.0)
    );
}

#[cfg(feature = "performance")]
#[test]
fn idle_hud_updates_count_as_actual_presentations() {
    let mut frames = WindowPresentationClock::default();
    let start = std::time::Instant::now();
    for tick in 0..5 {
        frames.note(start + std::time::Duration::from_millis(tick * 500));
    }
    let sample = frames.sample(start + std::time::Duration::from_millis(2100));
    assert_eq!(sample.frames, 5);
    assert_eq!(sample.fps, Some(2.0));
    assert_eq!(sample.sample_frames, 2);
    assert_eq!(sample.sample_seconds, 1.0);
}

#[cfg(feature = "performance")]
#[test]
fn performance_hud_refresh_is_excluded_from_page_cadence() {
    let mut frames = WindowPresentationClock::default();
    let start = std::time::Instant::now();
    frames.note(start);
    frames.overlay_update_pending = true;
    frames.note(start + std::time::Duration::from_millis(500));
    let sample = frames.sample(start + std::time::Duration::from_millis(1000));
    assert_eq!(sample.sample_frames, 1);
    assert_eq!(sample.content_sample_frames, 0);
    assert_eq!(sample.fps, Some(1.0));
    assert_eq!(sample.content_fps, Some(0.0));
}

#[cfg(feature = "performance")]
#[test]
fn window_fps_expires_by_time_without_animation_metadata() {
    let mut frames = WindowPresentationClock::default();
    let start = std::time::Instant::now();
    for frame in 0..20 {
        frames.note(start + std::time::Duration::from_millis(frame * 16));
    }
    let live = frames.sample(start + std::time::Duration::from_millis(304));
    assert!((live.fps.unwrap() - 62.5).abs() < 0.01);
    assert_eq!(
        frames.sample(start + std::time::Duration::from_secs(2)).fps,
        Some(0.0)
    );
}

#[cfg(feature = "performance")]
#[test]
fn render_timings_drain_only_the_requested_window() {
    let first = winit::window::WindowId::from_raw(1001);
    let second = winit::window::WindowId::from_raw(1002);
    enable_frame_timings(true);
    note_frame_timings(first, 12.0, 3.0, 2.0);
    note_frame_timings(second, 99.0, 20.0, 8.0);
    let sample = take_window_frame_timings(first);
    assert_eq!(sample.frames, 1);
    assert_eq!(sample.total_ms, 12.0);
    assert_eq!(take_window_frame_timings(first).frames, 0);
    assert_eq!(take_window_frame_timings(second).total_ms, 99.0);
    enable_frame_timings(false);
}

#[test]
fn note_collect_skip_counts() {
    reset_collect_skip_counters();
    note_collect_skip(2, 1);
    note_collect_skip(3, 0);
    assert_eq!(skipped_siblings_count(), 5);
    assert_eq!(patched_dl_count(), 1);
    assert_eq!(last_skipped_siblings(), 3);
    assert_eq!(last_patched_dl(), 0);
}
