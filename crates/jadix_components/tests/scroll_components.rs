use jadix_components::components::{
    ScrollViewportBuilder, ScrollbarBuilder, ScrollbarOrientation, scroll_viewport, scrollbar,
};
use jadix_components::core::reactivity::{create_root, use_signal};
use jadix_components::core::scroll_viewport::{
    activate_scroll_viewport, scroll_behavior_signal_key, scroll_content_h_signal_key,
    scroll_viewport_h_signal_key,
};
use jadix_components::core::scrollbar::scrollbar_visibility_signal_key;
use jadix_components::core::tree::Element;

#[test]
fn horizontal_lists_wait_for_width_follow_end_and_reveal_focus_without_y_navigation() {
    use jadix_components::components::virtual_list::{VirtualListHandle, virtual_list};
    use jadix_components::core::{scroll_viewport::ScrollViewportSignalKeys, tree::LayoutTree};
    use std::{cell::Cell, rc::Rc};
    fn sync(el: &mut Element, layout: &mut LayoutTree) {
        if let Some(callback) = el.reactive_sync.clone() {
            callback.run(el, layout);
        }
        for child in &mut el.children {
            sync(child, layout);
        }
    }
    let (_owner, ()) = create_root(|| {
        for measured in [false, true] {
            let id = format!("horizontal-focus-{measured}");
            let count = Rc::new(Cell::new(100));
            let focus = Rc::new(Cell::new(None));
            let extent = Rc::new(Cell::new(20.0));
            let renders = Rc::new(Cell::new(0));
            let handle = VirtualListHandle::default();
            let mut builder = virtual_list(&id)
                .horizontal()
                .handle(handle.clone())
                .item_count_fn({
                    let count = count.clone();
                    move || count.get()
                })
                .focused_index_fn({
                    let focus = focus.clone();
                    move || focus.get()
                })
                .item_extent_fn({
                    let extent = extent.clone();
                    move || extent.get()
                })
                .height(40.0)
                .overscan(0)
                .initial_scroll_at_end()
                .follow_end(true)
                .render_item({
                    let renders = renders.clone();
                    move |_| {
                        renders.set(renders.get() + 1);
                        Element::new().w(20.0)
                    }
                });
            if measured {
                builder = builder.measure_rows();
            }
            let mut root: Element = builder.into();
            let keys = ScrollViewportSignalKeys::new(&id);
            let x = use_signal(&keys.offset_x, || 0.0_f32);
            let y = use_signal(&keys.offset_y, || 0.0_f32);
            let width = use_signal(&keys.viewport_w, || 0.0_f32);
            let mut layout = LayoutTree::new();
            sync(&mut root, &mut layout);
            assert!(handle.viewport().is_none());
            width.set_silent(120.0);
            sync(&mut root, &mut layout);
            assert_eq!(x.peek(), 1880.0, "use actual width, not viewport height");
            count.set(101);
            sync(&mut root, &mut layout);
            assert_eq!(x.peek(), 1900.0);
            focus.set(Some(10));
            sync(&mut root, &mut layout);
            assert_eq!(x.peek(), 200.0);
            assert!(handle.viewport().unwrap().visible.contains(&10));
            focus.set(None);
            count.set(102);
            sync(&mut root, &mut layout);
            assert_eq!(x.peek(), 200.0, "focus supersedes End ownership");
            extent.set(40.0);
            sync(&mut root, &mut layout);
            assert_eq!(
                x.peek(),
                400.0,
                "retain the anchored item after estimate changes"
            );
            assert_eq!(y.peek(), 0.0, "horizontal navigation leaves Y alone");
            assert!(
                renders.get() < 35,
                "no intervening windows are materialized"
            );
        }
    });
}

#[test]
fn spaced_virtual_lists_share_navigation_measurement_and_visible_geometry() {
    use jadix_components::components::virtual_list::{
        ListScrollAlign, VirtualListHandle, virtual_list,
    };
    use jadix_components::core::{scroll_viewport::ScrollViewportSignalKeys, tree::LayoutTree};
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };
    fn sync(el: &mut Element, layout: &mut LayoutTree) {
        if let Some(callback) = el.reactive_sync.clone() {
            callback.run(el, layout);
        }
        for child in &mut el.children {
            sync(child, layout);
        }
    }
    fn measure(el: &mut Element, index: usize, height: f32, dimension: usize) -> bool {
        if el.attrs.get("__jadix_vlist_row_key") == Some(&format!("i:{index}")) {
            el.layout_bounds[dimension] = height;
            return true;
        }
        el.children
            .iter_mut()
            .any(|child| measure(child, index, height, dimension))
    }
    let (_owner, ()) = create_root(|| {
        for horizontal in [false, true] {
            for measured in [false, true] {
                let id = format!("spacing-{horizontal}-{measured}");
                let count = Rc::new(Cell::new(100_000));
                let renders = Rc::new(Cell::new(0));
                let position = Rc::new(RefCell::new(None));
                let handle = VirtualListHandle::default();
                let mut builder = virtual_list(&id)
                    .handle(handle.clone())
                    .item_count_fn({
                        let count = count.clone();
                        move || count.get()
                    })
                    .item_extent(20.0)
                    .item_gap(10.0)
                    .item_gap(f32::NAN)
                    .item_gap(-1.0)
                    .height(80.0)
                    .overscan(0)
                    .render_item({
                        let renders = renders.clone();
                        move |_| {
                            renders.set(renders.get() + 1);
                            if horizontal {
                                Element::new().w(20.0)
                            } else {
                                Element::new().h(20.0)
                            }
                        }
                    })
                    .on_scroll_position({
                        let position = position.clone();
                        move |value| *position.borrow_mut() = value
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
                    || 80.0_f32,
                );
                viewport.set_silent(80.0);
                let mut layout = LayoutTree::new();
                assert_eq!(handle.row_offset(100_000), Some(2_999_990.0));
                for (align, expected) in [
                    (ListScrollAlign::Start, 300.0),
                    (ListScrollAlign::Center, 270.0),
                    (ListScrollAlign::End, 240.0),
                ] {
                    handle.scroll_to_index(10, align, 0.0);
                    sync(&mut root, &mut layout);
                    assert_eq!(offset.peek(), expected);
                }
                offset.set_silent(25.0);
                viewport.set_silent(3.0);
                sync(&mut root, &mut layout);
                assert!(handle.viewport().unwrap().visible.is_empty());
                assert_eq!(*position.borrow(), None);
                viewport.set_silent(10.0);
                sync(&mut root, &mut layout);
                assert_eq!(handle.viewport().unwrap().visible, 1..2);
                assert_eq!(position.borrow().unwrap().offset, 0.0);
                viewport.set_silent(80.0);
                handle.scroll_to_index(10, ListScrollAlign::Center, 0.0);
                sync(&mut root, &mut layout);
                if measured {
                    assert!(measure(&mut root, 10, 60.0, if horizontal { 2 } else { 3 }));
                    sync(&mut root, &mut layout);
                    assert_eq!(offset.peek(), 290.0);
                    assert_eq!(handle.row_offset(11), Some(370.0));
                    // Reflow invalidates measured widths on a height change (and vice versa).
                    let cross = if horizontal {
                        &keys.viewport_h
                    } else {
                        &keys.viewport_w
                    };
                    use_signal(cross, || 0.0_f32).set_silent(120.0);
                    sync(&mut root, &mut layout);
                    assert_eq!(handle.row_offset(11), Some(330.0));
                }
                handle.scroll_to_end();
                sync(&mut root, &mut layout);
                count.set(100_001);
                sync(&mut root, &mut layout);
                assert_eq!(
                    offset.peek(),
                    handle.row_offset(100_001).unwrap() as f32 - 80.0
                );
                count.set(1);
                sync(&mut root, &mut layout);
                assert_eq!(handle.row_offset(1), Some(20.0));
                assert_eq!(offset.peek(), 0.0);
                assert!(
                    renders.get() < 45,
                    "navigation must mount only bounded destination windows"
                );
            }
        }
    });
}

