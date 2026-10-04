#[test]
fn nested_scroll_pins_fit_after_coverage_deduplication() {
    let mut pins: Vec<_> = (0..30)
        .map(|i| [100.0 + i as f32 * 2.0, 100.0, 1.0, 10.0])
        .collect();
    pins.push([100.0, 100.0, 80.0, 20.0]);
    let cull = scroll_repair_rects_physical(&[], &pins, 1.0)
        .expect("nested avatar slots must not force full viewport repaint");
    assert_eq!(
        cull.strips().collect::<Vec<_>>(),
        vec![[100.0, 100.0, 80.0, 20.0]]
    );
}

use crate::app::{damage::*, draw_cull::*};
use crate::core::tree::{Element, LayoutStatus, LayoutTree};

#[test]
fn scroll_repairs_never_pad_into_header_or_blend_overlap_twice() {
    let pin = [120.0, 64.0, 80.0, 30.0];
    let cull =
        scroll_repair_rects_physical(&[], &[pin, [140.0, 70.0, 90.0, 30.0], pin], 1.25).unwrap();
    let rects: Vec<_> = cull.strips().collect();
    assert!(
        rects.iter().all(|r| r[1] >= 80.0),
        "already-clipped repair must not enter header"
    );
    for (i, r) in rects.iter().enumerate() {
        assert!(
            rects[i + 1..]
                .iter()
                .all(|other| intersect_rects(*r, *other).is_none()),
            "overlapping holes repaint translucent avatar/text twice"
        );
    }
    // [150,80,100,38] + [175,87,113,38] - [175,87,75,31].
    assert_eq!(rects.iter().map(|r| r[2] * r[3]).sum::<f32>(), 5769.0);
}

#[test]
fn region_text_prepare_keeps_unchanged_hole_text_and_skips_distant_labels() {
    let mut root = Element::new()
        .w(300.0)
        .h(300.0)
        .flex_col()
        .child(
            Element::new()
                .w(300.0)
                .h(30.0)
                .flex_shrink(0.0)
                .text("Repair this caption"),
        )
        .child(Element::new().w(300.0).h(200.0).flex_shrink(0.0))
        .child(
            Element::new()
                .w(300.0)
                .h(30.0)
                .flex_shrink(0.0)
                .text("Idle distant caption"),
        );
    let mut tree = LayoutTree::new();
    let frame = tree.build_and_layout(&mut root, 300.0, 300.0, [-1.0, -1.0]);
    let texts = &frame.text_nodes;
    let submit = page_stack_band_submit_text(
        texts,
        texts,
        0,
        texts.len(),
        Some(CullRects::one([0.0, 0.0, 300.0, 32.0])),
        false,
    )
    .expect("region needs unchanged glyphs");
    assert_eq!(submit.len(), 1);
    assert_eq!(submit[0].text, "Repair this caption");
}

#[test]
fn stale_latest_dirty_uses_live_clip_even_after_large_scroll() {
    use crate::layout::layout_tree::NodePaint;
    // Captured from the actual header erase: dirty remained at its exit pose,
    // while the same live node moved hundreds of pixels above the viewport.
    let dirty = [485.0, 24.8, 86.0, 40.4];
    let live = [490.0, -290.2, 76.0, 30.4];
    let clip = [337.0, 60.0, 382.0, 0.0];
    let paints = [NodePaint {
        key: 4294967934,
        bounds: live,
        overlay: false,
        layered: true,
        instance: (0, 0),
        textured: (0, 0),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        rest: Some(live),
        overflow_clip: Some(clip),
    }];
    let key = paints[0].key;
    assert_eq!(node_present_overflow_clip(key, dirty, &paints), Some(clip));
    for kind in [PaintDirtyKind::Node, PaintDirtyKind::Region] {
        assert_eq!(
            node_present_scissor_logical(
                kind,
                key,
                dirty,
                &[],
                &paints,
                node_present_scissor_pad(kind)
            ),
            [0.0; 4]
        );
    }
    assert!(node_present_reset_rect(
        key,
        dirty,
        &paints,
        &[],
        &[],
        &[],
        NODE_PRESENT_RESET_PAD_PHYSICAL,
        1.0,
        None
    )
    .is_none());
}

#[test]
fn latest_empty_ancestor_clip_must_not_become_unclipped() {
    use crate::layout::layout_tree::NodePaint;
    let bounds = [485.0, 27.8, 86.0, 40.4];
    let paints = [NodePaint {
        key: 7,
        bounds,
        overlay: false,
        layered: true,
        instance: (0, 0),
        textured: (0, 0),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        rest: Some(bounds),
        overflow_clip: Some([336.0, 60.0, 384.0, 0.0]),
    }];
    assert_eq!(
        node_present_overflow_clip(7, bounds, &paints),
        paints[0].overflow_clip,
        "zero-height intersection means fully clipped, not no clipping"
    );
    let scissor =
        node_present_scissor_logical(PaintDirtyKind::Region, 7, bounds, &[], &paints, 0.0);
    assert_eq!(
        scissor, [0.0; 4],
        "offscreen Latest must not clear title pixels at y=27.8"
    );
}

