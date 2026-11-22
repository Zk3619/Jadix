use jadix_components::{
    components::{VirtualGridHandle, virtual_wrap},
    core::{
        reactivity::{create_root, use_signal},
        scroll_viewport::ScrollViewportSignalKeys,
        tree::{Element, LayoutStatus, LayoutTree},
    },
    virtualization::ListScrollAlign,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

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
                    return Some(el);
                }
                el.children.iter_mut().find_map(find)
            }
            find(child)
        })
        .unwrap()
}
fn cell<'a>(element: &'a mut Element, key: &str) -> &'a mut Element {
    body(element)
        .children
        .iter_mut()
        .find(|el| el.attrs["__jadix_vgrid_cell"] == format!("key:{key}"))
        .unwrap()
}

#[test]
fn keyed_wrap_mutations_reflow_and_navigation_keep_bounded_current_content() {
    let (_owner, ()) = create_root(|| {
        let data = Rc::new(RefCell::new((0..100_000).collect::<Vec<usize>>()));
        let revision = Rc::new(Cell::new(0));
        let reads = Rc::new(Cell::new(0));
        let renders = Rc::new(Cell::new(0));
        let handle = VirtualGridHandle::default();
        let mut element: Element = virtual_wrap("keyed-wrap")
            .handle(handle.clone())
            .minimum_item_width(80.0)
            .item_height(10.0)
            .item_gap(7.0, 3.0)
            .overscan(0)
            .scroll_ahead(true)
            .max_mounted_cells(32)
            .item_count_fn({
                let data = data.clone();
                move || data.borrow().len()
            })
            .data_key_fn({
                let revision = revision.clone();
                move || revision.get().to_string()
            })
            .item_key_fn({
                let data = data.clone();
                let reads = reads.clone();
                move |i| {
                    reads.set(reads.get() + 1);
                    data.borrow()[i].to_string()
                }
            })
            .render_item({
                let data = data.clone();
                let renders = renders.clone();
                move |i| {
                    renders.set(renders.get() + 1);
                    let mut el = Element::new();
                    el.set_attr("content", format!("{}:{i}", data.borrow()[i]));
                    el
                }
            })
            .into();
        let keys = ScrollViewportSignalKeys::new("keyed-wrap");
        let y = use_signal(&keys.offset_y, || 0.0_f32);
        let ty = use_signal(&keys.target_y, || 0.0_f32);
        let width = use_signal(&keys.viewport_w, || 0.0_f32);
        let height = use_signal(&keys.viewport_h, || 0.0_f32);
        width.set_silent(260.0);
        height.set_silent(25.0);
        y.set_silent(68.25);
        let mut layout = LayoutTree::new();
        sync(&mut element, &mut layout);
        assert_eq!(reads.get(), 100_000);
        assert_eq!(renders.get(), 9);
        cell(&mut element, "15").set_attr("retained", "yes");
        let identity = cell(&mut element, "15").template_id.clone();
        for _ in 0..5 {
            assert_eq!(sync(&mut element, &mut layout), LayoutStatus::Clean);
        }
        assert_eq!(reads.get(), 100_000, "no key scan on idle frames");
        data.borrow_mut().splice(0..0, [100_000, 100_001]);
        sync(&mut element, &mut layout);
        assert_eq!(y.peek(), 68.25, "sub-row insertion keeps the same anchor");
        assert_eq!(
            cell(&mut element, "15").children[0].attrs["content"],
            "15:17"
        );
        data.borrow_mut().insert(0, 100_002);
        sync(&mut element, &mut layout);
        assert_eq!([y.peek(), ty.peek()], [81.25; 2]);
        for (w, top) in [(350.0, 52.0), (175.0, 117.0), (260.0, 78.0)] {
            width.set_silent(w);
            sync(&mut element, &mut layout);
            assert_eq!(y.peek(), top + 3.25);
            assert_eq!(cell(&mut element, "15").template_id, identity);
            assert_eq!(cell(&mut element, "15").attrs["retained"], "yes");
            assert!(body(&mut element).children.len() <= 32);
        }
        let before_reads = reads.get();
        y.set_silent(94.25);
        sync(&mut element, &mut layout);
        assert_eq!(reads.get(), before_reads, "scroll does not scan keys");
        // User scroll selected key 18 at index 21. Same-count reorder follows it.
        data.borrow_mut().swap(21, 30);
        revision.set(1);
        sync(&mut element, &mut layout);
        assert_eq!(y.peek(), 133.25);
        assert_eq!(
            cell(&mut element, "18").children[0].attrs["content"],
            "18:30"
        );
        data.borrow_mut().remove(30);
        sync(&mut element, &mut layout);
        assert_eq!(
            y.peek(),
            130.0,
            "deleted anchor uses its next old survivor at its start"
        );
        data.borrow_mut().truncate(30);
        sync(&mut element, &mut layout);
        assert_eq!(
            y.peek(),
            102.0,
            "predecessor fallback clamps to viewport end"
        );
        data.borrow_mut().splice(0..0, [200_000, 200_001, 200_002]);
        assert!(handle.scroll_to_item(6, ListScrollAlign::Start, 0.0));
        sync(&mut element, &mut layout);
        assert_eq!([y.peek(), ty.peek()], [26.0; 2]);
        layout.build_and_layout(&mut element, 260.0, 25.0, [-1.0; 2]);
        let grid = body(&mut element);
        for el in &grid.children {
            let index: usize = el.attrs["__jadix_vgrid_index"].parse().unwrap();
            let origin = handle.item_origin(index).unwrap();
            assert!((el.layout_bounds[1] - grid.layout_bounds[1] - origin[1] as f32).abs() < 0.001);
        }
        *data.borrow_mut() = (300_000..300_033).collect();
        revision.set(2);
        sync(&mut element, &mut layout);
        assert_eq!(y.peek(), 26.0, "no survivors retains pixel position");
        data.borrow_mut().clear();
        sync(&mut element, &mut layout);
        assert_eq!(y.peek(), 0.0);
        assert!(body(&mut element).children.is_empty());
    });
}

