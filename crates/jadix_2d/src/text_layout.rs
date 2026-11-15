//! Prepared visible Cosmic Text layouts. Shaping, fallback, wrapping, bidi,
//! selection and font attributes use the existing workspace text engine.
use crate::{
    AtlasError, AtlasLimits, Canvas2D, GlyphRun, Image2D, ImageAtlas, ImageSampling, PathCommand,
};
pub use cosmic_text;
use cosmic_text::{
    Buffer, CacheKey, CacheKeyFlags, Color, Cursor, FontSystem, SwashCache, SwashContent,
};
use std::{collections::HashMap, ops::Range, sync::Arc};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextRenderMode {
    /// Scalable monochrome/COLR outlines; SVG and bitmap glyphs use an atlas.
    #[default]
    Outline,
    /// Rasterize all glyphs at raster_scale; prepared atlas draws need no fonts.
    Atlas,
}
#[derive(Clone, Copy, Debug)]
pub struct TextPrepareOptions {
    pub mode: TextRenderMode,
    /// Raster pixels per layout unit, 0.25..=8. Reprepare for a new target DPI.
    pub raster_scale: f32,
    pub color: Color,
    pub atlas: AtlasLimits,
    /// Visible glyph instance limit, independent of unique atlas entries.
    pub max_glyphs: usize,
    /// CPAL palette for COLRv0/v1 and SVG; bitmap emoji ignore this field.
    pub palette: u16,
}
impl Default for TextPrepareOptions {
    fn default() -> Self {
        Self {
            mode: TextRenderMode::Outline,
            raster_scale: 1.,
            color: Color::rgb(0, 0, 0),
            atlas: AtlasLimits::default(),
            max_glyphs: 65536,
            palette: 0,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextLayoutError {
    InvalidOptions,
    InvalidLayout,
    MissingFont,
    UnsupportedGlyph,
    Atlas(AtlasError),
    GlyphBudget,
    BrushRequiresOutlines,
    Color(crate::ColorGlyphError),
    Scene(crate::SceneError),
}
impl std::fmt::Display for TextLayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "2D text layout: {self:?}")
    }
}
impl std::error::Error for TextLayoutError {}
impl From<AtlasError> for TextLayoutError {
    fn from(v: AtlasError) -> Self {
        Self::Atlas(v)
    }
}

