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

#[test]
fn prepared_portal_removal_keeps_remaining_hud_layout_during_scroll() {
    let mut hover = Element::new().w(222.0).h(46.0).template_id("closing-hover");
    hover.set_attr("jadix_portal", "1");
    hover.set_attr(ATTR_PORTAL_KEY, "closing-hover");
    let hud = crate::utils::viewport_portal_stage(
        "persistent-hud",
        Element::new().absolute().right(12.0).bottom(12.0)
            .w(320.0).h(150.0).template_id("persistent-hud-panel"),
        1_000_000,
    );
    let mut root = Element::new().w(1000.0).h(700.0)
        .child(Element::new().w(0.0).h(0.0).child(hover))
        .child(Element::new().w(0.0).h(0.0).child(hud));
    let mut tree = LayoutTree::new();
    tree.build_and_layout(&mut root, 1000.0, 700.0, [-1.0; 2]);
    let rebuilds = tree.full_rebuilds;
    let before = find_by_tid(&root, "persistent-hud-panel").unwrap().layout_bounds;
    root.children[0].children.clear();
    let changed = hoist_portal_subtrees(&mut root);
    assert!(changed > 0);
    tree.note_portals_prepared(true);
    tree.build_and_layout_fast_ex(&mut root, 1000.0, 700.0, [-1.0; 2], true, true, None);
    assert_eq!(find_by_tid(&root, "persistent-hud-panel").unwrap().layout_bounds, before);
    assert_eq!(tree.full_rebuilds, rebuilds);
    assert_eq!(count_in_layer(&root), 1);
}

