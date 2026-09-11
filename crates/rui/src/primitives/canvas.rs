//! # 公开 `<canvas>` 叶（WP-3.12）
//!
//! HTML `<canvas>` 心智：**这块像素我自己画**。回调跑在 UI 线程，缓冲是 CPU pixmap →
//! `queue_image_rgba8`，不是 `CanvasRenderingContext2D` 全家桶。
//!
//! `fill_path` / `stroke_path` 走与 Chart / Svg 同一 [`PathCommand`] 核。paint 内读 Signal
//! 必须 [`peek`](crate::core::reactivity::Signal::peek)（实现侧再包一层 [`untrack`]，
//! 避免误 `get()` 订到 UI 根 effect）。
//!
//! 双书写：[`canvas`] Builder；`ui!` `<canvas paint={…}>`（见 `rui_macros`）。

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::core::reactivity::untrack;
use crate::core::tree::{Element, IntoCssDim, IntoElementList, LayoutStatus};
use crate::primitives::path::{
    filled_rect, paint_path_premul, polygon, premul_to_straight_rgba, src_over_premul, PathCommand,
    PathStyle, Point2,
};
use crate::style::parse_color;

/// 叶上登记 paint 回调的 attr（hex id）。
pub const ATTR_CANVAS_PAINT: &str = "rui_canvas_paint";

/// 画布逻辑尺寸（叶 CSS px；MVP 不做 HTML 那种 bitmap ≠ CSS 分叉）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CanvasSize {
    pub w: f32,
    pub h: f32,
}

/// 规范 `docs/drawing.md` §4 里的 `Size`。
pub type Size = CanvasSize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineCap {
    Butt,
    Round,
    Square,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineJoin {
    Miter,
    Round,
    Bevel,
}

/// CPU-visible image source for `draw_image`. GPU texture keys may be wired later by
/// the unified resource graph; the software canvas currently draws RGBA buffers directly.
pub enum CanvasImageSource<'a> {
    Rgba {
        width: u32,
        height: u32,
        rgba: &'a [u8],
    },
    TextureKey(&'a str),
}

#[derive(Clone, Debug, Default)]
pub struct CanvasPath2D {
    cmds: Vec<PathCommand>,
}

impl CanvasPath2D {
    pub fn new() -> Self {
        Self { cmds: Vec::new() }
    }

    pub fn move_to(&mut self, x: f32, y: f32) {
        self.cmds.push(PathCommand::MoveTo((x, y)));
    }

    pub fn line_to(&mut self, x: f32, y: f32) {
        self.cmds.push(PathCommand::LineTo((x, y)));
    }

    pub fn quadratic_curve_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        let p0 = self
            .cmds
            .iter()
            .rev()
            .find_map(|c| match *c {
                PathCommand::MoveTo(p) | PathCommand::LineTo(p) => Some(p),
                PathCommand::Close => None,
            })
            .unwrap_or((x, y));
        for i in 1..=12 {
            let t = i as f32 / 12.0;
            let mt = 1.0 - t;
            let q = (
                mt * mt * p0.0 + 2.0 * mt * t * cx + t * t * x,
                mt * mt * p0.1 + 2.0 * mt * t * cy + t * t * y,
            );
            self.cmds.push(PathCommand::LineTo(q));
        }
    }

    pub fn bezier_curve_to(&mut self, c1x: f32, c1y: f32, c2x: f32, c2y: f32, x: f32, y: f32) {
        let p0 = self
            .cmds
            .iter()
            .rev()
            .find_map(|c| match *c {
                PathCommand::MoveTo(p) | PathCommand::LineTo(p) => Some(p),
                PathCommand::Close => None,
            })
            .unwrap_or((x, y));
        for i in 1..=16 {
            let t = i as f32 / 16.0;
            let mt = 1.0 - t;
            let q = (
                mt * mt * mt * p0.0
                    + 3.0 * mt * mt * t * c1x
                    + 3.0 * mt * t * t * c2x
                    + t * t * t * x,
                mt * mt * mt * p0.1
                    + 3.0 * mt * mt * t * c1y
                    + 3.0 * mt * t * t * c2y
                    + t * t * t * y,
            );
            self.cmds.push(PathCommand::LineTo(q));
        }
    }

    pub fn arc(&mut self, cx: f32, cy: f32, r: f32, start_rad: f32, end_rad: f32) {
        let span = end_rad - start_rad;
        let steps = ((span.abs() / std::f32::consts::TAU) * 32.0)
            .ceil()
            .max(8.0) as usize;
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let a = start_rad + span * t;
            let p = (cx + r * a.cos(), cy + r * a.sin());
            if i == 0 && self.cmds.is_empty() {
                self.cmds.push(PathCommand::MoveTo(p));
            } else {
                self.cmds.push(PathCommand::LineTo(p));
            }
        }
    }

    pub fn close_path(&mut self) {
        self.cmds.push(PathCommand::Close);
    }

    pub fn commands(&self) -> &[PathCommand] {
        &self.cmds
    }
}

