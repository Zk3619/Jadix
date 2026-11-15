//! Explicit scalable glyph controls using the same Skrifa/Kurbo engines as Vello.
use super::*;
use skrifa::{
    instance::{Location, NormalizedCoord, Size},
    outline::{DrawSettings, HintingInstance, HintingOptions, OutlinePen},
    FontRef, GlyphId, MetadataProvider,
};

/// Synthetic contour expansion in pixels, before the glyph-local transform.
/// Expansion does not change shaped advances. Signed amounts can thin contours.
/// Negative/large expansion can produce corner/self-intersection artifacts, as
/// specified by Kurbo expand_path; this is contour expansion, not weight-axis shaping.
#[derive(Clone, Copy, Debug)]
pub struct FontEmbolden {
    pub amount: [f64; 2],
    pub join: kurbo::Join,
    pub miter_limit: f64,
    pub tolerance: f64,
}
impl Default for FontEmbolden {
    fn default() -> Self {
        Self {
            amount: [0.; 2],
            join: kurbo::Join::Miter,
            miter_limit: 4.,
            tolerance: 0.1,
        }
    }
}
#[derive(Clone, Debug)]
pub struct GlyphRunOptions {
    /// Hinting assumes this preparation size is the final pixel size. Canvas
    /// transforms still work but do not rerun grid fitting at the transformed size.
    pub hint: bool,
    pub embolden: FontEmbolden,
    /// Optional exact F2Dot14 coordinates in fvar axis order, after avar mapping.
    /// Requires exactly one coordinate per axis, each in [-16384,16384].
    /// Overrides Font2D user-space variations for this run's outlines only.
    pub normalized_coords: Option<Vec<i16>>,
    /// Applied to each outline in Y-down space before adding its baseline.
    pub glyph_transform: [f32; 6],
    /// Optional extra per-glyph transforms, one for every input glyph.
    pub per_glyph_transforms: Vec<[f32; 6]>,
}
impl Default for GlyphRunOptions {
    fn default() -> Self {
        Self {
            hint: false,
            embolden: Default::default(),
            normalized_coords: None,
            glyph_transform: [1., 0., 0., 1., 0., 0.],
            per_glyph_transforms: Vec::new(),
        }
    }
}
impl Font2D {
    /// Current user settings normalized through fvar/avar in font axis order.
    pub fn normalized_coords(&self) -> Result<Vec<i16>, FontError> {
        self.face()?;
        let font =
            FontRef::from_index(&self.data, self.index).map_err(|_| FontError::InvalidFont)?;
        Ok(location(&font, self)
            .coords()
            .iter()
            .map(|v| v.to_bits())
            .collect())
    }
}
fn location(font: &FontRef<'_>, source: &Font2D) -> Location {
    font.axes().location(
        source
            .variations
            .iter()
            .map(|(tag, value)| (skrifa::Tag::new(tag), *value)),
    )
}
impl GlyphRun {
    pub fn with_options(
        font: &Font2D,
        size: f32,
        glyphs: &[PositionedGlyph],
        options: &GlyphRunOptions,
    ) -> Result<Self, FontError> {
        let e = options.embolden;
        if !valid_size(size) || size > 16384. {
            return Err(FontError::InvalidSize);
        }
        if glyphs.len() > 1_000_000
            || (!options.per_glyph_transforms.is_empty()
                && options.per_glyph_transforms.len() != glyphs.len())
            || options
                .glyph_transform
                .iter()
                .chain(options.per_glyph_transforms.iter().flatten())
                .any(|v| !v.is_finite() || v.abs() > 1e6)
            || e.amount.iter().any(|v| !v.is_finite() || v.abs() > 16384.)
            || !e.miter_limit.is_finite()
            || !(1. ..=1024.).contains(&e.miter_limit)
            || !e.tolerance.is_finite()
            || !(0.001..=10.).contains(&e.tolerance)
        {
            return Err(FontError::InvalidPosition);
        }
        let face = font.face()?;
        let parsed =
            FontRef::from_index(&font.data, font.index).map_err(|_| FontError::InvalidFont)?;
        let mut coords = location(&parsed, font);
        if let Some(raw) = &options.normalized_coords {
            if raw.len() != coords.coords().len()
                || raw.iter().any(|v| !(-16384..=16384).contains(v))
            {
                return Err(FontError::InvalidVariation);
            }
            for (dst, src) in coords.coords_mut().iter_mut().zip(raw) {
                *dst = NormalizedCoord::from_bits(*src);
            }
        }
        let outlines = parsed.outline_glyphs();
        let hinter = if options.hint {
            Some(
                HintingInstance::new(
                    &outlines,
                    Size::new(size),
                    &coords,
                    HintingOptions::default(),
                )
                .map_err(|_| FontError::OutlineDraw)?,
            )
        } else {
            None
        };
        let mut cache = HashMap::new();
        let mut paths = Vec::with_capacity(glyphs.len());
        for (index, glyph) in glyphs.iter().enumerate() {
            if !glyph.x.is_finite() || !glyph.y.is_finite() {
                return Err(FontError::InvalidPosition);
            }
            if glyph.id >= face.number_of_glyphs() {
                return Err(FontError::InvalidGlyph);
            }
            if face.is_color_glyph(ttf_parser::GlyphId(glyph.id)) {
                return Err(FontError::ColorGlyph);
            }
            if let std::collections::hash_map::Entry::Vacant(entry) = cache.entry(glyph.id) {
                let mut pen = Outline(kurbo::BezPath::new());
                if let Some(outline) = outlines.get(GlyphId::new(glyph.id as u32)) {
                    let settings = match &hinter {
                        Some(h) => DrawSettings::hinted(h, false),
                        None => DrawSettings::unhinted(Size::new(size), &coords),
                    };
                    outline
                        .draw(settings, &mut pen)
                        .map_err(|_| FontError::OutlineDraw)?;
                }
                let path = if e.amount == [0.; 2] {
                    pen.0
                } else {
                    kurbo::expand_path(
                        &pen.0,
                        kurbo::Diagonal2::new(e.amount[0], e.amount[1]),
                        e.join,
                        e.miter_limit,
                        e.tolerance,
                    )
                };
                entry.insert(path);
            }
            let global = kurbo::Affine::new(options.glyph_transform.map(f64::from));
            let individual = options
                .per_glyph_transforms
                .get(index)
                .map_or(kurbo::Affine::IDENTITY, |m| {
                    kurbo::Affine::new(m.map(f64::from))
                });
            let transform = kurbo::Affine::translate((glyph.x as f64, glyph.y as f64))
                * global
                * individual
                * kurbo::Affine::scale_non_uniform(1., -1.);
            let path = &cache[&glyph.id];
            let mut result = Vec::with_capacity(path.elements().len());
            let point = |p: kurbo::Point| {
                let p = transform * p;
                (p.x as f32, p.y as f32)
            };
            for el in path.elements() {
                result.push(match *el {
                    kurbo::PathEl::MoveTo(p) => PathCommand::MoveTo(point(p)),
                    kurbo::PathEl::LineTo(p) => PathCommand::LineTo(point(p)),
                    kurbo::PathEl::QuadTo(c, p) => PathCommand::QuadTo {
                        ctrl: point(c),
                        to: point(p),
                    },
                    kurbo::PathEl::CurveTo(a, b, p) => PathCommand::CubicTo {
                        c1: point(a),
                        c2: point(b),
                        to: point(p),
                    },
                    kurbo::PathEl::ClosePath => PathCommand::Close,
                });
            }
            if result.iter().any(|c| match *c {
                PathCommand::MoveTo(p) | PathCommand::LineTo(p) => !finite(p),
                PathCommand::QuadTo { ctrl, to } => !finite(ctrl) || !finite(to),
                PathCommand::CubicTo { c1, c2, to } => !finite(c1) || !finite(c2) || !finite(to),
                _ => false,
            }) {
                return Err(FontError::InvalidPosition);
            }
            paths.push(result);
        }
        Ok(Self {
            paths: paths.into(),
            unique_glyphs: cache.len(),
        })
    }
}
struct Outline(kurbo::BezPath);
impl OutlinePen for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to((x as f64, y as f64));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to((x as f64, y as f64));
    }
    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        self.0.quad_to((cx as f64, cy as f64), (x as f64, y as f64));
    }
    fn curve_to(&mut self, ax: f32, ay: f32, bx: f32, by: f32, x: f32, y: f32) {
        self.0.curve_to(
            (ax as f64, ay as f64),
            (bx as f64, by as f64),
            (x as f64, y as f64),
        );
    }
    fn close(&mut self) {
        self.0.close_path();
    }
}
