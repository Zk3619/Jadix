//! # 公开 `<canvas>` 叶（WP-3.12）
//!
//! HTML `<canvas>` 心智：**这块像素我自己画**。回调跑在 UI 线程。`JADIX_GPU_2D` 默认把
//! fill/stroke/clip 记成 PathPipeline 作业 → `jadix_canvas:{id}`；`draw_image` /
//! `put_image_data` 回退 CPU pixmap → `queue_image_rgba8`。`get_image_data` 对已记录
//! 的 path 做 CPU 采样；`get_image_data` 同时登记 Path RT readback（flush 后可取 GPU 像素）。
//! `is_point_in_path` / `is_point_in_stroke` 按当前 CTM 测填充 / 描边。不是 WHATWG
//! `CanvasRenderingContext2D` 全家桶。
//!
//! `fill_path` / `stroke_path` 走与 Chart / Svg 同一 [`PathCommand`] 核。paint 内读 Signal
//! 必须 [`peek`](crate::core::reactivity::Signal::peek)（实现侧再包一层 [`untrack`]，
//! 避免误 `get()` 订到 UI 根 effect）。
//!
//! 双书写：[`canvas`] Builder；`ui!` `<canvas paint={…}>`（见 `jadix_macros`）。

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::core::reactivity::untrack;
use crate::core::tree::{Element, IntoCssDim, IntoElementList, LayoutStatus};
use crate::primitives::path::{premul_to_straight_rgba, queue_path_job, PathGpuJob, PathGpuOp};

/// 叶上登记 paint 回调的 attr（hex id）。
pub const ATTR_CANVAS_PAINT: &str = "jadix_canvas_paint";

pub use jadix_2d::canvas::*;

type PaintFn = Box<dyn FnMut(&mut dyn Canvas2D, CanvasSize) -> LayoutStatus + 'static>;

thread_local! {
    static PAINT_STORE: RefCell<HashMap<u64, PaintFn>> = RefCell::new(HashMap::new());
    static PAINT_DIRTY: RefCell<HashSet<u64>> = RefCell::new(HashSet::new());
    static RASTER_HIT: RefCell<HashMap<u64, (u32, u32, String)>> = RefCell::new(HashMap::new());
    /// 指针拖线标 dirty 后必须补一帧；`CursorMoved` 默认不 `request_redraw`。
    static WANT_REDRAW: Cell<bool> = const { Cell::new(false) };
}

static NEXT_PAINT_ID: AtomicU64 = AtomicU64::new(1);

fn register_paint(paint: PaintFn) -> u64 {
    let id = NEXT_PAINT_ID.fetch_add(1, Ordering::Relaxed);
    PAINT_STORE.with(|s| {
        s.borrow_mut().insert(id, paint);
    });
    PAINT_DIRTY.with(|d| {
        d.borrow_mut().insert(id);
    });
    id
}

fn unregister_paint(id: u64) {
    PAINT_STORE.with(|s| {
        s.borrow_mut().remove(&id);
    });
    PAINT_DIRTY.with(|d| {
        d.borrow_mut().remove(&id);
    });
    RASTER_HIT.with(|c| {
        c.borrow_mut().remove(&id);
    });
}

/// 指针按住拖线 / 尺寸变化之外：强制下一帧重跑 paint。
pub fn mark_canvas_dirty_hex(hex: &str) {
    if let Ok(id) = u64::from_str_radix(hex.trim(), 16) {
        PAINT_DIRTY.with(|d| {
            d.borrow_mut().insert(id);
        });
        WANT_REDRAW.set(true);
    }
}

pub(crate) fn canvas_paint_is_dirty(attrs: &HashMap<String, String>) -> bool {
    attrs
        .get(ATTR_CANVAS_PAINT)
        .and_then(|hex| u64::from_str_radix(hex.trim(), 16).ok())
        .is_some_and(|id| PAINT_DIRTY.with(|dirty| dirty.borrow().contains(&id)))
}

pub(crate) fn has_dirty_canvases() -> bool {
    PAINT_DIRTY.with(|dirty| !dirty.borrow().is_empty())
}

