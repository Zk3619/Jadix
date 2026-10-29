use super::*;
use crate::utils::popup::panel_block_clicks;

fn layer_children(root: &Element) -> &[Element] {
    root.children
        .iter()
        .find(|c| c.template_id.as_deref() == Some(PORTAL_LAYER_ID))
        .map(|c| c.children.as_slice())
        .unwrap_or(&[])
}

#[test]
fn hoisted_portal_new_children_are_laid_out_during_scroll_only_collect() {
    let panel = Element::new()
        .template_id("retained-perf-panel")
        .absolute()
        .right(12.0)
        .bottom(12.0)
        .flex_col()
        .gap(4.0)
        .child(Element::new().w(80.0).h(24.0));
    let mut root = Element::new()
        .w(300.0)
        .h(200.0)
        .child(crate::utils::viewport_portal_stage(
            "retained-perf",
            panel,
            1000,
        ));
    let mut tree = LayoutTree::new();
    tree.build_and_layout(&mut root, 300.0, 200.0, [-1.0; 2]);
    let panel = crate::core::tree::find_in_tree_mut(&mut root, |element| {
        element.template_id.as_deref() == Some("retained-perf-panel")
    })
    .unwrap();
    panel.children.push(Element::new().w(140.0).h(40.0));
    tree.build_and_layout_fast(&mut root, 300.0, 200.0, [-1.0; 2], true, true);
    let panel = crate::core::tree::find_in_tree(&root, |element| {
        element.template_id.as_deref() == Some("retained-perf-panel")
    })
    .unwrap();
    let bounds = panel.viewport_layout_bounds();
    assert_eq!(bounds, [148.0, 120.0, 140.0, 68.0]);
    assert_eq!(panel.children[1].layout_bounds[2..], [140.0, 40.0]);
    assert_eq!(
        tree.last_compute_root,
        LayoutTree::portal_layer_layout_node(&root)
    );
}

#[test]
fn portal_content_survives_second_hoist_pass() {
    // Solid keep-alive: after lift, source keeps a slot; next hoist must retain the panel.
    let mut panel = Element::new().bg([1.0, 1.0, 1.0, 1.0]).w(160.0).h(80.0);
    panel.set_attr(ATTR_PORTAL_KEY, "panel:test");
    panel = panel_block_clicks(panel, 1001);

    let mut root = Element::new().w(800.0).h(600.0).child(panel);
    let moved1 = hoist_portal_subtrees(&mut root);
    assert_eq!(moved1, 1, "first pass lifts the panel");
    assert_eq!(layer_children(&root).len(), 1);
    let bg = layer_children(&root)[0].style.bg_color;
    assert!(bg[3] > 0.0, "panel chrome must survive hoist");

    assert!(
        root.children
            .iter()
            .any(|c| c.attrs.get(ATTR_PORTAL_SLOT).is_some()),
        "slot marker must remain in source tree"
    );

    let moved2 = hoist_portal_subtrees(&mut root);
    assert_eq!(moved2, 0, "retention path must not rewrite the layer");
    assert_eq!(
        layer_children(&root).len(),
        1,
        "hoisted panel must persist while slot is wanted"
    );
    let bg2 = layer_children(&root)[0].style.bg_color;
    assert_eq!(bg2, bg, "background style must not be stripped on retain");
    // Identity retained: same node object still has opaque chrome.
    assert!(
        layer_children(&root)[0].style.bg_color[3] > 0.99,
        "panel background must stay opaque"
    );
}

#[test]
fn source_slot_after_hoist_is_a_portal_island() {
    let mut panel = Element::new().w(80.0).h(32.0);
    panel.set_attr("jadix_portal", "1");
    panel.set_attr(ATTR_PORTAL_KEY, "rx-expand");
    let island = Element::new().w(0.0).h(0.0).child(panel);
    assert!(subtree_declares_portal(&island));

    let mut root = Element::new().w(800.0).h(600.0).child(island);
    assert!(hoist_portal_subtrees(&mut root) >= 1);
    let source = root
        .children
        .iter()
        .find(|c| c.template_id.as_deref() != Some(PORTAL_LAYER_ID))
        .expect("source island");
    assert!(
        subtree_declares_portal(source),
        "keep-alive slot must still count as a portal island so close skips FullClear"
    );
    assert!(!subtree_declares_portal(&Element::new().text("page")));
}

#[test]
fn hoisted_popover_damage_is_local_modal_underlay_covers_viewport() {
    let page = Element::new()
        .w(800.0)
        .h(600.0)
        .text("gallery page")
        .template_id("page");

    let mut popover = Element::new()
        .w(72.0)
        .h(36.0)
        .bg([1.0; 4])
        .template_id("rx-panel");
    popover.set_attr("jadix_portal", "1");
    popover.set_attr(ATTR_PORTAL_KEY, "rx");
    popover.layout_bounds = [120.0, 80.0, 72.0, 36.0];

    let mut root = Element::new().w(800.0).h(600.0).child(page).child(popover);
    assert!(hoist_portal_subtrees(&mut root) >= 1);
    // Hoist does not run Taffy; stamp the laid-out box onto the lifted node.
    if let Some(layer) = root
        .children
        .iter_mut()
        .find(|c| c.template_id.as_deref() == Some(PORTAL_LAYER_ID))
    {
        for c in &mut layer.children {
            if c.template_id.as_deref() == Some("rx-panel") {
                c.layout_bounds = [120.0, 80.0, 72.0, 36.0];
            }
        }
    }
    let d = hoisted_portal_damage(&root).expect("popover box");
    assert!((d[2] - 72.0).abs() < 0.5 && (d[3] - 36.0).abs() < 0.5);
    assert!(!crate::app::damage::damage_covers_full(
        [0.0, 0.0, 800.0, 600.0],
        d
    ));

    let mut underlay = Element::new()
        .w(800.0)
        .h(600.0)
        .bg([0.0, 0.0, 0.0, 0.5])
        .template_id("dlg-underlay");
    underlay.set_attr("jadix_portal", "1");
    underlay.set_attr(ATTR_PORTAL_KEY, "dlg");
    let mut modal_root = Element::new().w(800.0).h(600.0).child(underlay);
    assert!(hoist_portal_subtrees(&mut modal_root) >= 1);
    if let Some(layer) = modal_root
        .children
        .iter_mut()
        .find(|c| c.template_id.as_deref() == Some(PORTAL_LAYER_ID))
    {
        for c in &mut layer.children {
            c.layout_bounds = [0.0, 0.0, 800.0, 600.0];
        }
    }
    let full = hoisted_portal_damage(&modal_root).expect("underlay");
    assert!(crate::app::damage::damage_covers_full(
        [0.0, 0.0, 800.0, 600.0],
        full
    ));
}

#[test]
fn page_mounted_portal_does_not_reserve_global_overlay_occupancy() {
    // Gallery's local scrim lives above the drag examples. After scrolling,
    // its old document coordinates overlap T/B despite painting offscreen.
    let bounds = [350.0, 460.0, 264.0, 168.0];
    let mut panel = Element::new().bg([1.0; 4]);
    panel.layout_bounds = bounds;
    let mut portal = Element::new().child(panel);
    portal.layout_bounds = [306.0, 432.0, 352.0, 224.0];
    portal.set_attr(ATTR_HOISTED_PORTAL, "1");
    portal.set_attr(ATTR_HOISTED_MOUNT, "gal-local-scrim-host");
    portal.set_attr(ATTR_PORTAL_KEY, "gal-local-scrim");
    let mut root = Element::new().child(portal);
    let stamps = hoisted_portal_stamps(&root);
    assert!(stamps.is_empty());
    assert!(hoisted_portal_damage(&root).is_none());
    assert_eq!(
        crate::app::damage::page_scissors_excluding_portal_boxes(bounds, &stamps),
        vec![bounds]
    );

    // A local mount inside a global dialog remains part of its overlay pass.
    root.template_id = Some(PORTAL_LAYER_ID.to_string());
    assert_eq!(hoisted_portal_stamps(&root)[0].1, bounds);
    root.template_id = None;
    root.children[0].attrs.remove(ATTR_HOISTED_MOUNT);
    assert_eq!(hoisted_portal_stamps(&root)[0].1, bounds);
}

#[test]
fn empty_hoisted_portal_does_not_occlude_page_damage() {
    // Portal's inset-0 host survives while a nested Show is empty.
    let mut portal = Element::new().child(Element::new());
    portal.set_attr(ATTR_HOISTED_PORTAL, "1");
    portal.set_attr(ATTR_PORTAL_KEY, "empty-root-portal");
    portal.layout_bounds = [0.0, 0.0, 800.0, 600.0];
    portal.children[0].layout_bounds = portal.layout_bounds;
    let root = Element::new().child(portal);
    let stamps = hoisted_portal_stamps(&root);
    assert!(
        stamps.is_empty(),
        "empty hosts must not become painted stamps"
    );
    assert!(hoisted_portal_damage(&root).is_none());
    let dirty = [120.0, 100.0, 176.0, 92.0];
    assert_eq!(
        crate::app::damage::page_scissors_excluding_portal_boxes(dirty, &stamps),
        vec![dirty],
        "stationary page drag must retain its repaint scissor"
    );
}

#[test]
fn hoisted_portal_occupancy_tracks_painted_children() {
    let mut portal = Element::new().child(Element::new());
    portal.set_attr(ATTR_HOISTED_PORTAL, "1");
    portal.set_attr(ATTR_PORTAL_KEY, "conditional-portal");
    portal.layout_bounds = [0.0, 0.0, 800.0, 600.0];
    portal.children[0].layout_bounds = portal.layout_bounds;
    assert!(hoisted_portal_paint_box(&portal).is_none());

    let mut panel = Element::new().bg([1.0; 4]);
    panel.layout_bounds = [120.0, 80.0, 72.0, 36.0];
    portal.children[0].children.push(panel);
    assert_eq!(
        hoisted_portal_paint_box(&portal),
        Some([120.0, 80.0, 72.0, 36.0])
    );
    portal.children[0].children.clear();
    assert!(
        hoisted_portal_paint_box(&portal).is_none(),
        "closed content must release occupancy"
    );
}

#[test]
fn hoisted_portal_text_and_graphics_leaves_keep_paint_boxes() {
    let bounds = [120.0, 80.0, 72.0, 36.0];
    let mut text = Element::new().text("Caption");
    text.layout_bounds = bounds;
    assert_eq!(hoisted_portal_paint_box(&text), Some(bounds));
    let mut image = Element::new().image_texture("panel-image");
    image.layout_bounds = bounds;
    assert_eq!(hoisted_portal_paint_box(&image), Some(bounds));
    for attr in [
        crate::primitives::path::ATTR_PATH_D,
        crate::primitives::canvas::ATTR_CANVAS_PAINT,
        crate::primitives::shader::ATTR_SHADER,
        crate::primitives::scene3d::ATTR_SCENE3D,
        "jadix_svg_scene",
    ] {
        let mut leaf = Element::new();
        leaf.set_attr(attr, "1");
        leaf.layout_bounds = bounds;
        assert_eq!(hoisted_portal_paint_box(&leaf), Some(bounds), "{attr}");
    }
}

#[test]
fn expand_measure_portal_is_not_a_paint_box() {
    let mut panel = Element::new();
    panel.set_attr(ATTR_HOISTED_PORTAL, "1");
    panel.set_attr("jadix_floating_expand_measure", "1");
    panel.layout_bounds = [90.0, 140.0, 40.0, 200.0];
    let root = Element::new().child(panel);
    assert!(hoisted_expand_measuring(&root));
    assert!(
        hoisted_portal_boxes(&root).is_empty(),
        "unconstrained measure must not enter last_overlay_boxes"
    );
}

#[test]
fn expand_clip_portal_box_follows_max_height() {
    let mut panel = Element::new().bg([1.0; 4]);
    panel.set_attr(ATTR_HOISTED_PORTAL, "1");
    panel.layout_bounds = [90.0, 140.0, 40.0, 200.0];
    panel.style.max_size.height = taffy::style::Dimension::length(40.0);
    let root = Element::new().child(panel);
    let boxes = hoisted_portal_boxes(&root);
    assert_eq!(boxes.len(), 1);
    assert!(
        (boxes[0][3] - 40.0).abs() < 0.01,
        "paint box must be the overflow clip, got {}",
        boxes[0][3]
    );
    assert!(!hoisted_expand_measuring(&root));
}