#[test]
fn keyed_wrap_policy_unique_encoding_revision_and_error_recovery() {
    let (_owner, ()) = create_root(|| {
        for maintain in [false, true] {
            let id = format!("keyed-policy-{maintain}");
            let data = Rc::new(RefCell::new(
                (0..50).map(|i| format!("item:{i} 🦀")).collect::<Vec<_>>(),
            ));
            data.borrow_mut()[0] = String::new();
            let revision = Rc::new(Cell::new(0));
            let handle = VirtualGridHandle::default();
            let mut element: Element = virtual_wrap(&id)
                .handle(handle.clone())
                .minimum_item_width(80.0)
                .item_height(10.0)
                .item_gap(7.0, 3.0)
                .overscan(0)
                .maintain_visible_content_position(maintain)
                .max_mounted_cells(20)
                .item_count_fn({
                    let data = data.clone();
                    move || data.borrow().len()
                })
                .item_key_fn({
                    let data = data.clone();
                    move |i| data.borrow()[i].clone()
                })
                .data_key_fn({
                    let revision = revision.clone();
                    move || revision.get().to_string()
                })
                .render_item({
                    let revision = revision.clone();
                    move |i| {
                        let mut el = Element::new();
                        el.set_attr("content", format!("{i}:{}", revision.get()));
                        el
                    }
                })
                .into();
            let keys = ScrollViewportSignalKeys::new(&id);
            let y = use_signal(&keys.offset_y, || 0.0_f32);
            let width = use_signal(&keys.viewport_w, || 0.0_f32);
            let height = use_signal(&keys.viewport_h, || 0.0_f32);
            width.set_silent(260.0);
            height.set_silent(25.0);
            let mut layout = LayoutTree::new();
            sync(&mut element, &mut layout);
            let ids: std::collections::HashSet<_> = body(&mut element)
                .children
                .iter()
                .map(|e| e.template_id.clone())
                .collect();
            assert_eq!(body(&mut element).children.len(), 6);
            assert_eq!(ids.len(), 6, "empty, delimiter and unicode keys are unique");
            assert!(ids.contains(&Some(format!("{id}__key_"))));
            y.set_silent(68.25);
            sync(&mut element, &mut layout);
            cell(&mut element, "item:15 🦀").set_attr("retained", "yes");
            revision.set(1);
            sync(&mut element, &mut layout);
            assert_eq!(
                cell(&mut element, "item:15 🦀").children[0].attrs["content"],
                "15:1"
            );
            assert_eq!(cell(&mut element, "item:15 🦀").attrs["retained"], "yes");
            data.borrow_mut()
                .splice(0..0, ["a".into(), "b".into(), "c".into()]);
            sync(&mut element, &mut layout);
            assert_eq!(y.peek(), if maintain { 81.25 } else { 68.25 });
            data.borrow_mut().swap(50, 51);
            revision.set(2);
            width.set_silent(350.0);
            sync(&mut element, &mut layout);
            assert_eq!(y.peek(), if maintain { 55.25 } else { 42.25 });
            width.set_silent(260.0);
            sync(&mut element, &mut layout);
            assert_eq!(y.peek(), if maintain { 81.25 } else { 68.25 });
            let duplicate = data.borrow()[51].clone();
            data.borrow_mut()[52] = duplicate;
            revision.set(3);
            sync(&mut element, &mut layout);
            assert_eq!(
                body(&mut element).attrs["data-virtual-grid-error"],
                "DuplicateKey"
            );
            assert!(body(&mut element).children.is_empty());
            assert!(handle.viewport().is_none());
            assert!(handle.scroll_to_item(3, ListScrollAlign::Start, 0.0));
            data.borrow_mut()[52] = "fixed".into();
            revision.set(4);
            sync(&mut element, &mut layout);
            assert_eq!(y.peek(), 13.0);
            assert!(
                !body(&mut element)
                    .attrs
                    .contains_key("data-virtual-grid-error")
            );
            height.set_silent(1000.0);
            sync(&mut element, &mut layout);
            assert_eq!(
                body(&mut element).attrs["data-virtual-grid-error"],
                "Capacity"
            );
            height.set_silent(25.0);
            sync(&mut element, &mut layout);
            assert!(handle.viewport().is_some());
            assert!(body(&mut element).children.len() <= 20);
        }
    });
}

