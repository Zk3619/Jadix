use jadix_components::{
    components::{VirtualGridHandle, virtual_grid, virtual_wrap},
    core::{
        reactivity::{create_root, use_signal},
        scroll_viewport::{
            ScrollViewportSignalKeys, activate_scroll_viewport, scroll_content_h_signal_key,
            scroll_content_w_signal_key,
        },
        tree::{Element, LayoutTree, find_in_tree},
    },
    prelude::vstack,
    virtualization::ListScrollAlign,
};
use std::{cell::Cell, rc::Rc};

fn sync(root: &mut Element, layout: &mut LayoutTree) {
    if root
        .template_id
        .as_deref()
        .is_some_and(|id| id.ends_with("__grid_body"))
    {
        root.reactive_sync.clone().unwrap().run(root, layout);
    }
    for child in &mut root.children {
        sync(child, layout);
    }
}

fn body<'a>(root: &'a Element, id: &str) -> &'a Element {
    find_in_tree(root, |el| el.template_id.as_deref() == Some(id)).unwrap()
}

fn cells(root: &Element, id: &str) -> usize {
    body(root, id)
        .children
        .iter()
        .filter(|cell| cell.attrs.contains_key("__jadix_vgrid_cell"))
        .count()
}

#[test]
fn parent_grid_binds_each_axis_through_orthogonal_owners_and_navigates_full_content() {
    let (_owner, ()) = create_root(|| {
        let handle = VirtualGridHandle::default();
        let grid: Element = virtual_grid("parent-grid")
            .use_parent_scroll()
            .handle(handle.clone())
            .row_count(100)
            .column_count(100)
            .cell_size(20.0, 10.0)
            .overscan(0, 0)
            .scroll_ahead(true)
            .max_mounted_cells(128)
            .render_cell(|_, _| Element::new())
            .into();
        assert!(grid.focus_id.is_none());
        let cross = activate_scroll_viewport(
            Element::new()
                .w(70.0)
                .h(1000.0)
                .flex_shrink(0.0)
                .child(grid),
            "parent-grid-x",
            "auto",
            "visible",
            "auto",
            "auto",
        );
        let mut root = activate_scroll_viewport(
            Element::from(vstack())
                .w(70.0)
                .h(100.0)
                .child(Element::new().h(150.0).flex_shrink(0.0))
                .child(cross)
                .child(Element::new().h(250.0).flex_shrink(0.0)),
            "parent-grid-y",
            "visible",
            "auto",
            "auto",
            "auto",
        );
        let mut layout = LayoutTree::new();
        sync(&mut root, &mut layout);
        assert_eq!(
            cells(&root, "parent-grid__grid_body"),
            0,
            "unbound has no implicit viewport"
        );
        layout.build_and_layout(&mut root, 70.0, 100.0, [-1.0; 2]);
        let node = body(&root, "parent-grid__grid_body")
            .layout_node_id()
            .unwrap();
        assert_eq!(
            layout.ancestor_scroll_viewport(node, true),
            Some(("parent-grid-x", 0.0))
        );
        assert_eq!(
            layout.ancestor_scroll_viewport(node, false),
            Some(("parent-grid-y", 150.0))
        );
        let kx = ScrollViewportSignalKeys::new("parent-grid-x");
        let ky = ScrollViewportSignalKeys::new("parent-grid-y");
        let x = use_signal(&kx.offset_x, || 0.0_f32);
        let y = use_signal(&ky.offset_y, || 0.0_f32);
        let width = use_signal(&kx.viewport_w, || 0.0_f32);
        let height = use_signal(&ky.viewport_h, || 0.0_f32);
        width.set_silent(70.0);
        height.set_silent(100.0);
        use_signal(&scroll_content_w_signal_key("parent-grid-x"), || 0.0_f32).set_silent(2000.0);
        use_signal(&scroll_content_h_signal_key("parent-grid-y"), || 0.0_f32).set_silent(1400.0);
        sync(&mut root, &mut layout);
        assert_eq!(handle.viewport().unwrap().rows.offset, -150.0);
        assert_eq!(
            cells(&root, "parent-grid__grid_body"),
            0,
            "offscreen overscan and prediction stay empty"
        );
        y.set_silent(125.0);
        sync(&mut root, &mut layout);
        let state = handle.viewport().unwrap();
        assert_eq!(
            (
                state.rows.visible,
                state.rows.offset,
                state.rows.viewport_extent
            ),
            (0..8, -25.0, 100.0)
        );
        assert_eq!(state.columns.visible, 0..4);
        assert_eq!(cells(&root, "parent-grid__grid_body"), 32);
        assert!(handle.scroll_to_cell(50, 60, [ListScrollAlign::Start; 2], [0.0; 2]));
        height.set_silent(0.0);
        sync(&mut root, &mut layout);
        assert_eq!(
            [x.peek(), y.peek()],
            [0.0, 125.0],
            "hidden navigation stays queued"
        );
        height.set_silent(100.0);
        sync(&mut root, &mut layout);
        assert_eq!([x.peek(), y.peek()], [1200.0, 650.0]);
        assert_eq!(use_signal(&kx.target_x, || 0.0_f32).peek(), 1200.0);
        assert_eq!(use_signal(&ky.target_y, || 0.0_f32).peek(), 650.0);
        assert_eq!(handle.viewport().unwrap().rows.visible, 50..60);
        assert!(handle.scroll_to_end());
        sync(&mut root, &mut layout);
        assert_eq!(
            [x.peek(), y.peek()],
            [1930.0, 1050.0],
            "child end preserves following parent content"
        );
        y.set_silent(1300.0);
        sync(&mut root, &mut layout);
        assert_eq!(y.peek(), 1300.0, "never clamp the owner to the child");
        assert_eq!(cells(&root, "parent-grid__grid_body"), 0);
        width.set_silent(2000.0);
        y.set_silent(150.0);
        x.set_silent(0.0);
        sync(&mut root, &mut layout);
        assert_eq!(
            body(&root, "parent-grid__grid_body").attrs["data-virtual-grid-error"],
            "Capacity"
        );
        width.set_silent(70.0);
        sync(&mut root, &mut layout);
        assert!(
            !body(&root, "parent-grid__grid_body")
                .attrs
                .contains_key("data-virtual-grid-error")
        );
        layout.build_and_layout(&mut root, 70.0, 100.0, [-1.0; 2]);
        assert_eq!(
            &body(&root, "parent-grid__grid_body").layout_bounds[2..],
            &[2000.0, 1000.0]
        );
    });
}