#[test]
fn hoisted_portal_without_handlers_keeps_auto_and_occludes() {
    use crate::app::pointer_events::hit_path;
    use crate::core::tree::PointerEvents;
    use crate::utils::popup::portal_panel;

    // Raw portal surface (no swallow_clicks / no floating attrs): must still
    // claim its box after hoist — never auto-coerce to box-none.
    let mut panel = Element::new()
        .w(200.0)
        .h(200.0)
        .bg([0.0, 1.0, 0.0, 1.0])
        .absolute()
        .left(100.0)
        .top(100.0);
    panel.set_attr(ATTR_PORTAL_KEY, "panel:plain");
    panel = portal_panel(panel, 1001);

    let under = Element::new()
        .w(400.0)
        .h(400.0)
        .bg([1.0, 0.0, 0.0, 1.0])
        .focus_id("under-btn")
        .on_click(|| {});

    let mut root = Element::new().w(400.0).h(400.0).child(under).child(panel);
    assert!(hoist_portal_subtrees(&mut root) >= 1);
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 400.0, 400.0, [0.0, 0.0]);

    let hoisted = &layer_children(&root)[0];
    assert_eq!(
        hoisted.pointer_events_mode(),
        PointerEvents::Auto,
        "plain portal panel must stay auto after hoist"
    );
    let path = hit_path(&root, 200.0, 200.0);
    assert!(
        path.iter()
            .any(|e| e.portal_key.as_deref() == Some("panel:plain")),
        "hit must land on the hoisted panel; path={:?}",
        path.iter()
            .map(|e| (e.focus_id.clone(), e.portal_key.clone()))
            .collect::<Vec<_>>()
    );
    assert!(
        path.iter()
            .all(|e| e.focus_id.as_deref() != Some("under-btn")),
        "page control under the panel must not receive the hit"
    );
}

#[test]
fn viewport_stage_box_none_still_passes_empty_chrome() {
    use crate::app::pointer_events::hit_path;
    use crate::core::reactivity::Signal;
    use crate::core::tree::PointerEvents;
    use crate::utils::popup::{
        mount_modal_layer, mount_viewport_portal_stage, swallow_clicks, underlay_close_on_click,
    };

    // Special-purpose pass-through: empty stage chrome → underlay, not page.
    let panel = swallow_clicks(
        Element::new()
            .w(120.0)
            .h(80.0)
            .bg([0.0, 1.0, 0.0, 1.0])
            .focus_id("dlg-panel"),
    );
    let stage = Element::new()
        .w("100%")
        .h("100%")
        .items_center()
        .justify_center()
        .child(panel);
    let underlay = underlay_close_on_click(Signal::new(true), 1000);
    let mut root = Element::new().w(400.0).h(400.0).child(mount_modal_layer(
        underlay,
        mount_viewport_portal_stage(stage, 1001),
    ));
    assert!(hoist_portal_subtrees(&mut root) >= 1);
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 400.0, 400.0, [0.0, 0.0]);

    let stage_el = layer_children(&root)
        .iter()
        .find(|c| c.attrs.get(ATTR_PORTAL_KEY).map(|s| s.as_str()) == Some("panel:1001"))
        .expect("stage on portal layer");
    assert_eq!(
        stage_el.pointer_events_mode(),
        PointerEvents::BoxNone,
        "viewport stage must keep explicit box-none"
    );

    // Corner of the fullscreen stage (outside the centered panel).
    let path = hit_path(&root, 8.0, 8.0);
    assert!(
        path.iter().any(|e| {
            e.portal_key
                .as_deref()
                .is_some_and(|k| k.starts_with("underlay:"))
        }),
        "empty stage chrome must hit underlay; path={:?}",
        path.iter()
            .map(|e| e.portal_key.clone())
            .collect::<Vec<_>>()
    );
}

#[test]
fn portal_overlay_paint_split_after_page_content() {
    let mut page = Element::new().w(200.0).h(40.0).bg([0.9, 0.9, 0.9, 1.0]);
    page = page.child(Element::new().text("page"));

    let mut panel = Element::new()
        .bg([1.0, 1.0, 1.0, 1.0])
        .w(120.0)
        .h(60.0)
        .absolute()
        .left(20.0)
        .top(20.0);
    panel.set_attr(ATTR_PORTAL_KEY, "panel:split");
    panel = panel_block_clicks(panel, 1001);
    panel = panel.child(Element::new().text("overlay"));

    let mut root = Element::new().w(800.0).h(600.0).child(page).child(panel);
    assert!(hoist_portal_subtrees(&mut root) >= 1);

    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);
    assert!(
        result.overlay_split.marked,
        "portal layer must mark the overlay paint split"
    );
    assert!(
        result.overlay_split.instance_start < result.instances.len(),
        "overlay must contribute rect instances"
    );
    assert!(
        result.overlay_split.text_start < result.text_nodes.len(),
        "overlay must contribute text nodes after the split"
    );
    // Page glyphs are collected before the portal suffix.
    assert!(result.text_nodes[..result.overlay_split.text_start]
        .iter()
        .any(|t| t.text == "page"));
    assert!(result.text_nodes[result.overlay_split.text_start..]
        .iter()
        .any(|t| t.text == "overlay"));
    assert!(
        !result.overlay_split.bands.is_empty(),
        "each portal child should open a paint band"
    );
    let overlay_i0 = result.overlay_split.instance_start as u32;
    for op in &result.paint_ops[..result.overlay_split.paint_op_start] {
        if let PaintOp::Rects { start, count } = *op {
            assert!(
                    start + count <= overlay_i0,
                    "page Rects must not swallow overlay panel fills (would paint page glyphs on the popover); op={start}+{count} overlay_i0={overlay_i0}"
                );
        }
    }
}

#[test]
fn portal_panel_frost_stays_local_while_modal_underlay_blurs_the_viewport() {
    for underlay in [false, true] {
        let panel = if underlay {
            crate::primitives::overlay::overlay(1000)
                .backdrop_blur(20.0)
                .into()
        } else {
            let mut panel = Element::new()
                .absolute()
                .left(40.0)
                .top(40.0)
                .w(120.0)
                .h(60.0)
                .bg([0.2, 0.2, 0.2, 0.2])
                .backdrop_blur(20.0);
            panel.set_attr(ATTR_PORTAL_KEY, "panel:local-frost");
            panel_block_clicks(panel, 1000)
        };
        let mut root = Element::new().w(400.0).h(300.0).child(panel);
        hoist_portal_subtrees(&mut root);
        let mut tree = LayoutTree::new();
        for _ in 0..2 {
            let result = tree.build_and_layout(&mut root, 400.0, 300.0, [-1.0; 2]);
            assert_eq!(
                result
                    .overlay_split
                    .bands
                    .iter()
                    .any(|band| band.backdrop_blur > 0.5),
                underlay
            );
            assert_eq!(
                result
                    .instances
                    .iter()
                    .any(|instance| instance.backdrop_blur > 0.5),
                !underlay
            );
        }
    }
}

#[test]
fn stacked_portal_panels_get_separate_paint_bands() {
    // Two overlapping panels: band1's instances must start after band0's texts would
    // otherwise paint over band1's fill if all overlay rects were drawn before all text.
    let page = Element::new().w(100.0).h(40.0).bg([0.95, 0.95, 0.95, 1.0]);

    let mut lower = Element::new()
        .bg([1.0, 1.0, 1.0, 1.0])
        .w(160.0)
        .h(120.0)
        .absolute()
        .left(10.0)
        .top(50.0)
        .z_index(1000);
    lower.set_attr(ATTR_PORTAL_KEY, "panel:lower");
    lower = panel_block_clicks(lower, 1000);
    lower = lower.child(Element::new().text("lower-glyph"));

    let mut upper = Element::new()
        .bg([1.0, 1.0, 1.0, 1.0])
        .w(160.0)
        .h(120.0)
        .absolute()
        .left(80.0)
        .top(80.0)
        .z_index(1010);
    upper.set_attr(ATTR_PORTAL_KEY, "panel:upper");
    upper = panel_block_clicks(upper, 1010);
    upper = upper.child(Element::new().text("upper-glyph"));

    let mut root = Element::new()
        .w(800.0)
        .h(600.0)
        .child(page)
        .child(lower)
        .child(upper);
    assert!(hoist_portal_subtrees(&mut root) >= 1);

    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);
    assert!(
        result.overlay_split.bands.len() >= 2,
        "expected ≥2 portal bands, got {}",
        result.overlay_split.bands.len()
    );
    let ranges = result.overlay_split.band_ranges(
        result.instances.len(),
        result.textured_draws.len(),
        result.text_nodes.len(),
        result.input_chrome_instances.len(),
        result.paint_ops.len(),
    );
    assert!(ranges.len() >= 2);
    let a = ranges[ranges.len() - 2];
    let b = ranges[ranges.len() - 1];
    assert!(a.text.0 < a.text.1, "lower band must include text");
    assert!(
        b.instance.0 >= a.instance.1 || b.instance.0 > a.instance.0,
        "upper band instances follow lower band"
    );
    assert!(
        b.text.0 >= a.text.1,
        "upper band text follows lower band text"
    );
    assert!(result.text_nodes[a.text.0..a.text.1]
        .iter()
        .any(|t| t.text == "lower-glyph"));
    assert!(result.text_nodes[b.text.0..b.text.1]
        .iter()
        .any(|t| t.text == "upper-glyph"));
}

#[test]
fn page_stack_bands_interleave_rects_and_text() {
    let lower = Element::new()
        .bg([1.0, 0.0, 0.0, 1.0])
        .w(120.0)
        .h(80.0)
        .absolute()
        .left(10.0)
        .top(10.0)
        .z_index(1)
        .child(Element::new().text("stack-low"));
    let upper = Element::new()
        .bg([0.0, 0.0, 1.0, 1.0])
        .w(120.0)
        .h(80.0)
        .absolute()
        .left(40.0)
        .top(30.0)
        .z_index(2)
        .child(Element::new().text("stack-high"));
    let mut stage = Element::new()
        .w(400.0)
        .h(200.0)
        .relative()
        .child(lower)
        .child(upper);
    stage
        .attrs
        .insert("data-stacking-context".to_string(), "1".to_string());
    let mut root = Element::new().w(400.0).h(200.0).child(stage);
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 400.0, 200.0, [0.0, 0.0]);
    assert!(
        result.overlay_split.page_stack_bands.len() >= 2,
        "expected page_stack_bands, got {}",
        result.overlay_split.page_stack_bands.len()
    );
    let ranges = result.overlay_split.page_stack_band_ranges();
    assert!(ranges.len() >= 2);
    // Highest z is last band; its text must not be in an earlier band.
    let last = ranges.last().unwrap();
    assert!(
        result.text_nodes[last.text.0..last.text.1]
            .iter()
            .any(|t| t.text == "stack-high"),
        "top card text belongs to last page stack band"
    );
    let earlier_has_high = ranges[..ranges.len() - 1].iter().any(|r| {
        result.text_nodes[r.text.0..r.text.1]
            .iter()
            .any(|t| t.text == "stack-high")
    });
    assert!(!earlier_has_high);
}

#[test]
fn z_index_children_auto_page_stack_bands_without_attr() {
    // Toast-style pile: z-ordered absolute cards, no data-stacking-context.
    // Without auto bands, all glyphs paint after all rects → text bleed.
    let lower = Element::new()
        .bg([1.0, 1.0, 1.0, 1.0])
        .w(120.0)
        .h(60.0)
        .absolute()
        .left(0.0)
        .top(0.0)
        .z_index(1)
        .child(Element::new().text("toast-low"));
    let upper = Element::new()
        .bg([1.0, 1.0, 1.0, 1.0])
        .w(120.0)
        .h(60.0)
        .absolute()
        .left(0.0)
        .top(20.0)
        .z_index(2)
        .child(Element::new().text("toast-high"));
    let pile = Element::new()
        .w(200.0)
        .h(120.0)
        .relative()
        .child(lower)
        .child(upper);
    let mut root = Element::new().w(400.0).h(200.0).child(pile);
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 400.0, 200.0, [0.0, 0.0]);
    assert!(
        result.overlay_split.page_stack_bands.len() >= 2,
        "z-index children must auto-enable page_stack_bands, got {}",
        result.overlay_split.page_stack_bands.len()
    );
    let ranges = result.overlay_split.page_stack_band_ranges();
    let last = ranges.last().unwrap();
    assert!(
        result.text_nodes[last.text.0..last.text.1]
            .iter()
            .any(|t| t.text == "toast-high"),
        "higher z text in last band"
    );
}

