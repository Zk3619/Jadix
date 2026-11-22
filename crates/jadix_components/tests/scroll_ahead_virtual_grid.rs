use jadix_components::{
    components::{virtual_grid, virtual_wrap, VirtualGridHandle},
    core::{
        reactivity::{create_root, use_signal},
        scroll_viewport::ScrollViewportSignalKeys,
        tree::{find_in_tree, Element, LayoutStatus, LayoutTree},
    },
    virtualization::ListScrollAlign,
};
use std::{cell::Cell, ops::Range, rc::Rc};

fn sync(element: &mut Element, layout: &mut LayoutTree) -> LayoutStatus {
    let mut status = element
        .reactive_sync
        .clone()
        .map_or(LayoutStatus::Clean, |f| f.run(element, layout));
    for child in &mut element.children {
        status = status.merge(sync(child, layout));
    }
    status
}

fn body(element: &Element) -> &Element {
    find_in_tree(element, |el| {
        el.template_id.as_deref() == Some("ahead-grid__grid_body")
    })
    .unwrap()
}

fn grid_window(element: &Element, rows: Range<usize>, columns: Range<usize>) {
    let body = body(element);
    assert!(!body.attrs.contains_key("data-virtual-grid-error"));
    let actual: Vec<_> = body
        .children
        .iter()
        .map(|el| el.attrs["__jadix_vgrid_cell"].clone())
        .collect();
    let expected: Vec<_> = rows
        .flat_map(|r| columns.clone().map(move |c| format!("{r}:{c}")))
        .collect();
    assert_eq!(actual, expected);
}

fn wrap_window(element: &Element, items: Range<usize>) {
    let body = body(element);
    assert!(!body.attrs.contains_key("data-virtual-grid-error"));
    let actual: Vec<_> = body
        .children
        .iter()
        .map(|el| el.attrs["__jadix_vgrid_cell"].clone())
        .collect();
    assert_eq!(
        actual,
        items.map(|i| format!("item:{i}")).collect::<Vec<_>>()
    );
}

