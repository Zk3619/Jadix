use jadix_components::{
    components::virtual_list::{
        ListContentAlign, ListScrollAlign, VirtualListHandle, virtual_list,
    },
    core::{
        reactivity::{create_root, use_signal},
        scroll_viewport::ScrollViewportSignalKeys,
        tree::{Element, LayoutStatus, LayoutTree, find_in_tree},
    },
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

fn sync(el: &mut Element, layout: &mut LayoutTree) -> LayoutStatus {
    let mut status = el
        .reactive_sync
        .clone()
        .map_or(LayoutStatus::Clean, |f| f.run(el, layout));
    for child in &mut el.children {
        status = status.merge(sync(child, layout));
    }
    status
}

fn body(root: &Element) -> &Element {
    find_in_tree(root, |el| el.attrs.contains_key("__jadix_vlist_range")).unwrap()
}

fn row<'a>(root: &'a Element, id: &str, index: usize) -> &'a Element {
    let id = format!("{id}__row_{index}");
    find_in_tree(root, |el| el.template_id.as_deref() == Some(id.as_str())).unwrap()
}

#[test]
fn short_content_layout_resize_measurement_and_callbacks_agree_on_both_axes() {
    let (_owner, ()) = create_root(|| {
        for horizontal in [false, true] {
            for measured in [false, true] {
                for align in [
                    ListContentAlign::Start,
                    ListContentAlign::Center,
                    ListContentAlign::End,
                ] {
                    let id = format!("aligned-{horizontal}-{measured}-{align:?}");
                    let count = Rc::new(Cell::new(2));
                    let renders = Rc::new(Cell::new(0));
                    let positions = Rc::new(RefCell::new(Vec::new()));
                    let ranges = Rc::new(RefCell::new(Vec::new()));
                    let handle = VirtualListHandle::default();
                    assert_eq!(handle.row_offset(0), None);
                    let mut builder = virtual_list(&id)
                        .handle(handle.clone())
                        .item_count_fn({
                            let count = count.clone();
                            move || count.get()
                        })
                        .item_extent(20.0)
                        .item_gap(5.0)
                        .height(100.0)
                        .overscan(0)
                        .content_align(align)
                        .sticky_header_indices([0])
                        .sticky_header_offset(5.0)
                        .on_scroll_position({
                            let positions = positions.clone();
                            move |p| positions.borrow_mut().push(p)
                        })
                        .on_visible_range_change({
                            let ranges = ranges.clone();
                            move |r| ranges.borrow_mut().push(r)
                        })
                        .render_item({
                            let renders = renders.clone();
                            move |_| {
                                renders.set(renders.get() + 1);
                                let extent = if measured { 30.0 } else { 20.0 };
                                Element::new().w(extent).h(extent)
                            }
                        });
                    if horizontal {
                        builder = builder.horizontal();
                    }
                    if measured {
                        builder = builder.measure_rows();
                    }
                    let mut root: Element = builder.into();
                    let keys = ScrollViewportSignalKeys::new(&id);
                    let viewport = use_signal(
                        if horizontal {
                            &keys.viewport_w
                        } else {
                            &keys.viewport_h
                        },
                        || 100.0_f32,
                    );
                    viewport.set_silent(100.0);
                    let offset = use_signal(
                        if horizontal {
                            &keys.offset_x
                        } else {
                            &keys.offset_y
                        },
                        || 0.0_f32,
                    );
                    let mut layout = LayoutTree::new();
                    let natural = if measured { 65.0 } else { 45.0 };
                    let extent = if measured { 30.0 } else { 20.0 };
                    for size in [100.0, 140.0] {
                        viewport.set_silent(size);
                        root = root.h(size);
                        for _ in 0..3 {
                            sync(&mut root, &mut layout);
                            layout.build_and_layout(&mut root, size, size, [-1.0; 2]);
                        }
                        let [leading, trailing] = align.padding(natural, f64::from(size)).unwrap();
                        let total = natural + leading + trailing;
                        assert_eq!(handle.row_offset(0), Some(leading));
                        assert_eq!(handle.row_offset(1), Some(leading + extent + 5.0));
                        assert_eq!(handle.row_offset(2), Some(total));
                        assert_eq!(handle.row_offset(3), None);
                        let v = handle.viewport().unwrap();
                        assert_eq!((v.visible, v.offset, v.total_extent), (0..2, 0.0, total));
                        let dimension = if horizontal { 0 } else { 1 };
                        let pin = if align == ListContentAlign::Start {
                            5.0
                        } else {
                            0.0
                        };
                        assert_eq!(
                            row(&root, &id, 0).layout_bounds[dimension]
                                - body(&root).layout_bounds[dimension],
                            leading as f32 + pin
                        );
                        assert_eq!(
                            row(&root, &id, 1).layout_bounds[dimension]
                                - body(&root).layout_bounds[dimension],
                            (leading + extent + 5.0) as f32
                        );
                        assert_eq!(body(&root).layout_bounds[dimension + 2], total as f32);
                        assert_eq!(
                            handle.sticky_header().is_some(),
                            align == ListContentAlign::Start
                        );
                        assert_eq!(sync(&mut root, &mut layout), LayoutStatus::Clean);
                        assert_eq!(renders.get(), 2, "resize and alignment reuse row payloads");
                        assert_eq!(offset.peek(), 0.0, "alignment does not create scrolling");
                    }
                    assert_eq!(
                        positions.borrow().len(),
                        1,
                        "padding changes do not change the row anchor"
                    );
                    assert_eq!(positions.borrow()[0].unwrap().index, 0);
                    assert_eq!(positions.borrow()[0].unwrap().offset, 0.0);
                    assert_eq!(&*ranges.borrow(), &[Some(0..2)]);
                    assert!(handle.scroll_to_index(1, ListScrollAlign::End, 0.0));
                    sync(&mut root, &mut layout);
                    assert_eq!(offset.peek(), 0.0);
                    count.set(8);
                    sync(&mut root, &mut layout);
                    assert_eq!(
                        handle.row_offset(0),
                        Some(0.0),
                        "long content has no alignment padding"
                    );
                    assert!(handle.viewport().unwrap().total_extent > 140.0);
                    count.set(0);
                    for _ in 0..2 {
                        sync(&mut root, &mut layout);
                        layout.build_and_layout(&mut root, 140.0, 140.0, [-1.0; 2]);
                    }
                    let total = if align == ListContentAlign::Start {
                        0.0
                    } else {
                        140.0
                    };
                    assert_eq!(handle.row_offset(0), Some(total));
                    assert_eq!(handle.viewport().unwrap().visible, 0..0);
                    assert_eq!(handle.viewport().unwrap().total_extent, total);
                    assert!(handle.sticky_header().is_none());
                }
            }
        }
    });
}