#[test]
fn page_stack_cache_identity_follows_host_when_earlier_band_is_culled() {
    let host = |label: &str| {
        Element::new()
            .w(200.0)
            .h(120.0)
            .flex_shrink(0.0)
            .z_index(1)
            .text(label)
    };
    let mut root = Element::new().w(200.0).h(150.0).overflow_y("scroll").child(
        Element::new()
            .w(200.0)
            .flex_col()
            .children([host("first"), host("second"), host("third")]),
    );
    let mut tree = LayoutTree::new();
    let first = tree.build_and_layout(&mut root, 200.0, 150.0, [-1.0; 2]);
    let key_for = |result: &RenderResult, label: &str| {
        result
            .overlay_split
            .page_stack_band_ranges()
            .iter()
            .find(|b| {
                result.text_nodes[b.text.0..b.text.1]
                    .iter()
                    .any(|t| t.text == label)
            })
            .map(|b| b.stamp_key)
            .unwrap()
    };
    let second_key = key_for(&first, "second");
    assert_ne!(second_key, 0);
    assert_ne!(key_for(&first, "first"), second_key);
    root.scroll_offset = [0.0, 120.0];
    let moved = tree.build_and_layout_fast(&mut root, 200.0, 150.0, [-1.0; 2], true, false);
    assert_eq!(second_key, key_for(&moved, "second"));
}

#[test]
fn nested_wrapper_z_index_with_stacking_context_keeps_glyphs_in_band() {
    // Drag free-move: relative arena (stacking context) → vstack shell (z) →
    // absolute pill + text. Without bands, both glyphs paint after both shells.
    let pill = |label: &str, x: f32| {
        Element::new()
            .bg([0.2, 0.4, 0.9, 1.0])
            .w(120.0)
            .h(40.0)
            .absolute()
            .left(x)
            .top(20.0)
            .child(Element::new().text(label))
    };
    let shell = |z: i32, pill: Element| Element::new().z_index(z).child(pill);
    let mut arena = Element::new()
        .w(400.0)
        .h(120.0)
        .relative()
        .child(shell(1, pill("drag-low", 10.0)))
        .child(shell(2, pill("drag-high", 50.0)));
    arena.set_attr("data-stacking-context", "1");
    let mut root = Element::new().w(400.0).h(200.0).child(arena);
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 400.0, 200.0, [0.0, 0.0]);
    assert!(
        result.overlay_split.page_stack_bands.len() >= 2,
        "expected page_stack_bands for overlapping drag shells, got {}",
        result.overlay_split.page_stack_bands.len()
    );
    let ranges = result.overlay_split.page_stack_band_ranges();
    let last = ranges.last().expect("band");
    assert!(
        result.text_nodes[last.text.0..last.text.1]
            .iter()
            .any(|t| t.text == "drag-high"),
        "front shell glyphs must ride with its rect band"
    );
    let earlier_has_high = ranges[..ranges.len() - 1].iter().any(|r| {
        result.text_nodes[r.text.0..r.text.1]
            .iter()
            .any(|t| t.text == "drag-high")
    });
    assert!(
        !earlier_has_high,
        "front glyphs must not sit in an earlier band (text bleed)"
    );
}

#[test]
fn gallery_wrapped_drag_shells_inside_outer_page_stack_occlude_glyphs() {
    // Gallery Arbitrary-move: ScrollArea scrollbar (z=1000) already owns
    // page_stack_depth. DragProvider is a stacking-context whose only child
    // is a relative arena; item z-index lives on nested shells. Without a
    // stacking-context on each shell, both pills share one band and the
    // lower glyph paints after the upper fill.
    let pill = |label: &str, x: f32, y: f32, color: [f32; 4]| {
        Element::new()
            .bg(color)
            .w(176.0)
            .h(92.0)
            .absolute()
            .left(x)
            .top(y)
            .child(Element::new().text(label))
    };
    let shell = |z: i32, pill: Element| {
        let mut s = Element::new().z_index(z).child(pill);
        s.set_attr("data-stacking-context", "1");
        s
    };
    let arena = Element::new()
        .w(400.0)
        .h(200.0)
        .relative()
        .child(shell(1, pill("drag-low", 16.0, 20.0, [0.1, 0.2, 0.9, 1.0])))
        .child(shell(
            2,
            pill("drag-high", 88.0, 52.0, [0.9, 0.1, 0.1, 1.0]),
        ));
    let mut provider = Element::new().child(arena);
    provider.set_attr("data-stacking-context", "1");
    let bar = Element::new()
        .w(8.0)
        .h(80.0)
        .absolute()
        .right(0.0)
        .z_index(1000)
        .bg([0.0, 0.0, 0.0, 0.4]);
    let mut page = Element::new()
        .w(400.0)
        .h(240.0)
        .relative()
        .child(provider)
        .child(bar);
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut page, 400.0, 240.0, [0.0, 0.0]);
    let ranges = result.overlay_split.page_stack_band_ranges();
    let low_band = ranges.iter().position(|r| {
        result.text_nodes[r.text.0..r.text.1]
            .iter()
            .any(|t| t.text == "drag-low")
    });
    let high_band = ranges.iter().rposition(|r| {
        result.text_nodes[r.text.0..r.text.1]
            .iter()
            .any(|t| t.text == "drag-high")
    });
    assert!(
            high_band.is_some_and(|h| low_band.is_some_and(|l| h > l)),
            "upper pill glyphs must paint in a later band than lower glyphs (high={high_band:?} low={low_band:?} bands={})",
            ranges.len()
        );
    let same_band = low_band.zip(high_band).is_some_and(|(l, h)| l == h);
    assert!(
        !same_band,
        "sharing a band paints both fills then both glyphs (text bleed)"
    );
}

#[test]
fn nested_overlay_z_index_keeps_glyphs_before_both_scrollbars() {
    // Gallery: outer ScrollArea overlay bar (z=1000) already owns page_stack_bands.
    // The nested bar also needs a split: its fill must cover inner glyphs.
    // Verify ownership on full and cached collects, rather than freezing a
    // band count that incorrectly batches those glyphs above the bar.
    let nested_bar = Element::new()
        .w(8.0)
        .h(40.0)
        .absolute()
        .right(0.0)
        .z_index(1000)
        .bg([0.0, 0.0, 0.0, 0.4]);
    let nested = Element::new()
        .w(160.0)
        .h(80.0)
        .relative()
        .child(Element::new().text("inner-glyph"))
        .child(nested_bar);
    let outer_bar = Element::new()
        .w(8.0)
        .h(80.0)
        .absolute()
        .right(0.0)
        .z_index(1000)
        .bg([0.0, 0.0, 0.0, 0.4]);
    let mut outer = Element::new()
        .w(200.0)
        .h(120.0)
        .relative()
        .child(nested)
        .child(outer_bar);
    let mut tree = LayoutTree::new();
    for frame in 0..2 {
        let result = if frame == 0 {
            tree.build_and_layout(&mut outer, 200.0, 120.0, [0.0, 0.0])
        } else {
            tree.build_and_layout_fast(&mut outer, 200.0, 120.0, [0.0, 0.0], false, false)
        };
        let bands = result.overlay_split.page_stack_band_ranges();
        let text_band = bands
            .iter()
            .position(|b| {
                result.text_nodes[b.text.0..b.text.1]
                    .iter()
                    .any(|t| t.text == "inner-glyph")
            })
            .expect("inner text");
        for height in [40.0, 80.0] {
            let bar_band = bands
                .iter()
                .position(|b| {
                    result.instances[b.instance.0..b.instance.1]
                        .iter()
                        .any(|i| i.rect_size == [8.0, height] && i.color[3] > 0.1)
                })
                .unwrap_or_else(|| {
                    panic!(
                        "frame={frame} bar height={height}, bands={:?}, end={}, instances={:?}",
                        bands
                            .iter()
                            .map(|b| (b.instance, b.text))
                            .collect::<Vec<_>>(),
                        result.overlay_split.instance_start,
                        result
                            .instances
                            .iter()
                            .map(|i| (i.rect_pos, i.rect_size, i.color))
                            .collect::<Vec<_>>()
                    )
                });
            assert!(
                text_band < bar_band,
                "frame {frame}: text must precede scrollbar {height}"
            );
        }
    }
}

#[test]
fn nested_stacking_context_does_not_band_each_in_flow_child() {
    // DragProvider is a stacking-context whose body is often the sortable
    // vstack itself. Under Gallery's ScrollArea (already page_stack_depth),
    // banding every in-flow row reshapes labels on scroll.
    const N: usize = 12;
    let mut list = Element::new().w(200.0).relative();
    list.set_attr("data-stacking-context", "1");
    for i in 0..N {
        list = list.child(Element::new().text(format!("row-{i}")).w(160.0).h(20.0));
    }
    let bar = Element::new()
        .w(8.0)
        .h(80.0)
        .absolute()
        .right(0.0)
        .z_index(1000)
        .bg([0.0, 0.0, 0.0, 0.4]);
    let mut page = Element::new()
        .w(240.0)
        .h(280.0)
        .relative()
        .child(list)
        .child(bar);
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut page, 240.0, 280.0, [0.0, 0.0]);
    let bands = result.overlay_split.page_stack_bands.len();
    assert!(
        bands <= 4,
        "nested stacking-context must not open one band per in-flow row; bands={bands} rows={N}"
    );
    assert!(
        (0..N).all(|i| result
            .text_nodes
            .iter()
            .any(|t| t.text == format!("row-{i}"))),
        "in-flow row glyphs must still collect"
    );
}

#[test]
fn nested_stacking_context_can_split_inside_outer_page_bands() {
    // Jump button inside a gallery ScrollArea: explicit stacking-context must
    // subdivide the current band so later-z chrome paints after message glyphs.
    let jump = Element::new()
        .w(80.0)
        .h(28.0)
        .absolute()
        .bottom(8.0)
        .z_index(20)
        .bg([1.0, 1.0, 1.0, 1.0])
        .child(Element::new().text("Latest"));
    let mut nested = Element::new()
        .w(160.0)
        .h(80.0)
        .relative()
        .child(Element::new().text("inner-glyph"))
        .child(jump);
    nested
        .attrs
        .insert("data-stacking-context".to_string(), "1".to_string());
    let outer_bar = Element::new()
        .w(8.0)
        .h(80.0)
        .absolute()
        .right(0.0)
        .z_index(1000)
        .bg([0.0, 0.0, 0.0, 0.4]);
    let mut outer = Element::new()
        .w(200.0)
        .h(120.0)
        .relative()
        .child(nested)
        .child(outer_bar);
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut outer, 200.0, 120.0, [0.0, 0.0]);
    assert!(
        result.overlay_split.page_stack_bands.len() > 2,
        "nested stacking-context must add splits, got {}",
        result.overlay_split.page_stack_bands.len()
    );
    let ranges = result.overlay_split.page_stack_band_ranges();
    let latest_band = ranges.iter().rposition(|r| {
        result.text_nodes[r.text.0..r.text.1]
            .iter()
            .any(|t| t.text == "Latest")
    });
    let glyph_band = ranges.iter().position(|r| {
        result.text_nodes[r.text.0..r.text.1]
            .iter()
            .any(|t| t.text == "inner-glyph")
    });
    assert!(
            latest_band.is_some_and(|j| glyph_band.is_some_and(|g| j > g)),
            "Latest must paint in a later band than inner-glyph (latest={latest_band:?} glyph={glyph_band:?})"
        );
}

