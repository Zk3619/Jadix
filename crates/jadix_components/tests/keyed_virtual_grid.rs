use jadix_components::{
    components::{VirtualGridHandle, virtual_grid},
    core::{
        reactivity::{create_root, use_signal},
        scroll_viewport::{
            ScrollViewportSignalKeys, activate_scroll_viewport, scroll_content_h_signal_key,
            scroll_content_w_signal_key,
        },
        tree::{Element, LayoutStatus, LayoutTree},
    },
    virtualization::ListScrollAlign,
};
use std::{
    cell::{Cell, RefCell},
    collections::HashSet,
    rc::Rc,
};

fn body(element: &mut Element) -> &mut Element {
    if element
        .template_id
        .as_deref()
        .is_some_and(|id| id.ends_with("__grid_body"))
    {
        return element;
    }
    element
        .children
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
fn sync(element: &mut Element, layout: &mut LayoutTree) -> LayoutStatus {
    let root = body(element);
    root.reactive_sync.clone().unwrap().run(root, layout)
}
fn cell<'a>(element: &'a mut Element, content: &str) -> &'a mut Element {
    body(element)
        .children
        .iter_mut()
        .find(|el| el.children[0].attrs["content"] == content)
        .unwrap()
}

#[test]
fn keyed_grid_keeps_axis_identity_reading_position_and_interaction_in_a_bounded_window() {
    let (_owner, ()) = create_root(|| {
        let rows = Rc::new(RefCell::new((0..10_000).collect::<Vec<usize>>()));
        let columns = Rc::new(RefCell::new((0..10_000).collect::<Vec<usize>>()));
        let revision = Rc::new(Cell::new(0));
        let reads = Rc::new(Cell::new(0));
        let renders = Rc::new(Cell::new(0));
        let handle = VirtualGridHandle::default();
        let mut element: Element = virtual_grid("keyed-sheet")
            .handle(handle.clone())
            .row_count_fn({
                let rows = rows.clone();
                move || rows.borrow().len()
            })
            .column_count_fn({
                let columns = columns.clone();
                move || columns.borrow().len()
            })
            .row_key_fn({
                let rows = rows.clone();
                let reads = reads.clone();
                move |i| {
                    reads.set(reads.get() + 1);
                    rows.borrow()[i].to_string()
                }
            })
            .column_key_fn({
                let columns = columns.clone();
                let reads = reads.clone();
                move |i| {
                    reads.set(reads.get() + 1);
                    columns.borrow()[i].to_string()
                }
            })
            .data_key_fn({
                let revision = revision.clone();
                move || revision.get().to_string()
            })
            .cell_size(20.0, 10.0)
            .cell_gap(5.0, 3.0)
            .overscan(0, 0)
            .max_mounted_cells(12)
            .render_cell({
                let rows = rows.clone();
                let columns = columns.clone();
                let renders = renders.clone();
                move |r, c| {
                    renders.set(renders.get() + 1);
                    let value = format!("{}:{}", rows.borrow()[r], columns.borrow()[c]);
                    let mut content = Element::new().focus_id(format!("sheet-{value}"));
                    content.set_attr("content", value);
                    content.set_attr("position", format!("{r}:{c}"));
                    content
                }
            })
            .into();
        let keys = ScrollViewportSignalKeys::new("keyed-sheet");
        let x = use_signal(&keys.offset_x, || 0.0_f32);
        let y = use_signal(&keys.offset_y, || 0.0_f32);
        let tx = use_signal(&keys.target_x, || 0.0_f32);
        let ty = use_signal(&keys.target_y, || 0.0_f32);
        use_signal(&keys.viewport_w, || 0.0_f32).set_silent(70.0);
        use_signal(&keys.viewport_h, || 0.0_f32).set_silent(30.0);
        x.set_silent(253.0);
        y.set_silent(262.0);
        let mut layout = LayoutTree::new();
        sync(&mut element, &mut layout);
        assert_eq!(
            reads.get(),
            20_000,
            "index 100 million cells using only axis keys"
        );
        assert_eq!(renders.get(), 9);
        cell(&mut element, "20:10").set_attr("retained", "yes");
        let identity = cell(&mut element, "20:10").template_id.clone();
        for _ in 0..4 {
            assert_eq!(sync(&mut element, &mut layout), LayoutStatus::Clean);
        }
        assert_eq!(reads.get(), 20_000);
        rows.borrow_mut().splice(0..0, [10_000, 10_001]);
        columns.borrow_mut().insert(0, 10_000);
        sync(&mut element, &mut layout);
        assert_eq!(
            [x.peek(), y.peek(), tx.peek(), ty.peek()],
            [278.0, 288.0, 278.0, 288.0]
        );
        assert_eq!(
            cell(&mut element, "20:10").children[0].attrs["position"],
            "22:11"
        );
        assert_eq!(cell(&mut element, "20:10").attrs["retained"], "yes");
        rows.borrow_mut().swap(22, 40);
        columns.borrow_mut().swap(11, 30);
        revision.set(1);
        sync(&mut element, &mut layout);
        assert_eq!([x.peek(), y.peek()], [753.0, 522.0]);
        assert_eq!(cell(&mut element, "20:10").template_id, identity);
        assert_eq!(
            cell(&mut element, "20:10").children[0].attrs["position"],
            "40:30"
        );
        let read_count = reads.get();
        let focus = use_signal("__jadix_button_focus_id", String::new);
        focus.set_silent("sheet-20:10".into());
        x.set_silent(2500.0);
        y.set_silent(1300.0);
        sync(&mut element, &mut layout);
        assert_eq!(reads.get(), read_count, "scroll does not scan either axis");
        assert_eq!(body(&mut element).children.len(), 10);
        assert_eq!(cell(&mut element, "20:10").attrs["retained"], "yes");
        rows.borrow_mut().swap(40, 500);
        columns.borrow_mut().swap(30, 600);
        revision.set(2);
        sync(&mut element, &mut layout);
        assert_eq!([x.peek(), y.peek()], [2500.0, 1300.0]);
        assert_eq!(cell(&mut element, "20:10").template_id, identity);
        assert_eq!(
            cell(&mut element, "20:10").children[0].attrs["position"],
            "500:600"
        );
        assert!(body(&mut element).children.len() <= 12);
        assert!(!handle.viewport().unwrap().rows.visible.contains(&500));
        focus.set_silent(String::new());
        sync(&mut element, &mut layout);
        assert_eq!(body(&mut element).children.len(), 9);
        x.set_silent(15003.0);
        y.set_silent(6502.0);
        sync(&mut element, &mut layout);
        rows.borrow_mut().remove(500);
        columns.borrow_mut().remove(600);
        sync(&mut element, &mut layout);
        assert_eq!(
            [x.peek(), y.peek()],
            [15000.0, 6500.0],
            "deleted anchors choose the next old survivors at their starts"
        );
        rows.borrow_mut().insert(0, 20_000);
        columns.borrow_mut().insert(0, 20_000);
        assert!(handle.scroll_to_cell(7, 8, [ListScrollAlign::Start; 2], [0.0; 2]));
        sync(&mut element, &mut layout);
        assert_eq!(
            [x.peek(), y.peek()],
            [200.0, 91.0],
            "explicit navigation overrides restoration atomically"
        );
        layout.build_and_layout(&mut element, 70.0, 30.0, [-1.0; 2]);
        let grid = body(&mut element);
        for cell in &grid.children {
            let (r, c) = cell.attrs["__jadix_vgrid_index"].split_once(':').unwrap();
            let origin = handle
                .cell_origin(r.parse().unwrap(), c.parse().unwrap())
                .unwrap();
            assert!(
                (cell.layout_bounds[0] - grid.layout_bounds[0] - origin[0] as f32).abs() < 0.001
            );
            assert!(
                (cell.layout_bounds[1] - grid.layout_bounds[1] - origin[1] as f32).abs() < 0.001
            );
        }
    });
}