#[test]
fn latest_scale_capture_descendant_keeps_screen_overflow_clip() {
    let label = Element::new().text("Latest");
    let mut button = Element::new()
        .w(90.0)
        .h(32.0)
        .bg([1.0; 4])
        .rounded(12.0)
        .absolute()
        .top(-20.0)
        .left(100.0)
        .z_index(20)
        .child(label);
    button.style.scale = [0.95, 0.95];
    button.style.opacity = 0.02;
    let viewport = Element::new()
        .w(320.0)
        .h(150.0)
        .relative()
        .overflow_y("hidden")
        .child(button);
    let mut root = Element::new()
        .w(320.0)
        .h(214.0)
        .flex_col()
        .child(
            Element::new()
                .w(320.0)
                .h(64.0)
                .flex_shrink(0.0)
                .text("Header must survive"),
        )
        .child(viewport);
    let mut tree = LayoutTree::new();
    let frame = tree.build_and_layout(&mut root, 320.0, 214.0, [-1.0, -1.0]);
    let label = &root.children[1].children[0].children[0];
    let key = paint_element_key(label);
    let slot = frame
        .node_paints
        .iter()
        .find(|p| p.key == key)
        .expect("label slot");
    assert!(
        slot.overflow_clip.is_some_and(|c| c[1] >= 64.0),
        "scale capture's source clip must not replace its screen destination clip: {:?}",
        slot.overflow_clip
    );
    let scissor = node_present_scissor_logical(
        PaintDirtyKind::Region,
        key,
        [100.0, 28.0, 90.0, 50.0],
        &[],
        &frame.node_paints,
        0.0,
    );
    assert!(
        scissor[3] == 0.0 || scissor[1] >= 64.0,
        "actual collected capture label must not clear header: {scissor:?}"
    );
    let dirty = [PaintDirty {
        key,
        bounds: [100.0, 28.0, 90.0, 50.0],
        kind: PaintDirtyKind::Node,
    }];
    let mut culled = Vec::new();
    restore_culled_dirty_clips(&root, &mut culled, &dirty);
    let scissor = node_present_scissor_logical(
        PaintDirtyKind::Node,
        key,
        dirty[0].bounds,
        &[],
        &culled,
        8.0,
    );
    assert!(
        scissor[3] == 0.0 || scissor[1] >= 64.0,
        "missing NodePaint must retain tree clip before padding: {scissor:?}"
    );
}

#[test]
fn latest_exit_region_at_viewport_top_must_not_clear_header() {
    use crate::layout::layout_tree::NodePaint;
    let slot = |key, bounds, clip| NodePaint {
        key,
        bounds,
        overflow_clip: Some(clip),
        overlay: false,
        layered: true,
        instance: (0, 0),
        textured: (0, 0),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        rest: Some(bounds),
    };
    let viewport = [20.0, 64.0, 460.0, 400.0];
    let latest = [160.0, 60.0, 100.0, 28.0];
    let paints = [
        slot(1, latest, viewport),
        slot(2, [0.0, 0.0, 500.0, 500.0], [0.0, 0.0, 500.0, 500.0]),
    ];
    let scissor = node_present_scissor_logical(
        PaintDirtyKind::Region,
        1,
        [158.0, 54.0, 104.0, 38.0],
        &[(1, latest)],
        &paints,
        0.0,
    );
    assert!(
        scissor[1] >= 64.0,
        "Latest Region must use its own viewport, not a larger unrelated clip: {scissor:?}"
    );
    let scissor = node_present_scissor_logical(
        PaintDirtyKind::Region,
        99,
        [158.0, 20.0, 104.0, 30.0],
        &[(99, latest)],
        &paints[..1],
        0.0,
    );
    assert_eq!(
        scissor, [0.0; 4],
        "culled Latest above viewport must not erase the title bar"
    );
}

#[test]
fn latest_enter_and_exit_rebuild_one_ordered_composite() {
    let mut root = Element::new()
        .w(320.0)
        .h(200.0)
        .relative()
        .bg([1.0; 4])
        .child(
            Element::new()
                .text("The scroll button is a real button — no ghost focus stops.")
                .w(280.0),
        )
        .child(
            Element::new()
                .absolute()
                .left(110.0)
                .top(36.0)
                .w(100.0)
                .h(32.0)
                .z_index(20)
                .bg([1.0; 4])
                .rounded(12.0)
                .child(Element::new().text("Latest")),
        );
    let mut tree = LayoutTree::new();
    let mut prev = Vec::new();
    for alpha in [0.15, 0.6, 1.0, 0.08, 0.0] {
        root.children[1].style.opacity = alpha;
        root.children[1].style.translate[1] = (1.0 - alpha) * 8.0;
        tree.set_collect_skip_allowed(false);
        let frame = tree.build_and_layout(&mut root, 320.0, 200.0, [-1.0, -1.0]);
        let button = &root.children[1];
        let key = paint_element_key(button);
        let bounds = crate::core::tree::visual_paint_aabb(button, [0.0, 0.0]);
        let mut dirty = vec![PaintDirty {
            key,
            bounds,
            kind: PaintDirtyKind::Node,
        }];
        prepare_node_present_dirties(
            &mut dirty,
            &frame.node_paints,
            &prev,
            &frame.instances,
            &[],
            &[],
            1.0,
            None,
        );
        assert_eq!(
            dirty[0].kind,
            PaintDirtyKind::Region,
            "alpha={alpha}: translucent stacking chrome must composite background glyphs before its own fill/text"
        );
        for (_, old) in prev.iter().filter(|(k, _)| *k == key) {
            let d = dirty[0].bounds;
            assert!(
                d[0] <= old[0]
                    && d[1] <= old[1]
                    && d[0] + d[2] >= old[0] + old[2]
                    && d[1] + d[3] >= old[1] + old[3],
                "hide must erase previous ink completely: dirty={d:?}, previous={old:?}"
            );
        }
        prev = last_present_visual_slots(
            &frame.node_paints,
            &frame.instances,
            &[],
            &frame.text_nodes,
            &[],
        );
    }
}