#[test]
fn parent_wrap_reflows_without_moving_owner_and_uses_slots_for_navigation_and_edges() {
    let (_owner, ()) = create_root(|| {
        let handle = VirtualGridHandle::default();
        let count = Rc::new(Cell::new(100));
        let starts = Rc::new(Cell::new(0));
        let ends = Rc::new(Cell::new(0));
        let wrap: Element = virtual_wrap("parent-wrap")
            .use_parent_scroll()
            .handle(handle.clone())
            .item_count_fn({
                let count = count.clone();
                move || count.get()
            })
            .minimum_item_width(20.0)
            .item_height(10.0)
            .overscan(0)
            .scroll_ahead(true)
            .header(20.0, Element::new())
            .footer(30.0, Element::new())
            .initial_scroll_at_end()
            .follow_end(true)
            .load_more_threshold(0.0)
            .on_load_more_at_start({
                let calls = starts.clone();
                move || calls.set(calls.get() + 1)
            })
            .on_load_more({
                let calls = ends.clone();
                move || calls.set(calls.get() + 1)
            })
            .render_item(|_| Element::new())
            .into();
        assert!(wrap.focus_id.is_none());
        let mut root = activate_scroll_viewport(
            Element::from(vstack())
                .w(70.0)
                .h(100.0)
                .child(Element::new().h(150.0).flex_shrink(0.0))
                .child(wrap)
                .child(Element::new().h(250.0).flex_shrink(0.0)),
            "parent-wrap-owner",
            "visible",
            "auto",
            "auto",
            "auto",
        );
        let keys = ScrollViewportSignalKeys::new("parent-wrap-owner");
        let y = use_signal(&keys.offset_y, || 0.0_f32);
        let height = use_signal(&keys.viewport_h, || 0.0_f32);
        let mut layout = LayoutTree::new();
        layout.build_and_layout(&mut root, 70.0, 100.0, [-1.0; 2]);
        height.set_silent(100.0);
        use_signal(&scroll_content_h_signal_key("parent-wrap-owner"), || {
            0.0_f32
        })
        .set_silent(790.0);
        sync(&mut root, &mut layout);
        assert_eq!(
            y.peek(),
            0.0,
            "local initial/end policies cannot move the parent"
        );
        assert_eq!(cells(&root, "parent-wrap__grid_body"), 0);
        assert_eq!((starts.get(), ends.get()), (0, 0));
        y.set_silent(125.0);
        sync(&mut root, &mut layout);
        assert_eq!(handle.viewport().unwrap().columns.item_count, 3);
        assert_eq!(handle.viewport().unwrap().rows.visible, 0..6);
        assert_eq!(cells(&root, "parent-wrap__grid_body"), 18);
        assert_eq!(starts.get(), 1);
        assert!(handle.scroll_to_item(99, ListScrollAlign::Start, 0.0));
        sync(&mut root, &mut layout);
        assert_eq!(y.peek(), 500.0);
        assert_eq!(ends.get(), 1);
        assert!(handle.scroll_to_end());
        sync(&mut root, &mut layout);
        assert_eq!(y.peek(), 440.0);
        y.set_silent(700.0);
        sync(&mut root, &mut layout);
        assert_eq!(cells(&root, "parent-wrap__grid_body"), 0);
        assert_eq!(ends.get(), 1, "passing the local end disarms its edge");
        y.set_silent(200.0);
        count.set(120);
        root = root.w(40.0);
        layout.build_and_layout(&mut root, 40.0, 100.0, [-1.0; 2]);
        height.set_silent(100.0);
        y.set_silent(200.0);
        sync(&mut root, &mut layout);
        assert_eq!(
            y.peek(),
            200.0,
            "reflow and growth leave parent position alone"
        );
        assert_eq!(handle.viewport().unwrap().columns.item_count, 2);
        layout.build_and_layout(&mut root, 40.0, 100.0, [-1.0; 2]);
        assert_eq!(
            &body(&root, "parent-wrap__grid_body").layout_bounds[2..],
            &[40.0, 650.0]
        );
        height.set_silent(0.0);
        assert!(handle.scroll_to_item(119, ListScrollAlign::Start, 0.0));
        sync(&mut root, &mut layout);
        assert_eq!(y.peek(), 200.0);
        use_signal(&scroll_content_h_signal_key("parent-wrap-owner"), || {
            0.0_f32
        })
        .set_silent(1050.0);
        height.set_silent(100.0);
        sync(&mut root, &mut layout);
        assert_eq!(y.peek(), 760.0);
        assert_eq!(use_signal(&keys.target_y, || 0.0_f32).peek(), 760.0);
        assert_eq!(
            cells(&root, "parent-wrap__grid_body"),
            2,
            "partial last row counts real items"
        );
        let mut replacement = Element::new();
        layout.build_and_layout(&mut replacement, 40.0, 100.0, [-1.0; 2]);
        sync(&mut root, &mut layout);
        assert_eq!(
            cells(&root, "parent-wrap__grid_body"),
            0,
            "retired nodes unbind safely"
        );
    });
}
