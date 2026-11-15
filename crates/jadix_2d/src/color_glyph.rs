//! COLRv0/v1 paint graphs through the existing scene, brush and layer pipeline.
use crate::*;
use skrifa::{
    color::{Brush, ColorPainter, CompositeMode as ColrComposite, Extend, Transform},
    instance::{Location, Size},
    outline::{DrawSettings, OutlinePen},
    raw::types::BoundingBox,
    FontRef, GlyphId, MetadataProvider,
};
use std::{collections::HashMap, sync::Arc};

#[derive(Clone, Copy, Debug)]
pub struct ColorGlyphOptions {
    pub palette: u16,
    /// Straight display-sRGB foreground used by palette index 0xFFFF.
    pub foreground: [f32; 4],
    pub limits: SceneLimits,
    /// SVG bitmap pixels per logical unit; COLR ink remains vector.
    pub raster_scale: f32,
    /// Both compressed input and expanded SVG document byte limit, <=8 MiB.
    pub max_svg_bytes: usize,
}
impl Default for ColorGlyphOptions {
    fn default() -> Self {
        Self {
            palette: 0,
            foreground: [0., 0., 0., 1.],
            limits: SceneLimits::default(),
            raster_scale: 1.,
            max_svg_bytes: 4 * 1024 * 1024,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorGlyphError {
    Font(FontError),
    InvalidOptions,
    InvalidPalette,
    NotColorGlyph,
    MalformedPaint,
    UnboundedPaint,
    UnsupportedSvg,
    MalformedSvg,
    SvgBudget,
    Scene(SceneError),
}
impl std::fmt::Display for ColorGlyphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "2D color glyph: {self:?}")
    }
}
impl std::error::Error for ColorGlyphError {}
impl From<SceneError> for ColorGlyphError {
    fn from(e: SceneError) -> Self {
        Self::Scene(e)
    }
}

