//! Standalone immediate 2D canvas with CPU pixels and GPU operation recording.
pub use crate::blend::{BlendMode, CompositeMode, LayerOptions};
use crate::blur::BlurredRect;
use crate::gradient::Gradient;
use crate::path::*;
pub use crate::{AtlasError, AtlasLimits, AtlasRegion, ImageAtlas};
pub use crate::{
    Brush2D, ExtendMode, Font2D, FontError, FontMetrics2D, GlyphRun, GradientAlpha, GradientError,
    GradientStops, Image2D, ImageSampling, PositionedGlyph, VectorGradient,
};
pub use crate::{LayerFilter, Morphology};
#[path = "scene.rs"]
mod scene;
#[cfg(feature = "text-layout")]
pub use crate::{PreparedText2D, TextLayoutError, TextPrepareOptions, TextRenderMode};
use jadix_theme::color::parse_color;
use jadix_theme::color_space::{
    encode_linear_srgb_component as encode_srgb, linearize_display_srgb_component as decode_srgb,
};
pub use scene::{
    SampledScene2D, Scene2D, SceneError, SceneLimits, SceneOptions, SceneSampling, SceneStats,
};
fn linear_color(mut color: [f32; 4]) -> [f32; 4] {
    for v in &mut color[..3] {
        *v = decode_srgb(*v);
    }
    color
}

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

/// Immutable resources or borrowed straight-alpha RGBA pixels for CPU/GPU drawing.
/// TextureKey is reserved; host texture-key resolution is not implemented.
pub enum CanvasImageSource<'a> {
    Image(&'a Image2D),
    /// Integer source texels of an immutable image. Must be nonempty and fully
    /// inside the image. Filtering may sample neighboring texels; atlas gutters
    /// provide transparent borders without extracting/reuploading glyph images.
    Region {
        image: &'a Image2D,
        rect: [u32; 4],
    },
    Rgba {
        width: u32,
        height: u32,
        rgba: &'a [u8],
    },
    TextureKey(&'a str),
}
impl CanvasImageSource<'_> {
    fn region(&self) -> Option<[u32; 4]> {
        match self {
            Self::Region { rect, .. } => Some(*rect),
            _ => None,
        }
    }
    fn valid_region(&self) -> bool {
        match self {
            Self::Region {
                image,
                rect: [x, y, w, h],
            } => {
                *w > 0
                    && *h > 0
                    && x.checked_add(*w).is_some_and(|v| v <= image.width())
                    && y.checked_add(*h).is_some_and(|v| v <= image.height())
            }
            _ => true,
        }
    }
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
        self.cmds.push(PathCommand::QuadTo {
            ctrl: (cx, cy),
            to: (x, y),
        });
    }

    pub fn bezier_curve_to(&mut self, c1x: f32, c1y: f32, c2x: f32, c2y: f32, x: f32, y: f32) {
        self.cmds.push(PathCommand::CubicTo {
            c1: (c1x, c1y),
            c2: (c2x, c2y),
            to: (x, y),
        });
    }

    pub fn arc(&mut self, cx: f32, cy: f32, r: f32, start_rad: f32, end_rad: f32) {
        let r = r.max(0.0);
        let start = (cx + r * start_rad.cos(), cy + r * start_rad.sin());
        let end = (cx + r * end_rad.cos(), cy + r * end_rad.sin());
        if self.cmds.is_empty() {
            self.cmds.push(PathCommand::MoveTo(start));
        } else if self
            .cmds
            .iter()
            .rev()
            .find_map(PathCommand::pen)
            .is_none_or(|p| (p.0 - start.0).abs() > 1e-4 || (p.1 - start.1).abs() > 1e-4)
        {
            self.cmds.push(PathCommand::LineTo(start));
        }
        let span = end_rad - start_rad;
        if r < 1e-6 || span.abs() < 1e-6 {
            if self.cmds.iter().rev().find_map(PathCommand::pen) != Some(end) {
                self.cmds.push(PathCommand::LineTo(end));
            }
            return;
        }
        // SVG endpoint arcs cannot represent a full circle with coincident endpoints.
        let span = span.clamp(-std::f32::consts::TAU, std::f32::consts::TAU);
        let pieces = (span.abs() / std::f32::consts::PI).ceil().max(1.0) as u32;
        for i in 1..=pieces {
            let angle = start_rad + span * i as f32 / pieces as f32;
            self.cmds.push(PathCommand::Arc {
                radii: (r, r),
                x_rot_deg: 0.0,
                large: false,
                sweep: span > 0.0,
                to: (cx + r * angle.cos(), cy + r * angle.sin()),
            });
        }
    }

    pub fn close_path(&mut self) {
        self.cmds.push(PathCommand::Close);
    }

    pub fn commands(&self) -> &[PathCommand] {
        &self.cmds
    }
}

