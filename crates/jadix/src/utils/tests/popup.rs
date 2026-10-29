use super::*;

#[test]
fn scaled_anchor_follows_nested_scroll_in_paint_space() {
    let mut target = Element::new().focus_id("scaled-anchor");
    target.layout_bounds = [120.0, 180.0, 80.0, 24.0];
    target.paint_ancestor_scale = [0.5; 2];
    let mut inner = Element::new().overflow_y("scroll").child(target);
    inner.paint_ancestor_scale = [0.5; 2];
    inner.scroll_offset = [0.0, 40.0];
    let mut outer = Element::new().scale(0.5).overflow_y("scroll").child(inner);
    outer.scroll_offset = [0.0, 100.0];
    let mut triggers = Vec::new();
    collect_focus_bounds(&outer, [0.0; 2], &mut triggers);
    let expected =
        crate::core::tree::visual_paint_aabb(&outer.children[0].children[0], [0.0, 70.0]);
    assert_eq!(
        floating_trigger_bounds(&triggers, "scaled-anchor"),
        Some(expected)
    );
}

#[cfg(feature = "product-tests")]
fn menu_panel(id: &str, cursor: [f32; 2]) -> Element {
    let rows: Element = crate::primitives::vstack()
        .w("100%")
        .children((0..10).map(|i| {
            Element::new()
                .w(192.0)
                .h(40.0)
                .flex_shrink(0.0)
                .focus_id(format!("{id}-row-{i}"))
        }))
        .into();
    let viewport: Element = crate::components::scroll_area(&format!("{id}-scroll"))
        .w("100%")
        .h("auto")
        .min_h(0.0)
        .flex_grow(1.0)
        .flex_shrink(1.0)
        .child(rows)
        .into();
    let mut panel: Element = crate::primitives::vstack()
        .absolute(true)
        .w("auto")
        .h("auto")
        .min_w(192.0)
        .min_h(0.0)
        .child(viewport)
        .into();
    mark_floating_at_point(&mut panel, id, "", cursor[0], cursor[1], 2.0);
    panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "0");
    apply_stored_floating_place(&mut panel, id);
    mount_anchored_overlay(panel, 1000)
}

#[test]
#[cfg(feature = "product-tests")]
fn cursor_menu_first_frame_and_reopen_keep_full_height_above() {
    crate::core::reactivity::create_root(|| {
        let mut tree = crate::layout::layout_tree::LayoutTree::new();
        for (cursor_y, viewport_h) in [
            (500.0, 600.0),
            (1250.0, 1500.0),
            (1250.0, 1500.0),
            (120.0, 600.0),
            (500.0, 600.0),
            (200.0, 240.0),
            (500.0, 600.0),
            (200.0, 600.0),
            (500.0, 600.0),
        ] {
            let mut root = Element::new().w(800.0).h(600.0);
            tree.build_and_layout(&mut root, 800.0, viewport_h, [0.0, 0.0]);
            root.children
                .push(menu_panel("cursor-reopen", [300.0, cursor_y]));
            tree.build_and_layout_fast_ex(
                &mut root,
                800.0,
                viewport_h,
                [0.0, 0.0],
                true,
                false,
                None,
            );
            visit_floating_panels(&root, |panel| {
                let b = panel.layout_bounds;
                let (_, expected_y, _) = compute_cursor_menu_placement(
                    [300.0, cursor_y],
                    [800.0, viewport_h],
                    b[2],
                    400.0,
                    2.0,
                );
                assert!(
                    (b[3] - 400.0_f32.min(viewport_h - 16.0)).abs() < 0.5,
                    "first frame must retain intrinsic menu height at y={cursor_y}: {b:?}"
                );
                assert!((b[1] - expected_y).abs() < 0.5,
                        "first frame must flip before translating at y={cursor_y}: expected {expected_y}, got {b:?}");
            });
        }
    });
}

#[test]
fn submenu_viewport_shift_does_not_become_vertical_flip() {
    crate::core::reactivity::create_root(|| {
        let trigger = Element::new()
            .absolute()
            .left(200.0)
            .top(480.0)
            .w(192.0)
            .h(32.0)
            .focus_id("submenu-shift-trigger");
        let mut panel = Element::new().absolute().w(192.0).h(200.0);
        mark_floating_with_gap(
            &mut panel,
            "submenu-shift",
            "submenu-shift-trigger",
            PopupPlacement::RightStart,
            2.0,
        );
        panel.set_attr(ATTR_FLOATING_W, "192px");
        apply_stored_floating_place(&mut panel, "submenu-shift");
        let mut root = Element::new()
            .w(800.0)
            .h(600.0)
            .child(trigger)
            .child(mount_anchored_overlay(panel, 1000));
        let mut tree = crate::layout::layout_tree::LayoutTree::new();
        tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);
        visit_floating_panels(&root, |panel| {
            assert!(
                (panel.layout_bounds[1] - 392.0).abs() < 0.5,
                "submenu must shift to bottom viewport margin: {:?}",
                panel.layout_bounds
            );
            assert!(
                !panel.attrs.contains_key(ATTR_FLOATING_ANCHOR_EDGE_Y),
                "horizontal submenu must not acquire an above-trigger edge"
            );
        });
        let panel = &mut root.children.last_mut().unwrap().children[0];
        assert_eq!(
            panel.attrs.get(ATTR_FLOATING).map(String::as_str),
            Some("submenu-shift")
        );
        panel.set_attr(ATTR_FLOATING_EXITING, "1");
        sync_floating_placement(&mut root, 800.0, 600.0);
        visit_floating_panels(&root, |panel| {
            assert_eq!(
                inset_length(panel.style.inset.top),
                Some(392.0),
                "submenu exit must retain horizontal placement"
            );
        });
    });
}

#[test]
#[cfg(feature = "product-tests")]
fn nested_submenu_first_frame_keeps_parent_height_and_live_trigger() {
    crate::core::reactivity::create_root(|| {
        let mut root = Element::new()
            .w(800.0)
            .h(600.0)
            .child(menu_panel("nested-parent", [300.0, 500.0]));
        let mut tree = crate::layout::layout_tree::LayoutTree::new();
        tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);
        fn find_mut<'a>(el: &'a mut Element, id: &str) -> Option<&'a mut Element> {
            if el.attrs.get(ATTR_FLOATING).map(String::as_str) == Some(id) {
                return Some(el);
            }
            el.children.iter_mut().find_map(|c| find_mut(c, id))
        }
        let parent = find_mut(&mut root, "nested-parent").unwrap();
        let before = parent.layout_bounds;
        // The same frame can also move the parent (e.g. a new context click).
        parent.set_attr(ATTR_FLOATING_POINT, "100,560");
        let mut sub = Element::new().absolute().w(192.0).h(200.0);
        mark_floating_with_gap(
            &mut sub,
            "nested-sub",
            "nested-parent-row-8",
            PopupPlacement::RightStart,
            2.0,
        );
        sub.set_attr(ATTR_FLOATING_W, "192px");
        apply_stored_floating_place(&mut sub, "nested-sub");
        parent.children.push(mount_anchored_overlay(sub, 1001));
        tree.build_and_layout_fast_ex(&mut root, 800.0, 600.0, [0.0, 0.0], true, false, None);
        let parent = find_mut(&mut root, "nested-parent").unwrap();
        assert!(
            (parent.layout_bounds[3] - before[3]).abs() < 0.5,
            "opening a submenu must not stretch its parent: {:?}",
            parent.layout_bounds
        );
        let parent_bounds = parent.layout_bounds;
        let sub = find_mut(&mut root, "nested-sub").unwrap();
        assert!(
            (sub.layout_bounds[0] - (parent_bounds[0] + parent_bounds[2] + 2.0)).abs() < 0.5,
            "submenu must use the live parent trigger on its first frame: {:?}",
            sub.layout_bounds
        );
        assert!(
            (sub.layout_bounds[1] - 392.0).abs() < 0.5,
            "submenu must shift into the viewport on its first frame: {:?}",
            sub.layout_bounds
        );
    });
}

#[test]
fn visit_floating_panels_finds_marked() {
    let mut panel = Element::new();
    mark_floating(&mut panel, "m1", "t1", PopupPlacement::BottomStart);
    let nested = Element::new().child(panel);
    let root = Element::new().child(Element::new()).child(nested);
    let mut n = 0usize;
    visit_floating_panels(&root, |_| n += 1);
    assert_eq!(n, 1);
}

#[test]
fn floating_panel_summary_tracks_mount_and_removal() {
    use crate::layout::collect_cache::{
        invalidate_collect_paint_hash_path, stamp_subtree_paint_hashes,
    };
    let mut root = Element::new().child(Element::new()).child(Element::new());
    stamp_subtree_paint_hashes(&mut root, false);
    assert!(!root.subtree_floating_panel);
    let mut panel = Element::new();
    panel.set_attr("jadix_select_menu", "compat");
    root.children[1].children.push(panel);
    invalidate_collect_paint_hash_path(&mut root, &[1]);
    for stamped in [false, true] {
        if stamped {
            stamp_subtree_paint_hashes(&mut root, true);
            assert!(root.subtree_floating_panel);
            assert!(!root.children[0].subtree_floating_panel);
        }
        let mut found = 0;
        visit_floating_panels(&root, |_| found += 1);
        assert_eq!(found, 1);
    }
    root.children[1].children.clear();
    invalidate_collect_paint_hash_path(&mut root, &[1]);
    stamp_subtree_paint_hashes(&mut root, true);
    assert!(!root.subtree_floating_panel);
    visit_floating_panels(&root, |_| panic!("removed panel"));
}

#[test]
fn identity_floating_skips_xy_even_with_trigger() {
    let mut btn = Element::new().w(40.0).h(20.0);
    btn = btn.focus_id("hud-btn");
    btn.layout_bounds = [8.0, 8.0, 40.0, 20.0];
    let mut panel = Element::new()
        .fixed()
        .top("1rem")
        .right("1rem")
        .w(80.0)
        .h(40.0);
    panel.set_attr(ATTR_FLOATING, "hud");
    panel.set_attr(ATTR_FLOATING_TRIGGER, "hud-btn");
    let left_before = panel.style.inset.left;
    let mut root = Element::new().child(btn).child(panel);
    assert!(
        !sync_floating_placement(&mut root, 800.0, 600.0),
        "identity-only ATTR_FLOATING must not rewrite author inset"
    );
    assert_eq!(root.children[1].style.inset.left, left_before);
}

#[test]
fn write_floating_place_pairs_max_h_with_overflow_scroll() {
    let mut el = Element::new().absolute().w(160.0);
    assert_eq!(el.style.overflow.y, taffy::style::Overflow::Visible);
    let place = FloatingPlace {
        x: 10.0,
        y: 20.0,
        w: 160.0,
        max_h: 120.0,
        width_auto: false,
        min_w: 0.0,
        anchor_edge_y: None,
    };
    assert!(write_floating_place(&mut el, place));
    assert_eq!(el.style.overflow.y, taffy::style::Overflow::Hidden);
    match crate::layout::dim_kind(el.style.max_size.height) {
        crate::layout::DimKind::Length(h) => assert!((h - 120.0).abs() < 0.1),
        other => panic!("expected max height length, got {other:?}"),
    }
    // Capability layer: no default min-height unless ATTR_FLOATING_MIN_H is set.
    assert!(
        dimension_length(el.style.min_size.height).unwrap_or(0.0) == 0.0,
        "popup must not invent a style min-height"
    );
}