/// `CursorMoved` / 按下 在 dispatch 之后取走：需要 `window.request_redraw`。
///
/// Also take **after** layout collect / `render`: `request_frame` sets the flag
/// during paint, which runs inside collect. Taking before collect drops the pulse
/// and a clock canvas freezes until the next pointer event.
pub(crate) fn take_canvas_wants_redraw() -> bool {
    WANT_REDRAW.replace(false)
}

const MAX_CANVAS_PX: u32 = 1024 * 1024;
const DEFAULT_W: f32 = 300.0;
const DEFAULT_H: f32 = 150.0;

fn canvas_size(width: u32, height: u32) -> CanvasSize {
    CanvasSize {
        w: width as f32,
        h: height as f32,
    }
}

fn invoke_paint(id: u64, canvas: &mut dyn Canvas2D, size: CanvasSize) -> Option<LayoutStatus> {
    PAINT_STORE.with(|s| {
        let mut g = s.borrow_mut();
        let paint = g.get_mut(&id)?;
        Some(untrack(|| paint(canvas, size)))
    })
}

fn finish_paint_status(want_frame: bool, status: LayoutStatus) -> LayoutStatus {
    if want_frame {
        // Keep PAINT_DIRTY *and* wake the window. StyleChanged alone does not
        // request_redraw; without WANT_REDRAW a clock canvas paints once then freezes.
        WANT_REDRAW.set(true);
        status.merge(LayoutStatus::StyleChanged)
    } else {
        status
    }
}

enum PaintOut {
    Gpu(Vec<PathGpuOp>),
    Cpu(Vec<f32>),
}

fn run_paint_cpu(id: u64, width: u32, height: u32) -> Option<(Vec<f32>, LayoutStatus)> {
    let mut canvas = RasterCanvas::new_linear(width, height);
    let status = invoke_paint(id, &mut canvas, canvas_size(width, height))?;
    let status = finish_paint_status(canvas.want_frame, status);
    Some((canvas.into_pixels(), status))
}

fn run_paint(id: u64, width: u32, height: u32) -> Option<(PaintOut, LayoutStatus)> {
    if crate::renderer::gpu2d_enabled() {
        let mut gpu = GpuCanvas::new_linear(width, height, Some(format!("jadix_canvas:{id:016x}")));
        let status = invoke_paint(id, &mut gpu, canvas_size(width, height))?;
        let status = finish_paint_status(gpu.want_frame, status);
        if !gpu.needs_cpu {
            return Some((PaintOut::Gpu(gpu.into_ops()), status));
        }
        return Some((PaintOut::Cpu(gpu.into_raster()), status));
    }
    let (buf, status) = run_paint_cpu(id, width, height)?;
    Some((PaintOut::Cpu(buf), status))
}

/// 布局期：dirty 或尺寸变化时跑 paint。
/// GPU2D：PathPipeline → `jadix_canvas:{id}`（无 CPU 全幅上传）。
/// 否则 / 位图 API：CPU pixmap → `queue_image_rgba8`。
/// 静态叶（paint 返回 Clean）在滚动/无关 mousemove 时复用上一张贴图。
pub fn canvas_raster_from_element_attrs(
    attrs: &HashMap<String, String>,
    leaf_w: f32,
    leaf_h: f32,
) -> Option<(String, f32, f32)> {
    let id = u64::from_str_radix(attrs.get(ATTR_CANVAS_PAINT)?.trim(), 16).ok()?;
    let w = leaf_w.round() as i32;
    let h = leaf_h.round() as i32;
    if w < 2 || h < 2 {
        return None;
    }
    let w = w as u32;
    let h = h as u32;
    if w.saturating_mul(h) > MAX_CANVAS_PX {
        return None;
    }
    let dirty = PAINT_DIRTY.with(|d| d.borrow().contains(&id));
    if !dirty {
        if let Some((cw, ch, key)) = RASTER_HIT.with(|c| c.borrow().get(&id).cloned()) {
            if cw == w && ch == h {
                return Some((key, w as f32, h as f32));
            }
        }
    }
    let (out, status) = run_paint(id, w, h)?;
    let keep_dirty = !matches!(status, LayoutStatus::Clean);
    PAINT_DIRTY.with(|d| {
        let mut g = d.borrow_mut();
        if keep_dirty {
            g.insert(id);
        } else {
            g.remove(&id);
        }
    });
    let tex_id = format!("jadix_canvas:{id:016x}");
    match out {
        PaintOut::Gpu(ops) => {
            queue_path_job(PathGpuJob {
                key: tex_id.clone(),
                width: w,
                height: h,
                ops: crate::primitives::path::PathGpuOps::Ready(ops),
                force_redraw: true,
            });
        }
        PaintOut::Cpu(premul) => {
            let rgba = premul_to_straight_rgba(&premul, w, h);
            crate::queue_image_rgba8(tex_id.clone(), w, h, rgba);
        }
    }
    RASTER_HIT.with(|c| {
        c.borrow_mut().insert(id, (w, h, tex_id.clone()));
    });
    Some((tex_id, w as f32, h as f32))
}

