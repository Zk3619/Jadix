use super::*;

fn ctx_xy() -> DragModifierCtx {
    DragModifierCtx {
        pointer: [20.0, 30.0],
        start_pointer: [10.0, 10.0],
        grab_offset: [0.0, 0.0],
        start_bounds: [10.0, 10.0, 20.0, 20.0],
        viewport: [400.0, 300.0],
        axis: Axis::Xy,
        snap: None,
        restrict: Restrict::None,
        live_slot: None,
        at_end: false,
        restrict_end_only: false,
        restrict_anchor: RestrictAnchor::FULL,
    }
}

#[test]
fn axis_builtin_zeros_locked_delta() {
    let mut ctx = ctx_xy();
    ctx.axis = Axis::Y;
    let t = apply_modifiers(DragDelta { x: 8.0, y: 12.0 }, &builtin_modifiers(), &ctx);
    assert!((t.x).abs() < 1e-4);
    assert!((t.y - 12.0).abs() < 1e-4);
}

#[test]
fn author_modifier_runs_after_axis() {
    let mut ctx = ctx_xy();
    ctx.axis = Axis::Y;
    let mut pipeline = builtin_modifiers();
    pipeline.push(DragModifier::new(
        "nudge-x",
        MODIFIER_ORDER_AUTHOR,
        |t, _| DragDelta {
            x: t.x + 5.0,
            y: t.y,
        },
    ));
    let t = apply_modifiers(DragDelta { x: 8.0, y: 0.0 }, &pipeline, &ctx);
    assert!((t.x - 5.0).abs() < 1e-4, "axis zeros x then author adds 5");
    assert!(t.y.abs() < 1e-4);
}

#[test]
fn lower_order_runs_first() {
    let ctx = ctx_xy();
    let pipeline = vec![
        DragModifier::new("late", 20, |t, _| DragDelta {
            x: t.x * 2.0,
            y: t.y,
        }),
        DragModifier::new("early", 10, |t, _| DragDelta {
            x: t.x + 3.0,
            y: t.y,
        }),
    ];
    let t = apply_modifiers(DragDelta { x: 1.0, y: 0.0 }, &pipeline, &ctx);
    assert!((t.x - 8.0).abs() < 1e-4, "(1+3)*2 = 8, got {}", t.x);
}

#[test]
fn snap_and_restrict_match_legacy_origin_math() {
    let mut ctx = ctx_xy();
    ctx.snap = Some(Snap::grid(SnapGrid::new(10.0, 10.0)));
    ctx.restrict = Restrict::Viewport;
    ctx.start_bounds = [0.0, 0.0, 50.0, 50.0];
    ctx.viewport = [100.0, 80.0];
    let t = apply_modifiers(DragDelta { x: 13.0, y: 18.0 }, &builtin_modifiers(), &ctx);
    let origin = ctx.origin_of(t);
    // delta (13,18) → origin (23,28) → snap 10 → (20,30); viewport clamp y max = 80-50 = 30.
    assert!((origin[0] - 20.0).abs() < 1e-3, "{origin:?}");
    assert!((origin[1] - 30.0).abs() < 1e-3, "{origin:?}");
    assert!(origin[0] + 50.0 <= 100.0 + 1e-3);
    assert!(origin[1] + 50.0 <= 80.0 + 1e-3);
}

#[test]
fn snap_end_only_skips_until_at_end() {
    let mut ctx = ctx_xy();
    ctx.snap = Some(Snap::grid(SnapGrid::new(10.0, 10.0).end_only(true)));
    let t = apply_modifiers(DragDelta { x: 13.0, y: 0.0 }, &builtin_modifiers(), &ctx);
    let o = ctx.origin_of(t);
    assert!((o[0] - 23.0).abs() < 1e-3, "tick must not snap; {o:?}");
    ctx.at_end = true;
    let t = apply_modifiers(DragDelta { x: 13.0, y: 0.0 }, &builtin_modifiers(), &ctx);
    let o = ctx.origin_of(t);
    assert!((o[0] - 20.0).abs() < 1e-3, "end must snap; {o:?}");
}

#[test]
fn restrict_anchor_center_in_pipeline() {
    let mut ctx = ctx_xy();
    ctx.restrict = Restrict::Rect([0.0, 0.0, 100.0, 80.0]);
    ctx.restrict_anchor = RestrictAnchor::CENTER;
    ctx.start_bounds = [0.0, 0.0, 20.0, 20.0];
    ctx.start_pointer = [10.0, 10.0];
    ctx.grab_offset = [10.0, 10.0];
    let t = apply_modifiers(DragDelta { x: -40.0, y: -40.0 }, &builtin_modifiers(), &ctx);
    let o = ctx.origin_of(t);
    assert!(
        (o[0] + 10.0).abs() < 1e-3 && (o[1] + 10.0).abs() < 1e-3,
        "center may sit on the box edge; origin {o:?}"
    );
}
