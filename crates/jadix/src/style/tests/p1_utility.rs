use super::*;

#[test]
fn apply_aspect_ratio_utility() {
    let mut el = Element::new();
    let impact = apply_class_list(&mut el, "aspect-square");
    assert_eq!(impact, LayoutStatus::LayoutChanged);
    assert_eq!(el.style.aspect_ratio, Some(1.0));

    let mut el = Element::new();
    apply_class_list(&mut el, "aspect-[16/9]");
    assert!((el.style.aspect_ratio.unwrap() - 16.0 / 9.0).abs() < 0.001);
}

#[test]
fn apply_fixed_and_rtl() {
    let mut el = Element::new();
    apply_class_list(&mut el, "fixed");
    assert!(el.style.position_fixed);
    assert_eq!(el.style.position, taffy::style::Position::Absolute);

    let mut el = Element::new().text("x");
    apply_class_list(&mut el, "rtl");
    assert_eq!(el.text_style.direction, crate::style::TextDirection::Rtl);
}

#[test]
fn apply_negative_margin() {
    let mut el = Element::new();
    let impact = apply_class_list(&mut el, "-m-4");
    assert_eq!(impact, LayoutStatus::LayoutChanged);
    assert_eq!(
        el.style.margin.left,
        taffy::style::LengthPercentageAuto::length(-16.0)
    );
    assert_eq!(
        el.style.margin.top,
        taffy::style::LengthPercentageAuto::length(-16.0)
    );
}

#[test]
fn apply_transition_and_animate_attrs() {
    let mut el = Element::new();
    apply_class_list(&mut el, "transition-opacity");
    assert!(
        el.style
            .transition
            .iter()
            .any(|e| e.property == crate::style::TransitionProperty::Opacity),
        "transition-opacity must register a CSS opacity tween, got {:?}",
        el.style.transition
    );

    let mut el = Element::new();
    apply_class_list(&mut el, "animate-spin");
    assert_eq!(
        el.attrs.get("jadix_animate").map(|s| s.as_str()),
        Some("spin")
    );
    assert_eq!(el.style.animations.len(), 1);
    assert_eq!(el.style.animations[0].name, "spin");

    let mut el = Element::new();
    apply_class_list(&mut el, "transition-transform animate-none");
    assert!(
        el.style
            .transition
            .iter()
            .any(|e| e.property == crate::style::TransitionProperty::Transform),
        "transition-transform must register a CSS transform tween, got {:?}",
        el.style.transition
    );
    assert!(el.style.animations.is_empty());
    assert_eq!(
        el.attrs.get("jadix_animate").map(|s| s.as_str()),
        Some("none")
    );
    // Must not also set `jadix_transition` — that dual-drove UI springs against the
    // CSS tween and kept gallery `hover:scale-*` tiles dirty while hovering the sidebar.
    assert!(el.attrs.get("jadix_transition").is_none());
}

#[test]
fn apply_shimmer_utility() {
    let mut el = Element::new();
    apply_class_list(&mut el, "shimmer");
    assert_eq!(
        el.attrs
            .get(crate::renderer::ATTR_GLYPH_FX)
            .map(|s| s.as_str()),
        Some("shimmer")
    );
    assert_eq!(
        el.attrs.get("jadix_animate").map(|s| s.as_str()),
        Some("tw-shimmer")
    );
    assert_eq!(el.style.animations.len(), 1);
    assert_eq!(el.style.animations[0].name, "tw-shimmer");
    assert_eq!(el.style.animations[0].duration_ms, 2000.0);
    assert!(matches!(
        el.style.animations[0].iteration_count,
        crate::style::AnimationIterationCount::Infinite
    ));

    apply_class_list(&mut el, "shimmer shimmer-duration-1000 shimmer-reverse");
    assert_eq!(el.style.animations[0].duration_ms, 1000.0);
    assert_eq!(
        el.style.animations[0].direction,
        crate::style::AnimationDirection::Reverse
    );
    assert_eq!(
        el.attrs
            .get(crate::renderer::ATTR_SHIMMER_REVERSE)
            .map(|s| s.as_str()),
        Some("1")
    );

    apply_class_list(&mut el, "shimmer shimmer-count-3");
    assert!(matches!(
        el.style.animations[0].iteration_count,
        crate::style::AnimationIterationCount::Finite(n) if (n - 3.0).abs() < f32::EPSILON
    ));
    assert_eq!(
        el.attrs
            .get(crate::renderer::ATTR_SHIMMER_COUNT)
            .map(|s| s.as_str()),
        Some("3")
    );

    apply_class_list(&mut el, "shimmer shimmer-none");
    assert_eq!(
        el.attrs
            .get(crate::renderer::ATTR_GLYPH_FX)
            .map(|s| s.as_str()),
        Some("")
    );
    assert!(el.style.animations.iter().all(|e| e.name != "tw-shimmer"));
}

#[test]
fn apply_shimmer_box_utility() {
    let mut el = Element::new();
    apply_class_list(&mut el, "shimmer-box shimmer-duration-800 shimmer-count-2");
    assert_eq!(
        el.attrs
            .get(crate::renderer::ATTR_SHIMMER_BOX)
            .map(|s| s.as_str()),
        Some("1")
    );
    assert!(el.attrs.get(crate::renderer::ATTR_GLYPH_FX).is_none());
    assert_eq!(el.style.animations[0].name, "tw-shimmer");
    assert_eq!(el.style.animations[0].duration_ms, 800.0);
    assert!(matches!(
        el.style.animations[0].iteration_count,
        crate::style::AnimationIterationCount::Finite(n) if (n - 2.0).abs() < f32::EPSILON
    ));

    let mut el = Element::new();
    apply_class_list(&mut el, "animate-shimmer");
    assert_eq!(
        el.attrs
            .get(crate::renderer::ATTR_SHIMMER_BOX)
            .map(|s| s.as_str()),
        Some("1")
    );

    apply_class_list(&mut el, "animate-shimmer shimmer-none");
    assert!(el.attrs.get(crate::renderer::ATTR_SHIMMER_BOX).is_none());
}

#[test]
fn shimmer_class_inherits_to_text_leaf() {
    let mut root = Element::new();
    apply_class_list(&mut root, "shimmer shimmer-duration-1100");
    root.children.push(Element::new().text("Thinking..."));
    crate::layout::layout_tree::cascade_inheritable_text_style_for_test(&mut root);
    let leaf = &root.children[0];
    assert_eq!(
        leaf.attrs
            .get(crate::renderer::ATTR_GLYPH_FX)
            .map(|s| s.as_str()),
        Some("shimmer")
    );
    assert_eq!(
        leaf.attrs
            .get(crate::renderer::ATTR_SHIMMER_DURATION_MS)
            .map(|s| s.as_str()),
        Some("1100")
    );
}

#[test]
fn shimmer_box_does_not_inherit_to_children() {
    let mut root = Element::new();
    apply_class_list(&mut root, "shimmer-box shimmer-count-3");
    root.children.push(Element::new().text("Thinking..."));
    crate::layout::layout_tree::cascade_inheritable_text_style_for_test(&mut root);
    let leaf = &root.children[0];
    assert!(leaf.attrs.get(crate::renderer::ATTR_SHIMMER_BOX).is_none());
    assert!(leaf
        .attrs
        .get(crate::renderer::ATTR_GLYPH_FX)
        .is_none_or(|s| s.is_empty()));
    assert_eq!(
        leaf.attrs
            .get(crate::renderer::ATTR_SHIMMER_COUNT)
            .map(|s| s.as_str()),
        Some("3")
    );
}

