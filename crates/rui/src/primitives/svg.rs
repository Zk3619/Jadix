//! # 公开 `<svg>` 叶（WP-3.11）
//!
//! HTML `<svg viewBox>` 心智，但是 **RUI Element 叶内的矢量场景图**，不是 DOM。
//! 子形状（`path` / `circle` / `rect` / `line` / `polyline` / `g`）**不**各占 taffy 节点。
//! 绘制走与 Chart 同一 [`PathCommand`] 核（CPU 覆盖率光栅贴图）。emoji-svg / resvg **不算**本包。
//!
//! 双书写：[`svg`] Builder；`ui!` `<svg>` / `<circle>` / `<path>`…（见 `rui_macros`）。

use crate::core::tree::{Element, IntoCssDim, IntoElementList};
use crate::primitives::path::{
    decode_path_d, encode_path_d, ensure_raster_texture, filled_circle, filled_rect,
    polyline as path_polyline, register_path_scene, PathCommand, PathStyle, Point2, ATTR_PATH_D,
    ATTR_PATH_FIT_LEAF, ATTR_PATH_SCENE, ATTR_PATH_VIEWBOX,
};
use crate::style::parse_color;

/// 子形状 Element 标记（场景图编译时消费，不进布局）。
pub const ATTR_SVG_KIND: &str = "rui_svg_kind";

/// SVG 用户坐标 / 字面量 → f32（`cx={50}` / `cx="50"` / `50.0`）。
pub trait IntoSvgNum {
    fn into_svg_num(self) -> f32;
}

impl IntoSvgNum for f32 {
    fn into_svg_num(self) -> f32 {
        self
    }
}
impl IntoSvgNum for f64 {
    fn into_svg_num(self) -> f32 {
        self as f32
    }
}
impl IntoSvgNum for i32 {
    fn into_svg_num(self) -> f32 {
        self as f32
    }
}
impl IntoSvgNum for i64 {
    fn into_svg_num(self) -> f32 {
        self as f32
    }
}
impl IntoSvgNum for u32 {
    fn into_svg_num(self) -> f32 {
        self as f32
    }
}
impl IntoSvgNum for &str {
    fn into_svg_num(self) -> f32 {
        let t = self.trim();
        t.parse()
            .or_else(|_| t.trim_end_matches("px").trim().parse())
            .unwrap_or(0.0)
    }
}
impl IntoSvgNum for String {
    fn into_svg_num(self) -> f32 {
        self.as_str().into_svg_num()
    }
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

