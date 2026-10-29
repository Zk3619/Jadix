use super::*;
use std::collections::HashSet;

#[test]
fn portal_composite_ignores_offscreen_nodes_before_union() {
    let portal = PaintDirty {
        key: 1,
        bounds: [320.0, 340.0, 214.0, 24.0],
        kind: PaintDirtyKind::Overlay,
    };
    let viewport = [0.0, 0.0, 1000.0, 640.0];
    let expected = portal_composite_damage(&[portal], Some(portal.bounds), viewport).unwrap();
    let offscreen = PaintDirty {
        key: 2,
        bounds: [305.0, 3384.0, 214.0, 24.0],
        kind: PaintDirtyKind::Node,
    };
    assert_eq!(
        portal_composite_damage(&[portal, offscreen], Some(portal.bounds), viewport),
        Some(expected)
    );
    assert!(expected[2] < 300.0 && expected[3] < 100.0);
}

#[test]
fn scaled_preview_damage_includes_scaled_outline_outset() {
    let mut element = Element::new()
        .outline([0.0, 0.0, 0.0, 1.0], 3.0)
        .outline_offset(2.0);
    for scale in [0.35, 0.57, 1.0, 1.5] {
        element.paint_ancestor_scale = [scale; 2];
        assert!((element_paint_outset_pad(&element, 0.0) - 5.0 * scale).abs() < 0.001);
    }
}

#[test]
fn clean_chrome_preserves_retain_for_scroll_strip_copy() {
    assert!(!force_full_redraw_after_chrome(
        LayoutStatus::Clean,
        LayoutStatus::Clean,
        false,
        false,
        false,
        false
    ));
    assert!(force_full_redraw_after_chrome(
        LayoutStatus::Clean,
        LayoutStatus::Clean,
        true,
        false,
        false,
        false
    ));
    assert!(force_full_redraw_after_chrome(
        LayoutStatus::LayoutChanged,
        LayoutStatus::Clean,
        false,
        false,
        false,
        false
    ));
    assert!(force_full_redraw_after_chrome(
        LayoutStatus::StyleChanged,
        LayoutStatus::Clean,
        false,
        false,
        false,
        false
    ));
}

#[test]
fn paint_kind_for_modal_chrome_is_overlay() {
    let mut overlay = Element::new();
    overlay.template_id = Some("dlg__modal_overlay".into());
    assert_eq!(paint_kind_for_element(&overlay), PaintDirtyKind::Overlay);
    let mut hoisted = Element::new();
    hoisted.set_attr("jadix_hoisted_portal", "1");
    assert_eq!(paint_kind_for_element(&hoisted), PaintDirtyKind::Overlay);
    assert_eq!(
        paint_kind_for_element(&Element::new()),
        PaintDirtyKind::Node
    );
    let mut floating = Element::new();
    floating.template_id = Some("cal-month__floating_panel".into());
    assert_eq!(
        paint_kind_for_element(&floating),
        PaintDirtyKind::Overlay,
        "Select/Popover fade chrome must not dest-reset as a page Node"
    );
}

#[test]
fn rounded_translucent_hover_chrome_keeps_node_damage() {
    let mut el = Element::new()
        .rounded(4.0)
        .bg([0.20, 0.20, 0.20, 0.18])
        .on_hover(|_| {});
    el.style.bg_color[3] = 0.0;
    let _ = crate::style::apply_class_list(&mut el, "hover:bg-#202020");
    el.style.bg_color[3] = 0.18;
    assert_eq!(
        paint_kind_for_element(&el),
        PaintDirtyKind::Node,
        "rounded hover chrome must keep its own paint slot"
    );

    // Exercise both directions repeatedly: the leave state is transparent,
    // while the enter state is an opaque hover fill over the same rounded box.
    for alpha in [1.0, 0.0].into_iter().cycle().take(120) {
        el.style.bg_color[3] = if alpha == 1.0 { 1.0 } else { 0.0 };
        assert_eq!(
            paint_kind_for_element(&el),
            PaintDirtyKind::Node,
            "hover transition sample with alpha={alpha} changed paint ownership"
        );
    }
}

#[test]
fn local_chart_overlay_replays_its_page_band() {
    let mut overlay = Element::new();
    overlay.paint_region_replay = true;
    assert_eq!(
        paint_kind_for_element(&overlay),
        PaintDirtyKind::Region,
        "moving a local tooltip/crosshair must replay the chart underlay"
    );
}

#[test]
fn scaled_page_animation_replays_its_composite() {
    let mut node = Element::new();
    assert_eq!(paint_kind_for_element(&node), PaintDirtyKind::Node);
    node.paint_ancestor_scale = [0.57; 2];
    assert_eq!(paint_kind_for_element(&node), PaintDirtyKind::Region);
    node.paint_ancestor_scale = [1.25; 2];
    assert_eq!(paint_kind_for_element(&node), PaintDirtyKind::Region);
    node.paint_overlay_stamp = Some(42);
    assert_eq!(paint_kind_for_element(&node), PaintDirtyKind::Overlay);
}

#[test]
fn fading_scrollbar_replays_its_fringe_and_preserves_portal_membership() {
    crate::core::reactivity::create_root(|| {
        let mut thumb = Element::new().focus_id("scrollbar-thumb:composite-fade:y");
        thumb.layout_bounds = [90.0, 10.0, 8.0, 60.0];
        assert_eq!(paint_kind_for_element(&thumb), PaintDirtyKind::Region);
        let _ = take_frame_paint_nodes();
        push_frame_paint_element(&thumb, [0.0; 2]);
        let dirty = take_frame_paint_nodes();
        assert_eq!(dirty.len(), 1);
        assert_eq!(dirty[0].kind, PaintDirtyKind::Region);
        assert_eq!(dirty[0].bounds, [88.0, 8.0, 12.0, 64.0]);
        thumb.paint_overlay_stamp = Some(42);
        assert_eq!(paint_kind_for_element(&thumb), PaintDirtyKind::Overlay);
    });
}

#[test]
fn trigger_hover_keeps_shadow_fringe_and_skips_menu_panel() {
    // Trigger 32px. Menu panel starts 4px below. Md halo (blur 10 → SDF ~22)
    // covers the trigger's lower half; the panel itself does not.
    let trigger = [10.0, 40.0, 220.0, 32.0];
    let halo = [4.0, 54.0, 232.0, 120.0];
    let panel = [10.0, 76.0, 220.0, 96.0];
    let cut_halo = page_scissors_excluding_overlay(trigger, Some(halo));
    assert!(
        cut_halo.iter().all(|r| r[1] + r[3] <= halo[1] + 0.5),
        "halo subtract clips the hover fill, parts={cut_halo:?}"
    );
    // Button hover dest-resets as Node. Present must use layout-box subtract
    // (same as Region) — halo subtract is what stacks the portal shadow darker
    // and flickers the clipped hover fill under AnchoredOverlay.
    let kept = page_scissors_excluding_portal_boxes(trigger, &[(1, panel)]);
    assert_eq!(
        kept,
        vec![trigger],
        "Node/Region present must keep the full trigger (incl. shadow fringe)"
    );
    let into_panel = [10.0, 40.0, 220.0, 80.0];
    let parts = page_scissors_excluding_portal_boxes(into_panel, &[(1, panel)]);
    assert!(
        parts.iter().all(|r| intersect_rects(*r, panel).is_none()),
        "panel pixels must stay out of the page clear, parts={parts:?}"
    );
    assert!(
        parts.iter().any(|r| r[3] > 30.0),
        "shadow fringe on the trigger must remain, parts={parts:?}"
    );
    // Hover transition ticks must CLEAR the trigger face but leave the
    // portal halo ring in retain — wiping it forced Overlay punch-replay
    // every ~150ms transition-colors frame (卡顿 + 悬停中间态).
    let coverage = overlay_replay_coverage(&[(1, panel)], Some(halo));
    let hover_wipe = [10.0, 40.0, 220.0, 48.0];
    let after_stamp = page_scissors_excluding_portal_boxes(hover_wipe, &[(1, panel)]);
    let preserved = page_node_present_scissors_preserving_portal_halo(
        after_stamp,
        trigger,
        coverage,
        &[(1, panel)],
    );
    assert!(
        preserved
            .iter()
            .any(|r| intersect_rects(*r, trigger).is_some_and(|i| i[2] * i[3] > 100.0)),
        "trigger face must still dest-reset for hover fill; preserved={preserved:?}"
    );
    let gap = [10.0, 72.0, 220.0, 4.0];
    assert!(
        preserved.iter().all(|r| intersect_rects(*r, gap)
            .map(|i| i[2] * i[3] < 1.0)
            .unwrap_or(true)),
        "gap halo ring must stay in retain; preserved={preserved:?}"
    );
    // Face under the halo still CLEARs (hover fill) and therefore overlaps
    // coverage — but Node encode must not punch that overlap.
    assert!(
        preserved
            .iter()
            .any(|p| overlay_page_punch_overlap(coverage, *p).is_some()),
        "fixture: face under halo intersects coverage; preserved={preserved:?}"
    );
    assert!(
        !page_dirty_punches_overlay_after_clear(PaintDirtyKind::Node),
        "Node hover must stay page-local after halo preserve"
    );
    assert!(
        page_dirty_punches_overlay_after_clear(PaintDirtyKind::Region),
        "Region clears under coverage still punch"
    );
    assert!(
        page_dirty_punches_overlay_after_clear(PaintDirtyKind::OverlayRestore),
        "Restore clears under coverage still punch"
    );
}

#[test]
fn node_present_uses_capture_text_mesh_only() {
    assert!(
        node_present_skips_page_stack_glyphon(true, false),
        "Node Present capture must remain the only text submission source for changed captions"
    );
    assert!(!node_present_skips_page_stack_glyphon(false, false));
    assert!(!node_present_skips_page_stack_glyphon(true, true));
}

#[test]
fn node_present_caption_change_must_not_submit_page_and_capture_glyphs_twice() {
    // A button caption is part of the Node capture after its face is cleared.
    // Submitting the changed page band as well draws the same glyphon run a
    // second time in the same scissor and makes unchanged neighboring text
    // appear to flicker while rapidly toggling the trigger.
    assert!(
        node_present_skips_page_stack_glyphon(true, false),
        "Node Present must have one text submission source even when a trigger caption changed"
    );
}

#[test]
fn page_caption_change_must_not_prepare_overlay_glyphon() {
    // Open AnchoredOverlay + button transition-colors: the frame's dirties
    // are the trigger Node only. Reshaping the popover mesh every tick is
    // the hover-switch hitch, and drawing that mesh onto the caption is
    // the bold intermediate stroke.
    assert!(
        !overlay_glyphon_prepare_needed(true, &[PaintDirtyKind::Node]),
        "page-node damage, including a changed trigger caption, must reuse the portal glyphon mesh"
    );
    assert!(overlay_glyphon_prepare_needed(
        true,
        &[PaintDirtyKind::Node, PaintDirtyKind::Overlay]
    ));
    assert!(overlay_glyphon_prepare_needed(
        true,
        &[PaintDirtyKind::OverlayRestore]
    ));
    assert!(overlay_glyphon_prepare_needed(
        true,
        &[PaintDirtyKind::Region]
    ));
    assert!(overlay_glyphon_prepare_needed(
        false,
        &[PaintDirtyKind::Node]
    ));
}

#[test]
fn hoisted_overlay_descendant_hover_scale_dirties_portal_stamp() {
    // +N expand chips live under `__floating_panel` but have no chrome template_id.
    // Classifying them as page `Node` lets `page_scissor_excluding_overlay` drop
    // every hover:scale frame (fully inside occupancy, not a punch). The overlay
    // pass only encodes Overlay-kind keys, so GPU retains rest until occupancy
    // jumps — rest↔1.25 snap instead of the 150ms tween.
    let _ = take_frame_paint_nodes();
    let mut chip = Element::new().w(16.0).h(16.0);
    crate::style::apply_class_list(
        &mut chip,
        "cursor-pointer transition-transform duration-150 ease-out hover:scale-125",
    );
    let mut panel = Element::new().w(80.0).h(32.0).child(chip);
    panel.template_id = Some("rx__floating_panel".into());
    panel.set_attr("jadix_portal", "1");
    panel.set_attr(crate::layout::layout_tree::ATTR_PORTAL_KEY, "rx-expand");

    let mut root = Element::new().w(400.0).h(400.0).child(panel);
    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 400.0, 400.0, [-1.0, -1.0]);

    fn find_hover_scale_chip(el: &Element) -> Option<&Element> {
        if el
            .class_set
            .as_ref()
            .is_some_and(crate::style::ClassSet::has_pointer_hover_linear_xform)
        {
            return Some(el);
        }
        el.children.iter().find_map(find_hover_scale_chip)
    }
    fn find_hoisted(el: &Element) -> Option<&Element> {
        if el.attrs.get("jadix_hoisted_portal").map(String::as_str) == Some("1") {
            return Some(el);
        }
        el.children.iter().find_map(find_hoisted)
    }
    let chip = find_hover_scale_chip(&root).expect("hover:scale chip");
    let panel = find_hoisted(&root).expect("hoisted panel");
    let stamp = crate::layout::layout_tree::overlay_portal_stamp_key(panel);

    assert_eq!(
        paint_kind_for_element(chip),
        PaintDirtyKind::Overlay,
        "overlay descendants must Overlay-dirty; page Node is scissored out of occupancy"
    );
    let chip_box = crate::core::tree::visual_paint_aabb(chip, chip.paint_scroll_acc);
    let occ = crate::core::tree::visual_paint_aabb(panel, panel.paint_scroll_acc);
    assert!(
        page_scissor_excluding_overlay(chip_box, Some(occ)).is_none(),
        "chip box is inside overlay occupancy — a page Node dirty would never present"
    );

    let _ = take_frame_paint_nodes();
    push_frame_paint_element(chip, chip.paint_scroll_acc);
    let dirties = take_frame_paint_nodes();
    assert_eq!(dirties.len(), 1, "expected one dirty, got {dirties:?}");
    assert_eq!(dirties[0].kind, PaintDirtyKind::Overlay);
    assert_eq!(
        dirties[0].key, stamp,
        "descendant Overlay dirty must use the host portal stamp so the band encodes"
    );
    assert!(
        overlay_band_should_encode(stamp, &[dirties[0].key], false, &[stamp], &[stamp]),
        "chip hover:scale must encode the host overlay band"
    );
}

#[test]
fn push_frame_paint_element_without_halo_stays_on_layout_box() {
    let _ = take_frame_paint_nodes();
    let mut el = Element::new();
    el.layout_bounds = [10.0, 20.0, 100.0, 30.0];
    push_frame_paint_element(&el, [0.0, 0.0]);
    let dirties = take_frame_paint_nodes();
    assert_eq!(dirties.len(), 1, "expected one dirty node, got {dirties:?}");
    let b = dirties[0].bounds;
    assert!(
        (b[0] - 10.0).abs() < 0.01
            && (b[1] - 20.0).abs() < 0.01
            && (b[2] - 100.0).abs() < 0.01
            && (b[3] - 30.0).abs() < 0.01,
        "no-halo node must dirty the visual layout box, not vertex-AA pad, got {b:?}"
    );
}

#[test]
fn button_group_hover_dirty_includes_shared_edge_paint_nudge() {
    let _ = take_frame_paint_nodes();
    let mut el = Element::new();
    el.layout_bounds = [10.0, 20.0, 100.0, 30.0];
    el.set_attr("jadix_button_group_nudge", "x");

    push_frame_paint_element(&el, [0.0, 0.0]);
    let dirties = take_frame_paint_nodes();
    assert_eq!(dirties.len(), 1, "expected one dirty node, got {dirties:?}");
    let b = dirties[0].bounds;
    assert_eq!(b, [9.0, 20.0, 101.0, 30.0]);
}

#[test]
#[cfg(feature = "product-tests")]
fn button_group_joined_damage_box_matches_layout() {
    let _ = take_frame_paint_nodes();
    let mut el = Element::new();
    el.layout_bounds = [10.0, 20.0, 100.0, 30.0];
    el.set_attr(crate::components::button_group::ATTR_GROUP_EDGE, "middle");

    push_frame_paint_element(&el, [0.0, 0.0]);
    let dirties = take_frame_paint_nodes();
    assert_eq!(dirties.len(), 1, "expected one dirty node, got {dirties:?}");
    let b = dirties[0].bounds;
    assert_eq!(
        b,
        [10.0, 20.0, 100.0, 30.0],
        "a joined fill must dirty its layout box; shrinking it splits the group frame"
    );
}

#[test]
fn push_frame_paint_skips_transparent_shown_enter_without_product_slot() {
    let _ = take_frame_paint_nodes();
    let mut el = Element::new();
    el.layout_bounds = [120.0, 340.0, 80.0, 32.0];
    el.style.opacity = 0.0;
    push_frame_paint_element(&el, [0.0, 0.0]);
    assert!(
        take_frame_paint_nodes().is_empty(),
        "opacity≈0 enter must not dest-reset empty fill+shadow"
    );

    el.transition_snapshot.mark_settled_this_tick();
    push_frame_paint_element(&el, [0.0, 0.0]);
    assert_eq!(
        take_frame_paint_nodes().len(),
        1,
        "hide settle still dest-resets the last faded box"
    );

    el.transition_snapshot = Default::default();
    el.style.opacity = 0.0;
    let baseline = crate::style::AnimationBaseline::from_style(&el.style);
    el.animation_snapshot
        .runs
        .push(crate::style::AnimationRunState {
            elapsed_ms: 200.0,
            baseline,
            finished: true,
            entry_fp: 0,
        });
    push_frame_paint_element(&el, [0.0, 0.0]);
    assert_eq!(
        take_frame_paint_nodes().len(),
        1,
        "finished keyframe fade-out must dest-reset (Dialog exit)"
    );
}

#[test]
fn overlay_expand_clip_dirty_must_not_use_unconstrained_height() {
    let _ = take_frame_paint_nodes();
    let mut el = Element::new();
    el.template_id = Some("cal-month__floating_panel".into());
    el.layout_bounds = [100.0, 400.0, 144.0, 220.0];
    el.style.max_size.height = taffy::style::Dimension::length(40.0);
    push_frame_paint_element(&el, [0.0, 0.0]);
    let dirties = take_frame_paint_nodes();
    assert_eq!(
        dirties.len(),
        1,
        "expected one overlay dirty, got {dirties:?}"
    );
    let b = dirties[0].bounds;
    assert_eq!(dirties[0].kind, PaintDirtyKind::Overlay);
    assert!(
        b[3] < 80.0,
        "expand Overlay dirty must be the max-height clip, not pinned content height; got {b:?}"
    );
}

#[test]
fn overlay_same_size_fade_is_not_restore() {
    let panel = [90.0, 140.0, 40.0, 160.0];
    assert!(
        !overlay_restore_needed(Some(panel), Some(panel)),
        "same-size occupancy is not a shrink — OverlayRestore Clear punches the panel"
    );
    assert!(!overlay_restore_needed(None, Some(panel)));
    assert!(
        overlay_partial_load_retains_page(true, false, true, true),
        "fade PartialLoad must keep page glyphon retain"
    );
}

