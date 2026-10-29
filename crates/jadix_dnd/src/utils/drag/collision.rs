//! Collision detectors from solid-dnd (`closestCenter` / `closestCorners` /
//! `mostIntersecting`) plus dnd-kit `pointerWithin` for nested droppables.

use std::rc::Rc;

use super::geometry::{distance, Rect};

/// Input for [`CollisionFn`] / [`detect_with`] (solid-dnd `CollisionDetector`).
pub struct CollisionInput<'a> {
    pub pointer: [f32; 2],
    /// Ghost / source layout slot in pointer space.
    pub dragged: Rect,
    /// Already type/group filtered.
    pub droppables: &'a [Candidate],
    pub active_id: Option<&'a str>,
    pub draggable_id: &'a str,
    /// When true, skip [`refine_innermost_nested`] after the detector returns.
    pub skip_innermost: bool,
}

/// Author collision detector (solid-dnd `CollisionDetector`). Prefer this over
/// swapping [`CollisionAlg`] when the pick depends on payload / custom scoring.
pub type CollisionFn = Rc<dyn for<'a> Fn(&CollisionInput<'a>) -> Option<String>>;

/// Which algorithm picks the active droppable each move.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CollisionAlg {
    /// Highest Jaccard overlap with the transformed draggable (solid-dnd default).
    #[default]
    MostIntersecting,
    /// Nearest droppable center to the transformed draggable center.
    ClosestCenter,
    /// Sum of distances between corresponding corners.
    ClosestCorners,
    /// Smallest droppable that contains the pointer (nested zones).
    PointerWithin,
}

impl CollisionAlg {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "closest-center" | "closestcenter" | "center" => Self::ClosestCenter,
            "closest-corners" | "closestcorners" | "corners" => Self::ClosestCorners,
            "pointer-within" | "pointerwithin" | "pointer" => Self::PointerWithin,
            _ => Self::MostIntersecting,
        }
    }
}

/// A droppable candidate with a stable id (already type/group filtered).
#[derive(Clone, Debug)]
pub struct Candidate {
    pub id: String,
    pub rect: Rect,
}

/// Pick one droppable. Ties keep `active_id` when that candidate is still valid
/// (solid-dnd hysteresis).
pub fn detect(
    alg: CollisionAlg,
    pointer: [f32; 2],
    dragged: Rect,
    droppables: &[Candidate],
    active_id: Option<&str>,
) -> Option<String> {
    detect_with(
        alg,
        None,
        CollisionInput {
            pointer,
            dragged,
            droppables,
            active_id,
            draggable_id: "",
            skip_innermost: false,
        },
    )
}

/// [`detect`] plus an optional author [`CollisionFn`]. The function wins when
/// set; [`CollisionAlg`] is the fallback. Innermost nested refine still runs
/// unless [`CollisionInput::skip_innermost`].
pub fn detect_with(
    alg: CollisionAlg,
    collision_fn: Option<&CollisionFn>,
    input: CollisionInput<'_>,
) -> Option<String> {
    if input.droppables.is_empty() {
        return None;
    }
    let picked = if let Some(f) = collision_fn {
        f(&input)
    } else {
        match alg {
            CollisionAlg::MostIntersecting => {
                most_intersecting(input.dragged, input.droppables, input.active_id)
            }
            CollisionAlg::ClosestCenter => {
                closest_center(input.dragged, input.droppables, input.active_id)
            }
            CollisionAlg::ClosestCorners => {
                closest_corners(input.dragged, input.droppables, input.active_id)
            }
            CollisionAlg::PointerWithin => {
                pointer_within(input.pointer, input.droppables, input.active_id)
            }
        }
    };
    if input.skip_innermost {
        picked
    } else {
        refine_innermost_nested(input.pointer, input.droppables, picked)
    }
}

/// When the winner fully contains a smaller candidate that also contains the
/// pointer, prefer the inner droppable (nested lists / folders).
pub(crate) fn refine_innermost_nested(
    pointer: [f32; 2],
    droppables: &[Candidate],
    picked: Option<String>,
) -> Option<String> {
    let id = picked?;
    let winner = droppables.iter().find(|d| d.id == id)?;
    let inner = droppables
        .iter()
        .filter(|d| {
            d.id != id && winner.rect.contains_rect(d.rect) && d.rect.contains_point(pointer)
        })
        .min_by(|a, b| {
            a.rect
                .area()
                .partial_cmp(&b.rect.area())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    Some(inner.map(|d| d.id.clone()).unwrap_or(id))
}

fn keep_active_on_tie(best_id: &str, candidate_id: &str, active_id: Option<&str>) -> bool {
    active_id == Some(candidate_id) && active_id != Some(best_id)
}

fn most_intersecting(
    dragged: Rect,
    droppables: &[Candidate],
    active_id: Option<&str>,
) -> Option<String> {
    let mut best_ratio = 0.0_f32;
    let mut best: Option<&Candidate> = None;
    for d in droppables {
        let ratio = dragged.intersection_ratio(d.rect);
        if ratio > best_ratio + 1e-6 {
            best_ratio = ratio;
            best = Some(d);
        } else if (ratio - best_ratio).abs() <= 1e-6
            && ratio > 0.0
            && best.is_some_and(|b| keep_active_on_tie(&b.id, &d.id, active_id))
        {
            best = Some(d);
        }
    }
    if best_ratio <= 0.0 {
        None
    } else {
        best.map(|d| d.id.clone())
    }
}

fn closest_center(
    dragged: Rect,
    droppables: &[Candidate],
    active_id: Option<&str>,
) -> Option<String> {
    let origin = dragged.center();
    let mut best_dist = f32::INFINITY;
    let mut best: Option<&Candidate> = None;
    for d in droppables {
        let dist = distance(origin, d.rect.center());
        if dist < best_dist - 1e-6 {
            best_dist = dist;
            best = Some(d);
        } else if (dist - best_dist).abs() <= 1e-6
            && best.is_some_and(|b| keep_active_on_tie(&b.id, &d.id, active_id))
        {
            best = Some(d);
        }
    }
    best.map(|d| d.id.clone())
}

fn closest_corners(
    dragged: Rect,
    droppables: &[Candidate],
    active_id: Option<&str>,
) -> Option<String> {
    let dc = dragged.corners();
    let mut best_dist = f32::INFINITY;
    let mut best: Option<&Candidate> = None;
    for d in droppables {
        let pc = d.rect.corners();
        let dist = (0..4).map(|i| distance(dc[i], pc[i])).sum::<f32>();
        if dist < best_dist - 1e-6 {
            best_dist = dist;
            best = Some(d);
        } else if (dist - best_dist).abs() <= 1e-6
            && best.is_some_and(|b| keep_active_on_tie(&b.id, &d.id, active_id))
        {
            best = Some(d);
        }
    }
    best.map(|d| d.id.clone())
}

fn pointer_within(
    pointer: [f32; 2],
    droppables: &[Candidate],
    active_id: Option<&str>,
) -> Option<String> {
    let mut best_area = f32::INFINITY;
    let mut best: Option<&Candidate> = None;
    for d in droppables {
        if !d.rect.contains_point(pointer) {
            continue;
        }
        let area = d.rect.area();
        if area < best_area - 1e-6 {
            best_area = area;
            best = Some(d);
        } else if (area - best_area).abs() <= 1e-6
            && best.is_some_and(|b| keep_active_on_tie(&b.id, &d.id, active_id))
        {
            best = Some(d);
        }
    }
    best.map(|d| d.id.clone())
}

#[cfg(all(test, feature = "product-tests"))]
#[path = "tests/collision.rs"]
mod tests;