#[test]
fn grid_predicts_both_axes_reuses_nodes_and_falls_back_without_losing_visible_cells() {
    for enabled in [false, true] {
        for capacity in [36, 128] {
            let (_owner, ()) = create_root(|| {
                let count = Rc::new(Cell::new(10_000));
                let size = Rc::new(Cell::new(20.0));
                let revision = Rc::new(Cell::new(0));
                let renders = Rc::new(Cell::new(0));
                let reports = Rc::new(Cell::new(0));
                let handle = VirtualGridHandle::default();
                let mut element: Element = virtual_grid("ahead-grid")
                    .handle(handle.clone())
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
                        move || [size.get(); 2]
                    })
                    .cell_gap(5.0, 5.0)
                    .overscan(1, 1)
                    .scroll_ahead(enabled)
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
                        let renders = renders.clone();
                        move |_, _| {
                            renders.set(renders.get() + 1);
                            let mut cell = Element::new();
                            cell.set_attr("token", renders.get().to_string());
                            cell
                        }
                    })
                    .into();
                let keys = ScrollViewportSignalKeys::new("ahead-grid");
                let x = use_signal(&keys.offset_x, || 0.0_f32);
                let y = use_signal(&keys.offset_y, || 0.0_f32);
                let width = use_signal(&keys.viewport_w, || 0.0_f32);
                let height = use_signal(&keys.viewport_h, || 0.0_f32);
                let mut layout = LayoutTree::new();
                x.set_silent(1000.0);
                y.set_silent(1000.0);
                width.set_silent(100.0);
                height.set_silent(100.0);
                sync(&mut element, &mut layout);
                grid_window(&element, 39..45, 39..45);
                assert_eq!(
                    renders.get(),
                    36,
                    "100 million cells have bounded initial work"
                );
                let token = body(&element).children[21].children[0].attrs["token"].clone(); // 42:42
                x.set_silent(1050.0);
                y.set_silent(1050.0);
                sync(&mut element, &mut layout);
                let predicted = enabled && capacity >= 81;
                let mounted = if predicted { 41..50 } else { 41..47 };
                grid_window(&element, mounted.clone(), mounted);
                let state = handle.viewport().unwrap();
                assert_eq!(
                    (state.rows.visible, state.columns.visible),
                    (42..46, 42..46)
                );
                let reused = body(&element)
                    .children
                    .iter()
                    .find(|c| c.attrs["__jadix_vgrid_cell"] == "42:42")
                    .unwrap();
                assert_eq!(reused.children[0].attrs["token"], token);
                let before = (renders.get(), reports.get());
                for _ in 0..3 {
                    assert_eq!(sync(&mut element, &mut layout), LayoutStatus::Clean);
                }
                assert_eq!((renders.get(), reports.get()), before);
                x.set_silent(1000.0);
                y.set_silent(1000.0);
                sync(&mut element, &mut layout);
                let mounted = if predicted { 36..45 } else { 39..45 };
                grid_window(&element, mounted.clone(), mounted);

                revision.set(1);
                let callbacks = reports.get();
                sync(&mut element, &mut layout);
                grid_window(&element, 39..45, 39..45);
                assert_eq!(
                    reports.get(),
                    callbacks,
                    "mount prediction never changes actual visibility"
                );
                x.set_silent(1025.0);
                y.set_silent(1025.0);
                sync(&mut element, &mut layout);
                let mounted = if enabled && capacity >= 49 {
                    40..47
                } else {
                    40..46
                };
                grid_window(&element, mounted.clone(), mounted);
                assert!(handle.scroll_to_cell(41, 41, [ListScrollAlign::Start; 2], [0.0; 2]));
                sync(&mut element, &mut layout);
                grid_window(&element, 40..46, 40..46);
                x.set_silent(1050.0);
                y.set_silent(1050.0);
                sync(&mut element, &mut layout);
                assert!(handle.scroll_to_cell(
                    10_000,
                    10_000,
                    [ListScrollAlign::Start; 2],
                    [0.0; 2]
                ));
                sync(&mut element, &mut layout);
                assert_eq!([x.peek(), y.peek()], [1050.0; 2]);
                grid_window(&element, 41..47, 41..47);

                size.set(25.0);
                sync(&mut element, &mut layout);
                grid_window(&element, 34..40, 34..40);
                width.set_silent(75.0);
                height.set_silent(75.0);
                sync(&mut element, &mut layout);
                grid_window(&element, 34..39, 34..39);
                count.set(20);
                sync(&mut element, &mut layout);
                assert_eq!([x.peek(), y.peek()], [520.0; 2]);
                grid_window(&element, 16..20, 16..20);
                count.set(0);
                sync(&mut element, &mut layout);
                grid_window(&element, 0..0, 0..0);
                size.set(f32::NAN);
                sync(&mut element, &mut layout);
                assert_eq!(
                    body(&element).attrs["data-virtual-grid-error"],
                    "InvalidExtent"
                );
                assert!(handle.viewport().is_none());
                count.set(10_000);
                size.set(20.0);
                width.set_silent(100.0);
                height.set_silent(100.0);
                sync(&mut element, &mut layout);
                grid_window(&element, 0..5, 0..5);
                let before = renders.get();
                x.set_silent(100_000.0);
                y.set_silent(100_000.0);
                sync(&mut element, &mut layout);
                grid_window(&element, 3999..4005, 3999..4005);
                assert_eq!(
                    renders.get() - before,
                    36,
                    "long jumps never build a corridor"
                );
            });
        }
    }
}

