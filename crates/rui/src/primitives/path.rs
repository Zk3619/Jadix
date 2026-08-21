//! # Path primitives（WP-3.2 / WP-3.11 共用核）
//!
//! 折线 / 多边形 / 扇区 → [`PathCommand`]；编码为 `rui_path_d`（SVG 子集 `M/L/Z`）。
//! 产品绘制：**CPU 覆盖率光栅 → RGBA 贴图**（抗锯齿填充 + 连续描边）；tessellate 四边形仅作回退。
//! 不另开 Skia/Vello。Chart / `<svg>` / `<canvas>` 禁止第二套折线格式。

use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
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

/// 路径绘制命令（直线核；曲线留给 3.11 加深）。
#[derive(Clone, Debug, PartialEq)]
pub enum PathCommand {
    MoveTo(Point2),
    LineTo(Point2),
    Close,
}

/// 描边 / 填充样式。
#[derive(Clone, Debug, PartialEq)]
pub struct PathStyle {
    pub fill: Option<[f32; 4]>,
    pub stroke: Option<[f32; 4]>,
    pub stroke_width: f32,
}

impl PathStyle {
    pub fn stroke(color: [f32; 4], width: f32) -> Self {
        Self {
            fill: None,
            stroke: Some(color),
            stroke_width: width.max(0.5),
        }
    }

    pub fn fill(color: [f32; 4]) -> Self {
        Self {
            fill: Some(color),
            stroke: None,
            stroke_width: 0.0,
        }
    }

    pub fn fill_stroke(fill: [f32; 4], stroke: [f32; 4], width: f32) -> Self {
        Self {
            fill: Some(fill),
            stroke: Some(stroke),
            stroke_width: width.max(0.5),
        }
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

/// SVG 子集 `d`：`M x,y L x,y Z`。
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

/// 解析 [`encode_path_d`] 或宽松空白分隔的 `M/L/Z`。
pub fn decode_path_d(d: &str) -> Vec<PathCommand> {
    let mut out = Vec::new();
    let mut chars = d.chars().peekable();
    let mut cur_cmd: Option<char> = None;
    while let Some(c) = chars.next() {
        if c.is_whitespace() || c == ',' {
            continue;
        }
        if c.is_ascii_alphabetic() {
            let u = c.to_ascii_uppercase();
            if u == 'Z' {
                out.push(PathCommand::Close);
                cur_cmd = None;
            } else {
                cur_cmd = Some(u);
            }
            continue;
        }
        // number: put back and parse pair for M/L
        let mut num = String::new();
        if c == '-' || c == '+' {
            num.push(c);
        } else {
            num.push(c);
        }
        while let Some(&n) = chars.peek() {
            if n.is_ascii_digit() || n == '.' || n == 'e' || n == 'E' {
                num.push(n);
                chars.next();
            } else {
                break;
            }
        }
        let x: f32 = num.parse().unwrap_or(0.0);
        while let Some(&n) = chars.peek() {
            if n.is_whitespace() || n == ',' {
                chars.next();
            } else {
                break;
            }
        }
        let mut num2 = String::new();
        if let Some(&n) = chars.peek() {
            if n == '-' || n == '+' || n.is_ascii_digit() || n == '.' {
                num2.push(chars.next().unwrap());
                while let Some(&m) = chars.peek() {
                    if m.is_ascii_digit() || m == '.' || m == 'e' || m == 'E' {
                        num2.push(m);
                        chars.next();
                    } else {
                        break;
                    }
                }
            }
        }
        let y: f32 = num2.parse().unwrap_or(0.0);
        match cur_cmd {
            Some('M') => {
                out.push(PathCommand::MoveTo((x, y)));
                cur_cmd = Some('L'); // implicit LineTo after first M pair (SVG)
            }
            Some('L') | None => out.push(PathCommand::LineTo((x, y))),
            _ => {}
        }
    }
    out
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
    if fill.is_none() && stroke.is_none() {
        return PathStyle::default();
    }
    PathStyle {
        fill,
        stroke,
        stroke_width,
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
    } else {
        el.attrs.remove(ATTR_PATH_STROKE);
        el.attrs.remove(ATTR_PATH_STROKE_WIDTH);
    }
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
            if style.fill.is_some() && has_open && *closed {
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
const MAX_RASTER_CACHE: usize = 48;
const MAX_SCENE_STORE: usize = 64;

#[derive(Clone)]
struct PathScene {
    layers: Vec<(Vec<PathCommand>, PathStyle)>,
    markers: Vec<Point2>,
    marker_r: f32,
    vb: [f32; 4],
}

thread_local! {
    static RASTER_CACHE: RefCell<HashMap<u64, ()>> = RefCell::new(HashMap::new());
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

    if let Some(fill) = style.fill {
        for (pts, closed) in &paths {
            if !*closed || pts.len() < 3 || fill[3] <= 0.0 {
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
                if style.fill.is_some() && has_open && *closed {
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

                // Stamp round joins / caps at every vertex. Segment capsules are usually
                // enough, but sharp turns plus subpixel sampling can still leave a tiny
                // seam where adjacent segments meet. Taking the max with a vertex disk
                // preserves the non-darkening behavior while guaranteeing continuity.
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
    let mut miss = false;
    RASTER_CACHE.with(|c| {
        let mut g = c.borrow_mut();
        if g.len() >= MAX_RASTER_CACHE {
            g.clear();
        }
        if g.insert(key, ()).is_none() {
            miss = true;
        }
    });
    if miss {
        let rgba = rasterize_layers_rgba(layers, markers, marker_radius, width, height, ssaa);
        // TexturedQuadPipeline: rect top → uv.y=0, matching this buffer's y-down row order.
        crate::queue_image_rgba8(tex_id.clone(), width, height, rgba);
    }
    Some(tex_id)
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
