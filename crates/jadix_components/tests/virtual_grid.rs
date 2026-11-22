use jadix_components::{
    components::{VirtualGridHandle, virtual_grid, virtual_wrap},
    core::{
        reactivity::{create_root, use_signal},
        scroll_viewport::ScrollViewportSignalKeys,
        tree::{Element, LayoutStatus, LayoutTree, find_in_tree},
    },
    virtualization::ListScrollAlign,
};
use std::{cell::Cell, rc::Rc};

fn sync(element: &mut Element, layout: &mut LayoutTree) -> LayoutStatus {
    let mut status = element
        .reactive_sync
        .clone()
        .map_or(LayoutStatus::Clean, |callback| {
            callback.run(element, layout)
        });
    for child in &mut element.children {
        status = status.merge(sync(child, layout));
    }
    status
}

fn body<'a>(element: &'a Element, id: &str) -> &'a Element {
    find_in_tree(element, |el| el.template_id.as_deref() == Some(id)).unwrap()
}

#[test]
fn grid_and_wrap_resample_render_edits_before_publishing_and_bound_unstable_work() {
    let (_owner, ()) = create_root(|| {
        for wrap in [false, true] {
            for keyed in [false, true] {
                let id = format!("render-edit-{wrap}-{keyed}");
                let count = Rc::new(Cell::new(20));
                let revision = Rc::new(Cell::new(0));
                let size = Rc::new(Cell::new([20.0, 20.0]));
                let mode = Rc::new(Cell::new(1));
                let renders = Rc::new(Cell::new(0));
                let reports = Rc::new(Cell::new(0));
                let handle = VirtualGridHandle::default();
                let render = {
                    let count = count.clone();
                    let revision = revision.clone();
                    let size = size.clone();
                    let mode = mode.clone();
                    let renders = renders.clone();
                    move |row, column| {
                        assert!(
                            row < count.get() && column < count.get(),
                            "stale coordinates"
                        );
                        renders.set(renders.get() + 1);
                        if mode.get() == 1 {
                            count.set(2);
                            revision.set(revision.get() + 1);
                            mode.set(0);
                        } else if mode.get() == 2 {
                            revision.set(revision.get() + 1);
                        } else if mode.get() == 3 {
                            size.set([15.0, 15.0]);
                            revision.set(revision.get() + 1);
                            mode.set(0);
                        }
                        let mut child = Element::new();
                        child.set_attr("render_revision", revision.get().to_string());
                        child
                    }
                };
                let data_key = {
                    let revision = revision.clone();
                    move || revision.get().to_string()
                };
                let on_viewport = {
                    let count = count.clone();
                    let handle = handle.clone();
                    let reports = reports.clone();
                    move |viewport: Option<jadix_components::components::GridViewport>| {
                        assert_eq!(handle.viewport(), viewport);
                        let viewport = viewport.unwrap();
                        assert_eq!(viewport.rows.item_count, if wrap { 1 } else { count.get() });
                        assert_eq!(viewport.columns.item_count, count.get());
                        reports.set(reports.get() + 1);
                    }
                };
                let mut element: Element = if wrap {
                    let mut builder = virtual_wrap(&id)
                        .minimum_item_width(20.0)
                        .item_height(20.0)
                        .overscan(0)
                        .item_count_fn({
                            let count = count.clone();
                            move || count.get()
                        })
                        .render_item(move |index| render(index, 0));
                    if keyed {
                        builder = builder.item_key_fn(|index| format!("item-{index}"));
                    }
                    builder
                        .handle(handle.clone())
                        .data_key_fn(data_key)
                        .on_viewport_change(on_viewport)
                        .item_height_fn({
                            let size = size.clone();
                            move || size.get()[1]
                        })
                        .into()
                } else {
                    let mut builder = virtual_grid(&id)
                        .row_count_fn({
                            let count = count.clone();
                            move || count.get()
                        })
                        .column_count_fn({
                            let count = count.clone();
                            move || count.get()
                        })
                        .cell_size_fn({
                            let size = size.clone();
                            move || size.get()
                        })
                        .overscan(0, 0)
                        .render_cell(render);
                    if keyed {
                        builder = builder
                            .row_key_fn(|row| format!("row-{row}"))
                            .column_key_fn(|column| format!("column-{column}"));
                    }
                    builder
                        .handle(handle.clone())
                        .data_key_fn(data_key)
                        .on_viewport_change(on_viewport)
                        .into()
                };
                let keys = ScrollViewportSignalKeys::new(&id);
                use_signal(&keys.viewport_w, || 0.0_f32).set_silent(60.0);
                use_signal(&keys.viewport_h, || 0.0_f32).set_silent(60.0);
                let mut layout = LayoutTree::new();
                sync(&mut element, &mut layout);
                assert_eq!(reports.get(), 1, "notify only the final window: {id}");
                let check = |element: &Element| {
                    let grid = body(element, &format!("{id}__grid_body"));
                    assert_eq!(grid.children.len(), if wrap { 2 } else { 4 });
                    for cell in &grid.children {
                        assert_eq!(
                            cell.children[0].attrs["render_revision"],
                            revision.get().to_string()
                        );
                    }
                };
                check(&element);
                revision.set(2);
                mode.set(3);
                sync(&mut element, &mut layout);
                check(&element);
                let before = renders.get();
                let before_reports = reports.get();
                revision.set(4);
                mode.set(2);
                sync(&mut element, &mut layout);
                assert_eq!(renders.get() - before, 4, "retry budget: {id}");
                assert_eq!(
                    reports.get(),
                    before_reports,
                    "unstable pass must not notify"
                );
                mode.set(0);
                sync(&mut element, &mut layout);
                check(&element);
                let before = renders.get();
                assert_eq!(sync(&mut element, &mut layout), LayoutStatus::Clean);
                assert_eq!(renders.get(), before, "stable frames reuse cells");
            }
        }
    });
}