#[test]
fn overlay_same_size_fade_replace_does_not_arm_restore_clear() {
    assert!(
        overlay_fade_replace_needed(true, true, true, false, true),
        "occupancy + Overlay dirty must replace retain (shadow stack / lost exit)"
    );
    assert!(
        !overlay_fade_replace_needed(false, true, true, false, true),
        "first paint has no prior overlay pixels to replace"
    );
    assert!(
        overlay_fade_replace_needed(true, true, true, true, true),
        "shrink while mounted replaces the previous stamp — leftover-strip Clear cuts captions"
    );
    assert!(!overlay_fade_replace_needed(true, true, false, false, true));
    assert!(
        overlay_fade_replace_needed(true, true, true, false, false),
        "expand accordion CSS tick must replace the previous keyframe (src-over stacks shadow)"
    );
    assert!(
        !overlay_fade_replace_needed(true, false, true, true, true),
        "unmount restore uses overlay_restore_clear so page glyphs return in the hole"
    );
    assert!(
        overlay_partial_load_retains_page(true, false, true, true),
        "fade replace Restore must keep page glyphon retain"
    );
    assert!(
        !overlay_partial_load_retains_page(true, true, true, true),
        "unmount restore_clear still repaints page glyphs in the hole"
    );
}

#[test]
fn overlay_primitive_encode_is_once_per_present() {
    assert_eq!(overlay_primitive_encode_count(0), 0);
    assert_eq!(overlay_primitive_encode_count(1), 1);
    assert_eq!(overlay_primitive_encode_count(2), 1);
    assert_eq!(overlay_primitive_encode_count(5), 1);
}

#[test]
fn overlay_suffix_must_not_submit_without_physical_scissor() {
    assert!(
        !overlay_suffix_submitted(None),
        "leftover page Node scissor clips the panel to the trigger (invisible but hittable)"
    );
    assert!(overlay_suffix_submitted(Some((10, 20, 40, 160))));
}

#[test]
fn page_trigger_scissor_must_not_enter_overlay_occupancy() {
    let occ = [90.0, 140.0, 40.0, 160.0];
    let trigger = [90.0, 110.0, 40.0, 32.0];
    let trigger_scissor = crate::app::draw_cull::padded_xywh(trigger, 8.0);
    assert!(
        overlay_page_punch_overlap(Some(occ), trigger_scissor).is_some(),
        "Select trigger pad overlaps the dropdown"
    );
    let clipped = page_scissor_excluding_overlay(trigger_scissor, Some(occ))
        .expect("trigger still has page chrome above the menu");
    assert!(
        intersect_rects(clipped, occ).is_none(),
        "clipped trigger scissor must not dest-reset overlay pixels"
    );
    assert!(
        !overlay_page_dirty_punches_occupancy(trigger_scissor, Some(clipped), Some(occ)),
        "a single-AABB clip is not a punch — overlay stays in retain"
    );
    assert!(
        overlay_encode_scissor(None, Some(occ), None).is_none(),
        "idle overlay stays in retain — do not src-over the halo again"
    );
    let inside = [90.0, 150.0, 40.0, 20.0];
    assert!(
        page_scissor_excluding_overlay(inside, Some(occ)).is_none(),
        "page dirty entirely in the overlay must not dest-reset as a Node"
    );
}

#[test]
fn page_trigger_under_modal_scrim_skips_dest_reset() {
    // Gallery AlertDialog: the clicked trigger sits under the fullscreen scrim,
    // not under the centered panel. Hover dest-reset of that caption punches
    // through unless occupancy includes the scrim (not just the card).
    let scrim = [0.0, 0.0, 1280.0, 800.0];
    let trigger = [24.0, 80.0, 96.0, 32.0];
    let trigger_scissor = crate::app::draw_cull::padded_xywh(trigger, 8.0);
    assert!(
        overlay_page_punch_overlap(Some(scrim), trigger_scissor).is_some(),
        "clicked trigger is inside the scrim"
    );
    assert!(
        page_scissor_excluding_overlay(trigger_scissor, Some(scrim)).is_none(),
        "dest-reset of the trigger would wipe scrim pixels and redraw the caption on top"
    );
    assert!(
        page_node_present_occluded_by_overlay(trigger, &[(1, scrim)]),
        "glyph capture/flush must skip the same trigger or the caption src-overs the scrim"
    );
    let neighbor = [200.0, 80.0, 96.0, 32.0];
    assert!(
        !page_node_present_occluded_by_overlay(neighbor, &[(1, [90.0, 140.0, 40.0, 160.0])]),
        "a dropdown stamp that misses the trigger must still present its glyphs"
    );
    let panel = [10.0, 76.0, 220.0, 96.0];
    let above = [10.0, 40.0, 220.0, 32.0];
    assert!(
        !page_node_present_occluded_by_overlay(above, &[(1, panel)]),
        "popover layout stamp must not occlude the trigger above the panel"
    );
}

#[test]
fn l_shaped_page_scissor_keeps_largest_remainder_outside_overlay() {
    let occ = [90.0, 100.0, 80.0, 80.0];
    let corner = [80.0, 90.0, 40.0, 40.0];
    assert!(subtract_rect(corner, occ).len() > 1);
    let clipped = page_scissor_excluding_overlay(corner, Some(occ))
        .expect("corner still has page chrome outside the overlay");
    assert!(
        intersect_rects(clipped, occ).is_none(),
        "largest remainder must not dest-reset overlay pixels"
    );
    assert!(!overlay_page_dirty_punches_occupancy(
        corner,
        Some(clipped),
        Some(occ)
    ));
}

#[test]
fn overlay_encode_scissor_unions_occupancy_so_fade_dirty_cannot_clip_panel() {
    let fade_document = [0.0, 8000.0, 40.0, 160.0];
    let occ = [90.0, 140.0, 40.0, 160.0];
    let scissor = overlay_encode_scissor(Some(fade_document), Some(occ), None)
        .expect("occupancy must keep the panel in the encode scissor");
    assert!(
        intersect_rects(scissor, occ).is_some(),
        "fade dirty in document space must not clip overlay to an off-screen hole"
    );
}

#[test]
fn overlay_encode_scissor_clips_overflow_dirty_to_stamp() {
    // Expand pin reports content height; accordion clip is the stamp.
    let clip = [100.0, 400.0, 144.0, 40.0];
    let tall = [100.0, 400.0, 144.0, 220.0];
    let scissor = overlay_encode_scissor(Some(tall), Some(clip), None)
        .expect("clip occupancy must keep the growing menu in scissor");
    assert!(
        intersect_rects(scissor, clip).is_some(),
        "encode must still cover the clip stamp; scissor={scissor:?}"
    );
    assert!(
            scissor[3] < 120.0,
            "overflow dirty must not reopen the measured height as a transparent hole; scissor={scissor:?}"
        );
}

#[test]
fn expand_width_overflow_dirty_does_not_widen_encode_past_stamp() {
    // AvatarGroup exit: max-width clamps the stamp, but the last face's
    // Overlay dirty still sits at the expanded x. Unioning it paints F and
    // the unclipped right border; restore only covers the stamp.
    let stamp = [16.0, 54.0, 120.0, 40.0];
    let face_f = [200.0, 58.0, 32.0, 32.0];
    let wide = union_rects(stamp, face_f);
    let scissor = overlay_encode_scissor(Some(wide), Some(stamp), None)
        .expect("clip stamp stays in the encode scissor");
    let hits =
        |r: [f32; 4], x: f32, y: f32| x >= r[0] && x < r[0] + r[2] && y >= r[1] && y < r[1] + r[3];
    assert!(
        hits(scissor, 40.0, 70.0),
        "the clamped pill must still encode, scissor={scissor:?}"
    );
    assert!(
        !hits(scissor, 210.0, 70.0),
        "overflow face must not widen the scissor past the stamp, scissor={scissor:?}"
    );
    assert!(
        hits(scissor, 40.0, 46.0),
        "the stamp halo pad still covers the top shadow, scissor={scissor:?}"
    );
}

const PARENT_KEY: u64 = 1;
const NESTED_KEY: u64 = 2;
const PANEL_KEY: u64 = 3;
const COLOR_KEY: u64 = 4;

#[test]
fn dialog_exit_panel_stamp_must_not_punch_scrim_without_redraw() {
    // AlertDialog: overlay and the centering stage are sibling portals. The
    // stage layout box is fullscreen (`inset 0` / 100%), but CSS ticks the
    // inner `{id}__modal_zoom` card which inherits the stage stamp.
    // Dest-reset of that empty host wiped the scrim; overlay_band_should_encode
    // then skipped the underlay because the dirty key is the panel stamp —
    // "遮罩瞬间关闭、稍等后弹窗也瞬间关闭".
    const OVERLAY: u64 = 0xA1;
    const STAGE: u64 = 0xA2;
    let vp = [0.0, 0.0, 800.0, 600.0];
    let card = [144.0, 120.0, 512.0, 280.0];
    let stamps = [(OVERLAY, vp), (STAGE, vp)];
    let holes = overlay_mounted_replace_holes(&stamps, &stamps, STAGE, Some(card));
    let flags = overlay_band_encode_flags(&[OVERLAY, STAGE], &[STAGE], false, &[OVERLAY, STAGE]);
    let punches = holes.iter().any(|h| damage_covers_full(vp, *h));
    assert!(
            !punches || flags[0],
            "panel-stamp dest-reset must not wipe the fullscreen scrim without encoding the overlay band; holes={holes:?} flags={flags:?}"
        );
    let occ = overlay_encode_occupancy(STAGE, Some(card), &stamps, Some(vp));
    assert!(
        occ.is_some_and(|b| !damage_covers_full(vp, b)),
        "zoom occupancy must be the card, not the empty fullscreen stage; occ={occ:?}"
    );
}

#[test]
fn dialog_exit_overlay_stamp_must_not_punch_card_without_redraw() {
    // Inverse of panel-stamp: overlay fade dest-resets the fullscreen scrim.
    // Without promoting STAGE, the card is wiped and later pops off.
    const OVERLAY: u64 = 0xA1;
    const STAGE: u64 = 0xA2;
    let vp = [0.0, 0.0, 800.0, 600.0];
    let stamps = [(OVERLAY, vp), (STAGE, vp)];
    let holes = overlay_mounted_replace_holes(&stamps, &stamps, OVERLAY, Some(vp));
    let mut flags =
        overlay_band_encode_flags(&[OVERLAY, STAGE], &[OVERLAY], false, &[OVERLAY, STAGE]);
    overlay_band_promote_dest_reset_siblings(
        &mut flags,
        &[OVERLAY, STAGE],
        &[OVERLAY],
        &stamps,
        &holes,
    );
    let punches = holes.iter().any(|h| damage_covers_full(vp, *h));
    assert!(
            !punches || flags[1],
            "overlay-stamp dest-reset must not wipe the fullscreen stage/card without encoding the stage band; holes={holes:?} flags={flags:?}"
        );
}

#[test]
fn dialog_button_hover_dest_reset_must_cover_card_halo() {
    // Footer OK sits on the card corner. Overlay encode pads occupancy 16px;
    // dest-reset of the chip alone leaves elevated chrome in that pad.
    const OVERLAY: u64 = 0xA1;
    const STAGE: u64 = 0xA2;
    let vp = [0.0, 0.0, 800.0, 600.0];
    let card = [144.0, 120.0, 512.0, 280.0];
    let button = [560.0, 352.0, 72.0, 32.0];
    let stamps = [(OVERLAY, vp), (STAGE, vp)];
    let holes = overlay_mounted_replace_holes(&stamps, &stamps, STAGE, Some(button));
    let occ = overlay_encode_occupancy(STAGE, Some(button), &stamps, Some(vp));
    let scissor = overlay_encode_scissor(Some(button), occ, None)
        .expect("button hover must still have an overlay encode scissor");
    let card_halo = crate::app::draw_cull::padded_xywh(card, 16.0);
    let halo_in_scissor =
        intersect_rects(card_halo, scissor).expect("OK pad must overlap the elevated card chrome");
    assert!(
            holes
                .iter()
                .any(|h| damage_covers_full(halo_in_scissor, *h)),
            "button hover dest-reset must cover card chrome in the encode scissor or src-over stacks the halo; holes={holes:?} scissor={scissor:?}"
        );
    assert!(
        !holes.iter().any(|h| damage_covers_full(vp, *h)),
        "button hover must not dest-reset the fullscreen viewport; holes={holes:?}"
    );
    let mut flags =
        overlay_band_encode_flags(&[OVERLAY, STAGE], &[STAGE], false, &[OVERLAY, STAGE]);
    overlay_band_promote_dest_reset_siblings(
        &mut flags,
        &[OVERLAY, STAGE],
        &[STAGE],
        &stamps,
        &holes,
    );
    let punches_scrim = holes
        .iter()
        .any(|h| overlay_box_extends_beyond(*h, card_halo, 0.5));
    assert!(
            !punches_scrim || flags[0],
            "expanded button dest-reset that leaves the card halo must encode the scrim; holes={holes:?} flags={flags:?}"
        );
}

#[test]
fn dialog_card_stamp_same_footprint_dirty_must_dest_reset_encode_halo() {
    // Runtime: empty centering stage records child-union occupancy (the card),
    // not the fullscreen host. A card-sized Overlay dirty is same-footprint.
    // Encode still pads 16px — dest-reset of the unpadded card stacks the
    // elevated chrome when footer Cancel/OK hover unions onto the card key.
    const OVERLAY: u64 = 0xA1;
    const STAGE: u64 = 0xA2;
    let vp = [0.0, 0.0, 800.0, 600.0];
    let card = [144.0, 120.0, 512.0, 280.0];
    let stamps = [(OVERLAY, vp), (STAGE, card)];
    let holes = overlay_mounted_replace_holes(&stamps, &stamps, STAGE, Some(card));
    let occ = overlay_encode_occupancy(STAGE, Some(card), &stamps, None);
    let scissor = overlay_encode_scissor(Some(card), occ, None)
        .expect("card dirty must still have an overlay encode scissor");
    assert!(
            holes.iter().any(|h| damage_covers_full(scissor, *h)),
            "same-footprint card dirty must dest-reset the encode-padded halo or hover stacks the shadow; holes={holes:?} scissor={scissor:?}"
        );
    assert!(
        !holes.iter().any(|h| damage_covers_full(vp, *h)),
        "card dest-reset must not punch the fullscreen scrim; holes={holes:?}"
    );
    let mut flags =
        overlay_band_encode_flags(&[OVERLAY, STAGE], &[STAGE], false, &[OVERLAY, STAGE]);
    overlay_band_promote_dest_reset_siblings(
        &mut flags,
        &[OVERLAY, STAGE],
        &[STAGE],
        &stamps,
        &holes,
    );
    let punches_scrim = holes
        .iter()
        .any(|h| overlay_box_extends_beyond(*h, card, 0.5));
    assert!(
            !punches_scrim || flags[0],
            "padded card dest-reset that spills into the dim ring must encode the scrim; holes={holes:?} flags={flags:?}"
        );
}

#[test]
fn dialog_button_hover_card_stamp_must_cover_nearby_halo() {
    const OVERLAY: u64 = 0xA1;
    const STAGE: u64 = 0xA2;
    let vp = [0.0, 0.0, 800.0, 600.0];
    let card = [144.0, 120.0, 512.0, 280.0];
    let button = [560.0, 352.0, 72.0, 32.0];
    let stamps = [(OVERLAY, vp), (STAGE, card)];
    let holes = overlay_mounted_replace_holes(&stamps, &stamps, STAGE, Some(button));
    let occ = overlay_encode_occupancy(STAGE, Some(button), &stamps, None);
    let scissor = overlay_encode_scissor(Some(button), occ, None)
        .expect("button hover must still have an overlay encode scissor");
    let card_halo = crate::app::draw_cull::padded_xywh(card, 16.0);
    let halo_in_scissor =
        intersect_rects(card_halo, scissor).expect("OK pad must overlap the elevated card chrome");
    assert!(
            holes
                .iter()
                .any(|h| damage_covers_full(halo_in_scissor, *h)),
            "runtime card-stamp button hover must dest-reset nearby chrome; holes={holes:?} scissor={scissor:?}"
        );
    assert!(
        !holes.iter().any(|h| damage_covers_full(vp, *h)),
        "button hover must not dest-reset the fullscreen viewport; holes={holes:?}"
    );
}

#[test]
fn dialog_cancel_hover_must_not_clip_card_bottom_chrome() {
    // Gallery Cancel sits just above the info card's 1px border. Dest-reset
    // / encode stay on pad(Cancel) so nearby chrome is wiped without PAGE
    // punching the 16px backdrop-blur ring around every card edge.
    const OVERLAY: u64 = 0xA1;
    const STAGE: u64 = 0xA2;
    let vp = [0.0, 0.0, 800.0, 600.0];
    let card = [144.0, 120.0, 512.0, 280.0];
    let cancel = [400.0, 360.0, 88.0, 32.0];
    let stamps = [(OVERLAY, vp), (STAGE, card)];
    let holes = overlay_mounted_replace_holes(&stamps, &stamps, STAGE, Some(cancel));
    let occ = overlay_encode_occupancy(STAGE, Some(cancel), &stamps, None);
    let scissor = overlay_encode_scissor(Some(cancel), occ, None)
        .expect("Cancel hover must still have an overlay encode scissor");
    let card_bottom = [card[0], card[1] + card[3] - 2.0, card[2], 2.0];
    let nearby = intersect_rects(card_bottom, scissor)
        .expect("Cancel pad must overlap the card bottom stroke");
    assert!(
            holes.iter().any(|h| damage_covers_full(nearby, *h)),
            "Cancel dest-reset must cover nearby bottom chrome in the encode scissor; holes={holes:?} nearby={nearby:?}"
        );
    let card_pad = crate::app::draw_cull::padded_xywh(card, 16.0);
    assert!(
            !holes.iter().any(|h| damage_covers_full(card_pad, *h)),
            "Cancel hover must not dest-reset pad(card) — that punches the blur ring; holes={holes:?} card_pad={card_pad:?}"
        );
    assert!(
            holes.iter().any(|h| damage_covers_full(scissor, *h)),
            "dest-reset must cover encode or PAGE shows through as residual; holes={holes:?} scissor={scissor:?}"
        );
    assert!(
            holes.iter().any(|h| damage_covers_full(*h, scissor)),
            "encode must cover dest-reset or leftover chrome stacks; holes={holes:?} scissor={scissor:?}"
        );
    assert!(
        !holes.iter().any(|h| damage_covers_full(vp, *h)),
        "Cancel hover must not dest-reset the fullscreen viewport; holes={holes:?}"
    );
}

#[test]
fn dialog_button_hover_must_not_punch_scrim_halo() {
    // Success AlertDialog: Cancel hover used to promote dest-reset to
    // pad(card). OverlayRestore PAGE-fills that 16px ring and the
    // backdrop-blur around every edge goes transparent.
    const OVERLAY: u64 = 0xA1;
    const STAGE: u64 = 0xA2;
    let vp = [0.0, 0.0, 800.0, 600.0];
    let card = [144.0, 120.0, 512.0, 280.0];
    let cancel = [400.0, 360.0, 88.0, 32.0];
    let stamps = [(OVERLAY, vp), (STAGE, card)];
    let holes = overlay_mounted_replace_holes(&stamps, &stamps, STAGE, Some(cancel));
    let occ = overlay_encode_occupancy(STAGE, Some(cancel), &stamps, None)
        .expect("Cancel hover must resolve occupancy");
    let card_pad = crate::app::draw_cull::padded_xywh(card, 16.0);
    let far_scrim = [card[0] - 16.0, card[1] - 16.0, 8.0, 8.0];
    assert!(
        !holes.iter().any(|h| damage_covers_full(card_pad, *h)),
        "button hover dest-reset must stay on the chip, not pad(card); holes={holes:?}"
    );
    assert!(
        !holes
            .iter()
            .any(|h| intersect_rects(*h, far_scrim).is_some()),
        "button hover must not PAGE-punch the far blur ring; holes={holes:?} far={far_scrim:?}"
    );
    assert!(
        !damage_covers_full(card, occ) && !damage_covers_full(card_pad, occ),
        "occupancy must stay chip-sized; occ={occ:?} card={card:?}"
    );
    let mut flags =
        overlay_band_encode_flags(&[OVERLAY, STAGE], &[STAGE], false, &[OVERLAY, STAGE]);
    overlay_band_promote_dest_reset_siblings(
        &mut flags,
        &[OVERLAY, STAGE],
        &[STAGE],
        &stamps,
        &holes,
    );
    let spills = holes
        .iter()
        .any(|h| overlay_box_extends_beyond(*h, card, 0.5));
    assert!(
            !spills || flags[0],
            "chip dest-reset that nips the dim ring must encode the scrim; holes={holes:?} flags={flags:?}"
        );
}

