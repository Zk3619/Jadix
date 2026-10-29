use super::*;

#[test]
fn box_rect_normalize() {
    let n = BoxRect::from_xywh(10.0, 10.0, -4.0, -6.0).normalized();
    assert!((n.x - 6.0).abs() < 1e-4);
    assert!((n.y - 4.0).abs() < 1e-4);
    assert!(n.width > 0.0 && n.height > 0.0);
}

#[test]
fn resize_autoscroll_pumps_scroll_viewport() {
    use crate::core::reactivity::create_root;
    use crate::core::scroll_viewport::scroll_offset_y_signal_key;
    use crate::core::tree::Element;
    use crate::utils::edge_scroll::ScrollLock;

    on_up();
    begin_resize(
        BoxRect::from_xywh(0.0, 0.0, 40.0, 20.0),
        [40.0, 10.0],
        ResizeOpts::default()
            .edges(Edges::right())
            .autoscroll_opts(AutoScroll::default().lock(ScrollLock::None)),
        None,
    );
    let (_owner, y) = create_root(|| {
        use crate::core::reactivity::use_signal;
        use crate::core::scroll_viewport::{
            scroll_content_h_signal_key, scroll_viewport_h_signal_key, SCROLL_VIEWPORT_FOCUS_PREFIX,
        };
        let mut vp = Element::new();
        vp.focus_id = Some(format!("{}{}", SCROLL_VIEWPORT_FOCUS_PREFIX, "resize"));
        vp.layout_bounds = [0.0, 0.0, 200.0, 120.0];
        use_signal(&scroll_offset_y_signal_key("resize"), || 0.0_f32).set_silent(0.0);
        use_signal(&scroll_viewport_h_signal_key("resize"), || 0.0_f32).set_silent(120.0);
        use_signal(&scroll_content_h_signal_key("resize"), || 0.0_f32).set_silent(800.0);
        tick_resize(40.0, 105.0);
        assert!(pump_edge_scroll(&vp), "resize near bottom edge must scroll");
        use_signal(&scroll_offset_y_signal_key("resize"), || 0.0_f32).peek()
    });
    assert!(y > 0.5, "expected Y scroll, got {y}");
    on_up();
}

#[test]
fn live_resize_writes_signal() {
    on_up();
    begin_resize(
        BoxRect::from_xywh(0.0, 0.0, 40.0, 20.0),
        [40.0, 10.0],
        ResizeOpts::default().edges(Edges::right()),
        Some("jadix_test_resize".into()),
    );
    let r = tick_resize(60.0, 10.0).unwrap();
    assert!((r.width - 60.0).abs() < 1e-3);
    let stored = crate::core::reactivity::use_signal("jadix_test_resize", || [0.0_f32; 4]).peek();
    assert!((stored[2] - 60.0).abs() < 1e-3);
    assert!(on_up());
    assert!(!resize_is_active());
}
