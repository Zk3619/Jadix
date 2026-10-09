//! # Path primitives（WP-3.2 / WP-3.11 共用核）
//!
//! 折线 / 多边形 / 扇区 → [`PathCommand`]；编码为 `rui_path_d`。
//! [`decode_path_d`] 接受常用 HTML/SVG `d`（含 C/Q/A 等），**保留曲线**；
//! [`flatten_path`] 只在光栅 / tessellate / 命中边界按像素误差展平。
//! 产品绘制：**CPU 覆盖率光栅**是 golden / `RUI_GPU_2D=0` 回退；默认走 PathPipeline
//! 描边 mesh + stencil-cover fill + 4× MSAA（S3+S4）→ `rui_path:`。
//! tessellate 四边形不再进 `rounded_rect.wgsl`。
//! 不另开 Skia/Vello。Chart / `<svg>` / `<canvas>` 禁止第二套折线格式。

use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use crate::core::tree::Element;
use crate::style::parse_color;

/// 2D 点（逻辑像素，叶局部坐标，y 向下）。
pub type Point2 = (f32, f32);

/// Attr：SVG 子集 path `d`（与 WP-3.11 `<path d>` 同形）。
pub const ATTR_PATH_D: &str = "rui_path_d";
/// Attr：填充色 `r,g,b,a`（0..=1）；缺省不填。
pub const ATTR_PATH_FILL: &str = "rui_path_fill";
/// Attr：描边色 `r,g,b,a`。
pub const ATTR_PATH_STROKE: &str = "rui_path_stroke";
/// Attr：描边宽（逻辑 px）。
pub const ATTR_PATH_STROKE_WIDTH: &str = "rui_path_stroke_width";
/// Attr：`butt` / `round` / `square`（默认 round，对齐 Lucide）。
pub const ATTR_PATH_STROKE_LINECAP: &str = "rui_path_stroke_linecap";
/// Attr：`miter` / `round` / `bevel`（默认 round）。
pub const ATTR_PATH_STROKE_LINEJOIN: &str = "rui_path_stroke_linejoin";
/// Attr：`nonzero` / `evenodd`（填充规则；光栅现为 even-odd 扫描，evenodd 显式记录）。
pub const ATTR_PATH_FILL_RULE: &str = "rui_path_fill_rule";
/// 旧折线 kind（仍可读）。
pub const ATTR_PATH_KIND: &str = "rui_path_kind";
/// 旧点列 `x,y;x,y;…`（仍可读，写入时同时写 [`ATTR_PATH_D`]）。
pub const ATTR_PATH_POINTS: &str = "rui_path_points";
/// 可选 viewBox：`minx miny w h`。
///
/// - 轴 span `< 2`：归一化，缩放到叶对应边（宽常为 `1`）。
/// - 轴 span `>= 2`：视为逻辑 px，绘制尺寸取 `min(叶边, span)`，**不**跟被撑高的叶走。
/// - [`ATTR_PATH_FIT_LEAF`]`=1`：两端都跟叶（公开 `<svg>`，HTML viewBox 拉伸）。
pub const ATTR_PATH_VIEWBOX: &str = "rui_path_viewbox";
/// 为 `1` 时 viewBox 两端都映射到叶尺寸（WP-3.11 Svg）；Chart 不写此 attr。
pub const ATTR_PATH_FIT_LEAF: &str = "rui_path_fit_leaf";
/// 可选标记点（viewBox / 局部坐标，`x,y;x,y`），光栅时画实心圆。
pub const ATTR_PATH_MARKERS: &str = "rui_path_markers";
/// 多层 scene 键（Chart 等）；布局期按叶宽光栅，避免 `image_texture` 低分拉伸 + `d` 默认描边叠画。
pub const ATTR_PATH_SCENE: &str = "rui_path_scene";

/// 路径绘制命令。曲线活到光栅 / tessellate 边界（[`flatten_path`]）。
#[derive(Clone, Debug, PartialEq)]
pub enum PathCommand {
    MoveTo(Point2),
    LineTo(Point2),
    QuadTo {
        ctrl: Point2,
        to: Point2,
    },
    CubicTo {
        c1: Point2,
        c2: Point2,
        to: Point2,
    },
    /// SVG 椭圆弧（终点参数：`rx,ry x-rot large sweep x,y`）。
    Arc {
        radii: Point2,
        x_rot_deg: f32,
        large: bool,
        sweep: bool,
        to: Point2,
    },
    Close,
}

impl PathCommand {
    /// Pen after this command (`Close` has none).
    pub fn pen(&self) -> Option<Point2> {
        match *self {
            Self::MoveTo(p) | Self::LineTo(p) => Some(p),
            Self::QuadTo { to, .. } | Self::CubicTo { to, .. } | Self::Arc { to, .. } => Some(to),
            Self::Close => None,
        }
    }

    pub fn map_xy(self, mut f: impl FnMut(Point2) -> Point2) -> Self {
        match self {
            Self::MoveTo(p) => Self::MoveTo(f(p)),
            Self::LineTo(p) => Self::LineTo(f(p)),
            Self::QuadTo { ctrl, to } => Self::QuadTo {
                ctrl: f(ctrl),
                to: f(to),
            },
            Self::CubicTo { c1, c2, to } => Self::CubicTo {
                c1: f(c1),
                c2: f(c2),
                to: f(to),
            },
            Self::Arc {
                radii,
                x_rot_deg,
                large,
                sweep,
                to,
            } => Self::Arc {
                radii,
                x_rot_deg,
                large,
                sweep,
                to: f(to),
            },
            Self::Close => Self::Close,
        }
    }
}

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
    pub fill_gradient: Option<crate::style::Gradient>,
    pub stroke: Option<[f32; 4]>,
    pub stroke_width: f32,
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
            stroke_width: width.max(0.5),
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
            stroke_width: width.max(0.5),
            stroke_linecap: StrokeLinecap::Round,
            stroke_linejoin: StrokeLinejoin::Round,
            fill_rule: FillRule::NonZero,
        }
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

/// Tessellate 产出：与 [`crate::renderer::InstanceInput`] 对齐的旋转四边形。
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

