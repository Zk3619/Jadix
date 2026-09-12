//! CSS `animate-spin` must register a node-present dirty every tick.
//!
//! Gallery: typing in Kbd Search switches the frame to PartialLoad (Input is in
//! `damage_nodes`). Spin ticks only set `StyleChanged` + `request_redraw`, so the
//! Marker loader is not dest-reset/redrawn — retain keeps an old angle, then the
//! next FullClear snaps. That is the jump; sidebar ghosts are the same present
//! mode (Load without redrawing neighbors).

use crate::app::damage::{
    intersect_rects, take_frame_paint_nodes, PaintDirtyKind,
};
use crate::core::tree::{Element, LayoutStatus, LayoutTree};
use crate::style::{apply_class_list, sync_class_variants_subtree};

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
        dirty.iter().any(|d| {
            d.kind == PaintDirtyKind::Node && intersect_rects(d.bounds, b).is_some()
        }),
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