#[test]
fn latest_region_must_absorb_intersecting_live_text_update() {
    let mut root = Element::new()
        .w(320.0)
        .h(200.0)
        .relative()
        .bg([1.0; 4])
        .child(
            Element::new()
                .w(280.0)
                .text("announcing messages while Latest enters"),
        )
        .child(
            Element::new()
                .absolute()
                .left(100.0)
                .top(4.0)
                .w(100.0)
                .h(32.0)
                .z_index(20)
                .bg([1.0, 1.0, 1.0, 0.5])
                .child(Element::new().text("Latest")),
        );
    let mut tree = LayoutTree::new();
    let scene = tree.build_and_layout(&mut root, 320.0, 200.0, [-1.0, -1.0]);
    let mut dirty: Vec<_> = root
        .children
        .iter()
        .take(2)
        .map(|el| PaintDirty {
            key: paint_element_key(el),
            bounds: el.layout_bounds,
            kind: PaintDirtyKind::Node,
        })
        .collect();
    prepare_node_present_dirties(
        &mut dirty,
        &scene.node_paints,
        &[],
        &scene.instances,
        &[],
        &[],
        1.0,
        None,
    );
    assert!(
        dirty.iter().all(|d| d.kind == PaintDirtyKind::Region),
        "a live caption intersecting Latest must not flush through the later Node capture pass: {dirty:?}"
    );
}

#[test]
fn region_escalation_must_happen_before_glyph_capture() {
    let mut root = Element::new()
        .w(200.0)
        .h(40.0)
        .bg([1.0, 1.0, 1.0, 1.0])
        .text("abcdefghijklmnopqrstuvwxyz");
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 200.0, 100.0, [-1.0, -1.0]);
    let key = u64::MAX;
    let bounds = [20.0, 0.0, 20.0, 20.0];
    let mut dirties = vec![PaintDirty {
        key,
        bounds,
        kind: PaintDirtyKind::Node,
    }];
    // Production frame preparation resolves tiers before any text capture.
    prepare_node_present_dirties(
        &mut dirties,
        &result.node_paints,
        &[(key, bounds)],
        &result.instances,
        &[],
        &[],
        1.0,
        None,
    );
    let prepared = node_present_clipped_capture_text_wipes(
        &dirties,
        &result.text_nodes,
        &result.node_paints,
        &[],
    );
    assert_eq!(dirties[0].kind, PaintDirtyKind::Region);
    let required = node_present_clipped_capture_text_wipes(
        &dirties,
        &result.text_nodes,
        &result.node_paints,
        &[],
    );
    assert!(
        !required.is_empty(),
        "the erased region intersects a live caption"
    );
    assert_eq!(
        prepared.len(),
        required.len(),
        "promoting after text prepare leaves region-cleared caption pixels missing"
    );
}

#[test]
fn node_reset_miss_without_background_must_not_stay_node() {
    let mut root = Element::new()
        .w(200.0)
        .h(100.0)
        .child(Element::new().w(40.0).h(20.0).bg([1.0, 0.0, 0.0, 0.5]));
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 200.0, 100.0, [-1.0, -1.0]);
    let child = &root.children[0];
    let key = paint_element_key(child);
    let bounds = child.layout_bounds;
    let mut dirties = vec![PaintDirty {
        key,
        bounds,
        kind: PaintDirtyKind::Node,
    }];
    prepare_node_present_dirties(
        &mut dirties,
        &result.node_paints,
        &[(key, bounds)],
        &result.instances,
        &[],
        &[],
        1.0,
        None,
    );
    let reset = node_present_dest_reset_full(
        key,
        bounds,
        &result.node_paints,
        &result.instances,
        &[],
        &[],
        1.0,
        Some(bounds),
    );
    assert!(
        reset.is_some() || dirties[0].kind != PaintDirtyKind::Node,
        "existing slot has no recoverable backdrop, but stays Node with reset=None; next frame src-overs old ink"
    );
}