#[test]
fn portal_nested_z_stack_extends_overlay_bands_not_page_stack() {
    // Toast lives under `__jadix_portal_layer`. Nested z-cards must become *portal*
    // bands; page_stack_bands end at the portal split and would drop overlay glyphs.
    let lower = Element::new()
        .bg([1.0, 1.0, 1.0, 1.0])
        .w(120.0)
        .h(60.0)
        .absolute()
        .left(0.0)
        .top(0.0)
        .z_index(1)
        .child(Element::new().text("portal-low"));
    let upper = Element::new()
        .bg([1.0, 1.0, 1.0, 1.0])
        .w(120.0)
        .h(60.0)
        .absolute()
        .left(0.0)
        .top(24.0)
        .z_index(2)
        .child(Element::new().text("portal-high"));
    let pile = Element::new()
        .w(200.0)
        .h(120.0)
        .relative()
        .child(lower)
        .child(upper);
    let host = Element::new().w(400.0).h(300.0).child(pile);
    let layer = Element::new()
        .w(400.0)
        .h(300.0)
        .template_id(PORTAL_LAYER_ID)
        .child(host);
    let mut root = Element::new().w(400.0).h(300.0).child(layer);
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 400.0, 300.0, [0.0, 0.0]);
    assert!(
        result.overlay_split.marked,
        "portal layer must mark overlay split"
    );
    assert!(
        result.overlay_split.page_stack_bands.is_empty(),
        "nested portal z-stack must not use page_stack_bands (got {})",
        result.overlay_split.page_stack_bands.len()
    );
    assert!(
        result.overlay_split.bands.len() >= 3,
        "expect host band + 2 z-card bands, got {}",
        result.overlay_split.bands.len()
    );
    let ranges = result.overlay_split.band_ranges(
        result.instances.len(),
        result.textured_draws.len(),
        result.text_nodes.len(),
        result.input_chrome_instances.len(),
        result.paint_ops.len(),
    );
    let last = ranges.last().unwrap();
    assert!(
        result.text_nodes[last.text.0..last.text.1]
            .iter()
            .any(|t| t.text == "portal-high"),
        "highest z toast text must be in the last overlay band"
    );
}

#[test]
fn translated_opaque_shell_keeps_child_text_aligned() {
    // Shell translate must not be applied twice (rect_pos + matrix tx/ty).
    let mut shell = Element::new()
        .bg([1.0, 1.0, 1.0, 1.0])
        .w(160.0)
        .h(48.0)
        .absolute()
        .left(10.0)
        .top(10.0)
        .child(Element::new().text("shell-label").w(140.0).h(20.0));
    shell.style.translate = [0.0, 40.0];
    let mut root = Element::new().w(400.0).h(200.0).child(shell);
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 400.0, 200.0, [0.0, 0.0]);
    let shell_inst = result
        .instances
        .iter()
        .find(|i| i.color[0] > 0.9 && i.rect_size[1] > 40.0)
        .expect("opaque shell instance");
    let label = result
        .text_nodes
        .iter()
        .find(|t| t.text == "shell-label")
        .expect("label text");
    // Text top should sit inside the translated shell, not ~40px above it.
    assert!(
        (label.y - shell_inst.rect_pos[1]).abs() < 24.0,
        "text y={:.1} drifted from shell y={:.1} (double-translate?)",
        label.y,
        shell_inst.rect_pos[1]
    );
    // Matrix tx/ty must stay ~0; translation lives in rect_pos only.
    let tx = shell_inst.transform[4];
    let ty = shell_inst.transform[5];
    assert!(
        tx.abs() < 0.01 && ty.abs() < 0.01,
        "transform tx/ty should be 0 when translate is baked into rect_pos, got [{tx}, {ty}]"
    );
}

#[test]
fn rotate_origin_left_top_moves_shader_pivot() {
    fn collect_shell(origin: &str) -> crate::style::Mat2d {
        let mut shell = Element::new()
            .bg([0.9, 0.1, 0.1, 1.0])
            .w(100.0)
            .h(100.0)
            .absolute()
            .left(0.0)
            .top(0.0);
        shell.style.rotate_deg = 90.0;
        shell.style.transform_origin = crate::style::parse_transform_origin(origin);
        let mut root = Element::new().w(200.0).h(200.0).child(shell);
        let mut tree = LayoutTree::new();
        let result = tree.build_and_layout(&mut root, 200.0, 200.0, [0.0, 0.0]);
        result
            .instances
            .iter()
            .find(|i| i.color[0] > 0.8 && i.rect_size[0] > 80.0)
            .map(|i| i.transform)
            .expect("shell instance")
    }
    let center = collect_shell("center");
    let left_top = collect_shell("left top");
    assert!(
        center[4].abs() < 0.05 && center[5].abs() < 0.05,
        "default origin must keep shader tx/ty at 0, got {center:?}"
    );
    assert!(
        (left_top[4] + 100.0).abs() < 1.0 && left_top[5].abs() < 1.0,
        "left-top 90° on 100×100 must put origin into VS tx/ty (-100, 0), got {left_top:?}"
    );
    for i in 0..4 {
        assert!(
            (center[i] - left_top[i]).abs() < 1e-3,
            "origin must not change the 2×2 linear part"
        );
    }
}

#[test]
fn rotate_origin_left_top_keeps_shell_and_child_text_together() {
    fn collect(origin: &str) -> (crate::style::Mat2d, [f32; 4], crate::style::Mat2d) {
        let mut shell = Element::new()
            .bg([0.2, 0.4, 0.9, 1.0])
            .w(120.0)
            .h(48.0)
            .absolute()
            .left(40.0)
            .top(40.0)
            .child(Element::new().text("shell-label").w(100.0).h(24.0));
        shell.style.rotate_deg = 45.0;
        shell.style.transform_origin = crate::style::parse_transform_origin(origin);
        let mut root = Element::new().w(400.0).h(300.0).child(shell);
        let mut tree = LayoutTree::new();
        let result = tree.build_and_layout(&mut root, 400.0, 300.0, [0.0, 0.0]);
        let xf = result
            .instances
            .iter()
            .find(|i| i.color[0] > 0.15 && i.rect_size[1] > 40.0)
            .map(|i| i.transform)
            .expect("opaque shell");
        let bake = result
            .textured_draws
            .iter()
            .find(|d| {
                d.texture_key
                    .starts_with(crate::utils::text_texture::TEXT_XFORM_TEX_PREFIX)
                    || d.texture_key
                        .starts_with(crate::utils::text_texture::TEXT_MSDF_TEX_PREFIX)
            })
            .expect("baked child text");
        (xf, bake.rect, bake.transform)
    }
    let (center_xf, center_rect, center_bake_xf) = collect("center");
    let (lt_xf, lt_rect, lt_bake_xf) = collect("left top");
    assert!(
        center_xf[4].abs() < 0.05 && center_xf[5].abs() < 0.05,
        "center origin shell txty must stay 0, got {center_xf:?}"
    );
    assert!(
        lt_xf[4].abs() > 1.0 || lt_xf[5].abs() > 1.0,
        "left-top origin must move the shell shader pivot, got {lt_xf:?}"
    );
    let dcx = (center_rect[0] + center_rect[2] * 0.5) - (lt_rect[0] + lt_rect[2] * 0.5);
    let dcy = (center_rect[1] + center_rect[3] * 0.5) - (lt_rect[1] + lt_rect[3] * 0.5);
    assert!(
            dcx.abs() > 1.0 || dcy.abs() > 1.0,
            "child bake must orbit CSS origin, not stay at the center-pivot rect; center={center_rect:?} left-top={lt_rect:?}"
        );
    // Child itself has default origin (center): its VS txty stays 0; orbit is rect_pos.
    assert!(
        center_bake_xf[4].abs() < 0.05 && center_bake_xf[5].abs() < 0.05,
        "child bake txty at default origin must stay 0, got {center_bake_xf:?}"
    );
    assert!(
            lt_bake_xf[4].abs() < 0.05 && lt_bake_xf[5].abs() < 0.05,
            "child bake txty must stay 0 (orbit via rect); parent txty is on the shell, got {lt_bake_xf:?}"
        );
}

#[test]
fn scale_origin_left_top_moves_child_rect() {
    fn child_x(origin: &str) -> f32 {
        let mut shell = Element::new()
            .bg([0.2, 0.4, 0.9, 1.0])
            .w(100.0)
            .h(40.0)
            .absolute()
            .left(20.0)
            .top(20.0)
            .child(
                Element::new()
                    .bg([0.9, 0.2, 0.2, 1.0])
                    .w(20.0)
                    .h(20.0)
                    .absolute()
                    .left(10.0)
                    .top(10.0),
            );
        shell.style.scale = [2.0, 1.0];
        shell.style.transform_origin = crate::style::parse_transform_origin(origin);
        let mut root = Element::new().w(400.0).h(200.0).child(shell);
        let mut tree = LayoutTree::new();
        let result = tree.build_and_layout(&mut root, 400.0, 200.0, [0.0, 0.0]);
        result
            .instances
            .iter()
            .find(|i| i.color[0] > 0.8 && i.color[1] < 0.35)
            .map(|i| i.rect_pos[0])
            .unwrap_or_else(|| {
                panic!(
                    "child fill missing; instances={:?}",
                    result
                        .instances
                        .iter()
                        .map(|i| (i.color, i.rect_pos, i.rect_size))
                        .collect::<Vec<_>>()
                )
            })
    }
    let center = child_x("center");
    let left_top = child_x("left top");
    assert!(
        (center - left_top).abs() > 1.0,
        "child under scaled parent must follow origin, center_x={center} left_top_x={left_top}"
    );
}

#[test]
fn rotated_parent_propagates_paint_transform_to_child_text() {
    let mut shell = Element::new()
        .bg([0.2, 0.4, 0.9, 1.0])
        .w(120.0)
        .h(48.0)
        .absolute()
        .left(40.0)
        .top(40.0)
        .child(Element::new().text("rotate-me").w(100.0).h(24.0));
    shell.style.rotate_deg = 45.0;
    let mut root = Element::new().w(400.0).h(300.0).child(shell);
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 400.0, 300.0, [0.0, 0.0]);
    let cos45 = 45.0f32.to_radians().cos();
    // Sticky Bake: rotated text is a textured quad, not a glyphon TextLayoutInfo.
    assert!(
        result.text_nodes.iter().all(|t| t.text != "rotate-me"),
        "Bake path must not push glyphon text_nodes for rotated labels"
    );
    let bake = result
        .textured_draws
        .iter()
        .find(|d| {
            d.texture_key
                .starts_with(crate::utils::text_texture::TEXT_XFORM_TEX_PREFIX)
                || d.texture_key
                    .starts_with(crate::utils::text_texture::TEXT_MSDF_TEX_PREFIX)
        })
        .expect("baked text textured draw");
    assert!(
        !crate::style::mat2d_is_axis_aligned(&bake.transform),
        "child bake must inherit parent rotate via textured transform, got {:?}",
        bake.transform
    );
    assert!(
        (bake.transform[0] - cos45).abs() < 0.05,
        "expected ~45° linear a={}, got {}",
        cos45,
        bake.transform[0]
    );
    // Draw height must include leading + pad so glyphs aren't vertically clipped.
    let min_h = 24.0 * crate::style::tailwind_default_leading_scale(16.0);
    // Child text "rotate-me" uses default ~16px; pad expands beyond layout 24px height.
    assert!(
        bake.rect[3] + 0.5 >= 24.0,
        "bake draw h={} should be at least layout text h",
        bake.rect[3]
    );
    let _ = min_h;
}

#[test]
fn rotated_medium_label_bake_pads_synthetic_bold() {
    // DragOverlay rotate (gallery 四角) bakes the Label. font-medium 500 on
    // static Segoe uses Regular + synth halo — the bake quad must grow so the
    // halo is not clipped (Coverage already had the 0.18/0.32 stamp).
    fn bake_h(weight: u16) -> f32 {
        let mut t = crate::utils::text_node::apply_label_defaults(
            Element::new().text("拖到四角").w(120.0).h(24.0),
        );
        t.text_style.weight = Some(weight);
        let mut shell = Element::new()
            .w(160.0)
            .h(48.0)
            .absolute()
            .left(10.0)
            .top(10.0)
            .child(t);
        shell.style.rotate_deg = 15.0;
        let mut root = Element::new().w(400.0).h(200.0).child(shell);
        let mut tree = LayoutTree::new();
        let result = tree.build_and_layout(&mut root, 400.0, 200.0, [0.0, 0.0]);
        result
            .textured_draws
            .iter()
            .find(|d| {
                d.texture_key
                    .starts_with(crate::utils::text_texture::TEXT_XFORM_TEX_PREFIX)
                    || d.texture_key
                        .starts_with(crate::utils::text_texture::TEXT_MSDF_TEX_PREFIX)
            })
            .map(|d| d.rect[3])
            .expect("baked label")
    }
    let needs_synth = crate::text::measure::with_layout_font_system(|fs| {
        let family = crate::renderer::text_hit::best_font_family(fs);
        crate::utils::font::resolve_face_weight(fs.db(), family, 500).synthetic_bold
    });
    if !needs_synth {
        return;
    }
    let h400 = bake_h(400);
    let h500 = bake_h(500);
    assert!(
        h500 > h400 + 1.0,
        "synth-bold bake must expand shadow pad, 400={h400} 500={h500}"
    );
}