/// Logical cluster coordinates in the prepared visible viewport, before draw origin/CTM.
#[derive(Clone, Debug)]
pub struct TextGlyphInfo {
    pub line: usize,
    pub cluster: Range<usize>,
    pub baseline: [f32; 2],
    pub advance: f32,
    pub bidi_level: u8,
    pub font_id: cosmic_text::fontdb::ID,
    pub glyph_id: u16,
    pub metadata: usize,
    pub color_bitmap: bool,
    pub color_vector: bool,
    pub font_variations: cosmic_text::FontVariations,
}
#[derive(Clone, Debug)]
enum Item {
    Outline(GlyphRun, Color),
    Bitmap(usize, [f32; 4]),
    Rect([f32; 4], Color),
    Color(crate::ColorGlyphRun, [f32; 6], f32),
}
#[derive(Debug)]
struct Prepared {
    mode: TextRenderMode,
    buffer: Buffer,
    atlas: ImageAtlas,
    items: Vec<Item>,
    glyphs: Vec<TextGlyphInfo>,
    lines: usize,
}
/// Immutable snapshot of the buffer's visible viewport and prepared ink.
/// Clones share paths, page identities, hit-test layout and cluster metadata.
/// There is no font access, shaping, rasterization or atlas mutation at draw time.
#[derive(Clone, Debug)]
pub struct PreparedText2D(Arc<Prepared>);
/// Keep namespace unique per FontSystem/database generation; CacheKey IDs alone
/// are only meaningful inside that database. Palette participates in cache identity.
pub type GlyphAtlasCache = crate::AtlasCache<(u64, CacheKey, u32, u16)>;
impl PreparedText2D {
    pub fn new(
        fonts: &mut FontSystem,
        buffer: &mut Buffer,
        cache: &mut SwashCache,
        options: TextPrepareOptions,
    ) -> Result<Self, TextLayoutError> {
        Self::prepare(fonts, buffer, cache, options, None)
    }
    /// Share incrementally updated pages across prepared snapshots. Existing
    /// snapshots retain old immutable pages safely when entries are evicted.
    pub fn new_with_atlas_cache(
        fonts: &mut FontSystem,
        buffer: &mut Buffer,
        cache: &mut SwashCache,
        options: TextPrepareOptions,
        atlas: &mut GlyphAtlasCache,
        namespace: u64,
    ) -> Result<Self, TextLayoutError> {
        Self::prepare(fonts, buffer, cache, options, Some((atlas, namespace)))
    }
    fn prepare(
        fonts: &mut FontSystem,
        buffer: &mut Buffer,
        cache: &mut SwashCache,
        options: TextPrepareOptions,
        incremental: Option<(&mut GlyphAtlasCache, u64)>,
    ) -> Result<Self, TextLayoutError> {
        if !options.raster_scale.is_finite()
            || !(0.25..=8.).contains(&options.raster_scale)
            || options.max_glyphs == 0
            || options.max_glyphs > 1_000_000
        {
            return Err(TextLayoutError::InvalidOptions);
        }
        ImageAtlas::pack(&[], options.atlas)?;
        buffer.shape_until_scroll(fonts, false);
        let mut images = Vec::new();
        let mut image_bytes = 0usize;
        let atlas_budget = options.atlas.page_size as usize
            * options.atlas.page_size as usize
            * 4
            * options.atlas.max_pages as usize;
        let mut entries = HashMap::<(CacheKey, u32), (Option<usize>, [f32; 4], bool)>::new();
        let mut outlines = HashMap::<CacheKey, Option<Vec<PathCommand>>>::new();
        let mut items = Vec::new();
        let mut glyphs = Vec::new();
        let mut lines = 0;
        let mut color_context = swash::scale::ScaleContext::new();
        let mut colr_fonts = HashMap::new();
        let mut color_runs = HashMap::new();
        for run in buffer.layout_runs() {
            lines += 1;
            if [run.line_y, run.line_top, run.line_height, run.line_w]
                .iter()
                .any(|v| !coordinate(*v))
            {
                return Err(TextLayoutError::InvalidLayout);
            }
            for glyph in run.glyphs {
                if glyphs.len() >= options.max_glyphs {
                    return Err(TextLayoutError::GlyphBudget);
                }
                if !glyph.font_size.is_finite()
                    || glyph.font_size <= 0.
                    || glyph.font_size * options.raster_scale > 1024.
                    || [glyph.x, glyph.y, glyph.x_offset, glyph.y_offset, glyph.w]
                        .iter()
                        .any(|v| !coordinate(*v))
                    || glyph.start > glyph.end
                    || !run.text.is_char_boundary(glyph.start)
                    || !run.text.is_char_boundary(glyph.end)
                {
                    return Err(TextLayoutError::InvalidLayout);
                }
                let color = glyph.color_opt.unwrap_or(options.color);
                let physical = glyph.physical(
                    (0., run.line_y * options.raster_scale),
                    options.raster_scale,
                );
                let key = physical.cache_key;
                if fonts
                    .get_font_with_variations(key.font_id, key.font_weight, key.font_variations)
                    .is_none()
                {
                    return Err(TextLayoutError::MissingFont);
                }
                let baseline = [
                    glyph.x + glyph.font_size * glyph.x_offset,
                    run.line_y + glyph.y - glyph.font_size * glyph.y_offset,
                ];
                if baseline.iter().any(|v| !coordinate(*v)) {
                    return Err(TextLayoutError::InvalidLayout);
                }
                let cached = color_runs.entry((key, color.0)).or_insert_with(|| {
                    let font = fonts
                        .get_font_with_variations(key.font_id, key.font_weight, key.font_variations)
                        .ok_or(TextLayoutError::MissingFont)?;
                    let index = fonts
                        .db()
                        .face(key.font_id)
                        .ok_or(TextLayoutError::MissingFont)?
                        .index;
                    let face = ttf_parser::Face::parse(font.data(), index)
                        .map_err(|_| TextLayoutError::MissingFont)?;
                    if !face.is_color_glyph(ttf_parser::GlyphId(key.glyph_id))
                        && face
                            .glyph_svg_image(ttf_parser::GlyphId(key.glyph_id))
                            .is_none()
                    {
                        return Ok(None);
                    }
                    let base = colr_fonts
                        .entry(key.font_id)
                        .or_insert_with(|| crate::Font2D::new(font.data(), index));
                    let base = base
                        .as_ref()
                        .map_err(|e| TextLayoutError::Color(crate::ColorGlyphError::Font(*e)))?;
                    let settings = key
                        .font_variations
                        .settings(key.font_weight)
                        .filter(|(tag, _)| {
                            face.variation_axes()
                                .into_iter()
                                .any(|axis| axis.tag == ttf_parser::Tag::from_bytes(tag))
                        })
                        .collect::<Vec<_>>();
                    let base = base
                        .with_variations(&settings)
                        .map_err(|e| TextLayoutError::Color(crate::ColorGlyphError::Font(e)))?;
                    let [r, g, b, _] = color.as_rgba();
                    crate::ColorGlyphRun::new(
                        &base,
                        glyph.font_size,
                        &[crate::PositionedGlyph {
                            id: key.glyph_id,
                            x: 0.,
                            y: 0.,
                        }],
                        crate::ColorGlyphOptions {
                            palette: options.palette,
                            raster_scale: options.raster_scale,
                            foreground: [r as f32 / 255., g as f32 / 255., b as f32 / 255., 1.],
                            ..Default::default()
                        },
                    )
                    .map(Some)
                    .map_err(TextLayoutError::Color)
                });
                if let Some(color_run) = cached.as_ref().map_err(|e| *e)? {
                    let shear = if key.flags.contains(CacheKeyFlags::FAKE_ITALIC) {
                        -14f32.to_radians().tan()
                    } else {
                        0.
                    };
                    let transform = [1., 0., shear, 1., baseline[0], baseline[1]];
                    glyphs.push(TextGlyphInfo {
                        line: run.line_i,
                        cluster: glyph.start..glyph.end,
                        baseline,
                        advance: glyph.w,
                        bidi_level: glyph.level.number(),
                        font_id: glyph.font_id,
                        glyph_id: glyph.glyph_id,
                        metadata: glyph.metadata,
                        color_bitmap: options.mode == TextRenderMode::Atlas
                            || color_run.bitmap_bytes() > 0,
                        font_variations: key.font_variations,
                        color_vector: options.mode == TextRenderMode::Outline
                            && color_run.bitmap_bytes() == 0,
                    });
                    if options.mode == TextRenderMode::Outline && color_run.bitmap_bytes() == 0 {
                        items.push(Item::Color(
                            color_run.clone(),
                            transform,
                            color.as_rgba()[3] as f32 / 255.,
                        ));
                    } else {
                        let identity = (key, color.0);
                        let (entry, placement) = if let Some(v) = entries.get(&identity) {
                            (v.0, v.1)
                        } else {
                            if images.len() >= options.atlas.max_entries {
                                return Err(TextLayoutError::Atlas(AtlasError::BudgetExceeded));
                            }
                            let v = raster_color(
                                color_run,
                                shear,
                                options.raster_scale,
                                color.as_rgba()[3],
                                options.atlas,
                                atlas_budget.saturating_sub(image_bytes),
                            )?;
                            let v = if let Some((image, placement)) = v {
                                image_bytes += image.rgba8().len();
                                let entry = images.len();
                                images.push(image);
                                (Some(entry), placement)
                            } else {
                                (None, [0.; 4])
                            };
                            entries.insert(identity, (v.0, v.1, true));
                            v
                        };
                        if let Some(entry) = entry {
                            items.push(Item::Bitmap(
                                entry,
                                [
                                    baseline[0] + placement[0],
                                    baseline[1] + placement[1],
                                    placement[2],
                                    placement[3],
                                ],
                            ));
                        }
                    }
                    continue;
                }
                // Cache only within this preparation, so an unbounded external
                // Swash image cache is not required to retain atlas glyphs.
                let (entry, placement, is_color) = if let Some(v) = entries.get(&(key, color.0)) {
                    *v
                } else {
                    let mut image = cache.get_image_uncached(fonts, key);
                    if image
                        .as_ref()
                        .is_none_or(|i| i.content != SwashContent::Color)
                    {
                        let font = fonts
                            .get_font_with_variations(
                                key.font_id,
                                key.font_weight,
                                key.font_variations,
                            )
                            .ok_or(TextLayoutError::MissingFont)?;
                        let index = fonts
                            .db()
                            .face(key.font_id)
                            .ok_or(TextLayoutError::MissingFont)?
                            .index;
                        if let Ok(face) = ttf_parser::Face::parse(font.data(), index) {
                            let id = ttf_parser::GlyphId(key.glyph_id);
                            if face.is_color_glyph(id) || face.glyph_svg_image(id).is_some() {
                                return Err(TextLayoutError::UnsupportedGlyph);
                            }
                        }
                    }
                    if let Some(ref mut image) = image {
                        if matches!(image.source, swash::scale::Source::ColorOutline(_)) {
                            *image = color_outline(fonts, &mut color_context, key, color)
                                .ok_or(TextLayoutError::UnsupportedGlyph)?;
                        }
                    }
                    let is_color = image
                        .as_ref()
                        .is_some_and(|i| i.content == SwashContent::Color);
                    let use_bitmap = options.mode == TextRenderMode::Atlas || is_color;
                    let (entry, placement) = if use_bitmap {
                        if let Some(image) =
                            image.filter(|i| i.placement.width > 0 && i.placement.height > 0)
                        {
                            let p = image.placement;
                            if p.width > options.atlas.page_size - 2
                                || p.height > options.atlas.page_size - 2
                            {
                                return Err(TextLayoutError::Atlas(AtlasError::EntryTooLarge));
                            }
                            let bytes = p.width as usize * p.height as usize * 4;
                            image_bytes = image_bytes
                                .checked_add(bytes)
                                .ok_or(TextLayoutError::Atlas(AtlasError::BudgetExceeded))?;
                            if image_bytes > atlas_budget {
                                return Err(TextLayoutError::Atlas(AtlasError::BudgetExceeded));
                            }
                            if images.len() >= options.atlas.max_entries {
                                return Err(TextLayoutError::Atlas(AtlasError::BudgetExceeded));
                            }
                            let rgba = glyph_rgba(&image, color)?;
                            let bitmap = Image2D::from_rgba8(p.width, p.height, &rgba)
                                .ok_or(TextLayoutError::UnsupportedGlyph)?;
                            let entry = images.len();
                            images.push(bitmap);
                            (
                                Some(entry),
                                [
                                    p.left as f32,
                                    -(p.top as f32),
                                    p.width as f32,
                                    p.height as f32,
                                ],
                            )
                        } else {
                            (None, [0.; 4])
                        }
                    } else {
                        (None, [0.; 4])
                    };
                    let v = (entry, placement, is_color);
                    entries.insert((key, color.0), v);
                    v
                };
                glyphs.push(TextGlyphInfo {
                    line: run.line_i,
                    cluster: glyph.start..glyph.end,
                    baseline,
                    advance: glyph.w,
                    bidi_level: glyph.level.number(),
                    font_id: glyph.font_id,
                    glyph_id: glyph.glyph_id,
                    metadata: glyph.metadata,
                    color_bitmap: is_color,
                    color_vector: false,
                    font_variations: key.font_variations,
                });
                if let Some(entry) = entry {
                    let scale = options.raster_scale;
                    items.push(Item::Bitmap(
                        entry,
                        [
                            (physical.x as f32 + placement[0]) / scale,
                            (physical.y as f32 + placement[1]) / scale,
                            placement[2] / scale,
                            placement[3] / scale,
                        ],
                    ));
                } else if options.mode == TextRenderMode::Outline && !is_color {
                    let key = glyph.physical((0., 0.), 1.).cache_key;
                    let path = outlines.entry(key).or_insert_with(|| {
                        cache.get_outline_commands(fonts, key).map(outline_path)
                    });
                    if let Some(path) = path {
                        let positioned = translate_path(path, baseline);
                        items.push(Item::Outline(
                            GlyphRun::from_prepared_paths(vec![positioned]),
                            color,
                        ));
                    }
                }
            }
            let mut decorations = Decorations(Vec::new());
            cosmic_text::render_decoration(&mut decorations, &run, options.color);
            items.extend(decorations.0);
        }
        let atlas = if let Some((cache, namespace)) = incremental {
            let mut keys = vec![None; images.len()];
            for ((key, color), (index, _, _)) in &entries {
                if let Some(index) = index {
                    keys[*index] = Some((namespace, *key, *color, options.palette));
                }
            }
            for (image, key) in images.iter().zip(&keys) {
                cache.insert(key.ok_or(TextLayoutError::InvalidLayout)?, image)?;
            }
            let snapshots = keys
                .iter()
                .map(|k| cache.get(&k.unwrap()).ok_or(AtlasError::BudgetExceeded))
                .collect::<Result<Vec<_>, _>>()?;
            ImageAtlas::from_entries(&snapshots)
        } else {
            ImageAtlas::pack(&images, options.atlas)?
        };
        Ok(Self(Arc::new(Prepared {
            mode: options.mode,
            buffer: buffer.clone(),
            atlas,
            items,
            glyphs,
            lines,
        })))
    }
    pub fn glyphs(&self) -> &[TextGlyphInfo] {
        &self.0.glyphs
    }
    pub fn line_count(&self) -> usize {
        self.0.lines
    }
    pub fn atlas(&self) -> &ImageAtlas {
        &self.0.atlas
    }
    pub fn render_mode(&self) -> TextRenderMode {
        self.0.mode
    }
    pub fn hit(&self, point: [f32; 2]) -> Option<Cursor> {
        if point.iter().any(|v| !v.is_finite()) {
            return None;
        }
        self.0.buffer.hit(point[0], point[1])
    }
    /// Selection rectangles in layout coordinates, split at visual bidi spans.
    pub fn selection_rects(&self, start: Cursor, end: Cursor) -> Vec<[f32; 4]> {
        let (start, end) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        self.0
            .buffer
            .layout_runs()
            .flat_map(|run| {
                run.highlight(start, end)
                    .map(move |(x, w)| [x, run.line_top, w, run.line_height])
            })
            .collect()
    }
    pub fn caret_rect(&self, cursor: Cursor) -> Option<[f32; 4]> {
        self.0.buffer.layout_runs().find_map(|run| {
            run.cursor_position(&cursor)
                .map(|x| [x, run.line_top, 1., run.line_height])
        })
    }
    /// Paint prepared colors, preserving caller state, CTM, alpha, clip/mask and
    /// layer context. Outline text remains vector under arbitrary transforms;
    /// atlas, SVG and bitmap text have the fixed prepared raster resolution.
    pub fn draw<C: Canvas2D + ?Sized>(&self, canvas: &mut C, origin: [f32; 2]) {
        let _ = self.try_draw(canvas, origin);
    }
    /// Like draw, but reports scene preflight failures for scalable color ink.
    pub fn try_draw<C: Canvas2D + ?Sized>(
        &self,
        canvas: &mut C,
        origin: [f32; 2],
    ) -> Result<(), TextLayoutError> {
        self.draw_impl(canvas, origin, false)
    }
    /// Override prepared monochrome span colors with the caller's fill brush,
    /// including gradients/patterns and its alpha. Color glyphs retain intrinsic
    /// palette/bitmap colors and their prepared foreground. Atlas-only text cannot
    /// be recolored this way; reject it before drawing anything.
    pub fn draw_with_brush<C: Canvas2D + ?Sized>(
        &self,
        canvas: &mut C,
        origin: [f32; 2],
    ) -> Result<(), TextLayoutError> {
        if self.0.mode != TextRenderMode::Outline {
            return Err(TextLayoutError::BrushRequiresOutlines);
        }
        self.draw_impl(canvas, origin, true)
    }
    fn draw_impl<C: Canvas2D + ?Sized>(
        &self,
        canvas: &mut C,
        origin: [f32; 2],
        current_brush: bool,
    ) -> Result<(), TextLayoutError> {
        if origin.iter().any(|v| !coordinate(*v)) {
            return Err(TextLayoutError::InvalidLayout);
        }
        for item in &self.0.items {
            if let Item::Color(run, mut transform, _) = item {
                transform[4] += origin[0];
                transform[5] += origin[1];
                run.scene()
                    .validate(
                        canvas,
                        &crate::SceneOptions {
                            transform,
                            ..Default::default()
                        },
                    )
                    .map_err(TextLayoutError::Scene)?;
            }
        }
        let alpha = canvas.current_global_alpha().unwrap_or(1.);
        canvas.save();
        canvas.translate(origin[0], origin[1]);
        canvas.set_fill_rule(crate::FillRule::NonZero);
        for item in &self.0.items {
            match item {
                Item::Outline(run, color) => {
                    if !current_brush {
                        set_color(canvas, *color);
                    }
                    canvas.fill_glyph_run(run);
                }
                Item::Bitmap(entry, rect) => {
                    self.0
                        .atlas
                        .draw(canvas, *entry, *rect, ImageSampling::Bilinear)
                }
                Item::Rect([x, y, w, h], color) => {
                    if !current_brush {
                        set_color(canvas, *color);
                    }
                    canvas.fill_rect(*x, *y, *w, *h);
                }
                Item::Color(run, transform, opacity) => {
                    canvas.save();
                    canvas.set_global_alpha(alpha * opacity);
                    let result = run.scene().draw(
                        canvas,
                        crate::SceneOptions {
                            transform: *transform,
                            ..Default::default()
                        },
                    );
                    canvas.restore();
                    if let Err(e) = result {
                        canvas.restore();
                        return Err(TextLayoutError::Scene(e));
                    }
                }
            }
        }
        canvas.restore();
        Ok(())
    }
}

