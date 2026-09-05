//! Empty fullscreen portal layer must not steal wheel/scroll hit chain from shell scroll areas.
//! Body-mounted portals must escape ancestor overflow clipping (Solid `<Portal>` → document.body).

use crate::app::scroll_support::collect_scroll_rest_chain_at;
use crate::app::{hoist_portal_subtrees_refreshing_scroll_flags, recompute_scroll_viewport_flags};
use crate::core::scroll_viewport::SCROLL_AREA_FOCUS_PREFIX;
use crate::core::tree::Element;
use crate::layout::layout_tree::LayoutTree;
use crate::layout::layout_tree::{
    ATTR_HOISTED_MOUNT, ATTR_HOISTED_PORTAL, ATTR_PORTAL_KEY, ATTR_PORTAL_MOUNT, PORTAL_LAYER_ID,
    escapes_ancestor_overflow, hoist_portal_subtrees,
};
use crate::renderer::pipeline::InstanceInput;
use crate::utils::popup::panel_block_clicks;

#[test]
fn empty_portal_layer_does_not_block_scroll_chain_underneath() {
    // Shell: sidebar scroll (left) + main scroll (right), then a fullscreen portal layer on top.
    let mut side = Element::new()
        .w(200.0)
        .h(600.0)
        .overflow_y("auto")
        .focus_id(format!("{SCROLL_AREA_FOCUS_PREFIX}side-nav"))
        .template_id("side")
        .child(Element::new().w(200.0).h(2000.0));
    side.layout_bounds = [0.0, 0.0, 200.0, 600.0];

    let mut main = Element::new()
        .flex_grow(1.0)
        .h(600.0)
        .overflow_y("auto")
        .focus_id(format!("{SCROLL_AREA_FOCUS_PREFIX}root-scroll"))
        .template_id("main")
        .child(Element::new().w(800.0).h(3000.0));
    main.layout_bounds = [200.0, 0.0, 800.0, 600.0];

    let mut layer = Element::new()
        .absolute()
        .inset(0.0)
        .w(1000.0)
        .h(600.0)
        .template_id(PORTAL_LAYER_ID);
    // Closed portal host: full-screen absolute wrapper with no interactive children.
    let mut ghost = Element::new()
        .absolute()
        .inset(0.0)
        .w(1000.0)
        .h(600.0)
        .pointer_events_box_none()
        .template_id("closed-portal");
    ghost.set_attr(ATTR_HOISTED_PORTAL, "1");
    ghost.layout_bounds = [0.0, 0.0, 1000.0, 600.0];
    layer.children.push(ghost);
    layer.layout_bounds = [0.0, 0.0, 1000.0, 600.0];

    let mut root = Element::new()
        .flex_row()
        .w(1000.0)
        .h(600.0)
        .children([side, main, layer]);
    root.layout_bounds = [0.0, 0.0, 1000.0, 600.0];

    let mut chain = Vec::new();
    assert!(
        collect_scroll_rest_chain_at(&root, 80.0, 300.0, [0.0, 0.0], &mut chain),
        "pointer over sidebar must hit a scroll branch"
    );
    assert_eq!(
        chain.first().map(String::as_str),
        Some("side-nav"),
        "empty portal layer must not swallow sidebar wheel target; got {chain:?}"
    );

    chain.clear();
    assert!(collect_scroll_rest_chain_at(
        &root,
        500.0,
        300.0,
        [0.0, 0.0],
        &mut chain
    ));
    assert_eq!(
        chain.first().map(String::as_str),
        Some("root-scroll"),
        "empty portal layer must not swallow main wheel target; got {chain:?}"
    );
}

#[test]
fn hoist_then_layout_keeps_scroll_chain_under_empty_portals() {
    let mut root = Element::new().flex_row().w(1000.0).h(600.0).children([
        Element::new()
            .w(200.0)
            .h("100%")
            .overflow_y("auto")
            .focus_id(format!("{SCROLL_AREA_FOCUS_PREFIX}side-nav"))
            .template_id("side")
            .child(Element::new().w(200.0).h(2000.0)),
        Element::new()
            .flex_grow(1.0)
            .min_w(0.0)
            .h("100%")
            .overflow_y("auto")
            .focus_id(format!("{SCROLL_AREA_FOCUS_PREFIX}root-scroll"))
            .template_id("main")
            .child({
                // Empty fullscreen portal shell: must opt into pass-through
                // (same as [`crate::primitives::portal::Portal`]). Hoist no longer
                // auto-coerces Auto → box-none — popups/menus keep Auto by default.
                let mut portal = Element::new()
                    .absolute()
                    .inset(0.0)
                    .w("100%")
                    .h("100%")
                    .pointer_events_box_none()
                    .template_id("page-portal");
                portal.set_attr("rui_portal", "1");
                portal
            }),
    ]);

    assert_eq!(hoist_portal_subtrees(&mut root), 1);
    let mut layout = LayoutTree::new();
    let _ = layout.build_and_layout(&mut root, 1000.0, 600.0, [-1.0, -1.0]);

    let mut chain = Vec::new();
    assert!(collect_scroll_rest_chain_at(
        &root,
        80.0,
        300.0,
        [0.0, 0.0],
        &mut chain
    ));
    assert_eq!(
        chain.first().map(String::as_str),
        Some("side-nav"),
        "after hoist, empty portal must not block sidebar; got {chain:?}"
    );
}