/// 2D 立即模式上下文（MVP：clear / 矩形 / path / 变换 / clip）。
pub trait Canvas2D {
    fn size(&self) -> CanvasSize;
    fn set_fill(&mut self, color: &str);
    fn set_stroke(&mut self, color: &str);
    fn set_line_width(&mut self, width: f32);
    fn set_line_cap(&mut self, _cap: LineCap) {}
    fn set_line_join(&mut self, _join: LineJoin) {}
    /// 整缓冲重置为 `color`（忽略当前变换与 clip）。
    fn clear(&mut self, color: &str);
    fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32);
    fn stroke_rect(&mut self, x: f32, y: f32, w: f32, h: f32);
    fn fill_path(&mut self, cmds: &[PathCommand]);
    fn stroke_path(&mut self, cmds: &[PathCommand]);
    fn translate(&mut self, dx: f32, dy: f32);
    /// 绕当前原点旋转；单位为**度**（与 RUI `rotate_deg` / Svg transform 一致，不是 HTML 弧度）。
    fn rotate(&mut self, deg: f32);
    fn scale(&mut self, sx: f32, sy: f32);
    /// 与当前 clip 求交。旋转后取变换后 AABB（MVP）。
    fn clip_rect(&mut self, x: f32, y: f32, w: f32, h: f32);
    fn clip(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.clip_rect(x, y, w, h);
    }
    fn save(&mut self);
    fn restore(&mut self);
    fn draw_image(&mut self, _src: &CanvasImageSource<'_>, _dx: f32, _dy: f32, _dw: f32, _dh: f32) {
    }
    fn get_image_data(&self, _x: u32, _y: u32, _w: u32, _h: u32) -> Option<Vec<u8>> {
        None
    }
    fn put_image_data(&mut self, _x: u32, _y: u32, _w: u32, _h: u32, _rgba: &[u8]) {}
    fn is_point_in_path(&self, _path: &[PathCommand], _x: f32, _y: f32) -> bool {
        false
    }
    /// 下一帧继续 paint（时钟/粒子）。返回 [`LayoutStatus::Clean`] 且不调用本方法时，布局收集可复用上一张贴图。
    fn request_frame(&mut self) {
        let _ = self;
    }
}

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

/// `CursorMoved` / 按下 在 dispatch 之后取走：需要 `window.request_redraw`。
///
/// Also take **after** layout collect / `render`: `request_frame` sets the flag
/// during paint, which runs inside collect. Taking before collect drops the pulse
/// and a clock canvas freezes until the next pointer event.
pub(crate) fn take_canvas_wants_redraw() -> bool {
    WANT_REDRAW.replace(false)
}

#[derive(Clone, Copy)]
struct Affine {
    a: f32,
    b: f32,
    c: f32,
    d: f32,
    e: f32,
    f: f32,
}

