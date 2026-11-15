#![cfg(feature = "product-tests")]
//! VirtualList: silent scroll remounts window; overlapping rows reuse layout nodes.

use crate::app::test_support::sync_reactive_elements_subtree;
use crate::components::virtual_list::virtual_list;
use crate::core::reactivity::{create_effect, create_root, use_signal};
use crate::core::scroll_viewport::scroll_offset_y_signal_key;
use crate::core::tree::{Element, LayoutStatus, LayoutTree};

fn collect_texts(el: &Element, out: &mut Vec<String>) {
    if let Some(t) = el.text.as_ref() {
        out.push(t.clone());
    }
    for child in &el.children {
        collect_texts(child, out);
    }
}

fn find_vlist_body<'a>(el: &'a Element) -> Option<&'a Element> {
    if el.attrs.contains_key("__jadix_vlist_range") {
        return Some(el);
    }
    for child in &el.children {
        if let Some(b) = find_vlist_body(child) {
            return Some(b);
        }
    }
    None
}

fn find_vlist_body_mut<'a>(el: &'a mut Element) -> Option<&'a mut Element> {
    if el.attrs.contains_key("__jadix_vlist_range") {
        return Some(el);
    }
    for child in &mut el.children {
        if let Some(b) = find_vlist_body_mut(child) {
            return Some(b);
        }
    }
    None
}

#[test]
fn underfilled_lists_settle_without_remounting_or_repeated_notifications() {
    use std::cell::Cell;
    use std::rc::Rc;

    let (_handle, _) = create_root(|| {
        for measured in [false, true] {
            for count in [0, 3, 5] {
                let id = format!("underfilled-{measured}-{count}");
                let rows = Rc::new(Cell::new(count));
                let renders = Rc::new(Cell::new(0));
                let notifications = Rc::new(Cell::new(0));
                let builder = virtual_list(&id)
                    .item_count_fn({
                        let rows = rows.clone();
                        move || rows.get()
                    })
                    .item_height(20.0)
                    .height(100.0)
                    .render_item({
                        let renders = renders.clone();
                        move |index| {
                            renders.set(renders.get() + 1);
                            Element::new().text(format!("r{index}"))
                        }
                    })
                    .on_scroll_position({
                        let notifications = notifications.clone();
                        move |_| notifications.set(notifications.get() + 1)
                    });
                let mut root = Element::from(if measured {
                    builder.measure_rows()
                } else {
                    builder
                });
                let scroll = use_signal(&scroll_offset_y_signal_key(&id), || 0.0_f32);
                let target = use_signal(
                    &crate::core::scroll_viewport::scroll_target_y_signal_key(&id),
                    || 0.0_f32,
                );
                let mut layout = LayoutTree::new();
                // A stale scroll offset can remain after replacing a larger data set.
                scroll.set_silent(800.0);
                target.set_silent(800.0);
                let mut impact = LayoutStatus::Clean;
                sync_reactive_elements_subtree(&mut root, &mut layout, &mut impact);
                assert_eq!(scroll.peek(), 0.0, "{id}");
                assert_eq!(target.peek(), 0.0, "{id}");

                let settled_renders = renders.get();
                let settled_notifications = notifications.get();
                for _ in 0..3 {
                    let mut impact = LayoutStatus::Clean;
                    sync_reactive_elements_subtree(&mut root, &mut layout, &mut impact);
                    assert_eq!(impact, LayoutStatus::Clean, "{id}");
                }
                assert_eq!(renders.get(), settled_renders, "{id}");
                assert_eq!(notifications.get(), settled_notifications, "{id}");

                rows.set(0);
                sync_reactive_elements_subtree(&mut root, &mut layout, &mut impact);
                let mut texts = Vec::new();
                collect_texts(&root, &mut texts);
                assert!(
                    texts.is_empty(),
                    "stale rows after clearing {id}: {texts:?}"
                );
                let cleared_notifications = notifications.get();
                let mut impact = LayoutStatus::Clean;
                sync_reactive_elements_subtree(&mut root, &mut layout, &mut impact);
                assert_eq!(impact, LayoutStatus::Clean, "{id}");
                assert_eq!(notifications.get(), cleared_notifications, "{id}");
            }
        }
    });
}

#[test]
fn spacer_column_keeps_total_scroll_height() {
    use crate::components::scroll_area::scroll_area;
    use crate::primitives::r#box::{r#box, vstack};
    use crate::utils::virtual_list::visible_item_range;

    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let item_h = 21.0;
            let count = 120usize;
            let viewport_h = 168.0;
            let (start, end) = visible_item_range(11.0 * item_h, viewport_h, item_h, count, 4);
            assert!(start < 11, "overscan should mount rows above the window");
            let total_h = count as f32 * item_h;
            let top_h = start as f32 * item_h;
            let bottom_h = (count.saturating_sub(end) as f32) * item_h;
            let mut body = vstack()
                .w("100%")
                .h(total_h)
                .min_h(total_h)
                .flex_shrink(0.0);
            if top_h > 0.0 {
                body = body.child(r#box().w("100%").h(top_h).min_h(top_h).flex_shrink(0.0));
            }
            for _ in start..end {
                body = body.child(
                    r#box()
                        .w("100%")
                        .h(item_h)
                        .min_h(item_h)
                        .max_h(item_h)
                        .flex_shrink(0.0),
                );
            }
            if bottom_h > 0.0 {
                body = body.child(
                    r#box()
                        .w("100%")
                        .h(bottom_h)
                        .min_h(bottom_h)
                        .flex_shrink(0.0),
                );
            }
            let mut root = Element::from(
                scroll_area("diag-vlist-ok")
                    .w(300.0)
                    .h(viewport_h)
                    .overflow_y("auto")
                    .overflow_x("hidden")
                    .child(body),
            );
            let mut layout = LayoutTree::new();
            let result = layout.build_and_layout(&mut root, 300.0, viewport_h, [-1.0, -1.0]);
            let (_id, bounds, max_scroll, content_size) = result
                .scroll_metrics
                .iter()
                .find(|(id, ..)| id == "diag-vlist-ok")
                .expect("scroll metrics");
            let expect_h = count as f32 * item_h;
            assert!(
                content_size[1] >= expect_h - 1.0,
                "content_h={} want ≥ {}",
                content_size[1],
                expect_h
            );
            assert!(
                content_size[1] > bounds[3] + 1.0,
                "content must exceed viewport (content_h={} vp={})",
                content_size[1],
                bounds[3]
            );
            assert!(
                max_scroll[1] > 1.0,
                "max_scroll_y must be > 0, got {}",
                max_scroll[1]
            );
        });
    });
}