#[test]
fn body_portal_escapes_scroll_overflow_clip() {
    // Solid: `<Portal>` into document.body is outside cropping layouts.
    // Panel is authored inside a 200×200 overflow:scroll host but must paint past its edge.
    let mut scroller = Element::new()
        .w(200.0)
        .h(200.0)
        .overflow_x("scroll")
        .overflow_y("scroll")
        .template_id("crop-scroll");

    let mut panel = Element::new()
        .bg([1.0, 1.0, 1.0, 1.0])
        .w(120.0)
        .h(100.0)
        .absolute()
        .left(120.0)
        .top(50.0);
    panel.set_attr(ATTR_PORTAL_KEY, "panel:escape-clip");
    panel = panel_block_clicks(panel, 1001);

    let host = Element::new().relative().w(200.0).h(200.0).child(panel);
    scroller = scroller.child(host);

    let mut root = Element::new().w(800.0).h(600.0).child(scroller);
    // build_and_layout hoists internally; explicit hoist asserts the mount target.
    assert!(hoist_portal_subtrees(&mut root) >= 1);
    let layer = root
        .children
        .iter()
        .find(|c| c.template_id.as_deref() == Some(PORTAL_LAYER_ID))
        .expect("body portal layer");
    assert!(
        layer.style.position_fixed,
        "body portal layer must be viewport-fixed"
    );
    assert_eq!(
        layer.children.len(),
        1,
        "panel must live on the body portal layer, not under the scroller"
    );
    assert!(
        layer.children[0].style.position_fixed,
        "body-mounted panel must be position_fixed"
    );

    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);
    assert!(
        result.overlay_split.instance_start < result.instances.len(),
        "overlay paint split must include portal instances"
    );
    let overlay = &result.instances[result.overlay_split.instance_start..];
    let panel_inst = overlay
        .iter()
        .find(|i| i.color[3] > 0.5 && (i.rect_size[0] - 120.0).abs() < 2.0)
        .expect("panel background instance in overlay suffix");

    assert!(
        panel_inst.clip_size == InstanceInput::NO_CLIP_SIZE || panel_inst.clip_size[0] < 0.0,
        "body portal must not inherit scroll clip; clip_size={:?}",
        panel_inst.clip_size
    );
    let right = panel_inst.rect_pos[0] + panel_inst.rect_size[0];
    assert!(
        right > 200.5,
        "panel must paint past the scroll viewport’s right edge (Solid escape cropping); right={right}"
    );
}

fn dialog_like_portal_with_body_scroll() -> Element {
    let mut body = Element::new()
        .w("100%")
        .h(120.0)
        .overflow_y("auto")
        .focus_id(format!("{SCROLL_AREA_FOCUS_PREFIX}gal-dialog__body"))
        .child(Element::new().w("100%").h(400.0));
    body.template_id = Some("dialog-body".into());

    let mut panel = Element::new().w(320.0).h(200.0).child(body);
    panel.set_attr("rui_portal", "1");
    panel.template_id = Some("dialog-panel".into());

    Element::new().w(800.0).h(600.0).child(panel)
}

fn portal_layer(root: &Element) -> &Element {
    root.children
        .iter()
        .find(|c| c.template_id.as_deref() == Some(PORTAL_LAYER_ID))
        .expect("portal layer")
}

#[test]
fn hoist_without_flag_refresh_leaves_portal_scroll_prepass_blind() {
    let mut root = dialog_like_portal_with_body_scroll();
    recompute_scroll_viewport_flags(&mut root);
    assert!(
        hoist_portal_subtrees(&mut root) > 0,
        "dialog panel must lift"
    );
    assert!(
        !portal_layer(&root).contains_scroll_viewport,
        "stale flags after hoist: prepass would skip DialogBody (hittable, alpha-0 chrome)"
    );
}