    /// `self` 在左：先 `other`，再 `self`（父变换包住子变换）。
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

fn parse_transform(raw: &str) -> Affine {
    let s = raw.trim();
    if s.is_empty() {
        return Affine::identity();
    }
    let mut acc = Affine::identity();
    let mut rest = s;
    while !rest.is_empty() {
        rest = rest.trim_start();
        if rest.is_empty() {
            break;
        }
        let name_end = rest.find('(').unwrap_or(rest.len());
        let name = rest[..name_end].trim().to_ascii_lowercase();
        let after = rest[name_end..].trim_start();
        if !after.starts_with('(') {
            break;
        }
        let inner_end = after.find(')').unwrap_or(after.len());
        let args_raw = after[1..inner_end].replace(',', " ");
        let args: Vec<f32> = args_raw
            .split_whitespace()
            .filter_map(|t| t.parse().ok())
            .collect();
        let xf = match name.as_str() {
            "translate" => Affine::translate(
                args.first().copied().unwrap_or(0.0),
                args.get(1).copied().unwrap_or(0.0),
            ),
            "scale" => {
                let sx = args.first().copied().unwrap_or(1.0);
                Affine::scale(sx, args.get(1).copied().unwrap_or(sx))
            }
            "rotate" => {
                let deg = args.first().copied().unwrap_or(0.0);
                if args.len() >= 3 {
                    let cx = args[1];
                    let cy = args[2];
                    Affine::translate(cx, cy)
                        .mul(Affine::rotate_deg(deg))
                        .mul(Affine::translate(-cx, -cy))
                } else {
                    Affine::rotate_deg(deg)
                }
            }
            "matrix" if args.len() >= 6 => Affine {
                a: args[0],
                b: args[1],
                c: args[2],
                d: args[3],
                e: args[4],
                f: args[5],
            },
            _ => Affine::identity(),
        };
        acc = acc.mul(xf);
        rest = if inner_end < after.len() {
            after[inner_end + 1..].trim_start()
        } else {
            ""
        };
    }
    acc
}

fn parse_paint(raw: Option<&str>) -> Option<[f32; 4]> {
    let s = raw.map(str::trim).unwrap_or("");
    if s.is_empty() || s.eq_ignore_ascii_case("none") {
        return None;
    }
    Some(parse_color(s))
}

fn parse_svg_points(s: &str) -> Vec<Point2> {
    let t = s.trim();
    if t.is_empty() {
        return Vec::new();
    }
    if t.contains(';') {
        return t
            .split(';')
            .filter_map(|pair| {
                let mut it = pair.split(',');
                let x = it.next()?.trim().parse().ok()?;
                let y = it.next()?.trim().parse().ok()?;
                Some((x, y))
            })
            .collect();
    }
    let nums: Vec<f32> = t
        .replace(',', " ")
        .split_whitespace()
        .filter_map(|n| n.parse().ok())
        .collect();
    nums.chunks(2)
        .filter_map(|c| (c.len() == 2).then_some((c[0], c[1])))
        .collect()
}

fn attr_f32(el: &Element, key: &str) -> f32 {
    el.get_attr(key)
        .map(|s| s.as_str().into_svg_num())
        .unwrap_or(0.0)
}

fn mul_alpha(c: [f32; 4], opacity: f32) -> [f32; 4] {
    [c[0], c[1], c[2], (c[3] * opacity).clamp(0.0, 1.0)]
}

fn resolve_style(
    fill_raw: Option<&str>,
    stroke_raw: Option<&str>,
    stroke_width: f32,
    opacity: f32,
    default_fill: bool,
    default_stroke: bool,
    xf: Affine,
) -> Option<PathStyle> {
    let fill = parse_paint(fill_raw).or_else(|| {
        if default_fill && fill_raw.is_none() {
            Some(parse_color("foreground"))
        } else {
            None
        }
    });
    let stroke = parse_paint(stroke_raw).or_else(|| {
        if default_stroke && stroke_raw.is_none() {
            Some(parse_color("foreground"))
        } else {
            None
        }
    });
    if fill.is_none() && stroke.is_none() {
        return None;
    }
    let sw = (if stroke.is_some() {
        if stroke_width > 0.0 {
            stroke_width
        } else {
            1.0
        }
    } else {
        0.0
    }) * xf.scale_avg();
    Some(PathStyle {
        fill: fill.map(|c| mul_alpha(c, opacity)),
        stroke: stroke.map(|c| mul_alpha(c, opacity)),
        stroke_width: sw,
    })
}

#[derive(Clone)]
enum SvgNode {
    Circle {
        cx: f32,
        cy: f32,
        r: f32,
        fill: Option<String>,
        stroke: Option<String>,
        stroke_width: f32,
        opacity: f32,
        transform: String,
    },
    Path {
        d: String,
        fill: Option<String>,
        stroke: Option<String>,
        stroke_width: f32,
        opacity: f32,
        transform: String,
    },
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        fill: Option<String>,
        stroke: Option<String>,
        stroke_width: f32,
        opacity: f32,
        transform: String,
    },
    Line {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        stroke: Option<String>,
        stroke_width: f32,
        opacity: f32,
        transform: String,
    },
    Polyline {
        points: Vec<Point2>,
        fill: Option<String>,
        stroke: Option<String>,
        stroke_width: f32,
        opacity: f32,
        transform: String,
    },
    Group {
        transform: String,
        opacity: f32,
        children: Vec<SvgNode>,
    },
}