#[test]
fn virtual_list_navigation_coalesces_waits_for_viewport_and_realigns_measured_rows() {
    use jadix_components::components::virtual_list::{
        ListScrollAlign, VirtualListHandle, virtual_list,
    };
    use jadix_components::core::{scroll_viewport::ScrollViewportSignalKeys, tree::LayoutTree};
    use std::{cell::Cell, rc::Rc};
    fn sync(el: &mut Element, layout: &mut LayoutTree) {
        if let Some(callback) = el.reactive_sync.clone() {
            callback.run(el, layout);
        }
        for child in &mut el.children {
            sync(child, layout);
        }
    }
    fn measure(el: &mut Element, index: usize, extent: f32) -> bool {
        if el.attrs.get("__jadix_vlist_row_key") == Some(&format!("i:{index}")) {
            el.layout_bounds[3] = extent;
            return true;
        }
        el.children
            .iter_mut()
            .any(|child| measure(child, index, extent))
    }
    let handle = VirtualListHandle::default();
    assert!(!handle.scroll_to_end());
    let (owner, retained) = create_root(|| {
        for measured in [false, true] {
            let id = format!("navigation-{measured}");
            let count = Rc::new(Cell::new(100_000));
            let renders = Rc::new(Cell::new(0));
            let queued_in_callback = Rc::new(Cell::new(false));
            let mut builder = virtual_list(&id)
                .handle(handle.clone())
                .item_count_fn({
                    let count = count.clone();
                    move || count.get()
                })
                .item_height(20.0)
                .height(80.0)
                .overscan(1)
                .render_item({
                    let renders = renders.clone();
                    move |i| {
                        renders.set(renders.get() + 1);
                        Element::new().text(i.to_string())
                    }
                })
                .on_visible_range_change({
                    let handle = handle.clone();
                    let queued = queued_in_callback.clone();
                    move |range| {
                        if !measured && range == Some(99_996..100_000) && !queued.replace(true) {
                            assert!(handle.scroll_to_index(1, ListScrollAlign::Start, 0.0));
                        }
                    }
                });
            if measured {
                builder = builder.measure_rows();
            }
            let mut root: Element = builder.into();
            let keys = ScrollViewportSignalKeys::new(&id);
            let viewport = use_signal(&keys.viewport_h, || 0.0_f32);
            let offset = use_signal(&keys.offset_y, || 0.0_f32);
            let target = use_signal(&keys.target_y, || 0.0_f32);
            let mut layout = LayoutTree::new();
            viewport.set_silent(0.0);
            assert!(handle.scroll_to_index(50_000, ListScrollAlign::Start, 0.0));
            assert!(handle.scroll_to_index(10, ListScrollAlign::Fraction(0.25), 5.0));
            assert!(!handle.scroll_to_index(0, ListScrollAlign::Fraction(f64::NAN), 0.0));
            sync(&mut root, &mut layout);
            assert_eq!(offset.peek(), 0.0);
            viewport.set_silent(80.0);
            sync(&mut root, &mut layout);
            assert_eq!(offset.peek(), 180.0);
            assert_eq!(target.peek(), 180.0);
            if measured {
                assert!(measure(&mut root, 10, 60.0));
                sync(&mut root, &mut layout);
                assert_eq!(
                    offset.peek(),
                    190.0,
                    "fraction realigns using the measured target height"
                );
                // An explicit user move cancels a pending correction.
                assert!(handle.scroll_to_index(20, ListScrollAlign::Center, 0.0));
                sync(&mut root, &mut layout);
                assert!(measure(&mut root, 20, 60.0));
                offset.set_silent(50.0);
                sync(&mut root, &mut layout);
                assert_eq!(offset.peek(), 50.0);
            }
            assert!(
                renders.get() < 50,
                "distant queued requests must not mount the intervening rows"
            );
            assert!(handle.scroll_to_end());
            sync(&mut root, &mut layout);
            assert_eq!(
                offset.peek(),
                handle.row_offset(100_000).unwrap() as f32 - 80.0
            );
            if !measured {
                assert!(queued_in_callback.get());
                sync(&mut root, &mut layout);
                assert_eq!(
                    offset.peek(),
                    20.0,
                    "callback request survives the current sync"
                );
            } else {
                assert!(measure(&mut root, 99_999, 100.0));
                sync(&mut root, &mut layout);
                assert_eq!(
                    offset.peek(),
                    handle.row_offset(100_000).unwrap() as f32 - 80.0,
                    "End remains attached to the measured bottom"
                );
            }
            assert!(handle.scroll_to_index(usize::MAX, ListScrollAlign::End, 0.0));
            sync(&mut root, &mut layout);
            count.set(0);
            assert!(handle.scroll_to_end());
            sync(&mut root, &mut layout);
            assert_eq!(offset.peek(), 0.0);
            drop(root);
            assert!(!handle.scroll_to_end());
        }
        let _retained: Element = virtual_list("disposed-navigation")
            .handle(handle.clone())
            .into();
        // Keep the element beyond owner disposal to distinguish owner from node lifetime.
        _retained
    });
    drop(owner);
    assert!(!handle.scroll_to_end());
    drop(retained);
}