#[test]
fn rotated_label_bake_height_uses_leading_none_not_text_sm() {
    // DragOverlay 四角 ghost clones the Label (`leading-none` / 1em). Bake
    // must not grow the quad to Tailwind text-sm leading (~1.43em) — that
    // extra line box half-leads the glyphs ~1px down vs Coverage.
    let mut t = crate::utils::text_node::apply_label_defaults(
        Element::new().text("拖到四角").white_space("nowrap"),
    );
    t.style.rotate_deg = 15.0;
    // Shrink-wrap: a stretched 200px leaf would hide the 1em vs 1.43em delta.
    t = t.absolute().left(10.0).top(10.0);
    let mut root = Element::new().w(400.0).h(200.0).child(t);
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 400.0, 200.0, [0.0, 0.0]);
    let bake = result
        .textured_draws
        .iter()
        .find(|d| {
            d.texture_key
                .starts_with(crate::utils::text_texture::TEXT_XFORM_TEX_PREFIX)
                || d.texture_key
                    .starts_with(crate::utils::text_texture::TEXT_MSDF_TEX_PREFIX)
        })
        .expect("baked label");
    let font = 14.0_f32;
    let ink_pad = crate::utils::text_texture::text_xform_pad_px(font);
    let needs_synth = crate::text::measure::with_layout_font_system(|fs| {
        let family = crate::renderer::text_hit::best_font_family(fs);
        crate::utils::font::resolve_face_weight(fs.db(), family, 500).synthetic_bold
    });
    let extra = if needs_synth {
        let (_, off, blur) = crate::utils::font::synthetic_bold_shadow([20, 20, 20, 255], 1.0);
        crate::utils::text_texture::text_xform_shadow_pad(off, blur)
    } else {
        0.0
    };
    let pad = ink_pad + extra;
    let none_h = font + pad * 2.0;
    let sm_h = font * crate::style::tailwind_default_leading_scale(font) + pad * 2.0;
    assert!(
        (bake.rect[3] - none_h).abs() < 1.0,
        "leading-none bake h={} must match 1em+pad ({none_h}), not text-sm ({sm_h})",
        bake.rect[3]
    );
    assert!(
        bake.rect[3] + 1.0 < sm_h,
        "must not use text-sm leading for Label ghost (h={} sm={sm_h})",
        bake.rect[3]
    );
}

#[test]
fn rotated_flex_centered_caption_bake_pivot_matches_layout_box() {
    let mut shell = Element::new()
        .bg([0.2, 0.4, 0.9, 1.0])
        .w(120.0)
        .h(48.0)
        .items_center()
        .justify_center()
        .absolute()
        .left(40.0)
        .top(40.0)
        .child({
            let mut t = Element::new().text("倾斜按钮").w(90.0).h(28.0);
            t.text_style.size = Some(14.0);
            t.text_style.align = Some(crate::style::TextAlign::Center);
            t
        });
    shell.style.rotate_deg = -15.0;
    let mut root = Element::new().w(400.0).h(300.0).child(shell);
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 400.0, 300.0, [0.0, 0.0]);
    let caption = root
        .children
        .first()
        .and_then(|s| s.children.first())
        .expect("caption");
    let [lx, ly, lw, lh] = caption.layout_bounds;
    let layout_cx = lx + lw * 0.5;
    let layout_cy = ly + lh * 0.5;
    let bake = result
        .textured_draws
        .iter()
        .find(|d| {
            d.texture_key
                .starts_with(crate::utils::text_texture::TEXT_XFORM_TEX_PREFIX)
                || d.texture_key
                    .starts_with(crate::utils::text_texture::TEXT_MSDF_TEX_PREFIX)
        })
        .expect("baked caption");
    let bake_cx = bake.rect[0] + bake.rect[2] * 0.5;
    let bake_cy = bake.rect[1] + bake.rect[3] * 0.5;
    assert!(
            (bake_cx - layout_cx).abs() < 1.5 && (bake_cy - layout_cy).abs() < 1.5,
            "bake pivot ({bake_cx:.1},{bake_cy:.1}) must match layout box center ({layout_cx:.1},{layout_cy:.1})"
        );
}

#[test]
fn bake_draw_taller_than_tight_font_box() {
    let mut shell = Element::new()
        .bg([0.2, 0.4, 0.9, 1.0])
        .w(100.0)
        .h(40.0)
        .absolute()
        .left(10.0)
        .top(10.0)
        .child({
            let mut t = Element::new().text("45°").w(40.0).h(12.0);
            t.text_style.size = Some(12.0);
            t
        });
    shell.style.rotate_deg = 45.0;
    let mut root = Element::new().w(200.0).h(200.0).child(shell);
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 200.0, 200.0, [0.0, 0.0]);
    let bake = result
        .textured_draws
        .iter()
        .find(|d| {
            d.texture_key
                .starts_with(crate::utils::text_texture::TEXT_XFORM_TEX_PREFIX)
                || d.texture_key
                    .starts_with(crate::utils::text_texture::TEXT_MSDF_TEX_PREFIX)
        })
        .expect("bake draw");
    let leading = 12.0 * crate::style::tailwind_default_leading_scale(12.0);
    let pad = crate::utils::text_texture::text_xform_pad_px(12.0);
    assert!(
        bake.rect[3] + 0.01 >= leading + pad * 2.0,
        "bake h={} must cover leading+pad ({}), got clip risk",
        bake.rect[3],
        leading + pad * 2.0
    );
}

#[cfg(feature = "gallery-tests")]
#[test]
fn rotated_lucide_chevron_uses_square_texture_not_glyphon() {
    let glyph = crate::utils::lucide_glyph("chevron-down").expect("glyph");
    let mut icon = crate::utils::build_named_icon_text_node("chevron-down", glyph)
        .text_size(16.0)
        .w(16.0)
        .h(16.0)
        .flex_shrink(0.0)
        .text_color([128, 128, 128, 255]);
    icon.style.rotate_deg = 180.0;
    let mut root = Element::new().w(64.0).h(64.0).child(icon);
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 64.0, 64.0, [0.0, 0.0]);
    let draw = result
        .textured_draws
        .iter()
        .find(|d| d.texture_key.starts_with("jadix_lucide_"))
        .expect("rotated lucide must use icon texture");
    assert!(
        (draw.rect[2] - draw.rect[3]).abs() < 0.01,
        "icon quad must be square, got {}x{}",
        draw.rect[2],
        draw.rect[3]
    );
    assert!(
        (draw.rect[2] - 16.0).abs() < 0.5,
        "draw side must match text size, got {}",
        draw.rect[2]
    );
    assert!(
        (draw.transform[0] + 1.0).abs() < 0.05 && (draw.transform[3] + 1.0).abs() < 0.05,
        "180° linear should be -I, got {:?}",
        draw.transform
    );
    assert!(
        result
            .text_nodes
            .iter()
            .all(|t| t.style.font_family.as_deref() != Some("lucide")),
        "must not also emit glyphon lucide (double-draw thickens strokes)"
    );
}

#[cfg(feature = "gallery-tests")]
#[test]
fn lucide_opacity_fades_quad_without_rebaking_alpha() {
    // Latest entrance: the arrow is a texture, the label is glyphon.
    // Baking opacity into the raster and multiplying the quad again dropped
    // the arrow while "Latest" was still visible.
    let glyph = crate::utils::lucide_glyph("arrow-down").expect("glyph");
    let icon = || {
        crate::utils::build_named_icon_text_node("arrow-down", glyph.clone())
            .text_size(16.0)
            .w(16.0)
            .h(16.0)
            .text_color([20, 20, 20, 255])
    };
    let mut opaque = Element::new().w(32.0).h(32.0).child(icon());
    let mut faded_icon = icon();
    faded_icon.style.opacity = 0.4;
    let mut faded = Element::new().w(32.0).h(32.0).child(faded_icon);
    let mut tree = LayoutTree::new();
    let solid = tree.build_and_layout(&mut opaque, 32.0, 32.0, [0.0, 0.0]);
    let mid = tree.build_and_layout(&mut faded, 32.0, 32.0, [0.0, 0.0]);
    let solid_draw = solid
        .textured_draws
        .iter()
        .find(|d| d.texture_key.starts_with("jadix_lucide_"))
        .expect("opaque arrow texture");
    let faded_draw = mid
        .textured_draws
        .iter()
        .find(|d| d.texture_key.starts_with("jadix_lucide_"))
        .expect("faded arrow texture");
    assert_eq!(
        solid_draw.texture_key, faded_draw.texture_key,
        "opacity must not mint a new raster; the quad fades"
    );
    assert!(
        (faded_draw.opacity - 0.4).abs() < 0.02,
        "quad opacity carries the fade once, got {}",
        faded_draw.opacity
    );
}

/// Scaled parent under scroll must pass pre-scroll `abs_offset` to children.
/// Using post-scroll `render_*` double-subtracted scroll and culled CarouselStack sides.
#[test]
fn scaled_parent_under_scroll_keeps_child_paint_aligned() {
    let mut child = Element::new()
        .w(100.0)
        .h(100.0)
        .bg([1.0, 0.0, 0.0, 1.0])
        .template_id("scaled-child");
    child = child.child(Element::new().text("scale-me"));
    let mut shell = Element::new()
        .w(100.0)
        .h(100.0)
        .bg([0.0, 0.0, 0.0, 0.0])
        .child(child);
    shell.style.scale = [0.8, 0.8];
    // `overflow: scroll` (not hidden) contributes `scroll_offset` into child `scroll_acc`.
    let mut root = Element::new()
        .w(200.0)
        .h(200.0)
        .overflow_y("scroll")
        .overflow_x("scroll")
        .child(shell);
    root.scroll_offset = [0.0, 50.0];

    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 200.0, 200.0, [-1.0, -1.0]);

    let red = result
        .instances
        .iter()
        .find(|i| i.color[0] > 0.9 && i.color[1] < 0.1 && i.rect_size[0] > 1.0)
        .expect("scaled red child instance");
    // Layout top 0 − scroll 50 = −50; scale 0.8 about center → visual top −50 + 10 = −40.
    assert!(
        (red.rect_pos[1] - (-40.0)).abs() < 2.0,
        "child visual top must be ~-40 after one scroll subtract (not -90); got {}",
        red.rect_pos[1]
    );
    assert!(
        (red.rect_size[0] - 80.0).abs() < 2.0,
        "child width must bake ancestor scale 0.8 → 80; got {}",
        red.rect_size[0]
    );
    assert!(
        result.text_nodes.iter().any(|t| t.text == "scale-me"),
        "text under scaled+scrolled parent must still collect"
    );
    let label = result
        .text_nodes
        .iter()
        .find(|t| t.text == "scale-me")
        .expect("scale-me text");
    assert!(
        (label.paint_scale - 0.8).abs() < 0.02,
        "CSS scale must go to paint_scale (not font size); got {}",
        label.paint_scale
    );
    // Default text size is not multiplied by 0.8 (that was the drag-lag path).
    assert!(
        label.style.size_resolved() > 10.0,
        "layout font size must stay unscaled for stable reshape; got {}",
        label.style.size_resolved()
    );
}

