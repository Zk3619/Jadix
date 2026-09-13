//! # Public `<svg>` leaf (WP-3.11)
//!
//! HTML `<svg viewBox>` mental model, but a **vector scene graph inside an RUI Element leaf**, not the DOM.
//! Child shapes (`path` / `circle` / `ellipse` / `rect` / `line` / `polyline` / `polygon` / `g`) do **not** each own a taffy node.
//! Paint uses the same [`PathCommand`] core as Chart (CPU coverage raster atlas); curves in `<path d>` flatten to M/L/Z in [`decode_path_d`].
//! emoji-svg / resvg are **out of scope** for this module.
//!
//! Dual surface: [`svg`] Builder; `ui!` `<svg>` / `<circle>` / `<path>`… (see `rui_macros`).

use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use crate::core::tree::{Element, IntoCssDim, IntoElementList};
use crate::primitives::path::{
    decode_path_d, encode_path_d, ensure_raster_texture, filled_circle, filled_rect,
    polygon as path_polygon, polyline as path_polyline, register_path_scene, FillRule, PathCommand,
    PathStyle, Point2, StrokeLinecap, StrokeLinejoin, ATTR_PATH_D, ATTR_PATH_FIT_LEAF,
    ATTR_PATH_SCENE, ATTR_PATH_VIEWBOX,
};
use crate::style::parse_color;

/// Child-shape Element marker (consumed when compiling the scene graph; not laid out).
pub const ATTR_SVG_KIND: &str = "rui_svg_kind";
/// Stored resvg scene (`register_svg_scene` hex id).
pub const ATTR_SVG_SCENE: &str = "rui_svg_scene";
pub const ATTR_SVG_HAS_SMIL: &str = "rui_svg_has_smil";
/// Text content for `<text>` / `<title>` / `<desc>` / `<tspan>` (`text_content` Builder).
pub const ATTR_SVG_TEXT: &str = "rui_svg_text";

/// Tags that must not enter the public SVG surface (no DOM / no host HTML).
fn is_blocked_svg_tag(tag: &str) -> bool {
    matches!(
        tag,
        "script" | "Script" | "foreignObject" | "ForeignObject" | "foreignobject"
    )
}

/// Paint / never-rendered tags that belong in `<defs>` when not under an explicit `<defs>`.
fn is_elem_defs_tag(tag: &str) -> bool {
    matches!(
        tag,
        "filter"
            | "mask"
            | "pattern"
            | "marker"
            | "style"
            | "metadata"
            | "title"
            | "desc"
            | "mpath"
            | "linearGradient"
            | "radialGradient"
            | "clipPath"
            | "symbol"
    ) || tag.starts_with("fe")
}

/// SVG user units / literals → f32 (`cx={50}` / `cx="50"` / `50.0`).
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

/// SVG coordinate or gradient stop position (`50` / `"50%"` / `"0.5"`).
pub trait IntoSvgCoord {
    fn into_svg_coord(self) -> String;
}

impl IntoSvgCoord for f32 {
    fn into_svg_coord(self) -> String {
        self.to_string()
    }
}
impl IntoSvgCoord for f64 {
    fn into_svg_coord(self) -> String {
        self.to_string()
    }
}
impl IntoSvgCoord for i32 {
    fn into_svg_coord(self) -> String {
        self.to_string()
    }
}
impl IntoSvgCoord for &str {
    fn into_svg_coord(self) -> String {
        self.to_string()
    }
}
impl IntoSvgCoord for String {
    fn into_svg_coord(self) -> String {
        self
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

    /// `self` on the left: apply `other` first, then `self` (parent transform wraps child).
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
    if parse_url_paint_id(s).is_some() {
        // Resolved via paint defs in `resolve_style`.
        return None;
    }
    Some(parse_color(s))
}

fn parse_url_paint_id(raw: &str) -> Option<&str> {
    let s = raw.trim();
    let inner = s.strip_prefix("url(")?.trim();
    let inner = inner.strip_suffix(')')?.trim();
    let id = inner.trim_matches(|c| c == '"' || c == '\'').trim();
    let id = id.strip_prefix('#').unwrap_or(id).trim();
    if id.is_empty() {
        None
    } else {
        Some(id)
    }
}

fn parse_grad_unit(raw: &str) -> f32 {
    let t = raw.trim();
    if let Some(p) = t.strip_suffix('%') {
        p.trim().parse::<f32>().unwrap_or(0.0) / 100.0
    } else {
        t.parse::<f32>().unwrap_or(0.0)
    }
}

fn svg_stops_to_css(stops: &[GradientStop]) -> Vec<crate::style::ColorStop> {
    stops
        .iter()
        .map(|s| {
            let mut c = parse_color(&s.color);
            c[3] = (c[3] * s.opacity).clamp(0.0, 1.0);
            crate::style::ColorStop::at(c, parse_grad_unit(&s.offset).clamp(0.0, 1.0))
        })
        .collect()
}

fn linear_gradient_to_css(
    x1: &str,
    y1: &str,
    x2: &str,
    y2: &str,
    stops: &[GradientStop],
) -> crate::style::Gradient {
    let x1 = parse_grad_unit(x1);
    let y1 = parse_grad_unit(y1);
    let x2 = parse_grad_unit(x2);
    let y2 = parse_grad_unit(y2);
    // PathStyle sampling uses the same convention as CSS linear: 0° = left→right.
    let angle_deg = (y2 - y1).atan2(x2 - x1).to_degrees();
    crate::style::Gradient::Linear(crate::style::LinearGradient {
        angle_deg,
        stops: svg_stops_to_css(stops),
        hints: Vec::new(),
        repeating: false,
    })
}

fn radial_gradient_to_css(
    cx: &str,
    cy: &str,
    r: &str,
    stops: &[GradientStop],
) -> crate::style::Gradient {
    let cx = parse_grad_unit(cx);
    let cy = parse_grad_unit(cy);
    let r = parse_grad_unit(r).max(0.01);
    crate::style::Gradient::Radial(crate::style::RadialGradient {
        shape: crate::style::RadialShape::Circle,
        extent: crate::style::RadialExtent::Circle(r),
        position: crate::style::GradientPosition { x: cx, y: cy },
        stops: svg_stops_to_css(stops),
        hints: Vec::new(),
        repeating: false,
    })
}

fn collect_paint_defs(nodes: &[SvgNode], out: &mut HashMap<String, crate::style::Gradient>) {
    for n in nodes {
        match n {
            SvgNode::Defs { children }
            | SvgNode::Group { children, .. }
            | SvgNode::ClipPath { children, .. }
            | SvgNode::Symbol { children, .. } => collect_paint_defs(children, out),
            SvgNode::LinearGradient {
                id,
                x1,
                y1,
                x2,
                y2,
                stops,
                ..
            } => {
                if !id.is_empty() {
                    out.insert(id.clone(), linear_gradient_to_css(x1, y1, x2, y2, stops));
                }
            }
            SvgNode::RadialGradient {
                id,
                cx,
                cy,
                r,
                stops,
                ..
            } => {
                if !id.is_empty() {
                    out.insert(id.clone(), radial_gradient_to_css(cx, cy, r, stops));
                }
            }
            _ => {}
        }
    }
}

fn resolve_style(
    fill_raw: Option<&str>,
    stroke_raw: Option<&str>,
    stroke_width: f32,
    opacity: f32,
    default_fill: bool,
    default_stroke: bool,
    linecap: StrokeLinecap,
    linejoin: StrokeLinejoin,
    fill_rule: FillRule,
    xf: Affine,
    paints: &HashMap<String, crate::style::Gradient>,
) -> Option<PathStyle> {
    let mut fill_gradient = None;
    let fill = if let Some(raw) = fill_raw.map(str::trim).filter(|s| !s.is_empty()) {
        if raw.eq_ignore_ascii_case("none") {
            None
        } else if let Some(id) = parse_url_paint_id(raw) {
            fill_gradient = paints.get(id).cloned();
            // Opacity carrier for gradient fills.
            fill_gradient.as_ref().map(|_| [1.0, 1.0, 1.0, 1.0])
        } else {
            Some(parse_color(raw))
        }
    } else if default_fill {
        Some(parse_color("foreground"))
    } else {
        None
    };
    let stroke = parse_paint(stroke_raw).or_else(|| {
        if default_stroke && stroke_raw.is_none() {
            Some(parse_color("foreground"))
        } else {
            None
        }
    });
    if fill.is_none() && fill_gradient.is_none() && stroke.is_none() {
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
        fill_gradient,
        stroke: stroke.map(|c| mul_alpha(c, opacity)),
        stroke_width: sw,
        stroke_linecap: linecap,
        stroke_linejoin: linejoin,
        fill_rule,
    })
}