#[test]
#[cfg(feature = "product-tests")]
fn persistent_hud_survives_floating_portal_and_reactive_refresh() {
    use crate::core::reactivity::{create_effect, create_root, use_signal};
    use std::{cell::RefCell, rc::Rc};
    crate::init();
    crate::app::reactive::reset_registered_anchors();
    crate::app::portal_overlay::clear_overlay_stack();
    let tree = Rc::new(RefCell::new(Element::new()));
    crate::app::runtime::install_test_reconcile_root(tree.clone());
    let (owner, (open, caption)) = create_root(|| {
        let open = use_signal("hud-tip-open", || false);
        let caption = use_signal("hud-caption", || "FPS 60".to_string());
        let live = tree.clone();
        create_effect(move || {
            let tip = crate::components::tooltip::tooltip("hud-tip", "Tip")
                .open_signal(open)
                .trigger(|_| Element::new().w(100.0).h(30.0).text("Trigger"));
            let hud = crate::utils::viewport_portal_stage(
                "gallery-performance",
                Element::new()
                    .absolute()
                    .right(12.0)
                    .bottom(12.0)
                    .w(280.0)
                    .h(118.0)
                    .template_id("perf-hud")
                    .child(Element::new().template_id("perf-toggle").text("性能 / FPS"))
                    .child(Element::new().text(caption.get())),
                1_000_000,
            );
            let mut next = Element::new()
                .w(960.0)
                .h(640.0)
                .child(tip)
                .child(Element::new().w(0.0).h(0.0).child(hud));
            let mut old = live.borrow_mut();
            crate::app::test_support::reconcile_elements(&mut next, &mut old);
            *old = next;
        });
        (open, caption)
    });
    let mut layout = LayoutTree::new();
    for i in 0..5 {
        if i == 1 {
            open.set(true);
        }
        if i == 2 {
            caption.set("FPS 48".into());
        }
        if i == 3 {
            open.set(false);
        }
        crate::app::reactive::drain_deferred_view_remounts();
        let mut root = tree.borrow_mut();
        hoist_portal_subtrees(&mut root);
        let (width, height) = if i < 2 {
            (960.0, 640.0)
        } else {
            (1706.6666, 978.6667)
        };
        layout.build_and_layout_fast_ex(&mut root, width, height, [-1.0; 2], false, false, None);
        let hud = find_by_tid(&root, "perf-hud").expect("HUD disappeared");
        assert!(
            (width - hud.layout_bounds[0] - hud.layout_bounds[2] - 12.0).abs() < 1.0
                && (height - hud.layout_bounds[1] - hud.layout_bounds[3] - 12.0).abs() < 1.0,
            "HUD must stay at viewport bottom-right step {i}: {:?} viewport={width}x{height}",
            hud.layout_bounds
        );
        assert!(
            find_by_tid(&root, "perf-toggle").is_some(),
            "toggle disappeared at step {i}"
        );
    }
    crate::app::runtime::clear_test_reconcile_root();
    owner.dispose();
    crate::app::reactive::reset_registered_anchors();
    crate::app::portal_overlay::clear_overlay_stack();
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
fn persistent_hud_keeps_viewport_edges_after_floating_follow_and_maximize() {
    let hud = crate::utils::viewport_portal_stage(
        "perf",
        Element::new()
            .absolute()
            .right(12.0)
            .bottom(12.0)
            .w(280.0)
            .h(118.0)
            .template_id("perf-hud"),
        1_000_000,
    );
    let mut root = Element::new().w("100%").h("100%").child(hud);
    let mut tree = LayoutTree::new();
    tree.build_and_layout(&mut root, 960.0, 640.0, [-1.0; 2]);
    tree.relayout_overlay_follow_and_collect(&mut root, 960.0, 640.0, [-1.0; 2], true, false);
    for (width, height) in [(1706.6666, 978.6667), (800.0, 600.0), (1706.6666, 978.6667)] {
        tree.build_and_layout_fast_ex(&mut root, width, height, [-1.0; 2], true, false, None);
        let hud = find_by_tid(&root, "perf-hud").unwrap();
        assert!(
            (width - hud.layout_bounds[0] - hud.layout_bounds[2] - 12.0).abs() < 1.0
                && (height - hud.layout_bounds[1] - hud.layout_bounds[3] - 12.0).abs() < 1.0,
            "HUD moved after maximize/restore: {:?} viewport={width}x{height}",
            hud.layout_bounds
        );
    }
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
                portal.set_attr("jadix_portal", "1");
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
                portal.set_attr("jadix_portal", "1");
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
        portal.set_attr("jadix_portal", "1");
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

    // No slot / jadix_portal source → retention prune drops the orphan and reports dirty.
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
    portal.set_attr("jadix_portal", "1");
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
            .all(|c| c.attrs.get("jadix_portal").is_none()),
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
    portal.set_attr("jadix_portal", "1");
    portal.set_attr(ATTR_PORTAL_KEY, "local-portal");
    portal.set_attr(ATTR_PORTAL_MOUNT, "card-host");

    let mut root = Element::new()
        .w(800.0)
        .h(600.0)
        .child(Element::new().template_id("main").children([host, portal]));

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
    portal.set_attr("jadix_portal", "1");
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
    portal.set_attr("jadix_portal", "1");
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
    portal.set_attr("jadix_portal", "1");
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
    underlay.set_attr("jadix_portal", "1");
    underlay.set_attr(ATTR_PORTAL_KEY, "underlay:1000");

    let panel = Element::new().w(320.0).h(200.0).template_id("dlg-panel");
    let mut stage = Element::new()
        .w("100%")
        .h("100%")
        .template_id("dlg-stage")
        .child(panel);
    stage.set_attr("jadix_portal", "1");
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

/// Nested `jadix_portal` lift count must match (regression for debug_assert pending vs lifted).
#[test]
fn hoist_nested_portals_count_matches_lifted() {
    let mut inner = Element::new().template_id("nested-inner");
    inner.set_attr("jadix_portal", "1");
    inner.set_attr(ATTR_PORTAL_KEY, "inner");
    let mut outer = Element::new().template_id("nested-outer").child(inner);
    outer.set_attr("jadix_portal", "1");
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
    stage.set_attr("jadix_portal", "1");
    stage.set_attr(ATTR_PORTAL_KEY, "panel:dlg");

    let mut root = Element::new()
        .w(800.0)
        .h(600.0)
        .children([Element::new().child(stage)]);

    assert!(hoist_portal_subtrees(&mut root) >= 1);
    assert_eq!(count_in_layer(&root), 1);

    // Simulate Select opening: fresh jadix_portal inserted under the live hoisted panel.
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
        menu.set_attr("jadix_portal", "1");
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
        menu.attrs.get("jadix_portal").is_none(),
        "jadix_portal attr cleared after hoist"
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

/// Exit rebuild inserts a fresh `jadix_portal` under the live parent overlay while the
/// previous menu is still keep-alive on the layer. Must replace that copy, not paint both.
#[test]
fn hoist_nested_portal_rebuild_does_not_duplicate() {
    let panel = Element::new().w(320.0).h(200.0).template_id("dlg-panel");
    let mut stage = Element::new()
        .w("100%")
        .h("100%")
        .template_id("dlg-stage")
        .child(panel);
    stage.set_attr("jadix_portal", "1");
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
        menu.set_attr("jadix_portal", "1");
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
        menu.set_attr("jadix_portal", "1");
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
    dlg.set_attr("jadix_portal", "1");
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

/// Hoist of a new portal must attach overlay Taffy nodes without recycling the
/// page tree. Full `build_and_layout` after +N is the gallery scroll hitch.
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
    tree.text_measure_calls.set(0);

    let mut dlg = Element::new()
        .w(240.0)
        .h(120.0)
        .template_id("dlg-panel")
        .child(Element::new().text("Dialog title").text_size(18.0));
    dlg.set_attr("jadix_portal", "1");
    dlg.set_attr(ATTR_PORTAL_KEY, "dlg-panel");
    let island = root
        .children
        .iter_mut()
        .find(|c| c.template_id.as_deref() == Some("dialog-island"))
        .expect("island");
    island.children.push(dlg);

    let out = tree.build_and_layout_fast_ex(&mut root, 800.0, 600.0, [0.0, 0.0], true, true, None);
    assert_eq!(
        tree.full_rebuilds, rebuilds_after_cold,
        "portal membership must not rebuild the gallery Taffy tree"
    );
    assert!(
        tree.text_measure_calls.get() < 8,
        "opening +N during scroll must not remasure page labels, got {}",
        tree.text_measure_calls.get()
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

/// Live bubble +N: 0×0 `floating_portal_anchor` + `jadix_portal` panel, then fast layout.
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
    panel.set_attr("jadix_portal", "1");
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

/// Gallery startup: cold `build_and_layout` then a same-frame `fast_ex` after a
/// parent wrapper is replaced while the child keeps `layout_node`.
///
/// `attach_unbuilt` used `new_with_children`, which does **not** splice the child
/// out of the dropped parent's Taffy list. `sync_taffy_children_from_elements`
/// then `remove_taffy_subtree`'d that orphan and recursively deleted the live
/// child — `compute_layout` panics `invalid SlotMap key used`.
#[test]
fn replaced_parent_keeping_child_layout_node_must_not_panic() {
    let keep = Element::new()
        .w(40.0)
        .h(40.0)
        .bg([1.0, 0.0, 0.0, 1.0])
        .template_id("keep");
    let page = Element::new()
        .w(200.0)
        .h(200.0)
        .template_id("page")
        .child(keep);
    let mut root = Element::new().w(800.0).h(600.0).child(page);

    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);
    let keep_id = find_by_tid(&root, "keep")
        .and_then(|e| e.layout_node)
        .expect("keep must have a Taffy id after cold build");

    let mut old_page = root.children.remove(0);
    let keep = old_page.children.remove(0);
    assert_eq!(keep.layout_node, Some(keep_id));
    let wrapper = Element::new()
        .w(200.0)
        .h(200.0)
        .template_id("page")
        .child(keep);
    assert!(
        wrapper.layout_node.is_none(),
        "replacement wrapper is unbuilt"
    );
    root.children.insert(0, wrapper);

    let out =
        tree.build_and_layout_fast_ex(&mut root, 800.0, 600.0, [0.0, 0.0], false, false, None);
    let keep = find_by_tid(&root, "keep").expect("keep");
    assert!(
        keep.layout_node.is_some(),
        "child SlotMap id must survive the wrapper replace"
    );
    assert!(
        keep.layout_bounds[2] > 0.0 && keep.layout_bounds[3] > 0.0,
        "keep box {:?}",
        keep.layout_bounds
    );
    assert!(
        !out.instances.is_empty(),
        "replaced wrapper must still collect"
    );
}

/// Cold tree already has a live hoisted overlay; a second `jadix_portal` joins on
/// the fast path (gallery toasts + first +N). Must not recycle page nodes or
/// panic on the layer's existing SlotMap children.
#[test]
fn second_portal_joins_existing_layer_without_slotmap_panic() {
    let page = Element::new()
        .w(800.0)
        .h(400.0)
        .flex_col()
        .child(Element::new().text("page label").text_size(14.0).w(400.0));
    let mut toast = Element::new()
        .w(160.0)
        .h(40.0)
        .template_id("toast-card")
        .child(Element::new().text("saved").text_size(14.0));
    toast.set_attr("jadix_portal", "1");
    toast.set_attr(ATTR_PORTAL_KEY, "toast-card");
    let island = Element::new()
        .w(0.0)
        .h(0.0)
        .template_id("island")
        .child(toast);
    let mut root = Element::new().w(800.0).h(600.0).child(page).child(island);

    let mut tree = LayoutTree::new();
    tree.note_portals_prepared(false);
    let _ = tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);
    assert_eq!(count_in_layer(&root), 1);
    let rebuilds_after_cold = tree.full_rebuilds;

    // A stable AppState frame skips its duplicate hoist, but that hint must
    // expire before a subsequent standalone layout introduces a new portal.
    tree.note_portals_prepared(false);
    tree.build_and_layout_fast_ex(&mut root, 800.0, 600.0, [0.0, 0.0], true, true, None);

    let mut plus_n = Element::new()
        .w(72.0)
        .h(28.0)
        .template_id("plus-n")
        .child(Element::new().text("+3").text_size(12.0));
    plus_n.set_attr("jadix_portal", "1");
    plus_n.set_attr(ATTR_PORTAL_KEY, "plus-n");
    let island = root
        .children
        .iter_mut()
        .find(|c| c.template_id.as_deref() == Some("island"))
        .expect("island");
    island.children.push(plus_n);

    let out = tree.build_and_layout_fast_ex(&mut root, 800.0, 600.0, [0.0, 0.0], true, true, None);
    assert_eq!(
        tree.full_rebuilds, rebuilds_after_cold,
        "joining a second overlay must not rebuild the page Taffy tree"
    );
    assert_eq!(count_in_layer(&root), 2, "toast + +N on the layer");
    for tid in ["toast-card", "plus-n"] {
        let el = find_by_tid(&root, tid).expect(tid);
        assert!(
            el.layout_bounds[2] > 0.0 && el.layout_bounds[3] > 0.0,
            "{tid} box {:?}",
            el.layout_bounds
        );
    }
    assert!(
        !out.text_nodes.is_empty(),
        "page + both overlays must collect glyphs"
    );
}

/// Gallery cold start: `new_bootstrap` already `build_and_layout`s a shell, so
/// `root_node` is `Some`. `rebuild_ui` then swaps in the real app tree with
/// `layout_node = None`. Fast path must rebuild — not `sync` the unbound tree
/// (that drops every bootstrap SlotMap id, then `compute_layout` panics
/// `invalid SlotMap key used`).
#[test]
fn fast_path_after_unbound_tree_replace_must_not_panic() {
    let mut bootstrap = Element::new()
        .w(100.0)
        .h(100.0)
        .child(Element::new().w(10.0).h(10.0).bg([0.0, 0.0, 0.0, 1.0]));
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut bootstrap, 100.0, 100.0, [0.0, 0.0]);
    assert!(tree.root_node.is_some(), "bootstrap occupies root_node");

    let page = Element::new()
        .w(800.0)
        .h(400.0)
        .flex_col()
        .child(Element::new().text("gallery page").text_size(14.0).w(400.0));
    let mut toast = Element::new()
        .w(160.0)
        .h(40.0)
        .template_id("toast-viewport")
        .child(Element::new().text("hi").text_size(14.0));
    toast.set_attr("jadix_portal", "1");
    toast.set_attr(ATTR_PORTAL_KEY, "toast-viewport");
    let mut gallery = Element::new().w(800.0).h(600.0).child(page).child(toast);
    assert!(
        gallery.layout_node.is_none(),
        "rebuild_ui initial mount is an unbound tree"
    );

    let out =
        tree.build_and_layout_fast_ex(&mut gallery, 800.0, 600.0, [0.0, 0.0], false, false, None);
    assert!(
        gallery.layout_node.is_some(),
        "unbound replace must rebuild"
    );
    assert_eq!(gallery.layout_node, tree.root_node);
    assert_eq!(count_in_layer(&gallery), 1);
    let toast = find_by_tid(&gallery, "toast-viewport").expect("toast");
    assert!(
        toast.layout_bounds[2] > 0.0 && toast.layout_bounds[3] > 0.0,
        "toast box {:?}",
        toast.layout_bounds
    );
    assert!(
        !out.text_nodes.is_empty(),
        "rebuilt gallery must collect page glyphs"
    );
}
