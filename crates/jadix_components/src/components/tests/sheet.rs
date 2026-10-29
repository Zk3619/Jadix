use super::*;
use crate::utils::popup::{panel_block_clicks, portal_panel};

fn has_portal_mark(el: &Element) -> bool {
    el.attrs.get("jadix_portal").map(String::as_str) == Some("1")
        || el.children.iter().any(has_portal_mark)
}

fn portal_nests_portal(el: &Element) -> bool {
    let self_p = el.attrs.get("jadix_portal").map(String::as_str) == Some("1");
    if self_p && el.children.iter().any(has_portal_mark) {
        return true;
    }
    el.children.iter().any(portal_nests_portal)
}

#[test]
fn nested_portal_panel_is_the_old_sheet_bug() {
    let panel = panel_block_clicks(Element::new().text("sheet"), 1001);
    let stage = portal_panel(Element::new().child(panel), 1001);
    assert!(
        portal_nests_portal(&stage),
        "portal stage wrapping a panel_block_clicks child nests jadix_portal"
    );
}

#[test]
fn responsive_side_sheet_clamps_explicit_width_to_viewport() {
    use crate::core::tree::LayoutTree;
    use crate::style::{resolve_class_variants, VariantContext};

    for side in [SheetSide::Left, SheetSide::Right] {
        for width in [320.0, 375.0, 639.5, 640.0, 1024.0] {
            let mut panel = apply_sheet_shell_surface(Element::new(), side, Some("50rem"), None)
                .class("shadow-none");
            resolve_class_variants(
                &mut panel,
                &VariantContext {
                    viewport_w: width,
                    ..Default::default()
                },
            );
            let mut root = Element::new().relative().w(width).h(600.0).child(panel);
            LayoutTree::new().build_and_layout(&mut root, width, 600.0, [-1.0, -1.0]);
            let bounds = root.children[0].layout_bounds;
            assert!(
                bounds[0] >= -0.5 && bounds[0] + bounds[2] <= width + 0.5,
                "side={side:?}, width={width}: {bounds:?}"
            );
        }
    }
}

#[test]
fn sheet_side_parses_shadcn_tokens() {
    assert_eq!(SheetSide::from("right"), SheetSide::Right);
    assert_eq!(SheetSide::from("LEFT"), SheetSide::Left);
    assert_eq!(SheetSide::from("top"), SheetSide::Top);
    assert_eq!(SheetSide::from("bottom"), SheetSide::Bottom);
}

#[test]
fn sheet_shell_stays_absolute_on_dock_edge() {
    use taffy::style::Position;
    let right = apply_sheet_shell_surface(Element::new(), SheetSide::Right, None, None);
    assert_eq!(right.style.position, Position::Absolute);
    assert_eq!(
        swallow_clicks(right).style.position,
        Position::Absolute,
        "swallow_clicks must not reset dock positioning to relative"
    );
    let bottom = apply_sheet_shell_surface(Element::new(), SheetSide::Bottom, None, None);
    assert_eq!(bottom.style.position, Position::Absolute);
}
