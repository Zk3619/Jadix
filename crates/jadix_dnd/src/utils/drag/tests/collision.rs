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

#[test]
fn collision_fn_overrides_enum_pick() {
    let dragged = Rect::from_bounds([0.0, 0.0, 10.0, 10.0]);
    let hits = vec![
        c("near", [12.0, 0.0, 10.0, 10.0]),
        c("far", [80.0, 80.0, 10.0, 10.0]),
    ];
    let f: CollisionFn = Rc::new(|input| Some(input.droppables[1].id.clone()));
    let picked = detect_with(
        CollisionAlg::ClosestCenter,
        Some(&f),
        CollisionInput {
            pointer: [0.0, 0.0],
            dragged,
            droppables: &hits,
            active_id: None,
            draggable_id: "src",
            skip_innermost: false,
        },
    );
    assert_eq!(picked.as_deref(), Some("far"));
}

#[test]
fn collision_fn_still_refines_innermost() {
    let dragged = Rect::from_bounds([10.0, 10.0, 80.0, 80.0]);
    let hits = vec![
        c("outer", [0.0, 0.0, 200.0, 200.0]),
        c("inner", [20.0, 20.0, 40.0, 40.0]),
    ];
    let f: CollisionFn = Rc::new(|_| Some("outer".into()));
    let picked = detect_with(
        CollisionAlg::MostIntersecting,
        Some(&f),
        CollisionInput {
            pointer: [30.0, 30.0],
            dragged,
            droppables: &hits,
            active_id: None,
            draggable_id: "src",
            skip_innermost: false,
        },
    );
    assert_eq!(picked.as_deref(), Some("inner"));
    let skipped = detect_with(
        CollisionAlg::MostIntersecting,
        Some(&f),
        CollisionInput {
            pointer: [30.0, 30.0],
            dragged,
            droppables: &hits,
            active_id: None,
            draggable_id: "src",
            skip_innermost: true,
        },
    );
    assert_eq!(skipped.as_deref(), Some("outer"));
}
