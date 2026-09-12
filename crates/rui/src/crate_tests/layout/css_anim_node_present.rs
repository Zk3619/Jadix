//! CSS `animate-spin` must register a node-present dirty every tick.
//!
//! Gallery: typing in Kbd Search switches the frame to PartialLoad (Input is in
//! `damage_nodes`). Spin ticks only set `StyleChanged` + `request_redraw`, so the
//! Marker loader is not dest-reset/redrawn — retain keeps an old angle, then the
//! next FullClear snaps. That is the jump; sidebar ghosts are the same present
//! mode (Load without redrawing neighbors).

use crate::app::damage::{intersect_rects, take_frame_paint_nodes, PaintDirtyKind};
use crate::core::reactivity::use_signal;
use crate::core::scroll_viewport::{scroll_offset_y_signal_key, SCROLL_VIEWPORT_FOCUS_PREFIX};
use crate::core::tree::{visual_paint_aabb, Element, LayoutStatus, LayoutTree};
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
    crate::layout::collect_cache::stamp_subtree_paint_hashes(&mut root);
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

    assert!(
        crate::app::damage::node_present_dirties_moved(
            &[spin_dirty.clone()],
            &[(key, slot.bounds)],
            &[(
                key,
                [
                    slot.bounds[0],
                    slot.bounds[1] + 80.0,
                    slot.bounds[2],
                    slot.bounds[3]
                ]
            )],
        ),
        "visual Y shift of a named spin cannot stay paint-in-place"
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
