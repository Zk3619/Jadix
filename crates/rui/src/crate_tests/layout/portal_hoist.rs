//! Portal hoist: root overlay layer + optional Solid-style `mount` host.

use crate::core::tree::Element;
use crate::layout::layout_tree::LayoutTree;
use crate::layout::layout_tree::{
    escapes_ancestor_overflow, hoist_portal_subtrees, ATTR_HOISTED_MOUNT, ATTR_HOISTED_PORTAL,
    ATTR_PORTAL_KEY, ATTR_PORTAL_MOUNT, ATTR_PORTAL_SLOT, PORTAL_LAYER_ID,
};

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
                .filter(|c| c.attrs.get(ATTR_HOISTED_PORTAL).map(|s| s.as_str()) == Some("1"))
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
                let mut portal = Element::new()
                    .w("100%")
                    .h("100%")
                    .template_id("page-portal");
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
    assert!((side.layout_bounds[2] - 200.0).abs() < 1.0, "side width");
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
                let mut portal = Element::new()
                    .w("100%")
                    .h("100%")
                    .template_id("page-portal");
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
        let mut portal = Element::new()
            .w("100%")
            .h("100%")
            .template_id("page-portal");
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
    ghost.set_attr(ATTR_PORTAL_KEY, "ghost");
    root.children.push(ghost);

    // No slot / rui_portal source → retention prune drops the orphan and reports dirty.
    assert!(
        hoist_portal_subtrees(&mut root) >= 1,
        "orphan hoisted nodes must be pruned"
    );
    let layer = portal_layer(&root).expect("empty portal layer");
    assert!(layer.children.is_empty());
    assert!(
        root.children
            .iter()
            .all(|c| c.attrs.get(ATTR_HOISTED_PORTAL).map(|s| s.as_str()) != Some("1")),
        "orphans outside the layer must also be pruned"
    );
}