#[test]
fn shimmer_box_overlay_paints_in_chrome_above_glyphs() {
    let mut box_el = Element::new()
        .w(200.0)
        .h(48.0)
        .bg(crate::style::parse_color("secondary"));
    apply_class_list(&mut box_el, "shimmer-box shimmer-duration-1000");
    // Pin mid-cycle: tests have global_time_s() == 0.
    box_el.set_attr(crate::renderer::ATTR_SHIMMER_EPOCH, "-0.5");
    box_el = box_el.child(Element::new().text("shimmer-box · container overlay"));
    let mut root = Element::new().w(200.0).h(48.0).child(box_el);
    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 200.0, 48.0, [-1.0, -1.0]);
    let sheen = |i: &crate::renderer::InstanceInput| i.color[3] >= 0.01 && i.glass[0] < 0.0;
    let in_chrome: Vec<_> = result
        .input_chrome_instances
        .iter()
        .filter(|i| sheen(i))
        .collect();
    let in_instances: Vec<_> = result.instances.iter().filter(|i| sheen(i)).collect();
    assert_eq!(
        in_chrome.len(),
        1,
        "one continuous sheen above glyphs, not overlapping strips"
    );
    assert_eq!(in_chrome[0].rect_size, [200.0, 48.0]);
    assert!(
        in_instances.is_empty(),
        "sheen must not ride the SDF fill band (that draws under glyphon)"
    );
    assert!(
        in_chrome
            .iter()
            .all(|i| i.color[0] + i.color[1] + i.color[2] > 2.5),
        "sheen must be a light lift, not a dark stamp"
    );
    assert!(
        !result.text_nodes.is_empty(),
        "glyphs stay a separate earlier pass"
    );
}

#[test]
fn hover_scale_outside_scroll_clip_does_not_match() {
    let mut tile = Element::new();
    apply_class_list(
        &mut tile,
        "scale-100 transition-transform hover:scale-x-150",
    );
    // Stale abs at the sidebar (x≈0) while the live scroll viewport is the content column.
    tile.layout_bounds = [8.0, 80.0, 64.0, 64.0];
    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    let content_clip = Some([208.0, 0.0, 600.0, 800.0]);
    let hit_nodes = crate::app::pointer_events::pointer_hit_node_set(&tile, 40.0, 100.0);
    sync_class_variants_subtree_inner(
        &mut tile,
        [40.0, 100.0],
        "",
        false,
        1024.0,
        800.0,
        &[],
        &[],
        [0.0, 0.0],
        content_clip,
        &hit_nodes,
        &mut tree,
        &mut impact,
    );
    assert!(
        (tile.style.scale[0] - 1.0).abs() < 0.01,
        "sidebar pointer must not apply hover:scale-x-150 when outside the scroll clip, scale={:?}",
        tile.style.scale
    );
    assert!(
        !tile.transition_snapshot.has_active_tweens(),
        "must not start a transform tween from a clipped false-hover"
    );
}

#[test]
fn peer_hover_applies_to_next_ltr_sibling_only() {
    let mut marker = Element::new();
    apply_class_list(&mut marker, "peer");
    marker.layout_bounds = [0.0, 0.0, 40.0, 40.0];
    let mut follower = Element::new();
    apply_class_list(&mut follower, "bg-transparent peer-hover:bg-accent");
    follower.layout_bounds = [50.0, 0.0, 40.0, 40.0];
    let base_bg = follower.style.bg_color;
    let mut root = Element::new()
        .w(100.0)
        .h(40.0)
        .child(marker)
        .child(follower);
    root.layout_bounds = [0.0, 0.0, 100.0, 40.0];
    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_class_variants_subtree(
        &mut root,
        [20.0, 20.0],
        "",
        false,
        200.0,
        80.0,
        &mut tree,
        &mut impact,
    );
    assert_ne!(
        root.children[1].style.bg_color, base_bg,
        "pointer on the previous LTR sibling must match peer-hover on the next"
    );

    let mut marker = Element::new();
    apply_class_list(&mut marker, "peer");
    marker.layout_bounds = [0.0, 0.0, 40.0, 40.0];
    let mut follower = Element::new();
    apply_class_list(&mut follower, "bg-transparent peer-hover:bg-accent");
    follower.layout_bounds = [50.0, 0.0, 40.0, 40.0];
    let base_bg = follower.style.bg_color;
    let mut root = Element::new()
        .w(100.0)
        .h(40.0)
        .child(marker)
        .child(follower);
    root.layout_bounds = [0.0, 0.0, 100.0, 40.0];
    let mut impact = LayoutStatus::Clean;
    sync_class_variants_subtree(
        &mut root,
        [70.0, 20.0],
        "",
        false,
        200.0,
        80.0,
        &mut tree,
        &mut impact,
    );
    assert_eq!(
        root.children[1].style.bg_color, base_bg,
        "pointer on the follower itself is not a previous-sibling peer hover"
    );
}

#[test]
#[cfg(feature = "animation-base")]
fn scroll_paint_applies_hover_scale_from_pointer() {
    // CSS `:hover` is geometric: a tile sliding under a still cursor must enter
    // `hover:scale` on the freeze path (same overlay as color, no snapshot restore).
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    let mut tile = Element::new();
    apply_class_list(
        &mut tile,
        "scale-100 transition-transform hover:scale-x-150",
    );
    tile.layout_bounds = [400.0, 200.0, 64.0, 64.0];
    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    tick_active_css_transitions_subtree(
        &mut tile,
        [432.0, 232.0],
        "",
        false,
        1024.0,
        800.0,
        [0.0, 0.0],
        &mut tree,
        &mut impact,
    );
    assert!(
        tile.transition_snapshot.has_active_tweens(),
        "geometric :hover must start a transform tween on the freeze path"
    );
    assert!(
        (tile.style.scale[0] - 1.0).abs() < 0.01,
        "t=0 sample must hold rest scale, got {:?}",
        tile.style.scale
    );
    crate::style::advance_css_transition_timeline_ms(80.0);
    impact = LayoutStatus::Clean;
    tick_active_css_transitions_subtree(
        &mut tile,
        [432.0, 232.0],
        "",
        false,
        1024.0,
        800.0,
        [0.0, 0.0],
        &mut tree,
        &mut impact,
    );
    assert!(
        tile.style.scale[0] > 1.01 && tile.style.scale[0] < 1.5,
        "freeze path must advance hover:scale after enter, got {:?}",
        tile.style.scale
    );
    crate::style::set_css_transition_virtual_timeline(false);
}

#[test]
fn scroll_paint_freeze_applies_hover_color_from_pointer() {
    // CSS `:hover` is geometric: content sliding under a still cursor must
    // pick up hover:bg on the freeze path (Tabs / Toggle / ToggleGroup).
    let mut chip = Element::new();
    apply_class_list(&mut chip, "bg-transparent hover:bg-muted");
    chip.layout_bounds = [400.0, 200.0, 80.0, 32.0];
    let rest = chip.style.bg_color;
    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    tick_active_css_transitions_subtree(
        &mut chip,
        [440.0, 216.0],
        "",
        false,
        1024.0,
        800.0,
        [0.0, 0.0],
        &mut tree,
        &mut impact,
    );
    let hover_bg = parse_color("muted");
    assert_eq!(
        chip.style.bg_color, hover_bg,
        "freeze path must apply hover:bg when the pointer is over the chip"
    );
    assert_ne!(chip.style.bg_color, rest);
    assert!(chip
        .class_set
        .as_ref()
        .is_some_and(|cs| cs.last_pointer_hover == Some(true)));

    impact = LayoutStatus::Clean;
    chip.style.bg_color = parse_color("transparent");
    tick_active_css_transitions_subtree(
        &mut chip,
        [440.0, 216.0],
        "",
        false,
        1024.0,
        800.0,
        [0.0, 0.0],
        &mut tree,
        &mut impact,
    );
    assert_eq!(
        chip.style.bg_color, hover_bg,
        "freeze path must re-apply hover:bg after chrome restamps rest"
    );
}