#[test]
fn dialog_enter_zoom_dirty_must_not_promote_card_dest_reset() {
    // zoom-in-90 is contained in the rest card (~90%) but is not a footer
    // chip. Promoting it dest-resets the rest card with PAGE while encode
    // is still opacity 0 — first-open looks like the dialog never appears.
    const OVERLAY: u64 = 0xA1;
    const STAGE: u64 = 0xA2;
    let vp = [0.0, 0.0, 800.0, 600.0];
    let card = [144.0, 120.0, 512.0, 280.0];
    let zoom = [
        card[0] + card[2] * 0.05,
        card[1] + card[3] * 0.05,
        card[2] * 0.90,
        card[3] * 0.90,
    ];
    let stamps = [(OVERLAY, vp), (STAGE, card)];
    let holes = overlay_mounted_replace_holes(&stamps, &stamps, STAGE, Some(zoom));
    let occ = overlay_encode_occupancy(STAGE, Some(zoom), &stamps, None);
    let scissor =
        overlay_encode_scissor(Some(zoom), occ, None).expect("enter zoom must still encode");
    let card_pad = crate::app::draw_cull::padded_xywh(card, 16.0);
    assert!(
            !holes.iter().any(|h| damage_covers_full(card_pad, *h)),
            "enter zoom dest-reset must not promote to the padded rest card; holes={holes:?} zoom={zoom:?}"
        );
    assert!(
        holes.iter().any(|h| damage_covers_full(scissor, *h)),
        "enter dest-reset must still cover encode; holes={holes:?} scissor={scissor:?}"
    );
    assert!(
        !holes.iter().any(|h| damage_covers_full(vp, *h)),
        "enter zoom must not dest-reset the fullscreen viewport; holes={holes:?}"
    );
}

#[test]
fn dialog_hover_encode_must_not_double_pad_past_dest_reset() {
    // Footer hover dest-resets and encodes pad(chip). They must mutually
    // cover that halo — promoting to pad(card) punches the blur ring, and
    // a chip encode without a matching hole src-overs nearby chrome thicker.
    const OVERLAY: u64 = 0xA1;
    const STAGE: u64 = 0xA2;
    let vp = [0.0, 0.0, 800.0, 600.0];
    let card = [144.0, 120.0, 512.0, 280.0];
    let button = [560.0, 352.0, 72.0, 32.0];
    let stamps = [(OVERLAY, vp), (STAGE, card)];
    let holes = overlay_mounted_replace_holes(&stamps, &stamps, STAGE, Some(button));
    let encode_dirty = overlay_suffix_encode_bounds(button);
    let occ = overlay_encode_occupancy(STAGE, Some(encode_dirty), &stamps, None);
    let scissor = overlay_encode_scissor(Some(encode_dirty), occ, None)
        .expect("raw hover dirty must still encode");
    assert!(
        holes.iter().any(|h| damage_covers_full(scissor, *h)),
        "dest-reset must cover encode; holes={holes:?} scissor={scissor:?}"
    );
    assert!(
        holes.iter().any(|h| damage_covers_full(*h, scissor)),
        "encode must cover dest-reset; holes={holes:?} scissor={scissor:?}"
    );
    let card_pad = crate::app::draw_cull::padded_xywh(card, 16.0);
    assert!(
        !holes.iter().any(|h| damage_covers_full(card_pad, *h)),
        "chip hover must not dest-reset pad(card); holes={holes:?}"
    );
    assert!(
        !holes.iter().any(|h| damage_covers_full(vp, *h)),
        "hover must not dest-reset the fullscreen viewport; holes={holes:?}"
    );
}

#[test]
fn dialog_sibling_overlay_dirties_encode_once() {
    const OVERLAY: u64 = 0xA1;
    const STAGE: u64 = 0xA2;
    let vp = [0.0, 0.0, 800.0, 600.0];
    let card = [144.0, 120.0, 512.0, 280.0];
    let stamps = [(OVERLAY, vp), (STAGE, vp)];
    let groups = overlay_merge_same_footprint_dirties(&[(OVERLAY, vp), (STAGE, card)], &stamps);
    assert_eq!(
        groups.len(),
        1,
        "fullscreen overlay+stage must encode once; groups={groups:?}"
    );
    assert!(
        groups[0].0.contains(&OVERLAY) && groups[0].0.contains(&STAGE),
        "merged group must keep both sibling keys; groups={groups:?}"
    );
    let parent = [80.0, 360.0, 280.0, 320.0];
    let nested = [88.0, 40.0, 144.0, 220.0];
    let pop = overlay_merge_same_footprint_dirties(
        &[(PARENT_KEY, parent), (NESTED_KEY, nested)],
        &[(PARENT_KEY, parent), (NESTED_KEY, nested)],
    );
    assert_eq!(
        pop.len(),
        2,
        "DatePicker + flipped Select must keep separate scissors; groups={pop:?}"
    );
}

#[test]
fn overlay_encode_scissor_must_keep_nested_menu_flipped_above_parent() {
    // DatePicker month Select places above the popover; the boxes do not overlap.
    let parent = [80.0, 360.0, 280.0, 320.0];
    let nested = [88.0, 40.0, 144.0, 220.0];
    assert!(intersect_rects(parent, nested).is_none());
    let stamps = [(PARENT_KEY, parent)];
    assert_eq!(
        overlay_encode_occupancy(NESTED_KEY, Some(nested), &stamps, Some(parent)),
        Some(nested),
        "flip-above dirty must not fall back to the DatePicker visual occupancy"
    );
    let nested_scissor =
        overlay_encode_scissor(Some(nested), Some(nested), None).expect("nested occupancy");
    assert!(intersect_rects(nested_scissor, nested).is_some());
    assert!(overlay_band_should_encode(
        NESTED_KEY,
        &[NESTED_KEY],
        false,
        &[PARENT_KEY, NESTED_KEY],
        &[PARENT_KEY, NESTED_KEY],
    ));
    assert!(
        !overlay_band_should_encode(
            PARENT_KEY,
            &[NESTED_KEY],
            false,
            &[PARENT_KEY, NESTED_KEY],
            &[PARENT_KEY, NESTED_KEY],
        ),
        "parent must not restack the DatePicker shadow during nested encode"
    );
    let leftover = overlay_restore_leftover_stamps(
        &[(PARENT_KEY, parent), (NESTED_KEY, nested)],
        &[(PARENT_KEY, parent)],
    );
    assert_eq!(leftover, vec![(NESTED_KEY, nested)]);
}

#[test]
fn nested_overlay_fade_replace_must_not_restore_sibling_union() {
    let parent = [80.0, 360.0, 280.0, 320.0];
    let nested = [88.0, 40.0, 144.0, 220.0];
    let fade = nested;
    let stamps = [(PARENT_KEY, parent), (NESTED_KEY, nested)];
    let restore = overlay_fade_replace_restore_boxes(NESTED_KEY, Some(fade), &stamps);
    assert_eq!(restore, vec![nested]);
    assert!(
        intersect_rects(restore[0], parent).is_none(),
        "Restore of the nested menu must not dest-reset the parent popover / page between them"
    );
    let encode_occ = overlay_encode_occupancy(
        NESTED_KEY,
        Some(fade),
        &stamps,
        rects_union(&[parent, nested]),
    );
    assert_eq!(encode_occ, Some(nested));
}

#[test]
fn nested_overlay_over_parent_must_not_restore_or_encode_parent() {
    let parent = [80.0, 360.0, 280.0, 320.0];
    let nested = [100.0, 400.0, 144.0, 220.0];
    assert!(intersect_rects(parent, nested).is_some());
    let fade = nested;
    let stamps = [(PARENT_KEY, parent), (NESTED_KEY, nested)];
    let restore = overlay_fade_replace_restore_boxes(NESTED_KEY, Some(fade), &stamps);
    assert_eq!(restore, vec![nested]);
    assert!(
        !rect_contains_rect(restore[0], parent),
        "Restore of the month menu must not dest-reset the DatePicker popover"
    );
    let encode_occ = overlay_encode_occupancy(
        NESTED_KEY,
        Some(fade),
        &stamps,
        rects_union(&[parent, nested]),
    );
    assert_eq!(
        encode_occ,
        Some(nested),
        "sibling-union occupancy must not replace the dirty's own portal"
    );
    let encode = overlay_encode_scissor(Some(fade), encode_occ, None).expect("nested scissor");
    assert!(
            !rect_contains_rect(encode, parent),
            "encode scissor must not cover the parent halo (would restack the shadow); encode={encode:?}"
        );

    let parent_only = [(PARENT_KEY, parent)];
    let join_holes = overlay_mounted_replace_holes(&parent_only, &stamps, NESTED_KEY, Some(fade));
    assert!(
            join_holes.is_empty(),
            "opening a nested menu has no prev stamp — OverlayRestore would punch the parent; holes={join_holes:?}"
        );
    let mounted_holes = overlay_mounted_replace_holes(&stamps, &stamps, NESTED_KEY, Some(fade));
    assert_eq!(
        mounted_holes,
        vec![nested],
        "nested keyframe replace dest-resets the menu stamp only"
    );
    let close_holes = overlay_mounted_replace_holes(&stamps, &parent_only, NESTED_KEY, Some(fade));
    assert_eq!(
        close_holes,
        vec![nested],
        "closing the nested menu dest-resets the menu stamp"
    );
    assert!(overlay_restore_is_closed_portal(
        &[NESTED_KEY],
        &[PARENT_KEY]
    ));
    assert!(!overlay_restore_is_closed_portal(
        &[NESTED_KEY],
        &[PARENT_KEY, NESTED_KEY]
    ));
    assert!(overlay_band_should_encode(
        NESTED_KEY,
        &[NESTED_KEY],
        false,
        &[PARENT_KEY, NESTED_KEY],
        &[PARENT_KEY, NESTED_KEY],
    ));
    assert!(!overlay_band_should_encode(
        PARENT_KEY,
        &[NESTED_KEY],
        false,
        &[PARENT_KEY, NESTED_KEY],
        &[PARENT_KEY, NESTED_KEY],
    ));
    assert!(
        overlay_band_should_encode(
            PARENT_KEY,
            &[NESTED_KEY],
            true,
            &[PARENT_KEY],
            &[PARENT_KEY]
        ),
        "close Restore must redraw the parent calendar in the hole"
    );
    assert!(
        overlay_band_should_encode(
            PARENT_KEY,
            &[NESTED_KEY],
            false,
            &[PARENT_KEY, NESTED_KEY],
            &[PARENT_KEY],
        ),
        "nested first-open: menu instances still live in the sole DatePicker band"
    );
    assert!(
        overlay_band_should_encode(0, &[NESTED_KEY], false, &[PARENT_KEY, NESTED_KEY], &[0]),
        "synthetic whole-suffix band still draws the nested menu"
    );
    assert!(
            overlay_band_should_encode(0, &[PARENT_KEY], false, &[PARENT_KEY], &[0, NESTED_KEY],),
            "DatePicker dirty whose portal-layer band is unkeyed must still encode when a nested Select already has a keyed band"
        );
    assert!(
        overlay_band_should_encode(
            PARENT_KEY,
            &[PARENT_KEY],
            false,
            &[PARENT_KEY],
            &[PARENT_KEY, NESTED_KEY],
        ),
        "opening the calendar encodes the DatePicker band, not only the nested Select"
    );
    assert_eq!(
            overlay_band_encode_flags(
                &[PARENT_KEY, 0, NESTED_KEY, 0],
                &[NESTED_KEY],
                false,
                &[PARENT_KEY, NESTED_KEY],
            ),
            vec![false, false, true, true],
            "ColorPicker/month stacking 0-key follows the dirty host; skipped DatePicker 0-key must not submit calendar glyphs"
        );
    assert_eq!(
        overlay_band_encode_flags(&[PARENT_KEY, 0], &[PARENT_KEY], false, &[PARENT_KEY],),
        vec![true, true],
        "ColorPicker SV/hue unkeyed stacking must submit with the ColorPicker stamp"
    );
    assert_eq!(
        overlay_band_encode_flags(
            &[PARENT_KEY, 0, COLOR_KEY, 0],
            &[COLOR_KEY],
            false,
            &[COLOR_KEY],
        ),
        vec![false, false, true, true],
        "ColorPicker join dirty must encode its own stamp, not the leftover DatePicker band"
    );
    assert_eq!(
        overlay_added_portal_stamps(&parent_only, &stamps)
            .into_iter()
            .map(|(_, b)| b)
            .collect::<Vec<_>>(),
        vec![nested]
    );
    assert_eq!(
        overlay_membership_paint_stamps(&parent_only, &stamps)
            .into_iter()
            .map(|(_, b)| b)
            .collect::<Vec<_>>(),
        vec![nested],
        "join Overlay dirty is the new menu only"
    );
    assert!(
        overlay_membership_paint_stamps(&stamps, &parent_only).is_empty(),
        "leave is OverlayRestore of leftover stamps, not an Overlay dirty of the closed key"
    );
    assert_eq!(
        overlay_restore_leftover_stamps(&stamps, &parent_only)
            .into_iter()
            .map(|(_, b)| b)
            .collect::<Vec<_>>(),
        vec![nested],
        "leave Restore is the leftover hole, not the remaining DatePicker"
    );
    let color = [640.0, 120.0, 280.0, 360.0];
    let leftover_dp = [64.0, 344.0, 312.0, 352.0];
    let prev_dp = [(PARENT_KEY, leftover_dp)];
    let cur_cp = [(COLOR_KEY, color)];
    assert_eq!(
        overlay_membership_paint_stamps(&prev_dp, &cur_cp),
        vec![(COLOR_KEY, color)],
        "opening ColorPicker must Overlay-paint ColorPicker, not leftover DatePicker"
    );
    assert_eq!(
        overlay_band_encode_flags(
            &[PARENT_KEY, 0, COLOR_KEY, 0],
            &[COLOR_KEY],
            false,
            &[COLOR_KEY],
        ),
        vec![false, false, true, true],
        "leftover DatePicker band must stay skipped while ColorPicker encodes"
    );
    let leftover_occ = overlay_encode_occupancy(
        NESTED_KEY,
        Some(nested),
        &parent_only,
        Some([64.0, 344.0, 312.0, 352.0]),
    );
    assert_eq!(
        leftover_occ,
        Some(nested),
        "closed nested hole must not fall back to the parent visual occupancy"
    );
    let leftover_scissor =
        overlay_encode_scissor(Some(nested), leftover_occ, None).expect("leave scissor");
    assert!(
            !rect_contains_rect(leftover_scissor, parent),
            "leave encode must stay on the hole; restacking the parent halo jumps the shadow; scissor={leftover_scissor:?}"
        );
}

#[test]
fn overlay_encode_occupancy_single_portal_covers_visual_halo() {
    let layout = [90.0, 140.0, 40.0, 160.0];
    let visual = [74.0, 124.0, 72.0, 192.0];
    let occ = overlay_encode_occupancy(
        PANEL_KEY,
        Some(layout),
        &[(PANEL_KEY, layout)],
        Some(visual),
    )
    .expect("single portal encode occupancy");
    assert!(
            occ[0] <= visual[0] + 1e-3 && occ[1] <= visual[1] + 1e-3,
            "encode occupancy must reach the dest-reset visual halo, not just the Taffy box; occ={occ:?}"
        );
    assert!(
        occ[0] + occ[2] + 1e-3 >= visual[0] + visual[2]
            && occ[1] + occ[3] + 1e-3 >= visual[1] + visual[3],
        "encode occupancy must cover visual occupancy; occ={occ:?} visual={visual:?}"
    );
}

#[test]
fn overlay_encode_occupancy_must_not_union_foreign_leftover_after_tab_switch() {
    // TabsContent unmounts gal-date; overlay_occupied still holds that popover.
    // Opening ColorPicker / gal-month must not clip encode to the leftover box
    // (Inline Calendar dest-reset sliver, picker appears not to open).
    let leftover = [80.0, 360.0, 280.0, 320.0];
    let color = [640.0, 88.0, 36.0, 32.0];
    assert!(intersect_rects(leftover, color).is_none());
    let occ = overlay_encode_occupancy(
        COLOR_KEY,
        Some(color),
        &[(COLOR_KEY, color)],
        Some(leftover),
    );
    assert_eq!(
        occ,
        Some(color),
        "leftover DatePicker occupancy is not this portal's shadow halo"
    );
    let scissor = overlay_encode_scissor(Some(color), occ, None).expect("color scissor");
    assert!(intersect_rects(scissor, color).is_some());
    assert!(
        intersect_rects(scissor, leftover).is_none(),
        "ColorPicker encode must not dest-reset the previous tab's DatePicker hole"
    );
}

#[test]
fn overlay_encode_scissor_must_not_clip_color_picker_popover_to_leftover_datepicker() {
    // Gallery 2b: ColorPicker is top-right; leftover DatePicker occupancy is the
    // calendar below the tabs. Expand-overflow KEEP occupancy clips the popover
    // to that leftover box — PartialLoad submits nothing where the panel is.
    let leftover = [80.0, 360.0, 280.0, 320.0];
    let popover = [640.0, 120.0, 280.0, 360.0];
    assert!(intersect_rects(leftover, popover).is_none());
    let unconstrained = union_rects(leftover, popover);
    let scissor = overlay_encode_scissor(Some(unconstrained), Some(leftover), None)
        .expect("ColorPicker encode scissor");
    assert!(
            intersect_rects(scissor, popover).is_some(),
            "ColorPicker popover must stay in the encode scissor; leftover DatePicker clip stamp is a different portal; scissor={scissor:?} popover={popover:?}"
        );
    let trigger = [640.0, 88.0, 36.0, 32.0];
    let scissor_from_trigger =
        overlay_encode_scissor(Some(popover), Some(trigger), None).expect("popover vs trigger");
    assert!(
            intersect_rects(scissor_from_trigger, popover).is_some(),
            "Fade popover dirty must not clip to the trigger-sized occupancy; scissor={scissor_from_trigger:?}"
        );
}

#[test]
fn nested_overlay_wrong_space_fade_does_not_punch_sibling_on_restore() {
    let parent = [80.0, 360.0, 280.0, 320.0];
    let nested = [88.0, 400.0, 144.0, 220.0];
    let fade_document = [0.0, 8000.0, 40.0, 160.0];
    let stamps = [(PARENT_KEY, parent), (NESTED_KEY, nested)];
    let restore = overlay_fade_replace_restore_boxes(NESTED_KEY, Some(fade_document), &stamps);
    assert_eq!(restore, vec![nested]);
    let occ = rects_union(&[parent, nested]);
    let encode_occ = overlay_encode_occupancy(NESTED_KEY, Some(fade_document), &stamps, occ);
    assert_eq!(
        encode_occ,
        Some(nested),
        "wrong-space fade must resolve to the nested stamp, never sibling-union occupancy"
    );
}