impl Affine {
    fn identity() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: 0.0,
            f: 0.0,
        }
    }

    fn translate(tx: f32, ty: f32) -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: tx,
            f: ty,
        }
    }

    fn scale(sx: f32, sy: f32) -> Self {
        Self {
            a: sx,
            b: 0.0,
            c: 0.0,
            d: sy,
            e: 0.0,
            f: 0.0,
        }
    }

    fn rotate_deg(deg: f32) -> Self {
        let r = deg.to_radians();
        let (s, c) = r.sin_cos();
        Self {
            a: c,
            b: s,
            c: -s,
            d: c,
            e: 0.0,
            f: 0.0,
        }
    }

    fn apply(self, p: Point2) -> Point2 {
        (
            self.a * p.0 + self.c * p.1 + self.e,
            self.b * p.0 + self.d * p.1 + self.f,
        )
    }

    /// `self` 在左：先 `other`，再 `self`。
    fn mul(self, other: Self) -> Self {
        Self {
            a: self.a * other.a + self.c * other.b,
            b: self.b * other.a + self.d * other.b,
            c: self.a * other.c + self.c * other.d,
            d: self.b * other.c + self.d * other.d,
            e: self.a * other.e + self.c * other.f + self.e,
            f: self.b * other.e + self.d * other.f + self.f,
        }
    }

    fn is_axis_aligned(self) -> bool {
        self.b.abs() < 1e-6 && self.c.abs() < 1e-6
    }

    fn scale_avg(self) -> f32 {
        ((self.a.hypot(self.b) + self.c.hypot(self.d)) * 0.5).max(0.25)
    }
}

fn apply_affine(cmds: &[PathCommand], xf: Affine) -> Vec<PathCommand> {
    cmds.iter()
        .map(|c| match *c {
            PathCommand::MoveTo(p) => PathCommand::MoveTo(xf.apply(p)),
            PathCommand::LineTo(p) => PathCommand::LineTo(xf.apply(p)),
            PathCommand::Close => PathCommand::Close,
        })
        .collect()
}

fn intersect_clip(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let x0 = a[0].max(b[0]);
    let y0 = a[1].max(b[1]);
    let x1 = (a[0] + a[2]).min(b[0] + b[2]);
    let y1 = (a[1] + a[3]).min(b[1] + b[3]);
    [x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0)]
}

#[derive(Clone, Copy)]
struct CanvasState {
    xf: Affine,
    clip: [f32; 4],
    fill: [f32; 4],
    stroke: [f32; 4],
    line_width: f32,
    line_cap: LineCap,
    line_join: LineJoin,
}

struct RasterCanvas {
    buf: Vec<f32>,
    width: u32,
    height: u32,
    state: CanvasState,
    stack: Vec<CanvasState>,
    want_frame: bool,
}

impl RasterCanvas {
    fn new(width: u32, height: u32) -> Self {
        let n = (width as usize) * (height as usize) * 4;
        Self {
            buf: vec![0.0f32; n],
            width,
            height,
            state: CanvasState {
                xf: Affine::identity(),
                clip: [0.0, 0.0, width as f32, height as f32],
                fill: [0.0, 0.0, 0.0, 1.0],
                stroke: [0.0, 0.0, 0.0, 1.0],
                line_width: 1.0,
                line_cap: LineCap::Round,
                line_join: LineJoin::Round,
            },
            stack: Vec::new(),
            want_frame: false,
        }
    }

    fn fill_device_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
        if w <= 0.0 || h <= 0.0 || color[3] <= 0.0 {
            return;
        }
        let clip = self.state.clip;
        let x0 = x.max(clip[0]).floor().max(0.0) as usize;
        let y0 = y.max(clip[1]).floor().max(0.0) as usize;
        let x1 = (x + w).min(clip[0] + clip[2]).ceil().min(self.width as f32) as usize;
        let y1 = (y + h)
            .min(clip[1] + clip[3])
            .ceil()
            .min(self.height as f32) as usize;
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let stride = self.width as usize;
        for py in y0..y1 {
            for px in x0..x1 {
                let i = (py * stride + px) * 4;
                src_over_premul(&mut self.buf, i, color[0], color[1], color[2], color[3]);
            }
        }
    }

    fn paint_cmds(&mut self, cmds: &[PathCommand], style: PathStyle) {
        if cmds.is_empty() {
            return;
        }
        let device = apply_affine(cmds, self.state.xf);
        paint_path_premul(
            &mut self.buf,
            self.width,
            self.height,
            &device,
            &style,
            Some(self.state.clip),
            1,
        );
    }
}