/// Immediate 2D context with gradients, path clips and isolated compositing layers.
pub trait Canvas2D {
    fn size(&self) -> CanvasSize;
    /// Optional state introspection used to append reusable scenes. Custom
    /// canvases returning None reject scenes before any drawing/state mutation.
    fn current_transform(&self) -> Option<[f32; 6]> {
        None
    }
    fn current_global_alpha(&self) -> Option<f32> {
        None
    }
    /// Append a frozen scene, preserving caller state. The default replays
    /// ordinary drawing methods; native recording retains the shared scene.
    fn draw_scene(&mut self, scene: &Scene2D, options: SceneOptions) -> Result<(), SceneError> {
        scene.replay(self, options)
    }
    /// Prepared scalable COLRv0/v1 paint graphs; palette/variation resolution
    /// happens during ColorGlyphRun construction, not while drawing.
    fn fill_color_glyph_run(&mut self, run: &crate::ColorGlyphRun) -> Result<(), SceneError> {
        run.draw(self)
    }
    fn set_fill(&mut self, color: &str);
    fn set_stroke(&mut self, color: &str);
    /// Replace the fill paint with an explicit brush-local domain and affine.
    /// The current canvas transform is composed with the brush when drawing.
    fn set_fill_brush(&mut self, brush: Brush2D);
    /// Use an explicit brush on the stroke outline, including caps/joins/dashes.
    fn set_stroke_brush(&mut self, brush: Brush2D);
    /// Gradient coordinates use the transformed path's bounding box.
    fn set_fill_gradient(&mut self, _gradient: Gradient) {}
    /// Gradient coordinates use the transformed stroke outline bounding box.
    fn set_stroke_gradient(&mut self, gradient: Gradient);
    fn set_global_alpha(&mut self, _alpha: f32) {}
    fn set_fill_rule(&mut self, _rule: FillRule) {}
    /// Postmultiply the current affine matrix `[a,b,c,d,e,f]`.
    fn transform(&mut self, _matrix: [f32; 6]) {}
    fn set_transform(&mut self, _matrix: [f32; 6]) {}
    fn reset_transform(&mut self) {
        self.set_transform([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    }
    fn set_line_width(&mut self, width: f32);
    fn set_line_cap(&mut self, _cap: LineCap) {}
    fn set_line_join(&mut self, _join: LineJoin) {}
    fn set_line_dash(&mut self, _pattern: &[f32]) {}
    fn set_line_dash_offset(&mut self, _offset: f32) {}
    fn set_miter_limit(&mut self, _limit: f32) {}
    /// 整缓冲重置为 `color`（忽略当前变换与 clip）。
    fn clear(&mut self, color: &str);
    fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32);
    fn stroke_rect(&mut self, x: f32, y: f32, w: f32, h: f32);
    fn fill_path(&mut self, cmds: &[PathCommand]);
    fn stroke_path(&mut self, cmds: &[PathCommand]);
    /// Draw positioned monochrome glyph outlines using the current fill brush.
    /// This preserves per-glyph painter order; it does not shape text.
    fn fill_glyph_run(&mut self, run: &GlyphRun) {
        for path in run.paths() {
            self.fill_path(path);
        }
    }
    fn stroke_glyph_run(&mut self, run: &GlyphRun) {
        for path in run.paths() {
            self.stroke_path(path);
        }
    }
    /// Draw an immutable shaped text snapshot; color glyphs use shared atlas pages.
    #[cfg(feature = "text-layout")]
    fn draw_text_layout(&mut self, layout: &PreparedText2D, origin: [f32; 2]) {
        layout.draw(self, origin);
    }
    #[cfg(feature = "text-layout")]
    fn draw_text_layout_with_brush(
        &mut self,
        layout: &PreparedText2D,
        origin: [f32; 2],
    ) -> Result<(), TextLayoutError> {
        layout.draw_with_brush(self, origin)
    }
    /// Gaussian-blurred circular-corner rectangle in current user coordinates.
    /// Uses explicit display-sRGB color, current CTM/alpha/clip/mask. Invalid
    /// parameters paint nothing; sigma below 0.001 uses an ordinary rounded fill.
    fn draw_blurred_rounded_rect(
        &mut self,
        rect: [f32; 4],
        radius: f32,
        std_dev: f32,
        color: [f32; 4],
    );
    /// Gaussian rounded shadow with independent circular corner radii (TL, TR,
    /// BR, BL). Inset is the blurred complement clipped to the rounded shape.
    /// Uses current user-space CTM/alpha/clip and explicit display-sRGB color.
    fn draw_rounded_rect_shadow(
        &mut self,
        rect: [f32; 4],
        radii: [f32; 4],
        std_dev: f32,
        color: [f32; 4],
        inset: bool,
    );
    fn translate(&mut self, dx: f32, dy: f32);
    /// 绕当前原点旋转；单位为**度**（与 Jadix `rotate_deg` / Svg transform 一致，不是 HTML 弧度）。
    fn rotate(&mut self, deg: f32);
    fn scale(&mut self, sx: f32, sy: f32);
    /// Intersect with a transformed rectangle; rotated/sheared clips use a path mask.
    fn clip_rect(&mut self, x: f32, y: f32, w: f32, h: f32);
    fn clip(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.clip_rect(x, y, w, h);
    }
    /// Intersect with an antialiased path clip captured under the current transform.
    fn clip_path(&mut self, _path: &[PathCommand], _rule: FillRule) {}
    /// Intersect with an image's alpha in device pixels. Dimensions must match the canvas.
    /// Retaining the Image2D lets GPU submissions reuse its upload.
    fn clip_image_mask(&mut self, _mask: &Image2D) {}
    /// Begin an isolated transparent group. Opacity applies once when popped.
    /// The layer stack is independent of save/restore.
    fn push_layer(&mut self, _opacity: f32, _blend: BlendMode) {}
    /// Begin a group with independent mixing, composition and final clip/mask.
    /// Implementations must support these options explicitly.
    fn push_layer_with_options(&mut self, options: LayerOptions);
    /// Content drawn in the group masks the existing backdrop by its luminance.
    /// Linear canvases/GPU use linear-sRGB coefficients; invert removes luminance.
    fn push_luminance_layer(&mut self, opacity: f32, invert: bool) {
        self.push_layer_with_options(LayerOptions {
            opacity,
            compose: if invert {
                CompositeMode::LuminanceOut
            } else {
                CompositeMode::LuminanceIn
            },
            ..Default::default()
        });
    }
    fn pop_layer(&mut self) {}
    fn save(&mut self);
    fn restore(&mut self);
    fn set_image_sampling(&mut self, _sampling: ImageSampling) {}
    fn draw_image(&mut self, _src: &CanvasImageSource<'_>, _dx: f32, _dy: f32, _dw: f32, _dh: f32) {
    }
    fn get_image_data(&self, _x: u32, _y: u32, _w: u32, _h: u32) -> Option<Vec<u8>> {
        None
    }
    fn put_image_data(&mut self, _x: u32, _y: u32, _w: u32, _h: u32, _rgba: &[u8]) {}
    fn is_point_in_path(&self, _path: &[PathCommand], _x: f32, _y: f32) -> bool {
        false
    }
    fn is_point_in_stroke(&self, _path: &[PathCommand], _x: f32, _y: f32) -> bool {
        false
    }
    /// Request another animation frame; the framework can otherwise cache clean paints.
    fn request_frame(&mut self) {
        let _ = self;
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
    fn array(self) -> [f32; 6] {
        [self.a, self.b, self.c, self.d, self.e, self.f]
    }
    fn from_array(m: [f32; 6]) -> Self {
        Self {
            a: m[0],
            b: m[1],
            c: m[2],
            d: m[3],
            e: m[4],
            f: m[5],
        }
    }
    fn inverse(self) -> Option<Self> {
        let det = self.a * self.d - self.b * self.c;
        if !det.is_finite() || det.abs() < 1e-12 {
            return None;
        }
        Some(Self {
            a: self.d / det,
            b: -self.b / det,
            c: -self.c / det,
            d: self.a / det,
            e: (self.c * self.f - self.d * self.e) / det,
            f: (self.b * self.e - self.a * self.f) / det,
        })
    }
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

    fn scale_bound(self) -> f32 {
        (self.a * self.a + self.b * self.b + self.c * self.c + self.d * self.d)
            .sqrt()
            .max(1e-6)
    }
}

fn map_line_cap(c: LineCap) -> StrokeLinecap {
    match c {
        LineCap::Butt => StrokeLinecap::Butt,
        LineCap::Round => StrokeLinecap::Round,
        LineCap::Square => StrokeLinecap::Square,
    }
}

fn map_line_join(j: LineJoin) -> StrokeLinejoin {
    match j {
        LineJoin::Miter => StrokeLinejoin::Miter,
        LineJoin::Round => StrokeLinejoin::Round,
        LineJoin::Bevel => StrokeLinejoin::Bevel,
    }
}

fn apply_affine(cmds: &[PathCommand], xf: Affine) -> Vec<PathCommand> {
    apply_affine_tol(cmds, xf, None)
}
fn apply_affine_tol(cmds: &[PathCommand], xf: Affine, tolerance: Option<f32>) -> Vec<PathCommand> {
    // Flatten endpoint arcs before affine mapping: a mean scale cannot represent
    // elliptical radii, reflection or shear correctly.
    let flat;
    let cmds = if tolerance.is_some() || cmds.iter().any(|c| matches!(c, PathCommand::Arc { .. })) {
        let bound = (xf.a * xf.a + xf.b * xf.b + xf.c * xf.c + xf.d * xf.d)
            .sqrt()
            .max(1e-6);
        flat = flatten_path_with_tolerance(
            cmds,
            tolerance.unwrap_or(jadix_core::path::FLATTEN_FLATNESS) / bound,
        );
        &flat[..]
    } else {
        cmds
    };
    cmds.iter()
        .cloned()
        .map(|c| c.map_xy(|p| xf.apply(p)))
        .collect()
}

fn brush_inverse(brush: &Brush2D, ctm: Affine) -> Option<[f32; 6]> {
    if !brush.valid() || !brush.transform().iter().all(|v| v.is_finite()) {
        return None;
    }
    let inverse = ctm
        .mul(Affine::from_array(brush.transform()))
        .inverse()?
        .array();
    inverse.iter().all(|v| v.is_finite()).then_some(inverse)
}

fn canvas_point_in_path(state: &CanvasState, path: &[PathCommand], x: f32, y: f32) -> bool {
    let device = apply_affine(path, state.xf);
    path_contains_point(&device, (x, y), state.fill_rule)
}

fn canvas_point_in_stroke(state: &CanvasState, path: &[PathCommand], x: f32, y: f32) -> bool {
    let outline = stroke_outline(path, &state.stroke_style(), 0.05 / state.xf.scale_bound());
    path_contains_point(&apply_affine(&outline, state.xf), (x, y), FillRule::NonZero)
}

fn intersect_clip(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let x0 = a[0].max(b[0]);
    let y0 = a[1].max(b[1]);
    let x1 = (a[0] + a[2]).min(b[0] + b[2]);
    let y1 = (a[1] + a[3]).min(b[1] + b[3]);
    [x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0)]
}

#[derive(Clone)]
struct CanvasState {
    xf: Affine,
    clip: [f32; 4],
    fill: [f32; 4],
    gradient: Option<Gradient>,
    fill_brush: Option<Brush2D>,
    stroke_brush: Option<Brush2D>,
    alpha: f32,
    fill_rule: FillRule,
    mask: Option<Image2D>,
    sampling: ImageSampling,
    clip_samples: u32,
    stroke: [f32; 4],
    stroke_gradient: Option<Gradient>,
    line_width: f32,
    line_cap: LineCap,
    line_join: LineJoin,
    stroke_options: StrokeOptions,
}

impl CanvasState {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            xf: Affine::identity(),
            clip: [0.0, 0.0, width as f32, height as f32],
            fill: [0.0, 0.0, 0.0, 1.0],
            gradient: None,
            fill_brush: None,
            stroke_brush: None,
            alpha: 1.0,
            fill_rule: FillRule::NonZero,
            mask: None,
            sampling: ImageSampling::Nearest,
            clip_samples: 2,
            stroke: [0.0, 0.0, 0.0, 1.0],
            stroke_gradient: None,
            line_width: 1.0,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            stroke_options: StrokeOptions::default(),
        }
    }

    fn translate(&mut self, dx: f32, dy: f32) {
        self.xf = self.xf.mul(Affine::translate(dx, dy));
    }

    fn rotate(&mut self, deg: f32) {
        self.xf = self.xf.mul(Affine::rotate_deg(deg));
    }

    fn scale(&mut self, sx: f32, sy: f32) {
        self.xf = self.xf.mul(Affine::scale(sx, sy));
    }

    fn clip_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        if w <= 0.0 || h <= 0.0 {
            self.clip = [0.0, 0.0, 0.0, 0.0];
            return;
        }
        let corners = [
            self.xf.apply((x, y)),
            self.xf.apply((x + w, y)),
            self.xf.apply((x + w, y + h)),
            self.xf.apply((x, y + h)),
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
        self.clip = intersect_clip(self.clip, aabb);
    }

    fn clip_path(&mut self, path: &[PathCommand], rule: FillRule, width: u32, height: u32) {
        let device = apply_affine(path, self.xf);
        let mut style = PathStyle::fill([1.0; 4]);
        style.fill_rule = rule;
        let mut pixels = vec![0.0; width as usize * height as usize * 4];
        paint_path_premul(
            &mut pixels,
            width,
            height,
            &device,
            &style,
            Some(self.clip),
            self.clip_samples,
        );
        let rgba: Vec<u8> = pixels
            .chunks_exact(4)
            .enumerate()
            .flat_map(|(i, p)| {
                [
                    255,
                    255,
                    255,
                    (p[3] * self.mask_at(i) * 255.0).round() as u8,
                ]
            })
            .collect();
        self.mask = Image2D::from_rgba8(width, height, &rgba);
    }
    fn clip_image_mask(&mut self, mask: &Image2D, width: u32, height: u32) {
        if mask.width() != width || mask.height() != height {
            return;
        }
        if self.mask.is_none() {
            self.mask = Some(mask.clone());
            return;
        }
        let rgba: Vec<u8> = mask
            .rgba8()
            .chunks_exact(4)
            .enumerate()
            .flat_map(|(i, p)| [255, 255, 255, (p[3] as f32 * self.mask_at(i)).round() as u8])
            .collect();
        self.mask = Image2D::from_rgba8(width, height, &rgba);
    }
    fn mask_at(&self, index: usize) -> f32 {
        self.mask
            .as_ref()
            .map_or(1.0, |mask| mask.rgba8()[index * 4 + 3] as f32 / 255.0)
    }
    fn fill_style(&self) -> PathStyle {
        let mut color = self.fill;
        color[3] *= self.alpha;
        let mut style = PathStyle::fill(color);
        style.fill_gradient = self.gradient.clone();
        style.fill_rule = self.fill_rule;
        style
    }
    fn stroke_style(&self) -> PathStyle {
        let sw = self.line_width;
        let mut color = self.stroke;
        color[3] *= self.alpha;
        let mut style = PathStyle::stroke(color, sw)
            .linecap(map_line_cap(self.line_cap))
            .linejoin(map_line_join(self.line_join));
        style.stroke_options = self.stroke_options.clone();
        style
    }
}

