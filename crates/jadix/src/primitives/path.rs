//! # Path primitives（WP-3.2 / WP-3.11 共用核）
//!
//! 折线 / 多边形 / 扇区 → [`PathCommand`]；编码为 `jadix_path_d`。
//! [`decode_path_d`] 接受常用 HTML/SVG `d`（含 C/Q/A 等），**保留曲线**；
//! [`flatten_path`] 只在光栅 / tessellate / 命中边界按像素误差展平。
//! 产品绘制：**CPU 覆盖率光栅**是 golden / `JADIX_GPU_2D=0` 回退；默认走 PathPipeline
//! 描边 mesh + stencil-cover fill + 4× MSAA（S3+S4）→ `jadix_path:`。
//! tessellate 四边形不再进 `rounded_rect.wgsl`。
//! 绘制核由 jadix_2d 提供；本模块保留 Element、缓存及提交调度适配。

use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use crate::core::tree::Element;

pub use jadix_2d::path::*;

/// Attr：SVG 子集 path `d`（与 WP-3.11 `<path d>` 同形）。
pub const ATTR_PATH_D: &str = "jadix_path_d";
/// Attr：填充色 `r,g,b,a`（0..=1）；缺省不填。
pub const ATTR_PATH_FILL: &str = "jadix_path_fill";
/// Attr：描边色 `r,g,b,a`。
pub const ATTR_PATH_STROKE: &str = "jadix_path_stroke";
/// Attr：描边宽（逻辑 px）。
pub const ATTR_PATH_STROKE_WIDTH: &str = "jadix_path_stroke_width";
/// Attr：`butt` / `round` / `square`（默认 round，对齐 Lucide）。
pub const ATTR_PATH_STROKE_LINECAP: &str = "jadix_path_stroke_linecap";
/// Attr：`miter` / `round` / `bevel`（默认 round）。
pub const ATTR_PATH_STROKE_LINEJOIN: &str = "jadix_path_stroke_linejoin";
pub const ATTR_PATH_STROKE_DASHARRAY: &str = "jadix_path_stroke_dasharray";
pub const ATTR_PATH_STROKE_DASHOFFSET: &str = "jadix_path_stroke_dashoffset";
pub const ATTR_PATH_STROKE_MITERLIMIT: &str = "jadix_path_stroke_miterlimit";
/// Attr：`nonzero` / `evenodd`（填充规则；光栅现为 even-odd 扫描，evenodd 显式记录）。
pub const ATTR_PATH_FILL_RULE: &str = "jadix_path_fill_rule";
/// 旧折线 kind（仍可读）。
pub const ATTR_PATH_KIND: &str = "jadix_path_kind";
/// 旧点列 `x,y;x,y;…`（仍可读，写入时同时写 [`ATTR_PATH_D`]）。
pub const ATTR_PATH_POINTS: &str = "jadix_path_points";
/// 可选 viewBox：`minx miny w h`。
///
/// - 轴 span `< 2`：归一化，缩放到叶对应边（宽常为 `1`）。
/// - 轴 span `>= 2`：视为逻辑 px，绘制尺寸取 `min(叶边, span)`，**不**跟被撑高的叶走。
/// - [`ATTR_PATH_FIT_LEAF`]`=1`：两端都跟叶（公开 `<svg>`，HTML viewBox 拉伸）。
pub const ATTR_PATH_VIEWBOX: &str = "jadix_path_viewbox";
/// 为 `1` 时 viewBox 两端都映射到叶尺寸（WP-3.11 Svg）；Chart 不写此 attr。
pub const ATTR_PATH_FIT_LEAF: &str = "jadix_path_fit_leaf";
/// 可选标记点（viewBox / 局部坐标，`x,y;x,y`），光栅时画实心圆。
pub const ATTR_PATH_MARKERS: &str = "jadix_path_markers";
/// 多层 scene 键（Chart 等）；布局期按叶宽光栅，避免 `image_texture` 低分拉伸 + `d` 默认描边叠画。
pub const ATTR_PATH_SCENE: &str = "jadix_path_scene";

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
        stroke_options: StrokeOptions {
            dash_array: attrs
                .get(ATTR_PATH_STROKE_DASHARRAY)
                .map(|s| s.split([',', ' ']).filter_map(|v| v.parse().ok()).collect())
                .unwrap_or_default(),
            dash_offset: attrs
                .get(ATTR_PATH_STROKE_DASHOFFSET)
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.0),
            miter_limit: attrs
                .get(ATTR_PATH_STROKE_MITERLIMIT)
                .and_then(|s| s.parse().ok())
                .unwrap_or(4.0),
        },
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
        el.set_attr(
            ATTR_PATH_STROKE_DASHARRAY,
            style
                .stroke_options
                .dash_array
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(","),
        );
        el.set_attr(
            ATTR_PATH_STROKE_DASHOFFSET,
            style.stroke_options.dash_offset.to_string(),
        );
        el.set_attr(
            ATTR_PATH_STROKE_MITERLIMIT,
            style.stroke_options.miter_limit.to_string(),
        );
    } else {
        el.attrs.remove(ATTR_PATH_STROKE);
        el.attrs.remove(ATTR_PATH_STROKE_WIDTH);
        el.attrs.remove(ATTR_PATH_STROKE_LINECAP);
        el.attrs.remove(ATTR_PATH_STROKE_LINEJOIN);
        el.attrs.remove(ATTR_PATH_STROKE_DASHARRAY);
        el.attrs.remove(ATTR_PATH_STROKE_DASHOFFSET);
        el.attrs.remove(ATTR_PATH_STROKE_MITERLIMIT);
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

fn build_gpu_ops(
    layers: &[(Vec<PathCommand>, PathStyle)],
    markers: &[Point2],
    marker_r: f32,
) -> Vec<PathGpuOp> {
    #[cfg(test)]
    GPU_OP_BUILDS.with(|count| count.set(count.get() + 1));
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
                mask: None,
            }));
        }
    }
    ops
}

