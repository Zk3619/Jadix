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
#[path = "tests/debug.rs"]
mod tests;