#[test]
fn virtual_list_viewport_and_page_notifications_share_geometry_and_rearm_after_data_changes() {
    use jadix_components::components::virtual_list::{VirtualListHandle, virtual_list};
    use jadix_components::core::scroll_viewport::ScrollViewportSignalKeys;
    use jadix_components::core::tree::LayoutTree;
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };

    fn sync(element: &mut Element, layout: &mut LayoutTree) {
        if let Some(callback) = element.reactive_sync.clone() {
            callback.run(element, layout);
        }
        for child in &mut element.children {
            sync(child, layout);
        }
    }
    fn measure_first(element: &mut Element) -> bool {
        if element
            .attrs
            .get("__jadix_vlist_row_key")
            .map(String::as_str)
            == Some("k:row-0")
        {
            element.layout_bounds[3] = 120.0;
            return true;
        }
        element.children.iter_mut().any(measure_first)
    }
    let (_owner, ()) = create_root(|| {
        for measured in [false, true] {
            let id = format!("page-notifications-{measured}");
            let handle = VirtualListHandle::default();
            let count = Rc::new(Cell::new(100));
            let revision = Rc::new(Cell::new(0));
            let key_reads = Rc::new(Cell::new(0));
            let ranges = Rc::new(RefCell::new(Vec::new()));
            let end_requests = Rc::new(Cell::new(0));
            let start_requests = Rc::new(Cell::new(0));
            assert_eq!(handle.viewport(), None);
            let mut builder = virtual_list(&id)
                .handle(handle.clone())
                .item_count_fn({
                    let count = count.clone();
                    move || count.get()
                })
                .data_key_fn({
                    let revision = revision.clone();
                    move || revision.get().to_string()
                })
                .item_key_fn({
                    let reads = key_reads.clone();
                    move |i| {
                        reads.set(reads.get() + 1);
                        format!("row-{i}")
                    }
                })
                .item_height(20.0)
                .height(80.0)
                .overscan(5)
                .load_more_threshold(0.5)
                .load_more_threshold(f64::NAN)
                .on_visible_range_change({
                    let ranges = ranges.clone();
                    let handle = handle.clone();
                    move |range| {
                        assert_eq!(handle.viewport().map(|v| v.visible), range);
                        ranges.borrow_mut().push(range);
                    }
                })
                .on_load_more({
                    let requests = end_requests.clone();
                    let handle = handle.clone();
                    let count = count.clone();
                    move || {
                        let next = requests.get() + 1;
                        requests.set(next);
                        let viewport = handle.viewport().unwrap();
                        assert_eq!(
                            handle.row_offset(viewport.item_count),
                            Some(viewport.total_extent)
                        );
                        // Mutating the source inside the callback takes effect next sync.
                        if next == 3 {
                            count.set(120);
                        }
                    }
                })
                .on_load_more_at_start({
                    let requests = start_requests.clone();
                    let handle = handle.clone();
                    move || {
                        assert!(handle.viewport().unwrap().item_count > 0);
                        requests.set(requests.get() + 1);
                    }
                });
            if measured {
                builder = builder.measure_rows();
            }
            let mut root: Element = builder.into();
            let keys = ScrollViewportSignalKeys::new(&id);
            let offset = use_signal(&keys.offset_y, || 0.0_f32);
            let viewport = use_signal(&keys.viewport_h, || 0.0_f32);
            viewport.set_silent(0.0);
            let mut layout = LayoutTree::new();
            sync(&mut root, &mut layout);
            assert_eq!(*ranges.borrow(), vec![None]);
            assert_eq!(end_requests.get(), 0);
            assert_eq!(start_requests.get(), 0);
            viewport.set_silent(80.0);
            offset.set_silent(205.0);
            sync(&mut root, &mut layout);
            assert_eq!(handle.viewport().unwrap().visible, 10..15);
            assert_eq!(ranges.borrow().last(), Some(&Some(10..15)));
            let notifications = ranges.borrow().len();
            offset.set_silent(206.0);
            sync(&mut root, &mut layout);
            assert_eq!(
                ranges.borrow().len(),
                notifications,
                "within-row motion must not notify"
            );
            assert_eq!(handle.viewport().unwrap().offset, 206.0);
            assert_eq!(
                key_reads.get(),
                100,
                "stable frames must not rescan all keys"
            );

            offset.set_silent(1880.0);
            sync(&mut root, &mut layout);
            sync(&mut root, &mut layout);
            assert_eq!(end_requests.get(), 1);
            revision.set(1);
            sync(&mut root, &mut layout);
            sync(&mut root, &mut layout);
            assert_eq!(
                end_requests.get(),
                2,
                "same-count data replacement rearms once"
            );
            assert_eq!(key_reads.get(), 200);
            offset.set_silent(1000.0);
            sync(&mut root, &mut layout);
            offset.set_silent(1920.0);
            sync(&mut root, &mut layout);
            assert_eq!(end_requests.get(), 3);
            assert_eq!(handle.viewport().unwrap().item_count, 100);
            sync(&mut root, &mut layout);
            assert_eq!(handle.viewport().unwrap().item_count, 120);
            assert_eq!(end_requests.get(), 3);
            assert_eq!(key_reads.get(), 320);
            count.set(0);
            sync(&mut root, &mut layout);
            sync(&mut root, &mut layout);
            assert_eq!(handle.viewport().unwrap().visible, 0..0);
            assert_eq!(
                end_requests.get(),
                4,
                "empty measured data can request its first page"
            );
            assert_eq!(start_requests.get(), 0);
            count.set(2);
            sync(&mut root, &mut layout);
            sync(&mut root, &mut layout);
            assert_eq!(start_requests.get(), 1);
            assert_eq!(end_requests.get(), 5);
            assert_eq!(handle.viewport().unwrap().visible, 0..2);
            if measured {
                assert!(measure_first(&mut root));
                sync(&mut root, &mut layout);
                let geometry = handle.viewport().unwrap();
                assert_eq!(geometry.visible, 0..1, "overscan rows are not viewable");
                assert_eq!(geometry.total_extent, 140.0);
                assert_eq!(handle.row_offset(1), Some(120.0));
                assert_eq!(
                    end_requests.get(),
                    5,
                    "measured rows can leave the load edge"
                );
            }
        }
    });
}