#[test]
fn wrap_prediction_rebases_for_reflow_navigation_and_hidden_viewports() {
    for enabled in [false, true] {
        for capacity in [32, 64] {
            let (_owner, ()) = create_root(|| {
                let count = Rc::new(Cell::new(100_003));
                let item_height = Rc::new(Cell::new(20.0));
                let revision = Rc::new(Cell::new(0));
                let renders = Rc::new(Cell::new(0));
                let handle = VirtualGridHandle::default();
                let mut element: Element = virtual_wrap("ahead-grid")
                    .handle(handle.clone())
                    .item_count_fn({
                        let count = count.clone();
                        move || count.get()
                    })
                    .item_height_fn({
                        let item_height = item_height.clone();
                        move || item_height.get()
                    })
                    .minimum_item_width(20.0)
                    .item_gap(5.0, 5.0)
                    .overscan(1)
                    .scroll_ahead(enabled)
                    .max_mounted_cells(capacity)
                    .data_key_fn({
                        let revision = revision.clone();
                        move || revision.get().to_string()
                    })
                    .render_item({
                        let renders = renders.clone();
                        move |_| {
                            renders.set(renders.get() + 1);
                            Element::new()
                        }
                    })
                    .into();
                let keys = ScrollViewportSignalKeys::new("ahead-grid");
                let y = use_signal(&keys.offset_y, || 0.0_f32);
                let width = use_signal(&keys.viewport_w, || 0.0_f32);
                let height = use_signal(&keys.viewport_h, || 0.0_f32);
                let mut layout = LayoutTree::new();
                width.set_silent(95.0);
                height.set_silent(100.0);
                y.set_silent(1000.0);
                sync(&mut element, &mut layout);
                wrap_window(&element, 156..180);
                assert_eq!(renders.get(), 24);
                y.set_silent(1050.0);
                sync(&mut element, &mut layout);
                wrap_window(
                    &element,
                    164..if enabled && capacity >= 36 { 200 } else { 188 },
                );
                let state = handle.viewport().unwrap();
                assert_eq!((state.rows.visible, state.columns.visible), (42..46, 0..4));
                let before = renders.get();
                for _ in 0..3 {
                    assert_eq!(sync(&mut element, &mut layout), LayoutStatus::Clean);
                }
                assert_eq!(renders.get(), before);

                width.set_silent(120.0);
                sync(&mut element, &mut layout);
                assert_eq!(y.peek(), 825.0);
                wrap_window(&element, 160..190);
                width.set_silent(70.0);
                sync(&mut element, &mut layout);
                assert_eq!(y.peek(), 1400.0);
                wrap_window(&element, 165..183);
                width.set_silent(95.0);
                sync(&mut element, &mut layout);
                assert_eq!(y.peek(), 1050.0);
                wrap_window(&element, 164..188);
                item_height.set(30.0);
                sync(&mut element, &mut layout);
                assert_eq!(y.peek(), 1470.0);
                wrap_window(&element, 164..184);

                y.set_silent(1505.0);
                sync(&mut element, &mut layout);
                revision.set(1);
                sync(&mut element, &mut layout);
                wrap_window(&element, 168..188);
                y.set_silent(1540.0);
                sync(&mut element, &mut layout);
                assert!(handle.scroll_to_item(176, ListScrollAlign::Start, 0.0));
                sync(&mut element, &mut layout);
                wrap_window(&element, 172..192);
                width.set_silent(0.0);
                sync(&mut element, &mut layout);
                wrap_window(&element, 0..0);
                width.set_silent(95.0);
                sync(&mut element, &mut layout);
                assert!(body(&element).children.len() <= 24);

                count.set(43);
                item_height.set(20.0);
                height.set_silent(50.0);
                y.set_silent(150.0);
                sync(&mut element, &mut layout);
                // Count/metric restoration can anchor elsewhere; explicit navigation
                // establishes the new dataset's row before measuring a scroll step.
                assert!(handle.scroll_to_item(24, ListScrollAlign::Start, 0.0));
                sync(&mut element, &mut layout);
                assert_eq!(y.peek(), 150.0);
                y.set_silent(175.0);
                sync(&mut element, &mut layout);
                wrap_window(&element, 24..if enabled { 43 } else { 40 });
                assert_eq!(handle.viewport().unwrap().rows.visible, 7..9);
                assert!(body(&element).children.len() <= capacity);
            });
        }
    }
}
