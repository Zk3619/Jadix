use jadix_components::{
    components::{VirtualGridHandle, virtual_wrap},
    core::{
        reactivity::{create_root, use_signal},
        scroll_viewport::ScrollViewportSignalKeys,
        tree::{Element, LayoutStatus, LayoutTree, find_in_tree},
    },
    virtualization::{ListContentAlign, ListScrollAlign},
};
use std::{cell::Cell, rc::Rc};

fn sync(root: &mut Element, layout: &mut LayoutTree) -> LayoutStatus {
    let mut status = LayoutStatus::Clean;
    if root
        .template_id
        .as_deref()
        .is_some_and(|id| id.ends_with("__grid_body"))
    {
        if let Some(callback) = root.reactive_sync.clone() {
            status = callback.run(root, layout);
        }
    }
    for child in &mut root.children {
        status = status.merge(sync(child, layout));
    }
    status
}

fn body(root: &Element) -> &Element {
    find_in_tree(root, |el| {
        el.template_id
            .as_deref()
            .is_some_and(|id| id.ends_with("__grid_body"))
    })
    .unwrap()
}

fn slot(root: &Element, index: usize) -> &Element {
    body(root)
        .children
        .iter()
        .find(|el| el.attrs.get("__jadix_vwrap_slot") == Some(&index.to_string()))
        .unwrap()
}

fn tag_slots(root: &mut Element) {
    if root
        .template_id
        .as_deref()
        .is_some_and(|id| id.ends_with("__grid_body"))
    {
        for slot in &mut root.children {
            if slot.attrs.contains_key("__jadix_vwrap_slot") {
                slot.set_attr("retained", "yes");
            }
        }
    }
    for child in &mut root.children {
        tag_slots(child);
    }
}

