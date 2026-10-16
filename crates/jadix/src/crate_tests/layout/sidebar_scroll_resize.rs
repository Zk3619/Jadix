//! Short-screen sidebar scroll must clamp when the window grows until content fits.
//!
//! `is_stale_zero_max_sample` used to treat any `max→0` as a nested clip-cull and
//! keep the short-viewport metrics. After scrolling a gallery rail, raising the
//! window left the content translated by the old offset.

use std::collections::HashMap;

use crate::app::scroll_support::{clamp_scroll_offset_and_target, merge_scroll_metrics_cache};
use crate::components::sidebar::{
    sidebar_group, sidebar_group_content, sidebar_group_label, sidebar_menu, sidebar_menu_button,
    sidebar_provider_scope, Sidebar, SidebarContent, SidebarContentProps, SidebarFooter,
    SidebarFooterProps, SidebarHeader, SidebarHeaderProps, SidebarProps, SidebarSeparator,
    SidebarSeparatorProps,
};
use crate::core::reactivity::{create_effect, create_root};
use crate::core::tree::{Element, LayoutTree};
use crate::utils::text_node::apply_label_defaults;

fn pack(kids: Vec<Element>) -> Element {
    let mut frag = Element::new();
    frag.set_attr("jadix_fragment", "1");
    frag.children(kids)
}

fn nav_item(id: &str, label: &str) -> Element {
    sidebar_menu_button(id)
        .child(apply_label_defaults(
            Element::new()
                .text(label.to_string())
                .text_size(0.875 * 16.0),
        ))
        .into()
}

fn gallery_like_sidebar(labels: &[&str]) -> Element {
    let items: Vec<Element> = labels
        .iter()
        .enumerate()
        .map(|(i, label)| nav_item(&format!("nav-{i}"), label))
        .collect();
    let group = sidebar_group()
        .child(sidebar_group_label().text("导航"))
        .child(sidebar_group_content().child(sidebar_menu().child(pack(items))))
        .into();
    sidebar_provider_scope("gal-clip", None, true, Some("inset"), move || {
        Sidebar(SidebarProps {
            id: "gal-sidebar".into(),
            rail_w: Some("13rem".into()),
            side: None,
            variant: Some("inset".into()),
            collapsible: Some("icon".into()),
            children: Some(pack(vec![
                SidebarHeader(SidebarHeaderProps {
                    children: Some(nav_item("hdr", "Jadix Gallery")),
                    base: None,
                }),
                SidebarSeparator(SidebarSeparatorProps {
                    children: None,
                    base: None,
                }),
                SidebarContent(SidebarContentProps {
                    children: Some(group),
                    base: None,
                }),
                SidebarSeparator(SidebarSeparatorProps {
                    children: None,
                    base: None,
                }),
                SidebarFooter(SidebarFooterProps {
                    children: Some(nav_item("ftr", "Gallery")),
                    base: None,
                }),
            ])),
            base: None,
        })
    })
}

fn sidebar_content_metrics(
    metrics: &[(String, [f32; 4], [f32; 2], [f32; 2])],
) -> ([f32; 4], [f32; 2], [f32; 2]) {
    metrics
        .iter()
        .find(|(id, ..)| id == "gal-clip__sidebar-content")
        .map(|(_, bounds, max, content)| (*bounds, *max, *content))
        .expect("scroll metrics for SidebarContent")
}

#[test]
fn growing_window_until_sidebar_fits_clamps_stale_offset() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let labels = [
                "基础",
                "指针事件",
                "表单",
                "浮层 / 反馈",
                "数据",
                "绘制",
                "导航",
                "布局",
                "文本",
                "关于",
                "额外甲",
                "额外乙",
                "额外丙",
            ];
            let mut layout = LayoutTree::new();

            let mut short = gallery_like_sidebar(&labels);
            let short_result = layout.build_and_layout(&mut short, 800.0, 360.0, [-1.0, -1.0]);
            let (short_bounds, short_max, short_content) =
                sidebar_content_metrics(&short_result.scroll_metrics);
            assert!(
                short_max[1] > 8.0,
                "short window must overflow SidebarContent (max_y={}, vh={}, ch={})",
                short_max[1],
                short_bounds[3],
                short_content[1]
            );

            let mut tall = gallery_like_sidebar(&labels);
            let tall_result = layout.build_and_layout(&mut tall, 800.0, 900.0, [-1.0, -1.0]);
            let (tall_bounds, tall_max, tall_content) =
                sidebar_content_metrics(&tall_result.scroll_metrics);
            assert!(
                tall_bounds[3] > short_bounds[3] + 8.0,
                "SidebarContent viewport must grow with the window (short_vh={} tall_vh={})",
                short_bounds[3],
                tall_bounds[3]
            );
            assert!(
                tall_max[1] < short_max[1] - 8.0 || tall_max[1] < 0.5,
                "max_scroll_y must shrink when the rail gets taller (short_max={} tall_max={})",
                short_max[1],
                tall_max[1]
            );

            let mut cache = HashMap::new();
            cache.insert(
                "gal-clip__sidebar-content".into(),
                (short_bounds, short_max, short_content),
            );
            merge_scroll_metrics_cache(
                &mut cache,
                [(
                    "gal-clip__sidebar-content".into(),
                    tall_bounds,
                    tall_max,
                    tall_content,
                )],
            );
            let merged = cache
                .get("gal-clip__sidebar-content")
                .expect("sidebar metrics kept");
            assert!(
                (merged.1[1] - tall_max[1]).abs() < 0.5,
                "grown viewport must replace short-screen max, got {:?} want {:?}",
                merged.1,
                tall_max
            );

            let scrolled = short_max[1];
            let (offset, target) = clamp_scroll_offset_and_target(scrolled, scrolled, merged.1[1]);
            assert!(
                (offset - tall_max[1]).abs() < 0.5 && (target - tall_max[1]).abs() < 0.5,
                "stale short-screen offset must clamp to new max (offset={offset} target={target} max={})",
                merged.1[1]
            );
        });
    });
}