#[test]
fn finished_css_animation_must_stop_requesting_frames() {
    use crate::style::*;
    set_css_transition_virtual_timeline(true);
    set_css_transition_timeline_ms(0.0);
    let mut root = Element::new().w(40.0).h(20.0).bg([1.0, 0.0, 0.0, 1.0]);
    root.style.animations.push(AnimationEntry {
        name: "fade-out".into(),
        duration_ms: 10.0,
        iteration_count: AnimationIterationCount::Finite(1.0),
        fill_mode: AnimationFillMode::Forwards,
        ..Default::default()
    });
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 200.0, 100.0, [-1.0, -1.0]);
    for i in 0..5 {
        set_css_transition_timeline_ms(i as f64 * 20.0);
        utility::set_frame_dt(0.02);
        let mut impact = LayoutStatus::Clean;
        sync_class_variants_subtree(
            &mut root,
            [-1.0, -1.0],
            "",
            false,
            200.0,
            100.0,
            &mut tree,
            &mut impact,
        );
    }
    assert!(root.animation_snapshot.runs[0].finished);
    // A cached ancestor/live flag can survive the final animation tick.
    root.collect_skip_live = true;
    let mut impact = LayoutStatus::Clean;
    sync_class_variants_subtree(
        &mut root,
        [-1.0, -1.0],
        "",
        false,
        200.0,
        100.0,
        &mut tree,
        &mut impact,
    );
    assert!(
        !any_page_css_needs_frame(),
        "finished animation still arms the app frame pump"
    );
}

#[test]
fn hover_opacity_zero_must_queue_erase() {
    let _ = take_frame_paint_nodes();
    let mut leaf = Element::new().w(40.0).h(20.0).bg([1.0, 0.0, 0.0, 1.0]);
    crate::style::apply_class_list(&mut leaf, "hover:opacity-0");
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut leaf, 200.0, 100.0, [-1.0, -1.0]);
    let mut impact = LayoutStatus::Clean;
    crate::style::sync_class_variants_subtree(
        &mut leaf,
        [-1.0, -1.0],
        "",
        false,
        200.0,
        100.0,
        &mut tree,
        &mut impact,
    );
    let _ = take_frame_paint_nodes();
    crate::style::sync_class_variants_subtree(
        &mut leaf,
        [10.0, 10.0],
        "",
        false,
        200.0,
        100.0,
        &mut tree,
        &mut impact,
    );
    assert_eq!(leaf.style.opacity, 0.0, "hover must actually hide the leaf");
    assert!(
        !take_frame_paint_nodes().is_empty(),
        "visible-to-transparent change must erase prior retained pixels"
    );
}

#[test]
fn dirty_queue_must_be_scoped_to_owning_window() {
    let _ = take_frame_paint_nodes();
    let primary = crate::app::window_scope::enter(crate::app::window_scope::WindowScope::Primary);
    push_frame_paint_key(123, [10.0, 10.0, 20.0, 20.0]);
    let secondary =
        crate::app::window_scope::enter(crate::app::window_scope::WindowScope::Secondary(42));
    push_frame_paint_key(123, [100.0, 100.0, 20.0, 20.0]);
    drop(secondary);
    let own = take_frame_paint_nodes();
    assert_eq!(
        own[0].bounds,
        [10.0, 10.0, 20.0, 20.0],
        "secondary dirty must not coalesce with primary NodeId"
    );
    let secondary =
        crate::app::window_scope::enter(crate::app::window_scope::WindowScope::Secondary(42));
    let own = take_frame_paint_nodes();
    assert_eq!(own.len(), 1);
    assert_eq!(own[0].bounds, [100.0, 100.0, 20.0, 20.0]);
    drop(secondary);
    drop(primary);
}

#[test]
fn node_glyph_capture_preserves_both_sides_of_portal() {
    let mut root = Element::new()
        .w(200.0)
        .h(32.0)
        .text("abcdefghijklmnopqrstuv");
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 200.0, 80.0, [-1.0, -1.0]);
    let dirty = [PaintDirty {
        key: paint_element_key(&root),
        bounds: root.layout_bounds,
        kind: PaintDirtyKind::Node,
    }];
    let overlay = [80.0, -8.0, 20.0, root.layout_bounds[3] + 16.0];
    let capture = node_present_clipped_capture_text_wipes_excluding_overlay(
        &dirty,
        &result.text_nodes,
        &result.node_paints,
        &[],
        &[(1, overlay)],
    );
    assert_eq!(
        capture.len(),
        2,
        "both exposed text fragments must be prepared"
    );
    assert!(capture
        .iter()
        .all(|n| intersect_rects(n.clip_bounds.unwrap(), overlay).is_none()));
    assert!(capture.iter().any(|n| n.clip_bounds.unwrap()[0] < 80.0));
    assert!(capture.iter().any(|n| n.clip_bounds.unwrap()[0] >= 100.0));
    assert!(
        capture
            .iter()
            .all(|n| n.x == result.text_nodes[0].x && n.text == result.text_nodes[0].text),
        "scissor fragments must keep original shaping origin and content"
    );
}

#[test]
fn paused_and_cancelled_players_stop_pumping_and_resume_rearms() {
    use crate::style::*;
    let anim = Animation::from_effect(KeyframeEffect::named("spin").duration(1000.0));
    let mut root = Element::new().w(40.0).h(20.0);
    root.style.animations.push(anim.to_entry());
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 200.0, 80.0, [-1.0, -1.0]);
    anim.pause();
    let mut impact = LayoutStatus::Clean;
    sync_class_variants_subtree(
        &mut root,
        [-1.0, -1.0],
        "",
        false,
        200.0,
        80.0,
        &mut tree,
        &mut impact,
    );
    assert!(!any_page_css_needs_frame());
    anim.play();
    sync_class_variants_subtree(
        &mut root,
        [-1.0, -1.0],
        "",
        false,
        200.0,
        80.0,
        &mut tree,
        &mut impact,
    );
    assert!(any_page_css_needs_frame());
    anim.cancel();
    for _ in 0..2 {
        sync_class_variants_subtree(
            &mut root,
            [-1.0, -1.0],
            "",
            false,
            200.0,
            80.0,
            &mut tree,
            &mut impact,
        );
    }
    assert!(!any_page_css_needs_frame());
}

