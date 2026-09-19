//! CSS `animate-spin` must register a node-present dirty every tick.
//!
//! Gallery: typing in Kbd Search switches the frame to PartialLoad (Input is in
//! `damage_nodes`). Spin ticks only set `StyleChanged` + `request_redraw`, so the
//! Marker loader is not dest-reset/redrawn — retain keeps an old angle, then the
//! next FullClear snaps. That is the jump; sidebar ghosts are the same present
//! mode (Load without redrawing neighbors).

use crate::app::damage::{
    expand_paint_dirties_for_translated_slots, intersect_rects, paint_dirty_has_keyed_slot,
    paint_dirty_matches_present_slot, paint_dirty_nested_in_larger_node,
    paint_dirty_skips_node_dest_reset, paint_element_key, take_frame_paint_nodes, PaintDirtyKind,
};
use crate::app::draw_cull::{
    last_present_prev_for_dirty, last_present_visual_slots, node_paint_visual_bounds,
    node_present_clipped_capture_text, node_present_dest_reset_scaled,
    node_present_scissor_logical, slot_visual_exceeds_rest,
};
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
fn idle_sync_must_wake_spin_under_offscreen_live_parent() {
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
    // Stale chrome bounds after maximize: the wrapper is "below the window"
    // while the user still sees the icon (collect already used the new layout).
    root.layout_bounds = [0.0, 4000.0, 16.0, 32.0];
    root.children[0].layout_bounds = [0.0, 4010.0, 16.0, 16.0];

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
    if el.attrs.get("rui_animate").map(String::as_str) == Some("spin")
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