impl Canvas2D for RasterCanvas {
    fn size(&self) -> CanvasSize {
        CanvasSize {
            w: self.width as f32,
            h: self.height as f32,
        }
    }

    fn set_fill(&mut self, color: &str) {
        self.state.fill = parse_color(color);
    }

    fn set_stroke(&mut self, color: &str) {
        self.state.stroke = parse_color(color);
    }

    fn set_line_width(&mut self, width: f32) {
        self.state.line_width = width.max(0.0);
    }

    fn set_line_cap(&mut self, cap: LineCap) {
        self.state.line_cap = cap;
    }

    fn set_line_join(&mut self, join: LineJoin) {
        self.state.line_join = join;
    }

    fn clear(&mut self, color: &str) {
        let c = parse_color(color);
        let a = c[3].clamp(0.0, 1.0);
        let r = c[0] * a;
        let g = c[1] * a;
        let b = c[2] * a;
        for px in self.buf.chunks_exact_mut(4) {
            px[0] = r;
            px[1] = g;
            px[2] = b;
            px[3] = a;
        }
    }

    fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        if self.state.xf.is_axis_aligned() {
            let p0 = self.state.xf.apply((x, y));
            let p1 = self.state.xf.apply((x + w, y + h));
            let dx = (p1.0 - p0.0).abs();
            let dy = (p1.1 - p0.1).abs();
            self.fill_device_rect(p0.0.min(p1.0), p0.1.min(p1.1), dx, dy, self.state.fill);
        } else {
            self.paint_cmds(
                &filled_rect(x, y, w.max(1e-3), h.max(1e-3)),
                PathStyle::fill(self.state.fill),
            );
        }
    }

    fn stroke_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        self.stroke_path(&polygon(&[(x, y), (x + w, y), (x + w, y + h), (x, y + h)]));
    }

    fn fill_path(&mut self, cmds: &[PathCommand]) {
        self.paint_cmds(cmds, PathStyle::fill(self.state.fill));
    }

    fn stroke_path(&mut self, cmds: &[PathCommand]) {
        let sw = (self.state.line_width * self.state.xf.scale_avg()).max(0.5);
        self.paint_cmds(cmds, PathStyle::stroke(self.state.stroke, sw));
    }

    fn translate(&mut self, dx: f32, dy: f32) {
        self.state.xf = self.state.xf.mul(Affine::translate(dx, dy));
    }

    fn rotate(&mut self, deg: f32) {
        self.state.xf = self.state.xf.mul(Affine::rotate_deg(deg));
    }

    fn scale(&mut self, sx: f32, sy: f32) {
        self.state.xf = self.state.xf.mul(Affine::scale(sx, sy));
    }

    fn clip_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        if w <= 0.0 || h <= 0.0 {
            self.state.clip = [0.0, 0.0, 0.0, 0.0];
            return;
        }
        let corners = [
            self.state.xf.apply((x, y)),
            self.state.xf.apply((x + w, y)),
            self.state.xf.apply((x + w, y + h)),
            self.state.xf.apply((x, y + h)),
        ];
        let min_x = corners.iter().map(|p| p.0).fold(f32::MAX, f32::min);
        let min_y = corners.iter().map(|p| p.1).fold(f32::MAX, f32::min);
        let max_x = corners.iter().map(|p| p.0).fold(f32::MIN, f32::max);
        let max_y = corners.iter().map(|p| p.1).fold(f32::MIN, f32::max);
        let aabb = [
            min_x,
            min_y,
            (max_x - min_x).max(0.0),
            (max_y - min_y).max(0.0),
        ];
        self.state.clip = intersect_clip(self.state.clip, aabb);
    }

    fn save(&mut self) {
        self.stack.push(self.state);
    }

    fn restore(&mut self) {
        if let Some(s) = self.stack.pop() {
            self.state = s;
        }
    }

    fn draw_image(&mut self, src: &CanvasImageSource<'_>, dx: f32, dy: f32, dw: f32, dh: f32) {
        let CanvasImageSource::Rgba {
            width,
            height,
            rgba,
        } = src
        else {
            return;
        };
        if *width == 0 || *height == 0 || dw <= 0.0 || dh <= 0.0 {
            return;
        }
        let x0 = dx.floor().max(0.0) as i32;
        let y0 = dy.floor().max(0.0) as i32;
        let x1 = (dx + dw).ceil().min(self.width as f32) as i32;
        let y1 = (dy + dh).ceil().min(self.height as f32) as i32;
        for py in y0.max(0)..y1.max(0) {
            for px in x0.max(0)..x1.max(0) {
                let u = (((px as f32 + 0.5) - dx) / dw).clamp(0.0, 0.999_999);
                let v = (((py as f32 + 0.5) - dy) / dh).clamp(0.0, 0.999_999);
                let sx = (u * *width as f32) as usize;
                let sy = (v * *height as f32) as usize;
                let si = (sy * *width as usize + sx) * 4;
                if si + 3 >= rgba.len() {
                    continue;
                }
                let a = rgba[si + 3] as f32 / 255.0;
                let i = (py as usize * self.width as usize + px as usize) * 4;
                src_over_premul(
                    &mut self.buf,
                    i,
                    rgba[si] as f32 / 255.0,
                    rgba[si + 1] as f32 / 255.0,
                    rgba[si + 2] as f32 / 255.0,
                    a,
                );
            }
        }
    }

    fn get_image_data(&self, x: u32, y: u32, w: u32, h: u32) -> Option<Vec<u8>> {
        if w == 0 || h == 0 || x >= self.width || y >= self.height {
            return None;
        }
        let rw = w.min(self.width - x);
        let rh = h.min(self.height - y);
        let mut out = vec![0u8; rw as usize * rh as usize * 4];
        for row in 0..rh as usize {
            for col in 0..rw as usize {
                let si = (((y as usize + row) * self.width as usize) + (x as usize + col)) * 4;
                let di = (row * rw as usize + col) * 4;
                let a = self.buf[si + 3].clamp(0.0, 1.0);
                out[di + 3] = (a * 255.0).round() as u8;
                if a > 1e-6 {
                    let inv = 1.0 / a;
                    out[di] = (self.buf[si] * inv * 255.0).round().clamp(0.0, 255.0) as u8;
                    out[di + 1] = (self.buf[si + 1] * inv * 255.0).round().clamp(0.0, 255.0) as u8;
                    out[di + 2] = (self.buf[si + 2] * inv * 255.0).round().clamp(0.0, 255.0) as u8;
                }
            }
        }
        Some(out)
    }

    fn put_image_data(&mut self, x: u32, y: u32, w: u32, h: u32, rgba: &[u8]) {
        if rgba.len() < w as usize * h as usize * 4 {
            return;
        }
        for row in 0..h as usize {
            for col in 0..w as usize {
                let dx = x as usize + col;
                let dy = y as usize + row;
                if dx >= self.width as usize || dy >= self.height as usize {
                    continue;
                }
                let si = (row * w as usize + col) * 4;
                let di = (dy * self.width as usize + dx) * 4;
                let a = rgba[si + 3] as f32 / 255.0;
                self.buf[di] = rgba[si] as f32 / 255.0 * a;
                self.buf[di + 1] = rgba[si + 1] as f32 / 255.0 * a;
                self.buf[di + 2] = rgba[si + 2] as f32 / 255.0 * a;
                self.buf[di + 3] = a;
            }
        }
    }

    fn request_frame(&mut self) {
        self.want_frame = true;
    }
}

