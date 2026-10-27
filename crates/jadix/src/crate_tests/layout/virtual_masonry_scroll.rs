#![cfg(feature = "product-tests")]
//! VirtualMasonry: 千级项只挂可见格；静默滚动换窗。

use crate::app::test_support::sync_reactive_elements_subtree;
use crate::components::virtual_masonry::virtual_masonry;
use crate::core::reactivity::{create_effect, create_root, use_signal};
use crate::core::scroll_viewport::{scroll_offset_y_signal_key, scroll_viewport_w_signal_key};
use crate::core::tree::{Element, LayoutStatus, LayoutTree};
use crate::primitives::r#box::r#box;

fn find_body<'a>(el: &'a Element) -> Option<&'a Element> {
    if el.attrs.contains_key("__jadix_vmasonry_range") {
        return Some(el);
    }
    for child in &el.children {
        if let Some(b) = find_body(child) {
            return Some(b);
        }
    }
    None
}

fn cell_count(el: &Element) -> usize {
    find_body(el)
        .map(|b| {
            b.children
                .iter()
                .filter(|c| {
                    c.template_id
                        .as_deref()
                        .is_some_and(|id| id.contains("__cell_"))
                })
                .count()
        })
        .unwrap_or(0)
}

#[test]
fn thousand_items_only_mount_visible_window() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let id = "diag-vmasonry-1k";
            let mut root = Element::from(
                virtual_masonry(id)
                    .item_count(1000)
                    .columns(3)
                    .gap(8.0)
                    .height(160.0)
                    .overscan_viewports(1.0)
                    .item_height(|i, _| 40.0 + (i % 5) as f32 * 8.0)
                    .render_item(|i, w, h| {
                        Element::from(r#box().w(w).h(h).text(format!("m{i:04}")))
                    }),
            );
            use_signal(&scroll_viewport_w_signal_key(id), || 0.0_f32).set_silent(320.0);
            let mut layout = LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            sync_reactive_elements_subtree(&mut root, &mut layout, &mut impact);
            let n = cell_count(&root);
            assert!(
                n > 0 && n < 120,
                "visible window should be far below 1000, got {n}"
            );
            let body = find_body(&root).expect("masonry body");
            let total: f32 = body
                .attrs
                .get("__jadix_vmasonry_total_h")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.0);
            assert!(
                total > 160.0 * 4.0,
                "scroll height must exceed a few viewports, got {total}"
            );
        });
    });
}

#[test]
fn silent_scroll_remounts_masonry_window() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let id = "diag-vmasonry-scroll";
            let mut root = Element::from(
                virtual_masonry(id)
                    .item_count(200)
                    .columns(2)
                    .gap(0.0)
                    .height(100.0)
                    .overscan_viewports(0.0)
                    .item_height(|_, _| 50.0)
                    .render_item(|i, w, h| {
                        Element::from(r#box().w(w).h(h).text(format!("m{i:03}")))
                    }),
            );
            use_signal(&scroll_viewport_w_signal_key(id), || 0.0_f32).set_silent(200.0);

            let mut layout = LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            sync_reactive_elements_subtree(&mut root, &mut layout, &mut impact);

            fn has_text(el: &Element, needle: &str) -> bool {
                el.text.as_deref() == Some(needle)
                    || el.children.iter().any(|c| has_text(c, needle))
            }
            assert!(has_text(&root, "m000"), "first window includes m000");

            use_signal(&scroll_offset_y_signal_key(id), || 0.0_f32).set_silent(400.0);
            let mut impact2 = LayoutStatus::Clean;
            sync_reactive_elements_subtree(&mut root, &mut layout, &mut impact2);
            assert!(
                matches!(
                    impact2,
                    LayoutStatus::StructureChanged | LayoutStatus::LayoutChanged
                ),
                "scroll must remount window, got {impact2:?}"
            );
            assert!(
                has_text(&root, "m016") || has_text(&root, "m008"),
                "scrolled window should include later tiles"
            );
            assert!(
                !has_text(&root, "m000"),
                "m000 must drop after scrolling far"
            );
        });
    });
}