#[test]
fn scroll_paint_freeze_hover_color_leave_pins_paint_dirty() {
    // Strip blit copies last frame's hover ink. Color `:hover` has no tween, so
    // freeze leave must named-pin dest-reset or the chip stays hovered until
    // scroll settles (no blit) and a full present runs.
    let _ = crate::app::damage::take_frame_paint_nodes();
    let mut chip = Element::new();
    apply_class_list(&mut chip, "bg-transparent hover:bg-muted");
    chip.layout_bounds = [400.0, 200.0, 80.0, 32.0];
    let rest = chip.style.bg_color;
    let hover_bg = parse_color("muted");
    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    tick_active_css_transitions_subtree(
        &mut chip,
        [440.0, 216.0],
        "",
        false,
        1024.0,
        800.0,
        [0.0, 0.0],
        &mut tree,
        &mut impact,
    );
    assert_eq!(chip.style.bg_color, hover_bg);
    let enter_dirty = crate::app::damage::take_frame_paint_nodes();
    assert!(
        !enter_dirty.is_empty(),
        "first geometric hover enter must pin blit dest-reset, got {enter_dirty:?}"
    );

    impact = LayoutStatus::Clean;
    tick_active_css_transitions_subtree(
        &mut chip,
        [440.0, 216.0],
        "",
        false,
        1024.0,
        800.0,
        [0.0, 0.0],
        &mut tree,
        &mut impact,
    );
    let stay_dirty = crate::app::damage::take_frame_paint_nodes();
    assert!(
        stay_dirty.is_empty(),
        "still-hovered re-apply must not pin every scroll frame, got {stay_dirty:?}"
    );

    impact = LayoutStatus::Clean;
    tick_active_css_transitions_subtree(
        &mut chip,
        [10.0, 10.0],
        "",
        false,
        1024.0,
        800.0,
        [0.0, 0.0],
        &mut tree,
        &mut impact,
    );
    assert_eq!(
        chip.class_set.as_ref().and_then(|cs| cs.last_pointer_hover),
        Some(false),
        "freeze leave must clear last_pointer_hover"
    );
    assert_eq!(chip.style.bg_color, rest, "hover:bg must restore rest");
    let leave_dirty = crate::app::damage::take_frame_paint_nodes();
    assert!(
        !leave_dirty.is_empty(),
        "hover leave during scroll freeze must pin dest-reset, got {leave_dirty:?}"
    );
}

#[test]
fn scroll_paint_freeze_hover_leave_follows_scroll_offset() {
    let _ = crate::app::damage::take_frame_paint_nodes();
    let mut chip = Element::new();
    apply_class_list(&mut chip, "bg-transparent hover:bg-muted");
    chip.layout_bounds = [20.0, 100.0, 80.0, 32.0];
    let rest = chip.style.bg_color;

    let mut viewport = Element::new().overflow("scroll");
    viewport.focus_id = Some("scroll:hover-leave".into());
    viewport.layout_bounds = [0.0, 0.0, 200.0, 200.0];
    viewport.scroll_offset = [0.0, 0.0];
    viewport = viewport.child(chip);

    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    tick_active_css_transitions_subtree(
        &mut viewport,
        [40.0, 110.0],
        "",
        false,
        400.0,
        400.0,
        [0.0, 0.0],
        &mut tree,
        &mut impact,
    );
    assert_eq!(
        viewport.children[0]
            .class_set
            .as_ref()
            .and_then(|cs| cs.last_pointer_hover),
        Some(true),
        "stationary pointer over the chip must enter hover"
    );
    let _ = crate::app::damage::take_frame_paint_nodes();

    viewport.scroll_offset = [0.0, 80.0];
    crate::core::reactivity::use_signal(
        &crate::core::scroll_viewport::scroll_offset_y_signal_key("hover-leave"),
        || 0.0_f32,
    )
    .set_silent(80.0);
    impact = LayoutStatus::Clean;
    tick_active_css_transitions_subtree(
        &mut viewport,
        [40.0, 110.0],
        "",
        false,
        400.0,
        400.0,
        [0.0, 0.0],
        &mut tree,
        &mut impact,
    );
    assert_eq!(
        viewport.children[0]
            .class_set
            .as_ref()
            .and_then(|cs| cs.last_pointer_hover),
        Some(false),
        "content sliding out from under a still pointer must leave hover"
    );
    assert_eq!(viewport.children[0].style.bg_color, rest);
    let leave_dirty = crate::app::damage::take_frame_paint_nodes();
    assert!(
        !leave_dirty.is_empty(),
        "scroll-induced hover leave must pin dest-reset, got {leave_dirty:?}"
    );
}

#[test]
#[cfg(feature = "animation-base")]
fn cached_css_branches_refresh_after_style_and_structure_invalidation() {
    use crate::layout::collect_cache::{
        invalidate_collect_paint_hash_path, stamp_subtree_paint_hashes,
    };
    let mut leaf = Element::new();
    leaf.layout_bounds = [10.0, 10.0, 40.0, 40.0];
    let mut root = Element::new().child(leaf);
    root.layout_bounds = [0.0, 0.0, 400.0, 400.0];
    stamp_subtree_paint_hashes(&mut root, false);
    assert!(!root.css_scroll_active);
    assert!(!root.subtree_paint_sync);
    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    let tick = |root: &mut Element, tree: &mut crate::layout::layout_tree::LayoutTree| {
        let mut impact = LayoutStatus::Clean;
        tick_active_css_transitions_subtree(
            root,
            [20.0, 20.0],
            "",
            false,
            400.0,
            400.0,
            [0.0, 0.0],
            tree,
            &mut impact,
        );
    };

    // Same invalidation used by mounted class/style binds: previously static
    // ancestors must discover a new hover variant before the next collect.
    apply_class_list(&mut root.children[0], "bg-transparent hover:bg-muted");
    let rest = root.children[0].style.bg_color;
    invalidate_collect_paint_hash_path(&mut root, &[0]);
    tick(&mut root, &mut tree);
    assert_ne!(root.children[0].style.bg_color, rest);
    stamp_subtree_paint_hashes(&mut root, true);
    assert!(root.css_scroll_active);
    assert!(root.css_hover_armed);
    assert!(root.children[0].css_hover_armed);

    // Replacing the branch must drop its old hover summary and discover the
    // newly mounted animation, even during a scroll-only frame.
    let mut spin = Element::new();
    apply_class_list(&mut spin, "animate-spin");
    spin.layout_bounds = [10.0, 10.0, 40.0, 40.0];
    root.children[0] = spin;
    invalidate_collect_paint_hash_path(&mut root, &[0]);
    tick(&mut root, &mut tree);
    assert!(super::super::animation::any_page_css_needs_frame());
    stamp_subtree_paint_hashes(&mut root, true);
    assert!(root.css_scroll_active);
    assert!(!root.css_hover_armed);

    root.children.clear();
    let mut selectable = Element::new().text("selectable");
    selectable.text_style.selectable = true;
    root.children.push(selectable);
    invalidate_collect_paint_hash_path(&mut root, &[]);
    stamp_subtree_paint_hashes(&mut root, true);
    assert!(!root.css_scroll_active);
    assert!(root.subtree_paint_sync);
    root.children.clear();
    invalidate_collect_paint_hash_path(&mut root, &[]);
    stamp_subtree_paint_hashes(&mut root, true);
    assert!(!root.subtree_paint_sync);
}