/// `__modal_zoom` at 0.90: live rounded chrome + one inner blit of rest children.
/// Fade lives on chrome and the blit; the RT stays content-stable.
#[test]
fn modal_zoom_layer_paints_as_one_scaled_capture() {
    let _ = crate::renderer::take_capture_jobs();
    let title = Element::new().text("Dialog title");
    let mut card = Element::new()
        .w(240.0)
        .h(120.0)
        .bg([1.0, 1.0, 1.0, 1.0])
        .child(title);
    card.template_id = Some("dlg__modal_zoom".into());
    card.style.scale = [0.90, 0.90];
    card.style.opacity = 0.5;
    let mut root = Element::new().w(400.0).h(400.0).child(card);

    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 400.0, 400.0, [-1.0, -1.0]);

    let caps = crate::renderer::take_capture_jobs();
    let cap = caps
        .iter()
        .find(|c| c.key.starts_with("jadix_zoom_cap:"))
        .expect("zoom capture");
    assert!(
        (cap.bounds[2] - 240.0).abs() < 0.5,
        "capture is rest layout (no shadow pad); got {}",
        cap.bounds[2]
    );
    let live = result
        .instances
        .iter()
        .find(|i| i.rect_size[0] > 100.0)
        .expect("live chrome in main pass");
    assert!(
        (live.rect_size[0] - 216.0).abs() < 1.0,
        "live chrome bakes CSS scale; got {}",
        live.rect_size[0]
    );
    assert!(
        (live.color[3] - 0.5).abs() < 0.05,
        "fade lives on live chrome; a={}",
        live.color[3]
    );
    let blit = result
        .textured_draws
        .iter()
        .find(|d| d.texture_key.starts_with("jadix_zoom_cap:"))
        .expect("zoom blit dest");
    assert!(
        (blit.rect[2] - 216.0).abs() < 0.5,
        "dest w should be layout×0.90 about card center; got {}",
        blit.rect[2]
    );
    assert!(
        (blit.opacity - 0.5).abs() < 1e-3,
        "fade lives on the blit; got {}",
        blit.opacity
    );
    let title = cap
        .text_nodes
        .iter()
        .find(|t| t.text == "Dialog title")
        .expect("title in capture at rest");
    assert!(
        (title.paint_scale - 1.0).abs() < 1e-4,
        "capture freezes CSS scale; got {}",
        title.paint_scale
    );
    assert!(
        result.text_nodes.iter().all(|t| t.text != "Dialog title"),
        "title must not also paint in the main pass"
    );

    root.children[0].style.scale = [0.995, 0.995];
    let _ = crate::renderer::take_capture_jobs();
    let near_one = tree.build_and_layout(&mut root, 400.0, 400.0, [-1.0, -1.0]);
    assert!(
        crate::renderer::take_capture_jobs()
            .iter()
            .any(|c| c.key.starts_with("jadix_zoom_cap:")),
        "scale 0.995 must stay on the compositor (1e-3 used to drain early)"
    );
    assert!(near_one
        .textured_draws
        .iter()
        .any(|d| d.texture_key.starts_with("jadix_zoom_cap:")));

    root.children[0].style.scale = [1.0, 1.0];
    root.children[0].style.animations = vec![crate::style::keyframes_anim_entry(
        "zoom-in-90",
        200.0,
        crate::style::TimingFunction::Ease,
        crate::style::AnimationFillMode::Both,
    )];
    let _ = crate::renderer::take_capture_jobs();
    let at_one = tree.build_and_layout(&mut root, 400.0, 400.0, [-1.0, -1.0]);
    assert!(
        crate::renderer::take_capture_jobs()
            .iter()
            .any(|c| c.key.starts_with("jadix_zoom_cap:")),
        "fill-mode scale=1 must keep the 1:1 blit until zoom-in-90 is cleared"
    );
    assert!(at_one
        .textured_draws
        .iter()
        .any(|d| d.texture_key.starts_with("jadix_zoom_cap:")));
    assert!(
        at_one.text_nodes.iter().all(|t| t.text != "Dialog title"),
        "1:1 blit must not also emit live Coverage (that pop is the row jitter)"
    );

    root.children[0].style.animations.clear();
    root.children[0].style.opacity = 1.0;
    let settled = tree.build_and_layout(&mut root, 400.0, 400.0, [-1.0, -1.0]);
    let idle = settled
        .text_nodes
        .iter()
        .find(|t| t.text == "Dialog title")
        .expect("idle Coverage");
    assert!((idle.paint_scale - 1.0).abs() < 1e-4);
    assert!(
        crate::renderer::take_capture_jobs()
            .iter()
            .all(|c| !c.key.starts_with("jadix_zoom_cap:")),
        "scale ≈ 1 must not capture"
    );
}

#[test]
fn modal_zoom_capture_keeps_semantic_description_color() {
    use crate::style::{muted_from_accent, parse_color, parse_text_color};

    let _ = crate::renderer::take_capture_jobs();
    let muted = muted_from_accent(parse_text_color("green-600"));
    let card_bg = parse_color("card");
    let mut desc = Element::new().text("You can change this later in Settings.");
    desc.text_style.color = Some(muted);
    let mut card = Element::new().w(240.0).h(120.0).bg(card_bg).child(desc);
    card.template_id = Some("dlg__modal_zoom".into());
    card.style.scale = [0.90, 0.90];
    let mut root = Element::new().w(400.0).h(400.0).child(card);

    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 400.0, 400.0, [-1.0, -1.0]);
    let caps = crate::renderer::take_capture_jobs();
    let cap = caps
        .iter()
        .find(|c| c.key.starts_with("jadix_zoom_cap:"))
        .expect("zoom capture");
    let desc = cap
        .text_nodes
        .iter()
        .find(|t| t.text.contains("You can change"))
        .expect("description in capture");
    let got = desc.style.color_resolved();
    assert_eq!(
        got[3], 255,
        "zoom capture must not leave translucent glyphs on a transparent RT"
    );
    assert_eq!(
            got, muted,
            "success description must keep the variant muted hue during enter, not theme muted-foreground"
        );
    assert_ne!(got, parse_text_color("muted-foreground"));
    assert_ne!(got, parse_text_color("foreground"));
}

#[test]
fn modal_zoom_capture_flattens_translucent_glyphs_over_card() {
    use crate::style::{parse_color, parse_text_color};

    let _ = crate::renderer::take_capture_jobs();
    let mut faded = parse_text_color("green-600");
    faded[3] = 199; // old muted_from_accent alpha (~0.78)
    let card_bg = parse_color("card");
    let mut desc = Element::new().text("You can change this later in Settings.");
    desc.text_style.color = Some(faded);
    let mut card = Element::new().w(240.0).h(120.0).bg(card_bg).child(desc);
    card.template_id = Some("dlg__modal_zoom".into());
    card.style.scale = [0.90, 0.90];
    let mut root = Element::new().w(400.0).h(400.0).child(card);

    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 400.0, 400.0, [-1.0, -1.0]);
    let caps = crate::renderer::take_capture_jobs();
    let cap = caps
        .iter()
        .find(|c| c.key.starts_with("jadix_zoom_cap:"))
        .expect("zoom capture");
    let desc = cap
        .text_nodes
        .iter()
        .find(|t| t.text.contains("You can change"))
        .expect("description in capture");
    let got = desc.style.color_resolved();
    assert_eq!(got[3], 255);
    assert_ne!(
        got, faded,
        "translucent glyphs must composite over the card, not sit on a transparent RT"
    );
    assert_ne!(got, parse_text_color("muted-foreground"));
    assert_ne!(got, parse_text_color("foreground"));
}

/// Dialog `shadow-lg` + rounded overflow: chrome stays live SDF in the main pass
/// (rounded border + halo). Capture is the inner padding box, not a dest AABB
/// around the shadow that reads as a rectangular stroke.
#[test]
fn modal_zoom_capture_includes_box_shadow_outset() {
    let _ = crate::renderer::take_capture_jobs();
    let paint_scale = 0.90;
    let expected_shadow_blur = 16.0 * paint_scale;
    let mut card = Element::new()
        .w(240.0)
        .h(120.0)
        .bg([1.0, 1.0, 1.0, 1.0])
        .rounded(8.0)
        .border_style("solid")
        .border_width(1.0)
        .border_color([0.2, 0.2, 0.2, 1.0])
        .shadow([0.0, 0.0, 0.0, 0.16], 16.0)
        .overflow_y("hidden")
        .child(Element::new().text("Dialog title"));
    card.template_id = Some("dlg__modal_zoom".into());
    card.style.scale = [paint_scale, paint_scale];
    let mut root = Element::new().w(400.0).h(400.0).child(card);

    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 400.0, 400.0, [-1.0, -1.0]);
    let caps = crate::renderer::take_capture_jobs();
    let cap = caps
        .iter()
        .find(|c| c.key.starts_with("jadix_zoom_cap:"))
        .expect("zoom capture");
    assert!(
        (cap.bounds[2] - 238.0).abs() < 0.5,
        "capture is padding box (240−2×1px border); got {}",
        cap.bounds[2]
    );
    let blit = result
        .textured_draws
        .iter()
        .find(|d| d.texture_key.starts_with("jadix_zoom_cap:"))
        .expect("zoom blit dest");
    assert!(
        (blit.rect[2] - 214.2).abs() < 0.5,
        "dest w should be padding×0.90; got {}",
        blit.rect[2]
    );
    assert!(
        cap.instances
            .iter()
            .all(|i| i.shadow_blur < expected_shadow_blur - 0.1 || i.rect_size[0] < 1.0),
        "shadow chrome must not enter the inner capture"
    );
    let chrome = result
        .instances
        .iter()
        .find(|i| (i.shadow_blur - expected_shadow_blur).abs() < 0.1 && i.rect_size[0] > 1.0)
        .expect("dialog chrome live in main pass");
    assert!(
        chrome.radii.iter().all(|r| *r > 7.0),
        "live chrome must stay rounded SDF; got {:?}",
        chrome.radii
    );
    assert!(
        (chrome.border_width - 1.0).abs() < 0.15,
        "1px border stays on the live SDF instance; got {}",
        chrome.border_width
    );
    assert!(
        chrome.clip_size == InstanceInput::NO_CLIP_SIZE || chrome.clip_size[0] < 0.0,
        "own overflow must not AABB-clip the shadow; clip_size={:?}",
        chrome.clip_size
    );
    assert!(
        (chrome.rect_size[0] - 216.0).abs() < 1.0,
        "live chrome bakes CSS scale 0.90; got {}",
        chrome.rect_size[0]
    );
}

/// Select-style 1px outline must stay live SDF (not a minified blit hairline).
#[test]
fn modal_zoom_inner_border_stays_live_one_px() {
    let _ = crate::renderer::take_capture_jobs();
    let trigger = Element::new()
        .w(200.0)
        .h(32.0)
        .bg([1.0, 1.0, 1.0, 1.0])
        .rounded(8.0)
        .border_style("solid")
        .border_width(1.0)
        .border_color([0.7, 0.7, 0.7, 1.0])
        .child(Element::new().text("Pick an option"));
    let mut card = Element::new()
        .w(240.0)
        .h(120.0)
        .bg([1.0, 1.0, 1.0, 1.0])
        .rounded(8.0)
        .child(trigger);
    card.template_id = Some("dlg__modal_zoom".into());
    card.style.scale = [0.90, 0.90];
    let mut root = Element::new().w(400.0).h(400.0).child(card);

    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 400.0, 400.0, [-1.0, -1.0]);
    let caps = crate::renderer::take_capture_jobs();
    let cap = caps
        .iter()
        .find(|c| c.key.starts_with("jadix_zoom_cap:"))
        .expect("zoom capture");
    assert!(
        cap.instances
            .iter()
            .all(|i| (i.border_width - 1.0).abs() > 0.5 || i.rect_size[1] > 40.0),
        "1px trigger chrome must not enter the blit"
    );
    let trigger = result
        .instances
        .iter()
        .find(|i| i.border_width > 0.5 && i.rect_size[1] < 40.0)
        .expect("live 1px trigger in main pass");
    assert!(
        (trigger.border_width - 1.0).abs() < 0.15,
        "hairline stays 1px SDF; got {}",
        trigger.border_width
    );
    assert!(
        (trigger.rect_size[0] - 180.0).abs() < 2.0,
        "trigger scales about the card center (200×0.90); got {}",
        trigger.rect_size[0]
    );
    assert!(
        trigger.radii.iter().all(|r| *r > 6.0 && *r < 8.0),
        "radii scale with the card; got {:?}",
        trigger.radii
    );
}