#[test]
fn overlay_flip_restore_must_not_punch_page_gap_between_positions() {
    let below = [88.0, 400.0, 144.0, 80.0];
    let above = [88.0, 40.0, 144.0, 220.0];
    let leftover = overlay_restore_leftover_boxes(&[below], &[above]);
    assert_eq!(leftover, vec![below]);
    let gap = [88.0, 260.0, 144.0, 140.0];
    assert!(
        leftover.iter().all(|b| intersect_rects(*b, gap).is_none()),
        "union AABB of below∪above would dest-reset the page between the two positions"
    );
    assert!(overlay_same_footprint(Some(below), Some(below), 2.0));
    assert!(!overlay_same_footprint(Some(below), Some(above), 2.0));
    let holes = overlay_mounted_replace_holes(
        &[(PANEL_KEY, below)],
        &[(PANEL_KEY, above)],
        PANEL_KEY,
        Some(above),
    );
    assert_eq!(holes, vec![below, above]);
    assert!(holes.iter().all(|b| intersect_rects(*b, gap).is_none()));
}

#[test]
fn overlay_grow_has_no_leftover_restore() {
    let small = [90.0, 140.0, 40.0, 40.0];
    let grown = [90.0, 140.0, 40.0, 160.0];
    assert!(overlay_restore_leftover_boxes(&[small], &[grown]).is_empty());
    assert!(!overlay_same_footprint(Some(small), Some(grown), 2.0));
    assert!(
        overlay_restore_leftover_stamps(&[(PANEL_KEY, small)], &[(PANEL_KEY, grown)]).is_empty()
    );
}

#[test]
fn expand_measure_box_must_not_restore_against_first_clip() {
    let measure = [90.0, 140.0, 40.0, 200.0];
    let clip = [90.0, 140.0, 40.0, 40.0];
    assert!(
        !overlay_restore_leftover_boxes(&[measure], &[clip]).is_empty(),
        "document: recording the unconstrained measure box makes the next clip look like a shrink"
    );
    assert!(
        overlay_restore_leftover_boxes(&[], &[clip]).is_empty(),
        "measure frames must not enter last_overlay_boxes"
    );
    assert!(
        overlay_restore_leftover_stamps(&[], &[(PANEL_KEY, clip)]).is_empty(),
        "measure frames omit the portal key so leftover is empty"
    );
    assert!(
        !overlay_restore_needed(None, Some(clip)),
        "first painted clip is a grow from empty occupancy — Restore dest-resets a white frame"
    );
    assert!(!overlay_restore_needed(
        Some(clip),
        Some([90.0, 140.0, 40.0, 80.0])
    ));
    assert!(overlay_restore_needed(
        Some([90.0, 140.0, 40.0, 80.0]),
        Some(clip)
    ));
    assert!(
        overlay_fade_replace_needed(true, true, true, false, false),
        "expand growth still replaces the previous stamp (src-over would stack the shadow)"
    );
}

#[test]
fn overlay_mounted_replace_holes_shrink_is_union_not_leftover_ring() {
    let prev = [80.0, 30.0, 164.0, 240.0];
    let cur = [90.0, 140.0, 144.0, 100.0];
    let leftover = overlay_restore_leftover_boxes(&[prev], &[cur]);
    assert!(
        leftover.len() > 1 || leftover.iter().any(|b| b[2] > 0.0 && b[3] > 0.0),
        "leftover of shrink is a strip/ring"
    );
    let holes = overlay_mounted_replace_holes(
        &[(PANEL_KEY, prev)],
        &[(PANEL_KEY, cur)],
        PANEL_KEY,
        Some(cur),
    );
    assert_eq!(holes, vec![union_rects(prev, cur)]);
    assert!(
        leftover.iter().all(|s| rect_contains_rect(holes[0], *s)),
        "filled previous visual covers leftover strips so dest-reset is not a caption-cutting ring"
    );
}

#[test]
fn overlay_mounted_replace_holes_flip_does_not_punch_gap() {
    let below = [88.0, 400.0, 144.0, 80.0];
    let above = [88.0, 40.0, 144.0, 220.0];
    let holes = overlay_mounted_replace_holes(
        &[(PANEL_KEY, below)],
        &[(PANEL_KEY, above)],
        PANEL_KEY,
        Some(above),
    );
    assert_eq!(holes, vec![below, above]);
    let gap = [88.0, 260.0, 144.0, 140.0];
    assert!(
        holes.iter().all(|b| intersect_rects(*b, gap).is_none()),
        "union AABB of below∪above would dest-reset the page between the two positions"
    );
}

#[test]
fn overlay_mounted_replace_holes_nested_does_not_restore_parent() {
    let parent = [80.0, 360.0, 280.0, 320.0];
    let nested = [88.0, 40.0, 144.0, 220.0];
    let fade = nested;
    let stamps = [(PARENT_KEY, parent), (NESTED_KEY, nested)];
    let holes = overlay_mounted_replace_holes(&stamps, &stamps, NESTED_KEY, Some(fade));
    assert_eq!(holes, vec![nested]);
    assert!(intersect_rects(holes[0], parent).is_none());
}

#[test]
fn nested_expand_unstamped_must_not_dest_reset_parent_halo() {
    let parent = [80.0, 360.0, 280.0, 320.0];
    let nested = [100.0, 400.0, 144.0, 80.0];
    let parent_only = [(PARENT_KEY, parent)];
    let join = overlay_mounted_replace_holes(&parent_only, &parent_only, NESTED_KEY, Some(nested));
    assert!(
            join.is_empty(),
            "unstamped nested join dest-resets nothing — the expand measure AABB would punch DatePicker fill and page captions"
        );
    let unconstrained = [80.0, 360.0, 280.0, 800.0];
    assert!(overlay_mounted_replace_holes(
        &parent_only,
        &parent_only,
        NESTED_KEY,
        Some(unconstrained)
    )
    .is_empty());
    let leave = overlay_mounted_replace_holes(&parent_only, &parent_only, NESTED_KEY, Some(nested));
    assert!(leave.is_empty());
    let leftover = overlay_restore_leftover_stamps(
        &[(PARENT_KEY, parent), (NESTED_KEY, nested)],
        &parent_only,
    );
    assert_eq!(leftover, vec![(NESTED_KEY, nested)]);
    assert!(
        overlay_restore_leftover_stamps(&parent_only, &parent_only).is_empty(),
        "parent outset halo must not count as a leftover of the same key"
    );
}

#[test]
fn overlay_mounted_replace_holes_same_size_fade() {
    let panel = [90.0, 140.0, 40.0, 160.0];
    let holes = overlay_mounted_replace_holes(
        &[(PANEL_KEY, panel)],
        &[(PANEL_KEY, panel)],
        PANEL_KEY,
        Some(panel),
    );
    assert_eq!(holes, vec![panel]);
}

#[test]
fn overlay_restore_sorts_before_overlay_encode() {
    let mut nodes = vec![
        PaintDirty {
            key: 1,
            bounds: [90.0, 140.0, 40.0, 160.0],
            kind: PaintDirtyKind::Overlay,
        },
        PaintDirty {
            key: 2,
            bounds: [80.0, 130.0, 80.0, 200.0],
            kind: PaintDirtyKind::OverlayRestore,
        },
    ];
    sort_paint_dirty_nodes(&mut nodes);
    assert_eq!(nodes[0].kind, PaintDirtyKind::OverlayRestore);
    assert_eq!(nodes[1].kind, PaintDirtyKind::Overlay);
}

#[test]
fn node_dirties_sort_larger_before_nested() {
    let shell = PaintDirty {
        key: 1,
        bounds: [10.0, 10.0, 320.0, 40.0],
        kind: PaintDirtyKind::Node,
    };
    let input = PaintDirty {
        key: 2,
        bounds: [12.0, 12.0, 260.0, 36.0],
        kind: PaintDirtyKind::Node,
    };
    let mut nodes = vec![input, shell];
    sort_paint_dirty_nodes(&mut nodes);
    assert_eq!(
        nodes[0].key, 1,
        "shell wipe must precede nested Input redraw"
    );
    assert_eq!(nodes[1].key, 2);
}

#[test]
fn nested_input_dirty_skips_dest_reset_inside_adornment() {
    let shell = PaintDirty {
        key: 1,
        bounds: [10.0, 10.0, 320.0, 40.0],
        kind: PaintDirtyKind::Node,
    };
    let input = PaintDirty {
        key: 2,
        bounds: [12.0, 12.0, 260.0, 36.0],
        kind: PaintDirtyKind::Node,
    };
    assert!(paint_dirty_nested_in_larger_node(&input, &[shell, input]));
    assert!(!paint_dirty_nested_in_larger_node(&shell, &[shell, input]));
    assert!(!paint_dirty_nested_in_larger_node(&input, &[input]));
}

#[test]
fn layered_page_stack_dirty_must_not_skip_dest_reset_inside_viewport() {
    let viewport = PaintDirty {
        key: 1,
        bounds: [0.0, 0.0, 320.0, 384.0],
        kind: PaintDirtyKind::Node,
    };
    let latest = PaintDirty {
        key: 2,
        bounds: [120.0, 340.0, 80.0, 32.0],
        kind: PaintDirtyKind::Node,
    };
    assert!(
        paint_dirty_nested_in_larger_node(&latest, &[viewport, latest]),
        "precondition: Latest center sits in the viewport dirty"
    );
    assert!(
            !paint_dirty_skips_node_dest_reset(&latest, &[viewport, latest], true),
            "page-stack Latest / overlay scrollbar must still dest-reset; ancestor in-flow present does not recapture that band"
        );
    assert!(
        paint_dirty_skips_node_dest_reset(&latest, &[viewport, latest], false),
        "in-flow children still skip under a larger ancestor"
    );
}

#[test]
fn overlay_restore_must_replay_page_fill_before_overlapping_avatar_faces() {
    // Cover-overlay close: hole is the expanded panel (larger than A/C/+N).
    // Card/page fill must paint before stacked faces, or dest-reset white /
    // a later fill wipes the overlap for a frame.
    let hole = [8.0, 40.0, 96.0, 48.0];
    let face_a = [8.0, 40.0, 32.0, 32.0];
    let face_c = [24.0, 40.0, 32.0, 32.0];
    let count = [40.0, 40.0, 32.0, 32.0];
    let page = [0.0, 0.0, 320.0, 384.0];
    assert_eq!(overlay_restore_replay_rank(page, hole), Some(0));
    let row = [8.0, 40.0, 80.0, 32.0];
    assert_eq!(
        overlay_restore_replay_rank(row, hole),
        Some(0),
        "collapsed group row is not a chip and must fill before A/B/C"
    );
    assert_eq!(overlay_restore_replay_rank(face_a, hole), Some(1));
    assert_eq!(overlay_restore_replay_rank(face_c, hole), Some(1));
    assert_eq!(overlay_restore_replay_rank(count, hole), Some(1));
    let beside = [200.0, 40.0, 32.0, 32.0];
    assert_eq!(overlay_restore_replay_rank(beside, hole), None);
}

#[test]
fn overlay_dirty_is_contained_keyframe_uses_inner_card_not_empty_stage() {
    let stage = [0.0, 0.0, 800.0, 600.0];
    let card = [200.0, 120.0, 400.0, 280.0];
    assert!(
        overlay_dirty_is_contained_keyframe(card, stage),
        "zoom card inside fullscreen stage must dest-reset the card, not the host"
    );
    assert!(!overlay_dirty_is_contained_keyframe(stage, card));
    assert!(
        !overlay_dirty_is_contained_keyframe(card, card),
        "same footprint is not a contained keyframe"
    );
}

#[test]
fn overlay_restore_must_skip_overlapping_avatar_node_dirties() {
    // AvatarGroup +N close: Restore hole covers A/C/+N. Each face is also a
    // Node dirty. Dest-reset of A after C punches the overlap (stacking flash).
    let restore = PaintDirty {
        key: 9,
        bounds: [8.0, 40.0, 96.0, 48.0],
        kind: PaintDirtyKind::OverlayRestore,
    };
    let face_a = PaintDirty {
        key: 1,
        bounds: [8.0, 40.0, 32.0, 32.0],
        kind: PaintDirtyKind::Node,
    };
    let face_c = PaintDirty {
        key: 2,
        bounds: [24.0, 40.0, 32.0, 32.0],
        kind: PaintDirtyKind::Node,
    };
    let nodes = [restore, face_a, face_c];
    // Faces sit inside the panel stamp (Restore pad == panel for this fixture).
    let panel_stamp = [(9u64, restore.bounds)];
    assert!(
        paint_dirty_covered_by_overlay_restore(&face_a, &nodes, &panel_stamp),
        "earlier overlapped face is inside the Restore hole"
    );
    assert!(
        paint_dirty_covered_by_overlay_restore(&face_c, &nodes, &panel_stamp),
        "later overlapped face is inside the Restore hole"
    );
    assert!(
        !paint_dirty_covered_by_overlay_restore(&restore, &nodes, &panel_stamp),
        "Restore itself is not a Node skip"
    );
    let beside = PaintDirty {
        key: 3,
        bounds: [200.0, 40.0, 32.0, 32.0],
        kind: PaintDirtyKind::Node,
    };
    assert!(
        !paint_dirty_covered_by_overlay_restore(&beside, &nodes, &panel_stamp),
        "faces outside the overlay hole still dest-reset"
    );
    let page = PaintDirty {
        key: 0,
        bounds: [0.0, 0.0, 320.0, 384.0],
        kind: PaintDirtyKind::Node,
    };
    assert!(
        !paint_dirty_covered_by_overlay_restore(&page, &nodes, &panel_stamp),
        "larger page dest-reset is not swallowed by a small Restore hole"
    );

    // Wave 49/51: Region island (group AABB) also swallows per-face Node wipes.
    let region = PaintDirty {
        key: 8,
        bounds: [8.0, 40.0, 96.0, 48.0],
        kind: PaintDirtyKind::Region,
    };
    let region_nodes = [region, face_a, face_c];
    assert!(
        paint_dirty_covered_by_overlay_restore(&face_a, &region_nodes, &[]),
        "Region group AABB must skip earlier face dest-reset"
    );
    assert!(
        paint_dirty_covered_by_overlay_restore(&face_c, &region_nodes, &[]),
        "Region group AABB must skip later face dest-reset"
    );
}

#[test]
fn node_hover_preserves_portal_halo_ring_without_overlay_punch() {
    let trigger = [10.0, 40.0, 220.0, 32.0];
    let panel = [10.0, 76.0, 220.0, 96.0];
    let halo = [4.0, 54.0, 232.0, 120.0];
    let coverage = overlay_replay_coverage(&[(1, panel)], Some(halo));
    let wipe = [10.0, 40.0, 220.0, 48.0];
    let pieces = page_scissors_excluding_portal_boxes(wipe, &[(1, panel)]);
    let kept =
        page_node_present_scissors_preserving_portal_halo(pieces, trigger, coverage, &[(1, panel)]);
    let face = intersect_rects(kept.iter().copied().fold(trigger, union_rects), trigger)
        .expect("trigger face coverage");
    assert!(
        face[2] * face[3] > trigger[2] * trigger[3] * 0.9,
        "almost all of the trigger face must dest-reset; kept={kept:?}"
    );
    let gap_pixel = [100.0, 73.0, 1.0, 1.0];
    assert!(
        kept.iter()
            .all(|r| intersect_rects(*r, gap_pixel).is_none()),
        "gap halo pixel must not be CLEARed on hover; kept={kept:?}"
    );
    assert!(
        kept.iter()
            .any(|p| overlay_page_punch_overlap(coverage, *p).is_some()),
        "fixture: cleared face intersects coverage"
    );
    assert!(!page_dirty_punches_overlay_after_clear(
        PaintDirtyKind::Node
    ));
}

#[test]
fn exclusive_restore_pad_must_not_skip_trigger_above_panel() {
    // Fade-replace / exclusive peer dismiss builds OverlayRestore from
    // encode_scissor(visual halo) — pad(halo) fully contains the trigger
    // sitting 4px above the panel. Old containment-only skip swallowed that
    // Node; hover fringe stayed uncleared and the portal shadow stacked.
    let panel = [10.0, 76.0, 220.0, 96.0];
    let visual_halo = [4.0, 54.0, 232.0, 120.0];
    let restore_pad = crate::app::draw_cull::padded_xywh(visual_halo, 16.0);
    let trigger = PaintDirty {
        key: 2,
        bounds: [10.0, 40.0, 220.0, 32.0],
        kind: PaintDirtyKind::Node,
    };
    let restore = PaintDirty {
        key: 9,
        bounds: restore_pad,
        kind: PaintDirtyKind::OverlayRestore,
    };
    let nodes = [restore, trigger];
    assert!(
        restore_pad[0] <= trigger.bounds[0]
            && restore_pad[1] <= trigger.bounds[1]
            && restore_pad[0] + restore_pad[2] >= trigger.bounds[0] + trigger.bounds[2]
            && restore_pad[1] + restore_pad[3] >= trigger.bounds[1] + trigger.bounds[3],
        "fixture: encode Restore pad must fully contain the trigger; pad={restore_pad:?}"
    );
    assert!(
        !paint_dirty_covered_by_overlay_restore(&trigger, &nodes, &[(9, panel)]),
        "trigger above the panel stamp must still dest-reset after exclusive Restore"
    );
    let chip = PaintDirty {
        key: 3,
        bounds: [20.0, 90.0, 40.0, 40.0],
        kind: PaintDirtyKind::Node,
    };
    assert!(
        paint_dirty_covered_by_overlay_restore(&chip, &[restore, chip], &[(9, panel)]),
        "chips inside the panel stamp stay swallowed by Restore"
    );
}

#[test]
fn region_island_unions_prev_and_now_inside_clip() {
    let clip = Some([0.0, 0.0, 400.0, 300.0]);
    let prev = [10.0, 100.0, 200.0, 40.0];
    let now = [10.0, 140.0, 200.0, 80.0];
    let d = region_island_structure_damage(clip, prev, Some(now)).expect("region");
    assert!(
        (d[1] - 100.0).abs() < 0.5 && (d[3] - 120.0).abs() < 0.5,
        "Region must cover prev∪now Y span, got {d:?}"
    );
    let page = region_island_structure_damage(
        Some([0.0, 0.0, 800.0, 600.0]),
        [0.0, 48.0, 752.0, 520.0],
        Some([0.0, 48.0, 752.0, 520.0]),
    )
    .expect("page-sized");
    assert!(
        region_is_page_route_tier(page, [0.0, 0.0, 800.0, 600.0]),
        "Match section_body sized region is Window tier"
    );
}

#[test]
fn stacked_input_dirties_must_not_skip_sibling_dest_reset() {
    // Gallery forms: leave field already unfocused (pad blur+8=9), enter field
    // focused (pad spread+8=11). Pads overlap across gap-8. Intersect+area
    // treated the larger enter dirty as an ancestor and skipped the leave wipe.
    let leave = PaintDirty {
        key: 1,
        bounds: [8.0, 8.0, 380.0, 36.0 + 18.0],
        kind: PaintDirtyKind::Node,
    };
    let enter = PaintDirty {
        key: 2,
        bounds: [8.0, 8.0 + 36.0 + 8.0 - 11.0, 380.0, 36.0 + 22.0],
        kind: PaintDirtyKind::Node,
    };
    assert!(
        intersect_rects(leave.bounds, enter.bounds).is_some(),
        "precondition: padded sibling dirties must overlap (the false nested case)"
    );
    assert!(
        enter.bounds[2] * enter.bounds[3] > leave.bounds[2] * leave.bounds[3] + 1.0,
        "precondition: focused sibling dirty is larger (unfocus pad vs ring pad)"
    );
    assert!(
        !paint_dirty_nested_in_larger_node(&leave, &[leave, enter]),
        "leave Input dest-reset must still run when a sibling focus dirty overlaps"
    );
    assert!(!paint_dirty_nested_in_larger_node(&enter, &[leave, enter]));
}

