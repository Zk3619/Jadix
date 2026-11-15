//! Path geometry, CPU rasterization, hit testing and GPU mesh preparation.
pub use crate::geometry::{PathBooleanError, PathBooleanOp, PathBooleanOptions, PathGeometry2D};
pub use crate::stroke::{stroke_outline, StrokeOptions};
use jadix_core::path::{arc_segments, dist_to_segment};
pub use jadix_core::path::{
    decode_path_d, encode_path_d, flatten_path, flatten_path_with_tolerance, PathCommand, Point2,
};
use jadix_theme::color::parse_color;
/// CSS/SVG `stroke-linecap`。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum StrokeLinecap {
    Butt = 0,
    #[default]
    Round = 1,
    Square = 2,
}

impl StrokeLinecap {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "butt" => Self::Butt,
            "square" => Self::Square,
            _ => Self::Round,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Butt => "butt",
            Self::Round => "round",
            Self::Square => "square",
        }
    }
}

/// CSS/SVG `stroke-linejoin`。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum StrokeLinejoin {
    Miter = 0,
    #[default]
    Round = 1,
    Bevel = 2,
}

impl StrokeLinejoin {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "miter" => Self::Miter,
            "bevel" => Self::Bevel,
            _ => Self::Round,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Miter => "miter",
            Self::Round => "round",
            Self::Bevel => "bevel",
        }
    }
}

/// CSS/SVG `fill-rule`。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum FillRule {
    #[default]
    NonZero = 0,
    EvenOdd = 1,
}

impl FillRule {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "evenodd" => Self::EvenOdd,
            _ => Self::NonZero,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::NonZero => "nonzero",
            Self::EvenOdd => "evenodd",
        }
    }
}

/// 描边 / 填充样式。
#[derive(Clone, Debug, PartialEq)]
pub struct PathStyle {
    pub fill: Option<[f32; 4]>,
    /// When set, fill color is sampled from this gradient (object-bbox normalized).
    /// `fill` still carries opacity (alpha multiplied in the painter).
    pub fill_gradient: Option<crate::gradient::Gradient>,
    pub stroke: Option<[f32; 4]>,
    pub stroke_width: f32,
    pub stroke_options: StrokeOptions,
    pub stroke_linecap: StrokeLinecap,
    pub stroke_linejoin: StrokeLinejoin,
    pub fill_rule: FillRule,
}

impl PathStyle {
    pub fn stroke(color: [f32; 4], width: f32) -> Self {
        Self {
            fill: None,
            fill_gradient: None,
            stroke: Some(color),
            stroke_width: width.max(0.0),
            stroke_options: StrokeOptions::default(),
            stroke_linecap: StrokeLinecap::Round,
            stroke_linejoin: StrokeLinejoin::Round,
            fill_rule: FillRule::NonZero,
        }
    }

    pub fn fill(color: [f32; 4]) -> Self {
        Self {
            fill: Some(color),
            fill_gradient: None,
            stroke: None,
            stroke_width: 0.0,
            stroke_options: StrokeOptions::default(),
            stroke_linecap: StrokeLinecap::Round,
            stroke_linejoin: StrokeLinejoin::Round,
            fill_rule: FillRule::NonZero,
        }
    }

    pub fn fill_stroke(fill: [f32; 4], stroke: [f32; 4], width: f32) -> Self {
        Self {
            fill: Some(fill),
            fill_gradient: None,
            stroke: Some(stroke),
            stroke_width: width.max(0.0),
            stroke_options: StrokeOptions::default(),
            stroke_linecap: StrokeLinecap::Round,
            stroke_linejoin: StrokeLinejoin::Round,
            fill_rule: FillRule::NonZero,
        }
    }

    pub fn dashes(mut self, pattern: &[f32], offset: f32) -> Self {
        if let Some(dashes) = StrokeOptions::normalized_dashes(pattern) {
            self.stroke_options.dash_array = dashes;
        }
        if offset.is_finite() {
            self.stroke_options.dash_offset = offset;
        }
        self
    }

    pub fn miter_limit(mut self, limit: f32) -> Self {
        if limit.is_finite() && limit >= 1.0 {
            self.stroke_options.miter_limit = limit;
        }
        self
    }

    pub fn linecap(mut self, c: StrokeLinecap) -> Self {
        self.stroke_linecap = c;
        self
    }

    pub fn linejoin(mut self, j: StrokeLinejoin) -> Self {
        self.stroke_linejoin = j;
        self
    }
}

impl Default for PathStyle {
    fn default() -> Self {
        Self::stroke(parse_color("foreground"), 2.0)
    }
}

/// Tessellate 产出：与 the framework instance input 对齐的旋转四边形。
/// S3 产品核改走 [`PathTri`] / PathPipeline；本类型只留作极窄 stroke 回退与测试。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathQuad {
    /// 未旋转矩形左上角（与 `InstanceInput.rect_pos` 同语义）。
    pub pos: Point2,
    pub size: Point2,
    pub rotate_deg: f32,
    pub color: [f32; 4],
    pub radii: [f32; 4],
}

/// PathPipeline 三角形（叶局部像素，y 向下）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathTri {
    pub a: Point2,
    pub b: Point2,
    pub c: Point2,
    pub color: [f32; 4],
}

/// S4 cover pass：线性 / 径向 / 锥形及重复渐变（hints / px-stop 仍 CPU）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GpuFillGrad {
    /// Device pixels to brush-local coordinates; identity for legacy bbox gradients.
    pub device_to_brush: [f32; 6],
    /// 1..=3: gradients; 5..=7: repeating gradients; 8: Gaussian rounded rect.
    /// For kind 8, bbox is the local rect and center contains radius/std_dev.
    /// Kind 9: same bbox/center; stop_t[0..4] are TL/TR/BR/BL radii and angle
    /// is 1 for inset, 0 for outer. Kind 8 retains its original internal ABI.
    pub kind: u32,
    pub angle: f32,
    pub bbox: [f32; 4],
    pub center: [f32; 2],
    pub radii: [f32; 2],
    pub stop_n: u32,
    pub stop_t: [f32; 8],
    pub stop_c: [[f32; 4]; 8],
}