const MAX_CANVAS_PX: u32 = 1024 * 1024;
const DEFAULT_W: f32 = 300.0;
const DEFAULT_H: f32 = 150.0;

fn run_paint(id: u64, width: u32, height: u32) -> Option<(Vec<f32>, LayoutStatus)> {
    let mut canvas = RasterCanvas::new(width, height);
    let size = CanvasSize {
        w: width as f32,
        h: height as f32,
    };
    let status = PAINT_STORE.with(|s| {
        let mut g = s.borrow_mut();
        let paint = g.get_mut(&id)?;
        Some(untrack(|| paint(&mut canvas, size)))
    })?;
    let status = if canvas.want_frame {
        // Keep PAINT_DIRTY *and* wake the window. StyleChanged alone does not
        // request_redraw; without WANT_REDRAW a clock canvas paints once then freezes.
        WANT_REDRAW.set(true);
        status.merge(LayoutStatus::StyleChanged)
    } else {
        status
    };
    Some((canvas.buf, status))
}

/// 布局期：dirty 或尺寸变化时跑 paint → CPU pixmap → `rui_canvas:{id}`。
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
    let (premul, status) = run_paint(id, w, h)?;
    let keep_dirty = !matches!(status, LayoutStatus::Clean);
    PAINT_DIRTY.with(|d| {
        let mut g = d.borrow_mut();
        if keep_dirty {
            g.insert(id);
        } else {
            g.remove(&id);
        }
    });
    let rgba = premul_to_straight_rgba(&premul, w, h);
    let tex_id = format!("rui_canvas:{id:016x}");
    crate::queue_image_rgba8(tex_id.clone(), w, h, rgba);
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
    let (buf, _) = run_paint(id, w, h)?;
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
mod tests {
    use super::*;
    use crate::primitives::path::polyline;

