//! Overlay scrollbar must sit on `SidebarContent`, not the Header/Footer column.
//!
//! `Sidebar` `take_packed`s its children. An anonymous two-child host (viewport +
//! bar) used to be peeled so the track became a Header sibling: Taffy then
//! placed `top:0` against the rail while overlay height stayed at content `vh`.

use crate::components::scrollbar::SCROLLBAR_TRACK_FOCUS_PREFIX;
use crate::components::sidebar::{sidebar, sidebar_menu_button};
use crate::core::reactivity::{create_effect, create_root};
use crate::core::tree::{Element, LayoutTree};

fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
    if el.attrs.get("data-slot").map(String::as_str) == Some(slot) {
        return Some(el);
    }
    el.children.iter().find_map(|c| find_slot(c, slot))
}

fn find_focus_prefix<'a>(el: &'a Element, prefix: &str) -> Option<&'a Element> {
    if el
        .focus_id
        .as_deref()
        .is_some_and(|f| f.starts_with(prefix))
    {
        return Some(el);
    }
    el.children
        .iter()
        .find_map(|c| find_focus_prefix(c, prefix))
}

#[test]
fn overlay_track_aligns_with_sidebar_content_not_header() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let items: Vec<Element> = (0..20)
                .map(|i| {
                    sidebar_menu_button(format!("nav-{i}"))
                        .text(format!("Item {i}"))
                        .into()
                })
                .collect();
            let mut root: Element = sidebar("sb-bar")
                .variant("inset")
                .w("13rem")
                .provider(true)
                .header(Element::new().h(48.0).w("100%").text("HeaderChrome"))
                .items(items)
                .footer(Element::new().h(40.0).w("100%").text("FooterChrome"))
                .into();

            let mut layout = LayoutTree::new();
            let _ = layout.build_and_layout(&mut root, 800.0, 480.0, [-1.0, -1.0]);

            let header = find_slot(&root, "sidebar-header").expect("header");
            let content = find_slot(&root, "sidebar-content").expect("content");
            let track = find_focus_prefix(&root, SCROLLBAR_TRACK_FOCUS_PREFIX).expect("track");

            let [_, hy, _, hh] = header.layout_bounds;
            let [_, cy, _, _ch] = content.layout_bounds;
            let [_, ty, _, _th] = track.layout_bounds;

            assert!(
                cy >= hy + hh - 1.0,
                "content must start below header: header={hy}+{hh} content={cy}"
            );
            assert!(
                (ty - hy).abs() > 8.0,
                "track must not start at SidebarHeader (track.y={ty} header.y={hy})"
            );
            assert!(
                (ty - cy).abs() < 4.0,
                "track top must match SidebarContent, not the rail: track.y={ty} content.y={cy}"
            );
        });
    });
}