#[derive(Clone, Debug, PartialEq)]
pub struct PathGpuFill {
    /// Non-overlapping triangles: render directly without stencil/cover.
    pub direct: bool,
    pub even_odd: bool,
    pub stencil: Vec<PathTri>,
    pub cover: [PathTri; 2],
    pub grad: Option<GpuFillGrad>,
    /// Exact immutable vector stops; geometry/transform are in grad kinds 10..12.
    pub paint: Option<crate::VectorGradient>,
    /// Leaf-pixel AABB scissor. `None` = full RT.
    pub clip: Option<[f32; 4]>,
    pub mask: Option<crate::Image2D>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PathGpuMesh {
    pub tris: Vec<PathTri>,
    pub clip: Option<[f32; 4]>,
    pub mask: Option<crate::Image2D>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PathGpuImage {
    pub image: crate::Image2D,
    /// Source texel crop; None paints the whole image. Must lie inside image bounds.
    pub source_rect: Option<[u32; 4]>,
    /// Top-left, top-right, bottom-right, bottom-left, in device pixels.
    pub corners: [Point2; 4],
    pub opacity: f32,
    pub sampling: crate::ImageSampling,
    pub clip: Option<[f32; 4]>,
    pub mask: Option<crate::Image2D>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PathGpuOp {
    Fill(PathGpuFill),
    Mesh(PathGpuMesh),
    Image(PathGpuImage),
    Pattern(PathGpuPattern),
    Layer(PathGpuLayer),
}

/// A tessellated fill with an image brush. Canvas prepares the inverse affine.
#[derive(Clone, Debug, PartialEq)]
pub struct PathGpuPattern {
    pub fill: PathGpuFill,
    pub image: crate::Image2D,
    pub device_to_brush: [f32; 6],
    pub extend_x: crate::ExtendMode,
    pub extend_y: crate::ExtendMode,
    pub sampling: crate::ImageSampling,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PathGpuLayer {
    pub ops: Vec<PathGpuOp>,
    pub opacity: f32,
    pub blend: crate::BlendMode,
    pub compose: crate::CompositeMode,
    pub clip: Option<[f32; 4]>,
    pub mask: Option<crate::Image2D>,
    pub filters: Vec<crate::LayerFilter>,
}

impl PathGpuLayer {
    pub fn new(ops: Vec<PathGpuOp>, options: crate::LayerOptions) -> Self {
        Self {
            ops,
            opacity: options.opacity,
            blend: options.blend,
            compose: options.compose,
            clip: options.clip,
            mask: options.mask,
            filters: options.filters,
        }
    }
}

impl PathGpuOp {
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Fill(f) => f.stencil.is_empty(),
            Self::Mesh(m) => m.tris.is_empty(),
            Self::Image(_) => false,
            Self::Pattern(p) => p.fill.stencil.is_empty(),
            Self::Layer(l) => {
                l.compose.preserves_destination_with_empty_source()
                    && (l.opacity <= 0.
                        || (l.ops.iter().all(Self::is_empty)
                            && !l.filters.iter().any(crate::LayerFilter::creates_content)))
            }
        }
    }

    pub fn clip(&self) -> Option<[f32; 4]> {
        match self {
            Self::Fill(f) => f.clip,
            Self::Mesh(m) => m.clip,
            Self::Image(m) => m.clip,
            Self::Pattern(p) => p.fill.clip,
            Self::Layer(_) => None,
        }
    }
    pub fn mask(&self) -> Option<&crate::Image2D> {
        match self {
            Self::Fill(f) => f.mask.as_ref(),
            Self::Mesh(m) => m.mask.as_ref(),
            Self::Image(i) => i.mask.as_ref(),
            Self::Pattern(p) => p.fill.mask.as_ref(),
            Self::Layer(_) => None,
        }
    }
}

/// 折线 → 命令列表。
pub fn polyline(points: &[Point2]) -> Vec<PathCommand> {
    let mut out = Vec::with_capacity(points.len().saturating_add(1));
    let mut iter = points.iter();
    if let Some(&p0) = iter.next() {
        out.push(PathCommand::MoveTo(p0));
        for &p in iter {
            out.push(PathCommand::LineTo(p));
        }
    }
    out
}

/// 闭合多边形。
pub fn polygon(points: &[Point2]) -> Vec<PathCommand> {
    let mut cmds = polyline(points);
    if !cmds.is_empty() {
        cmds.push(PathCommand::Close);
    }
    cmds
}

fn push_pen(out: &mut Vec<PathCommand>, p: Point2) {
    match out.last().and_then(PathCommand::pen) {
        None => out.push(PathCommand::MoveTo(p)),
        Some(q) if (q.0 - p.0).abs() > 1e-4 || (q.1 - p.1).abs() > 1e-4 => {
            out.push(PathCommand::LineTo(p));
        }
        Some(_) => {}
    }
}

/// 饼图扇区：圆心 + 半径 + 起止角（弧度，0 = 正右；屏幕 y 向下时增大角顺时针）。
/// `segments` 保留给调用方；展平段数由 [`flatten_path`] 按像素误差决定。
pub fn pie_sector(
    cx: f32,
    cy: f32,
    r: f32,
    start: f32,
    end: f32,
    segments: usize,
) -> Vec<PathCommand> {
    let _ = segments;
    let r = r.max(0.0);
    let start_p = (cx + r * start.cos(), cy + r * start.sin());
    let end_p = (cx + r * end.cos(), cy + r * end.sin());
    let span = end - start;
    if r < 1e-6 || span.abs() < 1e-6 {
        return polygon(&[(cx, cy), start_p, end_p]);
    }
    vec![
        PathCommand::MoveTo((cx, cy)),
        PathCommand::LineTo(start_p),
        PathCommand::Arc {
            radii: (r, r),
            x_rot_deg: 0.0,
            large: span.abs() > std::f32::consts::PI,
            sweep: span > 0.0,
            to: end_p,
        },
        PathCommand::Close,
    ]
}

/// 实心椭圆：两段半椭圆 [`PathCommand::Arc`]。`segments` 保留给调用方；
/// 展平段数由 [`flatten_path`] 按像素误差决定。
pub fn filled_ellipse(cx: f32, cy: f32, rx: f32, ry: f32, segments: usize) -> Vec<PathCommand> {
    let _ = segments;
    let rx = rx.max(0.5);
    let ry = ry.max(0.5);
    let a = (cx + rx, cy);
    let b = (cx - rx, cy);
    vec![
        PathCommand::MoveTo(a),
        PathCommand::Arc {
            radii: (rx, ry),
            x_rot_deg: 0.0,
            large: false,
            sweep: true,
            to: b,
        },
        PathCommand::Arc {
            radii: (rx, ry),
            x_rot_deg: 0.0,
            large: false,
            sweep: true,
            to: a,
        },
        PathCommand::Close,
    ]
}

/// 实心圆：两段半圆 [`PathCommand::Arc`]（不含圆心顶点），fill 为盘、stroke 为圆周。
/// 点 mark 必须用这个而不是 [`pie_sector`] 满周——后者 stroke 会画出半径。
/// `segments` 保留给调用方；展平段数由 [`flatten_path`] 按像素误差决定。
pub fn filled_circle(cx: f32, cy: f32, r: f32, segments: usize) -> Vec<PathCommand> {
    filled_ellipse(cx, cy, r, r, segments)
}

/// 轴对齐矩形（柱、气泡 backing）。
pub fn filled_rect(x: f32, y: f32, w: f32, h: f32) -> Vec<PathCommand> {
    if ![x, y, w, h].iter().all(|v| v.is_finite()) || w <= 0. || h <= 0. {
        return Vec::new();
    }
    polygon(&[(x, y), (x + w, y), (x + w, y + h), (x, y + h)])
}

/// 轴对齐圆角矩形。`radii` = `[tl, tr, br, bl]`，各角自动钳到宽高一半。
pub fn rounded_rect(x: f32, y: f32, w: f32, h: f32, radii: [f32; 4]) -> Vec<PathCommand> {
    if ![x, y, w, h]
        .iter()
        .chain(radii.iter())
        .all(|v| v.is_finite())
        || w <= 0.
        || h <= 0.
    {
        return Vec::new();
    }
    let max_r = (w * 0.5).min(h * 0.5);
    let tl = radii[0].clamp(0.0, max_r);
    let tr = radii[1].clamp(0.0, max_r);
    let br = radii[2].clamp(0.0, max_r);
    let bl = radii[3].clamp(0.0, max_r);
    if tl + tr + br + bl == 0. {
        return filled_rect(x, y, w, h);
    }
    let mut cmds = Vec::new();
    let mut corner = |cx: f32, cy: f32, r: f32, a0: f32, span: f32| {
        if r == 0. {
            push_pen(&mut cmds, (cx, cy));
            return;
        }
        let start = (cx + r * a0.cos(), cy + r * a0.sin());
        let a1 = a0 + span;
        let end = (cx + r * a1.cos(), cy + r * a1.sin());
        push_pen(&mut cmds, start);
        cmds.push(PathCommand::Arc {
            radii: (r, r),
            x_rot_deg: 0.0,
            large: span.abs() > std::f32::consts::PI,
            sweep: span > 0.0,
            to: end,
        });
    };
    corner(
        x + tl,
        y + tl,
        tl,
        std::f32::consts::PI,
        std::f32::consts::FRAC_PI_2,
    );
    corner(
        x + w - tr,
        y + tr,
        tr,
        -std::f32::consts::FRAC_PI_2,
        std::f32::consts::FRAC_PI_2,
    );
    corner(x + w - br, y + h - br, br, 0.0, std::f32::consts::FRAC_PI_2);
    corner(
        x + bl,
        y + h - bl,
        bl,
        std::f32::consts::FRAC_PI_2,
        std::f32::consts::FRAC_PI_2,
    );
    if !cmds.is_empty() {
        cmds.push(PathCommand::Close);
    }
    cmds
}

/// 柱状图顶圆角矩形（底边贴基线、顶边左右圆角）。`r` 自动钳到宽高一半。
/// `y` 为顶边（较小 y），`h` 向下延伸。
pub fn rounded_rect_top(x: f32, y: f32, w: f32, h: f32, r: f32) -> Vec<PathCommand> {
    rounded_rect(x, y, w, h, [r, r, 0.0, 0.0])
}

/// 水平柱：右端（或左端，若 `!round_right`）圆角。
pub fn rounded_rect_end(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    r: f32,
    round_right: bool,
) -> Vec<PathCommand> {
    if round_right {
        rounded_rect(x, y, w, h, [0.0, r, r, 0.0])
    } else {
        rounded_rect(x, y, w, h, [r, 0.0, 0.0, r])
    }
}

/// 环形扇区（TanStack donut）：外弧 + 反向内弧。`inner >= outer` 时退回实心扇区。
pub fn pie_ring(
    cx: f32,
    cy: f32,
    outer: f32,
    inner: f32,
    start: f32,
    end: f32,
    segments: usize,
) -> Vec<PathCommand> {
    let outer = outer.max(0.0);
    let inner = inner.max(0.0);
    if inner < 1.0 || inner >= outer {
        return pie_sector(cx, cy, outer, start, end, segments);
    }
    let span = end - start;
    if outer < 1e-6 || span.abs() < 1e-6 {
        return pie_sector(cx, cy, outer, start, end, segments);
    }
    let o0 = (cx + outer * start.cos(), cy + outer * start.sin());
    let o1 = (cx + outer * end.cos(), cy + outer * end.sin());
    let i1 = (cx + inner * end.cos(), cy + inner * end.sin());
    let i0 = (cx + inner * start.cos(), cy + inner * start.sin());
    let large = span.abs() > std::f32::consts::PI;
    let sweep = span > 0.0;
    vec![
        PathCommand::MoveTo(o0),
        PathCommand::Arc {
            radii: (outer, outer),
            x_rot_deg: 0.0,
            large,
            sweep,
            to: o1,
        },
        PathCommand::LineTo(i1),
        PathCommand::Arc {
            radii: (inner, inner),
            x_rot_deg: 0.0,
            large,
            sweep: !sweep,
            to: i0,
        },
        PathCommand::Close,
    ]
}

/// 将点列编码为旧 attr（`x,y;x,y;…`）。
pub fn encode_points(points: &[Point2]) -> String {
    points
        .iter()
        .map(|(x, y)| format!("{x:.3},{y:.3}"))
        .collect::<Vec<_>>()
        .join(";")
}

pub fn decode_points(s: &str) -> Vec<Point2> {
    s.split(';')
        .filter_map(|pair| {
            let mut it = pair.split(',');
            let x = it.next()?.trim().parse().ok()?;
            let y = it.next()?.trim().parse().ok()?;
            Some((x, y))
        })
        .collect()
}

pub fn encode_path_color(c: [f32; 4]) -> String {
    format!("{:.4},{:.4},{:.4},{:.4}", c[0], c[1], c[2], c[3])
}

pub fn decode_path_color(s: &str) -> Option<[f32; 4]> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    if t.contains(',') {
        let mut it = t.split(',');
        let r = it.next()?.trim().parse().ok()?;
        let g = it.next()?.trim().parse().ok()?;
        let b = it.next()?.trim().parse().ok()?;
        let a = it.next().and_then(|x| x.trim().parse().ok()).unwrap_or(1.0);
        return Some([r, g, b, a]);
    }
    Some(parse_color(t))
}

/// Collect flattened path endpoints.
pub fn points_from_commands(cmds: &[PathCommand]) -> Vec<Point2> {
    let mut pts = Vec::new();
    for cmd in flatten_path(cmds) {
        match cmd {
            PathCommand::MoveTo(p) | PathCommand::LineTo(p) => pts.push(p),
            PathCommand::Close => {}
            PathCommand::QuadTo { .. } | PathCommand::CubicTo { .. } | PathCommand::Arc { .. } => {}
        }
    }
    pts
}

#[doc(hidden)]
pub fn bounds_size(points: &[Point2]) -> (f32, f32) {
    if points.is_empty() {
        return (0.0, 0.0);
    }
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    for &(x, y) in points {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    ((max_x - min_x).max(0.0), (max_y - min_y).max(0.0))
}

/// 子路径：每次 `MoveTo` 起一段；`Close` 闭合。曲线先 [`flatten_path`]。
#[doc(hidden)]
pub fn subpaths(cmds: &[PathCommand]) -> Vec<(Vec<Point2>, bool)> {
    let cmds = flatten_path(cmds);
    let mut out = Vec::new();
    let mut cur: Vec<Point2> = Vec::new();
    let mut start: Option<Point2> = None;
    let mut closed_start = None;
    for cmd in &cmds {
        match *cmd {
            PathCommand::MoveTo(p) => {
                if !cur.is_empty() {
                    out.push((std::mem::take(&mut cur), false));
                }
                cur.push(p);
                start = Some(p);
                closed_start = None;
            }
            PathCommand::LineTo(p) => {
                if cur.is_empty() {
                    if let Some(s) = closed_start.take() {
                        cur.push(s);
                        start = Some(s);
                    }
                }
                cur.push(p);
            }
            PathCommand::Close => {
                if let Some(s) = start {
                    let needs_close = match cur.last() {
                        Some(q) => (q.0 - s.0).abs() > 1e-4 || (q.1 - s.1).abs() > 1e-4,
                        None => true,
                    };
                    if needs_close {
                        cur.push(s);
                    }
                }
                if !cur.is_empty() {
                    out.push((std::mem::take(&mut cur), true));
                }
                if start.is_some() {
                    closed_start = start;
                }
                start = None;
            }
            PathCommand::QuadTo { .. } | PathCommand::CubicTo { .. } | PathCommand::Arc { .. } => {}
        }
    }
    if !cur.is_empty() {
        out.push((cur, false));
    }
    out
}

fn push_oriented_quad(
    center: Point2,
    length: f32,
    thickness: f32,
    rotate_deg: f32,
    color: [f32; 4],
    radii: [f32; 4],
    out: &mut Vec<PathQuad>,
) {
    if length <= 0.0 || thickness <= 0.0 || color[3] <= 0.0 {
        return;
    }
    out.push(PathQuad {
        pos: (center.0 - length * 0.5, center.1 - thickness * 0.5),
        size: (length, thickness),
        rotate_deg,
        color,
        radii,
    });
}

fn unit_dir(a: Point2, b: Point2) -> Option<(f32, f32)> {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let len = dx.hypot(dy);
    if len < 1e-4 {
        None
    } else {
        Some((dx / len, dy / len))
    }
}

fn tessellate_stroke_points(
    pts: &[Point2],
    width: f32,
    color: [f32; 4],
    cap: StrokeLinecap,
    join: StrokeLinejoin,
    closed: bool,
    out: &mut Vec<PathQuad>,
) {
    let w = width.max(0.75);
    let hw = w * 0.5;
    let round = [hw, hw, hw, hw];
    let n = pts.len();
    let mut segs: Vec<(Point2, Point2, f32, f32, f32)> = Vec::new();
    for win in pts.windows(2) {
        let a = win[0];
        let b = win[1];
        let Some((dx, dy)) = unit_dir(a, b) else {
            continue;
        };
        let len = (b.0 - a.0).hypot(b.1 - a.1);
        let angle = dy.atan2(dx).to_degrees();
        let mid = ((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5);
        push_oriented_quad(mid, len, w, angle, color, [0.0; 4], out);
        segs.push((a, b, dx, dy, angle));
    }
    if segs.is_empty() {
        return;
    }
    if !closed {
        let (a0, _, dx0, dy0, ang0) = segs[0];
        let (_, b1, dx1, dy1, ang1) = segs[segs.len() - 1];
        match cap {
            StrokeLinecap::Butt => {}
            StrokeLinecap::Square => {
                push_oriented_quad(
                    (a0.0 - dx0 * hw * 0.5, a0.1 - dy0 * hw * 0.5),
                    hw,
                    w,
                    ang0,
                    color,
                    [0.0; 4],
                    out,
                );
                push_oriented_quad(
                    (b1.0 + dx1 * hw * 0.5, b1.1 + dy1 * hw * 0.5),
                    hw,
                    w,
                    ang1,
                    color,
                    [0.0; 4],
                    out,
                );
            }
            StrokeLinecap::Round => {
                push_oriented_quad(a0, w, w, 0.0, color, round, out);
                push_oriented_quad(b1, w, w, 0.0, color, round, out);
            }
        }
    }
    if join == StrokeLinejoin::Round {
        let wrapped = closed && n >= 3;
        for (i, &p) in pts.iter().enumerate() {
            let is_end = !closed && (i == 0 || i + 1 == n);
            if is_end {
                continue;
            }
            if wrapped && i + 1 == n {
                continue;
            }
            push_oriented_quad(p, w, w, 0.0, color, round, out);
        }
    }
}

/// Even-odd 扫描线填充 → 轴对齐四边形。
/// 旋转大矩形近似三角形会在面积/扇区上产生巨大 overdraw（Gallery 折线/饼图伪影）。
fn tessellate_fill_polygon(pts: &[Point2], color: [f32; 4], out: &mut Vec<PathQuad>) {
    if pts.len() < 3 || color[3] <= 0.0 {
        return;
    }
    let mut min_x = f32::MAX;
    let mut max_x = f32::MIN;
    for &(x, _) in pts {
        min_x = min_x.min(x);
        max_x = max_x.max(x);
    }
    if !min_x.is_finite() || max_x - min_x < 0.5 {
        return;
    }
    let x0 = min_x.floor() as i32;
    let x1 = max_x.ceil() as i32;
    let n = pts.len();
    let mut open: Option<(f32, f32, f32, f32)> = None; // x, y, w, h
    let flush = |span: (f32, f32, f32, f32), out: &mut Vec<PathQuad>| {
        if span.2 > 0.0 && span.3 > 0.0 {
            // +1.25px 水平重叠：SDF 圆角管线在 1px 柱两侧 AA，否则 Gallery 出现竖纹。
            out.push(PathQuad {
                pos: (span.0, span.1),
                size: (span.2 + 1.25, span.3),
                rotate_deg: 0.0,
                color,
                radii: [0.0; 4],
            });
        }
    };
    for x in x0..x1 {
        let xf = x as f32 + 0.5;
        let mut ys: Vec<f32> = Vec::with_capacity(n);
        for i in 0..n {
            let a = pts[i];
            let b = pts[(i + 1) % n];
            if (a.0 - b.0).abs() < 1e-6 {
                continue;
            }
            let (l, r) = if a.0 < b.0 { (a, b) } else { (b, a) };
            if xf < l.0 || xf >= r.0 {
                continue;
            }
            let t = (xf - l.0) / (r.0 - l.0);
            ys.push(l.1 + t * (r.1 - l.1));
        }
        ys.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mut local: Vec<(f32, f32)> = Vec::new();
        for chunk in ys.chunks(2) {
            if chunk.len() < 2 {
                continue;
            }
            let top = chunk[0].min(chunk[1]);
            let h = (chunk[0] - chunk[1]).abs().max(1.0);
            local.push((top, h));
        }
        // 同一扫描列多段：先冲刷无法延续的 open，再尝试与第一段合并。
        if local.is_empty() {
            if let Some(span) = open.take() {
                flush(span, out);
            }
            continue;
        }
        let xf0 = x as f32;
        let mut used_open = false;
        if let Some(span) = open.as_mut() {
            if let Some(&(top, h)) = local.first() {
                if (span.0 + span.2 - xf0).abs() < 0.01
                    && (span.1 - top).abs() < 0.6
                    && (span.3 - h).abs() < 0.6
                {
                    span.2 += 1.0;
                    used_open = true;
                }
            }
        }
        if !used_open {
            if let Some(span) = open.take() {
                flush(span, out);
            }
            let (top, h) = local[0];
            open = Some((xf0, top, 1.0, h));
        }
        for &(top, h) in local.iter().skip(1) {
            flush((xf0, top, 1.0, h), out);
        }
    }
    if let Some(span) = open.take() {
        flush(span, out);
    }
}

/// 将命令 tessellate 为批处理四边形（局部坐标）。
pub fn tessellate_path(cmds: &[PathCommand], style: &PathStyle) -> Vec<PathQuad> {
    let mut out = Vec::new();
    if cmds.is_empty() {
        return out;
    }
    let paths = subpaths(cmds);
    if let Some(fill) = style.fill {
        for (pts, closed) in &paths {
            if *closed && pts.len() >= 3 {
                tessellate_fill_polygon(pts, fill, &mut out);
            }
        }
    }
    if let Some(stroke) = style.stroke {
        let w = style.stroke_width;
        let has_open = paths.iter().any(|(_, c)| !*c);
        for (pts, closed) in &paths {
            // 面积图：闭合环只填不描，避免底边再描一刀；开放折线才是描边。
            if (style.fill.is_some() || style.fill_gradient.is_some()) && has_open && *closed {
                continue;
            }
            tessellate_stroke_points(
                pts,
                w,
                stroke,
                style.stroke_linecap,
                style.stroke_linejoin,
                *closed,
                &mut out,
            );
        }
    }
    out
}

fn fan_fill(pts: &[Point2], color: [f32; 4], out: &mut Vec<PathTri>) {
    let pts = ring_pts(pts);
    if pts.len() < 3 || color[3] <= 0.0 {
        return;
    }
    let a = pts[0];
    for i in 1..pts.len() - 1 {
        out.push(PathTri {
            a,
            b: pts[i],
            c: pts[i + 1],
            color,
        });
    }
}

/// Sagitta bounds the distance from the arc to each chord. Unlike an arc-length
/// heuristic, this adds geometry only where it improves the rendered silhouette.
#[doc(hidden)]
pub fn disc_fan(center: Point2, radius: f32, color: [f32; 4], out: &mut Vec<PathTri>) {
    let r = radius.max(0.5);
    if color[3] <= 0.0 {
        return;
    }
    let tau = std::f32::consts::TAU;
    let n = arc_segments(r, tau);
    for i in 0..n {
        let a0 = i as f32 / n as f32 * tau;
        let a1 = (i + 1) as f32 / n as f32 * tau;
        out.push(PathTri {
            a: center,
            b: (center.0 + r * a0.cos(), center.1 + r * a0.sin()),
            c: (center.0 + r * a1.cos(), center.1 + r * a1.sin()),
            color,
        });
    }
}

/// Stroke mesh（填充走 stencil-cover，不经过这里）。
pub fn tessellate_path_mesh(
    cmds: &[PathCommand],
    style: &PathStyle,
    markers: &[Point2],
    marker_r: f32,
) -> Vec<PathTri> {
    let mut out = Vec::new();
    if cmds.is_empty() && markers.is_empty() {
        return out;
    }
    let paths = subpaths(cmds);
    if let Some(fill) = style.fill {
        if style.fill_gradient.is_none() {
            let closed_n = paths.iter().filter(|(_, c)| *c).count();
            if closed_n == 1 {
                for (pts, closed) in &paths {
                    if *closed && pts.len() >= 3 {
                        fan_fill(pts, fill, &mut out);
                    }
                }
            }
        }
    }
    if let Some(stroke) = style.stroke {
        if stroke[3] > 0.0 {
            let outline = stroke_outline(cmds, style, 0.05);
            if let Some(mesh) = crate::stroke::fill_mesh(&outline, stroke, FillRule::NonZero) {
                out.extend(mesh);
            }
        }
    }
    if marker_r > 0.0 {
        let color = style.stroke.or(style.fill).unwrap_or([1.0, 1.0, 1.0, 1.0]);
        for &p in markers {
            disc_fan(p, marker_r, color, &mut out);
        }
    }
    out
}

/// 复杂渐变（hint / px-stop / >8 stops）仍 CPU golden。
#[doc(hidden)]
pub fn mesh_needs_cpu(layers: &[(Vec<PathCommand>, PathStyle)]) -> bool {
    layers.iter().any(|(_, st)| {
        st.fill_gradient
            .as_ref()
            .is_some_and(|g| pack_gpu_gradient(g).is_none())
    })
}

fn pack_stops(stops: &[crate::gradient::ColorStop]) -> Option<Vec<(f32, [f32; 4])>> {
    if stops.is_empty() || stops.len() > 8 {
        return None;
    }
    if stops.iter().any(|s| s.position_px || s.position_end_px) {
        return None;
    }
    if stops.iter().any(|s| {
        !s.color.iter().all(|v| v.is_finite())
            || s.position.is_some_and(|v| !v.is_finite())
            || s.position_end.is_some_and(|v| !v.is_finite())
    }) {
        return None;
    }
    let resolved = crate::gradient::expand_and_resolve_stops(stops, &[], 1.0);
    if resolved.len() > 8 {
        return None;
    }
    Some(resolved.into_iter().map(|s| (s.pos, s.color)).collect())
}

fn radial_radii_unit(rg: &crate::gradient::RadialGradient) -> (f32, f32) {
    let (rx, ry) = crate::gradient::resolve_radial_radii(rg, 1.0, 1.0);
    (rx.max(1e-6), ry.max(1e-6))
}

#[doc(hidden)]
pub fn pack_gpu_gradient(g: &crate::gradient::Gradient) -> Option<GpuFillGrad> {
    let (kind, angle, center, radii, stops) = match g {
        crate::gradient::Gradient::Conic(cg) => {
            if cg.hints.iter().any(|h| h.is_some()) {
                return None;
            }
            (
                3u32 + if cg.repeating { 4 } else { 0 },
                cg.from_deg,
                [cg.position.x, cg.position.y],
                [1.0, 1.0],
                pack_stops(&cg.stops)?,
            )
        }
        crate::gradient::Gradient::Linear(lg) => {
            if lg.hints.iter().any(|h| h.is_some()) {
                return None;
            }
            (
                1u32 + if lg.repeating { 4 } else { 0 },
                lg.angle_deg,
                [0.5, 0.5],
                [1.0, 1.0],
                pack_stops(&lg.stops)?,
            )
        }
        crate::gradient::Gradient::Radial(rg) => {
            if rg.hints.iter().any(|h| h.is_some()) {
                return None;
            }
            let (rx, ry) = radial_radii_unit(rg);
            (
                2u32 + if rg.repeating { 4 } else { 0 },
                0.0,
                [rg.position.x, rg.position.y],
                [rx, ry],
                pack_stops(&rg.stops)?,
            )
        }
    };
    if !angle.is_finite() || !center.iter().chain(radii.iter()).all(|v| v.is_finite()) {
        return None;
    }
    let mut stop_t = [1.0f32; 8];
    let mut stop_c = [[0.0f32; 4]; 8];
    let n = stops.len().min(8);
    for (i, (t, c)) in stops.iter().take(n).enumerate() {
        stop_t[i] = *t;
        stop_c[i] = *c;
    }
    Some(GpuFillGrad {
        device_to_brush: [1., 0., 0., 1., 0., 0.],
        kind,
        angle,
        bbox: [0.0; 4],
        center,
        radii,
        stop_n: n as u32,
        stop_t,
        stop_c,
    })
}

fn stencil_fan(pts: &[Point2], dummy: [f32; 4], out: &mut Vec<PathTri>) {
    let pts = ring_pts(pts);
    if pts.len() < 3 {
        return;
    }
    let a = pts[0];
    for i in 1..pts.len() - 1 {
        out.push(PathTri {
            a,
            b: pts[i],
            c: pts[i + 1],
            color: dummy,
        });
    }
}

fn cover_quad(min: Point2, max: Point2, color: [f32; 4]) -> [PathTri; 2] {
    let p0 = (min.0, min.1);
    let p1 = (max.0, min.1);
    let p2 = (max.0, max.1);
    let p3 = (min.0, max.1);
    [
        PathTri {
            a: p0,
            b: p1,
            c: p2,
            color,
        },
        PathTri {
            a: p0,
            b: p2,
            c: p3,
            color,
        },
    ]
}

pub fn build_fill_op(cmds: &[PathCommand], style: &PathStyle) -> Option<PathGpuFill> {
    if style.fill.is_none() && style.fill_gradient.is_none() {
        return None;
    }
    let solid = style.fill.unwrap_or([1.0, 1.0, 1.0, 1.0]);
    if style.fill_gradient.is_none() && solid[3] <= 0.0 {
        return None;
    }
    let cover_color = if style.fill_gradient.is_some() {
        [1.0, 1.0, 1.0, solid[3].clamp(0.0, 1.0)]
    } else {
        solid
    };
    let mut stencil = Vec::new();
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    let mut any = false;
    let contours = subpaths(cmds);
    let mut direct = contours.len() == 1 && convex_contour(&contours[0].0);
    for (pts, _) in contours {
        if pts.len() < 3 {
            continue;
        }
        stencil_fan(&pts, cover_color, &mut stencil);
        for &(x, y) in &pts {
            if !x.is_finite() || !y.is_finite() {
                return None;
            }
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
        any = true;
    }
    if !any || stencil.is_empty() {
        return None;
    }
    if !direct {
        if let Some(mesh) = crate::stroke::fill_mesh(cmds, cover_color, style.fill_rule) {
            stencil = mesh;
            direct = true;
        }
    }
    let gradient_bbox = [
        min_x,
        min_y,
        (max_x - min_x).max(1e-6),
        (max_y - min_y).max(1e-6),
    ];
    let pad = 0.75;
    min_x -= pad;
    min_y -= pad;
    max_x += pad;
    max_y += pad;
    let mut grad = style.fill_gradient.as_ref().and_then(pack_gpu_gradient);
    if let Some(g) = grad.as_mut() {
        g.bbox = gradient_bbox;
    }
    Some(PathGpuFill {
        direct,
        even_odd: matches!(style.fill_rule, FillRule::EvenOdd),
        stencil,
        cover: cover_quad((min_x, min_y), (max_x, max_y), cover_color),
        grad,
        paint: None,
        clip: None,
        mask: None,
    })
}

fn convex_contour(pts: &[Point2]) -> bool {
    let pts = ring_pts(pts);
    // Bound preprocessing work; large/ambiguous paths retain the winding renderer.
    if !(3..=128).contains(&pts.len()) {
        return false;
    }
    let mut sign = 0.0f32;
    for i in 0..pts.len() {
        let a = pts[i];
        let b = pts[(i + 1) % pts.len()];
        for &p in pts {
            let cross = (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0);
            if !cross.is_finite() {
                return false;
            }
            if cross.abs() <= 1e-6 {
                continue;
            }
            if sign == 0.0 {
                sign = cross.signum();
            }
            if cross * sign < 0.0 {
                return false;
            }
        }
    }
    sign != 0.0
}

fn ring_pts(pts: &[Point2]) -> &[Point2] {
    if pts.len() >= 2 {
        let a = pts[0];
        let b = pts[pts.len() - 1];
        if (a.0 - b.0).abs() < 1e-4 && (a.1 - b.1).abs() < 1e-4 {
            return &pts[..pts.len() - 1];
        }
    }
    pts
}

fn even_odd_contains(p: Point2, pts: &[Point2]) -> bool {
    let ring = ring_pts(pts);
    let n = ring.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let (px, py) = p;
    for i in 0..n {
        let a = ring[i];
        let b = ring[(i + 1) % n];
        if (a.1 > py) != (b.1 > py) {
            let x_at = a.0 + (b.0 - a.0) * (py - a.1) / (b.1 - a.1);
            if px < x_at {
                inside = !inside;
            }
        }
    }
    inside
}

/// Non-zero winding fill test（SVG `fill-rule: nonzero`）。
fn winding_number(p: Point2, pts: &[Point2]) -> i32 {
    let ring = ring_pts(pts);
    let n = ring.len();
    if n < 3 {
        return 0;
    }
    let (px, py) = p;
    let mut winding = 0i32;
    for i in 0..n {
        let a = ring[i];
        let b = ring[(i + 1) % n];
        let cross = (b.0 - a.0) * (py - a.1) - (px - a.0) * (b.1 - a.1);
        if a.1 <= py {
            if b.1 > py && cross > 0.0 {
                winding += 1;
            }
        } else if b.1 <= py && cross < 0.0 {
            winding -= 1;
        }
    }
    winding
}

fn nonzero_contains(p: Point2, pts: &[Point2]) -> bool {
    winding_number(p, pts) != 0
}

#[doc(hidden)]
pub fn fill_contains(p: Point2, pts: &[Point2], rule: FillRule) -> bool {
    match rule {
        FillRule::EvenOdd => even_odd_contains(p, pts),
        FillRule::NonZero => nonzero_contains(p, pts),
    }
}

/// Fill hit in path user space (after CTM if the caller applied it). Canvas `is_point_in_path`.
pub fn path_contains_point(cmds: &[PathCommand], p: Point2, rule: FillRule) -> bool {
    contours_contain_point(&subpaths(cmds), p, rule)
}
pub(crate) fn contours_contain_point(
    paths: &[(Vec<Point2>, bool)],
    p: Point2,
    rule: FillRule,
) -> bool {
    let winding: i32 = paths.iter().map(|(pts, _)| winding_number(p, pts)).sum();
    match rule {
        FillRule::NonZero => winding != 0,
        FillRule::EvenOdd => winding % 2 != 0,
    }
}

/// Stroke hit in path user space. Canvas `is_point_in_stroke`（当前线宽 / cap）。
pub fn path_stroke_contains(cmds: &[PathCommand], style: &PathStyle, p: Point2) -> bool {
    let mut st = style.clone();
    if st.stroke.is_none() {
        st.stroke = Some([0.0, 0.0, 0.0, 1.0]);
    }
    point_in_stroke(cmds, &st, p)
}

#[doc(hidden)]
pub fn point_in_fill(cmds: &[PathCommand], style: &PathStyle, p: Point2) -> bool {
    if style.fill.is_none() && style.fill_gradient.is_none() {
        return false;
    }
    path_contains_point(cmds, p, style.fill_rule)
}

#[doc(hidden)]
pub fn point_in_stroke(cmds: &[PathCommand], style: &PathStyle, p: Point2) -> bool {
    if style.stroke.is_none() {
        return false;
    }
    let outline = stroke_outline(cmds, style, 0.05);
    path_contains_point(&outline, p, FillRule::NonZero)
        || subpaths(&outline).iter().any(|(pts, _)| {
            pts.windows(2)
                .any(|s| dist_to_segment(p, s[0], s[1]) <= 1e-5)
        })
}

pub fn src_over_premul(buf: &mut [f32], i: usize, r: f32, g: f32, b: f32, a: f32) {
    if a <= 0.0 {
        return;
    }
    let a = a.clamp(0.0, 1.0);
    let inv = 1.0 - a;
    buf[i] = r * a + buf[i] * inv;
    buf[i + 1] = g * a + buf[i + 1] * inv;
    buf[i + 2] = b * a + buf[i + 2] * inv;
    buf[i + 3] = a + buf[i + 3] * inv;
}

#[inline]
fn pixel_in_clip(x: usize, y: usize, clip: Option<[f32; 4]>) -> bool {
    match clip {
        None => true,
        Some(c) => {
            if c[2] <= 0.0 || c[3] <= 0.0 {
                return false;
            }
            let xf = x as f32 + 0.5;
            let yf = y as f32 + 0.5;
            xf >= c[0] && yf >= c[1] && xf < c[0] + c[2] && yf < c[1] + c[3]
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Shared raster entry keeps existing geometry, target and sampling parameters explicit"
)]
fn paint_layer_premul(
    buf: &mut [f32],
    width: u32,
    height: u32,
    cmds: &[PathCommand],
    style: &PathStyle,
    markers: &[Point2],
    marker_radius: f32,
    ssaa: u32,
    clip: Option<[f32; 4]>,
) {
    let paths = subpaths(cmds);
    let w = width as usize;
    let h = height as usize;
    let area = ssaa == 0;
    let ssaa = ssaa.max(1);
    let inv_ssaa = 1.0 / ssaa as f32;
    if area {
        crate::area::paint(buf, width, height, cmds, style, clip);
    }
    if !area && (style.fill.is_some() || style.fill_gradient.is_some()) {
        let solid = style.fill.unwrap_or([0.0, 0.0, 0.0, 1.0]);
        let mut edges = Vec::new();
        let mut bounds = [
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        ];
        for (pts, _) in &paths {
            if pts.len() < 3 {
                continue;
            }
            for i in 0..pts.len() {
                let a = pts[i];
                let b = pts[(i + 1) % pts.len()];
                if ![a.0, a.1, b.0, b.1].iter().all(|v| v.is_finite()) {
                    continue;
                }
                bounds[0] = bounds[0].min(a.0);
                bounds[1] = bounds[1].min(a.1);
                bounds[2] = bounds[2].max(a.0);
                bounds[3] = bounds[3].max(a.1);
                if a.1 != b.1 {
                    edges.push((a, b));
                }
            }
        }
        let [min_x, min_y, max_x, max_y] = bounds;
        let bw = (max_x - min_x).max(1e-6);
        let bh = (max_y - min_y).max(1e-6);
        let sample = style
            .fill_gradient
            .as_ref()
            .map(|g| crate::gradient::prepared_gradient_sampler(g, bw, bh));
        let x0 = min_x.floor().max(0.0).min(w as f32) as usize;
        let x1 = max_x.ceil().max(0.0).min(w as f32) as usize;
        let y0 = min_y.floor().max(0.0).min(h as f32) as usize;
        let y1 = max_y.ceil().max(0.0).min(h as f32) as usize;
        let mut crossings = Vec::with_capacity(edges.len());
        let mut coverage = vec![0u32; x1.saturating_sub(x0)];
        for y in y0..y1 {
            coverage.fill(0);
            for sy in 0..ssaa {
                let py = y as f32 + (sy as f32 + 0.5) * inv_ssaa;
                crossings.clear();
                for &(a, b) in &edges {
                    if (a.1 > py) != (b.1 > py) {
                        let x = a.0 + (py - a.1) * (b.0 - a.0) / (b.1 - a.1);
                        crossings.push((x, if b.1 > a.1 { 1i32 } else { -1 }));
                    }
                }
                crossings.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
                let mut winding = 0i32;
                for pair in crossings.windows(2) {
                    winding += pair[0].1;
                    let inside = match style.fill_rule {
                        FillRule::NonZero => winding != 0,
                        FillRule::EvenOdd => winding % 2 != 0,
                    };
                    if !inside {
                        continue;
                    }
                    let left = (pair[0].0 * ssaa as f32 - 0.5)
                        .ceil()
                        .max((x0 as f32) * ssaa as f32) as usize;
                    let right = (pair[1].0 * ssaa as f32 - 0.5)
                        .ceil()
                        .max(0.0)
                        .min((x1 as f32) * ssaa as f32) as usize;
                    for sx in left..right {
                        coverage[sx / ssaa as usize - x0] += 1;
                    }
                }
            }
            for x in x0..x1 {
                let cover = coverage[x - x0] as f32 * inv_ssaa * inv_ssaa;
                if cover <= 0.0 || !pixel_in_clip(x, y, clip) {
                    continue;
                }
                let fill = if let Some(ref sample) = sample {
                    let mut color =
                        sample((x as f32 + 0.5 - min_x) / bw, (y as f32 + 0.5 - min_y) / bh);
                    color[3] *= solid[3];
                    color
                } else {
                    solid
                };
                src_over_premul(
                    buf,
                    (y * w + x) * 4,
                    fill[0],
                    fill[1],
                    fill[2],
                    fill[3] * cover,
                );
            }
        }
    }

    if let Some(stroke) = style.stroke {
        if stroke[3] > 0.0 {
            let outline = stroke_outline(cmds, style, 0.05);
            paint_layer_premul(
                buf,
                width,
                height,
                &outline,
                &PathStyle::fill(stroke),
                &[],
                0.0,
                if area { 0 } else { ssaa },
                clip,
            );
        }
    }

    if marker_radius > 0.0 && !markers.is_empty() {
        let color = style.stroke.or(style.fill).unwrap_or([0.2, 0.4, 0.9, 1.0]);
        let r = marker_radius;
        for &m in markers {
            let x0 = (m.0 - r - 1.5).floor().max(0.0) as usize;
            let y0 = (m.1 - r - 1.5).floor().max(0.0) as usize;
            let x1 = ((m.0 + r + 1.5).ceil() as usize + 1).min(w);
            let y1 = ((m.1 + r + 1.5).ceil() as usize + 1).min(h);
            for y in y0..y1 {
                for x in x0..x1 {
                    if !pixel_in_clip(x, y, clip) {
                        continue;
                    }
                    let mut cover = 0.0;
                    for sy in 0..ssaa {
                        for sx in 0..ssaa {
                            let px = x as f32 + (sx as f32 + 0.5) * inv_ssaa;
                            let py = y as f32 + (sy as f32 + 0.5) * inv_ssaa;
                            let d = (px - m.0).hypot(py - m.1);
                            cover += (r + 0.35 - d).clamp(0.0, 1.0);
                        }
                    }
                    cover *= inv_ssaa * inv_ssaa;
                    if cover > 0.0 {
                        let i = (y * w + x) * 4;
                        src_over_premul(buf, i, color[0], color[1], color[2], color[3] * cover);
                    }
                }
            }
        }
    }
}

pub fn premul_to_straight_rgba(buf: &[f32], width: u32, height: u32) -> Vec<u8> {
    let n = (width as usize) * (height as usize);
    let mut out = vec![0u8; n * 4];
    for i in 0..n {
        let a = buf[i * 4 + 3];
        if a <= 1e-6 {
            continue;
        }
        let inv = 1.0 / a;
        out[i * 4] = (buf[i * 4] * inv * 255.0).round().clamp(0.0, 255.0) as u8;
        out[i * 4 + 1] = (buf[i * 4 + 1] * inv * 255.0).round().clamp(0.0, 255.0) as u8;
        out[i * 4 + 2] = (buf[i * 4 + 2] * inv * 255.0).round().clamp(0.0, 255.0) as u8;
        out[i * 4 + 3] = (a * 255.0).round().clamp(0.0, 255.0) as u8;
    }
    out
}

/// CPU 覆盖率光栅（2×2+ SSAA 填充 + 解析描边）。产品绘制走贴图，tessellate 仅回退。
pub fn rasterize_path_rgba(
    cmds: &[PathCommand],
    style: &PathStyle,
    markers: &[Point2],
    width: u32,
    height: u32,
    ssaa: u32,
) -> Vec<u8> {
    let mr = if markers.is_empty() {
        0.0
    } else {
        style.stroke_width.max(2.5) * 0.9
    };
    rasterize_layers_rgba(
        &[(cmds.to_vec(), style.clone())],
        markers,
        mr,
        width,
        height,
        ssaa,
    )
}

/// 多层 path（饼图扇区等）合成到一张 RGBA。
pub fn rasterize_layers_rgba(
    layers: &[(Vec<PathCommand>, PathStyle)],
    markers: &[Point2],
    marker_radius: f32,
    width: u32,
    height: u32,
    ssaa: u32,
) -> Vec<u8> {
    let w = width.max(1);
    let h = height.max(1);
    let mut buf = vec![0.0f32; (w as usize) * (h as usize) * 4];
    for (i, (cmds, style)) in layers.iter().enumerate() {
        let m = if i + 1 == layers.len() {
            markers
        } else {
            &[][..]
        };
        let mr = if i + 1 == layers.len() {
            marker_radius
        } else {
            0.0
        };
        paint_layer_premul(&mut buf, w, h, cmds, style, m, mr, ssaa, None);
    }
    premul_to_straight_rgba(&buf, w, h)
}

/// 把一层 path 画进已有 premul 缓冲（WP-3.12 Canvas 与 path 核共用）。
pub fn paint_path_premul(
    buf: &mut [f32],
    width: u32,
    height: u32,
    cmds: &[PathCommand],
    style: &PathStyle,
    clip: Option<[f32; 4]>,
    ssaa: u32,
) {
    paint_layer_premul(buf, width, height, cmds, style, &[], 0.0, ssaa, clip);
}

pub use crate::readback::*;