fn opt_attr(el: &Element, key: &str) -> Option<String> {
    el.get_attr(key).filter(|s| !s.trim().is_empty()).cloned()
}

fn node_from_element(el: &Element) -> Option<SvgNode> {
    let kind = el.get_attr(ATTR_SVG_KIND)?.as_str();
    let opacity = el
        .get_attr("opacity")
        .map(|s| s.as_str().into_svg_num())
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    let transform = el.get_attr("transform").cloned().unwrap_or_default();
    let fill = opt_attr(el, "fill");
    let stroke = opt_attr(el, "stroke");
    let stroke_width = el
        .get_attr("stroke-width")
        .or_else(|| el.get_attr("stroke_width"))
        .map(|s| s.as_str().into_svg_num())
        .unwrap_or(0.0);
    match kind {
        "circle" => Some(SvgNode::Circle {
            cx: attr_f32(el, "cx"),
            cy: attr_f32(el, "cy"),
            r: attr_f32(el, "r").max(0.0),
            fill,
            stroke,
            stroke_width,
            opacity,
            transform,
        }),
        "path" => Some(SvgNode::Path {
            d: el.get_attr("d").cloned().unwrap_or_default(),
            fill,
            stroke,
            stroke_width,
            opacity,
            transform,
        }),
        "rect" => Some(SvgNode::Rect {
            x: attr_f32(el, "x"),
            y: attr_f32(el, "y"),
            w: el
                .get_attr("width")
                .map(|s| s.as_str().into_svg_num())
                .unwrap_or(0.0),
            h: el
                .get_attr("height")
                .map(|s| s.as_str().into_svg_num())
                .unwrap_or(0.0),
            fill,
            stroke,
            stroke_width,
            opacity,
            transform,
        }),
        "line" => Some(SvgNode::Line {
            x1: attr_f32(el, "x1"),
            y1: attr_f32(el, "y1"),
            x2: attr_f32(el, "x2"),
            y2: attr_f32(el, "y2"),
            stroke,
            stroke_width,
            opacity,
            transform,
        }),
        "polyline" => Some(SvgNode::Polyline {
            points: el
                .get_attr("points")
                .map(|s| parse_svg_points(s))
                .unwrap_or_default(),
            fill,
            stroke,
            stroke_width,
            opacity,
            transform,
        }),
        "g" => Some(SvgNode::Group {
            transform,
            opacity,
            children: el.children.iter().filter_map(node_from_element).collect(),
        }),
        _ => None,
    }
}