#[test]
fn hoist_refreshing_flags_marks_portal_layer_scroll_viewport() {
    let mut root = dialog_like_portal_with_body_scroll();
    recompute_scroll_viewport_flags(&mut root);
    assert!(
        hoist_portal_subtrees_refreshing_scroll_flags(&mut root) > 0,
        "dialog panel must lift"
    );
    let layer = portal_layer(&root);
    assert!(
        layer.contains_scroll_viewport,
        "portal layer must be visible to scrollbar prepass after hoist+refresh"
    );
    assert!(
        layer.scroll_viewport_subtree_count >= 1,
        "hoisted DialogBody viewport must count on the portal layer, got {}",
        layer.scroll_viewport_subtree_count
    );
}

#[test]
fn local_mount_portal_clips_to_host_and_follows_scroll() {
    // Host-mounted portal must not viewport-fix: overflow:hidden on the host clips
    // oversized children, and ancestor scroll offsets the paint (unlike body portals).
    let mut blob = Element::new()
        .bg([1.0, 0.0, 0.0, 1.0])
        .w(180.0)
        .h(160.0)
        .template_id("overflow-blob");
    blob.style.position = taffy::style::Position::Absolute;
    blob = blob.left(0.0).top(0.0);

    let mut portal = Element::new()
        .w("100%")
        .h("100%")
        .template_id("local-scrim");
    portal.set_attr("rui_portal", "1");
    portal.set_attr(ATTR_PORTAL_KEY, "local-scrim");
    portal.set_attr(ATTR_PORTAL_MOUNT, "clip-host");
    portal.children.push(blob);

    let host = Element::new()
        .relative()
        .w(120.0)
        .h(100.0)
        .overflow_x("hidden")
        .overflow_y("hidden")
        .template_id("clip-host")
        .child(portal);

    let mut root = Element::new()
        .flex_col()
        .w(400.0)
        .h(240.0)
        .overflow_y("scroll")
        .child(Element::new().w(400.0).h(80.0))
        .child(host)
        .child(Element::new().w(400.0).h(400.0));
    assert_eq!(hoist_portal_subtrees(&mut root), 1);
    let host = find_tid(&root, "clip-host").expect("host");
    let local = host
        .children
        .iter()
        .find(|c| c.template_id.as_deref() == Some("local-scrim"))
        .expect("local portal on host");
    assert!(
        !local.style.position_fixed,
        "local mount must not stay viewport-fixed"
    );
    assert_eq!(
        local.attrs.get(ATTR_HOISTED_MOUNT).map(String::as_str),
        Some("clip-host")
    );
    assert!(
        !escapes_ancestor_overflow(local),
        "local mount must inherit host clip + scroll"
    );

    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 400.0, 240.0, [0.0, 0.0]);
    let blob_inst = result
        .instances
        .iter()
        .find(|i| i.color[0] > 0.9 && i.color[1] < 0.1 && i.color[3] > 0.5)
        .expect("overflow blob instance");
    assert!(
        blob_inst.clip_size != InstanceInput::NO_CLIP_SIZE && blob_inst.clip_size[0] > 0.0,
        "local mount must inherit host overflow clip; clip_size={:?}",
        blob_inst.clip_size
    );
    let clip_r = blob_inst.clip_pos[0] + blob_inst.clip_size[0];
    let host_el = find_tid(&root, "clip-host").expect("host after layout");
    let hb = host_el.layout_bounds;
    assert!(
        clip_r <= hb[0] + hb[2] + 1.5,
        "clip must not extend past the host; clip_r={clip_r} host_r={}",
        hb[0] + hb[2]
    );
    let y0 = blob_inst.rect_pos[1];

    root.scroll_offset = [0.0, 40.0];
    let mut tree = LayoutTree::new();
    let scrolled = tree.build_and_layout(&mut root, 400.0, 240.0, [0.0, 0.0]);
    let blob_scrolled = scrolled
        .instances
        .iter()
        .find(|i| i.color[0] > 0.9 && i.color[1] < 0.1 && i.color[3] > 0.5)
        .expect("overflow blob after scroll");
    assert!(
        blob_scrolled.rect_pos[1] < y0 - 20.0,
        "local mount must move with host scroll; y0={y0} y1={}",
        blob_scrolled.rect_pos[1]
    );
}

fn find_tid<'a>(el: &'a Element, id: &str) -> Option<&'a Element> {
    if el.template_id.as_deref() == Some(id) {
        return Some(el);
    }
    el.children.iter().find_map(|c| find_tid(c, id))
}
