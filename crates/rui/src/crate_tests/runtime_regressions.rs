use crate::app::{damage::*, draw_cull::*};
use crate::core::tree::{Element, LayoutStatus, LayoutTree};

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
        let mut dirty = [PaintDirty {
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
        assert_eq!(dirty[0].kind,PaintDirtyKind::Region,
            "alpha={alpha}: translucent stacking chrome must composite background glyphs before its own fill/text");
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
    assert!(dirty.iter().all(|d|d.kind==PaintDirtyKind::Region),
        "a live caption intersecting Latest must not flush through the later Node capture pass: {dirty:?}");
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
    let mut dirties = [PaintDirty {
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
    let mut dirties = [PaintDirty {
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
    assert!(reset.is_some() || dirties[0].kind != PaintDirtyKind::Node,
        "existing slot has no recoverable backdrop, but stays Node with reset=None; next frame src-overs old ink");
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
        Some(overlay),
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