fn push_styled(
    out: &mut Vec<(Vec<PathCommand>, PathStyle)>,
    cmds: &[PathCommand],
    fill: Option<&str>,
    stroke: Option<&str>,
    stroke_width: f32,
    opacity: f32,
    default_fill: bool,
    default_stroke: bool,
    linecap: StrokeLinecap,
    linejoin: StrokeLinejoin,
    fill_rule: FillRule,
    xf: Affine,
    paints: &HashMap<String, crate::style::Gradient>,
) {
    let Some(style) = resolve_style(
        fill,
        stroke,
        stroke_width,
        opacity,
        default_fill,
        default_stroke,
        linecap,
        linejoin,
        fill_rule,
        xf,
        paints,
    ) else {
        return;
    };
    out.push((apply_affine(cmds, xf), style));
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

fn stroke_attrs(el: &Element) -> (StrokeLinecap, StrokeLinejoin, FillRule) {
    let linecap = el
        .get_attr("stroke-linecap")
        .or_else(|| el.get_attr("stroke_linecap"))
        .map(|s| StrokeLinecap::parse(s))
        .unwrap_or_default();
    let linejoin = el
        .get_attr("stroke-linejoin")
        .or_else(|| el.get_attr("stroke_linejoin"))
        .map(|s| StrokeLinejoin::parse(s))
        .unwrap_or_default();
    let fill_rule = el
        .get_attr("fill-rule")
        .or_else(|| el.get_attr("fill_rule"))
        .map(|s| FillRule::parse(s))
        .unwrap_or_default();
    (linecap, linejoin, fill_rule)
}

fn filled_ellipse(cx: f32, cy: f32, rx: f32, ry: f32, segments: usize) -> Vec<PathCommand> {
    let n = segments.max(8);
    let rx = rx.max(0.0);
    let ry = ry.max(0.0);
    let mut pts = Vec::with_capacity(n);
    for i in 0..n {
        let a = std::f32::consts::TAU * (i as f32) / (n as f32);
        pts.push((cx + rx * a.cos(), cy + ry * a.sin()));
    }
    path_polygon(&pts)
}

#[derive(Clone, Debug)]
pub(crate) struct GradientStop {
    pub offset: String,
    pub color: String,
    pub opacity: f32,
}

#[derive(Clone, Debug)]
pub(crate) enum SvgNode {
    Circle {
        id: Option<String>,
        clip_path: Option<String>,
        cx: f32,
        cy: f32,
        r: f32,
        fill: Option<String>,
        stroke: Option<String>,
        stroke_width: f32,
        linecap: StrokeLinecap,
        linejoin: StrokeLinejoin,
        fill_rule: FillRule,
        opacity: f32,
        transform: String,
    },
    Ellipse {
        id: Option<String>,
        clip_path: Option<String>,
        cx: f32,
        cy: f32,
        rx: f32,
        ry: f32,
        fill: Option<String>,
        stroke: Option<String>,
        stroke_width: f32,
        linecap: StrokeLinecap,
        linejoin: StrokeLinejoin,
        fill_rule: FillRule,
        opacity: f32,
        transform: String,
    },
    Path {
        id: Option<String>,
        clip_path: Option<String>,
        d: String,
        fill: Option<String>,
        stroke: Option<String>,
        stroke_width: f32,
        linecap: StrokeLinecap,
        linejoin: StrokeLinejoin,
        fill_rule: FillRule,
        opacity: f32,
        transform: String,
    },
    Rect {
        id: Option<String>,
        clip_path: Option<String>,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        fill: Option<String>,
        stroke: Option<String>,
        stroke_width: f32,
        linecap: StrokeLinecap,
        linejoin: StrokeLinejoin,
        fill_rule: FillRule,
        opacity: f32,
        transform: String,
    },
    Line {
        id: Option<String>,
        clip_path: Option<String>,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        stroke: Option<String>,
        stroke_width: f32,
        linecap: StrokeLinecap,
        linejoin: StrokeLinejoin,
        opacity: f32,
        transform: String,
    },
    Polyline {
        id: Option<String>,
        clip_path: Option<String>,
        points: Vec<Point2>,
        closed: bool,
        fill: Option<String>,
        stroke: Option<String>,
        stroke_width: f32,
        linecap: StrokeLinecap,
        linejoin: StrokeLinejoin,
        fill_rule: FillRule,
        opacity: f32,
        transform: String,
    },
    Group {
        transform: String,
        opacity: f32,
        children: Vec<SvgNode>,
    },
    Defs {
        children: Vec<SvgNode>,
    },
    Use {
        id: Option<String>,
        href: String,
        x: f32,
        y: f32,
        width: Option<f32>,
        height: Option<f32>,
        opacity: f32,
        transform: String,
    },
    ClipPath {
        id: String,
        clip_rule: String,
        children: Vec<SvgNode>,
    },
    LinearGradient {
        id: String,
        x1: String,
        y1: String,
        x2: String,
        y2: String,
        gradient_units: String,
        stops: Vec<GradientStop>,
    },
    RadialGradient {
        id: String,
        cx: String,
        cy: String,
        r: String,
        fx: Option<String>,
        fy: Option<String>,
        gradient_units: String,
        stops: Vec<GradientStop>,
    },
    Symbol {
        id: String,
        view_box: Option<String>,
        children: Vec<SvgNode>,
    },
    Animate {
        attribute_name: String,
        from: String,
        to: String,
        dur: String,
        repeat_count: String,
        /// Parsed for future begin-offset support; not yet applied in SMIL eval.
        #[allow(dead_code)]
        begin: String,
    },
    AnimateTransform {
        transform_type: String,
        from: String,
        to: String,
        dur: String,
        repeat_count: String,
        /// Parsed for future begin-offset support; not yet applied in SMIL eval.
        #[allow(dead_code)]
        begin: String,
    },
    Set {
        attribute_name: String,
        to: String,
        /// Parsed for future begin-offset support; not yet applied in SMIL eval.
        #[allow(dead_code)]
        begin: String,
    },
    /// Passthrough element for resvg (text/image/mask/filter/fe*/…); not compiled to PathCommand.
    Elem {
        tag: String,
        attrs: Vec<(String, String)>,
        children: Vec<SvgNode>,
        text: Option<String>,
    },
}

fn opt_attr(el: &Element, key: &str) -> Option<String> {
    el.get_attr(key).filter(|s| !s.trim().is_empty()).cloned()
}

fn shape_id_clip(el: &Element) -> (Option<String>, Option<String>) {
    (
        opt_attr(el, "id"),
        opt_attr(el, "clip-path").or_else(|| opt_attr(el, "clip_path")),
    )
}

fn href_from_element(el: &Element) -> String {
    opt_attr(el, "href")
        .or_else(|| opt_attr(el, "xlink:href"))
        .unwrap_or_default()
}

fn nodes_from_children(el: &Element) -> Vec<SvgNode> {
    el.children.iter().filter_map(node_from_element).collect()
}

fn stop_from_element(el: &Element) -> Option<GradientStop> {
    if el.get_attr(ATTR_SVG_KIND)?.as_str() != "stop" {
        return None;
    }
    Some(GradientStop {
        offset: opt_attr(el, "offset").unwrap_or_else(|| "0".into()),
        color: opt_attr(el, "stop-color")
            .or_else(|| opt_attr(el, "stop_color"))
            .unwrap_or_else(|| "black".into()),
        opacity: opt_attr(el, "stop-opacity")
            .or_else(|| opt_attr(el, "stop_opacity"))
            .map(|s| s.as_str().into_svg_num())
            .unwrap_or(1.0)
            .clamp(0.0, 1.0),
    })
}

fn is_smil_node(node: &SvgNode) -> bool {
    match node {
        SvgNode::Animate { .. } | SvgNode::AnimateTransform { .. } | SvgNode::Set { .. } => true,
        SvgNode::Elem { tag, .. } => matches!(
            tag.as_str(),
            "animate" | "animateTransform" | "animateMotion" | "set" | "mpath"
        ),
        _ => false,
    }
}

/// SMIL nodes that are baked by `svg_smil` and must not be re-emitted for resvg.
fn is_baked_smil_node(node: &SvgNode) -> bool {
    matches!(
        node,
        SvgNode::Animate { .. } | SvgNode::AnimateTransform { .. } | SvgNode::Set { .. }
    )
}

/// Shape Elements may nest SMIL children (`<circle><animate/></circle>`). Fold them into a
/// synthetic `<g>` so [`svg_smil::apply_smil`] can bake attributes onto the shape.
fn wrap_shape_with_smil(shape: SvgNode, el: &Element) -> SvgNode {
    let smil: Vec<SvgNode> = el
        .children
        .iter()
        .filter_map(node_from_element)
        .filter(is_smil_node)
        .collect();
    if smil.is_empty() {
        return shape;
    }
    let mut children = Vec::with_capacity(1 + smil.len());
    children.push(shape);
    children.extend(smil);
    SvgNode::Group {
        transform: String::new(),
        opacity: 1.0,
        children,
    }
}

fn is_defs_only_node(node: &SvgNode) -> bool {
    match node {
        SvgNode::Defs { .. }
        | SvgNode::LinearGradient { .. }
        | SvgNode::RadialGradient { .. }
        | SvgNode::ClipPath { .. }
        | SvgNode::Symbol { .. } => true,
        SvgNode::Elem { tag, .. } => is_elem_defs_tag(tag),
        _ => false,
    }
}

fn elem_attrs_from_element(el: &Element) -> Vec<(String, String)> {
    let mut attrs: Vec<(String, String)> = el
        .attrs
        .iter()
        .filter(|(k, _)| {
            k.as_str() != ATTR_SVG_KIND && k.as_str() != ATTR_SVG_TEXT && !k.starts_with("rui_")
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    attrs.sort_by(|a, b| a.0.cmp(&b.0));
    attrs
}

fn elem_text_from_element(el: &Element) -> Option<String> {
    if let Some(t) = el.get_attr(ATTR_SVG_TEXT).filter(|s| !s.is_empty()) {
        return Some(t.clone());
    }
    el.text
        .as_ref()
        .map(|s| s.as_str().to_string())
        .filter(|s| !s.is_empty())
}

fn elem_from_element(el: &Element, tag: &str) -> Option<SvgNode> {
    if is_blocked_svg_tag(tag) {
        return None;
    }
    Some(SvgNode::Elem {
        tag: tag.to_string(),
        attrs: elem_attrs_from_element(el),
        children: nodes_from_children(el),
        text: elem_text_from_element(el),
    })
}

fn node_from_element(el: &Element) -> Option<SvgNode> {
    let kind_owned = el.get_attr(ATTR_SVG_KIND)?.clone();
    let kind = kind_owned.as_str();
    // filter/mask are presentation effects typed CPU shapes don't model → Elem + resvg.
    if matches!(
        kind,
        "circle" | "ellipse" | "path" | "rect" | "line" | "polyline" | "polygon" | "g"
    ) && (opt_attr(el, "filter").is_some() || opt_attr(el, "mask").is_some())
    {
        return elem_from_element(el, kind);
    }
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
    let (linecap, linejoin, fill_rule) = stroke_attrs(el);
    let (id, clip_path) = shape_id_clip(el);
    match kind {
        "circle" => Some(wrap_shape_with_smil(
            SvgNode::Circle {
                id,
                clip_path,
                cx: attr_f32(el, "cx"),
                cy: attr_f32(el, "cy"),
                r: attr_f32(el, "r").max(0.0),
                fill,
                stroke,
                stroke_width,
                linecap,
                linejoin,
                fill_rule,
                opacity,
                transform,
            },
            el,
        )),
        "ellipse" => Some(wrap_shape_with_smil(
            SvgNode::Ellipse {
                id,
                clip_path,
                cx: attr_f32(el, "cx"),
                cy: attr_f32(el, "cy"),
                rx: attr_f32(el, "rx").max(0.0),
                ry: attr_f32(el, "ry").max(0.0),
                fill,
                stroke,
                stroke_width,
                linecap,
                linejoin,
                fill_rule,
                opacity,
                transform,
            },
            el,
        )),
        "path" => Some(wrap_shape_with_smil(
            SvgNode::Path {
                id,
                clip_path,
                d: el.get_attr("d").cloned().unwrap_or_default(),
                fill,
                stroke,
                stroke_width,
                linecap,
                linejoin,
                fill_rule,
                opacity,
                transform,
            },
            el,
        )),
        "rect" => Some(wrap_shape_with_smil(
            SvgNode::Rect {
                id,
                clip_path,
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
                linecap,
                linejoin,
                fill_rule,
                opacity,
                transform,
            },
            el,
        )),
        "line" => Some(wrap_shape_with_smil(
            SvgNode::Line {
                id,
                clip_path,
                x1: attr_f32(el, "x1"),
                y1: attr_f32(el, "y1"),
                x2: attr_f32(el, "x2"),
                y2: attr_f32(el, "y2"),
                stroke,
                stroke_width,
                linecap,
                linejoin,
                opacity,
                transform,
            },
            el,
        )),
        "polyline" => Some(wrap_shape_with_smil(
            SvgNode::Polyline {
                id,
                clip_path,
                points: el
                    .get_attr("points")
                    .map(|s| parse_svg_points(s))
                    .unwrap_or_default(),
                closed: false,
                fill,
                stroke,
                stroke_width,
                linecap,
                linejoin,
                fill_rule,
                opacity,
                transform,
            },
            el,
        )),
        "polygon" => Some(wrap_shape_with_smil(
            SvgNode::Polyline {
                id,
                clip_path,
                points: el
                    .get_attr("points")
                    .map(|s| parse_svg_points(s))
                    .unwrap_or_default(),
                closed: true,
                fill,
                stroke,
                stroke_width,
                linecap,
                linejoin,
                fill_rule,
                opacity,
                transform,
            },
            el,
        )),
        "g" => Some(SvgNode::Group {
            transform,
            opacity,
            children: nodes_from_children(el),
        }),
        "defs" => Some(SvgNode::Defs {
            children: nodes_from_children(el),
        }),
        "use" => Some(wrap_shape_with_smil(
            SvgNode::Use {
                id,
                href: href_from_element(el),
                x: attr_f32(el, "x"),
                y: attr_f32(el, "y"),
                width: opt_attr(el, "width").map(|s| s.as_str().into_svg_num()),
                height: opt_attr(el, "height").map(|s| s.as_str().into_svg_num()),
                opacity,
                transform,
            },
            el,
        )),
        "clippath" | "clipPath" => Some(SvgNode::ClipPath {
            id: opt_attr(el, "id").unwrap_or_default(),
            clip_rule: opt_attr(el, "clip-rule")
                .or_else(|| opt_attr(el, "clip_rule"))
                .unwrap_or_else(|| "nonzero".into()),
            children: nodes_from_children(el),
        }),
        "lineargradient" | "linearGradient" => Some(SvgNode::LinearGradient {
            id: opt_attr(el, "id").unwrap_or_default(),
            x1: opt_attr(el, "x1").unwrap_or_else(|| "0%".into()),
            y1: opt_attr(el, "y1").unwrap_or_else(|| "0%".into()),
            x2: opt_attr(el, "x2").unwrap_or_else(|| "100%".into()),
            y2: opt_attr(el, "y2").unwrap_or_else(|| "0%".into()),
            gradient_units: opt_attr(el, "gradientUnits")
                .or_else(|| opt_attr(el, "gradient_units"))
                .unwrap_or_else(|| "objectBoundingBox".into()),
            stops: el.children.iter().filter_map(stop_from_element).collect(),
        }),
        "radialgradient" | "radialGradient" => Some(SvgNode::RadialGradient {
            id: opt_attr(el, "id").unwrap_or_default(),
            cx: opt_attr(el, "cx").unwrap_or_else(|| "50%".into()),
            cy: opt_attr(el, "cy").unwrap_or_else(|| "50%".into()),
            r: opt_attr(el, "r").unwrap_or_else(|| "50%".into()),
            fx: opt_attr(el, "fx"),
            fy: opt_attr(el, "fy"),
            gradient_units: opt_attr(el, "gradientUnits")
                .or_else(|| opt_attr(el, "gradient_units"))
                .unwrap_or_else(|| "objectBoundingBox".into()),
            stops: el.children.iter().filter_map(stop_from_element).collect(),
        }),
        "symbol" => Some(SvgNode::Symbol {
            id: opt_attr(el, "id").unwrap_or_default(),
            view_box: opt_attr(el, "viewBox").or_else(|| opt_attr(el, "view_box")),
            children: nodes_from_children(el),
        }),
        "animate" => Some(SvgNode::Animate {
            attribute_name: opt_attr(el, "attributeName")
                .or_else(|| opt_attr(el, "attribute_name"))
                .unwrap_or_default(),
            from: opt_attr(el, "from").unwrap_or_default(),
            to: opt_attr(el, "to").unwrap_or_default(),
            dur: opt_attr(el, "dur").unwrap_or_else(|| "1s".into()),
            repeat_count: opt_attr(el, "repeatCount")
                .or_else(|| opt_attr(el, "repeat_count"))
                .unwrap_or_else(|| "1".into()),
            begin: opt_attr(el, "begin").unwrap_or_default(),
        }),
        "animatetransform" | "animateTransform" => Some(SvgNode::AnimateTransform {
            transform_type: opt_attr(el, "type").unwrap_or_else(|| "rotate".into()),
            from: opt_attr(el, "from").unwrap_or_default(),
            to: opt_attr(el, "to").unwrap_or_default(),
            dur: opt_attr(el, "dur").unwrap_or_else(|| "1s".into()),
            repeat_count: opt_attr(el, "repeatCount")
                .or_else(|| opt_attr(el, "repeat_count"))
                .unwrap_or_else(|| "1".into()),
            begin: opt_attr(el, "begin").unwrap_or_default(),
        }),
        "set" => Some(SvgNode::Set {
            attribute_name: opt_attr(el, "attributeName")
                .or_else(|| opt_attr(el, "attribute_name"))
                .unwrap_or_default(),
            to: opt_attr(el, "to").unwrap_or_default(),
            begin: opt_attr(el, "begin").unwrap_or_default(),
        }),
        other => elem_from_element(el, other),
    }
}

fn compile_node(
    node: &SvgNode,
    parent: Affine,
    parent_opacity: f32,
    paints: &HashMap<String, crate::style::Gradient>,
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
                if is_smil_node(c) {
                    continue;
                }
                compile_node(c, xf, o, paints, out);
            }
        }
        SvgNode::Defs { .. }
        | SvgNode::Use { .. }
        | SvgNode::ClipPath { .. }
        | SvgNode::LinearGradient { .. }
        | SvgNode::RadialGradient { .. }
        | SvgNode::Symbol { .. }
        | SvgNode::Animate { .. }
        | SvgNode::AnimateTransform { .. }
        | SvgNode::Set { .. }
        | SvgNode::Elem { .. } => {}
        SvgNode::Circle {
            cx,
            cy,
            r,
            fill,
            stroke,
            stroke_width,
            linecap,
            linejoin,
            fill_rule,
            opacity,
            transform,
            ..
        } => {
            if *r <= 0.0 {
                return;
            }
            let xf = parent.mul(parse_transform(transform));
            let o = (parent_opacity * *opacity).clamp(0.0, 1.0);
            push_styled(
                out,
                &filled_circle(*cx, *cy, *r, 32),
                fill.as_deref(),
                stroke.as_deref(),
                *stroke_width,
                o,
                true,
                false,
                *linecap,
                *linejoin,
                *fill_rule,
                xf,
                paints,
            );
        }
        SvgNode::Ellipse {
            cx,
            cy,
            rx,
            ry,
            fill,
            stroke,
            stroke_width,
            linecap,
            linejoin,
            fill_rule,
            opacity,
            transform,
            ..
        } => {
            if *rx <= 0.0 || *ry <= 0.0 {
                return;
            }
            let xf = parent.mul(parse_transform(transform));
            let o = (parent_opacity * *opacity).clamp(0.0, 1.0);
            push_styled(
                out,
                &filled_ellipse(*cx, *cy, *rx, *ry, 32),
                fill.as_deref(),
                stroke.as_deref(),
                *stroke_width,
                o,
                true,
                false,
                *linecap,
                *linejoin,
                *fill_rule,
                xf,
                paints,
            );
        }
        SvgNode::Path {
            d,
            fill,
            stroke,
            stroke_width,
            linecap,
            linejoin,
            fill_rule,
            opacity,
            transform,
            ..
        } => {
            let raw = decode_path_d(d);
            if raw.is_empty() {
                return;
            }
            let xf = parent.mul(parse_transform(transform));
            let o = (parent_opacity * *opacity).clamp(0.0, 1.0);
            push_styled(
                out,
                &raw,
                fill.as_deref(),
                stroke.as_deref(),
                *stroke_width,
                o,
                true,
                false,
                *linecap,
                *linejoin,
                *fill_rule,
                xf,
                paints,
            );
        }
        SvgNode::Rect {
            x,
            y,
            w,
            h,
            fill,
            stroke,
            stroke_width,
            linecap,
            linejoin,
            fill_rule,
            opacity,
            transform,
            ..
        } => {
            if *w <= 0.0 || *h <= 0.0 {
                return;
            }
            let xf = parent.mul(parse_transform(transform));
            let o = (parent_opacity * *opacity).clamp(0.0, 1.0);
            push_styled(
                out,
                &filled_rect(*x, *y, *w, *h),
                fill.as_deref(),
                stroke.as_deref(),
                *stroke_width,
                o,
                true,
                false,
                *linecap,
                *linejoin,
                *fill_rule,
                xf,
                paints,
            );
        }
        SvgNode::Line {
            x1,
            y1,
            x2,
            y2,
            stroke,
            stroke_width,
            linecap,
            linejoin,
            opacity,
            transform,
            ..
        } => {
            let xf = parent.mul(parse_transform(transform));
            let o = (parent_opacity * *opacity).clamp(0.0, 1.0);
            push_styled(
                out,
                &path_polyline(&[(*x1, *y1), (*x2, *y2)]),
                Some("none"),
                stroke.as_deref(),
                *stroke_width,
                o,
                false,
                true,
                *linecap,
                *linejoin,
                FillRule::default(),
                xf,
                paints,
            );
        }
        SvgNode::Polyline {
            points,
            closed,
            fill,
            stroke,
            stroke_width,
            linecap,
            linejoin,
            fill_rule,
            opacity,
            transform,
            ..
        } => {
            if points.len() < 2 {
                return;
            }
            let xf = parent.mul(parse_transform(transform));
            let o = (parent_opacity * *opacity).clamp(0.0, 1.0);
            let cmds = if *closed {
                path_polygon(points)
            } else {
                path_polyline(points)
            };
            push_styled(
                out,
                &cmds,
                fill.as_deref(),
                stroke.as_deref(),
                *stroke_width,
                o,
                *closed,
                true,
                *linecap,
                *linejoin,
                *fill_rule,
                xf,
                paints,
            );
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
    let mut paints = HashMap::new();
    collect_paint_defs(nodes, &mut paints);
    let mut layers = Vec::new();
    for n in nodes {
        compile_node(n, Affine::identity(), 1.0, &paints, &mut layers);
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

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
}

fn push_transform(buf: &mut String, transform: &str) {
    if !transform.trim().is_empty() {
        push_attr(buf, "transform", Some(transform));
    }
}

fn push_attr(buf: &mut String, name: &str, val: Option<&str>) {
    if let Some(v) = val.filter(|s| !s.trim().is_empty()) {
        buf.push(' ');
        buf.push_str(name);
        buf.push_str("=\"");
        buf.push_str(&xml_escape(v));
        buf.push('"');
    }
}

fn push_f_attr(buf: &mut String, name: &str, val: f32) {
    if val.is_finite() {
        push_attr(buf, name, Some(&format!("{val}")));
    }
}

/// Theme tokens (`primary`, `border`, `chart-1`, …) are not CSS named colors.
/// resvg/usvg drops unknown paints → fully transparent pixmap while the leaf
/// still occupies layout (Gallery ProgressCircle looks like empty space).
fn svg_paint_to_css(raw: &str) -> String {
    let s = raw.trim();
    if s.is_empty()
        || s.eq_ignore_ascii_case("none")
        || s.eq_ignore_ascii_case("currentcolor")
        || s.eq_ignore_ascii_case("transparent")
        || s.starts_with("url(")
        || s.starts_with('#')
        || s.contains('(')
    {
        return s.to_string();
    }
    match crate::style::try_parse_color(s) {
        Some(c) => {
            let r = (c[0] * 255.0).round().clamp(0.0, 255.0) as u8;
            let g = (c[1] * 255.0).round().clamp(0.0, 255.0) as u8;
            let b = (c[2] * 255.0).round().clamp(0.0, 255.0) as u8;
            let a = c[3].clamp(0.0, 1.0);
            if a >= 1.0 - f32::EPSILON {
                format!("rgb({r},{g},{b})")
            } else {
                format!("rgba({r},{g},{b},{a:.4})")
            }
        }
        None => s.to_string(),
    }
}

fn push_paint_attr(buf: &mut String, name: &str, val: Option<&str>) {
    let owned = val.map(svg_paint_to_css);
    push_attr(buf, name, owned.as_deref());
}

fn emit_stroke_attrs(
    buf: &mut String,
    stroke: &Option<String>,
    stroke_width: f32,
    linecap: StrokeLinecap,
    linejoin: StrokeLinejoin,
    fill_rule: FillRule,
) {
    push_attr(buf, "fill-rule", Some(fill_rule.as_str()));
    push_attr(buf, "stroke-linecap", Some(linecap.as_str()));
    push_attr(buf, "stroke-linejoin", Some(linejoin.as_str()));
    push_paint_attr(buf, "stroke", stroke.as_deref());
    if stroke.is_some() && stroke_width > 0.0 {
        push_f_attr(buf, "stroke-width", stroke_width);
    }
}

fn emit_points_attr(buf: &mut String, points: &[Point2]) {
    if points.is_empty() {
        return;
    }
    let mut s = String::new();
    for (i, (x, y)) in points.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        use std::fmt::Write;
        let _ = write!(s, "{x},{y}");
    }
    push_attr(buf, "points", Some(&s));
}

fn push_id_clip(buf: &mut String, id: &Option<String>, clip_path: &Option<String>) {
    push_attr(buf, "id", id.as_deref());
    push_attr(buf, "clip-path", clip_path.as_deref());
}

fn emit_gradient_stops(buf: &mut String, stops: &[GradientStop]) {
    for st in stops {
        buf.push_str("<stop");
        push_attr(buf, "offset", Some(&st.offset));
        push_paint_attr(buf, "stop-color", Some(&st.color));
        if st.opacity < 1.0 {
            push_f_attr(buf, "stop-opacity", st.opacity);
        }
        buf.push_str("/>");
    }
}

fn split_defs_body(nodes: &[SvgNode]) -> (Vec<SvgNode>, Vec<SvgNode>) {
    let mut defs = Vec::new();
    let mut body = Vec::new();
    for n in nodes {
        match n {
            SvgNode::Defs { children } => defs.extend(children.clone()),
            other if is_defs_only_node(other) => defs.push(other.clone()),
            other => body.push(other.clone()),
        }
    }
    (defs, body)
}

fn emit_svg_node(buf: &mut String, node: &SvgNode) {
    match node {
        SvgNode::Group {
            transform,
            opacity,
            children,
        } => {
            buf.push_str("<g");
            push_transform(buf, transform);
            if *opacity < 1.0 {
                push_f_attr(buf, "opacity", *opacity);
            }
            buf.push('>');
            for c in children {
                if is_baked_smil_node(c) {
                    continue;
                }
                emit_svg_node(buf, c);
            }
            buf.push_str("</g>");
        }
        SvgNode::Defs { children } => {
            buf.push_str("<defs>");
            for c in children {
                emit_svg_node(buf, c);
            }
            buf.push_str("</defs>");
        }
        SvgNode::Use {
            id,
            href,
            x,
            y,
            width,
            height,
            opacity,
            transform,
        } => {
            buf.push_str("<use");
            push_attr(buf, "id", id.as_deref());
            let href_attr = if href.starts_with('#') {
                href.clone()
            } else if href.is_empty() {
                String::new()
            } else {
                format!("#{href}")
            };
            if !href_attr.is_empty() {
                push_attr(buf, "href", Some(&href_attr));
                push_attr(buf, "xlink:href", Some(&href_attr));
            }
            push_f_attr(buf, "x", *x);
            push_f_attr(buf, "y", *y);
            if let Some(w) = width {
                push_f_attr(buf, "width", *w);
            }
            if let Some(h) = height {
                push_f_attr(buf, "height", *h);
            }
            if *opacity < 1.0 {
                push_f_attr(buf, "opacity", *opacity);
            }
            push_transform(buf, transform);
            buf.push_str("/>");
        }
        SvgNode::ClipPath {
            id,
            clip_rule,
            children,
        } => {
            buf.push_str("<clipPath");
            push_attr(buf, "id", Some(id));
            push_attr(buf, "clip-rule", Some(clip_rule));
            buf.push('>');
            for c in children {
                emit_svg_node(buf, c);
            }
            buf.push_str("</clipPath>");
        }
        SvgNode::LinearGradient {
            id,
            x1,
            y1,
            x2,
            y2,
            gradient_units,
            stops,
        } => {
            buf.push_str("<linearGradient");
            push_attr(buf, "id", Some(id));
            push_attr(buf, "x1", Some(x1));
            push_attr(buf, "y1", Some(y1));
            push_attr(buf, "x2", Some(x2));
            push_attr(buf, "y2", Some(y2));
            push_attr(buf, "gradientUnits", Some(gradient_units));
            buf.push('>');
            emit_gradient_stops(buf, stops);
            buf.push_str("</linearGradient>");
        }
        SvgNode::RadialGradient {
            id,
            cx,
            cy,
            r,
            fx,
            fy,
            gradient_units,
            stops,
        } => {
            buf.push_str("<radialGradient");
            push_attr(buf, "id", Some(id));
            push_attr(buf, "cx", Some(cx));
            push_attr(buf, "cy", Some(cy));
            push_attr(buf, "r", Some(r));
            push_attr(buf, "fx", fx.as_deref());
            push_attr(buf, "fy", fy.as_deref());
            push_attr(buf, "gradientUnits", Some(gradient_units));
            buf.push('>');
            emit_gradient_stops(buf, stops);
            buf.push_str("</radialGradient>");
        }
        SvgNode::Symbol {
            id,
            view_box,
            children,
        } => {
            buf.push_str("<symbol");
            push_attr(buf, "id", Some(id));
            push_attr(buf, "viewBox", view_box.as_deref());
            buf.push('>');
            for c in children {
                emit_svg_node(buf, c);
            }
            buf.push_str("</symbol>");
        }
        SvgNode::Circle {
            id,
            clip_path,
            cx,
            cy,
            r,
            fill,
            stroke,
            stroke_width,
            linecap,
            linejoin,
            fill_rule,
            opacity,
            transform,
        } => {
            buf.push_str("<circle");
            push_id_clip(buf, id, clip_path);
            push_f_attr(buf, "cx", *cx);
            push_f_attr(buf, "cy", *cy);
            push_f_attr(buf, "r", *r);
            push_paint_attr(buf, "fill", fill.as_deref());
            emit_stroke_attrs(buf, stroke, *stroke_width, *linecap, *linejoin, *fill_rule);
            if *opacity < 1.0 {
                push_f_attr(buf, "opacity", *opacity);
            }
            push_transform(buf, transform);
            buf.push_str("/>");
        }
        SvgNode::Ellipse {
            id,
            clip_path,
            cx,
            cy,
            rx,
            ry,
            fill,
            stroke,
            stroke_width,
            linecap,
            linejoin,
            fill_rule,
            opacity,
            transform,
        } => {
            buf.push_str("<ellipse");
            push_id_clip(buf, id, clip_path);
            push_f_attr(buf, "cx", *cx);
            push_f_attr(buf, "cy", *cy);
            push_f_attr(buf, "rx", *rx);
            push_f_attr(buf, "ry", *ry);
            push_paint_attr(buf, "fill", fill.as_deref());
            emit_stroke_attrs(buf, stroke, *stroke_width, *linecap, *linejoin, *fill_rule);
            if *opacity < 1.0 {
                push_f_attr(buf, "opacity", *opacity);
            }
            push_transform(buf, transform);
            buf.push_str("/>");
        }
        SvgNode::Path {
            id,
            clip_path,
            d,
            fill,
            stroke,
            stroke_width,
            linecap,
            linejoin,
            fill_rule,
            opacity,
            transform,
        } => {
            buf.push_str("<path");
            push_id_clip(buf, id, clip_path);
            push_attr(buf, "d", Some(d));
            push_paint_attr(buf, "fill", fill.as_deref());
            emit_stroke_attrs(buf, stroke, *stroke_width, *linecap, *linejoin, *fill_rule);
            if *opacity < 1.0 {
                push_f_attr(buf, "opacity", *opacity);
            }
            push_transform(buf, transform);
            buf.push_str("/>");
        }
        SvgNode::Rect {
            id,
            clip_path,
            x,
            y,
            w,
            h,
            fill,
            stroke,
            stroke_width,
            linecap,
            linejoin,
            fill_rule,
            opacity,
            transform,
        } => {
            buf.push_str("<rect");
            push_id_clip(buf, id, clip_path);
            push_f_attr(buf, "x", *x);
            push_f_attr(buf, "y", *y);
            push_f_attr(buf, "width", *w);
            push_f_attr(buf, "height", *h);
            push_paint_attr(buf, "fill", fill.as_deref());
            emit_stroke_attrs(buf, stroke, *stroke_width, *linecap, *linejoin, *fill_rule);
            if *opacity < 1.0 {
                push_f_attr(buf, "opacity", *opacity);
            }
            push_transform(buf, transform);
            buf.push_str("/>");
        }
        SvgNode::Line {
            id,
            clip_path,
            x1,
            y1,
            x2,
            y2,
            stroke,
            stroke_width,
            linecap,
            linejoin,
            opacity,
            transform,
        } => {
            buf.push_str("<line");
            push_id_clip(buf, id, clip_path);
            push_f_attr(buf, "x1", *x1);
            push_f_attr(buf, "y1", *y1);
            push_f_attr(buf, "x2", *x2);
            push_f_attr(buf, "y2", *y2);
            push_paint_attr(buf, "stroke", stroke.as_deref());
            if stroke.is_some() && *stroke_width > 0.0 {
                push_f_attr(buf, "stroke-width", *stroke_width);
            }
            push_attr(buf, "stroke-linecap", Some(linecap.as_str()));
            push_attr(buf, "stroke-linejoin", Some(linejoin.as_str()));
            if *opacity < 1.0 {
                push_f_attr(buf, "opacity", *opacity);
            }
            push_transform(buf, transform);
            buf.push_str("/>");
        }
        SvgNode::Polyline {
            id,
            clip_path,
            points,
            closed,
            fill,
            stroke,
            stroke_width,
            linecap,
            linejoin,
            fill_rule,
            opacity,
            transform,
        } => {
            let tag = if *closed { "polygon" } else { "polyline" };
            buf.push('<');
            buf.push_str(tag);
            push_id_clip(buf, id, clip_path);
            emit_points_attr(buf, points);
            push_paint_attr(buf, "fill", fill.as_deref());
            emit_stroke_attrs(buf, stroke, *stroke_width, *linecap, *linejoin, *fill_rule);
            if *opacity < 1.0 {
                push_f_attr(buf, "opacity", *opacity);
            }
            push_transform(buf, transform);
            buf.push_str("/>");
        }
        SvgNode::Animate { .. } | SvgNode::AnimateTransform { .. } | SvgNode::Set { .. } => {}
        SvgNode::Elem {
            tag,
            attrs,
            children,
            text,
        } => {
            buf.push('<');
            buf.push_str(tag);
            for (k, v) in attrs {
                if matches!(k.as_str(), "fill" | "stroke" | "stop-color" | "color") {
                    push_paint_attr(buf, k, Some(v.as_str()));
                } else {
                    push_attr(buf, k, Some(v.as_str()));
                }
            }
            let empty = children.is_empty() && text.as_ref().map(|s| s.is_empty()).unwrap_or(true);
            if empty {
                buf.push_str("/>");
            } else {
                buf.push('>');
                if let Some(t) = text {
                    escape_xml_text(buf, t);
                }
                for c in children {
                    emit_svg_node(buf, c);
                }
                buf.push_str("</");
                buf.push_str(tag);
                buf.push('>');
            }
        }
    }
}

fn escape_xml_text(buf: &mut String, s: &str) {
    for ch in s.chars() {
        match ch {
            '&' => buf.push_str("&amp;"),
            '<' => buf.push_str("&lt;"),
            '>' => buf.push_str("&gt;"),
            '"' => buf.push_str("&quot;"),
            '\'' => buf.push_str("&apos;"),
            c => buf.push(c),
        }
    }
}

/// Whether any descendant `<svg>` leaf uses SMIL (needs per-frame resvg bake).
pub fn subtree_has_svg_smil(el: &crate::core::tree::Element) -> bool {
    if el.get_attr(ATTR_SVG_HAS_SMIL).is_some() {
        return true;
    }
    el.children.iter().any(subtree_has_svg_smil)
}

/// Serialize scene graph to SVG document (preserves curve `d`, defs, gradients).
#[allow(dead_code)] // kept for tests / non-`svg` builds; raster path uses `emit_svg_document_at`
pub(crate) fn emit_svg_document(nodes: &[SvgNode], vb: [f32; 4]) -> String {
    emit_svg_document_at(nodes, vb, None)
}

pub(crate) fn emit_svg_document_at(nodes: &[SvgNode], vb: [f32; 4], smil_t: Option<f32>) -> String {
    let nodes = if let Some(t) = smil_t {
        crate::primitives::svg_smil::apply_smil(nodes, t)
    } else {
        nodes.to_vec()
    };
    let (defs, body) = split_defs_body(&nodes);
    let mut doc = String::new();
    if !defs.is_empty() {
        doc.push_str("<defs>");
        for d in &defs {
            emit_svg_node(&mut doc, d);
        }
        doc.push_str("</defs>");
    }
    for n in &body {
        emit_svg_node(&mut doc, n);
    }
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="{} {} {} {}" color="{}" fill="none">{doc}</svg>"#,
        vb[0],
        vb[1],
        vb[2],
        vb[3],
        foreground_css_color(),
    )
}

/// Theme `foreground` as CSS color for SVG `currentColor` / default stroke (Tailwind `text-foreground`).
fn foreground_css_color() -> String {
    let c = crate::style::parse_text_color("foreground");
    if c[3] == 255 {
        format!("rgb({},{},{})", c[0], c[1], c[2])
    } else {
        format!(
            "rgba({},{},{},{:.4})",
            c[0],
            c[1],
            c[2],
            c[3] as f32 / 255.0
        )
    }
}

#[derive(Clone)]
#[allow(dead_code)] // `nodes`/`vb` read under `feature = "svg"` raster path
struct StoredSvgScene {
    nodes: Vec<SvgNode>,
    vb: [f32; 4],
    has_smil: bool,
}

thread_local! {
    static SVG_SCENE_STORE: RefCell<HashMap<u64, StoredSvgScene>> = RefCell::new(HashMap::new());
}

pub(crate) fn register_svg_scene(nodes: Vec<SvgNode>, vb: [f32; 4], has_smil: bool) -> u64 {
    let mut hasher = DefaultHasher::new();
    for n in &nodes {
        hash_svg_node(&mut hasher, n);
    }
    vb[0].to_bits().hash(&mut hasher);
    vb[1].to_bits().hash(&mut hasher);
    vb[2].to_bits().hash(&mut hasher);
    vb[3].to_bits().hash(&mut hasher);
    has_smil.hash(&mut hasher);
    let id = hasher.finish();
    SVG_SCENE_STORE.with(|s| {
        let mut g = s.borrow_mut();
        if g.len() >= 64 {
            g.clear();
        }
        g.insert(
            id,
            StoredSvgScene {
                nodes,
                vb,
                has_smil,
            },
        );
    });
    id
}

fn hash_svg_node(h: &mut DefaultHasher, node: &SvgNode) {
    use SvgNode::*;
    match node {
        Circle {
            cx,
            cy,
            r,
            fill,
            stroke,
            ..
        } => {
            "circle".hash(h);
            cx.to_bits().hash(h);
            cy.to_bits().hash(h);
            r.to_bits().hash(h);
            fill.hash(h);
            stroke.hash(h);
        }
        Path {
            d, fill, stroke, ..
        } => {
            "path".hash(h);
            d.hash(h);
            fill.hash(h);
            stroke.hash(h);
        }
        Defs { children } | Group { children, .. } => {
            "container".hash(h);
            for c in children {
                hash_svg_node(h, c);
            }
        }
        LinearGradient { id, stops, .. } => {
            "linearGradient".hash(h);
            id.hash(h);
            stops.len().hash(h);
        }
        Use { href, .. } => {
            "use".hash(h);
            href.hash(h);
        }
        Elem {
            tag,
            attrs,
            children,
            text,
        } => {
            "elem".hash(h);
            tag.hash(h);
            for (k, v) in attrs {
                k.hash(h);
                v.hash(h);
            }
            text.hash(h);
            for c in children {
                hash_svg_node(h, c);
            }
        }
        _ => {
            "other".hash(h);
        }
    }
}

/// Logical leaf size → raster pixels. Window DPI (same as gradient textures) plus
/// 2× SSAA so thin strokes (ProgressCircle, chart lines) stay sharp after
/// `scale_textured_draws` stretches logical quads to the swapchain.
fn svg_raster_pixel_size(logical: [f32; 2]) -> (u32, u32) {
    let dpi = crate::style::gradient_raster_dpi().max(1.0);
    let scale = (dpi * 2.0).clamp(2.0, 4.0);
    let w = (logical[0].max(1.0) * scale).round().clamp(2.0, 1024.0) as u32;
    let h = (logical[1].max(1.0) * scale).round().clamp(2.0, 1024.0) as u32;
    (w, h)
}

/// Unified resvg raster path for `<svg>` leaves (SMIL-aware).
pub fn svg_raster_from_element_attrs(
    attrs: &std::collections::HashMap<String, String>,
    leaf_w: f32,
    leaf_h: f32,
) -> Option<(String, f32, f32)> {
    let scene_hex = attrs.get(ATTR_SVG_SCENE)?;
    let id = u64::from_str_radix(scene_hex.trim(), 16).ok()?;
    let scene = SVG_SCENE_STORE.with(|s| s.borrow().get(&id).cloned())?;
    let target = crate::primitives::path::path_target_leaf_size(attrs, [leaf_w, leaf_h]);
    let (w, h) = svg_raster_pixel_size(target);
    let smil_t = scene
        .has_smil
        .then(crate::primitives::svg_smil::smil_time_secs);
    #[cfg(feature = "svg")]
    {
        let doc = emit_svg_document_at(&scene.nodes, scene.vb, smil_t);
        let tex = if scene.has_smil {
            // SMIL: include time bucket in cache key via fresh raster each ~frame
            let frame = (smil_t.unwrap_or(0.0) * 60.0) as u64;
            let mut hasher = DefaultHasher::new();
            doc.hash(&mut hasher);
            w.hash(&mut hasher);
            h.hash(&mut hasher);
            frame.hash(&mut hasher);
            let key = format!("rui_svg_anim:{:016x}", hasher.finish());
            let rgba = crate::media::svg_raster::rasterize_svg_str(&doc, w, h)?;
            crate::app::pending_images::queue_image_rgba8(&key, w, h, rgba);
            key
        } else {
            crate::media::svg_raster::ensure_svg_document_texture(&doc, w, h)?
        };
        return Some((tex, target[0], target[1]));
    }
    #[cfg(not(feature = "svg"))]
    {
        let _ = (scene, smil_t, w, h);
        None
    }
}

#[cfg(test)]
pub(crate) fn emit_svg_document_from_element_attrs(
    attrs: &std::collections::HashMap<String, String>,
) -> Option<String> {
    let scene_hex = attrs.get(ATTR_SVG_SCENE)?;
    let id = u64::from_str_radix(scene_hex.trim(), 16).ok()?;
    let scene = SVG_SCENE_STORE.with(|s| s.borrow().get(&id).cloned())?;
    Some(emit_svg_document_at(&scene.nodes, scene.vb, None))
}

fn attach_svg_scene(
    mut el: Element,
    nodes: &[SvgNode],
    layers: &[(Vec<PathCommand>, PathStyle)],
    vb: [f32; 4],
    view_box_raw: Option<&str>,
) -> Element {
    el.children.clear();
    el.set_attr(ATTR_PATH_FIT_LEAF, "1");
    el.set_attr(
        ATTR_PATH_VIEWBOX,
        format!("{} {} {} {}", vb[0], vb[1], vb[2], vb[3]),
    );
    // Path-kernel fallback for Chart / no-`svg` feature. Defs/use/gradient scenes may
    // compile to zero path layers — still register the resvg scene below.
    if !layers.is_empty() {
        let id = register_path_scene(layers.to_vec(), Vec::new(), 0.0, vb[2], vb[3]);
        el.set_attr(ATTR_PATH_SCENE, format!("{id:016x}"));
        if let Some((cmds, _)) = layers.first() {
            if !cmds.is_empty() {
                el.set_attr(ATTR_PATH_D, encode_path_d(cmds));
            }
        }
    }
    if nodes.is_empty() {
        return el;
    }
    let (dim_w, dim_h) = svg_raster_pixel_size([vb[2].max(16.0), vb[3].max(16.0)]);
    let vb_raw = view_box_raw
        .and_then(parse_view_box)
        .unwrap_or([0.0, 0.0, vb[2], vb[3]]);
    let has_smil = crate::primitives::svg_smil::scene_has_smil(nodes);
    let svg_id = register_svg_scene(nodes.to_vec(), vb_raw, has_smil);
    el.set_attr(ATTR_SVG_SCENE, format!("{svg_id:016x}"));
    if has_smil {
        el.set_attr(ATTR_SVG_HAS_SMIL, "1");
    }
    #[cfg(feature = "svg")]
    {
        let doc = emit_svg_document_at(nodes, vb_raw, None);
        if let Some(tex) = crate::media::svg_raster::ensure_svg_document_texture(&doc, dim_w, dim_h)
        {
            el = el.image_texture(tex);
            return el;
        }
    }
    if !layers.is_empty() {
        if let Some(tex) = ensure_raster_texture(layers, &[], 0.0, dim_w, dim_h, 2) {
            el = el.image_texture(tex);
        }
    }
    el
}

/// Public Svg root.
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

    /// Hit policy on the Svg leaf ([CSS pointer-events](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/pointer-events)).
    /// Defaults in [`From`] to SVG-only `bounding-box`; override with generic `auto`/`none` or other SVG keywords (path-level fill/stroke).
    pub fn pointer_events(mut self, mode: impl Into<crate::core::tree::PointerEvents>) -> Self {
        self.el = self.el.pointer_events(mode);
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
        // Default color token = text-foreground (for currentColor / theme alignment).
        if el.text_style.color == crate::style::TextStyle::default().color {
            el.text_style.color = Some(crate::style::parse_text_color("foreground"));
        }
        // SVG leaf defaults to `bounding-box` (MDN SVG-only: hit the container); callers may override.
        if el
            .attrs
            .get(crate::core::tree::ATTR_POINTER_EVENTS)
            .is_none()
        {
            el = el.pointer_events("bounding-box");
        }
        attach_svg_scene(el, &nodes, &layers, vb, builder.view_box.as_deref())
    }
}

