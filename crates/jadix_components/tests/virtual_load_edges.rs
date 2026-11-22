use jadix_components::{
    components::{
        VirtualGridHandle,
        virtual_list::{VirtualListHandle, virtual_list},
        virtual_wrap,
    },
    core::{
        reactivity::{create_root, use_signal},
        scroll_viewport::ScrollViewportSignalKeys,
        tree::{Element, LayoutTree},
    },
};
use std::{cell::Cell, rc::Rc};

fn sync(root: &mut Element, layout: &mut LayoutTree) {
    // Drive only the component adapter, as the host does after viewport metrics
    // are published. Do not let synthetic, unlaid-out scroll chrome replace them.
    if root.attrs.contains_key("__jadix_vlist_range")
        || root
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
fn wrap_loading_uses_actual_edges_and_recovers_from_empty_reflow_and_invalid_geometry() {
    let (_owner, ()) = create_root(|| {
        let count = Rc::new(Cell::new(100_001));
        let revision = Rc::new(Cell::new(0));
        let extent = Rc::new(Cell::new(20.0));
        let start = Rc::new(Cell::new(0));
        let end = Rc::new(Cell::new(0));
        let renders = Rc::new(Cell::new(0));
        let handle = VirtualGridHandle::default();
        let observed = use_signal("wrap-load-callback-signal", || 0);
        let mut root: Element = virtual_wrap("load-wrap")
            .handle(handle.clone())
            .item_count_fn({
                let count = count.clone();
                move || count.get()
            })
            .data_key_fn({
                let revision = revision.clone();
                move || revision.get().to_string()
            })
            .item_height_fn({
                let extent = extent.clone();
                move || extent.get()
            })
            .minimum_item_width(80.0)
            .overscan(50)
            .scroll_ahead(true)
            .max_mounted_cells(512)
            .load_more_at_start_threshold(0.25)
            .load_more_threshold(0.5)
            .load_more_at_start_threshold(f64::NAN)
            .load_more_threshold(-1.0)
            .render_item({
                let renders = renders.clone();
                move |_| {
                    renders.set(renders.get() + 1);
                    Element::new()
                }
            })
            .on_load_more_at_start({
                let start = start.clone();
                let handle = handle.clone();
                move || {
                    assert!(handle.viewport().is_some());
                    start.set(start.get() + 1);
                    observed.set(observed.peek() + 1);
                }
            })
            .on_load_more({
                let end = end.clone();
                let handle = handle.clone();
                move || {
                    assert!(handle.viewport().is_some());
                    end.set(end.get() + 1);
                    observed.set(observed.peek() + 1);
                }
            })
            .into();
        let keys = ScrollViewportSignalKeys::new("load-wrap");
        let y = use_signal(&keys.offset_y, || 0.0_f32);
        let width = use_signal(&keys.viewport_w, || 0.0_f32);
        let height = use_signal(&keys.viewport_h, || 0.0_f32);
        let mut layout = LayoutTree::new();
        height.set_silent(100.0);
        sync(&mut root, &mut layout);
        assert_eq!(
            (start.get(), end.get()),
            (0, 0),
            "width is not measured yet"
        );
        width.set_silent(180.0);
        sync(&mut root, &mut layout);
        assert_eq!((start.get(), end.get()), (1, 0));
        start.set(0);
        y.set_silent(26.0);
        sync(&mut root, &mut layout);
        assert_eq!(
            start.get(),
            0,
            "overscan reaching row zero is not an approach"
        );
        y.set_silent(25.0);
        sync(&mut root, &mut layout);
        for _ in 0..5 {
            sync(&mut root, &mut layout);
        }
        assert_eq!((start.get(), end.get()), (1, 0));
        assert!(handle.scroll_to_end());
        sync(&mut root, &mut layout);
        sync(&mut root, &mut layout);
        assert_eq!(end.get(), 1);
        let mounted_renders = renders.get();
        for _ in 0..5 {
            sync(&mut root, &mut layout);
        }
        assert_eq!(
            renders.get(),
            mounted_renders,
            "idle does not rebuild viewport content"
        );
        revision.set(1);
        sync(&mut root, &mut layout);
        sync(&mut root, &mut layout);
        assert_eq!(end.get(), 2);
        count.set(100_003);
        sync(&mut root, &mut layout);
        sync(&mut root, &mut layout);
        assert_eq!(
            end.get(),
            3,
            "a short page within the threshold requests again"
        );
        y.set_silent(1000.0);
        sync(&mut root, &mut layout);
        assert!(handle.scroll_to_end());
        sync(&mut root, &mut layout);
        assert_eq!(end.get(), 4);
        extent.set(f32::NAN);
        sync(&mut root, &mut layout);
        assert!(handle.viewport().is_none());
        assert_eq!(end.get(), 4);
        extent.set(20.0);
        sync(&mut root, &mut layout);
        assert_eq!(end.get(), 5);
        width.set_silent(0.0);
        sync(&mut root, &mut layout);
        width.set_silent(180.0);
        assert!(handle.scroll_to_end());
        sync(&mut root, &mut layout);
        assert_eq!(end.get(), 6, "unmeasured viewport rearms the approach");
        height.set_silent(10_000.0);
        sync(&mut root, &mut layout);
        assert!(
            handle.viewport().is_none(),
            "capacity error does not issue loads"
        );
        assert_eq!(end.get(), 6);
        height.set_silent(100.0);
        assert!(handle.scroll_to_end());
        sync(&mut root, &mut layout);
        assert_eq!(end.get(), 7);
        count.set(0);
        sync(&mut root, &mut layout);
        sync(&mut root, &mut layout);
        assert_eq!(
            (start.get(), end.get()),
            (1, 8),
            "empty data requests the first page only"
        );
        count.set(7);
        sync(&mut root, &mut layout);
        assert_eq!((start.get(), end.get()), (2, 9));
        width.set_silent(90.0);
        sync(&mut root, &mut layout);
        sync(&mut root, &mut layout);
        assert_eq!(
            (start.get(), end.get()),
            (2, 9),
            "reflow within both thresholds is not new data"
        );
        assert_eq!(observed.peek(), 12);
    });
}

#[test]
fn list_start_threshold_is_independent_on_both_axes_and_inverted_orders() {
    let (_owner, ()) = create_root(|| {
        for horizontal in [false, true] {
            for measured in [false, true] {
                for inverted in [false, true] {
                    let id = format!("load-list-{horizontal}-{measured}-{inverted}");
                    let start = Rc::new(Cell::new(0));
                    let end = Rc::new(Cell::new(0));
                    let revision = Rc::new(Cell::new(0));
                    let handle = VirtualListHandle::default();
                    let mut builder = virtual_list(&id)
                        .handle(handle.clone())
                        .item_count(100)
                        .item_height(20.0)
                        .height(100.0)
                        .overscan(20)
                        .scroll_ahead(true)
                        .load_more_at_start_threshold(0.25)
                        .load_more_threshold(0.5)
                        .load_more_at_start_threshold(f64::INFINITY)
                        .load_more_threshold(f64::NAN)
                        .data_key_fn({
                            let revision = revision.clone();
                            move || revision.get().to_string()
                        })
                        .on_load_more_at_start({
                            let start = start.clone();
                            move || start.set(start.get() + 1)
                        })
                        .on_load_more({
                            let end = end.clone();
                            let handle = handle.clone();
                            move || {
                                assert!(handle.viewport().is_some());
                                end.set(end.get() + 1);
                            }
                        });
                    if horizontal {
                        builder = builder.horizontal();
                    }
                    if measured {
                        builder = builder.measure_rows();
                    }
                    if inverted {
                        builder = builder.inverted();
                    }
                    let mut root: Element = builder.into();
                    let keys = ScrollViewportSignalKeys::new(&id);
                    let x = use_signal(&keys.offset_x, || 0.0_f32);
                    let y = use_signal(&keys.offset_y, || 0.0_f32);
                    use_signal(&keys.viewport_w, || 0.0_f32).set_silent(100.0);
                    use_signal(&keys.viewport_h, || 0.0_f32).set_silent(100.0);
                    let offset = if horizontal { x } else { y };
                    let mut layout = LayoutTree::new();
                    sync(&mut root, &mut layout);
                    assert!(handle.viewport().is_some(), "{id}");
                    start.set(0);
                    end.set(0);
                    let physical = |from_start| {
                        if inverted {
                            1900.0 - from_start
                        } else {
                            from_start
                        }
                    };
                    offset.set_silent(physical(26.0));
                    sync(&mut root, &mut layout);
                    assert_eq!(start.get(), 0, "{id}: independent start threshold");
                    offset.set_silent(physical(25.0));
                    sync(&mut root, &mut layout);
                    sync(&mut root, &mut layout);
                    assert_eq!(start.get(), 1, "{id}");
                    offset.set_silent(physical(1849.0));
                    sync(&mut root, &mut layout);
                    assert_eq!(end.get(), 0, "{id}: one pixel beyond end threshold");
                    offset.set_silent(physical(1850.0));
                    sync(&mut root, &mut layout);
                    sync(&mut root, &mut layout);
                    assert_eq!(end.get(), 1, "{id}");
                    revision.set(1);
                    sync(&mut root, &mut layout);
                    sync(&mut root, &mut layout);
                    assert_eq!(end.get(), 2, "{id}: revision rearms exactly once");
                }
            }
        }
    });
}
