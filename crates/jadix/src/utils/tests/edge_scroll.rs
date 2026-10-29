use super::*;

#[test]
fn edge_delta_top_and_bottom() {
    assert!(edge_delta(5.0, 0.0, 200.0, 40.0, 10.0) < 0.0);
    assert!(edge_delta(190.0, 0.0, 200.0, 40.0, 10.0) > 0.0);
    assert_eq!(edge_delta(100.0, 0.0, 200.0, 40.0, 10.0), 0.0);
}

#[test]
fn autoscroll_builder() {
    let cfg = AutoScroll::default()
        .enabled(false)
        .sensitivity(30.0)
        .speed(10.0)
        .lock(ScrollLock::None);
    assert!(!cfg.enabled);
    assert!((cfg.sensitivity - 30.0).abs() < 1e-4);
    assert!((cfg.speed - 10.0).abs() < 1e-4);
    assert_eq!(cfg.lock, ScrollLock::None);
}

#[test]
fn apply_scroll_lock_and_auto_axis() {
    use crate::core::reactivity::create_root;
    use crate::core::scroll_viewport::{
        scroll_content_h_signal_key, scroll_content_w_signal_key, scroll_offset_x_signal_key,
        scroll_offset_y_signal_key, scroll_viewport_h_signal_key, scroll_viewport_w_signal_key,
    };

    fn cfg(lock: ScrollLock) -> AutoScroll {
        AutoScroll {
            lock,
            ..AutoScroll::default()
        }
    }
    let views = [ScrollHit {
        rest: "page".into(),
        rect: Rect {
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 300.0,
        },
    }];

    let (_o, x_after_auto_y) = create_root(|| {
        use_signal(&scroll_offset_x_signal_key("page"), || 0.0_f32).set_silent(0.0);
        use_signal(&scroll_viewport_w_signal_key("page"), || 0.0_f32).set_silent(200.0);
        use_signal(&scroll_content_w_signal_key("page"), || 0.0_f32).set_silent(800.0);
        let _ = apply(
            [195.0, 100.0],
            &views,
            cfg(ScrollLock::Auto),
            1.0 / 60.0,
            Axis::Y,
        );
        use_signal(&scroll_offset_x_signal_key("page"), || 0.0_f32).peek()
    });
    assert!(
        x_after_auto_y.abs() < 0.5,
        "auto + axis=y must not pan X; got {x_after_auto_y}"
    );

    let (_o, y_after_auto_x) = create_root(|| {
        use_signal(&scroll_offset_y_signal_key("page"), || 0.0_f32).set_silent(0.0);
        use_signal(&scroll_viewport_h_signal_key("page"), || 0.0_f32).set_silent(300.0);
        use_signal(&scroll_content_h_signal_key("page"), || 0.0_f32).set_silent(800.0);
        let _ = apply(
            [100.0, 285.0],
            &views,
            cfg(ScrollLock::Auto),
            1.0 / 60.0,
            Axis::X,
        );
        use_signal(&scroll_offset_y_signal_key("page"), || 0.0_f32).peek()
    });
    assert!(
        y_after_auto_x.abs() < 0.5,
        "auto + axis=x must not pan Y; got {y_after_auto_x}"
    );

    let (_o, x_none_with_axis_y) = create_root(|| {
        use_signal(&scroll_offset_x_signal_key("page"), || 0.0_f32).set_silent(0.0);
        use_signal(&scroll_viewport_w_signal_key("page"), || 0.0_f32).set_silent(200.0);
        use_signal(&scroll_content_w_signal_key("page"), || 0.0_f32).set_silent(800.0);
        let _ = apply(
            [195.0, 100.0],
            &views,
            cfg(ScrollLock::None),
            1.0 / 60.0,
            Axis::Y,
        );
        use_signal(&scroll_offset_x_signal_key("page"), || 0.0_f32).peek()
    });
    assert!(
        x_none_with_axis_y > 1.0,
        "scroll_lock=none must pan X even when axis=y; got {x_none_with_axis_y}"
    );

    let (_o, x_all) = create_root(|| {
        use_signal(&scroll_offset_x_signal_key("page"), || 0.0_f32).set_silent(0.0);
        use_signal(&scroll_viewport_w_signal_key("page"), || 0.0_f32).set_silent(200.0);
        use_signal(&scroll_content_w_signal_key("page"), || 0.0_f32).set_silent(800.0);
        let _ = apply(
            [195.0, 100.0],
            &views,
            cfg(ScrollLock::All),
            1.0 / 60.0,
            Axis::Xy,
        );
        use_signal(&scroll_offset_x_signal_key("page"), || 0.0_f32).peek()
    });
    assert!(
        x_all.abs() < 0.5,
        "scroll_lock=all must freeze X even with axis=xy; got {x_all}"
    );

    let (_o, x_lock_x) = create_root(|| {
        use_signal(&scroll_offset_x_signal_key("page"), || 0.0_f32).set_silent(0.0);
        use_signal(&scroll_viewport_w_signal_key("page"), || 0.0_f32).set_silent(200.0);
        use_signal(&scroll_content_w_signal_key("page"), || 0.0_f32).set_silent(800.0);
        let _ = apply(
            [195.0, 100.0],
            &views,
            cfg(ScrollLock::X),
            1.0 / 60.0,
            Axis::Xy,
        );
        use_signal(&scroll_offset_x_signal_key("page"), || 0.0_f32).peek()
    });
    assert!(
        x_lock_x.abs() < 0.5,
        "scroll_lock=x must freeze X with axis=xy; got {x_lock_x}"
    );

    let (_o, y_lock_y) = create_root(|| {
        use_signal(&scroll_offset_y_signal_key("page"), || 0.0_f32).set_silent(0.0);
        use_signal(&scroll_viewport_h_signal_key("page"), || 0.0_f32).set_silent(300.0);
        use_signal(&scroll_content_h_signal_key("page"), || 0.0_f32).set_silent(800.0);
        let _ = apply(
            [100.0, 285.0],
            &views,
            cfg(ScrollLock::Y),
            1.0 / 60.0,
            Axis::Xy,
        );
        use_signal(&scroll_offset_y_signal_key("page"), || 0.0_f32).peek()
    });
    assert!(
        y_lock_y.abs() < 0.5,
        "scroll_lock=y must freeze Y with axis=xy; got {y_lock_y}"
    );

    assert_eq!(ScrollLock::parse("scroll-lock-all"), ScrollLock::All);
    assert_eq!(ScrollLock::parse("scroll-lock-x"), ScrollLock::X);
    assert_eq!(ScrollLock::parse("scroll_lock_y"), ScrollLock::Y);
    assert_eq!(ScrollLock::parse("scroll-lock-none"), ScrollLock::None);
    assert_eq!(ScrollLock::parse("x"), ScrollLock::X);
    assert_eq!(ScrollLock::parse("auto"), ScrollLock::Auto);
    assert_eq!(ScrollLock::parse("none"), ScrollLock::None);
    assert_eq!(ScrollLock::All.skip_axes(Axis::Xy), (true, true));
    assert_eq!(ScrollLock::X.skip_axes(Axis::Xy), (true, false));
    assert_eq!(ScrollLock::Y.skip_axes(Axis::Xy), (false, true));
    assert_eq!(ScrollLock::None.skip_axes(Axis::Y), (false, false));
    assert_eq!(ScrollLock::Auto.skip_axes(Axis::Y), (true, false));
    assert_eq!(ScrollLock::Auto.skip_axes(Axis::X), (false, true));
}
