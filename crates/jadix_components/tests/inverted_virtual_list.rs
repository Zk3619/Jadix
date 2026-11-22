use jadix_components::{
    components::virtual_list::{ListScrollAlign, VirtualListHandle, virtual_list},
    core::{
        reactivity::{create_root, use_signal},
        scroll_viewport::ScrollViewportSignalKeys,
        tree::{Element, LayoutTree, find_in_tree},
    },
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

fn settle(root: &mut Element, layout: &mut LayoutTree, size: f32) {
    for _ in 0..4 {
        sync(root, layout);
        layout.build_and_layout(root, size, size, [-1.0; 2]);
    }
}

fn body(root: &Element) -> &Element {
    find_in_tree(root, |el| el.attrs.contains_key("__jadix_vlist_range")).unwrap()
}

fn payload(root: &Element, index: usize) -> &Element {
    let value = index.to_string();
    find_in_tree(root, |el| el.attrs.get("logical-index") == Some(&value)).unwrap()
}

#[test]
fn reversed_short_layout_measurements_and_public_geometry_agree_on_both_axes() {
    let (_owner, ()) = create_root(|| {
        for horizontal in [false, true] {
            for measured in [false, true] {
                let id = format!("inverted-short-{horizontal}-{measured}");
                let count = Rc::new(Cell::new(2));
                let positions = Rc::new(RefCell::new(Vec::new()));
                let handle = VirtualListHandle::default();
                let mut builder = virtual_list(&id)
                    .handle(handle.clone())
                    .inverted()
                    .item_count_fn({
                        let count = count.clone();
                        move || count.get()
                    })
                    .item_extent(20.0)
                    .item_gap(5.0)
                    .height(100.0)
                    .overscan(0)
                    .sticky_header_indices([0, 1])
                    .on_scroll_position({
                        let positions = positions.clone();
                        move |p| positions.borrow_mut().push(p)
                    })
                    .render_item(move |index| {
                        let extent = if measured {
                            20.0 + index as f32 * 10.0
                        } else {
                            20.0
                        };
                        let mut el = Element::new().w(extent).h(extent);
                        el.attrs.insert("logical-index".into(), index.to_string());
                        el
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
                let mut layout = LayoutTree::new();
                for size in [100.0, 140.0] {
                    viewport.set_silent(size);
                    root = root.h(size);
                    settle(&mut root, &mut layout, size);
                    let leading = size - if measured { 55.0 } else { 45.0 };
                    assert_eq!(handle.row_offset(0), Some(f64::from(size - 20.0)));
                    assert_eq!(handle.row_offset(1), Some(f64::from(leading)));
                    assert_eq!(handle.row_offset(2), Some(f64::from(size)));
                    assert_eq!(handle.row_offset(3), None);
                    let v = handle.viewport().unwrap();
                    assert_eq!(
                        (v.visible, v.offset, v.total_extent),
                        (0..2, 0.0, f64::from(size))
                    );
                    assert!(handle.sticky_header().is_none());
                    let dimension = usize::from(!horizontal);
                    for index in 0..2 {
                        assert_eq!(
                            payload(&root, index).layout_bounds[dimension]
                                - body(&root).layout_bounds[dimension],
                            handle.row_offset(index).unwrap() as f32
                        );
                    }
                    assert_eq!(body(&root).layout_bounds[dimension + 2], size);
                }
                assert_eq!(positions.borrow().len(), 1);
                assert_eq!(positions.borrow()[0].unwrap().index, 1);
                assert_eq!(positions.borrow()[0].unwrap().offset, 0.0);
                count.set(0);
                settle(&mut root, &mut layout, 140.0);
                assert_eq!(handle.row_offset(0), Some(140.0));
                assert_eq!(handle.viewport().unwrap().visible, 0..0);
                assert_eq!(*positions.borrow().last().unwrap(), None);
            }
        }
    });
}

#[test]
fn inverted_mutations_preserve_reading_rows_and_navigation_has_priority() {
    let (_owner, ()) = create_root(|| {
        for horizontal in [false, true] {
            for measured in [false, true] {
                for keyed in [false, true] {
                    let id = format!("inverted-edits-{horizontal}-{measured}-{keyed}");
                    let data = Rc::new(RefCell::new((0..100).collect::<Vec<usize>>()));
                    let revision = Rc::new(Cell::new(0));
                    let positions = Rc::new(RefCell::new(Vec::new()));
                    let handle = VirtualListHandle::default();
                    let mut builder = virtual_list(&id)
                        .handle(handle.clone())
                        .inverted()
                        .item_count_fn({
                            let data = data.clone();
                            move || data.borrow().len()
                        })
                        .data_key_fn({
                            let revision = revision.clone();
                            move || revision.get().to_string()
                        })
                        .item_extent(20.0)
                        .height(100.0)
                        .overscan(0)
                        .on_scroll_position({
                            let positions = positions.clone();
                            move |p| positions.borrow_mut().push(p)
                        })
                        .render_item(|_| Element::new().w(20.0).h(20.0));
                    if horizontal {
                        builder = builder.horizontal();
                    }
                    if measured {
                        builder = builder.measure_rows();
                    }
                    if keyed {
                        builder = builder.item_key_fn({
                            let data = data.clone();
                            move |i| data.borrow()[i].to_string()
                        });
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
                        || 100.0_f32,
                    )
                    .set_silent(100.0);
                    let mut layout = LayoutTree::new();
                    settle(&mut root, &mut layout, 100.0);
                    assert_eq!(offset.peek(), 1900.0, "inversion opens at the visual end");
                    assert_eq!(handle.viewport().unwrap().visible, 0..5);
                    offset.set_silent(707.25);
                    target.set_silent(707.25);
                    sync(&mut root, &mut layout);
                    assert_eq!(positions.borrow().last().unwrap().unwrap().index, 64);
                    assert_eq!(positions.borrow().last().unwrap().unwrap().offset, 7.25);
                    data.borrow_mut().extend([100, 101]);
                    sync(&mut root, &mut layout);
                    assert_eq!(
                        offset.peek(),
                        747.25,
                        "logical append adds rows before the visual viewport"
                    );
                    assert_eq!(target.peek(), offset.peek());
                    if keyed {
                        data.borrow_mut().splice(0..0, [200, 201]);
                        revision.set(revision.get() + 1);
                        sync(&mut root, &mut layout);
                        assert_eq!(
                            offset.peek(),
                            747.25,
                            "inserting at logical start retains the visible key"
                        );
                        data.borrow_mut().remove(66);
                        revision.set(revision.get() + 1);
                        sync(&mut root, &mut layout);
                        assert_eq!(
                            offset.peek(),
                            740.0,
                            "deleted anchor falls to its visual successor"
                        );
                    }
                    // A jump maps the logical row only after the new count is known.
                    data.borrow_mut().push(300);
                    assert!(handle.scroll_to_index(50, ListScrollAlign::Center, 3.0));
                    sync(&mut root, &mut layout);
                    let at = handle.row_offset(50).unwrap() as f32;
                    assert_eq!(offset.peek(), at - 40.0 - 3.0);
                    assert_eq!(target.peek(), offset.peek());
                    assert!(handle.scroll_to_index(usize::MAX, ListScrollAlign::Start, 0.0));
                    let before = offset.peek();
                    sync(&mut root, &mut layout);
                    assert_eq!(
                        offset.peek(),
                        before,
                        "missing logical rows cancel without jumping"
                    );
                    assert!(handle.scroll_to_end());
                    sync(&mut root, &mut layout);
                    data.borrow_mut().push(301);
                    settle(&mut root, &mut layout, 100.0);
                    assert_eq!(
                        offset.peek(),
                        handle.row_offset(data.borrow().len()).unwrap() as f32 - 100.0
                    );
                    assert_eq!(handle.viewport().unwrap().visible, 0..5);
                }
            }
        }
    });
}

#[test]
fn inverted_large_list_focus_and_loading_edges_use_logical_indices_with_bounded_mounts() {
    let (_owner, ()) = create_root(|| {
        for horizontal in [false, true] {
            let id = format!("inverted-large-{horizontal}");
            let focus = Rc::new(Cell::new(None));
            let renders = Rc::new(Cell::new(0));
            let start_loads = Rc::new(Cell::new(0));
            let end_loads = Rc::new(Cell::new(0));
            let handle = VirtualListHandle::default();
            let mut builder = virtual_list(&id)
                .inverted()
                .handle(handle.clone())
                .item_count(1_000_000)
                .item_extent(20.0)
                .height(100.0)
                .overscan(0)
                .focused_index_fn({
                    let focus = focus.clone();
                    move || focus.get()
                })
                .on_load_more_at_start({
                    let loads = start_loads.clone();
                    move || loads.set(loads.get() + 1)
                })
                .on_load_more({
                    let loads = end_loads.clone();
                    move || loads.set(loads.get() + 1)
                })
                .render_item({
                    let renders = renders.clone();
                    move |index| {
                        renders.set(renders.get() + 1);
                        let mut row = Element::new().w(20.0).h(20.0);
                        row.attrs.insert("logical-index".into(), index.to_string());
                        row
                    }
                });
            if horizontal {
                builder = builder.horizontal();
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
            use_signal(
                if horizontal {
                    &keys.viewport_w
                } else {
                    &keys.viewport_h
                },
                || 100.0_f32,
            )
            .set_silent(100.0);
            let mut layout = LayoutTree::new();
            settle(&mut root, &mut layout, 100.0);
            assert_eq!(offset.peek(), 19_999_900.0);
            assert_eq!((start_loads.get(), end_loads.get()), (1, 0));
            assert!(body(&root).children.len() <= 8);
            assert_eq!(
                renders.get(),
                5,
                "fixed-size inversion must not walk the dataset"
            );
            assert!(handle.scroll_to_index(999_999, ListScrollAlign::Start, 0.0));
            sync(&mut root, &mut layout);
            assert_eq!(offset.peek(), 0.0);
            assert_eq!(handle.viewport().unwrap().visible, 999_995..1_000_000);
            assert_eq!((start_loads.get(), end_loads.get()), (1, 1));
            sync(&mut root, &mut layout);
            assert_eq!(
                (start_loads.get(), end_loads.get()),
                (1, 1),
                "edge callbacks are deduplicated"
            );
            focus.set(Some(123));
            sync(&mut root, &mut layout);
            assert!(handle.viewport().unwrap().visible.contains(&123));
            assert!(payload(&root, 123).attrs.contains_key("logical-index"));
            assert!(
                body(&root).children.len() <= 10,
                "far focus must not expand the mounted span"
            );
            assert!(renders.get() < 25);
        }
    });
}
