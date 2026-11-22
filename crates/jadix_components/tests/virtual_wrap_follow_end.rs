use jadix_components::{
    components::{VirtualGridHandle, virtual_wrap},
    core::{
        reactivity::{create_root, use_signal},
        scroll_viewport::ScrollViewportSignalKeys,
        tree::{Element, LayoutTree},
    },
    virtualization::ListScrollAlign,
};
use std::{cell::Cell, rc::Rc};

fn sync(root: &mut Element, layout: &mut LayoutTree) {
    if root
        .template_id
        .as_deref()
        .is_some_and(|id| id.ends_with("__grid_body"))
    {
        if let Some(callback) = root.reactive_sync.clone() {
            callback.run(root, layout);
        }
    }
    for child in &mut root.children {
        sync(child, layout);
    }
}

#[test]
fn wrap_follows_growth_reflow_and_resize_only_near_the_previous_end() {
    let (_owner, ()) = create_root(|| {
        let count = Rc::new(Cell::new(100_001));
        let extent = Rc::new(Cell::new(20.0));
        let handle = VirtualGridHandle::default();
        let mut root: Element = virtual_wrap("follow-wrap")
            .handle(handle.clone())
            .item_count_fn({
                let count = count.clone();
                move || count.get()
            })
            .item_height_fn({
                let extent = extent.clone();
                move || extent.get()
            })
            .minimum_item_width(80.0)
            .item_gap(0.0, 2.0)
            .initial_scroll_at_end()
            .follow_end_threshold(8.0)
            .follow_end_threshold(f32::NAN)
            .scroll_ahead(true)
            .max_mounted_cells(128)
            .render_item(|_| Element::new())
            .into();
        let keys = ScrollViewportSignalKeys::new("follow-wrap");
        let y = use_signal(&keys.offset_y, || 0.0_f32);
        let target = use_signal(&keys.target_y, || 0.0_f32);
        let width = use_signal(&keys.viewport_w, || 0.0_f32);
        let height = use_signal(&keys.viewport_h, || 0.0_f32);
        let mut layout = LayoutTree::new();
        let maximum = || {
            let v = handle.viewport().unwrap();
            (v.rows.total_extent - v.rows.viewport_extent).max(0.0)
        };
        height.set_silent(100.0);
        sync(&mut root, &mut layout);
        assert_eq!(y.peek(), 0.0, "wait for measured width");
        width.set_silent(180.0);
        sync(&mut root, &mut layout);
        assert_eq!(f64::from(y.peek()), maximum());
        assert_eq!(target.peek(), y.peek());
        count.set(100_003);
        sync(&mut root, &mut layout);
        assert_eq!(f64::from(y.peek()), maximum(), "append a partial row");
        y.set_silent(y.peek() - 8.0);
        sync(&mut root, &mut layout);
        let before = y.peek();
        sync(&mut root, &mut layout);
        assert_eq!(y.peek(), before, "idle near the end does not snap");
        extent.set(24.0);
        sync(&mut root, &mut layout);
        assert_eq!(
            f64::from(y.peek()),
            maximum(),
            "threshold includes boundary"
        );
        width.set_silent(90.0);
        sync(&mut root, &mut layout);
        assert_eq!(f64::from(y.peek()), maximum(), "column reflow follows");
        height.set_silent(140.0);
        sync(&mut root, &mut layout);
        assert_eq!(f64::from(y.peek()), maximum(), "viewport resize follows");
        y.set_silent(y.peek() - 9.0);
        sync(&mut root, &mut layout);
        let left = y.peek();
        count.set(100_005);
        sync(&mut root, &mut layout);
        assert_eq!(y.peek(), left, "user left the threshold");
        assert!(handle.scroll_to_end());
        sync(&mut root, &mut layout);
        count.set(100_007);
        assert!(handle.scroll_to_item(20, ListScrollAlign::Start, 0.0));
        sync(&mut root, &mut layout);
        assert_eq!(
            y.peek(),
            520.0,
            "explicit navigation wins over append follow"
        );
        extent.set(30.0);
        sync(&mut root, &mut layout);
        assert_eq!(y.peek(), 640.0, "away from end, preserve row anchor");
        assert!(handle.scroll_to_end());
        sync(&mut root, &mut layout);
        let before_error = y.peek();
        height.set_silent(10_000.0);
        sync(&mut root, &mut layout);
        assert!(handle.viewport().is_none());
        assert_eq!(
            y.peek(),
            before_error,
            "failed capacity must not publish a jump"
        );
        height.set_silent(140.0);
        count.set(100_009);
        sync(&mut root, &mut layout);
        assert_eq!(
            f64::from(y.peek()),
            maximum(),
            "follow recovers after capacity error"
        );
        extent.set(f32::NAN);
        sync(&mut root, &mut layout);
        assert!(handle.viewport().is_none());
        count.set(100_011);
        extent.set(30.0);
        sync(&mut root, &mut layout);
        assert_eq!(
            f64::from(y.peek()),
            maximum(),
            "follow recovers after invalid geometry"
        );
        count.set(7);
        sync(&mut root, &mut layout);
        assert_eq!(
            f64::from(y.peek()),
            maximum(),
            "shrinking dataset clamps to end"
        );
        count.set(0);
        sync(&mut root, &mut layout);
        assert_eq!(y.peek(), 0.0);
        count.set(15);
        sync(&mut root, &mut layout);
        assert_eq!(f64::from(y.peek()), maximum(), "growth from empty follows");
    });
}