/// Prepared scalable COLRv0/v1 glyphs. Palettes, foreground, variations, outlines
/// and paint graphs are resolved once; drawing needs no font or bitmap atlas.
#[derive(Clone, Debug)]
pub struct ColorGlyphRun {
    scene: Scene2D,
    glyphs: usize,
    unique: usize,
    bounds: Option<[f32; 4]>,
    bitmap_bytes: usize,
}
impl ColorGlyphRun {
    pub fn new(
        font: &Font2D,
        size: f32,
        glyphs: &[PositionedGlyph],
        options: ColorGlyphOptions,
    ) -> Result<Self, ColorGlyphError> {
        let face = font.face().map_err(ColorGlyphError::Font)?;
        if !size.is_finite()
            || size <= 0.
            || size > 1e6
            || options.foreground.iter().any(|v| !v.is_finite())
            || !options.raster_scale.is_finite()
            || !(0.25..=8.).contains(&options.raster_scale)
            || options.max_svg_bytes == 0
            || options.max_svg_bytes > 8 * 1024 * 1024
        {
            return Err(ColorGlyphError::InvalidOptions);
        }
        if glyphs.len() > options.limits.max_commands {
            return Err(SceneError::CommandBudget.into());
        }
        Scene2D::record_with_limits([1, 1], options.limits, |_| {})?;
        let source = FontRef::from_index(&font.data, font.index)
            .map_err(|_| ColorGlyphError::Font(FontError::InvalidFont))?;
        let location = source.axes().location(
            font.variations
                .iter()
                .map(|(t, v)| (skrifa::raw::types::Tag::new(t), *v)),
        );
        let palettes = source.color_palettes();
        let palette = if palettes.is_empty() && options.palette == 0 {
            None
        } else {
            Some(
                palettes
                    .get(options.palette)
                    .ok_or(ColorGlyphError::InvalidPalette)?,
            )
        };
        let colors = palette
            .as_ref()
            .map_or(&[][..], |p| p.colors())
            .iter()
            .map(|c| {
                [
                    c.red() as f32 / 255.,
                    c.green() as f32 / 255.,
                    c.blue() as f32 / 255.,
                    c.alpha() as f32 / 255.,
                ]
            })
            .collect::<Vec<_>>();
        let mut prepared = HashMap::new();
        let mut paths = 0usize;
        #[cfg(feature = "svg-fonts")]
        let mut bitmap_bytes = 0usize;
        #[cfg(not(feature = "svg-fonts"))]
        let bitmap_bytes = 0usize;
        for g in glyphs {
            if !g.x.is_finite() || !g.y.is_finite() || g.x.abs() > 1e6 || g.y.abs() > 1e6 {
                return Err(ColorGlyphError::InvalidOptions);
            }
            if prepared.contains_key(&g.id) {
                continue;
            }
            let mut limits = options.limits;
            limits.max_path_commands = limits.max_path_commands.saturating_sub(paths);
            if limits.max_path_commands == 0 {
                return Err(SceneError::PathBudget.into());
            }
            let (scene, bounds) = if source
                .color_glyphs()
                .get(GlyphId::new(g.id as u32))
                .is_some()
            {
                prepare(
                    &source,
                    &location,
                    GlyphId::new(g.id as u32),
                    &colors,
                    options.foreground.map(|v| v.clamp(0., 1.)),
                    limits,
                )?
            } else if let Some(doc) = face.glyph_svg_image(ttf_parser::GlyphId(g.id)) {
                #[cfg(feature = "svg-fonts")]
                {
                    let (scene, bounds, bytes) = crate::svg_glyph::prepare_svg(
                        doc,
                        g.id,
                        face.units_per_em(),
                        size,
                        &colors,
                        options,
                        options
                            .limits
                            .max_copied_image_bytes
                            .saturating_sub(bitmap_bytes),
                    )?;
                    bitmap_bytes += bytes;
                    (scene, Some(bounds))
                }
                #[cfg(not(feature = "svg-fonts"))]
                {
                    let _ = doc;
                    return Err(ColorGlyphError::UnsupportedSvg);
                }
            } else {
                return Err(ColorGlyphError::NotColorGlyph);
            };
            paths = paths.saturating_add(scene.stats().path_commands);
            prepared.insert(g.id, (scene, bounds));
        }
        let scale = size / face.units_per_em() as f32;
        let scene = Scene2D::record_with_limits([1, 1], options.limits, |c| {
            for g in glyphs {
                let _ = prepared[&g.id].0.draw(
                    c,
                    SceneOptions {
                        transform: [scale, 0., 0., -scale, g.x, g.y],
                        layer: Some(LayerOptions::default()),
                        ..Default::default()
                    },
                );
            }
        })?;
        let mut bounds: Option<[f32; 4]> = None;
        for g in glyphs {
            if let Some(b) = prepared[&g.id].1 {
                let v = [
                    g.x + b[0] * scale,
                    g.y - b[3] * scale,
                    g.x + b[2] * scale,
                    g.y - b[1] * scale,
                ];
                bounds = Some(bounds.map_or(v, |b| {
                    [
                        b[0].min(v[0]),
                        b[1].min(v[1]),
                        b[2].max(v[2]),
                        b[3].max(v[3]),
                    ]
                }));
            }
        }
        let bounds = bounds.map(|b| [b[0], b[1], b[2] - b[0], b[3] - b[1]]);
        Ok(Self {
            scene,
            glyphs: glyphs.len(),
            unique: prepared.len(),
            bounds,
            bitmap_bytes,
        })
    }
    pub fn glyph_count(&self) -> usize {
        self.glyphs
    }
    pub fn unique_glyph_count(&self) -> usize {
        self.unique
    }
    pub fn scene(&self) -> &Scene2D {
        &self.scene
    }
    /// Conservative outline/control-box bound in baseline coordinates (y down).
    pub fn bounds(&self) -> Option<[f32; 4]> {
        self.bounds
    }
    pub fn bitmap_bytes(&self) -> usize {
        self.bitmap_bytes
    }
    pub fn draw<C: Canvas2D + ?Sized>(&self, c: &mut C) -> Result<(), SceneError> {
        self.scene.draw(c, SceneOptions::default())
    }
}

