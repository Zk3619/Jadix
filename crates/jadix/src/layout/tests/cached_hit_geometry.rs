use super::*;

#[test]
#[cfg(all(debug_assertions, feature = "devtools"))]
fn scaled_local_floating_panel_is_placed_before_first_collect() {
    crate::core::reactivity::create_root(|| {
        use crate::utils::popup::{mark_floating, mark_portal_mount, PopupPlacement};
        let trigger = Element::new()
            .absolute()
            .left(30.0)
            .top(260.0)
            .w(80.0)
            .h(32.0)
            .focus_id("local-expand-trigger");
        let mut panel = Element::new()
            .absolute()
            .w(230.0)
            .h(50.0)
            .focus_id("local-expand-panel");
        mark_floating(
            &mut panel,
            "local-expand",
            "local-expand-trigger",
            PopupPlacement::BottomStart,
        );
        panel.set_attr("jadix_floating_match_trigger", "0");
        panel.set_attr("jadix_floating_w", "230px");
        panel.style.position_fixed = false;
        let panel = mark_portal_mount(panel, crate::app::devtools::DEVICE_PORTAL_HOST);
        let mut root = Element::new()
            .w(800.0)
            .h(600.0)
            .child(
                Element::new()
                    .absolute()
                    .left(80.0)
                    .top(60.0)
                    .w(390.0)
                    .h(844.0)
                    .scale(0.57)
                    .transform_origin("left top")
                    .child(trigger)
                    .child(panel)
                    .child(
                        Element::new()
                            .absolute()
                            .w("100%")
                            .h("100%")
                            .template_id(crate::app::devtools::DEVICE_PORTAL_HOST),
                    ),
            )
            .child(
                Element::new()
                    .absolute()
                    .w("100%")
                    .h("100%")
                    .template_id(PORTAL_LAYER_ID),
            );
        let mut tree = LayoutTree::new();
        for (frame, scale) in [0.57, 0.8, 0.5].into_iter().enumerate() {
            root.children[0].style.scale = [scale; 2];
            if frame == 0 {
                tree.build_and_layout(&mut root, 800.0, 600.0, [-1.0; 2]);
            } else {
                tree.build_and_layout_fast(&mut root, 800.0, 600.0, [-1.0; 2], false, false);
            }
            let trigger = crate::core::tree::find_in_tree(&root, |node| {
                node.focus_id.as_deref() == Some("local-expand-trigger")
            })
            .unwrap();
            let trigger_bounds =
                crate::core::tree::visual_paint_aabb(trigger, trigger.paint_scroll_acc);
            let panel = crate::core::tree::find_in_tree(&root, |node| {
                node.focus_id.as_deref() == Some("local-expand-panel")
            })
            .unwrap();
            let panel_bounds = crate::core::tree::visual_paint_aabb(panel, panel.paint_scroll_acc);
            let expected_y = trigger_bounds[1]
                + trigger_bounds[3]
                + crate::utils::popup::FLOATING_TRIGGER_GAP * scale;
            assert!((panel_bounds[0] - trigger_bounds[0]).abs() < 1.0);
            assert!(
                    (panel_bounds[1] - expected_y).abs() < 1.0,
                    "local mount must place before collecting: {panel_bounds:?} vs {trigger_bounds:?} at scale {scale}"
                );
        }
    });
}