#[test]
fn offscreen_loop_does_not_pump_but_visible_motion_does() {
    let mut spin = Element::new();
    apply_class_list(&mut spin, "animate-spin");
    spin.layout_bounds = [10.0, 10000.0, 40.0, 40.0];
    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    for scrolling in [false, true] {
        let mut impact = LayoutStatus::Clean;
        if scrolling {
            tick_active_css_transitions_subtree(
                &mut spin,
                [-1.0, -1.0],
                "",
                false,
                400.0,
                400.0,
                [0.0, 0.0],
                &mut tree,
                &mut impact,
            );
        } else {
            sync_class_variants_subtree(
                &mut spin,
                [-1.0, -1.0],
                "",
                false,
                400.0,
                400.0,
                &mut tree,
                &mut impact,
            );
        }
        assert!(
            !super::super::animation::any_page_css_needs_frame(),
            "an invisible rotation must not keep the window rendering"
        );
    }
    assert!(
        animation_frame_affects_viewport(
            &spin,
            [0.0, 9900.0],
            crate::style::MAT2D_IDENTITY,
            [1.0, 1.0],
            400.0,
            400.0
        ),
        "scrolling the spinner back into view must resume its frame pump"
    );
    spin.style.translate = [0.0, -9900.0];
    assert!(
        animation_frame_affects_viewport(
            &spin,
            [0.0, 0.0],
            crate::style::MAT2D_IDENTITY,
            [1.0, 1.0],
            400.0,
            400.0
        ),
        "a transformed box can be visible despite its offscreen layout box"
    );
    spin.style.translate = [0.0, 0.0];
    spin.style.animations[0].iteration_count = super::super::AnimationIterationCount::Finite(1.0);
    assert!(
        animation_frame_affects_viewport(
            &spin,
            [0.0, 0.0],
            crate::style::MAT2D_IDENTITY,
            [1.0, 1.0],
            400.0,
            400.0
        ),
        "finite completion must remain observable offscreen"
    );
    spin.style.animations[0].iteration_count = super::super::AnimationIterationCount::Infinite;
    spin.style.animations[0].name = "bounce".into();
    assert!(
        !animation_frame_affects_viewport(
            &spin,
            [0.0, 0.0],
            crate::style::MAT2D_IDENTITY,
            [1.0, 1.0],
            400.0,
            400.0
        ),
        "a bounded bounce far below the viewport must sleep"
    );
    super::super::register_keyframes(
        "cpu-test-enters-view",
        vec![
            super::super::KeyframeStop::at(0.0).translate([0.0, 0.0]),
            super::super::KeyframeStop::at(1.0).translate([0.0, -10000.0]),
        ],
    );
    spin.style.animations[0].name = "cpu-test-enters-view".into();
    assert!(
        animation_frame_affects_viewport(
            &spin,
            [0.0, 0.0],
            crate::style::MAT2D_IDENTITY,
            [1.0, 1.0],
            400.0,
            400.0
        ),
        "the complete keyframe trajectory can enter the viewport"
    );

    let mut overflow = Element::new();
    apply_class_list(&mut overflow, "animate-spin");
    overflow.layout_bounds = [0.0, 10000.0, 40.0, 40.0];
    let mut child = Element::new();
    child.layout_bounds = [0.0, 10.0, 30.0, 30.0];
    overflow = overflow.child(child);
    assert!(
        animation_frame_affects_viewport(
            &overflow,
            [0.0, 0.0],
            crate::style::MAT2D_IDENTITY,
            [1.0, 1.0],
            400.0,
            400.0
        ),
        "overflow-visible descendants must not be hidden by their parent's layout bounds"
    );
}

#[test]
fn freeze_leaves_hover_on_child_when_parent_layout_box_is_offscreen() {
    // Overflow-visible (or stale skip-replay) parent: layout box is fully past
    // the viewport pad, child chip is still painted in-view. Freeze used to
    // `continue` the parent and never `force_leave` the leaf — hover stuck until
    // settle (full sync does not have that second child skip).
    let _ = crate::app::damage::take_frame_paint_nodes();
    let mut chip = Element::new();
    apply_class_list(&mut chip, "bg-transparent hover:bg-muted");
    chip.layout_bounds = [20.0, 80.0, 80.0, 32.0];
    let rest = chip.style.bg_color;

    let mut parent = Element::new();
    parent.layout_bounds = [0.0, 0.0, 200.0, 50.0];
    parent = parent.child(chip);

    let mut viewport = Element::new().overflow("scroll");
    viewport.focus_id = Some("scroll:offscreen-parent-hover".into());
    viewport.layout_bounds = [0.0, 0.0, 200.0, 200.0];
    viewport.scroll_offset = [0.0, 0.0];
    viewport = viewport.child(parent);

    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    let mouse = [40.0, 96.0];
    tick_active_css_transitions_subtree(
        &mut viewport,
        mouse,
        "",
        false,
        200.0,
        200.0,
        [0.0, 0.0],
        &mut tree,
        &mut impact,
    );
    assert_eq!(
        viewport.children[0].children[0]
            .class_set
            .as_ref()
            .and_then(|cs| cs.last_pointer_hover),
        Some(true),
        "pointer on the overflowing child must enter hover"
    );
    let _ = crate::app::damage::take_frame_paint_nodes();

    // Parent visual y = 0-100 = -100, h=50 → fully past PAD (32). Child visual
    // y = 80-100 = -20, h=32 → still intersects the viewport. Mouse stays put.
    viewport.scroll_offset = [0.0, 100.0];
    crate::core::reactivity::use_signal(
        &crate::core::scroll_viewport::scroll_offset_y_signal_key("offscreen-parent-hover"),
        || 0.0_f32,
    )
    .set_silent(100.0);
    impact = LayoutStatus::Clean;
    tick_active_css_transitions_subtree(
        &mut viewport,
        mouse,
        "",
        false,
        200.0,
        200.0,
        [0.0, 0.0],
        &mut tree,
        &mut impact,
    );
    assert_eq!(
        viewport.children[0].children[0]
            .class_set
            .as_ref()
            .and_then(|cs| cs.last_pointer_hover),
        Some(false),
        "freeze must leave a hovered child even when its parent layout box is offscreen"
    );
    assert_eq!(viewport.children[0].children[0].style.bg_color, rest);
    let leave_dirty = crate::app::damage::take_frame_paint_nodes();
    assert!(
        !leave_dirty.is_empty(),
        "overflowing-child hover leave must pin dest-reset, got {leave_dirty:?}"
    );
}

#[test]
fn scroll_paint_freeze_hover_scale_leave_follows_scroll_offset() {
    let _ = crate::app::damage::take_frame_paint_nodes();
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    let mut chip = Element::new();
    apply_class_list(
        &mut chip,
        "scale-100 transition-transform duration-300 hover:scale-125",
    );
    chip.layout_bounds = [20.0, 40.0, 64.0, 64.0];

    let mut viewport = Element::new().overflow("scroll");
    viewport.focus_id = Some("scroll:hover-scale-leave".into());
    viewport.layout_bounds = [0.0, 0.0, 200.0, 200.0];
    viewport.scroll_offset = [0.0, 0.0];
    viewport = viewport.child(chip);

    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    tick_active_css_transitions_subtree(
        &mut viewport,
        [52.0, 72.0],
        "",
        false,
        200.0,
        200.0,
        [0.0, 0.0],
        &mut tree,
        &mut impact,
    );
    assert_eq!(
        viewport.children[0]
            .class_set
            .as_ref()
            .and_then(|cs| cs.last_pointer_hover),
        Some(true),
        "stationary pointer over the tile must enter hover:scale"
    );
    crate::style::advance_css_transition_timeline_ms(320.0);
    impact = LayoutStatus::Clean;
    tick_active_css_transitions_subtree(
        &mut viewport,
        [52.0, 72.0],
        "",
        false,
        200.0,
        200.0,
        [0.0, 0.0],
        &mut tree,
        &mut impact,
    );
    let hovered_scale = viewport.children[0].style.scale[0];
    assert!(
        hovered_scale > 1.1,
        "hover:scale must settle near 1.25 before leave, got {hovered_scale}"
    );
    let _ = crate::app::damage::take_frame_paint_nodes();

    viewport.scroll_offset = [0.0, 120.0];
    crate::core::reactivity::use_signal(
        &crate::core::scroll_viewport::scroll_offset_y_signal_key("hover-scale-leave"),
        || 0.0_f32,
    )
    .set_silent(120.0);
    impact = LayoutStatus::Clean;
    tick_active_css_transitions_subtree(
        &mut viewport,
        [52.0, 72.0],
        "",
        false,
        200.0,
        200.0,
        [0.0, 0.0],
        &mut tree,
        &mut impact,
    );
    assert_eq!(
        viewport.children[0]
            .class_set
            .as_ref()
            .and_then(|cs| cs.last_pointer_hover),
        Some(false),
        "content sliding out from under a still pointer must leave hover:scale"
    );
    crate::style::set_css_transition_virtual_timeline(false);
}

#[test]
#[cfg(feature = "animation-base")]
fn scroll_paint_applies_color_and_scale_hover() {
    let mut chip = Element::new();
    apply_class_list(
        &mut chip,
        "bg-transparent scale-100 transition-transform hover:bg-muted hover:scale-x-150",
    );
    chip.layout_bounds = [400.0, 200.0, 80.0, 32.0];
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    tick_active_css_transitions_subtree(
        &mut chip,
        [440.0, 216.0],
        "",
        false,
        1024.0,
        800.0,
        [0.0, 0.0],
        &mut tree,
        &mut impact,
    );
    assert_eq!(chip.style.bg_color, parse_color("muted"));
    assert!(
        chip.transition_snapshot.has_active_tweens(),
        "color+scale hover must start a transform tween on the freeze path"
    );
    assert!(
        (chip.style.scale[0] - 1.0).abs() < 0.01,
        "t=0 sample must hold rest scale alongside color hover, scale={:?}",
        chip.style.scale
    );
    crate::style::advance_css_transition_timeline_ms(80.0);
    impact = LayoutStatus::Clean;
    tick_active_css_transitions_subtree(
        &mut chip,
        [440.0, 216.0],
        "",
        false,
        1024.0,
        800.0,
        [0.0, 0.0],
        &mut tree,
        &mut impact,
    );
    assert_eq!(chip.style.bg_color, parse_color("muted"));
    assert!(
        chip.style.scale[0] > 1.01 && chip.style.scale[0] < 1.5,
        "freeze path must advance hover:scale with color hover, scale={:?}",
        chip.style.scale
    );
    crate::style::set_css_transition_virtual_timeline(false);
}