impl IntoElementList for SvgBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

/// Child-shape Builder (`circle` / `ellipse` / `path` / `rect` / `line` / `polyline` / `polygon` / `g`).
pub struct SvgShapeBuilder {
    kind: String,
    attrs: Vec<(String, String)>,
    children: Vec<Element>,
    text: Option<String>,
}

pub type CircleBuilder = SvgShapeBuilder;
pub type EllipseBuilder = SvgShapeBuilder;
pub type PathBuilder = SvgShapeBuilder;
pub type RectBuilder = SvgShapeBuilder;
pub type LineBuilder = SvgShapeBuilder;
pub type PolylineBuilder = SvgShapeBuilder;
pub type PolygonBuilder = SvgShapeBuilder;
pub type GroupBuilder = SvgShapeBuilder;
pub type DefsBuilder = SvgShapeBuilder;
pub type UseBuilder = SvgShapeBuilder;
pub type ClipPathBuilder = SvgShapeBuilder;
pub type LinearGradientBuilder = SvgShapeBuilder;
pub type RadialGradientBuilder = SvgShapeBuilder;
pub type SymbolBuilder = SvgShapeBuilder;
pub type StopBuilder = SvgShapeBuilder;
pub type AnimateBuilder = SvgShapeBuilder;
pub type SetBuilder = SvgShapeBuilder;

