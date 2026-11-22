use jadix_components::{
    components::virtual_list::{VirtualListHandle, virtual_list},
    core::{
        reactivity::{create_root, use_signal},
        scroll_viewport::ScrollViewportSignalKeys,
        tree::{Element, LayoutStatus, LayoutTree, find_in_tree},
    },
    virtualization::ListScrollAlign,
};
use std::{cell::Cell, rc::Rc};

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
fn row<'a>(root: &'a Element, id: &str, index: usize) -> Option<&'a Element> {
    let id = format!("{id}__row_{index}");
    find_in_tree(root, |el| el.template_id.as_deref() == Some(id.as_str()))
}

#[test]
fn sticky_lists_reuse_one_extra_row_push_off_and_keep_natural_layout_on_both_axes() {
    let (_owner, ()) = create_root(|| {
        for horizontal in [false, true] {
            for measured in [false, true] {
                let id = format!("sticky-{horizontal}-{measured}");
                let handle = VirtualListHandle::default();
                let renders = Rc::new(Cell::new(0));
                let mut builder = virtual_list(&id)
                    .handle(handle.clone())
                    .item_count(100_000)
                    .item_extent(20.0)
                    .item_gap(5.0)
                    .overscan(0)
                    .height(50.0)
                    .sticky_header_indices([0, 505, 1000])
                    .render_item({
                        let renders = renders.clone();
                        move |_| {
                            renders.set(renders.get() + 1);
                            let mut child = Element::new().w(20.0).h(20.0);
                            child.set_attr("token", renders.get().to_string());
                            child
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
                    || 50.0_f32,
                );
                viewport.set_silent(50.0);
                let mut layout = LayoutTree::new();
                offset.set_silent(12500.25);
                for _ in 0..3 {
                    sync(&mut root, &mut layout);
                    layout.build_and_layout(&mut root, 50.0, 50.0, [-1.0; 2]);
                }
                let pin = handle.sticky_header().unwrap();
                assert_eq!((pin.index, pin.translation), (0, 12500.25));
                assert!(renders.get() < 12, "do not mount intervening rows");
                assert_eq!(
                    body(&root)
                        .children
                        .iter()
                        .filter(|r| r.attrs.contains_key("__jadix_vlist_row_key"))
                        .count(),
                    4
                );
                let token = row(&root, &id, 0).unwrap().children[0].attrs["token"].clone();
                let before = renders.get();
                offset.set_silent(12500.5);
                assert_eq!(sync(&mut root, &mut layout), LayoutStatus::LayoutChanged);
                assert_eq!(renders.get(), before);
                layout.build_and_layout(&mut root, 50.0, 50.0, [-1.0; 2]);
                let b = body(&root);
                let dimension = if horizontal { 0 } else { 1 };
                assert_eq!(
                    row(&root, &id, 0).unwrap().layout_bounds[dimension]
                        - b.layout_bounds[dimension],
                    12500.5
                );
                assert_eq!(
                    row(&root, &id, 501).unwrap().layout_bounds[dimension]
                        - b.layout_bounds[dimension],
                    12525.0
                );
                assert_eq!(
                    row(&root, &id, 0).unwrap().children[0].attrs["token"],
                    token
                );
                assert_eq!(sync(&mut root, &mut layout), LayoutStatus::Clean);
                assert_eq!(handle.viewport().unwrap().visible, 500..503);
                offset.set_silent(12620.0);
                sync(&mut root, &mut layout);
                assert_eq!(
                    handle.sticky_header().unwrap().translation,
                    12605.0,
                    "next header pushes away"
                );
                offset.set_silent(12625.0);
                sync(&mut root, &mut layout);
                assert_eq!(handle.sticky_header().unwrap().index, 505);
                assert!(row(&root, &id, 0).is_none());
                offset.set_silent(0.0);
                sync(&mut root, &mut layout);
                assert_eq!(
                    handle.row_offset(505),
                    Some(12625.0),
                    "translation must not feed row measurements"
                );
                assert!(handle.scroll_to_index(1000, ListScrollAlign::Start, 0.0));
                sync(&mut root, &mut layout);
                assert_eq!(handle.sticky_header().unwrap().index, 1000);
                viewport.set_silent(0.0);
                sync(&mut root, &mut layout);
                assert!(handle.sticky_header().is_none());
                assert!(
                    !body(&root)
                        .children
                        .iter()
                        .any(|r| r.attrs.contains_key("__jadix_vlist_sticky"))
                );
            }
        }
    });
}

#[test]
fn sticky_configuration_revisions_insets_measurements_and_empty_data_reconcile() {
    let (_owner, ()) = create_root(|| {
        let handle = VirtualListHandle::default();
        let count = Rc::new(Cell::new(100));
        let revision = Rc::new(Cell::new(0));
        let reads = Rc::new(Cell::new(0));
        let inset = Rc::new(Cell::new(0.0_f32));
        let mut root: Element = virtual_list("sticky-dynamic")
            .handle(handle.clone())
            .item_count_fn({
                let count = count.clone();
                move || count.get()
            })
            .data_key_fn({
                let revision = revision.clone();
                move || revision.get().to_string()
            })
            .sticky_header_indices_fn({
                let reads = reads.clone();
                let revision = revision.clone();
                move || {
                    reads.set(reads.get() + 1);
                    if revision.get() == 0 {
                        vec![]
                    } else {
                        vec![0, 5, 10, 99]
                    }
                }
            })
            .sticky_header_offset(-5.0)
            .sticky_header_offset_fn({
                let inset = inset.clone();
                move || inset.get()
            })
            .item_extent(20.0)
            .height(50.0)
            .overscan(0)
            .measure_rows()
            .render_item(|i| Element::new().h(if i == 0 { 40.0 } else { 20.0 }))
            .into();
        let keys = ScrollViewportSignalKeys::new("sticky-dynamic");
        let offset = use_signal(&keys.offset_y, || 0.0_f32);
        let viewport = use_signal(&keys.viewport_h, || 50.0_f32);
        viewport.set_silent(50.0);
        let mut layout = LayoutTree::new();
        sync(&mut root, &mut layout);
        assert!(handle.sticky_header().is_none());
        revision.set(1);
        for _ in 0..3 {
            sync(&mut root, &mut layout);
            layout.build_and_layout(&mut root, 80.0, 50.0, [-1.0; 2]);
        }
        offset.set_silent(110.0);
        for _ in 0..2 {
            sync(&mut root, &mut layout);
            layout.build_and_layout(&mut root, 80.0, 50.0, [-1.0; 2]);
        }
        assert_eq!(handle.row_offset(5), Some(120.0));
        assert_eq!(handle.sticky_header().unwrap().translation, 80.0);
        assert_eq!(
            reads.get(),
            2,
            "scroll and measurement do not refresh header table"
        );
        inset.set(150.0);
        sync(&mut root, &mut layout);
        assert_eq!(
            handle.sticky_header().unwrap().index,
            10,
            "inset can pin after the mounted viewport"
        );
        assert!(row(&root, "sticky-dynamic", 10).is_some());
        layout.build_and_layout(&mut root, 80.0, 50.0, [-1.0; 2]);
        let b = body(&root);
        assert_eq!(
            row(&root, "sticky-dynamic", 10).unwrap().layout_bounds[1] - b.layout_bounds[1],
            260.0
        );
        inset.set(-150.0);
        sync(&mut root, &mut layout);
        assert!(handle.sticky_header().is_none());
        inset.set(f32::NAN);
        sync(&mut root, &mut layout);
        assert_eq!(
            handle.sticky_header().unwrap().index,
            0,
            "fallback to configured finite inset"
        );
        count.set(1);
        sync(&mut root, &mut layout);
        assert!(
            handle.sticky_header().is_none(),
            "negative inset has not reached the first header"
        );
        inset.set(0.0);
        sync(&mut root, &mut layout);
        assert_eq!(handle.sticky_header().unwrap().index, 0);
        count.set(0);
        sync(&mut root, &mut layout);
        assert!(handle.sticky_header().is_none());
        assert!(body(&root).children.is_empty());
        assert_eq!(reads.get(), 4);
    });
}