#[test]
fn virtual_list_reports_the_viewport_and_keeps_rows_anchored_when_heights_shrink() {
    use jadix_components::components::virtual_list::{ListScrollPosition, virtual_list};
    use jadix_components::core::scroll_viewport::ScrollViewportSignalKeys;
    use jadix_components::core::tree::LayoutTree;
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };

    fn sync(element: &mut Element, layout: &mut LayoutTree) {
        if let Some(callback) = element.reactive_sync.clone() {
            callback.run(element, layout);
        }
        for child in &mut element.children {
            sync(child, layout);
        }
    }
    fn has_row(element: &Element, index: usize) -> bool {
        element.template_id.as_deref() == Some(format!("resizing-list__row_{index}").as_str())
            || element.children.iter().any(|child| has_row(child, index))
    }

    let (_owner, ()) = create_root(|| {
        let height = Rc::new(Cell::new(40.0_f32));
        let count = Rc::new(Cell::new(100_usize));
        let positions = Rc::new(RefCell::new(Vec::new()));
        let renders = Rc::new(Cell::new(0));
        let mut root: Element = virtual_list("resizing-list")
            .item_count_fn({
                let count = count.clone();
                move || count.get()
            })
            .item_height_fn({
                let height = height.clone();
                move || height.get()
            })
            .on_scroll_position({
                let positions = positions.clone();
                move |at| positions.borrow_mut().push(at)
            })
            .overscan(0)
            .height(80.0)
            .render_item({
                let renders = renders.clone();
                move |index| {
                    renders.set(renders.get() + 1);
                    Element::new().text(format!("row {index}"))
                }
            })
            .into();
        let keys = ScrollViewportSignalKeys::new("resizing-list");
        let offset = use_signal(&keys.offset_y, || 0.0_f32);
        let target = use_signal(&keys.target_y, || 0.0_f32);
        let viewport = use_signal(&keys.viewport_h, || 0.0_f32);
        viewport.set_silent(0.0);
        let mut layout = LayoutTree::new();
        sync(&mut root, &mut layout);
        assert_eq!(*positions.borrow(), vec![None]);
        viewport.set_silent(80.0);
        offset.set_silent(205.0);
        target.set_silent(205.0);
        sync(&mut root, &mut layout);
        assert_eq!(
            positions.borrow().last(),
            Some(&Some(ListScrollPosition {
                index: 5,
                offset: 5.0
            }))
        );
        assert!(
            has_row(&root, 7),
            "bottom partially visible row must be mounted"
        );
        let notifications = positions.borrow().len();
        sync(&mut root, &mut layout);
        assert_eq!(positions.borrow().len(), notifications);
        height.set(20.0);
        sync(&mut root, &mut layout);
        assert_eq!(offset.peek(), 105.0);
        assert_eq!(target.peek(), 105.0);
        assert_eq!(
            positions.borrow().last(),
            Some(&Some(ListScrollPosition {
                index: 5,
                offset: 5.0
            }))
        );
        assert!(has_row(&root, 9));
        let result = layout.build_and_layout(&mut root, 300.0, 80.0, [-1.0; 2]);
        let (_, _, _, content) = result
            .scroll_metrics
            .iter()
            .find(|(id, ..)| id == "resizing-list")
            .unwrap();
        assert!((content[1] - 2000.0).abs() < 1.0);
        let before = renders.get();
        sync(&mut root, &mut layout);
        assert_eq!(renders.get(), before, "stable geometry must reuse rows");
        offset.set_silent(115.0);
        target.set_silent(115.0);
        height.set(10.0);
        sync(&mut root, &mut layout);
        let position = positions.borrow().last().copied().flatten().unwrap();
        assert_eq!(position.index, 5);
        assert!(position.offset >= 0.0 && position.offset < 10.0);
        assert_eq!(offset.peek(), target.peek());
        assert!(has_row(&root, 5), "the anchor row must remain mounted");
        count.set(3);
        sync(&mut root, &mut layout);
        assert_eq!(offset.peek(), 0.0);
        assert_eq!(target.peek(), 0.0);
        assert!(has_row(&root, 0));
        count.set(0);
        sync(&mut root, &mut layout);
        assert_eq!(positions.borrow().last(), Some(&None));
    });
}

#[test]
fn virtual_list_follows_the_end_only_while_the_user_remains_there() {
    use jadix_components::components::virtual_list::virtual_list;
    use jadix_components::core::scroll_viewport::ScrollViewportSignalKeys;
    use jadix_components::core::tree::LayoutTree;
    use std::{cell::Cell, rc::Rc};

    fn sync(element: &mut Element, layout: &mut LayoutTree) {
        if let Some(callback) = element.reactive_sync.clone() {
            callback.run(element, layout);
        }
        for child in &mut element.children {
            sync(child, layout);
        }
    }
    let (_owner, ()) = create_root(|| {
        for follow in [false, true] {
            let id = format!("follow-end-{follow}");
            let count = Rc::new(Cell::new(10));
            let row_height = Rc::new(Cell::new(20.0));
            let mut root: Element = virtual_list(&id)
                .item_count_fn({
                    let count = count.clone();
                    move || count.get()
                })
                .item_height_fn({
                    let height = row_height.clone();
                    move || height.get()
                })
                .height(80.0)
                .overscan(0)
                .follow_end(follow)
                .into();
            let keys = ScrollViewportSignalKeys::new(&id);
            let offset = use_signal(&keys.offset_y, || 0.0_f32);
            let target = use_signal(&keys.target_y, || 0.0_f32);
            let viewport = use_signal(&keys.viewport_h, || 80.0_f32);
            let mut layout = LayoutTree::new();
            offset.set_silent(120.0);
            target.set_silent(120.0);
            sync(&mut root, &mut layout);
            count.set(12);
            sync(&mut root, &mut layout);
            assert_eq!(offset.peek(), if follow { 160.0 } else { 120.0 });
            assert_eq!(offset.peek(), target.peek());
            offset.set_silent(40.0);
            target.set_silent(40.0);
            count.set(14);
            sync(&mut root, &mut layout);
            assert_eq!(
                offset.peek(),
                40.0,
                "reading older rows must suspend following"
            );
            row_height.set(40.0);
            sync(&mut root, &mut layout);
            assert_eq!(
                offset.peek(),
                80.0,
                "height changes must retain the visible row away from the end"
            );
            offset.set_silent(480.0);
            target.set_silent(480.0);
            viewport.set_silent(100.0);
            sync(&mut root, &mut layout);
            assert_eq!(offset.peek(), 460.0);
            row_height.set(20.0);
            sync(&mut root, &mut layout);
            assert_eq!(offset.peek(), 180.0);
            assert_eq!(target.peek(), 180.0);
            count.set(0);
            sync(&mut root, &mut layout);
            assert_eq!(offset.peek(), 0.0);
            count.set(10);
            sync(&mut root, &mut layout);
            assert_eq!(offset.peek(), if follow { 100.0 } else { 0.0 });
        }
    });
}