#[test]
fn write_floating_place_skips_max_h_during_accordion_animation() {
    use crate::style::{keyframes_anim_entry, AnimationFillMode, TimingFunction};
    use taffy::style::Dimension;

    let mut el = Element::new().absolute().w(160.0);
    el.style.max_size.height = Dimension::length(24.0);
    el.style.animations = vec![keyframes_anim_entry(
        "accordion-down",
        200.0,
        TimingFunction::EaseOut,
        AnimationFillMode::Both,
    )];
    let place = FloatingPlace {
        x: 10.0,
        y: 20.0,
        w: 160.0,
        max_h: 400.0,
        width_auto: false,
        min_w: 0.0,
        anchor_edge_y: None,
    };
    let _ = write_floating_place(&mut el, place);
    match crate::layout::dim_kind(el.style.max_size.height) {
        crate::layout::DimKind::Length(h) => assert!(
            (h - 24.0).abs() < 0.1,
            "accordion mid max-height must not be replaced by viewport cap, got {h}"
        ),
        other => panic!("expected preserved Length, got {other:?}"),
    }
}

#[test]
fn write_floating_place_skips_max_h_during_expand_measure() {
    use taffy::style::Dimension;

    let mut el = Element::new().absolute().w(160.0);
    el.style.max_size.height = Dimension::auto();
    el.set_attr(ATTR_FLOATING_EXPAND_MEASURE, "1");
    let place = FloatingPlace {
        x: 10.0,
        y: 20.0,
        w: 160.0,
        max_h: 80.0,
        width_auto: false,
        min_w: 0.0,
        anchor_edge_y: None,
    };
    let _ = write_floating_place(&mut el, place);
    assert!(
        el.style.max_size.height.is_auto(),
        "measure frame must keep unconstrained max-height, not viewport cap"
    );
}

#[test]
fn write_floating_place_skips_max_h_when_expand_from_h() {
    use taffy::style::Dimension;

    let mut el = Element::new().absolute().w(160.0);
    el.style.max_size.height = Dimension::length(28.0);
    el.set_attr(ATTR_FLOATING_EXPAND_FROM_H, "28");
    let place = FloatingPlace {
        x: 10.0,
        y: 20.0,
        w: 160.0,
        max_h: 400.0,
        width_auto: false,
        min_w: 0.0,
        anchor_edge_y: None,
    };
    let _ = write_floating_place(&mut el, place);
    match crate::layout::dim_kind(el.style.max_size.height) {
        crate::layout::DimKind::Length(h) => assert!(
            (h - 28.0).abs() < 0.1,
            "expand from_h panel must not take viewport max_h before MEASURE, got {h}"
        ),
        other => panic!("expected preserved clip height, got {other:?}"),
    }
}

#[test]
fn sync_floating_estimate_h_uses_content_h_not_clip() {
    let mut trigger = Element::new().focus_id("t-clip");
    // Near the bottom: clipped height would stay below; content height must flip above.
    trigger.layout_bounds = [20.0, 500.0, 80.0, 28.0];

    let mut panel = Element::new().absolute();
    mark_floating(
        &mut panel,
        "clip-menu",
        "t-clip",
        PopupPlacement::BottomStart,
    );
    panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "1");
    panel.layout_bounds = [0.0, 0.0, 80.0, 20.0];
    panel.set_attr(ATTR_FLOATING_CONTENT_H, "200");

    let mut root = Element::new().child(trigger).child(panel);
    assert!(sync_floating_placement(&mut root, 800.0, 600.0));
    let menu = &root.children[1];
    match inset_length(menu.style.inset.top) {
        Some(y) => assert!(
            (y - (500.0 - FLOATING_TRIGGER_GAP - 200.0)).abs() < 0.5,
            "top must use content height 200 (y={}), not clip 20, got {y}",
            500.0 - FLOATING_TRIGGER_GAP - 200.0
        ),
        None => panic!("expected top inset"),
    }
}

#[test]
fn sync_floating_top_placed_accordion_tracks_visual_height() {
    use crate::style::{keyframes_anim_entry, AnimationFillMode, TimingFunction};
    use taffy::style::Dimension;

    let mut trigger = Element::new().focus_id("t-up");
    trigger.layout_bounds = [20.0, 500.0, 80.0, 28.0];

    let mut panel = Element::new().absolute();
    mark_floating(&mut panel, "up-menu", "t-up", PopupPlacement::BottomStart);
    panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "1");
    panel.layout_bounds = [0.0, 0.0, 80.0, 20.0];
    panel.set_attr(ATTR_FLOATING_CONTENT_H, "200");
    panel.style.max_size.height = Dimension::length(20.0);
    panel.style.animations = vec![keyframes_anim_entry(
        "accordion-down",
        200.0,
        TimingFunction::EaseOut,
        AnimationFillMode::Both,
    )];

    let mut root = Element::new().child(trigger).child(panel);
    assert!(sync_floating_placement(&mut root, 800.0, 600.0));
    let menu = &root.children[1];
    // Trigger-facing edge = 500 - gap; visual clip 20 → grows from trigger.
    match inset_length(menu.style.inset.top) {
        Some(y) => assert!(
            (y - (500.0 - FLOATING_TRIGGER_GAP - 20.0)).abs() < 0.5,
            "upward accordion must track visual height from trigger, got {y}"
        ),
        None => panic!("expected top inset"),
    }
}

#[test]
fn sync_floating_places_above_using_content_height_not_viewport_remainder() {
    let mut trigger = Element::new().focus_id("t-up-stretch");
    trigger.layout_bounds = [20.0, 500.0, 116.0, 32.0];

    let mut panel = Element::new().absolute();
    mark_floating(
        &mut panel,
        "up-stretch",
        "t-up-stretch",
        PopupPlacement::BottomStart,
    );
    panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "0");
    panel.set_attr(ATTR_FLOATING_W, "144px");
    panel.set_attr(ATTR_FLOATING_MAX_H, "256px");
    // Wrapper stretched to the viewport remainder (ScrollArea h:100% / max_h cap).
    panel.layout_bounds = [20.0, 8.0, 144.0, 500.0];

    let mut root = Element::new().child(trigger).child(panel);
    assert!(sync_floating_placement(&mut root, 800.0, 600.0));
    let top = inset_length(root.children[1].style.inset.top).expect("top");
    // Edge = 500 - gap; painted height min(500, attr max 256) = 256.
    // Using estimate.min(space_above) slams top to the viewport margin.
    assert!(
        (top - (500.0 - FLOATING_TRIGGER_GAP - 256.0)).abs() < 0.5,
        "upward menu must glue to the trigger with painted height, got {top}"
    );
}

#[test]
fn sync_floating_exit_keeps_top_placement_when_clip_shrinks() {
    use crate::style::{keyframes_anim_entry, AnimationFillMode, TimingFunction};
    use taffy::style::Dimension;

    let mut trigger = Element::new().focus_id("t-exit");
    trigger.layout_bounds = [20.0, 500.0, 80.0, 28.0];

    let mut panel = Element::new().absolute();
    mark_floating(
        &mut panel,
        "exit-menu",
        "t-exit",
        PopupPlacement::BottomStart,
    );
    panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "1");
    panel.layout_bounds = [0.0, 0.0, 80.0, 20.0];
    // Rebuild often drops CONTENT_H; clip 20 would flip below (y=536) without a frozen edge.
    panel.set_attr(ATTR_FLOATING_ANCHOR_EDGE_Y, "492");
    panel.set_attr(ATTR_FLOATING_GAP, "8");
    panel.style.max_size.height = Dimension::length(20.0);
    panel.style.animations = vec![keyframes_anim_entry(
        "accordion-up",
        200.0,
        TimingFunction::EaseIn,
        AnimationFillMode::Both,
    )];

    let mut root = Element::new().child(trigger).child(panel);
    assert!(sync_floating_placement(&mut root, 800.0, 600.0));
    let menu = &root.children[1];
    match inset_length(menu.style.inset.top) {
        Some(y) => assert!(
            (y - 472.0).abs() < 0.5,
            "collapsing top menu must not flip below the trigger, got {y}"
        ),
        None => panic!("expected top inset"),
    }
    assert_eq!(
        menu.attrs
            .get(ATTR_FLOATING_ANCHOR_EDGE_Y)
            .map(String::as_str),
        Some("492")
    );
}

/// Idle reverse-expand after enter: `max-h-16rem` / place cap is 256px while the
/// 6-item list is ~192px. Glue must use content, not the cap — otherwise the
/// open menu sits too high (gap between panel bottom and trigger).
#[test]
fn sync_floating_above_idle_must_glue_content_not_place_cap() {
    use taffy::style::Dimension;

    let mut trigger = Element::new().focus_id("t-idle-up");
    trigger.layout_bounds = [20.0, 500.0, 80.0, 28.0];

    let mut panel = Element::new().absolute();
    mark_floating(
        &mut panel,
        "idle-up",
        "t-idle-up",
        PopupPlacement::BottomStart,
    );
    panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "1");
    // Intrinsic measurement can retain the old expanded viewport cap even
    // after the idle ScrollArea has shrunk to its six rows.
    panel.set_attr(ATTR_FLOATING_CONTENT_H, "256");
    panel.set_attr(ATTR_FLOATING_MAX_H, "16rem");
    panel.set_attr(ATTR_FLOATING_ANCHOR_EDGE_Y, "492");
    // Idle Expand writes the place cap onto max-height; that is a cap, not paint.
    panel.set_attr(ATTR_FLOATING_GAP, "8");
    panel.style.max_size.height = Dimension::length(256.0);
    panel.style.inset.top = Element::into_margin_auto(236.0);
    panel.layout_bounds = [20.0, 236.0, 80.0, 192.0];

    let mut root = Element::new().child(trigger).child(panel);
    let _ = sync_floating_placement(&mut root, 800.0, 600.0);
    let y = inset_length(root.children[1].style.inset.top).expect("top");
    assert!(
        (y - 300.0).abs() < 1.0,
        "upward idle menu must sit at edge-content (300), not edge-cap (236); y={y}"
    );
}

/// Exit after idle: used-layout / clip_height can be the 16rem place cap (256)
/// while the panel is still glued at edge-content (top=300). Growing visual_h
/// to 256 slams `top` to 236 (jump up) before accordion-up shrinks it back.
#[test]
fn sync_floating_above_exit_must_not_grow_past_glued_height() {
    use taffy::style::Dimension;

    let mut trigger = Element::new().focus_id("t-exit-grow");
    trigger.layout_bounds = [20.0, 500.0, 80.0, 28.0];

    let mut panel = Element::new().absolute();
    mark_floating(
        &mut panel,
        "exit-grow",
        "t-exit-grow",
        PopupPlacement::BottomStart,
    );
    panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "1");
    panel.set_attr(ATTR_FLOATING_CONTENT_H, "256");
    panel.set_attr(ATTR_FLOATING_MAX_H, "16rem");
    panel.set_attr(ATTR_FLOATING_ANCHOR_EDGE_Y, "492");
    panel.set_attr(ATTR_FLOATING_GAP, "8");
    panel.style.max_size.height = Dimension::length(256.0);
    panel.style.inset.top = Element::into_margin_auto(300.0);
    panel.layout_bounds = [20.0, 300.0, 80.0, 256.0];

    let mut root = Element::new().child(trigger).child(panel);
    let _ = sync_floating_placement(&mut root, 800.0, 600.0);
    let y = inset_length(root.children[1].style.inset.top).expect("top");
    assert!(
        (y - 300.0).abs() < 1.0,
        "exit must not grow past the glued silhouette (300), not jump to edge-cap (236); y={y}"
    );
}