fn text_geom(info: &TextLayoutInfo) -> (f32, f32, f32, f32) {
    (info.x, info.y, info.width, info.height)
}

/// Gallery Dialog shell at enter: live rounded chrome in the main pass;
/// 「行 1–6」 stay at rest inside the inner capture (no per-leaf crawl).
#[test]
#[cfg(feature = "product-tests")]
fn modal_zoom_dialog_enter_captures_rows_at_rest() {
    use crate::app::portal_overlay::clear_overlay_stack;
    use crate::authoring::SlotChrome;
    use crate::components::dialog::dialog_shell_frame;
    use crate::components::scroll_area::scroll_area;
    use crate::core::reactivity::{create_root, Signal};
    use crate::utils::modal_motion::flush_modal_shell_motion_subtree;
    use crate::utils::modal_shell::{default_modal_overlay_color, ShellOverlayOpts};

    let _ = crate::renderer::take_capture_jobs();
    clear_overlay_stack();
    let modal_id = "gal-dialog";
    let (_owner, mut host) = create_root(|| {
        let open = Signal::new(true);
        let overlay_opts = ShellOverlayOpts {
            color: default_modal_overlay_color(None, Some(0.5)),
            close_on_click: true,
            backdrop_blur: 0.0,
        };
        let mut rows = Element::new().w("100%").flex_col().gap(8.0);
        for i in 1..=6 {
            rows = rows.child(Element::new().text(format!("行 {i}")).text_size(14.0));
        }
        let body_id = format!("{modal_id}__body");
        let scroll: Element = scroll_area(body_id.as_str())
            .overflow_x("hidden")
            .overflow_y("auto")
            .scrollbar_visibility("auto")
            .w("100%")
            .h("auto")
            .min_h("0")
            .flex_grow(1.0)
            .flex_shrink(1.0)
            .child(rows)
            .into();
        let inner = Element::new()
            .w("100%")
            .flex_col()
            .child(
                Element::new()
                    .text("Gallery Dialog")
                    .text_size(18.0)
                    .font_weight(600.0),
            )
            .child(scroll);
        let shell = dialog_shell_frame(
            modal_id,
            open,
            inner,
            &overlay_opts,
            None,
            false,
            None,
            None,
            None,
            None,
            SlotChrome::default(),
        );
        Element::new().w(800.0).h(600.0).child(shell)
    });
    assert!(hoist_portal_subtrees(&mut host) >= 2);
    let _ = flush_modal_shell_motion_subtree(&mut host);

    // Pin a visible mid-enter pose. At t=0 chrome is deliberately omitted
    // by the alpha-zero collect gate. Product shadow tokens are not the
    // geometry contract this capture test is checking.
    crate::core::tree::walk_elements_mut(&mut host, |el| {
        if el.template_id.as_deref() == Some("gal-dialog__modal_zoom") {
            el.style.opacity = 0.5;
            el.style.scale = [0.95, 0.95];
            el.style.shadow_color = [0.0, 0.0, 0.0, 0.16];
            el.style.shadow_blur = 16.0;
        }
    });

    let mut tree = LayoutTree::new();
    let zooming = tree.build_and_layout(&mut host, 800.0, 600.0, [-1.0, -1.0]);
    let caps = crate::renderer::take_capture_jobs();
    let cap = caps
        .iter()
        .find(|c| c.key.starts_with("jadix_zoom_cap:"))
        .expect("enter must capture __modal_zoom");
    assert!(
        zooming
            .textured_draws
            .iter()
            .any(|d| d.texture_key.starts_with("jadix_zoom_cap:")),
        "main pass blits the capture"
    );
    let chrome = zooming
        .instances
        .iter()
        .find(|i| i.shadow_blur > 15.0 && i.radii[0] > 1.0)
        .expect("rounded dialog chrome live in main pass");
    assert!(
        chrome.radii.iter().all(|r| *r > 7.0),
        "live chrome must stay rounded SDF; got {:?}",
        chrome.radii
    );
    assert!(
        (chrome.border_width - 1.0).abs() < 0.15,
        "1px border stays on live chrome; got {}",
        chrome.border_width
    );
    for i in 1..=6 {
        let needle = format!("行 {i}");
        let node = cap
            .text_nodes
            .iter()
            .find(|t| t.text == needle)
            .unwrap_or_else(|| panic!("missing capture {needle}"));
        assert!(
            (node.paint_scale - 1.0).abs() < 1e-4,
            "{needle} must stay rest-size in the RT; paint_scale={}",
            node.paint_scale
        );
        assert!(
            zooming.text_nodes.iter().all(|t| t.text != needle),
            "{needle} must not also paint as live Coverage (per-leaf crawl)"
        );
    }
}

fn find_text<'a>(nodes: &'a [TextLayoutInfo], needle: &str) -> &'a TextLayoutInfo {
    nodes
        .iter()
        .find(|t| t.text == needle)
        .unwrap_or_else(|| panic!("missing text {needle:?}"))
}

/// Capture text is rest layout (same boxes as idle). The GPU scales the whole
/// layer about the card center — rows must not crawl toward/away from the title.
#[test]
fn modal_zoom_capture_text_geometry_matches_idle() {
    let _ = crate::renderer::take_capture_jobs();
    let chevron = Element::new()
        .font_family("lucide")
        .text("v")
        .text_size(16.0)
        .w(16.0)
        .h(16.0);
    let trigger = Element::new()
        .w(220.0)
        .h(32.0)
        .flex_row()
        .items_center()
        .child(
            Element::new()
                .text("Pick an option")
                .text_size(14.0)
                .white_space("nowrap"),
        )
        .child(chevron);
    let mut body = Element::new().w(240.0).flex_col().child(trigger);
    for i in 1..=6 {
        body = body.child(
            Element::new()
                .text(format!("行 {i}"))
                .text_size(14.0)
                .text_leading_none(),
        );
    }
    let mut card = Element::new()
        .w(240.0)
        .h(280.0)
        .bg([1.0, 1.0, 1.0, 1.0])
        .child(body);
    card.template_id = Some("dlg__modal_zoom".into());
    card.style.scale = [0.90, 0.90];
    let mut root = Element::new().w(400.0).h(400.0).child(card);

    let mut tree = LayoutTree::new();
    let _zooming = tree.build_and_layout(&mut root, 400.0, 400.0, [-1.0, -1.0]);
    let caps = crate::renderer::take_capture_jobs();
    let cap = caps
        .iter()
        .find(|c| c.key.starts_with("jadix_zoom_cap:"))
        .expect("zoom capture");

    root.children[0].style.scale = [1.0, 1.0];
    let idle = tree.build_and_layout(&mut root, 400.0, 400.0, [-1.0, -1.0]);
    let _ = crate::renderer::take_capture_jobs();

    for needle in [
        "Pick an option",
        "行 1",
        "行 2",
        "行 3",
        "行 4",
        "行 5",
        "行 6",
    ] {
        let z = find_text(&cap.text_nodes, needle);
        let i = find_text(&idle.text_nodes, needle);
        let (zx, zy, zw, zh) = text_geom(z);
        let (ix, iy, iw, ih) = text_geom(i);
        assert!(
            (zx - ix).abs() < 1.0 && (zy - iy).abs() < 1.0,
            "{needle}: capture ({zx:.2},{zy:.2}) must match idle rest ({ix:.2},{iy:.2})"
        );
        assert!(
            (zw - iw).abs() < 0.5 && (zh - ih).abs() < 0.5,
            "{needle} layout size jumped: capture {zw:.2}x{zh:.2} idle {iw:.2}x{ih:.2}"
        );
        assert!((z.paint_scale - 1.0).abs() < 1e-4);
        assert!((i.paint_scale - 1.0).abs() < 1e-4);
    }

    let z_icon = cap
        .text_nodes
        .iter()
        .find(|t| t.style.font_family.as_deref() == Some("lucide"))
        .expect("capture lucide");
    let i_icon = idle
        .text_nodes
        .iter()
        .find(|t| t.style.font_family.as_deref() == Some("lucide"))
        .expect("idle lucide");
    assert!((z_icon.style.size_resolved() - i_icon.style.size_resolved()).abs() < 1e-3);
}

/// Dialog `zoom-in-95` keeps `scale` < 1 during entrance; soft-wrapping description
/// must still use layout width (not forced nowrap) so lines match the stable frame.
#[test]
fn css_scale_preserves_soft_wrap_during_paint() {
    let long = "desktop_smoke #7: Body 内含 Select; 先开下拉再 Esc, 应只关顶层浮层。";
    let child = Element::new().w(200.0).h(60.0).text(long);
    let mut shell = Element::new().w(200.0).h(60.0).child(child);
    shell.style.scale = [0.95, 0.95];
    let mut root = Element::new().w(400.0).h(400.0).child(shell);

    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 400.0, 400.0, [0.0, 0.0]);

    let label = result
        .text_nodes
        .iter()
        .find(|t| t.text.contains("desktop_smoke"))
        .expect("scaled description text");
    assert!(
        label.style.white_space == crate::style::WhiteSpace::Normal,
        "CSS scale must not force nowrap during paint; got {:?}",
        label.style.white_space
    );
    assert!(
        (label.paint_scale - 0.95).abs() < 0.02,
        "paint_scale must carry CSS scale; got {}",
        label.paint_scale
    );
}

#[test]
fn paint_ops_texture_then_sibling_rect() {
    let mut tex = Element::new().w(40.0).h(8.0);
    tex.image_texture = Some("ramp".into());
    let thumb = Element::new().w(20.0).h(20.0).bg([1.0, 1.0, 1.0, 1.0]);
    let mut root = Element::new()
        .w(200.0)
        .h(40.0)
        .relative()
        .child(tex)
        .child(thumb);
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 200.0, 40.0, [0.0, 0.0]);
    let kinds: Vec<&'static str> = result
        .paint_ops
        .iter()
        .map(|op| match op {
            PaintOp::Rects { .. } => "r",
            PaintOp::Textured { .. } => "t",
        })
        .collect();
    let tex_at = kinds.iter().position(|k| *k == "t");
    let rect_after = kinds.iter().rposition(|k| *k == "r");
    assert!(tex_at.is_some() && rect_after.is_some());
    assert!(
        tex_at.unwrap() < rect_after.unwrap(),
        "textured track must paint before sibling thumb rects, ops={kinds:?}"
    );
}

#[test]
fn portal_drops_when_slot_removed() {
    let mut panel = Element::new().w(100.0).h(40.0);
    panel.set_attr(ATTR_PORTAL_KEY, "panel:close");
    let mut root = Element::new().child(panel_block_clicks(panel, 1001));
    assert_eq!(hoist_portal_subtrees(&mut root), 1);
    assert_eq!(layer_children(&root).len(), 1);

    // Overlay closed: reactive tree no longer declares the portal/slot.
    root.children.retain(|c| {
        c.template_id.as_deref() == Some(PORTAL_LAYER_ID) || c.attrs.get(ATTR_PORTAL_SLOT).is_none()
    });

    let _ = hoist_portal_subtrees(&mut root);
    assert!(
        layer_children(&root).is_empty(),
        "closing must drop hoisted content when no longer wanted"
    );
}