struct Layer {
    pixels: Vec<f32>,
    options: LayerOptions,
}

pub struct RasterCanvas {
    /// Premultiplied working-space values; linear for `new_linear`.
    pub buf: Vec<f32>,
    linear: bool,
    hdr: bool,
    scratch: Vec<f32>,
    raster_samples: u32,
    curve_tolerance: Option<f32>,
    width: u32,
    height: u32,
    state: CanvasState,
    stack: Vec<CanvasState>,
    pub want_frame: bool,
    groups: Vec<Layer>,
}

impl RasterCanvas {
    /// One center sample for geometry and path clips. Set before drawing; this
    /// is used by whole-scene supersampling, not per-path coverage accumulation.
    pub fn set_point_sampling(&mut self, enabled: bool) {
        self.raster_samples = if enabled { 1 } else { 2 };
        self.state.clip_samples = if enabled { 1 } else { 2 };
    }
    /// Integrate tessellated path area per pixel before each draw's source-over.
    /// Curves retain the tessellator tolerance; brush colors use pixel centers.
    /// Image/blur sampling remains independent. Default is the legacy sampled AA.
    pub fn set_area_antialiasing(&mut self, enabled: bool) {
        self.raster_samples = if enabled { 0 } else { 2 };
        self.state.clip_samples = 2;
    }
    /// Device-pixel flattening tolerance for fills. The legacy default is unchanged.
    pub fn set_curve_tolerance(&mut self, tolerance: f32) -> bool {
        if !tolerance.is_finite() || !(0.001..=1.).contains(&tolerance) {
            return false;
        }
        self.curve_tolerance = Some(tolerance);
        true
    }
    /// Extended linear-sRGB working pixels, including normal Porter-Duff groups.
    /// Non-normal blend modes and bounded imaging filters retain their LDR semantics.
    pub fn new_hdr(width: u32, height: u32) -> Self {
        let mut c = Self::new_linear(width, height);
        c.hdr = true;
        c
    }
    /// Finish groups and export premultiplied linear-sRGB f32, retaining HDR.
    pub fn into_linear_rgba32f(mut self) -> Vec<f32> {
        self.finish_layers();
        if !self.linear {
            for p in self.buf.chunks_exact_mut(4) {
                if p[3] > 0. {
                    for i in 0..3 {
                        p[i] = decode_srgb(p[i] / p[3]) * p[3];
                    }
                }
            }
        }
        self.buf
    }
    /// Straight float output in an explicitly chosen color space. No gamut map
    /// is applied; negative/wide-gamut/HDR components are retained.
    pub fn into_color_space(self, space: crate::ColorSpaceTag) -> Vec<f32> {
        self.into_linear_rgba32f()
            .chunks_exact(4)
            .flat_map(|p| {
                let a = p[3];
                let c = if a > 0. {
                    [p[0] / a, p[1] / a, p[2] / a, a]
                } else {
                    [0.; 4]
                };
                crate::Color2D::new(crate::ColorSpaceTag::LinearSrgb, c)
                    .unwrap()
                    .0
                    .convert(space)
                    .components
            })
            .collect()
    }
    /// Finish remaining isolated groups and return premultiplied RGBA pixels.
    pub fn into_pixels(mut self) -> Vec<f32> {
        self.finish_layers();
        if self.linear {
            for p in self.buf.chunks_exact_mut(4) {
                if p[3] > 1e-6 {
                    for c in 0..3 {
                        p[c] = encode_srgb(p[c] / p[3]) * p[3];
                    }
                }
            }
        }
        self.buf
    }
    /// Composite in linear-sRGB, matching the GPU. Pixel APIs and `into_pixels`
    /// still export display-sRGB. `new` retains its legacy display-space behavior.
    pub fn new_linear(width: u32, height: u32) -> Self {
        let mut canvas = Self::new(width, height);
        canvas.linear = true;
        canvas
    }
    pub fn new(width: u32, height: u32) -> Self {
        let n = (width as usize) * (height as usize) * 4;
        Self {
            buf: vec![0.0f32; n],
            linear: false,
            hdr: false,
            scratch: Vec::new(),
            raster_samples: 2,
            curve_tolerance: None,
            width,
            height,
            state: CanvasState::new(width, height),
            stack: Vec::new(),
            want_frame: false,
            groups: Vec::new(),
        }
    }