    fn sample_px(buf: &[f32], w: u32, x: u32, y: u32) -> [f32; 4] {
        let i = (y as usize * w as usize + x as usize) * 4;
        [buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]
    }

    #[test]
    fn fill_rect_covers_expected_pixels() {
        let el: Element = canvas()
            .w(32.0)
            .h(16.0)
            .paint(|ctx, _| {
                ctx.clear("#00000000");
                ctx.set_fill("#ff0000");
                ctx.fill_rect(2.0, 2.0, 8.0, 6.0);
                LayoutStatus::Clean
            })
            .into();
        let (buf, w, h) = rasterize_canvas_premul_for_test(&el.attrs, 32.0, 16.0).expect("raster");
        assert_eq!((w, h), (32, 16));
        let p = sample_px(&buf, w, 5, 4);
        assert!(p[0] > 0.8 && p[3] > 0.8, "fill inside rect, got {p:?}");
        let outside = sample_px(&buf, w, 20, 4);
        assert!(
            outside[3] < 0.05,
            "outside rect must stay empty, {outside:?}"
        );
    }

    #[test]
    fn translate_shifts_fill() {
        let el: Element = canvas()
            .w(24.0)
            .h(16.0)
            .paint(|ctx, _| {
                ctx.set_fill("#00ff00");
                ctx.translate(10.0, 0.0);
                ctx.fill_rect(0.0, 2.0, 4.0, 4.0);
                LayoutStatus::Clean
            })
            .into();
        let (buf, w, _) = rasterize_canvas_premul_for_test(&el.attrs, 24.0, 16.0).expect("raster");
        let shifted = sample_px(&buf, w, 12, 4);
        assert!(shifted[1] > 0.8, "translated fill, {shifted:?}");
        let origin = sample_px(&buf, w, 1, 4);
        assert!(origin[3] < 0.05, "unshifted origin empty, {origin:?}");
    }

    #[test]
    fn clip_rect_hides_pixels_outside() {
        let el: Element = canvas()
            .w(24.0)
            .h(16.0)
            .paint(|ctx, _| {
                ctx.clip_rect(0.0, 0.0, 6.0, 16.0);
                ctx.set_fill("#0000ff");
                ctx.fill_rect(0.0, 0.0, 24.0, 16.0);
                LayoutStatus::Clean
            })
            .into();
        let (buf, w, _) = rasterize_canvas_premul_for_test(&el.attrs, 24.0, 16.0).expect("raster");
        let inside = sample_px(&buf, w, 2, 8);
        assert!(inside[2] > 0.8, "inside clip, {inside:?}");
        let outside = sample_px(&buf, w, 16, 8);
        assert!(outside[3] < 0.05, "outside clip empty, {outside:?}");
    }