fn prepare(
    font: &FontRef<'_>,
    location: &Location,
    id: GlyphId,
    colors: &[[f32; 4]],
    foreground: [f32; 4],
    limits: SceneLimits,
) -> Result<(Scene2D, Option<[f32; 4]>), ColorGlyphError> {
    let glyph = font
        .color_glyphs()
        .get(id)
        .ok_or(ColorGlyphError::NotColorGlyph)?;
    let mut painter = Painter {
        font,
        location,
        colors,
        foreground,
        limits,
        canvas: None,
        paths: HashMap::new(),
        transform: kurbo::Affine::IDENTITY,
        stack: Vec::new(),
        bounds: None,
        clips: 0,
        commands: 0,
        path_commands: 0,
        error: None,
    };
    glyph
        .paint(location, &mut painter)
        .map_err(|_| ColorGlyphError::MalformedPaint)?;
    if let Some(e) = painter.error {
        return Err(e);
    }
    let paths = painter.paths;
    let bounds = painter.bounds;
    let mut error = None;
    let scene = Scene2D::record_with_limits([1, 1], limits, |c| {
        let mut painter = Painter {
            font,
            location,
            colors,
            foreground,
            limits,
            canvas: Some(c),
            paths,
            transform: kurbo::Affine::IDENTITY,
            stack: Vec::new(),
            bounds,
            clips: 0,
            commands: 0,
            path_commands: 0,
            error: None,
        };
        if glyph.paint(location, &mut painter).is_err() {
            error = Some(ColorGlyphError::MalformedPaint);
        }
        if let Some(e) = painter.error {
            error = Some(e);
        }
    })?;
    if let Some(e) = error {
        return Err(e);
    }
    Ok((scene, bounds.map(|b| b.map(|v| v as f32))))
}
struct Painter<'f, 'a, 'c> {
    font: &'a FontRef<'f>,
    location: &'a Location,
    colors: &'a [[f32; 4]],
    foreground: [f32; 4],
    limits: SceneLimits,
    canvas: Option<&'c mut dyn Canvas2D>,
    paths: HashMap<GlyphId, Arc<[PathCommand]>>,
    transform: kurbo::Affine,
    stack: Vec<kurbo::Affine>,
    bounds: Option<[f64; 4]>,
    clips: usize,
    commands: usize,
    path_commands: usize,
    error: Option<ColorGlyphError>,
}
impl Painter<'_, '_, '_> {
    fn fail(&mut self, e: ColorGlyphError) {
        if self.error.is_none() {
            self.error = Some(e);
        }
    }
    fn step(&mut self) -> bool {
        if self.error.is_some() {
            return false;
        }
        self.commands += 1;
        if self.commands > self.limits.max_commands {
            self.fail(SceneError::CommandBudget.into());
            return false;
        }
        true
    }
    fn include(&mut self, x: f32, y: f32) {
        let p = self.transform * kurbo::Point::new(x as f64, y as f64);
        if !p.is_finite() || p.x.abs() > 1e6 || p.y.abs() > 1e6 {
            self.fail(ColorGlyphError::MalformedPaint);
            return;
        }
        self.bounds = Some(self.bounds.map_or([p.x, p.y, p.x, p.y], |b| {
            [b[0].min(p.x), b[1].min(p.y), b[2].max(p.x), b[3].max(p.y)]
        }));
    }
    fn path(&mut self, id: GlyphId) -> Option<Arc<[PathCommand]>> {
        if let Some(p) = self.paths.get(&id) {
            return Some(p.clone());
        }
        let Some(glyph) = self.font.outline_glyphs().get(id) else {
            self.fail(ColorGlyphError::MalformedPaint);
            return None;
        };
        let mut pen = Pen {
            path: Vec::new(),
            limit: self
                .limits
                .max_path_commands
                .saturating_sub(self.path_commands),
            failed: false,
        };
        if glyph
            .draw(
                DrawSettings::unhinted(Size::unscaled(), self.location),
                &mut pen,
            )
            .is_err()
        {
            self.fail(ColorGlyphError::MalformedPaint);
            return None;
        }
        if pen.failed {
            self.fail(SceneError::PathBudget.into());
            return None;
        }
        self.path_commands += pen.path.len();
        let path: Arc<[PathCommand]> = pen.path.into();
        self.paths.insert(id, path.clone());
        Some(path)
    }
    fn color(&mut self, index: u16, alpha: f32) -> [f32; 4] {
        let mut c = if index == u16::MAX {
            self.foreground
        } else if let Some(c) = self.colors.get(index as usize) {
            *c
        } else {
            self.fail(ColorGlyphError::InvalidPalette);
            [0.; 4]
        };
        if !alpha.is_finite() {
            self.fail(ColorGlyphError::MalformedPaint);
        }
        c[3] *= alpha.clamp(0., 1.);
        c
    }
    fn stops(&mut self, stops: &[skrifa::color::ColorStop]) -> Option<GradientStops> {
        if stops.is_empty() || stops.len() > 4096 {
            self.fail(ColorGlyphError::MalformedPaint);
            return None;
        }
        let v = stops
            .iter()
            .map(|s| (s.offset, self.color(s.palette_index, s.alpha)))
            .collect::<Vec<_>>();
        match GradientStops::new(&v) {
            Ok(v) => Some(v),
            Err(_) => {
                self.fail(ColorGlyphError::MalformedPaint);
                None
            }
        }
    }
}
impl ColorPainter for Painter<'_, '_, '_> {
    fn push_transform(&mut self, t: Transform) {
        if !self.step() {
            return;
        }
        if self.stack.len() >= self.limits.max_nesting {
            self.fail(SceneError::NestingBudget.into());
            return;
        }
        let m = [t.xx, t.yx, t.xy, t.yy, t.dx, t.dy];
        if m.iter().any(|v| !v.is_finite()) {
            self.fail(ColorGlyphError::MalformedPaint);
            return;
        }
        self.stack.push(self.transform);
        self.transform *= kurbo::Affine::new(m.map(f64::from));
        if let Some(c) = self.canvas.as_mut() {
            c.save();
            c.transform(m);
        }
    }
    fn pop_transform(&mut self) {
        if !self.step() {
            return;
        }
        if let Some(t) = self.stack.pop() {
            self.transform = t;
        } else {
            self.fail(ColorGlyphError::MalformedPaint);
            return;
        }
        if let Some(c) = self.canvas.as_mut() {
            c.restore();
        }
    }
    fn push_clip_glyph(&mut self, id: GlyphId) {
        if !self.step() {
            return;
        }
        self.clips += 1;
        let Some(path) = self.path(id) else {
            return;
        };
        for cmd in path.iter() {
            match *cmd {
                PathCommand::MoveTo(p) | PathCommand::LineTo(p) => self.include(p.0, p.1),
                PathCommand::QuadTo { ctrl, to } => {
                    self.include(ctrl.0, ctrl.1);
                    self.include(to.0, to.1);
                }
                PathCommand::CubicTo { c1, c2, to } => {
                    self.include(c1.0, c1.1);
                    self.include(c2.0, c2.1);
                    self.include(to.0, to.1);
                }
                _ => {}
            }
        }
        if let Some(c) = self.canvas.as_mut() {
            c.save();
            c.clip_path(&path, FillRule::NonZero);
        }
    }
    fn push_clip_box(&mut self, b: BoundingBox<f32>) {
        if !self.step() {
            return;
        }
        self.clips += 1;
        for (x, y) in [
            (b.x_min, b.y_min),
            (b.x_min, b.y_max),
            (b.x_max, b.y_min),
            (b.x_max, b.y_max),
        ] {
            self.include(x, y);
        }
        if let Some(c) = self.canvas.as_mut() {
            c.save();
            c.clip_rect(b.x_min, b.y_min, b.x_max - b.x_min, b.y_max - b.y_min);
        }
    }
    fn pop_clip(&mut self) {
        if self.step() {
            if self.clips == 0 {
                self.fail(ColorGlyphError::MalformedPaint);
                return;
            }
            self.clips -= 1;
            if let Some(c) = self.canvas.as_mut() {
                c.restore();
            }
        }
    }
    fn fill(&mut self, brush: Brush<'_>) {
        if !self.step() {
            return;
        }
        if self.clips == 0 {
            self.fail(ColorGlyphError::UnboundedPaint);
            return;
        }
        let mut transform = self.transform;
        let (color, gradient) = match brush {
            Brush::Solid {
                palette_index,
                alpha,
            } => (Some(self.color(palette_index, alpha)), None),
            Brush::LinearGradient {
                p0,
                p1,
                color_stops,
                extend,
            } => {
                let Some(stops) = self.stops(color_stops) else {
                    return;
                };
                (
                    None,
                    Some(
                        VectorGradient::linear([p0.x, p0.y], [p1.x, p1.y], stops)
                            .and_then(|g| Ok(g.with_extend(extend_mode(extend)?))),
                    ),
                )
            }
            Brush::RadialGradient {
                c0,
                r0,
                c1,
                r1,
                color_stops,
                extend,
            } => {
                let Some(stops) = self.stops(color_stops) else {
                    return;
                };
                (
                    None,
                    Some(
                        VectorGradient::radial_signed([c0.x, c0.y], r0, [c1.x, c1.y], r1, stops)
                            .and_then(|g| Ok(g.with_extend(extend_mode(extend)?))),
                    ),
                )
            }
            Brush::SweepGradient {
                c0,
                start_angle,
                end_angle,
                color_stops,
                extend,
            } => {
                let Some(stops) = self.stops(color_stops) else {
                    return;
                };
                transform *= kurbo::Affine::scale_non_uniform(1., -1.);
                (
                    None,
                    Some(
                        VectorGradient::sweep_range([c0.x, -c0.y], start_angle, end_angle, stops)
                            .and_then(|g| Ok(g.with_extend(extend_mode(extend)?))),
                    ),
                )
            }
        };
        let gradient = match gradient {
            Some(Ok(g)) => Some(g),
            Some(Err(_)) => {
                self.fail(ColorGlyphError::MalformedPaint);
                return;
            }
            None => None,
        };
        let Some(b) = self.bounds else {
            return;
        };
        if let Some(c) = self.canvas.as_mut() {
            c.save();
            c.reset_transform();
            if let Some(g) = gradient {
                c.set_fill_brush(
                    Brush2D::vector(g).with_transform(transform.as_coeffs().map(|v| v as f32)),
                );
            }
            if let Some(color) = color {
                // Keep foreground and paint alpha at f32 precision instead of
                // quantizing a solid through the string color API.
                let stops =
                    GradientStops::new(&[(0., color)]).expect("validated palette/foreground");
                c.set_fill_brush(Brush2D::vector(
                    VectorGradient::linear([0., 0.], [1., 0.], stops).unwrap(),
                ));
            }
            c.fill_rect(
                b[0] as f32 - 1.,
                b[1] as f32 - 1.,
                (b[2] - b[0]) as f32 + 2.,
                (b[3] - b[1]) as f32 + 2.,
            );
            c.restore();
        }
    }
    fn push_layer(&mut self, mode: ColrComposite) {
        if mode == ColrComposite::Unknown {
            self.fail(ColorGlyphError::MalformedPaint);
            return;
        }
        if self.step() {
            if let Some(c) = self.canvas.as_mut() {
                c.push_layer_with_options(layer(mode));
            }
        }
    }
    fn pop_layer(&mut self) {
        if self.step() {
            if let Some(c) = self.canvas.as_mut() {
                c.pop_layer();
            }
        }
    }
    fn paint_cached_color_glyph(
        &mut self,
        _: GlyphId,
    ) -> Result<skrifa::color::PaintCachedColorGlyph, skrifa::color::PaintError> {
        if self.error.is_some() {
            Err(skrifa::color::PaintError::DepthLimitExceeded)
        } else {
            Ok(skrifa::color::PaintCachedColorGlyph::Unimplemented)
        }
    }
}
fn extend_mode(e: Extend) -> Result<ExtendMode, GradientError> {
    match e {
        Extend::Pad => Ok(ExtendMode::Pad),
        Extend::Repeat => Ok(ExtendMode::Repeat),
        Extend::Reflect => Ok(ExtendMode::Reflect),
        _ => Err(GradientError::Geometry),
    }
}
fn layer(m: ColrComposite) -> LayerOptions {
    use ColrComposite as C;
    let compose = match m {
        C::Clear => CompositeMode::Clear,
        C::Src => CompositeMode::Copy,
        C::Dest => CompositeMode::Destination,
        C::SrcOver => CompositeMode::SourceOver,
        C::DestOver => CompositeMode::DestinationOver,
        C::SrcIn => CompositeMode::SourceIn,
        C::DestIn => CompositeMode::DestinationIn,
        C::SrcOut => CompositeMode::SourceOut,
        C::DestOut => CompositeMode::DestinationOut,
        C::SrcAtop => CompositeMode::SourceAtop,
        C::DestAtop => CompositeMode::DestinationAtop,
        C::Xor => CompositeMode::Xor,
        C::Plus => CompositeMode::Plus,
        _ => CompositeMode::SourceOver,
    };
    let blend = match m {
        C::Screen => BlendMode::Screen,
        C::Overlay => BlendMode::Overlay,
        C::Darken => BlendMode::Darken,
        C::Lighten => BlendMode::Lighten,
        C::ColorDodge => BlendMode::ColorDodge,
        C::ColorBurn => BlendMode::ColorBurn,
        C::HardLight => BlendMode::HardLight,
        C::SoftLight => BlendMode::SoftLight,
        C::Difference => BlendMode::Difference,
        C::Exclusion => BlendMode::Exclusion,
        C::Multiply => BlendMode::Multiply,
        C::HslHue => BlendMode::Hue,
        C::HslSaturation => BlendMode::Saturation,
        C::HslColor => BlendMode::Color,
        C::HslLuminosity => BlendMode::Luminosity,
        _ => BlendMode::Normal,
    };
    LayerOptions {
        compose,
        blend,
        ..Default::default()
    }
}
struct Pen {
    path: Vec<PathCommand>,
    limit: usize,
    failed: bool,
}
impl Pen {
    fn add(&mut self, p: PathCommand) {
        if self.path.len() < self.limit {
            self.path.push(p);
        } else {
            self.failed = true;
        }
    }
}
impl OutlinePen for Pen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.add(PathCommand::MoveTo((x, y)));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.add(PathCommand::LineTo((x, y)));
    }
    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        self.add(PathCommand::QuadTo {
            ctrl: (cx, cy),
            to: (x, y),
        });
    }
    fn curve_to(&mut self, ax: f32, ay: f32, bx: f32, by: f32, x: f32, y: f32) {
        self.add(PathCommand::CubicTo {
            c1: (ax, ay),
            c2: (bx, by),
            to: (x, y),
        });
    }
    fn close(&mut self) {
        self.add(PathCommand::Close);
    }
}