    fn paint_blur(&mut self, capture: &CapturedBlur) {
        let blur = &capture.blur;
        let state = &capture.state;
        if blur.sigma == 0. {
            self.state.mask = state.mask.clone();
            self.paint_device(
                &apply_affine(&blur.path(), state.xf),
                &PathStyle::fill(capture.color()),
                Some(state.clip),
            );
            return;
        }
        let bounds = blur.bounds();
        let corners = [
            (bounds[0], bounds[1]),
            (bounds[0] + bounds[2], bounds[1]),
            (bounds[0] + bounds[2], bounds[1] + bounds[3]),
            (bounds[0], bounds[1] + bounds[3]),
        ]
        .map(|p| state.xf.apply(p));
        let min_x = corners
            .iter()
            .map(|p| p.0)
            .fold(f32::INFINITY, f32::min)
            .floor()
            .max(0.) as u32;
        let min_y = corners
            .iter()
            .map(|p| p.1)
            .fold(f32::INFINITY, f32::min)
            .floor()
            .max(0.) as u32;
        let max_x = corners
            .iter()
            .map(|p| p.0)
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil()
            .clamp(0., self.width as f32) as u32;
        let max_y = corners
            .iter()
            .map(|p| p.1)
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil()
            .clamp(0., self.height as f32) as u32;
        let color = if self.linear {
            linear_color(capture.color())
        } else {
            capture.color()
        };
        for y in min_y..max_y {
            for x in min_x..max_x {
                let mut coverage = 0.;
                let samples = if self.raster_samples == 1 {
                    1
                } else if blur.extended() {
                    4
                } else {
                    2
                };
                for iy in 0..samples {
                    let oy = (iy as f32 + 0.5) / samples as f32;
                    for ix in 0..samples {
                        let ox = (ix as f32 + 0.5) / samples as f32;
                        let p = (x as f32 + ox, y as f32 + oy);
                        let clip = state.clip;
                        if p.0 >= clip[0]
                            && p.1 >= clip[1]
                            && p.0 < clip[0] + clip[2]
                            && p.1 < clip[1] + clip[3]
                        {
                            let local = capture.inverse.apply(p);
                            if local.0 >= bounds[0]
                                && local.1 >= bounds[1]
                                && local.0 < bounds[0] + bounds[2]
                                && local.1 < bounds[1] + bounds[3]
                            {
                                coverage += blur.alpha(local) / (samples * samples) as f32;
                            }
                        }
                    }
                }
                let i = (y as usize * self.width as usize + x as usize) * 4;
                let mask = state
                    .mask
                    .as_ref()
                    .map_or(1., |m| m.rgba8()[i + 3] as f32 / 255.);
                let a = coverage * color[3] * mask;
                for (channel, value) in color[..3].iter().enumerate() {
                    self.buf[i + channel] = value * a + self.buf[i + channel] * (1. - a);
                }
                self.buf[i + 3] = a + self.buf[i + 3] * (1. - a);
            }
        }
    }
    fn paint_brush_cmds(&mut self, cmds: &[PathCommand], brush: Brush2D, rule: FillRule) {
        let Some(inverse) = brush_inverse(&brush, self.state.xf) else {
            return;
        };
        self.paint_brush_device(
            &apply_affine_tol(cmds, self.state.xf, self.curve_tolerance),
            &brush,
            inverse,
            rule,
            self.state.alpha,
            Some(self.state.clip),
        );
    }
    fn paint_brush_device(
        &mut self,
        cmds: &[PathCommand],
        brush: &Brush2D,
        inverse: [f32; 6],
        rule: FillRule,
        alpha: f32,
        clip: Option<[f32; 4]>,
    ) {
        let flattened = self
            .curve_tolerance
            .map(|t| flatten_path_with_tolerance(cmds, t));
        let cmds = flattened.as_deref().unwrap_or(cmds);
        let mut coverage = std::mem::take(&mut self.scratch);
        coverage.resize(self.buf.len(), 0.);
        coverage.fill(0.);
        let mut style = PathStyle::fill([1.; 4]);
        style.fill_rule = rule;
        let vector_paths = (self.raster_samples != 0 && matches!(brush, Brush2D::Vector { .. }))
            .then(|| subpaths(cmds));
        paint_path_premul(
            &mut coverage,
            self.width,
            self.height,
            cmds,
            &style,
            clip,
            if vector_paths.is_some() {
                4
            } else {
                self.raster_samples
            },
        );
        let inverse = Affine::from_array(inverse);
        let sampler = match brush {
            Brush2D::Gradient { gradient, rect, .. } => Some(
                crate::gradient::prepared_gradient_sampler(gradient, rect[2], rect[3]),
            ),
            _ => None,
        };
        for (i, p) in coverage.chunks_exact_mut(4).enumerate() {
            if let (Brush2D::Vector { gradient, .. }, Some(paths)) = (brush, &vector_paths) {
                if p[3] == 0. {
                    p.fill(0.);
                    continue;
                }
                let (x, y) = (
                    (i % self.width as usize) as f32,
                    (i / self.width as usize) as f32,
                );
                let mut sum = [0f32; 4];
                let samples = if self.raster_samples == 1 { 1 } else { 4 };
                for sy in 0..samples {
                    for sx in 0..samples {
                        let point = (
                            x + (sx as f32 + 0.5) / samples as f32,
                            y + (sy as f32 + 0.5) / samples as f32,
                        );
                        if clip.is_some_and(|c| {
                            point.0 < c[0]
                                || point.1 < c[1]
                                || point.0 >= c[0] + c[2]
                                || point.1 >= c[1] + c[3]
                        }) || !contours_contain_point(paths, point, rule)
                        {
                            continue;
                        }
                        let q = inverse.apply(point);
                        let mut c = gradient.sample([q.0, q.1]);
                        if !self.linear && c[3] > 0. {
                            for k in 0..3 {
                                c[k] = encode_srgb(c[k] / c[3]) * c[3];
                            }
                        }
                        for k in 0..4 {
                            sum[k] += c[k] / (samples * samples) as f32;
                        }
                    }
                }
                let a = alpha * self.state.mask_at(i);
                for k in 0..4 {
                    p[k] = sum[k] * a;
                }
                continue;
            }
            let a = p[3] * alpha * self.state.mask_at(i);
            if a == 0. {
                p.fill(0.);
                continue;
            }
            let point = inverse.apply((
                (i % self.width as usize) as f32 + 0.5,
                (i / self.width as usize) as f32 + 0.5,
            ));
            let mut c = match brush {
                Brush2D::Vector { gradient, .. } => gradient.sample([point.0, point.1]),
                Brush2D::Gradient { rect, .. } => {
                    let c = sampler.as_ref().unwrap()(
                        (point.0 - rect[0]) / rect[2],
                        (point.1 - rect[1]) / rect[3],
                    );
                    let c = linear_color(c);
                    [c[0] * c[3], c[1] * c[3], c[2] * c[3], c[3]]
                }
                Brush2D::Image {
                    image,
                    extend_x,
                    extend_y,
                    sampling,
                    ..
                } => image.sample_linear_with_footprint(
                    [point.0, point.1],
                    [inverse.a, inverse.b],
                    [inverse.c, inverse.d],
                    *sampling,
                    *extend_x,
                    *extend_y,
                ),
            };
            if !self.linear && c[3] > 1e-6 {
                let alpha = c[3];
                for v in &mut c[..3] {
                    *v = encode_srgb(*v / alpha) * alpha;
                }
            }
            for j in 0..4 {
                p[j] = c[j] * a;
            }
        }
        crate::blend::composite(&mut self.buf, &coverage, 1., BlendMode::Normal);
        self.scratch = coverage;
    }
    fn paint_cmds(&mut self, cmds: &[PathCommand], style: PathStyle) {
        if cmds.is_empty() {
            return;
        }
        let device = apply_affine_tol(cmds, self.state.xf, self.curve_tolerance);
        self.paint_device(&device, &style, Some(self.state.clip));
    }
    fn paint_device(&mut self, device: &[PathCommand], style: &PathStyle, clip: Option<[f32; 4]>) {
        let flattened = self
            .curve_tolerance
            .map(|t| flatten_path_with_tolerance(device, t));
        let device = flattened.as_deref().unwrap_or(device);
        let gradient_conversion = self.linear && style.fill_gradient.is_some();
        let converted;
        let style = if self.linear && !gradient_conversion {
            converted = PathStyle {
                fill: style.fill.map(linear_color),
                stroke: style.stroke.map(linear_color),
                ..style.clone()
            };
            &converted
        } else {
            style
        };
        if self.state.mask.is_none() && !gradient_conversion {
            paint_path_premul(
                &mut self.buf,
                self.width,
                self.height,
                device,
                style,
                clip,
                self.raster_samples,
            );
        } else {
            let mut pixels = std::mem::take(&mut self.scratch);
            pixels.resize(self.buf.len(), 0.);
            pixels.fill(0.);
            paint_path_premul(
                &mut pixels,
                self.width,
                self.height,
                device,
                style,
                clip,
                self.raster_samples,
            );
            for (index, px) in pixels.chunks_exact_mut(4).enumerate() {
                if gradient_conversion && px[3] > 1e-6 {
                    for c in 0..3 {
                        px[c] = decode_srgb(px[c] / px[3]) * px[3];
                    }
                }
                let mask = self.state.mask_at(index);
                for v in px {
                    *v *= mask;
                }
            }
            crate::blend::composite(&mut self.buf, &pixels, 1.0, BlendMode::Normal);
            self.scratch = pixels;
        }
    }
    fn finish_layers(&mut self) {
        while !self.groups.is_empty() {
            self.pop_layer();
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Readback target region and working color space are explicit"
)]
fn sample_premul_image_data(
    linear: bool,
    buf: &[f32],
    width: u32,
    height: u32,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
) -> Option<Vec<u8>> {
    if w == 0 || h == 0 || x >= width || y >= height {
        return None;
    }
    let rw = w.min(width - x);
    let rh = h.min(height - y);
    let mut out = vec![0u8; rw as usize * rh as usize * 4];
    for row in 0..rh as usize {
        for col in 0..rw as usize {
            let si = (((y as usize + row) * width as usize) + (x as usize + col)) * 4;
            let di = (row * rw as usize + col) * 4;
            let a = buf[si + 3].clamp(0.0, 1.0);
            out[di + 3] = (a * 255.0).round() as u8;
            if a > 1e-6 {
                for c in 0..3 {
                    let value = buf[si + c] / a;
                    out[di + c] = (if linear { encode_srgb(value) } else { value } * 255.)
                        .round()
                        .clamp(0., 255.) as u8;
                }
            }
        }
    }
    Some(out)
}

impl Canvas2D for RasterCanvas {
    fn current_transform(&self) -> Option<[f32; 6]> {
        Some(self.state.xf.array())
    }
    fn current_global_alpha(&self) -> Option<f32> {
        Some(self.state.alpha)
    }
    fn fill_glyph_run(&mut self, run: &GlyphRun) {
        let n = if self.raster_samples == 1 { 1 } else { 4 };
        let old = std::mem::replace(&mut self.raster_samples, n);
        for path in run.paths() {
            self.fill_path(path);
        }
        self.raster_samples = old;
    }
    fn stroke_glyph_run(&mut self, run: &GlyphRun) {
        let n = if self.raster_samples == 1 { 1 } else { 4 };
        let old = std::mem::replace(&mut self.raster_samples, n);
        for path in run.paths() {
            self.stroke_path(path);
        }
        self.raster_samples = old;
    }

    fn draw_blurred_rounded_rect(
        &mut self,
        rect: [f32; 4],
        radius: f32,
        std_dev: f32,
        color: [f32; 4],
    ) {
        if let Some(capture) = BlurredRect::new(rect, radius, std_dev, color)
            .and_then(|b| CapturedBlur::new(b, &self.state))
        {
            self.paint_blur(&capture);
        }
    }

    fn draw_rounded_rect_shadow(
        &mut self,
        rect: [f32; 4],
        radii: [f32; 4],
        std_dev: f32,
        color: [f32; 4],
        inset: bool,
    ) {
        if let Some(capture) = BlurredRect::shadow(rect, radii, std_dev, color, inset)
            .and_then(|b| CapturedBlur::new(b, &self.state))
        {
            self.paint_blur(&capture);
        }
    }

    fn clip_image_mask(&mut self, mask: &Image2D) {
        self.state.clip_image_mask(mask, self.width, self.height);
    }
    fn size(&self) -> CanvasSize {
        CanvasSize {
            w: self.width as f32,
            h: self.height as f32,
        }
    }

    fn set_fill(&mut self, color: &str) {
        self.state.fill = parse_color(color);
        self.state.gradient = None;
        self.state.fill_brush = None;
    }

    fn set_fill_brush(&mut self, brush: Brush2D) {
        self.state.fill_brush = Some(brush);
        self.state.gradient = None;
    }
    fn set_stroke_brush(&mut self, brush: Brush2D) {
        self.state.stroke_brush = Some(brush);
        self.state.stroke_gradient = None;
    }
    fn set_stroke(&mut self, color: &str) {
        self.state.stroke = parse_color(color);
        self.state.stroke_gradient = None;
        self.state.stroke_brush = None;
    }
    fn set_stroke_gradient(&mut self, gradient: Gradient) {
        self.state.stroke = [1.; 4];
        self.state.stroke_gradient = Some(gradient);
        self.state.stroke_brush = None;
    }

    fn set_fill_gradient(&mut self, gradient: Gradient) {
        self.state.fill = [1.0; 4];
        self.state.gradient = Some(gradient);
        self.state.fill_brush = None;
    }
    fn set_global_alpha(&mut self, alpha: f32) {
        if alpha.is_finite() {
            self.state.alpha = alpha.clamp(0.0, 1.0);
        }
    }
    fn set_fill_rule(&mut self, rule: FillRule) {
        self.state.fill_rule = rule;
    }
    fn transform(&mut self, matrix: [f32; 6]) {
        if matrix.iter().all(|v| v.is_finite()) {
            self.state.xf = self.state.xf.mul(Affine::from_array(matrix));
        }
    }
    fn set_transform(&mut self, matrix: [f32; 6]) {
        if matrix.iter().all(|v| v.is_finite()) {
            self.state.xf = Affine::from_array(matrix);
        }
    }
    fn set_line_width(&mut self, width: f32) {
        if width.is_finite() && width > 0.0 {
            self.state.line_width = width;
        }
    }