fn compile_node(
    node: &SvgNode,
    parent: Affine,
    parent_opacity: f32,
    out: &mut Vec<(Vec<PathCommand>, PathStyle)>,
) {
    match node {
        SvgNode::Group {
            transform,
            opacity,
            children,
        } => {
            let xf = parent.mul(parse_transform(transform));
            let o = (parent_opacity * *opacity).clamp(0.0, 1.0);
            for c in children {
                compile_node(c, xf, o, out);
            }
        }
        SvgNode::Circle {
            cx,
            cy,
            r,
            fill,
            stroke,
            stroke_width,
            opacity,
            transform,
        } => {
            if *r <= 0.0 {
                return;
            }
            let xf = parent.mul(parse_transform(transform));
            let o = (parent_opacity * *opacity).clamp(0.0, 1.0);
            let Some(style) = resolve_style(
                fill.as_deref(),
                stroke.as_deref(),
                *stroke_width,
                o,
                true,
                false,
                xf,
            ) else {
                return;
            };
            let cmds = apply_affine(&filled_circle(*cx, *cy, *r, 32), xf);
            out.push((cmds, style));
        }
        SvgNode::Path {
            d,
            fill,
            stroke,
            stroke_width,
            opacity,
            transform,
        } => {
            let raw = decode_path_d(d);
            if raw.is_empty() {
                return;
            }
            let xf = parent.mul(parse_transform(transform));
            let o = (parent_opacity * *opacity).clamp(0.0, 1.0);
            let Some(style) = resolve_style(
                fill.as_deref(),
                stroke.as_deref(),
                *stroke_width,
                o,
                true,
                false,
                xf,
            ) else {
                return;
            };
            out.push((apply_affine(&raw, xf), style));
        }
        SvgNode::Rect {
            x,
            y,
            w,
            h,
            fill,
            stroke,
            stroke_width,
            opacity,
            transform,
        } => {
            if *w <= 0.0 || *h <= 0.0 {
                return;
            }
            let xf = parent.mul(parse_transform(transform));
            let o = (parent_opacity * *opacity).clamp(0.0, 1.0);
            let Some(style) = resolve_style(
                fill.as_deref(),
                stroke.as_deref(),
                *stroke_width,
                o,
                true,
                false,
                xf,
            ) else {
                return;
            };
            out.push((apply_affine(&filled_rect(*x, *y, *w, *h), xf), style));
        }
        SvgNode::Line {
            x1,
            y1,
            x2,
            y2,
            stroke,
            stroke_width,
            opacity,
            transform,
        } => {
            let xf = parent.mul(parse_transform(transform));
            let o = (parent_opacity * *opacity).clamp(0.0, 1.0);
            let Some(style) = resolve_style(
                Some("none"),
                stroke.as_deref(),
                *stroke_width,
                o,
                false,
                true,
                xf,
            ) else {
                return;
            };
            out.push((
                apply_affine(&path_polyline(&[(*x1, *y1), (*x2, *y2)]), xf),
                style,
            ));
        }
        SvgNode::Polyline {
            points,
            fill,
            stroke,
            stroke_width,
            opacity,
            transform,
        } => {
            if points.len() < 2 {
                return;
            }
            let xf = parent.mul(parse_transform(transform));
            let o = (parent_opacity * *opacity).clamp(0.0, 1.0);
            let Some(style) = resolve_style(
                fill.as_deref(),
                stroke.as_deref(),
                *stroke_width,
                o,
                false,
                true,
                xf,
            ) else {
                return;
            };
            out.push((apply_affine(&path_polyline(points), xf), style));
        }
    }
}

fn parse_view_box(raw: &str) -> Option<[f32; 4]> {
    crate::primitives::path::parse_path_viewbox(raw)
}

fn compile_layers(
    nodes: &[SvgNode],
    view_box: Option<&str>,
) -> (Vec<(Vec<PathCommand>, PathStyle)>, [f32; 4]) {
    let mut layers = Vec::new();
    for n in nodes {
        compile_node(n, Affine::identity(), 1.0, &mut layers);
    }
    let vb = view_box
        .and_then(parse_view_box)
        .unwrap_or([0.0, 0.0, 100.0, 100.0]);
    if (vb[0].abs() > 1e-6) || (vb[1].abs() > 1e-6) {
        let origin = Affine::translate(-vb[0], -vb[1]);
        for (cmds, _) in &mut layers {
            *cmds = apply_affine(cmds, origin);
        }
    }
    (layers, [0.0, 0.0, vb[2].max(1.0), vb[3].max(1.0)])
}