#[cfg(test)]
thread_local! {
    static GPU_OP_BUILDS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn gpu_op_build_count() -> usize {
    GPU_OP_BUILDS.with(std::cell::Cell::get)
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
}

/// GPU PathPipeline 作业：布局收集排队，渲染期画到 `jadix_path:` / `jadix_canvas:`。
#[derive(Clone, Debug)]
pub struct PathGpuJob {
    pub key: String,
    pub width: u32,
    pub height: u32,
    pub(crate) ops: PathGpuOps,
    /// Canvas keys are id-stable; skip-if-same-size would freeze `request_frame`.
    pub force_redraw: bool,
}

/// Keep canonical path geometry until the renderer knows its texture is missing.
/// Canvas already records GPU operations and still submits them for every repaint.
#[derive(Clone, Debug)]
pub(crate) enum PathGpuOps {
    Ready(Vec<PathGpuOp>),
    Path(PreparedPathDraw),
}

impl PathGpuOps {
    pub(crate) fn into_ops(self) -> Vec<PathGpuOp> {
        match self {
            Self::Ready(ops) => ops,
            Self::Path(path) => build_gpu_ops(&path.layers, &path.markers, path.marker_r),
        }
    }
}

pub(crate) fn queue_path_job(job: PathGpuJob) {
    PATH_JOBS.with(|j| j.borrow_mut().push(job));
}

pub(crate) fn take_path_jobs() -> Vec<PathGpuJob> {
    PATH_JOBS.with(|j| std::mem::take(&mut *j.borrow_mut()))
}

pub(crate) fn take_path_jobs_matching(mut owns: impl FnMut(&str) -> bool) -> Vec<PathGpuJob> {
    PATH_JOBS.with(|jobs| {
        let mut jobs = jobs.borrow_mut();
        let (selected, remaining) = std::mem::take(&mut *jobs)
            .into_iter()
            .partition(|job| owns(&job.key));
        *jobs = remaining;
        selected
    })
}

pub(crate) fn has_path_jobs() -> bool {
    PATH_JOBS.with(|j| !j.borrow().is_empty())
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
        if !policy.require_fill_painted
            && !fill_painted
            && path_contains_point(cmds, p, style.fill_rule)
        {
            return true;
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
    style.stroke_options.miter_limit.to_bits().hash(h);
    style.stroke_options.dash_offset.to_bits().hash(h);
    style.stroke_options.dash_array.len().hash(h);
    for dash in &style.stroke_options.dash_array {
        dash.to_bits().hash(h);
    }
    (style.stroke_linecap as u8).hash(h);
    (style.stroke_linejoin as u8).hash(h);
    (style.fill_rule as u8).hash(h);
}

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
    let tex_id = format!("jadix_path:{key:016x}");
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
    let key = format!("jadix_path:{:016x}", hasher.finish());
    let target = prepared.target;
    queue_path_job(PathGpuJob {
        key: key.clone(),
        width: prepared.width,
        height: prepared.height,
        ops: PathGpuOps::Path(prepared),
        force_redraw: false,
    });
    Some((key, target[0], target[1]))
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedPathDraw {
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
                                style.stroke_options.dash_offset *= dpi;
                                for dash in &mut style.stroke_options.dash_array {
                                    *dash *= dpi;
                                }
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
    style.stroke_options.dash_offset *= dpi;
    for dash in &mut style.stroke_options.dash_array {
        *dash *= dpi;
    }
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
        .get("jadix_path_marker_r")
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
#[path = "tests/path.rs"]
mod tests;