#[test]
fn end_follow_threshold_respects_user_distance_measurements_and_explicit_navigation() {
    let (_owner, ()) = create_root(|| {
        for horizontal in [false, true] {
            for measured in [false, true] {
                for enabled in [false, true] {
                    let id = format!("near-end-{horizontal}-{measured}-{enabled}");
                    let count = Rc::new(Cell::new(2));
                    let handle = VirtualListHandle::default();
                    let mut builder = virtual_list(&id)
                        .handle(handle.clone())
                        .item_count_fn({
                            let count = count.clone();
                            move || count.get()
                        })
                        .item_extent(20.0)
                        .height(100.0)
                        .overscan(0)
                        .content_align(ListContentAlign::End)
                        .follow_end_threshold(10.0)
                        .follow_end_threshold(f32::NAN)
                        .follow_end_threshold(-1.0)
                        .follow_end_threshold(f32::INFINITY)
                        .follow_end(enabled)
                        .render_item(move |_| {
                            let size = if measured { 30.0 } else { 20.0 };
                            Element::new().w(size).h(size)
                        });
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
                    let viewport = use_signal(
                        if horizontal {
                            &keys.viewport_w
                        } else {
                            &keys.viewport_h
                        },
                        || 100.0_f32,
                    );
                    viewport.set_silent(100.0);
                    let mut layout = LayoutTree::new();
                    for _ in 0..3 {
                        sync(&mut root, &mut layout);
                        layout.build_and_layout(&mut root, 100.0, 100.0, [-1.0; 2]);
                    }
                    count.set(12);
                    for _ in 0..5 {
                        sync(&mut root, &mut layout);
                        layout.build_and_layout(&mut root, 100.0, 100.0, [-1.0; 2]);
                    }
                    let max = (handle.row_offset(count.get()).unwrap() - 100.0) as f32;
                    assert_eq!(
                        offset.peek(),
                        if enabled { max } else { 0.0 },
                        "short-to-long growth follows only when enabled"
                    );
                    for distance in [10.25, 10.0, 0.0] {
                        // First settle measurements at the old end, then emulate a user scroll.
                        offset.set_silent(max);
                        target.set_silent(max);
                        for _ in 0..4 {
                            sync(&mut root, &mut layout);
                            layout.build_and_layout(&mut root, 100.0, 100.0, [-1.0; 2]);
                        }
                        let old_max = (handle.row_offset(count.get()).unwrap() - 100.0) as f32;
                        offset.set_silent(old_max - distance);
                        target.set_silent(old_max - distance);
                        sync(&mut root, &mut layout);
                        let before = offset.peek();
                        count.set(count.get() + 1);
                        sync(&mut root, &mut layout);
                        let next_max = (handle.row_offset(count.get()).unwrap() - 100.0) as f32;
                        assert_eq!(
                            offset.peek(),
                            if enabled && distance <= 10.0 {
                                next_max
                            } else {
                                before
                            }
                        );
                        assert_eq!(target.peek(), offset.peek());
                    }
                    // Viewport contraction moves the end without changing the dataset.
                    for distance in [10.25, 10.0] {
                        viewport.set_silent(100.0);
                        sync(&mut root, &mut layout);
                        let old_max = (handle.row_offset(count.get()).unwrap() - 100.0) as f32;
                        offset.set_silent(old_max - distance);
                        target.set_silent(old_max - distance);
                        sync(&mut root, &mut layout);
                        viewport.set_silent(80.0);
                        sync(&mut root, &mut layout);
                        assert_eq!(
                            offset.peek(),
                            if enabled && distance <= 10.0 {
                                old_max + 20.0
                            } else {
                                old_max - distance
                            }
                        );
                        assert_eq!(target.peek(), offset.peek());
                    }
                    viewport.set_silent(100.0);
                    sync(&mut root, &mut layout);
                    // An explicit request in the same append batch overrides automatic follow.
                    offset.set_silent((handle.row_offset(count.get()).unwrap() - 100.0) as f32);
                    count.set(count.get() + 1);
                    assert!(handle.scroll_to_index(0, ListScrollAlign::Start, 0.0));
                    sync(&mut root, &mut layout);
                    assert_eq!(offset.peek(), 0.0);
                    assert_eq!(target.peek(), 0.0);
                    // Short-content alignment alone must not open a long list at the end.
                    let mut separate: Element = virtual_list(format!("{id}-initial"))
                        .item_count(20)
                        .item_extent(20.0)
                        .height(100.0)
                        .content_align(ListContentAlign::End)
                        .follow_end_threshold(10.0)
                        .render_item(|_| Element::new().h(20.0))
                        .into();
                    sync(&mut separate, &mut layout);
                    assert_eq!(
                        use_signal(
                            &ScrollViewportSignalKeys::new(&format!("{id}-initial")).offset_y,
                            || 0.0_f32
                        )
                        .peek(),
                        0.0
                    );
                }
            }
        }
    });
}
