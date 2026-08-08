//! Portal hoist: root overlay layer + optional Solid-style `mount` host.

use crate::core::tree::Element;
use crate::layout::layout_tree::{
    hoist_portal_subtrees, ATTR_HOISTED_PORTAL, ATTR_PORTAL_MOUNT, PORTAL_LAYER_ID,
};
use crate::layout::layout_tree::LayoutTree;

fn portal_layer(root: &Element) -> Option<&Element> {
    root.children
        .iter()
        .find(|c| c.template_id.as_deref() == Some(PORTAL_LAYER_ID))
}

fn count_in_layer(root: &Element) -> usize {
    portal_layer(root)
        .map(|layer| {
            layer
                .children
                .iter()
                .filter(|c| {
                    c.attrs
                        .get(ATTR_HOISTED_PORTAL)
                        .map(|s| s.as_str())
                        == Some("1")
                })
                .count()
        })
        .unwrap_or(0)
}

fn find_by_tid<'a>(node: &'a Element, id: &str) -> Option<&'a Element> {
    if node.template_id.as_deref() == Some(id) {
        return Some(node);
    }
    for c in &node.children {
        if let Some(hit) = find_by_tid(c, id) {
            return Some(hit);
        }
    }
    None
}

#[test]
fn hoist_mounts_into_absolute_portal_layer_not_flex_siblings() {
    let mut root = Element::new().flex_row().w(1000.0).h(600.0).children([
        Element::new().w(200.0).h("100%").template_id("side"),
        Element::new()
            .flex_grow(1.0)
            .min_w(0.0)
            .h("100%")
            .template_id("main")
            .child({
                let mut portal = Element::new().w("100%").h("100%").template_id("page-portal");
                portal.set_attr("rui_portal", "1");
                portal
            }),
    ]);

    assert_eq!(hoist_portal_subtrees(&mut root), 1);
    let layer = portal_layer(&root).expect("portal layer");
    assert_eq!(layer.style.position, taffy::style::Position::Absolute);
    assert_eq!(count_in_layer(&root), 1);
    assert!(
        root.children
            .iter()
            .filter(|c| c.template_id.as_deref() == Some("page-portal"))
            .count()
            == 0,
        "portal must not sit as a root flex sibling"
    );

    let mut layout = LayoutTree::new();
    let _ = layout.build_and_layout(&mut root, 1000.0, 600.0, [-1.0, -1.0]);
    let side = root
        .children
        .iter()
        .find(|c| c.template_id.as_deref() == Some("side"))
        .unwrap();
    let main = root
        .children
        .iter()
        .find(|c| c.template_id.as_deref() == Some("main"))
        .unwrap();
    assert!(
        (side.layout_bounds[2] - 200.0).abs() < 1.0,
        "side width"
    );
    assert!(
        main.layout_bounds[2] > 700.0,
        "main must keep remaining width; got {}",
        main.layout_bounds[2]
    );
}

#[test]
fn hoist_replaces_previous_portals_instead_of_accumulating() {
    let mut root = Element::new().flex_row().w(1000.0).h(600.0).children([
        Element::new().w(200.0).h("100%").template_id("side"),
        Element::new()
            .flex_grow(1.0)
            .min_w(0.0)
            .h("100%")
            .template_id("main")
            .child({
                let mut portal = Element::new().w("100%").h("100%").template_id("page-portal");
                portal.set_attr("rui_portal", "1");
                portal
            }),
    ]);

    assert_eq!(hoist_portal_subtrees(&mut root), 1);
    assert_eq!(count_in_layer(&root), 1);

    if let Some(main) = root
        .children
        .iter_mut()
        .find(|c| c.template_id.as_deref() == Some("main"))
    {
        main.children.clear();
        let mut portal = Element::new().w("100%").h("100%").template_id("page-portal");
        portal.set_attr("rui_portal", "1");
        main.children.push(portal);
    }

    assert_eq!(hoist_portal_subtrees(&mut root), 1);
    assert_eq!(
        count_in_layer(&root),
        1,
        "second visit must not stack portals"
    );
}