#[test]
fn viewport_component_and_core_activation_share_the_scroll_contract() {
    let (_owner, _) = create_root(|| {
        let builder: ScrollViewportBuilder = scroll_viewport("component-scroll")
            .w(200.0)
            .h(120.0)
            .overflow_x("hidden")
            .overflow_y("auto")
            .scroll_behavior("jump");
        let component: Element = builder.into();
        let raw = activate_scroll_viewport(
            Element::new().w(200.0).h(120.0),
            "raw-scroll",
            "hidden",
            "auto",
            "jump",
            "scrollbar",
        );
        assert_eq!(component.style.size, raw.style.size);
        assert_eq!(component.style.overflow, raw.style.overflow);
        assert_eq!(
            component.focus_id.as_deref(),
            Some("scroll:component-scroll")
        );
        assert_eq!(raw.focus_id.as_deref(), Some("scroll:raw-scroll"));
        assert!(component.children.is_empty());
        assert_eq!(
            use_signal(&scroll_behavior_signal_key("component-scroll"), String::new).peek(),
            "jump"
        );
    });
}

#[test]
fn scrollbar_component_keeps_target_geometry_and_visibility_contracts() {
    let (_owner, _) = create_root(|| {
        use_signal(&scroll_viewport_h_signal_key("scroll-target"), || 120.0_f32);
        use_signal(&scroll_content_h_signal_key("scroll-target"), || 480.0_f32);
        let builder: ScrollbarBuilder = scrollbar("bar")
            .target("scroll-target")
            .orientation(ScrollbarOrientation::Vertical)
            .thickness(12.0)
            .rect()
            .always();
        let track: Element = builder.into();
        assert_eq!(
            track.focus_id.as_deref(),
            Some("scrollbar-track:scroll-target:y")
        );
        assert_eq!(track.children.len(), 1);
        assert_eq!(
            track.children[0].focus_id.as_deref(),
            Some("scrollbar-thumb:scroll-target:y")
        );
        assert_eq!(
            track.style.size.width,
            taffy::style::Dimension::length(12.0)
        );
        assert_eq!(
            track.style.size.height,
            taffy::style::Dimension::length(120.0)
        );
        assert_eq!(
            use_signal(
                &scrollbar_visibility_signal_key("scroll-target", "y"),
                String::new
            )
            .peek(),
            "always"
        );
        let auto: Element = scrollbar("unmeasured-bar")
            .target("unmeasured")
            .auto()
            .into();
        assert_eq!(auto.children.len(), 1);
        let never: Element = scrollbar("hidden-bar").never().into();
        assert!(never.children.is_empty());
        assert_eq!(never.style.size.width, taffy::style::Dimension::length(0.0));
        assert_eq!(
            never.style.size.height,
            taffy::style::Dimension::length(0.0)
        );
    });
}

#[test]
fn scroll_component_tags_remain_available_from_prelude() {
    use jadix_components::prelude::*;
    let (_owner, _) = create_root(|| {
        let viewport = ui! {
            <ScrollViewport id="tag-scroll" overflow_y="auto" scroll_behavior="jump">
                <Box h={480.0} />
            </ScrollViewport>
        };
        let bar = ui! {
            <Scrollbar target="tag-scroll" orientation="vertical" visibility="always" thickness={12.0} />
        };
        assert_eq!(viewport.focus_id.as_deref(), Some("scroll:tag-scroll"));
        assert_eq!(
            bar.focus_id.as_deref(),
            Some("scrollbar-track:tag-scroll:y")
        );
    });
}

#[test]
fn arrow_switches_work_in_both_scroll_component_tags() {
    use jadix_components::prelude::*;
    let (_owner, ()) = create_root(|| {
        let bar = ui! {
            <Scrollbar target="arrows" orientation="horizontal" visibility="always" arrows={true}
                arrow_class="hover:bg-primary" />
        };
        assert_eq!(bar.children.len(), 3);
        let area = ui! {
            <ScrollArea id="arrows" w={240.0} h={160.0} scrollbar_arrows={true}
                scrollbar_arrow_class="hover:bg-primary">
                <Div w={600.0} h={700.0} />
            </ScrollArea>
        };
        assert!(
            jadix_components::core::tree::find_in_tree(&area, |el| el
                .get_attr("data-slot")
                .map(String::as_str)
                == Some("scrollbar-arrow-end"))
            .is_some()
        );
    });
}

#[test]
fn product_viewports_default_to_smooth_and_allow_explicit_jump() {
    use jadix_components::prelude::*;
    let (_owner, ()) = create_root(|| {
        for behavior in ["smooth", "jump"] {
            let list: Element = if behavior == "jump" {
                virtual_list("default-list").scroll_behavior("jump").into()
            } else {
                virtual_list("default-list").into()
            };
            let masonry: Element = if behavior == "jump" {
                virtual_masonry("default-masonry")
                    .scroll_behavior("jump")
                    .into()
            } else {
                virtual_masonry("default-masonry").into()
            };
            let messages = if behavior == "jump" {
                ui! { <MessageScrollerProvider id="default-messages">
                    <MessageScrollerViewport scroll_behavior="jump" />
                </MessageScrollerProvider> }
            } else {
                ui! { <MessageScrollerProvider id="default-messages">
                    <MessageScrollerViewport />
                </MessageScrollerProvider> }
            };
            for root in [&list, &masonry, &messages] {
                let viewport = jadix_components::core::tree::find_in_tree(root, |node| {
                    node.focus_id
                        .as_deref()
                        .is_some_and(|id| id.starts_with("scroll:"))
                })
                .unwrap();
                let id = viewport
                    .focus_id
                    .as_deref()
                    .unwrap()
                    .strip_prefix("scroll:")
                    .unwrap();
                let keys =
                    jadix_components::core::scroll_viewport::ScrollViewportSignalKeys::new(id);
                assert_eq!(
                    use_signal(keys.behavior.as_str(), String::new).peek(),
                    behavior
                );
                use_signal(keys.viewport_h.as_str(), || 100.0_f32).set_silent(100.0);
                use_signal(keys.content_h.as_str(), || 500.0_f32).set_silent(500.0);
                use_signal(keys.offset_y.as_str(), || 0.0_f32).set_silent(0.0);
                use_signal(keys.target_y.as_str(), || 0.0_f32).set_silent(0.0);
                jadix_components::authoring::input::scroll_by(id, 0.0, 40.0);
                assert_eq!(use_signal(keys.target_y.as_str(), || 0.0_f32).peek(), 40.0);
                assert_eq!(
                    use_signal(keys.offset_y.as_str(), || 0.0_f32).peek(),
                    if behavior == "smooth" { 0.0 } else { 40.0 }
                );
            }
        }
    });
}