#[test]
fn overlapping_portal_repair_holes_replay_each_pixel_once() {
    let mut holes = Vec::new();
    push_disjoint_rect(&mut holes, [0.0, 0.0, 20.0, 20.0]);
    push_disjoint_rect(&mut holes, [10.0, 10.0, 20.0, 20.0]);
    push_disjoint_rect(&mut holes, [0.0, 0.0, 20.0, 20.0]);
    assert_eq!(holes.iter().map(|r| r[2] * r[3]).sum::<f32>(), 700.0);
    for (i, a) in holes.iter().enumerate() {
        assert!(holes[i + 1..]
            .iter()
            .all(|b| intersect_rects(*a, *b).is_none()));
    }
}

#[test]
fn offscreen_animation_preserves_retained_page_without_preparing_labels() {
    let nodes = [
        PaintDirty {
            key: 1,
            bounds: [306.0, -4662.0, 250.0, 16.0],
            kind: PaintDirtyKind::Region,
        },
        PaintDirty {
            key: 2,
            bounds: [306.0, 3177.4, 56.0, 56.0],
            kind: PaintDirtyKind::Region,
        },
    ];
    let viewport = [0.0, 0.0, 800.0, 600.0];
    assert_eq!(
        frame_clear_policy_for_nodes(true, false, &nodes, viewport),
        FrameClearPolicy::PartialLoad
    );
    assert_eq!(
        frame_clear_policy_for_nodes(true, true, &nodes, viewport),
        FrameClearPolicy::FullClear
    );
    assert_eq!(
        frame_clear_policy_for_nodes(false, false, &nodes, viewport),
        FrameClearPolicy::FullClear
    );
    let mut root = Element::new()
        .w(300.0)
        .h(100.0)
        .text("Visible avatar caption");
    let mut tree = LayoutTree::new();
    let frame = tree.build_and_layout(&mut root, 300.0, 100.0, [-1.0, -1.0]);
    assert!(!frame.text_nodes.is_empty());
    assert!(page_stack_band_submit_text(
        &frame.text_nodes,
        &frame.text_nodes,
        0,
        frame.text_nodes.len(),
        Some(region_repair_text_cull(&nodes, &[], &[], 1.0, 800, 600)),
        false
    )
    .is_none());
}

#[test]
fn closing_last_avatar_portal_restores_its_painted_halo() {
    let stamp = [297.0, 75.6, 126.5, 32.75];
    let visual = [277.0, 55.6, 166.5, 72.75];
    assert_eq!(
        overlay_restore_leftover_coverage(&[(7, stamp)], &[], Some(visual)),
        vec![(0, visual)]
    );
    let sibling = (8, [500.0, 100.0, 120.0, 80.0]);
    assert_eq!(
        overlay_restore_leftover_coverage(&[(7, stamp), sibling], &[sibling], Some(visual)),
        vec![(7, stamp)]
    );
}

#[test]
fn culled_metadata_without_history_does_not_reconstruct_a_backdrop() {
    use crate::layout::layout_tree::NodePaint;
    let bounds = [120.0, -250.0, 80.0, 32.0];
    let paints = [NodePaint {
        key: 7,
        bounds,
        overlay: false,
        layered: true,
        instance: (0, 0),
        textured: (0, 0),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        rest: Some(bounds),
        overflow_clip: Some([0.0, 60.0, 800.0, 540.0]),
    }];
    let mut dirty = vec![PaintDirty {
        key: 7,
        bounds,
        kind: PaintDirtyKind::Node,
    }];
    prepare_node_present_dirties(&mut dirty, &paints, &[], &[], &[], &[], 1.0, None);
    assert_eq!(dirty[0].kind, PaintDirtyKind::Node);
    assert_eq!(dirty[0].bounds, bounds);
    let previous = [(7, [120.0, 80.0, 80.0, 32.0])];
    prepare_node_present_dirties(&mut dirty, &paints, &previous, &[], &[], &[], 1.0, None);
    assert_eq!(
        dirty[0].kind,
        PaintDirtyKind::Region,
        "previous visible pixels must still be restored"
    );
}

#[test]
fn repair_at_line_box_edge_keeps_glyph_coverage_without_widening_clip() {
    let mut root = Element::new().w(200.0).h(20.0).text("Header caption");
    let mut tree = LayoutTree::new();
    let frame = tree.build_and_layout(&mut root, 200.0, 20.0, [-1.0, -1.0]);
    let mut text = frame.text_nodes[0].clone();
    text.clip_bounds = None;
    let edge = text.y + text.height;
    let hole = [text.x, edge, text.width, 1.0];
    let repair = clip_text_nodes_to_rect(&[text.clone()], hole);
    assert_eq!(
        repair.len(),
        1,
        "raster fringe can occupy the first row past the line box"
    );
    assert_eq!(repair[0].clip_bounds, Some(hole));
    text.clip_bounds = Some([text.x, text.y, text.width, text.height]);
    assert!(
        clip_text_nodes_to_rect(&[text], hole).is_empty(),
        "an actual ancestor clip remains hard"
    );
}