#[test]
#[cfg(feature = "animation-base")]
fn scroll_paint_freeze_still_advances_active_tween() {
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    let mut tile = Element::new();
    apply_class_list(
        &mut tile,
        "scale-100 transition-transform hover:scale-x-150",
    );
    tile.layout_bounds = [400.0, 200.0, 64.0, 64.0];
    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    // Seed idle tween slot at scale 1.0 (same as a prior settled frame).
    sync_class_variants_subtree(
        &mut tile,
        [10.0, 10.0],
        "",
        false,
        1024.0,
        800.0,
        &mut tree,
        &mut impact,
    );
    // Full sync: pointer on tile → retarget hover tween from 1.0 → 1.5.
    impact = LayoutStatus::Clean;
    sync_class_variants_subtree(
        &mut tile,
        [432.0, 232.0],
        "",
        false,
        1024.0,
        800.0,
        &mut tree,
        &mut impact,
    );
    assert!(
        tile.transition_snapshot.has_active_tweens(),
        "expected an active transform tween after hover enter"
    );
    let scale_after_enter = tile.style.scale[0];
    // Freeze path: advance timeline without re-resolving hover.
    crate::style::advance_css_transition_timeline_ms(80.0);
    impact = LayoutStatus::Clean;
    tick_active_css_transitions_subtree(
        &mut tile,
        [432.0, 232.0],
        "",
        false,
        1024.0,
        800.0,
        [0.0, 0.0],
        &mut tree,
        &mut impact,
    );
    assert!(
            tile.style.scale[0] > scale_after_enter + 0.01,
            "active tween must keep advancing during scroll freeze, before={scale_after_enter} after={}",
            tile.style.scale[0]
        );
    crate::style::set_css_transition_virtual_timeline(false);
}

#[test]
fn offset_distance_is_paint_only_for_taffy() {
    let before = Style::default();
    let mut after = before.clone();
    after.offset_distance = 0.42;
    after.offset_rotate_deg = Some(15.0);
    assert!(
        !style_needs_taffy_sync(&before, &after),
        "offset-path motion must not dirty Taffy"
    );
    after.radii = [-1.0, -1.0, -1.0, -1.0];
    assert!(
        !style_needs_taffy_sync(&before, &after),
        "border-radius is paint-only"
    );
    after.padding.top = taffy::style::LengthPercentage::length(8.0);
    assert!(
        style_needs_taffy_sync(&before, &after),
        "padding still requires a Taffy sync"
    );
}

#[test]
fn hover_text_color_transitions_away_from_foreground() {
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    let fg = parse_text_color("foreground");
    let dest = parse_text_color("destructive");
    assert_ne!(fg, dest, "demo color must differ from Label default");
    let mut el = Element::new().text("悬停变色");
    el.text_style.color = Some(fg);
    apply_class_list(
        &mut el,
        "transition-colors duration-300 hover:text-destructive",
    );
    el.layout_bounds = [10.0, 10.0, 120.0, 24.0];
    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    for _ in 0..12 {
        crate::style::advance_css_transition_timeline_ms(30.0);
        let mut impact = LayoutStatus::Clean;
        sync_class_variants_subtree(
            &mut el,
            [40.0, 20.0],
            "",
            false,
            400.0,
            200.0,
            &mut tree,
            &mut impact,
        );
    }
    let got = el.text_style.color.expect("color");
    assert_ne!(
        got, fg,
        "hover:text-destructive must leave foreground, got {got:?}"
    );
    crate::style::set_css_transition_virtual_timeline(false);
}

#[test]
#[cfg(feature = "animation-base")]
fn responsive_padding_mount_seeds_resolved_breakpoint_before_animation() {
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    let mut tree = crate::layout::LayoutTree::new();
    for _ in 0..3 {
        let mut element = with_class_list(
            Element::new()
                .w(400.0)
                .h(200.0)
                .transition("padding 200ms linear"),
            "p-3 sm:p-4 lg:p-6",
        );
        let mut impact = LayoutStatus::Clean;
        sync_class_variants_subtree(
            &mut element,
            [-1.0; 2],
            "",
            false,
            1100.0,
            600.0,
            &mut tree,
            &mut impact,
        );
        assert_eq!(
            crate::layout::lp_kind(element.style.padding.top),
            crate::layout::DimKind::Length(24.0),
            "mounting a page must not animate from the unselected base breakpoint"
        );
        assert!(!element.transition_snapshot.has_active_tweens());
        sync_class_variants_subtree(
            &mut element,
            [-1.0; 2],
            "",
            false,
            900.0,
            600.0,
            &mut tree,
            &mut impact,
        );
        assert_eq!(
            crate::layout::lp_kind(element.style.padding.top),
            crate::layout::DimKind::Length(24.0)
        );
        crate::style::advance_css_transition_timeline_ms(100.0);
        sync_class_variants_subtree(
            &mut element,
            [-1.0; 2],
            "",
            false,
            900.0,
            600.0,
            &mut tree,
            &mut impact,
        );
        assert_eq!(
            crate::layout::lp_kind(element.style.padding.top),
            crate::layout::DimKind::Length(20.0)
        );
    }
    crate::style::set_css_transition_virtual_timeline(false);
}

#[test]
fn hover_padding_leave_restores_authored_padding() {
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    let mut el = Element::new().p("0.25rem").w(128.0).h(56.0);
    apply_class_list(&mut el, "transition-padding duration-300 hover:p-4");
    el.layout_bounds = [0.0, 0.0, 128.0, 56.0];
    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    let pad_px = |el: &Element| match crate::layout::lp_kind(el.style.padding.top) {
        crate::layout::DimKind::Length(px) => px,
        other => panic!("expected length padding, got {other:?}"),
    };
    let base_px = pad_px(&el);
    assert!((base_px - 4.0).abs() < 0.2, "0.25rem base, got {base_px}");
    for _ in 0..12 {
        crate::style::advance_css_transition_timeline_ms(30.0);
        let mut impact = LayoutStatus::Clean;
        sync_class_variants_subtree(
            &mut el,
            [20.0, 20.0],
            "",
            false,
            400.0,
            200.0,
            &mut tree,
            &mut impact,
        );
    }
    let hovered = pad_px(&el);
    assert!(
        hovered > 10.0,
        "hover:p-4 should grow padding, got {hovered}"
    );
    let mut leave_requested_layout = false;
    for _ in 0..16 {
        crate::style::advance_css_transition_timeline_ms(30.0);
        let mut impact = LayoutStatus::Clean;
        sync_class_variants_subtree(
            &mut el,
            [300.0, 20.0],
            "",
            false,
            400.0,
            200.0,
            &mut tree,
            &mut impact,
        );
        if matches!(
            impact,
            LayoutStatus::LayoutChanged | LayoutStatus::StructureChanged
        ) {
            leave_requested_layout = true;
        }
    }
    assert!(
        leave_requested_layout,
        "padding leave tween must request compute_layout, not paint-only StyleChanged"
    );
    let left = pad_px(&el);
    assert!(
        (left - base_px).abs() < 1.0,
        "leave must restore authored 0.25rem, base={base_px} left={left}"
    );
    crate::style::set_css_transition_virtual_timeline(false);
}