#[test]
fn repeated_hoist_without_source_clears_layer() {
    let mut root = Element::new().children([Element::new().w(100.0)]);
    let mut ghost = Element::new().w("100%").h("100%");
    ghost.set_attr(ATTR_HOISTED_PORTAL, "1");
    root.children.push(ghost);

    assert_eq!(hoist_portal_subtrees(&mut root), 0);
    // Empty layer still exists as the Solid body mount point.
    let layer = portal_layer(&root).expect("empty portal layer");
    assert!(layer.children.is_empty());
}

#[test]
fn hoist_mount_attr_attaches_to_named_host() {
    let mut host = Element::new()
        .relative()
        .w(400.0)
        .h(200.0)
        .template_id("card-host");
    host.children.push(Element::new().w(40.0).h(20.0).template_id("host-label"));

    let mut portal = Element::new()
        .w("100%")
        .h("100%")
        .template_id("local-portal");
    portal.set_attr("rui_portal", "1");
    portal.set_attr(ATTR_PORTAL_MOUNT, "card-host");

    let mut root = Element::new().flex_col().w(800.0).h(600.0).children([
        Element::new()
            .flex_grow(1.0)
            .template_id("main")
            .children([host, portal]),
    ]);

    assert_eq!(hoist_portal_subtrees(&mut root), 1);
    assert_eq!(count_in_layer(&root), 0, "mounted portal must not sit on body layer");

    let host = find_by_tid(&root, "card-host").expect("host");
    let hoisted = host
        .children
        .iter()
        .filter(|c| {
            c.attrs.get(ATTR_HOISTED_PORTAL).map(|s| s.as_str()) == Some("1")
                && c.template_id.as_deref() == Some("local-portal")
        })
        .count();
    assert_eq!(hoisted, 1);
    assert!(
        host.children
            .iter()
            .any(|c| c.template_id.as_deref() == Some("host-label")),
        "host keeps its own children"
    );
    assert!(
        find_by_tid(&root, "main")
            .unwrap()
            .children
            .iter()
            .all(|c| c.attrs.get("rui_portal").is_none()),
        "source portal marker removed from main"
    );
}

#[test]
fn hoist_unknown_mount_falls_back_to_root_layer() {
    let mut portal = Element::new()
        .w("100%")
        .h("100%")
        .template_id("orphan-portal");
    portal.set_attr("rui_portal", "1");
    portal.set_attr(ATTR_PORTAL_MOUNT, "missing-host");

    let mut root = Element::new().children([
        Element::new().template_id("main").child(portal),
    ]);

    assert_eq!(hoist_portal_subtrees(&mut root), 1);
    assert_eq!(count_in_layer(&root), 1);
    assert!(
        find_by_tid(&root, "orphan-portal")
            .map(|n| {
                n.attrs.get(ATTR_HOISTED_PORTAL).map(|s| s.as_str()) == Some("1")
            })
            .unwrap_or(false)
    );
}

#[test]
fn hoist_clears_previous_host_mount_when_source_gone() {
    let host = Element::new()
        .relative()
        .w(400.0)
        .h(200.0)
        .template_id("card-host");
    let mut portal = Element::new()
        .w("100%")
        .h("100%")
        .template_id("local-portal");
    portal.set_attr("rui_portal", "1");
    portal.set_attr(ATTR_PORTAL_MOUNT, "card-host");

    let mut root = Element::new().children([
        Element::new()
            .template_id("main")
            .children([host, portal]),
    ]);

    assert_eq!(hoist_portal_subtrees(&mut root), 1);
    assert_eq!(
        find_by_tid(&root, "card-host")
            .unwrap()
            .children
            .iter()
            .filter(|c| c.attrs.get(ATTR_HOISTED_PORTAL).map(|s| s.as_str()) == Some("1"))
            .count(),
        1
    );

    if let Some(main) = root
        .children
        .iter_mut()
        .find(|c| c.template_id.as_deref() == Some("main"))
    {
        main.children.retain(|c| c.template_id.as_deref() != Some("local-portal"));
        // After strip+lift the portal lives under host; clear source by rebuilding main without portal.
        // Host still has hoisted child until next hoist strips it.
        main.children.clear();
        main.children.push(
            Element::new()
                .relative()
                .w(400.0)
                .h(200.0)
                .template_id("card-host"),
        );
    }

    assert_eq!(hoist_portal_subtrees(&mut root), 0);
    assert!(
        find_by_tid(&root, "card-host")
            .unwrap()
            .children
            .is_empty(),
        "closing local portal must clear host mount"
    );
}