#[test]
fn grid_crops_both_axes_reuses_cells_and_preserves_absolute_gap_geometry() {
    let (_owner, ()) = create_root(|| {
        let handle = VirtualGridHandle::default();
        assert!(!handle.scroll_to_end());
        let renders = Rc::new(Cell::new(0));
        let revision = Rc::new(Cell::new(0));
        let reports = Rc::new(Cell::new(0));
        let mut element: Element = virtual_grid("grid-rect")
            .handle(handle.clone())
            .row_count(10_000)
            .column_count(10_000)
            .cell_size(20.0, 10.0)
            .cell_gap(5.0, 3.0)
            .overscan(0, 0)
            .height(30.0)
            .data_key_fn({
                let revision = revision.clone();
                move || revision.get().to_string()
            })
            .on_viewport_change({
                let reports = reports.clone();
                let handle = handle.clone();
                move |v| {
                    assert_eq!(handle.viewport(), v);
                    reports.set(reports.get() + 1);
                }
            })
            .render_cell({
                let renders = renders.clone();
                move |row, column| {
                    renders.set(renders.get() + 1);
                    let mut cell = Element::new().text(format!("{row}:{column}"));
                    cell.set_attr("token", renders.get().to_string());
                    cell
                }
            })
            .into();
        let keys = ScrollViewportSignalKeys::new("grid-rect");
        let x = use_signal(&keys.offset_x, || 0.0_f32);
        let y = use_signal(&keys.offset_y, || 0.0_f32);
        let width = use_signal(&keys.viewport_w, || 0.0_f32);
        let height = use_signal(&keys.viewport_h, || 0.0_f32);
        let mut layout = LayoutTree::new();
        assert!(handle.scroll_to_cell(500, 600, [ListScrollAlign::Start; 2], [0.0; 2]));
        sync(&mut element, &mut layout);
        assert_eq!(renders.get(), 0, "wait for actual width and height");
        width.set_silent(70.0);
        height.set_silent(30.0);
        sync(&mut element, &mut layout);
        assert_eq!([x.peek(), y.peek()], [15000.0, 6500.0]);
        let state = handle.viewport().unwrap();
        assert_eq!(
            (state.rows.visible, state.columns.visible),
            (500..503, 600..603)
        );
        assert_eq!(
            renders.get(),
            9,
            "100 million cells mount a 3 by 3 rectangle"
        );
        layout.build_and_layout(&mut element, 70.0, 30.0, [-1.0; 2]);
        let grid = body(&element, "grid-rect__grid_body");
        assert_eq!(
            [grid.layout_bounds[2], grid.layout_bounds[3]],
            [249995.0, 129997.0]
        );
        for cell in &grid.children {
            let key = &cell.attrs["__jadix_vgrid_cell"];
            let (r, c) = key.split_once(':').unwrap();
            let origin = handle
                .cell_origin(r.parse().unwrap(), c.parse().unwrap())
                .unwrap();
            assert_eq!(
                [
                    cell.layout_bounds[0] - grid.layout_bounds[0],
                    cell.layout_bounds[1] - grid.layout_bounds[1]
                ],
                origin.map(|n| n as f32)
            );
            assert_eq!([cell.layout_bounds[2], cell.layout_bounds[3]], [20.0, 10.0]);
        }
        let token = grid.children[4].children[0].attrs["token"].clone();
        let before_reports = reports.get();
        assert_eq!(sync(&mut element, &mut layout), LayoutStatus::Clean);
        assert_eq!(reports.get(), before_reports);
        x.set_silent(15025.0);
        y.set_silent(6513.0);
        sync(&mut element, &mut layout);
        assert_eq!(renders.get(), 14, "reuse the four overlapping cells");
        assert_eq!(
            body(&element, "grid-rect__grid_body").children[0].children[0].attrs["token"],
            token
        );
        revision.set(1);
        sync(&mut element, &mut layout);
        assert_eq!(
            renders.get(),
            23,
            "dataset replacement rebuilds only the window"
        );
        drop(element);
        assert!(!handle.scroll_to_end());
    });
}

