//! ColorPicker 拖拽会话（产品层）。

use super::model::*;
use super::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[derive(Clone, Copy)]
pub(super) enum ColorDragKind {
    Sv,
}

#[derive(Clone)]
pub(super) struct ColorDrag {
    pub(super) kind: ColorDragKind,
    pub(super) bounds: Rc<Cell<[f32; 4]>>,
    pub(super) rgba: Signal<Rgba>,
    pub(super) hue: Signal<f32>,
    pub(super) sat: Signal<f32>,
    pub(super) val: Signal<f32>,
    pub(super) show_alpha: bool,
    pub(super) out: Signal<String>,
    pub(super) fmt_sig: Signal<ColorFormat>,
    pub(super) sv_id: Option<String>,
}

thread_local! {
    static COLOR_DRAG: RefCell<Option<ColorDrag>> = const { RefCell::new(None) };
    static PENDING_COLOR_DRAG: Cell<Option<(f32, f32, Option<[f32; 4]>)>> =
        const { Cell::new(None) };
}

pub(super) fn begin_color_drag(drag: ColorDrag) {
    let sid = drag.sv_id.clone().unwrap_or_else(|| "color_picker".into());
    crate::utils::pointer_drag::begin(
        crate::utils::pointer_drag::DragKind::ColorPicker,
        sid,
        0.0,
        0.0,
    );
    COLOR_DRAG.with(|d| *d.borrow_mut() = Some(drag));
}

pub(super) fn end_color_drag() {
    crate::utils::pointer_drag::clear();
    COLOR_DRAG.with(|d| *d.borrow_mut() = None);
    crate::app::release_pointer_capture();
}

pub(crate) fn is_color_dragging() -> bool {
    COLOR_DRAG.with(|d| d.borrow().is_some())
}

#[cfg(test)]
pub(crate) fn tick_color_drag(x: f32, y: f32) -> bool {
    tick_color_drag_with_bounds(x, y, None)
}

/// Snapshot SV chart bounds. Safe while `AppState` is borrowed (no `Signal::set`).
pub(crate) fn snapshot_color_drag_bounds(
    root: &crate::core::tree::Element,
    x: f32,
    y: f32,
) -> Option<[f32; 4]> {
    let live = crate::utils::pointer_drag::peek()
        .filter(|s| s.kind == crate::utils::pointer_drag::DragKind::ColorPicker)
        .and_then(|s| {
            let fid = format!("{}:sv", s.id);
            let path = crate::app::pointer_events::hit_path(root, x, y);
            if let Some(e) = path
                .iter()
                .rev()
                .find(|e| e.focus_id.as_deref() == Some(fid.as_str()))
            {
                if e.bounds[2] > 1.0 && e.bounds[3] > 1.0 {
                    return Some(e.bounds);
                }
            }
            crate::app::tree_hit::find_paint_aabb_by_focus_id(root, &fid, [0.0, 0.0])
                .map(|(aabb, _)| aabb)
        });
    if let Some(b) = live {
        if b[2] > 1.0 && b[3] > 1.0 {
            COLOR_DRAG.with(|d| {
                if let Some(drag) = d.borrow().as_ref() {
                    drag.bounds.set(b);
                }
            });
        }
    }
    live
}

pub(crate) fn queue_color_drag_apply(x: f32, y: f32, live_bounds: Option<[f32; 4]>) {
    PENDING_COLOR_DRAG.with(|p| p.set(Some((x, y, live_bounds))));
}

/// Apply a queued SV tick. Call only when `AppState` is not mutably borrowed.
pub(crate) fn drain_color_drag_apply() {
    let pending = PENDING_COLOR_DRAG.with(|p| p.take());
    if let Some((x, y, live)) = pending {
        let _ = tick_color_drag_with_bounds(x, y, live);
    }
}

/// Remap SV from the chart's current window-space box (hit-path, else paint AABB).
#[allow(dead_code)]
pub(crate) fn tick_color_drag_from_tree(root: &crate::core::tree::Element, x: f32, y: f32) -> bool {
    let live = snapshot_color_drag_bounds(root, x, y);
    tick_color_drag_with_bounds(x, y, live)
}

/// `live_bounds` is the SV chart's current window-space box (hit-path visual AABB).
/// Pass it when the page / floating panel may have moved since pointer-down;
/// frozen document-space y maps the pointer to V→0 (knob glued to the SV bottom).
pub(crate) fn tick_color_drag_with_bounds(x: f32, y: f32, live_bounds: Option<[f32; 4]>) -> bool {
    COLOR_DRAG.with(|d| {
        let Some(drag) = d.borrow().clone() else {
            return false;
        };
        if let Some(b) = live_bounds {
            if b[2] > 1.0 && b[3] > 1.0 {
                drag.bounds.set(b);
            }
        }
        apply_color_pointer(&drag, x, y);
        sync_string_from_rgba(drag.out, drag.rgba.peek(), drag.fmt_sig.peek());
        true
    })
}

pub(crate) fn clear_color_drag_if_any() {
    if is_color_dragging() {
        end_color_drag();
    }
}

pub(super) fn apply_color_pointer(drag: &ColorDrag, x: f32, y: f32) {
    let b = drag.bounds.get();
    if b[2] <= 1.0 || b[3] <= 1.0 {
        return;
    }
    let tx = ((x - b[0]) / b[2]).clamp(0.0, 1.0);
    let ty = ((y - b[1]) / b[3]).clamp(0.0, 1.0);
    let ColorDragKind::Sv = drag.kind;
    let sat = tx;
    let val = 1.0 - ty;
    drag.sat.set(sat);
    drag.val.set(val);
    let (r, g, bch) = hsv_to_rgb(drag.hue.peek(), sat, val);
    let a = drag.rgba.peek().a;
    drag.rgba.set(Rgba {
        r,
        g,
        b: bch,
        a: if drag.show_alpha { a } else { 1.0 },
    });
}