#[test]
fn settled_above_menu_recomputes_edge_after_trigger_moves() {
    use taffy::style::Dimension;

    let mut trigger = Element::new().focus_id("stale-edge-trigger");
    trigger.layout_bounds = [20.0, 320.0, 80.0, 28.0];

    let mut panel = Element::new().absolute();
    mark_floating(
        &mut panel,
        "stale-edge-menu",
        "stale-edge-trigger",
        PopupPlacement::BottomStart,
    );
    panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "1");
    panel.set_attr(ATTR_FLOATING_CONTENT_H, "96");
    panel.set_attr(ATTR_FLOATING_ANCHOR_EDGE_Y, "492");
    panel.style.max_size.height = Dimension::length(96.0);
    panel.style.inset.top = Element::into_margin_auto(396.0);
    panel.layout_bounds = [20.0, 396.0, 80.0, 96.0];

    let mut root = Element::new().child(trigger).child(panel);
    assert!(sync_floating_placement(&mut root, 800.0, 360.0));
    let menu = &root.children[1];
    let top = inset_length(menu.style.inset.top).expect("top");
    assert!(
        (top - 220.0).abs() < 1.0,
        "settled menu must follow moved trigger, got {top}"
    );
    assert_eq!(
        menu.attrs
            .get(ATTR_FLOATING_ANCHOR_EDGE_Y)
            .map(String::as_str),
        Some("316")
    );
}

/// Item-pick rebuilds the panel and drops accordion-up for a frame. Using
/// `CONTENT_H` as visual height would slam `top` to the fully-open y (jump up)
/// before the next motion seed glues it back.
#[test]
fn sync_floating_exit_rebuild_without_anim_must_not_jump_up() {
    use taffy::style::Dimension;

    let mut trigger = Element::new().focus_id("t-exit-rebuild");
    trigger.layout_bounds = [20.0, 500.0, 80.0, 28.0];

    let mut panel = Element::new().absolute();
    mark_floating(
        &mut panel,
        "exit-rebuild",
        "t-exit-rebuild",
        PopupPlacement::BottomStart,
    );
    panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "1");
    panel.set_attr(ATTR_FLOATING_CONTENT_H, "192");
    panel.set_attr(ATTR_FLOATING_MAX_H, "16rem");
    panel.set_attr(ATTR_FLOATING_ANCHOR_EDGE_Y, "492");
    // Mid-collapse clip; layout_bounds still the unconstrained list.
    panel.set_attr(ATTR_FLOATING_EXITING, "1");
    panel.style.max_size.height = Dimension::length(80.0);
    panel.style.inset.top = Element::into_margin_auto(412.0);
    panel.layout_bounds = [20.0, 412.0, 80.0, 192.0];

    let mut root = Element::new().child(trigger).child(panel);
    let _ = sync_floating_placement(&mut root, 800.0, 600.0);
    let y = inset_length(root.children[1].style.inset.top).expect("top");
    let mh = dimension_length(root.children[1].style.max_size.height);
    assert!(
            (y - 416.0).abs() < 1.0,
            "mid-exit rebuild must follow the current trigger edge, not jump to edge-CONTENT_H ({}); y={y} mh={mh:?}",
            496.0 - 80.0
        );
    if let Some(h) = mh {
        assert!(
            (h - 80.0).abs() < 1.0,
            "placement must not retarget accordion max-height to the viewport cap; mh={h}"
        );
    }
}

#[test]
fn apply_stored_floating_place_seeds_unbounded_max_h_without_attr() {
    use crate::core::reactivity::{create_effect, create_root};
    let (_h, _) = create_root(|| {
        create_effect(|| {
            let mut el = Element::new().absolute().w(160.0);
            el.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "0");
            el.set_attr(ATTR_FLOATING_MIN_W, "0");
            apply_stored_floating_place(&mut el, "ao-seed-max-h");
            assert!(
                el.style.max_size.height.is_auto(),
                "no jadix_floating_max_h must seed style max-height auto, not a 4096px band"
            );
        });
    });
}

#[test]
fn write_floating_place_coerces_percent_height_to_auto() {
    let mut el = Element::new().absolute().w(160.0).h("100%").max_h(192.0);
    let place = FloatingPlace {
        x: 0.0,
        y: 40.0,
        w: 160.0,
        max_h: 400.0,
        width_auto: false,
        min_w: 0.0,
        anchor_edge_y: None,
    };
    assert!(write_floating_place(&mut el, place));
    assert!(
        el.style.size.height.is_auto(),
        "ScrollArea h:100% must not stretch floating menus"
    );
    match crate::layout::dim_kind(el.style.max_size.height) {
        crate::layout::DimKind::Length(h) => assert!(
            (h - 400.0).abs() < 0.1,
            "viewport max_h must not ratchet against prior style.max_h, got {h}"
        ),
        other => panic!("expected max height length, got {other:?}"),
    }
}

#[test]
fn write_floating_place_clamps_max_h_to_content_h_when_shorter() {
    use taffy::style::Dimension;

    let mut el = Element::new().absolute().w(160.0);
    el.set_attr(ATTR_FLOATING_CONTENT_H, "152");
    el.style.max_size.height = Dimension::length(152.0);
    let place = FloatingPlace {
        x: 10.0,
        y: 20.0,
        w: 160.0,
        max_h: 256.0,
        width_auto: false,
        min_w: 0.0,
        anchor_edge_y: None,
    };
    let _ = write_floating_place(&mut el, place);
    match crate::layout::dim_kind(el.style.max_size.height) {
        crate::layout::DimKind::Length(h) => assert!(
            (h - 152.0).abs() < 0.1,
            "place CAP must not stretch style max-h past CONTENT_H, got {h}"
        ),
        other => panic!("expected Length, got {other:?}"),
    }
}

#[test]
fn write_floating_place_max_h_can_grow_after_viewport_opens() {
    let mut el = Element::new().absolute().w(160.0);
    let small = FloatingPlace {
        x: 10.0,
        y: 20.0,
        w: 160.0,
        max_h: 80.0,
        width_auto: true,
        min_w: 0.0,
        anchor_edge_y: None,
    };
    assert!(write_floating_place(&mut el, small));
    let big = FloatingPlace {
        max_h: 320.0,
        ..small
    };
    assert!(write_floating_place(&mut el, big));
    match crate::layout::dim_kind(el.style.max_size.height) {
        crate::layout::DimKind::Length(h) => assert!(
            (h - 320.0).abs() < 0.1,
            "max_h must grow back after scroll, got {h}"
        ),
        other => panic!("expected Length max height, got {other:?}"),
    }
}

#[test]
fn write_floating_place_respects_fixed_height_attr() {
    let mut el = Element::new().absolute().w(160.0);
    el.set_attr(ATTR_FLOATING_H, "200px");
    let place = FloatingPlace {
        x: 0.0,
        y: 0.0,
        w: 160.0,
        max_h: 400.0,
        width_auto: false,
        min_w: 0.0,
        anchor_edge_y: None,
    };
    assert!(write_floating_place(&mut el, place));
    match crate::layout::dim_kind(el.style.size.height) {
        crate::layout::DimKind::Length(h) => assert!((h - 200.0).abs() < 0.1),
        other => panic!("expected fixed height, got {other:?}"),
    }
}

#[test]
fn write_floating_place_respects_min_h_override() {
    let mut el = Element::new().absolute().w(160.0).min_h(64.0);
    let place = FloatingPlace {
        x: 0.0,
        y: 0.0,
        w: 160.0,
        max_h: 200.0,
        width_auto: false,
        min_w: 0.0,
        anchor_edge_y: None,
    };
    assert!(write_floating_place(&mut el, place));
    match crate::layout::dim_kind(el.style.min_size.height) {
        crate::layout::DimKind::Length(h) => assert!((h - 64.0).abs() < 0.1),
        other => panic!("expected preserved min height, got {other:?}"),
    }
}

#[test]
fn write_floating_place_auto_width_uses_min_not_forced_w() {
    let mut el = Element::new().absolute().w(400.0);
    let place = FloatingPlace {
        x: 10.0,
        y: 20.0,
        w: 120.0,
        max_h: 200.0,
        width_auto: true,
        min_w: 120.0,
        anchor_edge_y: None,
    };
    assert!(write_floating_place(&mut el, place));
    assert!(
        el.style.size.width.is_auto(),
        "independent panel must not force trigger width"
    );
    match crate::layout::dim_kind(el.style.min_size.width) {
        crate::layout::DimKind::Length(w) => assert!((w - 120.0).abs() < 0.1),
        other => panic!("expected min-width = trigger, got {other:?}"),
    }
}

#[test]
fn sync_floating_independent_width_is_content_auto() {
    let mut trigger = Element::new().focus_id("t-wide");
    trigger.layout_bounds = [20.0, 40.0, 80.0, 28.0];

    let mut panel = Element::new().absolute().w(300.0);
    mark_floating(
        &mut panel,
        "indep-menu",
        "t-wide",
        PopupPlacement::BottomStart,
    );
    // match_trigger=0 and no jadix_floating_w → content auto, min ≥ trigger
    panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "0");
    panel.layout_bounds = [0.0, 0.0, 220.0, 60.0];

    let mut root = Element::new().child(trigger).child(panel);
    assert!(sync_floating_placement(&mut root, 800.0, 600.0));
    let menu = &root.children[1];
    assert!(
        menu.style.size.width.is_auto(),
        "panel width must stay independent of trigger"
    );
    match crate::layout::dim_kind(menu.style.min_size.width) {
        crate::layout::DimKind::Length(w) => assert!((w - 80.0).abs() < 0.5),
        other => panic!("min-width should be trigger width, got {other:?}"),
    }
}

#[test]
fn sync_floating_w_attr_auto_is_content_sized() {
    let mut trigger = Element::new().focus_id("t-auto-w");
    trigger.layout_bounds = [20.0, 40.0, 96.0, 32.0];

    let mut panel = Element::new().absolute();
    mark_floating(
        &mut panel,
        "expand-auto-w",
        "t-auto-w",
        PopupPlacement::BottomStart,
    );
    panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "0");
    panel.set_attr(ATTR_FLOATING_MIN_W, "0");
    panel.set_attr(ATTR_FLOATING_W, "auto");
    panel.set_attr(ATTR_FLOATING_STRATEGY, "fixed");
    panel.layout_bounds = [0.0, 0.0, 240.0, 48.0];

    let mut root = Element::new().child(trigger).child(panel);
    assert!(sync_floating_placement(&mut root, 800.0, 600.0));
    let menu = &root.children[1];
    assert!(
        menu.style.size.width.is_auto(),
        "jadix_floating_w=auto must stay content-sized, not lock to trigger width {}",
        crate::layout::dim_length_px(menu.style.size.width).unwrap_or(-1.0)
    );
    match crate::layout::dim_kind(menu.style.min_size.width) {
        crate::layout::DimKind::Auto => {}
        crate::layout::DimKind::Length(w) if w < 0.5 => {}
        other => panic!("w=auto must not floor min-width to trigger, got {other:?}"),
    }
}