#[test]
fn plain_input_blur_repairs_selection_not_only_ibeam() {
    use crate::components::input::{
        focus_id_for_input, input_selection_signal_key, input_value_signal_key,
    };
    use crate::core::reactivity::{create_root, use_signal};
    create_root(|| {
        let id = "plain-selection-blur-regression";
        use_signal(&input_value_signal_key(id), String::new).set("1111111".into());
        use_signal(&input_selection_signal_key(id), || None::<(usize, usize)>).set(Some((0, 7)));
        let focus = use_signal("__rui_button_focus_id", String::new);
        focus.set(focus_id_for_input(id));
        let mut root: Element = crate::components::input::input(id)
            .plain_chrome(true)
            .into();
        let mut tree = LayoutTree::new();
        tree.build_and_layout(&mut root, 300.0, 40.0, [-1.0, -1.0]);
        let mut status = LayoutStatus::Clean;
        crate::components::internal::text_edit::sync_text_edit_paint_subtree(
            &mut root,
            &mut tree,
            &mut status,
        );
        let _ = take_frame_paint_nodes();
        focus.set(String::new());
        crate::components::internal::text_edit::sync_text_edit_paint_subtree(
            &mut root,
            &mut tree,
            &mut status,
        );
        assert!(
            take_frame_paint_nodes()
                .iter()
                .any(|d| d.kind == PaintDirtyKind::Region && d.bounds[2] > 10.0),
            "blur must erase retained selection highlight even when the caret blink is off"
        );
    });
}

#[test]
fn cached_scroll_viewport_keeps_metrics_for_next_scroll_repair() {
    let mut viewport = Element::new()
        .w(300.0)
        .h(120.0)
        .overflow_y("scroll")
        .child(Element::new().w(300.0).h(800.0));
    viewport.focus_id = Some(crate::core::scroll_viewport::focus_id_for_scroll_viewport(
        "metric-replay",
    ));
    let mut root = Element::new().w(300.0).h(120.0).child(viewport);
    let mut tree = LayoutTree::new();
    let initial = tree.build_and_layout(&mut root, 300.0, 120.0, [-1.0, -1.0]);
    assert!(initial
        .scroll_metrics
        .iter()
        .any(|(id, ..)| id == "metric-replay"));
    for _ in 0..3 {
        let next = tree.build_and_layout_fast(&mut root, 300.0, 120.0, [-1.0, -1.0], true, false);
        assert!(
            next.scroll_metrics
                .iter()
                .any(|(id, ..)| id == "metric-replay"),
            "cached frame lost the viewport needed by scroll blit"
        );
    }
}
#[test]
fn paint_synced_input_refreshes_warmed_idle_cache() {
    use crate::components::internal::text_edit::sync_text_edit_paint_subtree;
    use crate::core::reactivity::use_signal;
    use crate::core::tree::{Element, LayoutStatus};
    use crate::layout::layout_tree::LayoutTree;
    for multiline in [false, true] {
        let id = if multiline {
            "cache-textarea"
        } else {
            "cache-input"
        };
        let field: Element = if multiline {
            crate::components::textarea::textarea(id)
                .placeholder("placeholder")
                .into()
        } else {
            crate::components::input::input(id)
                .clearable(true)
                .placeholder("placeholder")
                .into()
        };
        let mut root = Element::new().w(320.0).h(160.0).child(field);
        let mut tree = LayoutTree::new();
        tree.build_and_layout(&mut root, 320.0, 160.0, [-1.0; 2]);
        for _ in 0..3 {
            sync_text_edit_paint_subtree(&mut root, &mut tree, &mut LayoutStatus::Clean);
            tree.set_collect_skip_allowed(true);
            tree.build_and_layout_fast(&mut root, 320.0, 160.0, [-1.0; 2], true, true);
        }
        use_signal(
            &crate::utils::text_field::input_value_signal_key(id),
            String::new,
        )
        .set("typed".into());
        sync_text_edit_paint_subtree(&mut root, &mut tree, &mut LayoutStatus::Clean);
        let result = tree.build_and_layout_fast(&mut root, 320.0, 160.0, [-1.0; 2], true, true);
        assert!(
            result.text_nodes.iter().any(|t| t.text == "typed"),
            "paint sync must invalidate cached ancestor, multiline={multiline}"
        );
        assert!(!result.text_nodes.iter().any(|t| t.text == "placeholder"));
    }
}