#[test]
fn measured_virtual_list_uses_real_layout_and_preserves_a_shrinking_anchor() {
    use jadix_components::components::virtual_list::{ListScrollPosition, virtual_list};
    use jadix_components::core::scroll_viewport::ScrollViewportSignalKeys;
    use jadix_components::core::tree::LayoutTree;
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };

    fn sync(element: &mut Element, layout: &mut LayoutTree) {
        if let Some(callback) = element.reactive_sync.clone() {
            callback.run(element, layout);
        }
        for child in &mut element.children {
            sync(child, layout);
        }
    }
    fn resize(element: &mut Element, index: usize, height: f32) -> bool {
        if element.template_id.as_deref() == Some(format!("measured-list__row_{index}").as_str()) {
            let child = &mut element.children[0];
            *child = std::mem::take(child).h(height).min_h(height).max_h(height);
            return true;
        }
        element
            .children
            .iter_mut()
            .any(|child| resize(child, index, height))
    }
    let (_owner, ()) = create_root(|| {
        let positions = Rc::new(RefCell::new(Vec::new()));
        let data = Rc::new(Cell::new(0));
        let focus = Rc::new(Cell::new(None));
        let count = Rc::new(Cell::new(20));
        let mut root: Element = virtual_list("measured-list")
            .item_height(20.0)
            .item_count_fn({
                let count = count.clone();
                move || count.get()
            })
            .measure_rows()
            .overscan(2)
            .height(60.0)
            .data_key_fn({
                let data = data.clone();
                move || data.get().to_string()
            })
            .focused_index_fn({
                let focus = focus.clone();
                move || focus.get()
            })
            .on_scroll_position({
                let positions = positions.clone();
                move |at| positions.borrow_mut().push(at)
            })
            .render_item(|i| {
                let h = match i {
                    0 => 40.0,
                    1 => 30.0,
                    2 => 10.0,
                    _ => 20.0,
                };
                Element::new().h(h).min_h(h).max_h(h)
            })
            .into();
        let keys = ScrollViewportSignalKeys::new("measured-list");
        let offset = use_signal(&keys.offset_y, || 0.0_f32);
        let target = use_signal(&keys.target_y, || 0.0_f32);
        use_signal(&keys.viewport_h, || 60.0_f32).set_silent(60.0);
        let width = use_signal(&keys.viewport_w, || 300.0_f32);
        let mut layout = LayoutTree::new();
        sync(&mut root, &mut layout);
        layout.build_and_layout(&mut root, 300.0, 60.0, [-1.0; 2]);
        sync(&mut root, &mut layout);
        offset.set_silent(75.0);
        target.set_silent(75.0);
        sync(&mut root, &mut layout);
        assert_eq!(
            positions.borrow().last(),
            Some(&Some(ListScrollPosition {
                index: 2,
                offset: 5.0
            }))
        );
        assert!(resize(&mut root, 0, 60.0));
        layout.build_and_layout(&mut root, 300.0, 60.0, [-1.0; 2]);
        sync(&mut root, &mut layout);
        assert_eq!(
            offset.peek(),
            95.0,
            "growth above the viewport must retain row 2 and its depth"
        );
        assert!(resize(&mut root, 2, 3.0));
        layout.build_and_layout(&mut root, 300.0, 60.0, [-1.0; 2]);
        sync(&mut root, &mut layout);
        let at = positions.borrow().last().copied().flatten().unwrap();
        assert_eq!(at.index, 2);
        assert!(at.offset >= 0.0 && at.offset < 3.0);
        assert_eq!(offset.peek(), target.peek());
        focus.set(Some(15));
        sync(&mut root, &mut layout);
        assert!(
            offset.peek() > 200.0,
            "focus must reveal a previously unmounted row"
        );
        focus.set(None);
        width.set_silent(200.0);
        sync(&mut root, &mut layout);
        assert_eq!(
            positions.borrow().last().copied().flatten().unwrap().index,
            13,
            "width invalidation must preserve the current row using fresh estimates"
        );
        data.set(1);
        sync(&mut root, &mut layout);
        count.set(2);
        sync(&mut root, &mut layout);
        assert_eq!(offset.peek(), 0.0);
        count.set(0);
        sync(&mut root, &mut layout);
        assert_eq!(positions.borrow().last(), Some(&None));
    });
}

#[test]
fn measured_virtual_list_follows_appends_and_delayed_sizes_only_at_the_end() {
    use jadix_components::components::virtual_list::virtual_list;
    use jadix_components::core::scroll_viewport::ScrollViewportSignalKeys;
    use jadix_components::core::tree::LayoutTree;
    use std::{cell::Cell, rc::Rc};
    fn sync(element: &mut Element, layout: &mut LayoutTree) {
        if let Some(callback) = element.reactive_sync.clone() {
            callback.run(element, layout);
        }
        for child in &mut element.children {
            sync(child, layout);
        }
    }
    let (_owner, ()) = create_root(|| {
        let count = Rc::new(Cell::new(10));
        let mut root: Element = virtual_list("measured-follow")
            .item_height(20.0)
            .item_count_fn({
                let count = count.clone();
                move || count.get()
            })
            .measure_rows()
            .follow_end(true)
            .overscan(0)
            .height(80.0)
            .render_item(|i| {
                let h = if i == 11 { 60.0 } else { 20.0 };
                Element::new().h(h).min_h(h).max_h(h)
            })
            .into();
        let keys = ScrollViewportSignalKeys::new("measured-follow");
        let offset = use_signal(&keys.offset_y, || 0.0_f32);
        let target = use_signal(&keys.target_y, || 0.0_f32);
        use_signal(&keys.viewport_h, || 80.0_f32).set_silent(80.0);
        let mut layout = LayoutTree::new();
        offset.set_silent(120.0);
        target.set_silent(120.0);
        sync(&mut root, &mut layout);
        count.set(12);
        sync(&mut root, &mut layout);
        assert_eq!(offset.peek(), 160.0);
        layout.build_and_layout(&mut root, 300.0, 80.0, [-1.0; 2]);
        sync(&mut root, &mut layout);
        assert_eq!(
            offset.peek(),
            200.0,
            "the newly laid out taller final row must remain visible"
        );
        offset.set_silent(40.0);
        target.set_silent(40.0);
        count.set(13);
        sync(&mut root, &mut layout);
        assert_eq!(
            offset.peek(),
            40.0,
            "appending must not interrupt reading earlier rows"
        );
        layout.build_and_layout(&mut root, 300.0, 80.0, [-1.0; 2]);
        sync(&mut root, &mut layout);
        assert_eq!(offset.peek(), 40.0);
        assert_eq!(target.peek(), 40.0);
        use_signal(&keys.viewport_h, || 80.0_f32).set_silent(f32::INFINITY);
        sync(&mut root, &mut layout);
        assert_eq!(
            offset.peek(),
            40.0,
            "invalid viewport uses the configured height"
        );
    });
}