#[test]
fn sync_floating_w_fit_min_w_zero() {
    let mut trigger = Element::new().focus_id("t-fit");
    trigger.layout_bounds = [20.0, 40.0, 160.0, 28.0];

    let mut panel = Element::new().absolute();
    mark_floating_with_gap(
        &mut panel,
        "tip-fit",
        "t-fit",
        PopupPlacement::BottomCenter,
        4.0,
    );
    panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "0");
    panel.set_attr(ATTR_FLOATING_MIN_W, "0");
    panel.layout_bounds = [0.0, 0.0, 72.0, 24.0];

    let mut root = Element::new().child(trigger).child(panel);
    assert!(sync_floating_placement(&mut root, 800.0, 600.0));
    let tip = &root.children[1];
    assert!(
        tip.style.size.width.is_auto(),
        "tooltip must be w-fit (width: auto)"
    );
    match crate::layout::dim_kind(tip.style.min_size.width) {
        crate::layout::DimKind::Auto => {}
        crate::layout::DimKind::Length(w) if w < 0.5 => {}
        other => panic!("w-fit must not floor min-width to trigger, got {other:?}"),
    }
    // Prefer below with gap 4; center on trigger mid-x (20+80=100 → x=100-36=64).
    match inset_length(tip.style.inset.top) {
        Some(y) => assert!((y - (40.0 + 28.0 + 4.0)).abs() < 0.5, "y={y}"),
        None => panic!("expected top inset"),
    }
    match inset_length(tip.style.inset.left) {
        Some(x) => assert!((x - 64.0).abs() < 0.5, "x={x}"),
        None => panic!("expected left inset"),
    }
}

#[test]
fn sync_floating_content_auto_defers_xy_until_measured() {
    let mut trigger = Element::new().focus_id("t-defer");
    trigger.layout_bounds = [100.0, 40.0, 80.0, 28.0];

    let mut panel = Element::new().absolute();
    mark_floating_with_gap(
        &mut panel,
        "hc-defer",
        "t-defer",
        PopupPlacement::BottomCenter,
        4.0,
    );
    panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "0");
    panel.set_attr(ATTR_FLOATING_MIN_W, "0");
    // Unmeasured first paint — must not commit a 32px-wide center guess.
    panel.layout_bounds = [0.0, 0.0, 0.0, 0.0];

    let mut root = Element::new().child(trigger).child(panel);
    let _ = sync_floating_placement(&mut root, 800.0, 600.0);
    assert!(
        inset_length(root.children[1].style.inset.left).is_none(),
        "unmeasured content-auto must not write left"
    );
    assert!(
        inset_length(root.children[1].style.inset.top).is_none(),
        "unmeasured content-auto must not write top"
    );

    root.children[1].layout_bounds = [0.0, 0.0, 256.0, 80.0];
    assert!(sync_floating_placement(&mut root, 800.0, 600.0));
    // Center: mid trigger 140 − half panel 128 → x=12.
    match inset_length(root.children[1].style.inset.left) {
        Some(x) => assert!((x - 12.0).abs() < 0.5, "x={x}"),
        None => panic!("expected left after measure"),
    }
    match inset_length(root.children[1].style.inset.top) {
        Some(y) => assert!((y - (40.0 + 28.0 + 4.0)).abs() < 0.5, "y={y}"),
        None => panic!("expected top after measure"),
    }
}

#[test]
fn sync_floating_explicit_w_stable_before_and_after_layout() {
    let mut trigger = Element::new().focus_id("t-stable");
    trigger.layout_bounds = [100.0, 40.0, 80.0, 28.0];

    let mut panel = Element::new().absolute();
    mark_floating_with_gap(
        &mut panel,
        "hc-stable",
        "t-stable",
        PopupPlacement::BottomCenter,
        4.0,
    );
    panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "0");
    panel.set_attr(ATTR_FLOATING_MIN_W, "0");
    panel.set_attr(ATTR_FLOATING_W, "256px");
    panel.layout_bounds = [0.0, 0.0, 0.0, 0.0];

    let mut root = Element::new().child(trigger).child(panel);
    assert!(sync_floating_placement(&mut root, 800.0, 600.0));
    let x0 = inset_length(root.children[1].style.inset.left).expect("left before layout");
    assert!((x0 - 12.0).abs() < 0.5, "x0={x0}");

    root.children[1].layout_bounds = [0.0, 0.0, 256.0, 120.0];
    let _ = sync_floating_placement(&mut root, 800.0, 600.0);
    let x1 = inset_length(root.children[1].style.inset.left).expect("left after layout");
    assert!(
        (x1 - x0).abs() < 0.5,
        "explicit w must not re-center after layout: {x0} → {x1}"
    );
}

#[test]
fn compute_flips_above_when_below_is_tight() {
    // Trigger near bottom; small panel must flip above.
    let trigger = [100.0, 560.0, 80.0, 28.0];
    let (_x, y, _max_h, flipped) = compute_anchored_placement(
        trigger,
        [800.0, 600.0],
        72.0,
        24.0,
        PopupPlacement::BottomCenter,
        4.0,
    );
    assert!(flipped, "must flip above when below has no room");
    // ty - gap - h = 560 - 4 - 24 = 532
    assert!((y - 532.0).abs() < 0.5, "y={y}");
}

#[test]
fn compute_picks_roomier_side_when_neither_fits() {
    // Trigger mid-low: below ≈ 100px, above ≈ 400px; content 300 fits neither.
    // Old min(80) threshold kept below; must open above to maximize scroll viewport.
    let trigger = [100.0, 420.0, 80.0, 28.0];
    let (_x, y, max_h, flipped) = compute_anchored_placement(
        trigger,
        [800.0, 600.0],
        160.0,
        300.0,
        PopupPlacement::BottomStart,
        8.0,
    );
    assert!(
        flipped,
        "neither side fits 300px; must pick the larger (above) side"
    );
    // space_above = 420 - 8 - 8 = 404; y = 420 - 8 - min(300, max(404,48))
    assert!(
        (max_h - 404.0).abs() < 0.5,
        "max_h must be the larger side, got {max_h}"
    );
    assert!(
        y + 0.5 < trigger[1],
        "panel must sit above the trigger, got y={y}"
    );
}

#[test]
fn sync_floating_flip_uses_attr_max_h_not_unconstrained_content() {
    // Nested Select (DatePicker year list): intrinsic content is thousands of px,
    // but `max-h-16rem` (256px) fits below. Flip must not use the uncapped height
    // or the menu jitters above/below and sticks on the roomier-but-wrong side.
    let mut trigger = Element::new().focus_id("cal:cap_y:trigger");
    // space_below = 600-8-(286+32)-4 = 270; space_above = 286-8-4 = 274.
    trigger.layout_bounds = [100.0, 286.0, 80.0, 32.0];

    let mut panel = Element::new().absolute();
    mark_floating(
        &mut panel,
        "cal:cap_y",
        "cal:cap_y:trigger",
        PopupPlacement::BottomStart,
    );
    panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "1");
    panel.set_attr(ATTR_FLOATING_MAX_H, "256px");
    panel.set_attr(ATTR_FLOATING_CONTENT_H, "2000");
    panel.layout_bounds = [0.0, 0.0, 80.0, 2000.0];

    let mut root = Element::new().child(trigger).child(panel);
    assert!(sync_floating_placement(&mut root, 800.0, 600.0));
    let top = inset_length(root.children[1].style.inset.top).expect("top");
    let expect_below = 286.0 + 32.0 + FLOATING_TRIGGER_GAP;
    assert!(
        (top - expect_below).abs() < 0.5,
        "capped 256px fits below (270px); must not flip above, got {top}"
    );
    assert!(
        root.children[1]
            .attrs
            .get(ATTR_FLOATING_ANCHOR_EDGE_Y)
            .is_none(),
        "must not glue a wrong above-edge after an unconstrained-content flip"
    );
}

#[test]
fn sync_floating_flip_stays_stable_when_content_grows_past_attr_max_h() {
    let mut trigger = Element::new().focus_id("t-grow");
    trigger.layout_bounds = [100.0, 286.0, 80.0, 32.0];

    let mut panel = Element::new().absolute();
    mark_floating(
        &mut panel,
        "grow-menu",
        "t-grow",
        PopupPlacement::BottomStart,
    );
    panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "1");
    panel.set_attr(ATTR_FLOATING_MAX_H, "256px");
    panel.layout_bounds = [0.0, 0.0, 80.0, 32.0];

    let mut root = Element::new().child(trigger).child(panel);
    assert!(sync_floating_placement(&mut root, 800.0, 600.0));
    let top0 = inset_length(root.children[1].style.inset.top).expect("top0");

    root.children[1].set_attr(ATTR_FLOATING_CONTENT_H, "2000");
    root.children[1].layout_bounds = [100.0, top0, 80.0, 2000.0];
    let _ = sync_floating_placement(&mut root, 800.0, 600.0);
    let top1 = inset_length(root.children[1].style.inset.top).expect("top1");
    assert!(
        (top1 - top0).abs() < 0.5,
        "expand-measure content height must not flip a menu whose cap still fits, {top0} → {top1}"
    );
}

#[test]
fn compute_center_aligns_mid_x() {
    // trigger mid = 100+40 = 140; panel w=72 → x = 140 - 36 = 104
    let (x, y, _, flipped) = compute_anchored_placement(
        [100.0, 40.0, 80.0, 28.0],
        [800.0, 600.0],
        72.0,
        24.0,
        PopupPlacement::BottomCenter,
        4.0,
    );
    assert!(!flipped);
    assert!((x - 104.0).abs() < 0.5, "x={x}");
    assert!((y - (40.0 + 28.0 + 4.0)).abs() < 0.5, "y={y}");
}

#[test]
fn compute_center_clamps_to_viewport_edge() {
    // Near left edge: ideal center would go past margin → clamp to margin.
    let (x, _, _, _) = compute_anchored_placement(
        [10.0, 40.0, 40.0, 28.0],
        [800.0, 600.0],
        120.0,
        24.0,
        PopupPlacement::BottomCenter,
        4.0,
    );
    assert!((x - FLOATING_VIEWPORT_MARGIN).abs() < 0.5, "x={x}");

    // Near right edge: clamp so panel right stays at vw - margin.
    let (x2, _, _, _) = compute_anchored_placement(
        [760.0, 40.0, 30.0, 28.0],
        [800.0, 600.0],
        100.0,
        24.0,
        PopupPlacement::BottomCenter,
        4.0,
    );
    let expect = 800.0 - FLOATING_VIEWPORT_MARGIN - 100.0;
    assert!((x2 - expect).abs() < 0.5, "x2={x2}");
}

#[test]
fn compute_bottom_start_basic() {
    let (x, y, max_h, flipped) = compute_anchored_placement(
        [10.0, 10.0, 100.0, 24.0],
        [800.0, 600.0],
        100.0,
        120.0,
        PopupPlacement::BottomStart,
        FLOATING_TRIGGER_GAP,
    );
    assert!(!flipped);
    assert_eq!(x, 10.0);
    // trigger bottom 34 + FLOATING_TRIGGER_GAP; not flush to trigger
    assert_eq!(y, 34.0 + FLOATING_TRIGGER_GAP);
    assert!(max_h > 48.0);
}

#[test]
fn compute_clamps_away_from_viewport_edges() {
    let (x, y, _, _) = compute_anchored_placement(
        [0.0, 0.0, 40.0, 20.0],
        [200.0, 200.0],
        80.0,
        40.0,
        PopupPlacement::BottomStart,
        FLOATING_TRIGGER_GAP,
    );
    assert!(
        x >= FLOATING_VIEWPORT_MARGIN - 0.01,
        "x must keep viewport margin, got {x}"
    );
    assert!(
        (y - (20.0 + FLOATING_TRIGGER_GAP)).abs() < 0.01,
        "y must keep trigger gap, got {y}"
    );

    let (x2, _, _, _) = compute_anchored_placement(
        [180.0, 10.0, 40.0, 20.0],
        [200.0, 200.0],
        80.0,
        40.0,
        PopupPlacement::BottomEnd,
        FLOATING_TRIGGER_GAP,
    );
    assert!(
        x2 + 80.0 <= 200.0 - FLOATING_VIEWPORT_MARGIN + 0.01,
        "right edge must keep viewport margin, x={x2}"
    );
}