#[test]
fn keyed_wrap_retains_interactions_through_edits_reflow_and_release() {
    use jadix_components::utils::text_selection::label_selection_signal_key;
    let (_owner, ()) = create_root(|| {
        let data = Rc::new(RefCell::new((0..20).collect::<Vec<_>>()));
        let revision = Rc::new(Cell::new(0));
        let renders = Rc::new(Cell::new(0));
        let focus = use_signal("__jadix_button_focus_id", String::new);
        focus.set_silent("wrap-owner-0".into());
        let selection = use_signal(&label_selection_signal_key("wrap-owner-1"), || {
            None::<(usize, usize)>
        });
        selection.set_silent(Some((0, 3)));
        let mut el: Element = virtual_wrap("wrap-owners")
            .minimum_item_width(20.0)
            .item_height(10.0)
            .height(20.0)
            .overscan(0)
            .max_mounted_cells(8)
            .maintain_visible_content_position(false)
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
            .render_item({
                let data = data.clone();
                let revision = revision.clone();
                let renders = renders.clone();
                move |i| {
                    renders.set(renders.get() + 1);
                    let key = data.borrow()[i];
                    let mut child = Element::new()
                        .focus_id(format!("wrap-owner-{key}"))
                        .text(format!("中 {key} {}", revision.get()))
                        .selectable(true);
                    if key == 2 {
                        child.set_attr("jadix_portal", "1");
                    }
                    child
                }
            })
            .into();
        let keys = ScrollViewportSignalKeys::new("wrap-owners");
        let y = use_signal(&keys.offset_y, || 0.0_f32);
        let width = use_signal(&keys.viewport_w, || 0.0_f32);
        width.set_silent(40.0);
        use_signal(&keys.viewport_h, || 0.0_f32).set_silent(20.0);
        let mut layout = LayoutTree::new();
        sync(&mut el, &mut layout);
        cell(&mut el, "0").set_attr("state", "alive");
        let identity = cell(&mut el, "0").template_id.clone();
        y.set_silent(50.0);
        sync(&mut el, &mut layout);
        assert_eq!(renders.get(), 8);
        for key in ["0", "1", "2"] {
            assert!(body(&mut el).children.iter().any(|c| {
                c.attrs
                    .get("__jadix_vgrid_cell")
                    .is_some_and(|v| v == &format!("key:{key}"))
            }));
        }
        assert_eq!(cell(&mut el, "0").attrs["state"], "alive");
        assert_eq!(cell(&mut el, "0").template_id, identity);
        data.borrow_mut().rotate_left(3);
        revision.set(1);
        sync(&mut el, &mut layout);
        assert_eq!(y.peek(), 50.0);
        assert_eq!(cell(&mut el, "0").attrs["__jadix_vgrid_index"], "17");
        assert_eq!(cell(&mut el, "0").attrs["state"], "alive");
        assert_eq!(
            cell(&mut el, "1").children[0].text.as_deref(),
            Some("中 1 1")
        );
        width.set_silent(20.0);
        sync(&mut el, &mut layout);
        assert_eq!(cell(&mut el, "0").template_id, identity);
        layout.build_and_layout(&mut el, 20.0, 20.0, [-1.0; 2]);
        assert_eq!(
            cell(&mut el, "0").layout_bounds[1] - body(&mut el).layout_bounds[1],
            170.0
        );
        assert!(body(&mut el).children.len() <= 8);
        data.borrow_mut().retain(|k| *k != 0);
        revision.set(2);
        sync(&mut el, &mut layout);
        assert!(
            !body(&mut el).children.iter().any(|c| c
                .attrs
                .get("__jadix_vgrid_cell")
                .map(String::as_str)
                == Some("key:0"))
        );
        assert_eq!(
            cell(&mut el, "1").children[0].text.as_deref(),
            Some("中 1 2")
        );
        focus.set_silent(String::new());
        selection.set_silent(None);
        cell(&mut el, "2").children.clear();
        let offset = y.peek();
        sync(&mut el, &mut layout);
        assert_eq!(body(&mut el).children.len(), 2);
        assert_eq!(y.peek(), offset);
        width.set_silent(0.0);
        sync(&mut el, &mut layout);
        assert!(body(&mut el).children.is_empty());
    });
}
