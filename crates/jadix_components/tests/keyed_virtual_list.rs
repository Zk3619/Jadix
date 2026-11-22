use jadix_components::{
    components::virtual_list::{VirtualListHandle, virtual_list},
    core::{
        reactivity::{create_root, use_signal},
        scroll_viewport::ScrollViewportSignalKeys,
        tree::{Element, LayoutTree},
    },
    virtualization::ListScrollAlign,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

fn sync(el: &mut Element, layout: &mut LayoutTree) {
    if let Some(f) = el.reactive_sync.clone() {
        f.run(el, layout);
    }
    for child in &mut el.children {
        sync(child, layout);
    }
}

#[test]
fn keyed_mutations_preserve_reading_position_and_navigation_priority_on_both_axes() {
    let (_owner, ()) = create_root(|| {
        for horizontal in [false, true] {
            for measured in [false, true] {
                for maintain in [false, true] {
                    let id = format!("key-anchor-{horizontal}-{measured}-{maintain}");
                    let data = Rc::new(RefCell::new((0..100).collect::<Vec<usize>>()));
                    let revision = Rc::new(Cell::new(0));
                    let reads = Rc::new(Cell::new(0));
                    let extent = Rc::new(Cell::new(20.0_f32));
                    let handle = VirtualListHandle::default();
                    let mut builder = virtual_list(&id)
                        .handle(handle.clone())
                        .item_count_fn({
                            let data = data.clone();
                            move || data.borrow().len()
                        })
                        .data_key_fn({
                            let revision = revision.clone();
                            move || revision.get().to_string()
                        })
                        .item_key_fn({
                            let data = data.clone();
                            let reads = reads.clone();
                            move |i| {
                                reads.set(reads.get() + 1);
                                data.borrow()[i].to_string()
                            }
                        })
                        .maintain_visible_content_position(maintain)
                        .follow_end(true)
                        .item_extent(20.0)
                        .item_extent_fn({
                            let extent = extent.clone();
                            move || extent.get()
                        })
                        .item_gap(5.0)
                        .overscan(0)
                        .height(50.0)
                        .render_item(|_| Element::new().w(20.0).h(20.0));
                    if horizontal {
                        builder = builder.horizontal();
                    }
                    if measured {
                        builder = builder.measure_rows();
                    }
                    let mut root: Element = builder.into();
                    let keys = ScrollViewportSignalKeys::new(&id);
                    let offset = use_signal(
                        if horizontal {
                            &keys.offset_x
                        } else {
                            &keys.offset_y
                        },
                        || 0.0_f32,
                    );
                    let target = use_signal(
                        if horizontal {
                            &keys.target_x
                        } else {
                            &keys.target_y
                        },
                        || 0.0_f32,
                    );
                    use_signal(
                        if horizontal {
                            &keys.viewport_w
                        } else {
                            &keys.viewport_h
                        },
                        || 50.0_f32,
                    )
                    .set_silent(50.0);
                    let mut layout = LayoutTree::new();
                    offset.set_silent(257.25);
                    target.set_silent(257.25);
                    for _ in 0..3 {
                        sync(&mut root, &mut layout);
                        layout.build_and_layout(&mut root, 50.0, 50.0, [-1.0; 2]);
                    }
                    let initial_reads = reads.get();
                    for _ in 0..5 {
                        sync(&mut root, &mut layout);
                    }
                    assert_eq!(
                        reads.get(),
                        initial_reads,
                        "no key scan during unchanged frames"
                    );

                    data.borrow_mut().splice(0..0, [100, 101]);
                    sync(&mut root, &mut layout);
                    assert_eq!(offset.peek(), if maintain { 307.25 } else { 257.25 });
                    assert_eq!(target.peek(), offset.peek());

                    // Same-count reorder must follow the key through its revision.
                    data.borrow_mut().swap(12, 20);
                    revision.set(1);
                    sync(&mut root, &mut layout);
                    assert_eq!(offset.peek(), if maintain { 507.25 } else { 257.25 });
                    if maintain {
                        // Key 10 was moved to 20. Its next old row is key 19.
                        data.borrow_mut().remove(20);
                        sync(&mut root, &mut layout);
                        assert_eq!(offset.peek(), 500.0);
                        // Remove every old row at/after the anchor: nearest predecessor wins.
                        data.borrow_mut().truncate(20);
                        sync(&mut root, &mut layout);
                        assert_eq!(offset.peek(), 445.0, "predecessor clamped to viewport end");
                    }
                    // Full replacement retains a bounded pixel offset.
                    offset.set_silent(107.5);
                    sync(&mut root, &mut layout);
                    let before = offset.peek();
                    *data.borrow_mut() = (1000..1100).collect();
                    revision.set(2);
                    extent.set(10.0);
                    sync(&mut root, &mut layout);
                    assert_eq!(
                        offset.peek(),
                        before,
                        "full replacement and estimate change coalesce"
                    );
                    extent.set(20.0);
                    sync(&mut root, &mut layout);

                    assert!(handle.scroll_to_end());
                    sync(&mut root, &mut layout);
                    data.borrow_mut().push(1100);
                    sync(&mut root, &mut layout);
                    assert_eq!(
                        offset.peek(),
                        2470.0,
                        "following the end beats keyed anchoring"
                    );
                    offset.set_silent(107.5);
                    sync(&mut root, &mut layout);
                    data.borrow_mut().insert(0, 999);
                    assert!(handle.scroll_to_index(30, ListScrollAlign::Start, 0.0));
                    sync(&mut root, &mut layout);
                    assert_eq!(offset.peek(), 750.0, "explicit navigation beats anchoring");
                    data.borrow_mut().clear();
                    sync(&mut root, &mut layout);
                    assert_eq!(offset.peek(), 0.0);
                    assert_eq!(handle.viewport().unwrap().visible, 0..0);
                }
            }
        }
    });
}

#[test]
fn measured_keyed_anchor_tracks_new_row_estimates_then_actual_extents() {
    let (_owner, ()) = create_root(|| {
        let data = Rc::new(RefCell::new((0..20).collect::<Vec<usize>>()));
        let handle = VirtualListHandle::default();
        let mut root: Element = virtual_list("measured-key-anchor")
            .handle(handle.clone())
            .item_count_fn({
                let data = data.clone();
                move || data.borrow().len()
            })
            .item_key_fn({
                let data = data.clone();
                move |i| data.borrow()[i].to_string()
            })
            .item_extent(20.0)
            .item_gap(5.0)
            .height(50.0)
            .overscan(100)
            .measure_rows()
            .render_item({
                let data = data.clone();
                move |i| {
                    Element::new()
                        .w(30.0)
                        .h(20.0 + (data.borrow()[i] % 3) as f32 * 10.0)
                }
            })
            .into();
        let keys = ScrollViewportSignalKeys::new("measured-key-anchor");
        let offset = use_signal(&keys.offset_y, || 0.0_f32);
        use_signal(&keys.viewport_h, || 50.0_f32).set_silent(50.0);
        let mut layout = LayoutTree::new();
        for _ in 0..3 {
            layout.build_and_layout(&mut root, 50.0, 50.0, [-1.0; 2]);
            sync(&mut root, &mut layout);
        }
        assert_eq!(handle.row_offset(10), Some(340.0));
        offset.set_silent(347.25);
        sync(&mut root, &mut layout);
        data.borrow_mut().splice(0..0, [100, 101]);
        sync(&mut root, &mut layout);
        assert_eq!(offset.peek(), 397.25, "new keys initially use estimates");
        for _ in 0..3 {
            layout.build_and_layout(&mut root, 50.0, 50.0, [-1.0; 2]);
            sync(&mut root, &mut layout);
        }
        assert_eq!(
            offset.peek(),
            427.25,
            "newly measured prefix preserves key and depth"
        );
        assert_eq!(handle.row_offset(12), Some(420.0));
        data.borrow_mut().remove(12);
        sync(&mut root, &mut layout);
        assert_eq!(
            offset.peek(),
            420.0,
            "deleted key selects its successor at zero depth"
        );
    });
}
