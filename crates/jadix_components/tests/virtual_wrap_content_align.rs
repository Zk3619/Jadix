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

fn body<'a>(root: &'a Element, id: &str) -> &'a Element {
    find_in_tree(root, |el| el.template_id.as_deref() == Some(id)).unwrap()
}

#[test]
fn wrap_alignment_keeps_layout_navigation_identity_and_follow_in_one_coordinate_space() {
    for (name, align) in [
        ("start", ListContentAlign::Start),
        ("center", ListContentAlign::Center),
        ("end", ListContentAlign::End),
    ] {
        let (_owner, ()) = create_root(|| {
            let id = format!("aligned-wrap-{name}");
            let body_id = format!("{id}__grid_body");
            let count = Rc::new(Cell::new(3));
            let extent = Rc::new(Cell::new(20.0));
            let renders = Rc::new(Cell::new(0));
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
                .minimum_item_width(80.0)
                .item_gap(10.0, 5.0)
                .content_align(align)
                .follow_end(true)
                .scroll_ahead(true)
                .overscan(0)
                .max_mounted_cells(16)
                .render_item({
                    let renders = renders.clone();
                    move |index| {
                        renders.set(renders.get() + 1);
                        Element::new().text(index.to_string())
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
            let pad = |natural: f64, viewport: f64| match align {
                ListContentAlign::Start => 0.0,
                ListContentAlign::Center => (viewport - natural).max(0.0) * 0.5,
                ListContentAlign::End => (viewport - natural).max(0.0),
            };
            width.set_silent(180.0);
            height.set_silent(100.0);
            y.set_silent(999.0);
            sync(&mut root, &mut layout);
            assert_eq!([y.peek(), target.peek()], [0.0; 2]);
            let state = handle.viewport().unwrap();
            assert_eq!(state.rows.visible, 0..2);
            assert_eq!(
                state.rows.total_extent,
                if align == ListContentAlign::Start {
                    45.0
                } else {
                    100.0
                }
            );
            assert_eq!(handle.item_origin(0), Some([0.0, pad(45.0, 100.0)]));
            assert_eq!(handle.cell_origin(1, 0), handle.item_origin(2));
            assert_eq!(
                handle.cell_origin(1, 1),
                None,
                "partial row has no padding item"
            );
            layout.build_and_layout(&mut root, 180.0, 100.0, [-1.0; 2]);
            let grid = body(&root, &body_id);
            assert_eq!(f64::from(grid.layout_bounds[3]), state.rows.total_extent);
            for cell in &grid.children {
                let index = cell.attrs["__jadix_vgrid_index"].parse().unwrap();
                let origin = handle.item_origin(index).unwrap().map(|v| v as f32);
                assert_eq!(
                    [
                        cell.layout_bounds[0] - grid.layout_bounds[0],
                        cell.layout_bounds[1] - grid.layout_bounds[1]
                    ],
                    origin
                );
            }
            let renders_before = renders.get();
            assert_eq!(sync(&mut root, &mut layout), LayoutStatus::Clean);
            height.set_silent(140.0);
            sync(&mut root, &mut layout);
            assert_eq!(handle.item_origin(0), Some([0.0, pad(45.0, 140.0)]));
            assert_eq!(
                renders.get(),
                renders_before,
                "alignment moves existing wrappers"
            );
            width.set_silent(90.0);
            sync(&mut root, &mut layout);
            assert_eq!(handle.item_origin(2), Some([0.0, 50.0 + pad(70.0, 140.0)]));
            assert_eq!(
                renders.get(),
                renders_before,
                "reflow retains flat-item identity"
            );
            assert!(handle.scroll_to_cell(2, 0, [ListScrollAlign::Start; 2], [0.0; 2]));
            sync(&mut root, &mut layout);
            assert_eq!(y.peek(), 0.0);
            count.set(100_001);
            sync(&mut root, &mut layout);
            let state = handle.viewport().unwrap();
            assert_eq!(
                f64::from(y.peek()),
                state.rows.total_extent - 140.0,
                "short-to-long follows the old end"
            );
            assert_eq!(handle.item_origin(0), Some([0.0; 2]));
            assert!(body(&root, &body_id).children.len() <= 16);
            count.set(3);
            sync(&mut root, &mut layout);
            assert_eq!(y.peek(), 0.0);
            assert_eq!(handle.item_origin(0), Some([0.0, pad(70.0, 140.0)]));
            extent.set(f32::NAN);
            sync(&mut root, &mut layout);
            assert!(handle.viewport().is_none());
            extent.set(20.0);
            width.set_silent(180.0);
            sync(&mut root, &mut layout);
            assert_eq!(handle.item_origin(0), Some([0.0, pad(45.0, 140.0)]));
            width.set_silent(0.0);
            sync(&mut root, &mut layout);
            assert!(body(&root, &body_id).children.is_empty());
            width.set_silent(180.0);
            sync(&mut root, &mut layout);
            assert_eq!(body(&root, &body_id).children.len(), 3);
            count.set(0);
            sync(&mut root, &mut layout);
            assert_eq!(
                handle.viewport().unwrap().rows.total_extent,
                if align == ListContentAlign::Start {
                    0.0
                } else {
                    140.0
                }
            );
            assert!(handle.item_origin(0).is_none());
            assert!(body(&root, &body_id).children.is_empty());
        });
    }
}
