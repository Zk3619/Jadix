//! Header / Footer RSX string leaves must keep `sidebar-foreground` across a full
//! cascade (overlay enter / portal hoist). Idle frames skip cascade (`scroll_only`);
//! the first animation layout does not — without an inherit-lock, `TextStyle::default`
//! white flashes on light chrome then chrome-sync paints black again.

use crate::components::sidebar::{sidebar, SidebarMenuButton, SidebarMenuButtonProps};
use crate::core::reactivity::{create_effect, create_root};
use crate::core::tree::{parse_text_color, Element, LayoutStatus, LayoutTree};
use crate::layout::layout_tree::{RenderResult, TextLayoutInfo, ATTR_PORTAL_KEY};

const HEADER: &str = "HeaderChrome";
const NAV: &str = "NavItem";
const FOOTER: &str = "FooterChrome";
const DEFAULT_WHITE: [u8; 4] = [255, 255, 255, 255];

fn rsx_string_menu_button(id: &str, label: &str) -> Element {
    SidebarMenuButton(SidebarMenuButtonProps {
        id: id.into(),
        is_active: None,
        variant: None,
        size: None,
        class: None,
        on_click: None,
        children: Some(Element::from(label)),
        base: None,
    })
}

fn find_text<'a>(nodes: &'a [TextLayoutInfo], s: &str) -> Option<&'a TextLayoutInfo> {
    nodes.iter().find(|n| n.text == s)
}

fn shell_sidebar() -> Element {
    sidebar("flash-nav")
        .variant("inset")
        .w("13rem")
        .header(rsx_string_menu_button("flash-h", HEADER))
        .item(rsx_string_menu_button("flash-n", NAV))
        .footer(rsx_string_menu_button("flash-f", FOOTER))
        .into()
}

fn assert_chrome_color(result: &RenderResult, when: &str) {
    let fg = parse_text_color("sidebar-foreground");
    for label in [HEADER, NAV, FOOTER] {
        let node = find_text(&result.text_nodes, label)
            .unwrap_or_else(|| panic!("{when}: missing text node `{label}`"));
        assert_ne!(
            node.style.color_resolved(),
            DEFAULT_WHITE,
            "{when}: `{label}` flashed default white (got {:?})",
            node.style.color_resolved()
        );
        assert_eq!(
            node.style.color_resolved(),
            fg,
            "{when}: `{label}` color={:?} want sidebar-foreground={fg:?}",
            node.style.color_resolved()
        );
    }
}

#[test]
fn header_footer_rsx_string_color_survives_overlay_cascade() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let mut root = Element::new()
                .w("100%")
                .h("100%")
                .relative()
                .child(shell_sidebar());

            let mut layout = LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            crate::components::sidebar::sync_sidebar_menu_button_paint_subtree(
                &mut root,
                &mut layout,
                &mut impact,
            );
            let first = layout.build_and_layout(&mut root, 800.0, 600.0, [-1.0, -1.0]);
            assert_chrome_color(&first, "after chrome sync + cascade");

            let mut toast = Element::new()
                .w(160.0)
                .h(48.0)
                .absolute()
                .left(320.0)
                .top(8.0)
                .z_index(50)
                .child(Element::new().text("toast-body"));
            toast.set_attr("rui_portal", "1");
            toast.set_attr(ATTR_PORTAL_KEY, "toast:flash");
            toast.style.opacity = 0.0;
            toast.style.scale = [0.95, 0.95];
            root = root.child(toast);

            // Overlay enter: full rebuild cascades again *without* an intervening chrome sync
            // (matches AppState: sync then hoist then build_and_layout).
            let second = layout.build_and_layout(&mut root, 800.0, 600.0, [-1.0, -1.0]);
            assert_chrome_color(&second, "after overlay hoist cascade");
        });
    });
}