    fn set_line_cap(&mut self, cap: LineCap) {
        self.state.line_cap = cap;
    }

    fn set_line_join(&mut self, join: LineJoin) {
        self.state.line_join = join;
    }
    fn set_line_dash(&mut self, pattern: &[f32]) {
        if let Some(dashes) = StrokeOptions::normalized_dashes(pattern) {
            self.state.stroke_options.dash_array = dashes;
        }
    }
    fn set_line_dash_offset(&mut self, offset: f32) {
        if offset.is_finite() {
            self.state.stroke_options.dash_offset = offset;
        }
    }
    fn set_miter_limit(&mut self, limit: f32) {
        if limit.is_finite() && limit >= 1.0 {
            self.state.stroke_options.miter_limit = limit;
        }
    }

    fn clear(&mut self, color: &str) {
        let c = parse_color(color);
        let c = if self.linear { linear_color(c) } else { c };
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
        self.fill_path(&filled_rect(x, y, w, h));
    }

    fn stroke_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        self.stroke_path(&polygon(&[(x, y), (x + w, y), (x + w, y + h), (x, y + h)]));
    }

    fn fill_path(&mut self, cmds: &[PathCommand]) {
        if let Some(brush) = self.state.fill_brush.clone() {
            self.paint_brush_cmds(cmds, brush, self.state.fill_rule);
        } else {
            self.paint_cmds(cmds, self.state.fill_style());
        }
    }

    fn stroke_path(&mut self, cmds: &[PathCommand]) {
        let outline = stroke_outline(
            cmds,
            &self.state.stroke_style(),
            0.05 / self.state.xf.scale_bound(),
        );
        if let Some(brush) = self.state.stroke_brush.clone() {
            self.paint_brush_cmds(&outline, brush, FillRule::NonZero);
            return;
        }
        let color = self.state.stroke_style().stroke.unwrap();
        let mut style = PathStyle::fill(color);
        style.fill_gradient = self.state.stroke_gradient.clone();
        self.paint_cmds(&outline, style);
    }

    fn translate(&mut self, dx: f32, dy: f32) {
        self.state.translate(dx, dy);
    }

    fn rotate(&mut self, deg: f32) {
        self.state.rotate(deg);
    }

    fn scale(&mut self, sx: f32, sy: f32) {
        self.state.scale(sx, sy);
    }

    fn clip_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        if self.state.xf.b.abs() > 1e-6 || self.state.xf.c.abs() > 1e-6 {
            self.clip_path(&filled_rect(x, y, w, h), FillRule::NonZero);
        } else {
            self.state.clip_rect(x, y, w, h);
        }
    }
    fn clip_path(&mut self, path: &[PathCommand], rule: FillRule) {
        self.state.clip_path(path, rule, self.width, self.height);
    }
    fn push_layer(&mut self, opacity: f32, blend: BlendMode) {
        self.push_layer_with_options(LayerOptions::new(opacity, blend));
    }
    fn push_layer_with_options(&mut self, options: LayerOptions) {
        let pixels = std::mem::replace(
            &mut self.buf,
            vec![0.; self.width as usize * self.height as usize * 4],
        );
        self.groups.push(Layer {
            pixels,
            options: options.normalized(self.width, self.height),
        });
    }
    fn pop_layer(&mut self) {
        if let Some(mut layer) = self.groups.pop() {
            crate::filter::apply(
                &mut self.buf,
                [self.width, self.height],
                &layer.options.filters,
                self.linear,
            );
            crate::blend::composite_layer(
                &mut layer.pixels,
                &self.buf,
                &layer.options,
                self.width,
                self.hdr,
            );
            self.buf = layer.pixels;
        }
    }

    fn save(&mut self) {
        self.stack.push(self.state.clone());
    }

    fn restore(&mut self) {
        if let Some(s) = self.stack.pop() {
            self.state = s;
        }
    }

    fn set_image_sampling(&mut self, sampling: ImageSampling) {
        self.state.sampling = sampling;
    }
    fn draw_image(&mut self, src: &CanvasImageSource<'_>, dx: f32, dy: f32, dw: f32, dh: f32) {
        if !src.valid_region() || [dx, dy, dw, dh].iter().any(|v| !v.is_finite()) {
            return;
        }
        let (width, height, rgba) = match src {
            CanvasImageSource::Rgba {
                width,
                height,
                rgba,
            } => (*width, *height, *rgba),
            CanvasImageSource::Image(image) => (image.width(), image.height(), image.rgba8()),
            CanvasImageSource::Region { image, .. } => {
                (image.width(), image.height(), image.rgba8())
            }
            CanvasImageSource::TextureKey(_) => return,
        };
        if width == 0 || height == 0 || dw <= 0.0 || dh <= 0.0 {
            return;
        }
        let Some(required) = (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| n.checked_mul(4))
        else {
            return;
        };
        if rgba.len() < required {
            return;
        }
        let [sx, sy, sw, sh] = src
            .region()
            .unwrap_or([0, 0, width, height])
            .map(|v| v as f32);
        let Some(inverse) = self.state.xf.inverse() else {
            return;
        };
        let float_image = match src {
            CanvasImageSource::Image(i) | CanvasImageSource::Region { image: i, .. }
                if i.linear_rgba32f().is_some() || self.state.sampling as u8 >= 4 =>
            {
                Some(*i)
            }
            _ => None,
        };
        let sample = |u: f32, v: f32| {
            let (u, v) = if src.region().is_some() {
                ((sx + u * sw) / width as f32, (sy + v * sh) / height as f32)
            } else {
                (u, v)
            };
            if self.linear || self.state.sampling as u8 >= 4 {
                if let Some(image) = float_image {
                    let point = [u * width as f32 - sx, v * height as f32 - sy];
                    let mut c = if src.region().is_none()
                        || src.region() == Some([0, 0, width, height])
                    {
                        image.sample_linear_with_footprint(
                            point,
                            [inverse.a * sw / dw, inverse.b * sh / dh],
                            [inverse.c * sw / dw, inverse.d * sh / dh],
                            self.state.sampling,
                            crate::ExtendMode::Pad,
                            crate::ExtendMode::Pad,
                        )
                    } else {
                        image
                            .sample_region_linear(src.region().unwrap(), point, self.state.sampling)
                            .unwrap_or([0.; 4])
                    };
                    if c[3] > 0. {
                        for i in 0..3 {
                            c[i] /= c[3];
                        }
                    }
                    if !self.linear {
                        for v in &mut c[..3] {
                            *v = encode_srgb(*v);
                        }
                    }
                    return c;
                }
            }
            crate::image::sample_region(
                rgba,
                width,
                height,
                src.region().unwrap_or([0, 0, width, height]),
                u,
                v,
                self.state.sampling,
            )
        };
        let corners = [(dx, dy), (dx + dw, dy), (dx + dw, dy + dh), (dx, dy + dh)]
            .map(|p| self.state.xf.apply(p));
        let x0 = corners
            .iter()
            .map(|p| p.0)
            .fold(f32::INFINITY, f32::min)
            .floor()
            .max(0.0) as u32;
        let y0 = corners
            .iter()
            .map(|p| p.1)
            .fold(f32::INFINITY, f32::min)
            .floor()
            .max(0.0) as u32;
        let x1 = corners
            .iter()
            .map(|p| p.0)
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil()
            .max(0.0)
            .min(self.width as f32) as u32;
        let y1 = corners
            .iter()
            .map(|p| p.1)
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil()
            .max(0.0)
            .min(self.height as f32) as u32;
        let clip = self.state.clip;
        for py in y0..y1 {
            for px in x0..x1 {
                let point = (px as f32 + 0.5, py as f32 + 0.5);
                if point.0 < clip[0]
                    || point.1 < clip[1]
                    || point.0 >= clip[0] + clip[2]
                    || point.1 >= clip[1] + clip[3]
                {
                    continue;
                }
                // Match the CPU path coverage budget. Integrate premultiplied
                // linear samples, including image boundaries and nearest-texel
                // discontinuities, before compositing the pixel once.
                let color = if self.linear {
                    let mut sum = [0.; 4];
                    let samples = if self.raster_samples == 1 {
                        1
                    } else if src.region().is_some() {
                        4
                    } else {
                        2
                    };
                    let weight = 1. / (samples * samples) as f32;
                    for sy in 0..samples {
                        for sx in 0..samples {
                            let local = inverse.apply((
                                px as f32 + (sx as f32 + 0.5) / samples as f32,
                                py as f32 + (sy as f32 + 0.5) / samples as f32,
                            ));
                            let u = (local.0 - dx) / dw;
                            let v = (local.1 - dy) / dh;
                            if !(0.0..1.0).contains(&u) || !(0.0..1.0).contains(&v) {
                                continue;
                            }
                            let c = if float_image.is_some() {
                                sample(u, v)
                            } else {
                                linear_color(sample(u, v))
                            };
                            for i in 0..3 {
                                sum[i] += c[i] * c[3] * weight;
                            }
                            sum[3] += c[3] * weight;
                        }
                    }
                    if sum[3] > 1e-6 {
                        for i in 0..3 {
                            sum[i] /= sum[3];
                        }
                    }
                    sum
                } else {
                    let local = inverse.apply(point);
                    let u = (local.0 - dx) / dw;
                    let v = (local.1 - dy) / dh;
                    if !(0.0..1.0).contains(&u) || !(0.0..1.0).contains(&v) {
                        continue;
                    }
                    sample(u, v)
                };
                let alpha = color[3]
                    * self.state.alpha
                    * self
                        .state
                        .mask_at(py as usize * self.width as usize + px as usize);
                let i = (py as usize * self.width as usize + px as usize) * 4;
                src_over_premul(&mut self.buf, i, color[0], color[1], color[2], alpha);
            }
        }
    }

    fn get_image_data(&self, x: u32, y: u32, w: u32, h: u32) -> Option<Vec<u8>> {
        sample_premul_image_data(self.linear, &self.buf, self.width, self.height, x, y, w, h)
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
                for c in 0..3 {
                    let value = rgba[si + c] as f32 / 255.;
                    self.buf[di + c] = if self.linear {
                        decode_srgb(value) * a
                    } else {
                        value * a
                    };
                }
                self.buf[di + 3] = a;
            }
        }
    }

    fn request_frame(&mut self) {
        self.want_frame = true;
    }

    fn is_point_in_path(&self, path: &[PathCommand], x: f32, y: f32) -> bool {
        canvas_point_in_path(&self.state, path, x, y)
    }

    fn is_point_in_stroke(&self, path: &[PathCommand], x: f32, y: f32) -> bool {
        canvas_point_in_stroke(&self.state, path, x, y)
    }
}