#[test]
fn keyed_grid_partial_axes_unicode_hidden_policy_and_invalid_dataset_recovery() {
    let (_owner, ()) = create_root(|| {
        for keyed_axis in [0, 1] {
            for maintain in [false, true] {
                let id = format!("partial-sheet-{keyed_axis}-{maintain}");
                let data = Rc::new(RefCell::new(vec![
                    String::new(),
                    ":".into(),
                    "::".into(),
                    "🦀:中".into(),
                    "a".into(),
                    "b".into(),
                    "c".into(),
                ]));
                let revision = Rc::new(Cell::new(0));
                let handle = VirtualGridHandle::default();
                let mut builder = virtual_grid(&id)
                    .handle(handle.clone())
                    .cell_size(10.0, 10.0)
                    .overscan(0, 0)
                    .max_mounted_cells(9)
                    .maintain_visible_content_position(maintain)
                    .data_key_fn({
                        let revision = revision.clone();
                        move || revision.get().to_string()
                    })
                    .row_count_fn({
                        let data = data.clone();
                        move || data.borrow().len()
                    })
                    .column_count_fn({
                        let data = data.clone();
                        move || data.borrow().len()
                    })
                    .render_cell(|r, c| {
                        let mut el = Element::new();
                        el.set_attr("content", format!("{r}:{c}"));
                        el
                    });
                let key = {
                    let data = data.clone();
                    move |i: usize| data.borrow()[i].clone()
                };
                builder = if keyed_axis == 0 {
                    builder.column_key_fn(key)
                } else {
                    builder.row_key_fn(key)
                };
                let mut element: Element = builder.into();
                let keys = ScrollViewportSignalKeys::new(&id);
                let x = use_signal(&keys.offset_x, || 0.0_f32);
                let y = use_signal(&keys.offset_y, || 0.0_f32);
                let width = use_signal(&keys.viewport_w, || 0.0_f32);
                let height = use_signal(&keys.viewport_h, || 0.0_f32);
                width.set_silent(30.0);
                height.set_silent(30.0);
                let mut layout = LayoutTree::new();
                sync(&mut element, &mut layout);
                assert_eq!(
                    body(&mut element)
                        .children
                        .iter()
                        .map(|e| e.template_id.clone())
                        .collect::<HashSet<_>>()
                        .len(),
                    9
                );
                let initial = if keyed_axis == 0 { "1:0" } else { "0:1" };
                let identity = cell(&mut element, initial).template_id.clone();
                cell(&mut element, initial).set_attr("retained", "yes");
                data.borrow_mut().swap(0, 1);
                revision.set(1);
                sync(&mut element, &mut layout);
                let coordinate = "1:1";
                assert_eq!(cell(&mut element, coordinate).template_id, identity);
                assert_eq!(cell(&mut element, coordinate).attrs["retained"], "yes");
                assert_eq!(
                    [x.peek(), y.peek()][keyed_axis],
                    if maintain { 10.0 } else { 0.0 }
                );
                // Duplicate keys outside the mounted rectangle still fail closed.
                let duplicate = data.borrow()[5].clone();
                data.borrow_mut()[6] = duplicate;
                revision.set(2);
                sync(&mut element, &mut layout);
                assert_eq!(
                    body(&mut element).attrs["data-virtual-grid-error"],
                    "DuplicateKey"
                );
                assert!(body(&mut element).children.is_empty());
                assert!(handle.viewport().is_none());
                data.borrow_mut()[6] = "repaired".into();
                revision.set(3);
                sync(&mut element, &mut layout);
                assert!(handle.viewport().is_some());
                assert!(
                    !body(&mut element)
                        .attrs
                        .contains_key("data-virtual-grid-error")
                );
                width.set_silent(40.0);
                height.set_silent(40.0);
                sync(&mut element, &mut layout);
                assert_eq!(
                    body(&mut element).attrs["data-virtual-grid-error"],
                    "Capacity"
                );
                width.set_silent(20.0);
                height.set_silent(20.0);
                sync(&mut element, &mut layout);
                assert!(handle.viewport().is_some());
                width.set_silent(0.0);
                assert!(handle.scroll_to_cell(4, 4, [ListScrollAlign::Start; 2], [0.0; 2]));
                data.borrow_mut().swap(0, 2);
                revision.set(4);
                let before = [x.peek(), y.peek()];
                sync(&mut element, &mut layout);
                assert!(body(&mut element).children.is_empty());
                assert_eq!(
                    [x.peek(), y.peek()],
                    before,
                    "hidden key edits do not restore a scroll owner"
                );
                width.set_silent(20.0);
                sync(&mut element, &mut layout);
                assert_eq!([x.peek(), y.peek()], [40.0; 2]);
                data.borrow_mut().clear();
                sync(&mut element, &mut layout);
                assert!(body(&mut element).children.is_empty());
                assert_eq!([x.peek(), y.peek()], [0.0; 2]);
                data.borrow_mut().extend(["one".into(), "two".into()]);
                sync(&mut element, &mut layout);
                assert_eq!(body(&mut element).children.len(), 4);
            }
        }
        // Business identity composes with a real registered parent owner.
        let data = Rc::new(RefCell::new((0..10).collect::<Vec<usize>>()));
        let revision = Rc::new(Cell::new(0));
        let grid: Element = virtual_grid("parent-keyed-grid")
            .use_parent_scroll()
            .row_count(10)
            .column_count(10)
            .cell_size(10.0, 10.0)
            .overscan(0, 0)
            .max_mounted_cells(4)
            .row_key_fn({
                let data = data.clone();
                move |i| data.borrow()[i].to_string()
            })
            .column_key_fn({
                let data = data.clone();
                move |i| data.borrow()[i].to_string()
            })
            .data_key_fn({
                let revision = revision.clone();
                move || revision.get().to_string()
            })
            .render_cell({
                let data = data.clone();
                move |r, c| {
                    let mut el = Element::new();
                    el.set_attr(
                        "content",
                        format!("{}:{}", data.borrow()[r], data.borrow()[c]),
                    );
                    el
                }
            })
            .into();
        let mut root = activate_scroll_viewport(
            Element::new().w(20.0).h(20.0).child(grid),
            "keyed-grid-owner",
            "auto",
            "auto",
            "auto",
            "auto",
        );
        let mut layout = LayoutTree::new();
        layout.build_and_layout(&mut root, 20.0, 20.0, [-1.0; 2]);
        let keys = ScrollViewportSignalKeys::new("keyed-grid-owner");
        let x = use_signal(&keys.offset_x, || 0.0_f32);
        let y = use_signal(&keys.offset_y, || 0.0_f32);
        x.set_silent(10.0);
        y.set_silent(10.0);
        use_signal(&keys.viewport_w, || 0.0_f32).set_silent(20.0);
        use_signal(&keys.viewport_h, || 0.0_f32).set_silent(20.0);
        use_signal(&scroll_content_w_signal_key("keyed-grid-owner"), || 0.0_f32).set_silent(100.0);
        use_signal(&scroll_content_h_signal_key("keyed-grid-owner"), || 0.0_f32).set_silent(100.0);
        sync(&mut root, &mut layout);
        let identity = cell(&mut root, "1:1").template_id.clone();
        cell(&mut root, "1:1").set_attr("retained", "parent");
        data.borrow_mut().swap(1, 2);
        revision.set(1);
        sync(&mut root, &mut layout);
        assert_eq!(
            [x.peek(), y.peek()],
            [10.0; 2],
            "keyed edits preserve parent offsets"
        );
        assert_eq!(cell(&mut root, "1:1").template_id, identity);
        assert_eq!(cell(&mut root, "1:1").attrs["retained"], "parent");
        assert_eq!(body(&mut root).children.len(), 4);
    });
}
