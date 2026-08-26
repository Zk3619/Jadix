//! ColorPicker 拖拽会话（产品层）。

use super::*;
use super::model::*;

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
}

pub(super) fn begin_color_drag(drag: ColorDrag) {
    let sid = drag
        .sv_id
        .clone()
        .unwrap_or_else(|| "color_picker".into());
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

pub(crate) fn tick_color_drag(x: f32, y: f32) -> bool {
    COLOR_DRAG.with(|d| {
        let Some(drag) = d.borrow().clone() else {
            return false;
        };
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

