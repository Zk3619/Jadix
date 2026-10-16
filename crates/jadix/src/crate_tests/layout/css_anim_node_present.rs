//! CSS `animate-spin` must register a node-present dirty every tick.
//!
//! Gallery: typing in Kbd Search switches the frame to PartialLoad (Input is in
//! `damage_nodes`). Spin ticks only set `StyleChanged` + `request_redraw`, so the
//! Marker loader is not dest-reset/redrawn — retain keeps an old angle, then the
//! next FullClear snaps. That is the jump; sidebar ghosts are the same present
//! mode (Load without redrawing neighbors).

use crate::app::damage::{
    clip_culled_scroll_blit_pins, expand_paint_dirties_for_clip_culled_slots,
    expand_paint_dirties_for_translated_slots, intersect_rects, paint_dirty_has_keyed_slot,
    paint_dirty_matches_present_slot, paint_dirty_nested_in_larger_node,
    paint_dirty_skips_node_dest_reset, paint_element_key, take_frame_paint_nodes, PaintDirtyKind,
};
#[cfg(feature = "bundled-lucide")]
use crate::app::draw_cull::skip_covered_nested_node_present;
use crate::app::draw_cull::{
    escalate_dirties_for_node_present, last_present_prev_for_dirty, last_present_visual_slots,
    node_paint_visual_bounds, node_present_clipped_capture_text,
    node_present_clipped_capture_text_wipes, node_present_dest_reset_full,
    node_present_dest_reset_scaled, node_present_glyph_clip_logical, node_present_scissor_logical,
    node_present_scissor_pad, slot_visual_exceeds_rest,
};
use crate::app::frame_stats::reset_collect_skip_counters;
use crate::components::bubble::{bubble, bubble_content};
use crate::components::marker::marker;
use crate::components::message::message;
use crate::core::reactivity::use_signal;
use crate::core::scroll_viewport::{scroll_offset_y_signal_key, SCROLL_VIEWPORT_FOCUS_PREFIX};
use crate::core::tree::{visual_paint_aabb, Element, LayoutStatus, LayoutTree};
use crate::primitives::r#box::hstack;
use crate::style::{
    apply_class_list, set_css_transition_timeline_ms, set_css_transition_virtual_timeline,
    sync_class_variants_subtree, tick_active_css_transitions_subtree,
};

#[cfg(feature = "bundled-lucide")]
use crate::app::draw_cull::{textured_hits_damage, textured_paint_bounds};
#[cfg(feature = "bundled-lucide")]
use crate::renderer::scale_textured_draws;
#[cfg(feature = "bundled-lucide")]
use crate::style::class_variant_hover_needs_frame;

fn fmt_rect(r: [f32; 4]) -> String {
    format!("[{:.1},{:.1} {:.1}x{:.1}]", r[0], r[1], r[2], r[3])
}

#[test]
fn trigger_buttons_hover_must_match_on_full_and_fast_frames() {
    use crate::components::button::{button, sync_button_hover_paint_subtree};
    use crate::core::reactivity::create_root;

    set_css_transition_virtual_timeline(true);
    set_css_transition_timeline_ms(0.0);
    crate::style::set_theme_mode(crate::style::ThemeMode::Light);
    let make = || {
        Element::new()
            .flex_col()
            .w(240.0)
            .h(120.0)
            .gap(12.0)
            .child(Element::from(
                button("trigger-full-a").size("sm").text("Open RSX Trigger"),
            ))
            .child(Element::from(
                button("trigger-full-b")
                    .variant("outline")
                    .size("sm")
                    .text("Open Builder Trigger"),
            ))
    };
    let (_owner, (mut full, mut fast)) = create_root(|| (make(), make()));
    let mut full_tree = LayoutTree::new();
    let mut fast_tree = LayoutTree::new();
    let _ = full_tree.build_and_layout(&mut full, 240.0, 120.0, [-1.0, -1.0]);
    let _ = fast_tree.build_and_layout(&mut fast, 240.0, 120.0, [-1.0, -1.0]);
    for step in 0..24 {
        set_css_transition_timeline_ms(step as f64 * 8.0);
        let mouse = if step % 3 == 2 {
            [-1.0, -1.0]
        } else {
            let b = full.children[step % 3].layout_bounds;
            [b[0] + b[2] * 0.5, b[1] + b[3] * 0.5]
        };
        let mut impact = LayoutStatus::Clean;
        sync_button_hover_paint_subtree(
            &mut full,
            &mut full_tree,
            &mut impact,
            240.0,
            120.0,
            [0.0, 0.0],
        );
        sync_class_variants_subtree(
            &mut full,
            mouse,
            "",
            false,
            240.0,
            120.0,
            &mut full_tree,
            &mut impact,
        );
        sync_button_hover_paint_subtree(
            &mut fast,
            &mut fast_tree,
            &mut impact,
            240.0,
            120.0,
            [0.0, 0.0],
        );
        tick_active_css_transitions_subtree(
            &mut fast,
            mouse,
            "",
            false,
            240.0,
            120.0,
            [0.0, 0.0],
            &mut fast_tree,
            &mut impact,
        );
        for i in 0..2 {
            assert_eq!(
                full.children[i].style.bg_color, fast.children[i].style.bg_color,
                "trigger {i}, pointer step {step}: frame selection must not change hover paint"
            );
        }
    }
    set_css_transition_virtual_timeline(false);
}

#[test]
fn css_spin_tick_must_push_node_present_dirty() {
    let _ = take_frame_paint_nodes();

    let mut root = Element::new().w(16.0).h(16.0);
    apply_class_list(&mut root, "animate-spin");
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 200.0, 200.0, [-1.0, -1.0]);
    let _ = take_frame_paint_nodes();

    let mut impact = LayoutStatus::Clean;
    sync_class_variants_subtree(
        &mut root,
        [0.0, 0.0],
        "",
        false,
        200.0,
        200.0,
        &mut tree,
        &mut impact,
    );
    let dirty = take_frame_paint_nodes();
    assert!(
        matches!(impact, LayoutStatus::StyleChanged),
        "spin must stay paint-only, got {impact:?}"
    );
    assert!(
        !dirty.is_empty(),
        "animate-spin StyleChanged must push a node-present dirty so a sibling Input PartialLoad cannot skip the icon"
    );
    let b = root.layout_bounds;
    assert!(
        dirty
            .iter()
            .any(|d| { d.kind == PaintDirtyKind::Node && intersect_rects(d.bounds, b).is_some() }),
        "spin dirty must cover the icon box {}\n dirty={:?}",
        fmt_rect(b),
        dirty
            .iter()
            .map(|d| format!("{:?} {}", d.kind, fmt_rect(d.bounds)))
            .collect::<Vec<_>>()
    );
}

#[test]
fn input_and_spin_dirties_must_stay_disjoint() {
    let _ = take_frame_paint_nodes();

    let mut spin = Element::new().w(16.0).h(16.0);
    apply_class_list(&mut spin, "animate-spin");
    spin.set_attr("data-slot", "spin-icon");

    let mut root = Element::new()
        .w(400.0)
        .h(200.0)
        .child(Element::new().w(80.0).h(32.0).child(spin))
        .child(Element::new().w(200.0).h(32.0));
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 400.0, 200.0, [-1.0, -1.0]);
    let _ = take_frame_paint_nodes();

    let mut impact = LayoutStatus::Clean;
    sync_class_variants_subtree(
        &mut root,
        [0.0, 0.0],
        "",
        false,
        400.0,
        200.0,
        &mut tree,
        &mut impact,
    );
    // Same pad as `sync_one_text_edit_paint` StyleChanged (caret / focus ring).
    let input = &root.children[1];
    let b = input.layout_bounds;
    let pad = 8.0;
    crate::app::damage::push_frame_paint_damage([
        b[0] - pad,
        b[1] - pad,
        b[2] + pad * 2.0,
        b[3] + pad * 2.0,
    ]);
    let dirty = take_frame_paint_nodes();

    let spin_el = find_slot(&root, "spin-icon").expect("spin icon");
    let spin_box = spin_el.layout_bounds;
    assert!(
        dirty.iter().any(|d| {
            d.kind == PaintDirtyKind::Node && intersect_rects(d.bounds, spin_box).is_some()
        }),
        "spin must stay in damage_nodes when Input is also dirty\n dirty={:?}",
        dirty
            .iter()
            .map(|d| format!("{:?} {}", d.kind, fmt_rect(d.bounds)))
            .collect::<Vec<_>>()
    );

    let input_dirty = dirty
        .iter()
        .find(|d| d.kind == PaintDirtyKind::Node && intersect_rects(d.bounds, b).is_some())
        .expect("Input dirty");
    let spin_dirty = dirty
        .iter()
        .find(|d| d.kind == PaintDirtyKind::Node && intersect_rects(d.bounds, spin_box).is_some())
        .expect("spin dirty");
    assert!(
        input_dirty.key != spin_dirty.key
            || intersect_rects(input_dirty.bounds, spin_dirty.bounds).is_none(),
        "Input and spin must remain separate present nodes (got Input {} spin {})",
        fmt_rect(input_dirty.bounds),
        fmt_rect(spin_dirty.bounds)
    );
}

fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
    if el.attrs.get("data-slot").map(String::as_str) == Some(slot) {
        return Some(el);
    }
    el.children.iter().find_map(|c| find_slot(c, slot))
}

fn find_slot_mut<'a>(el: &'a mut Element, slot: &str) -> Option<&'a mut Element> {
    if el.attrs.get("data-slot").map(String::as_str) == Some(slot) {
        return Some(el);
    }
    el.children.iter_mut().find_map(|c| find_slot_mut(c, slot))
}

fn apply_viewport_scroll_y(root: &mut Element, scroll_id: &str, y: f32) {
    use_signal(&scroll_offset_y_signal_key(scroll_id), || 0.0_f32).set(y);
    fn walk(el: &mut Element, scroll_id: &str, y: f32) {
        if el
            .focus_id
            .as_deref()
            .and_then(|f| f.strip_prefix(SCROLL_VIEWPORT_FOCUS_PREFIX))
            == Some(scroll_id)
        {
            el.scroll_offset[1] = y;
        }
        for c in &mut el.children {
            walk(c, scroll_id, y);
        }
    }
    walk(root, scroll_id, y);
}

#[test]
fn idle_sync_must_tick_offscreen_animate_spin() {
    let _ = crate::primitives::interactive::take_hover_paint_dirty();
    crate::style::utility::set_frame_dt(1.0 / 60.0);

    let mut root = Element::new().w(16.0).h(16.0);
    apply_class_list(&mut root, "animate-spin");
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 200.0, 200.0, [-1.0, -1.0]);
    // Paint-space bounds after a scroll collect (subtracted once) plus chrome
    // `scroll_acc` look like this: the icon is on screen, the test says it is not.
    root.layout_bounds = [0.0, 4000.0, 16.0, 16.0];

    let mut impact = LayoutStatus::Clean;
    sync_class_variants_subtree(
        &mut root,
        [0.0, 0.0],
        "",
        false,
        200.0,
        200.0,
        &mut tree,
        &mut impact,
    );
    assert!(
        root.animation_snapshot
            .runs
            .first()
            .is_some_and(|r| r.elapsed_ms > 0.0),
        "idle sync must tick animate-spin even when element_is_offscreen is true — that is why spin dies when scrolling stops, got {:?}",
        root.animation_snapshot.runs
    );
    let _ = crate::primitives::interactive::take_hover_paint_dirty();
    assert!(
        crate::style::any_page_css_needs_frame(),
        "chrome must arm the page-CSS wake so about_to_wait does not WaitUntil scrollbar hide"
    );
}

#[test]
fn idle_sync_must_wake_visible_spin_under_offscreen_live_parent() {
    let _ = crate::primitives::interactive::take_hover_paint_dirty();
    crate::style::utility::set_frame_dt(1.0 / 60.0);

    let mut spin = Element::new().w(16.0).h(16.0);
    apply_class_list(&mut spin, "animate-spin");
    let mut root = Element::new().w(16.0).h(32.0).child(spin);
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 200.0, 200.0, [-1.0, -1.0]);
    crate::layout::collect_cache::stamp_subtree_paint_hashes(&mut root, false);
    assert!(
        root.collect_skip_live,
        "spin descendant must mark the parent live"
    );
    // An overflow-visible wrapper is below the window while its child remains
    // visible. Culling must follow the child's actual ink, not its parent box.
    root.layout_bounds = [0.0, 4000.0, 16.0, 32.0];
    root.children[0].layout_bounds = [0.0, 10.0, 16.0, 16.0];

    let mut impact = LayoutStatus::Clean;
    sync_class_variants_subtree(
        &mut root,
        [0.0, 0.0],
        "",
        false,
        200.0,
        200.0,
        &mut tree,
        &mut impact,
    );
    assert!(
        root.children[0]
            .animation_snapshot
            .runs
            .first()
            .is_some_and(|r| r.elapsed_ms > 0.0),
        "idle chrome must walk a live offscreen parent and tick the child spin, got {:?}",
        root.children[0].animation_snapshot.runs
    );
    assert!(
        crate::style::any_page_css_needs_frame(),
        "live parent must arm page-CSS wake; otherwise maximize/stop-scroll WaitUntil freezes every clock"
    );
}

/// Chrome dirty / NodePaint / dest-reset must live in **visual** space (abs − scroll).
/// Document `layout_bounds` as the present scissor is why Marker stacked labels after
/// scroll: Load retain kept the previous offset, glyphon stamped the new one.
#[test]
fn chrome_spin_dirty_must_cover_visual_instance_after_scroll() {
    let _ = take_frame_paint_nodes();
    crate::style::utility::set_frame_dt(1.0 / 60.0);

    let mut spin = Element::new().w(16.0).h(16.0).bg([0.1, 0.1, 0.1, 1.0]);
    apply_class_list(&mut spin, "animate-spin");
    spin.set_attr("data-slot", "spin-icon");

    let mut root = Element::new()
        .flex_col()
        .w(200.0)
        .h(160.0)
        .bg([0.96, 0.96, 0.96, 1.0])
        .overflow_y("hidden")
        .overflow_x("hidden")
        .child(Element::new().w(200.0).h(100.0).flex_shrink(0.0))
        .child(spin);
    root.focus_id = Some(format!("{SCROLL_VIEWPORT_FOCUS_PREFIX}spin-visual-space"));
    root.scroll_offset = [0.0, 80.0];
    use_signal(&scroll_offset_y_signal_key("spin-visual-space"), || 0.0_f32).set_silent(80.0);

    let mut tree = LayoutTree::new();
    let collected = tree.build_and_layout(&mut root, 200.0, 160.0, [-1.0, -1.0]);
    let _ = take_frame_paint_nodes();

    let mut impact = LayoutStatus::Clean;
    tick_active_css_transitions_subtree(
        &mut root,
        [0.0, 0.0],
        "",
        false,
        200.0,
        160.0,
        [0.0, 0.0],
        &mut tree,
        &mut impact,
    );
    let dirty = take_frame_paint_nodes();

    let spin_el = find_slot(&root, "spin-icon").expect("spin icon");
    let layout = spin_el.layout_bounds;
    let visual = visual_paint_aabb(spin_el, [0.0, 80.0]);
    assert!(
        (layout[1] - visual[1]).abs() > 40.0,
        "fixture must separate document Y {} from visual Y {}",
        layout[1],
        visual[1]
    );

    let spin_dirty = dirty
        .iter()
        .find(|d| d.kind == PaintDirtyKind::Node && intersect_rects(d.bounds, visual).is_some())
        .unwrap_or_else(|| {
            panic!(
                "scroll-frame chrome dirty must cover the painted spin {}, not the document box {}\n dirty={:?}",
                fmt_rect(visual),
                fmt_rect(layout),
                dirty
                    .iter()
                    .map(|d| format!("{:?} {}", d.kind, fmt_rect(d.bounds)))
                    .collect::<Vec<_>>()
            )
        });
    assert!(
        intersect_rects(spin_dirty.bounds, layout).is_none()
            || (spin_dirty.bounds[1] - visual[1]).abs() < (spin_dirty.bounds[1] - layout[1]).abs(),
        "dirty scissor is still document space {}\n visual={} layout={}",
        fmt_rect(spin_dirty.bounds),
        fmt_rect(visual),
        fmt_rect(layout)
    );

    let key = u64::from(spin_el.layout_node.expect("spin taffy node"));
    let slot = collected
        .node_paints
        .iter()
        .find(|p| p.key == key)
        .expect("spin NodePaint");
    assert!(
        (slot.bounds[1] - visual[1]).abs() < 2.0,
        "NodePaint.bounds must be visual {}, got {} (layout was {})",
        fmt_rect(visual),
        fmt_rect(slot.bounds),
        fmt_rect(layout)
    );

    let spin_inst = collected
        .instances
        .get(slot.instance.0)
        .or_else(|| {
            collected.instances.iter().find(|i| {
                (i.rect_size[0] - 16.0).abs() < 1.0 && (i.rect_size[1] - 16.0).abs() < 1.0
            })
        })
        .expect("spin instance");
    let inst_box = [
        spin_inst.rect_pos[0],
        spin_inst.rect_pos[1],
        spin_inst.rect_size[0],
        spin_inst.rect_size[1],
    ];
    assert!(
        intersect_rects(spin_dirty.bounds, inst_box).is_some(),
        "chrome dirty {} must cover collect instance {}\n layout={} visual={}",
        fmt_rect(spin_dirty.bounds),
        fmt_rect(inst_box),
        fmt_rect(layout),
        fmt_rect(visual)
    );

    let (reset, _) = crate::app::draw_cull::node_present_dest_reset(
        spin_dirty.key,
        spin_dirty.bounds,
        &collected.node_paints,
        &collected.instances,
    )
    .expect("dest-reset behind the card");
    assert!(
        intersect_rects(reset, inst_box).is_some(),
        "dest-reset {} must overlap the visual instance {} (scissor/dirty was {})",
        fmt_rect(reset),
        fmt_rect(inst_box),
        fmt_rect(spin_dirty.bounds)
    );

    let spin_dirties_moved = crate::app::damage::node_present_dirties_moved(
        &[spin_dirty.clone()],
        &[(key, slot.bounds)],
        &[(
            key,
            [
                slot.bounds[0],
                slot.bounds[1] + 80.0,
                slot.bounds[2],
                slot.bounds[3],
            ],
        )],
    );
    assert!(
        spin_dirties_moved,
        "visual Y shift of a named spin cannot stay paint-in-place"
    );
    assert!(
        !crate::app::damage::node_present_moved_forces_full_clear(spin_dirties_moved, true),
        "scroll-layer frames pin the spin; they must not mark_full_redraw"
    );
}

/// Rotated overflow children disable local `clip_pos` (`InstanceInput::with_clip`).
/// Idle PartialLoad must bind the matching `clip_polys[i]` (0-based range copy);
/// `bind_sdf(false)` left the spinner unclipped vs FullClear scroll frames.
#[test]
fn idle_spin_in_overflow_must_emit_world_clip_poly() {
    let _ = take_frame_paint_nodes();
    crate::style::utility::set_frame_dt(1.0 / 60.0);

    let mut spin = Element::new().w(16.0).h(16.0).bg([0.1, 0.1, 0.1, 1.0]);
    apply_class_list(&mut spin, "animate-spin");
    spin.set_attr("data-slot", "spin-icon");

    let mut root = Element::new()
        .w(24.0)
        .h(24.0)
        .overflow_y("hidden")
        .overflow_x("hidden")
        .bg([0.96, 0.96, 0.96, 1.0])
        .child(spin);

    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 24.0, 24.0, [-1.0, -1.0]);
    let _ = take_frame_paint_nodes();

    let mut impact = LayoutStatus::Clean;
    tick_active_css_transitions_subtree(
        &mut root,
        [0.0, 0.0],
        "",
        false,
        24.0,
        24.0,
        [0.0, 0.0],
        &mut tree,
        &mut impact,
    );
    let collected = tree.build_and_layout_fast(&mut root, 24.0, 24.0, [-1.0, -1.0], false, false);

    let spin_inst = collected
        .instances
        .iter()
        .enumerate()
        .find(|(_, i)| (i.rect_size[0] - 16.0).abs() < 1.0 && (i.rect_size[1] - 16.0).abs() < 1.0)
        .expect("spin SDF instance");
    let (idx, inst) = spin_inst;
    assert!(
        !crate::style::mat2d_is_axis_aligned(&inst.transform),
        "spin tick must rotate the instance so overflow uses clip_poly, not local clip_pos"
    );
    assert!(
        inst.clip_size[0] < 0.0,
        "rotated overflow must not keep a local AABB clip (would chop bake / leak corners)"
    );
    let poly = collected
        .clip_polys
        .get(idx)
        .copied()
        .unwrap_or(crate::style::GpuClipPoly::NONE);
    assert!(
        poly.is_active(),
        "idle spin must stamp a world clip_poly at the instance index; node-present copies this range to 0"
    );
    let ranged = if idx < collected.clip_polys.len() {
        collected.clip_polys[idx..idx + 1].to_vec()
    } else {
        Vec::new()
    };
    assert!(
        ranged.first().is_some_and(|p| p.is_active()),
        "0-based range copy must keep the spin clip_poly at index 0 for GLES instance_index"
    );
}

