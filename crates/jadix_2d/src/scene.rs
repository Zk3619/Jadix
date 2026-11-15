//! Immutable reusable high-level scenes, replayed through the existing Canvas API.
use super::*;
#[path = "scene/sampling.rs"]
mod sampling;
pub use sampling::{SampledScene2D, SceneSampling};
use std::{cell::Cell, sync::Arc};

/// Recording or preflight validation failure. Built-in canvases reject failed
/// preflight without painting or changing caller state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneError {
    InvalidSize,
    InvalidTransform,
    InvalidOptions,
    CommandBudget,
    PathBudget,
    ImageBudget,
    MaskBudget,
    ExpandedBudget,
    NestingBudget,
    UnbalancedState,
    UnsupportedOperation,
    UnsupportedCanvas,
}
impl std::fmt::Display for SceneError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "2D scene: {self:?}")
    }
}
impl std::error::Error for SceneError {}

/// Bounds for retained commands, expanded work and remapped masks. These exclude
/// shared resources, tessellation, ordinary path clips and renderer scratch space.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneLimits {
    pub max_commands: usize,
    pub max_path_commands: usize,
    /// Budget for owned copies of borrowed RGBA sources, not shared Image2D data.
    pub max_copied_image_bytes: usize,
    /// Conservative sum of final RGBA payload bytes for remapped masks,
    /// including nested instances; excludes staging copies and GPU conversion.
    /// This is not a total process/VRAM limit.
    pub max_replay_mask_bytes: usize,
    pub max_expanded_commands: usize,
    pub max_nesting: usize,
}
impl Default for SceneLimits {
    fn default() -> Self {
        Self {
            max_commands: 65536,
            max_path_commands: 1_000_000,
            max_copied_image_bytes: 64 * 1024 * 1024,
            max_replay_mask_bytes: 64 * 1024 * 1024,
            max_expanded_commands: 1_000_000,
            max_nesting: 32,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SceneStats {
    /// Commands retained directly by this scene; nested scenes remain Arc references.
    pub commands: usize,
    /// Retained path commands plus dash entries; child data is shared, not counted.
    pub path_commands: usize,
    pub copied_image_bytes: usize,
    /// Conservative count of full-target mask remappings, including children.
    pub mask_mappings: usize,
    /// Logical replay cost including each occurrence of nested shared scenes.
    pub expanded_commands: usize,
    /// Combined save/layer/scene nesting bound, not a GPU stack size claim.
    pub nesting: usize,
}
#[derive(Clone, Debug, PartialEq)]
pub struct SceneOptions {
    /// Appended to the caller CTM. Scene set/reset_transform stay relative to it.
    pub transform: [f32; 6],
    /// None appends inline (caller alpha per draw). Some isolates and applies
    /// caller alpha times layer opacity ONCE to the completed instance.
    /// This outer layer's clip/mask/filter units are target device pixels.
    pub layer: Option<LayerOptions>,
    /// Clip content to the transformed local scene rectangle. Filters may extend
    /// beyond this input clip. False leaves the declared viewport informational.
    pub clip_to_bounds: bool,
}
impl Default for SceneOptions {
    fn default() -> Self {
        Self {
            transform: [1., 0., 0., 1., 0., 0.],
            layer: None,
            clip_to_bounds: false,
        }
    }
}
#[derive(Clone, Debug)]
enum Command {
    Fill(Arc<str>),
    Stroke(Arc<str>),
    FillBrush(Brush2D),
    StrokeBrush(Brush2D),
    FillGradient(Gradient),
    StrokeGradient(Gradient),
    Alpha(f32),
    Rule(FillRule),
    Transform([f32; 6]),
    SetTransform([f32; 6]),
    Width(f32),
    Cap(LineCap),
    Join(LineJoin),
    Dash(Arc<[f32]>),
    DashOffset(f32),
    Miter(f32),
    FillPath(Arc<[PathCommand]>),
    StrokePath(Arc<[PathCommand]>),
    Glyph(GlyphRun, bool),
    Blur([f32; 4], [f32; 4], f32, [f32; 4], bool, bool),
    Clip([f32; 4]),
    ClipPath(Arc<[PathCommand]>, FillRule),
    Mask(Image2D),
    Push(LayerOptions),
    Pop,
    Save,
    Restore,
    Sampling(ImageSampling),
    Image(Image2D, Option<[u32; 4]>, [f32; 4]),
    Scene(Scene2D, SceneOptions),
    RequestFrame,
}
#[derive(Debug)]
struct SceneData {
    point_sample_masks: bool,
    size: [u32; 2],
    commands: Vec<Command>,
    stats: SceneStats,
    max_replay_mask_bytes: usize,
}
/// Frozen commands and shared images/gradients/glyphs/subscenes. Recording does
/// not allocate a canvas bitmap, rasterize, tessellate or keep the callback.
/// It is an acyclic snapshot: children already exist before a parent is sealed.
///
/// ```
/// use jadix_2d::{Canvas2D, RasterCanvas, Scene2D, SceneOptions};
/// let icon = Scene2D::record([24, 24], |c| {
///     c.set_fill("#20a0ff");
///     c.fill_rect(2., 2., 20., 20.);
/// }).unwrap();
/// let mut canvas = RasterCanvas::new_linear(64, 32);
/// for x in [4., 34.] {
///     icon.draw(&mut canvas, SceneOptions {
///         transform: [1., 0., 0., 1., x, 4.],
///         ..Default::default()
///     }).unwrap();
/// }
/// ```
#[derive(Clone, Debug)]
pub struct Scene2D(Arc<SceneData>);
impl Scene2D {
    /// Execute the callback once against a recorder with deterministic defaults.
    /// Clear, pixel IO/hit queries and host texture keys are unsupported.
    pub fn record(
        size: [u32; 2],
        paint: impl FnOnce(&mut dyn Canvas2D),
    ) -> Result<Self, SceneError> {
        Self::record_with_limits(size, SceneLimits::default(), paint)
    }
    pub fn record_with_limits(
        size: [u32; 2],
        limits: SceneLimits,
        paint: impl FnOnce(&mut dyn Canvas2D),
    ) -> Result<Self, SceneError> {
        if size.contains(&0) || size.iter().any(|v| *v > 1_000_000) {
            return Err(SceneError::InvalidSize);
        }
        if limits.max_commands == 0
            || limits.max_commands > 1_000_000
            || limits.max_path_commands == 0
            || limits.max_path_commands > 4_000_000
            || limits.max_copied_image_bytes > 256 * 1024 * 1024
            || limits.max_replay_mask_bytes > 256 * 1024 * 1024
            || limits.max_expanded_commands == 0
            || limits.max_expanded_commands > 4_000_000
            || limits.max_nesting == 0
            || limits.max_nesting > 64
        {
            return Err(SceneError::InvalidOptions);
        }
        let mut recorder = Recorder {
            size,
            limits,
            commands: Vec::new(),
            stats: SceneStats::default(),
            error: Cell::new(None),
            state: (Affine::identity(), 1.),
            stack: Vec::new(),
            groups: 0,
        };
        paint(&mut recorder);
        if let Some(e) = recorder.error.get() {
            return Err(e);
        }
        if !recorder.stack.is_empty() || recorder.groups != 0 {
            return Err(SceneError::UnbalancedState);
        }
        Ok(Self(Arc::new(SceneData {
            point_sample_masks: false,
            size,
            commands: recorder.commands,
            stats: recorder.stats,
            max_replay_mask_bytes: limits.max_replay_mask_bytes,
        })))
    }
    pub fn size(&self) -> [u32; 2] {
        self.0.size
    }
    /// Clear this handle without modifying any clones or retained subscenes.
    /// Unique command storage keeps its capacity; shared storage detaches empty.
    pub fn reset(&mut self) {
        self.reset_and_resize(self.size())
            .expect("existing scene size");
    }
    /// Reset and change the declared viewport. Invalid sizes leave the scene intact.
    pub fn reset_and_resize(&mut self, size: [u32; 2]) -> Result<(), SceneError> {
        if size.contains(&0) || size.iter().any(|v| *v > 1_000_000) {
            return Err(SceneError::InvalidSize);
        }
        if let Some(data) = Arc::get_mut(&mut self.0) {
            data.commands.clear();
            data.stats = SceneStats::default();
            data.size = size;
            data.point_sample_masks = false;
        } else {
            self.0 = Arc::new(SceneData {
                point_sample_masks: false,
                size,
                commands: Vec::new(),
                stats: SceneStats::default(),
                max_replay_mask_bytes: self.0.max_replay_mask_bytes,
            });
        }
        Ok(())
    }
    /// Atomically replace recorded commands. Failure preserves this handle and
    /// snapshots; no callback is retained. Uses a fresh validated command vector.
    pub fn rerecord(
        &mut self,
        size: [u32; 2],
        limits: SceneLimits,
        paint: impl FnOnce(&mut dyn Canvas2D),
    ) -> Result<(), SceneError> {
        let next = Self::record_with_limits(size, limits, paint)?;
        *self = next;
        Ok(())
    }
    pub fn stats(&self) -> SceneStats {
        self.0.stats
    }
    pub fn requests_frame(&self) -> bool {
        self.0.commands.iter().any(|c| {
            matches!(c, Command::RequestFrame)
                || matches!(c,Command::Scene(scene,_) if scene.requests_frame())
        })
    }
    pub(super) fn requires_cpu(&self) -> bool {
        self.0.commands.iter().any(|c| match c {
            Command::FillGradient(g) | Command::StrokeGradient(g) => pack_gpu_gradient(g).is_none(),
            Command::FillBrush(Brush2D::Gradient { gradient, .. })
            | Command::StrokeBrush(Brush2D::Gradient { gradient, .. }) => {
                pack_gpu_gradient(gradient).is_none()
            }
            Command::Scene(s, _) => s.requires_cpu(),
            _ => false,
        })
    }
    /// Append to a canvas without changing its styles or state. The caller's
    /// clip/mask constrain content; an optional outer layer clips its final output.
    /// Finite invertible instance transforms are required. GPU recording defers
    /// expansion, but rendering still tessellates each instance independently.
    pub fn draw<C: Canvas2D + ?Sized>(
        &self,
        canvas: &mut C,
        options: SceneOptions,
    ) -> Result<(), SceneError> {
        canvas.draw_scene(self, options)
    }
    /// Check built-in replay requirements without modifying the destination.
    pub fn validate<C: Canvas2D + ?Sized>(
        &self,
        canvas: &C,
        options: &SceneOptions,
    ) -> Result<(), SceneError> {
        self.context(canvas, options).map(|_| ())
    }
    pub(super) fn context<C: Canvas2D + ?Sized>(
        &self,
        c: &C,
        options: &SceneOptions,
    ) -> Result<(Affine, f32), SceneError> {
        let size = c.size();
        if !size.w.is_finite()
            || !size.h.is_finite()
            || size.w < 1.
            || size.h < 1.
            || size.w > 1_000_000.
            || size.h > 1_000_000.
        {
            return Err(SceneError::InvalidSize);
        }
        let base = c.current_transform().ok_or(SceneError::UnsupportedCanvas)?;
        let alpha = c
            .current_global_alpha()
            .ok_or(SceneError::UnsupportedCanvas)?;
        let base = Affine::from_array(base).mul(Affine::from_array(options.transform));
        if !valid_matrix(options.transform)
            || !valid_matrix(base.array())
            || base.inverse().is_none()
        {
            return Err(SceneError::InvalidTransform);
        }
        if !alpha.is_finite() || !(0. ..=1.).contains(&alpha) {
            return Err(SceneError::UnsupportedCanvas);
        }
        if options
            .layer
            .as_ref()
            .is_some_and(|l| !l.opacity.is_finite())
        {
            return Err(SceneError::InvalidOptions);
        }
        let mask_bytes = (size.w as usize)
            .saturating_mul(size.h as usize)
            .saturating_mul(4)
            .saturating_mul(self.stats().mask_mappings);
        if mask_bytes > self.0.max_replay_mask_bytes {
            return Err(SceneError::MaskBudget);
        }
        self.check_child_mask_budgets([size.w as u32, size.h as u32])?;
        self.check_transforms(base)?;
        Ok((base, alpha))
    }
    fn check_child_mask_budgets(&self, target: [u32; 2]) -> Result<(), SceneError> {
        let bytes = (target[0] as usize)
            .saturating_mul(target[1] as usize)
            .saturating_mul(4);
        for cmd in &self.0.commands {
            if let Command::Scene(scene, _) = cmd {
                if bytes.saturating_mul(scene.stats().mask_mappings) > scene.0.max_replay_mask_bytes
                {
                    return Err(SceneError::MaskBudget);
                }
                scene.check_child_mask_budgets(target)?;
            }
        }
        Ok(())
    }
    // Validate the complete transform walk before mutating the destination, so
    // a nested overflowing matrix cannot leave a half-painted instance.
    fn check_transforms(&self, base: Affine) -> Result<(), SceneError> {
        if !valid_matrix(base.array()) || base.inverse().is_none() {
            return Err(SceneError::InvalidTransform);
        }
        let mut current = base;
        let mut stack = Vec::new();
        for cmd in &self.0.commands {
            match cmd {
                Command::Transform(m) => current = current.mul(Affine::from_array(*m)),
                Command::SetTransform(m) => current = base.mul(Affine::from_array(*m)),
                Command::Save => stack.push(current),
                Command::Restore => current = stack.pop().unwrap(),
                Command::Scene(scene, options) => {
                    scene.check_transforms(current.mul(Affine::from_array(options.transform)))?
                }
                _ => {}
            }
            if !valid_matrix(current.array()) {
                return Err(SceneError::InvalidTransform);
            }
        }
        Ok(())
    }
    pub(super) fn replay<C: Canvas2D + ?Sized>(
        &self,
        c: &mut C,
        options: SceneOptions,
    ) -> Result<(), SceneError> {
        let (base, alpha) = self.context(c, &options)?;
        let target = c.size();
        let target = [target.w as u32, target.h as u32];
        c.save();
        let isolated = options.layer.is_some();
        if let Some(mut layer) = options.layer {
            layer.opacity = layer.opacity.clamp(0., 1.) * alpha;
            c.push_layer_with_options(layer);
        }
        c.set_transform(base.array());
        // Scene styles have deterministic defaults independent of the caller.
        c.set_fill("#000000");
        c.set_stroke("#000000");
        c.set_line_width(1.);
        c.set_line_cap(LineCap::Round);
        c.set_line_join(LineJoin::Round);
        c.set_line_dash(&[]);
        c.set_line_dash_offset(0.);
        c.set_miter_limit(4.);
        c.set_fill_rule(FillRule::NonZero);
        c.set_image_sampling(ImageSampling::Nearest);
        let inherited = if isolated { 1. } else { alpha };
        c.set_global_alpha(inherited);
        if options.clip_to_bounds {
            c.clip_rect(0., 0., self.0.size[0] as f32, self.0.size[1] as f32);
        }
        for cmd in &self.0.commands {
            match cmd {
                Command::Fill(v) => c.set_fill(v),
                Command::Stroke(v) => c.set_stroke(v),
                Command::FillBrush(v) => c.set_fill_brush(v.clone()),
                Command::StrokeBrush(v) => c.set_stroke_brush(v.clone()),
                Command::FillGradient(v) => c.set_fill_gradient(v.clone()),
                Command::StrokeGradient(v) => c.set_stroke_gradient(v.clone()),
                Command::Alpha(v) => c.set_global_alpha(v * inherited),
                Command::Rule(v) => c.set_fill_rule(*v),
                Command::Transform(v) => c.transform(*v),
                Command::SetTransform(v) => {
                    c.set_transform(base.mul(Affine::from_array(*v)).array())
                }
                Command::Width(v) => c.set_line_width(*v),
                Command::Cap(v) => c.set_line_cap(*v),
                Command::Join(v) => c.set_line_join(*v),
                Command::Dash(v) => c.set_line_dash(v),
                Command::DashOffset(v) => c.set_line_dash_offset(*v),
                Command::Miter(v) => c.set_miter_limit(*v),
                Command::FillPath(v) => c.fill_path(v),
                Command::StrokePath(v) => c.stroke_path(v),
                Command::Glyph(v, false) => c.fill_glyph_run(v),
                Command::Glyph(v, true) => c.stroke_glyph_run(v),
                Command::Blur(rect, radii, sigma, color, inset, extended) => {
                    if *extended {
                        c.draw_rounded_rect_shadow(*rect, *radii, *sigma, *color, *inset);
                    } else {
                        c.draw_blurred_rounded_rect(*rect, radii[0], *sigma, *color);
                    }
                }
                Command::Clip([x, y, w, h]) => c.clip_rect(*x, *y, *w, *h),
                Command::ClipPath(v, r) => c.clip_path(v, *r),
                Command::Mask(mask) => c.clip_image_mask(&map_mask(
                    Some(mask),
                    None,
                    base,
                    target,
                    self.0.point_sample_masks,
                )),
                Command::Push(layer) => c.push_layer_with_options(map_layer(
                    layer.clone(),
                    base,
                    target,
                    self.0.point_sample_masks,
                )),
                Command::Pop => c.pop_layer(),
                Command::Save => c.save(),
                Command::Restore => c.restore(),
                Command::Sampling(v) => c.set_image_sampling(*v),
                Command::Image(image, rect, [x, y, w, h]) => {
                    let source = rect.map_or(CanvasImageSource::Image(image), |rect| {
                        CanvasImageSource::Region { image, rect }
                    });
                    c.draw_image(&source, *x, *y, *w, *h);
                }
                Command::Scene(scene, options) => {
                    let mut options = options.clone();
                    options.layer = options
                        .layer
                        .map(|layer| map_layer(layer, base, target, self.0.point_sample_masks));
                    scene.draw(c, options).expect("validated frozen scene");
                }
                Command::RequestFrame => c.request_frame(),
            }
        }
        if isolated {
            c.pop_layer();
        }
        c.restore();
        Ok(())
    }
}
fn valid_matrix(m: [f32; 6]) -> bool {
    m.iter().all(|v| v.is_finite() && v.abs() <= 1e12)
}
fn map_layer(
    mut layer: LayerOptions,
    base: Affine,
    target: [u32; 2],
    point_sample: bool,
) -> LayerOptions {
    if layer.mask.is_some() || layer.clip.is_some() {
        layer.mask = Some(map_mask(
            layer.mask.as_ref(),
            layer.clip,
            base,
            target,
            point_sample,
        ));
        layer.clip = None;
    }
    layer
}
// Layer clips/masks and image masks recorded in scene-device coordinates move
// with the instance base, independently of local CTM changes. Filters stay in
// target device pixels, matching Canvas LayerFilter's explicit unit contract.
fn map_mask(
    mask: Option<&Image2D>,
    clip: Option<[f32; 4]>,
    base: Affine,
    target: [u32; 2],
    point_sample: bool,
) -> Image2D {
    if clip.is_none() && base.array() == Affine::identity().array() {
        if let Some(mask) = mask.filter(|m| [m.width(), m.height()] == target) {
            return mask.clone();
        }
    }
    let inverse = base.inverse();
    let mut rgba = vec![0u8; target[0] as usize * target[1] as usize * 4];
    if let Some(inverse) = inverse {
        for y in 0..target[1] {
            for x in 0..target[0] {
                let mut a = 0.;
                let samples = if point_sample { 1 } else { 2 };
                for sy in 0..samples {
                    for sx in 0..samples {
                        let p = inverse.apply((
                            x as f32 + (sx as f32 + 0.5) / samples as f32,
                            y as f32 + (sy as f32 + 0.5) / samples as f32,
                        ));
                        if clip.is_some_and(|[cx, cy, w, h]| {
                            !cx.is_finite()
                                || !cy.is_finite()
                                || !w.is_finite()
                                || !h.is_finite()
                                || p.0 < cx
                                || p.1 < cy
                                || p.0 >= cx + w
                                || p.1 >= cy + h
                        }) {
                            continue;
                        }
                        a += mask.map_or(1., |m| {
                            if p.0 < 0.
                                || p.1 < 0.
                                || p.0 >= m.width() as f32
                                || p.1 >= m.height() as f32
                            {
                                0.
                            } else {
                                crate::image::sample(
                                    m.rgba8(),
                                    m.width(),
                                    m.height(),
                                    p.0 / m.width() as f32,
                                    p.1 / m.height() as f32,
                                    ImageSampling::Bilinear,
                                )[3]
                            }
                        }) / (samples * samples) as f32;
                    }
                }
                rgba[((y * target[0] + x) * 4 + 3) as usize] = (a * 255.).round() as u8;
            }
        }
    }
    Image2D::from_rgba8(target[0], target[1], &rgba).expect("nonempty destination")
}

struct Recorder {
    size: [u32; 2],
    limits: SceneLimits,
    commands: Vec<Command>,
    stats: SceneStats,
    error: Cell<Option<SceneError>>,
    state: (Affine, f32),
    stack: Vec<(Affine, f32)>,
    groups: usize,
}
impl Recorder {
    fn fail(&self, e: SceneError) {
        if self.error.get().is_none() {
            self.error.set(Some(e));
        }
    }
    fn add(&mut self, command: Command) {
        if self.error.get().is_some() {
            return;
        }
        let expanded = match &command {
            Command::Scene(s, _) => 1 + s.stats().expanded_commands,
            _ => 1,
        };
        let masks = match &command {
            Command::Mask(_) => 1,
            Command::Push(l) => usize::from(l.clip.is_some() || l.mask.is_some()),
            Command::Scene(s, options) => {
                s.stats().mask_mappings
                    + usize::from(
                        options
                            .layer
                            .as_ref()
                            .is_some_and(|l| l.clip.is_some() || l.mask.is_some()),
                    )
            }
            _ => 0,
        };
        let nesting = self.stack.len()
            + self.groups
            + match &command {
                Command::Scene(s, _) => 1 + s.stats().nesting,
                _ => 0,
            };
        if self.stats.commands >= self.limits.max_commands {
            self.fail(SceneError::CommandBudget);
            return;
        }
        if self.stats.expanded_commands.saturating_add(expanded) > self.limits.max_expanded_commands
        {
            self.fail(SceneError::ExpandedBudget);
            return;
        }
        if nesting > self.limits.max_nesting {
            self.fail(SceneError::NestingBudget);
            return;
        }
        self.stats.commands += 1;
        self.stats.expanded_commands += expanded;
        self.stats.mask_mappings += masks;
        self.stats.nesting = self.stats.nesting.max(nesting);
        self.commands.push(command);
    }
    fn path(&mut self, path: &[PathCommand], kind: u8, rule: FillRule) {
        if self.error.get().is_some() {
            return;
        }
        if path.iter().any(|cmd| match *cmd {
            PathCommand::MoveTo(p) | PathCommand::LineTo(p) => !p.0.is_finite() || !p.1.is_finite(),
            PathCommand::QuadTo { ctrl, to } => {
                [ctrl.0, ctrl.1, to.0, to.1].iter().any(|v| !v.is_finite())
            }
            PathCommand::CubicTo { c1, c2, to } => [c1.0, c1.1, c2.0, c2.1, to.0, to.1]
                .iter()
                .any(|v| !v.is_finite()),
            PathCommand::Arc {
                radii,
                x_rot_deg,
                to,
                ..
            } => [radii.0, radii.1, x_rot_deg, to.0, to.1]
                .iter()
                .any(|v| !v.is_finite()),
            PathCommand::Close => false,
        }) {
            self.fail(SceneError::InvalidOptions);
            return;
        }
        if self.stats.path_commands.saturating_add(path.len()) > self.limits.max_path_commands {
            self.fail(SceneError::PathBudget);
            return;
        }
        self.stats.path_commands += path.len();
        self.add(match kind {
            0 => Command::FillPath(path.into()),
            1 => Command::StrokePath(path.into()),
            _ => Command::ClipPath(path.into(), rule),
        });
    }
}
impl Canvas2D for Recorder {
    fn size(&self) -> CanvasSize {
        CanvasSize {
            w: self.size[0] as f32,
            h: self.size[1] as f32,
        }
    }
    fn current_transform(&self) -> Option<[f32; 6]> {
        Some(self.state.0.array())
    }
    fn current_global_alpha(&self) -> Option<f32> {
        Some(self.state.1)
    }
    fn draw_scene(&mut self, scene: &Scene2D, options: SceneOptions) -> Result<(), SceneError> {
        if let Err(e) = scene.context(self, &options) {
            self.fail(e);
            return Err(e);
        }
        self.add(Command::Scene(scene.clone(), options));
        self.error.get().map_or(Ok(()), Err)
    }
    fn set_fill(&mut self, v: &str) {
        self.add(Command::Fill(v.into()));
    }
    fn set_stroke(&mut self, v: &str) {
        self.add(Command::Stroke(v.into()));
    }
    fn set_fill_brush(&mut self, v: Brush2D) {
        self.add(Command::FillBrush(v));
    }
    fn set_stroke_brush(&mut self, v: Brush2D) {
        self.add(Command::StrokeBrush(v));
    }
    fn set_fill_gradient(&mut self, v: Gradient) {
        self.add(Command::FillGradient(v));
    }
    fn set_stroke_gradient(&mut self, v: Gradient) {
        self.add(Command::StrokeGradient(v));
    }
    fn set_global_alpha(&mut self, v: f32) {
        if v.is_finite() {
            self.state.1 = v.clamp(0., 1.);
            self.add(Command::Alpha(self.state.1));
        }
    }
    fn set_fill_rule(&mut self, v: FillRule) {
        self.add(Command::Rule(v));
    }
    fn transform(&mut self, v: [f32; 6]) {
        if valid_matrix(v) {
            self.state.0 = self.state.0.mul(Affine::from_array(v));
            self.add(Command::Transform(v));
        } else {
            self.fail(SceneError::InvalidTransform);
        }
    }
    fn set_transform(&mut self, v: [f32; 6]) {
        if valid_matrix(v) {
            self.state.0 = Affine::from_array(v);
            self.add(Command::SetTransform(v));
        } else {
            self.fail(SceneError::InvalidTransform);
        }
    }
    fn translate(&mut self, x: f32, y: f32) {
        self.transform(Affine::translate(x, y).array());
    }
    fn rotate(&mut self, v: f32) {
        self.transform(Affine::rotate_deg(v).array());
    }
    fn scale(&mut self, x: f32, y: f32) {
        self.transform(Affine::scale(x, y).array());
    }
    fn set_line_width(&mut self, v: f32) {
        self.add(Command::Width(v));
    }
    fn set_line_cap(&mut self, v: LineCap) {
        self.add(Command::Cap(v));
    }
    fn set_line_join(&mut self, v: LineJoin) {
        self.add(Command::Join(v));
    }
    fn set_line_dash(&mut self, v: &[f32]) {
        if self.stats.path_commands.saturating_add(v.len()) > self.limits.max_path_commands {
            self.fail(SceneError::PathBudget);
        } else {
            self.stats.path_commands += v.len();
            self.add(Command::Dash(v.into()));
        }
    }
    fn set_line_dash_offset(&mut self, v: f32) {
        self.add(Command::DashOffset(v));
    }
    fn set_miter_limit(&mut self, v: f32) {
        self.add(Command::Miter(v));
    }
    fn clear(&mut self, _: &str) {
        self.fail(SceneError::UnsupportedOperation);
    }
    fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        if w > 0. && h > 0. {
            self.fill_path(&filled_rect(x, y, w, h));
        }
    }
    fn stroke_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        if w > 0. && h > 0. {
            self.stroke_path(&filled_rect(x, y, w, h));
        }
    }
    fn fill_path(&mut self, v: &[PathCommand]) {
        self.path(v, 0, FillRule::NonZero);
    }
    fn stroke_path(&mut self, v: &[PathCommand]) {
        self.path(v, 1, FillRule::NonZero);
    }
    fn fill_glyph_run(&mut self, v: &GlyphRun) {
        self.add(Command::Glyph(v.clone(), false));
    }
    fn stroke_glyph_run(&mut self, v: &GlyphRun) {
        self.add(Command::Glyph(v.clone(), true));
    }
    fn draw_blurred_rounded_rect(&mut self, r: [f32; 4], radius: f32, s: f32, c: [f32; 4]) {
        self.add(Command::Blur(r, [radius; 4], s, c, false, false));
    }
    fn draw_rounded_rect_shadow(
        &mut self,
        r: [f32; 4],
        radius: [f32; 4],
        s: f32,
        c: [f32; 4],
        inset: bool,
    ) {
        self.add(Command::Blur(r, radius, s, c, inset, true));
    }
    fn clip_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.add(Command::Clip([x, y, w, h]));
    }
    fn clip_path(&mut self, v: &[PathCommand], r: FillRule) {
        self.path(v, 2, r);
    }
    fn clip_image_mask(&mut self, v: &Image2D) {
        if [v.width(), v.height()] != self.size {
            self.fail(SceneError::InvalidSize);
        } else {
            self.add(Command::Mask(v.clone()));
        }
    }
    fn push_layer(&mut self, o: f32, b: BlendMode) {
        self.push_layer_with_options(LayerOptions::new(o, b));
    }
    fn push_layer_with_options(&mut self, v: LayerOptions) {
        if self.error.get().is_some() {
            return;
        }
        if v.mask
            .as_ref()
            .is_some_and(|m| [m.width(), m.height()] != self.size)
        {
            self.fail(SceneError::InvalidSize);
            return;
        }
        self.groups += 1;
        self.add(Command::Push(v));
    }
    fn pop_layer(&mut self) {
        if self.groups == 0 {
            self.fail(SceneError::UnbalancedState);
        } else {
            self.groups -= 1;
            self.add(Command::Pop);
        }
    }
    fn save(&mut self) {
        if self.stack.len() >= self.limits.max_nesting {
            self.fail(SceneError::NestingBudget);
            return;
        }
        self.stack.push(self.state);
        self.add(Command::Save);
    }
    fn restore(&mut self) {
        if let Some(s) = self.stack.pop() {
            self.state = s;
            self.add(Command::Restore);
        } else {
            self.fail(SceneError::UnbalancedState);
        }
    }
    fn set_image_sampling(&mut self, v: ImageSampling) {
        self.add(Command::Sampling(v));
    }
    fn draw_image(&mut self, src: &CanvasImageSource<'_>, x: f32, y: f32, w: f32, h: f32) {
        if self.error.get().is_some() {
            return;
        }
        if !src.valid_region() {
            self.fail(SceneError::InvalidSize);
            return;
        }
        let image = match src {
            CanvasImageSource::Image(v) | CanvasImageSource::Region { image: v, .. } => {
                (*v).clone()
            }
            CanvasImageSource::Rgba {
                width,
                height,
                rgba,
            } => {
                if self.stats.copied_image_bytes.saturating_add(rgba.len())
                    > self.limits.max_copied_image_bytes
                {
                    self.fail(SceneError::ImageBudget);
                    return;
                }
                self.stats.copied_image_bytes += rgba.len();
                let Some(image) = Image2D::from_rgba8(*width, *height, rgba) else {
                    self.fail(SceneError::InvalidSize);
                    return;
                };
                image
            }
            CanvasImageSource::TextureKey(_) => {
                self.fail(SceneError::UnsupportedOperation);
                return;
            }
        };
        self.add(Command::Image(image, src.region(), [x, y, w, h]));
    }
    fn get_image_data(&self, _: u32, _: u32, _: u32, _: u32) -> Option<Vec<u8>> {
        self.fail(SceneError::UnsupportedOperation);
        None
    }
    fn put_image_data(&mut self, _: u32, _: u32, _: u32, _: u32, _: &[u8]) {
        self.fail(SceneError::UnsupportedOperation);
    }
    fn is_point_in_path(&self, _: &[PathCommand], _: f32, _: f32) -> bool {
        self.fail(SceneError::UnsupportedOperation);
        false
    }
    fn is_point_in_stroke(&self, _: &[PathCommand], _: f32, _: f32) -> bool {
        self.fail(SceneError::UnsupportedOperation);
        false
    }
    fn request_frame(&mut self) {
        self.add(Command::RequestFrame);
    }
}
