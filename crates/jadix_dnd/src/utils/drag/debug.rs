//! Collision-slot debugger (solid-dnd `DragDropDebugger`).
//!
//! Off by default. [`set_debug_drag_hits`] / Provider `.debug_hits(true)` paints
//! dashed layout slots from [`super::collect_hits`] after the regular collect
//! pass — no island remount, no `overlay_pos` subscription.

use super::session::{
    active_draggable_id, active_droppable_id, active_scope_id, collect_hits, is_dragging,
    last_hits_snapshot, DragHit,
};
use crate::core::tree::Element;

crate::authoring::window::window_local! {
    static DEBUG_DRAG_HITS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Toggle slot-rectangle overlay (solid-dnd `DragDropDebugger`).
pub fn set_debug_drag_hits(on: bool) {
    if on {
        super::session_runtime::install();
    }
    DEBUG_DRAG_HITS.set(on);
}

pub fn debug_drag_hits_enabled() -> bool {
    DEBUG_DRAG_HITS.get()
}

/// What kind of target the slot is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragDebugRole {
    Draggable,
    Droppable,
    Both,
}

/// Whether this slot is the live source / collision winner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragDebugEmphasis {
    Idle,
    ActiveSource,
    ActiveTarget,
}

/// One painted collision slot (pointer-space layout rect).
#[derive(Clone, Debug, PartialEq)]
pub struct DragDebugFrame {
    pub id: String,
    pub scope_id: String,
    pub rect: [f32; 4],
    pub role: DragDebugRole,
    pub emphasis: DragDebugEmphasis,
}

/// Stroke + fill for [`DragDebugFrame`] (GPU instance chrome).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DragDebugStroke {
    pub fill: [f32; 4],
    pub border: [f32; 4],
    pub width: f32,
    pub dashed: bool,
}

impl DragDebugFrame {
    pub fn stroke(&self) -> DragDebugStroke {
        match self.emphasis {
            DragDebugEmphasis::ActiveTarget => DragDebugStroke {
                fill: [0.92, 0.22, 0.45, 0.14],
                border: [0.92, 0.18, 0.40, 0.95],
                width: 3.0,
                dashed: false,
            },
            DragDebugEmphasis::ActiveSource => DragDebugStroke {
                fill: [0.98, 0.62, 0.12, 0.12],
                border: [0.98, 0.55, 0.08, 0.95],
                width: 2.5,
                dashed: false,
            },
            DragDebugEmphasis::Idle => match self.role {
                DragDebugRole::Draggable => DragDebugStroke {
                    fill: [0.25, 0.55, 0.98, 0.06],
                    border: [0.25, 0.55, 0.98, 0.85],
                    width: 1.0,
                    dashed: true,
                },
                DragDebugRole::Droppable => DragDebugStroke {
                    fill: [0.15, 0.75, 0.40, 0.06],
                    border: [0.12, 0.72, 0.38, 0.85],
                    width: 1.0,
                    dashed: true,
                },
                DragDebugRole::Both => DragDebugStroke {
                    fill: [0.10, 0.78, 0.82, 0.06],
                    border: [0.08, 0.72, 0.78, 0.85],
                    width: 1.0,
                    dashed: true,
                },
            },
        }
    }
}

/// Map hits to overlay frames. `scope` limits emphasis (and listing) to one provider.
pub fn debug_hit_frames(
    hits: &[DragHit],
    active_source: Option<&str>,
    active_target: Option<&str>,
    scope: Option<&str>,
) -> Vec<DragDebugFrame> {
    hits.iter()
        .filter(|h| h.rect.is_usable())
        .filter(|h| scope.is_none_or(|s| h.scope_id == s))
        .map(|h| {
            let role = match (h.is_draggable, h.is_droppable) {
                (true, true) => DragDebugRole::Both,
                (true, false) => DragDebugRole::Draggable,
                _ => DragDebugRole::Droppable,
            };
            let same_scope = scope.is_none_or(|s| h.scope_id == s);
            let emphasis = if same_scope && active_source == Some(h.id.as_str()) {
                DragDebugEmphasis::ActiveSource
            } else if same_scope && active_target == Some(h.id.as_str()) {
                DragDebugEmphasis::ActiveTarget
            } else {
                DragDebugEmphasis::Idle
            };
            DragDebugFrame {
                id: h.id.clone(),
                scope_id: h.scope_id.clone(),
                rect: h.rect.to_bounds(),
                role,
                emphasis,
            }
        })
        .collect()
}

/// Frames from the last [`super::tick`] hit list (empty when nothing has ticked).
pub fn last_debug_frames() -> Vec<DragDebugFrame> {
    let hits = last_hits_snapshot();
    frames_from_session(&hits)
}

/// Prefer the tick cache while a session is live (expanded empty shells, caps);
/// otherwise walk `root`.
pub fn debug_frames_for_tree(root: &Element) -> Vec<DragDebugFrame> {
    if !debug_drag_hits_enabled() {
        return Vec::new();
    }
    let snap = last_hits_snapshot();
    let hits;
    let slice: &[DragHit] = if is_dragging() && !snap.is_empty() {
        &snap
    } else {
        hits = collect_hits(root);
        &hits
    };
    frames_from_session(slice)
}

fn frames_from_session(hits: &[DragHit]) -> Vec<DragDebugFrame> {
    let src = active_draggable_id();
    let dst = active_droppable_id();
    let scope = active_scope_id();
    debug_hit_frames(hits, src.as_deref(), dst.as_deref(), scope.as_deref())
}

#[cfg(all(test, feature = "product-tests"))]
mod tests {
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
}
