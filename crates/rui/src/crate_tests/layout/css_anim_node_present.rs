//! CSS `animate-spin` must register a node-present dirty every tick.
//!
//! Gallery: typing in Kbd Search switches the frame to PartialLoad (Input is in
//! `damage_nodes`). Spin ticks only set `StyleChanged` + `request_redraw`, so the
//! Marker loader is not dest-reset/redrawn — retain keeps an old angle, then the
//! next FullClear snaps. That is the jump; sidebar ghosts are the same present
//! mode (Load without redrawing neighbors).

use crate::app::damage::{
    expand_paint_dirties_for_translated_slots, intersect_rects, paint_dirty_has_keyed_slot,
    paint_dirty_matches_present_slot, paint_dirty_nested_in_larger_node, paint_element_key,
    take_frame_paint_nodes, PaintDirtyKind,
};
use crate::app::draw_cull::{
    node_paint_visual_bounds, node_present_clipped_capture_text, node_present_dest_reset_scaled,
    node_present_scissor_logical,
};
use crate::core::reactivity::use_signal;
use crate::core::scroll_viewport::{scroll_offset_y_signal_key, SCROLL_VIEWPORT_FOCUS_PREFIX};
use crate::core::tree::{visual_paint_aabb, Element, LayoutStatus, LayoutTree};
use crate::primitives::r#box::hstack;
use crate::style::{
    apply_class_list, sync_class_variants_subtree, tick_active_css_transitions_subtree,
};

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
        crate::primitives::interactive::take_hover_paint_dirty(),
        "idle sync must tick animate-spin even when element_is_offscreen is true — that is why spin dies when scrolling stops"
    );
    assert!(
        root.animation_snapshot
            .runs
            .first()
            .is_some_and(|r| r.elapsed_ms > 0.0),
        "spin elapsed must advance on the idle path, got {:?}",
        root.animation_snapshot.runs
    );
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

/// Gallery animation row: ping scale-2 / bounce translate must not dest-reset the
/// caption above; pulse opacity must keep a full wipe when a grown ping neighbor
/// only overlaps visually.
#[test]
fn gallery_ping_bounce_pulse_dest_reset_must_not_punch_captions() {
    let _ = take_frame_paint_nodes();
    crate::style::utility::set_frame_dt(1.0 / 60.0);

    let mut ping_label = Element::new().h(14.0).flex_shrink(0.0).text("ping");
    ping_label.set_attr("data-slot", "ping-label");
    let mut ping = Element::new()
        .w(32.0)
        .h(32.0)
        .flex_shrink(0.0)
        .bg([0.2, 0.2, 0.85, 1.0]);
    apply_class_list(&mut ping, "rounded-full animate-ping");
    ping.set_attr("data-slot", "ping-box");
    let ping_col = Element::new()
        .flex_col()
        .gap(6.0)
        .flex_shrink(0.0)
        .child(ping_label)
        .child(ping);

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
        .h(80.0)
        .bg([0.96, 0.96, 0.96, 1.0])
        .child(ping_col)
        .child(pulse_col)
        .child(bounce_col);
    let mut tree = LayoutTree::new();
    let _first = tree.build_and_layout(&mut root, 400.0, 80.0, [-1.0, -1.0]);
    let _ = take_frame_paint_nodes();

    // Ping scale=2 at 75% of the 1s rule; bounce peak is 50%.
    crate::style::utility::set_frame_dt(0.75);
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
    let later = tree.build_and_layout_fast(&mut root, 400.0, 80.0, [-1.0, -1.0], false, false);

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
        !rect_contains_point(ping_wipe, pl[0] + pl[2] * 0.5, pl[1] + pl[3] * 0.5),
        "ping dest-reset must not punch the caption\n wipe={} label={}",
        fmt_rect(ping_wipe),
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
        pulse_wipe[0] <= pulse_box[0] + 0.5
            && pulse_wipe[1] <= pulse_box[1] + 0.5
            && pulse_wipe[0] + pulse_wipe[2] >= pulse_box[0] + pulse_box[2] - 0.5
            && pulse_wipe[1] + pulse_wipe[3] >= pulse_box[1] + pulse_box[3] - 0.5,
        "pulse dest-reset must cover the full rounded box (no nested frames)\n wipe={} pulse={}",
        fmt_rect(pulse_wipe),
        fmt_rect(pulse_box)
    );
}