fn raster_color(
    run: &crate::ColorGlyphRun,
    shear: f32,
    scale: f32,
    alpha: u8,
    limits: AtlasLimits,
    bytes: usize,
) -> Result<Option<(Image2D, [f32; 4])>, TextLayoutError> {
    let Some([x, y, w, h]) = run.bounds() else {
        return Ok(None);
    };
    let left = (x + (shear * y).min(shear * (y + h))) * scale;
    let right = (x + w + (shear * y).max(shear * (y + h))) * scale;
    let left = left.floor() - 1.;
    let top = (y * scale).floor() - 1.;
    let width = (right.ceil() + 1. - left) as u32;
    let height = ((y + h) * scale).ceil() + 1. - top;
    if width == 0 || !height.is_finite() || height < 1. || !left.is_finite() || !top.is_finite() {
        return Err(TextLayoutError::InvalidLayout);
    }
    if width > limits.page_size - 2 || height > limits.page_size as f32 - 2. {
        return Err(TextLayoutError::Atlas(AtlasError::EntryTooLarge));
    }
    let height = height as u32;
    if width as usize * height as usize * 4 > bytes {
        return Err(TextLayoutError::Atlas(AtlasError::BudgetExceeded));
    }
    let mut c = crate::RasterCanvas::new_linear(width, height);
    c.set_global_alpha(alpha as f32 / 255.);
    run.scene()
        .draw(
            &mut c,
            crate::SceneOptions {
                transform: [scale, 0., shear * scale, scale, -left, -top],
                ..Default::default()
            },
        )
        .map_err(TextLayoutError::Scene)?;
    let image = Image2D::from_rgba8(
        width,
        height,
        &c.get_image_data(0, 0, width, height).unwrap(),
    )
    .ok_or(TextLayoutError::UnsupportedGlyph)?;
    Ok(Some((
        image,
        [
            left / scale,
            top / scale,
            width as f32 / scale,
            height as f32 / scale,
        ],
    )))
}
fn coordinate(v: f32) -> bool {
    v.is_finite() && v.abs() <= 1e6
}
fn set_color<C: Canvas2D + ?Sized>(canvas: &mut C, color: Color) {
    let [r, g, b, a] = color.as_rgba();
    canvas.set_fill(&format!("#{r:02x}{g:02x}{b:02x}{a:02x}"));
}
fn outline_path(commands: &[cosmic_text::Command]) -> Vec<PathCommand> {
    let point = |p: swash::zeno::Point| (p.x, -p.y);
    commands
        .iter()
        .map(|c| match *c {
            cosmic_text::Command::MoveTo(p) => PathCommand::MoveTo(point(p)),
            cosmic_text::Command::LineTo(p) => PathCommand::LineTo(point(p)),
            cosmic_text::Command::QuadTo(c, p) => PathCommand::QuadTo {
                ctrl: point(c),
                to: point(p),
            },
            cosmic_text::Command::CurveTo(a, b, p) => PathCommand::CubicTo {
                c1: point(a),
                c2: point(b),
                to: point(p),
            },
            cosmic_text::Command::Close => PathCommand::Close,
        })
        .collect()
}
fn translate_path(path: &[PathCommand], offset: [f32; 2]) -> Vec<PathCommand> {
    let p = |p: (f32, f32)| (p.0 + offset[0], p.1 + offset[1]);
    path.iter()
        .map(|c| match *c {
            PathCommand::MoveTo(v) => PathCommand::MoveTo(p(v)),
            PathCommand::LineTo(v) => PathCommand::LineTo(p(v)),
            PathCommand::QuadTo { ctrl, to } => PathCommand::QuadTo {
                ctrl: p(ctrl),
                to: p(to),
            },
            PathCommand::CubicTo { c1, c2, to } => PathCommand::CubicTo {
                c1: p(c1),
                c2: p(c2),
                to: p(to),
            },
            PathCommand::Close => PathCommand::Close,
            PathCommand::Arc { .. } => unreachable!("Swash outlines contain no arcs"),
        })
        .collect()
}
struct Decorations(Vec<Item>);
impl cosmic_text::Renderer for Decorations {
    fn rectangle(&mut self, x: i32, y: i32, w: u32, h: u32, color: Color) {
        self.0
            .push(Item::Rect([x as f32, y as f32, w as f32, h as f32], color));
    }
    fn glyph(&mut self, _: cosmic_text::PhysicalGlyph, _: Color) {
        unreachable!("decoration renderer")
    }
}
fn color_outline(
    fonts: &mut FontSystem,
    context: &mut swash::scale::ScaleContext,
    key: CacheKey,
    color: Color,
) -> Option<cosmic_text::SwashImage> {
    use swash::{
        scale::{Render, Source},
        zeno::{Angle, Format, Transform, Vector},
    };
    let font = fonts.get_font_with_variations(key.font_id, key.font_weight, key.font_variations)?;
    let coords = font.normalized_coords();
    let font = font.as_swash();
    let mut scaler = context
        .builder(font)
        .size(f32::from_bits(key.font_size_bits))
        .hint(!key.flags.contains(CacheKeyFlags::DISABLE_HINTING));
    scaler = scaler.normalized_coords(coords.iter().copied());
    let [r, g, b, _] = color.as_rgba();
    Render::new(&[Source::ColorOutline(0)])
        .format(Format::Alpha)
        .default_color([r, g, b, 255])
        .offset(Vector::new(key.x_bin.as_float(), key.y_bin.as_float()))
        .transform(if key.flags.contains(CacheKeyFlags::FAKE_ITALIC) {
            Some(Transform::skew(
                Angle::from_degrees(14.),
                Angle::from_degrees(0.),
            ))
        } else {
            None
        })
        .render(&mut scaler.build(), key.glyph_id)
}
fn glyph_rgba(image: &cosmic_text::SwashImage, color: Color) -> Result<Vec<u8>, TextLayoutError> {
    let n = image.placement.width as usize * image.placement.height as usize;
    let c = color.as_rgba();
    match image.content {
        SwashContent::Mask if image.data.len() == n => Ok(image
            .data
            .iter()
            .flat_map(|a| {
                [
                    c[0],
                    c[1],
                    c[2],
                    ((u16::from(*a) * u16::from(c[3]) + 127) / 255) as u8,
                ]
            })
            .collect()),
        SwashContent::Color if image.data.len() == n * 4 => {
            let mut rgba = image.data.clone();
            for p in rgba.chunks_exact_mut(4) {
                // Swash COLR layer blitting stores display-premultiplied RGBA;
                // decoded color PNG strikes are straight RGBA. Normalize once.
                if matches!(image.source, swash::scale::Source::ColorOutline(_)) {
                    let a = u32::from(p[3]);
                    for c in &mut p[..3] {
                        *c = (u32::from(*c) * 255 + a / 2)
                            .checked_div(a)
                            .unwrap_or(0)
                            .min(255) as u8;
                    }
                }
                p[3] = ((u16::from(p[3]) * u16::from(c[3]) + 127) / 255) as u8;
            }
            Ok(rgba)
        }
        _ => Err(TextLayoutError::UnsupportedGlyph),
    }
}