#[test]
fn scaled_floating_prestamp_matches_collected_trigger_geometry() {
    crate::core::reactivity::create_root(|| {
        let trigger = crate::core::tree::Element::new()
            .focus_id("prestamp-trigger")
            .w(80.0)
            .h(28.0);
        let mut root = crate::core::tree::Element::new().w(800.0).h(600.0).child(
            crate::core::tree::Element::new()
                .absolute()
                .left(60.0)
                .top(40.0)
                .w(390.0)
                .h(844.0)
                .scale(0.57)
                .transform_origin("left top")
                .child(
                    crate::core::tree::Element::new()
                        .w(390.0)
                        .h(700.0)
                        .overflow_y("scroll")
                        .child(
                            crate::core::tree::Element::new()
                                .absolute()
                                .left(24.0)
                                .top(900.0)
                                .w(300.0)
                                .h(60.0)
                                .child(trigger),
                        ),
                ),
        );
        root.children[0].children[0].scroll_offset = [0.0, 650.0];
        let mut layout = super::LayoutTree::new();
        layout.build_and_layout(&mut root, 800.0, 600.0, [-1.0; 2]);
        let trigger = &root.children[0].children[0].children[0].children[0];
        let expected = crate::core::tree::visual_paint_aabb(trigger, trigger.paint_scroll_acc);
        layout.stamp_tree_layout_bounds_from_taffy(&mut root);
        let trigger = &root.children[0].children[0].children[0].children[0];
        let actual = crate::core::tree::visual_paint_aabb(trigger, trigger.paint_scroll_acc);
        for axis in 0..4 {
            assert!(
                (actual[axis] - expected[axis]).abs() < 0.01,
                "floating prepass must use collected paint geometry: {actual:?} vs {expected:?}"
            );
        }
    });
}
#[test]
fn cached_scaled_subtree_keeps_collected_hit_and_damage_geometry() {
    let mut root = Element::new().w(800.0).h(700.0).child(
        Element::new()
            .absolute()
            .left(80.0)
            .top(90.0)
            .w(390.0)
            .h(844.0)
            .scale(0.57)
            .transform_origin("left top")
            .child(
                Element::new().w(320.0).h(280.0).child(
                    Element::new()
                        .absolute()
                        .left(112.0)
                        .top(180.0)
                        .w(102.0)
                        .h(36.0)
                        .bg([0.8, 0.5, 0.9, 1.0])
                        .child(Element::new().text("Destructive")),
                ),
            ),
    );
    let mut tree = LayoutTree::new();
    tree.build_and_layout(&mut root, 800.0, 700.0, [-1.0; 2]);
    let mut expected = Vec::new();
    crate::core::tree::walk_elements(&root.children[0], |element| {
        expected.push((
            element.layout_bounds,
            element.paint_ancestor_scale,
            crate::core::tree::visual_paint_aabb(element, element.paint_scroll_acc),
        ));
    });
    let child = &mut root.children[0];
    let [left, top, width, height] = child.layout_bounds;
    tree.stamp_skipped_sibling_hit_bounds(
        child,
        child.layout_node.unwrap(),
        [left, top],
        [width, height],
        false,
        false,
        &mut None,
    );
    let mut actual = Vec::new();
    crate::core::tree::walk_elements(child, |element| {
        actual.push((
            element.layout_bounds,
            element.paint_ancestor_scale,
            crate::core::tree::visual_paint_aabb(element, element.paint_scroll_acc),
        ));
    });
    assert_eq!(
        actual, expected,
        "cached pixels must retain the same hit/damage coordinates"
    );
}

#[test]
fn scaled_preview_peer_occupancy_keeps_the_entire_hover_repair() {
    let mut root = Element::new().w(800.0).h(700.0).child(
        Element::new()
            .absolute()
            .left(80.0)
            .top(90.0)
            .w(390.0)
            .h(844.0)
            .scale(0.57)
            .transform_origin("left top")
            .child(
                Element::new()
                    .w(320.0)
                    .h(280.0)
                    .flex_row()
                    .gap(8.0)
                    .child(
                        Element::new()
                            .w(102.0)
                            .h(36.0)
                            .bg([0.8, 0.5, 0.9, 1.0])
                            .child(Element::new().text("Destructive")),
                    )
                    .child(
                        Element::new()
                            .w(90.0)
                            .h(36.0)
                            .bg([0.5, 0.9, 0.5, 1.0])
                            .child(Element::new().text("Success")),
                    ),
            ),
    );
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 800.0, 700.0, [-1.0; 2]);
    let buttons = &root.children[0].children[0].children;
    for button in buttons {
        let key = u64::from(button.layout_node.unwrap());
        let paint = result
            .node_paints
            .iter()
            .find(|paint| paint.key == key)
            .unwrap();
        assert_eq!(
            paint.rest,
            Some(paint.bounds),
            "rest occupancy must retain ancestor zoom"
        );
        let scissor = crate::app::draw_cull::node_present_scissor_logical(
            crate::app::damage::PaintDirtyKind::Node,
            key,
            paint.bounds,
            &[],
            &result.node_paints,
            2.5,
        );
        assert!(
            crate::app::damage::rect_contains_rect(scissor, paint.bounds),
            "peer occupancy cropped a scaled button: {:?} -> {scissor:?}",
            paint.bounds
        );
    }
    let button = &mut root.children[0].children[0].children[0];
    let rest = crate::core::tree::rest_paint_aabb(button, button.paint_scroll_acc);
    button.style.scale = [1.1, 1.1];
    assert_eq!(
        crate::core::tree::rest_paint_aabb(button, button.paint_scroll_acc),
        rest,
        "local hover growth must not enlarge sibling occupancy"
    );
}