#[test]
fn grid_navigation_shrink_empty_invalid_and_capacity_recover_on_both_axes() {
    let mut saved = None;
    let (owner, ()) = create_root(|| {
        let handle = VirtualGridHandle::default();
        let rows = Rc::new(Cell::new(100));
        let columns = Rc::new(Cell::new(200));
        let size = Rc::new(Cell::new([20.0, 10.0]));
        let mut element: Element = virtual_grid("grid-resize")
            .handle(handle.clone())
            .row_count_fn({
                let rows = rows.clone();
                move || rows.get()
            })
            .column_count_fn({
                let columns = columns.clone();
                move || columns.get()
            })
            .cell_size_fn({
                let size = size.clone();
                move || size.get()
            })
            .cell_gap(5.0, 3.0)
            .overscan(0, 0)
            .max_mounted_cells(12)
            .into();
        let keys = ScrollViewportSignalKeys::new("grid-resize");
        let x = use_signal(&keys.offset_x, || 0.0_f32);
        let y = use_signal(&keys.offset_y, || 0.0_f32);
        let tx = use_signal(&keys.target_x, || 0.0_f32);
        let ty = use_signal(&keys.target_y, || 0.0_f32);
        let width = use_signal(&keys.viewport_w, || 0.0_f32);
        let height = use_signal(&keys.viewport_h, || 0.0_f32);
        width.set_silent(70.0);
        height.set_silent(30.0);
        let mut layout = LayoutTree::new();
        assert!(handle.scroll_to_end());
        sync(&mut element, &mut layout);
        assert_eq!([x.peek(), y.peek()], [4925.0, 1267.0]);
        assert_eq!([tx.peek(), ty.peek()], [4925.0, 1267.0]);
        rows.set(3);
        columns.set(4);
        sync(&mut element, &mut layout);
        assert_eq!(
            [x.peek(), y.peek(), tx.peek(), ty.peek()],
            [25.0, 6.0, 25.0, 6.0]
        );
        assert_eq!(body(&element, "grid-resize__grid_body").children.len(), 9);
        tx.set_silent(0.0);
        ty.set_silent(0.0);
        assert!(handle.scroll_to_end());
        sync(&mut element, &mut layout);
        assert_eq!(
            [tx.peek(), ty.peek()],
            [25.0, 6.0],
            "same-position jump cancels prior animation targets"
        );
        assert!(handle.scroll_to_cell(99, 0, [ListScrollAlign::Start; 2], [0.0; 2]));
        sync(&mut element, &mut layout);
        assert_eq!(
            [x.peek(), y.peek()],
            [25.0, 6.0],
            "invalid row cannot move the valid column"
        );
        columns.set(0);
        sync(&mut element, &mut layout);
        assert_eq!([x.peek(), y.peek()], [0.0; 2]);
        assert!(body(&element, "grid-resize__grid_body").children.is_empty());
        columns.set(10);
        rows.set(10);
        width.set_silent(250.0);
        height.set_silent(130.0);
        sync(&mut element, &mut layout);
        let grid = body(&element, "grid-resize__grid_body");
        assert_eq!(grid.attrs["data-virtual-grid-error"], "Capacity");
        assert!(grid.children.is_empty());
        width.set_silent(70.0);
        height.set_silent(30.0);
        sync(&mut element, &mut layout);
        assert!(
            !body(&element, "grid-resize__grid_body")
                .attrs
                .contains_key("data-virtual-grid-error")
        );
        size.set([f32::NAN, 10.0]);
        sync(&mut element, &mut layout);
        assert!(handle.viewport().is_none());
        assert_eq!(
            body(&element, "grid-resize__grid_body").attrs["data-virtual-grid-error"],
            "InvalidExtent"
        );
        size.set([40.0, 20.0]);
        sync(&mut element, &mut layout);
        assert_eq!(handle.cell_origin(1, 1), Some([45.0, 23.0]));
        assert!(!handle.scroll_to_cell(0, 0, [ListScrollAlign::Fraction(f64::NAN); 2], [0.0; 2]));
        saved = Some((handle, element));
    });
    drop(owner);
    let (handle, _element) = saved.unwrap();
    assert!(
        !handle.scroll_to_end(),
        "disposed reactive owner rejects a retained tree handle"
    );
}