#[test]
fn measured_virtual_list_normalizes_nonfinite_configured_geometry() {
    use jadix_components::components::virtual_list::virtual_list;
    use jadix_components::core::tree::LayoutTree;
    fn sync(element: &mut Element, layout: &mut LayoutTree) {
        if let Some(callback) = element.reactive_sync.clone() {
            callback.run(element, layout);
        }
        for child in &mut element.children {
            sync(child, layout);
        }
    }
    let (_owner, ()) = create_root(|| {
        for from_font in [false, true] {
            let builder = virtual_list(if from_font {
                "invalid-font-list"
            } else {
                "invalid-measured-list"
            })
            .item_count(10)
            .height(f32::INFINITY)
            .measure_rows();
            let mut root: Element = if from_font {
                builder.item_height_from_font(f32::INFINITY, 1.5)
            } else {
                builder.item_height(f32::INFINITY)
            }
            .into();
            let mut layout = LayoutTree::new();
            layout.build_and_layout(&mut root, 300.0, 80.0, [-1.0; 2]);
            sync(&mut root, &mut layout);
            assert!(root.layout_bounds.iter().all(|value| value.is_finite()));
        }
    });
}

#[test]
fn measured_virtual_list_reorder_keeps_extents_and_scroll_anchor_by_item_key() {
    use jadix_components::components::virtual_list::{ListScrollPosition, virtual_list};
    use jadix_components::core::scroll_viewport::ScrollViewportSignalKeys;
    use jadix_components::core::tree::LayoutTree;
    use std::{cell::RefCell, rc::Rc};

    fn sync(element: &mut Element, layout: &mut LayoutTree) {
        if let Some(callback) = element.reactive_sync.clone() {
            callback.run(element, layout);
        }
        for child in &mut element.children {
            sync(child, layout);
        }
    }
    fn has_template(element: &Element, id: &str) -> bool {
        element.template_id.as_deref() == Some(id)
            || element.children.iter().any(|child| has_template(child, id))
    }

    let (_owner, ()) = create_root(|| {
        let rows = Rc::new(RefCell::new(vec![
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
            "e".to_string(),
        ]));
        let positions = Rc::new(RefCell::new(Vec::new()));
        let mut root: Element = virtual_list("stable-list")
            .item_height(20.0)
            .item_count_fn({
                let rows = rows.clone();
                move || rows.borrow().len()
            })
            .data_key_fn({
                let rows = rows.clone();
                move || rows.borrow().join("|")
            })
            .item_key_fn({
                let rows = rows.clone();
                move |index| rows.borrow()[index].clone()
            })
            .measure_rows()
            .overscan(0)
            .height(50.0)
            .on_scroll_position({
                let positions = positions.clone();
                move |position| positions.borrow_mut().push(position)
            })
            .render_item({
                let rows = rows.clone();
                move |index| {
                    let key = rows.borrow()[index].clone();
                    let height = match key.as_str() {
                        "a" => 40.0,
                        "b" => 30.0,
                        "c" => 10.0,
                        _ => 20.0,
                    };
                    Element::new()
                        .text(key)
                        .h(height)
                        .min_h(height)
                        .max_h(height)
                }
            })
            .into();
        let keys = ScrollViewportSignalKeys::new("stable-list");
        let offset = use_signal(&keys.offset_y, || 0.0_f32);
        let target = use_signal(&keys.target_y, || 0.0_f32);
        use_signal(&keys.viewport_h, || 50.0_f32).set_silent(50.0);
        let mut layout = LayoutTree::new();

        sync(&mut root, &mut layout);
        layout.build_and_layout(&mut root, 300.0, 50.0, [-1.0; 2]);
        sync(&mut root, &mut layout);
        offset.set_silent(43.0);
        target.set_silent(43.0);
        sync(&mut root, &mut layout);
        assert_eq!(
            positions.borrow().last(),
            Some(&Some(ListScrollPosition {
                index: 1,
                offset: 3.0,
            }))
        );

        *rows.borrow_mut() = vec![
            "x".to_string(),
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
            "e".to_string(),
        ];
        sync(&mut root, &mut layout);
        assert_eq!(offset.peek(), 63.0, "the keyed row a remains 3px anchored");
        assert_eq!(target.peek(), offset.peek());
        assert_eq!(
            positions.borrow().last(),
            Some(&Some(ListScrollPosition {
                index: 2,
                offset: 3.0,
            }))
        );
        assert!(has_template(&root, "stable-list__key_62"));

        offset.set_silent(90.0);
        target.set_silent(90.0);
        sync(&mut root, &mut layout);
        assert_eq!(
            positions.borrow().last(),
            Some(&Some(ListScrollPosition {
                index: 3,
                offset: 0.0,
            })),
            "row c must begin at 90px, preserving row b's measured 30px extent"
        );
    });
}

#[test]
fn virtual_list_layout_motion_requires_stable_keys_and_tracks_reordered_items() {
    use jadix_components::components::virtual_list::virtual_list;
    use jadix_components::core::reactivity::create_root;
    use jadix_components::core::tree::LayoutTree;
    use jadix_components::motion::LayoutAnimation;
    use std::{cell::RefCell, rc::Rc};

    fn sync(element: &mut Element, layout: &mut LayoutTree) {
        if let Some(callback) = element.reactive_sync.clone() {
            callback.run(element, layout);
        }
        for child in &mut element.children {
            sync(child, layout);
        }
    }

    fn has_layout_id(element: &Element, id: &str) -> bool {
        element
            .attrs
            .get("jadix_layout_id")
            .is_some_and(|value| value == id)
            || element
                .children
                .iter()
                .any(|child| has_layout_id(child, id))
    }

    let (_owner, ()) = create_root(|| {
        let rows = Rc::new(RefCell::new(vec!["a".to_string(), "b".to_string()]));
        let mut root: Element = virtual_list("motion-list")
            .item_height(20.0)
            .item_count_fn({
                let rows = rows.clone();
                move || rows.borrow().len()
            })
            .item_key_fn({
                let rows = rows.clone();
                move |index| rows.borrow()[index].clone()
            })
            .animate_item_changes(LayoutAnimation::Position)
            .render_item({
                let rows = rows.clone();
                move |index| Element::new().text(rows.borrow()[index].clone())
            })
            .into();
        let mut layout = LayoutTree::new();

        sync(&mut root, &mut layout);
        layout.build_and_layout(&mut root, 240.0, 80.0, [-1.0; 2]);
        assert!(has_layout_id(&root, "motion-list__vlist_item_61"));
        assert!(has_layout_id(&root, "motion-list__vlist_item_62"));

        *rows.borrow_mut() = vec!["b".to_string(), "a".to_string()];
        sync(&mut root, &mut layout);
        assert!(has_layout_id(&root, "motion-list__vlist_item_61"));
        assert!(has_layout_id(&root, "motion-list__vlist_item_62"));
    });
}