#[test]
fn sync_floating_uses_scrolled_visual_trigger_bounds() {
    // Scroll viewport at (0,0) 800×600; content scrolled down by 400px.
    // Trigger layout_bounds y=500 (unscrolled) → visual y=100.
    let mut trigger = Element::new().focus_id("t1");
    trigger.layout_bounds = [40.0, 500.0, 120.0, 36.0];

    let mut scroller = Element::new().child(trigger);
    scroller.style.overflow.y = taffy::style::Overflow::Scroll;
    scroller.scroll_offset = [0.0, 400.0];
    scroller.layout_bounds = [0.0, 0.0, 800.0, 600.0];

    let mut panel = Element::new().absolute();
    mark_floating(&mut panel, "menu1", "t1", PopupPlacement::BottomStart);
    panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "0");
    panel.set_attr(ATTR_FLOATING_W, "120px");
    panel.layout_bounds = [0.0, 0.0, 120.0, 80.0];

    let mut root = Element::new().child(scroller).child(panel);
    root.layout_bounds = [0.0, 0.0, 800.0, 600.0];

    assert!(sync_floating_placement(&mut root, 800.0, 600.0));

    let menu = &root.children[1];
    let left = inset_length(menu.style.inset.left).expect("expected Length left inset");
    let top = inset_length(menu.style.inset.top).expect("expected Length top inset");
    assert!(
        (left - 40.0).abs() < 0.5,
        "x should follow visual trigger, got {left}"
    );
    // visual trigger bottom = 100+36=136; + FLOATING_TRIGGER_GAP (not unscrolled 500+36+gap)
    assert!(
        (top - (136.0 + FLOATING_TRIGGER_GAP)).abs() < 0.5,
        "y must use scrolled visual bounds (expect ~{}), got {top}",
        136.0 + FLOATING_TRIGGER_GAP
    );
}

#[test]
fn sync_floating_uses_scroll_viewport_prefix_when_overflow_visible() {
    // ScrollArea viewport may keep scroll_offset after overflow is cleared to Visible.
    // Paint/hit-test still accumulate via `scroll:` focus id; floating must too.
    let mut trigger = Element::new().focus_id("t-sv");
    trigger.layout_bounds = [20.0, 480.0, 100.0, 32.0];

    let mut scroller = Element::new().child(trigger);
    scroller.focus_id = Some(format!("{SCROLL_VIEWPORT_FOCUS_PREFIX}gallery"));
    scroller.style.overflow.y = taffy::style::Overflow::Visible;
    scroller.scroll_offset = [0.0, 360.0];
    scroller.layout_bounds = [0.0, 0.0, 800.0, 600.0];

    let mut panel = Element::new().absolute();
    mark_floating(&mut panel, "menu-sv", "t-sv", PopupPlacement::BottomStart);
    panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "0");
    panel.set_attr(ATTR_FLOATING_W, "100px");
    panel.layout_bounds = [0.0, 0.0, 100.0, 80.0];

    let mut root = Element::new().child(scroller).child(panel);
    root.layout_bounds = [0.0, 0.0, 800.0, 600.0];

    assert!(sync_floating_placement(&mut root, 800.0, 600.0));
    let top = inset_length(root.children[1].style.inset.top).expect("expected Length top inset");
    // visual trigger y=120, h=32 + FLOATING_TRIGGER_GAP (not unscrolled 480+32+gap)
    assert!(
        (top - (152.0 + FLOATING_TRIGGER_GAP)).abs() < 0.5,
        "y must follow scroll: viewport offset, got {top}"
    );
}

#[test]
fn sync_floating_skips_zero_size_duplicate_focus_id() {
    // In-page portal slot / unmeasured ghost is first in document order.
    let mut ghost = Element::new().focus_id("cal:cap_m:trigger");
    ghost.layout_bounds = [0.0, 0.0, 0.0, 0.0];

    let mut trigger = Element::new().focus_id("cal:cap_m:trigger");
    trigger.layout_bounds = [80.0, 360.0, 116.0, 32.0];

    let mut panel = Element::new().absolute();
    mark_floating(
        &mut panel,
        "cal:cap_m",
        "cal:cap_m:trigger",
        PopupPlacement::BottomStart,
    );
    panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "0");
    panel.set_attr(ATTR_FLOATING_W, "144px");
    panel.layout_bounds = [0.0, 0.0, 144.0, 220.0];

    let mut root = Element::new().child(ghost).child(trigger).child(panel);
    root.layout_bounds = [0.0, 0.0, 800.0, 600.0];

    assert!(sync_floating_placement(&mut root, 800.0, 600.0));
    let top = inset_length(root.children[2].style.inset.top).expect("top");
    // Trigger at y=360, estimate 220 → flips above (360-gap-220), not origin.
    assert!(
        (top - (360.0 - FLOATING_TRIGGER_GAP - 220.0)).abs() < 0.5,
        "must ignore 0×0 duplicate focus_id (got {top})"
    );
}

#[test]
fn sync_floating_prefers_hoisted_trigger_over_page_ghost() {
    use crate::layout::layout_tree::ATTR_HOISTED_PORTAL;

    // Forms DatePicker leftover (scrolled into view at y=40) shares focus_id
    // with the hoisted copy inside the portal layer (inline / nested chip).
    let mut page_ghost = Element::new().focus_id("cal:cap_m:trigger");
    page_ghost.layout_bounds = [80.0, 40.0, 116.0, 32.0];

    let mut hoisted_trigger = Element::new().focus_id("cal:cap_m:trigger");
    hoisted_trigger.layout_bounds = [88.0, 400.0, 116.0, 32.0];

    let mut host = Element::new().child(hoisted_trigger);
    host.style.position_fixed = true;
    host.layout_bounds = [80.0, 360.0, 280.0, 320.0];
    host.set_attr(ATTR_HOISTED_PORTAL, "1");

    let mut layer = Element::new().child(host);
    layer.template_id = Some(PORTAL_LAYER_ID.into());
    layer.layout_bounds = [0.0, 0.0, 800.0, 600.0];

    let mut panel = Element::new().absolute();
    mark_floating(
        &mut panel,
        "cal:cap_m",
        "cal:cap_m:trigger",
        PopupPlacement::BottomStart,
    );
    panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "0");
    panel.set_attr(ATTR_FLOATING_W, "144px");
    panel.layout_bounds = [0.0, 0.0, 144.0, 220.0];
    panel.set_attr(ATTR_HOISTED_PORTAL, "1");
    layer.children.push(panel);

    let mut root = Element::new().child(page_ghost).child(layer);
    root.layout_bounds = [0.0, 0.0, 800.0, 600.0];

    assert!(sync_floating_placement(&mut root, 800.0, 600.0));
    let menu = root
        .children
        .iter()
        .find(|c| c.template_id.as_deref() == Some(PORTAL_LAYER_ID))
        .expect("portal layer")
        .children
        .iter()
        .find(|c| c.attrs.get(ATTR_FLOATING).map(String::as_str) == Some("cal:cap_m"))
        .expect("menu");
    let top = inset_length(menu.style.inset.top).expect("top");
    // Hoisted chip at y=400, estimate 220 → flips above (400-gap-220).
    // Page ghost at y=40 would have placed at ~80.
    assert!(
        (top - (400.0 - FLOATING_TRIGGER_GAP - 220.0)).abs() < 0.5,
        "nested/inline select must follow the hoisted trigger, got {top}"
    );
}

#[test]
fn upward_enter_glues_from_trigger_not_content_height() {
    // First placement pass often has CONTENT_H but no accordion clip yet.
    // Gluing with full content parks the menu far above; enter then snaps.
    let mut trigger = Element::new().focus_id("enter-glue-trigger");
    trigger.layout_bounds = [80.0, 400.0, 100.0, 32.0];
    let mut panel = Element::new().absolute();
    mark_floating(
        &mut panel,
        "enter-glue",
        "enter-glue-trigger",
        PopupPlacement::BottomStart,
    );
    panel.set_attr(ATTR_FLOATING_W, "104px");
    panel.set_attr(ATTR_FLOATING_CONTENT_H, "192");
    panel.set_attr(ATTR_FLOATING_MAX_H, "16rem");
    // No max-height clip yet (pre-enter / post-measure).
    panel.style.max_size.height = taffy::style::Dimension::auto();
    panel.layout_bounds = [0.0, 0.0, 104.0, 192.0];
    let mut root = Element::new().child(trigger).child(panel);
    sync_floating_placement(&mut root, 800.0, 480.0);
    let menu = &root.children[1];
    let edge = reverse_expand_edge_y(menu).expect("must place above");
    let top = inset_length(menu.style.inset.top).expect("top");
    assert!(
        (edge - (400.0 - FLOATING_TRIGGER_GAP)).abs() < 0.5,
        "edge={edge}"
    );
    assert!(
            (top - edge).abs() < 1.0,
            "pre-enter upward glue must sit on the trigger edge (gap≈0 silhouette), not edge-CONTENT_H; top={top} edge={edge}"
        );
}

/// Placement / measure write the author CAP (`16rem`) onto style max-height
/// before accordion keyframes own the clip. That CAP must not become
/// `visual_h` (large gap above the trigger, then snap on enter).
#[test]
fn upward_enter_ignores_place_cap_as_silhouette() {
    let mut trigger = Element::new().focus_id("enter-cap-trigger");
    trigger.layout_bounds = [80.0, 449.0, 100.0, 32.0];
    let mut panel = Element::new().absolute();
    mark_floating(
        &mut panel,
        "enter-cap",
        "enter-cap-trigger",
        PopupPlacement::BottomStart,
    );
    panel.set_attr(ATTR_FLOATING_W, "104px");
    panel.set_attr(ATTR_FLOATING_CONTENT_H, "192");
    panel.set_attr(ATTR_FLOATING_MAX_H, "16rem");
    // Place already wrote the CAP — the bug frame for inline ColorPicker.
    panel.style.max_size.height = taffy::style::Dimension::length(256.0);
    panel.layout_bounds = [0.0, 0.0, 104.0, 152.0];
    let mut root = Element::new().child(trigger).child(panel);
    sync_floating_placement(&mut root, 800.0, 600.0);
    let menu = &root.children[1];
    let edge = reverse_expand_edge_y(menu).expect("must place above");
    let top = inset_length(menu.style.inset.top).expect("top");
    let expect_edge = 449.0 - FLOATING_TRIGGER_GAP;
    assert!(
        (edge - expect_edge).abs() < 0.5,
        "edge={edge} expect={expect_edge}"
    );
    // Glue to the laid-out box (152), not the 16rem CAP (256) — gap≈4, not ~108.
    assert!(
        (top - (edge - 152.0)).abs() < 1.0,
        "place CAP must not park the menu at edge-256; top={top} edge={edge}"
    );
}