    #[test]
    fn stroke_path_uses_path_command_kernel() {
        let el: Element = canvas()
            .w(40.0)
            .h(20.0)
            .paint(|ctx, _| {
                ctx.set_stroke("#ff00ff");
                ctx.set_line_width(3.0);
                ctx.stroke_path(&polyline(&[(2.0, 10.0), (38.0, 10.0)]));
                LayoutStatus::Clean
            })
            .into();
        let (buf, w, _) = rasterize_canvas_premul_for_test(&el.attrs, 40.0, 20.0).expect("raster");
        let mid = sample_px(&buf, w, 20, 10);
        assert!(mid[0] > 0.4 && mid[2] > 0.4, "stroke coverage, {mid:?}");
    }

    #[test]
    fn leaf_has_no_taffy_children() {
        let el: Element = canvas()
            .w(40.0)
            .h(20.0)
            .paint(|ctx, _| {
                ctx.clear("secondary");
                LayoutStatus::Clean
            })
            .child(Element::new().text("nope"))
            .into();
        assert!(el.children.is_empty());
        assert!(el.get_attr(ATTR_CANVAS_PAINT).is_some());
    }

    #[test]
    fn clean_paint_is_cached_until_dirty() {
        use std::cell::Cell;
        use std::rc::Rc;
        let runs = Rc::new(Cell::new(0u32));
        let el: Element = canvas()
            .w(24.0)
            .h(16.0)
            .paint({
                let runs = Rc::clone(&runs);
                move |ctx, _| {
                    runs.set(runs.get() + 1);
                    ctx.clear("secondary");
                    LayoutStatus::Clean
                }
            })
            .into();
        canvas_raster_from_element_attrs(&el.attrs, 24.0, 16.0).expect("first");
        canvas_raster_from_element_attrs(&el.attrs, 24.0, 16.0).expect("cached");
        assert_eq!(runs.get(), 1, "Clean paint must not rerun on every collect");
        mark_canvas_dirty_hex(el.get_attr(ATTR_CANVAS_PAINT).unwrap());
        canvas_raster_from_element_attrs(&el.attrs, 24.0, 16.0).expect("dirty");
        assert_eq!(runs.get(), 2, "mark_canvas_dirty_hex must force a repaint");
    }

    #[test]
    fn mark_canvas_dirty_sets_redraw_flag() {
        let _ = take_canvas_wants_redraw();
        let el: Element = canvas()
            .w(8.0)
            .h(8.0)
            .paint(|ctx, _| {
                ctx.clear("secondary");
                LayoutStatus::Clean
            })
            .into();
        let hex = el.get_attr(ATTR_CANVAS_PAINT).expect("paint id");
        let _ = take_canvas_wants_redraw();
        mark_canvas_dirty_hex(hex);
        assert!(
            take_canvas_wants_redraw(),
            "pointer dirty must wake a frame; CursorMoved does not redraw by default"
        );
        assert!(!take_canvas_wants_redraw(), "flag is edge-triggered");
    }

    #[test]
    fn request_frame_keeps_dirty_and_wakes_redraw() {
        use std::cell::Cell;
        use std::rc::Rc;
        let runs = Rc::new(Cell::new(0u32));
        let _ = take_canvas_wants_redraw();
        let el: Element = canvas()
            .w(8.0)
            .h(8.0)
            .paint({
                let runs = Rc::clone(&runs);
                move |ctx, _| {
                    runs.set(runs.get() + 1);
                    ctx.request_frame();
                    LayoutStatus::Clean
                }
            })
            .into();
        canvas_raster_from_element_attrs(&el.attrs, 8.0, 8.0).expect("first");
        assert!(
            take_canvas_wants_redraw(),
            "request_frame must wake the window, not only keep PAINT_DIRTY"
        );
        canvas_raster_from_element_attrs(&el.attrs, 8.0, 8.0).expect("second");
        assert_eq!(
            runs.get(),
            2,
            "request_frame must keep the leaf dirty so the next collect repaints"
        );
        assert!(
            take_canvas_wants_redraw(),
            "the window event loop must take this *after* collect, not before"
        );
    }
}