fn attach_svg_scene(
    mut el: Element,
    layers: &[(Vec<PathCommand>, PathStyle)],
    vb: [f32; 4],
) -> Element {
    el.children.clear();
    el.set_attr(ATTR_PATH_FIT_LEAF, "1");
    el.set_attr(
        ATTR_PATH_VIEWBOX,
        format!("{} {} {} {}", vb[0], vb[1], vb[2], vb[3]),
    );
    if layers.is_empty() {
        return el;
    }
    let id = register_path_scene(layers.to_vec(), Vec::new(), 0.0, vb[2], vb[3]);
    el.set_attr(ATTR_PATH_SCENE, format!("{id:016x}"));
    if let Some((cmds, _)) = layers.first() {
        if !cmds.is_empty() {
            el.set_attr(ATTR_PATH_D, encode_path_d(cmds));
        }
    }
    let dim_w = vb[2].round().max(16.0) as u32;
    let dim_h = vb[3].round().max(16.0) as u32;
    if let Some(tex) = ensure_raster_texture(layers, &[], 0.0, dim_w, dim_h, 2) {
        el = el.image_texture(tex);
    }
    el
}

/// 公开 Svg 根。
pub struct SvgBuilder {
    el: Element,
    view_box: Option<String>,
    has_w: bool,
    has_h: bool,
}

pub fn svg() -> SvgBuilder {
    SvgBuilder {
        el: Element::new(),
        view_box: None,
        has_w: false,
        has_h: false,
    }
}

impl SvgBuilder {
    pub fn view_box(mut self, vb: impl AsRef<str>) -> Self {
        self.view_box = Some(vb.as_ref().replace(',', " "));
        self
    }

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

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.el = self.el.child(child);
        self
    }

    pub fn children(mut self, children: impl IntoIterator<Item = Element>) -> Self {
        for c in children {
            self = self.child(c);
        }
        self
    }

    pub fn opacity(mut self, o: impl IntoSvgNum) -> Self {
        self.el = self.el.opacity(o.into_svg_num());
        self
    }
}

impl From<SvgBuilder> for Element {
    fn from(builder: SvgBuilder) -> Self {
        let nodes: Vec<SvgNode> = builder
            .el
            .children
            .iter()
            .filter_map(node_from_element)
            .collect();
        let (layers, vb) = compile_layers(&nodes, builder.view_box.as_deref());
        let mut el = builder.el;
        if !builder.has_w {
            el = el.w(100.0);
        }
        if !builder.has_h {
            el = el.h(100.0);
        }
        attach_svg_scene(el, &layers, vb)
    }
}

impl IntoElementList for SvgBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

/// 子形状 Builder（`circle` / `path` / `rect` / `line` / `polyline` / `g`）。
pub struct SvgShapeBuilder {
    kind: &'static str,
    attrs: Vec<(&'static str, String)>,
    children: Vec<Element>,
}

pub type CircleBuilder = SvgShapeBuilder;
pub type PathBuilder = SvgShapeBuilder;
pub type RectBuilder = SvgShapeBuilder;
pub type LineBuilder = SvgShapeBuilder;
pub type PolylineBuilder = SvgShapeBuilder;
pub type GroupBuilder = SvgShapeBuilder;

fn shape(kind: &'static str) -> SvgShapeBuilder {
    SvgShapeBuilder {
        kind,
        attrs: Vec::new(),
        children: Vec::new(),
    }
}

pub fn circle() -> CircleBuilder {
    shape("circle")
}
pub fn path() -> PathBuilder {
    shape("path")
}
pub fn rect() -> RectBuilder {
    shape("rect")
}
pub fn line() -> LineBuilder {
    shape("line")
}
pub fn polyline() -> PolylineBuilder {
    shape("polyline")
}
pub fn g() -> GroupBuilder {
    shape("g")
}

impl SvgShapeBuilder {
    fn set(mut self, key: &'static str, value: impl Into<String>) -> Self {
        let v = value.into();
        if let Some((_, slot)) = self.attrs.iter_mut().find(|(k, _)| *k == key) {
            *slot = v;
        } else {
            self.attrs.push((key, v));
        }
        self
    }