/// Gallery Marker: MarkerIcon is a `size-4` wrap around Icon `size-4` with
/// `overflow: visible`. Parent collect ranges include the child's lucide/SDF.
/// If last-present of the wrap unions the rotating AABB, idle
/// `expand_paint_dirties_for_translated_slots` dirties the wrap every tick and
/// PartialLoad src-overs the spinner twice (stacked loader while not scrolling).
#[test]
fn idle_marker_icon_spin_must_not_dirty_same_box_wrap() {
    let _ = take_frame_paint_nodes();
    crate::style::utility::set_frame_dt(1.0 / 30.0);

    let mut icon = Element::new().w(16.0).h(16.0).bg([0.1, 0.1, 0.1, 1.0]);
    apply_class_list(&mut icon, "animate-spin");
    icon.set_attr("data-slot", "spin-icon");

    let mut wrap = Element::new().w(16.0).h(16.0).child(icon);
    wrap.set_attr("data-slot", "spin-wrap");

    let mut root = Element::new()
        .w(40.0)
        .h(40.0)
        .bg([0.96, 0.96, 0.96, 1.0])
        .child(wrap);
    let mut tree = LayoutTree::new();
    let first = tree.build_and_layout(&mut root, 40.0, 40.0, [-1.0, -1.0]);
    let _ = take_frame_paint_nodes();
    let prev_slots = crate::app::draw_cull::last_present_visual_slots(
        &first.node_paints,
        &first.instances,
        &[],
        &first.text_nodes,
        &first.input_chrome_instances,
    );

    // 45° of a 1s linear spin — 90° keeps a square AABB and hides wrap union.
    crate::style::utility::set_frame_dt(0.125);
    let mut impact = LayoutStatus::Clean;
    tick_active_css_transitions_subtree(
        &mut root,
        [0.0, 0.0],
        "",
        false,
        40.0,
        40.0,
        [0.0, 0.0],
        &mut tree,
        &mut impact,
    );
    let mut dirty = take_frame_paint_nodes();
    let later = tree.build_and_layout_fast(&mut root, 40.0, 40.0, [-1.0, -1.0], false, false);
    let now_slots = crate::app::draw_cull::last_present_visual_slots(
        &later.node_paints,
        &later.instances,
        &[],
        &later.text_nodes,
        &later.input_chrome_instances,
    );
    let wrap_el = find_slot(&root, "spin-wrap").expect("wrap");
    let icon_el = find_slot(&root, "spin-icon").expect("icon");
    let wrap_key = paint_element_key(wrap_el);
    let icon_key = paint_element_key(icon_el);
    let wrap_now = now_slots
        .iter()
        .find(|(k, _)| *k == wrap_key)
        .map(|(_, b)| *b)
        .expect("wrap last-present");
    let icon_now = now_slots
        .iter()
        .find(|(k, _)| *k == icon_key)
        .map(|(_, b)| *b)
        .expect("icon last-present");
    assert!(
        icon_now[2] > 20.0 || icon_now[3] > 20.0,
        "45° spin must grow the icon visual slot (circumcircle), got {}",
        fmt_rect(icon_now)
    );
    assert!(
        wrap_now[2] <= 18.0 && wrap_now[3] <= 18.0,
        "wrap last-present must stay the unrotated size-4 box, not the child's circumcircle\n wrap={} icon={}",
        fmt_rect(wrap_now),
        fmt_rect(icon_now)
    );

    expand_paint_dirties_for_translated_slots(&mut dirty, &now_slots, &prev_slots);
    assert!(
        dirty
            .iter()
            .any(|d| d.kind == PaintDirtyKind::Node && d.key == icon_key),
        "idle spin must still dirty the icon"
    );
    assert!(
        !dirty
            .iter()
            .any(|d| d.kind == PaintDirtyKind::Node && d.key == wrap_key),
        "idle spin must not dirty the MarkerIcon wrap — that src-overs the spinner twice\n dirty={:?}",
        dirty
            .iter()
            .map(|d| format!("key={} {}", d.key, fmt_rect(d.bounds)))
            .collect::<Vec<_>>()
    );
}

/// Gallery `<Marker>` is a row: MarkerIcon wrap + MarkerContent (`shimmer` /
/// "Thinking..."). The row's collect range is a **proper subset parent** of the
/// spinner — not equal-span like the wrap. At 45° the icon visual AABB sticks
/// out of the row's unrotated box, so `aabb_contains(row, icon)` fails and
/// last-present of the row unions the circumcircle. Idle
/// `expand_paint_dirties_for_translated_slots` then dirties the row;
/// `paint_dirty_nested_in_larger_node` skips the icon dest-reset → 残影, and
/// PartialLoad src-overs the spinner twice → 卡顿.
#[test]
fn idle_marker_row_spin_must_not_dirty_content_row() {
    let _ = take_frame_paint_nodes();
    crate::style::utility::set_frame_dt(1.0 / 30.0);

    let mut icon = Element::new().w(16.0).h(16.0).bg([0.1, 0.1, 0.1, 1.0]);
    apply_class_list(&mut icon, "animate-spin");
    icon.set_attr("data-slot", "spin-icon");

    let mut wrap = Element::new().w(16.0).h(16.0).child(icon);
    wrap.set_attr("data-slot", "spin-wrap");

    let mut content = Element::new().w(80.0).h(16.0).bg([0.85, 0.85, 0.85, 1.0]);
    content.set_attr("data-slot", "shimmer");

    let mut row: Element = hstack()
        .items("center")
        .h(16.0)
        .gap(8.0)
        .child(wrap)
        .child(content)
        .into();
    row.set_attr("data-slot", "marker-row");

    let mut root = Element::new()
        .w(200.0)
        .h(48.0)
        .bg([0.96, 0.96, 0.96, 1.0])
        .child(row);
    let mut tree = LayoutTree::new();
    let first = tree.build_and_layout(&mut root, 200.0, 48.0, [-1.0, -1.0]);
    let _ = take_frame_paint_nodes();
    let prev_slots = crate::app::draw_cull::last_present_visual_slots(
        &first.node_paints,
        &first.instances,
        &[],
        &first.text_nodes,
        &first.input_chrome_instances,
    );

    crate::style::utility::set_frame_dt(0.125);
    let mut impact = LayoutStatus::Clean;
    tick_active_css_transitions_subtree(
        &mut root,
        [0.0, 0.0],
        "",
        false,
        200.0,
        48.0,
        [0.0, 0.0],
        &mut tree,
        &mut impact,
    );
    let mut dirty = take_frame_paint_nodes();
    let later = tree.build_and_layout_fast(&mut root, 200.0, 48.0, [-1.0, -1.0], false, false);
    let now_slots = crate::app::draw_cull::last_present_visual_slots(
        &later.node_paints,
        &later.instances,
        &[],
        &later.text_nodes,
        &later.input_chrome_instances,
    );
    let row_el = find_slot(&root, "marker-row").expect("row");
    let icon_el = find_slot(&root, "spin-icon").expect("icon");
    let row_key = paint_element_key(row_el);
    let icon_key = paint_element_key(icon_el);
    let row_prev = prev_slots
        .iter()
        .find(|(k, _)| *k == row_key)
        .map(|(_, b)| *b)
        .expect("row last-present prev");
    let row_now = now_slots
        .iter()
        .find(|(k, _)| *k == row_key)
        .map(|(_, b)| *b)
        .expect("row last-present");
    let icon_now = now_slots
        .iter()
        .find(|(k, _)| *k == icon_key)
        .map(|(_, b)| *b)
        .expect("icon last-present");
    assert!(
        icon_now[2] > 20.0 || icon_now[3] > 20.0,
        "45° spin must grow the icon visual slot, got {}",
        fmt_rect(icon_now)
    );
    assert!(
        row_now[3] <= 18.0,
        "Marker row last-present must stay the unrotated 16px row, not the child's circumcircle\n row={} icon={}",
        fmt_rect(row_now),
        fmt_rect(icon_now)
    );
    assert!(
        !crate::app::damage::node_present_slot_moved(row_prev, row_now),
        "idle spin must not look like the Marker row translated\n prev={} now={} icon={}",
        fmt_rect(row_prev),
        fmt_rect(row_now),
        fmt_rect(icon_now)
    );

    expand_paint_dirties_for_translated_slots(&mut dirty, &now_slots, &prev_slots);
    let icon_dirty = dirty
        .iter()
        .find(|d| d.kind == PaintDirtyKind::Node && d.key == icon_key)
        .cloned()
        .expect("idle spin must still dirty the icon");
    assert!(
        !dirty
            .iter()
            .any(|d| d.kind == PaintDirtyKind::Node && d.key == row_key),
        "idle spin must not dirty the Marker row — that skips icon dest-reset and src-overs twice\n dirty={:?}",
        dirty
            .iter()
            .map(|d| format!("key={} {}", d.key, fmt_rect(d.bounds)))
            .collect::<Vec<_>>()
    );
    assert!(
        !paint_dirty_nested_in_larger_node(&icon_dirty, &dirty),
        "icon dest-reset must not be nested-skipped under the Marker row"
    );
}

#[test]
fn marker_hover_must_not_nested_skip_spin_dest_reset() {
    // Moving the pointer over MarkerContent (shimmer) used to dest-reset the row
    // and nested-skip the loader-circle — retain kept one angle (卡顿 / 停止).
    let _ = take_frame_paint_nodes();
    crate::style::utility::set_frame_dt(1.0 / 30.0);

    let mut icon = Element::new().w(16.0).h(16.0).bg([0.1, 0.1, 0.1, 1.0]);
    apply_class_list(&mut icon, "animate-spin");
    icon.set_attr("data-slot", "spin-icon");

    let mut wrap = Element::new()
        .w(16.0)
        .h(16.0)
        .pointer_events_none()
        .child(icon);
    wrap.set_attr("data-slot", "spin-wrap");

    let mut content = Element::new().w(80.0).h(16.0).bg([0.85, 0.85, 0.85, 1.0]);
    apply_class_list(&mut content, "shimmer");
    content.set_attr("data-slot", "shimmer");

    let mut row: Element = hstack()
        .items("center")
        .h(16.0)
        .gap(8.0)
        .child(wrap)
        .child(content)
        .into();
    row.set_attr("data-slot", "marker-row");

    let mut root = Element::new()
        .w(200.0)
        .h(48.0)
        .bg([0.96, 0.96, 0.96, 1.0])
        .child(row);
    let mut tree = LayoutTree::new();
    let first = tree.build_and_layout(&mut root, 200.0, 48.0, [-1.0, -1.0]);
    let _ = take_frame_paint_nodes();
    let prev_slots = crate::app::draw_cull::last_present_visual_slots(
        &first.node_paints,
        &first.instances,
        &[],
        &first.text_nodes,
        &first.input_chrome_instances,
    );

    let content_el = find_slot(&root, "shimmer").expect("content");
    let mouse = [
        content_el.layout_bounds[0] + content_el.layout_bounds[2] * 0.5,
        content_el.layout_bounds[1] + content_el.layout_bounds[3] * 0.5,
    ];

    crate::style::utility::set_frame_dt(0.125);
    let mut impact = LayoutStatus::Clean;
    sync_class_variants_subtree(
        &mut root,
        mouse,
        "",
        false,
        200.0,
        48.0,
        &mut tree,
        &mut impact,
    );
    let mut dirty = take_frame_paint_nodes();
    let later = tree.build_and_layout_fast(&mut root, 200.0, 48.0, [-1.0, -1.0], false, false);
    let now_slots = crate::app::draw_cull::last_present_visual_slots(
        &later.node_paints,
        &later.instances,
        &[],
        &later.text_nodes,
        &later.input_chrome_instances,
    );
    let row_el = find_slot(&root, "marker-row").expect("row");
    let icon_el = find_slot(&root, "spin-icon").expect("icon");
    let row_key = paint_element_key(row_el);
    let icon_key = paint_element_key(icon_el);
    expand_paint_dirties_for_translated_slots(&mut dirty, &now_slots, &prev_slots);
    let icon_dirty = dirty
        .iter()
        .find(|d| d.kind == PaintDirtyKind::Node && d.key == icon_key)
        .cloned()
        .expect("hover spin must still dirty the icon");
    assert!(
        !dirty
            .iter()
            .any(|d| d.kind == PaintDirtyKind::Node && d.key == row_key),
        "hovering MarkerContent must not dest-reset the row\n dirty={:?}",
        dirty
            .iter()
            .map(|d| format!("key={} {}", d.key, fmt_rect(d.bounds)))
            .collect::<Vec<_>>()
    );
    assert!(
        !paint_dirty_nested_in_larger_node(&icon_dirty, &dirty),
        "icon dest-reset must run while the pointer is on the Marker caption"
    );
    assert!(
        icon_el
            .animation_snapshot
            .runs
            .first()
            .is_some_and(|r| r.elapsed_ms > 0.0),
        "spin elapsed must keep advancing under hover, got {:?}",
        icon_el.animation_snapshot.runs
    );
}

#[cfg(feature = "bundled-lucide")]
fn find_spin_el(el: &Element) -> Option<&Element> {
    if el.attrs.get("jadix_animate").map(String::as_str) == Some("spin")
        || el.style.animations.iter().any(|a| a.name == "spin")
    {
        return Some(el);
    }
    el.children.iter().find_map(find_spin_el)
}

#[cfg(feature = "bundled-lucide")]
fn spin_elapsed_ms(el: &Element) -> f32 {
    el.animation_snapshot
        .runs
        .first()
        .map(|r| r.elapsed_ms)
        .unwrap_or(0.0)
}