#[test]
fn hoist_mount_attr_attaches_to_named_host() {
    let mut host = Element::new()
        .relative()
        .w(400.0)
        .h(200.0)
        .template_id("card-host");
    host.children
        .push(Element::new().w(40.0).h(20.0).template_id("host-label"));

    let mut portal = Element::new()
        .w("100%")
        .h("100%")
        .template_id("local-portal");
    portal.style.position_fixed = true;
    portal.set_attr("rui_portal", "1");
    portal.set_attr(ATTR_PORTAL_MOUNT, "card-host");

    let mut root = Element::new()
        .flex_col()
        .w(800.0)
        .h(600.0)
        .children([Element::new()
            .flex_grow(1.0)
            .template_id("main")
            .children([host, portal])]);

    assert_eq!(hoist_portal_subtrees(&mut root), 1);
    assert_eq!(
        count_in_layer(&root),
        0,
        "mounted portal must not sit on body layer"
    );

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
    let local = host
        .children
        .iter()
        .find(|c| c.template_id.as_deref() == Some("local-portal"))
        .expect("local portal");
    assert!(
        !local.style.position_fixed,
        "local mount must stay host-absolute, not viewport-fixed"
    );
    assert_eq!(
        local.attrs.get(ATTR_HOISTED_MOUNT).map(String::as_str),
        Some("card-host")
    );
    assert!(
        !escapes_ancestor_overflow(local),
        "local mount must clip and scroll with the host"
    );
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
fn local_mount_clears_stacking_context_when_overlay_closes() {
    // Stamping `data-stacking-context` on the host is required for scrim/glyph order,
    // but leaving it after close bubbles `collect_skip_live` up the gallery column
    // and keeps page_stack_bands live — scroll hitch after a one-shot local Portal.
    let mut host = Element::new()
        .relative()
        .w(400.0)
        .h(200.0)
        .template_id("card-host");
    host.children
        .push(Element::new().w(40.0).h(20.0).template_id("host-label"));

    let mut portal = Element::new()
        .w("100%")
        .h("100%")
        .template_id("local-portal")
        .z_index(2);
    portal.set_attr("rui_portal", "1");
    portal.set_attr(ATTR_PORTAL_KEY, "local-portal");
    portal.set_attr(ATTR_PORTAL_MOUNT, "card-host");

    let mut root = Element::new().w(800.0).h(600.0).child(
        Element::new()
            .template_id("main")
            .children([host, portal]),
    );

    assert!(hoist_portal_subtrees(&mut root) >= 1);
    let host = find_by_tid(&root, "card-host").expect("host");
    assert_eq!(
        host.attrs.get("data-stacking-context").map(String::as_str),
        Some("1"),
        "open local overlay must split host paint bands"
    );

    fn strip_slots(el: &mut Element) {
        el.children
            .retain(|c| c.attrs.get(ATTR_PORTAL_SLOT).is_none());
        for c in &mut el.children {
            strip_slots(c);
        }
    }
    strip_slots(&mut root);
    let _ = hoist_portal_subtrees(&mut root);
    let host = find_by_tid(&root, "card-host").expect("host after close");
    assert!(
        host.attrs.get("data-stacking-context").is_none(),
        "closed local overlay must not leave the host as a stacking context"
    );
    assert!(
        host.children
            .iter()
            .all(|c| c.attrs.get(ATTR_HOISTED_PORTAL).map(String::as_str) != Some("1")),
        "hoisted local overlay must be pruned"
    );
}

#[test]
fn local_mount_keeps_author_stacking_context_after_close() {
    let mut host = Element::new()
        .relative()
        .w(400.0)
        .h(200.0)
        .template_id("card-host");
    host.set_attr("data-stacking-context", "1");

    let mut portal = Element::new()
        .w("100%")
        .h("100%")
        .template_id("local-portal")
        .z_index(2);
    portal.set_attr("rui_portal", "1");
    portal.set_attr(ATTR_PORTAL_KEY, "local-portal");
    portal.set_attr(ATTR_PORTAL_MOUNT, "card-host");

    let mut root = Element::new().child(host.child(portal));
    assert!(hoist_portal_subtrees(&mut root) >= 1);
    fn strip_slots(el: &mut Element) {
        el.children
            .retain(|c| c.attrs.get(ATTR_PORTAL_SLOT).is_none());
        for c in &mut el.children {
            strip_slots(c);
        }
    }
    strip_slots(&mut root);
    let _ = hoist_portal_subtrees(&mut root);
    let host = find_by_tid(&root, "card-host").expect("host");
    assert_eq!(
        host.attrs.get("data-stacking-context").map(String::as_str),
        Some("1"),
        "author stacking-context must survive local overlay close"
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

    let mut root = Element::new().children([Element::new().template_id("main").child(portal)]);

    assert_eq!(hoist_portal_subtrees(&mut root), 1);
    assert_eq!(count_in_layer(&root), 1);
    assert!(find_by_tid(&root, "orphan-portal")
        .map(|n| { n.attrs.get(ATTR_HOISTED_PORTAL).map(|s| s.as_str()) == Some("1") })
        .unwrap_or(false));
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

    let mut root =
        Element::new().children([Element::new().template_id("main").children([host, portal])]);

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
        main.children
            .retain(|c| c.template_id.as_deref() != Some("local-portal"));
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
        find_by_tid(&root, "card-host").unwrap().children.is_empty(),
        "closing local portal must clear host mount"
    );
}

/// Dialog-shaped tree: underlay + stage portals as siblings; panel is not a portal.
#[test]
fn hoist_dialog_stage_keeps_inner_panel() {
    let mut underlay = Element::new().template_id("dlg-underlay");
    underlay.set_attr("rui_portal", "1");
    underlay.set_attr(ATTR_PORTAL_KEY, "underlay:1000");

    let panel = Element::new().w(320.0).h(200.0).template_id("dlg-panel");
    let mut stage = Element::new()
        .w("100%")
        .h("100%")
        .template_id("dlg-stage")
        .child(panel);
    stage.set_attr("rui_portal", "1");
    stage.set_attr(ATTR_PORTAL_KEY, "panel:1001");

    let mut root = Element::new()
        .w(800.0)
        .h(600.0)
        .children([Element::new().child(underlay).child(stage)]);

    assert_eq!(hoist_portal_subtrees(&mut root), 2);
    assert_eq!(count_in_layer(&root), 2);
    let stage = portal_layer(&root)
        .unwrap()
        .children
        .iter()
        .find(|c| c.attrs.get(ATTR_PORTAL_KEY).map(|s| s.as_str()) == Some("panel:1001"))
        .expect("stage in layer");
    assert_eq!(
        stage.children.len(),
        1,
        "panel must remain a child of the hoisted stage"
    );
    assert_eq!(stage.children[0].template_id.as_deref(), Some("dlg-panel"));
}

/// Nested `rui_portal` lift count must match (regression for debug_assert pending vs lifted).
#[test]
fn hoist_nested_portals_count_matches_lifted() {
    let mut inner = Element::new().template_id("nested-inner");
    inner.set_attr("rui_portal", "1");
    inner.set_attr(ATTR_PORTAL_KEY, "inner");
    let mut outer = Element::new().template_id("nested-outer").child(inner);
    outer.set_attr("rui_portal", "1");
    outer.set_attr(ATTR_PORTAL_KEY, "outer");

    let mut root = Element::new().child(outer);
    // Must not panic on lifted.len() == pending (both 2).
    assert_eq!(hoist_portal_subtrees(&mut root), 2);
    assert_eq!(count_in_layer(&root), 2);
}

/// Dialog stage already on the portal layer + Select menu opens inside it (retention path).
/// The menu must become a **sibling** on the layer (viewport-fixed), not stay absolute under
/// the stage — otherwise floating left/top are applied in the wrong containing block.
#[test]
fn hoist_select_portal_inside_already_hoisted_dialog() {
    let panel = Element::new().w(320.0).h(200.0).template_id("dlg-panel");
    let mut stage = Element::new()
        .w("100%")
        .h("100%")
        .template_id("dlg-stage")
        .child(panel);
    stage.set_attr("rui_portal", "1");
    stage.set_attr(ATTR_PORTAL_KEY, "panel:dlg");

    let mut root = Element::new()
        .w(800.0)
        .h(600.0)
        .children([Element::new().child(stage)]);

    assert!(hoist_portal_subtrees(&mut root) >= 1);
    assert_eq!(count_in_layer(&root), 1);

    // Simulate Select opening: fresh rui_portal inserted under the live hoisted panel.
    {
        let layer = root
            .children
            .iter_mut()
            .find(|c| c.template_id.as_deref() == Some(PORTAL_LAYER_ID))
            .expect("portal layer");
        let stage = layer
            .children
            .iter_mut()
            .find(|c| c.template_id.as_deref() == Some("dlg-stage"))
            .expect("hoisted stage");
        let panel = stage
            .children
            .iter_mut()
            .find(|c| c.template_id.as_deref() == Some("dlg-panel"))
            .expect("panel");
        let mut menu = Element::new().w(160.0).h(120.0).template_id("select-menu");
        menu.set_attr("rui_portal", "1");
        menu.set_attr(ATTR_PORTAL_KEY, "panel:select-menu");
        panel.children.push(menu);
    }

    assert!(
        hoist_portal_subtrees(&mut root) >= 1,
        "nested select portal must dirty structure"
    );
    assert_eq!(
        count_in_layer(&root),
        2,
        "select menu must be a portal-layer sibling of the dialog stage"
    );

    let layer = portal_layer(&root).expect("portal layer");
    let menu = layer
        .children
        .iter()
        .find(|c| c.template_id.as_deref() == Some("select-menu"))
        .expect("select menu on layer");
    assert_eq!(
        menu.attrs.get(ATTR_HOISTED_PORTAL).map(|s| s.as_str()),
        Some("1")
    );
    assert!(
        menu.attrs.get("rui_portal").is_none(),
        "rui_portal attr cleared after hoist"
    );
    assert!(
        menu.style.position_fixed,
        "body-mounted nested portal must be viewport-fixed"
    );

    let stage = layer
        .children
        .iter()
        .find(|c| c.template_id.as_deref() == Some("dlg-stage"))
        .expect("stage");
    let panel = &stage.children[0];
    assert!(
        panel
            .children
            .iter()
            .all(|c| c.template_id.as_deref() != Some("select-menu")),
        "menu must not remain under the dialog panel"
    );
    assert!(
        panel
            .children
            .iter()
            .any(|c| c.attrs.get(ATTR_PORTAL_SLOT).is_some()),
        "panel keeps a portal slot for the lifted select"
    );

    // Retention: slot inside the hoisted stage keeps the menu alive.
    assert_eq!(hoist_portal_subtrees(&mut root), 0);
    assert_eq!(count_in_layer(&root), 2);
}

/// Exit rebuild inserts a fresh `rui_portal` under the live parent overlay while the
/// previous menu is still keep-alive on the layer. Must replace that copy, not paint both.
#[test]
fn hoist_nested_portal_rebuild_does_not_duplicate() {
    let panel = Element::new().w(320.0).h(200.0).template_id("dlg-panel");
    let mut stage = Element::new()
        .w("100%")
        .h("100%")
        .template_id("dlg-stage")
        .child(panel);
    stage.set_attr("rui_portal", "1");
    stage.set_attr(ATTR_PORTAL_KEY, "panel:dlg");

    let mut root = Element::new()
        .w(800.0)
        .h(600.0)
        .children([Element::new().child(stage)]);

    assert!(hoist_portal_subtrees(&mut root) >= 1);

    {
        let layer = root
            .children
            .iter_mut()
            .find(|c| c.template_id.as_deref() == Some(PORTAL_LAYER_ID))
            .expect("portal layer");
        let stage = layer
            .children
            .iter_mut()
            .find(|c| c.template_id.as_deref() == Some("dlg-stage"))
            .expect("hoisted stage");
        let panel = stage
            .children
            .iter_mut()
            .find(|c| c.template_id.as_deref() == Some("dlg-panel"))
            .expect("panel");
        let mut menu = Element::new().w(160.0).h(120.0).template_id("select-menu");
        menu.set_attr("rui_portal", "1");
        menu.set_attr(ATTR_PORTAL_KEY, "panel:select-menu");
        panel.children.push(menu);
    }
    assert!(hoist_portal_subtrees(&mut root) >= 1);
    assert_eq!(count_in_layer(&root), 2);

    {
        let layer = root
            .children
            .iter_mut()
            .find(|c| c.template_id.as_deref() == Some(PORTAL_LAYER_ID))
            .expect("portal layer");
        let stage = layer
            .children
            .iter_mut()
            .find(|c| c.template_id.as_deref() == Some("dlg-stage"))
            .expect("hoisted stage");
        let panel = stage
            .children
            .iter_mut()
            .find(|c| c.template_id.as_deref() == Some("dlg-panel"))
            .expect("panel");
        let mut menu = Element::new().w(160.0).h(48.0).template_id("select-menu");
        menu.set_attr("rui_portal", "1");
        menu.set_attr(ATTR_PORTAL_KEY, "panel:select-menu");
        menu.style.max_size.height = taffy::style::Dimension::length(48.0);
        panel.children.push(menu);
    }

    assert!(hoist_portal_subtrees(&mut root) >= 1);
    assert_eq!(
        count_in_layer(&root),
        2,
        "exit rebuild must replace the keep-alive menu, not stack a second copy"
    );
    let layer = portal_layer(&root).expect("portal layer");
    let menus: Vec<_> = layer
        .children
        .iter()
        .filter(|c| c.template_id.as_deref() == Some("select-menu"))
        .collect();
    assert_eq!(menus.len(), 1, "one format menu on the portal layer");
    assert!(
        crate::layout::dim_length_px(menus[0].style.max_size.height)
            .is_some_and(|h| (h - 48.0).abs() < 0.01),
        "replacement must be the rebuilt exit panel"
    );
}

/// Dialog open rewrites the portal layer. That must not remasure every page text leaf
/// (whole-program first open hitch: full `build_and_layout` after hoist).
#[test]
fn portal_hoist_does_not_remasure_unchanged_page_text() {
    let mut labels = Vec::new();
    for i in 0..24 {
        labels.push(
            Element::new()
                .text(format!("gallery page label {i:02} 中文"))
                .text_size(14.0)
                .w(400.0)
                .template_id(format!("page-label-{i}")),
        );
    }
    let page = Element::new()
        .w(800.0)
        .flex_col()
        .template_id("page")
        .children(labels);

    let island = Element::new().w(0.0).h(0.0).template_id("dialog-island");

    let mut root = Element::new().w(800.0).h(600.0).child(page).child(island);
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);
    let cold = tree.text_measure_calls.get();
    assert!(
        cold >= 24,
        "cold layout must measure page labels, got {cold}"
    );

    let _ = tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);
    let same = tree.text_measure_calls.get();
    assert!(
        same < cold / 4,
        "identical rebuild must reuse Taffy cache, not remasure the gallery; cold={cold} same={same}"
    );

    let mut dlg = Element::new()
        .w(240.0)
        .h(120.0)
        .template_id("dlg-panel")
        .child(Element::new().text("Dialog title").text_size(18.0));
    dlg.set_attr("rui_portal", "1");
    dlg.set_attr(ATTR_PORTAL_KEY, "dlg-panel");
    let island = root
        .children
        .iter_mut()
        .find(|c| c.template_id.as_deref() == Some("dialog-island"))
        .expect("island");
    island.children.push(dlg);

    assert!(hoist_portal_subtrees(&mut root) >= 1);
    let _ = tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);
    let after_hoist = tree.text_measure_calls.get();
    assert!(
        after_hoist < cold / 2,
        "hoisting a dialog must measure the overlay, not every page label; cold={cold} after={after_hoist}"
    );
}