#[test]
#[cfg(feature = "animation-base")]
fn scroll_paint_freeze_still_advances_spring() {
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    let mut tile = Element::new().transition("transform spring()");
    apply_class_list(&mut tile, "hover:scale-125");
    tile.layout_bounds = [400.0, 200.0, 64.0, 64.0];
    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_class_variants_subtree(
        &mut tile,
        [10.0, 10.0],
        "",
        false,
        1024.0,
        800.0,
        &mut tree,
        &mut impact,
    );
    impact = LayoutStatus::Clean;
    sync_class_variants_subtree(
        &mut tile,
        [432.0, 232.0],
        "",
        false,
        1024.0,
        800.0,
        &mut tree,
        &mut impact,
    );
    assert!(
        tile.transition_snapshot.has_active_tweens(),
        "spring hover enter must start an active tween"
    );
    let scale_after_enter = tile.style.scale[0];
    crate::style::advance_css_transition_timeline_ms(80.0);
    impact = LayoutStatus::Clean;
    tick_active_css_transitions_subtree(
        &mut tile,
        [432.0, 232.0],
        "",
        false,
        1024.0,
        800.0,
        [0.0, 0.0],
        &mut tree,
        &mut impact,
    );
    assert!(
        tile.style.scale[0] > scale_after_enter + 0.005,
        "spring must keep integrating during scroll freeze, before={scale_after_enter} after={}",
        tile.style.scale[0]
    );
    crate::style::set_css_transition_virtual_timeline(false);
}

#[test]
fn scroll_paint_freeze_leaves_stale_hover_spring() {
    // Wheel can slide a hovered tile out from under a still cursor. Freeze enters
    // geometric `:hover` (including scale), and must retarget the spring back to rest
    // when the painted AABB no longer contains the pointer.
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    let mut tile = Element::new().transition("transform spring()");
    apply_class_list(&mut tile, "hover:scale-125");
    tile.layout_bounds = [400.0, 200.0, 64.0, 64.0];
    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_class_variants_subtree(
        &mut tile,
        [10.0, 10.0],
        "",
        false,
        1024.0,
        800.0,
        &mut tree,
        &mut impact,
    );
    impact = LayoutStatus::Clean;
    sync_class_variants_subtree(
        &mut tile,
        [432.0, 232.0],
        "",
        false,
        1024.0,
        800.0,
        &mut tree,
        &mut impact,
    );
    for _ in 0..12 {
        crate::style::advance_css_transition_timeline_ms(16.0);
        impact = LayoutStatus::Clean;
        tick_active_css_transitions_subtree(
            &mut tile,
            [432.0, 232.0],
            "",
            false,
            1024.0,
            800.0,
            [0.0, 0.0],
            &mut tree,
            &mut impact,
        );
    }
    let hovered = tile.style.scale[0];
    assert!(
        hovered > 1.05,
        "spring should have grown under hover, got {hovered}"
    );
    assert_eq!(
        tile.class_set.as_ref().and_then(|cs| cs.last_pointer_hover),
        Some(true),
        "hover enter must record last_pointer_hover"
    );
    for _ in 0..16 {
        crate::style::advance_css_transition_timeline_ms(16.0);
        impact = LayoutStatus::Clean;
        tick_active_css_transitions_subtree(
            &mut tile,
            [10.0, 10.0],
            "",
            false,
            1024.0,
            800.0,
            [0.0, 0.0],
            &mut tree,
            &mut impact,
        );
    }
    assert_eq!(
        tile.class_set.as_ref().and_then(|cs| cs.last_pointer_hover),
        Some(false),
        "freeze leave must clear last_pointer_hover"
    );
    assert!(
        tile.style.scale[0] < hovered - 0.02,
        "stale hover must leave during scroll freeze, before={hovered} after={}",
        tile.style.scale[0]
    );
    crate::style::set_css_transition_virtual_timeline(false);
}

#[test]
fn scroll_paint_freeze_leave_uses_live_parent_rotate() {
    let mut child = Element::new();
    apply_class_list(&mut child, "hover:scale-125");
    child.layout_bounds = [60.0, 90.0, 80.0, 20.0];
    child.paint_ancestor_linear = crate::style::MAT2D_IDENTITY;
    child.paint_ancestor_scale = [1.0, 1.0];
    {
        let cs = child.class_set.as_mut().expect("hover:scale-125 variants");
        assert!(!cs.variants.is_empty());
        cs.last_pointer_hover = Some(true);
    }
    let mut parent = Element::new().w(200.0).h(200.0).child(child);
    parent.layout_bounds = [0.0, 0.0, 200.0, 200.0];
    parent.style.rotate_deg = 90.0;
    parent.style.transform_origin = crate::style::parse_transform_origin("center");
    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    tick_active_css_transitions_subtree(
        &mut parent,
        [100.0, 130.0],
        "",
        false,
        1024.0,
        800.0,
        [0.0, 0.0],
        &mut tree,
        &mut impact,
    );
    assert_eq!(
        parent.children[0]
            .class_set
            .as_ref()
            .and_then(|cs| cs.last_pointer_hover),
        Some(true),
        "live parent rotate must keep hover on the painted child AABB (stamps were identity)"
    );
    impact = LayoutStatus::Clean;
    tick_active_css_transitions_subtree(
        &mut parent,
        [40.0, 100.0],
        "",
        false,
        1024.0,
        800.0,
        [0.0, 0.0],
        &mut tree,
        &mut impact,
    );
    assert_eq!(
        parent.children[0]
            .class_set
            .as_ref()
            .and_then(|cs| cs.last_pointer_hover),
        Some(false),
        "old layout overhang must still leave under live parent rotate"
    );
}

#[test]
fn idle_hover_scale_transition_settles() {
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    let mut el = Element::new();
    apply_class_list(&mut el, "scale-100 transition-transform hover:scale-x-150");
    el.layout_bounds = [400.0, 200.0, 64.0, 64.0];
    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    for _ in 0..20 {
        crate::style::advance_css_transition_timeline_ms(1000.0 / 60.0);
        let mut impact = LayoutStatus::Clean;
        sync_class_variants_subtree(
            &mut el,
            [10.0, 10.0],
            "",
            false,
            1024.0,
            800.0,
            &mut tree,
            &mut impact,
        );
    }
    assert!(
        !el.transition_snapshot.has_active_tweens(),
        "unhovered transition-transform tile must not keep an active tween"
    );
    assert!((el.style.scale[0] - 1.0).abs() < 0.01);
    crate::style::set_css_transition_virtual_timeline(false);
}

#[test]
fn hover_scale_keeps_pointer_on_painted_aabb() {
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    let mut el = Element::new();
    apply_class_list(&mut el, "scale-100 transition-transform hover:scale-x-150");
    // 64px box at x=400. After scale-x 1.5 the paint AABB starts at 384.
    el.layout_bounds = [400.0, 200.0, 64.0, 64.0];
    el.style.scale = [1.5, 1.0];
    let paint = crate::core::tree::visual_paint_aabb(&el, [0.0, 0.0]);
    assert!(
        (paint[0] - 384.0).abs() < 0.5 && (paint[2] - 96.0).abs() < 0.5,
        "scale-x 1.5 AABB should be 96px wide starting 16px left, got {paint:?}"
    );
    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    // Pointer on the scaled overflow (left of the Taffy box) must still count as hover.
    for _ in 0..20 {
        crate::style::advance_css_transition_timeline_ms(1000.0 / 60.0);
        let mut impact = LayoutStatus::Clean;
        sync_class_variants_subtree(
            &mut el,
            [390.0, 232.0],
            "",
            false,
            1024.0,
            800.0,
            &mut tree,
            &mut impact,
        );
    }
    assert!(
        (el.style.scale[0] - 1.5).abs() < 0.05,
        "pointer on painted scale overflow must keep hover:scale-x-150, scale={:?}",
        el.style.scale
    );
    crate::style::set_css_transition_virtual_timeline(false);
}

