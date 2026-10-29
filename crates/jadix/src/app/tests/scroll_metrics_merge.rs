use super::{is_stale_zero_max_sample, merge_scroll_metrics_cache, wheel_metrics_for_rest};
use crate::core::reactivity::{create_root, use_signal};
use crate::core::scroll_viewport::{scroll_content_h_signal_key, scroll_viewport_h_signal_key};
use std::collections::HashMap;

#[test]
fn zero_max_cull_sample_does_not_overwrite_live_inner() {
    let mut cache = HashMap::new();
    cache.insert(
        "inner".into(),
        ([0.0, 100.0, 320.0, 384.0], [0.0, 240.0], [320.0, 624.0]),
    );
    cache.insert(
        "outer".into(),
        ([0.0, 0.0, 400.0, 800.0], [0.0, 200.0], [400.0, 1000.0]),
    );
    merge_scroll_metrics_cache(
        &mut cache,
        [
            (
                "outer".into(),
                [0.0, 0.0, 400.0, 800.0],
                [0.0, 220.0],
                [400.0, 1020.0],
            ),
            (
                "inner".into(),
                [0.0, 80.0, 320.0, 384.0],
                [0.0, 0.0],
                [320.0, 384.0],
            ),
        ],
    );
    let inner = cache.get("inner").expect("inner kept");
    assert!(
        inner.1[1] > 8.0,
        "cull sample max=0 must not replace live inner, got {:?}",
        inner.1
    );
    assert!((cache.get("outer").unwrap().1[1] - 220.0).abs() < 0.1);
}

#[test]
fn omitted_inner_is_retained() {
    let mut cache = HashMap::new();
    cache.insert(
        "inner".into(),
        ([0.0, 100.0, 320.0, 384.0], [0.0, 240.0], [320.0, 624.0]),
    );
    merge_scroll_metrics_cache(
        &mut cache,
        [(
            "outer".into(),
            [0.0, 0.0, 400.0, 800.0],
            [0.0, 200.0],
            [400.0, 1000.0],
        )],
    );
    assert!(
        cache.get("inner").is_some_and(|e| e.1[1] > 8.0),
        "band-culled nested viewport must stay in the wheel cache"
    );
}

#[test]
fn stale_zero_max_detects_cull_sample() {
    let prev = ([0.0, 100.0, 320.0, 384.0], [0.0, 100.0], [320.0, 484.0]);
    assert!(is_stale_zero_max_sample(
        [0.0, 0.0],
        [0.0, 80.0, 320.0, 384.0],
        [320.0, 384.0],
        Some(&prev),
    ));
    assert!(!is_stale_zero_max_sample(
        [0.0, 80.0],
        [0.0, 80.0, 320.0, 384.0],
        [320.0, 464.0],
        Some(&prev),
    ));
    assert!(!is_stale_zero_max_sample(
        [0.0, 0.0],
        [0.0, 0.0, 320.0, 384.0],
        [320.0, 384.0],
        None,
    ));
}

#[test]
fn expand_viewport_until_fit_is_not_stale_zero() {
    // Scrolled sidebar: vh=300, content=500, max=200. Window grows to vh=600.
    let prev = ([0.0, 48.0, 208.0, 300.0], [0.0, 200.0], [208.0, 500.0]);
    assert!(
        !is_stale_zero_max_sample(
            [0.0, 0.0],
            [0.0, 48.0, 208.0, 600.0],
            [208.0, 500.0],
            Some(&prev),
        ),
        "grown viewport that covers previous content must clamp, not keep the short-screen max"
    );
}

#[test]
fn expand_viewport_collapsed_content_is_not_stale_zero() {
    // Maximize + scroll_only can publish content_h == new vh (looks like a cull).
    let prev = ([0.0, 48.0, 208.0, 300.0], [0.0, 200.0], [208.0, 500.0]);
    assert!(
        !is_stale_zero_max_sample(
            [0.0, 0.0],
            [0.0, 48.0, 208.0, 600.0],
            [208.0, 600.0],
            Some(&prev),
        ),
        "grown viewport whose previous content now fits must not keep short-screen overflow"
    );
}

#[test]
fn expand_viewport_until_fit_replaces_cache_and_clamps() {
    let mut cache = HashMap::new();
    cache.insert(
        "gal-clip__sidebar-content".into(),
        ([0.0, 48.0, 208.0, 300.0], [0.0, 200.0], [208.0, 500.0]),
    );
    merge_scroll_metrics_cache(
        &mut cache,
        [(
            "gal-clip__sidebar-content".into(),
            [0.0, 48.0, 208.0, 600.0],
            [0.0, 0.0],
            [208.0, 500.0],
        )],
    );
    let entry = cache
        .get("gal-clip__sidebar-content")
        .expect("sidebar viewport kept");
    assert!(
        entry.1[1] < 0.5,
        "expand-to-fit must publish max=0, got {:?}",
        entry.1
    );
    let (offset, target) = super::clamp_scroll_offset_and_target(180.0, 180.0, entry.1[1]);
    assert_eq!((offset, target), (0.0, 0.0));
}

#[test]
fn wheel_metrics_fall_back_to_signals() {
    let _ = create_root(|| {
        use_signal(&scroll_viewport_h_signal_key("ms-sig"), || 0.0_f32).set_silent(384.0);
        use_signal(&scroll_content_h_signal_key("ms-sig"), || 0.0_f32).set_silent(800.0);
        let cache = HashMap::new();
        let m = wheel_metrics_for_rest("ms-sig", &cache).expect("signal fallback");
        assert!(m.1[1] > 8.0, "got max_y={}", m.1[1]);
    });
}