// Image commands do not retain unrelated fill/stroke brushes or dash vectors.
struct ImageState {
    xf: Affine,
    clip: [f32; 4],
    mask: Option<Image2D>,
    alpha: f32,
    sampling: ImageSampling,
}
impl ImageState {
    fn capture(state: &CanvasState) -> Self {
        Self {
            xf: state.xf,
            clip: state.clip,
            mask: state.mask.clone(),
            alpha: state.alpha,
            sampling: state.sampling,
        }
    }
}
struct CapturedBlur {
    blur: BlurredRect,
    state: ImageState,
    inverse: Affine,
}
impl CapturedBlur {
    fn new(blur: BlurredRect, state: &CanvasState) -> Option<Self> {
        let inverse = state.xf.inverse()?;
        if inverse.array().iter().any(|v| !v.is_finite()) {
            return None;
        }
        let [x, y, w, h] = blur.bounds();
        if [(x, y), (x + w, y), (x + w, y + h), (x, y + h)]
            .iter()
            .map(|p| state.xf.apply(*p))
            .any(|p| !p.0.is_finite() || !p.1.is_finite())
        {
            return None;
        }
        Some(Self {
            blur,
            state: ImageState::capture(state),
            inverse,
        })
    }
    fn color(&self) -> [f32; 4] {
        let mut color = self.blur.color;
        color[3] *= self.state.alpha;
        color
    }
}
struct CapturedGlyphs {
    run: GlyphRun,
    stroke: bool,
    state: CanvasState,
}
struct CapturedScene {
    scene: Scene2D,
    options: SceneOptions,
    state: CanvasState,
}
enum RecordedDraw {
    Scene(Box<CapturedScene>),
    Glyphs(Box<CapturedGlyphs>),
    Blur(CapturedBlur),
    Brush(
        Vec<PathCommand>,
        Brush2D,
        [f32; 6],
        FillRule,
        f32,
        Option<[f32; 4]>,
        Option<Image2D>,
    ),
    Path(
        Vec<PathCommand>,
        PathStyle,
        Option<[f32; 4]>,
        Option<Image2D>,
    ),
    Image(Image2D, Option<[u32; 4]>, [f32; 4], ImageState),
    PushLayer(LayerOptions),
    PopLayer,
}

impl RecordedDraw {
    fn replay(&self, cpu: &mut RasterCanvas) {
        match self {
            Self::Scene(capture) => {
                let state = std::mem::replace(&mut cpu.state, capture.state.clone());
                capture
                    .scene
                    .replay(cpu, capture.options.clone())
                    .expect("validated frozen scene");
                cpu.state = state;
            }
            Self::Glyphs(glyphs) => {
                let state = std::mem::replace(&mut cpu.state, glyphs.state.clone());
                if glyphs.stroke {
                    cpu.stroke_glyph_run(&glyphs.run);
                } else {
                    cpu.fill_glyph_run(&glyphs.run);
                }
                cpu.state = state;
            }
            Self::Blur(capture) => cpu.paint_blur(capture),
            Self::Brush(cmds, brush, inverse, rule, alpha, clip, mask) => {
                cpu.state.mask = mask.clone();
                cpu.paint_brush_device(cmds, brush, *inverse, *rule, *alpha, *clip);
            }
            Self::Path(cmds, style, clip, mask) => {
                cpu.state.mask = mask.clone();
                cpu.paint_device(cmds, style, *clip);
            }
            Self::PushLayer(options) => cpu.push_layer_with_options(options.clone()),
            Self::PopLayer => cpu.pop_layer(),
            Self::Image(image, region, [x, y, w, h], state) => {
                cpu.state.xf = state.xf;
                cpu.state.clip = state.clip;
                cpu.state.mask = state.mask.clone();
                cpu.state.alpha = state.alpha;
                cpu.state.sampling = state.sampling;
                let source = region.map_or(CanvasImageSource::Image(image), |rect| {
                    CanvasImageSource::Region { image, rect }
                });
                cpu.draw_image(&source, *x, *y, *w, *h);
            }
        }
    }
}

/// S5: records the same Canvas2D ops, flush → PathPipeline `jadix_canvas:{id}`.
pub struct GpuCanvas {
    curve_tolerance: Option<f32>,
    linear: bool,
    width: u32,
    height: u32,
    tex_key: Option<String>,
    state: CanvasState,
    stack: Vec<CanvasState>,
    layers: Vec<RecordedDraw>,
    group_depth: usize,
    pub want_frame: bool,
    pub needs_cpu: bool,
    cpu: Option<RasterCanvas>,
}

impl GpuCanvas {
    /// Point-sample path clips when paired with AaQuality::Aliased, sample count
    /// one. Set before recording. Whole-scene sampling configures both together.
    pub fn set_point_sampling(&mut self, enabled: bool) {
        self.state.clip_samples = if enabled { 1 } else { 2 };
    }

    /// CPU readback and continuation use the GPU's linear-sRGB compositing space.
    pub fn new_linear(width: u32, height: u32, tex_key: Option<String>) -> Self {
        let mut canvas = Self::new(width, height, tex_key);
        canvas.linear = true;
        canvas
    }
    pub fn new(width: u32, height: u32, tex_key: Option<String>) -> Self {
        Self {
            curve_tolerance: None,
            width,
            height,
            tex_key,
            linear: false,
            state: CanvasState::new(width, height),
            stack: Vec::new(),
            layers: Vec::new(),
            group_depth: 0,
            want_frame: false,
            needs_cpu: false,
            cpu: None,
        }
    }

    /// Device-pixel flattening tolerance for recorded fills and glyph outlines.
    /// Set before recording; affine arc conversion happens at draw time.
    pub fn set_curve_tolerance(&mut self, tolerance: f32) -> bool {
        if !tolerance.is_finite() || !(0.001..=1.).contains(&tolerance) {
            return false;
        }
        if let Some(cpu) = &mut self.cpu {
            cpu.set_curve_tolerance(tolerance);
        }
        self.curve_tolerance = Some(tolerance);
        true
    }
    fn activate_cpu(&mut self) -> &mut RasterCanvas {
        if self.cpu.is_none() {
            let mut cpu = if self.linear {
                RasterCanvas::new_linear(self.width, self.height)
            } else {
                RasterCanvas::new(self.width, self.height)
            };
            if let Some(t) = self.curve_tolerance {
                cpu.set_curve_tolerance(t);
            }
            for draw in &self.layers {
                draw.replay(&mut cpu);
            }
            self.cpu = Some(cpu);
            self.layers.clear();
        }
        self.needs_cpu = true;
        let cpu = self.cpu.as_mut().unwrap();
        cpu.state = self.state.clone();
        cpu
    }
    /// Replay a wholly recorded canvas with per-path area AA. A canvas that
    /// already entered CPU continuation cannot change its past AA decisions.
    pub fn into_raster_area(self) -> Result<Vec<f32>, SceneError> {
        if self.cpu.is_some() {
            return Err(SceneError::UnsupportedCanvas);
        }
        let mut cpu = if self.linear {
            RasterCanvas::new_linear(self.width, self.height)
        } else {
            RasterCanvas::new(self.width, self.height)
        };
        cpu.set_area_antialiasing(true);
        if let Some(t) = self.curve_tolerance {
            cpu.set_curve_tolerance(t);
        }
        for draw in &self.layers {
            draw.replay(&mut cpu);
        }
        Ok(cpu.into_pixels())
    }
    /// Finish a software continuation without invoking the paint callback again.
    pub fn into_raster(mut self) -> Vec<f32> {
        self.activate_cpu();
        self.cpu.unwrap().into_pixels()
    }
    fn clip_opt(&self) -> Option<[f32; 4]> {
        let c = self.state.clip;
        if c[2] <= 0.0 || c[3] <= 0.0 {
            return Some([0.0, 0.0, 0.0, 0.0]);
        }
        // Preserve half-open endpoint ties for the exact sample grid. The
        // legacy half-pixel full-viewport shortcut includes the last center.
        if self.state.clip_samples == 1 {
            return Some(c);
        }
        let fw = self.width as f32;
        let fh = self.height as f32;
        if c[0] <= 0.5 && c[1] <= 0.5 && (c[0] + c[2]) >= fw - 0.5 && (c[1] + c[3]) >= fh - 0.5 {
            None
        } else {
            Some(c)
        }
    }