/// 测试用：未翻转、premul 浮点缓冲（y=0 为叶顶）。
#[cfg(test)]
pub(crate) fn rasterize_canvas_premul_for_test(
    attrs: &HashMap<String, String>,
    leaf_w: f32,
    leaf_h: f32,
) -> Option<(Vec<f32>, u32, u32)> {
    let id = u64::from_str_radix(attrs.get(ATTR_CANVAS_PAINT)?.trim(), 16).ok()?;
    let w = leaf_w.round().max(2.0) as u32;
    let h = leaf_h.round().max(2.0) as u32;
    let (buf, _) = run_paint_cpu(id, w, h)?;
    Some((buf, w, h))
}

pub struct CanvasBuilder {
    el: Element,
    has_w: bool,
    has_h: bool,
    paint: Option<PaintFn>,
}

pub fn canvas() -> CanvasBuilder {
    CanvasBuilder {
        el: Element::new(),
        has_w: false,
        has_h: false,
        paint: None,
    }
}

impl CanvasBuilder {
    pub fn w(mut self, w: impl IntoCssDim) -> Self {
        self.has_w = true;
        self.el = self.el.w(w);
        self
    }

    pub fn h(mut self, h: impl IntoCssDim) -> Self {
        self.has_h = true;
        self.el = self.el.h(h);
        self
    }

    pub fn paint<F>(mut self, f: F) -> Self
    where
        F: FnMut(&mut dyn Canvas2D, CanvasSize) -> LayoutStatus + 'static,
    {
        self.paint = Some(Box::new(f));
        self
    }

    pub fn cursor(mut self, value: impl crate::style::IntoCursor) -> Self {
        self.el = self.el.cursor(value);
        self
    }

    pub fn opacity(mut self, o: f32) -> Self {
        self.el = self.el.opacity(o);
        self
    }

    pub fn on_mousedown(
        mut self,
        handler: impl FnMut(&crate::app::pointer_events::PointerEvent) + 'static,
    ) -> Self {
        self.el = self.el.on_mousedown(handler);
        self
    }

    pub fn on_mousemove(
        mut self,
        handler: impl FnMut(&crate::app::pointer_events::PointerEvent) + 'static,
    ) -> Self {
        self.el = self.el.on_mousemove(handler);
        self
    }

    pub fn on_mouseup(
        mut self,
        handler: impl FnMut(&crate::app::pointer_events::PointerEvent) + 'static,
    ) -> Self {
        self.el = self.el.on_mouseup(handler);
        self
    }

    /// 叶节点：子节点不进 taffy（与 Svg 形状不同，canvas 没有场景图子标签）。
    pub fn child(self, _child: impl Into<Element>) -> Self {
        self
    }

    pub fn children(self, _children: impl IntoIterator<Item = Element>) -> Self {
        self
    }
}

impl From<CanvasBuilder> for Element {
    fn from(builder: CanvasBuilder) -> Self {
        let CanvasBuilder {
            mut el,
            has_w,
            has_h,
            paint,
        } = builder;
        el.children.clear();
        if !has_w {
            el = el.w(DEFAULT_W);
        }
        if !has_h {
            el = el.h(DEFAULT_H);
        }
        if let Some(paint) = paint {
            let id = register_paint(paint);
            el.set_attr(ATTR_CANVAS_PAINT, format!("{id:x}"));
            el = el.on_unmount(move || unregister_paint(id));
        }
        el
    }
}

impl IntoElementList for CanvasBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

#[cfg(test)]
#[path = "tests/canvas.rs"]
mod tests;
