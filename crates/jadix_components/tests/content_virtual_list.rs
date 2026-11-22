use jadix_components::{
    components::virtual_list::{
        ListContentAlign, ListScrollAlign, VirtualListHandle, virtual_list,
    },
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

fn settle(root: &mut Element, layout: &mut LayoutTree) {
    for _ in 0..4 {
        sync(root, layout);
        layout.build_and_layout(root, 100.0, 100.0, [-1.0; 2]);
    }
}

fn named<'a>(root: &'a Element, name: &str) -> Option<&'a Element> {
    find_in_tree(root, |el| el.template_id.as_deref() == Some(name))
}

fn identity(el: &Element) -> usize {
    el.inner.as_ref() as *const _ as usize
}

#[test]
fn scrolling_slots_share_geometry_navigation_and_identity_on_both_axes_and_orders() {
    let (_owner, ()) = create_root(|| {
        for horizontal in [false, true] {
            for measured in [false, true] {
                for inverted in [false, true] {
                    let id = format!("content-{horizontal}-{measured}-{inverted}");
                    let count = Rc::new(Cell::new(if measured { 1000 } else { 1_000_000 }));
                    let renders = Rc::new(Cell::new(0));
                    let handle = VirtualListHandle::default();
                    let mut builder = virtual_list(&id)
                        .handle(handle.clone())
                        .item_count_fn({
                            let n = count.clone();
                            move || n.get()
                        })
                        .item_extent(20.0)
                        .item_gap(5.0)
                        .height(100.0)
                        .overscan(0)
                        .follow_end(false)
                        .header(120.0, Element::new().template_id("header"))
                        .footer(130.0, Element::new().template_id("footer"))
                        .empty(30.0, Element::new().template_id("empty"))
                        .render_item({
                            let renders = renders.clone();
                            move |i| {
                                renders.set(renders.get() + 1);
                                Element::new()
                                    .w(20.0)
                                    .h(20.0)
                                    .template_id(format!("payload-{i}"))
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
                    let offset = use_signal(
                        if horizontal {
                            &keys.offset_x
                        } else {
                            &keys.offset_y
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
                    settle(&mut root, &mut layout);
                    let head = identity(named(&root, "header").unwrap());
                    let foot = identity(named(&root, "footer").unwrap());
                    assert!(named(&root, "empty").is_none());
                    let total = count.get() as f64 * 25.0 - 5.0 + 250.0;
                    let leading = if inverted { 130.0 } else { 120.0 };
                    let logical = |visual| {
                        if inverted {
                            count.get() - 1 - visual
                        } else {
                            visual
                        }
                    };
                    assert_eq!(handle.row_offset(logical(0)), Some(leading));
                    assert_eq!(handle.row_offset(count.get()), Some(total));
                    assert_eq!(handle.viewport().unwrap().total_extent, total);
                    // Both end slots are larger than a viewport: neither can fabricate a row.
                    if inverted {
                        assert!(handle.scroll_to_index(
                            logical(0),
                            ListScrollAlign::Start,
                            leading
                        ));
                    }
                    offset.set_silent(0.0);
                    sync(&mut root, &mut layout);
                    assert!(handle.viewport().unwrap().visible.is_empty());
                    for align in [
                        ListScrollAlign::Start,
                        ListScrollAlign::Center,
                        ListScrollAlign::End,
                    ] {
                        let index = logical(10);
                        assert!(handle.scroll_to_index(index, align, 0.0));
                        settle(&mut root, &mut layout);
                        let expected = leading + 250.0
                            - match align {
                                ListScrollAlign::Start => 0.0,
                                ListScrollAlign::Center => 40.0,
                                _ => 80.0,
                            };
                        assert_eq!(offset.peek(), expected as f32);
                        assert!(handle.viewport().unwrap().visible.contains(&index));
                    }
                    let p = format!("payload-{}", logical(10));
                    let row = identity(named(&root, &p).unwrap());
                    offset.set_silent(offset.peek() + 5.0);
                    settle(&mut root, &mut layout);
                    assert_eq!(identity(named(&root, &p).unwrap()), row);
                    assert_eq!(identity(named(&root, "header").unwrap()), head);
                    assert_eq!(identity(named(&root, "footer").unwrap()), foot);
                    let body =
                        find_in_tree(&root, |el| el.attrs.contains_key("__jadix_vlist_range"))
                            .unwrap();
                    assert!(
                        body.children.len() <= 12,
                        "slots do not expand the row window"
                    );
                    assert!(handle.scroll_to_end());
                    settle(&mut root, &mut layout);
                    assert_eq!(offset.peek(), (total - 100.0) as f32);
                    assert!(handle.viewport().unwrap().visible.is_empty());
                    assert!(renders.get() < 80, "jumps never build intervening rows");
                    count.set(0);
                    settle(&mut root, &mut layout);
                    assert_eq!(handle.viewport().unwrap().total_extent, 280.0);
                    let empty = identity(named(&root, "empty").unwrap());
                    let dimension = usize::from(!horizontal);
                    let first = named(&root, if inverted { "footer" } else { "header" }).unwrap();
                    let empty_node = named(&root, "empty").unwrap();
                    assert_eq!(
                        empty_node.layout_bounds[dimension] - first.layout_bounds[dimension],
                        leading as f32
                    );
                    count.set(4);
                    settle(&mut root, &mut layout);
                    assert!(named(&root, "empty").is_none());
                    count.set(0);
                    settle(&mut root, &mut layout);
                    assert_eq!(identity(named(&root, "empty").unwrap()), empty);
                    assert_eq!(identity(named(&root, "header").unwrap()), head);
                    assert_eq!(identity(named(&root, "footer").unwrap()), foot);
                }
            }
        }
    });
}

#[test]
fn aligned_empty_content_and_keyed_measurement_keep_slots_out_of_row_anchors() {
    let (_owner, ()) = create_root(|| {
        for horizontal in [false, true] {
            for align in [
                ListContentAlign::Start,
                ListContentAlign::Center,
                ListContentAlign::End,
            ] {
                let id = format!("empty-align-{horizontal}-{align:?}");
                let handle = VirtualListHandle::default();
                let mut builder = virtual_list(&id)
                    .handle(handle.clone())
                    .height(100.0)
                    .content_align(align)
                    .header(10.0, Element::new().template_id("header"))
                    .empty(20.0, Element::new().template_id("empty"))
                    .footer(10.0, Element::new().template_id("footer"));
                if horizontal {
                    builder = builder.horizontal();
                }
                let mut root: Element = builder.into();
                let keys = ScrollViewportSignalKeys::new(&id);
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
                settle(&mut root, &mut layout);
                let body =
                    find_in_tree(&root, |el| el.attrs.contains_key("__jadix_vlist_range")).unwrap();
                let dimension = usize::from(!horizontal);
                let leading = align.padding(40.0, 100.0).unwrap()[0] as f32;
                assert_eq!(
                    named(&root, "header").unwrap().layout_bounds[dimension]
                        - body.layout_bounds[dimension],
                    leading
                );
                assert_eq!(
                    named(&root, "empty").unwrap().layout_bounds[dimension]
                        - body.layout_bounds[dimension],
                    leading + 10.0
                );
                assert_eq!(
                    named(&root, "footer").unwrap().layout_bounds[dimension]
                        - body.layout_bounds[dimension],
                    leading + 30.0
                );
                assert_eq!(
                    handle.viewport().unwrap().total_extent,
                    if align == ListContentAlign::Start {
                        40.0
                    } else {
                        100.0
                    }
                );
                assert!(handle.viewport().unwrap().visible.is_empty());
            }
        }
        let data = Rc::new(RefCell::new((0..100).collect::<Vec<_>>()));
        let revision = Rc::new(Cell::new(0));
        let focus = Rc::new(Cell::new(None));
        let handle = VirtualListHandle::default();
        let mut root: Element = virtual_list("content-keyed")
            .handle(handle.clone())
            .item_count_fn({
                let data = data.clone();
                move || data.borrow().len()
            })
            .item_key_fn({
                let data = data.clone();
                move |i| data.borrow()[i].to_string()
            })
            .data_key_fn({
                let revision = revision.clone();
                move || revision.get().to_string()
            })
            .focused_index_fn({
                let focus = focus.clone();
                move || focus.get()
            })
            .item_extent(20.0)
            .height(100.0)
            .overscan(0)
            .measure_rows()
            .scroll_ahead(true)
            .sticky_header_indices([0, 10])
            .follow_end(true)
            .header(120.0, Element::new().template_id("keyed-header"))
            .footer(150.0, Element::new())
            .render_item(|_| Element::new().h(30.0))
            .into();
        let keys = ScrollViewportSignalKeys::new("content-keyed");
        let offset = use_signal(&keys.offset_y, || 0.0_f32);
        use_signal(&keys.viewport_h, || 100.0_f32).set_silent(100.0);
        let mut layout = LayoutTree::new();
        settle(&mut root, &mut layout);
        assert_eq!(
            offset.peek(),
            0.0,
            "measuring at header does not create a row anchor"
        );
        assert!(handle.scroll_to_index(10, ListScrollAlign::Start, -7.0));
        settle(&mut root, &mut layout);
        assert_eq!(offset.peek() as f64, handle.row_offset(10).unwrap() + 7.0);
        let before = offset.peek();
        data.borrow_mut().insert(0, 100);
        revision.set(1);
        settle(&mut root, &mut layout);
        assert_eq!(offset.peek(), before + 30.0);
        assert_eq!(offset.peek() as f64, handle.row_offset(11).unwrap() + 7.0);
        assert!(handle.scroll_to_end());
        settle(&mut root, &mut layout);
        assert!(handle.viewport().unwrap().visible.is_empty());
        let pin = handle.sticky_header().unwrap();
        assert!(
            handle.row_offset(pin.index).unwrap() + pin.translation + 30.0
                <= handle.viewport().unwrap().total_extent - 150.0
        );
        focus.set(Some(5));
        settle(&mut root, &mut layout);
        assert!(handle.viewport().unwrap().visible.contains(&5));
        focus.set(None);
        let before = offset.peek();
        data.borrow_mut().push(101);
        revision.set(2);
        settle(&mut root, &mut layout);
        assert_eq!(
            offset.peek(),
            before,
            "focus cancels end following ownership"
        );
    });
}