/// Hoist of a new portal **does** full-rebuild Taffy (layer rewrite). Page labels
/// must still get boxes — the live bug was overlay_split marking too early so
/// bubble captions vanished while chrome remained.
#[test]
fn portal_hoist_rebuilds_but_keeps_page_text_boxes() {
    let mut labels = Vec::new();
    for i in 0..16 {
        labels.push(
            Element::new()
                .text(format!("page label {i:02}"))
                .text_size(14.0)
                .w(400.0)
                .template_id(format!("page-label-{i}")),
        );
    }
    let page = Element::new()
        .w(800.0)
        .flex_col()
        .template_id("page")
        .children(labels);
    let island = Element::new().w(0.0).h(0.0).template_id("dialog-island");
    let mut root = Element::new().w(800.0).h(600.0).child(page).child(island);

    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);
    let rebuilds_after_cold = tree.full_rebuilds;

    let mut dlg = Element::new()
        .w(240.0)
        .h(120.0)
        .template_id("dlg-panel")
        .child(Element::new().text("Dialog title").text_size(18.0));
    dlg.set_attr("rui_portal", "1");
    dlg.set_attr(ATTR_PORTAL_KEY, "dlg-panel");
    let island = root
        .children
        .iter_mut()
        .find(|c| c.template_id.as_deref() == Some("dialog-island"))
        .expect("island");
    island.children.push(dlg);

    let out =
        tree.build_and_layout_fast_ex(&mut root, 800.0, 600.0, [0.0, 0.0], false, false, None);
    assert!(
        tree.full_rebuilds > rebuilds_after_cold,
        "portal membership must rebuild Taffy so overlay_split stays a suffix"
    );
    assert_eq!(
        count_in_layer(&root),
        1,
        "panel must land on the portal layer"
    );
    let panel = find_by_tid(&root, "dlg-panel").expect("hoisted panel");
    assert!(
        panel.layout_bounds[2] > 0.0 && panel.layout_bounds[3] > 0.0,
        "overlay must receive a real layout box, got {:?}",
        panel.layout_bounds
    );
    let label = find_by_tid(&root, "page-label-0").expect("page label");
    assert!(
        label.layout_bounds[2] > 0.0,
        "page text box must survive hoist, got {:?}",
        label.layout_bounds
    );
    assert!(
        !out.text_nodes.is_empty(),
        "page + overlay glyphs must be collected"
    );
}