#[test]
fn bind_text_dirty_must_use_visual_scroll_not_document() {
    let _ = take_frame_paint_nodes();
    let mut el = Element::new().w(400.0).h(20.0);
    el.layout_node = Some(taffy::prelude::NodeId::from(42u64));
    el.layout_bounds = [16.0, 800.0, 400.0, 18.0];
    el.paint_scroll_acc = [0.0, 600.0];
    let visual = crate::core::tree::visual_paint_aabb(&el, el.paint_scroll_acc);
    assert!(
        (visual[1] - 200.0).abs() < 0.5,
        "visual y must be document y minus scroll, got {}",
        visual[1]
    );

    push_frame_paint_element(&el, [0.0, 0.0]);
    let document_dirty = take_frame_paint_nodes();
    assert_eq!(document_dirty.len(), 1);
    assert!(
            !paint_dirty_matches_slot(
                document_dirty[0].key,
                document_dirty[0].bounds,
                paint_element_key(&el),
                visual,
            ),
            "document-space bind_text dirty must not match the scrolled collect slot (gallery click caption freeze / wipe)"
        );

    push_frame_paint_element(&el, el.paint_scroll_acc);
    let visual_dirty = take_frame_paint_nodes();
    assert_eq!(visual_dirty.len(), 1);
    assert!(
        paint_dirty_matches_slot(
            visual_dirty[0].key,
            visual_dirty[0].bounds,
            paint_element_key(&el),
            visual,
        ),
        "bind_text StyleChanged must dest-reset the visual box collect actually painted"
    );
}

#[test]
fn union_disjoint_rects() {
    let u = union_rects([0.0, 0.0, 10.0, 10.0], [20.0, 5.0, 10.0, 10.0]);
    assert_eq!(u, [0.0, 0.0, 30.0, 15.0]);
}

#[test]
fn union_overlapping_rects() {
    let u = union_rects([0.0, 0.0, 20.0, 20.0], [10.0, 10.0, 20.0, 20.0]);
    assert_eq!(u, [0.0, 0.0, 30.0, 30.0]);
}

#[test]
fn union_contained_rect() {
    let u = union_rects([0.0, 0.0, 100.0, 100.0], [25.0, 25.0, 10.0, 10.0]);
    assert_eq!(u, [0.0, 0.0, 100.0, 100.0]);
}

#[test]
fn rects_union_empty_and_many() {
    assert_eq!(rects_union(&[]), None);
    assert_eq!(
        rects_union(&[[0.0, 0.0, 10.0, 10.0]]),
        Some([0.0, 0.0, 10.0, 10.0])
    );
    let u = rects_union(&[
        [0.0, 0.0, 10.0, 10.0],
        [20.0, 5.0, 10.0, 10.0],
        [5.0, 0.0, 5.0, 5.0],
    ])
    .expect("non-empty");
    assert_eq!(u, [0.0, 0.0, 30.0, 15.0]);
}

#[test]
fn damage_covers_full_viewport() {
    let vp = [0.0, 0.0, 800.0, 600.0];
    assert!(damage_covers_full(vp, vp));
    assert!(damage_covers_full(vp, [-1.0, -1.0, 802.0, 602.0]));
    assert!(!damage_covers_full(vp, [0.0, 0.0, 400.0, 600.0]));
    assert!(!damage_covers_full(vp, [0.0, 0.0, 0.0, 0.0]));
}

#[test]
fn frame_clear_policy_partial_for_local_damage() {
    let vp = [0.0, 0.0, 800.0, 600.0];
    assert_eq!(
        frame_clear_policy(true, false, Some([10.0, 10.0, 40.0, 20.0]), vp),
        FrameClearPolicy::PartialLoad
    );
}

#[test]
fn frame_clear_policy_full_when_damage_outside_viewport() {
    let vp = [0.0, 0.0, 800.0, 600.0];
    assert_eq!(
        frame_clear_policy(true, false, Some([100.0, 700.0, 200.0, 40.0]), vp),
        FrameClearPolicy::FullClear
    );
}

#[test]
fn damage_to_scissor_clamps_inside_target() {
    // Regression: y=738 with h=1 on a 600-tall target panicked wgpu validation.
    assert_eq!(
        damage_to_scissor_physical([228.0, 730.0, 230.0, 20.0], 8.0, 1.0, 800, 600),
        None
    );
    let s = damage_to_scissor_physical([100.0, 500.0, 50.0, 80.0], 8.0, 1.0, 800, 600)
        .expect("intersects bottom");
    assert!(s.0 + s.2 <= 800);
    assert!(s.1 + s.3 <= 600);
    assert!(s.3 >= 1);
}

#[test]
fn intersect_rects_basic() {
    assert_eq!(
        intersect_rects([0.0, 0.0, 100.0, 100.0], [50.0, 50.0, 100.0, 100.0]),
        Some([50.0, 50.0, 50.0, 50.0])
    );
    assert_eq!(
        intersect_rects([0.0, 0.0, 10.0, 10.0], [20.0, 20.0, 10.0, 10.0]),
        None
    );
}

#[test]
fn frame_clear_policy_full_when_forced_or_covers() {
    let vp = [0.0, 0.0, 800.0, 600.0];
    assert_eq!(
        frame_clear_policy(true, true, Some([10.0, 10.0, 40.0, 20.0]), vp),
        FrameClearPolicy::FullClear
    );
    assert_eq!(
        frame_clear_policy(false, false, Some([10.0, 10.0, 40.0, 20.0]), vp),
        FrameClearPolicy::FullClear
    );
    assert_eq!(
        frame_clear_policy(true, false, Some(vp), vp),
        FrameClearPolicy::FullClear
    );
    assert_eq!(
        frame_clear_policy(true, false, None, vp),
        FrameClearPolicy::FullClear
    );
}

#[test]
fn overlay_partial_load_does_not_repaint_page() {
    assert!(overlay_partial_load_retains_page(true, false, true, true));
    assert!(
        !overlay_partial_load_retains_page(false, false, true, true),
        "FullClear must still paint page glyphs"
    );
    assert!(
        !overlay_partial_load_retains_page(true, true, true, true),
        "scroll-layer Clear wiped the scissor; page must repaint"
    );
    assert!(!overlay_partial_load_retains_page(true, false, false, true));
    assert!(
        !overlay_partial_load_retains_page(true, false, true, false),
        "page text in the overlay scissor changed; must repaint glyphs"
    );
    assert!(
        overlay_partial_load_retains_page(true, false, true, true),
        "overlay present + PartialLoad must keep retain page pixels"
    );
}

#[test]
fn overlay_close_or_shrink_needs_scissor_clear() {
    let panel = [90.0, 140.0, 40.0, 160.0];
    let chip = [90.0, 260.0, 32.0, 28.0];
    assert!(overlay_scissor_needs_clear(Some(panel), None));
    assert!(
        overlay_scissor_needs_clear(Some(panel), Some(chip)),
        "exit keyframes leave expanded pixels in retain; Restore then redraw"
    );
    assert!(
        !overlay_scissor_needs_clear(Some(chip), Some(panel)),
        "grow: new pixels were page; Load + overlay draw is enough"
    );
    assert!(!overlay_scissor_needs_clear(Some(panel), Some(panel)));
    assert!(!overlay_scissor_needs_clear(None, Some(panel)));
    assert!(overlay_restore_needed(Some(panel), Some(chip)));
    assert!(!overlay_restore_needed(Some(chip), Some(panel)));
    assert!(!overlay_restore_needed(None, Some(panel)));
}

#[test]
fn skip_full_clear_only_when_portal_membership_is_isolated() {
    assert!(skip_full_clear_for_portal_structure(true, false));
    assert!(!skip_full_clear_for_portal_structure(true, true));
    assert!(!skip_full_clear_for_portal_structure(false, false));
    assert!(!skip_full_clear_for_portal_structure(false, true));
}

#[test]
fn overlay_open_is_not_a_page_structure_rebuild() {
    assert!(
        !page_structure_rebuild(true, true, false),
        "popover / bubble +N hoist must keep real pointer for class :hover"
    );
    assert!(page_structure_rebuild(true, false, false));
    assert!(page_structure_rebuild(true, true, true));
    assert!(!page_structure_rebuild(false, true, false));
}

#[test]
fn overlay_without_layout_box_cannot_partial_load() {
    assert!(overlay_pixels_need_full_clear(true, None));
    assert!(!overlay_pixels_need_full_clear(
        true,
        Some([90.0, 190.0, 160.0, 48.0])
    ));
    assert!(!overlay_pixels_need_full_clear(false, None));
}

#[test]
fn retained_overlay_missing_box_restores_locally_during_reopen() {
    assert!(overlay_requires_full_clear(true, None, false));
    assert!(!overlay_requires_full_clear(true, None, true));
    assert!(!overlay_requires_full_clear(
        true,
        Some([90.0, 190.0, 160.0, 48.0]),
        true
    ));
}

#[test]
fn leftover_chip_damage_grows_to_overlay_box() {
    let chip = [100.0, 200.0, 24.0, 24.0];
    let overlay = [90.0, 190.0, 160.0, 48.0];
    let u = union_rects(chip, overlay);
    assert!(u[2] >= overlay[2] - 0.5);
    assert!(u[3] >= overlay[3] - 0.5);
    assert!(u[0] <= overlay[0] + 0.5);
    assert!(u[1] <= overlay[1] + 0.5);
}

#[test]
fn faded_overlay_close_restores_panel_outside_remaining_child_ink() {
    let panel = [306.0, 926.0, 300.0, 330.0];
    let child_ink = [319.0, 1000.0, 274.0, 193.0];
    let holes = overlay_restore_leftover_coverage(&[(7, panel)], &[], Some(child_ink));
    assert!(holes.iter().any(|(_, b)| rect_contains_rect(*b, panel)));
}

#[test]
fn shrinking_menu_restore_covers_shader_halo_past_encode_pad() {
    // popup_shadow Md: shader ext is blur*2 + border + 2 ≈ 23px. The 16px
    // encode pad leaves that fringe in retain (the line under a closed menu).
    let previous = [100.0, 80.0, 140.0, 180.0];
    let live = [100.0, 80.0, 140.0, 40.0];
    let visual = super::super::draw_cull::padded_xywh(previous, 22.0);
    let holes = overlay_restore_leftover_coverage(&[(1, previous)], &[(1, live)], Some(visual));
    let smear = [
        previous[0] + previous[2] * 0.5,
        previous[1] + previous[3] + 20.0,
    ];
    let covers = |holes: &[(u64, [f32; 4])]| {
        holes.iter().any(|(_, b)| {
            smear[0] >= b[0] && smear[1] >= b[1] && smear[0] < b[0] + b[2] && smear[1] < b[1] + b[3]
        })
    };
    assert!(
        covers(&holes),
        "expand-up must erase the painted halo, not only the layout box + 16px; holes={holes:?}"
    );
    let unrecorded = overlay_restore_leftover_coverage(&[(1, previous)], &[(1, live)], None);
    assert!(
        covers(&unrecorded),
        "a stamp with no recorded visual still has to cover the shader halo; holes={unrecorded:?}"
    );
}

#[test]
fn single_portal_unclamped_visual_is_restored_past_the_stamp() {
    // max-height clamps the stamp. The painted box can still be the content
    // height, so the bottom shadow sits far below the 32px halo slack.
    let stamp = [100.0, 80.0, 140.0, 40.0];
    let visual = [100.0, 80.0, 140.0, 220.0];
    let shrunk = overlay_restore_leftover_coverage(&[(1, visual)], &[(1, stamp)], Some(visual));
    let below = [170.0, 80.0 + 180.0];
    let covers = |holes: &[(u64, [f32; 4])], y: f32| {
        holes.iter().any(|(_, b)| {
            below[0] >= b[0] && y >= b[1] && below[0] < b[0] + b[2] && y < b[1] + b[3]
        })
    };
    assert!(
        covers(&shrunk, below[1]),
        "a lone portal must restore paint taller than its stamp; holes={shrunk:?}"
    );
    let settled = overlay_restore_leftover_coverage(&[(1, stamp)], &[(1, stamp)], Some(visual));
    assert!(
        covers(&settled, 80.0 + 40.0 + 24.0 + 8.0),
        "stable stamp must still clear ink below the restore pad; holes={settled:?}"
    );
    let sibling = [400.0, 80.0, 80.0, 40.0];
    let nested = overlay_restore_leftover_coverage(
        &[(1, stamp), (2, sibling)],
        &[(1, stamp), (2, sibling)],
        Some(visual),
    );
    assert!(
        nested.is_empty(),
        "a sibling union is not this stamp's overflow; holes={nested:?}"
    );
}

#[test]
fn shrinking_or_removed_nested_overlay_restores_previous_shadow() {
    let parent = (7, [570.0, 960.0, 280.0, 360.0]);
    let menu = (8, [583.0, 977.0, 144.0, 200.0]);
    let smaller_parent = (7, [570.0, 960.0, 280.0, 340.0]);
    let holes = overlay_restore_leftover_coverage(&[parent, menu], &[smaller_parent], None);
    assert!(holes
        .iter()
        .any(|(k, b)| *k == 7 && rect_contains_rect(*b, [840.0, 1325.0, 1.0, 1.0])));
    assert!(holes
        .iter()
        .any(|(k, b)| *k == 8 && rect_contains_rect(*b, [730.0, 1178.0, 1.0, 1.0])));
}

#[test]
fn overlay_replay_keeps_chrome_outside_exit_child_ink() {
    let stamps = [(7, [300.0, 900.0, 300.0, 330.0])];
    let child = Some([319.0, 963.0, 274.0, 193.0]);
    let hole = [300.0, 1180.0, 160.0, 50.0];
    let coverage = overlay_replay_coverage(&stamps, child);
    assert_eq!(overlay_page_punch_overlap(coverage, hole), Some(hole));
}

#[test]
fn nested_overlay_hover_restore_covers_encode_shadow() {
    let parent = [306.0, 926.0, 300.0, 330.0];
    let menu = [319.0, 983.0, 144.0, 200.0];
    let stamps = [(7, parent), (8, menu)];
    let dirty = menu;
    let encode = overlay_encode_scissor(Some(dirty), Some(menu), None).unwrap();
    let restore = overlay_replay_restore_bounds(8, dirty, &stamps, None).unwrap();
    assert!(rect_contains_rect(restore, encode));
}

#[test]
fn merged_overlay_restore_covers_gaps_between_animation_dirties() {
    let stamps = [
        (7, [306.0, 926.0, 300.0, 310.0]),
        (8, [319.0, 986.0, 144.0, 196.0]),
    ];
    let dirties = [(8, [299.0, 1026.0, 184.0, 177.0]), (8, stamps[1].1)];
    let merged = merge_overlay_replay_dirties(&dirties, &stamps);
    assert_eq!(merged.len(), 1);
    let restore =
        overlay_replay_restore_bounds(merged[0].0[0], merged[0].1, &stamps, None).unwrap();
    let corner = [490.0, 980.0, 1.0, 1.0];
    assert!(rect_contains_rect(restore, corner));
    assert!(
        !dirties.iter().any(|(key, dirty)| {
            rect_contains_rect(
                overlay_replay_restore_bounds(*key, *dirty, &stamps, None).unwrap(),
                corner,
            )
        }),
        "the old per-dirty restores miss the merged corner"
    );
}

#[test]
fn overlay_restore_occupancy_covers_visual_overflow() {
    let chip = [400.0, 200.0, 32.0, 24.0];
    let visual_panel = [360.0, 140.0, 120.0, 88.0];
    assert_eq!(
        union_overlay_visual_bounds(None, [0.0, 0.0, 0.0, 0.0]),
        None
    );
    let occupied = union_overlay_visual_bounds(Some(chip), visual_panel).expect("visual");
    assert!(
        rect_contains_rect(occupied, visual_panel),
        "occupancy is the instance paint AABB; dest-reset that hole, do not pad a white frame"
    );
    assert!(rect_contains_rect(occupied, chip));
    assert!(overlay_restore_needed(Some(occupied), Some(chip)));
}