    pub fn cx(self, v: impl IntoSvgNum) -> Self {
        self.set("cx", v.into_svg_num().to_string())
    }
    pub fn cy(self, v: impl IntoSvgNum) -> Self {
        self.set("cy", v.into_svg_num().to_string())
    }
    pub fn r(self, v: impl IntoSvgNum) -> Self {
        self.set("r", v.into_svg_num().to_string())
    }
    pub fn d(self, d: impl Into<String>) -> Self {
        self.set("d", d)
    }
    pub fn x(self, v: impl IntoSvgNum) -> Self {
        self.set("x", v.into_svg_num().to_string())
    }
    pub fn y(self, v: impl IntoSvgNum) -> Self {
        self.set("y", v.into_svg_num().to_string())
    }
    pub fn w(self, v: impl IntoSvgNum) -> Self {
        self.set("width", v.into_svg_num().to_string())
    }
    pub fn h(self, v: impl IntoSvgNum) -> Self {
        self.set("height", v.into_svg_num().to_string())
    }
    pub fn x1(self, v: impl IntoSvgNum) -> Self {
        self.set("x1", v.into_svg_num().to_string())
    }
    pub fn y1(self, v: impl IntoSvgNum) -> Self {
        self.set("y1", v.into_svg_num().to_string())
    }
    pub fn x2(self, v: impl IntoSvgNum) -> Self {
        self.set("x2", v.into_svg_num().to_string())
    }
    pub fn y2(self, v: impl IntoSvgNum) -> Self {
        self.set("y2", v.into_svg_num().to_string())
    }
    pub fn points(self, pts: impl Into<String>) -> Self {
        self.set("points", pts)
    }
    pub fn fill(self, c: impl Into<String>) -> Self {
        self.set("fill", c)
    }
    pub fn stroke(self, c: impl Into<String>) -> Self {
        self.set("stroke", c)
    }
    pub fn stroke_width(self, w: impl IntoSvgNum) -> Self {
        self.set("stroke-width", w.into_svg_num().to_string())
    }
    pub fn opacity(self, o: impl IntoSvgNum) -> Self {
        self.set("opacity", o.into_svg_num().to_string())
    }
    pub fn transform(self, t: impl Into<String>) -> Self {
        self.set("transform", t)
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.push(child.into());
        self
    }
    pub fn children(mut self, children: impl IntoIterator<Item = Element>) -> Self {
        self.children.extend(children);
        self
    }
}

impl From<SvgShapeBuilder> for Element {
    fn from(b: SvgShapeBuilder) -> Self {
        let mut el = Element::new();
        el.set_attr(ATTR_SVG_KIND, b.kind);
        for (k, v) in b.attrs {
            el.set_attr(k, v);
        }
        el.children = b.children;
        el
    }
}

impl IntoElementList for SvgShapeBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::path::{points_from_commands, rasterize_layers_rgba};

    fn sample_svg() -> Element {
        svg()
            .view_box("0 0 100 100")
            .w(160.0)
            .h(160.0)
            .child(
                circle()
                    .cx(50.0)
                    .cy(50.0)
                    .r(36.0)
                    .fill("primary")
                    .stroke("foreground")
                    .stroke_width(3.0),
            )
            .child(
                path()
                    .d("M12,78 L50,88 L88,78")
                    .fill("none")
                    .stroke("destructive")
                    .stroke_width(4.0),
            )
            .into()
    }

    #[test]
    fn builder_leaf_has_no_taffy_children() {
        let el = sample_svg();
        assert!(
            el.children.is_empty(),
            "svg shapes must flatten into the leaf, got {} children",
            el.children.len()
        );
        assert!(el.get_attr(ATTR_PATH_SCENE).is_some());
        let d = el.get_attr(ATTR_PATH_D).expect("shared path d");
        assert!(d.contains('M') && d.contains('L'), "{d}");
        assert_eq!(
            el.get_attr(ATTR_PATH_FIT_LEAF).map(String::as_str),
            Some("1")
        );
    }

