//! Press-delta **modifier** pipeline (interact.js / dnd-kit `Modifier`).
//!
//! solid-dnd calls the same plugin `Transformer`; RUI avoids that name so it
//! does not collide with CSS `transform` / FLIP `translate` / overlay `rotate`.
//!
//! Built-ins (axis → snap → pin-locked-axis → restrict) run every tick. Authors
//! register extra stages with [`DragModifier::new`] / [`super::DragScopeBuilder::modifier`]
//! without growing `session::tick`. The value flowing through the pipeline is a
//! [`DragDelta`], not a CSS transform matrix.

use std::rc::Rc;

use super::geometry::{Axis, Rect, Restrict, SnapGrid, clamp_origin_to_box};

/// [`builtin_modifiers`] axis lock (interact.js `lockAxis`).
pub const MODIFIER_ORDER_AXIS: i32 = 10;
/// Snap-to-grid on the ghost origin.
pub const MODIFIER_ORDER_SNAP: i32 = 20;
/// Locked axis follows the live layout slot (nested `ScrollArea`).
pub const MODIFIER_ORDER_PIN_AXIS: i32 = 25;
/// Viewport / box clamp.
pub const MODIFIER_ORDER_RESTRICT: i32 = 30;
/// Default order for author modifiers (after builtins).
pub const MODIFIER_ORDER_AUTHOR: i32 = 40;

/// Press-delta in pointer space (solid-dnd `Transform`, renamed to avoid CSS
/// `transform`). Ghost origin is `start_pointer + delta - grab_offset`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DragDelta {
    pub x: f32,
    pub y: f32,
}

/// Per-tick inputs for a modifier callback.
pub struct DragModifierCtx {
    pub pointer: [f32; 2],
    pub start_pointer: [f32; 2],
    pub grab_offset: [f32; 2],
    pub start_bounds: [f32; 4],
    pub viewport: [f32; 2],
    pub axis: Axis,
    pub snap: Option<SnapGrid>,
    pub restrict: Restrict,
    /// Current source layout slot (scroll-adjusted). Used by pin-locked-axis.
    pub live_slot: Option<Rect>,
}

impl DragModifierCtx {
    pub fn origin_of(&self, t: DragDelta) -> [f32; 2] {
        [
            self.start_pointer[0] + t.x - self.grab_offset[0],
            self.start_pointer[1] + t.y - self.grab_offset[1],
        ]
    }

    pub fn from_origin(&self, origin: [f32; 2]) -> DragDelta {
        DragDelta {
            x: origin[0] - self.start_pointer[0] + self.grab_offset[0],
            y: origin[1] - self.start_pointer[1] + self.grab_offset[1],
        }
    }

    pub fn item_size(&self) -> [f32; 2] {
        [self.start_bounds[2].max(1.0), self.start_bounds[3].max(1.0)]
    }
}

pub type DragModifierFn = Rc<dyn Fn(DragDelta, &DragModifierCtx) -> DragDelta>;

/// Named stage in the drag-delta pipeline (solid-dnd `Transformer` / dnd-kit
/// `Modifier`). Does **not** write CSS `transform`.
#[derive(Clone)]
pub struct DragModifier {
    pub id: String,
    pub order: i32,
    pub callback: DragModifierFn,
}

impl DragModifier {
    pub fn new(
        id: impl Into<String>,
        order: i32,
        f: impl Fn(DragDelta, &DragModifierCtx) -> DragDelta + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            order,
            callback: Rc::new(f),
        }
    }
}

/// Reduce `t` through `modifiers` sorted by `(order, id)`.
pub fn apply_modifiers(
    mut t: DragDelta,
    modifiers: &[DragModifier],
    ctx: &DragModifierCtx,
) -> DragDelta {
    let mut ordered: Vec<&DragModifier> = modifiers.iter().collect();
    ordered.sort_by(|a, b| a.order.cmp(&b.order).then_with(|| a.id.cmp(&b.id)));
    for x in ordered {
        t = (x.callback)(t, ctx);
    }
    t
}

/// Built-in stages matching the pre-pipeline `ghost_origin` order.
pub fn builtin_modifiers() -> Vec<DragModifier> {
    vec![
        DragModifier::new("axis", MODIFIER_ORDER_AXIS, |t, ctx| {
            let (x, y) = ctx.axis.apply(t.x, t.y);
            DragDelta { x, y }
        }),
        DragModifier::new("snap", MODIFIER_ORDER_SNAP, |t, ctx| {
            let Some(snap) = ctx.snap else {
                return t;
            };
            let o = ctx.origin_of(t);
            ctx.from_origin(snap.apply(o[0], o[1]))
        }),
        DragModifier::new("pin-locked-axis", MODIFIER_ORDER_PIN_AXIS, |t, ctx| {
            if ctx.axis == Axis::Xy {
                return t;
            }
            let Some(slot) = ctx.live_slot else {
                return t;
            };
            let mut o = ctx.origin_of(t);
            match ctx.axis {
                Axis::Y => o[0] = slot.x,
                Axis::X => o[1] = slot.y,
                Axis::Xy => {}
            }
            ctx.from_origin(o)
        }),
        DragModifier::new("restrict", MODIFIER_ORDER_RESTRICT, |t, ctx| {
            let origin = ctx.origin_of(t);
            let size = ctx.item_size();
            let clamped = match ctx.restrict {
                Restrict::None => return t,
                Restrict::Viewport => clamp_origin_to_box(
                    origin,
                    size,
                    [0.0, 0.0, ctx.viewport[0].max(1.0), ctx.viewport[1].max(1.0)],
                ),
                Restrict::Rect(r) => clamp_origin_to_box(origin, size, r),
            };
            ctx.from_origin(clamped)
        }),
    ]
}

#[cfg(test)]
mod tests {
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
        ctx.snap = Some(SnapGrid {
            x: 10.0,
            y: 10.0,
            offset: [0.0, 0.0],
        });
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
}