/// Mid-enter: the accordion clip is still short enough to fit *below* the
/// trigger. Placement must not drop a committed above-edge and flip the
/// growing menu downward (then back up when CONTENT_H returns).
#[test]
fn upward_enter_must_not_flip_below_mid_animation() {
    use crate::style::{keyframes_anim_entry, AnimationFillMode, TimingFunction};
    use taffy::style::Dimension;

    let mut trigger = Element::new().focus_id("enter-no-flip");
    // Room below for a short mid-enter clip; full content would still prefer above
    // when CONTENT_H is present — strip it so estimate falls back to layout 48.
    trigger.layout_bounds = [80.0, 400.0, 100.0, 32.0];
    let mut panel = Element::new().absolute();
    mark_floating(
        &mut panel,
        "enter-no-flip",
        "enter-no-flip",
        PopupPlacement::BottomStart,
    );
    panel.set_attr(ATTR_FLOATING_W, "104px");
    panel.set_attr(ATTR_FLOATING_MAX_H, "16rem");
    panel.set_attr(ATTR_FLOATING_ANCHOR_EDGE_Y, "396");
    panel.style.max_size.height = Dimension::length(48.0);
    panel.style.inset.top = Element::into_margin_auto(348.0);
    panel.layout_bounds = [80.0, 348.0, 104.0, 48.0];
    panel.style.animations = vec![keyframes_anim_entry(
        "accordion-down",
        200.0,
        TimingFunction::EaseOut,
        AnimationFillMode::Both,
    )];
    let mut root = Element::new().child(trigger).child(panel);
    sync_floating_placement(&mut root, 800.0, 600.0);
    let menu = &root.children[1];
    let edge = reverse_expand_edge_y(menu).expect("must stay above during enter");
    let top = inset_length(menu.style.inset.top).expect("top");
    let mh = dimension_length(menu.style.max_size.height).unwrap_or(48.0);
    assert!(
        (edge - (400.0 - FLOATING_TRIGGER_GAP)).abs() < 0.5,
        "enter must re-seed edge from live trigger; edge={edge}"
    );
    assert!(
        top + 0.5 < 400.0,
        "mid-enter must not flip below the trigger; top={top}"
    );
    assert!(
        (top + mh - edge).abs() < 1.0,
        "silhouette must stay glued above; top={top} mh={mh} edge={edge}"
    );
}

#[test]
fn reopened_above_menu_reseeds_edge_from_live_trigger() {
    // Inline ColorPicker: open format Select above, then reopen after the alpha
    // row shifts the trigger. Enter must not glue to the previous open's edge.
    let mut trigger = Element::new().focus_id("inline-fmt-trigger");
    trigger.layout_bounds = [80.0, 280.0, 100.0, 36.0];
    let mut panel = Element::new().absolute();
    mark_floating(
        &mut panel,
        "inline-fmt",
        "inline-fmt-trigger",
        PopupPlacement::BottomStart,
    );
    panel.set_attr(ATTR_FLOATING_W, "104px");
    panel.set_attr(ATTR_FLOATING_CONTENT_H, "192");
    panel.layout_bounds = [0.0, 0.0, 104.0, 192.0];
    let mut root = Element::new().child(trigger).child(panel);
    sync_floating_placement(&mut root, 800.0, 360.0);
    let old_edge = reverse_expand_edge_y(&root.children[1]).expect("first open above");
    assert!((old_edge - (280.0 - FLOATING_TRIGGER_GAP)).abs() < 0.5);

    // Alpha row mounts → trigger moves down. Fresh portal restores the old place.
    root.children[0].layout_bounds[1] = 340.0;
    let mut fresh = Element::new().absolute();
    mark_floating(
        &mut fresh,
        "inline-fmt",
        "inline-fmt-trigger",
        PopupPlacement::BottomStart,
    );
    fresh.set_attr(ATTR_FLOATING_W, "104px");
    fresh.set_attr(ATTR_FLOATING_CONTENT_H, "192");
    fresh.layout_bounds = [0.0, 0.0, 104.0, 96.0];
    apply_stored_floating_place(&mut fresh, "inline-fmt");
    fresh.style.animations = vec![crate::style::keyframes_anim_entry(
        "accordion-down",
        200.0,
        crate::style::TimingFunction::EaseOut,
        crate::style::AnimationFillMode::Both,
    )];
    fresh.style.max_size.height = taffy::style::Dimension::length(96.0);
    assert!(
        reverse_expand_edge_y(&fresh).is_none(),
        "fresh open must not restore the previous open's edge"
    );
    root.children[1] = fresh;
    sync_floating_placement(&mut root, 800.0, 360.0);
    let menu = &root.children[1];
    let edge = reverse_expand_edge_y(menu).expect("enter stays above");
    let expect_edge = 340.0 - FLOATING_TRIGGER_GAP;
    assert!(
            (edge - expect_edge).abs() < 0.5,
            "enter must re-seed from the live trigger, not the previous open; edge={edge} old={old_edge} expect={expect_edge}"
        );
    let top = inset_length(menu.style.inset.top).unwrap();
    let mh = dimension_length(menu.style.max_size.height).unwrap_or(96.0);
    assert!(
        (top + mh - edge).abs() < 0.5,
        "enter silhouette must glue to the new edge; top={top} mh={mh} edge={edge}"
    );
}

#[test]
fn exiting_above_menu_tracks_trigger_without_reflipping() {
    let mut trigger = Element::new().focus_id("moving-exit-trigger");
    trigger.layout_bounds = [80.0, 320.0, 100.0, 32.0];
    let mut panel = Element::new().absolute();
    mark_floating(
        &mut panel,
        "moving-exit",
        "moving-exit-trigger",
        PopupPlacement::BottomStart,
    );
    panel.layout_bounds = [0.0, 0.0, 100.0, 152.0];
    let mut root = Element::new().child(trigger).child(panel);
    sync_floating_placement(&mut root, 800.0, 360.0);
    let original_cap = root.children[1].style.max_size.height;
    root.children[1].set_attr(ATTR_FLOATING_EXITING, "1");
    // Alpha row removal/addition moves the trigger while the menu collapses.
    for (trigger_y, height) in [(288.0, 96.0), (320.0, 48.0), (288.0, 12.0)] {
        root.children[0].layout_bounds[1] = trigger_y;
        let menu = &mut root.children[1];
        let old_edge = reverse_expand_edge_y(menu).unwrap();
        menu.style.inset.top = Element::into_margin_auto(old_edge - height);
        menu.style.max_size.height = taffy::style::Dimension::length(height);
        menu.layout_bounds[3] = height;
        sync_floating_placement(&mut root, 800.0, 360.0);
        let menu = &root.children[1];
        let edge = reverse_expand_edge_y(menu).expect("must stay above");
        assert!(
            (edge - (trigger_y - FLOATING_TRIGGER_GAP)).abs() < 0.5,
            "exit anchor must follow trigger: edge={edge}, trigger={trigger_y}"
        );
        assert!((inset_length(menu.style.inset.top).unwrap() + height - edge).abs() < 0.5);
        assert_eq!(dimension_length(menu.style.max_size.height), Some(height));
    }
    assert!(dimension_length(original_cap).unwrap() > 100.0);
}

#[test]
fn exiting_below_menu_tracks_trigger_without_reflipping() {
    let mut trigger = Element::new().focus_id("moving-exit-below");
    trigger.layout_bounds = [80.0, 80.0, 100.0, 32.0];
    let mut panel = Element::new().absolute();
    mark_floating(
        &mut panel,
        "moving-exit-below",
        "moving-exit-below",
        PopupPlacement::BottomStart,
    );
    panel.layout_bounds = [0.0, 0.0, 100.0, 152.0];
    let mut root = Element::new().child(trigger).child(panel);
    sync_floating_placement(&mut root, 800.0, 600.0);
    assert!(
        reverse_expand_edge_y(&root.children[1]).is_none(),
        "fixture must open below the trigger"
    );
    let original_cap = root.children[1].style.max_size.height;
    root.children[1].set_attr(ATTR_FLOATING_EXITING, "1");
    // ColorPicker alpha row mount/unmount shifts the format Select trigger.
    for (trigger_y, height) in [(112.0, 96.0), (80.0, 48.0), (112.0, 12.0)] {
        root.children[0].layout_bounds[1] = trigger_y;
        let menu = &mut root.children[1];
        menu.style.max_size.height = taffy::style::Dimension::length(height);
        menu.layout_bounds[3] = height;
        sync_floating_placement(&mut root, 800.0, 600.0);
        let menu = &root.children[1];
        let top = inset_length(menu.style.inset.top).expect("top");
        let expect = trigger_y + 32.0 + FLOATING_TRIGGER_GAP;
        assert!(
            (top - expect).abs() < 0.5,
            "exit top must follow trigger: top={top}, expect={expect}, trigger={trigger_y}"
        );
        assert!(reverse_expand_edge_y(menu).is_none(), "must not flip above");
        assert_eq!(dimension_length(menu.style.max_size.height), Some(height));
    }
    assert!(dimension_length(original_cap).unwrap() > 100.0);
}

#[test]
fn rebuilt_top_menu_preserves_anchor_while_shrinking() {
    let mut trigger = Element::new().focus_id("t-rebuilt-top");
    trigger.layout_bounds = [80.0, 500.0, 100.0, 32.0];
    let mut panel = Element::new().absolute();
    mark_floating(
        &mut panel,
        "rebuilt-top",
        "t-rebuilt-top",
        PopupPlacement::BottomStart,
    );
    panel.set_attr(ATTR_FLOATING_W, "100px");
    panel.layout_bounds = [0.0, 0.0, 100.0, 200.0];
    let mut root = Element::new().child(trigger).child(panel);
    sync_floating_placement(&mut root, 800.0, 600.0);
    let edge = reverse_expand_edge_y(&root.children[1]).unwrap();

    let mut fresh = Element::new().absolute();
    mark_floating(
        &mut fresh,
        "rebuilt-top",
        "t-rebuilt-top",
        PopupPlacement::BottomStart,
    );
    fresh.set_attr(ATTR_FLOATING_W, "100px");
    fresh.layout_bounds = [0.0, 0.0, 100.0, 24.0];
    fresh.set_attr(ATTR_FLOATING_EXITING, "1");
    // Same order as assemble_mounted_panel: restore the exit pose, rather
    // than seed a new open (which intentionally drops the old above edge).
    apply_stored_floating_place(&mut fresh, "rebuilt-top");
    fresh.style.animations = vec![crate::style::keyframes_anim_entry(
        "accordion-up",
        200.0,
        crate::style::TimingFunction::EaseIn,
        crate::style::AnimationFillMode::Both,
    )];
    fresh.style.max_size.height = taffy::style::Dimension::length(24.0);
    assert_eq!(reverse_expand_edge_y(&fresh), Some(edge));
    root.children[1] = fresh;
    sync_floating_placement(&mut root, 800.0, 600.0);
    clamp_animated_max_h_to_place(&mut root.children[1], "rebuilt-top");
    let top = inset_length(root.children[1].style.inset.top).unwrap();
    assert!((top + 24.0 - edge).abs() < 0.5, "top={top}, edge={edge}");
}

#[test]
fn sync_floating_places_unmeasured_content_auto_on_first_open() {
    // DatePicker / content-auto: panel layout_bounds still 0×0 on the first sync
    // (collect stamps later). Must still write trigger-relative XY — not leave
    // apply_stored (0,0) under Fade enter.
    let mut trigger = Element::new().focus_id("t-unmeasured");
    trigger.layout_bounds = [220.0, 180.0, 120.0, 36.0];
    let mut panel = Element::new().absolute();
    mark_floating(
        &mut panel,
        "unmeasured-menu",
        "t-unmeasured",
        PopupPlacement::BottomStart,
    );
    panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "0");
    panel.set_attr(ATTR_FLOATING_MIN_W, "0");
    apply_stored_floating_place(&mut panel, "unmeasured-menu");
    assert_eq!(
        inset_length(panel.style.inset.left),
        Some(0.0),
        "precondition: stored seed starts at origin"
    );
    // Unmeasured — the old size_unknown path skipped XY here.
    panel.layout_bounds = [0.0, 0.0, 0.0, 0.0];
    let mut root = Element::new().child(trigger).child(panel);
    assert!(sync_floating_placement(&mut root, 800.0, 600.0));
    let left = inset_length(root.children[1].style.inset.left).expect("left");
    let top = inset_length(root.children[1].style.inset.top).expect("top");
    assert!(
        (left - 220.0).abs() < 0.5,
        "first sync must place X on the trigger; left={left}"
    );
    assert!(
        (top - (180.0 + 36.0 + FLOATING_TRIGGER_GAP)).abs() < 0.5,
        "first sync must place Y below the trigger; top={top}"
    );
}