fn shape(kind: &str) -> SvgShapeBuilder {
    SvgShapeBuilder {
        kind: kind.to_string(),
        attrs: Vec::new(),
        children: Vec::new(),
        text: None,
    }
}

/// Generic SVG element for MDN tags without a dedicated typed constructor.
pub fn svg_el(tag: impl AsRef<str>) -> SvgShapeBuilder {
    shape(tag.as_ref())
}

pub fn svg_text() -> SvgShapeBuilder {
    shape("text")
}
pub fn tspan() -> SvgShapeBuilder {
    shape("tspan")
}
pub fn text_path() -> SvgShapeBuilder {
    shape("textPath")
}
pub fn image() -> SvgShapeBuilder {
    shape("image")
}
pub fn mask() -> SvgShapeBuilder {
    shape("mask")
}
pub fn pattern() -> SvgShapeBuilder {
    shape("pattern")
}
pub fn marker() -> SvgShapeBuilder {
    shape("marker")
}
pub fn filter() -> SvgShapeBuilder {
    shape("filter")
}
pub fn title() -> SvgShapeBuilder {
    shape("title")
}
pub fn desc() -> SvgShapeBuilder {
    shape("desc")
}
pub fn animate_motion() -> SvgShapeBuilder {
    shape("animateMotion")
}
pub fn mpath() -> SvgShapeBuilder {
    shape("mpath")
}
pub fn fe_gaussian_blur() -> SvgShapeBuilder {
    shape("feGaussianBlur")
}
pub fn fe_drop_shadow() -> SvgShapeBuilder {
    shape("feDropShadow")
}
pub fn fe_offset() -> SvgShapeBuilder {
    shape("feOffset")
}
pub fn fe_blend() -> SvgShapeBuilder {
    shape("feBlend")
}
pub fn fe_color_matrix() -> SvgShapeBuilder {
    shape("feColorMatrix")
}
pub fn fe_flood() -> SvgShapeBuilder {
    shape("feFlood")
}
pub fn fe_merge() -> SvgShapeBuilder {
    shape("feMerge")
}
pub fn fe_merge_node() -> SvgShapeBuilder {
    shape("feMergeNode")
}