#[test]
fn wrap_reflows_without_anchor_drift_and_reuses_flat_items() {
    let (_owner, ()) = create_root(|| {
        let handle = VirtualGridHandle::default();
        let renders = Rc::new(Cell::new(0));
        let mut element: Element = virtual_wrap("wrap-reflow")
            .handle(handle.clone())
            .item_count(1000)
            .minimum_item_width(80.0)
            .item_height(10.0)
            .item_gap(7.0, 3.0)
            .overscan(0)
            .height(25.0)
            .render_item({
                let renders = renders.clone();
                move |i| {
                    renders.set(renders.get() + 1);
                    let mut item = Element::new().text(i.to_string());
                    item.set_attr("token", renders.get().to_string());
                    item
                }
            })
            .into();
        let keys = ScrollViewportSignalKeys::new("wrap-reflow");
        let y = use_signal(&keys.offset_y, || 0.0_f32);
        let ty = use_signal(&keys.target_y, || 0.0_f32);
        let width = use_signal(&keys.viewport_w, || 0.0_f32);
        let height = use_signal(&keys.viewport_h, || 0.0_f32);
        width.set_silent(260.0);
        height.set_silent(25.0);
        y.set_silent(68.1);
        let mut layout = LayoutTree::new();
        sync(&mut element, &mut layout);
        assert_eq!(renders.get(), 9);
        let token = body(&element, "wrap-reflow__item_15").children[0].attrs["token"].clone();
        let depth = y.peek() - 65.0;
        for (available, row_top) in [(350.0, 39.0), (175.0, 91.0), (260.0, 65.0), (5.0, 195.0)] {
            width.set_silent(available);
            sync(&mut element, &mut layout);
            assert!((y.peek() - (row_top + depth)).abs() < 0.0001);
            assert_eq!(ty.peek(), y.peek());
            assert_eq!(
                body(&element, "wrap-reflow__item_15").children[0].attrs["token"],
                token
            );
            assert_eq!(sync(&mut element, &mut layout), LayoutStatus::Clean);
            layout.build_and_layout(&mut element, available, 25.0, [-1.0; 2]);
            let grid = body(&element, "wrap-reflow__grid_body");
            assert!((grid.layout_bounds[2] - available).abs() < 0.001);
            for item in &grid.children {
                let index: usize = item.attrs["__jadix_vgrid_cell"]
                    .strip_prefix("item:")
                    .unwrap()
                    .parse()
                    .unwrap();
                let origin = handle.item_origin(index).unwrap();
                assert!(
                    (item.layout_bounds[0] - grid.layout_bounds[0] - origin[0] as f32).abs()
                        < 0.001
                );
                assert!(
                    (item.layout_bounds[1] - grid.layout_bounds[1] - origin[1] as f32).abs()
                        < 0.001
                );
                assert_eq!(item.layout_bounds[3], 10.0);
            }
        }
        width.set_silent(175.0);
        sync(&mut element, &mut layout);
        y.set_silent(29.5);
        sync(&mut element, &mut layout);
        width.set_silent(260.0);
        sync(&mut element, &mut layout);
        assert_eq!(
            y.peek(),
            16.5,
            "a user scroll starts a new anchor at item 4"
        );
        width.set_silent(350.0);
        assert!(handle.scroll_to_item(0, ListScrollAlign::Start, 0.0));
        assert!(handle.scroll_to_item(81, ListScrollAlign::Start, 0.0));
        sync(&mut element, &mut layout);
        assert_eq!(
            y.peek(),
            260.0,
            "latest request uses the current column count"
        );
        assert_eq!(handle.item_origin(81).unwrap()[1], 260.0);
        width.set_silent(175.0);
        sync(&mut element, &mut layout);
        assert_eq!(y.peek(), 520.0);
        assert!(!handle.scroll_to_item(1, ListScrollAlign::Fraction(f64::NAN), 0.0));
        assert!(handle.scroll_to_item(1000, ListScrollAlign::Start, 0.0));
        sync(&mut element, &mut layout);
        assert_eq!(y.peek(), 520.0);
    });
}