#[test]
fn wrap_slots_align_and_survive_empty_data_reflow_hidden_and_error_transitions() {
    for align in [
        ListContentAlign::Start,
        ListContentAlign::Center,
        ListContentAlign::End,
    ] {
        let (_owner, ()) = create_root(|| {
            let id = format!("wrap-slots-{align:?}");
            let count = Rc::new(Cell::new(3));
            let item_height = Rc::new(Cell::new(20.0));
            let revision = Rc::new(Cell::new(0));
            let renders = Rc::new(Cell::new(0));
            let handle = VirtualGridHandle::default();
            let mut root: Element = virtual_wrap(&id)
                .handle(handle.clone())
                .item_count_fn({
                    let count = count.clone();
                    move || count.get()
                })
                .item_height_fn({
                    let height = item_height.clone();
                    move || height.get()
                })
                .data_key_fn({
                    let revision = revision.clone();
                    move || revision.get().to_string()
                })
                .minimum_item_width(80.0)
                .item_gap(10.0, 5.0)
                .overscan(0)
                .max_mounted_cells(8)
                .content_align(align)
                .follow_end(true)
                .header(
                    30.0,
                    Element::new().template_id("authored-header").text("header"),
                )
                .footer(
                    15.0,
                    Element::new().template_id("authored-footer").text("footer"),
                )
                .empty(
                    40.0,
                    Element::new().template_id("authored-empty").text("empty"),
                )
                .render_item({
                    let renders = renders.clone();
                    move |i| {
                        renders.set(renders.get() + 1);
                        Element::new().text(i.to_string())
                    }
                })
                .on_viewport_change({
                    let handle = handle.clone();
                    move |v| assert_eq!(handle.viewport(), v)
                })
                .into();
            let keys = ScrollViewportSignalKeys::new(&id);
            let width = use_signal(&keys.viewport_w, || 0.0_f32);
            let height = use_signal(&keys.viewport_h, || 0.0_f32);
            let y = use_signal(&keys.offset_y, || 0.0_f32);
            let target = use_signal(&keys.target_y, || 0.0_f32);
            let mut layout = LayoutTree::new();
            let padding = |natural: f64| match align {
                ListContentAlign::Start => 0.0,
                ListContentAlign::Center => (100.0 - natural).max(0.0) * 0.5,
                ListContentAlign::End => (100.0 - natural).max(0.0),
            };
            sync(&mut root, &mut layout);
            assert!(
                body(&root).children.is_empty(),
                "slots wait for a measured viewport"
            );
            width.set_silent(180.0);
            height.set_silent(100.0);
            sync(&mut root, &mut layout);
            assert_eq!(renders.get(), 3);
            assert_eq!(body(&root).children.len(), 5);
            assert_eq!(handle.item_origin(0), Some([0.0, 30.0 + padding(90.0)]));
            tag_slots(&mut root);
            layout.build_and_layout(&mut root, 180.0, 100.0, [-1.0; 2]);
            let grid = body(&root);
            for (i, top, extent) in [(0, padding(90.0), 30.0), (1, 75.0 + padding(90.0), 15.0)] {
                assert_eq!(
                    slot(&root, i).layout_bounds[1] - grid.layout_bounds[1],
                    top as f32
                );
                assert_eq!(slot(&root, i).layout_bounds[2..], [180.0, extent]);
            }
            assert_eq!(sync(&mut root, &mut layout), LayoutStatus::Clean);
            count.set(0);
            sync(&mut root, &mut layout);
            assert_eq!(body(&root).children.len(), 3);
            assert_eq!(renders.get(), 3, "empty is not a data item");
            assert!(handle.item_origin(0).is_none());
            assert_eq!(
                handle.viewport().unwrap().rows.total_extent,
                if align == ListContentAlign::Start {
                    85.0
                } else {
                    100.0
                }
            );
            for i in [0, 1] {
                assert_eq!(slot(&root, i).attrs["retained"], "yes");
            }
            tag_slots(&mut root);
            layout.build_and_layout(&mut root, 180.0, 100.0, [-1.0; 2]);
            assert_eq!(
                slot(&root, 2).layout_bounds[1] - body(&root).layout_bounds[1],
                (30.0 + padding(85.0)) as f32
            );
            count.set(3);
            revision.set(1);
            sync(&mut root, &mut layout);
            assert!(
                !body(&root)
                    .children
                    .iter()
                    .any(|el| el.attrs.get("__jadix_vwrap_slot") == Some(&"2".into()))
            );
            width.set_silent(90.0);
            sync(&mut root, &mut layout);
            assert_eq!(handle.item_origin(2), Some([0.0, 80.0]));
            assert_eq!(handle.viewport().unwrap().rows.total_extent, 115.0);
            assert_eq!(y.peek(), 15.0, "following includes footer and reflow");
            width.set_silent(0.0);
            sync(&mut root, &mut layout);
            assert!(body(&root).children.is_empty());
            width.set_silent(180.0);
            item_height.set(f32::NAN);
            sync(&mut root, &mut layout);
            assert!(handle.viewport().is_none());
            assert!(body(&root).children.is_empty());
            item_height.set(20.0);
            count.set(100_001);
            height.set_silent(400.0);
            sync(&mut root, &mut layout);
            assert_eq!(body(&root).attrs["data-virtual-grid-error"], "Capacity");
            assert!(body(&root).children.is_empty());
            height.set_silent(100.0);
            sync(&mut root, &mut layout);
            for i in [0, 1] {
                assert_eq!(slot(&root, i).attrs["retained"], "yes");
            }
            assert!(
                body(&root).children.len() <= 10,
                "two authored slots outside the eight-cell budget"
            );
            assert!(handle.scroll_to_item(0, ListScrollAlign::Start, 0.0));
            sync(&mut root, &mut layout);
            assert_eq!([y.peek(), target.peek()], [30.0; 2]);
            assert!(handle.scroll_to_end());
            sync(&mut root, &mut layout);
            let total = handle.viewport().unwrap().rows.total_extent;
            assert_eq!(f64::from(y.peek()), total - 100.0);
            count.set(0);
            sync(&mut root, &mut layout);
            for i in [0, 1, 2] {
                assert_eq!(slot(&root, i).attrs["retained"], "yes");
            }
            assert_eq!([y.peek(), target.peek()], [0.0; 2]);
        });
    }
}
