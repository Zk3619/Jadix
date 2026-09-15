//! Last visible SidebarContent row must not paint past the footer separator.
//!
//! Gallery inset rail: Header / Separator / Content / Separator / Footer.
//! Overflow lives on Content (inner is `overflow: visible` so Header/Footer
//! glyphs survive). A last-item leak onto the footer rule is a clip/layout bug.

use crate::components::sidebar::{
    sidebar_group, sidebar_group_content, sidebar_group_label, sidebar_menu, sidebar_menu_button,
    sidebar_provider_scope, Sidebar, SidebarContent, SidebarContentProps, SidebarFooter,
    SidebarFooterProps, SidebarHeader, SidebarHeaderProps, SidebarProps, SidebarSeparator,
    SidebarSeparatorProps,
};
use crate::core::reactivity::{create_effect, create_root};
use crate::core::tree::{Element, LayoutTree};
use crate::layout::layout_tree::TextLayoutInfo;
use crate::utils::text_node::apply_label_defaults;

fn pack(kids: Vec<Element>) -> Element {
    let mut frag = Element::new();
    frag.set_attr("rui_fragment", "1");
    frag.children(kids)
}

fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
    if el.attrs.get("data-slot").map(String::as_str) == Some(slot) {
        return Some(el);
    }
    el.children.iter().find_map(|c| find_slot(c, slot))
}

fn find_all_slots<'a>(el: &'a Element, slot: &str, out: &mut Vec<&'a Element>) {
    if el.attrs.get("data-slot").map(String::as_str) == Some(slot) {
        out.push(el);
    }
    for c in &el.children {
        find_all_slots(c, slot, out);
    }
}

fn bottom(b: [f32; 4]) -> f32 {
    b[1] + b[3]
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
            w: Some("13rem".into()),
            side: None,
            variant: Some("inset".into()),
            collapsible: Some("icon".into()),
            children: Some(pack(vec![
                SidebarHeader(SidebarHeaderProps {
                    children: Some(nav_item("hdr", "RUI Gallery")),
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

fn last_visible_nav_text<'a>(
    nodes: &'a [TextLayoutInfo],
    labels: &[&str],
    host: [f32; 4],
) -> Option<&'a TextLayoutInfo> {
    let host_bottom = bottom(host);
    nodes
        .iter()
        .filter(|n| labels.iter().any(|l| n.text == *l))
        .filter(|n| n.y < host_bottom - 0.5)
        .max_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal))
}

fn check_viewport(vp_h: f32, labels: &[&str]) {
    let mut root = gallery_like_sidebar(labels);
    let mut layout = LayoutTree::new();
    let result = layout.build_and_layout(&mut root, 800.0, vp_h, [-1.0, -1.0]);

    let host = find_slot(&root, "sidebar-content-host").expect("content host");
    let content = find_slot(&root, "sidebar-content").expect("content");
    let footer = find_slot(&root, "sidebar-footer").expect("footer");
    let mut seps = Vec::new();
    find_all_slots(&root, "sidebar-separator", &mut seps);
    assert!(
        seps.len() >= 2,
        "vp_h={vp_h}: expected 2 separators, got {}",
        seps.len()
    );
    let footer_sep = seps[1];
    let mut buttons = Vec::new();
    find_all_slots(&root, "sidebar-menu-button", &mut buttons);
    let host_b = host.layout_bounds;
    let host_bottom = bottom(host_b);
    let sep_b = footer_sep.layout_bounds;
    let sep_top = sep_b[1];
    let last_visible_btn = buttons
        .iter()
        .filter(|b| {
            b.template_id.as_deref() != Some("hdr") && b.template_id.as_deref() != Some("ftr")
        })
        .filter(|b| b.layout_bounds[1] < host_bottom - 0.5)
        .max_by(|a, b| {
            a.layout_bounds[1]
                .partial_cmp(&b.layout_bounds[1])
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .expect("visible nav button");

    let last_text =
        last_visible_nav_text(&result.text_nodes, labels, host_b).expect("visible nav label");
    let text_bottom = last_text.y + last_text.height;
    let clip_bottom = last_text
        .clip_bounds
        .map(|c| c[1] + c[3])
        .unwrap_or(f32::INFINITY);
    let glyphon_clip_bottom = last_text
        .clip_bounds
        .and_then(crate::renderer::prepare_text::integerize_clip_rect)
        .map(|c| c[1] + c[3])
        .unwrap_or(f32::INFINITY);
    let last_b = last_visible_btn.layout_bounds;
    let last_bottom = bottom(last_b);

    let dump = format!(
        "vp_h={vp_h}\n\
         host={:?} bottom={host_bottom}\n\
         content={:?}\n\
         footer_sep={:?} top={sep_top} h={}\n\
         footer={:?}\n\
         last_visible_btn={:?} bottom={last_bottom} id={:?}\n\
         last_visible_text='{}' y={} h={} bottom={text_bottom} clip={:?}\n\
         clip_bottom={clip_bottom} glyphon_ceil={glyphon_clip_bottom}\n\
         host-sep={} btn-sep={} clip-sep={} text-sep={} glyphon-sep={}",
        host_b,
        content.layout_bounds,
        sep_b,
        sep_b[3],
        footer.layout_bounds,
        last_b,
        last_visible_btn.template_id,
        last_text.text,
        last_text.y,
        last_text.height,
        last_text.clip_bounds,
        sep_top - host_bottom,
        sep_top - last_bottom,
        sep_top - clip_bottom,
        sep_top - text_bottom,
        sep_top - glyphon_clip_bottom,
    );
    assert!(
        host_bottom <= sep_top + 0.01,
        "content host must not overlap the footer separator\n{dump}"
    );
    assert!(
        clip_bottom <= sep_top + 0.01,
        "visible nav text clip must stop at or above the footer separator\n{dump}"
    );
    assert!(
        glyphon_clip_bottom <= sep_top + 0.01,
        "glyphon TextBounds must not expand clip onto the footer separator\n{dump}"
    );
}

#[test]
fn last_content_item_must_not_cross_footer_separator() {
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
            // Overflow + integer and fractional leftover (ceil leak only shows on fractions).
            for vp_h in [420.0, 480.0, 500.7, 512.25, 533.3] {
                check_viewport(vp_h, &labels);
            }
        });
    });
}