#[test]
fn nested_local_mounts_convert_parent_coordinates_before_placement() {
    crate::core::reactivity::create_root(|| {
        let mut trigger = Element::new().focus_id("nested-local-trigger");
        trigger.layout_bounds = [155.0, 160.0, 20.0, 8.0];
        let mut panel = Element::new();
        mark_floating(
            &mut panel,
            "nested-local-panel",
            "nested-local-trigger",
            PopupPlacement::BottomStart,
        );
        panel.layout_bounds = [0.0, 0.0, 30.0, 20.0];
        panel.set_attr("jadix_floating_match_trigger", "0");
        panel.set_attr("jadix_floating_w", "30px");
        panel.set_attr(
            crate::layout::layout_tree::ATTR_HOISTED_MOUNT,
            "inner-local",
        );
        let mut inner = Element::new()
            .template_id("inner-local")
            .scale(0.5)
            .transform_origin("left top")
            .child(panel);
        inner.layout_bounds = [150.0, 150.0, 200.0, 300.0];
        inner.paint_ancestor_scale = [0.5; 2];
        let mut outer = Element::new()
            .scale(0.5)
            .transform_origin("left top")
            .child(inner);
        outer.layout_bounds = [100.0, 100.0, 800.0, 800.0];
        outer.set_attr(crate::core::scaled_viewport::ATTR_PORTALS, "1");
        let mut root = Element::new()
            .overflow_y("scroll")
            .children([trigger, outer]);
        for scroll in [0.0, 20.0, 50.0] {
            root.scroll_offset[1] = scroll;
            sync_floating_placement(&mut root, 1000.0, 1000.0);
            let panel = &root.children[1].children[0].children[0];
            assert!((inset_length(panel.style.inset.left).unwrap() - 20.0).abs() < 0.5);
            assert!(
                (inset_length(panel.style.inset.top).unwrap() - 72.0 - FLOATING_TRIGGER_GAP).abs()
                    < 0.5,
                "nested placement must stay local while ancestor scrolls by {scroll}"
            );
        }
    });
}

#[test]
fn scaled_preview_positions_popups_in_local_logical_coordinates() {
    crate::core::reactivity::create_root(|| {
        let mut trigger = Element::new().focus_id("device-popup-trigger");
        trigger.layout_bounds = [110.0, 120.0, 50.0, 20.0];
        let mut panel = Element::new().absolute();
        mark_floating(
            &mut panel,
            "device-popup-menu",
            "device-popup-trigger",
            PopupPlacement::BottomStart,
        );
        panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "0");
        panel.set_attr(ATTR_FLOATING_W, "100px");
        panel.layout_bounds = [0.0, 0.0, 100.0, 60.0];
        let mut scope = Element::new()
            .scale(0.5)
            .transform_origin("left top")
            .child(panel);
        scope.set_attr(crate::core::scaled_viewport::ATTR_PORTALS, "1");
        scope.layout_bounds = [100.0, 100.0, 400.0, 800.0];
        let mut root = Element::new().children([trigger, scope]);
        assert!(sync_floating_placement(&mut root, 1600.0, 1000.0));
        let popup = &root.children[1].children[0];
        let left = inset_length(popup.style.inset.left).unwrap();
        let top = inset_length(popup.style.inset.top).unwrap();
        assert!((left - 20.0).abs() < 0.5, "local x={left}");
        assert!(
            (top - 80.0 - FLOATING_TRIGGER_GAP).abs() < 0.5,
            "local y={top}"
        );
    });
}

#[test]
fn sync_floating_reapplies_place_onto_fresh_panel() {
    // First open writes place into the signal + element.
    let mut trigger = Element::new().focus_id("t-reopen");
    trigger.layout_bounds = [80.0, 120.0, 100.0, 32.0];

    let mut panel = Element::new().absolute();
    mark_floating(
        &mut panel,
        "reopen-menu",
        "t-reopen",
        PopupPlacement::BottomStart,
    );
    panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "0");
    panel.set_attr(ATTR_FLOATING_W, "100px");
    panel.layout_bounds = [0.0, 0.0, 100.0, 60.0];

    let mut root = Element::new().child(trigger).child(panel);
    assert!(sync_floating_placement(&mut root, 800.0, 600.0));
    let top1 = inset_length(root.children[1].style.inset.top).unwrap();
    // trigger bottom 152 + FLOATING_TRIGGER_GAP
    assert!(
        (top1 - (152.0 + FLOATING_TRIGGER_GAP)).abs() < 0.5,
        "first place y={top1}"
    );

    // Second open: brand-new panel with auto insets; signal already holds the place.
    // Old bug skipped writing when signal was unchanged → stuck at (0,0) / zero-size.
    let mut trigger2 = Element::new().focus_id("t-reopen");
    trigger2.layout_bounds = [80.0, 120.0, 100.0, 32.0];
    let mut panel2 = Element::new().absolute();
    mark_floating(
        &mut panel2,
        "reopen-menu",
        "t-reopen",
        PopupPlacement::BottomStart,
    );
    panel2.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "0");
    panel2.set_attr(ATTR_FLOATING_W, "100px");
    panel2.layout_bounds = [0.0, 0.0, 100.0, 60.0];
    apply_stored_floating_place(&mut panel2, "reopen-menu");
    assert!(
        inset_length(panel2.style.inset.left).is_some(),
        "seed must set left before layout"
    );

    let mut root2 = Element::new().child(trigger2).child(panel2);
    // Even if sync considers the signal clean, it must still write onto the new node.
    let _ = sync_floating_placement(&mut root2, 800.0, 600.0);
    let left = inset_length(root2.children[1].style.inset.left).unwrap();
    let top = inset_length(root2.children[1].style.inset.top).unwrap();
    let width = dimension_length(root2.children[1].style.size.width).unwrap();
    assert!((left - 80.0).abs() < 0.5, "reopen left={left}");
    assert!(
        (top - (152.0 + FLOATING_TRIGGER_GAP)).abs() < 0.5,
        "reopen top={top}"
    );
    assert!((width - 100.0).abs() < 0.5, "reopen width={width}");
}

#[test]
fn sync_floating_fixed_strategy_keeps_height_near_viewport_edge() {
    let mut trigger = Element::new().focus_id("t-fixed");
    trigger.layout_bounds = [40.0, 520.0, 120.0, 36.0];

    let mut panel = Element::new().absolute();
    mark_floating(
        &mut panel,
        "dp-fixed",
        "t-fixed",
        PopupPlacement::BottomStart,
    );
    panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "0");
    panel.set_attr(ATTR_FLOATING_MIN_W, "0");
    panel.set_attr(ATTR_FLOATING_STRATEGY, "fixed");
    panel.layout_bounds = [0.0, 0.0, 280.0, 320.0];

    let mut root = Element::new().child(trigger).child(panel);
    root.layout_bounds = [0.0, 0.0, 800.0, 600.0];
    assert!(sync_floating_placement(&mut root, 800.0, 600.0));

    let menu = &root.children[1];
    let top = inset_length(menu.style.inset.top).unwrap();
    // trigger bottom 556 + FLOATING_TRIGGER_GAP; not flipped above
    assert!(
        (top - (556.0 + FLOATING_TRIGGER_GAP)).abs() < 0.5,
        "fixed y must stay below trigger, got {top}"
    );
    assert!(
        menu.style.max_size.height.is_auto(),
        "fixed strategy must not clamp max_h to remaining viewport"
    );
}

#[test]
fn underlay_close_respects_disarm_until_arm() {
    disarm_underlay_close();
    let open = Signal::new(true);
    let underlay = underlay_close_on_click(open, 1000);
    if let Some(cb) = &underlay.on_click {
        cb.borrow_mut()();
    }
    assert!(open.peek(), "disarmed underlay must not close");
    arm_underlay_close();
    if let Some(cb) = &underlay.on_click {
        cb.borrow_mut()();
    }
    assert!(!open.peek(), "armed underlay closes");
}

#[test]
fn underlay_close_arm_is_per_window_scope() {
    use crate::app::window_scope::{enter, WindowScope};
    let _p = enter(WindowScope::Primary);
    disarm_underlay_close();
    assert!(!underlay_close_is_armed());
    drop(_p);

    let _s = enter(WindowScope::Secondary(7));
    assert!(
        underlay_close_is_armed(),
        "secondary scope must not inherit primary disarm"
    );
    disarm_underlay_close();
    drop(_s);

    let _p2 = enter(WindowScope::Primary);
    assert!(
        !underlay_close_is_armed(),
        "primary disarm must survive secondary session"
    );
    arm_underlay_close();
}

fn is_zero_len(d: taffy::style::Dimension) -> bool {
    crate::layout::dim_length_px(d).is_some_and(|n| n.abs() < f32::EPSILON)
}

fn find_anchored_overlay(el: &Element) -> bool {
    let child_is_portal_floating = el.children.iter().any(|c| {
        c.attrs.get("jadix_portal").map(String::as_str) == Some("1") && floating_id_of(c).is_some()
    });
    (is_zero_len(el.style.size.width)
        && is_zero_len(el.style.size.height)
        && child_is_portal_floating)
        || el.children.iter().any(find_anchored_overlay)
}

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
fn compute_cursor_menu_prefers_bottom_right() {
    let (x, y, max_h) =
        compute_cursor_menu_placement([100.0, 80.0], [800.0, 600.0], 160.0, 120.0, 2.0);
    assert!((x - 102.0).abs() < 0.5, "x={x}");
    assert!((y - 82.0).abs() < 0.5, "y={y}");
    assert!(max_h >= 120.0, "max_h={max_h}");
}

#[test]
fn compute_cursor_menu_flips_before_translate() {
    // Near bottom-right: preferred side can't hold the menu; opposite can → flip
    // (stay attached to the cursor) instead of sliding to the viewport edge.
    let (x, y, _) =
        compute_cursor_menu_placement([740.0, 520.0], [800.0, 600.0], 160.0, 120.0, 0.0);
    assert!(
        (x - (740.0 - 160.0)).abs() < 0.5,
        "must flip left of cursor, x={x}"
    );
    assert!(
        (y - (520.0 - 120.0)).abs() < 0.5,
        "must flip above cursor, y={y}"
    );
}

#[test]
fn compute_cursor_menu_translates_when_flip_still_overflows() {
    // Menu wider than space on either side of the cursor → flip left, then translate.
    let (x, y, _) = compute_cursor_menu_placement([100.0, 40.0], [800.0, 600.0], 400.0, 80.0, 0.0);
    // Right of cursor has 692px (>400) so no horizontal flip; stays at x=100.
    assert!((x - 100.0).abs() < 0.5, "x={x}");
    assert!((y - 40.0).abs() < 0.5, "y={y}");

    // Cursor near left with a menu wider than remaining right space → translate.
    let (x2, _, _) = compute_cursor_menu_placement([50.0, 40.0], [800.0, 600.0], 760.0, 80.0, 0.0);
    assert!(
        (x2 - (800.0 - 8.0 - 760.0)).abs() < 0.5,
        "must translate after flip fails, x={x2}"
    );
}