#[test]
fn overflow_scroll_does_not_clip_own_outset_shadow() {
    // Regression: pairing overflow with box-shadow used to scissor the element's own
    // paint to its border box, turning a soft rounded shadow into a sharp 90° corner.
    let mut panel = Element::new()
        .w(120.0)
        .h(80.0)
        .bg([1.0, 1.0, 1.0, 1.0])
        .rounded(8.0)
        .shadow([0.0, 0.0, 0.0, 0.25], 8.0)
        .overflow_y("auto")
        .absolute()
        .left(40.0)
        .top(40.0);
    panel = panel.child(Element::new().w(100.0).h(200.0).bg([0.5, 0.5, 0.5, 1.0]));

    let mut root = Element::new().w(800.0).h(600.0).child(panel);
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);

    let panel_idx = result
        .instances
        .iter()
        .position(|i| {
            i.shadow_blur > 0.0
                && i.color[3] > 0.5
                && (i.rect_size[0] - 120.0).abs() < 1.0
                && (i.rect_size[1] - 80.0).abs() < 1.0
        })
        .expect("panel chrome with shadow");
    let panel_inst = &result.instances[panel_idx];
    assert!(
        panel_inst.clip_size == InstanceInput::NO_CLIP_SIZE || panel_inst.clip_size[0] < 0.0,
        "own overflow must not clip outset shadow; clip_size={:?}",
        panel_inst.clip_size
    );
    assert!(
        result
            .clip_polys
            .get(panel_idx)
            .is_none_or(|p| !p.is_active()),
        "rounded overflow clip-poly on own chrome also kills the halo (Dialog/Drawer)"
    );

    let child_clipped = result.instances.iter().any(|i| {
        i.color[0] > 0.4
            && i.color[0] < 0.6
            && i.clip_size[0] > 0.0
            && i.clip_size[1] > 0.0
            && (i.clip_size[0] - 120.0).abs() < 2.0
            && (i.clip_size[1] - 80.0).abs() < 2.0
    });
    assert!(
        child_clipped,
        "overflow must still clip descendants to the panel box"
    );
}

#[test]
fn local_backdrop_blur_stamps_on_mask_instance() {
    // Gallery LocalOverlayShowcase: host stacking + 75% rounded scrim with blur.
    // Blur binds to that SDF instance (radii + overflow clip), not a parallel AABB.
    let host = Element::new()
        .relative()
        .w(220.0)
        .h(140.0)
        .template_id("card-host");
    let scrim = Element::new()
        .w(165.0)
        .h(105.0)
        .rounded(10.0)
        .bg([15.0, 23.0, 42.0, 140.0])
        .backdrop_blur(12.0)
        .template_id("local-scrim");
    let mut wrapper = Element::new()
        .w("100%")
        .h("100%")
        .absolute()
        .inset(0.0)
        .z_index(2)
        .child(scrim);
    wrapper.set_attr(ATTR_HOISTED_MOUNT, "card-host");
    let mut root = Element::new().children([host, wrapper]);
    assert_eq!(hoist_portal_subtrees(&mut root), 1);

    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 400.0, 300.0, [-1.0, -1.0]);
    let blur_insts: Vec<_> = result
        .instances
        .iter()
        .filter(|i| i.backdrop_blur > 0.5)
        .collect();
    assert_eq!(
        blur_insts.len(),
        1,
        "only the 75% mask instance samples backdrop; got {:?}",
        blur_insts
            .iter()
            .map(|i| (i.backdrop_blur, i.rect_size, i.radii))
            .collect::<Vec<_>>()
    );
    let inst = blur_insts[0];
    assert!(
        (inst.backdrop_blur - 12.0).abs() < 0.01,
        "backdrop_blur={}",
        inst.backdrop_blur
    );
    assert!(
        (inst.rect_size[0] - 165.0).abs() < 2.0 && (inst.rect_size[1] - 105.0).abs() < 2.0,
        "mask size {:?}",
        inst.rect_size
    );
    assert!(inst.radii[0] > 5.0, "mask radii={:?}", inst.radii);
}

#[test]
fn local_backdrop_blur_splits_from_host_text_inside_nested_stack() {
    // Gallery「浮层 / 反馈」sits under a page stacking context (sidebar / ScrollArea).
    // Nested z-index must still split so the scrim band is *after* host glyphs —
    // otherwise capture prefix is empty and the SDF sample covers the labels.
    let mut page = Element::new().w(400.0).h(300.0);
    page.set_attr("data-stacking-context", "1");
    page.style.z_index = 0;

    let labels = Element::new()
        .absolute()
        .inset(0.0)
        .z_index(0)
        .text("YELLOW 高对比 BLUR");
    let scrim = Element::new()
        .w(165.0)
        .h(105.0)
        .rounded(10.0)
        .bg([1.0, 1.0, 1.0, 0.55])
        .backdrop_blur(12.0)
        .template_id("local-scrim");
    let mut overlay = Element::new()
        .w("100%")
        .h("100%")
        .absolute()
        .inset(0.0)
        .z_index(2)
        .child(scrim);
    overlay.set_attr("jadix_portal", "1");
    overlay.set_attr(ATTR_PORTAL_MOUNT, "card-host");
    overlay.set_attr(ATTR_PORTAL_KEY, "local-scrim");
    let host = Element::new()
        .relative()
        .w(220.0)
        .h(140.0)
        .overflow_x("hidden")
        .overflow_y("hidden")
        .template_id("card-host")
        .children([labels, overlay]);
    let mut root = Element::new().child(page.child(host));
    assert!(
        hoist_portal_subtrees(&mut root) >= 1,
        "local mount portal must lift onto the host"
    );

    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 400.0, 300.0, [-1.0, -1.0]);
    assert!(
        result.text_nodes.iter().any(|t| t.text.contains("YELLOW")),
        "host contrast labels must still be collected"
    );
    let bands = result.overlay_split.page_stack_band_ranges();
    let blur_at = result
        .instances
        .iter()
        .position(|i| i.backdrop_blur > 0.5)
        .expect("scrim instance must stamp backdrop_blur");
    let text_at = result
        .text_nodes
        .iter()
        .position(|t| t.text.contains("YELLOW"))
        .unwrap();
    let blur_band = bands
        .iter()
        .position(|b| b.instance.0 <= blur_at && blur_at < b.instance.1)
        .expect("blur instance in a page_stack band");
    let text_band = bands
        .iter()
        .position(|b| b.text.0 <= text_at && text_at < b.text.1)
        .expect("label in a page_stack band");
    assert!(
            text_band < blur_band,
            "labels must paint in an earlier band than the scrim so capture includes them; text_band={text_band} blur_band={blur_band} bands={}",
            bands.len()
        );
    let blur = &result.instances[blur_at];
    assert!(
            blur.clip_size[0] > 0.5 && blur.clip_size[1] > 0.5,
            "local scrim must inherit host overflow clip so backdrop capture can scissor to the host; clip={:?}",
            (blur.clip_pos, blur.clip_size)
        );
}

#[test]
fn local_mount_scrim_follows_scroll_with_stale_position_fixed() {
    // Keyed overlay adopt can copy `position_fixed` onto a local mount. Scroll
    // must still subtract ancestor `scroll_acc` or the 75% mask stays put while
    // the host moves — off-host glyphs then occupy the mask for a frame.
    let labels = Element::new()
        .absolute()
        .inset(0.0)
        .z_index(0)
        .text("YELLOW 高对比 BLUR");
    let scrim = Element::new()
        .w(165.0)
        .h(105.0)
        .rounded(10.0)
        .bg([1.0, 1.0, 1.0, 0.55])
        .backdrop_blur(12.0)
        .template_id("local-scrim");
    let mut overlay = Element::new()
        .w("100%")
        .h("100%")
        .absolute()
        .inset(0.0)
        .z_index(2)
        .child(scrim);
    overlay.set_attr("jadix_portal", "1");
    overlay.set_attr(ATTR_PORTAL_MOUNT, "card-host");
    overlay.set_attr(ATTR_PORTAL_KEY, "local-scrim");
    let host = Element::new()
        .relative()
        .w(220.0)
        .h(140.0)
        .overflow_x("hidden")
        .overflow_y("hidden")
        .template_id("card-host")
        .children([labels, overlay]);
    let spacer = Element::new().w(220.0).h(80.0).flex_shrink(0.0);
    let mut viewport = Element::new()
        .w(240.0)
        .h(200.0)
        .overflow_y("scroll")
        .flex_col()
        .children([spacer, host]);
    assert!(hoist_portal_subtrees(&mut viewport) >= 1);

    let mut tree = LayoutTree::new();
    let before = tree.build_and_layout(&mut viewport, 240.0, 200.0, [-1.0, -1.0]);
    let y0 = before
        .instances
        .iter()
        .find(|i| i.backdrop_blur > 0.5)
        .map(|i| i.rect_pos[1])
        .expect("scrim");

    crate::core::tree::walk_elements_mut(&mut viewport, |el| {
        if el.attrs.get(ATTR_HOISTED_MOUNT).is_some() {
            el.style.position_fixed = true;
        }
    });
    viewport.scroll_offset = [0.0, 40.0];
    let after = tree.build_and_layout_fast(&mut viewport, 240.0, 200.0, [-1.0, -1.0], true, false);
    let y1 = after
        .instances
        .iter()
        .find(|i| i.backdrop_blur > 0.5)
        .map(|i| i.rect_pos[1])
        .expect("scrim after scroll");
    assert!(
            (y0 - y1 - 40.0).abs() < 1.5,
            "local mount scrim must follow host scroll even with leftover position_fixed; y0={y0} y1={y1}"
        );
}

#[test]
fn local_backdrop_blur_keeps_label_band_before_scrim_on_scroll() {
    let mut page = Element::new().w(400.0).h(800.0).flex_col();
    page.set_attr("data-stacking-context", "1");
    page.style.z_index = 0;
    page = page.child(
        Element::new()
            .w(400.0)
            .h(120.0)
            .flex_shrink(0.0)
            .text("OUTSIDE ABOVE"),
    );
    let labels = Element::new()
        .absolute()
        .inset(0.0)
        .z_index(0)
        .text("YELLOW 高对比 BLUR");
    let scrim = Element::new()
        .w(165.0)
        .h(105.0)
        .rounded(10.0)
        .bg([1.0, 1.0, 1.0, 0.55])
        .backdrop_blur(12.0)
        .template_id("local-scrim");
    let mut overlay = Element::new()
        .w("100%")
        .h("100%")
        .absolute()
        .inset(0.0)
        .z_index(2)
        .child(scrim);
    overlay.set_attr("jadix_portal", "1");
    overlay.set_attr(ATTR_PORTAL_MOUNT, "card-host");
    overlay.set_attr(ATTR_PORTAL_KEY, "local-scrim");
    let host = Element::new()
        .relative()
        .w(220.0)
        .h(140.0)
        .overflow_x("hidden")
        .overflow_y("hidden")
        .flex_shrink(0.0)
        .template_id("card-host")
        .children([labels, overlay]);
    page = page.child(host).child(
        Element::new()
            .w(400.0)
            .h(120.0)
            .flex_shrink(0.0)
            .text("OUTSIDE BELOW"),
    );
    let mut viewport = Element::new()
        .w(400.0)
        .h(200.0)
        .overflow_y("scroll")
        .child(page);
    assert!(hoist_portal_subtrees(&mut viewport) >= 1);

    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut viewport, 400.0, 200.0, [-1.0, -1.0]);
    for sy in [0.0_f32, 40.0, 80.0, 120.0] {
        viewport.scroll_offset = [0.0, sy];
        let result =
            tree.build_and_layout_fast(&mut viewport, 400.0, 200.0, [-1.0, -1.0], true, false);
        let bands = result.overlay_split.page_stack_band_ranges();
        let Some(blur_at) = result.instances.iter().position(|i| i.backdrop_blur > 0.5) else {
            continue;
        };
        let Some(text_at) = result
            .text_nodes
            .iter()
            .position(|t| t.text.contains("YELLOW"))
        else {
            continue;
        };
        let blur_band = bands
            .iter()
            .position(|b| b.instance.0 <= blur_at && blur_at < b.instance.1)
            .expect("blur instance in a page_stack band");
        let text_band = bands
            .iter()
            .position(|b| b.text.0 <= text_at && text_at < b.text.1)
            .expect("label in a page_stack band");
        assert!(
                text_band < blur_band,
                "scroll_only sy={sy}: labels must stay in an earlier band than the scrim; text_band={text_band} blur_band={blur_band}"
            );
        let blur = &result.instances[blur_at];
        for node in &result.text_nodes {
            if !node.text.contains("OUTSIDE") {
                continue;
            }
            let overlap_x = node.x < blur.rect_pos[0] + blur.rect_size[0]
                && node.x + node.width > blur.rect_pos[0];
            let overlap_y = node.y < blur.rect_pos[1] + blur.rect_size[1]
                && node.y + node.height > blur.rect_pos[1];
            assert!(
                !(overlap_x && overlap_y),
                "scroll_only sy={sy}: outside text {:?} must not overlap the scrim {:?} {:?}",
                (node.text.as_str(), node.x, node.y),
                blur.rect_pos,
                blur.rect_size
            );
        }
    }
}