#[test]
fn wrap_partial_rows_capacity_revision_and_metric_recovery() {
    let (_owner, ()) = create_root(|| {
        let handle = VirtualGridHandle::default();
        let count = Rc::new(Cell::new(17));
        let extent = Rc::new(Cell::new(10.0));
        let revision = Rc::new(Cell::new(0));
        let renders = Rc::new(Cell::new(0));
        let mut element: Element = virtual_wrap("wrap-bounds")
            .handle(handle.clone())
            .item_count_fn({
                let count = count.clone();
                move || count.get()
            })
            .minimum_item_width(80.0)
            .item_height_fn({
                let extent = extent.clone();
                move || extent.get()
            })
            .data_key_fn({
                let revision = revision.clone();
                move || revision.get().to_string()
            })
            .item_gap(7.0, 3.0)
            .overscan(0)
            .max_mounted_cells(17)
            .render_item({
                let renders = renders.clone();
                move |i| {
                    renders.set(renders.get() + 1);
                    Element::new().text(i.to_string())
                }
            })
            .into();
        let keys = ScrollViewportSignalKeys::new("wrap-bounds");
        let y = use_signal(&keys.offset_y, || 0.0_f32);
        let ty = use_signal(&keys.target_y, || 0.0_f32);
        let width = use_signal(&keys.viewport_w, || 0.0_f32);
        let height = use_signal(&keys.viewport_h, || 0.0_f32);
        let mut layout = LayoutTree::new();
        assert!(handle.scroll_to_item(16, ListScrollAlign::End, 0.0));
        sync(&mut element, &mut layout);
        assert_eq!(renders.get(), 0);
        width.set_silent(350.0);
        height.set_silent(1000.0);
        sync(&mut element, &mut layout);
        assert_eq!(body(&element, "wrap-bounds__grid_body").children.len(), 17);
        assert_eq!(handle.cell_origin(4, 1), None, "no phantom final-row cell");
        assert_eq!(handle.item_origin(17), None);
        width.set_silent(175.0);
        sync(&mut element, &mut layout);
        assert_eq!(renders.get(), 17, "all existing items survive a reflow");
        revision.set(1);
        sync(&mut element, &mut layout);
        assert_eq!(renders.get(), 34);
        count.set(18);
        sync(&mut element, &mut layout);
        assert_eq!(
            body(&element, "wrap-bounds__grid_body").attrs["data-virtual-grid-error"],
            "Capacity"
        );
        assert!(handle.viewport().is_none());
        height.set_silent(25.0);
        sync(&mut element, &mut layout);
        assert!(handle.viewport().is_some());
        assert!(handle.scroll_to_end());
        sync(&mut element, &mut layout);
        assert_eq!([y.peek(), ty.peek()], [89.0; 2]);
        count.set(3);
        sync(&mut element, &mut layout);
        assert_eq!([y.peek(), ty.peek()], [0.0; 2]);
        count.set(0);
        sync(&mut element, &mut layout);
        let grid = body(&element, "wrap-bounds__grid_body");
        assert!(grid.children.is_empty());
        layout.build_and_layout(&mut element, 175.0, 25.0, [-1.0; 2]);
        assert_eq!(
            body(&element, "wrap-bounds__grid_body").layout_bounds[2],
            175.0
        );
        count.set(17);
        extent.set(f32::NAN);
        sync(&mut element, &mut layout);
        assert_eq!(
            body(&element, "wrap-bounds__grid_body").attrs["data-virtual-grid-error"],
            "InvalidExtent"
        );
        extent.set(20.0);
        sync(&mut element, &mut layout);
        assert_eq!(handle.item_origin(16), Some([0.0, 184.0]));
        assert!(handle.scroll_to_cell(8, 1, [ListScrollAlign::Start; 2], [0.0; 2]));
        sync(&mut element, &mut layout);
        assert_eq!(y.peek(), 0.0, "invalid trailing coordinate is atomic");
        assert!(handle.scroll_to_item(16, ListScrollAlign::End, 0.0));
        sync(&mut element, &mut layout);
        assert_eq!([y.peek(), ty.peek()], [179.0; 2]);
    });
}