#[test]
fn replay_preserves_descendant_paint_membership() {
    use crate::core::tree::Element;
    use crate::layout::layout_tree::LayoutTree;
    let mut root = Element::new().w(300.0).h(100.0).child(
        Element::new()
            .w(240.0)
            .h(80.0)
            .child(Element::new().w(120.0).h(30.0).text("caption")),
    );
    let mut tree = LayoutTree::new();
    tree.build_and_layout(&mut root, 300.0, 100.0, [-1.0; 2]);
    tree.set_collect_skip_allowed(true);
    let initial = tree.build_and_layout_fast(&mut root, 300.0, 100.0, [-1.0; 2], false, false);
    let replay = tree.build_and_layout_fast(&mut root, 300.0, 100.0, [-1.0; 2], true, false);
    assert!(tree.last_collect_skip_counts().0 > 0);
    for p in &initial.node_paints {
        assert!(
            replay
                .node_paints
                .iter()
                .any(|q| q.key == p.key && q.text == p.text),
            "replay lost descendant paint slot: {p:?}"
        );
    }
}

#[test]
fn scroll_replay_refreshes_descendant_paint_coordinates() {
    let mut root = Element::new().w(300.0).h(100.0).overflow_y("scroll").child(
        Element::new().w(240.0).h(180.0).flex_shrink(0.0)
            .child(Element::new().w(180.0).h(50.0).mt(30.0)
                .child(Element::new().w(120.0).h(30.0).text("caption"))),
    );
    let mut tree = LayoutTree::new();
    tree.build_and_layout(&mut root, 300.0, 100.0, [-1.0; 2]);
    tree.set_collect_skip_allowed(true);
    tree.build_and_layout_fast(&mut root, 300.0, 100.0, [-1.0; 2], false, false);
    root.scroll_offset = [0.0, 10.0];
    tree.build_and_layout_fast(&mut root, 300.0, 100.0, [-1.0; 2], true, false);
    assert!(tree.last_collect_skip_counts().0 > 0);
    assert_eq!(root.children[0].children[0].children[0].paint_scroll_acc, [0.0, 10.0],
        "damage after replay must use the same scroll coordinates as the replayed glyphs");
}

#[test]
fn checked_sync_refreshes_cached_scrolled_control() {
    use crate::core::reactivity::use_signal;
    use crate::core::tree::{Element, LayoutStatus};
    use crate::layout::layout_tree::LayoutTree;
    let checked = use_signal("cache-checked", || false);
    let mut root = Element::new()
        .w(200.0)
        .h(100.0)
        .child(crate::components::checkbox::checkbox("cache-checked-box").checked_signal(checked));
    let mut tree = LayoutTree::new();
    tree.build_and_layout(&mut root, 200.0, 100.0, [-1.0; 2]);
    tree.set_collect_skip_allowed(true);
    let before = tree.build_and_layout_fast(&mut root, 200.0, 100.0, [-1.0; 2], true, true);
    checked.set(true);
    crate::components::checkbox::sync_checkbox_checked_subtree(
        &mut root,
        &mut tree,
        &mut LayoutStatus::Clean,
    );
    let after = tree.build_and_layout_fast(&mut root, 200.0, 100.0, [-1.0; 2], true, true);
    assert_ne!(
        before.instances[0].color, after.instances[0].color,
        "checked chrome must not replay unchecked cache"
    );
}

#[test]
fn textarea_horizontal_overlay_track_fills_live_width() {
    use crate::core::tree::Element;
    use crate::layout::layout_tree::LayoutTree;
    let field: Element = crate::components::textarea::textarea("full-width-track")
        .wrap("off")
        .into();
    let mut root = Element::new().w(360.0).h(150.0).child(field);
    let mut tree = LayoutTree::new();
    tree.build_and_layout(&mut root, 360.0, 150.0, [-1.0; 2]);
    let track = crate::core::tree::find_in_tree(&root, |n| {
        n.focus_id.as_deref().is_some_and(|id| {
            id.starts_with(crate::components::scrollbar::SCROLLBAR_TRACK_FOCUS_PREFIX)
                && id.ends_with(":x")
        })
    })
    .unwrap();
    assert!(
        track.layout_bounds[2] >= 340.0,
        "horizontal track must fill the field, got {:?}",
        track.layout_bounds
    );
}

#[test]
fn cancelled_selection_move_queues_ghost_erase() {
    use crate::core::tree::Element;
    use crate::layout::layout_tree::LayoutTree;
    use crate::utils::text_sel_move::*;
    begin_potential(
        "ghost-erase",
        (0, 4),
        "text".into(),
        2,
        (0.0, 0.0),
        14.0,
        400,
        [0, 0, 0, 255],
        20.0,
    );
    tick_pointer((20.0, 20.0), Some(2), 4);
    let mut root = Element::new().w(200.0).h(100.0);
    let mut tree = LayoutTree::new();
    let painted = tree.build_and_layout(&mut root, 200.0, 100.0, [-1.0; 2]);
    let ghost = painted
        .text_nodes
        .iter()
        .find(|t| t.template_id.as_deref() == Some("__rui_text_sel_move_ghost"))
        .unwrap();
    crate::app::damage::clear_frame_paint_nodes();
    assert!(matches!(on_release(), ReleaseAction::KeepSelection));
    let dirty = crate::app::damage::take_frame_paint_nodes();
    assert!(
        dirty.iter().any(|d| d.bounds[0] <= ghost.x
            && d.bounds[1] <= ghost.y
            && d.bounds[0] + d.bounds[2] >= ghost.x + ghost.width
            && d.bounds[1] + d.bounds[3] >= ghost.y + ghost.height),
        "release must erase unchanged selection ghost: {dirty:?}"
    );
}