    fn glyphs_need_cpu(&self, stroke: bool) -> bool {
        let (gradient, brush) = if stroke {
            (&self.state.stroke_gradient, &self.state.stroke_brush)
        } else {
            (&self.state.gradient, &self.state.fill_brush)
        };
        self.cpu.is_some()
            || gradient
                .as_ref()
                .is_some_and(|g| pack_gpu_gradient(g).is_none())
            || matches!(brush,Some(Brush2D::Gradient{gradient,..}) if pack_gpu_gradient(gradient).is_none())
    }
    fn push_brush_cmds(&mut self, cmds: &[PathCommand], brush: Brush2D, rule: FillRule) {
        let Some(inverse) = brush_inverse(&brush, self.state.xf) else {
            return;
        };
        let device = apply_affine_tol(cmds, self.state.xf, self.curve_tolerance);
        let clip = self.clip_opt();
        let alpha = self.state.alpha;
        if self.cpu.is_some()
            || matches!(&brush,Brush2D::Gradient {gradient,..} if pack_gpu_gradient(gradient).is_none())
        {
            self.activate_cpu()
                .paint_brush_device(&device, &brush, inverse, rule, alpha, clip);
        } else {
            self.layers.push(RecordedDraw::Brush(
                device,
                brush,
                inverse,
                rule,
                alpha,
                clip,
                self.state.mask.clone(),
            ));
        }
    }
    fn push_cmds(&mut self, cmds: &[PathCommand], style: PathStyle) {
        if cmds.is_empty() {
            return;
        }
        if self.state.clip[2] <= 0.0 || self.state.clip[3] <= 0.0 {
            return;
        }
        let device = apply_affine_tol(cmds, self.state.xf, self.curve_tolerance);
        let clip = self.clip_opt();
        if self.cpu.is_some()
            || style
                .fill_gradient
                .as_ref()
                .is_some_and(|g| pack_gpu_gradient(g).is_none())
        {
            let cpu = self.activate_cpu();
            cpu.paint_device(&device, &style, clip);
        } else {
            self.layers.push(RecordedDraw::Path(
                device,
                style,
                clip,
                self.state.mask.clone(),
            ));
        }
    }

    pub fn into_ops(self) -> Vec<PathGpuOp> {
        assert!(
            !self.needs_cpu,
            "Canvas requires software rendering; use into_raster instead"
        );
        let mut ops = Vec::new();
        let mut groups = Vec::new();
        let tolerance = self.curve_tolerance;
        let build_fill = |cmds: &[PathCommand], style: &PathStyle| {
            let flat = tolerance.map(|t| flatten_path_with_tolerance(cmds, t));
            build_fill_op(flat.as_deref().unwrap_or(cmds), style)
        };
        for draw in self.layers {
            match draw {
                RecordedDraw::Scene(capture) => {
                    let mut canvas = if self.linear {
                        GpuCanvas::new_linear(self.width, self.height, None)
                    } else {
                        GpuCanvas::new(self.width, self.height, None)
                    };
                    canvas.state = capture.state;
                    capture
                        .scene
                        .replay(&mut canvas, capture.options)
                        .expect("validated frozen scene");
                    ops.extend(canvas.into_ops());
                }
                RecordedDraw::Glyphs(glyphs) => {
                    let mut canvas = GpuCanvas::new_linear(self.width, self.height, None);
                    canvas.state = glyphs.state;
                    for path in glyphs.run.paths() {
                        if glyphs.stroke {
                            canvas.stroke_path(path);
                        } else {
                            canvas.fill_path(path);
                        }
                    }
                    ops.extend(canvas.into_ops());
                }
                RecordedDraw::Blur(capture) => {
                    let path = apply_affine(&capture.blur.path(), capture.state.xf);
                    if let Some(mut fill) = build_fill(&path, &PathStyle::fill(capture.color())) {
                        fill.clip = Some(capture.state.clip);
                        fill.mask = capture.state.mask;
                        if capture.blur.sigma > 0. {
                            fill.grad = Some(GpuFillGrad {
                                device_to_brush: capture.inverse.array(),
                                kind: if capture.blur.extended() { 9 } else { 8 },
                                angle: if capture.blur.inset { 1. } else { 0. },
                                bbox: capture.blur.rect,
                                center: [capture.blur.radius, capture.blur.sigma],
                                radii: [0.; 2],
                                stop_n: 0,
                                stop_t: std::array::from_fn(|i| {
                                    if i < 4 {
                                        capture.blur.corners[i]
                                    } else {
                                        0.
                                    }
                                }),
                                stop_c: [[0.; 4]; 8],
                            });
                        }
                        ops.push(PathGpuOp::Fill(fill));
                    }
                }
                RecordedDraw::PushLayer(options) => {
                    groups.push((std::mem::take(&mut ops), options));
                }
                RecordedDraw::PopLayer => {
                    if let Some((mut parent, options)) = groups.pop() {
                        parent.push(PathGpuOp::Layer(PathGpuLayer::new(ops, options)));
                        ops = parent;
                    }
                }
                RecordedDraw::Brush(cmds, brush, inverse, rule, alpha, clip, mask) => {
                    let mut style = PathStyle::fill([1., 1., 1., alpha]);
                    style.fill_rule = rule;
                    if let Brush2D::Gradient { gradient, .. } = &brush {
                        style.fill_gradient = Some(gradient.clone());
                    }
                    if let Some(mut fill) = build_fill(&cmds, &style) {
                        fill.clip = clip;
                        fill.mask = mask;
                        match brush {
                            Brush2D::Vector { gradient, .. } => {
                                fill.grad = Some(gradient.payload(inverse));
                                fill.paint = Some(gradient);
                                ops.push(PathGpuOp::Fill(fill));
                            }
                            Brush2D::Gradient { rect, .. } => {
                                let g = fill.grad.as_mut().unwrap();
                                g.bbox = rect;
                                g.device_to_brush = inverse;
                                ops.push(PathGpuOp::Fill(fill));
                            }
                            Brush2D::Image {
                                image,
                                extend_x,
                                extend_y,
                                sampling,
                                ..
                            } => ops.push(PathGpuOp::Pattern(PathGpuPattern {
                                fill,
                                image,
                                device_to_brush: inverse,
                                extend_x,
                                extend_y,
                                sampling,
                            })),
                        }
                    }
                }
                RecordedDraw::Path(cmds, style, clip, mask) => {
                    if let Some(mut fill) = build_fill(&cmds, &style) {
                        fill.clip = clip;
                        fill.mask = mask.clone();
                        ops.push(PathGpuOp::Fill(fill));
                    }
                    let mut stroke_style = style.clone();
                    stroke_style.fill = None;
                    stroke_style.fill_gradient = None;
                    let mesh = tessellate_path_mesh(&cmds, &stroke_style, &[], 0.0);
                    if !mesh.is_empty() {
                        ops.push(PathGpuOp::Mesh(PathGpuMesh {
                            tris: mesh,
                            clip,
                            mask,
                        }));
                    }
                }
                RecordedDraw::Image(image, source_rect, [x, y, w, h], state) => {
                    let corners =
                        [(x, y), (x + w, y), (x + w, y + h), (x, y + h)].map(|p| state.xf.apply(p));
                    ops.push(PathGpuOp::Image(PathGpuImage {
                        image,
                        source_rect,
                        corners,
                        opacity: state.alpha,
                        sampling: state.sampling,
                        clip: Some(state.clip),
                        mask: state.mask,
                    }));
                }
            }
        }
        while let Some((mut parent, options)) = groups.pop() {
            parent.push(PathGpuOp::Layer(PathGpuLayer::new(ops, options)));
            ops = parent;
        }
        ops
    }
}

impl Canvas2D for GpuCanvas {
    fn current_transform(&self) -> Option<[f32; 6]> {
        Some(self.state.xf.array())
    }
    fn current_global_alpha(&self) -> Option<f32> {
        Some(self.state.alpha)
    }
    fn draw_scene(&mut self, scene: &Scene2D, options: SceneOptions) -> Result<(), SceneError> {
        scene.context(self, &options)?;
        if self.cpu.is_some() || scene.requires_cpu() {
            scene.replay(self.activate_cpu(), options)?;
        } else {
            self.layers
                .push(RecordedDraw::Scene(Box::new(CapturedScene {
                    scene: scene.clone(),
                    options,
                    state: self.state.clone(),
                })));
        }
        self.want_frame |= scene.requests_frame();
        Ok(())
    }
    fn fill_glyph_run(&mut self, run: &GlyphRun) {
        if self.glyphs_need_cpu(false) {
            self.activate_cpu().fill_glyph_run(run);
        } else {
            self.layers
                .push(RecordedDraw::Glyphs(Box::new(CapturedGlyphs {
                    run: run.clone(),
                    stroke: false,
                    state: self.state.clone(),
                })));
        }
    }
    fn stroke_glyph_run(&mut self, run: &GlyphRun) {
        if self.glyphs_need_cpu(true) {
            self.activate_cpu().stroke_glyph_run(run);
        } else {
            self.layers
                .push(RecordedDraw::Glyphs(Box::new(CapturedGlyphs {
                    run: run.clone(),
                    stroke: true,
                    state: self.state.clone(),
                })));
        }
    }

    fn draw_blurred_rounded_rect(
        &mut self,
        rect: [f32; 4],
        radius: f32,
        std_dev: f32,
        color: [f32; 4],
    ) {
        if let Some(capture) = BlurredRect::new(rect, radius, std_dev, color)
            .and_then(|b| CapturedBlur::new(b, &self.state))
        {
            if self.cpu.is_some() {
                self.activate_cpu().paint_blur(&capture);
            } else {
                self.layers.push(RecordedDraw::Blur(capture));
            }
        }
    }

