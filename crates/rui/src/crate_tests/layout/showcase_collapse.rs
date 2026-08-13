//! Reproduce gallery ButtonShowcase: Show LayoutChanged must not collapse absolute Y.

use crate::app::reactive::reset_registered_anchors;
use crate::app::test_support::reconcile_elements;
use crate::components::button::button;
use crate::core::tree::{Element, LayoutTree};
use crate::primitives::{hstack, vstack};
use crate::renderer::font_loader;
use crate::text::install_shared_font_system;

fn dump_abs(
    tree: &LayoutTree,
    el: &Element,
    abs_x: f32,
    abs_y: f32,
    out: &mut Vec<(String, f32, f32, f32, f32)>,
) {
    let (x, y, w, h) = if let Some(nid) = el.layout_node {
        if let Ok(lay) = tree.taffy.layout(nid) {
            (
                abs_x + lay.location.x,
                abs_y + lay.location.y,
                lay.size.width,
                lay.size.height,
            )
        } else {
            (abs_x, abs_y, 0.0, 0.0)
        }
    } else {
        (abs_x, abs_y, 0.0, 0.0)
    };
    let label = el
        .text
        .clone()
        .or_else(|| el.template_id.clone())
        .unwrap_or_else(|| "?".into());
    if el.text.is_some() || el.template_id.is_some() {
        out.push((label, x, y, w, h));
    }
    let child_origin_x = if let Some(nid) = el.layout_node {
        tree.taffy
            .layout(nid)
            .map(|l| abs_x + l.location.x)
            .unwrap_or(abs_x)
    } else {
        abs_x
    };
    let child_origin_y = if let Some(nid) = el.layout_node {
        tree.taffy
            .layout(nid)
            .map(|l| abs_y + l.location.y)
            .unwrap_or(abs_y)
    } else {
        abs_y
    };
    for c in &el.children {
        dump_abs(tree, c, child_origin_x, child_origin_y, out);
    }
}

fn find_tid_mut<'a>(el: &'a mut Element, id: &str) -> Option<&'a mut Element> {
    if el.template_id.as_deref() == Some(id) {
        return Some(el);
    }
    for c in &mut el.children {
        if let Some(h) = find_tid_mut(c, id) {
            return Some(h);
        }
    }
    None
}

fn build_card() -> Element {
    Element::from(
        vstack()
            .w(800.0)
            .p("1rem")
            .gap("0.5rem")
            .child(
                Element::new()
                    .text("1) Button Variants + State")
                    .text_size(20.0)
                    .font_weight(700.0)
                    .w("100%"),
            )
            .child(Element::from(
                vstack()
                    .w("100%")
                    .gap("0.75rem")
                    .child(Element::from(
                        hstack()
                            .w("100%")
                            .gap("0.5rem")
                            .wrap("wrap")
                            .child(Element::from(
                                button("d1").child(Element::new().text("Default (0)")),
                            ))
                            .child(Element::from(
                                button("s1").child(Element::new().text("Secondary")),
                            )),
                    ))
                    .child(Element::new().template_id("show-anchor")),
            )),
    )
}