#[test]
fn grid_interaction_retention_is_discrete_released_and_bounded() {
    use jadix_components::{
        layout::layout_tree::{ATTR_PORTAL_KEY, hoist_portal_subtrees},
        utils::text_selection::label_selection_signal_key,
    };
    fn body_mut(el: &mut Element) -> &mut Element {
        if el
            .template_id
            .as_deref()
            .is_some_and(|id| id.ends_with("__grid_body"))
        {
            return el;
        }
        el.children
            .iter_mut()
            .find_map(|child| {
                fn find(el: &mut Element) -> Option<&mut Element> {
                    if el
                        .template_id
                        .as_deref()
                        .is_some_and(|id| id.ends_with("__grid_body"))
                    {
                        Some(el)
                    } else {
                        el.children.iter_mut().find_map(find)
                    }
                }
                find(child)
            })
            .unwrap()
    }
    fn mounted(el: &mut Element) -> Vec<String> {
        let mut keys: Vec<_> = body_mut(el)
            .children
            .iter()
            .filter_map(|c| c.attrs.get("__jadix_vgrid_cell").cloned())
            .collect();
        keys.sort();
        keys
    }
    let (_owner, ()) = create_root(|| {
        for capacity in [7, 4] {
            let id = format!("grid-interaction-{capacity}");
            let focused_id = format!("{id}-focus");
            let label_id = format!("{id}-label");
            let focus = use_signal("__jadix_button_focus_id", String::new);
            focus.set_silent(focused_id.clone());
            let selection = use_signal(&label_selection_signal_key(&label_id), || {
                None::<(usize, usize)>
            });
            selection.set_silent(Some((3, 0)));
            let revision = Rc::new(Cell::new(0));
            let calls = Rc::new(Cell::new(0));
            let reports = Rc::new(Cell::new(0));
            let mut el: Element = virtual_grid(&id)
                .row_count(100_000)
                .column_count(100_000)
                .cell_size(10.0, 10.0)
                .overscan(0, 0)
                .height(20.0)
                .max_mounted_cells(capacity)
                .data_key_fn({
                    let revision = revision.clone();
                    move || revision.get().to_string()
                })
                .on_viewport_change({
                    let reports = reports.clone();
                    move |_| reports.set(reports.get() + 1)
                })
                .render_cell({
                    let calls = calls.clone();
                    let focused_id = focused_id.clone();
                    let label_id = label_id.clone();
                    let portal_key = format!("{id}-portal");
                    move |r, c| {
                        calls.set(calls.get() + 1);
                        match (r, c) {
                            (0, 0) => Element::new().focus_id(&focused_id),
                            (0, 1) => Element::new()
                                .focus_id(&label_id)
                                .text("中x")
                                .selectable(true),
                            (1, 0) => {
                                let mut portal = Element::new().absolute().w(1.0).h(1.0);
                                portal.set_attr("jadix_portal", "1");
                                portal.set_attr(ATTR_PORTAL_KEY, &portal_key);
                                portal
                            }
                            _ => Element::new(),
                        }
                    }
                })
                .into();
            let keys = ScrollViewportSignalKeys::new(&id);
            let x = use_signal(&keys.offset_x, || 0.0_f32);
            let y = use_signal(&keys.offset_y, || 0.0_f32);
            use_signal(&keys.viewport_w, || 0.0_f32).set_silent(20.0);
            use_signal(&keys.viewport_h, || 0.0_f32).set_silent(20.0);
            let mut layout = LayoutTree::new();
            sync(&mut el, &mut layout);
            hoist_portal_subtrees(&mut el);
            layout.build_and_layout(&mut el, 20.0, 20.0, [-1.0; 2]);
            let original = body_mut(&mut el)
                .children
                .iter()
                .find(|c| c.attrs.get("__jadix_vgrid_cell").map(String::as_str) == Some("0:0"))
                .unwrap()
                .layout_node_id();
            x.set_silent(50.0);
            y.set_silent(50.0);
            sync(&mut el, &mut layout);
            let visible = vec!["5:5", "5:6", "6:5", "6:6"];
            let expected = if capacity == 7 {
                vec!["0:0", "0:1", "1:0", "5:5", "5:6", "6:5", "6:6"]
            } else {
                visible.clone()
            };
            assert_eq!(mounted(&mut el), expected, "{id}");
            assert_eq!(
                calls.get(),
                8,
                "no gap-sized rendering or extra render for retained owners"
            );
            assert_eq!([x.peek(), y.peek()], [50.0; 2]);
            let before_reports = reports.get();
            if capacity == 7 {
                assert_eq!(
                    body_mut(&mut el)
                        .children
                        .iter()
                        .find(|c| c.attrs.get("__jadix_vgrid_cell").map(String::as_str)
                            == Some("0:0"))
                        .unwrap()
                        .layout_node_id(),
                    original
                );
                focus.set_silent(String::new());
                sync(&mut el, &mut layout);
                assert!(!mounted(&mut el).contains(&"0:0".into()));
                selection.set_silent(None);
                sync(&mut el, &mut layout);
                assert!(!mounted(&mut el).contains(&"0:1".into()));
                body_mut(&mut el)
                    .children
                    .iter_mut()
                    .find(|c| c.attrs.get("__jadix_vgrid_cell").map(String::as_str) == Some("1:0"))
                    .unwrap()
                    .children
                    .clear();
                sync(&mut el, &mut layout);
                assert_eq!(mounted(&mut el), visible);
                assert_eq!(
                    reports.get(),
                    before_reports,
                    "retention is not viewport visibility"
                );
                x.set_silent(0.0);
                y.set_silent(0.0);
                focus.set_silent(focused_id);
                sync(&mut el, &mut layout);
                x.set_silent(50.0);
                y.set_silent(50.0);
                sync(&mut el, &mut layout);
                assert!(mounted(&mut el).contains(&"0:0".into()));
                revision.set(1);
                sync(&mut el, &mut layout);
                assert_eq!(
                    mounted(&mut el),
                    visible,
                    "coordinate dataset replacement releases stale ownership"
                );
            }
            use_signal(&keys.viewport_w, || 0.0_f32).set_silent(0.0);
            sync(&mut el, &mut layout);
            assert!(
                mounted(&mut el).is_empty(),
                "hidden geometry does not accumulate retained cells"
            );
        }
    });
}