pub fn circle() -> CircleBuilder {
    shape("circle")
}
pub fn ellipse() -> EllipseBuilder {
    shape("ellipse")
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
pub fn polygon() -> PolygonBuilder {
    shape("polygon")
}
pub fn g() -> GroupBuilder {
    shape("g")
}
pub fn defs() -> DefsBuilder {
    shape("defs")
}
pub fn svg_use() -> UseBuilder {
    shape("use")
}
pub fn clip_path() -> ClipPathBuilder {
    shape("clipPath")
}
pub fn linear_gradient() -> LinearGradientBuilder {
    shape("linearGradient")
}
pub fn radial_gradient() -> RadialGradientBuilder {
    shape("radialGradient")
}
pub fn symbol() -> SymbolBuilder {
    shape("symbol")
}
pub fn stop() -> StopBuilder {
    shape("stop")
}
pub fn animate() -> AnimateBuilder {
    shape("animate")
}
pub fn animate_transform() -> AnimateBuilder {
    shape("animateTransform")
}
pub fn set_el() -> SetBuilder {
    shape("set")
}

impl SvgShapeBuilder {
    fn set(mut self, key: &str, value: impl Into<String>) -> Self {
        let key = key.to_string();
        let v = value.into();
        if let Some((_, slot)) = self.attrs.iter_mut().find(|(k, _)| *k == key) {
            *slot = v;
        } else {
            self.attrs.push((key, v));
        }
        self
    }

    /// Arbitrary SVG attribute (MDN long-tail); emitted as-is for resvg.
    pub fn attr(self, name: impl AsRef<str>, value: impl Into<String>) -> Self {
        self.set(name.as_ref(), value)
    }

    /// Character data for text-bearing elements (`<text>`, `<title>`, …).
    pub fn text_content(mut self, s: impl Into<String>) -> Self {
        self.text = Some(s.into());
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
    pub fn rx(self, v: impl IntoSvgNum) -> Self {
        self.set("rx", v.into_svg_num().to_string())
    }
    pub fn ry(self, v: impl IntoSvgNum) -> Self {
        self.set("ry", v.into_svg_num().to_string())
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
    pub fn x1(self, v: impl IntoSvgCoord) -> Self {
        self.set("x1", v.into_svg_coord())
    }
    pub fn y1(self, v: impl IntoSvgCoord) -> Self {
        self.set("y1", v.into_svg_coord())
    }
    pub fn x2(self, v: impl IntoSvgCoord) -> Self {
        self.set("x2", v.into_svg_coord())
    }
    pub fn y2(self, v: impl IntoSvgCoord) -> Self {
        self.set("y2", v.into_svg_coord())
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
    pub fn stroke_linecap(self, c: impl AsRef<str>) -> Self {
        self.set("stroke-linecap", c.as_ref())
    }
    pub fn stroke_linejoin(self, j: impl AsRef<str>) -> Self {
        self.set("stroke-linejoin", j.as_ref())
    }
    pub fn fill_rule(self, r: impl AsRef<str>) -> Self {
        self.set("fill-rule", r.as_ref())
    }
    pub fn fill_opacity(self, o: impl IntoSvgNum) -> Self {
        self.set("fill-opacity", o.into_svg_num().to_string())
    }
    pub fn stroke_opacity(self, o: impl IntoSvgNum) -> Self {
        self.set("stroke-opacity", o.into_svg_num().to_string())
    }
    pub fn stroke_dasharray(self, v: impl Into<String>) -> Self {
        self.set("stroke-dasharray", v)
    }
    pub fn font_size(self, v: impl IntoSvgNum) -> Self {
        self.set("font-size", v.into_svg_num().to_string())
    }
    pub fn text_anchor(self, v: impl AsRef<str>) -> Self {
        self.set("text-anchor", v.as_ref())
    }
    pub fn mask_attr(self, v: impl Into<String>) -> Self {
        self.set("mask", v)
    }
    pub fn filter_attr(self, v: impl Into<String>) -> Self {
        self.set("filter", v)
    }
    pub fn opacity(self, o: impl IntoSvgNum) -> Self {
        self.set("opacity", o.into_svg_num().to_string())
    }
    pub fn transform(self, t: impl Into<String>) -> Self {
        self.set("transform", t)
    }
    pub fn id(self, id: impl Into<String>) -> Self {
        self.set("id", id)
    }
    pub fn clip_path(self, v: impl Into<String>) -> Self {
        self.set("clip-path", v)
    }
    pub fn href(self, v: impl Into<String>) -> Self {
        self.set("href", v)
    }
    pub fn offset(self, v: impl Into<String>) -> Self {
        self.set("offset", v)
    }
    pub fn stop_color(self, v: impl Into<String>) -> Self {
        self.set("stop-color", v)
    }
    pub fn stop_opacity(self, v: impl IntoSvgNum) -> Self {
        self.set("stop-opacity", v.into_svg_num().to_string())
    }
    pub fn gradient_units(self, v: impl Into<String>) -> Self {
        self.set("gradientUnits", v)
    }
    pub fn attribute_name(self, v: impl Into<String>) -> Self {
        self.set("attributeName", v)
    }
    pub fn transform_type(self, v: impl Into<String>) -> Self {
        self.set("type", v)
    }
    pub fn from_val(self, v: impl Into<String>) -> Self {
        self.set("from", v)
    }
    pub fn to_val(self, v: impl Into<String>) -> Self {
        self.set("to", v)
    }
    pub fn dur(self, v: impl Into<String>) -> Self {
        self.set("dur", v)
    }
    pub fn repeat_count(self, v: impl Into<String>) -> Self {
        self.set("repeatCount", v)
    }
    pub fn begin(self, v: impl Into<String>) -> Self {
        self.set("begin", v)
    }
    pub fn anim_type(self, v: impl Into<String>) -> Self {
        self.set("type", v)
    }
    pub fn view_box(self, vb: impl AsRef<str>) -> Self {
        self.set("viewBox", vb.as_ref().replace(',', " "))
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
        if let Some(t) = b.text {
            el.set_attr(ATTR_SVG_TEXT, t);
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

    #[test]
    fn svg_raster_pixel_size_is_at_least_2x() {
        crate::style::set_gradient_raster_dpi(1.0);
        assert_eq!(svg_raster_pixel_size([64.0, 64.0]), (128, 128));
        crate::style::set_gradient_raster_dpi(1.5);
        let (w, h) = svg_raster_pixel_size([64.0, 64.0]);
        assert!(w >= 192 && h >= 192, "1.5 dpi + 2× SSAA, got {w}×{h}");
        crate::style::set_gradient_raster_dpi(1.0);
    }

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

    #[test]
    fn defs_gradient_clip_use_emit_document() {
        let builder = svg()
            .view_box("0 0 100 100")
            .child(
                defs()
                    .child(
                        linear_gradient()
                            .id("g1")
                            .x1("0%")
                            .y1("0%")
                            .x2("100%")
                            .y2("0%")
                            .child(stop().offset("0%").stop_color("#2563eb"))
                            .child(stop().offset("100%").stop_color("#dc2626")),
                    )
                    .child(
                        clip_path()
                            .id("clip1")
                            .child(rect().x(10).y(10).w(80).h(80)),
                    )
                    .child(
                        symbol()
                            .id("icon")
                            .view_box("0 0 24 24")
                            .child(circle().cx(12).cy(12).r(10).fill("url(#g1)")),
                    ),
            )
            .child(
                svg_use()
                    .href("#icon")
                    .x(0)
                    .y(0)
                    .w(100)
                    .h(100)
                    .clip_path("url(#clip1)"),
            );
        let nodes: Vec<SvgNode> = builder
            .el
            .children
            .iter()
            .filter_map(node_from_element)
            .collect();
        let el: Element = builder.into();
        assert!(el.get_attr(super::ATTR_SVG_SCENE).is_some());
        let out = emit_svg_document(&nodes, [0.0, 0.0, 100.0, 100.0]);
        assert!(out.contains("<linearGradient"), "{out}");
        assert!(out.contains("<clipPath"), "{out}");
        assert!(out.contains("<use"), "{out}");
        assert!(out.contains("url(#g1)"), "{out}");
    }

    #[test]
    fn smil_rotate_bakes_transform_at_time() {
        let g_el: Element = g()
            .child(
                animate_transform()
                    .attribute_name("transform")
                    .transform_type("rotate")
                    .from_val("0 50 50")
                    .to_val("90 50 50")
                    .dur("1s")
                    .repeat_count("indefinite"),
            )
            .child(circle().cx(50).cy(50).r(20).fill("#111111"))
            .into();
        let node = node_from_element(&g_el).unwrap();
        assert!(crate::primitives::svg_smil::node_has_smil(&node));
        let baked = crate::primitives::svg_smil::apply_smil(&[node], 0.25);
        let doc = emit_svg_document_at(&baked, [0.0, 0.0, 100.0, 100.0], None);
        assert!(
            doc.contains("rotate(22.5") || doc.contains("rotate(22.5 "),
            "{doc}"
        );
    }

    #[test]
    fn smil_animate_r_bakes_on_circle() {
        let el: Element = circle()
            .cx(40)
            .cy(40)
            .r(10)
            .fill("#111111")
            .child(
                animate()
                    .attribute_name("r")
                    .from_val("10")
                    .to_val("30")
                    .dur("1s")
                    .repeat_count("1"),
            )
            .into();
        let node = node_from_element(&el).unwrap();
        assert!(crate::primitives::svg_smil::node_has_smil(&node));
        let baked = crate::primitives::svg_smil::apply_smil(&[node], 0.5);
        let (layers, _) = compile_layers(&baked, Some("0 0 80 80"));
        assert!(!layers.is_empty());
        let pts = points_from_commands(&layers[0].0);
        let n = pts.len().max(1) as f32;
        let mean_r = pts
            .iter()
            .map(|(x, y)| (x - 40.0).hypot(y - 40.0))
            .sum::<f32>()
            / n;
        assert!(
            (mean_r - 20.0).abs() < 2.0,
            "r should lerp to ~20 at t=0.5, got {mean_r}"
        );
    }

    #[test]
    fn smil_set_opacity_bakes_on_group() {
        let el: Element = g()
            .child(set_el().attribute_name("opacity").to_val("0.25"))
            .child(circle().cx(10).cy(10).r(5).fill("#ffffff"))
            .into();
        let node = node_from_element(&el).unwrap();
        let baked = crate::primitives::svg_smil::apply_smil(&[node], 0.0);
        match &baked[0] {
            SvgNode::Group { opacity, .. } => {
                assert!((*opacity - 0.25).abs() < 1e-3, "opacity={opacity}");
            }
            other => panic!("expected group, got {other:?}"),
        }
    }

    #[test]
    fn cpu_path_resolves_url_linear_gradient_fill() {
        let nodes = vec![
            node_from_element(
                &defs()
                    .child(
                        linear_gradient()
                            .id("g")
                            .x1("0%")
                            .y1("0%")
                            .x2("100%")
                            .y2("0%")
                            .child(stop().offset("0%").stop_color("#0000ff"))
                            .child(stop().offset("100%").stop_color("#ff0000")),
                    )
                    .into(),
            )
            .unwrap(),
            node_from_element(&rect().x(0).y(0).w(64).h(64).fill("url(#g)").into()).unwrap(),
        ];
        let (layers, _) = compile_layers(&nodes, Some("0 0 64 64"));
        assert_eq!(layers.len(), 1);
        assert!(
            layers[0].1.fill_gradient.is_some(),
            "url(#g) must resolve to fill_gradient"
        );
        let rgba = rasterize_layers_rgba(&layers, &[], 0.0, 64, 64, 1);
        let px = |x: u32, y: u32| {
            let i = ((y * 64 + x) * 4) as usize;
            (rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3])
        };
        let (lr, _, lb, la) = px(4, 32);
        let (rr, _, _, ra) = px(60, 32);
        assert!(la > 180 && lb > lr + 40, "left blue, got {lr},{lb},{la}");
        assert!(ra > 180 && rr > lb, "right red, got {rr},{ra}");
    }

    #[cfg(feature = "svg")]
    #[test]
    fn resvg_rasterizes_linear_gradient_fill() {
        let builder = svg()
            .view_box("0 0 64 64")
            .child(
                defs().child(
                    linear_gradient()
                        .id("g")
                        .x1("0%")
                        .y1("0%")
                        .x2("100%")
                        .y2("0%")
                        .child(stop().offset("0%").stop_color("#0000ff"))
                        .child(stop().offset("100%").stop_color("#ff0000")),
                ),
            )
            .child(rect().x(0).y(0).w(64).h(64).fill("url(#g)"));
        let nodes: Vec<SvgNode> = builder
            .el
            .children
            .iter()
            .filter_map(node_from_element)
            .collect();
        assert!(!nodes.is_empty(), "gradient scene must parse to SvgNode");
        let el: Element = builder.into();
        assert!(el.get_attr(ATTR_SVG_SCENE).is_some());
        let doc = emit_svg_document(&nodes, [0.0, 0.0, 64.0, 64.0]);
        assert!(doc.contains("linearGradient"), "{doc}");
        let rgba = crate::media::svg_raster::rasterize_svg_str(&doc, 64, 64).expect("resvg");
        let px = |x: u32, y: u32| {
            let i = ((y * 64 + x) * 4) as usize;
            (rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3])
        };
        let (lr, _, lb, la) = px(4, 32);
        let (rr, _, _, ra) = px(60, 32);
        assert!(
            la > 180 && lb > lr + 40,
            "left should be blue, got {lr},{lb},{la}"
        );
        assert!(ra > 180 && rr > lb, "right should be red, got {rr},{ra}");
    }

    #[test]
    fn elem_text_tspan_emits_font_size() {
        let nodes = vec![node_from_element(
            &svg_text()
                .attr("x", "10")
                .attr("y", "20")
                .font_size(14.0)
                .text_content("Hello ")
                .child(tspan().font_size(18.0).text_content("World"))
                .into(),
        )
        .unwrap()];
        let doc = emit_svg_document(&nodes, [0.0, 0.0, 100.0, 40.0]);
        assert!(doc.contains("<text"), "{doc}");
        assert!(doc.contains("font-size"), "{doc}");
        assert!(doc.contains("Hello "), "{doc}");
        assert!(doc.contains("<tspan"), "{doc}");
        assert!(doc.contains("World"), "{doc}");
    }

    #[test]
    fn filter_fe_gaussian_nested_in_defs() {
        let nodes = vec![
            node_from_element(
                &filter()
                    .id("blur")
                    .child(
                        fe_gaussian_blur()
                            .attr("in", "SourceGraphic")
                            .attr("stdDeviation", "2"),
                    )
                    .into(),
            )
            .unwrap(),
            node_from_element(
                &circle()
                    .cx(32)
                    .cy(32)
                    .r(20)
                    .fill("#2563eb")
                    .filter_attr("url(#blur)")
                    .into(),
            )
            .unwrap(),
        ];
        assert!(is_defs_only_node(&nodes[0]));
        let doc = emit_svg_document(&nodes, [0.0, 0.0, 64.0, 64.0]);
        assert!(doc.contains("<defs>"), "{doc}");
        assert!(doc.contains("<filter"), "{doc}");
        assert!(doc.contains("feGaussianBlur"), "{doc}");
        assert!(doc.contains("filter=\"url(#blur)\""), "{doc}");
    }

    #[cfg(feature = "svg")]
    #[test]
    fn resvg_rasterizes_filter_blur_scene() {
        let builder = svg()
            .view_box("0 0 64 64")
            .child(
                filter().id("blur").child(
                    fe_gaussian_blur()
                        .attr("in", "SourceGraphic")
                        .attr("stdDeviation", "1.5"),
                ),
            )
            .child(
                circle()
                    .cx(32)
                    .cy(32)
                    .r(18)
                    .fill("#2563eb")
                    .filter_attr("url(#blur)"),
            );
        let nodes: Vec<SvgNode> = builder
            .el
            .children
            .iter()
            .filter_map(node_from_element)
            .collect();
        let el: Element = builder.into();
        assert!(el.get_attr(ATTR_SVG_SCENE).is_some());
        let doc = emit_svg_document(&nodes, [0.0, 0.0, 64.0, 64.0]);
        let rgba = crate::media::svg_raster::rasterize_svg_str(&doc, 64, 64).expect("resvg");
        let nonempty = rgba.chunks(4).any(|px| px[3] > 20);
        assert!(nonempty, "filter scene should rasterize non-empty pixels");
    }

    #[test]
    fn scene_has_smil_detects_elem_animate_motion() {
        let n = node_from_element(
            &animate_motion()
                .attr("path", "M0 0 L10 10")
                .attr("dur", "1s")
                .into(),
        )
        .unwrap();
        assert!(crate::primitives::svg_smil::node_has_smil(&n));
    }

    #[test]
    fn pointer_events_fill_hits_interior_not_box_corner() {
        use crate::app::pointer_events::hit_path;
        use crate::core::tree::PointerEvents;

        let mut el: Element = svg()
            .view_box("0 0 100 100")
            .w(100.0)
            .h(100.0)
            .pointer_events("fill")
            .child(circle().cx(50.0).cy(50.0).r(20.0).fill("#f00"))
            .into();
        el.layout_bounds = [0.0, 0.0, 100.0, 100.0];
        assert_eq!(el.pointer_events_mode(), PointerEvents::Fill);
        assert!(crate::primitives::path::element_has_path_geometry(
            &el.attrs
        ));

        let center = hit_path(&el, 50.0, 50.0);
        assert!(!center.is_empty(), "fill mode must hit circle interior");
        let corner = hit_path(&el, 95.0, 95.0);
        assert!(
            corner.is_empty(),
            "fill mode must miss outside the circle (inside leaf box)"
        );
    }

    #[test]
    fn pointer_events_fill_hits_origin_left_rotate_90_painted_center() {
        use crate::app::pointer_events::hit_path;

        let mut el: Element = svg()
            .view_box("0 0 100 100")
            .w(100.0)
            .h(100.0)
            .pointer_events("fill")
            .child(circle().cx(50.0).cy(50.0).r(20.0).fill("#f00"))
            .into();
        el.layout_bounds = [400.0, 200.0, 100.0, 100.0];
        el.style.rotate_deg = 90.0;
        el.style.transform_origin = crate::style::parse_transform_origin("left top");
        assert!(
            !hit_path(&el, 350.0, 250.0).is_empty(),
            "fill must hit the painted circle after origin-left 90°"
        );
        assert!(
            hit_path(&el, 450.0, 250.0).is_empty(),
            "old layout center must miss the rotated circle"
        );
    }

    #[test]
    fn pointer_events_stroke_hits_perimeter_not_center() {
        use crate::app::pointer_events::hit_path;

        let mut el: Element = svg()
            .view_box("0 0 100 100")
            .w(100.0)
            .h(100.0)
            .pointer_events("stroke")
            .child(
                circle()
                    .cx(50.0)
                    .cy(50.0)
                    .r(30.0)
                    .fill("none")
                    .stroke("#0f0")
                    .stroke_width(6.0),
            )
            .into();
        el.layout_bounds = [0.0, 0.0, 100.0, 100.0];

        let rim = hit_path(&el, 50.0, 20.0); // top of circle
        assert!(!rim.is_empty(), "stroke mode must hit perimeter");
        let center = hit_path(&el, 50.0, 50.0);
        assert!(center.is_empty(), "stroke mode must miss hollow interior");
    }

    #[test]
    fn pointer_events_bounding_box_hits_empty_corner() {
        use crate::app::pointer_events::hit_path;

        let mut el: Element = svg()
            .view_box("0 0 100 100")
            .w(100.0)
            .h(100.0)
            .pointer_events("bounding-box")
            .child(circle().cx(50.0).cy(50.0).r(10.0).fill("#00f"))
            .into();
        el.layout_bounds = [0.0, 0.0, 100.0, 100.0];

        let corner = hit_path(&el, 2.0, 2.0);
        assert!(!corner.is_empty(), "bounding-box must hit entire leaf");
    }

    #[test]
    fn pointer_events_none_misses_everywhere() {
        use crate::app::pointer_events::hit_path;

        let mut el: Element = svg()
            .view_box("0 0 100 100")
            .w(100.0)
            .h(100.0)
            .pointer_events("none")
            .child(circle().cx(50.0).cy(50.0).r(40.0).fill("#00f"))
            .into();
        el.layout_bounds = [0.0, 0.0, 100.0, 100.0];
        assert!(hit_path(&el, 50.0, 50.0).is_empty());
    }

    #[test]
    fn pointer_events_auto_on_svg_equals_visible_painted() {
        use crate::app::pointer_events::hit_path;

        // auto on SVG leaf → visiblePainted: fill painted ⇒ interior hits; empty corner misses.
        let mut el: Element = svg()
            .view_box("0 0 100 100")
            .w(100.0)
            .h(100.0)
            .pointer_events("auto")
            .child(circle().cx(50.0).cy(50.0).r(20.0).fill("#f00"))
            .into();
        el.layout_bounds = [0.0, 0.0, 100.0, 100.0];
        assert!(!hit_path(&el, 50.0, 50.0).is_empty());
        assert!(hit_path(&el, 95.0, 95.0).is_empty());
    }
}