/// S4 cover pass：线性 / 径向渐变（conic / repeating / hints / px-stop 仍 CPU）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct GpuFillGrad {
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
pub(crate) struct PathGpuFill {
    pub even_odd: bool,
    pub stencil: Vec<PathTri>,
    pub cover: [PathTri; 2],
    pub grad: Option<GpuFillGrad>,
    /// Leaf-pixel AABB scissor. `None` = full RT.
    pub clip: Option<[f32; 4]>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PathGpuMesh {
    pub tris: Vec<PathTri>,
    pub clip: Option<[f32; 4]>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PathGpuOp {
    Fill(PathGpuFill),
    Mesh(PathGpuMesh),
}

impl PathGpuOp {
    pub(crate) fn is_empty(&self) -> bool {
        match self {
            Self::Fill(f) => f.stencil.is_empty(),
            Self::Mesh(m) => m.tris.is_empty(),
        }
    }

    pub(crate) fn clip(&self) -> Option<[f32; 4]> {
        match self {
            Self::Fill(f) => f.clip,
            Self::Mesh(m) => m.clip,
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
    let w = w.max(1.0);
    let h = h.max(1.0);
    polygon(&[(x, y), (x + w, y), (x + w, y + h), (x, y + h)])
}

/// 轴对齐圆角矩形。`radii` = `[tl, tr, br, bl]`，各角自动钳到宽高一半。
pub fn rounded_rect(x: f32, y: f32, w: f32, h: f32, radii: [f32; 4]) -> Vec<PathCommand> {
    let w = w.max(1.0);
    let h = h.max(0.5);
    let max_r = (w * 0.5).min(h * 0.5);
    let tl = radii[0].clamp(0.0, max_r);
    let tr = radii[1].clamp(0.0, max_r);
    let br = radii[2].clamp(0.0, max_r);
    let bl = radii[3].clamp(0.0, max_r);
    if tl + tr + br + bl < 0.5 {
        return filled_rect(x, y, w, h);
    }
    let mut cmds = Vec::new();
    let mut corner = |cx: f32, cy: f32, r: f32, a0: f32, span: f32| {
        if r < 0.5 {
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

/// SVG 子集 `d`：保留 C/Q/A（不再先展平）。
pub fn encode_path_d(cmds: &[PathCommand]) -> String {
    let mut parts = Vec::with_capacity(cmds.len());
    for cmd in cmds {
        match *cmd {
            PathCommand::MoveTo((x, y)) => parts.push(format!("M{x:.3},{y:.3}")),
            PathCommand::LineTo((x, y)) => parts.push(format!("L{x:.3},{y:.3}")),
            PathCommand::QuadTo {
                ctrl: (cx, cy),
                to: (x, y),
            } => parts.push(format!("Q{cx:.3},{cy:.3} {x:.3},{y:.3}")),
            PathCommand::CubicTo {
                c1: (x1, y1),
                c2: (x2, y2),
                to: (x, y),
            } => parts.push(format!("C{x1:.3},{y1:.3} {x2:.3},{y2:.3} {x:.3},{y:.3}")),
            PathCommand::Arc {
                radii: (rx, ry),
                x_rot_deg,
                large,
                sweep,
                to: (x, y),
            } => parts.push(format!(
                "A{rx:.3},{ry:.3} {x_rot_deg:.3} {} {} {x:.3},{y:.3}",
                if large { 1 } else { 0 },
                if sweep { 1 } else { 0 },
            )),
            PathCommand::Close => parts.push("Z".into()),
        }
    }
    parts.join(" ")
}

/// 解析 HTML/SVG path `d`（绝对/相对 M/L/H/V/C/S/Q/T/A/Z）。曲线保留为 [`PathCommand`] 变体。
pub fn decode_path_d(d: &str) -> Vec<PathCommand> {
    let parser = PathDParser::new(d);
    parser.parse()
}

/// Flatten curves to `MoveTo` / `LineTo` / `Close` for raster, tessellate, and hit-testing.
pub fn flatten_path(cmds: &[PathCommand]) -> Vec<PathCommand> {
    let mut out = Vec::with_capacity(cmds.len());
    let mut cur = (0.0f32, 0.0f32);
    for cmd in cmds {
        match *cmd {
            PathCommand::MoveTo(p) => {
                out.push(PathCommand::MoveTo(p));
                cur = p;
            }
            PathCommand::LineTo(p) => {
                out.push(PathCommand::LineTo(p));
                cur = p;
            }
            PathCommand::QuadTo { ctrl, to } => {
                flatten_quad(cur, ctrl, to, &mut out);
                cur = to;
            }
            PathCommand::CubicTo { c1, c2, to } => {
                flatten_cubic(cur, c1, c2, to, &mut out);
                cur = to;
            }
            PathCommand::Arc {
                radii,
                x_rot_deg,
                large,
                sweep,
                to,
            } => {
                flatten_arc(
                    cur,
                    radii.0.abs(),
                    radii.1.abs(),
                    x_rot_deg,
                    large,
                    sweep,
                    to,
                    &mut out,
                );
                cur = to;
            }
            PathCommand::Close => {
                out.push(PathCommand::Close);
            }
        }
    }
    out
}

struct PathDParser<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
}

impl<'a> PathDParser<'a> {
    fn new(d: &'a str) -> Self {
        Self {
            chars: d.chars().peekable(),
        }
    }

    fn skip_sep(&mut self) {
        while let Some(&c) = self.chars.peek() {
            if c.is_whitespace() || c == ',' {
                self.chars.next();
            } else {
                break;
            }
        }
    }

    fn peek_cmd(&mut self) -> Option<char> {
        self.skip_sep();
        self.chars
            .peek()
            .copied()
            .filter(|c| c.is_ascii_alphabetic())
    }

    fn next_number(&mut self) -> Option<f32> {
        self.skip_sep();
        let mut s = String::new();
        if let Some(&c) = self.chars.peek() {
            if c == '-' || c == '+' {
                s.push(c);
                self.chars.next();
            }
        }
        let mut saw_digit = false;
        let mut saw_dot = false;
        let mut saw_exp = false;
        while let Some(&c) = self.chars.peek() {
            if c.is_ascii_digit() {
                saw_digit = true;
                s.push(c);
                self.chars.next();
            } else if c == '.' && !saw_dot && !saw_exp {
                saw_dot = true;
                s.push(c);
                self.chars.next();
            } else if (c == 'e' || c == 'E') && saw_digit && !saw_exp {
                saw_exp = true;
                s.push(c);
                self.chars.next();
                if let Some(&s2) = self.chars.peek() {
                    if s2 == '+' || s2 == '-' {
                        s.push(s2);
                        self.chars.next();
                    }
                }
            } else {
                break;
            }
        }
        if !saw_digit {
            return None;
        }
        s.parse().ok()
    }

    fn parse(mut self) -> Vec<PathCommand> {
        let mut out = Vec::new();
        let mut cx = 0.0f32;
        let mut cy = 0.0f32;
        let mut sub_x = 0.0f32;
        let mut sub_y = 0.0f32;
        let mut last_c2: Option<(f32, f32)> = None; // last cubic control (absolute)
        let mut last_q: Option<(f32, f32)> = None; // last quad control
        let mut cur_cmd: Option<char> = None;

        while self.chars.peek().is_some() {
            if let Some(c) = self.peek_cmd() {
                self.chars.next();
                cur_cmd = Some(c);
            }
            let Some(cmd) = cur_cmd else {
                break;
            };
            let abs = cmd.is_ascii_uppercase();
            let op = cmd.to_ascii_uppercase();

            match op {
                'Z' => {
                    out.push(PathCommand::Close);
                    cx = sub_x;
                    cy = sub_y;
                    last_c2 = None;
                    last_q = None;
                    cur_cmd = None;
                }
                'M' => {
                    let Some(x) = self.next_number() else {
                        break;
                    };
                    let Some(y) = self.next_number() else {
                        break;
                    };
                    let (nx, ny) = if abs { (x, y) } else { (cx + x, cy + y) };
                    out.push(PathCommand::MoveTo((nx, ny)));
                    cx = nx;
                    cy = ny;
                    sub_x = nx;
                    sub_y = ny;
                    last_c2 = None;
                    last_q = None;
                    // Implicit LineTo for subsequent coordinate pairs.
                    cur_cmd = Some(if abs { 'L' } else { 'l' });
                    loop {
                        self.skip_sep();
                        if self
                            .chars
                            .peek()
                            .copied()
                            .is_some_and(|c| c.is_ascii_alphabetic())
                        {
                            break;
                        }
                        let Some(x) = self.next_number() else {
                            break;
                        };
                        let Some(y) = self.next_number() else {
                            break;
                        };
                        let (nx, ny) = if abs { (x, y) } else { (cx + x, cy + y) };
                        out.push(PathCommand::LineTo((nx, ny)));
                        cx = nx;
                        cy = ny;
                    }
                }
                'L' => {
                    let Some(x) = self.next_number() else {
                        break;
                    };
                    let Some(y) = self.next_number() else {
                        break;
                    };
                    let (nx, ny) = if abs { (x, y) } else { (cx + x, cy + y) };
                    out.push(PathCommand::LineTo((nx, ny)));
                    cx = nx;
                    cy = ny;
                    last_c2 = None;
                    last_q = None;
                }
                'H' => {
                    let Some(x) = self.next_number() else {
                        break;
                    };
                    let nx = if abs { x } else { cx + x };
                    out.push(PathCommand::LineTo((nx, cy)));
                    cx = nx;
                    last_c2 = None;
                    last_q = None;
                }
                'V' => {
                    let Some(y) = self.next_number() else {
                        break;
                    };
                    let ny = if abs { y } else { cy + y };
                    out.push(PathCommand::LineTo((cx, ny)));
                    cy = ny;
                    last_c2 = None;
                    last_q = None;
                }
                'C' => {
                    let Some(x1) = self.next_number() else {
                        break;
                    };
                    let Some(y1) = self.next_number() else {
                        break;
                    };
                    let Some(x2) = self.next_number() else {
                        break;
                    };
                    let Some(y2) = self.next_number() else {
                        break;
                    };
                    let Some(x) = self.next_number() else {
                        break;
                    };
                    let Some(y) = self.next_number() else {
                        break;
                    };
                    let (x1, y1, x2, y2, x, y) = if abs {
                        (x1, y1, x2, y2, x, y)
                    } else {
                        (cx + x1, cy + y1, cx + x2, cy + y2, cx + x, cy + y)
                    };
                    out.push(PathCommand::CubicTo {
                        c1: (x1, y1),
                        c2: (x2, y2),
                        to: (x, y),
                    });
                    last_c2 = Some((x2, y2));
                    last_q = None;
                    cx = x;
                    cy = y;
                }
                'S' => {
                    let Some(x2) = self.next_number() else {
                        break;
                    };
                    let Some(y2) = self.next_number() else {
                        break;
                    };
                    let Some(x) = self.next_number() else {
                        break;
                    };
                    let Some(y) = self.next_number() else {
                        break;
                    };
                    let (x2, y2, x, y) = if abs {
                        (x2, y2, x, y)
                    } else {
                        (cx + x2, cy + y2, cx + x, cy + y)
                    };
                    let (x1, y1) = match last_c2 {
                        Some((lx, ly)) => (2.0 * cx - lx, 2.0 * cy - ly),
                        None => (cx, cy),
                    };
                    out.push(PathCommand::CubicTo {
                        c1: (x1, y1),
                        c2: (x2, y2),
                        to: (x, y),
                    });
                    last_c2 = Some((x2, y2));
                    last_q = None;
                    cx = x;
                    cy = y;
                }
                'Q' => {
                    let Some(x1) = self.next_number() else {
                        break;
                    };
                    let Some(y1) = self.next_number() else {
                        break;
                    };
                    let Some(x) = self.next_number() else {
                        break;
                    };
                    let Some(y) = self.next_number() else {
                        break;
                    };
                    let (x1, y1, x, y) = if abs {
                        (x1, y1, x, y)
                    } else {
                        (cx + x1, cy + y1, cx + x, cy + y)
                    };
                    out.push(PathCommand::QuadTo {
                        ctrl: (x1, y1),
                        to: (x, y),
                    });
                    last_q = Some((x1, y1));
                    last_c2 = None;
                    cx = x;
                    cy = y;
                }
                'T' => {
                    let Some(x) = self.next_number() else {
                        break;
                    };
                    let Some(y) = self.next_number() else {
                        break;
                    };
                    let (x, y) = if abs { (x, y) } else { (cx + x, cy + y) };
                    let (x1, y1) = match last_q {
                        Some((lx, ly)) => (2.0 * cx - lx, 2.0 * cy - ly),
                        None => (cx, cy),
                    };
                    out.push(PathCommand::QuadTo {
                        ctrl: (x1, y1),
                        to: (x, y),
                    });
                    last_q = Some((x1, y1));
                    last_c2 = None;
                    cx = x;
                    cy = y;
                }
                'A' => {
                    let Some(rx) = self.next_number() else {
                        break;
                    };
                    let Some(ry) = self.next_number() else {
                        break;
                    };
                    let Some(x_axis_rot) = self.next_number() else {
                        break;
                    };
                    let Some(large_arc) = self.next_number() else {
                        break;
                    };
                    let Some(sweep) = self.next_number() else {
                        break;
                    };
                    let Some(x) = self.next_number() else {
                        break;
                    };
                    let Some(y) = self.next_number() else {
                        break;
                    };
                    let (x, y) = if abs { (x, y) } else { (cx + x, cy + y) };
                    out.push(PathCommand::Arc {
                        radii: (rx.abs(), ry.abs()),
                        x_rot_deg: x_axis_rot,
                        large: large_arc != 0.0,
                        sweep: sweep != 0.0,
                        to: (x, y),
                    });
                    last_c2 = None;
                    last_q = None;
                    cx = x;
                    cy = y;
                }
                _ => {
                    // Unknown: skip one number if any to avoid infinite loop
                    let _ = self.next_number();
                    cur_cmd = None;
                }
            }
        }
        out
    }
}

fn mid_pt(a: Point2, b: Point2) -> Point2 {
    ((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5)
}

/// Maximum chord error in raster pixels (coordinates already include display DPI).
const FLATTEN_FLATNESS: f32 = 0.125;
const FLATTEN_MAX_DEPTH: u8 = 10;

fn flatten_cubic(p0: Point2, p1: Point2, p2: Point2, p3: Point2, out: &mut Vec<PathCommand>) {
    flatten_cubic_rec(p0, p1, p2, p3, 0, out);
}

fn flatten_cubic_rec(
    p0: Point2,
    p1: Point2,
    p2: Point2,
    p3: Point2,
    depth: u8,
    out: &mut Vec<PathCommand>,
) {
    let err = dist_to_segment(p1, p0, p3).max(dist_to_segment(p2, p0, p3));
    if depth >= FLATTEN_MAX_DEPTH || err <= FLATTEN_FLATNESS {
        out.push(PathCommand::LineTo(p3));
        return;
    }
    let m01 = mid_pt(p0, p1);
    let m12 = mid_pt(p1, p2);
    let m23 = mid_pt(p2, p3);
    let m012 = mid_pt(m01, m12);
    let m123 = mid_pt(m12, m23);
    let m = mid_pt(m012, m123);
    flatten_cubic_rec(p0, m01, m012, m, depth.saturating_add(1), out);
    flatten_cubic_rec(m, m123, m23, p3, depth.saturating_add(1), out);
}

fn flatten_quad(p0: Point2, p1: Point2, p2: Point2, out: &mut Vec<PathCommand>) {
    flatten_quad_rec(p0, p1, p2, 0, out);
}

fn flatten_quad_rec(p0: Point2, p1: Point2, p2: Point2, depth: u8, out: &mut Vec<PathCommand>) {
    if depth >= FLATTEN_MAX_DEPTH || dist_to_segment(p1, p0, p2) <= FLATTEN_FLATNESS {
        out.push(PathCommand::LineTo(p2));
        return;
    }
    let m01 = mid_pt(p0, p1);
    let m12 = mid_pt(p1, p2);
    let m = mid_pt(m01, m12);
    flatten_quad_rec(p0, m01, m, depth.saturating_add(1), out);
    flatten_quad_rec(m, m12, p2, depth.saturating_add(1), out);
}

/// SVG elliptical arc → polyline (W3C endpoint-to-center parameterization).
fn flatten_arc(
    p0: Point2,
    mut rx: f32,
    mut ry: f32,
    x_axis_rot_deg: f32,
    large_arc: bool,
    sweep: bool,
    p1: Point2,
    out: &mut Vec<PathCommand>,
) {
    if (p0.0 - p1.0).abs() < 1e-6 && (p0.1 - p1.1).abs() < 1e-6 {
        return;
    }
    if rx < 1e-6 || ry < 1e-6 {
        out.push(PathCommand::LineTo(p1));
        return;
    }
    let phi = x_axis_rot_deg.to_radians();
    let (sin_phi, cos_phi) = phi.sin_cos();
    let dx = (p0.0 - p1.0) * 0.5;
    let dy = (p0.1 - p1.1) * 0.5;
    let x1p = cos_phi * dx + sin_phi * dy;
    let y1p = -sin_phi * dx + cos_phi * dy;
    let mut rx_sq = rx * rx;
    let mut ry_sq = ry * ry;
    let x1p_sq = x1p * x1p;
    let y1p_sq = y1p * y1p;
    let lambda = x1p_sq / rx_sq + y1p_sq / ry_sq;
    if lambda > 1.0 {
        let s = lambda.sqrt();
        rx *= s;
        ry *= s;
        rx_sq = rx * rx;
        ry_sq = ry * ry;
    }
    let sign = if large_arc == sweep { -1.0 } else { 1.0 };
    let mut sq = ((rx_sq * ry_sq - rx_sq * y1p_sq - ry_sq * x1p_sq)
        / (rx_sq * y1p_sq + ry_sq * x1p_sq))
        .max(0.0)
        .sqrt();
    sq *= sign;
    let cxp = sq * (rx * y1p) / ry;
    let cyp = sq * -(ry * x1p) / rx;
    let cx = cos_phi * cxp - sin_phi * cyp + (p0.0 + p1.0) * 0.5;
    let cy = sin_phi * cxp + cos_phi * cyp + (p0.1 + p1.1) * 0.5;

    let angle = |ux: f32, uy: f32, vx: f32, vy: f32| -> f32 {
        let n = (ux * ux + uy * uy).sqrt() * (vx * vx + vy * vy).sqrt();
        if n < 1e-12 {
            return 0.0;
        }
        let mut a = ((ux * vx + uy * vy) / n).clamp(-1.0, 1.0).acos();
        if ux * vy - uy * vx < 0.0 {
            a = -a;
        }
        a
    };
    let theta1 = angle(1.0, 0.0, (x1p - cxp) / rx, (y1p - cyp) / ry);
    let mut dtheta = angle(
        (x1p - cxp) / rx,
        (y1p - cyp) / ry,
        (-x1p - cxp) / rx,
        (-y1p - cyp) / ry,
    );
    if !sweep && dtheta > 0.0 {
        dtheta -= std::f32::consts::TAU;
    } else if sweep && dtheta < 0.0 {
        dtheta += std::f32::consts::TAU;
    }

    let segments = arc_segments(rx.max(ry), dtheta);
    for i in 1..=segments {
        let t = i as f32 / segments as f32;
        let theta = theta1 + dtheta * t;
        let (st, ct) = theta.sin_cos();
        let x = cos_phi * rx * ct - sin_phi * ry * st + cx;
        let y = sin_phi * rx * ct + cos_phi * ry * st + cy;
        out.push(PathCommand::LineTo((x, y)));
    }
    // Snap endpoint (numerical drift on long arcs).
    if let Some(PathCommand::LineTo(p)) = out.last_mut() {
        *p = p1;
    }
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

/// 从 Element attrs 还原命令（优先 `d`，否则旧点列）。
pub fn commands_from_attrs(attrs: &HashMap<String, String>) -> Vec<PathCommand> {
    if let Some(d) = attrs.get(ATTR_PATH_D) {
        if !d.trim().is_empty() {
            return decode_path_d(d);
        }
    }
    if let Some(pts) = attrs.get(ATTR_PATH_POINTS) {
        let points = decode_points(pts);
        if !points.is_empty() {
            return polyline(&points);
        }
    }
    Vec::new()
}

pub fn style_from_attrs(attrs: &HashMap<String, String>) -> PathStyle {
    let fill = attrs.get(ATTR_PATH_FILL).and_then(|s| decode_path_color(s));
    let stroke = attrs
        .get(ATTR_PATH_STROKE)
        .and_then(|s| decode_path_color(s));
    let stroke_width = attrs
        .get(ATTR_PATH_STROKE_WIDTH)
        .and_then(|s| s.parse().ok())
        .unwrap_or(2.0);
    let stroke_linecap = attrs
        .get(ATTR_PATH_STROKE_LINECAP)
        .map(|s| StrokeLinecap::parse(s))
        .unwrap_or_default();
    let stroke_linejoin = attrs
        .get(ATTR_PATH_STROKE_LINEJOIN)
        .map(|s| StrokeLinejoin::parse(s))
        .unwrap_or_default();
    let fill_rule = attrs
        .get(ATTR_PATH_FILL_RULE)
        .map(|s| FillRule::parse(s))
        .unwrap_or_default();
    if fill.is_none() && stroke.is_none() {
        return PathStyle::default();
    }
    PathStyle {
        fill,
        fill_gradient: None,
        stroke,
        stroke_width,
        stroke_linecap,
        stroke_linejoin,
        fill_rule,
    }
}

/// 把路径挂到 Element attrs，供 layout 批处理消费。
pub fn attach_path(mut el: Element, cmds: &[PathCommand], style: &PathStyle) -> Element {
    el.set_attr(ATTR_PATH_D, encode_path_d(cmds));
    let pts = points_from_commands(cmds);
    if !pts.is_empty() {
        el.set_attr(ATTR_PATH_POINTS, encode_points(&pts));
        el.set_attr(ATTR_PATH_KIND, "path");
    }
    if let Some(fill) = style.fill {
        el.set_attr(ATTR_PATH_FILL, encode_path_color(fill));
    } else {
        el.attrs.remove(ATTR_PATH_FILL);
    }
    if let Some(stroke) = style.stroke {
        el.set_attr(ATTR_PATH_STROKE, encode_path_color(stroke));
        el.set_attr(ATTR_PATH_STROKE_WIDTH, format!("{:.3}", style.stroke_width));
        el.set_attr(ATTR_PATH_STROKE_LINECAP, style.stroke_linecap.as_str());
        el.set_attr(ATTR_PATH_STROKE_LINEJOIN, style.stroke_linejoin.as_str());
    } else {
        el.attrs.remove(ATTR_PATH_STROKE);
        el.attrs.remove(ATTR_PATH_STROKE_WIDTH);
        el.attrs.remove(ATTR_PATH_STROKE_LINECAP);
        el.attrs.remove(ATTR_PATH_STROKE_LINEJOIN);
    }
    el.set_attr(ATTR_PATH_FILL_RULE, style.fill_rule.as_str());
    el
}

/// 旧 API：折线 attrs；现同时写 `d` + 默认描边，布局期会真实绘制。
pub fn attach_polyline(el: Element, points: &[Point2]) -> Element {
    attach_path(el, &polyline(points), &PathStyle::default())
}

/// 按点列包围盒建叶并挂描边路径（原 stub 现进入批处理）。
pub fn draw_polyline_batch(points: &[Point2], style: &PathStyle) -> Element {
    let (w, h) = bounds_size(points);
    let pad = style.stroke_width.max(2.0) + 2.0;
    let el = Element::new().w((w + pad).max(1.0)).h((h + pad).max(1.0));
    attach_path(el, &polyline(points), style)
}

/// 兼容名：与 [`draw_polyline_batch`] 相同，默认 foreground 描边。
pub fn draw_polyline_batch_stub(points: &[Point2]) -> Element {
    draw_polyline_batch(points, &PathStyle::default())
}

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

fn bounds_size(points: &[Point2]) -> (f32, f32) {
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
fn subpaths(cmds: &[PathCommand]) -> Vec<(Vec<Point2>, bool)> {
    let cmds = flatten_path(cmds);
    let mut out = Vec::new();
    let mut cur: Vec<Point2> = Vec::new();
    let mut start: Option<Point2> = None;
    for cmd in &cmds {
        match *cmd {
            PathCommand::MoveTo(p) => {
                if !cur.is_empty() {
                    out.push((std::mem::take(&mut cur), false));
                }
                cur.push(p);
                start = Some(p);
            }
            PathCommand::LineTo(p) => cur.push(p),
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

const STROKE_MITER_LIMIT: f32 = 4.0;

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

fn intersect_lines(p: Point2, d: (f32, f32), q: Point2, e: (f32, f32)) -> Option<Point2> {
    let cross = d.0 * e.1 - d.1 * e.0;
    if cross.abs() < 1e-8 {
        return None;
    }
    let t = ((q.0 - p.0) * e.1 - (q.1 - p.1) * e.0) / cross;
    Some((p.0 + t * d.0, p.1 + t * d.1))
}

fn push_stroke_tri(a: Point2, b: Point2, c: Point2, color: [f32; 4], out: &mut Vec<PathTri>) {
    if color[3] <= 0.0 {
        return;
    }
    out.push(PathTri { a, b, c, color });
}

/// Outer bevel/miter wedges. Round joins stay as discs in [`tessellate_stroke_points`].
fn stroke_join_tris(
    b: Point2,
    d1: (f32, f32),
    d2: (f32, f32),
    hw: f32,
    join: StrokeLinejoin,
    color: [f32; 4],
    out: &mut Vec<PathTri>,
) {
    if matches!(join, StrokeLinejoin::Round) {
        return;
    }
    let cross = d1.0 * d2.1 - d1.1 * d2.0;
    let dot = d1.0 * d2.0 + d1.1 * d2.1;
    if cross.abs() < 1e-5 && dot > 0.97 {
        return;
    }
    let n1 = (-d1.1 * hw, d1.0 * hw);
    let n2 = (-d2.1 * hw, d2.0 * hw);
    let (o1, o2) = if cross > 0.0 {
        ((b.0 - n1.0, b.1 - n1.1), (b.0 - n2.0, b.1 - n2.1))
    } else {
        ((b.0 + n1.0, b.1 + n1.1), (b.0 + n2.0, b.1 + n2.1))
    };
    let mut bevel = || {
        push_stroke_tri(o1, b, o2, color, out);
    };
    if join == StrokeLinejoin::Bevel {
        bevel();
        return;
    }
    if let Some(m) = intersect_lines(o1, d1, o2, d2) {
        let miter_len = (m.0 - b.0).hypot(m.1 - b.1);
        if miter_len <= hw * STROKE_MITER_LIMIT * 1.05 {
            push_stroke_tri(o1, m, o2, color, out);
            push_stroke_tri(o1, b, o2, color, out);
            return;
        }
    }
    bevel();
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

fn quad_to_tris(q: PathQuad, out: &mut Vec<PathTri>) {
    if q.size.0 <= 0.0 || q.size.1 <= 0.0 || q.color[3] <= 0.0 {
        return;
    }
    let cx = q.pos.0 + q.size.0 * 0.5;
    let cy = q.pos.1 + q.size.1 * 0.5;
    let hw = q.size.0 * 0.5;
    let hh = q.size.1 * 0.5;
    // Stroke round caps/joins are circular quads in the SDF path. Preserve
    // their shape when converting to triangles instead of emitting a square.
    if q.radii.iter().all(|&r| r > 0.0) {
        disc_fan((cx, cy), hw.min(hh), q.color, out);
        return;
    }
    let (s, c) = q.rotate_deg.to_radians().sin_cos();
    let rot = |x: f32, y: f32| (cx + x * c - y * s, cy + x * s + y * c);
    let p0 = rot(-hw, -hh);
    let p1 = rot(hw, -hh);
    let p2 = rot(hw, hh);
    let p3 = rot(-hw, hh);
    out.push(PathTri {
        a: p0,
        b: p1,
        c: p2,
        color: q.color,
    });
    out.push(PathTri {
        a: p0,
        b: p2,
        c: p3,
        color: q.color,
    });
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
fn arc_segments(radius: f32, angle: f32) -> usize {
    let step = (2.0 * (1.0 - FLATTEN_FLATNESS / radius.max(0.5)).acos())
        .clamp(0.001, std::f32::consts::PI / 6.0);
    (angle.abs() / step).ceil().clamp(1.0, 4096.0) as usize
}

fn disc_fan(center: Point2, radius: f32, color: [f32; 4], out: &mut Vec<PathTri>) {
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
        let w = style.stroke_width;
        let has_open = paths.iter().any(|(_, c)| !*c);
        let mut quads = Vec::new();
        for (pts, closed) in &paths {
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
                &mut quads,
            );
            let hw = w.max(0.75) * 0.5;
            let n = pts.len();
            let last = n.saturating_sub(1);
            let ring_closed = *closed && n >= 3;
            let wrap_last = if ring_closed
                && (pts[0].0 - pts[last].0).abs() < 1e-4
                && (pts[0].1 - pts[last].1).abs() < 1e-4
            {
                last.saturating_sub(1)
            } else {
                last
            };
            if ring_closed {
                for i in 0..=wrap_last {
                    let prev = if i == 0 { wrap_last } else { i - 1 };
                    let next = if i == wrap_last { 0 } else { i + 1 };
                    let Some(d1) = unit_dir(pts[prev], pts[i]) else {
                        continue;
                    };
                    let Some(d2) = unit_dir(pts[i], pts[next]) else {
                        continue;
                    };
                    stroke_join_tris(pts[i], d1, d2, hw, style.stroke_linejoin, stroke, &mut out);
                }
            } else {
                for i in 1..n.saturating_sub(1) {
                    let Some(d1) = unit_dir(pts[i - 1], pts[i]) else {
                        continue;
                    };
                    let Some(d2) = unit_dir(pts[i], pts[i + 1]) else {
                        continue;
                    };
                    stroke_join_tris(pts[i], d1, d2, hw, style.stroke_linejoin, stroke, &mut out);
                }
            }
        }
        for q in quads {
            quad_to_tris(q, &mut out);
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

/// 复杂渐变（conic / repeating / hint / px-stop / >8 stops）仍 CPU golden。
fn mesh_needs_cpu(layers: &[(Vec<PathCommand>, PathStyle)]) -> bool {
    layers.iter().any(|(_, st)| {
        st.fill_gradient
            .as_ref()
            .is_some_and(|g| pack_gpu_gradient(g).is_none())
    })
}

fn pack_stops(stops: &[crate::style::ColorStop]) -> Option<Vec<(f32, [f32; 4])>> {
    if stops.is_empty() || stops.len() > 8 {
        return None;
    }
    if stops.iter().any(|s| s.position_px || s.position_end_px) {
        return None;
    }
    let n = stops.len();
    let mut out = Vec::with_capacity(n.saturating_mul(2).min(8));
    for (i, s) in stops.iter().enumerate() {
        let t = s
            .position
            .unwrap_or(if n <= 1 {
                0.0
            } else {
                i as f32 / (n - 1) as f32
            })
            .clamp(0.0, 1.0);
        out.push((t, s.color));
        if let Some(end) = s.position_end {
            if out.len() >= 8 {
                return None;
            }
            out.push((end.clamp(0.0, 1.0), s.color));
        }
    }
    out.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    Some(out)
}

fn radial_radii_unit(rg: &crate::style::RadialGradient) -> (f32, f32) {
    let cx = rg.position.x;
    let cy = rg.position.y;
    match rg.extent {
        crate::style::RadialExtent::Ellipse { rx, ry } => (rx.max(1e-6), ry.max(1e-6)),
        crate::style::RadialExtent::Circle(r) => (r.max(1e-6), r.max(1e-6)),
        crate::style::RadialExtent::Keyword(kw) => {
            let sides = [cx.abs(), (1.0 - cx).abs(), cy.abs(), (1.0 - cy).abs()];
            let corners = [
                (cx * cx + cy * cy).sqrt(),
                ((1.0 - cx) * (1.0 - cx) + cy * cy).sqrt(),
                (cx * cx + (1.0 - cy) * (1.0 - cy)).sqrt(),
                ((1.0 - cx) * (1.0 - cx) + (1.0 - cy) * (1.0 - cy)).sqrt(),
            ];
            let r = match kw {
                crate::style::RadialSize::ClosestSide => {
                    sides.into_iter().fold(f32::INFINITY, f32::min)
                }
                crate::style::RadialSize::FarthestSide => sides.into_iter().fold(0.0_f32, f32::max),
                crate::style::RadialSize::ClosestCorner => {
                    corners.into_iter().fold(f32::INFINITY, f32::min)
                }
                crate::style::RadialSize::FarthestCorner => {
                    corners.into_iter().fold(0.0_f32, f32::max)
                }
            };
            let r = r.max(1e-6);
            (r, r)
        }
    }
}

fn pack_gpu_gradient(g: &crate::style::Gradient) -> Option<GpuFillGrad> {
    let (kind, angle, center, radii, stops) = match g {
        crate::style::Gradient::Conic(_) => return None,
        crate::style::Gradient::Linear(lg) => {
            if lg.repeating || lg.hints.iter().any(|h| h.is_some()) {
                return None;
            }
            (
                1u32,
                lg.angle_deg,
                [0.5, 0.5],
                [1.0, 1.0],
                pack_stops(&lg.stops)?,
            )
        }
        crate::style::Gradient::Radial(rg) => {
            if rg.repeating || rg.hints.iter().any(|h| h.is_some()) {
                return None;
            }
            let (rx, ry) = radial_radii_unit(rg);
            (
                2u32,
                0.0,
                [rg.position.x, rg.position.y],
                [rx, ry],
                pack_stops(&rg.stops)?,
            )
        }
    };
    let mut stop_t = [1.0f32; 8];
    let mut stop_c = [[0.0f32; 4]; 8];
    let n = stops.len().min(8);
    for (i, (t, c)) in stops.iter().take(n).enumerate() {
        stop_t[i] = *t;
        stop_c[i] = *c;
    }
    Some(GpuFillGrad {
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

pub(crate) fn build_fill_op(cmds: &[PathCommand], style: &PathStyle) -> Option<PathGpuFill> {
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
    for (pts, closed) in subpaths(cmds) {
        if !closed || pts.len() < 3 {
            continue;
        }
        stencil_fan(&pts, cover_color, &mut stencil);
        for &(x, y) in &pts {
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
    let pad = 0.75;
    min_x -= pad;
    min_y -= pad;
    max_x += pad;
    max_y += pad;
    let mut grad = style.fill_gradient.as_ref().and_then(pack_gpu_gradient);
    if let Some(g) = grad.as_mut() {
        g.bbox = [
            min_x,
            min_y,
            (max_x - min_x).max(1e-6),
            (max_y - min_y).max(1e-6),
        ];
    }
    Some(PathGpuFill {
        even_odd: matches!(style.fill_rule, FillRule::EvenOdd),
        stencil,
        cover: cover_quad((min_x, min_y), (max_x, max_y), cover_color),
        grad,
        clip: None,
    })
}

fn build_gpu_ops(
    layers: &[(Vec<PathCommand>, PathStyle)],
    markers: &[Point2],
    marker_r: f32,
) -> Vec<PathGpuOp> {
    let mut ops = Vec::new();
    let last = layers.len().saturating_sub(1);
    for (i, (cmds, style)) in layers.iter().enumerate() {
        if let Some(fill) = build_fill_op(cmds, style) {
            ops.push(PathGpuOp::Fill(fill));
        }
        let mr = if i == last { marker_r } else { 0.0 };
        let m = if i == last { markers } else { &[][..] };
        let mut stroke_style = style.clone();
        stroke_style.fill = None;
        stroke_style.fill_gradient = None;
        let mesh = tessellate_path_mesh(cmds, &stroke_style, m, mr);
        if !mesh.is_empty() {
            ops.push(PathGpuOp::Mesh(PathGpuMesh {
                tris: mesh,
                clip: None,
            }));
        }
    }
    ops
}

pub fn parse_path_viewbox(s: &str) -> Option<[f32; 4]> {
    let normalized = s.replace(',', " ");
    let mut it = normalized
        .split_whitespace()
        .filter_map(|t| t.parse::<f32>().ok());
    Some([it.next()?, it.next()?, it.next()?, it.next()?])
}

fn fit_viewbox_to_leaf(attrs: &HashMap<String, String>) -> bool {
    matches!(
        attrs.get(ATTR_PATH_FIT_LEAF).map(|s| s.trim()),
        Some("1" | "true" | "yes")
    )
}

/// viewBox：**高**在 span≥2 时锁定像素轴（防滚动列撑高）；**宽**始终跟叶，避免 Chart 320 逻辑宽被锁死后横向拉伸发虚。
/// [`ATTR_PATH_FIT_LEAF`]：两端都跟叶（公开 Svg）。
pub fn path_target_leaf_size(attrs: &HashMap<String, String>, leaf: [f32; 2]) -> [f32; 2] {
    if fit_viewbox_to_leaf(attrs) {
        return [leaf[0].max(1.0), leaf[1].max(1.0)];
    }
    let Some(vb) = attrs
        .get(ATTR_PATH_VIEWBOX)
        .and_then(|s| parse_path_viewbox(s))
    else {
        return [leaf[0].max(1.0), leaf[1].max(1.0)];
    };
    let w = leaf[0].max(1.0);
    let h = if vb[3] >= 2.0 {
        vb[3].min(leaf[1].max(1.0))
    } else {
        leaf[1].max(1.0)
    };
    [w, h]
}

fn scale_commands(cmds: &[PathCommand], vb: [f32; 4], leaf: [f32; 2]) -> Vec<PathCommand> {
    let sx = if vb[2].abs() > 1e-6 {
        leaf[0] / vb[2]
    } else {
        1.0
    };
    let sy = if vb[3].abs() > 1e-6 {
        leaf[1] / vb[3]
    } else {
        1.0
    };
    let map = |p: Point2| ((p.0 - vb[0]) * sx, (p.1 - vb[1]) * sy);
    cmds.iter()
        .cloned()
        .map(|c| match c {
            PathCommand::Arc {
                radii,
                x_rot_deg,
                large,
                sweep,
                to,
            } => PathCommand::Arc {
                radii: (radii.0 * sx.abs(), radii.1 * sy.abs()),
                x_rot_deg,
                large,
                sweep,
                to: map(to),
            },
            other => other.map_xy(map),
        })
        .collect()
}

/// 闭合正多边形（点 mark 的实心圆）：中心按各轴缩放，半径按 `min(sx,sy)`，避免被压成椭圆。
fn detect_regular_circle(cmds: &[PathCommand]) -> Option<(f32, f32, f32, usize)> {
    let closed = cmds.iter().any(|c| matches!(c, PathCommand::Close));
    if !closed {
        return None;
    }
    let mut pts = points_from_commands(cmds);
    // Closing arcs repeat the first vertex; counting it twice biases the center
    // when adaptive tessellation uses fewer segments.
    if pts.len() > 1
        && (pts[0].0 - pts[pts.len() - 1].0).abs() < 1e-4
        && (pts[0].1 - pts[pts.len() - 1].1).abs() < 1e-4
    {
        pts.pop();
    }
    let n = pts.len();
    if n < 8 {
        return None;
    }
    let nf = n as f32;
    let cx = pts.iter().map(|p| p.0).sum::<f32>() / nf;
    let cy = pts.iter().map(|p| p.1).sum::<f32>() / nf;
    let mut r_min = f32::MAX;
    let mut r_max = 0.0f32;
    for &(x, y) in &pts {
        let r = (x - cx).hypot(y - cy);
        r_min = r_min.min(r);
        r_max = r_max.max(r);
    }
    if r_min < 0.5 || r_max > r_min * 1.12 {
        return None;
    }
    Some((cx, cy, (r_min + r_max) * 0.5, n))
}

fn scale_commands_preserve_circles(
    cmds: &[PathCommand],
    vb: [f32; 4],
    target: [f32; 2],
) -> Vec<PathCommand> {
    let sx = if vb[2].abs() > 1e-6 {
        target[0] / vb[2]
    } else {
        1.0
    };
    let sy = if vb[3].abs() > 1e-6 {
        target[1] / vb[3]
    } else {
        1.0
    };
    if (sx - sy).abs() > 0.02 {
        if let Some((cx, cy, r, n)) = detect_regular_circle(cmds) {
            let c = ((cx - vb[0]) * sx, (cy - vb[1]) * sy);
            return filled_circle(c.0, c.1, r * sx.min(sy).max(0.25), n);
        }
    }
    scale_commands(cmds, vb, target)
}

/// 布局期：attrs → 世界坐标四边形。`origin` = 叶左上；`leaf_size` 用于 viewBox 缩放。
pub fn path_quads_from_element_attrs(
    attrs: &HashMap<String, String>,
    origin: [f32; 2],
    leaf_size: [f32; 2],
) -> Option<Vec<PathQuad>> {
    let raw = commands_from_attrs(attrs);
    if raw.is_empty() {
        return None;
    }
    let target = path_target_leaf_size(attrs, leaf_size);
    let cmds = if let Some(vb) = attrs
        .get(ATTR_PATH_VIEWBOX)
        .and_then(|s| parse_path_viewbox(s))
    {
        scale_commands_preserve_circles(&raw, vb, target)
    } else {
        raw
    };
    let style = style_from_attrs(attrs);
    let mut quads = tessellate_path(&cmds, &style);
    if quads.is_empty() {
        return None;
    }
    for q in &mut quads {
        q.pos.0 += origin[0];
        q.pos.1 += origin[1];
    }
    Some(quads)
}

const MAX_RASTER_PX: u32 = 1024 * 1024;
const MAX_RASTER_CACHE: usize = 64;
const MAX_SCENE_STORE: usize = 64;

#[derive(Clone)]
struct PathScene {
    layers: Vec<(Vec<PathCommand>, PathStyle)>,
    markers: Vec<Point2>,
    marker_r: f32,
    vb: [f32; 4],
}

struct PathCpuEntry {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

thread_local! {
    /// CPU rgba cache so TexturePool eviction can rehydrate without re-rasterizing.
    static RASTER_CPU: RefCell<HashMap<String, PathCpuEntry>> = RefCell::new(HashMap::new());
    static RASTER_NEEDED: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
    static SCENE_STORE: RefCell<HashMap<u64, PathScene>> = RefCell::new(HashMap::new());
    static PATH_JOBS: RefCell<Vec<PathGpuJob>> = const { RefCell::new(Vec::new()) };
    static RT_READBACK_REQ: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
    static RT_READBACK: RefCell<HashMap<String, (u32, u32, Vec<u8>)>> = RefCell::new(HashMap::new());
}

/// GPU PathPipeline 作业：布局收集排队，渲染期画到 `rui_path:` / `rui_canvas:`。
#[derive(Clone, Debug)]
pub struct PathGpuJob {
    pub key: String,
    pub width: u32,
    pub height: u32,
    pub(crate) ops: Vec<PathGpuOp>,
    /// Canvas keys are id-stable; skip-if-same-size would freeze `request_frame`.
    pub force_redraw: bool,
}

pub(crate) fn queue_path_job(job: PathGpuJob) {
    PATH_JOBS.with(|j| j.borrow_mut().push(job));
}

pub(crate) fn take_path_jobs() -> Vec<PathGpuJob> {
    PATH_JOBS.with(|j| std::mem::take(&mut *j.borrow_mut()))
}

pub(crate) fn has_path_jobs() -> bool {
    PATH_JOBS.with(|j| !j.borrow().is_empty())
}

/// Paint called `get_image_data`: after PathPipeline flush, copy the RT (低频, stalls).
pub fn request_path_rt_readback(key: impl Into<String>) {
    RT_READBACK_REQ.with(|s| {
        s.borrow_mut().insert(key.into());
    });
}

#[cfg(test)]
pub(crate) fn path_rt_readback_is_pending(key: &str) -> bool {
    RT_READBACK_REQ.with(|s| s.borrow().contains(key))
}

pub(crate) fn take_path_rt_readback_request(key: &str) -> bool {
    RT_READBACK_REQ.with(|s| s.borrow_mut().remove(key))
}

pub(crate) fn store_path_rt_readback(key: String, width: u32, height: u32, pixels: Vec<u8>) {
    RT_READBACK.with(|m| {
        m.borrow_mut().insert(key, (width, height, pixels));
    });
}

/// GPU RT pixels from the last flush that honored [`request_path_rt_readback`].
pub fn take_path_rt_readback(key: &str) -> Option<(u32, u32, Vec<u8>)> {
    RT_READBACK.with(|m| m.borrow_mut().remove(key))
}

pub(crate) fn enrich_path_graph(graph: &mut crate::renderer::RenderGraph) {
    PATH_JOBS.with(|j| {
        for job in j.borrow().iter() {
            graph.push(
                crate::renderer::RenderPassNode::new(
                    crate::renderer::PassKind::Path2d,
                    format!("path-{}", job.key),
                )
                .write(job.key.clone()),
            );
        }
    });
}

/// 注册多层 path 场景，返回 hex 键（写入 [`ATTR_PATH_SCENE`]）。
pub fn register_path_scene(
    layers: Vec<(Vec<PathCommand>, PathStyle)>,
    markers: Vec<Point2>,
    marker_r: f32,
    vb_w: f32,
    vb_h: f32,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    vb_w.to_bits().hash(&mut hasher);
    vb_h.to_bits().hash(&mut hasher);
    marker_r.to_bits().hash(&mut hasher);
    for (cmds, style) in &layers {
        encode_path_d(cmds).hash(&mut hasher);
        hash_style(&mut hasher, style);
    }
    for &(x, y) in &markers {
        x.to_bits().hash(&mut hasher);
        y.to_bits().hash(&mut hasher);
    }
    let id = hasher.finish();
    SCENE_STORE.with(|s| {
        let mut g = s.borrow_mut();
        if g.len() >= MAX_SCENE_STORE {
            g.clear();
        }
        g.insert(
            id,
            PathScene {
                layers,
                markers,
                marker_r,
                vb: [0.0, 0.0, vb_w.max(1.0), vb_h.max(1.0)],
            },
        );
    });
    id
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
fn nonzero_contains(p: Point2, pts: &[Point2]) -> bool {
    let ring = ring_pts(pts);
    let n = ring.len();
    if n < 3 {
        return false;
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
    winding != 0
}

fn fill_contains(p: Point2, pts: &[Point2], rule: FillRule) -> bool {
    match rule {
        FillRule::EvenOdd => even_odd_contains(p, pts),
        FillRule::NonZero => nonzero_contains(p, pts),
    }
}

/// Fill hit in path user space (after CTM if the caller applied it). Canvas `is_point_in_path`.
pub fn path_contains_point(cmds: &[PathCommand], p: Point2, rule: FillRule) -> bool {
    for (pts, closed) in subpaths(cmds) {
        if closed && pts.len() >= 3 && fill_contains(p, &pts, rule) {
            return true;
        }
    }
    false
}

/// Stroke hit in path user space. Canvas `is_point_in_stroke`（当前线宽 / cap）。
pub fn path_stroke_contains(cmds: &[PathCommand], style: &PathStyle, p: Point2) -> bool {
    let mut st = style.clone();
    if st.stroke.is_none() {
        st.stroke = Some([0.0, 0.0, 0.0, 1.0]);
    }
    point_in_stroke(cmds, &st, p)
}

fn dist_to_segment(p: Point2, a: Point2, b: Point2) -> f32 {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let len2 = dx * dx + dy * dy;
    if len2 < 1e-12 {
        return (p.0 - a.0).hypot(p.1 - a.1);
    }
    let t = ((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2;
    let t = t.clamp(0.0, 1.0);
    (p.0 - (a.0 + t * dx)).hypot(p.1 - (a.1 + t * dy))
}

fn point_in_fill(cmds: &[PathCommand], style: &PathStyle, p: Point2) -> bool {
    if style.fill.is_none() && style.fill_gradient.is_none() {
        return false;
    }
    for (pts, closed) in subpaths(cmds) {
        if !closed || pts.len() < 3 {
            continue;
        }
        if fill_contains(p, &pts, style.fill_rule) {
            return true;
        }
    }
    false
}

fn point_in_stroke(cmds: &[PathCommand], style: &PathStyle, p: Point2) -> bool {
    let Some(_) = style.stroke else {
        return false;
    };
    let hw = style.stroke_width.max(0.5) * 0.5;
    let round_caps = style.stroke_linecap == StrokeLinecap::Round;
    let square_caps = style.stroke_linecap == StrokeLinecap::Square;
    for (pts, closed) in subpaths(cmds) {
        if pts.len() < 2 {
            if pts.len() == 1 && (round_caps || square_caps) {
                let q = pts[0];
                if (p.0 - q.0).hypot(p.1 - q.1) <= hw {
                    return true;
                }
            }
            continue;
        }
        for win in pts.windows(2) {
            if dist_to_segment(p, win[0], win[1]) <= hw {
                return true;
            }
        }
        if closed && pts.len() >= 2 {
            let a = pts[pts.len() - 1];
            let b = pts[0];
            if (a.0 - b.0).abs() > 1e-4 || (a.1 - b.1).abs() > 1e-4 {
                if dist_to_segment(p, a, b) <= hw {
                    return true;
                }
            }
        }
        if !closed && pts.len() >= 2 {
            if round_caps {
                let first = pts[0];
                let last = pts[pts.len() - 1];
                if (p.0 - first.0).hypot(p.1 - first.1) <= hw
                    || (p.0 - last.0).hypot(p.1 - last.1) <= hw
                {
                    return true;
                }
            } else if square_caps {
                if let Some(d) = unit_dir(pts[1], pts[0]) {
                    let ext = (pts[0].0 + d.0 * hw, pts[0].1 + d.1 * hw);
                    if dist_to_segment(p, pts[0], ext) <= hw {
                        return true;
                    }
                }
                let last = pts.len() - 1;
                if let Some(d) = unit_dir(pts[last - 1], pts[last]) {
                    let ext = (pts[last].0 + d.0 * hw, pts[last].1 + d.1 * hw);
                    if dist_to_segment(p, pts[last], ext) <= hw {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn layer_hits_point(
    cmds: &[PathCommand],
    style: &PathStyle,
    p: Point2,
    policy: crate::core::tree::PathHitPolicy,
) -> bool {
    let fill_painted = style.fill.is_some() || style.fill_gradient.is_some();
    let stroke_painted = style.stroke.is_some() && style.stroke_width > 0.0;
    if policy.test_fill {
        let allow = !policy.require_fill_painted || fill_painted;
        if allow && point_in_fill(cmds, style, p) {
            return true;
        }
        // `fill` / `visibleFill`：几何填充命中，即使 fill=none 也测“内部”
        if !policy.require_fill_painted && !fill_painted {
            for (pts, closed) in subpaths(cmds) {
                if closed && pts.len() >= 3 && fill_contains(p, &pts, style.fill_rule) {
                    return true;
                }
            }
        }
    }
    if policy.test_stroke {
        let allow = !policy.require_stroke_painted || stroke_painted;
        if allow {
            if stroke_painted {
                if point_in_stroke(cmds, style, p) {
                    return true;
                }
            } else if !policy.require_stroke_painted {
                // stroke=none：仍按默认宽度测 perimeter（MDN visibleStroke / stroke / all / visible）
                let mut ghost = style.clone();
                ghost.stroke = Some([0.0, 0.0, 0.0, 1.0]);
                if ghost.stroke_width <= 0.0 {
                    ghost.stroke_width = 1.0;
                }
                if point_in_stroke(cmds, &ghost, p) {
                    return true;
                }
            }
        }
    }
    false
}

fn markers_hit(markers: &[Point2], marker_r: f32, p: Point2) -> bool {
    if markers.is_empty() || marker_r <= 0.0 {
        return false;
    }
    for &(mx, my) in markers {
        if (p.0 - mx).hypot(p.1 - my) <= marker_r {
            return true;
        }
    }
    false
}

/// Element 是否挂有可几何命中的 path 场景 / `d`。
pub fn element_has_path_geometry(attrs: &HashMap<String, String>) -> bool {
    if attrs
        .get(ATTR_PATH_SCENE)
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false)
    {
        return true;
    }
    attrs
        .get(ATTR_PATH_D)
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false)
}

/// 叶局部坐标下的 SVG/path `pointer-events` 几何命中（MDN fill/stroke 细分）。
///
/// `local` 相对叶左上；`leaf` 为布局宽高。无场景时返回 `false`（由调用方回退边界盒）。
pub fn path_pointer_hit(
    attrs: &HashMap<String, String>,
    leaf: [f32; 2],
    local: Point2,
    policy: crate::core::tree::PathHitPolicy,
) -> bool {
    let target = path_target_leaf_size(attrs, leaf);
    if let Some(scene_hex) = attrs.get(ATTR_PATH_SCENE) {
        if let Ok(id) = u64::from_str_radix(scene_hex.trim(), 16) {
            let scene = SCENE_STORE.with(|s| s.borrow().get(&id).cloned());
            if let Some(scene) = scene {
                for (cmds, style) in &scene.layers {
                    let scaled = scale_commands_preserve_circles(cmds, scene.vb, target);
                    if layer_hits_point(&scaled, style, local, policy) {
                        return true;
                    }
                }
                let markers = scale_points(&scene.markers, scene.vb, target);
                let sx = target[0] / scene.vb[2].max(1e-6);
                let sy = target[1] / scene.vb[3].max(1e-6);
                let mr = scene.marker_r * sx.min(sy).max(0.25);
                if (policy.test_fill || policy.test_stroke) && markers_hit(&markers, mr, local) {
                    return true;
                }
                return false;
            }
        }
    }

    let raw = commands_from_attrs(attrs);
    if raw.is_empty() {
        return false;
    }
    let cmds = if let Some(vb) = attrs
        .get(ATTR_PATH_VIEWBOX)
        .and_then(|s| parse_path_viewbox(s))
    {
        scale_commands_preserve_circles(&raw, vb, target)
    } else {
        raw
    };
    let style = style_from_attrs(attrs);
    layer_hits_point(&cmds, &style, local, policy)
}

fn scale_points(pts: &[Point2], vb: [f32; 4], leaf: [f32; 2]) -> Vec<Point2> {
    let sx = if vb[2].abs() > 1e-6 {
        leaf[0] / vb[2]
    } else {
        1.0
    };
    let sy = if vb[3].abs() > 1e-6 {
        leaf[1] / vb[3]
    } else {
        1.0
    };
    pts.iter()
        .map(|&(x, y)| ((x - vb[0]) * sx, (y - vb[1]) * sy))
        .collect()
}

fn hash_style(h: &mut DefaultHasher, style: &PathStyle) {
    match style.fill {
        Some(c) => {
            1u8.hash(h);
            for v in c {
                v.to_bits().hash(h);
            }
        }
        None => 0u8.hash(h),
    }
    match &style.fill_gradient {
        Some(g) => {
            1u8.hash(h);
            format!("{g:?}").hash(h);
        }
        None => 0u8.hash(h),
    }
    match style.stroke {
        Some(c) => {
            1u8.hash(h);
            for v in c {
                v.to_bits().hash(h);
            }
        }
        None => 0u8.hash(h),
    }
    style.stroke_width.to_bits().hash(h);
    (style.stroke_linecap as u8).hash(h);
    (style.stroke_linejoin as u8).hash(h);
    (style.fill_rule as u8).hash(h);
}

pub(crate) fn src_over_premul(buf: &mut [f32], i: usize, r: f32, g: f32, b: f32, a: f32) {
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
    let ssaa = ssaa.max(1);
    let inv_ssaa = 1.0 / ssaa as f32;
    let has_open = paths.iter().any(|(_, c)| !*c);

    if style.fill.is_some() || style.fill_gradient.is_some() {
        for (pts, closed) in &paths {
            if !*closed || pts.len() < 3 {
                continue;
            }
            let solid = style.fill.unwrap_or([0.0, 0.0, 0.0, 1.0]);
            if style.fill_gradient.is_none() && solid[3] <= 0.0 {
                continue;
            }
            let mut min_x = f32::MAX;
            let mut min_y = f32::MAX;
            let mut max_x = f32::MIN;
            let mut max_y = f32::MIN;
            for &(x, y) in pts.iter() {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
            let bw = (max_x - min_x).max(1e-6);
            let bh = (max_y - min_y).max(1e-6);
            let x0 = min_x.floor().max(0.0) as usize;
            let y0 = min_y.floor().max(0.0) as usize;
            let x1 = (max_x.ceil() as usize + 1).min(w);
            let y1 = (max_y.ceil() as usize + 1).min(h);
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
                            if even_odd_contains((px, py), pts) {
                                cover += 1.0;
                            }
                        }
                    }
                    cover *= inv_ssaa * inv_ssaa;
                    if cover > 0.0 {
                        let fill = if let Some(ref grad) = style.fill_gradient {
                            let nx = ((x as f32 + 0.5) - min_x) / bw;
                            let ny = ((y as f32 + 0.5) - min_y) / bh;
                            let mut c = crate::style::sample_gradient(grad, nx, ny);
                            c[3] *= solid[3];
                            c
                        } else {
                            solid
                        };
                        if fill[3] <= 0.0 {
                            continue;
                        }
                        let i = (y * w + x) * 4;
                        src_over_premul(buf, i, fill[0], fill[1], fill[2], fill[3] * cover);
                    }
                }
            }
        }
    }

    if let Some(stroke) = style.stroke {
        let sw = style.stroke_width.max(0.5);
        let hw = sw * 0.5;
        if stroke[3] > 0.0 {
            // Coverage buffer: accumulate max coverage per pixel across all segments
            // of a subpath, then blend once. Prevents alpha double-blending at joints
            // that caused visible darkening / jitter on fast mouse strokes.
            let mut cov_buf = vec![0.0f32; w * h];
            for (pts, closed) in &paths {
                if pts.len() < 2 {
                    continue;
                }
                if (style.fill.is_some() || style.fill_gradient.is_some()) && has_open && *closed {
                    continue;
                }
                let pad = hw + 1.5;

                // Compute subpath AABB for the coverage region to clear.
                let mut sp_min_x = f32::MAX;
                let mut sp_min_y = f32::MAX;
                let mut sp_max_x = f32::MIN;
                let mut sp_max_y = f32::MIN;
                for &(px, py) in pts.iter() {
                    sp_min_x = sp_min_x.min(px);
                    sp_min_y = sp_min_y.min(py);
                    sp_max_x = sp_max_x.max(px);
                    sp_max_y = sp_max_y.max(py);
                }
                let sp_x0 = (sp_min_x - pad).floor().max(0.0) as usize;
                let sp_y0 = (sp_min_y - pad).floor().max(0.0) as usize;
                let sp_x1 = ((sp_max_x + pad).ceil() as usize + 1).min(w);
                let sp_y1 = ((sp_max_y + pad).ceil() as usize + 1).min(h);

                for y in sp_y0..sp_y1 {
                    for x in sp_x0..sp_x1 {
                        cov_buf[y * w + x] = 0.0;
                    }
                }

                for seg in pts.windows(2) {
                    let a = seg[0];
                    let b = seg[1];
                    let min_x = a.0.min(b.0);
                    let min_y = a.1.min(b.1);
                    let max_x = a.0.max(b.0);
                    let max_y = a.1.max(b.1);
                    let x0 = (min_x - pad).floor().max(0.0) as usize;
                    let y0 = (min_y - pad).floor().max(0.0) as usize;
                    let x1 = ((max_x + pad).ceil() as usize + 1).min(w);
                    let y1 = ((max_y + pad).ceil() as usize + 1).min(h);
                    for y in y0..y1 {
                        for x in x0..x1 {
                            let p = (x as f32 + 0.5, y as f32 + 0.5);
                            let d = dist_to_segment(p, a, b);
                            let cover = (hw + 0.5 - d).clamp(0.0, 1.0);
                            if cover > 0.0 {
                                let ci = y * w + x;
                                if cover > cov_buf[ci] {
                                    cov_buf[ci] = cover;
                                }
                            }
                        }
                    }
                }

                // Stamp round joins / caps at vertices when linecap/linejoin request it.
                let stamp_round = matches!(
                    style.stroke_linecap,
                    StrokeLinecap::Round | StrokeLinecap::Square
                ) || style.stroke_linejoin == StrokeLinejoin::Round;
                if stamp_round {
                    for &vtx in pts.iter() {
                        let x0 = (vtx.0 - pad).floor().max(0.0) as usize;
                        let y0 = (vtx.1 - pad).floor().max(0.0) as usize;
                        let x1 = ((vtx.0 + pad).ceil() as usize + 1).min(w);
                        let y1 = ((vtx.1 + pad).ceil() as usize + 1).min(h);
                        for y in y0..y1 {
                            for x in x0..x1 {
                                let p = (x as f32 + 0.5, y as f32 + 0.5);
                                let d = (p.0 - vtx.0).hypot(p.1 - vtx.1);
                                let cover = (hw + 0.5 - d).clamp(0.0, 1.0);
                                if cover > 0.0 {
                                    let ci = y * w + x;
                                    if cover > cov_buf[ci] {
                                        cov_buf[ci] = cover;
                                    }
                                }
                            }
                        }
                    }
                }

                for y in sp_y0..sp_y1 {
                    for x in sp_x0..sp_x1 {
                        let ci = y * w + x;
                        let cover = cov_buf[ci];
                        if cover > 0.0 && pixel_in_clip(x, y, clip) {
                            let i = (y * w + x) * 4;
                            src_over_premul(
                                buf,
                                i,
                                stroke[0],
                                stroke[1],
                                stroke[2],
                                stroke[3] * cover,
                            );
                        }
                    }
                }
            }
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

pub(crate) fn premul_to_straight_rgba(buf: &[f32], width: u32, height: u32) -> Vec<u8> {
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
pub(crate) fn paint_path_premul(
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

/// 光栅并排队 GPU 贴图；相同内容复用 `rui_path:{hash}`。
/// CPU 像素保留在缓存中，供 [`flush_path_textures`] 在 TexturePool 驱逐后回补。
pub fn ensure_raster_texture(
    layers: &[(Vec<PathCommand>, PathStyle)],
    markers: &[Point2],
    marker_radius: f32,
    width: u32,
    height: u32,
    ssaa: u32,
) -> Option<String> {
    if layers.is_empty() || width < 2 || height < 2 {
        return None;
    }
    if width.saturating_mul(height) > MAX_RASTER_PX {
        return None;
    }
    let mut hasher = DefaultHasher::new();
    width.hash(&mut hasher);
    height.hash(&mut hasher);
    ssaa.hash(&mut hasher);
    marker_radius.to_bits().hash(&mut hasher);
    for (cmds, style) in layers {
        encode_path_d(cmds).hash(&mut hasher);
        hash_style(&mut hasher, style);
    }
    for &(x, y) in markers {
        x.to_bits().hash(&mut hasher);
        y.to_bits().hash(&mut hasher);
    }
    let key = hasher.finish();
    let tex_id = format!("rui_path:{key:016x}");
    RASTER_CPU.with(|c| {
        let mut g = c.borrow_mut();
        if !g.contains_key(&tex_id) {
            if g.len() >= MAX_RASTER_CACHE {
                g.clear();
            }
            let rgba = rasterize_layers_rgba(layers, markers, marker_radius, width, height, ssaa);
            g.insert(
                tex_id.clone(),
                PathCpuEntry {
                    width,
                    height,
                    rgba,
                },
            );
        }
    });
    RASTER_NEEDED.with(|n| {
        n.borrow_mut().insert(tex_id.clone());
    });
    Some(tex_id)
}

/// Re-queue path rasters that are missing from the GPU pool (after LRU eviction).
pub fn flush_path_textures(mut has_gpu: impl FnMut(&str) -> bool) -> bool {
    let needed: Vec<String> = RASTER_NEEDED
        .with(|n| std::mem::take(&mut *n.borrow_mut()))
        .into_iter()
        .collect();
    let mut any = false;
    RASTER_CPU.with(|c| {
        let map = c.borrow();
        for key in needed {
            if has_gpu(&key) {
                continue;
            }
            if let Some(entry) = map.get(&key) {
                crate::app::pending_images::queue_image_rgba8(
                    &key,
                    entry.width,
                    entry.height,
                    entry.rgba.clone(),
                );
                any = true;
            }
        }
    });
    any
}

/// RGBA8 行互换：缓冲顶行 ↔ 底行（仅测试 / 调试用；上传路径不再翻转）。
#[cfg(test)]
pub(crate) fn flip_rgba_vertical(rgba: &mut [u8], width: u32, height: u32) {
    let stride = width as usize * 4;
    let h = height as usize;
    if stride == 0 || h < 2 || rgba.len() < stride * h {
        return;
    }
    for y in 0..h / 2 {
        let top = y * stride;
        let bot = (h - 1 - y) * stride;
        for i in 0..stride {
            rgba.swap(top + i, bot + i);
        }
    }
}

/// 布局期：attrs → 光栅贴图键 + 绘制宽高（viewBox 像素轴已锁定）。失败则 tessellate。
pub fn path_raster_from_element_attrs(
    attrs: &HashMap<String, String>,
    leaf_w: f32,
    leaf_h: f32,
) -> Option<(String, f32, f32)> {
    let prepared = prepare_path_draw(attrs, leaf_w, leaf_h)?;
    let tex = ensure_raster_texture(
        &prepared.layers,
        &prepared.markers,
        prepared.marker_r,
        prepared.width,
        prepared.height,
        2,
    )?;
    Some((tex, prepared.target[0], prepared.target[1]))
}

/// 布局期：产品 GPU 核。复杂渐变回 `None`（调用方走 CPU golden）。
pub fn path_gpu_from_element_attrs(
    attrs: &HashMap<String, String>,
    leaf_w: f32,
    leaf_h: f32,
) -> Option<(String, f32, f32)> {
    if !crate::renderer::gpu2d_enabled() {
        return None;
    }
    let prepared = prepare_path_draw(attrs, leaf_w, leaf_h)?;
    if mesh_needs_cpu(&prepared.layers) {
        return None;
    }
    let ops = build_gpu_ops(&prepared.layers, &prepared.markers, prepared.marker_r);
    if ops.iter().all(PathGpuOp::is_empty) {
        return None;
    }
    let mut hasher = DefaultHasher::new();
    b"stencil-v1".hash(&mut hasher);
    prepared.width.hash(&mut hasher);
    prepared.height.hash(&mut hasher);
    prepared.marker_r.to_bits().hash(&mut hasher);
    for (cmds, style) in &prepared.layers {
        encode_path_d(cmds).hash(&mut hasher);
        hash_style(&mut hasher, style);
    }
    for &(x, y) in &prepared.markers {
        x.to_bits().hash(&mut hasher);
        y.to_bits().hash(&mut hasher);
    }
    let key = format!("rui_path:{:016x}", hasher.finish());
    queue_path_job(PathGpuJob {
        key: key.clone(),
        width: prepared.width,
        height: prepared.height,
        ops,
        force_redraw: false,
    });
    Some((key, prepared.target[0], prepared.target[1]))
}

struct PreparedPathDraw {
    layers: Vec<(Vec<PathCommand>, PathStyle)>,
    markers: Vec<Point2>,
    marker_r: f32,
    target: [f32; 2],
    width: u32,
    height: u32,
}

fn prepare_path_draw(
    attrs: &HashMap<String, String>,
    leaf_w: f32,
    leaf_h: f32,
) -> Option<PreparedPathDraw> {
    let target = path_target_leaf_size(attrs, [leaf_w, leaf_h]);
    let dpi = crate::style::gradient_raster_dpi();
    let raster_target = [target[0] * dpi, target[1] * dpi];
    let w = raster_target[0].ceil() as i32;
    let h = raster_target[1].ceil() as i32;
    if w < 2 || h < 2 {
        return None;
    }
    let width = w as u32;
    let height = h as u32;
    if width.saturating_mul(height) > MAX_RASTER_PX {
        return None;
    }

    if let Some(scene_hex) = attrs.get(ATTR_PATH_SCENE) {
        if let Ok(id) = u64::from_str_radix(scene_hex.trim(), 16) {
            let scene = SCENE_STORE.with(|s| s.borrow().get(&id).cloned());
            if let Some(scene) = scene {
                let layers: Vec<(Vec<PathCommand>, PathStyle)> = scene
                    .layers
                    .iter()
                    .map(|(cmds, st)| {
                        (
                            scale_commands_preserve_circles(cmds, scene.vb, raster_target),
                            {
                                let mut style = st.clone();
                                style.stroke_width *= dpi;
                                style
                            },
                        )
                    })
                    .collect();
                if layers.iter().all(|(c, _)| c.is_empty()) && scene.markers.is_empty() {
                    return None;
                }
                let markers = scale_points(&scene.markers, scene.vb, raster_target);
                let sx = target[0] / scene.vb[2].max(1e-6);
                let sy = target[1] / scene.vb[3].max(1e-6);
                let marker_r = scene.marker_r * sx.min(sy).max(0.25) * dpi;
                return Some(PreparedPathDraw {
                    layers,
                    markers,
                    marker_r,
                    target,
                    width,
                    height,
                });
            }
        }
    }

    let raw = commands_from_attrs(attrs);
    if raw.is_empty() {
        return None;
    }
    let vb = attrs
        .get(ATTR_PATH_VIEWBOX)
        .and_then(|s| parse_path_viewbox(s));
    let cmds = if let Some(vb) = vb {
        scale_commands_preserve_circles(&raw, vb, raster_target)
    } else {
        scale_commands(&raw, [0.0, 0.0, target[0], target[1]], raster_target)
    };
    let mut style = style_from_attrs(attrs);
    style.stroke_width *= dpi;
    let mut markers = attrs
        .get(ATTR_PATH_MARKERS)
        .map(|s| decode_points(s))
        .unwrap_or_default();
    if let Some(vb) = vb {
        markers = scale_points(&markers, vb, raster_target);
    } else {
        markers = scale_points(&markers, [0.0, 0.0, target[0], target[1]], raster_target);
    }
    let marker_r = attrs
        .get("rui_path_marker_r")
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| (style.stroke_width / dpi).max(2.5) * 0.9);
    let sx = if let Some(vb) = vb {
        target[0] / vb[2].max(1e-6)
    } else {
        1.0
    };
    let marker_r = marker_r * sx.max(0.25) * dpi;
    Some(PreparedPathDraw {
        layers: vec![(cmds, style)],
        markers,
        marker_r,
        target,
        width,
        height,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_caps_and_markers_obey_pixel_error_budget() {
        for radius in [1.0, 4.0, 16.0, 64.0, 128.0] {
            let mut tris = Vec::new();
            disc_fan((0.0, 0.0), radius, [1.0; 4], &mut tris);
            for tri in tris {
                let midpoint = ((tri.b.0 + tri.c.0) * 0.5, (tri.b.1 + tri.c.1) * 0.5);
                let error = radius - midpoint.0.hypot(midpoint.1);
                assert!(
                    error <= 0.1251,
                    "round cap radius {radius} has visible polygon facets: chord error {error}"
                );
            }
        }
    }

    #[test]
    fn polyline_and_pie_encode() {
        let cmds = polyline(&[(0.0, 0.0), (1.0, 2.0)]);
        assert_eq!(cmds.len(), 2);
        let pie = pie_sector(0.0, 0.0, 10.0, 0.0, std::f32::consts::FRAC_PI_2, 4);
        assert!(pie.len() >= 4);
        assert!(
            pie.iter().any(|c| matches!(c, PathCommand::Arc { .. })),
            "pie_sector must keep Arc until flatten, got {pie:?}"
        );
        let ring = pie_ring(0.0, 0.0, 10.0, 4.0, 0.0, std::f32::consts::FRAC_PI_2, 4);
        assert!(
            ring.len() > pie.len(),
            "donut ring should have outer+inner arc, ring={} pie={}",
            ring.len(),
            pie.len()
        );
        let s = encode_points(&[(1.5, 2.5)]);
        assert_eq!(s, "1.500,2.500");
        let c = filled_circle(10.0, 20.0, 5.0, 16);
        assert!(
            c.iter().any(|cmd| matches!(cmd, PathCommand::Arc { .. })),
            "filled_circle must keep Arc until flatten, got {c:?}"
        );
        let ell = filled_ellipse(0.0, 0.0, 10.0, 4.0, 8);
        assert!(
            ell.iter().any(|cmd| matches!(
                cmd,
                PathCommand::Arc {
                    radii: (rx, ry),
                    ..
                } if (*rx - 10.0).abs() < 1e-3 && (*ry - 4.0).abs() < 1e-3
            )),
            "filled_ellipse must keep rx/ry Arc, got {ell:?}"
        );
        let mut pts = points_from_commands(&c);
        pts.pop(); // the closing arc repeats the first point
        let n = pts.len().max(1) as f32;
        let cx = pts.iter().map(|p| p.0).sum::<f32>() / n;
        let cy = pts.iter().map(|p| p.1).sum::<f32>() / n;
        assert!(
            (cx - 10.0).abs() < 0.2 && (cy - 20.0).abs() < 0.2,
            "centroid=({cx},{cy})"
        );
    }

    #[test]
    fn circle_stays_round_when_viewbox_is_stretched_wide() {
        let cmds = filled_circle(160.0, 70.0, 8.0, 20);
        let scaled =
            scale_commands_preserve_circles(&cmds, [0.0, 0.0, 320.0, 140.0], [640.0, 140.0]);
        let mut pts = points_from_commands(&scaled);
        pts.pop(); // the closing arc repeats the first point
        let n = pts.len().max(1) as f32;
        let cx = pts.iter().map(|p| p.0).sum::<f32>() / n;
        let cy = pts.iter().map(|p| p.1).sum::<f32>() / n;
        let mut r_min = f32::MAX;
        let mut r_max = 0.0f32;
        for &(x, y) in &pts {
            let r = (x - cx).hypot(y - cy);
            r_min = r_min.min(r);
            r_max = r_max.max(r);
        }
        assert!(
            (r_max - r_min) < 1.0,
            "dot must stay circular under anisotropic stretch, r=[{r_min},{r_max}]"
        );
        assert!(
            (cx - 320.0).abs() < 1.0 && (cy - 70.0).abs() < 1.0,
            "centroid should map by sx/sy, got ({cx},{cy})"
        );
    }

    #[test]
    fn path_stroke_contains_mid_segment_and_square_cap() {
        let cmds = polyline(&[(10.0, 10.0), (30.0, 10.0)]);
        let butt = PathStyle::stroke([1.0, 1.0, 1.0, 1.0], 4.0).linecap(StrokeLinecap::Butt);
        assert!(path_stroke_contains(&cmds, &butt, (20.0, 10.0)));
        assert!(!path_stroke_contains(&cmds, &butt, (20.0, 16.0)));
        let corner = (32.0, 12.0);
        assert!(
            !path_stroke_contains(&cmds, &butt, corner),
            "butt must miss the square-cap corner"
        );
        let square = PathStyle::stroke([1.0, 1.0, 1.0, 1.0], 4.0).linecap(StrokeLinecap::Square);
        assert!(
            path_stroke_contains(&cmds, &square, corner),
            "square cap must cover the outer rectangle corner"
        );
    }

    #[test]
    fn polyline_still_stretches_anisotropically() {
        let cmds = polyline(&[(0.0, 0.0), (320.0, 140.0)]);
        let scaled =
            scale_commands_preserve_circles(&cmds, [0.0, 0.0, 320.0, 140.0], [640.0, 140.0]);
        let pts = points_from_commands(&scaled);
        assert_eq!(pts.len(), 2);
        assert!((pts[1].0 - 640.0).abs() < 0.5 && (pts[1].1 - 140.0).abs() < 0.5);
    }

    #[test]
    fn pie_sector_is_not_treated_as_circle() {
        let cmds = pie_sector(50.0, 50.0, 40.0, 0.0, std::f32::consts::FRAC_PI_2, 12);
        assert!(
            detect_regular_circle(&cmds).is_none(),
            "wedge with a center vertex must keep anisotropic scale"
        );
    }

    #[test]
    fn path_d_roundtrip_mlz() {
        let cmds = vec![
            PathCommand::MoveTo((0.0, 0.0)),
            PathCommand::LineTo((10.0, 0.0)),
            PathCommand::LineTo((10.0, 8.0)),
            PathCommand::Close,
        ];
        let d = encode_path_d(&cmds);
        let back = decode_path_d(&d);
        assert_eq!(back, cmds);
    }

    #[test]
    fn loader_circle_arc_flattens_smoothly() {
        let cmds = decode_path_d("M21 12a9 9 0 1 1-6.219-8.56");
        let pts = points_from_commands(&cmds);
        for segment in pts.windows(2) {
            let midpoint = (
                (segment[0].0 + segment[1].0) * 0.5,
                (segment[0].1 + segment[1].1) * 0.5,
            );
            let error = 9.0 - (midpoint.0 - 12.0).hypot(midpoint.1 - 12.0);
            assert!(
                error.abs() <= 0.1251,
                "loader-circle arc must have a smooth silhouette: chord error {error}"
            );
        }
    }

    #[test]
    fn path_d_preserves_cubic_and_arc() {
        let cubic = decode_path_d("M0,0 C10,0 10,10 0,10");
        assert_eq!(cubic.len(), 2, "cubic must stay one CubicTo, got {cubic:?}");
        assert!(matches!(cubic[0], PathCommand::MoveTo((0.0, 0.0))));
        assert!(matches!(
            cubic[1],
            PathCommand::CubicTo {
                to: (x, y),
                ..
            } if (x - 0.0).abs() < 1e-3 && (y - 10.0).abs() < 0.2
        ));
        let flat = flatten_path(&cubic);
        assert!(
            flat.len() > 4,
            "flatten_path should emit multiple LineTo, got {}",
            flat.len()
        );
        let last = points_from_commands(&cubic).last().copied().unwrap();
        assert!((last.0 - 0.0).abs() < 0.2 && (last.1 - 10.0).abs() < 0.2);

        let tiny = flatten_path(&[
            PathCommand::MoveTo((0.0, 0.0)),
            PathCommand::CubicTo {
                c1: (0.2, 0.0),
                c2: (0.4, 0.0),
                to: (0.5, 0.0),
            },
        ]);
        assert!(
            tiny.len() <= 4,
            "near-linear 0.5px cubic must not force 12 segments, got {}",
            tiny.len()
        );

        let arc = decode_path_d("M10,0 A10,10 0 0 1 0,10");
        assert!(
            matches!(arc.last(), Some(PathCommand::Arc { .. })),
            "arc must stay Arc, got {arc:?}"
        );
        let end = points_from_commands(&arc).last().copied().unwrap();
        assert!((end.0 - 0.0).abs() < 0.5 && (end.1 - 10.0).abs() < 0.5);
    }

    #[test]
    fn path_d_relative_and_hv() {
        let cmds = decode_path_d("M10 10 h20 v-5 l-5 5 Z");
        let pts = points_from_commands(&cmds);
        assert!(pts.len() >= 4);
        assert!((pts[0].0 - 10.0).abs() < 1e-3 && (pts[0].1 - 10.0).abs() < 1e-3);
        assert!((pts[1].0 - 30.0).abs() < 1e-3);
        assert!(cmds.iter().any(|c| matches!(c, PathCommand::Close)));
    }

    #[test]
    fn tessellate_stroke_emits_rotated_quads() {
        let cmds = polyline(&[(0.0, 0.0), (40.0, 0.0), (40.0, 30.0)]);
        let quads = tessellate_path(&cmds, &PathStyle::stroke([1.0, 0.0, 0.0, 1.0], 2.0));
        assert!(
            quads.iter().any(|q| q.rotate_deg.abs() > 1.0),
            "vertical segment should rotate, quads={quads:?}"
        );
        assert!(quads.iter().any(|q| q.rotate_deg.abs() < 1.0));
        assert!(quads.len() >= 3);
    }

    #[test]
    fn tessellate_stroke_butt_is_exact_length() {
        let cmds = polyline(&[(0.0, 10.0), (40.0, 10.0)]);
        let style = PathStyle::stroke([1.0, 0.0, 0.0, 1.0], 4.0).linecap(StrokeLinecap::Butt);
        let quads = tessellate_path(&cmds, &style);
        let seg = quads
            .iter()
            .find(|q| q.rotate_deg.abs() < 1.0 && q.size.0 > 20.0)
            .expect("horizontal segment");
        assert!(
            (seg.size.0 - 40.0).abs() < 0.6,
            "butt must not extend len+w, got {}",
            seg.size.0
        );
    }

    #[test]
    fn tessellate_stroke_miter_covers_outer_corner() {
        let cmds = polyline(&[(0.0, 20.0), (40.0, 20.0), (40.0, 60.0)]);
        let style = PathStyle::stroke([1.0, 0.0, 0.0, 1.0], 4.0)
            .linecap(StrokeLinecap::Butt)
            .linejoin(StrokeLinejoin::Miter);
        let tris = tessellate_path_mesh(&cmds, &style, &[], 0.0);
        let has_miter = tris
            .iter()
            .any(|t| [t.a, t.b, t.c].iter().any(|p| p.0 >= 41.4 && p.1 <= 18.6));
        assert!(
            has_miter,
            "90° miter should reach ~ (42, 18), tris={tris:?}"
        );
    }

    #[test]
    fn tessellate_path_mesh_stroke_is_triangles() {
        let cmds = polyline(&[(10.0, 10.0), (50.0, 10.0), (50.0, 40.0)]);
        let style = PathStyle::stroke([1.0, 0.2, 0.2, 1.0], 4.0);
        let tris = tessellate_path_mesh(&cmds, &style, &[], 0.0);
        assert!(
            tris.len() >= 4,
            "stroke mesh must be triangles, got {}",
            tris.len()
        );
        let mut min_x = f32::MAX;
        let mut max_x = f32::MIN;
        let mut min_y = f32::MAX;
        let mut max_y = f32::MIN;
        for t in &tris {
            for p in [t.a, t.b, t.c] {
                min_x = min_x.min(p.0);
                max_x = max_x.max(p.0);
                min_y = min_y.min(p.1);
                max_y = max_y.max(p.1);
            }
        }
        assert!(
            min_x <= 10.0 + 1.0 && max_x >= 50.0 - 1.0 && max_y >= 40.0 - 1.0,
            "mesh bbox [{min_x},{min_y}]-[{max_x},{max_y}] must cover the polyline"
        );
        let cpu = rasterize_path_rgba(&cmds, &style, &[], 64, 48, 2);
        let opaque = cpu.chunks(4).filter(|p| p[3] > 8).count();
        assert!(
            opaque > 40,
            "CPU golden stroke should paint, opaque={opaque}"
        );
    }

    #[test]
    fn path_gpu_queues_mesh_job_not_sdf_instances() {
        if !crate::renderer::gpu2d_enabled() {
            return;
        }
        let _ = take_path_jobs();
        let el = attach_path(
            Element::new().w(80.0).h(60.0),
            &polyline(&[(4.0, 30.0), (40.0, 8.0), (76.0, 50.0)]),
            &PathStyle::stroke([0.2, 0.5, 0.9, 1.0], 3.0),
        );
        let (key, w, h) = path_gpu_from_element_attrs(&el.attrs, 80.0, 60.0)
            .expect("stroke polyline must queue PathPipeline job");
        assert!(key.starts_with("rui_path:"));
        assert!(w > 40.0 && h > 20.0);
        let jobs = take_path_jobs();
        assert!(!jobs.is_empty());
        let n = jobs[0]
            .ops
            .iter()
            .map(|op| match op {
                PathGpuOp::Mesh(m) => m.tris.len(),
                PathGpuOp::Fill(_) => 0,
            })
            .sum::<usize>();
        assert!(n >= 4, "stroke mesh must be triangles, got {n}");
    }

    #[test]
    fn path_gpu_queues_stencil_fill() {
        if !crate::renderer::gpu2d_enabled() {
            return;
        }
        let _ = take_path_jobs();
        let el = attach_path(
            Element::new().w(80.0).h(60.0),
            &pie_sector(40.0, 30.0, 24.0, 0.0, 1.8, 8),
            &PathStyle::fill([0.2, 0.4, 0.9, 1.0]),
        );
        let (key, _, _) = path_gpu_from_element_attrs(&el.attrs, 80.0, 60.0)
            .expect("solid fill must queue stencil-cover");
        assert!(key.starts_with("rui_path:"));
        let jobs = take_path_jobs();
        let fill = jobs[0].ops.iter().find_map(|op| match op {
            PathGpuOp::Fill(f) => Some(f),
            PathGpuOp::Mesh(_) => None,
        });
        let fill = fill.expect("fill op");
        assert!(!fill.even_odd);
        assert!(fill.stencil.len() >= 3);
        assert!(fill.grad.is_none());
    }

    #[test]
    fn path_gpu_evenodd_hole_is_one_fill() {
        if !crate::renderer::gpu2d_enabled() {
            return;
        }
        let _ = take_path_jobs();
        let mut cmds = filled_circle(40.0, 30.0, 22.0, 16);
        cmds.extend(filled_circle(40.0, 30.0, 10.0, 12));
        let mut style = PathStyle::fill([0.9, 0.3, 0.2, 1.0]);
        style.fill_rule = FillRule::EvenOdd;
        let el = attach_path(Element::new().w(80.0).h(60.0), &cmds, &style);
        path_gpu_from_element_attrs(&el.attrs, 80.0, 60.0).expect("evenodd hole must stay on GPU");
        let jobs = take_path_jobs();
        let fill = jobs[0]
            .ops
            .iter()
            .find_map(|op| match op {
                PathGpuOp::Fill(f) => Some(f),
                _ => None,
            })
            .expect("fill");
        assert!(fill.even_odd);
        assert!(
            fill.stencil.len() >= 20,
            "outer+inner fans, got {}",
            fill.stencil.len()
        );
    }

    #[test]
    fn path_gpu_linear_gradient_packs() {
        let mut style = PathStyle::fill([1.0, 1.0, 1.0, 1.0]);
        style.fill_gradient = Some(crate::style::Gradient::Linear(
            crate::style::LinearGradient::two_stop(
                [0.1, 0.2, 0.9, 1.0],
                [0.9, 0.2, 0.1, 1.0],
                90.0,
            ),
        ));
        let fill = build_fill_op(&filled_rect(8.0, 8.0, 60.0, 40.0), &style).expect("fill");
        let g = fill.grad.expect("packed linear");
        assert_eq!(g.kind, 1);
        assert_eq!(g.stop_n, 2);
    }

    #[test]
    fn path_gpu_conic_falls_back_to_cpu() {
        let mut style = PathStyle::fill([1.0, 1.0, 1.0, 1.0]);
        style.fill_gradient = Some(crate::style::Gradient::Conic(
            crate::style::ConicGradient::default(),
        ));
        assert!(pack_gpu_gradient(style.fill_gradient.as_ref().unwrap()).is_none());
        assert!(mesh_needs_cpu(&[(
            filled_rect(8.0, 8.0, 60.0, 40.0),
            style
        )]));
    }

    #[test]
    fn tessellate_fill_pie_has_coverage() {
        let cmds = pie_sector(50.0, 50.0, 40.0, 0.0, std::f32::consts::FRAC_PI_2, 8);
        let quads = tessellate_path(&cmds, &PathStyle::fill([0.2, 0.4, 0.9, 1.0]));
        assert!(
            quads.len() >= 4,
            "pie fill should emit wedge quads, got {}",
            quads.len()
        );
        assert!(
            quads.iter().all(|q| q.rotate_deg.abs() < 0.01),
            "fill must be axis-aligned scanlines, not rotated wedges"
        );
        let area: f32 = quads.iter().map(|q| q.size.0 * q.size.1).sum();
        assert!(area > 200.0, "pie fill area too small: {area}");
    }

    #[test]
    fn viewbox_commas_parse_like_spaces() {
        let a = parse_path_viewbox("0 0 100 80").unwrap();
        let b = parse_path_viewbox("0,0,100,80").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn fit_leaf_viewbox_follows_both_axes() {
        let mut el = Element::new();
        el.set_attr(ATTR_PATH_VIEWBOX, "0 0 100 100");
        el.set_attr(ATTR_PATH_FIT_LEAF, "1");
        let target = path_target_leaf_size(&el.attrs, [160.0, 120.0]);
        assert!(
            (target[0] - 160.0).abs() < 0.5 && (target[1] - 120.0).abs() < 0.5,
            "svg fit-leaf must follow the leaf, got {target:?}"
        );
    }

    #[test]
    fn pixel_viewbox_height_locks_target_not_stretched_leaf() {
        let el = attach_path(
            Element::new().w(200.0).h(400.0),
            &polyline(&[(0.0, 40.0), (1.0, 10.0)]),
            &PathStyle::stroke([0.2, 0.5, 0.9, 1.0], 3.0),
        );
        let mut attrs = el.attrs.clone();
        attrs.insert(ATTR_PATH_VIEWBOX.into(), "0 0 1 80".into());
        let target = path_target_leaf_size(&attrs, [200.0, 400.0]);
        assert!(
            (target[0] - 200.0).abs() < 0.5 && (target[1] - 80.0).abs() < 0.5,
            "normalized x → leaf w, pixel y → viewBox h, got {target:?}"
        );
        let (key, dw, dh) = path_raster_from_element_attrs(&attrs, 200.0, 400.0).expect("raster");
        assert!(key.starts_with("rui_path:"));
        assert!((dw - 200.0).abs() < 0.5 && (dh - 80.0).abs() < 0.5);
    }

    #[test]
    fn viewbox_scales_commands_to_leaf() {
        let el = attach_path(
            Element::new().w(100.0).h(50.0),
            &polyline(&[(0.0, 0.0), (1.0, 1.0)]),
            &PathStyle::stroke([1.0, 0.0, 0.0, 1.0], 2.0),
        );
        let mut attrs = el.attrs.clone();
        attrs.insert(ATTR_PATH_VIEWBOX.into(), "0 0 1 1".into());
        let quads = path_quads_from_element_attrs(&attrs, [0.0, 0.0], [200.0, 100.0]).unwrap();
        assert!(
            quads.iter().any(|q| q.size.0 > 150.0),
            "viewBox 0..1 should stretch to leaf 200px, quads={quads:?}"
        );
    }

    #[test]
    fn attach_path_writes_shared_d_attr() {
        let el = attach_path(
            Element::new().w(10.0).h(10.0),
            &polyline(&[(0.0, 0.0), (4.0, 5.0)]),
            &PathStyle::stroke([0.1, 0.2, 0.3, 1.0], 2.0),
        );
        let d = el.get_attr(ATTR_PATH_D).expect("d");
        assert!(d.contains('M') && d.contains('L'), "{d}");
        assert!(el.get_attr(ATTR_PATH_STROKE).is_some());
        let quads = path_quads_from_element_attrs(&el.attrs, [100.0, 50.0], [10.0, 10.0]).unwrap();
        assert!(quads.iter().all(|q| q.pos.0 >= 100.0 - 4.0));
    }

    #[test]
    fn raster_stroke_is_continuous_at_joint() {
        let cmds = polyline(&[(8.0, 40.0), (40.0, 8.0), (72.0, 36.0)]);
        let rgba = rasterize_path_rgba(
            &cmds,
            &PathStyle::stroke([1.0, 0.0, 0.0, 1.0], 5.0),
            &[],
            80,
            48,
            1,
        );
        let alpha = |x: u32, y: u32| rgba[((y * 80 + x) * 4 + 3) as usize];
        assert!(
            alpha(40, 8) > 180,
            "joint center should be covered, a={}",
            alpha(40, 8)
        );
        assert!(
            alpha(24, 24) > 120,
            "first segment mid should be covered, a={}",
            alpha(24, 24)
        );
        assert!(
            alpha(56, 22) > 80,
            "second segment should be covered, a={}",
            alpha(56, 22)
        );
    }

    #[test]
    fn stroke_long_scribble_stays_under_interactive_budget() {
        let pts: Vec<Point2> = (0..400)
            .map(|i| {
                let t = i as f32;
                (4.0 + t * 0.75, 100.0 + (t * 0.11).sin() * 70.0)
            })
            .collect();
        let cmds = polyline(&pts);
        let t0 = std::time::Instant::now();
        let rgba = rasterize_path_rgba(
            &cmds,
            &PathStyle::stroke([0.1, 0.1, 0.1, 1.0], 2.5),
            &[],
            320,
            200,
            1,
        );
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(rgba.len(), 320 * 200 * 4);
        assert!(
            ms < 40.0,
            "per-segment stroke of 400 pts on 320x200 should stay interactive, took {ms:.1}ms"
        );
        assert!(
            rgba.chunks(4).any(|p| p[3] > 40),
            "scribble should leave visible stroke coverage"
        );
    }

    #[test]
    fn raster_pie_has_antialiased_rim() {
        let cmds = pie_sector(40.0, 40.0, 30.0, 0.0, std::f32::consts::FRAC_PI_2, 24);
        let rgba = rasterize_path_rgba(
            &cmds,
            &PathStyle::fill([0.2, 0.4, 0.9, 1.0]),
            &[],
            80,
            80,
            2,
        );
        let alpha = |x: u32, y: u32| rgba[((y * 80 + x) * 4 + 3) as usize];
        assert!(alpha(55, 55) > 200, "inside wedge, a={}", alpha(55, 55));
        assert!(alpha(78, 40) < 40, "outside rim, a={}", alpha(78, 40));
        let fringe = (0..80u32)
            .flat_map(|y| (0..80u32).map(move |x| alpha(x, y)))
            .filter(|&a| a > 20 && a < 230)
            .count();
        assert!(
            fringe > 8,
            "rim should have anti-aliased fringe pixels, got {fringe}"
        );
    }

    #[test]
    fn legacy_points_attr_still_decodes() {
        let mut el = Element::new();
        el.set_attr(ATTR_PATH_POINTS, "0,0;8,0;8,6");
        el.set_attr(ATTR_PATH_KIND, "polyline");
        let cmds = commands_from_attrs(&el.attrs);
        assert_eq!(cmds.len(), 3);
        match cmds[0] {
            PathCommand::MoveTo((x, y)) => {
                assert!((x - 0.0).abs() < 1e-3 && (y - 0.0).abs() < 1e-3);
            }
            _ => panic!("expected MoveTo"),
        }
    }

    #[test]
    fn flip_rgba_vertical_swaps_first_and_last_rows() {
        let mut rgba = vec![
            1, 0, 0, 255, 2, 0, 0, 255, // row 0
            0, 3, 0, 255, 0, 4, 0, 255, // row 1
            0, 0, 5, 255, 0, 0, 6, 255, // row 2
        ];
        flip_rgba_vertical(&mut rgba, 2, 3);
        assert_eq!(&rgba[0..8], &[0, 0, 5, 255, 0, 0, 6, 255]);
        assert_eq!(&rgba[8..16], &[0, 3, 0, 255, 0, 4, 0, 255]);
        assert_eq!(&rgba[16..24], &[1, 0, 0, 255, 2, 0, 0, 255]);
    }
}