#[test]
fn hover_rotate_origin_left_keeps_pointer_on_painted_aabb() {
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    let mut el = Element::new();
    apply_class_list(&mut el, "rotate-0 transition-transform hover:rotate-90");
    el.layout_bounds = [400.0, 200.0, 100.0, 100.0];
    let origin = crate::style::parse_transform_origin("left top");
    el.style.transform_origin = origin.clone();
    if let Some(cs) = el.class_set.as_mut() {
        if let Some(base) = cs.base_style.as_mut() {
            base.transform_origin = origin;
        }
    }
    el.style.rotate_deg = 90.0;
    let paint = crate::core::tree::visual_paint_aabb(&el, [0.0, 0.0]);
    assert!(
        (paint[0] - 300.0).abs() < 0.5 && (paint[2] - 100.0).abs() < 0.5,
        "left-top 90° AABB should sit at x=300, got {paint:?}"
    );
    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    for _ in 0..20 {
        crate::style::advance_css_transition_timeline_ms(1000.0 / 60.0);
        let mut impact = LayoutStatus::Clean;
        sync_class_variants_subtree(
            &mut el,
            [350.0, 250.0],
            "",
            false,
            1024.0,
            800.0,
            &mut tree,
            &mut impact,
        );
    }
    assert!(
        (el.style.rotate_deg - 90.0).abs() < 0.5,
        "pointer on painted origin-left rotate must keep hover:rotate-90, rotate={}",
        el.style.rotate_deg
    );
    crate::style::set_css_transition_virtual_timeline(false);
}

#[test]
#[cfg(feature = "animation-base")]
fn hover_rotate_origin_left_leaves_old_layout_center() {
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    let mut el = Element::new();
    apply_class_list(&mut el, "rotate-0 transition-transform hover:rotate-90");
    el.layout_bounds = [400.0, 200.0, 100.0, 100.0];
    let origin = crate::style::parse_transform_origin("left top");
    el.style.transform_origin = origin.clone();
    if let Some(cs) = el.class_set.as_mut() {
        if let Some(base) = cs.base_style.as_mut() {
            base.transform_origin = origin;
        }
    }
    el.style.rotate_deg = 90.0;
    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    for _ in 0..5 {
        crate::style::advance_css_transition_timeline_ms(1000.0 / 60.0);
        let mut impact = LayoutStatus::Clean;
        sync_class_variants_subtree(
            &mut el,
            [350.0, 250.0],
            "",
            false,
            1024.0,
            800.0,
            &mut tree,
            &mut impact,
        );
    }
    assert!(
        (el.style.rotate_deg - 90.0).abs() < 0.5,
        "expected hover held on painted AABB, got {}",
        el.style.rotate_deg
    );
    for _ in 0..8 {
        crate::style::advance_css_transition_timeline_ms(1000.0 / 60.0);
        let mut impact = LayoutStatus::Clean;
        sync_class_variants_subtree(
            &mut el,
            [450.0, 250.0],
            "",
            false,
            1024.0,
            800.0,
            &mut tree,
            &mut impact,
        );
    }
    assert!(
        el.style.rotate_deg < 85.0,
        "old layout center is outside the painted 90° AABB, so hover must start leaving (got {})",
        el.style.rotate_deg
    );
    crate::style::set_css_transition_virtual_timeline(false);
}

#[test]
fn class_hover_rotate_45_misses_aabb_corner() {
    let mut el = Element::new();
    apply_class_list(&mut el, "bg-transparent hover:bg-muted");
    el.layout_bounds = [0.0, 0.0, 80.0, 80.0];
    el.style.rotate_deg = 45.0;
    let rest = el.style.bg_color;
    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_class_variants_subtree(
        &mut el,
        [80.0, 0.0],
        "",
        false,
        1024.0,
        800.0,
        &mut tree,
        &mut impact,
    );
    assert_eq!(
        el.style.bg_color, rest,
        "AABB empty corner of a 45° tile must not enter geometric :hover"
    );
    let ink = crate::core::tree::layout_local_to_paint_world(
        &el,
        [40.0, 40.0],
        [0.0, 0.0],
        crate::style::MAT2D_IDENTITY,
        [1.0, 1.0],
    );
    impact = LayoutStatus::Clean;
    sync_class_variants_subtree(
        &mut el,
        [ink[0], ink[1]],
        "",
        false,
        1024.0,
        800.0,
        &mut tree,
        &mut impact,
    );
    assert_eq!(
        el.style.bg_color,
        parse_color("muted"),
        "painted diamond center must still enter :hover"
    );
}

#[test]
fn hover_scale_leave_uses_painted_aabb_not_layout_box() {
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    let mut el = Element::new();
    apply_class_list(&mut el, "scale-100 transition-transform hover:scale-x-150");
    el.layout_bounds = [400.0, 200.0, 64.0, 64.0];
    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    // Enter and settle at 1.5×.
    for _ in 0..20 {
        crate::style::advance_css_transition_timeline_ms(1000.0 / 60.0);
        let mut impact = LayoutStatus::Clean;
        sync_class_variants_subtree(
            &mut el,
            [432.0, 232.0],
            "",
            false,
            1024.0,
            800.0,
            &mut tree,
            &mut impact,
        );
    }
    assert!(
        (el.style.scale[0] - 1.5).abs() < 0.05,
        "expected settled hover scale, got {:?}",
        el.style.scale
    );
    // Pointer on the left overhang of the painted 96px box (layout starts at 400).
    for _ in 0..5 {
        crate::style::advance_css_transition_timeline_ms(1000.0 / 60.0);
        let mut impact = LayoutStatus::Clean;
        sync_class_variants_subtree(
            &mut el,
            [390.0, 232.0],
            "",
            false,
            1024.0,
            800.0,
            &mut tree,
            &mut impact,
        );
    }
    assert!(
        (el.style.scale[0] - 1.5).abs() < 0.05,
        "leave must use painted AABB; overhang at x=390 must stay hovered, scale={:?}",
        el.style.scale
    );
    crate::style::set_css_transition_virtual_timeline(false);
}

#[test]
fn class_variant_hover_needs_frame_on_enter_leave() {
    let mut el = Element::new();
    apply_class_list(&mut el, "scale-100 transition-transform hover:scale-x-150");
    el.layout_bounds = [400.0, 200.0, 64.0, 64.0];
    assert!(
        class_variant_hover_needs_frame(&el, [432.0, 232.0]),
        "first enter over a hover: variant tile must request a frame"
    );
    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_class_variants_subtree(
        &mut el,
        [432.0, 232.0],
        "",
        false,
        1024.0,
        800.0,
        &mut tree,
        &mut impact,
    );
    assert!(
        !class_variant_hover_needs_frame(&el, [432.0, 232.0]),
        "steady hover must not keep waking frames"
    );
    assert!(
        class_variant_hover_needs_frame(&el, [10.0, 10.0]),
        "leave must wake a frame so hover:scale can shrink"
    );
}

#[test]
fn class_variant_hover_needs_frame_ignores_non_pointer_variants() {
    let mut page = Element::new();
    apply_class_list(&mut page, "md:flex dark:bg-accent");
    page.layout_bounds = [0.0, 0.0, 400.0, 400.0];
    assert!(
            !class_variant_hover_needs_frame(&page, [40.0, 40.0]),
            "sm/md/dark/data variants must not request_redraw when the pointer enters a box — that storms Marker spin while sliding the page"
        );
    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_class_variants_subtree(
        &mut page,
        [40.0, 40.0],
        "",
        false,
        400.0,
        400.0,
        &mut tree,
        &mut impact,
    );
    assert!(
        !class_variant_hover_needs_frame(&page, [200.0, 80.0]),
        "sliding inside a breakpoint-only box must not wake extra paints"
    );
    assert!(
        !class_variant_hover_needs_frame(&page, [-10.0, -10.0]),
        "leaving a breakpoint-only box must not wake extra paints"
    );
}

#[test]
fn class_variant_hover_needs_frame_false_on_empty_padding_after_seed() {
    let mut icon = Element::new().w(16.0).h(16.0);
    apply_class_list(&mut icon, "animate-spin");
    icon.layout_bounds = [8.0, 8.0, 16.0, 16.0];
    let mut tile = Element::new().w(64.0).h(64.0);
    apply_class_list(&mut tile, "hover:scale-125");
    tile.layout_bounds = [300.0, 300.0, 64.0, 64.0];
    let mut root = Element::new().w(400.0).h(400.0).child(icon).child(tile);
    root.layout_bounds = [0.0, 0.0, 400.0, 400.0];
    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_class_variants_subtree(
        &mut root,
        [20.0, 80.0],
        "",
        false,
        400.0,
        400.0,
        &mut tree,
        &mut impact,
    );
    assert!(
        !class_variant_hover_needs_frame(&root, [20.0, 80.0]),
        "padding after seed must not keep waking frames"
    );
    assert!(
        !class_variant_hover_needs_frame(&root, [90.0, 110.0]),
        "sliding empty padding must not wake a hover: frame"
    );
    let a = crate::app::pointer_events::hit_path(&root, 20.0, 80.0);
    let b = crate::app::pointer_events::hit_path(&root, 90.0, 110.0);
    assert!(
        crate::app::pointer_events::hit_path_identity_eq(&a, &b),
        "empty-padding slides must keep the same hit chain so CursorMoved can skip gallery walks"
    );
}