#[test]
fn measured_virtual_list_rebases_anchor_motion_after_extent_changes() {
    use jadix_components::components::virtual_list::virtual_list;
    use jadix_components::core::scroll_viewport::ScrollViewportSignalKeys;
    use jadix_components::core::tree::LayoutTree;
    use jadix_components::motion::LayoutAnimation;
    use std::{cell::RefCell, rc::Rc};

    fn sync(element: &mut Element, layout: &mut LayoutTree) {
        if let Some(callback) = element.reactive_sync.clone() {
            callback.run(element, layout);
        }
        for child in &mut element.children {
            sync(child, layout);
        }
    }

    fn row_motion(element: &Element, id: &str) -> Option<jadix_components::motion::LayoutMotion> {
        if element
            .attrs
            .get("jadix_layout_id")
            .is_some_and(|value| value == id)
        {
            return element.layout_motion_handle();
        }
        element
            .children
            .iter()
            .find_map(|child| row_motion(child, id))
    }

    let (_owner, ()) = create_root(|| {
        let rows = Rc::new(RefCell::new(vec![
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
        ]));
        let mut root: Element = virtual_list("anchor-motion-list")
            .item_height(20.0)
            .item_count_fn({
                let rows = rows.clone();
                move || rows.borrow().len()
            })
            .item_key_fn({
                let rows = rows.clone();
                move |index| rows.borrow()[index].clone()
            })
            .measure_rows()
            .overscan(0)
            .animate_item_changes(LayoutAnimation::Position)
            .height(65.0)
            .render_item({
                let rows = rows.clone();
                move |index| {
                    let key = rows.borrow()[index].clone();
                    let height = match key.as_str() {
                        "a" => 40.0,
                        "b" => 30.0,
                        _ => 20.0,
                    };
                    Element::new()
                        .text(key)
                        .h(height)
                        .min_h(height)
                        .max_h(height)
                }
            })
            .into();
        let keys = ScrollViewportSignalKeys::new("anchor-motion-list");
        let offset = use_signal(&keys.offset_y, || 0.0_f32);
        let target = use_signal(&keys.target_y, || 0.0_f32);
        use_signal(&keys.viewport_h, || 65.0_f32).set_silent(65.0);
        let mut layout = LayoutTree::new();

        sync(&mut root, &mut layout);
        layout.build_and_layout(&mut root, 240.0, 65.0, [-1.0; 2]);
        sync(&mut root, &mut layout);
        offset.set_silent(43.0);
        target.set_silent(43.0);
        sync(&mut root, &mut layout);
        layout.build_and_layout(&mut root, 240.0, 65.0, [-1.0; 2]);
        sync(&mut root, &mut layout);

        let b = row_motion(&root, "anchor-motion-list__vlist_item_62").unwrap();
        assert_eq!(b.bounds().unwrap()[1], 40.0);
        assert!(!b.is_active());

        *rows.borrow_mut() = vec![
            "x".to_string(),
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
        ];
        sync(&mut root, &mut layout);
        layout.build_and_layout(&mut root, 240.0, 65.0, [-1.0; 2]);
        sync(&mut root, &mut layout);

        assert_eq!(offset.peek(), 63.0);
        assert_eq!(b.bounds().unwrap()[1], 60.0);
        assert!(
            !b.is_active(),
            "the keyed anchor follows the corrected scroll origin"
        );
    });
}

#[test]
#[should_panic(expected = "VirtualList item keys must be unique")]
fn virtual_list_rejects_duplicate_stable_keys() {
    use jadix_components::components::virtual_list::virtual_list;

    let _: Element = virtual_list("duplicate-keys")
        .item_count(2)
        .item_key_fn(|_| "same".to_string())
        .into();
}

#[test]
fn virtual_list_resamples_rows_after_render_callback_mutates_data() {
    use jadix_components::components::virtual_list::virtual_list;
    use jadix_components::core::reactivity::create_root;
    use jadix_components::core::scroll_viewport::ScrollViewportSignalKeys;
    use jadix_components::core::tree::LayoutTree;
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };

    fn sync(element: &mut Element, layout: &mut LayoutTree) {
        if let Some(callback) = element.reactive_sync.clone() {
            callback.run(element, layout);
        }
        for child in &mut element.children {
            sync(child, layout);
        }
    }

    fn has_row(element: &Element, index: usize) -> bool {
        element.template_id.as_deref() == Some(format!("render-mutation__row_{index}").as_str())
            || element.children.iter().any(|child| has_row(child, index))
    }

    let (_owner, ()) = create_root(|| {
        let rows = Rc::new(RefCell::new((0..10).collect::<Vec<_>>()));
        let shrink_on_render = Rc::new(Cell::new(false));
        let mut root: Element = virtual_list("render-mutation")
            .item_height(20.0)
            .item_count_fn({
                let rows = rows.clone();
                move || rows.borrow().len()
            })
            .overscan(0)
            .height(80.0)
            .render_item({
                let rows = rows.clone();
                let shrink_on_render = shrink_on_render.clone();
                move |index| {
                    if shrink_on_render.replace(false) {
                        rows.borrow_mut().truncate(5);
                    }
                    Element::new().text(rows.borrow()[index].to_string())
                }
            })
            .into();
        let keys = ScrollViewportSignalKeys::new("render-mutation");
        let offset = use_signal(&keys.offset_y, || 0.0_f32);
        use_signal(&keys.viewport_h, || 80.0_f32).set_silent(80.0);
        let mut layout = LayoutTree::new();

        sync(&mut root, &mut layout);
        shrink_on_render.set(true);
        offset.set_silent(80.0);
        sync(&mut root, &mut layout);
        sync(&mut root, &mut layout);

        assert_eq!(rows.borrow().len(), 5);
        assert!(has_row(&root, 4));
        for index in 5..10 {
            assert!(!has_row(&root, index));
        }
    });
}