#[test]
fn force_full_redraw_keeps_partial_load_for_named_nodes_without_page_geom() {
    use crate::core::tree::LayoutStatus::*;
    // Typing / caret / spin: pre_chrome may still be LayoutChanged from
    // fit-relayout of a fixed Input box. Named dirties must PartialLoad.
    assert!(
        !force_full_redraw_after_chrome(LayoutChanged, StyleChanged, false, false, true, false),
        "pre_chrome LayoutChanged must not undo Input+spin PartialLoad"
    );
    assert_eq!(
        classify_present_tier(LayoutChanged, StyleChanged, false, false, true, false),
        PresentTier::Node,
        "Input+spin is PresentTier::Node"
    );
    assert!(
        !force_full_redraw_after_chrome(Clean, LayoutChanged, false, false, true, false),
        "chrome LayoutChanged with named dirties stays PartialLoad"
    );
    assert!(
        force_full_redraw_after_chrome(LayoutChanged, StyleChanged, true, false, true, false),
        "page geometry (window resize / text-fit settle) FullClears even with named spin"
    );
    assert_eq!(
        classify_present_tier(LayoutChanged, StyleChanged, true, false, true, false),
        PresentTier::Window,
    );
    assert!(
        fit_relayout_is_page_geometry(true, false, false),
        "maximize layout_fast then glyphon_fit is page geometry"
    );
    assert!(
        !fit_relayout_is_page_geometry(false, false, false),
        "stable-viewport Input fit is not page geometry"
    );
    assert!(
            force_full_redraw_after_chrome(
                LayoutChanged,
                StyleChanged,
                fit_relayout_is_page_geometry(true, false, false),
                false,
                true,
                false,
            ),
            "named spin must not keep PartialLoad after maximize fit reflow — dest-reset of the new box leaves the old Y"
        );
    assert!(
        node_present_dirties_moved(
            &[PaintDirty {
                key: 7,
                bounds: [10.0, 90.0, 16.0, 16.0],
                kind: PaintDirtyKind::Node,
            }],
            &[(7, [10.0, 90.0, 16.0, 16.0])],
            &[(7, [10.0, 10.0, 16.0, 16.0])],
        ),
        "named spin after a page Y shift cannot dest-reset the previous pixels"
    );
    assert!(
        !node_present_dirties_moved(
            &[PaintDirty {
                key: 7,
                bounds: [10.0, 10.0, 16.0, 16.0],
                kind: PaintDirtyKind::Node,
            }],
            &[(7, [10.0, 10.0, 16.0, 16.0])],
            &[(7, [10.0, 10.0, 16.0, 16.0])],
        ),
        "idle spin at a stable box stays paint-in-place"
    );
    assert!(
        node_present_dirties_moved(
            &[PaintDirty {
                key: 7,
                bounds: [10.0, 12.0, 16.0, 16.0],
                kind: PaintDirtyKind::Node,
            }],
            &[(7, [10.0, 12.0, 16.0, 16.0])],
            &[(7, [10.0, 10.0, 16.0, 16.0])],
        ),
        "smooth-scroll 2px is still a visual translation — 8px slack left stacked labels"
    );
    assert!(
        !node_present_dirties_moved(
            &[PaintDirty {
                key: 7,
                bounds: [10.0, 10.0, 16.0, 16.0],
                kind: PaintDirtyKind::Node,
            }],
            &[(7, [10.0, 10.0, 16.0, 16.0])],
            &[],
        ),
        "first present has no previous pixels to leave behind"
    );
    let ibeam = [120.0, 40.0, 2.0, 16.0];
    let empty_at_leaf = [8.0, 36.0, 0.0, 0.0];
    assert!(
        !node_present_slot_moved(ibeam, empty_at_leaf),
        "caret blink OFF is no-ink, not a translate from the flex-leaf origin"
    );
    assert!(
        !node_present_slot_moved(empty_at_leaf, ibeam),
        "caret blink ON is no-ink→ink, not a translate of the flex leaf"
    );
    let fill = [234.0, 166.0, 1020.0, 48.0];
    let stroke = [235.0, 167.0, 1018.0, 46.0];
    assert!(
        !node_present_slot_moved(fill, stroke) && !node_present_slot_moved(stroke, fill),
        "1px border on/off is not a translate of the InputAdornment (⌘K flash)"
    );
    assert!(
        node_present_slot_moved(fill, [234.0, 167.0, 1020.0, 48.0]),
        "1px scroll still translates"
    );
    let mut blink = vec![PaintDirty {
        key: 9,
        bounds: ibeam,
        kind: PaintDirtyKind::Node,
    }];
    expand_paint_dirties_for_translated_slots(&mut blink, &[(9, empty_at_leaf)], &[(9, ibeam)]);
    assert!(
        blink[0].bounds[2] < 8.0,
        "blink OFF must not union the I-beam dirty with the leaf origin\n dirty={:?}",
        blink[0].bounds
    );
    let mut vanished = Vec::new();
    expand_paint_dirties_for_translated_slots(&mut vanished, &[], &[(9, ibeam)]);
    assert!(
            vanished.is_empty(),
            "omitted last-present is not a translate; I-beam dest-reset is the named caret dirty\n dirties={vanished:?}"
        );
    let kbd = [20.0, 8.0, 22.0, 20.0];
    let mut kbd_live = vec![PaintDirty {
        key: 9,
        bounds: ibeam,
        kind: PaintDirtyKind::Node,
    }];
    // Gallery KbdShowcase: presentational keycaps mint new Taffy ids on rebuild.
    // Vanished-slot expand dest-reset every old keycap → 整个 Kbd 区域闪烁.
    expand_paint_dirties_for_translated_slots(
        &mut kbd_live,
        &[(100, kbd)],
        &[(9, ibeam), (42, kbd)],
    );
    assert!(
            kbd_live.iter().all(|d| d.key != 42),
            "omitted kbd last-present key must not dest-reset (KbdShowcase flash)\n dirties={kbd_live:?}"
        );
    expand_paint_dirties_for_clip_culled_slots(
        &mut kbd_live,
        &[(100, kbd)],
        &[(9, ibeam), (42, kbd)],
        &HashSet::new(),
    );
    assert!(
            kbd_live.iter().all(|d| d.key != 42),
            "kbd last-present is Taffy recycle, not clip-cull; must not dest-reset\n dirties={kbd_live:?}"
        );
    let mut culled = Vec::new();
    expand_paint_dirties_for_clip_culled_slots(
        &mut culled,
        &[(80, [0.0, 52.0, 388.0, 28.0])],
        &[(41, [316.0, 52.0, 44.0, 20.0])],
        &HashSet::from([41u64]),
    );
    assert_eq!(
        culled.len(),
        1,
        "clip-culled Builder/Delivered at the live edge must dest-reset\n dirties={culled:?}"
    );
    assert_eq!(culled[0].key, 41);
    assert!(
        (culled[0].bounds[1] - 52.0).abs() < 0.5,
        "wipe the last-present box, not a title-bar phantom\n dirty={:?}",
        culled[0].bounds
    );
    // Gallery 基础: a vanished previous-page island is unmatched last-present
    // inside the page ScrollArea, not clip-cull. Dest-reset of that island
    // CLEARs the visible Button card (blank gallery body).
    let visible_card = [220.0, 64.0, 760.0, 280.0];
    let vanished_island = [220.0, 60.0, 760.0, 520.0];
    let live_bubble = [520.0, 56.0, 160.0, 28.0];
    let mut page = Vec::new();
    expand_paint_dirties_for_clip_culled_slots(
        &mut page,
        &[(1, visible_card)],
        &[(99, vanished_island), (41, live_bubble)],
        &HashSet::from([41u64]),
    );
    assert!(
            page.iter().all(|d| d.key != 99),
            "vanished page island / card must not dest-reset the visible gallery body\n dirties={page:?}"
        );
    assert!(
        page.iter().any(|d| d.key == 41),
        "clip-culled live-edge bubble must still dest-reset\n dirties={page:?}"
    );
    let card_cx = visible_card[0] + visible_card[2] * 0.5;
    let card_cy = visible_card[1] + visible_card[3] * 0.5;
    for d in &page {
        let covers_card = d.bounds[0] <= card_cx
            && card_cx < d.bounds[0] + d.bounds[2]
            && d.bounds[1] <= card_cy
            && card_cy < d.bounds[1] + d.bounds[3]
            && d.bounds[2] * d.bounds[3] >= visible_card[2] * visible_card[3] * 0.4;
        assert!(
            !covers_card,
            "clip-culled dest-reset wipes the visible 基础 card\n dirty={:?}",
            d.bounds
        );
    }
    let vp = [0.0, 52.0, 400.0, 160.0];
    let think_prev = [8.0, 52.0, 200.0, 20.0];
    let ms_cy = 62.0;
    let culled_pins = clip_culled_scroll_blit_pins(
        &[(41, think_prev), (99, vanished_island)],
        &HashSet::from([41u64]),
        &[vp],
        &[[0.0, 24.0]],
    );
    assert_eq!(
            culled_pins.len(),
            1,
            "clip-culled Thinking last-present must pin the live edge, not wait until the card fully leaves\n pins={culled_pins:?}"
        );
    let pin = culled_pins[0];
    assert!(
        pin[1] <= ms_cy && ms_cy < pin[1] + pin[3],
        "pin must cover 1c-ms) where Thinking leftover sits\n pin={pin:?}"
    );
    assert!(
        pin[2] * pin[3] < vp[2] * vp[3] * 0.5,
        "pin must be the culled leftover, not the whole ScrollArea\n pin={pin:?}"
    );
    let recycle_pins = clip_culled_scroll_blit_pins(
        &[(99, vanished_island)],
        &HashSet::new(),
        &[vp],
        &[[0.0, 24.0]],
    );
    assert!(
            recycle_pins.is_empty(),
            "Taffy recycle / previous-page island must not pin-wipe the gallery body\n pins={recycle_pins:?}"
        );
    let scroll_tick = [PaintDirty {
        key: 7,
        bounds: [10.0, 12.0, 16.0, 16.0],
        kind: PaintDirtyKind::Node,
    }];
    let moved = node_present_dirties_moved(
        &scroll_tick,
        &[(7, [10.0, 12.0, 16.0, 16.0])],
        &[(7, [10.0, 10.0, 16.0, 16.0])],
    );
    assert!(
        !node_present_moved_forces_full_clear(moved, true),
        "scroll viewport layer already translated pixels; named spin must not window-Clear"
    );
    assert!(
        node_present_moved_forces_full_clear(moved, false),
        "without a scroll layer, dest-reset of now leaves previous visual Y"
    );
    assert!(
        !node_present_moved_forces_full_clear(false, true),
        "scroll layer without a moved named slot does not FullClear"
    );
    assert!(
        force_full_redraw_after_chrome(LayoutChanged, Clean, false, false, false, false),
        "nobody named a dirty box → FullClear fallback"
    );
    assert!(
        !force_full_redraw_after_chrome(StructureChanged, Clean, false, false, true, false),
        "island StructureChanged with named damage stays PartialLoad (Send must not wipe Footer)"
    );
    assert_eq!(
        classify_present_tier(StructureChanged, Clean, false, false, true, true),
        PresentTier::Region,
        "Send nested island is PresentTier::Region"
    );
    assert_eq!(
        classify_present_tier(StructureChanged, Clean, true, false, true, true),
        PresentTier::Window,
        "App:section_body page Match is PresentTier::Window"
    );
    assert!(
        force_full_redraw_after_chrome(StructureChanged, Clean, true, false, true, false),
        "page geometry StructureChanged FullClears"
    );
    assert!(
        force_full_redraw_after_chrome(StructureChanged, Clean, false, false, false, false),
        "StructureChanged with no named box → FullClear fallback"
    );
    assert!(
        !force_full_redraw_after_chrome(StructureChanged, Clean, false, true, true, false),
        "portal-only structure skip"
    );
    assert_eq!(
        classify_present_tier(StructureChanged, Clean, false, true, true, false),
        PresentTier::Region,
        "portal-only is Region, not Window"
    );
    assert_eq!(
        classify_present_tier(LayoutChanged, Clean, false, false, true, true),
        PresentTier::Region,
        "Layout + region dirty is Region, not Node"
    );
    assert!(
        !force_full_redraw_after_chrome(LayoutChanged, Clean, false, false, true, true),
        "has_region_damage must reach the helper (not hardcoded false)"
    );
    assert!(
        region_is_page_route_tier([0.0, 0.0, 900.0, 700.0], [0.0, 0.0, 1280.0, 800.0]),
        "large inset Match body is page-route Window"
    );
    assert!(
        !region_is_page_route_tier([10.0, 100.0, 320.0, 200.0], [0.0, 0.0, 1280.0, 800.0]),
        "MessageScroller Send clip stays Region"
    );
    assert_eq!(
        promote_present_tier_on_miss(PresentTier::Node, true),
        PresentTier::Region
    );
    assert_eq!(
        promote_present_tier_on_miss(PresentTier::Region, true),
        PresentTier::Window
    );
    assert_eq!(
        promote_present_tier_on_miss(PresentTier::Node, false),
        PresentTier::Node
    );
    assert!(dest_reset_slot_miss(false, true));
    assert!(!dest_reset_slot_miss(true, false));
    assert!(!dest_reset_slot_miss(true, true));
    assert!(!dest_reset_slot_miss(false, false));
    // Circumcircle growth looks like slot_moved, but must stay Node dest-reset.
    let rest = [10.0, 10.0, 16.0, 16.0];
    let spun = [4.0, 4.0, 28.0, 28.0];
    assert!(
        node_present_slot_moved(rest, spun),
        "precondition: spin grows visual AABB"
    );
    assert_eq!(
        escalate_node_dest_reset_kind(PaintDirtyKind::Node, true, true),
        PaintDirtyKind::Node,
        "spin / hover-scale AABB growth must not escalate to Region (叠影)"
    );
    assert_eq!(
        escalate_node_dest_reset_kind(PaintDirtyKind::Node, false, true),
        PaintDirtyKind::Region,
        "Taffy / last-present miss still escalates"
    );
}

#[test]
fn overlay_membership_drops_viewport_hover_union() {
    let vp = [0.0, 0.0, 800.0, 600.0];
    let overlay = [90.0, 190.0, 160.0, 48.0];
    let d = overlay_membership_damage(Some(vp), Some(overlay), vp, true, false);
    assert_eq!(d, Some(overlay));
    assert_eq!(
        frame_clear_policy(true, false, d, vp),
        FrameClearPolicy::PartialLoad
    );
    let kept = overlay_membership_damage(
        Some([100.0, 200.0, 24.0, 24.0]),
        Some(overlay),
        vp,
        false,
        false,
    )
    .expect("union");
    assert!(kept[2] >= overlay[2] - 0.5);
    assert!(!damage_covers_full(vp, kept));
}

#[test]
fn overlay_in_scene_partial_loads_when_box_is_local() {
    // Overlay instances existing is not `force_full_redraw`. Chip hover + overlay box
    // stay PartialLoad (gallery bubble +N / +1 → emoji must not FullClear the page).
    let vp = [0.0, 0.0, 800.0, 600.0];
    let chip = [100.0, 200.0, 24.0, 24.0];
    let overlay = [90.0, 190.0, 160.0, 48.0];
    let damage = union_rects(chip, overlay);
    assert_eq!(
        frame_clear_policy(true, false, Some(damage), vp),
        FrameClearPolicy::PartialLoad
    );
    assert!(!overlay_pixels_need_full_clear(true, Some(overlay)));
    assert!(skip_full_clear_for_portal_structure(true, false));
}

#[test]
fn mass_hover_leave_union_covers_viewport_full_clears() {
    // Class-hover sentinel leave on a gallery of cards: union → FullClear even when
    // force_full_redraw is false (the leftover flash after overlay-only skip).
    let vp = [0.0, 0.0, 800.0, 600.0];
    let tiles = [
        [0.0, 0.0, 420.0, 320.0],
        [380.0, 0.0, 420.0, 320.0],
        [0.0, 280.0, 420.0, 320.0],
        [380.0, 280.0, 420.0, 320.0],
    ];
    let u = rects_union(&tiles).expect("tiles");
    assert!(damage_covers_full(vp, u));
    assert_eq!(
        frame_clear_policy(true, false, Some(u), vp),
        FrameClearPolicy::FullClear
    );
}

#[test]
fn popover_bounds_partial_load_modal_underlay_full_clear() {
    let vp = [0.0, 0.0, 800.0, 600.0];
    assert_eq!(
        frame_clear_policy(true, false, Some([120.0, 80.0, 72.0, 36.0]), vp),
        FrameClearPolicy::PartialLoad
    );
    assert_eq!(
        frame_clear_policy(true, false, Some(vp), vp),
        FrameClearPolicy::FullClear
    );
}

#[test]
fn accumulate_paint_damage_does_not_require_full_clear() {
    let mut damage = None;
    let force = false;
    accumulate_paint_damage(&mut damage, [100.0, 200.0, 50.0, 16.0]);
    // Callers must not set force_full when using this helper (WP-2.2 bind_text).
    assert!(!force);
    assert_eq!(damage, Some([100.0, 200.0, 50.0, 16.0]));
    let vp = [0.0, 0.0, 800.0, 600.0];
    assert_eq!(
        frame_clear_policy(true, force, damage, vp),
        FrameClearPolicy::PartialLoad
    );
}

#[test]
fn push_frame_paint_damage_is_local_not_full_viewport() {
    let _ = take_frame_paint_nodes();
    push_frame_paint_damage([10.0, 20.0, 40.0, 24.0]);
    push_frame_paint_damage([30.0, 20.0, 40.0, 24.0]);
    let nodes = take_frame_paint_nodes();
    assert_eq!(nodes.len(), 2, "disjoint widgets must not union");
    let vp = [0.0, 0.0, 800.0, 600.0];
    assert!(!nodes.iter().any(|d| damage_covers_full(vp, d.bounds)));
    assert_eq!(
        frame_clear_policy_for_nodes(true, false, &nodes, vp),
        FrameClearPolicy::PartialLoad
    );
    assert!(take_frame_paint_nodes().is_empty());
}

#[test]
fn disjoint_nodes_covering_viewport_still_partial_load() {
    let _ = take_frame_paint_nodes();
    push_frame_paint_damage([0.0, 0.0, 400.0, 600.0]);
    push_frame_paint_damage([400.0, 0.0, 400.0, 600.0]);
    let nodes = take_frame_paint_nodes();
    let vp = [0.0, 0.0, 800.0, 600.0];
    let union = rects_union(&nodes.iter().map(|d| d.bounds).collect::<Vec<_>>()).unwrap();
    assert!(damage_covers_full(vp, union));
    assert_eq!(
        frame_clear_policy_for_nodes(true, false, &nodes, vp),
        FrameClearPolicy::PartialLoad,
        "union covering the viewport is not a node FullClear"
    );
}

#[test]
fn hover_node_text_keeps_own_label_rejects_sibling() {
    let default_btn = [8.0, 8.0, 88.0, 36.0];
    let default_label = [24.0, 16.0, 56.0, 18.0];
    let outline_label = [104.0, 16.0, 56.0, 18.0];
    assert!(
        text_center_in_dirty(default_label, default_btn),
        "hovered Default must redraw its own label after chrome overwrites retain"
    );
    assert!(
        !text_center_in_dirty(outline_label, default_btn),
        "sibling Outline must not stamp into Default's scissor"
    );
}

#[test]
fn dirty_slot_match_rejects_ancestor_and_underlapping_neighbor() {
    let chip = [400.0, 200.0, 32.0, 24.0];
    let padded = [396.0, 196.0, 40.0, 32.0];
    let page = [0.0, 0.0, 800.0, 600.0];
    let title = [80.0, 180.0, 120.0, 20.0];
    assert!(paint_dirty_matches_slot(1, padded, 1, chip));
    assert!(paint_dirty_matches_slot(99, padded, 1, chip));
    assert!(
        !paint_dirty_matches_slot(1, chip, 2, page),
        "page root must not paint into a chip scissor"
    );
    assert!(
        !paint_dirty_matches_slot(1, chip, 3, title),
        "nearby caption must not paint into a chip scissor"
    );
}

#[test]
fn keyed_spin_must_not_also_match_same_box_parent() {
    // MarkerIcon `size-4` wraps Icon `size-4`. `same_box` matches both;
    // the parent's textured range includes the lucide quad. Drawing both
    // src-overs the spinner twice on PartialLoad (idle thicker than scroll).
    let icon = [8.0, 8.0, 16.0, 16.0];
    let parent = [8.0, 8.0, 16.0, 16.0];
    let rot_pad = (16.0f32.hypot(16.0) - 16.0).max(0.0) * 0.5;
    let pad = 2.0 + rot_pad;
    let dirty = [
        icon[0] - pad,
        icon[1] - pad,
        icon[2] + pad * 2.0,
        icon[3] + pad * 2.0,
    ];
    let icon_key = 10u64;
    let parent_key = 9u64;
    let slots = [(icon_key, icon), (parent_key, parent)];
    assert!(
        paint_dirty_matches_slot(icon_key, dirty, parent_key, parent),
        "raw same_box still matches the wrapper — present must not use that"
    );
    let has_keyed = paint_dirty_has_keyed_slot(icon_key, dirty, slots);
    assert!(has_keyed, "Icon layout_node must win over the wrapper");
    assert!(paint_dirty_matches_present_slot(
        icon_key, dirty, icon_key, icon, has_keyed
    ));
    assert!(
        !paint_dirty_matches_present_slot(icon_key, dirty, parent_key, parent, has_keyed),
        "wrapper collect range must not redraw the child's spinner a second time"
    );
    let bounds_key = 99u64;
    let has_keyed = paint_dirty_has_keyed_slot(bounds_key, dirty, slots);
    assert!(!has_keyed);
    assert!(
        paint_dirty_matches_present_slot(bounds_key, dirty, icon_key, icon, has_keyed),
        "bounds-key chrome with no Taffy slot still uses same_box"
    );
}

#[test]
fn recycled_taffy_key_must_not_match_disjoint_sidebar_footer() {
    let footer = [0.0, 752.0, 207.0, 48.0];
    let chat = [420.0, 180.0, 384.0, 384.0];
    assert!(
        !paint_dirty_matches_slot(42, chat, 42, footer),
        "NodeId reuse after Send must not treat SidebarFooter as the MessageScroller island"
    );
    let hover = [8.0, 760.0, 190.0, 36.0];
    assert!(
        paint_dirty_matches_slot(42, hover, 42, footer),
        "same-widget hover still matches the footer slot"
    );
}

#[test]
fn restore_slot_skips_page_root_keeps_covered_widget() {
    let hole = [400.0, 200.0, 80.0, 60.0];
    let page = [0.0, 0.0, 800.0, 600.0];
    let bubble = [410.0, 210.0, 60.0, 36.0];
    let neighbor = [80.0, 40.0, 160.0, 20.0];
    assert!(!restore_slot_in_hole(page, hole));
    assert!(restore_slot_in_hole(bubble, hole));
    assert!(!restore_slot_in_hole(neighbor, hole));
    assert!(
        restore_page_behind_overlay(page, hole),
        "overlay dest-reset AABB must be covered by the page/card fill, not left as a white frame"
    );
    assert!(restore_page_behind_overlay(bubble, hole));
    assert!(!restore_page_behind_overlay(neighbor, hole));
}

