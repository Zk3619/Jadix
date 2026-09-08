//! Collision detectors from solid-dnd (`closestCenter` / `closestCorners` /
//! `mostIntersecting`) plus dnd-kit `pointerWithin` for nested droppables.

use super::geometry::{Rect, distance};

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
    if droppables.is_empty() {
        return None;
    }
    let picked = match alg {
        CollisionAlg::MostIntersecting => most_intersecting(dragged, droppables, active_id),
        CollisionAlg::ClosestCenter => closest_center(dragged, droppables, active_id),
        CollisionAlg::ClosestCorners => closest_corners(dragged, droppables, active_id),
        CollisionAlg::PointerWithin => pointer_within(pointer, droppables, active_id),
    };
    refine_innermost_nested(pointer, droppables, picked)
}

/// When the winner fully contains a smaller candidate that also contains the
/// pointer, prefer the inner droppable (nested lists / folders).
fn refine_innermost_nested(
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

#[cfg(test)]
mod tests {
    use super::*;

    fn c(id: &str, b: [f32; 4]) -> Candidate {
        Candidate {
            id: id.into(),
            rect: Rect::from_bounds(b),
        }
    }

    #[test]
    fn most_intersecting_picks_larger_overlap() {
        let dragged = Rect::from_bounds([0.0, 0.0, 20.0, 20.0]);
        let hits = vec![
            c("a", [15.0, 0.0, 20.0, 20.0]),
            c("b", [2.0, 2.0, 20.0, 20.0]),
        ];
        assert_eq!(
            detect(
                CollisionAlg::MostIntersecting,
                [0.0, 0.0],
                dragged,
                &hits,
                None
            )
            .as_deref(),
            Some("b")
        );
    }

    #[test]
    fn closest_center_picks_nearest() {
        let dragged = Rect::from_bounds([0.0, 0.0, 10.0, 10.0]);
        let hits = vec![
            c("far", [80.0, 80.0, 10.0, 10.0]),
            c("near", [12.0, 0.0, 10.0, 10.0]),
        ];
        assert_eq!(
            detect(
                CollisionAlg::ClosestCenter,
                [0.0, 0.0],
                dragged,
                &hits,
                None
            )
            .as_deref(),
            Some("near")
        );
    }

    #[test]
    fn pointer_within_prefers_smaller_nested() {
        let hits = vec![
            c("outer", [0.0, 0.0, 100.0, 100.0]),
            c("inner", [20.0, 20.0, 20.0, 20.0]),
        ];
        let dragged = Rect::from_bounds([22.0, 22.0, 8.0, 8.0]);
        assert_eq!(
            detect(
                CollisionAlg::PointerWithin,
                [25.0, 25.0],
                dragged,
                &hits,
                None
            )
            .as_deref(),
            Some("inner")
        );
    }

    #[test]
    fn hysteresis_keeps_active_on_tie() {
        let dragged = Rect::from_bounds([0.0, 0.0, 10.0, 10.0]);
        let hits = vec![
            c("a", [0.0, 0.0, 10.0, 10.0]),
            c("b", [0.0, 0.0, 10.0, 10.0]),
        ];
        assert_eq!(
            detect(
                CollisionAlg::MostIntersecting,
                [0.0, 0.0],
                dragged,
                &hits,
                Some("b")
            )
            .as_deref(),
            Some("b")
        );
    }

    #[test]
    fn most_intersecting_prefers_nested_inner_when_pointer_inside() {
        let dragged = Rect::from_bounds([10.0, 10.0, 80.0, 80.0]);
        let hits = vec![
            c("outer", [0.0, 0.0, 200.0, 200.0]),
            c("inner", [20.0, 20.0, 40.0, 40.0]),
        ];
        assert_eq!(
            detect(
                CollisionAlg::MostIntersecting,
                [30.0, 30.0],
                dragged,
                &hits,
                None
            )
            .as_deref(),
            Some("inner"),
            "large ghost overlap with the parent must not beat a nested droppable under the pointer"
        );
    }
}
