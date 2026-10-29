use super::*;

#[test]
fn axis_locks() {
    assert_eq!(Axis::X.apply(3.0, 4.0), (3.0, 0.0));
    assert_eq!(Axis::Y.apply(3.0, 4.0), (0.0, 4.0));
    assert_eq!(Axis::Xy.apply(3.0, 4.0), (3.0, 4.0));
    assert_eq!(Axis::Start.apply(3.0, 4.0), (3.0, 4.0));
    assert_eq!(Axis::resolve_start(10.0, 3.0), Axis::X);
    assert_eq!(Axis::resolve_start(3.0, 10.0), Axis::Y);
    assert_eq!(Axis::resolve_start(5.0, 5.0), Axis::X);
    assert!(Axis::X.allows_start_move(8.0, 1.0));
    assert!(!Axis::X.allows_start_move(1.0, 8.0));
    assert!(Axis::X.allows_start_move(5.0, 5.0));
    assert_eq!(Axis::Start.sort_axis(), Axis::Y);
    assert_eq!(Axis::parse("start"), Axis::Start);
}

#[test]
fn drag_stack_parse_and_resolve() {
    assert_eq!(DragStack::parse("auto"), DragStack::Auto);
    assert_eq!(DragStack::parse("always-top"), DragStack::Top);
    assert_eq!(DragStack::parse("always_bottom"), DragStack::Bottom);
    assert_eq!(DragStack::parse("none"), DragStack::Keep);
    assert_eq!(DragStack::Keep.resolve(3, 9), 3);
    assert_eq!(
        DragStack::Top.resolve(2, 0),
        DragStack::TOP_BAND.saturating_add(2)
    );
    assert_eq!(
        DragStack::Bottom.resolve(1, 0),
        DragStack::BOTTOM_BAND.saturating_add(1)
    );
    assert_eq!(DragStack::Auto.resolve(4, 0), 4);
    assert_eq!(DragStack::Auto.resolve(4, 7), 7);
    assert!(DragStack::Top.resolve(0, 0) > DragStack::Auto.resolve(0, 99));
    assert!(DragStack::Bottom.resolve(0, 0) < DragStack::Auto.resolve(0, 1));
}

#[test]
fn inflated_grows_box() {
    let r = Rect::from_bounds([10.0, 10.0, 20.0, 8.0]).inflated(4.0);
    assert_eq!(r.to_bounds(), [6.0, 6.0, 28.0, 16.0]);
}

#[test]
fn snap_grid_rounds() {
    let g = SnapGrid::new(10.0, 10.0);
    assert_eq!(g.apply(14.0, 6.0), [10.0, 10.0]);
}

#[test]
fn snap_grid_range_skips_when_far() {
    let g = SnapGrid::new(10.0, 10.0).range(2.0);
    assert_eq!(g.apply(14.0, 10.0), [14.0, 10.0], "4px from 10 > range 2");
    assert_eq!(g.apply(11.0, 10.0), [10.0, 10.0]);
}

#[test]
fn snap_targets_picks_closest_in_range_grid() {
    let t = SnapTargets::new([
        SnapGrid::new(8.0, 8.0),
        SnapGrid::new(32.0, 32.0).offset([16.0, 16.0]),
    ])
    .range(20.0);
    assert_eq!(t.apply(10.0, 10.0), [8.0, 8.0]);
    assert_eq!(t.apply(30.0, 30.0), [32.0, 32.0]);
}

#[test]
fn snap_targets_skips_when_all_out_of_range() {
    let t = SnapTargets::new([SnapGrid::new(10.0, 10.0).range(2.0)]).range(2.0);
    assert_eq!(t.apply(14.0, 10.0), [14.0, 10.0]);
}

#[test]
fn snap_fn_target_snaps_to_line() {
    use std::rc::Rc;
    let t = SnapTargets::default().with_fn(Rc::new(|x, _y| Some([x, 64.0])));
    assert_eq!(t.apply(12.0, 70.0), [12.0, 64.0]);
}

#[test]
fn restrict_anchor_center_lets_origin_leave_box() {
    let origin = clamp_origin_to_box_anchor(
        [-10.0, -10.0],
        [20.0, 20.0],
        [0.0, 0.0, 100.0, 80.0],
        RestrictAnchor::CENTER,
    );
    assert!((origin[0] + 10.0).abs() < 1e-3, "{origin:?}");
    assert!((origin[1] + 10.0).abs() < 1e-3, "{origin:?}");
}

#[test]
fn restrict_overflow_parse() {
    assert_eq!(RestrictOverflow::parse(""), RestrictOverflow::Visible);
    assert_eq!(
        RestrictOverflow::parse("visible"),
        RestrictOverflow::Visible
    );
    assert_eq!(RestrictOverflow::parse("hidden"), RestrictOverflow::Hidden);
    assert_eq!(RestrictOverflow::parse("clip"), RestrictOverflow::Hidden);
    assert_eq!(RestrictOverflow::Hidden.as_str(), "hidden");
}

#[test]
fn intersection_none_and_full() {
    let a = Rect::from_bounds([0.0, 0.0, 10.0, 10.0]);
    let b = Rect::from_bounds([20.0, 20.0, 10.0, 10.0]);
    assert_eq!(a.intersection_ratio(b), 0.0);
    assert!((a.intersection_ratio(a) - 1.0).abs() < 1e-5);
}

#[test]
fn clamp_keeps_ghost_inside() {
    let p = clamp_origin_to_box([90.0, 90.0], [20.0, 20.0], [0.0, 0.0, 100.0, 100.0]);
    assert_eq!(p, [80.0, 80.0]);
}

#[test]
fn restrict_parse_viewport_none_and_element_id() {
    assert_eq!(Restrict::parse(""), Restrict::None);
    assert_eq!(Restrict::parse("none"), Restrict::None);
    assert_eq!(Restrict::parse("viewport"), Restrict::Viewport);
    assert_eq!(Restrict::parse("window"), Restrict::Viewport);
    assert_eq!(Restrict::parse("parent"), Restrict::Parent);
    assert_eq!(Restrict::Parent.as_str(), "parent");
    assert_eq!(
        Restrict::parse("corners-arena"),
        Restrict::Element("corners-arena".into())
    );
    assert_eq!(
        Restrict::parse("#dock"),
        Restrict::Element("dock".into()),
        "leading # is optional CSS-id sugar"
    );
}

#[test]
fn contains_rect_is_strict_nesting() {
    let outer = Rect::from_bounds([0.0, 0.0, 100.0, 100.0]);
    let inner = Rect::from_bounds([10.0, 10.0, 20.0, 20.0]);
    assert!(outer.contains_rect(inner));
    assert!(!inner.contains_rect(outer));
    assert!(!outer.contains_rect(outer));
    let overlap = Rect::from_bounds([80.0, 80.0, 40.0, 40.0]);
    assert!(!outer.contains_rect(overlap));
}