#[test]
fn note_frame_clear_policy_counts() {
    reset_frame_policy_counters_for_test();
    note_frame_clear_policy(FrameClearPolicy::PartialLoad);
    note_frame_clear_policy(FrameClearPolicy::PartialLoad);
    note_frame_clear_policy(FrameClearPolicy::FullClear);
    assert_eq!(partial_load_frame_count(), 2);
    assert_eq!(full_clear_frame_count(), 1);
}

#[test]
fn suppress_skips_increment_and_snapshot_captures() {
    reset_frame_policy_counters_for_test();
    suppress_policy_counts(1);
    note_frame_clear_policy(FrameClearPolicy::PartialLoad);
    assert_eq!(partial_load_frame_count(), 0);
    note_frame_clear_policy(FrameClearPolicy::PartialLoad);
    assert_eq!(partial_load_frame_count(), 1);
    arm_policy_snapshot();
    note_frame_clear_policy(FrameClearPolicy::FullClear);
    let snap = take_policy_snapshot().expect("armed");
    assert_eq!(snap.partial_load, 1);
    assert_eq!(snap.full_clear, 1);
    assert!(take_policy_snapshot().is_none());
}

#[test]
fn scroll_layer_clear_keeps_viewport_damage() {
    let vp = [0.0, 0.0, 800.0, 600.0];
    let overlay = [90.0, 190.0, 160.0, 48.0];
    let d = overlay_membership_damage(Some(vp), Some(overlay), vp, true, true);
    assert_eq!(d, Some(vp));
}

#[test]
fn portal_join_must_not_suppress_existing_page_scroll() {
    // Bubble +N hashes a new overlay ScrollArea into the fingerprint. That spawn
    // is not page motion. A page viewport that actually moved still owns the
    // layer Clear — otherwise dest-reset of prev∪now punches content below.
    let mut prev = HashMap::new();
    prev.insert("main".into(), [0u32, 100u32]);
    let mut cur_spawn = prev.clone();
    cur_spawn.insert("overlay".into(), [0, 0]);
    assert!(
        !existing_scroll_offset_changed(&prev, &cur_spawn),
        "overlay ScrollArea spawn is not page scroll"
    );
    let mut cur_scrolled = HashMap::new();
    cur_scrolled.insert("main".into(), [0, 140]);
    cur_scrolled.insert("overlay".into(), [0, 0]);
    assert!(
        existing_scroll_offset_changed(&prev, &cur_scrolled),
        "+N during scroll must still see the page viewport move"
    );
    let prev_slots = [(1u64, [10.0, 200.0, 300.0, 400.0])];
    let now_slots = [(1u64, [10.0, 120.0, 300.0, 400.0])];
    let overlay_only = [PaintDirty {
        key: 99,
        bounds: [40.0, 80.0, 48.0, 28.0],
        kind: PaintDirtyKind::Overlay,
    }];
    let unnamed = unnamed_translated_page_slots(&now_slots, &prev_slots, &overlay_only);
    assert!(
        unnamed,
        "overlay dirties do not name the translated page card"
    );
    let page_scrolled = existing_scroll_offset_changed(&prev, &cur_scrolled);
    assert!(
        !node_present_moved_forces_full_clear(unnamed, page_scrolled),
        "page scroll layer owns the translation; +N must not window-FullClear"
    );
    assert!(
        node_present_moved_forces_full_clear(unnamed, false),
        "without a scroll layer the unnamed card would FullClear — that is the flicker"
    );
    let mut dirties = Vec::new();
    expand_paint_dirties_for_translated_slots(&mut dirties, &now_slots, &prev_slots);
    assert_eq!(dirties.len(), 1);
    assert!(
            (dirties[0].bounds[3] - 480.0).abs() < 0.5,
            "dest-reset prev∪now of a scrolled 400px card is a 480px strip over content below\n bounds={:?}",
            dirties[0].bounds
        );
    let prev_cell = [48.0, 400.0, 36.0, 36.0];
    let now_cell = [48.0, 40.0, 36.0, 36.0];
    let mut cells = Vec::new();
    expand_paint_dirties_for_translated_slots(&mut cells, &[(3, now_cell)], &[(3, prev_cell)]);
    let gap_y = 220.0;
    assert!(
        cells
            .iter()
            .all(|d| gap_y < d.bounds[1] || gap_y > d.bounds[1] + d.bounds[3]),
        "a scrolled OTP cell must not clear the column between poses\n {cells:?}"
    );
    assert!(cells.len() >= 2, "each pose is its own hole\n {cells:?}");
}

#[test]
fn scroll_strip_blit_viewport_axis_aligned_rejects_rotate() {
    let mut el = Element::new();
    el.layout_bounds = [0.0, 0.0, 200.0, 100.0];
    assert!(
        scroll_strip_blit_viewport_axis_aligned(&el),
        "identity viewport may blit"
    );
    el.style.rotate_deg = 45.0;
    assert!(
        !scroll_strip_blit_viewport_axis_aligned(&el),
        "local rotate must abort strip blit"
    );
    el.style.rotate_deg = 0.0;
    el.paint_ancestor_linear = crate::style::mat2d_rotate(45.0_f32.to_radians());
    assert!(
        !scroll_strip_blit_viewport_axis_aligned(&el),
        "ancestor rotate must abort strip blit"
    );
    el.paint_ancestor_linear = crate::style::MAT2D_IDENTITY;
    assert!(
        scroll_strip_blit_viewport_axis_aligned(&el),
        "cleared rotate must blit again"
    );
    assert!(
        plan_scroll_strip_blit([0.0, 0.0, 200.0, 100.0], [0.0, 16.0]).is_some(),
        "axis-aligned plan walls unchanged"
    );
    assert!(
        plan_scroll_strip_blit([0.0, 0.0, 200.0, 100.0], [0.0, 100.0]).is_none(),
        "|delta|≥viewport stays a wall"
    );
}

#[test]
fn plan_scroll_strip_blit_down_exposes_bottom() {
    let vp = [10.0, 20.0, 200.0, 100.0];
    let plan = plan_scroll_strip_blit(vp, [0.0, 24.0]).expect("down");
    assert_eq!(plan.src, [10.0, 44.0, 200.0, 76.0]);
    assert_eq!(plan.dst_origin, [10.0, 20.0]);
    assert_eq!(plan.exposed, [10.0, 96.0, 200.0, 24.0]);
    assert!(plan.exposed_second.is_none());
    assert!(plan.exposed[3] < vp[3]);
}

#[test]
fn plan_scroll_strip_blit_up_exposes_top() {
    let vp = [0.0, 0.0, 100.0, 80.0];
    let plan = plan_scroll_strip_blit(vp, [0.0, -16.0]).expect("up");
    assert_eq!(plan.src, [0.0, 0.0, 100.0, 64.0]);
    assert_eq!(plan.dst_origin, [0.0, 16.0]);
    assert_eq!(plan.exposed, [0.0, 0.0, 100.0, 16.0]);
    assert!(plan.exposed_second.is_none());
}

#[test]
fn plan_scroll_strip_blit_right_exposes_right() {
    let vp = [0.0, 0.0, 120.0, 40.0];
    let plan = plan_scroll_strip_blit(vp, [30.0, 0.0]).expect("right");
    assert_eq!(plan.src, [30.0, 0.0, 90.0, 40.0]);
    assert_eq!(plan.dst_origin, [0.0, 0.0]);
    assert_eq!(plan.exposed, [90.0, 0.0, 30.0, 40.0]);
    assert!(plan.exposed_second.is_none());
}

#[test]
fn plan_scroll_strip_blit_left_exposes_left() {
    let vp = [8.0, 4.0, 80.0, 50.0];
    let plan = plan_scroll_strip_blit(vp, [-20.0, 0.0]).expect("left");
    assert_eq!(plan.src, [8.0, 4.0, 60.0, 50.0]);
    assert_eq!(plan.dst_origin, [28.0, 4.0]);
    assert_eq!(plan.exposed, [8.0, 4.0, 20.0, 50.0]);
    assert!(plan.exposed_second.is_none());
}

#[test]
fn plan_scroll_strip_blit_rejects_diagonal_full_and_tiny() {
    let vp = [0.0, 0.0, 200.0, 100.0];
    let diag = plan_scroll_strip_blit(vp, [12.0, 8.0]).expect("dual-axis");
    assert_eq!(diag.src, [12.0, 8.0, 188.0, 92.0]);
    assert_eq!(diag.dst_origin, [0.0, 0.0]);
    assert_eq!(diag.exposed, [0.0, 92.0, 200.0, 8.0]);
    assert_eq!(diag.exposed_second, Some([188.0, 0.0, 12.0, 92.0]));
    assert_eq!(diag.exposed_union()[2], 200.0);
    assert_eq!(diag.exposed_union()[3], 100.0);
    assert!(
        intersect_rects(diag.exposed, diag.exposed_second.unwrap()).is_none(),
        "L strips must be disjoint (corner lives in the |dy| band only)"
    );
    assert!(plan_scroll_strip_blit(vp, [0.0, 100.0]).is_none());
    assert!(plan_scroll_strip_blit(vp, [0.0, 0.25]).is_none());
    assert!(plan_scroll_strip_blit(vp, [0.0, 0.0]).is_none());
    assert!(plan_scroll_strip_blit([0.0, 0.0, 0.0, 10.0], [0.0, 4.0]).is_none());
}

fn overlap_mid(plan: &ScrollStripBlit) -> [f32; 2] {
    let dst = scroll_blit_copied_dst(plan);
    [dst[0] + dst[2] * 0.5, dst[1] + dst[3] * 0.5]
}

fn contains_point(r: [f32; 4], p: [f32; 2]) -> bool {
    p[0] >= r[0] && p[1] >= r[1] && p[0] < r[0] + r[2] && p[1] < r[1] + r[3]
}

#[test]
fn overlay_scrollbar_pin_must_not_expand_pass_scissor_over_copied_overlap() {
    // Gallery page scroll: overlay scrollbar is a pin. The old expand used
    // `viewport ∪ pin` as the wgpu scissor. Straddling card fills then
    // painted the whole card over the blit (empty shells); culled glyphon
    // only redrew the exposed strip (doubled / missing captions).
    let vp = [200.0, 0.0, 600.0, 800.0];
    let plan = plan_scroll_strip_blit(vp, [0.0, 75.0]).expect("down");
    let pin = [776.0, 0.0, 12.0, 800.0];
    let mid = overlap_mid(&plan);
    let legacy = union_rects(plan.viewport, pin);
    assert!(
        contains_point(legacy, mid),
        "sanity: viewport∪pin covers the copied overlap mid {mid:?}"
    );
    match scroll_blit_pass_scissor_logical(&[plan], &[pin]) {
        Some(sc) => assert!(
            !contains_point(sc, mid),
            "pass scissor {sc:?} must not cover copied overlap mid {mid:?}"
        ),
        None => {}
    }
    let no_pin = scroll_blit_pass_scissor_logical(&[plan], &[]).expect("single strip");
    assert_eq!(no_pin, plan.exposed);
    assert!(!contains_point(no_pin, mid));
    assert_eq!(
        physical_xywh_to_scissor([200.5, 725.2, 600.0, 75.0], 800, 800),
        Some((200, 725, 600, 75))
    );
}

#[test]
fn dual_axis_pass_scissor_must_not_be_exposed_union() {
    let plan = plan_scroll_strip_blit([0.0, 0.0, 200.0, 100.0], [12.0, 8.0]).expect("L");
    let mid = overlap_mid(&plan);
    assert!(
        contains_point(plan.exposed_union(), mid),
        "sanity: L AABB is the viewport"
    );
    assert!(
        scroll_blit_pass_scissor_logical(&[plan], &[]).is_none(),
        "dual-axis must not collapse to one AABB scissor"
    );
}

#[test]
fn scroll_copy_excludes_fractional_footer_separator_pixel() {
    let viewport = [8.0, 75.0, 240.0, 276.0];
    for scale in [1.0, 1.25, 1.5, 1.75, 2.0] {
        for delta in [[0.0, 20.0], [0.0, -20.0], [20.0, 0.0], [-20.0, 0.0]] {
            let plan = plan_scroll_strip_blit(viewport, delta).unwrap();
            let copy = scroll_strip_copy_physical(&plan, scale, 1200, 900).unwrap();
            let right = ((viewport[0] + viewport[2]) * scale).floor() as u32;
            let bottom = ((viewport[1] + viewport[3]) * scale).floor() as u32;
            assert!(copy.src_x + copy.width <= right && copy.dst_x + copy.width <= right);
            assert!(copy.src_y + copy.height <= bottom && copy.dst_y + copy.height <= bottom);
        }
    }
}

#[test]
fn scroll_strip_copy_physical_integer_delta() {
    let vp = [0.0, 0.0, 200.0, 100.0];
    let plan = plan_scroll_strip_blit(vp, [0.0, 10.0]).expect("down");
    let copy = scroll_strip_copy_physical(&plan, 2.0, 400, 200).expect("phys");
    assert_eq!(copy.src_y, 20);
    assert_eq!(copy.dst_y, 0);
    assert_eq!(copy.height, 180);
    assert_eq!(copy.width, 400);
    assert!(scroll_strip_copy_physical(&plan, 2.0, 400, 200).is_some());
    let tiny = plan_scroll_strip_blit(vp, [0.0, 0.6]).expect("logical ok");
    // 0.6 * 1.0 rounds to 1px; 0.2 * 1.0 rounds to 0 → None
    assert!(scroll_strip_copy_physical(&tiny, 1.0, 200, 100).is_some());
    let sub = plan_scroll_strip_blit(vp, [0.0, 0.6]).expect("sub");
    assert!(scroll_strip_copy_physical(&sub, 0.4, 80, 40).is_none());
    let diag = plan_scroll_strip_blit(vp, [12.0, 8.0]).expect("dual");
    let copy = scroll_strip_copy_physical(&diag, 1.0, 200, 100).expect("dual phys");
    assert_eq!(copy.src_x, 12);
    assert_eq!(copy.src_y, 8);
    assert_eq!(copy.dst_x, 0);
    assert_eq!(copy.dst_y, 0);
    assert_eq!(copy.width, 188);
    assert_eq!(copy.height, 92);
}

#[test]
fn chase_strip_blit_delta_must_match_glyphon_quad() {
    // Copied overlap pixels must move by the same device delta as glyphon
    // quads. A 1px disagreement is the chase-time "text jitter" (blit then
    // a later FullClear snaps back).
    use crate::app::scroll_support::{snap_scroll_offset_to_device_px, SmoothScrollAxis};

    fn glyphon_quad_y(layout_y: f32, scroll_y: f32, ui_scale: f32) -> i32 {
        let top = (layout_y - scroll_y) * ui_scale;
        top.trunc() as i32
    }

    let vp = [0.0, 48.6, 200.0, 400.0];
    for scale in [1.0_f32, 1.25, 1.5, 2.0] {
        let mut scroll = 0.0_f32;
        let mut motion = SmoothScrollAxis::new(scroll);
        let target = snap_scroll_offset_to_device_px(80.0, scale);
        let layout_y = 120.4_f32;
        loop {
            let (next, anim) = motion.step(scroll, target, 0.016, scale);
            let dy = next - scroll;
            if dy.abs() >= 0.5 {
                let plan = plan_scroll_strip_blit(vp, [0.0, dy]).expect("plan");
                let copy =
                    scroll_strip_copy_physical(&plan, scale, 800, 1200).expect("physical copy");
                let blit = copy.src_y as i32 - copy.dst_y as i32;
                let glyph =
                    glyphon_quad_y(layout_y, scroll, scale) - glyphon_quad_y(layout_y, next, scale);
                assert_eq!(
                    blit, glyph,
                    "scale={scale} scroll {scroll} -> {next}: blit {blit}px vs glyph {glyph}px"
                );
            }
            scroll = next;
            if !anim {
                break;
            }
        }
    }
}

#[test]
fn note_scroll_strip_blit_counts() {
    reset_frame_policy_counters_for_test();
    note_scroll_strip_blit();
    note_scroll_strip_blit();
    assert_eq!(scroll_strip_blit_count(), 2);
    reset_frame_policy_counters_for_test();
    assert_eq!(scroll_strip_blit_count(), 0);
}

fn bits(v: f32) -> u32 {
    v.to_bits()
}

fn allow_policy() -> ScrollStripBlitPolicy {
    ScrollStripBlitPolicy {
        enabled: true,
        gpu_partial: true,
        retain_valid: true,
        force_full_redraw: false,
        named_paint_dirty: false,
        overlay: false,
    }
}

#[test]
fn scroll_strip_blit_policy_rejects_invalid_retain_not_overlay_or_named_paint() {
    assert!(allow_policy().allows());
    assert!(
        ScrollStripBlitPolicy {
            overlay: true,
            ..allow_policy()
        }
        .allows(),
        "overlay is a pin hole, not an abort gate"
    );
    assert!(
        ScrollStripBlitPolicy {
            named_paint_dirty: true,
            ..allow_policy()
        }
        .allows(),
        "named paint is a pin hole, not an abort gate"
    );
    assert!(!ScrollStripBlitPolicy {
        retain_valid: false,
        ..allow_policy()
    }
    .allows());
    assert!(!ScrollStripBlitPolicy {
        force_full_redraw: true,
        ..allow_policy()
    }
    .allows());
    assert!(!ScrollStripBlitPolicy {
        gpu_partial: false,
        ..allow_policy()
    }
    .allows());
    assert!(!ScrollStripBlitPolicy {
        enabled: false,
        ..allow_policy()
    }
    .allows());
}

#[test]
fn failed_strip_copy_does_not_skip_page_on_full_clear() {
    assert!(
        !should_retain_unmoved_scroll(false, true, false),
        "full-clear fallback must render the page even when a strip copy failed"
    );
    assert!(should_retain_unmoved_scroll(true, true, false));
    assert!(!should_retain_unmoved_scroll(true, false, false));
    assert!(!should_retain_unmoved_scroll(true, true, true));
}

#[test]
fn single_viewport_scroll_delta_one_axis_ok_two_viewports_or_spawn_none() {
    let mut prev = HashMap::new();
    prev.insert("a".into(), [bits(0.0), bits(0.0)]);
    prev.insert("b".into(), [bits(0.0), bits(8.0)]);
    let mut cur = prev.clone();
    cur.insert("a".into(), [bits(0.0), bits(16.0)]);
    let (id, delta) = single_viewport_scroll_delta(&prev, &cur).expect("one viewport");
    assert_eq!(id, "a");
    assert!((delta[1] - 16.0).abs() < 1e-4);
    assert!(delta[0].abs() < 1e-4);

    cur.insert("b".into(), [bits(0.0), bits(24.0)]);
    assert!(
        single_viewport_scroll_delta(&prev, &cur).is_none(),
        "two viewports moving"
    );

    let mut spawn = prev.clone();
    spawn.insert("c".into(), [bits(0.0), bits(0.0)]);
    assert!(single_viewport_scroll_delta(&prev, &spawn).is_none());
    assert!(single_viewport_scroll_delta(&prev, &prev).is_none());
}