#[test]
fn scroll_only_hit_geometry_matches_full_restamp() {
    fn run(
        scroll_only: bool,
        shift: f32,
        pinned: bool,
    ) -> Vec<([f32; 4], [f32; 2], crate::style::UsedLayout)> {
        let mut leaf = Element::new().w(30.0).h(20.0);
        leaf.style.position_fixed = pinned;
        let mut root = Element::new().w(200.0).h(100.0).child(
            Element::new()
                .w(150.0)
                .h(70.0)
                .child(Element::new().w(80.0).h(50.0).child(leaf)),
        );
        let mut tree = LayoutTree::new();
        tree.build_and_layout(&mut root, 200.0, 100.0, [-1.0; 2]);
        let child = &mut root.children[0];
        assert!(child.collect_paint_hash != 0 && child.collect_extent_valid);
        let node = child.layout_node.unwrap();
        let [x, y, w, h] = child.layout_bounds;
        child.paint_scroll_acc = [0.0, 73.0];
        tree.stamp_skipped_sibling_hit_bounds(
            child,
            node,
            [x + shift, y],
            [w, h],
            false,
            scroll_only,
            &mut None,
        );
        let mut snapshot = Vec::new();
        crate::core::tree::walk_elements(child, |el| {
            snapshot.push((el.layout_bounds, el.paint_scroll_acc, el.used_layout));
        });
        snapshot
    }
    for shift in [0.0, 13.0] {
        for pinned in [false, true] {
            assert_eq!(
                run(true, shift, pinned),
                run(false, shift, pinned),
                "scroll stamps and used geometry must match: shift={shift} pinned={pinned}"
            );
        }
    }
}
#[test]
fn stamped_portal_queries_keep_visible_and_measuring_portals() {
    let mut visible = Element::new().bg([1.0; 4]);
    visible.set_attr(ATTR_HOISTED_PORTAL, "1");
    visible.layout_bounds = [90.0, 140.0, 40.0, 200.0];
    let mut measuring = Element::new().bg([1.0; 4]);
    measuring.set_attr(ATTR_HOISTED_PORTAL, "1");
    measuring.layout_bounds = visible.layout_bounds;
    measuring.set_attr("jadix_floating_expand_measure", "1");
    let mut layer = Element::new().child(visible).child(measuring);
    layer.template_id = Some(PORTAL_LAYER_ID.to_string());
    let mut root = Element::new()
        .child(Element::new().child(Element::new()))
        .child(layer);
    let stamps = hoisted_portal_stamps(&root);
    assert_eq!(stamps.len(), 1);
    assert!(hoisted_expand_measuring(&root));
    crate::layout::collect_cache::stamp_subtree_paint_hashes(&mut root, false);
    assert_eq!(hoisted_portal_stamps(&root), stamps);
    assert!(hoisted_expand_measuring(&root));
    root.children[1].children.pop();
    crate::layout::collect_cache::stamp_subtree_paint_hashes(&mut root, false);
    assert_eq!(hoisted_portal_stamps(&root), stamps);
    assert!(!hoisted_expand_measuring(&root));
    root.children[1].children.clear();
    crate::layout::collect_cache::stamp_subtree_paint_hashes(&mut root, false);
    assert!(hoisted_portal_stamps(&root).is_empty());
}