#[test]
fn compute_cursor_menu_caps_height_when_taller_than_viewport() {
    let (x, y, max_h) =
        compute_cursor_menu_placement([40.0, 500.0], [800.0, 600.0], 160.0, 900.0, 0.0);
    assert!((x - 40.0).abs() < 0.5, "x={x}");
    // Flip above first, then translate up to the margin, then clamp max_h.
    assert!(
        y <= 8.0 + 0.5,
        "should end near top after flip+translate, y={y}"
    );
    assert!(
        max_h <= 600.0 - 16.0 + 0.5,
        "max_h must fit viewport, max_h={max_h}"
    );
    assert!(max_h >= 48.0);
}

#[test]
fn mount_anchored_overlay_is_zero_anchor_with_portal_panel() {
    let mut panel = Element::new().text("menu");
    mark_floating(&mut panel, "sel", "trig", PopupPlacement::BottomStart);
    let root = mount_anchored_overlay(panel, 1001);
    assert!(
        find_anchored_overlay(&root),
        "anchored overlay must be 0×0 host + jadix_portal + jadix_floating"
    );
    assert_eq!(
        root.style.position,
        taffy::style::Position::Absolute,
        "leftover host must be out of flow so parent flex gap is not doubled"
    );
    assert!(!portal_nests_portal(&root), "must not nest jadix_portal");
}

fn neighbor_gap_in_row(middle: Element) -> f32 {
    use crate::layout::layout_tree::LayoutTree;
    use crate::primitives::hstack;

    fn find_tid<'a>(el: &'a Element, id: &str) -> Option<&'a Element> {
        if el.template_id.as_deref() == Some(id) {
            Some(el)
        } else {
            el.children.iter().find_map(|c| find_tid(c, id))
        }
    }

    let mut a = Element::new().w(40.0).h(20.0);
    a.template_id = Some("gap-a".into());
    let mut b = Element::new().w(40.0).h(20.0);
    b.template_id = Some("gap-b".into());
    let mut row: Element = hstack().gap(8.0).child(a).child(middle).child(b).into();
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut row, 400.0, 80.0, [0.0, 0.0]);
    let aa = find_tid(&row, "gap-a")
        .unwrap_or_else(|| panic!("missing gap-a"))
        .layout_bounds;
    let bb = find_tid(&row, "gap-b")
        .unwrap_or_else(|| panic!("missing gap-b"))
        .layout_bounds;
    bb[0] - (aa[0] + aa[2])
}

#[test]
fn flow_hidden_portal_host_does_not_occupy_flex_gap() {
    let in_flow = neighbor_gap_in_row(Element::new().w(0.0).h(0.0));
    let hidden = neighbor_gap_in_row(flow_hidden_portal_host(Element::new()));
    assert!(
        (in_flow - 16.0).abs() < 1.0,
        "static 0×0 still takes a gap slot (expected ~16, got {in_flow})"
    );
    assert!(
        (hidden - 8.0).abs() < 1.0,
        "absolute 0×0 leftover must not occupy gap (expected ~8, got {hidden})"
    );
}

#[test]
fn floating_portal_anchor_does_not_occupy_flex_gap() {
    let mut panel = Element::new().text("menu");
    mark_floating(
        &mut panel,
        "gap-sel",
        "gap-trig",
        PopupPlacement::BottomStart,
    );
    let gap = neighbor_gap_in_row(floating_portal_anchor(panel));
    assert!(
        (gap - 8.0).abs() < 1.0,
        "Select/Popover leftover host must not occupy parent gap, got {gap}"
    );
}

#[test]
fn mount_viewport_stage_does_not_nest_portal() {
    let inner = swallow_clicks(Element::new().text("dialog"));
    let stage = Element::new().w("100%").h("100%").child(inner);
    let root = mount_modal_layer(
        underlay_close_on_click(Signal::new(true), 1000),
        mount_viewport_portal_stage(stage, 1001),
    );
    assert!(!portal_nests_portal(&root));
    let mut n = 0usize;
    fn count(el: &Element, n: &mut usize) {
        if el.attrs.get("jadix_portal").map(String::as_str) == Some("1") {
            *n += 1;
        }
        for c in &el.children {
            count(c, n);
        }
    }
    count(&root, &mut n);
    assert_eq!(n, 2, "underlay + stage each portal once");
}

#[test]
fn floating_panel_after_hoist_occludes_page_hits() {
    use crate::app::pointer_events::hit_path;
    use crate::app::portal_overlay::{clear_overlay_stack, portal_sync, OverlayKind, OverlayMode};
    use crate::core::reactivity::{create_root, Signal};
    use crate::core::tree::PointerEvents;
    use crate::layout::layout_tree::{hoist_portal_subtrees, ATTR_HOISTED_PORTAL, PORTAL_LAYER_ID};

    clear_overlay_stack();
    let (_owner, mut root) = create_root(|| {
        let open = Signal::new(true);
        let oz = portal_sync("cp-hit", open, OverlayKind::Popover, OverlayMode::Stacked)
            .expect("stacked popover");

        // Full-page button under the future panel silhouette.
        let under = Element::new()
            .w(400.0)
            .h(400.0)
            .bg([1.0, 0.0, 0.0, 1.0])
            .focus_id("under-btn")
            .on_click(|| {});

        // Minimal floating panel (ColorPicker / Select chrome). Gaps between
        // children must still hit the panel — not the page underneath.
        let mut panel = Element::new()
            .w(200.0)
            .h(200.0)
            .bg([0.0, 1.0, 0.0, 1.0])
            .absolute()
            .left(100.0)
            .top(100.0)
            .child(
                Element::new()
                    .w(200.0)
                    .h(40.0)
                    .bg([0.0, 0.0, 1.0, 1.0])
                    .pointer_events_none(),
            )
            .child(Element::new().w(200.0).h(40.0).bg([0.0, 0.5, 1.0, 1.0]));
        mark_floating(
            &mut panel,
            "cp-hit",
            "cp-hit:trigger",
            PopupPlacement::BottomStart,
        );
        let overlay = mount_anchored_overlay(panel, oz.panel);
        Element::new().w(400.0).h(400.0).child(under).child(overlay)
    });

    assert!(hoist_portal_subtrees(&mut root) >= 1);
    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 400.0, 400.0, [0.0, 0.0]);

    let layer = root
        .children
        .iter()
        .find(|c| c.template_id.as_deref() == Some(PORTAL_LAYER_ID))
        .expect("portal layer");
    let hoisted = layer
        .children
        .iter()
        .find(|c| c.attrs.get(ATTR_HOISTED_PORTAL).map(|s| s.as_str()) == Some("1"))
        .expect("hoisted panel");
    assert_eq!(
        hoisted.pointer_events_mode(),
        PointerEvents::Auto,
        "floating panel must claim its box after hoist"
    );
    assert!(
        hoisted.on_click.is_some(),
        "swallow_clicks must survive hoist"
    );

    // Mid-panel, between the two children (gap / chrome) — previously box-none
    // would miss here and hit `under-btn`.
    let path = hit_path(&root, 200.0, 180.0);
    assert!(
        path.iter().any(|e| {
            e.floating_id.as_deref() == Some("cp-hit")
                || e.portal_key.as_deref() == Some("panel:cp-hit")
        }),
        "hit must stay on floating panel; path={:?}",
        path.iter()
            .map(|e| (
                e.focus_id.clone(),
                e.floating_id.clone(),
                e.portal_key.clone()
            ))
            .collect::<Vec<_>>()
    );
    assert!(
        path.iter()
            .all(|e| e.focus_id.as_deref() != Some("under-btn")),
        "page control under the panel must not receive the hit"
    );
    clear_overlay_stack();
}

#[test]
fn path_hits_floating_trigger_before_menu_portal_hoists() {
    use crate::app::pointer_events::HitPathEntry;

    let root = Element::new();
    let path = [HitPathEntry {
        focus_id: Some("gal-dialog-nested-select:trigger".into()),
        ..Default::default()
    }];
    assert!(
        path_hits_floating_trigger(&root, &path),
        "opening click on SelectTrigger must count as in-bounds before menu portal mounts"
    );
}

#[test]
fn fixed_anchor_negative_gap_covers_trigger_without_flipping() {
    let trigger = [10.0, 100.0, 80.0, 32.0];
    let viewport = [800.0, 600.0];
    let (_x, y) = compute_fixed_anchor(
        trigger,
        viewport,
        80.0,
        64.0,
        PopupPlacement::BottomStart,
        -32.0,
    );
    assert!(
        (y - 100.0).abs() < 0.1,
        "bottom + negative gap: panel top sits on the trigger top (got {y})"
    );
    let (_x, y) = compute_fixed_anchor(
        trigger,
        viewport,
        80.0,
        64.0,
        PopupPlacement::TopStart,
        -32.0,
    );
    assert!(
        (y - 68.0).abs() < 0.1,
        "top + negative gap: panel bottom sits on the trigger bottom (got {y})"
    );
    let (_x, y) = compute_fixed_anchor(
        trigger,
        viewport,
        80.0,
        64.0,
        PopupPlacement::BottomStart,
        -8.0,
    );
    assert!(
        (y - 100.0).abs() < 0.1,
        "any negative gap covers using -trigger_height, not the raw gap (got {y})"
    );
    let edge = [790.0, 100.0, 80.0, 32.0];
    let (x, _y) = compute_fixed_anchor(
        edge,
        viewport,
        80.0,
        32.0,
        PopupPlacement::BottomStart,
        -32.0,
    );
    assert!(
        (x - 790.0).abs() < 0.1,
        "cover placement must not clamp away from the trigger (got {x})"
    );
}

#[test]
fn fixed_cover_top_glues_bottom_when_height_grows() {
    let mut trigger = Element::new().focus_id("rx-t");
    trigger.layout_bounds = [20.0, 200.0, 96.0, 28.0];

    let mut panel = Element::new().absolute();
    mark_floating_with_gap(
        &mut panel,
        "rx-expand",
        "rx-t",
        PopupPlacement::TopStart,
        -28.0,
    );
    panel.set_attr(ATTR_FLOATING_STRATEGY, "fixed");
    panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "1");
    // First layout: short estimate (collapsed).
    panel.layout_bounds = [0.0, 0.0, 96.0, 28.0];

    let mut root = Element::new().child(trigger).child(panel);
    assert!(sync_floating_placement(&mut root, 800.0, 600.0));
    let y0 = inset_length(root.children[1].style.inset.top).expect("top");
    assert!(
        (y0 - 200.0).abs() < 0.5,
        "cover top with h=trigger: panel top aligns with trigger top, got {y0}"
    );

    // Wrap grew the panel — bottom seam must stay on trigger bottom (228).
    root.children[1].layout_bounds = [20.0, y0, 96.0, 88.0];
    assert!(sync_floating_placement(&mut root, 800.0, 600.0));
    let y1 = inset_length(root.children[1].style.inset.top).expect("top after grow");
    assert!(
        (y1 - (228.0 - 88.0)).abs() < 0.5,
        "growing upward must keep bottom on trigger bottom, got {y1}"
    );
    assert_eq!(
        root.children[1]
            .attrs
            .get(ATTR_FLOATING_ANCHOR_EDGE_Y)
            .map(String::as_str),
        Some("228")
    );
}