#[cfg(feature = "bundled-lucide")]
fn dump_paints(paints: &[crate::layout::layout_tree::NodePaint], root: &Element) -> String {
    paints
        .iter()
        .map(|p| {
            let slot = find_slot_by_key(root, p.key)
                .and_then(|e| e.attrs.get("data-slot"))
                .map(String::as_str)
                .unwrap_or("?");
            format!(
                "key={} slot={slot} bounds={} inst={:?} tex={:?} text={:?}",
                p.key,
                fmt_rect(p.bounds),
                p.instance,
                p.textured,
                p.text
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn dump_dirties(dirty: &[crate::app::damage::PaintDirty], root: &Element) -> String {
    dirty
        .iter()
        .map(|d| {
            let slot = find_slot_by_key(root, d.key)
                .and_then(|e| e.attrs.get("data-slot"))
                .map(String::as_str)
                .unwrap_or("?");
            format!(
                "key={} slot={slot} nested={} {}",
                d.key,
                paint_dirty_nested_in_larger_node(d, dirty),
                fmt_rect(d.bounds)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn find_slot_by_key(el: &Element, key: u64) -> Option<&Element> {
    if paint_element_key(el) == key {
        return Some(el);
    }
    el.children.iter().find_map(|c| find_slot_by_key(c, key))
}

/// Fast pointer motion *anywhere* on the gallery (sidebar, `hover:scale` tiles)
/// extra-paints. Marker spin must keep sampling the CSS timeline and dest-reset
/// even when the hovered tile is also a Node dirty — the pointer never has to
/// enter the Marker.
#[test]
fn page_pointer_storm_must_keep_distant_marker_spinning() {
    let _ = take_frame_paint_nodes();
    set_css_transition_virtual_timeline(true);
    set_css_transition_timeline_ms(0.0);
    struct ResetVirtual;
    impl Drop for ResetVirtual {
        fn drop(&mut self) {
            set_css_transition_virtual_timeline(false);
        }
    }
    let _reset_virtual = ResetVirtual;
    crate::style::utility::set_frame_dt(1.0 / 60.0);

    let mut icon = Element::new().w(16.0).h(16.0).bg([0.1, 0.1, 0.1, 1.0]);
    apply_class_list(&mut icon, "animate-spin");
    icon.set_attr("data-slot", "spin-icon");
    let mut marker_row: Element = hstack().items("center").h(20.0).gap(8.0).child(icon).into();
    marker_row.set_attr("data-slot", "marker-row");

    let mut tile = Element::new().w(64.0).h(64.0).bg([0.2, 0.4, 0.8, 1.0]);
    apply_class_list(&mut tile, "hover:scale-125");
    tile.set_attr("data-slot", "hover-tile");

    let mut root = Element::new()
        .flex_col()
        .w(400.0)
        .h(240.0)
        .bg([0.96, 0.96, 0.96, 1.0])
        .gap(80.0)
        .child(marker_row)
        .child(tile);
    let mut tree = LayoutTree::new();
    let _first = tree.build_and_layout(&mut root, 400.0, 240.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let _ = take_frame_paint_nodes();
    crate::style::utility::set_frame_dt(1.0 / 60.0);
    let mut seed_impact = LayoutStatus::Clean;
    sync_class_variants_subtree(
        &mut root,
        [-1.0, -1.0],
        "",
        false,
        400.0,
        240.0,
        &mut tree,
        &mut seed_impact,
    );
    let _ = take_frame_paint_nodes();
    let seeded = tree.build_and_layout_fast(&mut root, 400.0, 240.0, [-1.0, -1.0], false, false);
    let mut prev_slots = last_present_visual_slots(
        &seeded.node_paints,
        &seeded.instances,
        &[],
        &seeded.text_nodes,
        &seeded.input_chrome_instances,
    );

    let tile_el = find_slot(&root, "hover-tile").expect("tile");
    let spin_el = find_slot(&root, "spin-icon").expect("spin");
    let tile_c = [
        tile_el.layout_bounds[0] + tile_el.layout_bounds[2] * 0.5,
        tile_el.layout_bounds[1] + tile_el.layout_bounds[3] * 0.5,
    ];
    let far = [380.0, 220.0];
    assert!(
        (tile_c[1] - spin_el.layout_bounds[1]).abs() > 40.0,
        "hover tile must sit away from the spinner so this is a page-wide storm, not Marker hover"
    );

    let mut last_rotate = spin_el.style.rotate_deg;
    let mut virtual_ms = 0.0_f64;
    for (i, mouse) in [tile_c, far, tile_c, far]
        .iter()
        .cycle()
        .take(8)
        .enumerate()
    {
        virtual_ms += 16.0;
        set_css_transition_timeline_ms(virtual_ms);
        // Typical CursorMoved extra-paint dt (1–8ms), not dt≈0 — production used
        // this branch and ignored the CSS timeline, so spin crawled/froze.
        crate::style::utility::set_frame_dt(0.002);
        let mut impact = LayoutStatus::Clean;
        sync_class_variants_subtree(
            &mut root,
            *mouse,
            "",
            false,
            400.0,
            240.0,
            &mut tree,
            &mut impact,
        );
        let mut dirty = take_frame_paint_nodes();
        let later = tree.build_and_layout_fast(&mut root, 400.0, 240.0, *mouse, false, false);
        let now_slots = last_present_visual_slots(
            &later.node_paints,
            &later.instances,
            &[],
            &later.text_nodes,
            &later.input_chrome_instances,
        );
        expand_paint_dirties_for_translated_slots(&mut dirty, &now_slots, &prev_slots);

        let spin = find_slot(&root, "spin-icon").expect("spin");
        let spin_key = paint_element_key(spin);
        let tile_key = paint_element_key(find_slot(&root, "hover-tile").expect("tile"));
        let rotate = spin.style.rotate_deg;
        assert!(
            (rotate - last_rotate).abs() > 4.0,
            "i={i} extra hover-frame dt=2ms must still sample +16ms of CSS time, rotate {last_rotate:.1}->{rotate:.1}"
        );
        last_rotate = rotate;

        let icon_dirty = dirty
            .iter()
            .find(|d| d.kind == PaintDirtyKind::Node && d.key == spin_key)
            .cloned()
            .expect("spin must stay a Node dirty while the pointer storms the page");
        let skip = paint_dirty_skips_node_dest_reset(&icon_dirty, &dirty, false)
            && !slot_visual_exceeds_rest(spin_key, &later.node_paints);
        assert!(
            !skip,
            "i={i} hover-tile dirty must not nested-skip spin dest-reset\n{}",
            dump_dirties(&dirty, &root)
        );
        let prev_icon = last_present_prev_for_dirty(PaintDirtyKind::Node, spin_key, &prev_slots);
        let reset = node_present_dest_reset_scaled(
            spin_key,
            icon_dirty.bounds,
            &later.node_paints,
            &later.instances,
            &[],
            1.0,
            prev_icon,
        );
        assert!(
            reset.is_some(),
            "i={i} icon dest-reset must run during a page-wide pointer storm (hover tile key={tile_key})\n{}",
            dump_dirties(&dirty, &root)
        );
        prev_slots = now_slots;
    }
}

/// Gallery 1c-m: real `<Marker>` + lucide loader + shimmer caption.
/// CursorMoved extra paints used a simplified hstack tree in the previous test;
/// the cluster / MarkerIcon wrap / content last-present is what nested-skips.
/// Lucide path/font raster needs `bundled-lucide` (same as gallery).
#[cfg(feature = "bundled-lucide")]
#[test]
fn gallery_marker_pointer_storm_must_keep_spinning() {
    let _ = take_frame_paint_nodes();
    crate::style::utility::set_frame_dt(1.0 / 60.0);

    let mut glyph = crate::utils::build_named_icon_text_node("loader-circle", "\u{ea0e}");
    apply_class_list(&mut glyph, "animate-spin");
    let marker_el: Element = crate::components::marker()
        .role("status")
        .icon(glyph)
        .content_with(|c| c.class("shimmer").text("Thinking..."))
        .into();
    let mut root = Element::new()
        .w(320.0)
        .h(64.0)
        .bg([0.96, 0.96, 0.96, 1.0])
        .child(marker_el);
    let mut tree = LayoutTree::new();
    let first = tree.build_and_layout(&mut root, 320.0, 64.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let _ = take_frame_paint_nodes();
    assert!(
        !first.textured_draws.is_empty(),
        "loader-circle + animate-spin must collect a lucide texture (got 0 textured draws)"
    );
    let mut prev_slots = last_present_visual_slots(
        &first.node_paints,
        &first.instances,
        &scale_textured_draws(&first.textured_draws, 1.0),
        &first.text_nodes,
        &first.input_chrome_instances,
    );

    let content = find_slot(&root, "marker-content").expect("content");
    let icon_slot = find_slot(&root, "marker-icon").expect("icon slot");
    let row = find_slot(&root, "marker").expect("row");
    let probes = [
        [
            icon_slot.layout_bounds[0] + icon_slot.layout_bounds[2] * 0.5,
            icon_slot.layout_bounds[1] + icon_slot.layout_bounds[3] * 0.5,
        ],
        [
            content.layout_bounds[0] + content.layout_bounds[2] * 0.5,
            content.layout_bounds[1] + content.layout_bounds[3] * 0.5,
        ],
        [row.layout_bounds[0] + 4.0, row.layout_bounds[1] - 2.0],
        [
            row.layout_bounds[0] + row.layout_bounds[2] * 0.5,
            row.layout_bounds[1] + row.layout_bounds[3] * 0.5,
        ],
    ];

    let mut last_elapsed = spin_elapsed_ms(find_spin_el(&root).expect("spin"));
    let mut last_rotate = find_spin_el(&root).expect("spin").style.rotate_deg;
    for (i, mouse) in probes.iter().cycle().take(12).enumerate() {
        crate::style::utility::set_frame_dt(if i % 3 == 1 { 0.0 } else { 1.0 / 60.0 });
        let mut impact = LayoutStatus::Clean;
        sync_class_variants_subtree(
            &mut root,
            *mouse,
            "",
            false,
            320.0,
            64.0,
            &mut tree,
            &mut impact,
        );
        let mut dirty = take_frame_paint_nodes();
        let later = tree.build_and_layout_fast(&mut root, 320.0, 64.0, *mouse, false, false);
        let now_slots = last_present_visual_slots(
            &later.node_paints,
            &later.instances,
            &scale_textured_draws(&later.textured_draws, 1.0),
            &later.text_nodes,
            &later.input_chrome_instances,
        );
        expand_paint_dirties_for_translated_slots(&mut dirty, &now_slots, &prev_slots);
        assert!(
            !crate::primitives::interactive::take_hover_paint_dirty(),
            "spin/shimmer ticks must not mark hover-dirty — CursorMoved would storm dt≈0 paints"
        );
        assert!(
            !class_variant_hover_needs_frame(&root, *mouse),
            "Marker has no hover: utilities; pointer motion must not wake extra class-hover frames"
        );

        let spin = find_spin_el(&root).expect("spin");
        assert_eq!(
            spin.pointer_events_mode(),
            crate::core::tree::PointerEvents::None,
            "spin AABB must stay pointer-events:none so rotating hit tests cannot flicker hover"
        );
        let elapsed = spin_elapsed_ms(spin);
        let rotate = spin.style.rotate_deg;
        let spin_key = paint_element_key(spin);
        let row_key = paint_element_key(find_slot(&root, "marker").expect("row"));
        let wrap_key = paint_element_key(find_slot(&root, "marker-icon").expect("wrap"));
        let cluster_key = find_slot(&root, "marker-cluster").map(paint_element_key);
        let cluster_now =
            cluster_key.and_then(|ck| now_slots.iter().find(|(k, _)| *k == ck).map(|(_, b)| *b));
        let icon_dirty = dirty
            .iter()
            .find(|d| d.kind == PaintDirtyKind::Node && d.key == spin_key)
            .cloned();
        let row_now = now_slots
            .iter()
            .find(|(k, _)| *k == row_key)
            .map(|(_, b)| *b);
        let wrap_now = now_slots
            .iter()
            .find(|(k, _)| *k == wrap_key)
            .map(|(_, b)| *b);
        let icon_now = now_slots
            .iter()
            .find(|(k, _)| *k == spin_key)
            .map(|(_, b)| *b);
        let tex = scale_textured_draws(&later.textured_draws, 1.0);
        let dump = format!(
            "i={i} mouse={mouse:?} dt={} elapsed {last_elapsed:.1}->{elapsed:.1} rotate {last_rotate:.1}->{rotate:.1}\n{}\n paints={}\n last-present row={:?} wrap={:?} cluster={:?} icon={:?}",
            if i % 3 == 1 { 0.0 } else { 1.0 / 60.0 },
            dump_dirties(&dirty, &root),
            dump_paints(&later.node_paints, &root),
            row_now.map(fmt_rect),
            wrap_now.map(fmt_rect),
            cluster_now.map(fmt_rect),
            icon_now.map(fmt_rect),
        );
        assert!(
            icon_dirty.is_some(),
            "spin must stay a Node dirty while the pointer walks the Marker\n{dump}"
        );
        let icon_dirty = icon_dirty.unwrap();
        assert!(
            !dirty.iter().any(|d| d.kind == PaintDirtyKind::Node
                && (d.key == row_key
                    || d.key == wrap_key
                    || cluster_key.is_some_and(|k| d.key == k))),
            "row/wrap/cluster dest-reset nested-skips the loader\n{dump}"
        );
        assert!(
            !paint_dirty_nested_in_larger_node(&icon_dirty, &dirty),
            "icon dest-reset nested-skipped under a larger Marker dirty\n{dump}"
        );
        if let Some(cluster_now) = cluster_now {
            assert!(
                cluster_now[3] <= 24.0,
                "Marker cluster last-present must stay the unrotated caption row, not the loader circumcircle\n{dump}"
            );
        }
        if let Some(wrap_now) = wrap_now {
            assert!(
                wrap_now[2] <= 20.0 && wrap_now[3] <= 20.0,
                "MarkerIcon wrap last-present must stay the unrotated size-4 box\n{dump}"
            );
        }
        let prev_icon = last_present_prev_for_dirty(PaintDirtyKind::Node, spin_key, &prev_slots);
        let reset = node_present_dest_reset_scaled(
            spin_key,
            icon_dirty.bounds,
            &later.node_paints,
            &later.instances,
            &tex,
            1.0,
            prev_icon,
        );
        assert!(
            reset.is_some(),
            "icon dest-reset must run (None leaves the last angle in PartialLoad retain)\n{dump}"
        );
        let (wipe, _) = reset.unwrap();
        for d in &tex {
            let tb = textured_paint_bounds(d);
            if tb[2] <= 0.0 || tb[3] <= 0.0 {
                continue;
            }
            let cx = tb[0] + tb[2] * 0.5;
            let cy = tb[1] + tb[3] * 0.5;
            assert!(
                cx >= wipe[0] && cy >= wipe[1] && cx < wipe[0] + wipe[2] && cy < wipe[1] + wipe[3],
                "dest-reset wipe {} must cover lucide texture {}\n{dump}",
                fmt_rect(wipe),
                fmt_rect(tb),
            );
        }
        let scissor = node_present_scissor_logical(
            PaintDirtyKind::Node,
            spin_key,
            icon_dirty.bounds,
            &prev_slots,
            &later.node_paints,
            8.0,
        );
        assert!(
            tex.iter().any(|d| textured_hits_damage(d, scissor)),
            "PartialLoad scissor {} must cover the spinning lucide texture {:?}\n{dump}",
            fmt_rect(scissor),
            tex.iter()
                .map(|d| fmt_rect(textured_paint_bounds(d)))
                .collect::<Vec<_>>(),
        );
        prev_slots = now_slots;
        if i % 3 != 1 {
            assert!(
                elapsed > last_elapsed + 1.0,
                "spin elapsed must advance on vsync-sized dt\n{dump}"
            );
            assert!(
                (rotate - last_rotate).abs() > 0.5,
                "spin rotate must change on vsync-sized dt\n{dump}"
            );
        }
        last_elapsed = elapsed;
        last_rotate = rotate;
    }
}

/// Gallery Marker: idle spin+shimmer must not src-over "Thinking..." twice.
/// Spin circumcircle scissor / wipe that nips the caption, or nested-skipping the
/// content dest-reset, leaves PartialLoad stacking glyph alpha (叠影变粗).
#[cfg(feature = "bundled-lucide")]
#[test]
fn gallery_marker_spin_must_not_thicken_thinking_caption() {
    use crate::app::draw_cull::{node_present_dest_reset_full, node_present_scissor_pad};

    let _ = take_frame_paint_nodes();
    crate::style::utility::set_frame_dt(1.0 / 60.0);

    let mut glyph = crate::utils::build_named_icon_text_node("loader-circle", "\u{ea0e}");
    apply_class_list(&mut glyph, "animate-spin");
    let marker_el: Element = crate::components::marker()
        .role("status")
        .icon(glyph)
        .content_with(|c| c.class("shimmer").text("Thinking..."))
        .into();
    let mut root = Element::new()
        .w(320.0)
        .h(64.0)
        .bg([0.96, 0.96, 0.96, 1.0])
        .child(marker_el);
    let mut tree = LayoutTree::new();
    let first = tree.build_and_layout(&mut root, 320.0, 64.0, [-1.0, -1.0]);
    let _ = take_frame_paint_nodes();
    let prev_slots = last_present_visual_slots(
        &first.node_paints,
        &first.instances,
        &scale_textured_draws(&first.textured_draws, 1.0),
        &first.text_nodes,
        &first.input_chrome_instances,
    );

    crate::style::utility::set_frame_dt(0.08);
    let mut impact = LayoutStatus::Clean;
    for _ in 0..3 {
        tick_active_css_transitions_subtree(
            &mut root,
            [0.0, 0.0],
            "",
            false,
            320.0,
            64.0,
            [0.0, 0.0],
            &mut tree,
            &mut impact,
        );
        sync_class_variants_subtree(
            &mut root,
            [0.0, 0.0],
            "",
            false,
            320.0,
            64.0,
            &mut tree,
            &mut impact,
        );
    }
    let mut dirty = take_frame_paint_nodes();
    let later = tree.build_and_layout_fast(&mut root, 320.0, 64.0, [-1.0, -1.0], false, false);
    let now_slots = last_present_visual_slots(
        &later.node_paints,
        &later.instances,
        &scale_textured_draws(&later.textured_draws, 1.0),
        &later.text_nodes,
        &later.input_chrome_instances,
    );
    expand_paint_dirties_for_translated_slots(&mut dirty, &now_slots, &prev_slots);

    let content = find_slot(&root, "marker-content").expect("content");
    let spin = find_spin_el(&root).expect("spin");
    let content_key = paint_element_key(content);
    let spin_key = paint_element_key(spin);
    let thinking: Vec<(usize, &_)> = later
        .text_nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.text.contains("Thinking"))
        .collect();
    assert!(
        !thinking.is_empty(),
        "need Thinking... glyphon runs, got {:?}",
        later
            .text_nodes
            .iter()
            .map(|n| n.text.as_str())
            .collect::<Vec<_>>()
    );

    let content_dirty = dirty
        .iter()
        .find(|d| d.key == content_key && d.kind == PaintDirtyKind::Node);
    let spin_dirty = dirty
        .iter()
        .find(|d| d.key == spin_key && d.kind == PaintDirtyKind::Node)
        .cloned()
        .expect("spin must stay a Node dirty");

    let dump = format!(
        "dirties={:?}\n content_dirty={:?}\n spin={}\n thinking={:?}",
        dirty
            .iter()
            .map(|d| format!("{:?} k{} {}", d.kind, d.key, fmt_rect(d.bounds)))
            .collect::<Vec<_>>(),
        content_dirty.map(|d| fmt_rect(d.bounds)),
        fmt_rect(spin_dirty.bounds),
        thinking
            .iter()
            .map(|(i, n)| format!(
                "#{i} '{}' @[{:.1},{:.1} {:.1}x{:.1}]",
                n.text, n.x, n.y, n.width, n.height
            ))
            .collect::<Vec<_>>(),
    );

    if let Some(cd) = content_dirty {
        assert!(
            !paint_dirty_nested_in_larger_node(cd, &dirty),
            "shimmer content dest-reset nested-skipped under a larger dirty — Thinking src-overs\n{dump}"
        );
        assert!(
            !paint_dirty_skips_node_dest_reset(cd, &dirty, false),
            "shimmer content dest-reset skipped — Thinking stacks in retain\n{dump}"
        );
        let prev = last_present_prev_for_dirty(PaintDirtyKind::Node, content_key, &prev_slots);
        let reset = node_present_dest_reset_full(
            content_key,
            cd.bounds,
            &later.node_paints,
            &later.instances,
            &scale_textured_draws(&later.textured_draws, 1.0),
            &later.input_chrome_instances,
            1.0,
            prev,
        );
        assert!(
            reset.is_some(),
            "shimmer content must dest-reset (None leaves previous Thinking in PartialLoad)\n{dump}"
        );
    }

    let clipped = node_present_clipped_capture_text(&dirty, &later.text_nodes, &later.node_paints);
    let thinking_clipped: Vec<_> = clipped
        .iter()
        .filter(|n| n.text.contains("Thinking"))
        .collect();
    assert!(
        !thinking_clipped.is_empty(),
        "Thinking must stay in the once-capture set with an owner clip\n{dump}"
    );
    for n in &thinking_clipped {
        let clip = n.clip_bounds.expect("owner clip");
        // Owner pad is on the content dirty — must not swallow the spin diamond.
        assert!(
            clip[0] + clip[2] > 24.0 && clip[0] < 24.0 + 63.0,
            "Thinking owner clip must track the content dirty, not the spin scissor\n\
             clip={}\n{dump}",
            fmt_rect(clip),
        );
    }

    let spin_scissor = node_present_scissor_logical(
        PaintDirtyKind::Node,
        spin_key,
        spin_dirty.bounds,
        &prev_slots,
        &later.node_paints,
        node_present_scissor_pad(PaintDirtyKind::Node),
    );
    let thinking_in_spin_scissor: Vec<_> = thinking
        .iter()
        .filter(|(_, n)| {
            let cx = n.x + n.width * 0.5;
            let cy = n.y + n.height * 0.5;
            cx >= spin_scissor[0]
                && cy >= spin_scissor[1]
                && cx < spin_scissor[0] + spin_scissor[2]
                && cy < spin_scissor[1] + spin_scissor[3]
        })
        .collect();
    assert!(
        thinking_in_spin_scissor.is_empty(),
        "spin node-present scissor nips Thinking caption center — PartialLoad stacks glyphs\n\
         scissor={}\n{dump}",
        fmt_rect(spin_scissor),
    );

    let prev_spin = last_present_prev_for_dirty(PaintDirtyKind::Node, spin_key, &prev_slots);
    if let Some(wipe) = node_present_dest_reset_full(
        spin_key,
        spin_dirty.bounds,
        &later.node_paints,
        &later.instances,
        &scale_textured_draws(&later.textured_draws, 1.0),
        &later.input_chrome_instances,
        1.0,
        prev_spin,
    ) {
        for (_, n) in &thinking {
            let cx = n.x + n.width * 0.5;
            let cy = n.y + n.height * 0.5;
            assert!(
                !(cx >= wipe.rect[0]
                    && cy >= wipe.rect[1]
                    && cx < wipe.rect[0] + wipe.rect[2]
                    && cy < wipe.rect[1] + wipe.rect[3]),
                "spin dest-reset must not punch Thinking caption center\n\
                 wipe={}\n{dump}",
                fmt_rect(wipe.rect),
            );
        }
    }
}

/// Gallery Status (Marker inside Message): lucide spin + shimmer 「Oliver is typing...」.
///
/// Idle ticks used to dest-reset Marker/Message wraps that only inherited the
/// lucide collect range. That CLEAR of the caption every frame is the shimmer
/// flicker — even after the hole was wide enough to recapture the full string.
#[cfg(feature = "bundled-lucide")]
#[test]
fn message_status_shimmer_capture_must_keep_full_oliver_is_typing() {
    use crate::app::draw_cull::node_present_dest_reset_full;

    let _ = take_frame_paint_nodes();
    crate::style::utility::set_frame_dt(1.0 / 60.0);

    let mut glyph = crate::utils::build_named_icon_text_node("loader-circle", "\u{ea0e}");
    apply_class_list(&mut glyph, "animate-spin");
    let marker_el: Element = crate::components::marker()
        .role("status")
        .icon(glyph)
        .content_with(|c| c.class("shimmer").text("Oliver is typing..."))
        .into();
    let mut root = Element::new()
        .flex_col()
        .w("100%")
        .min_w(0.0)
        .max_w(448.0)
        .bg([0.96, 0.96, 0.96, 1.0])
        .child(Element::new().text("Status (Marker inside Message)"))
        .child(message().child(marker_el));
    let mut tree = LayoutTree::new();
    let first = tree.build_and_layout(&mut root, 448.0, 80.0, [-1.0, -1.0]);
    let _ = take_frame_paint_nodes();
    let mut prev_slots = last_present_visual_slots(
        &first.node_paints,
        &first.instances,
        &scale_textured_draws(&first.textured_draws, 1.0),
        &first.text_nodes,
        &first.input_chrome_instances,
    );

    for i in 0..6 {
        crate::style::utility::set_frame_dt(if i % 3 == 1 { 0.0 } else { 1.0 / 60.0 });
        let mut impact = LayoutStatus::Clean;
        tick_active_css_transitions_subtree(
            &mut root,
            [0.0, 0.0],
            "",
            false,
            448.0,
            80.0,
            [0.0, 0.0],
            &mut tree,
            &mut impact,
        );
        sync_class_variants_subtree(
            &mut root,
            [0.0, 0.0],
            "",
            false,
            448.0,
            80.0,
            &mut tree,
            &mut impact,
        );
        let mut dirty = take_frame_paint_nodes();
        let later = tree.build_and_layout_fast(&mut root, 448.0, 80.0, [-1.0, -1.0], false, false);
        let now_slots = last_present_visual_slots(
            &later.node_paints,
            &later.instances,
            &scale_textured_draws(&later.textured_draws, 1.0),
            &later.text_nodes,
            &later.input_chrome_instances,
        );
        expand_paint_dirties_for_translated_slots(&mut dirty, &now_slots, &prev_slots);

        let content = find_slot(&root, "marker-content").expect("content");
        let caption = later
            .text_nodes
            .iter()
            .find(|n| n.text.contains("Oliver is typing"))
            .expect("caption");
        let ink_w = caption
            .prepared
            .as_ref()
            .map(crate::text::intrinsic_width)
            .filter(|w| *w > 1.0)
            .unwrap_or(caption.width);
        let tex = scale_textured_draws(&later.textured_draws, 1.0);
        let present_dirties: Vec<_> = dirty
            .iter()
            .copied()
            .filter(|d| d.kind == PaintDirtyKind::Node)
            .filter(|d| {
                !skip_covered_nested_node_present(
                    d,
                    &dirty,
                    false,
                    &later.node_paints,
                    &later.instances,
                    1.0,
                )
            })
            .collect();
        let wipe_clips: Vec<(u64, [f32; 4])> = present_dirties
            .iter()
            .filter_map(|d| {
                let prev = last_present_prev_for_dirty(d.kind, d.key, &prev_slots);
                let wipe = node_present_dest_reset_full(
                    d.key,
                    d.bounds,
                    &later.node_paints,
                    &later.instances,
                    &tex,
                    &later.input_chrome_instances,
                    1.0,
                    prev,
                )?
                .rect;
                Some((d.key, node_present_glyph_clip_logical(d, Some(wipe), 1.0)))
            })
            .collect();
        let capture = node_present_clipped_capture_text_wipes(
            &present_dirties,
            &later.text_nodes,
            &later.node_paints,
            &wipe_clips,
        );
        let captured = capture.iter().find(|n| n.text.contains("Oliver is typing"));
        let wipe_diag: Vec<String> = present_dirties
            .iter()
            .filter_map(|d| {
                let prev = last_present_prev_for_dirty(d.kind, d.key, &prev_slots);
                let wipe = node_present_dest_reset_full(
                    d.key,
                    d.bounds,
                    &later.node_paints,
                    &later.instances,
                    &tex,
                    &later.input_chrome_instances,
                    1.0,
                    prev,
                )?;
                Some(format!(
                    "k{} wipe={} prev={:?} overflow={:?}",
                    d.key,
                    fmt_rect(wipe.rect),
                    prev.map(fmt_rect),
                    crate::app::draw_cull::node_present_overflow_clip(
                        d.key,
                        d.bounds,
                        &later.node_paints,
                    )
                    .map(fmt_rect)
                ))
            })
            .collect();
        let dump = format!(
            "i={i} ink={ink_w:.1} box={:.1} content={}\n dirties={:?}\n present={:?}\n clips={:?}\n wipes={:?}\n capture={:?}",
            caption.width,
            fmt_rect(content.layout_bounds),
            dirty
                .iter()
                .map(|d| format!("{:?} k{} {}", d.kind, d.key, fmt_rect(d.bounds)))
                .collect::<Vec<_>>(),
            present_dirties
                .iter()
                .map(|d| format!("k{} {}", d.key, fmt_rect(d.bounds)))
                .collect::<Vec<_>>(),
            wipe_clips
                .iter()
                .map(|(k, c)| format!("k{k} {}", fmt_rect(*c)))
                .collect::<Vec<_>>(),
            wipe_diag,
            captured.map(|n| {
                format!(
                    "{} clip={:?}",
                    fmt_rect([n.x, n.y, n.width, n.height]),
                    n.clip_bounds.map(fmt_rect)
                )
            }),
        );
        assert!(
            caption.width + 1.0 >= ink_w,
            "caption box narrower than ink\n{dump}"
        );
        let captured =
            captured.unwrap_or_else(|| panic!("capture dropped Oliver is typing...\n{dump}"));
        let clip = captured
            .clip_bounds
            .unwrap_or_else(|| panic!("capture missing owner clip\n{dump}"));
        assert!(
            clip[2] + 1.0 >= ink_w,
            "nested-skipped shimmer dest-reset clips recapture to Oliver i and flashes\n{dump}"
        );
        let content_key = paint_element_key(content);
        let caption_box = [caption.x, caption.y, caption.width, caption.height.max(1.0)];
        let caption_right = caption.x + ink_w;
        let mut content_wipe = false;
        for (k, hole) in &wipe_clips {
            if intersect_rects(*hole, caption_box).is_none() {
                continue;
            }
            if *k == content_key {
                content_wipe = true;
                assert!(
                    hole[0] + hole[2] + 1.0 >= caption_right,
                    "caption dest-reset k{k} crops Oliver is typing...\n hole={}\n{dump}",
                    fmt_rect(*hole)
                );
                continue;
            }
            panic!(
                "idle shimmer dest-reset of wrap k{k} CLEARs the caption every tick (flicker)\n hole={}\n{dump}",
                fmt_rect(*hole)
            );
        }
        assert!(
            content_wipe,
            "caption dest-reset missing — previous shimmer color stacks in retain\n{dump}"
        );
        prev_slots = now_slots;
    }
}

/// MarkerIcon is `size-4` wrapping Icon `size-4`. Parent `NodePaint` ranges include
/// the child's lucide texture. `same_box` matching both slots src-overs the spinner
/// twice on PartialLoad — idle looks thicker than a FullClear scroll frame.
#[test]
fn node_present_spin_must_not_double_draw_same_box_parent() {
    let _ = take_frame_paint_nodes();
    crate::style::utility::set_frame_dt(1.0 / 60.0);

    let mut icon = Element::new().w(16.0).h(16.0);
    apply_class_list(&mut icon, "animate-spin");
    icon.set_attr("data-slot", "spin-icon");

    let mut wrap = Element::new().w(16.0).h(16.0).child(icon);
    wrap.set_attr("data-slot", "spin-wrap");

    let mut root = Element::new()
        .w(40.0)
        .h(40.0)
        .bg([0.96, 0.96, 0.96, 1.0])
        .child(wrap);
    let mut tree = LayoutTree::new();
    let collected = tree.build_and_layout(&mut root, 40.0, 40.0, [-1.0, -1.0]);
    let _ = take_frame_paint_nodes();

    let mut impact = LayoutStatus::Clean;
    sync_class_variants_subtree(
        &mut root,
        [0.0, 0.0],
        "",
        false,
        40.0,
        40.0,
        &mut tree,
        &mut impact,
    );
    let dirty = take_frame_paint_nodes();
    let spin_el = find_slot(&root, "spin-icon").expect("spin icon");
    let wrap_el = find_slot(&root, "spin-wrap").expect("spin wrap");
    let spin_dirty = dirty
        .iter()
        .find(|d| {
            d.kind == PaintDirtyKind::Node
                && d.key == crate::app::damage::paint_element_key(spin_el)
        })
        .expect("icon chrome dirty");

    let slots: Vec<(u64, [f32; 4])> = collected
        .node_paints
        .iter()
        .filter(|p| !p.overlay)
        .map(|p| (p.key, p.bounds))
        .collect();
    let has_keyed =
        crate::app::damage::paint_dirty_has_keyed_slot(spin_dirty.key, spin_dirty.bounds, slots);
    let matched: Vec<_> = collected
        .node_paints
        .iter()
        .filter(|p| {
            !p.overlay
                && crate::app::damage::paint_dirty_matches_present_slot(
                    spin_dirty.key,
                    spin_dirty.bounds,
                    p.key,
                    p.bounds,
                    has_keyed,
                )
        })
        .collect();
    assert_eq!(
        matched.len(),
        1,
        "idle PartialLoad must draw the icon slot once, not the size-4 wrapper too\n matched={:?}",
        matched
            .iter()
            .map(|p| format!("key={} {}", p.key, fmt_rect(p.bounds)))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        matched[0].key,
        crate::app::damage::paint_element_key(spin_el)
    );
    let wrap_key = crate::app::damage::paint_element_key(wrap_el);
    assert!(
        crate::app::damage::paint_dirty_matches_slot(
            spin_dirty.key,
            spin_dirty.bounds,
            wrap_key,
            wrap_el.layout_bounds,
        ),
        "fixture: wrapper still same_box-matches (the bug this test locks)"
    );
}

/// Gallery Marker: `shimmer-box` dest-reset must stay in its column. Last-present
/// of a parent slot used to union descendant overflow and wipe the next section.
#[test]
fn shimmer_box_dest_reset_must_not_wipe_sibling_column() {
    let _ = take_frame_paint_nodes();
    crate::style::utility::set_frame_dt(1.0 / 60.0);

    let mut box_el = Element::new()
        .w(120.0)
        .h(40.0)
        .bg([0.90, 0.90, 0.92, 1.0])
        .child(Element::new().text("shimmer-box overlay on container"));
    apply_class_list(&mut box_el, "shimmer-box shimmer-duration-1000");
    box_el.set_attr("data-slot", "shimmer-box");
    box_el.set_attr(crate::renderer::ATTR_SHIMMER_EPOCH, "-0.5");

    let mut sibling = Element::new().w(120.0).h(40.0).bg([0.20, 0.40, 0.80, 1.0]);
    sibling.set_attr("data-slot", "sibling");

    let mut root = Element::new()
        .flex_row()
        .w(240.0)
        .h(40.0)
        .bg([0.96, 0.96, 0.96, 1.0])
        .child(box_el)
        .child(sibling);
    let mut tree = LayoutTree::new();
    let collected = tree.build_and_layout(&mut root, 240.0, 40.0, [-1.0, -1.0]);
    let _ = take_frame_paint_nodes();

    let mut impact = LayoutStatus::Clean;
    tick_active_css_transitions_subtree(
        &mut root,
        [0.0, 0.0],
        "",
        false,
        240.0,
        40.0,
        [0.0, 0.0],
        &mut tree,
        &mut impact,
    );
    let dirty = take_frame_paint_nodes();
    let box_el = find_slot(&root, "shimmer-box").expect("shimmer-box");
    let sibling_el = find_slot(&root, "sibling").expect("sibling");
    let key = paint_element_key(box_el);
    let box_dirty = dirty
        .iter()
        .find(|d| d.kind == PaintDirtyKind::Node && d.key == key)
        .expect("shimmer-box must push a node-present dirty");
    let prev = collected
        .node_paints
        .iter()
        .find(|p| p.key == key)
        .map(|p| {
            node_paint_visual_bounds(
                p,
                &collected.instances,
                &[],
                &collected.text_nodes,
                &collected.input_chrome_instances,
            )
        });
    let (wipe, _) = node_present_dest_reset_scaled(
        box_dirty.key,
        box_dirty.bounds,
        &collected.node_paints,
        &collected.instances,
        &[],
        1.0,
        prev,
    )
    .expect("card behind shimmer-box");
    let sib = sibling_el.layout_bounds;
    let cx = sib[0] + sib[2] * 0.5;
    let cy = sib[1] + sib[3] * 0.5;
    assert!(
        cx < wipe[0] || cx > wipe[0] + wipe[2] || cy < wipe[1] || cy > wipe[1] + wipe[3],
        "shimmer-box dest-reset must not punch the sibling column\n wipe={} sibling={} prev={prev:?}",
        fmt_rect(wipe),
        fmt_rect(sib)
    );
}

/// Overflowing shimmer label dest-reset must not punch a disjoint header bar.
#[test]
fn overflowing_shimmer_label_dest_reset_must_not_wipe_header() {
    let _ = take_frame_paint_nodes();
    crate::style::utility::set_frame_dt(1.0 / 60.0);

    let mut header = Element::new()
        .w(400.0)
        .h(32.0)
        .flex_shrink(0.0)
        .bg([1.0, 1.0, 1.0, 1.0])
        .child(Element::new().text("基础"));
    header.set_attr("data-slot", "header");

    let mut label = Element::new()
        .w(80.0)
        .h(20.0)
        .flex_shrink(0.0)
        .text("Generating response…");
    apply_class_list(&mut label, "shimmer shimmer-duration-1000");
    label.set_attr("data-slot", "shimmer-label");
    label.set_attr(crate::renderer::ATTR_SHIMMER_EPOCH, "-0.5");

    let mut root = Element::new()
        .flex_col()
        .w(400.0)
        .h(80.0)
        .bg([0.96, 0.96, 0.96, 1.0])
        .child(header)
        .child(label);
    let mut tree = LayoutTree::new();
    let collected = tree.build_and_layout(&mut root, 400.0, 80.0, [-1.0, -1.0]);
    let _ = take_frame_paint_nodes();

    let mut impact = LayoutStatus::Clean;
    tick_active_css_transitions_subtree(
        &mut root,
        [0.0, 0.0],
        "",
        false,
        400.0,
        80.0,
        [0.0, 0.0],
        &mut tree,
        &mut impact,
    );
    let dirty = take_frame_paint_nodes();
    let label_el = find_slot(&root, "shimmer-label").expect("shimmer label");
    let header_el = find_slot(&root, "header").expect("header");
    let key = paint_element_key(label_el);
    let label_dirty = dirty
        .iter()
        .find(|d| d.kind == PaintDirtyKind::Node && d.key == key)
        .expect("shimmer label must push a node-present dirty");
    let prev = collected
        .node_paints
        .iter()
        .find(|p| p.key == key)
        .map(|p| {
            node_paint_visual_bounds(
                p,
                &collected.instances,
                &[],
                &collected.text_nodes,
                &collected.input_chrome_instances,
            )
        });
    let (wipe, _) = node_present_dest_reset_scaled(
        label_dirty.key,
        label_dirty.bounds,
        &collected.node_paints,
        &collected.instances,
        &[],
        1.0,
        prev,
    )
    .expect("page behind shimmer label");
    let hb = header_el.layout_bounds;
    let cx = hb[0] + hb[2] * 0.5;
    let cy = hb[1] + hb[3] * 0.5;
    assert!(
        cx < wipe[0] || cx > wipe[0] + wipe[2] || cy < wipe[1] || cy > wipe[1] + wipe[3],
        "overflowing shimmer dest-reset must not punch the header\n wipe={} header={} prev={prev:?}",
        fmt_rect(wipe),
        fmt_rect(hb)
    );
}

fn union_xywh(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let x0 = a[0].min(b[0]);
    let y0 = a[1].min(b[1]);
    let x1 = (a[0] + a[2]).max(b[0] + b[2]);
    let y1 = (a[1] + a[3]).max(b[1] + b[3]);
    [x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0)]
}

fn rect_contains_point(r: [f32; 4], x: f32, y: f32) -> bool {
    x >= r[0] && y >= r[1] && x < r[0] + r[2] && y < r[1] + r[3]
}

/// Gallery 基础: title bar above a ScrollArea. Overflowing Marker shimmer and
/// shimmer-box dest-reset / capture / scissor must not punch the title or the
/// sibling column. Previous tests used a single-row tree without overflow clip.
#[test]
fn gallery_marker_like_dest_reset_must_not_punch_title_or_sibling() {
    let _ = take_frame_paint_nodes();
    crate::style::utility::set_frame_dt(1.0 / 60.0);

    let mut header = Element::new()
        .w(400.0)
        .h(48.0)
        .flex_shrink(0.0)
        .bg([1.0, 1.0, 1.0, 1.0])
        .child(Element::new().text("基础"));
    header.set_attr("data-slot", "title");

    let mut label = Element::new()
        .w(160.0)
        .h(20.0)
        .flex_shrink(0.0)
        .text("Generating response…");
    apply_class_list(
        &mut label,
        "shimmer shimmer-duration-1000 whitespace-nowrap",
    );
    label.set_attr("data-slot", "shimmer-label");
    label.set_attr(crate::renderer::ATTR_SHIMMER_EPOCH, "-0.5");

    let mut box_el = Element::new()
        .w("100%")
        .h(40.0)
        .bg([0.90, 0.90, 0.92, 1.0])
        .child(Element::new().text("shimmer-box overlay on container"));
    apply_class_list(&mut box_el, "shimmer-box shimmer-duration-1600");
    box_el.set_attr("data-slot", "shimmer-box");
    box_el.set_attr(crate::renderer::ATTR_SHIMMER_EPOCH, "-0.5");

    let mut sibling = Element::new()
        .w(180.0)
        .h(80.0)
        .flex_shrink(0.0)
        .bg([0.20, 0.40, 0.80, 1.0])
        .child(Element::new().text("Today"));
    sibling.set_attr("data-slot", "sibling");

    let left = Element::new()
        .flex_col()
        .w(180.0)
        .min_w(0.0)
        .gap(8.0)
        .child(label)
        .child(box_el);

    let mut body = Element::new()
        .flex_row()
        .w(400.0)
        .h(160.0)
        .min_h(0.0)
        .overflow_y("hidden")
        .overflow_x("hidden")
        .bg([0.96, 0.96, 0.96, 1.0])
        .child(left)
        .child(sibling);
    body.focus_id = Some(format!("{SCROLL_VIEWPORT_FOCUS_PREFIX}gal-basics"));
    body.set_attr("data-slot", "scroll-body");

    let mut root = Element::new()
        .flex_col()
        .w(400.0)
        .h(208.0)
        .bg([0.94, 0.94, 0.95, 1.0])
        .child(header)
        .child(body);

    let mut tree = LayoutTree::new();
    let _collected = tree.build_and_layout(&mut root, 400.0, 208.0, [-1.0, -1.0]);
    let _ = take_frame_paint_nodes();

    let mut impact = LayoutStatus::Clean;
    tick_active_css_transitions_subtree(
        &mut root,
        [0.0, 0.0],
        "",
        false,
        400.0,
        208.0,
        [0.0, 0.0],
        &mut tree,
        &mut impact,
    );
    let dirty = take_frame_paint_nodes();
    let collected = tree.build_and_layout_fast(&mut root, 400.0, 208.0, [-1.0, -1.0], false, false);

    let title = find_slot(&root, "title").expect("title");
    let sibling_el = find_slot(&root, "sibling").expect("sibling");
    let box_el = find_slot(&root, "shimmer-box").expect("shimmer-box");
    let label_el = find_slot(&root, "shimmer-label").expect("shimmer-label");
    let title_b = title.layout_bounds;
    let sib_b = sibling_el.layout_bounds;
    let box_b = box_el.layout_bounds;

    let mut wipe_union: Option<[f32; 4]> = None;
    let mut scissor_union: Option<[f32; 4]> = None;
    let mut parent_matches = Vec::new();
    for d in dirty.iter().filter(|d| d.kind == PaintDirtyKind::Node) {
        let prev = collected
            .node_paints
            .iter()
            .find(|p| p.key == d.key)
            .map(|p| {
                node_paint_visual_bounds(
                    p,
                    &collected.instances,
                    &[],
                    &collected.text_nodes,
                    &collected.input_chrome_instances,
                )
            });
        let slots: Vec<(u64, [f32; 4])> = prev.map(|b| (d.key, b)).into_iter().collect();
        let scissor = node_present_scissor_logical(
            d.kind,
            d.key,
            d.bounds,
            &slots,
            &collected.node_paints,
            8.0,
        );
        scissor_union = Some(match scissor_union {
            Some(prev) => union_xywh(prev, scissor),
            None => scissor,
        });
        if let Some((wipe, _)) = node_present_dest_reset_scaled(
            d.key,
            d.bounds,
            &collected.node_paints,
            &collected.instances,
            &[],
            1.0,
            prev,
        ) {
            wipe_union = Some(match wipe_union {
                Some(prev) => union_xywh(prev, wipe),
                None => wipe,
            });
        }
        let has_keyed = paint_dirty_has_keyed_slot(
            d.key,
            d.bounds,
            collected
                .node_paints
                .iter()
                .filter(|p| !p.overlay)
                .map(|p| (p.key, p.bounds)),
        );
        for p in collected.node_paints.iter().filter(|p| !p.overlay) {
            if paint_dirty_matches_present_slot(d.key, d.bounds, p.key, p.bounds, has_keyed)
                && p.bounds[2] * p.bounds[3] > d.bounds[2] * d.bounds[3] * 2.0 + 1.0
            {
                parent_matches.push(format!(
                    "dirty {} {} -> slot {} {}",
                    d.key,
                    fmt_rect(d.bounds),
                    p.key,
                    fmt_rect(p.bounds)
                ));
            }
        }
    }

    let capture =
        node_present_clipped_capture_text(&dirty, &collected.text_nodes, &collected.node_paints);
    let title_cx = title_b[0] + title_b[2] * 0.5;
    let title_cy = title_b[1] + title_b[3] * 0.5;
    let sib_cx = sib_b[0] + sib_b[2] * 0.5;
    let sib_cy = sib_b[1] + sib_b[3] * 0.5;

    let dump = format!(
        "title={} sibling={} box={} label_dirty={:?} box_dirty={:?}\n dirties={:?}\n wipe={:?} scissor={:?}\n parent_matches={:?}\n capture_clips={:?}\n text_clips={:?}",
        fmt_rect(title_b),
        fmt_rect(sib_b),
        fmt_rect(box_b),
        dirty
            .iter()
            .find(|d| d.key == paint_element_key(label_el))
            .map(|d| fmt_rect(d.bounds)),
        dirty
            .iter()
            .find(|d| d.key == paint_element_key(box_el))
            .map(|d| fmt_rect(d.bounds)),
        dirty
            .iter()
            .map(|d| format!("{} {}", d.key, fmt_rect(d.bounds)))
            .collect::<Vec<_>>(),
        wipe_union.map(fmt_rect),
        scissor_union.map(fmt_rect),
        parent_matches,
        capture
            .iter()
            .map(|n| n.clip_bounds.map(fmt_rect))
            .collect::<Vec<_>>(),
        collected
            .text_nodes
            .iter()
            .map(|n| format!("{:?} clip={:?}", n.text, n.clip_bounds.map(fmt_rect)))
            .collect::<Vec<_>>(),
    );

    if let Some(w) = wipe_union {
        assert!(
            !rect_contains_point(w, title_cx, title_cy),
            "dest-reset union punches the title\n{dump}"
        );
        assert!(
            !rect_contains_point(w, sib_cx, sib_cy),
            "dest-reset union punches the sibling column\n{dump}"
        );
    }
    if let Some(s) = scissor_union {
        assert!(
            !rect_contains_point(s, title_cx, title_cy),
            "node-present scissor punches the title\n{dump}"
        );
        assert!(
            !rect_contains_point(s, title_b[0] + 40.0, title_b[1] + title_b[3] - 4.0),
            "node-present scissor pad leaks into the title bottom\n{dump}"
        );
    }
    for n in &collected.text_nodes {
        if n.text.contains('基') {
            continue;
        }
        if let Some(c) = n.clip_bounds {
            assert!(
                !rect_contains_point(c, title_cx, title_cy),
                "collect clip includes the title for {:?}\n{dump}",
                n.text
            );
        } else if n.text.contains("Generating") {
            panic!(
                "overflowing shimmer label collect clip_bounds is None (full-surface glyphon)\n{dump}"
            );
        }
    }
    for n in &capture {
        if let Some(c) = n.clip_bounds {
            assert!(
                !rect_contains_point(c, title_cx, title_cy),
                "capture clip includes the title for {:?}\n{dump}",
                n.text
            );
        }
    }
    assert!(
        parent_matches.is_empty(),
        "leaf dirties matched a much larger ancestor slot\n{dump}"
    );
    let box_dirty = dirty
        .iter()
        .find(|d| d.key == paint_element_key(box_el))
        .expect("shimmer-box dirty");
    assert!(
        box_dirty.bounds[2] < 240.0,
        "shimmer-box dirty is page-wide\n{dump}"
    );

    let huge_prev = [0.0, 0.0, 400.0, 208.0];
    for d in dirty.iter().filter(|d| d.kind == PaintDirtyKind::Node) {
        if let Some((wipe, _)) = node_present_dest_reset_scaled(
            d.key,
            d.bounds,
            &collected.node_paints,
            &collected.instances,
            &[],
            1.0,
            Some(huge_prev),
        ) {
            assert!(
                !rect_contains_point(wipe, title_cx, title_cy),
                "recycled last-present dest-reset punches the title\n wipe={} overflow={:?}\n{dump}",
                fmt_rect(wipe),
                collected
                    .node_paints
                    .iter()
                    .filter(|p| p.key == d.key)
                    .map(|p| p.overflow_clip.map(fmt_rect))
                    .collect::<Vec<_>>(),
            );
            assert!(
                !rect_contains_point(wipe, sib_cx, sib_cy),
                "recycled last-present dest-reset punches the sibling\n wipe={}\n{dump}",
                fmt_rect(wipe)
            );
        }
        let scissor = node_present_scissor_logical(
            d.kind,
            d.key,
            d.bounds,
            &[(d.key, huge_prev)],
            &collected.node_paints,
            8.0,
        );
        assert!(
            !rect_contains_point(scissor, title_cx, title_cy),
            "recycled last-present scissor punches the title\n scissor={}\n{dump}",
            fmt_rect(scissor)
        );
        assert!(
            !rect_contains_point(scissor, title_b[0] + 40.0, title_b[1] + title_b[3] - 4.0),
            "recycled last-present scissor pad leaks into the title bottom\n scissor={}\n{dump}",
            fmt_rect(scissor)
        );
    }
}

/// Gallery 基础 title bar sits *above* the ScrollArea. Scrolling Marker to the
/// live edge used to dest-reset the spin circumcircle + 8px pad in document
/// space (or without overflow clip) — leftover 「基础」 / 「变体 / 尺寸 / hover」
/// glyphs and a hairline through the description.
#[test]
fn scrolled_marker_dest_reset_must_not_punch_gallery_title_bar() {
    let _ = take_frame_paint_nodes();
    crate::style::utility::set_frame_dt(1.0 / 60.0);

    let mut title = Element::new()
        .w(400.0)
        .h(24.0)
        .flex_shrink(0.0)
        .child(Element::new().text("基础"));
    title.set_attr("data-slot", "title");
    let mut desc = Element::new()
        .w(400.0)
        .h(20.0)
        .flex_shrink(0.0)
        .child(Element::new().text("Button 变体 / 尺寸 / hover 缓动"));
    desc.set_attr("data-slot", "desc");
    let header = Element::new()
        .flex_col()
        .w(400.0)
        .h(52.0)
        .flex_shrink(0.0)
        .bg([1.0, 1.0, 1.0, 1.0])
        .child(title)
        .child(desc);

    let mut icon = Element::new().w(16.0).h(16.0).bg([0.15, 0.15, 0.15, 1.0]);
    apply_class_list(&mut icon, "animate-spin");
    icon.set_attr("data-slot", "spin-icon");

    let mut marker_el: Element = crate::components::marker::marker()
        .icon(icon)
        .content_with(|c| c.class("shimmer").text("Thinking..."))
        .into();
    marker_el = marker_el.w("100%");
    marker_el.set_attr("data-slot", "marker-row");

    let mut sep: Element = crate::components::marker::marker()
        .variant("separator")
        .content("Today")
        .into();
    sep = sep.w("100%");
    sep.set_attr("data-slot", "marker-sep");

    let column = Element::new()
        .flex_col()
        .w("100%")
        .child(Element::new().w(400.0).h(80.0).flex_shrink(0.0))
        .child(marker_el)
        .child(sep);
    let mut body = Element::from(
        crate::components::scroll_area::scroll_area("gal-marker-title")
            .w(400.0)
            .h(160.0)
            .overflow_y("auto")
            .overflow_x("hidden")
            .child(column),
    );
    use_signal(&scroll_offset_y_signal_key("gal-marker-title"), || 0.0_f32).set_silent(80.0);
    body.set_attr("data-slot", "scroll-body");

    let mut root = Element::new()
        .flex_col()
        .w(400.0)
        .h(212.0)
        .bg([0.94, 0.94, 0.95, 1.0])
        .child(header)
        .child(body);

    let mut tree = LayoutTree::new();
    let first = tree.build_and_layout(&mut root, 400.0, 212.0, [-1.0, -1.0]);
    let _ = take_frame_paint_nodes();

    crate::style::utility::set_frame_dt(0.125);
    let mut impact = LayoutStatus::Clean;
    tick_active_css_transitions_subtree(
        &mut root,
        [0.0, 0.0],
        "",
        false,
        400.0,
        212.0,
        [0.0, 0.0],
        &mut tree,
        &mut impact,
    );
    let dirty = take_frame_paint_nodes();
    let collected = tree.build_and_layout_fast(&mut root, 400.0, 212.0, [-1.0, -1.0], false, false);

    let title = find_slot(&root, "title").expect("title");
    let desc = find_slot(&root, "desc").expect("desc");
    let marker = find_slot(&root, "marker-row").expect("marker");
    let sep = find_slot(&root, "marker-sep").expect("sep");
    let spin = find_slot(&root, "spin-icon").expect("spin");
    let title_b = title.layout_bounds;
    let desc_b = desc.layout_bounds;
    let body_b = find_slot(&root, "scroll-body")
        .expect("scroll-body")
        .layout_bounds;

    let dump = format!(
        "title={} desc={} body={} marker={} sep={} spin={}\n dirties={:?}\n overflow={:?}",
        fmt_rect(title_b),
        fmt_rect(desc_b),
        fmt_rect(body_b),
        fmt_rect(marker.layout_bounds),
        fmt_rect(sep.layout_bounds),
        fmt_rect(spin.layout_bounds),
        dirty
            .iter()
            .map(|d| format!("{} {}", d.key, fmt_rect(d.bounds)))
            .collect::<Vec<_>>(),
        collected
            .node_paints
            .iter()
            .filter(|p| p.key == paint_element_key(spin) || p.key == paint_element_key(marker))
            .map(|p| format!("key={} clip={:?}", p.key, p.overflow_clip.map(fmt_rect)))
            .collect::<Vec<_>>()
    );

    let spin_paint = collected
        .node_paints
        .iter()
        .find(|p| p.key == paint_element_key(spin));
    assert!(
        spin_paint.and_then(|p| p.overflow_clip).is_some(),
        "Marker spin inside ScrollArea must carry overflow_clip so dest-reset cannot leak into 基础\n{dump}"
    );

    assert!(
        desc_b[1] + desc_b[3] <= body_b[1] + 1.0,
        "fixture: description must sit on the title bar just above the scroller\n{dump}"
    );

    let spin_visual = visual_paint_aabb(spin, [0.0, 80.0]);
    assert!(
        (spin.layout_bounds[1] - spin_visual[1]).abs() > 40.0,
        "fixture must separate document Y from visual Y\n{dump}"
    );

    let mut wipe_union: Option<[f32; 4]> = None;
    let mut scissor_union: Option<[f32; 4]> = None;
    for d in dirty.iter().filter(|d| d.kind == PaintDirtyKind::Node) {
        let prev = collected
            .node_paints
            .iter()
            .find(|p| p.key == d.key)
            .map(|p| {
                node_paint_visual_bounds(
                    p,
                    &collected.instances,
                    &[],
                    &collected.text_nodes,
                    &collected.input_chrome_instances,
                )
            });
        let slots: Vec<(u64, [f32; 4])> = prev.map(|b| (d.key, b)).into_iter().collect();
        let scissor = node_present_scissor_logical(
            d.kind,
            d.key,
            d.bounds,
            &slots,
            &collected.node_paints,
            8.0,
        );
        scissor_union = Some(match scissor_union {
            Some(prev) => union_xywh(prev, scissor),
            None => scissor,
        });
        if let Some((wipe, _)) = node_present_dest_reset_scaled(
            d.key,
            d.bounds,
            &collected.node_paints,
            &collected.instances,
            &[],
            1.0,
            prev,
        ) {
            wipe_union = Some(match wipe_union {
                Some(prev) => union_xywh(prev, wipe),
                None => wipe,
            });
        }
    }

    let title_cx = title_b[0] + title_b[2] * 0.5;
    let title_cy = title_b[1] + title_b[3] * 0.5;
    let desc_cx = desc_b[0] + 48.0;
    let desc_cy = desc_b[1] + desc_b[3] * 0.5;
    let desc_bottom = desc_b[1] + desc_b[3] - 2.0;

    if let Some(w) = wipe_union {
        assert!(
            !rect_contains_point(w, title_cx, title_cy),
            "scrolled Marker dest-reset punches 基础\n wipe={}\n{dump}",
            fmt_rect(w)
        );
        assert!(
            !rect_contains_point(w, desc_cx, desc_cy),
            "scrolled Marker dest-reset punches the section description\n wipe={}\n{dump}",
            fmt_rect(w)
        );
    }
    if let Some(s) = scissor_union {
        assert!(
            !rect_contains_point(s, title_cx, title_cy),
            "scrolled Marker scissor punches 基础\n scissor={}\n{dump}",
            fmt_rect(s)
        );
        assert!(
            !rect_contains_point(s, desc_cx, desc_bottom),
            "scrolled Marker scissor pad leaks into 变体 / 尺寸 / hover\n scissor={}\n{dump}",
            fmt_rect(s)
        );
    }

    let thinking = collected
        .text_nodes
        .iter()
        .find(|n| n.text.contains("Thinking"))
        .expect("Thinking caption");
    assert!(
        thinking.y + 2.0 >= body_b[1],
        "Marker caption painted above the scroller (document Y / skip ghost)\n y={} body={}\n{dump}",
        thinking.y,
        fmt_rect(body_b)
    );
    assert!(
        thinking.y + thinking.height <= desc_b[1] + desc_b[3] + 1.0
            || thinking.y >= body_b[1] - 1.0,
        "Marker caption overlaps the title bar\n y={} h={} desc={}\n{dump}",
        thinking.y,
        thinking.height,
        fmt_rect(desc_b)
    );

    let hairlines: Vec<_> = first
        .instances
        .iter()
        .chain(collected.instances.iter())
        .filter(|i| i.rect_size[1] <= 4.0 && i.rect_size[0] > 24.0 && i.color[3] > 0.5)
        .collect();
    for h in &hairlines {
        assert!(
            h.rect_pos[1] + 1.0 >= body_b[1],
            "separator hairline painted in the title bar (错位)\n hairline=[{:.1},{:.1} {:.1}x{:.1}]\n{dump}",
            h.rect_pos[0],
            h.rect_pos[1],
            h.rect_size[0],
            h.rect_size[1]
        );
    }
}

/// Gallery 基础: page title sits *above* the ScrollArea. Message (shimmer Marker
/// + Delivered + end-aligned Builder) lives at the top of the scroll content.
/// After the user scrolls that block *out* (just above the viewport, still
/// ticking), dest-reset of shimmer ∪ last-present used to punch 「基础」/说明
/// blank and skip-replay Message glyphs at the pre-scroll Y (叠字 + 错位).
#[test]
fn scrolled_out_shimmer_marker_must_not_punch_title_or_misplace_message() {
    let _ = take_frame_paint_nodes();
    reset_collect_skip_counters();
    crate::style::utility::set_frame_dt(1.0 / 60.0);

    let mut title = Element::new()
        .w(400.0)
        .h(24.0)
        .flex_shrink(0.0)
        .child(Element::new().text("基础"));
    title.set_attr("data-slot", "title");
    let mut desc = Element::new()
        .w(400.0)
        .h(20.0)
        .flex_shrink(0.0)
        .child(Element::new().text("Button 变体 / 尺寸 / hover 缓动 · Badge · Kbd · Marker"));
    desc.set_attr("data-slot", "desc");
    let header = Element::new()
        .flex_col()
        .w(400.0)
        .h(52.0)
        .flex_shrink(0.0)
        .bg([1.0, 1.0, 1.0, 1.0])
        .child(title)
        .child(desc);

    let mut icon = Element::new().w(16.0).h(16.0).bg([0.15, 0.15, 0.15, 1.0]);
    apply_class_list(&mut icon, "animate-spin");
    icon.set_attr("data-slot", "spin-icon");
    let mut marker_el: Element = marker()
        .icon(icon)
        .content_with(|c| c.class("shimmer").text("Oliver is typing..."))
        .into();
    marker_el.set_attr("data-slot", "shimmer-marker");

    let start_row: Element = message()
        .align("start")
        .content_child(
            bubble()
                .variant("outline")
                .align("start")
                .child(bubble_content().text("The row packs to this side.")),
        )
        .footer("Delivered")
        .into();

    let end_row: Element = message()
        .align("end")
        .avatar(
            Element::new()
                .w(28.0)
                .h(28.0)
                .bg([0.85, 0.85, 0.88, 1.0])
                .child(Element::new().text("B")),
        )
        .header("Builder")
        .content_child(
            bubble()
                .variant("secondary")
                .align("end")
                .child(bubble_content().text("On it, one-line change.")),
        )
        .footer("Delivered")
        .into();

    let mut shimmer_caption = Element::new()
        .w(400.0)
        .h(16.0)
        .flex_shrink(0.0)
        .child(Element::new().text("Shimmer (streaming text)"));
    shimmer_caption.set_attr("data-slot", "shimmer-caption");

    let mut message_block = Element::new()
        .flex_col()
        .w("100%")
        .gap(8.0)
        .flex_shrink(0.0)
        .child(shimmer_caption)
        .child(marker_el)
        .child(start_row)
        .child(end_row);
    message_block.set_attr("data-slot", "message-block");

    let mut ms_title = Element::new()
        .w(400.0)
        .h(28.0)
        .flex_shrink(0.0)
        .child(Element::new().text("1c-ms) MessageScroller"));
    ms_title.set_attr("data-slot", "ms-title");

    let column = Element::new()
        .flex_col()
        .w("100%")
        .child(message_block)
        .child(ms_title)
        .child(Element::new().w(400.0).h(400.0).flex_shrink(0.0));

    let scroll_id = "gal-shimmer-title-punch";
    use_signal(&scroll_offset_y_signal_key(scroll_id), || 0.0_f32).set_silent(0.0);
    let mut body = Element::from(
        crate::components::scroll_area::scroll_area(scroll_id)
            .w(400.0)
            .h(160.0)
            .overflow_y("auto")
            .overflow_x("hidden")
            .child(column),
    );
    body.set_attr("data-slot", "scroll-body");

    let mut root = Element::new()
        .flex_col()
        .w(400.0)
        .h(212.0)
        .bg([0.94, 0.94, 0.95, 1.0])
        .child(header)
        .child(body);

    let mut tree = LayoutTree::new();
    let first = tree.build_and_layout(&mut root, 400.0, 212.0, [-1.0, -1.0]);
    let prev_slots = last_present_visual_slots(
        &first.node_paints,
        &first.instances,
        &[],
        &first.text_nodes,
        &first.input_chrome_instances,
    );
    let _ = take_frame_paint_nodes();
    tree.set_collect_skip_allowed(true);
    let _warm = tree.build_and_layout_fast(&mut root, 400.0, 212.0, [-1.0, -1.0], false, false);

    let message_block = find_slot(&root, "message-block").expect("message-block");
    let scroll_out = (message_block.layout_bounds[3] + 12.0).max(80.0);
    use_signal(&scroll_offset_y_signal_key(scroll_id), || 0.0_f32).set_silent(scroll_out);

    tree.set_collect_skip_allowed(true);
    crate::style::utility::set_frame_dt(0.125);
    let mut impact = LayoutStatus::Clean;
    tick_active_css_transitions_subtree(
        &mut root,
        [0.0, 0.0],
        "",
        false,
        400.0,
        212.0,
        [0.0, 0.0],
        &mut tree,
        &mut impact,
    );
    sync_class_variants_subtree(
        &mut root,
        [0.0, 0.0],
        "",
        false,
        400.0,
        212.0,
        &mut tree,
        &mut impact,
    );
    let mut dirty = take_frame_paint_nodes();
    let collected = tree.build_and_layout_fast(&mut root, 400.0, 212.0, [-1.0, -1.0], false, false);
    let now_slots = last_present_visual_slots(
        &collected.node_paints,
        &collected.instances,
        &[],
        &collected.text_nodes,
        &collected.input_chrome_instances,
    );
    expand_paint_dirties_for_translated_slots(&mut dirty, &now_slots, &prev_slots);
    escalate_dirties_for_node_present(&mut dirty, &collected.node_paints, &prev_slots);

    let title = find_slot(&root, "title").expect("title");
    let desc = find_slot(&root, "desc").expect("desc");
    let body_el = find_slot(&root, "scroll-body").expect("scroll-body");
    let title_b = title.layout_bounds;
    let desc_b = desc.layout_bounds;
    let body_b = body_el.layout_bounds;
    let title_cx = title_b[0] + title_b[2] * 0.5;
    let title_cy = title_b[1] + title_b[3] * 0.5;
    let desc_cx = desc_b[0] + 48.0;
    let desc_cy = desc_b[1] + desc_b[3] * 0.5;
    let desc_bottom = desc_b[1] + desc_b[3] - 2.0;

    let dump = format!(
        "scroll={scroll_out:.1} title={} desc={} body={}\n dirties={:?}\n overflows={:?}\n texts={:?}",
        fmt_rect(title_b),
        fmt_rect(desc_b),
        fmt_rect(body_b),
        dirty
            .iter()
            .map(|d| format!("{} {}", d.key, fmt_rect(d.bounds)))
            .collect::<Vec<_>>(),
        collected
            .node_paints
            .iter()
            .filter_map(|p| p.overflow_clip.map(fmt_rect))
            .collect::<Vec<_>>(),
        collected
            .text_nodes
            .iter()
            .map(|n| format!("{:?} y={:.1} x={:.1} clip={:?}", n.text, n.y, n.x, n.clip_bounds.map(fmt_rect)))
            .collect::<Vec<_>>()
    );

    assert!(
        desc_b[1] + desc_b[3] <= body_b[1] + 1.0,
        "fixture: description must sit on the title bar just above the scroller\n{dump}"
    );

    for d in dirty.iter().filter(|d| d.kind != PaintDirtyKind::Overlay) {
        let prev = last_present_prev_for_dirty(d.kind, d.key, &prev_slots);
        let scissor = node_present_scissor_logical(
            d.kind,
            d.key,
            d.bounds,
            &prev_slots,
            &collected.node_paints,
            node_present_scissor_pad(d.kind),
        );
        if scissor[2] > 0.5 && scissor[3] > 0.5 {
            assert!(
                !rect_contains_point(scissor, title_cx, title_cy),
                "scrolled-out shimmer scissor punches 基础\n scissor={} dirty={}\n{dump}",
                fmt_rect(scissor),
                fmt_rect(d.bounds)
            );
            assert!(
                !rect_contains_point(scissor, title_b[0] + 8.0, title_cy),
                "scrolled-out shimmer scissor punches the left of 基础\n scissor={} dirty={}\n{dump}",
                fmt_rect(scissor),
                fmt_rect(d.bounds)
            );
            assert!(
                !rect_contains_point(scissor, desc_cx, desc_cy),
                "scrolled-out shimmer scissor punches the section description\n scissor={} dirty={}\n{dump}",
                fmt_rect(scissor),
                fmt_rect(d.bounds)
            );
            assert!(
                !rect_contains_point(scissor, desc_cx, desc_bottom),
                "scrolled-out shimmer scissor pad leaks into 变体 / 尺寸 / hover\n scissor={} dirty={}\n{dump}",
                fmt_rect(scissor),
                fmt_rect(d.bounds)
            );
        }
        if let Some((wipe, _)) = node_present_dest_reset_scaled(
            d.key,
            d.bounds,
            &collected.node_paints,
            &collected.instances,
            &[],
            1.0,
            prev,
        ) {
            assert!(
                !rect_contains_point(wipe, title_cx, title_cy),
                "scrolled-out shimmer dest-reset punches 基础 blank\n wipe={} dirty={}\n{dump}",
                fmt_rect(wipe),
                fmt_rect(d.bounds)
            );
            assert!(
                !rect_contains_point(wipe, title_b[0] + 8.0, title_cy),
                "scrolled-out shimmer dest-reset punches the left of 基础\n wipe={} dirty={}\n{dump}",
                fmt_rect(wipe),
                fmt_rect(d.bounds)
            );
            assert!(
                !rect_contains_point(wipe, desc_cx, desc_cy),
                "scrolled-out shimmer dest-reset punches the section description blank\n wipe={} dirty={}\n{dump}",
                fmt_rect(wipe),
                fmt_rect(d.bounds)
            );
        }
    }

    let wipe_clips: Vec<(u64, [f32; 4])> = dirty
        .iter()
        .filter(|d| d.kind == PaintDirtyKind::Node)
        .filter_map(|d| {
            let prev = last_present_prev_for_dirty(d.kind, d.key, &prev_slots);
            let wipe = node_present_dest_reset_full(
                d.key,
                d.bounds,
                &collected.node_paints,
                &collected.instances,
                &[],
                &[],
                1.0,
                prev,
            )?
            .rect;
            Some((d.key, node_present_glyph_clip_logical(d, Some(wipe), 1.0)))
        })
        .collect();
    let capture = node_present_clipped_capture_text_wipes(
        &dirty,
        &collected.text_nodes,
        &collected.node_paints,
        &wipe_clips,
    );
    for n in &capture {
        let stamp = n.text.contains("Shimmer")
            || n.text.contains("Delivered")
            || n.text.contains("Oliver")
            || n.text.contains("On it")
            || n.text.contains("packs")
            || n.text == "B";
        if !stamp {
            continue;
        }
        assert!(
            n.y + 2.0 >= body_b[1],
            "scroll content recaptured into the title bar\n text={:?} y={:.1}\n{dump}",
            n.text,
            n.y
        );
        if let Some(clip) = n.clip_bounds {
            assert!(
                !rect_contains_point(clip, title_cx, title_cy),
                "capture clip stamps {:?} into 基础\n clip={}\n{dump}",
                n.text,
                fmt_rect(clip)
            );
            assert!(
                !rect_contains_point(clip, desc_cx, desc_cy),
                "capture clip stamps {:?} into the section description\n clip={}\n{dump}",
                n.text,
                fmt_rect(clip)
            );
        }
    }

    for needle in ["基础", "Button 变体", "Shimmer (streaming text)"] {
        let hits = collected
            .text_nodes
            .iter()
            .filter(|n| n.text.contains(needle))
            .count();
        assert!(
            hits <= 1,
            "skip-ghost + live double-paint of {needle:?} (叠字)\n{dump}"
        );
    }

    for needle in [
        "Oliver is typing",
        "The row packs",
        "On it, one-line",
        "Delivered",
        "Builder",
    ] {
        let hits: Vec<_> = collected
            .text_nodes
            .iter()
            .filter(|n| n.text.contains(needle))
            .collect();
        assert!(
            hits.len() <= 1,
            "skip-ghost + live double-paint of {needle:?} (叠字)\n{dump}"
        );
        for n in &hits {
            assert!(
                n.y + n.height <= desc_b[1] + desc_b[3] + 1.0 || n.y >= body_b[1] - 1.0,
                "Message {needle:?} painted in the title bar after scroll-out\n y={:.1} h={:.1}\n{dump}",
                n.y,
                n.height
            );
            let in_viewport = n.y + n.height > body_b[1] + 2.0 && n.y < body_b[1] + body_b[3] - 2.0;
            assert!(
                !in_viewport,
                "Message {needle:?} still painted at pre-scroll Y after scrolling out of the message block\n y={:.1} body={}\n{dump}",
                n.y,
                fmt_rect(body_b)
            );
        }
    }

    let on_it = collected
        .text_nodes
        .iter()
        .find(|n| n.text.contains("On it, one-line"));
    if let Some(n) = on_it {
        assert!(
            n.x + n.width * 0.5 > 200.0,
            "end-aligned Builder bubble landed on the start side after scroll-out\n x={:.1} w={:.1}\n{dump}",
            n.x,
            n.width
        );
    }

    let first_delivered_y = first
        .text_nodes
        .iter()
        .find(|n| n.text.contains("Delivered"))
        .map(|n| n.y);
    if let (Some(old_y), Some(now)) = (
        first_delivered_y,
        collected
            .text_nodes
            .iter()
            .find(|n| n.text.contains("Delivered")),
    ) {
        assert!(
            (now.y - old_y).abs() > 20.0,
            "Delivered stayed at the pre-scroll Y after the message block scrolled out (错位)\n old={old_y:.1} now={:.1}\n{dump}",
            now.y
        );
    }

    let ms_title = collected
        .text_nodes
        .iter()
        .find(|n| n.text.contains("1c-ms)"))
        .expect("MessageScroller card title");
    if ms_title.y < body_b[1] - 1.0 {
        let clip = ms_title
            .clip_bounds
            .expect("scrolled card title above the viewport must keep ScrollArea clip");
        assert!(
            !rect_contains_point(clip, title_cx, title_cy),
            "MessageScroller title clip punches 基础\n clip={}\n{dump}",
            fmt_rect(clip)
        );
        assert!(
            !rect_contains_point(clip, desc_cx, desc_cy),
            "MessageScroller title clip punches the section description\n clip={}\n{dump}",
            fmt_rect(clip)
        );
    }

    let stale_bubbles: Vec<_> = first
        .instances
        .iter()
        .filter(|i| {
            i.rect_size[0] > 80.0
                && i.rect_size[1] > 20.0
                && i.rect_size[1] < 80.0
                && i.color[3] > 0.4
                && i.rect_pos[1] >= body_b[1]
                && i.rect_pos[1] < body_b[1] + body_b[3]
        })
        .collect();
    for old in &stale_bubbles {
        let ghost = collected.instances.iter().any(|i| {
            (i.rect_pos[0] - old.rect_pos[0]).abs() < 2.0
                && (i.rect_pos[1] - old.rect_pos[1]).abs() < 2.0
                && (i.rect_size[0] - old.rect_size[0]).abs() < 2.0
                && (i.rect_size[1] - old.rect_size[1]).abs() < 2.0
        });
        assert!(
            !ghost,
            "Message bubble skip-replayed at the pre-scroll Y after scrolling out (错位)\n old=[{:.1},{:.1} {:.1}x{:.1}]\n{dump}",
            old.rect_pos[0],
            old.rect_pos[1],
            old.rect_size[0],
            old.rect_size[1]
        );
    }
}

/// Gallery 基础: Marker shimmer block sits above MessageScroller. After the user
/// scrolls Marker *out* so `1c-ms) MessageScroller` is at the live edge, dest-reset
/// of the still-ticking culled shimmer used last-present at the old visual Y —
/// CLEAR punched the MessageScroller title and capture stamped `Thinking…` /
/// `Reading 4 files` onto it. The 8px scissor pad of that same last-present
/// also leaked into 「基础」 / 说明.
#[test]
fn culled_shimmer_must_not_punch_live_edge_message_scroller_or_title() {
    let _ = take_frame_paint_nodes();
    reset_collect_skip_counters();
    crate::style::utility::set_frame_dt(1.0 / 60.0);

    let mut title = Element::new()
        .w(400.0)
        .h(24.0)
        .flex_shrink(0.0)
        .child(Element::new().text("基础"));
    title.set_attr("data-slot", "title");
    let mut desc = Element::new()
        .w(400.0)
        .h(20.0)
        .flex_shrink(0.0)
        .child(Element::new().text("Button 变体 / 尺寸 / hover 缓动 · Badge · Kbd · Marker"));
    desc.set_attr("data-slot", "desc");
    let header = Element::new()
        .flex_col()
        .w(400.0)
        .h(52.0)
        .flex_shrink(0.0)
        .bg([1.0, 1.0, 1.0, 1.0])
        .child(title)
        .child(desc);

    let mut shimmer_caption = Element::new()
        .w(400.0)
        .h(18.0)
        .flex_shrink(0.0)
        .child(Element::new().text("Shimmer (streaming text)"));
    shimmer_caption.set_attr("data-slot", "shimmer-caption");

    let mut thinking: Element = marker()
        .content_with(|c| c.class("shimmer").text("Thinking..."))
        .into();
    thinking.set_attr("data-slot", "thinking-marker");

    let mut reading: Element = marker()
        .variant("separator")
        .content_with(|c| c.class("shimmer").text("Reading 4 files"))
        .into();
    reading.set_attr("data-slot", "reading-marker");

    let mut generating = Element::new()
        .w("100%")
        .h(20.0)
        .flex_shrink(0.0)
        .text("Generating response...");
    apply_class_list(&mut generating, "shimmer shimmer-duration-1000");
    generating.set_attr("data-slot", "generating");
    generating.set_attr(crate::renderer::ATTR_SHIMMER_EPOCH, "-0.5");

    let mut box_el = Element::new()
        .w("100%")
        .h(32.0)
        .flex_shrink(0.0)
        .bg([0.90, 0.90, 0.92, 1.0])
        .child(Element::new().text("shimmer-box · overlay on container"));
    apply_class_list(&mut box_el, "shimmer-box shimmer-duration-1600");
    box_el.set_attr("data-slot", "shimmer-box");
    box_el.set_attr(crate::renderer::ATTR_SHIMMER_EPOCH, "-0.5");

    let mut marker_block = Element::new()
        .flex_col()
        .w("100%")
        .gap(8.0)
        .flex_shrink(0.0)
        .child(shimmer_caption)
        .child(thinking)
        .child(reading)
        .child(generating)
        .child(box_el);
    marker_block.set_attr("data-slot", "marker-block");

    let mut ms_title = Element::new()
        .w(400.0)
        .h(28.0)
        .flex_shrink(0.0)
        .child(Element::new().text("1c-ms) MessageScroller (ShadCN conversation scroller)"));
    ms_title.set_attr("data-slot", "ms-title");
    let mut ms_desc = Element::new()
        .w(400.0)
        .h(36.0)
        .flex_shrink(0.0)
        .child(Element::new().text("Official chat card: New Chat header, muted-user bubbles"));
    ms_desc.set_attr("data-slot", "ms-desc");

    let column = Element::new()
        .flex_col()
        .w("100%")
        .child(marker_block)
        .child(ms_title)
        .child(ms_desc)
        .child(Element::new().w(400.0).h(400.0).flex_shrink(0.0));

    let scroll_id = "gal-shimmer-live-edge-punch";
    use_signal(&scroll_offset_y_signal_key(scroll_id), || 0.0_f32).set_silent(0.0);
    let mut body = Element::from(
        crate::components::scroll_area::scroll_area(scroll_id)
            .w(400.0)
            .h(160.0)
            .overflow_y("auto")
            .overflow_x("hidden")
            .child(column),
    );
    body.set_attr("data-slot", "scroll-body");

    let mut root = Element::new()
        .flex_col()
        .w(400.0)
        .h(212.0)
        .bg([0.94, 0.94, 0.95, 1.0])
        .child(header)
        .child(body);

    let mut tree = LayoutTree::new();
    let first = tree.build_and_layout(&mut root, 400.0, 212.0, [-1.0, -1.0]);
    let prev_slots = last_present_visual_slots(
        &first.node_paints,
        &first.instances,
        &[],
        &first.text_nodes,
        &first.input_chrome_instances,
    );
    let _ = take_frame_paint_nodes();
    tree.set_collect_skip_allowed(true);
    let _warm = tree.build_and_layout_fast(&mut root, 400.0, 212.0, [-1.0, -1.0], false, false);

    let marker_block = find_slot(&root, "marker-block").expect("marker-block");
    let scroll_out = (marker_block.layout_bounds[3] + 8.0).max(80.0);
    use_signal(&scroll_offset_y_signal_key(scroll_id), || 0.0_f32).set_silent(scroll_out);

    crate::style::utility::set_frame_dt(0.125);
    let mut impact = LayoutStatus::Clean;
    tick_active_css_transitions_subtree(
        &mut root,
        [0.0, 0.0],
        "",
        false,
        400.0,
        212.0,
        [0.0, 0.0],
        &mut tree,
        &mut impact,
    );
    sync_class_variants_subtree(
        &mut root,
        [0.0, 0.0],
        "",
        false,
        400.0,
        212.0,
        &mut tree,
        &mut impact,
    );
    let mut dirty = take_frame_paint_nodes();
    let collected = tree.build_and_layout_fast(&mut root, 400.0, 212.0, [-1.0, -1.0], false, false);
    let now_slots = last_present_visual_slots(
        &collected.node_paints,
        &collected.instances,
        &[],
        &collected.text_nodes,
        &collected.input_chrome_instances,
    );
    expand_paint_dirties_for_translated_slots(&mut dirty, &now_slots, &prev_slots);
    escalate_dirties_for_node_present(&mut dirty, &collected.node_paints, &prev_slots);

    let title = find_slot(&root, "title").expect("title");
    let desc = find_slot(&root, "desc").expect("desc");
    let body_el = find_slot(&root, "scroll-body").expect("scroll-body");
    let ms = find_slot(&root, "ms-title").expect("ms-title");
    let title_b = title.layout_bounds;
    let desc_b = desc.layout_bounds;
    let body_b = body_el.layout_bounds;
    let ms_b = ms.layout_bounds;
    let title_cx = title_b[0] + title_b[2] * 0.5;
    let title_cy = title_b[1] + title_b[3] * 0.5;
    let desc_cx = desc_b[0] + 48.0;
    let desc_cy = desc_b[1] + desc_b[3] * 0.5;
    let ms_cx = ms_b[0] + 48.0;
    let ms_cy = {
        let ms_text = collected
            .text_nodes
            .iter()
            .find(|n| n.text.contains("1c-ms)"))
            .expect("MessageScroller title collected at live edge");
        ms_text.y + ms_text.height * 0.5
    };

    let dump = format!(
        "scroll={scroll_out:.1} title={} desc={} body={} ms_layout={} ms_cy={ms_cy:.1}\n dirties={:?}\n texts={:?}",
        fmt_rect(title_b),
        fmt_rect(desc_b),
        fmt_rect(body_b),
        fmt_rect(ms_b),
        dirty
            .iter()
            .map(|d| format!("{:?} {} {}", d.kind, d.key, fmt_rect(d.bounds)))
            .collect::<Vec<_>>(),
        collected
            .text_nodes
            .iter()
            .map(|n| format!("{:?} y={:.1} clip={:?}", n.text, n.y, n.clip_bounds.map(fmt_rect)))
            .collect::<Vec<_>>()
    );

    assert!(
        desc_b[1] + desc_b[3] <= body_b[1] + 1.0,
        "fixture: description must sit on the title bar just above the scroller\n{dump}"
    );
    assert!(
        ms_cy >= body_b[1] - 1.0 && ms_cy < body_b[1] + 40.0,
        "fixture: MessageScroller title must sit at the live edge after Marker scrolls out\n{dump}"
    );

    let phantom_dirty = |d: &crate::app::damage::PaintDirty,
                         paints: &[crate::layout::layout_tree::NodePaint]| {
        let culled = !paints.iter().any(|p| !p.overlay && p.key == d.key);
        let above = d.bounds[1] + d.bounds[3] <= body_b[1] + 1.0;
        culled || above
    };

    for d in dirty.iter().filter(|d| d.kind != PaintDirtyKind::Overlay) {
        let prev = last_present_prev_for_dirty(d.kind, d.key, &prev_slots);
        let scissor = node_present_scissor_logical(
            d.kind,
            d.key,
            d.bounds,
            &prev_slots,
            &collected.node_paints,
            node_present_scissor_pad(d.kind),
        );
        if scissor[2] > 0.5 && scissor[3] > 0.5 {
            assert!(
                !rect_contains_point(scissor, title_cx, title_cy),
                "culled shimmer scissor punches 基础\n scissor={} dirty={}\n{dump}",
                fmt_rect(scissor),
                fmt_rect(d.bounds)
            );
            assert!(
                !rect_contains_point(scissor, desc_cx, desc_cy),
                "culled shimmer scissor punches the section description\n scissor={} dirty={}\n{dump}",
                fmt_rect(scissor),
                fmt_rect(d.bounds)
            );
            // Scroll-frame `expand` of still-collected MessageScroller is a
            // prev∪now strip inside the scroller. Production skips that expand
            // (`scroll_layer_clear`). Culled Marker phantoms must still not punch.
            if phantom_dirty(d, &collected.node_paints) {
                assert!(
                    !rect_contains_point(scissor, ms_cx, ms_cy),
                    "culled shimmer last-present scissor punches 1c-ms) MessageScroller\n scissor={} dirty={}\n{dump}",
                    fmt_rect(scissor),
                    fmt_rect(d.bounds)
                );
            }
        }
        if let Some((wipe, _)) = node_present_dest_reset_scaled(
            d.key,
            d.bounds,
            &collected.node_paints,
            &collected.instances,
            &[],
            1.0,
            prev,
        ) {
            assert!(
                !rect_contains_point(wipe, title_cx, title_cy),
                "culled shimmer dest-reset punches 基础 blank\n wipe={} dirty={}\n{dump}",
                fmt_rect(wipe),
                fmt_rect(d.bounds)
            );
            assert!(
                !rect_contains_point(wipe, desc_cx, desc_cy),
                "culled shimmer dest-reset punches the section description\n wipe={} dirty={}\n{dump}",
                fmt_rect(wipe),
                fmt_rect(d.bounds)
            );
            if phantom_dirty(d, &collected.node_paints) {
                assert!(
                    !rect_contains_point(wipe, ms_cx, ms_cy),
                    "culled shimmer last-present dest-reset punches 1c-ms) MessageScroller\n wipe={} dirty={}\n{dump}",
                    fmt_rect(wipe),
                    fmt_rect(d.bounds)
                );
            }
        }
    }

    let hole_dirties: Vec<_> = dirty
        .iter()
        .copied()
        .filter(|d| d.kind != PaintDirtyKind::Overlay)
        .filter(|d| {
            let s = node_present_scissor_logical(
                d.kind,
                d.key,
                d.bounds,
                &prev_slots,
                &collected.node_paints,
                node_present_scissor_pad(d.kind),
            );
            s[2] > 0.5 && s[3] > 0.5
        })
        .collect();
    let wipe_clips: Vec<(u64, [f32; 4])> = hole_dirties
        .iter()
        .filter(|d| d.kind == PaintDirtyKind::Node)
        .filter_map(|d| {
            let prev = last_present_prev_for_dirty(d.kind, d.key, &prev_slots);
            let wipe = node_present_dest_reset_full(
                d.key,
                d.bounds,
                &collected.node_paints,
                &collected.instances,
                &[],
                &[],
                1.0,
                prev,
            )?
            .rect;
            Some((d.key, node_present_glyph_clip_logical(d, Some(wipe), 1.0)))
        })
        .collect();
    let capture = node_present_clipped_capture_text_wipes(
        &hole_dirties,
        &collected.text_nodes,
        &collected.node_paints,
        &wipe_clips,
    );
    for n in &capture {
        let stamp = n.text.contains("Shimmer")
            || n.text.contains("Thinking")
            || n.text.contains("Reading")
            || n.text.contains("Generating")
            || n.text.contains("shimmer-box");
        if !stamp {
            continue;
        }
        assert!(
            n.y + 2.0 >= body_b[1],
            "Marker glyph recaptured into the title bar\n text={:?} y={:.1}\n{dump}",
            n.text,
            n.y
        );
        if let Some(clip) = n.clip_bounds {
            assert!(
                !rect_contains_point(clip, title_cx, title_cy),
                "capture clip stamps {:?} into 基础\n clip={}\n{dump}",
                n.text,
                fmt_rect(clip)
            );
            assert!(
                !rect_contains_point(clip, ms_cx, ms_cy),
                "capture clip stamps {:?} onto 1c-ms) MessageScroller\n clip={}\n{dump}",
                n.text,
                fmt_rect(clip)
            );
        }
    }

    // After the scroll blit, last-present is the post-scroll slots (culled
    // Marker keys dropped). The next shimmer tick is node-present, not a
    // scroll-layer expand — this is the Gallery frame that punched 「基础」
    // and stamped Thinking / Reading onto 1c-ms).
    crate::style::utility::set_frame_dt(0.125);
    let mut settled_impact = LayoutStatus::Clean;
    tick_active_css_transitions_subtree(
        &mut root,
        [0.0, 0.0],
        "",
        false,
        400.0,
        212.0,
        [0.0, 0.0],
        &mut tree,
        &mut settled_impact,
    );
    sync_class_variants_subtree(
        &mut root,
        [0.0, 0.0],
        "",
        false,
        400.0,
        212.0,
        &mut tree,
        &mut settled_impact,
    );
    let mut settled_dirty = take_frame_paint_nodes();
    let settled = tree.build_and_layout_fast(&mut root, 400.0, 212.0, [-1.0, -1.0], false, false);
    let settled_slots = last_present_visual_slots(
        &settled.node_paints,
        &settled.instances,
        &[],
        &settled.text_nodes,
        &settled.input_chrome_instances,
    );
    expand_paint_dirties_for_translated_slots(&mut settled_dirty, &settled_slots, &now_slots);
    escalate_dirties_for_node_present(&mut settled_dirty, &settled.node_paints, &now_slots);
    let settled_dump = format!(
        "settled dirties={:?}\n texts={:?}",
        settled_dirty
            .iter()
            .map(|d| format!("{:?} {} {}", d.kind, d.key, fmt_rect(d.bounds)))
            .collect::<Vec<_>>(),
        settled
            .text_nodes
            .iter()
            .map(|n| format!("{:?} y={:.1}", n.text, n.y))
            .collect::<Vec<_>>()
    );
    for d in settled_dirty
        .iter()
        .filter(|d| d.kind != PaintDirtyKind::Overlay)
    {
        let prev = last_present_prev_for_dirty(d.kind, d.key, &now_slots);
        let scissor = node_present_scissor_logical(
            d.kind,
            d.key,
            d.bounds,
            &now_slots,
            &settled.node_paints,
            node_present_scissor_pad(d.kind),
        );
        if scissor[2] > 0.5 && scissor[3] > 0.5 {
            assert!(
                !rect_contains_point(scissor, title_cx, title_cy),
                "settled shimmer scissor punches 基础\n scissor={} dirty={}\n{settled_dump}",
                fmt_rect(scissor),
                fmt_rect(d.bounds)
            );
            assert!(
                !rect_contains_point(scissor, desc_cx, desc_cy),
                "settled shimmer scissor punches the section description\n scissor={} dirty={}\n{settled_dump}",
                fmt_rect(scissor),
                fmt_rect(d.bounds)
            );
            assert!(
                !rect_contains_point(scissor, ms_cx, ms_cy),
                "settled shimmer scissor punches 1c-ms) MessageScroller\n scissor={} dirty={}\n{settled_dump}",
                fmt_rect(scissor),
                fmt_rect(d.bounds)
            );
        }
        if let Some((wipe, _)) = node_present_dest_reset_scaled(
            d.key,
            d.bounds,
            &settled.node_paints,
            &settled.instances,
            &[],
            1.0,
            prev,
        ) {
            assert!(
                !rect_contains_point(wipe, title_cx, title_cy),
                "settled shimmer dest-reset punches 基础\n wipe={} dirty={}\n{settled_dump}",
                fmt_rect(wipe),
                fmt_rect(d.bounds)
            );
            assert!(
                !rect_contains_point(wipe, ms_cx, ms_cy),
                "settled shimmer dest-reset punches 1c-ms) MessageScroller\n wipe={} dirty={}\n{settled_dump}",
                fmt_rect(wipe),
                fmt_rect(d.bounds)
            );
        }
    }

    for needle in [
        "Thinking",
        "Reading 4 files",
        "Generating response",
        "shimmer-box",
        "Shimmer (streaming text)",
    ] {
        let hits: Vec<_> = collected
            .text_nodes
            .iter()
            .filter(|n| n.text.contains(needle))
            .collect();
        assert!(
            hits.len() <= 1,
            "skip-ghost + live double-paint of {needle:?} (叠字)\n{dump}"
        );
        for n in &hits {
            assert!(
                n.y + n.height <= desc_b[1] + desc_b[3] + 1.0 || n.y >= body_b[1] - 1.0,
                "Marker {needle:?} painted in the title bar\n y={:.1}\n{dump}",
                n.y
            );
            let on_ms = (n.y - ms_cy).abs() < 16.0;
            assert!(
                !on_ms,
                "Marker {needle:?} skip-replayed onto 1c-ms) MessageScroller (错位)\n y={:.1} ms_cy={ms_cy:.1}\n{dump}",
                n.y
            );
        }
    }
}

/// Gallery 基础: Marker (Thinking / Reading / Generating / shimmer-box) still
/// straddles the page ScrollArea while `1c-ms) MessageScroller` is already at
/// the live edge. Skip-replay of the incomplete Marker DL / last-present of
/// clip-culled children used to stamp those glyphs onto the section title
/// until the whole Marker card left the clip.
#[test]
fn straddling_marker_must_not_stamp_live_edge_message_scroller() {
    let _ = take_frame_paint_nodes();
    reset_collect_skip_counters();
    crate::style::utility::set_frame_dt(1.0 / 60.0);

    let mut title = Element::new()
        .w(400.0)
        .h(24.0)
        .flex_shrink(0.0)
        .child(Element::new().text("基础"));
    title.set_attr("data-slot", "title");
    let mut desc = Element::new()
        .w(400.0)
        .h(20.0)
        .flex_shrink(0.0)
        .child(Element::new().text("Button 变体 / 尺寸 / hover 缓动 · Badge · Kbd · Marker"));
    desc.set_attr("data-slot", "desc");
    let header = Element::new()
        .flex_col()
        .w(400.0)
        .h(52.0)
        .flex_shrink(0.0)
        .bg([1.0, 1.0, 1.0, 1.0])
        .child(title)
        .child(desc);

    let mut shimmer_caption = Element::new()
        .w(400.0)
        .h(18.0)
        .flex_shrink(0.0)
        .child(Element::new().text("Shimmer (streaming text)"));
    shimmer_caption.set_attr("data-slot", "shimmer-caption");
    let mut thinking: Element = marker()
        .content_with(|c| c.class("shimmer").text("Thinking..."))
        .into();
    thinking.set_attr("data-slot", "thinking-marker");
    let mut reading: Element = marker()
        .variant("separator")
        .content_with(|c| c.class("shimmer").text("Reading 4 files"))
        .into();
    reading.set_attr("data-slot", "reading-marker");
    let mut generating = Element::new()
        .w("100%")
        .h(20.0)
        .flex_shrink(0.0)
        .text("Generating response...");
    apply_class_list(&mut generating, "shimmer shimmer-duration-1000");
    generating.set_attr("data-slot", "generating");
    generating.set_attr(crate::renderer::ATTR_SHIMMER_EPOCH, "-0.5");
    let mut box_el = Element::new()
        .w("100%")
        .h(32.0)
        .flex_shrink(0.0)
        .bg([0.90, 0.90, 0.92, 1.0])
        .child(Element::new().text("shimmer-box · overlay on container"));
    apply_class_list(&mut box_el, "shimmer-box shimmer-duration-1600");
    box_el.set_attr("data-slot", "shimmer-box");
    box_el.set_attr(crate::renderer::ATTR_SHIMMER_EPOCH, "-0.5");
    let mut border_cap = Element::new()
        .w(400.0)
        .h(18.0)
        .flex_shrink(0.0)
        .child(Element::new().text("Border"));
    border_cap.set_attr("data-slot", "border-cap");
    let mut border_a: Element = marker()
        .variant("border")
        .content_with(|c| c.text("Switched to release-candidate"))
        .into();
    border_a.set_attr("data-slot", "border-a");
    let mut border_b: Element = marker()
        .variant("border")
        .content_with(|c| c.text("Opened implementation notes"))
        .into();
    border_b.set_attr("data-slot", "border-b");

    let mut marker_block = Element::new()
        .flex_col()
        .w("100%")
        .gap(8.0)
        .flex_shrink(0.0)
        .child(shimmer_caption)
        .child(thinking)
        .child(reading)
        .child(generating)
        .child(box_el)
        .child(border_cap)
        .child(border_a)
        .child(border_b);
    marker_block.set_attr("data-slot", "marker-block");

    let mut ms_title = Element::new()
        .w(400.0)
        .h(28.0)
        .flex_shrink(0.0)
        .child(Element::new().text("1c-ms) MessageScroller (ShadCN conversation scroller)"));
    ms_title.set_attr("data-slot", "ms-title");
    let mut ms_desc = Element::new().w(400.0).h(36.0).flex_shrink(0.0).child(
        Element::new().text("Official chat card: New Chat header, muted-user bubbles (align=end)"),
    );
    ms_desc.set_attr("data-slot", "ms-desc");

    let column = Element::new()
        .flex_col()
        .w("100%")
        .child(marker_block)
        .child(ms_title)
        .child(ms_desc)
        .child(Element::new().w(400.0).h(400.0).flex_shrink(0.0));

    let scroll_id = "gal-straddle-marker-ms";
    use_signal(&scroll_offset_y_signal_key(scroll_id), || 0.0_f32).set_silent(0.0);
    let mut body = Element::from(
        crate::components::scroll_area::scroll_area(scroll_id)
            .w(400.0)
            .h(320.0)
            .overflow_y("auto")
            .overflow_x("hidden")
            .child(column),
    );
    body.set_attr("data-slot", "scroll-body");

    let mut root = Element::new()
        .flex_col()
        .w(400.0)
        .h(372.0)
        .bg([0.94, 0.94, 0.95, 1.0])
        .child(header)
        .child(body);

    let mut tree = LayoutTree::new();
    let first = tree.build_and_layout(&mut root, 400.0, 372.0, [-1.0, -1.0]);
    let think_rest_y = first
        .text_nodes
        .iter()
        .find(|n| n.text.contains("Thinking"))
        .map(|n| n.y)
        .unwrap_or(80.0);
    let ms_rest_y = first
        .text_nodes
        .iter()
        .find(|n| n.text.contains("1c-ms)"))
        .map(|n| n.y)
        .unwrap_or(think_rest_y + 200.0);
    let prev_slots = last_present_visual_slots(
        &first.node_paints,
        &first.instances,
        &[],
        &first.text_nodes,
        &first.input_chrome_instances,
    );
    let _ = take_frame_paint_nodes();
    tree.set_collect_skip_allowed(true);
    let _warm = tree.build_and_layout_fast(&mut root, 400.0, 372.0, [-1.0, -1.0], true, false);

    // Land `1c-ms)` on Thinking's rest Y while Marker still straddles.
    let scroll_y = (ms_rest_y - think_rest_y).max(48.0);
    apply_viewport_scroll_y(&mut root, scroll_id, scroll_y);

    crate::style::utility::set_frame_dt(0.125);
    let mut impact = LayoutStatus::Clean;
    tick_active_css_transitions_subtree(
        &mut root,
        [0.0, 0.0],
        "",
        false,
        400.0,
        372.0,
        [0.0, 0.0],
        &mut tree,
        &mut impact,
    );
    sync_class_variants_subtree(
        &mut root,
        [0.0, 0.0],
        "",
        false,
        400.0,
        372.0,
        &mut tree,
        &mut impact,
    );
    let mut dirty = take_frame_paint_nodes();
    tree.set_collect_skip_allowed(true);
    let collected = tree.build_and_layout_fast(&mut root, 400.0, 372.0, [-1.0, -1.0], true, false);
    let now_slots = last_present_visual_slots(
        &collected.node_paints,
        &collected.instances,
        &[],
        &collected.text_nodes,
        &collected.input_chrome_instances,
    );
    expand_paint_dirties_for_translated_slots(&mut dirty, &now_slots, &prev_slots);
    expand_paint_dirties_for_clip_culled_slots(
        &mut dirty,
        &now_slots,
        &prev_slots,
        tree.last_clip_culled_paint_keys(),
    );
    escalate_dirties_for_node_present(&mut dirty, &collected.node_paints, &prev_slots);

    let body_el = find_slot(&root, "scroll-body").expect("scroll-body");
    let ms = find_slot(&root, "ms-title").expect("ms-title");
    let thinking = find_slot(&root, "thinking-marker").expect("thinking-marker");
    let marker_block = find_slot(&root, "marker-block").expect("marker-block");
    let body_b = body_el.layout_bounds;
    let ms_b = ms.layout_bounds;
    let think_vis = visual_paint_aabb(thinking, thinking.paint_scroll_acc);
    let marker_vis = visual_paint_aabb(marker_block, marker_block.paint_scroll_acc);
    let ms_text = collected
        .text_nodes
        .iter()
        .find(|n| n.text.contains("1c-ms)"))
        .unwrap_or_else(|| {
            panic!(
                "MessageScroller title collected\n texts={:?}",
                collected
                    .text_nodes
                    .iter()
                    .map(|n| format!("{:?} y={:.1}", n.text, n.y))
                    .collect::<Vec<_>>()
            )
        });
    let ms_cy = ms_text.y + ms_text.height * 0.5;
    let ms_cx = ms_text.x + 48.0;

    let dump = format!(
        "scroll={scroll_y:.1} body={} marker_vis={} think_vis={} ms_layout={} ms_cy={ms_cy:.1}\n culled={:?}\n dirties={:?}\n texts={:?}\n skipped={:?}",
        fmt_rect(body_b),
        fmt_rect(marker_vis),
        fmt_rect(think_vis),
        fmt_rect(ms_b),
        tree.last_clip_culled_paint_keys().len(),
        dirty
            .iter()
            .map(|d| format!("{:?} {} {}", d.kind, d.key, fmt_rect(d.bounds)))
            .collect::<Vec<_>>(),
        collected
            .text_nodes
            .iter()
            .map(|n| format!("{:?} y={:.1} x={:.1}", n.text, n.y, n.x))
            .collect::<Vec<_>>(),
        tree.last_collect_skip_counts()
    );

    assert!(
        marker_vis[1] + marker_vis[3] > body_b[1] + 8.0,
        "fixture: Marker card must still straddle the clip (bottom in view)\n{dump}"
    );
    assert!(
        collected
            .text_nodes
            .iter()
            .all(|n| { !n.text.contains("Thinking") || n.y + n.height <= body_b[1] + 1.0 }),
        "fixture: Thinking glyphs must have left the live edge (top of Marker culled)\n{dump}"
    );
    assert!(
        ms_cy >= body_b[1] - 1.0 && ms_cy < body_b[1] + body_b[3],
        "fixture: MessageScroller title must be in the viewport while Marker straddles\n{dump}"
    );

    let culled_pins = clip_culled_scroll_blit_pins(
        &prev_slots,
        tree.last_clip_culled_paint_keys(),
        &[body_b],
        &[[0.0, scroll_y]],
    );
    assert!(
        !culled_pins.is_empty()
            && culled_pins
                .iter()
                .any(|p| p[1] <= ms_cy && ms_cy < p[1] + p[3]),
        "scroll blit must pin clip-culled Marker leftovers on the live edge, not wait until the card fully leaves\n pins={culled_pins:?}\n{dump}"
    );

    for needle in [
        "Thinking",
        "Reading 4 files",
        "Generating response",
        "shimmer-box",
        "Shimmer (streaming text)",
    ] {
        let hits: Vec<_> = collected
            .text_nodes
            .iter()
            .filter(|n| n.text.contains(needle))
            .collect();
        assert!(
            hits.len() <= 1,
            "skip-ghost + live double-paint of {needle:?} (叠字)\n{dump}"
        );
        for n in &hits {
            let box_ = [n.x, n.y, n.width.max(1.0), n.height.max(1.0)];
            assert!(
                !rect_contains_point(box_, ms_cx, ms_cy) && (n.y - ms_cy).abs() >= 14.0,
                "straddling Marker {needle:?} painted onto 1c-ms) MessageScroller (错位)\n y={:.1} ms_cy={ms_cy:.1}\n{dump}",
                n.y
            );
        }
    }

    let hole_dirties: Vec<_> = dirty
        .iter()
        .copied()
        .filter(|d| d.kind != PaintDirtyKind::Overlay)
        .filter(|d| {
            let s = node_present_scissor_logical(
                d.kind,
                d.key,
                d.bounds,
                &prev_slots,
                &collected.node_paints,
                node_present_scissor_pad(d.kind),
            );
            s[2] > 0.5 && s[3] > 0.5
        })
        .collect();
    let wipe_clips: Vec<(u64, [f32; 4])> = hole_dirties
        .iter()
        .filter_map(|d| {
            let prev = last_present_prev_for_dirty(d.kind, d.key, &prev_slots);
            let wipe = node_present_dest_reset_full(
                d.key,
                d.bounds,
                &collected.node_paints,
                &collected.instances,
                &[],
                &[],
                1.0,
                prev,
            )?
            .rect;
            Some((d.key, node_present_glyph_clip_logical(d, Some(wipe), 1.0)))
        })
        .collect();
    let capture = node_present_clipped_capture_text_wipes(
        &hole_dirties,
        &collected.text_nodes,
        &collected.node_paints,
        &wipe_clips,
    );
    for n in &capture {
        let stamp = n.text.contains("Thinking")
            || n.text.contains("Reading")
            || n.text.contains("Generating")
            || n.text.contains("shimmer-box")
            || n.text.contains("Shimmer (streaming");
        if !stamp {
            continue;
        }
        if let Some(clip) = n.clip_bounds {
            assert!(
                !rect_contains_point(clip, ms_cx, ms_cy),
                "capture clip stamps {:?} onto 1c-ms) MessageScroller\n clip={}\n{dump}",
                n.text,
                fmt_rect(clip)
            );
        }
    }
}

/// Gallery 基础: after the Message card (Builder / On it / B / Delivered) scrolls
/// out, `1c-ms) MessageScroller` sits at the live edge. Skip-replay / last-present
/// dest-reset used to stamp those bubble glyphs onto the section title.
#[test]
fn scrolled_out_message_bubble_must_not_stamp_live_edge_message_scroller() {
    let _ = take_frame_paint_nodes();
    reset_collect_skip_counters();

    let mut desc = Element::new()
        .w(400.0)
        .h(20.0)
        .flex_shrink(0.0)
        .child(Element::new().text("Button 变体 / 尺寸 / hover 缓动 · Badge · Kbd · Marker"));
    desc.set_attr("data-slot", "desc");
    let mut title = Element::new()
        .w(400.0)
        .h(24.0)
        .flex_shrink(0.0)
        .child(Element::new().text("基础"));
    title.set_attr("data-slot", "title");
    let header = Element::new()
        .flex_col()
        .w(400.0)
        .h(52.0)
        .flex_shrink(0.0)
        .bg([1.0, 1.0, 1.0, 1.0])
        .child(title)
        .child(desc);

    let end_row: Element = message()
        .align("end")
        .avatar(
            Element::new()
                .w(28.0)
                .h(28.0)
                .bg([0.85, 0.85, 0.88, 1.0])
                .child(Element::new().text("B")),
        )
        .header("Builder")
        .content_child(
            bubble()
                .variant("secondary")
                .align("end")
                .child(bubble_content().text("On it, one-line change.")),
        )
        .footer("Delivered")
        .into();

    let mut message_block = Element::new()
        .flex_col()
        .w("100%")
        .gap(8.0)
        .flex_shrink(0.0)
        .child(end_row);
    message_block.set_attr("data-slot", "message-block");

    let mut ms_title = Element::new()
        .w(400.0)
        .h(28.0)
        .flex_shrink(0.0)
        .child(Element::new().text("1c-ms) MessageScroller (ShadCN conversation scroller)"));
    ms_title.set_attr("data-slot", "ms-title");

    let column = Element::new()
        .flex_col()
        .w("100%")
        .child(message_block)
        .child(ms_title)
        .child(Element::new().w(400.0).h(400.0).flex_shrink(0.0));

    let scroll_id = "gal-message-live-edge-stamp";
    use_signal(&scroll_offset_y_signal_key(scroll_id), || 0.0_f32).set_silent(0.0);
    let mut body = Element::from(
        crate::components::scroll_area::scroll_area(scroll_id)
            .w(400.0)
            .h(160.0)
            .overflow_y("auto")
            .overflow_x("hidden")
            .child(column),
    );
    body.set_attr("data-slot", "scroll-body");

    let mut root = Element::new()
        .flex_col()
        .w(400.0)
        .h(212.0)
        .bg([0.94, 0.94, 0.95, 1.0])
        .child(header)
        .child(body);

    let mut tree = LayoutTree::new();
    let first = tree.build_and_layout(&mut root, 400.0, 212.0, [-1.0, -1.0]);
    let prev_slots = last_present_visual_slots(
        &first.node_paints,
        &first.instances,
        &[],
        &first.text_nodes,
        &first.input_chrome_instances,
    );
    let _ = take_frame_paint_nodes();
    tree.set_collect_skip_allowed(true);
    let _warm = tree.build_and_layout_fast(&mut root, 400.0, 212.0, [-1.0, -1.0], true, false);

    let message_block = find_slot(&root, "message-block").expect("message-block");
    let scroll_out = (message_block.layout_bounds[3] + 8.0).max(48.0);
    apply_viewport_scroll_y(&mut root, scroll_id, scroll_out);

    tree.set_collect_skip_allowed(true);
    let collected = tree.build_and_layout_fast(&mut root, 400.0, 212.0, [-1.0, -1.0], true, false);
    let now_slots = last_present_visual_slots(
        &collected.node_paints,
        &collected.instances,
        &[],
        &collected.text_nodes,
        &collected.input_chrome_instances,
    );
    let mut dirty = take_frame_paint_nodes();
    expand_paint_dirties_for_translated_slots(&mut dirty, &now_slots, &prev_slots);
    expand_paint_dirties_for_clip_culled_slots(
        &mut dirty,
        &now_slots,
        &prev_slots,
        tree.last_clip_culled_paint_keys(),
    );
    escalate_dirties_for_node_present(&mut dirty, &collected.node_paints, &prev_slots);

    let desc = find_slot(&root, "desc").expect("desc");
    let body_el = find_slot(&root, "scroll-body").expect("scroll-body");
    let desc_b = desc.layout_bounds;
    let body_b = body_el.layout_bounds;
    let desc_cx = desc_b[0] + 48.0;
    let desc_cy = desc_b[1] + desc_b[3] * 0.5;
    let ms = collected
        .text_nodes
        .iter()
        .find(|n| n.text.contains("1c-ms)"))
        .expect("MessageScroller title collected at live edge");
    let ms_cx = ms.x + 48.0;
    let ms_cy = ms.y + ms.height * 0.5;
    let ms_box = [ms.x, ms.y, ms.width.max(1.0), ms.height.max(1.0)];

    let dump = format!(
        "scroll={scroll_out:.1} desc={} body={} ms_cy={ms_cy:.1} ms={}\n dirties={:?}\n texts={:?}\n skipped={:?}",
        fmt_rect(desc_b),
        fmt_rect(body_b),
        fmt_rect(ms_box),
        dirty
            .iter()
            .map(|d| format!("{:?} {} {}", d.kind, d.key, fmt_rect(d.bounds)))
            .collect::<Vec<_>>(),
        collected
            .text_nodes
            .iter()
            .map(|n| format!("{:?} y={:.1} x={:.1} clip={:?}", n.text, n.y, n.x, n.clip_bounds.map(fmt_rect)))
            .collect::<Vec<_>>(),
        tree.last_collect_skip_counts()
    );

    assert!(
        ms_cy >= body_b[1] - 1.0 && ms_cy < body_b[1] + 40.0,
        "fixture: MessageScroller title must sit at the live edge after Message scrolls out\n{dump}"
    );

    for needle in ["On it, one-line", "Delivered", "Builder"] {
        let hits: Vec<_> = collected
            .text_nodes
            .iter()
            .filter(|n| n.text.contains(needle))
            .collect();
        assert!(
            hits.len() <= 1,
            "skip-ghost + live double-paint of {needle:?} (叠字)\n{dump}"
        );
        for n in &hits {
            let box_ = [n.x, n.y, n.width.max(1.0), n.height.max(1.0)];
            assert!(
                n.y + n.height <= body_b[1] + 1.0 || n.y >= body_b[1] + body_b[3] - 1.0,
                "Message {needle:?} still painted after scrolling out of the message block\n y={:.1} h={:.1}\n{dump}",
                n.y,
                n.height
            );
            assert!(
                !rect_contains_point(box_, ms_cx, ms_cy) && (n.y - ms_cy).abs() >= 14.0,
                "Message {needle:?} skip-replayed onto 1c-ms) MessageScroller (错位)\n y={:.1} ms_cy={ms_cy:.1}\n{dump}",
                n.y
            );
            assert!(
                n.y + n.height <= desc_b[1] + 0.5 || n.y >= body_b[1] - 0.5,
                "Message {needle:?} stamped into the section description\n y={:.1}\n{dump}",
                n.y
            );
        }
    }
    let b_hits: Vec<_> = collected
        .text_nodes
        .iter()
        .filter(|n| n.text.trim() == "B")
        .collect();
    for n in &b_hits {
        assert!(
            (n.y - ms_cy).abs() >= 14.0,
            "Avatar B skip-replayed onto 1c-ms) MessageScroller (错位)\n y={:.1} ms_cy={ms_cy:.1}\n{dump}",
            n.y
        );
        assert!(
            n.y + n.height <= desc_b[1] + 0.5 || n.y >= body_b[1] - 0.5,
            "Avatar B stamped into the section description\n y={:.1}\n{dump}",
            n.y
        );
    }

    let live_band = [body_b[0], body_b[1], body_b[2], 48.0];
    let hole_dirties: Vec<_> = dirty
        .iter()
        .copied()
        .filter(|d| d.kind != PaintDirtyKind::Overlay)
        .filter(|d| {
            let s = node_present_scissor_logical(
                d.kind,
                d.key,
                d.bounds,
                &prev_slots,
                &collected.node_paints,
                node_present_scissor_pad(d.kind),
            );
            s[2] > 0.5 && s[3] > 0.5
        })
        .collect();
    let wipe_clips: Vec<(u64, [f32; 4])> = hole_dirties
        .iter()
        .filter(|d| d.kind == PaintDirtyKind::Node)
        .filter_map(|d| {
            let prev = last_present_prev_for_dirty(d.kind, d.key, &prev_slots);
            let wipe = node_present_dest_reset_full(
                d.key,
                d.bounds,
                &collected.node_paints,
                &collected.instances,
                &[],
                &[],
                1.0,
                prev,
            )?
            .rect;
            Some((d.key, node_present_glyph_clip_logical(d, Some(wipe), 1.0)))
        })
        .collect();
    let capture = node_present_clipped_capture_text_wipes(
        &hole_dirties,
        &collected.text_nodes,
        &collected.node_paints,
        &wipe_clips,
    );
    for n in &capture {
        let stamp = n.text.contains("On it")
            || n.text.contains("Delivered")
            || n.text.trim() == "B"
            || n.text.contains("Builder");
        if !stamp {
            continue;
        }
        assert!(
            (n.y - ms_cy).abs() >= 14.0,
            "Message glyph recaptured onto 1c-ms) MessageScroller\n text={:?} y={:.1}\n{dump}",
            n.text,
            n.y
        );
        if let Some(clip) = n.clip_bounds {
            assert!(
                !rect_contains_point(clip, ms_cx, ms_cy),
                "capture clip stamps {:?} onto 1c-ms) MessageScroller\n clip={}\n{dump}",
                n.text,
                fmt_rect(clip)
            );
            assert!(
                !rect_contains_point(clip, desc_cx, desc_cy),
                "capture clip stamps {:?} into the section description\n clip={}\n{dump}",
                n.text,
                fmt_rect(clip)
            );
        }
    }

    let first_on_it = first
        .text_nodes
        .iter()
        .find(|n| n.text.contains("On it, one-line"))
        .expect("On it present before scroll");
    let first_delivered = first
        .text_nodes
        .iter()
        .find(|n| n.text.contains("Delivered"))
        .expect("Delivered present before scroll");
    for (label, old) in [("On it", first_on_it), ("Delivered", first_delivered)] {
        let ghost = collected.text_nodes.iter().any(|n| {
            n.text.contains(old.text.trim())
                && (n.x - old.x).abs() < 2.0
                && (n.y - old.y).abs() < 2.0
        });
        assert!(
            !ghost,
            "{label} skip-replayed at the pre-scroll Y after scrolling out (错位)\n old=[{:.1},{:.1}]\n{dump}",
            old.x,
            old.y
        );
        let at_live = collected.text_nodes.iter().any(|n| {
            (n.text.contains("On it") || n.text.contains("Delivered") || n.text.trim() == "B")
                && n.text
                    .contains(old.text.split_whitespace().next().unwrap_or(""))
                && (n.y - ms_cy).abs() < 16.0
        });
        assert!(
            !at_live,
            "{label} translated onto the live-edge MessageScroller title\n{dump}"
        );
    }

    for (key, prev) in &prev_slots {
        if now_slots.iter().any(|(k, _)| k == key) {
            continue;
        }
        let prev_cy = prev[1] + prev[3] * 0.5;
        if prev_cy < body_b[1] - 0.5 || prev_cy > body_b[1] + 48.0 {
            continue;
        }
        if intersect_rects(*prev, live_band).is_none() {
            continue;
        }
        if intersect_rects(*prev, ms_box).is_none() && (prev_cy - ms_cy).abs() > 18.0 {
            continue;
        }
        let named = dirty
            .iter()
            .any(|d| d.key == *key && d.kind != PaintDirtyKind::Overlay);
        assert!(
            named,
            "clip-culled message last-present at the live edge must be dest-reset (else Delivered/On it stay in retain)\n key={key} prev={}\n{dump}",
            fmt_rect(*prev)
        );
        let d = dirty
            .iter()
            .find(|d| d.key == *key)
            .expect("named vanished");
        let (wipe, _) = node_present_dest_reset_scaled(
            d.key,
            d.bounds,
            &collected.node_paints,
            &collected.instances,
            &[],
            1.0,
            Some(*prev),
        )
        .expect("culled message last-present at the live edge must wipe retain");
        assert!(
            intersect_rects(wipe, *prev).is_some(),
            "vanished-message dest-reset must cover last-present\n wipe={} prev={}\n{dump}",
            fmt_rect(wipe),
            fmt_rect(*prev)
        );
        assert!(
            !rect_contains_point(wipe, desc_cx, desc_cy),
            "vanished-message dest-reset punches the section description\n wipe={}\n{dump}",
            fmt_rect(wipe)
        );
    }
}

/// Gallery animation row: absolute ping is layered — dest-reset must wipe the
/// caption overlap and restore glyphs (else PartialLoad stains the title).
/// Bounce stays in-flow and must not punch its caption. Pulse opacity must keep
/// a full wipe (including vertex AA) when a grown ping neighbor only overlaps visually.
#[test]
fn gallery_ping_bounce_pulse_dest_reset_must_not_punch_captions() {
    let _ = take_frame_paint_nodes();
    crate::style::utility::set_frame_dt(1.0 / 60.0);

    let mut ping_label = Element::new().h(14.0).flex_shrink(0.0).text("ping");
    ping_label.set_attr("data-slot", "ping-label");
    let mut ping_core = Element::new().w(32.0).h(32.0).bg([0.2, 0.2, 0.85, 1.0]);
    apply_class_list(&mut ping_core, "rounded-full");
    let mut ping = Element::new()
        .w(32.0)
        .h(32.0)
        .absolute()
        .inset(0.0)
        .bg([0.2, 0.2, 0.85, 1.0]);
    apply_class_list(&mut ping, "rounded-full animate-ping");
    ping.set_attr("data-slot", "ping-box");
    let ping_host = Element::new()
        .w(32.0)
        .h(32.0)
        .relative()
        .flex_shrink(0.0)
        .items_center()
        .justify_center()
        .child(ping_core)
        .child(ping);
    // Gallery: 4rem overflow-hidden stage so scale-2 ink cannot src-over the caption.
    let ping_stage = Element::new()
        .w(64.0)
        .h(64.0)
        .flex_shrink(0.0)
        .items_center()
        .justify_center()
        .overflow("hidden")
        .child(ping_host);
    let ping_col = Element::new()
        .flex_col()
        .gap(6.0)
        .flex_shrink(0.0)
        .child(ping_label)
        .child(ping_stage);

    let mut bounce_label = Element::new().h(14.0).flex_shrink(0.0).text("bounce");
    bounce_label.set_attr("data-slot", "bounce-label");
    let mut bounce = Element::new()
        .w(32.0)
        .h(32.0)
        .flex_shrink(0.0)
        .bg([0.92, 0.92, 0.94, 1.0]);
    apply_class_list(&mut bounce, "rounded-md animate-bounce");
    bounce.set_attr("data-slot", "bounce-box");
    let bounce_col = Element::new()
        .flex_col()
        .gap(6.0)
        .flex_shrink(0.0)
        .child(bounce_label)
        .child(bounce);

    let mut pulse_label = Element::new().h(14.0).flex_shrink(0.0).text("pulse");
    pulse_label.set_attr("data-slot", "pulse-label");
    let mut pulse = Element::new()
        .w(48.0)
        .h(32.0)
        .flex_shrink(0.0)
        .bg([0.55, 0.35, 0.85, 1.0]);
    apply_class_list(&mut pulse, "rounded-md animate-pulse");
    pulse.set_attr("data-slot", "pulse-box");
    let pulse_col = Element::new()
        .flex_col()
        .gap(6.0)
        .flex_shrink(0.0)
        .child(pulse_label)
        .child(pulse);

    let mut root = Element::new()
        .flex_row()
        .gap(20.0)
        .w(400.0)
        .h(140.0)
        .bg([0.96, 0.96, 0.96, 1.0])
        .child(ping_col)
        .child(pulse_col)
        .child(bounce_col);
    let mut tree = LayoutTree::new();
    let _first = tree.build_and_layout(&mut root, 400.0, 140.0, [-1.0, -1.0]);
    let _ = take_frame_paint_nodes();

    // CSS ticks clamp dt to 80ms; 10× that lands on ping's 75% scale-2 stop.
    crate::style::utility::set_frame_dt(0.08);
    let mut impact = LayoutStatus::Clean;
    for _ in 0..10 {
        tick_active_css_transitions_subtree(
            &mut root,
            [0.0, 0.0],
            "",
            false,
            400.0,
            140.0,
            [0.0, 0.0],
            &mut tree,
            &mut impact,
        );
    }
    let dirty = take_frame_paint_nodes();
    let later = tree.build_and_layout_fast(&mut root, 400.0, 140.0, [-1.0, -1.0], false, false);

    let ping_el = find_slot(&root, "ping-box").expect("ping");
    let ping_label_el = find_slot(&root, "ping-label").expect("ping label");
    let bounce_el = find_slot(&root, "bounce-box").expect("bounce");
    let bounce_label_el = find_slot(&root, "bounce-label").expect("bounce label");
    let pulse_el = find_slot(&root, "pulse-box").expect("pulse");

    let ping_key = paint_element_key(ping_el);
    let bounce_key = paint_element_key(bounce_el);
    let pulse_key = paint_element_key(pulse_el);

    let ping_visual = visual_paint_aabb(ping_el, [0.0, 0.0]);
    assert!(
        ping_visual[2] > 40.0 && ping_visual[3] > 40.0,
        "ping at 0.75s must be scale-2, got {}",
        fmt_rect(ping_visual)
    );

    let ping_paint = later
        .node_paints
        .iter()
        .find(|p| p.key == ping_key)
        .expect("ping NodePaint");
    assert!(
        ping_paint.rest.is_some(),
        "ping collect must record the unscaled Taffy box"
    );
    assert!(
        ping_paint.layered,
        "gallery ping overlay is position:absolute (layered stacking)"
    );

    let ping_dirty = dirty
        .iter()
        .find(|d| d.kind == PaintDirtyKind::Node && d.key == ping_key)
        .expect("ping node-present dirty");
    let (ping_wipe, _) = node_present_dest_reset_scaled(
        ping_dirty.key,
        ping_dirty.bounds,
        &later.node_paints,
        &later.instances,
        &[],
        1.0,
        Some(ping_visual),
    )
    .expect("card behind ping");
    let pl = ping_label_el.layout_bounds;
    assert!(
        intersect_rects(ping_wipe, pl).is_none(),
        "layered ping dest-reset must not punch the caption\n wipe={} label={}",
        fmt_rect(ping_wipe),
        fmt_rect(pl)
    );
    let ping_scissor = node_present_scissor_logical(
        PaintDirtyKind::Node,
        ping_dirty.key,
        ping_dirty.bounds,
        &[(ping_key, ping_visual)],
        &later.node_paints,
        8.0,
    );
    assert!(
        intersect_rects(ping_scissor, pl).is_none(),
        "layered ping scissor must not paint the caption\n scissor={} label={}",
        fmt_rect(ping_scissor),
        fmt_rect(pl)
    );
    let encode_scissor = crate::app::damage::union_rects(ping_scissor, ping_wipe);
    assert!(
        intersect_rects(encode_scissor, pl).is_none(),
        "GPU scissor (dest-reset ∪ node-present) must not paint the caption\n scissor={} label={}",
        fmt_rect(encode_scissor),
        fmt_rect(pl)
    );

    if let Some(bounce_dirty) = dirty
        .iter()
        .find(|d| d.kind == PaintDirtyKind::Node && d.key == bounce_key)
    {
        let bounce_visual = visual_paint_aabb(bounce_el, [0.0, 0.0]);
        let (bounce_wipe, _) = node_present_dest_reset_scaled(
            bounce_dirty.key,
            bounce_dirty.bounds,
            &later.node_paints,
            &later.instances,
            &[],
            1.0,
            Some(bounce_visual),
        )
        .expect("card behind bounce");
        let bl = bounce_label_el.layout_bounds;
        assert!(
            !rect_contains_point(bounce_wipe, bl[0] + bl[2] * 0.5, bl[1] + bl[3] * 0.5),
            "bounce dest-reset must not punch the caption\n wipe={} label={}",
            fmt_rect(bounce_wipe),
            fmt_rect(bl)
        );
    }

    let pulse_dirty = dirty
        .iter()
        .find(|d| d.kind == PaintDirtyKind::Node && d.key == pulse_key)
        .expect("pulse node-present dirty");
    let pulse_box = pulse_el.layout_bounds;
    let (pulse_wipe, _) = node_present_dest_reset_scaled(
        pulse_dirty.key,
        pulse_dirty.bounds,
        &later.node_paints,
        &later.instances,
        &[],
        1.0,
        Some(pulse_box),
    )
    .expect("card behind pulse");
    assert!(
        pulse_wipe[0] <= pulse_box[0] - 1.5
            && pulse_wipe[1] <= pulse_box[1] - 1.5
            && pulse_wipe[0] + pulse_wipe[2] >= pulse_box[0] + pulse_box[2] + 1.5
            && pulse_wipe[1] + pulse_wipe[3] >= pulse_box[1] + pulse_box[3] + 1.5,
        "pulse dest-reset must cover the rounded box plus vertex AA\n wipe={} pulse={}",
        fmt_rect(pulse_wipe),
        fmt_rect(pulse_box)
    );
}

#[test]
fn gallery_play_state_pause_dest_reset_must_cover_spin_circumcircle() {
    let _ = take_frame_paint_nodes();
    crate::style::utility::set_frame_dt(1.0 / 60.0);

    let mut label = Element::new().h(14.0).flex_shrink(0.0).text("play-state");
    label.set_attr("data-slot", "play-state-label");
    let mut spin = Element::new()
        .w(40.0)
        .h(40.0)
        .flex_shrink(0.0)
        .bg([0.1, 0.1, 0.12, 1.0]);
    apply_class_list(&mut spin, "rounded-md animate-spin");
    spin.set_attr("data-slot", "play-state-box");
    let stage = Element::new()
        .w(80.0)
        .h(80.0)
        .flex_shrink(0.0)
        .items_center()
        .justify_center()
        .child(spin);
    let mut btn = Element::new()
        .h(28.0)
        .w(48.0)
        .flex_shrink(0.0)
        .bg([1.0, 1.0, 1.0, 1.0])
        .text("Pause");
    btn.set_attr("data-slot", "play-state-btn");
    let col = Element::new()
        .flex_col()
        .gap(6.0)
        .flex_shrink(0.0)
        .items_center()
        .child(label)
        .child(stage)
        .child(btn);
    let mut fade = Element::new()
        .w(48.0)
        .h(32.0)
        .flex_shrink(0.0)
        .bg([0.9, 0.9, 0.92, 1.0]);
    apply_class_list(&mut fade, "rounded-md");
    fade.set_attr("data-slot", "fade-box");
    let mut root = Element::new()
        .flex_row()
        .gap(20.0)
        .w(280.0)
        .h(180.0)
        .bg([0.96, 0.96, 0.96, 1.0])
        .child(col)
        .child(fade);
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 280.0, 180.0, [-1.0, -1.0]);
    let _ = take_frame_paint_nodes();

    // 2×80ms ≈ 160ms of a 1s linear spin (~58°), past the 45° diamond.
    crate::style::utility::set_frame_dt(0.08);
    let mut impact = LayoutStatus::Clean;
    for _ in 0..2 {
        tick_active_css_transitions_subtree(
            &mut root,
            [0.0, 0.0],
            "",
            false,
            280.0,
            180.0,
            [0.0, 0.0],
            &mut tree,
            &mut impact,
        );
    }
    let spun = tree.build_and_layout_fast(&mut root, 280.0, 180.0, [-1.0, -1.0], false, false);
    let spin_el = find_slot(&root, "play-state-box").expect("play-state box");
    let spin_visual = visual_paint_aabb(spin_el, [0.0, 0.0]);
    let rest = spin_el.layout_bounds;
    assert!(
        spin_visual[2] > rest[2] + 4.0 && spin_visual[3] > rest[3] + 4.0,
        "spin must be rotated past identity, visual={} rest={}",
        fmt_rect(spin_visual),
        fmt_rect(rest)
    );
    let prev_slots = last_present_visual_slots(
        &spun.node_paints,
        &spun.instances,
        &[],
        &spun.text_nodes,
        &spun.input_chrome_instances,
    );

    let spin_mut = find_slot_mut(&mut root, "play-state-box").expect("play-state box");
    assert!(
        !spin_mut.style.animations.is_empty(),
        "play-state box must have a CSS animation"
    );
    spin_mut.style.animations[0].play_state = crate::style::AnimationPlayState::Paused;
    if let Some(btn_mut) = find_slot_mut(&mut root, "play-state-btn") {
        btn_mut.text = Some("Play".to_string());
    }
    let _ = take_frame_paint_nodes();
    impact = LayoutStatus::Clean;
    tick_active_css_transitions_subtree(
        &mut root,
        [0.0, 0.0],
        "",
        false,
        280.0,
        180.0,
        [0.0, 0.0],
        &mut tree,
        &mut impact,
    );
    let dirty = take_frame_paint_nodes();
    let paused = tree.build_and_layout_fast(&mut root, 280.0, 180.0, [-1.0, -1.0], false, false);
    let spin_el = find_slot(&root, "play-state-box").expect("play-state box");
    let spin_key = paint_element_key(spin_el);
    let spin_visual = visual_paint_aabb(spin_el, [0.0, 0.0]);
    let spin_dirty = dirty
        .iter()
        .find(|d| d.kind == PaintDirtyKind::Node && d.key == spin_key)
        .cloned();
    let d = spin_dirty.expect("pause must dest-reset the spinning box");
    let (wipe, _) = node_present_dest_reset_scaled(
        d.key,
        d.bounds,
        &paused.node_paints,
        &paused.instances,
        &[],
        1.0,
        last_present_prev_for_dirty(d.kind, d.key, &prev_slots),
    )
    .expect("card behind play-state spin");
    assert!(
        wipe[0] <= spin_visual[0] + 0.5
            && wipe[1] <= spin_visual[1] + 0.5
            && wipe[0] + wipe[2] >= spin_visual[0] + spin_visual[2] - 0.5
            && wipe[1] + wipe[3] >= spin_visual[1] + spin_visual[3] - 0.5,
        "pause dest-reset must cover the frozen diamond, not nested leftover frames\n wipe={} visual={} dirty={}",
        fmt_rect(wipe),
        fmt_rect(spin_visual),
        fmt_rect(d.bounds)
    );
    let label_el = find_slot(&root, "play-state-label").expect("play-state label");
    let pl = label_el.layout_bounds;
    assert!(
        intersect_rects(wipe, pl).is_none(),
        "pause dest-reset must not punch the caption\n wipe={} label={}",
        fmt_rect(wipe),
        fmt_rect(pl)
    );
    let scissor = node_present_scissor_logical(
        PaintDirtyKind::Node,
        d.key,
        d.bounds,
        &prev_slots,
        &paused.node_paints,
        8.0,
    );
    let encode = crate::app::damage::union_rects(scissor, wipe);
    assert!(
        encode[0] <= spin_visual[0] + 0.5
            && encode[1] <= spin_visual[1] + 0.5
            && encode[0] + encode[2] >= spin_visual[0] + spin_visual[2] - 0.5
            && encode[1] + encode[3] >= spin_visual[1] + spin_visual[3] - 0.5,
        "GPU scissor after pause must cover the diamond\n scissor={} wipe={} visual={}",
        fmt_rect(scissor),
        fmt_rect(wipe),
        fmt_rect(spin_visual)
    );
    assert!(
        intersect_rects(encode, pl).is_none(),
        "GPU scissor after pause must not dest-reset the caption\n scissor={} wipe={} label={}",
        fmt_rect(scissor),
        fmt_rect(wipe),
        fmt_rect(pl)
    );
}

#[test]
fn gallery_play_state_pause_must_not_hitch_sibling_css_animations() {
    // Click Play/Pause: play-state dest-reset must not punch bounce/fade, and
    // bounce must still advance on that frame (not a one-frame freeze).
    let _ = take_frame_paint_nodes();
    crate::style::utility::set_frame_dt(1.0 / 60.0);

    let mut bounce = Element::new()
        .w(32.0)
        .h(32.0)
        .flex_shrink(0.0)
        .bg([0.2, 0.6, 0.4, 1.0]);
    apply_class_list(&mut bounce, "rounded-md animate-spin");
    bounce.set_attr("data-slot", "bounce-box");

    let mut label = Element::new().h(14.0).flex_shrink(0.0).text("play-state");
    label.set_attr("data-slot", "play-state-label");
    let mut spin = Element::new()
        .w(40.0)
        .h(40.0)
        .flex_shrink(0.0)
        .bg([0.1, 0.1, 0.12, 1.0]);
    apply_class_list(&mut spin, "rounded-md animate-spin");
    spin.set_attr("data-slot", "play-state-box");
    let stage = Element::new()
        .w(80.0)
        .h(80.0)
        .flex_shrink(0.0)
        .items_center()
        .justify_center()
        .child(spin);
    let mut btn_wrap = Element::new().min_w(76.0).flex_shrink(0.0).items_center();
    let mut btn = Element::new()
        .h(28.0)
        .flex_shrink(0.0)
        .bg([1.0, 1.0, 1.0, 1.0])
        .text("Pause");
    btn.set_attr("data-slot", "play-state-btn");
    btn_wrap = btn_wrap.child(btn);
    let col = Element::new()
        .flex_col()
        .gap(6.0)
        .flex_shrink(0.0)
        .items_center()
        .child(label)
        .child(stage)
        .child(btn_wrap);
    let mut fade = Element::new()
        .w(48.0)
        .h(32.0)
        .flex_shrink(0.0)
        .bg([0.9, 0.9, 0.92, 1.0]);
    apply_class_list(&mut fade, "rounded-md animate-pulse");
    fade.set_attr("data-slot", "fade-box");

    let mut root = Element::new()
        .flex_row()
        .gap(20.0)
        .w(320.0)
        .h(180.0)
        .bg([0.96, 0.96, 0.96, 1.0])
        .child(bounce)
        .child(col)
        .child(fade);
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 320.0, 180.0, [-1.0, -1.0]);
    let _ = take_frame_paint_nodes();

    crate::style::utility::set_frame_dt(0.08);
    let mut impact = LayoutStatus::Clean;
    for _ in 0..3 {
        tick_active_css_transitions_subtree(
            &mut root,
            [0.0, 0.0],
            "",
            false,
            320.0,
            180.0,
            [0.0, 0.0],
            &mut tree,
            &mut impact,
        );
    }
    let bounce_deg = find_slot(&root, "bounce-box")
        .expect("bounce")
        .style
        .rotate_deg;
    assert!(
        bounce_deg.abs() > 4.0,
        "precondition: sibling spin must have left rest, rotate={bounce_deg}"
    );

    find_slot_mut(&mut root, "play-state-box")
        .expect("play-state box")
        .style
        .animations[0]
        .play_state = crate::style::AnimationPlayState::Paused;
    if let Some(btn_mut) = find_slot_mut(&mut root, "play-state-btn") {
        btn_mut.text = Some("Play".to_string());
    }
    let _ = take_frame_paint_nodes();
    impact = LayoutStatus::Clean;
    tick_active_css_transitions_subtree(
        &mut root,
        [0.0, 0.0],
        "",
        false,
        320.0,
        180.0,
        [0.0, 0.0],
        &mut tree,
        &mut impact,
    );
    let dirty = take_frame_paint_nodes();
    let paused = tree.build_and_layout_fast(&mut root, 320.0, 180.0, [-1.0, -1.0], false, false);

    let bounce_el = find_slot(&root, "bounce-box").expect("bounce");
    let bounce_deg2 = bounce_el.style.rotate_deg;
    assert!(
        (bounce_deg2 - bounce_deg).abs() > 2.0,
        "pause frame must still tick sibling spin, before={bounce_deg} after={bounce_deg2}"
    );

    let spin_el = find_slot(&root, "play-state-box").expect("play-state box");
    let spin_key = paint_element_key(spin_el);
    let spin_dirty = dirty
        .iter()
        .find(|d| d.kind == PaintDirtyKind::Node && d.key == spin_key)
        .cloned()
        .expect("pause must dest-reset the spinning box");
    let prev_slots = last_present_visual_slots(
        &paused.node_paints,
        &paused.instances,
        &[],
        &paused.text_nodes,
        &paused.input_chrome_instances,
    );
    let (wipe, _) = node_present_dest_reset_scaled(
        spin_dirty.key,
        spin_dirty.bounds,
        &paused.node_paints,
        &paused.instances,
        &[],
        1.0,
        last_present_prev_for_dirty(spin_dirty.kind, spin_dirty.key, &prev_slots),
    )
    .expect("card behind play-state spin");
    let scissor = node_present_scissor_logical(
        PaintDirtyKind::Node,
        spin_dirty.key,
        spin_dirty.bounds,
        &prev_slots,
        &paused.node_paints,
        8.0,
    );
    let encode = crate::app::damage::union_rects(scissor, wipe);
    let bounce_visual = visual_paint_aabb(bounce_el, [0.0, 0.0]);
    let fade_el = find_slot(&root, "fade-box").expect("fade");
    let fade_visual = fade_el.layout_bounds;
    assert!(
        intersect_rects(encode, bounce_visual).is_none(),
        "pause dest-reset must not punch bounce\n encode={} bounce={}",
        fmt_rect(encode),
        fmt_rect(bounce_visual)
    );
    assert!(
        intersect_rects(encode, fade_visual).is_none(),
        "pause dest-reset must not punch fade\n encode={} fade={}",
        fmt_rect(encode),
        fmt_rect(fade_visual)
    );
}

fn status_shimmer_marker() -> Element {
    let mut icon = Element::new().w(16.0).h(16.0).bg([0.15, 0.15, 0.15, 1.0]);
    apply_class_list(&mut icon, "animate-spin");
    marker()
        .role("status")
        .icon(icon)
        .content_with(|c| c.class("shimmer").text("Oliver is typing..."))
        .into()
}

fn caption_named<'a>(
    texts: &'a [crate::layout::layout_tree::TextLayoutInfo],
    needle: &str,
) -> Option<&'a crate::layout::layout_tree::TextLayoutInfo> {
    texts.iter().find(|n| n.text.contains(needle))
}

/// Gallery 基础「Status (Marker inside Message)」: shimmer caption must stay the
/// full 「Oliver is typing...」 while MarkerContent ticks. Nested Message
/// `min-w-0` / dest-reset used to clip it to 「Oliver i」 and flash the strip.
#[test]
fn message_status_shimmer_must_show_full_oliver_is_typing() {
    let _ = take_frame_paint_nodes();
    crate::style::utility::set_frame_dt(1.0 / 60.0);

    // Control: same Marker outside Message, same 28rem column as the gallery card.
    let mut standalone = Element::new()
        .flex_col()
        .w("100%")
        .min_w(0.0)
        .max_w(448.0)
        .child(status_shimmer_marker());
    let mut stand_tree = LayoutTree::new();
    let stand = stand_tree.build_and_layout(&mut standalone, 448.0, 64.0, [-1.0, -1.0]);
    let stand_caption =
        caption_named(&stand.text_nodes, "Oliver is typing").expect("standalone caption");
    let stand_w = stand_caption.width;

    let heading = Element::new().text("Status (Marker inside Message)");
    let mut root = Element::new()
        .flex_col()
        .w("100%")
        .min_w(0.0)
        .max_w(448.0)
        .gap(8.0)
        .child(heading)
        .child(message().child(status_shimmer_marker()));
    let mut tree = LayoutTree::new();
    let first = tree.build_and_layout(&mut root, 448.0, 80.0, [-1.0, -1.0]);
    let _ = take_frame_paint_nodes();
    let mut prev_slots = last_present_visual_slots(
        &first.node_paints,
        &first.instances,
        &[],
        &first.text_nodes,
        &first.input_chrome_instances,
    );

    let mut last_caption_w: Option<f32> = None;
    for i in 0..6 {
        crate::style::utility::set_frame_dt(1.0 / 60.0);
        let mut impact = LayoutStatus::Clean;
        tick_active_css_transitions_subtree(
            &mut root,
            [0.0, 0.0],
            "",
            false,
            448.0,
            80.0,
            [0.0, 0.0],
            &mut tree,
            &mut impact,
        );
        let dirty = take_frame_paint_nodes();
        let collected =
            tree.build_and_layout_fast(&mut root, 448.0, 80.0, [-1.0, -1.0], false, false);
        let now_slots = last_present_visual_slots(
            &collected.node_paints,
            &collected.instances,
            &[],
            &collected.text_nodes,
            &collected.input_chrome_instances,
        );

        let content = find_slot(&root, "marker-content").expect("marker-content");
        let marker_el = find_slot(&root, "marker").expect("marker");
        let caption = caption_named(&collected.text_nodes, "Oliver is typing");
        let caption_any = collected
            .text_nodes
            .iter()
            .filter(|n| n.text.contains("Oliver"))
            .map(|n| {
                format!(
                    "'{}' @{} clip={:?} fx={:?}",
                    n.text,
                    fmt_rect([n.x, n.y, n.width, n.height]),
                    n.clip_bounds.map(fmt_rect),
                    n.glyph_fx
                )
            })
            .collect::<Vec<_>>();

        let mut wipe_union: Option<[f32; 4]> = None;
        for d in dirty.iter().filter(|d| d.kind == PaintDirtyKind::Node) {
            let prev = last_present_prev_for_dirty(d.kind, d.key, &prev_slots);
            if let Some((wipe, _)) = node_present_dest_reset_scaled(
                d.key,
                d.bounds,
                &collected.node_paints,
                &collected.instances,
                &[],
                1.0,
                prev,
            ) {
                wipe_union = Some(match wipe_union {
                    Some(u) => union_xywh(u, wipe),
                    None => wipe,
                });
            }
        }

        let caption = caption.unwrap_or_else(|| {
            panic!(
                "full caption missing\ni={i} stand_w={stand_w:.1}\n marker={} content={}\n texts={:?}",
                fmt_rect(marker_el.layout_bounds),
                fmt_rect(content.layout_bounds),
                caption_any,
            )
        });
        let ink_w = caption
            .prepared
            .as_ref()
            .map(crate::text::intrinsic_width)
            .filter(|w| *w > 1.0)
            .unwrap_or(stand_w);
        let content_key = paint_element_key(content);
        let mut wipe_clips: Vec<(u64, [f32; 4])> = Vec::new();
        for d in dirty.iter().filter(|d| d.kind == PaintDirtyKind::Node) {
            let prev = last_present_prev_for_dirty(d.kind, d.key, &prev_slots);
            if let Some((wipe, _)) = node_present_dest_reset_scaled(
                d.key,
                d.bounds,
                &collected.node_paints,
                &collected.instances,
                &[],
                1.0,
                prev,
            ) {
                wipe_clips.push((d.key, node_present_glyph_clip_logical(d, Some(wipe), 1.0)));
            }
        }
        let capture = node_present_clipped_capture_text_wipes(
            &dirty,
            &collected.text_nodes,
            &collected.node_paints,
            &wipe_clips,
        );
        let capture_caption: Vec<_> = capture
            .iter()
            .filter(|n| n.text.contains("Oliver"))
            .map(|n| {
                format!(
                    "'{}' @{} clip={:?}",
                    n.text,
                    fmt_rect([n.x, n.y, n.width, n.height]),
                    n.clip_bounds.map(fmt_rect)
                )
            })
            .collect();
        let dump = format!(
            "i={i} stand_w={stand_w:.1} ink_w={ink_w:.1} ws={:?} overflow={:?} wrap={:?}\n marker={} content={}\n texts={:?}\n dirties={:?}\n wipe={:?} content_clip={:?}\n capture={:?}",
            caption.style.white_space,
            caption.style.overflow,
            caption.style.wrap_mode,
            fmt_rect(marker_el.layout_bounds),
            fmt_rect(content.layout_bounds),
            caption_any,
            dirty
                .iter()
                .map(|d| format!("{:?} k{} {}", d.kind, d.key, fmt_rect(d.bounds)))
                .collect::<Vec<_>>(),
            wipe_union.map(fmt_rect),
            wipe_clips
                .iter()
                .find(|(k, _)| *k == content_key)
                .map(|(_, c)| fmt_rect(*c)),
            capture_caption,
        );

        assert!(
            caption.text.contains("Oliver is typing"),
            "caption string truncated\n{dump}"
        );
        assert!(
            caption.glyph_fx.as_deref() == Some("shimmer"),
            "shimmer must land on the caption glyphs, not only the slot\n{dump}"
        );
        assert!(
            caption.width + 1.0 >= ink_w,
            "caption box narrower than 「Oliver is typing...」 ink (Oliver i)\n{dump}"
        );
        assert!(
            content.layout_bounds[2] + 1.0 >= ink_w,
            "marker-content slot narrower than the caption ink\n{dump}"
        );
        if let Some(clip) = caption.clip_bounds {
            assert!(
                clip[2] + 1.0 >= ink_w,
                "caption collect clip crops Oliver is typing...\n{dump}"
            );
        }
        let captured = capture
            .iter()
            .find(|n| n.text.contains("Oliver is typing"))
            .unwrap_or_else(|| panic!("dest-reset capture dropped the caption\n{dump}"));
        if let Some(clip) = captured.clip_bounds {
            assert!(
                clip[2] + 1.0 >= ink_w,
                "dest-reset capture clip crops to Oliver i and flashes\n{dump}"
            );
        }
        if let Some(w) = last_caption_w {
            assert!(
                (caption.width - w).abs() < 1.0,
                "caption width jitter across shimmer ticks (闪烁)\n{dump}"
            );
        }
        last_caption_w = Some(caption.width);
        prev_slots = now_slots;
    }
}