#[test]
fn culled_ancestor_queries_match_expanded_ids_and_ignore_removed_nodes() {
    let mut root = Element::new().w(200.0).h(100.0).flex_col();
    root.style.overflow.y = taffy::style::Overflow::Hidden;
    for _ in 0..20 {
        root.children.push(
            Element::new()
                .w(200.0)
                .h(20.0)
                .flex_shrink(0.0)
                .child(Element::new().w(100.0).h(15.0).bg([1.0; 4])),
        );
    }
    let mut tree = LayoutTree::new();
    tree.build_and_layout(&mut root, 200.0, 100.0, [-1.0; 2]);
    let mut previous = Vec::new();
    crate::core::tree::walk_elements(&root, |el| {
        if let Some(node) = el.layout_node {
            previous.push((u64::from(node), el.layout_bounds));
        }
    });
    for offset in [0.0, 40.0, 200.0, 0.0] {
        root.scroll_offset[1] = offset;
        tree.build_and_layout_fast(&mut root, 200.0, 100.0, [-1.0; 2], false, true);
        let queried = tree.clip_culled_previous_paint_keys(&previous);
        assert!(
            tree.collect_skip.clip_culled_expanded.get().is_none(),
            "normal repair must not expand hidden descendants"
        );
        let all = tree.last_clip_culled_paint_keys();
        assert!(!all.is_empty());
        assert_eq!(&queried, all);
    }
    root.children.pop();
    tree.build_and_layout(&mut root, 200.0, 100.0, [-1.0; 2]);
    let queried = tree.clip_culled_previous_paint_keys(&previous);
    assert_eq!(
        queried,
        previous
            .iter()
            .filter(|(key, _)| tree.last_clip_culled_paint_keys().contains(key))
            .map(|(key, _)| *key)
            .collect()
    );
}

#[test]
fn cached_offscreen_tail_keeps_hit_geometry_and_paint_identical() {
    fn run(
        cached: bool,
        intrinsic: bool,
    ) -> (
        Vec<(Vec<([f32; 4], [f32; 2], crate::style::UsedLayout)>, Vec<u8>)>,
        usize,
        usize,
    ) {
        let mut content = Element::new()
            .w(200.0)
            .h(800.0)
            .min_h(800.0)
            .flex_shrink(0.0)
            .flex_col();
        for _ in 0..40 {
            content.children.push(
                Element::new().w(200.0).h(20.0).flex_shrink(0.0).child(
                    Element::new()
                        .w(150.0)
                        .h(18.0)
                        .child(Element::new().w(100.0).h(16.0).bg([1.0; 4])),
                ),
            );
        }
        if intrinsic {
            content = content.h("max-content");
        }
        let mut root = Element::new().w(200.0).h(100.0).flex_col().child(content);
        root.style.overflow.y = taffy::style::Overflow::Scroll;
        let mut tree = LayoutTree::new();
        tree.build_and_layout(&mut root, 200.0, 100.0, [-1.0; 2]);
        tree.set_collect_skip_allowed(cached);
        let mut frames = Vec::new();
        for offset in [0.0, 0.0, 40.0, 80.0, 40.0, 0.0] {
            root.scroll_offset[1] = offset;
            let result =
                tree.build_and_layout_fast(&mut root, 200.0, 100.0, [-1.0; 2], true, false);
            let mut geometry = Vec::new();
            crate::core::tree::walk_elements(&root, |el| {
                geometry.push((el.layout_bounds, el.paint_scroll_acc, el.used_layout))
            });
            frames.push((geometry, bytemuck::cast_slice(&result.instances).to_vec()));
        }
        (
            frames,
            tree.reused_culled_tails.get(),
            tree.reused_clip_extents.get(),
        )
    }
    let (cached, hits, _) = run(true, false);
    let (reference, _, _) = run(false, false);
    if cfg!(all(debug_assertions, feature = "devtools")) {
        assert_eq!(
            hits, 0,
            "devtools must refresh its separate geometry records"
        );
    } else {
        assert!(hits > 0, "fixture must exercise cached tail geometry");
    }
    assert_eq!(cached, reference);
    let (cached, _, hits) = run(true, true);
    let (reference, _, _) = run(false, true);
    if cfg!(all(debug_assertions, feature = "devtools")) {
        assert_eq!(hits, 0);
    } else {
        assert!(
            hits > 0,
            "intrinsic fixture must exercise cached clip overflow"
        );
    }
    assert_eq!(cached, reference);
}