#[test]
fn changed_viewport_scroll_deltas_two_disjoint_ok_spawn_none() {
    let mut prev = HashMap::new();
    prev.insert("a".into(), [bits(0.0), bits(0.0)]);
    prev.insert("b".into(), [bits(0.0), bits(8.0)]);
    let mut cur = prev.clone();
    cur.insert("a".into(), [bits(0.0), bits(16.0)]);
    cur.insert("b".into(), [bits(0.0), bits(24.0)]);
    let mut deltas = changed_viewport_scroll_deltas(&prev, &cur).expect("both moved");
    deltas.sort_by(|x, y| x.0.cmp(&y.0));
    assert_eq!(deltas.len(), 2);
    assert_eq!(deltas[0].0, "a");
    assert!((deltas[0].1[1] - 16.0).abs() < 1e-4);
    assert_eq!(deltas[1].0, "b");
    assert!((deltas[1].1[1] - 16.0).abs() < 1e-4);

    let mut spawn = prev.clone();
    spawn.insert("c".into(), [bits(0.0), bits(0.0)]);
    assert!(changed_viewport_scroll_deltas(&prev, &spawn).is_none());
    assert!(changed_viewport_scroll_deltas(&prev, &prev).is_none());

    let mut moved_and_spawn = prev.clone();
    moved_and_spawn.insert("a".into(), [bits(0.0), bits(16.0)]);
    moved_and_spawn.insert("c".into(), [bits(0.0), bits(0.0)]);
    let deltas = changed_viewport_scroll_deltas(&prev, &moved_and_spawn).expect("A still moved");
    assert_eq!(deltas.len(), 1);
    assert_eq!(deltas[0].0, "a");

    let mut despawn = prev.clone();
    despawn.remove("b");
    despawn.insert("a".into(), [bits(0.0), bits(16.0)]);
    let deltas = changed_viewport_scroll_deltas(&prev, &despawn).expect("A moved under despawn");
    assert_eq!(deltas.len(), 1);
    assert_eq!(deltas[0].0, "a");
    let mut gone = despawned_viewport_ids(&prev, &despawn);
    gone.sort();
    assert_eq!(gone, ["b"]);
}

fn blit_job(id: &str, viewport: [f32; 4], dy: f32) -> ScrollStripBlitJob {
    ScrollStripBlitJob {
        viewport_id: id.into(),
        plan: plan_scroll_strip_blit(viewport, [0.0, dy]).expect("plan"),
    }
}

#[test]
fn nested_inner_is_pinned_outer_and_disjoint_still_blit() {
    let outer = blit_job("outer", [0.0, 0.0, 400.0, 600.0], 16.0);
    let inner = blit_job("inner", [40.0, 80.0, 320.0, 400.0], 8.0);
    let side = blit_job("side", [500.0, 0.0, 200.0, 600.0], 12.0);
    let (kept, dropped) = partition_disjoint_blit_jobs(vec![outer, inner, side]);
    let mut kept_ids: Vec<_> = kept.iter().map(|j| j.viewport_id.as_str()).collect();
    kept_ids.sort_unstable();
    assert_eq!(kept_ids, ["outer", "side"]);
    assert_eq!(dropped.len(), 1);
    assert_eq!(dropped[0].viewport_id, "inner");
    let occ = overlapping_sibling_occupancy_pins(&kept);
    assert!(
        occ.is_empty(),
        "nested/disjoint kept jobs must not emit sibling occupancy, got {occ:?}"
    );
}

#[test]
fn three_level_nesting_keeps_outermost_only() {
    let outer = blit_job("outer", [0.0, 0.0, 400.0, 600.0], 16.0);
    let mid = blit_job("mid", [20.0, 40.0, 360.0, 500.0], 8.0);
    let inner = blit_job("inner", [40.0, 80.0, 320.0, 400.0], 12.0);
    let (kept, dropped) = partition_disjoint_blit_jobs(vec![outer, mid, inner]);
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].viewport_id, "outer");
    let mut dropped_ids: Vec<_> = dropped.iter().map(|j| j.viewport_id.as_str()).collect();
    dropped_ids.sort_unstable();
    assert_eq!(dropped_ids, ["inner", "mid"]);
}

#[test]
fn overlapping_sibling_viewports_keep_both() {
    let a = blit_job("a", [0.0, 0.0, 200.0, 200.0], 16.0);
    let b = blit_job("b", [100.0, 100.0, 200.0, 200.0], 8.0);
    let (kept, dropped) = partition_disjoint_blit_jobs(vec![a, b]);
    let mut kept_ids: Vec<_> = kept.iter().map(|j| j.viewport_id.as_str()).collect();
    kept_ids.sort_unstable();
    assert_eq!(kept_ids, ["a", "b"]);
    assert!(dropped.is_empty());
    assert_eq!(overlapping_sibling_occupancy_pins(&kept).len(), 1);
}

#[test]
fn overlapping_sibling_occupancy_covers_intersection_and_smear() {
    let pin = overlapping_sibling_occupancy_pin(
        [0.0, 0.0, 200.0, 200.0],
        [0.0, 16.0],
        [100.0, 100.0, 200.0, 200.0],
        [0.0, 8.0],
    )
    .expect("overlap");
    let inter = intersect_rects([0.0, 0.0, 200.0, 200.0], [100.0, 100.0, 200.0, 200.0]).unwrap();
    assert!(
        rect_contains_rect(pin, inter),
        "pin must cover intersection"
    );
    assert!(
        rect_contains_rect(pin, [100.0, 116.0, 100.0, 84.0]),
        "pin must cover A's dest smear inside A∪B, got {pin:?}"
    );
    assert!(overlapping_sibling_occupancy_pin(
        [0.0, 0.0, 400.0, 600.0],
        [0.0, 16.0],
        [40.0, 80.0, 320.0, 400.0],
        [0.0, 8.0],
    )
    .is_none());
}

#[test]
fn identical_viewports_drop_both() {
    let a = blit_job("a", [0.0, 0.0, 200.0, 200.0], 16.0);
    let b = blit_job("b", [0.0, 0.0, 200.0, 200.0], 8.0);
    let (kept, dropped) = partition_disjoint_blit_jobs(vec![a, b]);
    assert!(kept.is_empty());
    assert_eq!(dropped.len(), 2);
}

#[test]
fn overlay_disjoint_from_viewport_needs_no_pin() {
    let scroller = [240.0, 0.0, 560.0, 600.0];
    assert!(!overlay_needs_scroll_blit_pin(
        [8.0, 8.0, 48.0, 32.0],
        &[scroller]
    ));
    assert!(overlay_needs_scroll_blit_pin(
        [300.0, 40.0, 160.0, 80.0],
        &[scroller]
    ));
}

#[test]
fn merge_scroll_blit_pins_overlay_intersect_named_sticky() {
    let scroller = [0.0, 0.0, 400.0, 300.0];
    let overlay = [40.0, 40.0, 80.0, 48.0];
    let hover = [500.0, 10.0, 32.0, 16.0];
    let sticky = [0.0, 0.0, 400.0, 28.0];
    let pins = merge_scroll_blit_pins(
        Some(overlay),
        &[],
        false,
        &[hover],
        &[sticky],
        &[],
        &[scroller],
    );
    assert!(
        pins.iter().any(|p| (p[2] - 80.0).abs() < 0.1),
        "intersecting overlay must pin, got {pins:?}"
    );
    assert!(pins.iter().any(|p| (p[0] - 500.0).abs() < 0.1));
    assert!(pins.iter().any(|p| (p[3] - 28.0).abs() < 0.1));

    let disjoint = merge_scroll_blit_pins(
        Some([500.0, 0.0, 40.0, 40.0]),
        &[],
        false,
        &[],
        &[],
        &[],
        &[scroller],
    );
    assert!(
        disjoint.is_empty(),
        "disjoint overlay must not pin, got {disjoint:?}"
    );
}

#[test]
fn sticky_layout_bounds_collects_nonzero_boxes() {
    let mut root = Element::new();
    let mut sticky = Element::new();
    sticky.style.position_sticky = true;
    sticky.layout_bounds = [0.0, 0.0, 200.0, 32.0];
    let mut other = Element::new();
    other.layout_bounds = [0.0, 32.0, 200.0, 100.0];
    root.children.push(sticky);
    root.children.push(other);
    assert_eq!(sticky_layout_bounds(&root), vec![[0.0, 0.0, 200.0, 32.0]]);
}

#[test]
fn overlay_scrollbar_on_blit_viewport_is_a_pin() {
    let mut root = Element::new();
    let mut track = Element::new();
    track.focus_id = Some(format!(
        "{}sa-main:y",
        crate::core::scrollbar::SCROLLBAR_TRACK_FOCUS_PREFIX
    ));
    track.set_attr("jadix_scrollbar_overlay", "1");
    track.layout_bounds = [392.0, 8.0, 8.0, 280.0];
    root.children.push(track);
    let vp = [0.0, 0.0, 400.0, 300.0];
    let pins = overlay_scrollbar_scroll_blit_pins(&root, &[String::from("sa-main")], &[vp]);
    assert_eq!(
        pins,
        vec![[392.0, 8.0, 8.0, 280.0]],
        "inside overlay rail sharing the blit viewport must pin"
    );
    let disjoint = overlay_scrollbar_scroll_blit_pins(&root, &[String::from("other")], &[vp]);
    assert!(
        disjoint.is_empty(),
        "rails of other viewports must not pin, got {disjoint:?}"
    );

    // Layout-only ancestors must retain the rail in the subtree summary.
    let mut root = Element::new()
        .child(Element::new().child(root))
        .child(Element::new().child(Element::new()));
    crate::layout::collect_cache::stamp_subtree_paint_hashes(&mut root, false);
    assert_eq!(
        overlay_scrollbar_scroll_blit_pins(&root, &[String::from("sa-main")], &[vp]),
        pins
    );
    let track = &mut root.children[0].children[0].children[0];
    track.focus_id = None;
    crate::layout::collect_cache::stamp_subtree_paint_hashes(&mut root, false);
    assert!(
        overlay_scrollbar_scroll_blit_pins(&root, &[String::from("sa-main")], &[vp]).is_empty()
    );
}

#[test]
fn nested_scrollbar_pins_follow_the_ancestor_scroll_transform() {
    let mut rail = Element::new()
        .absolute()
        .focus_id("scrollbar-track:inner:y");
    rail.layout_bounds = [180.0, 120.0, 10.0, 140.0];
    rail.paint_scroll_acc = [0.0, 80.0];
    let root = Element::new().child(rail);
    let pins =
        overlay_scrollbar_scroll_blit_pins(&root, &["inner".into()], &[[0.0, 0.0, 200.0, 100.0]]);
    assert_eq!(pins, vec![[180.0, 40.0, 10.0, 140.0]]);
}

#[test]
fn rectangular_ancestor_border_keeps_antialiasing_out_of_scroll_copy() {
    let viewport = [10.0, 10.0, 200.0, 160.0];
    let mut child = Element::new().focus_id("scroll:bordered");
    child.layout_bounds = viewport;
    child.contains_scroll_viewport = true;
    let mut root = Element::new()
        .border([0.5, 0.5, 0.5, 1.0], 1.0)
        .child(child);
    root.layout_bounds = viewport;
    root.contains_scroll_viewport = true;
    let pins = scroll_ancestor_chrome_blit_pins(&root, "bordered", viewport);
    assert!(pins.contains(&[10.0, 10.0, 200.0, 2.0]));
    assert!(pins
        .iter()
        .all(|pin| intersect_rects(*pin, [50.0, 50.0, 100.0, 40.0]).is_none()));
}

#[test]
fn rounded_ancestor_edges_and_copied_imprints_are_repaired() {
    let vp = [10.0, 40.0, 200.0, 160.0];
    let mut viewport = Element::new();
    viewport.focus_id = Some(crate::core::scroll_viewport::focus_id_for_scroll_viewport(
        "rounded",
    ));
    viewport.contains_scroll_viewport = true;
    viewport.layout_bounds = vp;
    let mut shell = Element::new()
        .rounded(12.0)
        .overflow("hidden")
        .child(viewport);
    shell.contains_scroll_viewport = true;
    shell.layout_bounds = [10.0, 10.0, 200.0, 190.0];
    let pins = scroll_ancestor_chrome_blit_pins(&shell, "rounded", vp);
    assert_eq!(
        pins.len(),
        3,
        "left, right and bottom intersect the scrollport"
    );
    assert!(pins
        .iter()
        .all(|pin| intersect_rects(*pin, vp) == Some(*pin)));
    assert!(scroll_ancestor_chrome_blit_pins(&shell, "other", vp).is_empty());
    let smear = smear_out_of_flow_after_strip_blit(&pins, &[vp], &[[0.0, 30.0]]);
    assert!(
        smear.contains(&[10.0, 156.0, 200.0, 14.0]),
        "copied bottom corners need repair too"
    );
    assert!(
        !pins
            .iter()
            .chain(&smear)
            .any(|p| { intersect_rects(*p, [90.0, 80.0, 30.0, 30.0]).is_some() }),
        "the copied center must stay reusable"
    );
}

#[test]
fn out_of_flow_page_chrome_on_blit_viewport_is_a_pin() {
    let mut btn = Element::new();
    btn.style.opacity = 1.0;
    btn.style.bg_color = [1.0, 1.0, 1.0, 1.0];
    btn.layout_bounds = [120.0, 340.0, 80.0, 32.0];
    let mut dock = Element::new();
    dock.style.position = taffy::style::Position::Absolute;
    dock.layout_bounds = [0.0, 340.0, 320.0, 32.0];
    dock.children.push(btn);
    let mut root = Element::new();
    root.children.push(dock);
    let vp = [0.0, 0.0, 320.0, 384.0];
    let pins = out_of_flow_scroll_blit_pins(&root, &[], &[vp]);
    assert_eq!(
        pins,
        vec![[120.0, 340.0, 80.0, 32.0]],
        "visible out-of-flow ink sharing the blit viewport must pin every scroll frame"
    );

    let mut hidden = Element::new();
    hidden.style.opacity = 0.0;
    hidden.style.bg_color = [1.0, 1.0, 1.0, 1.0];
    hidden.layout_bounds = [120.0, 340.0, 80.0, 32.0];
    let mut idle_dock = Element::new();
    idle_dock.style.position = taffy::style::Position::Absolute;
    idle_dock.children.push(hidden);
    let mut idle = Element::new();
    idle.children.push(idle_dock);
    assert!(
        out_of_flow_scroll_blit_pins(&idle, &[], &[vp]).is_empty(),
        "opacity≈0 out-of-flow chrome must not pin"
    );
}

#[test]
fn out_of_flow_dock_sibling_of_contained_viewport_is_a_pin() {
    // MessageScroller sets contains_scroll_viewport on the shell. The Latest
    // dock is a sibling of the viewport, not scrolling content. Using that
    // ancestor flag as in_scroll copied "Latest" into the thread.
    let mut btn = Element::new();
    btn.style.opacity = 1.0;
    btn.style.bg_color = [1.0, 1.0, 1.0, 1.0];
    btn.text = Some("Latest".into());
    btn.layout_bounds = [120.0, 340.0, 80.0, 32.0];
    let mut dock = Element::new();
    dock.style.position = taffy::style::Position::Absolute;
    dock.layout_bounds = [0.0, 340.0, 320.0, 32.0];
    dock.children.push(btn);
    let mut viewport = Element::new();
    viewport.focus_id = Some(format!(
        "{}ms-inner:viewport",
        crate::core::scroll_viewport::SCROLL_VIEWPORT_FOCUS_PREFIX
    ));
    viewport.layout_bounds = [0.0, 0.0, 320.0, 384.0];
    let mut shell = Element::new();
    shell.contains_scroll_viewport = true;
    shell.layout_bounds = [0.0, 0.0, 320.0, 384.0];
    shell.children.push(viewport);
    shell.children.push(dock);
    let mut page = Element::new();
    page.contains_scroll_viewport = true;
    page.focus_id = Some(format!(
        "{}page-scroll",
        crate::core::scroll_viewport::SCROLL_VIEWPORT_FOCUS_PREFIX
    ));
    page.layout_bounds = [0.0, 0.0, 400.0, 800.0];
    page.children.push(shell);
    let inner_vp = [0.0, 0.0, 320.0, 384.0];
    let inner_id = String::from("ms-inner:viewport");
    let pins = out_of_flow_scroll_blit_pins(&page, &[inner_id], &[inner_vp]);
    assert!(
        pins.iter().any(|p| {
            p[0] <= 121.0 && p[1] <= 341.0 && p[0] + p[2] >= 199.0 && p[1] + p[3] >= 371.0
        }),
        "Latest dock sibling of the blitted viewport must pin, got {pins:?}"
    );

    let outer_vp = [0.0, 0.0, 400.0, 800.0];
    let outer_id = String::from("page-scroll");
    let outer_pins = out_of_flow_scroll_blit_pins(&page, &[outer_id], &[outer_vp]);
    assert!(
        outer_pins.is_empty(),
        "Latest rides the outer page blit with the card, got {outer_pins:?}"
    );
}

#[test]
fn out_of_flow_smear_after_strip_blit_is_a_pin() {
    let live = [120.0, 340.0, 80.0, 32.0];
    let vp = [0.0, 0.0, 320.0, 384.0];
    let smeared = smear_out_of_flow_after_strip_blit(&[live], &[vp], &[[0.0, 40.0]]);
    assert_eq!(
        smeared,
        vec![[120.0, 300.0, 80.0, 32.0]],
        "blit copies oof pixels by −delta; that occupancy must dest-reset, got {smeared:?}"
    );
    assert!(
        smear_out_of_flow_after_strip_blit(&[live], &[vp], &[[0.0, 0.0]]).is_empty(),
        "zero delta must not emit a smear pin"
    );
}

#[test]
fn focused_input_caret_on_blit_viewport_is_a_pin() {
    let mut leaf = Element::new();
    leaf.layout_bounds = [40.0, 60.0, 120.0, 24.0];
    leaf.text_style.input_caret = Some(crate::style::InputCaretDraw::default());
    let mut root = Element::new();
    root.children.push(leaf);
    let vp = [0.0, 0.0, 400.0, 320.0];
    let pins = focused_input_caret_scroll_blit_pins(&root, &[vp]);
    assert_eq!(pins.len(), 1, "focused caret leaf must pin, got {pins:?}");
    assert!(
        pins[0][0] < 40.0 && pins[0][1] < 60.0,
        "pin must pad the caret leaf AABB; got {:?}",
        pins[0]
    );

    let mut idle = Element::new();
    idle.layout_bounds = [40.0, 60.0, 120.0, 24.0];
    assert!(
        focused_input_caret_scroll_blit_pins(&idle, &[vp]).is_empty(),
        "leaf without input_caret must not pin"
    );
}
#[test]
fn stamped_scroll_pins_match_unstamped_tree_after_content_changes() {
    let mut sticky = Element::new();
    sticky.style.position_sticky = true;
    sticky.layout_bounds = [10.0, 20.0, 100.0, 30.0];
    let mut caret = Element::new();
    caret.layout_bounds = [40.0, 60.0, 120.0, 24.0];
    caret.text_style.input_caret = Some(crate::style::InputCaretDraw::default());
    let mut root = Element::new().child(Element::new().child(sticky).child(caret));
    root.children.push(Element::new().child(Element::new()));
    let viewport = [[0.0, 0.0, 400.0, 320.0]];
    let expected_sticky = sticky_layout_bounds(&root);
    let expected_caret = focused_input_caret_scroll_blit_pins(&root, &viewport);
    crate::layout::collect_cache::stamp_subtree_paint_hashes(&mut root, false);
    assert_eq!(sticky_layout_bounds(&root), expected_sticky);
    assert_eq!(
        focused_input_caret_scroll_blit_pins(&root, &viewport),
        expected_caret
    );
    root.children[0].children[0].style.position_sticky = false;
    root.children[0].children[1].text_style.input_caret = None;
    crate::layout::collect_cache::stamp_subtree_paint_hashes(&mut root, false);
    assert!(sticky_layout_bounds(&root).is_empty());
    assert!(focused_input_caret_scroll_blit_pins(&root, &viewport).is_empty());
}