#[test]
fn reactive_structure_invalidates_idle_ancestor_cache() {
    use crate::core::reactivity::use_signal;
    let changed = use_signal("structure-cache-change", || false);
    let child = Element::new().w(100.0).h(30.0).reactive_sync(move |el, _| {
        if changed.peek() && el.children.is_empty() {
            el.children.push(Element::new().text("new child"));
            LayoutStatus::StructureChanged
        } else {
            LayoutStatus::Clean
        }
    });
    let mut root = Element::new().w(300.0).h(100.0).child(child);
    let mut tree = LayoutTree::new();
    tree.build_and_layout(&mut root, 300.0, 100.0, [-1.0; 2]);
    tree.set_collect_skip_allowed(true);
    tree.build_and_layout_fast(&mut root, 300.0, 100.0, [-1.0; 2], true, true);
    changed.set(true);
    crate::core::tree::sync_reactive_elements_subtree(
        &mut root,
        &mut tree,
        &mut LayoutStatus::Clean,
        300.0,
        100.0,
        [0.0; 2],
    );
    assert_eq!(
        root.collect_paint_hash, 0,
        "structure changes must invalidate cached ancestors"
    );
}

#[test]
fn page_with_portal_descendant_is_not_an_overlay_only_island() {
    let mut portal = Element::new();
    portal.set_attr("rui_portal", "1");
    let mut page = Element::new()
        .child(Element::new().text("Forms"))
        .child(portal);
    page.layout_bounds = [200.0, 60.0, 700.0, 800.0];
    assert!(crate::layout::layout_tree::subtree_declares_portal(&page));
    assert!(!crate::layout::layout_tree::subtree_is_portal_only(&page));
    assert!(crate::layout::layout_tree::subtree_is_portal_only(
        &Element::new().child(page.children.pop().unwrap())
    ));
}

#[test]
fn hiding_flow_content_invalidates_moved_siblings() {
    crate::app::damage::clear_frame_paint_nodes();
    let child = Element::new().reactive_sync(|element, _| {
        element.style.display = taffy::style::Display::None;
        LayoutStatus::LayoutChanged
    });
    let mut root = Element::new()
        .child(child)
        .child(Element::new().text("Footer"));
    root.layout_bounds = [0.0, 0.0, 300.0, 300.0];
    root.children[0].layout_bounds = [0.0, 0.0, 300.0, 200.0];
    let mut collect = crate::core::tree::ReactiveSyncCollect::default();
    crate::core::tree::sync_reactive_elements_subtree_ex(
        &mut root,
        &mut LayoutTree::new(),
        &mut LayoutStatus::Clean,
        300.0,
        300.0,
        [0.0; 2],
        false,
        &mut collect,
    );
    let damage = crate::app::damage::take_frame_paint_nodes();
    assert!(damage.iter().any(|dirty| dirty.bounds == [0.0, 0.0, 300.0, 300.0]),
        "the footer's old position also needs erasing");
}

#[test]
fn overlay_knob_layout_damage_keeps_overlay_identity() {
    let mut knob_area = Element::new().reactive_sync(|_, _| LayoutStatus::LayoutChanged);
    knob_area.layout_bounds = [40.0, 50.0, 192.0, 144.0];
    knob_area.paint_overlay_stamp = Some(73);
    crate::app::damage::clear_frame_paint_nodes();
    let mut collect = crate::core::tree::ReactiveSyncCollect::default();
    crate::core::tree::sync_reactive_elements_subtree_ex(
        &mut knob_area,
        &mut LayoutTree::new(),
        &mut LayoutStatus::Clean,
        400.0,
        300.0,
        [0.0; 2],
        true,
        &mut collect,
    );
    let damage = crate::app::damage::take_frame_paint_nodes();
    assert!(collect.portal_layout_changed);
    assert!(!collect.page_layout_changed);
    assert_eq!(damage.len(), 1);
    assert_eq!(damage[0].key, 73);
    assert_eq!(damage[0].kind, crate::app::damage::PaintDirtyKind::Overlay);
}

#[test]
fn scrolled_structure_damage_uses_the_element_visual_box() {
    let child = Element::new().reactive_sync(|el, _| {
        el.children.push(Element::new().text("updated"));
        LayoutStatus::StructureChanged
    });
    let mut root = Element::new().overflow_y("hidden").child(child);
    root.layout_bounds = [10.0, 150.0, 200.0, 60.0];
    root.children[0].layout_bounds = [10.0, 150.0, 100.0, 30.0];
    let mut tree = LayoutTree::new();
    crate::app::damage::clear_frame_paint_nodes();
    crate::core::tree::sync_reactive_elements_subtree(
        &mut root,
        &mut tree,
        &mut LayoutStatus::Clean,
        400.0,
        300.0,
        [0.0, 100.0],
    );
    let damage = crate::app::damage::take_frame_paint_nodes();
    assert!(
        damage
            .iter()
            .all(|d| d.kind == crate::app::damage::PaintDirtyKind::Region),
        "replaced children require region reconstruction, not a node capture: {damage:?}"
    );
    assert!(
        damage.iter().any(|d| d.bounds == [10.0, 50.0, 100.0, 30.0]),
        "structural erase is the changed element in visual space, not the ancestor clip: {damage:?}"
    );
}