    #[test]
    fn compile_circle_and_path_use_path_command_kernel() {
        let nodes = vec![
            node_from_element(&circle().cx(50).cy(50).r(10).fill("primary").into()).unwrap(),
            node_from_element(
                &path()
                    .d("M0 0 L10 0 L10 8 Z")
                    .fill("none")
                    .stroke("foreground")
                    .stroke_width(2)
                    .into(),
            )
            .unwrap(),
        ];
        let (layers, vb) = compile_layers(&nodes, Some("0 0 100 100"));
        assert_eq!(layers.len(), 2);
        assert!((vb[2] - 100.0).abs() < 1e-3 && (vb[3] - 100.0).abs() < 1e-3);
        let circle_pts = points_from_commands(&layers[0].0);
        assert!(circle_pts.len() >= 8);
        assert!(layers[0].1.fill.is_some());
        assert!(layers[1].1.stroke.is_some() && layers[1].1.fill.is_none());
        match &layers[1].0[0] {
            PathCommand::MoveTo(_) => {}
            other => panic!("path must start with MoveTo, got {other:?}"),
        }
    }

    #[test]
    fn raster_has_fill_and_stroke_coverage() {
        let nodes = vec![
            node_from_element(&circle().cx(40).cy(40).r(24).fill("#2563eb").into()).unwrap(),
            node_from_element(
                &path()
                    .d("M8 72 L40 80 L72 72")
                    .fill("none")
                    .stroke("#dc2626")
                    .stroke_width(5)
                    .into(),
            )
            .unwrap(),
        ];
        let (layers, _) = compile_layers(&nodes, Some("0 0 80 80"));
        let rgba = rasterize_layers_rgba(&layers, &[], 0.0, 80, 80, 1);
        let alpha = |x: u32, y: u32| rgba[((y * 80 + x) * 4 + 3) as usize];
        assert!(
            alpha(40, 40) > 180,
            "circle fill center, a={}",
            alpha(40, 40)
        );
        assert!(
            alpha(40, 80.min(79)) > 40 || alpha(40, 72) > 80,
            "path stroke"
        );
        assert!(
            alpha(2, 2) < 20,
            "outside should be empty, a={}",
            alpha(2, 2)
        );
    }

    #[test]
    fn circle_stroke_ring_is_distinct_from_fill() {
        let node = node_from_element(
            &circle()
                .cx(40)
                .cy(40)
                .r(20)
                .fill("#2563eb")
                .stroke("#dc2626")
                .stroke_width(5)
                .into(),
        )
        .unwrap();
        let (layers, _) = compile_layers(&[node], Some("0 0 80 80"));
        assert!(layers[0].1.fill.is_some() && layers[0].1.stroke.is_some());
        let rgba = rasterize_layers_rgba(&layers, &[], 0.0, 80, 80, 1);
        let px = |x: u32, y: u32| {
            let i = ((y * 80 + x) * 4) as usize;
            (rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3])
        };
        let (cr, cg, cb, ca) = px(40, 40);
        assert!(
            ca > 180 && cb > cr + 40,
            "center should be blue fill, got {cr},{cg},{cb},{ca}"
        );
        let (rr, rg, rb, ra) = px(40, 20);
        assert!(
            ra > 80 && rr > rb + 30,
            "rim should show red stroke, got {rr},{rg},{rb},{ra}"
        );
    }

    #[test]
    fn group_translate_moves_circle() {
        let g_el: Element = g()
            .transform("translate(30 0)")
            .child(circle().cx(10).cy(10).r(4).fill("#111111"))
            .into();
        let node = node_from_element(&g_el).unwrap();
        let (layers, _) = compile_layers(&[node], Some("0 0 80 80"));
        let pts = points_from_commands(&layers[0].0);
        let n = pts.len().max(1) as f32;
        let cx = pts.iter().map(|p| p.0).sum::<f32>() / n;
        assert!(
            (cx - 40.0).abs() < 1.5,
            "translated circle centroid x should be ~40, got {cx}"
        );
    }
}