#[test]
fn silent_scroll_remounts_visible_rows_via_reactive_sync() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let id = "diag-vlist-silent";
            let item_h = 20.0_f32;
            let mut root = Element::from(
                virtual_list(id)
                    .item_count(100)
                    .item_height(item_h)
                    .height(100.0)
                    .overscan(1)
                    .render_item(|i| Element::new().text(format!("r{i:03}"))),
            );

            let mut texts = Vec::new();
            collect_texts(&root, &mut texts);
            assert!(
                texts.iter().any(|t| t == "r000"),
                "initial window should include r000, got {texts:?}"
            );

            use_signal(&scroll_offset_y_signal_key(id), || 0.0_f32).set_silent(40.0 * item_h);

            let mut layout = LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            sync_reactive_elements_subtree(&mut root, &mut layout, &mut impact);
            assert!(
                matches!(
                    impact,
                    LayoutStatus::StructureChanged | LayoutStatus::LayoutChanged
                ),
                "window change must dirty layout, got {impact:?}"
            );

            texts.clear();
            collect_texts(&root, &mut texts);
            assert!(
                texts.iter().any(|t| t == "r040"),
                "after silent scroll, window should include r040, got {texts:?}"
            );
            assert!(
                !texts.iter().any(|t| t == "r000"),
                "stale r000 must unmount after window moves, got {texts:?}"
            );

            let mut impact2 = LayoutStatus::Clean;
            sync_reactive_elements_subtree(&mut root, &mut layout, &mut impact2);
            assert_eq!(impact2, LayoutStatus::Clean);
        });
    });
}

#[test]
fn focused_index_remounts_and_reveals_offscreen_row_via_reactive_sync() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let id = "diag-vlist-focus";
            let focus = use_signal("diag-vlist-focus-index", || Some(0usize));
            let mut root = Element::from(
                virtual_list(id)
                    .item_count(100)
                    .item_height(20.0)
                    .height(100.0)
                    .overscan(1)
                    .focused_index_fn(move || focus.peek())
                    .render_item(|i| Element::new().text(format!("r{i:03}"))),
            );

            let mut texts = Vec::new();
            collect_texts(&root, &mut texts);
            assert!(texts.iter().any(|t| t == "r000"));

            focus.set_silent(Some(70));
            let mut layout = LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            sync_reactive_elements_subtree(&mut root, &mut layout, &mut impact);

            texts.clear();
            collect_texts(&root, &mut texts);
            assert!(
                texts.iter().any(|t| t == "r070"),
                "focused offscreen row must be mounted, got {texts:?}"
            );
            assert!(
                !texts.iter().any(|t| t == "r000"),
                "rows outside the focused window must unmount, got {texts:?}"
            );
            assert!(
                matches!(
                    impact,
                    LayoutStatus::StructureChanged | LayoutStatus::LayoutChanged
                ),
                "focus window change must dirty layout, got {impact:?}"
            );
        });
    });
}

#[test]
fn silent_scroll_reuses_overlapping_row_layout_nodes() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let id = "diag-vlist-reuse";
            let item_h = 20.0_f32;
            let mut root = Element::from(
                virtual_list(id)
                    .item_count(80)
                    .item_height(item_h)
                    .height(100.0)
                    .overscan(2)
                    .render_item(|i| Element::new().text(format!("r{i:03}"))),
            );

            // Stamp layout_node on mid-window rows before scrolling.
            if let Some(body) = find_vlist_body_mut(&mut root) {
                for child in &mut body.children {
                    if let Some(tid) = child.template_id.as_deref() {
                        if let Some(rest) = tid.strip_prefix(&format!("{id}__row_")) {
                            if let Ok(i) = rest.parse::<usize>() {
                                if (2..8).contains(&i) {
                                    child.layout_node =
                                        Some(taffy::prelude::NodeId::from(5000u64 + i as u64));
                                }
                            }
                        }
                    }
                }
            }

            use_signal(&scroll_offset_y_signal_key(id), || 0.0_f32).set_silent(4.0 * item_h);
            let mut layout = LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            sync_reactive_elements_subtree(&mut root, &mut layout, &mut impact);

            let body = find_vlist_body(&root).expect("vlist body");
            let reused: u32 = body
                .attrs
                .get("__jadix_vlist_reused")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            let created: u32 = body
                .attrs
                .get("__jadix_vlist_created")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            assert!(
                reused >= 4,
                "expected overlap reuse, reused={reused} created={created}"
            );
            assert!(created > 0, "expected some newly mounted rows");

            let node5 = body
                .children
                .iter()
                .find(|c| c.template_id.as_deref() == Some(&format!("{id}__row_5")))
                .and_then(|c| c.layout_node);
            assert_eq!(
                node5,
                Some(taffy::prelude::NodeId::from(5005u64)),
                "overlapping row must keep its layout_node"
            );
        });
    });
}