/// Live bubble +N: 0×0 `floating_portal_anchor` + `rui_portal` panel, then fast layout.
/// Collect must emit overlay instances (otherwise PartialLoad has nothing to draw).
#[test]
fn anchored_overlay_hoist_emits_overlay_instances() {
    let page = Element::new()
        .w(800.0)
        .h(400.0)
        .flex_col()
        .child(Element::new().text("page label").text_size(14.0).w(400.0));
    let island = Element::new().w(0.0).h(0.0).template_id("rx-island");
    let mut root = Element::new().w(800.0).h(600.0).child(page).child(island);

    let mut tree = LayoutTree::new();
    let cold = tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);
    let page_n = cold.instances.len();

    let mut panel = Element::new()
        .w(160.0)
        .h(40.0)
        .bg([1.0, 1.0, 1.0, 1.0])
        .template_id("rx-panel")
        .child(Element::new().text("👍").text_size(16.0));
    panel.set_attr("rui_portal", "1");
    panel.set_attr(ATTR_PORTAL_KEY, "rx-panel");
    let anchor = Element::new().w(0.0).h(0.0).child(panel);
    let island = root
        .children
        .iter_mut()
        .find(|c| c.template_id.as_deref() == Some("rx-island"))
        .expect("island");
    island.children.push(anchor);

    let out =
        tree.build_and_layout_fast_ex(&mut root, 800.0, 600.0, [0.0, 0.0], false, false, None);
    assert_eq!(count_in_layer(&root), 1);
    assert!(
        out.overlay_split.instance_start < out.instances.len(),
        "reaction expand must collect overlay quads (start={} len={})",
        out.overlay_split.instance_start,
        out.instances.len()
    );
    assert!(
        out.instances.len() > page_n,
        "overlay paint must add instances beyond the page"
    );
    let panel = find_by_tid(&root, "rx-panel").expect("panel");
    assert!(
        panel.layout_bounds[2] > 0.0 && panel.layout_bounds[3] > 0.0,
        "anchored panel box {:?}",
        panel.layout_bounds
    );
}