#[test]
fn wrap_initial_end_waits_for_valid_nonempty_layout_and_can_be_overridden() {
    let (_owner, ()) = create_root(|| {
        for navigate in [false, true] {
            let id = format!("initial-wrap-{navigate}");
            let count = Rc::new(Cell::new(0));
            let extent = Rc::new(Cell::new(20.0));
            let handle = VirtualGridHandle::default();
            let mut root: Element = virtual_wrap(&id)
                .handle(handle.clone())
                .item_count_fn({
                    let count = count.clone();
                    move || count.get()
                })
                .item_height_fn({
                    let extent = extent.clone();
                    move || extent.get()
                })
                .initial_scroll_at_end()
                .follow_end_threshold(10.0)
                .follow_end_threshold(-1.0)
                .follow_end(false)
                .minimum_item_width(80.0)
                .max_mounted_cells(16)
                .render_item(|_| Element::new())
                .into();
            let keys = ScrollViewportSignalKeys::new(&id);
            let y = use_signal(&keys.offset_y, || 0.0_f32);
            let width = use_signal(&keys.viewport_w, || 0.0_f32);
            let height = use_signal(&keys.viewport_h, || 0.0_f32);
            let mut layout = LayoutTree::new();
            width.set_silent(180.0);
            height.set_silent(100.0);
            sync(&mut root, &mut layout);
            assert_eq!(y.peek(), 0.0, "empty must not consume initial jump");
            count.set(101);
            extent.set(f32::NAN);
            sync(&mut root, &mut layout);
            assert!(handle.viewport().is_none());
            extent.set(20.0);
            height.set_silent(1000.0);
            sync(&mut root, &mut layout);
            assert!(
                handle.viewport().is_none(),
                "capacity failure remains pending"
            );
            height.set_silent(100.0);
            if navigate {
                assert!(handle.scroll_to_item(10, ListScrollAlign::Start, 0.0));
            }
            sync(&mut root, &mut layout);
            let expected = if navigate { 100.0 } else { 920.0 };
            assert_eq!(y.peek(), expected);
            sync(&mut root, &mut layout);
            assert_eq!(y.peek(), expected, "initial jump settles exactly once");
            count.set(103);
            sync(&mut root, &mut layout);
            assert_eq!(
                y.peek(),
                expected,
                "explicit follow_end(false) retains pixels"
            );
            width.set_silent(0.0);
            sync(&mut root, &mut layout);
            width.set_silent(180.0);
            sync(&mut root, &mut layout);
            assert_ne!(
                y.peek(),
                940.0,
                "showing again does not repeat initial jump"
            );
        }
    });
}

#[test]
fn wrap_zero_slack_follow_uses_the_same_precision_as_scroll_signals() {
    let (_owner, ()) = create_root(|| {
        for (i, extent) in [0.1, 1.1, 20.1].into_iter().enumerate() {
            let id = format!("fractional-follow-{i}");
            let count = Rc::new(Cell::new(10_001));
            let handle = VirtualGridHandle::default();
            let mut root: Element = virtual_wrap(&id)
                .handle(handle.clone())
                .item_count_fn({
                    let count = count.clone();
                    move || count.get()
                })
                .item_height(extent)
                .initial_scroll_at_end()
                .follow_end(true)
                .render_item(|_| Element::new())
                .into();
            let keys = ScrollViewportSignalKeys::new(&id);
            let y = use_signal(&keys.offset_y, || 0.0_f32);
            use_signal(&keys.viewport_w, || 0.0_f32).set_silent(180.0);
            use_signal(&keys.viewport_h, || 0.0_f32).set_silent(10.0);
            let mut layout = LayoutTree::new();
            sync(&mut root, &mut layout);
            for next in [10_003, 10_005] {
                count.set(next);
                sync(&mut root, &mut layout);
                let v = handle.viewport().unwrap().rows;
                assert_eq!(
                    y.peek(),
                    (v.total_extent - v.viewport_extent).max(0.0) as f32,
                    "fractional extent {extent} at count {next}"
                );
            }
        }
    });
}