#[test]
fn resolve_breakpoint_sm() {
    let mut el = Element::new();
    apply_class_list(&mut el, "bg-transparent sm:bg-accent");
    let base_bg = el.style.bg_color;
    let _ = resolve_class_variants(
        &mut el,
        &VariantContext {
            viewport_w: 639.0,
            ..Default::default()
        },
    );
    assert_eq!(el.style.bg_color, base_bg);
    let _ = resolve_class_variants(
        &mut el,
        &VariantContext {
            viewport_w: 640.0,
            ..Default::default()
        },
    );
    assert_ne!(el.style.bg_color, base_bg);
}

#[test]
fn responsive_hidden_flex_preserves_direction_and_releases_layout_space() {
    let mut el = Element::new()
        .flex_col()
        .w(200.0)
        .h(100.0)
        .class("hidden md:flex");
    for (width, shown) in [
        (320.0, false),
        (767.5, false),
        (768.0, true),
        (320.0, false),
    ] {
        resolve_class_variants(
            &mut el,
            &VariantContext {
                viewport_w: width,
                ..Default::default()
            },
        );
        assert_eq!(
            el.style.display,
            if shown {
                taffy::style::Display::Flex
            } else {
                taffy::style::Display::None
            }
        );
        assert_eq!(el.style.flex_direction, taffy::style::FlexDirection::Column);
        let mut tree = crate::core::tree::LayoutTree::new();
        tree.build_and_layout(&mut el, width, 600.0, [-1.0, -1.0]);
        assert_eq!(el.layout_bounds[2] > 0.0, shown);
    }
}

#[test]
fn responsive_defaults_survive_class_binding_and_respect_stylesheet_overrides() {
    let mut el = with_default_class_list(Element::new().flex_col(), "sm:flex-row");
    resolve_class_variants(
        &mut el,
        &VariantContext {
            viewport_w: 800.0,
            ..Default::default()
        },
    );
    apply_class_list(&mut el, "gap-3");
    resolve_class_variants(
        &mut el,
        &VariantContext {
            viewport_w: 320.0,
            ..Default::default()
        },
    );
    assert_eq!(el.style.flex_direction, taffy::style::FlexDirection::Column);

    crate::style::clear_stylesheets();
    crate::style::register_stylesheet(
        crate::style::parse_stylesheet(
            "[[rule]]\nselectors = [\"[data-slot=responsive-default]\"]\nclasses = \"sm:flex-col\"",
        )
        .unwrap(),
    )
    .unwrap();
    el.set_attr("data-slot", "responsive-default");
    el = crate::style::apply_stylesheet_rules(el);
    resolve_class_variants(
        &mut el,
        &VariantContext {
            viewport_w: 800.0,
            ..Default::default()
        },
    );
    assert_eq!(el.style.flex_direction, taffy::style::FlexDirection::Column);
    apply_class_list(&mut el, "sm:flex-row gap-4");
    resolve_class_variants(
        &mut el,
        &VariantContext {
            viewport_w: 800.0,
            ..Default::default()
        },
    );
    assert_eq!(el.style.flex_direction, taffy::style::FlexDirection::Row);
    crate::style::clear_stylesheets();
}

#[test]
fn responsive_breakpoints_cover_fractional_widths_and_restore_on_resize() {
    for (min, max, width) in [
        ("sm", "max-sm", 640.0),
        ("md", "max-md", 768.0),
        ("lg", "max-lg", 1024.0),
        ("xl", "max-xl", 1280.0),
        ("2xl", "max-2xl", 1536.0),
        ("min-[800px]", "max-[800px]", 800.0),
    ] {
        let min = VariantSelector::parse_prefix(min).unwrap();
        let max = VariantSelector::parse_prefix(max).unwrap();
        for viewport_w in [width - 0.5, width, width + 0.5] {
            let ctx = VariantContext {
                viewport_w,
                ..Default::default()
            };
            assert_eq!(ctx.matches_selector(&min), viewport_w >= width);
            assert_eq!(ctx.matches_selector(&max), viewport_w < width);
            assert_ne!(ctx.matches_selector(&min), ctx.matches_selector(&max));
        }
    }
    let mut el = Element::new();
    apply_class_list(&mut el, "flex-col sm:flex-row md:flex-col");
    for (viewport_w, expected) in [
        (320.0, taffy::style::FlexDirection::Column),
        (639.5, taffy::style::FlexDirection::Column),
        (640.0, taffy::style::FlexDirection::Row),
        (768.0, taffy::style::FlexDirection::Column),
        (640.0, taffy::style::FlexDirection::Row),
        (320.0, taffy::style::FlexDirection::Column),
    ] {
        resolve_class_variants(
            &mut el,
            &VariantContext {
                viewport_w,
                ..Default::default()
            },
        );
        assert_eq!(el.style.flex_direction, expected, "width={viewport_w}");
    }
}

#[test]
fn resolve_arbitrary_min_max_breakpoints() {
    let mut el = Element::new();
    apply_class_list(&mut el, "bg-transparent min-[800px]:bg-accent");
    let base_bg = el.style.bg_color;
    let _ = resolve_class_variants(
        &mut el,
        &VariantContext {
            viewport_w: 799.0,
            ..Default::default()
        },
    );
    assert_eq!(el.style.bg_color, base_bg);
    let _ = resolve_class_variants(
        &mut el,
        &VariantContext {
            viewport_w: 800.0,
            ..Default::default()
        },
    );
    assert_ne!(el.style.bg_color, base_bg);

    let mut narrow = Element::new();
    apply_class_list(&mut narrow, "bg-transparent max-[599px]:bg-accent");
    let base_bg = narrow.style.bg_color;
    let _ = resolve_class_variants(
        &mut narrow,
        &VariantContext {
            viewport_w: 600.0,
            ..Default::default()
        },
    );
    assert_eq!(narrow.style.bg_color, base_bg);
    let _ = resolve_class_variants(
        &mut narrow,
        &VariantContext {
            viewport_w: 598.5,
            ..Default::default()
        },
    );
    assert_ne!(narrow.style.bg_color, base_bg);

    let mut band = Element::new();
    apply_class_list(
        &mut band,
        "bg-transparent min-[800px]:max-[1199px]:bg-accent",
    );
    let base_bg = band.style.bg_color;
    let _ = resolve_class_variants(
        &mut band,
        &VariantContext {
            viewport_w: 799.0,
            ..Default::default()
        },
    );
    assert_eq!(band.style.bg_color, base_bg);
    let _ = resolve_class_variants(
        &mut band,
        &VariantContext {
            viewport_w: 1000.0,
            ..Default::default()
        },
    );
    assert_ne!(band.style.bg_color, base_bg);
    let _ = resolve_class_variants(
        &mut band,
        &VariantContext {
            viewport_w: 1200.0,
            ..Default::default()
        },
    );
    assert_eq!(band.style.bg_color, base_bg);
}

#[test]
fn group_and_peer_markers_set_attrs() {
    let mut el = Element::new();
    apply_class_list(&mut el, "group");
    assert_eq!(el.attrs.get("jadix_group").map(|s| s.as_str()), Some("1"));
    let mut el = Element::new();
    apply_class_list(&mut el, "group/card peer/input");
    assert_eq!(
        el.attrs.get("jadix_group").map(|s| s.as_str()),
        Some("card")
    );
    assert_eq!(
        el.attrs.get("jadix_peer").map(|s| s.as_str()),
        Some("input")
    );
}

#[test]
fn resolve_group_hover_with_context() {
    let mut el = Element::new();
    apply_class_list(&mut el, "bg-transparent group-hover:bg-accent");
    let base_bg = el.style.bg_color;
    let groups = [None];
    let _ = resolve_class_variants(
        &mut el,
        &VariantContext {
            group_hovered: &groups,
            ..Default::default()
        },
    );
    assert_ne!(el.style.bg_color, base_bg);
}