#[test]
fn subtree_compute_zeros_parent_location() {
    // Documents the Taffy footgun the live path must avoid.
    reset_registered_anchors();
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let mut card = build_card();
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut card, 800.0, 600.0, [-1.0, -1.0]);

    let mut show_new = Element::new()
        .template_id("show-anchor")
        .text("Selected variant: secondary")
        .text_size(14.0)
        .w("100%");
    let old_show = find_tid_mut(&mut card, "show-anchor").expect("show anchor");
    let _ = reconcile_elements(&mut show_new, old_show);
    let dirty_node = old_show.layout_node.expect("show layout node");
    *old_show = show_new;
    if let Some(el) = find_tid_mut(&mut card, "show-anchor") {
        el.layout_node = Some(dirty_node);
    }

    let dirty = std::collections::HashSet::from([dirty_node]);
    tree.mark_dirty_nodes_in_tree(&card, &dirty);
    let sub = tree
        .pick_subtree_compute_root(&dirty)
        .expect("nested show should pick a parent");
    let _ = tree.build_and_layout_fast_ex(
        &mut card,
        800.0,
        600.0,
        [-1.0, -1.0],
        false,
        false,
        Some(sub),
    );

    let mut after = Vec::new();
    dump_abs(&tree, &card, 0.0, 0.0, &mut after);
    let title_y = after
        .iter()
        .find(|(l, _, _, _, _)| l.contains("Button Variants"))
        .map(|(_, _, y, _, _)| *y)
        .expect("title");
    let btn_y = after
        .iter()
        .find(|(l, _, _, _, _)| l.starts_with("Default"))
        .map(|(_, _, y, _, _)| *y)
        .expect("button");
    assert!(
        btn_y <= title_y + 10.0,
        "expected subtree compute footgun: btn_y={btn_y} should collapse near title_y={title_y}"
    );
}

#[test]
fn showcase_show_toggle_keeps_title_above_buttons() {
    reset_registered_anchors();
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let mut card = build_card();
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut card, 800.0, 600.0, [-1.0, -1.0]);
    let mut before = Vec::new();
    dump_abs(&tree, &card, 0.0, 0.0, &mut before);
    let title_y = before
        .iter()
        .find(|(l, _, _, _, _)| l.contains("Button Variants"))
        .map(|(_, _, y, _, _)| *y)
        .expect("title");
    let btn_y = before
        .iter()
        .find(|(l, _, _, _, _)| l == "Default (0)")
        .map(|(_, _, y, _, _)| *y)
        .expect("button");
    assert!(
        btn_y > title_y + 10.0,
        "before toggle: title={title_y} btn={btn_y}"
    );

    let mut show_new = Element::new()
        .template_id("show-anchor")
        .text("Selected variant: secondary")
        .text_size(14.0)
        .w("100%");
    let old_show = find_tid_mut(&mut card, "show-anchor").expect("show anchor");
    let status = reconcile_elements(&mut show_new, old_show);
    let dirty_node = old_show.layout_node;
    *old_show = show_new;
    assert!(
        matches!(
            status,
            crate::core::tree::LayoutStatus::LayoutChanged
                | crate::core::tree::LayoutStatus::StructureChanged
        ),
        "show insert should dirty layout, got {status:?}"
    );

    // Live AppState LayoutChanged path: mark dirty + compute from app root (subtree_root=None).
    if let Some(n) = dirty_node {
        if let Some(el) = find_tid_mut(&mut card, "show-anchor") {
            el.layout_node = Some(n);
        }
        let dirty = std::collections::HashSet::from([n]);
        tree.mark_dirty_nodes_in_tree(&card, &dirty);
    }
    let _ =
        tree.build_and_layout_fast_ex(&mut card, 800.0, 600.0, [-1.0, -1.0], false, false, None);

    let mut after = Vec::new();
    dump_abs(&tree, &card, 0.0, 0.0, &mut after);
    let title_y2 = after
        .iter()
        .find(|(l, _, _, _, _)| l.contains("Button Variants"))
        .map(|(_, _, y, _, _)| *y)
        .expect("title after");
    let btn_y2 = after
        .iter()
        .find(|(l, _, _, _, _)| l.starts_with("Default"))
        .map(|(_, _, y, _, _)| *y)
        .expect("button after");
    let sel_y = after
        .iter()
        .find(|(l, _, _, _, _)| l.contains("Selected variant"))
        .map(|(_, _, y, _, _)| *y);
    assert!(
        btn_y2 > title_y2 + 10.0,
        "after Show root compute: title={title_y2} btn={btn_y2} sel={sel_y:?}"
    );
    if let Some(sy) = sel_y {
        assert!(
            sy > btn_y2,
            "selected label should sit below buttons: btn={btn_y2} sel={sy}"
        );
    }
}
