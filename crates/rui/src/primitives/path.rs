//! # Path primitives（WP-3.2 / WP-3.11 共用核）
//!
//! 折线 / 多边形 / 扇区 → [`PathCommand`]；编码为 `rui_path_d`。
//! [`decode_path_d`] 接受常用 HTML/SVG `d`（含 C/Q/A 等），解析后展平为 M/L/Z。
//! 产品绘制：**CPU 覆盖率光栅 → RGBA 贴图**（抗锯齿填充 + 连续描边）；tessellate 四边形仅作回退。
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

/// 路径绘制命令（直线核；曲线在 [`decode_path_d`] 中展平）。
#[derive(Clone, Debug, PartialEq)]
pub enum PathCommand {
    MoveTo(Point2),
    LineTo(Point2),
    Close,
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
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathQuad {
    /// 未旋转矩形左上角（与 `InstanceInput.rect_pos` 同语义）。
    pub pos: Point2,
    pub size: Point2,
    pub rotate_deg: f32,
    pub color: [f32; 4],
    pub radii: [f32; 4],
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

/// 饼图扇区：圆心 + 半径 + 起止角（弧度，0 = 正右；屏幕 y 向下时增大角顺时针）。
pub fn pie_sector(
    cx: f32,
    cy: f32,
    r: f32,
    start: f32,
    end: f32,
    segments: usize,
) -> Vec<PathCommand> {
    let n = segments.max(3);
    let mut pts = Vec::with_capacity(n + 2);
    pts.push((cx, cy));
    for i in 0..=n {
        let t = i as f32 / n as f32;
        let a = start + (end - start) * t;
        pts.push((cx + r * a.cos(), cy + r * a.sin()));
    }
    polygon(&pts)
}

/// 实心圆：圆周多边形（不含圆心顶点），fill 为盘、stroke 为圆周。
/// 点 mark 必须用这个而不是 [`pie_sector`] 满周——后者 stroke 会画出半径。
pub fn filled_circle(cx: f32, cy: f32, r: f32, segments: usize) -> Vec<PathCommand> {
    let n = segments.max(8);
    let r = r.max(0.5);
    let mut pts = Vec::with_capacity(n);
    for i in 0..n {
        let a = std::f32::consts::TAU * (i as f32) / (n as f32);
        pts.push((cx + r * a.cos(), cy + r * a.sin()));
    }
    polygon(&pts)
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
    const SEGS: usize = 6;
    let mut pts = Vec::with_capacity(SEGS * 4 + 4);
    let mut arc = |cx: f32, cy: f32, r: f32, a0: f32, span: f32| {
        if r < 0.5 {
            pts.push((cx, cy));
            return;
        }
        for i in 0..=SEGS {
            let t = i as f32 / SEGS as f32;
            let a = a0 + span * t;
            pts.push((cx + r * a.cos(), cy + r * a.sin()));
        }
    };
    // clockwise from top-left
    arc(
        x + tl,
        y + tl,
        tl,
        std::f32::consts::PI,
        std::f32::consts::FRAC_PI_2,
    );
    arc(
        x + w - tr,
        y + tr,
        tr,
        -std::f32::consts::FRAC_PI_2,
        std::f32::consts::FRAC_PI_2,
    );
    arc(x + w - br, y + h - br, br, 0.0, std::f32::consts::FRAC_PI_2);
    arc(
        x + bl,
        y + h - bl,
        bl,
        std::f32::consts::FRAC_PI_2,
        std::f32::consts::FRAC_PI_2,
    );
    polygon(&pts)
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
    let n = segments.max(3);
    let mut pts = Vec::with_capacity((n + 1) * 2);
    for i in 0..=n {
        let t = i as f32 / n as f32;
        let a = start + (end - start) * t;
        pts.push((cx + outer * a.cos(), cy + outer * a.sin()));
    }
    for i in (0..=n).rev() {
        let t = i as f32 / n as f32;
        let a = start + (end - start) * t;
        pts.push((cx + inner * a.cos(), cy + inner * a.sin()));
    }
    polygon(&pts)
}

/// 将点列编码为旧 attr（`x,y;x,y;…`）。
pub fn encode_points(points: &[Point2]) -> String {
    points
        .iter()
        .map(|(x, y)| format!("{x:.3},{y:.3}"))
        .collect::<Vec<_>>()
        .join(";")
}

/// SVG 子集 `d`：`M x,y L x,y Z`（曲线已展平，编码仍为折线）。
pub fn encode_path_d(cmds: &[PathCommand]) -> String {
    let mut parts = Vec::with_capacity(cmds.len());
    for cmd in cmds {
        match *cmd {
            PathCommand::MoveTo((x, y)) => parts.push(format!("M{x:.3},{y:.3}")),
            PathCommand::LineTo((x, y)) => parts.push(format!("L{x:.3},{y:.3}")),
            PathCommand::Close => parts.push("Z".into()),
        }
    }
    parts.join(" ")
}

/// 解析 HTML/SVG path `d`（绝对/相对 M/L/H/V/C/S/Q/T/A/Z），曲线展平为折线。
pub fn decode_path_d(d: &str) -> Vec<PathCommand> {
    let parser = PathDParser::new(d);
    parser.parse()
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
                    flatten_cubic((cx, cy), (x1, y1), (x2, y2), (x, y), &mut out);
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
                    flatten_cubic((cx, cy), (x1, y1), (x2, y2), (x, y), &mut out);
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
                    flatten_quad((cx, cy), (x1, y1), (x, y), &mut out);
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
                    flatten_quad((cx, cy), (x1, y1), (x, y), &mut out);
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
                    flatten_arc(
                        (cx, cy),
                        rx.abs(),
                        ry.abs(),
                        x_axis_rot,
                        large_arc != 0.0,
                        sweep != 0.0,
                        (x, y),
                        &mut out,
                    );
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

fn flatten_cubic(p0: Point2, p1: Point2, p2: Point2, p3: Point2, out: &mut Vec<PathCommand>) {
    let chord = (p0.0 - p3.0).hypot(p0.1 - p3.1);
    let ctrl_len = (p0.0 - p1.0)
        .hypot(p0.1 - p1.1)
        .max((p2.0 - p3.0).hypot(p2.1 - p3.1));
    let by_len = ((chord + ctrl_len) / 0.35).ceil() as usize;
    let segments = by_len.clamp(12, 48);
    for i in 1..=segments {
        let t = i as f32 / segments as f32;
        let u = 1.0 - t;
        let tt = t * t;
        let uu = u * u;
        let uuu = uu * u;
        let ttt = tt * t;
        let x = uuu * p0.0 + 3.0 * uu * t * p1.0 + 3.0 * u * tt * p2.0 + ttt * p3.0;
        let y = uuu * p0.1 + 3.0 * uu * t * p1.1 + 3.0 * u * tt * p2.1 + ttt * p3.1;
        out.push(PathCommand::LineTo((x, y)));
    }
}

fn flatten_quad(p0: Point2, p1: Point2, p2: Point2, out: &mut Vec<PathCommand>) {
    let chord = (p0.0 - p2.0).hypot(p0.1 - p2.1);
    let by_len = (chord / 0.35).ceil() as usize;
    let segments = by_len.clamp(10, 32);
    for i in 1..=segments {
        let t = i as f32 / segments as f32;
        let u = 1.0 - t;
        let x = u * u * p0.0 + 2.0 * u * t * p1.0 + t * t * p2.0;
        let y = u * u * p0.1 + 2.0 * u * t * p1.1 + t * t * p2.1;
        out.push(PathCommand::LineTo((x, y)));
    }
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

    // Lucide spinners etc. use long circular arcs; ~45°/segment reads as a visible polygon.
    const MAX_ANGLE_STEP: f32 = std::f32::consts::PI / 30.0; // ~6°
    let avg_r = (rx + ry) * 0.5;
    let arc_len = avg_r * dtheta.abs();
    let by_angle = (dtheta.abs() / MAX_ANGLE_STEP).ceil() as usize;
    let by_length = (arc_len / 0.35).ceil() as usize;
    let segments = by_angle.max(by_length).clamp(8, 128);
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
    for cmd in cmds {
        match *cmd {
            PathCommand::MoveTo(p) | PathCommand::LineTo(p) => pts.push(p),
            PathCommand::Close => {}
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

/// 子路径：每次 `MoveTo` 起一段；`Close` 闭合。
fn subpaths(cmds: &[PathCommand]) -> Vec<(Vec<Point2>, bool)> {
    let mut out = Vec::new();
    let mut cur: Vec<Point2> = Vec::new();
    let mut start: Option<Point2> = None;
    for cmd in cmds {
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

fn tessellate_stroke_points(pts: &[Point2], width: f32, color: [f32; 4], out: &mut Vec<PathQuad>) {
    let w = width.max(0.75);
    let hw = w * 0.5;
    let round = [hw, hw, hw, hw];
    for win in pts.windows(2) {
        let a = win[0];
        let b = win[1];
        let dx = b.0 - a.0;
        let dy = b.1 - a.1;
        let len = dx.hypot(dy);
        if len < 1e-4 {
            continue;
        }
        let angle = dy.atan2(dx).to_degrees();
        let mid = ((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5);
        // 两端各伸出半线宽，避免 SDF 接头漏白。
        push_oriented_quad(mid, len + w, w, angle, color, [0.0; 4], out);
    }
    for &p in pts {
        push_oriented_quad(p, w, w, 0.0, color, round, out);
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
            tessellate_stroke_points(pts, w, stroke, &mut out);
        }
    }
    out
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
        .map(|c| match *c {
            PathCommand::MoveTo(p) => PathCommand::MoveTo(map(p)),
            PathCommand::LineTo(p) => PathCommand::LineTo(map(p)),
            PathCommand::Close => PathCommand::Close,
        })
        .collect()
}

/// 闭合正多边形（点 mark 的实心圆）：中心按各轴缩放，半径按 `min(sx,sy)`，避免被压成椭圆。
fn detect_regular_circle(cmds: &[PathCommand]) -> Option<(f32, f32, f32, usize)> {
    let closed = cmds.iter().any(|c| matches!(c, PathCommand::Close));
    if !closed {
        return None;
    }
    let pts = points_from_commands(cmds);
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
    let round_caps = matches!(
        style.stroke_linecap,
        StrokeLinecap::Round | StrokeLinecap::Square
    );
    for (pts, closed) in subpaths(cmds) {
        if pts.len() < 2 {
            if pts.len() == 1 && round_caps {
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
        if !closed && round_caps {
            let first = pts[0];
            let last = pts[pts.len() - 1];
            if (p.0 - first.0).hypot(p.1 - first.1) <= hw
                || (p.0 - last.0).hypot(p.1 - last.1) <= hw
            {
                return true;
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
    let target = path_target_leaf_size(attrs, [leaf_w, leaf_h]);
    let w = target[0].round() as i32;
    let h = target[1].round() as i32;
    if w < 2 || h < 2 {
        return None;
    }
    let w = w as u32;
    let h = h as u32;

    if let Some(scene_hex) = attrs.get(ATTR_PATH_SCENE) {
        if let Ok(id) = u64::from_str_radix(scene_hex.trim(), 16) {
            let scene = SCENE_STORE.with(|s| s.borrow().get(&id).cloned());
            if let Some(scene) = scene {
                let scaled: Vec<(Vec<PathCommand>, PathStyle)> = scene
                    .layers
                    .iter()
                    .map(|(cmds, st)| {
                        (
                            scale_commands_preserve_circles(cmds, scene.vb, target),
                            st.clone(),
                        )
                    })
                    .collect();
                if scaled.iter().all(|(c, _)| c.is_empty()) && scene.markers.is_empty() {
                    return None;
                }
                let markers = scale_points(&scene.markers, scene.vb, target);
                let sx = target[0] / scene.vb[2].max(1e-6);
                let sy = target[1] / scene.vb[3].max(1e-6);
                let mr = scene.marker_r * sx.min(sy).max(0.25);
                let tex = ensure_raster_texture(&scaled, &markers, mr, w, h, 2)?;
                return Some((tex, target[0], target[1]));
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
        scale_commands_preserve_circles(&raw, vb, target)
    } else {
        raw
    };
    let style = style_from_attrs(attrs);
    let mut markers = attrs
        .get(ATTR_PATH_MARKERS)
        .map(|s| decode_points(s))
        .unwrap_or_default();
    if let Some(vb) = vb {
        markers = scale_points(&markers, vb, target);
    }
    let marker_r = attrs
        .get("rui_path_marker_r")
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| style.stroke_width.max(2.5) * 0.9);
    let sx = if let Some(vb) = vb {
        target[0] / vb[2].max(1e-6)
    } else {
        1.0
    };
    let marker_r = marker_r * sx.max(0.25);
    let tex = ensure_raster_texture(&[(cmds, style)], &markers, marker_r, w, h, 2)?;
    Some((tex, target[0], target[1]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polyline_and_pie_encode() {
        let cmds = polyline(&[(0.0, 0.0), (1.0, 2.0)]);
        assert_eq!(cmds.len(), 2);
        let pie = pie_sector(0.0, 0.0, 10.0, 0.0, std::f32::consts::FRAC_PI_2, 4);
        assert!(pie.len() >= 4);
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
        let pts = points_from_commands(&c);
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
        let pts = points_from_commands(&scaled);
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
        assert!(
            pts.len() >= 32,
            "loader-circle arc should flatten to many segments, got {}",
            pts.len()
        );
    }

    #[test]
    fn path_d_flattens_cubic_and_arc() {
        let cubic = decode_path_d("M0,0 C10,0 10,10 0,10");
        assert!(
            cubic.len() > 4,
            "cubic should flatten to multiple LineTo, got {}",
            cubic.len()
        );
        assert!(matches!(cubic[0], PathCommand::MoveTo((0.0, 0.0))));
        let last = points_from_commands(&cubic).last().copied().unwrap();
        assert!((last.0 - 0.0).abs() < 0.2 && (last.1 - 10.0).abs() < 0.2);

        let arc = decode_path_d("M10,0 A10,10 0 0 1 0,10");
        assert!(arc.len() >= 3, "arc should flatten, got {}", arc.len());
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