    fn draw_rounded_rect_shadow(
        &mut self,
        rect: [f32; 4],
        radii: [f32; 4],
        std_dev: f32,
        color: [f32; 4],
        inset: bool,
    ) {
        if let Some(capture) = BlurredRect::shadow(rect, radii, std_dev, color, inset)
            .and_then(|b| CapturedBlur::new(b, &self.state))
        {
            if self.cpu.is_some() {
                self.activate_cpu().paint_blur(&capture);
            } else {
                self.layers.push(RecordedDraw::Blur(capture));
            }
        }
    }

    fn clip_image_mask(&mut self, mask: &Image2D) {
        self.state.clip_image_mask(mask, self.width, self.height);
    }
    fn size(&self) -> CanvasSize {
        CanvasSize {
            w: self.width as f32,
            h: self.height as f32,
        }
    }

    fn set_fill(&mut self, color: &str) {
        self.state.fill = parse_color(color);
        self.state.gradient = None;
        self.state.fill_brush = None;
    }

    fn set_fill_brush(&mut self, brush: Brush2D) {
        self.state.fill_brush = Some(brush);
        self.state.gradient = None;
    }
    fn set_stroke_brush(&mut self, brush: Brush2D) {
        self.state.stroke_brush = Some(brush);
        self.state.stroke_gradient = None;
    }
    fn set_stroke(&mut self, color: &str) {
        self.state.stroke = parse_color(color);
        self.state.stroke_gradient = None;
        self.state.stroke_brush = None;
    }
    fn set_stroke_gradient(&mut self, gradient: Gradient) {
        self.state.stroke = [1.; 4];
        self.state.stroke_gradient = Some(gradient);
        self.state.stroke_brush = None;
    }

    fn set_fill_gradient(&mut self, gradient: Gradient) {
        self.state.fill = [1.0; 4];
        self.state.gradient = Some(gradient);
        self.state.fill_brush = None;
    }
    fn set_global_alpha(&mut self, alpha: f32) {
        if alpha.is_finite() {
            self.state.alpha = alpha.clamp(0.0, 1.0);
        }
    }
    fn set_fill_rule(&mut self, rule: FillRule) {
        self.state.fill_rule = rule;
    }
    fn transform(&mut self, matrix: [f32; 6]) {
        if matrix.iter().all(|v| v.is_finite()) {
            self.state.xf = self.state.xf.mul(Affine::from_array(matrix));
        }
    }
    fn set_transform(&mut self, matrix: [f32; 6]) {
        if matrix.iter().all(|v| v.is_finite()) {
            self.state.xf = Affine::from_array(matrix);
        }
    }
    fn set_line_width(&mut self, width: f32) {
        if width.is_finite() && width > 0.0 {
            self.state.line_width = width;
        }
    }

    fn set_line_cap(&mut self, cap: LineCap) {
        self.state.line_cap = cap;
    }

    fn set_line_join(&mut self, join: LineJoin) {
        self.state.line_join = join;
    }
    fn set_line_dash(&mut self, pattern: &[f32]) {
        if let Some(dashes) = StrokeOptions::normalized_dashes(pattern) {
            self.state.stroke_options.dash_array = dashes;
        }
    }
    fn set_line_dash_offset(&mut self, offset: f32) {
        if offset.is_finite() {
            self.state.stroke_options.dash_offset = offset;
        }
    }
    fn set_miter_limit(&mut self, limit: f32) {
        if limit.is_finite() && limit >= 1.0 {
            self.state.stroke_options.miter_limit = limit;
        }
    }

    fn clear(&mut self, color: &str) {
        if self.group_depth > 0 {
            self.activate_cpu().clear(color);
            return;
        }
        self.layers.clear();
        if let Some(cpu) = self.cpu.as_mut() {
            cpu.clear(color);
        } else {
            self.layers.push(RecordedDraw::Path(
                filled_rect(0.0, 0.0, self.width as f32, self.height as f32),
                PathStyle::fill(parse_color(color)),
                None,
                None,
            ));
        }
    }

    fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        self.fill_path(&filled_rect(x, y, w.max(1e-3), h.max(1e-3)));
    }

    fn stroke_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        self.stroke_path(&polygon(&[(x, y), (x + w, y), (x + w, y + h), (x, y + h)]));
    }

    fn fill_path(&mut self, cmds: &[PathCommand]) {
        if let Some(brush) = self.state.fill_brush.clone() {
            self.push_brush_cmds(cmds, brush, self.state.fill_rule);
        } else {
            self.push_cmds(cmds, self.state.fill_style());
        }
    }

    fn stroke_path(&mut self, cmds: &[PathCommand]) {
        let outline = stroke_outline(
            cmds,
            &self.state.stroke_style(),
            0.05 / self.state.xf.scale_bound(),
        );
        if let Some(brush) = self.state.stroke_brush.clone() {
            self.push_brush_cmds(&outline, brush, FillRule::NonZero);
            return;
        }
        let color = self.state.stroke_style().stroke.unwrap();
        let mut style = PathStyle::fill(color);
        style.fill_gradient = self.state.stroke_gradient.clone();
        self.push_cmds(&outline, style);
    }

    fn translate(&mut self, dx: f32, dy: f32) {
        self.state.translate(dx, dy);
    }

    fn rotate(&mut self, deg: f32) {
        self.state.rotate(deg);
    }

    fn scale(&mut self, sx: f32, sy: f32) {
        self.state.scale(sx, sy);
    }

    fn clip_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        if self.state.xf.b.abs() > 1e-6 || self.state.xf.c.abs() > 1e-6 {
            self.clip_path(&filled_rect(x, y, w, h), FillRule::NonZero);
        } else {
            self.state.clip_rect(x, y, w, h);
        }
    }
    fn clip_path(&mut self, path: &[PathCommand], rule: FillRule) {
        self.state.clip_path(path, rule, self.width, self.height);
    }
    fn push_layer(&mut self, opacity: f32, blend: BlendMode) {
        self.push_layer_with_options(LayerOptions::new(opacity, blend));
    }
    fn push_layer_with_options(&mut self, options: LayerOptions) {
        let options = options.normalized(self.width, self.height);
        if let Some(cpu) = self.cpu.as_mut() {
            cpu.push_layer_with_options(options);
        } else {
            self.group_depth += 1;
            self.layers.push(RecordedDraw::PushLayer(options));
        }
    }
    fn pop_layer(&mut self) {
        if let Some(cpu) = self.cpu.as_mut() {
            cpu.pop_layer();
        } else if self.group_depth > 0 {
            self.group_depth -= 1;
            self.layers.push(RecordedDraw::PopLayer);
        }
    }

    fn save(&mut self) {
        self.stack.push(self.state.clone());
    }

    fn restore(&mut self) {
        if let Some(s) = self.stack.pop() {
            self.state = s;
        }
    }

    fn set_image_sampling(&mut self, sampling: ImageSampling) {
        self.state.sampling = sampling;
    }
    fn draw_image(&mut self, src: &CanvasImageSource<'_>, dx: f32, dy: f32, dw: f32, dh: f32) {
        if !src.valid_region() {
            return;
        }
        if ![dx, dy, dw, dh].iter().all(|x| x.is_finite())
            || dw <= 0.0
            || dh <= 0.0
            || self.state.xf.inverse().is_none()
        {
            return;
        }
        if self.cpu.is_some() {
            self.activate_cpu().draw_image(src, dx, dy, dw, dh);
            return;
        }
        let image = match src {
            CanvasImageSource::Image(image) => (*image).clone(),
            CanvasImageSource::Region { image, .. } => (*image).clone(),
            CanvasImageSource::Rgba {
                width,
                height,
                rgba,
            } => {
                let Some(n) = (*width as usize)
                    .checked_mul(*height as usize)
                    .and_then(|n| n.checked_mul(4))
                else {
                    return;
                };
                let Some(rgba) = rgba.get(..n) else {
                    return;
                };
                let Some(image) = Image2D::from_rgba8(*width, *height, rgba) else {
                    return;
                };
                image
            }
            CanvasImageSource::TextureKey(_) => return,
        };
        self.layers.push(RecordedDraw::Image(
            image,
            src.region(),
            [dx, dy, dw, dh],
            ImageState::capture(&self.state),
        ));
    }

    fn get_image_data(&self, x: u32, y: u32, w: u32, h: u32) -> Option<Vec<u8>> {
        if let Some(cpu) = &self.cpu {
            return cpu.get_image_data(x, y, w, h);
        }
        // Same-frame: CPU-sample recorded ops (RT not flushed yet). Also request GPU
        // copy after PathPipeline flush (低频; stalls that canvas).
        if let Some(key) = &self.tex_key {
            request_path_rt_readback(key.clone());
        }
        let mut cpu = if self.linear {
            RasterCanvas::new_linear(self.width, self.height)
        } else {
            RasterCanvas::new(self.width, self.height)
        };
        for draw in &self.layers {
            draw.replay(&mut cpu);
        }
        cpu.finish_layers();
        cpu.get_image_data(x, y, w, h)
    }

    fn put_image_data(&mut self, x: u32, y: u32, w: u32, h: u32, rgba: &[u8]) {
        self.activate_cpu().put_image_data(x, y, w, h, rgba);
    }

    fn request_frame(&mut self) {
        self.want_frame = true;
    }

    fn is_point_in_path(&self, path: &[PathCommand], x: f32, y: f32) -> bool {
        canvas_point_in_path(&self.state, path, x, y)
    }

    fn is_point_in_stroke(&self, path: &[PathCommand], x: f32, y: f32) -> bool {
        canvas_point_in_stroke(&self.state, path, x, y)
    }
}
