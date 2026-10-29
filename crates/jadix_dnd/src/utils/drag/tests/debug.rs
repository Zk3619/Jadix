use super::*;
use crate::utils::drag::{DragHit, Rect};

#[test]
fn debug_slots_keep_ancestor_clip_without_changing_collision_slots() {
    check_debug_slot_clip(false);
}

#[test]
fn droppable_debug_slots_keep_original_geometry_while_clipped() {
    check_debug_slot_clip(true);
}

fn check_debug_slot_clip(droppable: bool) {
    for offset in [-4.0, -12.0, -20.0] {
        check_debug_slot_clip_at(droppable, offset);
    }
}

fn check_debug_slot_clip_at(droppable: bool, offset: f32) {
    let previous = debug_drag_hits_enabled();
    set_debug_drag_hits(true);
    let attribute = if droppable {
        super::super::ATTR_DRAG_DROPPABLE
    } else {
        super::super::ATTR_DRAG_DRAGGABLE
    };
    let mut slot = Element::new().w(80.0).h(24.0);
    slot.attrs.insert(attribute.into(), "clipped-slot".into());
    slot.attrs
        .insert(super::super::ATTR_DRAG_SCOPE.into(), "clipped-debug".into());
    slot.style.position = taffy::style::Position::Absolute;
    slot.style.inset.top = taffy::style::LengthPercentageAuto::length(offset);
    let mut hidden = Element::new().w(80.0).h(24.0);
    hidden.attrs.insert(attribute.into(), "hidden-slot".into());
    hidden
        .attrs
        .insert(super::super::ATTR_DRAG_SCOPE.into(), "clipped-debug".into());
    hidden.style.position = taffy::style::Position::Absolute;
    hidden.style.inset.top = taffy::style::LengthPercentageAuto::length(-60.0);
    let mut viewport = Element::new()
        .w(160.0)
        .h(64.0)
        .overflow_y("hidden")
        .child(slot)
        .child(hidden);
    viewport.style.position = taffy::style::Position::Absolute;
    viewport.style.inset.top = taffy::style::LengthPercentageAuto::length(80.0);
    let mut root = Element::new().w(240.0).h(200.0).child(viewport);
    let mut tree = crate::layout::LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 240.0, 200.0, [-1.0; 2]);
    let hit = collect_hits(&root)
        .into_iter()
        .find(|hit| hit.id == "clipped-slot")
        .unwrap();
    assert_eq!(hit.rect.y, if droppable { 80.0 } else { 80.0 + offset });
    let border = if droppable {
        [0.12, 0.72, 0.38, 0.85]
    } else {
        [0.25, 0.55, 0.98, 0.85]
    };
    let debug = result
        .instances
        .iter()
        .find(|instance| instance.border_color == border)
        .unwrap();
    assert_eq!(debug.rect_pos[1], 80.0 + offset);
    assert_eq!(debug.rect_size[1], 24.0);
    assert_eq!(
        debug.clip_pos[1], 80.0,
        "debug ink must not escape the content viewport into its header"
    );
    assert_eq!(debug.clip_size[1], 64.0);
    assert_eq!(
        result
            .instances
            .iter()
            .filter(|instance| instance.border_color == border)
            .count(),
        1
    );
    set_debug_drag_hits(previous);
}

fn hit(id: &str, bounds: [f32; 4], drag: bool, drop: bool) -> DragHit {
    DragHit {
        id: id.into(),
        scope_id: "s".into(),
        is_draggable: drag,
        is_droppable: drop,
        rect: Rect::from_bounds(bounds),
        item_type: "card".into(),
        accept: String::new(),
        list_id: Some("list".into()),
        index: None,
        group: String::new(),
        disabled: false,
        list_axis: None,
        skip_shift: false,
        skip_follow: false,
        pull_clone: false,
    }
}

#[test]
fn frames_skip_unusable_and_mark_active_target() {
    let hits = vec![
        hit("src", [0.0, 0.0, 40.0, 24.0], true, true),
        hit("zone", [80.0, 0.0, 80.0, 80.0], false, true),
        hit("gone", [0.0, 0.0, 0.0, 0.0], false, true),
    ];
    let frames = debug_hit_frames(&hits, Some("src"), Some("zone"), Some("s"));
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0].emphasis, DragDebugEmphasis::ActiveSource);
    assert_eq!(frames[1].emphasis, DragDebugEmphasis::ActiveTarget);
    assert!(!frames[1].stroke().dashed);
    assert!(frames[1].stroke().width >= 2.5);
    assert!(frames[0].stroke().width >= 2.0);
}

#[test]
fn idle_droppable_is_dashed() {
    let hits = vec![hit("zone", [0.0, 0.0, 10.0, 10.0], false, true)];
    let frames = debug_hit_frames(&hits, None, None, None);
    assert_eq!(frames[0].role, DragDebugRole::Droppable);
    assert_eq!(frames[0].emphasis, DragDebugEmphasis::Idle);
    assert!(frames[0].stroke().dashed);
}

#[test]
fn foreign_scope_does_not_get_emphasis() {
    let mut other = hit("zone", [0.0, 0.0, 10.0, 10.0], false, true);
    other.scope_id = "other".into();
    let frames = debug_hit_frames(&[other], None, Some("zone"), Some("s"));
    assert!(frames.is_empty());
}

#[test]
fn flag_roundtrip() {
    let prev = debug_drag_hits_enabled();
    set_debug_drag_hits(true);
    assert!(debug_drag_hits_enabled());
    set_debug_drag_hits(false);
    assert!(!debug_drag_hits_enabled());
    set_debug_drag_hits(prev);
}
