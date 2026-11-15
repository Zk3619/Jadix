//! Positioned monochrome vector glyphs. Shaping and font fallback belong to the caller.
mod outline;
use crate::PathCommand;
pub use outline::{FontEmbolden, GlyphRunOptions};
use std::{collections::HashMap, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontError {
    InvalidFont,
    UnsupportedFont,
    InvalidVariation,
    InvalidSize,
    InvalidGlyph,
    InvalidPosition,
    ColorGlyph,
    OutlineDraw,
}
impl std::fmt::Display for FontError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "2D font: {self:?}")
    }
}
impl std::error::Error for FontError {}

/// Immutable font bytes, collection face index and user-space variation settings.
#[derive(Clone, Debug)]
pub struct Font2D {
    pub(crate) data: Arc<[u8]>,
    pub(crate) index: u32,
    pub(crate) variations: Arc<[([u8; 4], f32)]>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontMetrics2D {
    pub ascent: f32,
    pub descent: f32,
    pub line_gap: f32,
}
impl Font2D {
    pub fn new(data: impl Into<Arc<[u8]>>, index: u32) -> Result<Self, FontError> {
        let font = Self {
            data: data.into(),
            index,
            variations: Arc::from([]),
        };
        let face = font.face()?;
        let tables = face.tables();
        if tables.glyf.is_none() && tables.cff.is_none() && tables.cff2.is_none() {
            return Err(FontError::UnsupportedFont);
        }
        Ok(font)
    }
    /// Unknown tags and nonfinite values fail. Axis values clamp to the font range.
    pub fn with_variations(&self, variations: &[([u8; 4], f32)]) -> Result<Self, FontError> {
        let mut font = self.clone();
        font.variations = variations.into();
        font.face()?;
        Ok(font)
    }
    pub(crate) fn face(&self) -> Result<ttf_parser::Face<'_>, FontError> {
        let mut face =
            ttf_parser::Face::parse(&self.data, self.index).map_err(|_| FontError::InvalidFont)?;
        for &(tag, value) in self.variations.iter() {
            let tag = ttf_parser::Tag::from_bytes(&tag);
            if !value.is_finite() || !face.variation_axes().into_iter().any(|a| a.tag == tag) {
                return Err(FontError::InvalidVariation);
            }
            face.set_variation(tag, value)
                .ok_or(FontError::InvalidVariation)?;
        }
        Ok(face)
    }
    /// Character mapping only; this does not perform shaping, kerning or bidi.
    pub fn glyph_id(&self, character: char) -> Option<u16> {
        self.face().ok()?.glyph_index(character).map(|g| g.0)
    }
    pub fn advance(&self, glyph_id: u16, size: f32) -> Option<f32> {
        if !valid_size(size) {
            return None;
        }
        let face = self.face().ok()?;
        let advance = face.glyph_hor_advance(ttf_parser::GlyphId(glyph_id))? as f32
            * (size / face.units_per_em() as f32);
        advance.is_finite().then_some(advance)
    }
    pub fn metrics(&self, size: f32) -> Option<FontMetrics2D> {
        if !valid_size(size) {
            return None;
        }
        let face = self.face().ok()?;
        let scale = size / face.units_per_em() as f32;
        let metrics = FontMetrics2D {
            ascent: face.ascender() as f32 * scale,
            descent: face.descender() as f32 * scale,
            line_gap: face.line_gap() as f32 * scale,
        };
        [metrics.ascent, metrics.descent, metrics.line_gap]
            .iter()
            .all(|v| v.is_finite())
            .then_some(metrics)
    }
}
fn valid_size(size: f32) -> bool {
    size.is_finite() && size > 0.
}

/// Position is a glyph baseline in canvas user coordinates (x right, y down).
/// Glyph IDs may come from any external shaper for the same font/variation instance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PositionedGlyph {
    pub id: u16,
    pub x: f32,
    pub y: f32,
}
/// Prepared outlines can be cloned cheaply and reused at any Canvas transform.
/// No font parsing or outline extraction occurs when drawing this run.
#[derive(Clone, Debug)]
pub struct GlyphRun {
    paths: Arc<[Vec<PathCommand>]>,
    unique_glyphs: usize,
}
impl GlyphRun {
    #[cfg(feature = "text-layout")]
    pub(crate) fn from_prepared_paths(paths: Vec<Vec<PathCommand>>) -> Self {
        Self {
            unique_glyphs: paths.len(),
            paths: paths.into(),
        }
    }
    pub fn new(font: &Font2D, size: f32, glyphs: &[PositionedGlyph]) -> Result<Self, FontError> {
        if !valid_size(size) {
            return Err(FontError::InvalidSize);
        }
        let face = font.face()?;
        let scale = size / face.units_per_em() as f32;
        let mut outlines = HashMap::new();
        let mut paths = Vec::with_capacity(glyphs.len());
        for glyph in glyphs {
            if !glyph.x.is_finite() || !glyph.y.is_finite() {
                return Err(FontError::InvalidPosition);
            }
            if glyph.id >= face.number_of_glyphs() {
                return Err(FontError::InvalidGlyph);
            }
            if face.is_color_glyph(ttf_parser::GlyphId(glyph.id)) {
                return Err(FontError::ColorGlyph);
            }
            let outline = outlines.entry(glyph.id).or_insert_with(|| {
                let mut pen = Pen {
                    commands: Vec::new(),
                    scale,
                };
                // Empty glyphs (e.g. spaces) have no contours and paint nothing.
                if face
                    .outline_glyph(ttf_parser::GlyphId(glyph.id), &mut pen)
                    .is_none()
                {
                    pen.commands.clear();
                }
                pen.commands
            });
            let offset = |p: (f32, f32)| (p.0 + glyph.x, p.1 + glyph.y);
            let path: Vec<_> = outline
                .iter()
                .map(|c| match *c {
                    PathCommand::MoveTo(p) => PathCommand::MoveTo(offset(p)),
                    PathCommand::LineTo(p) => PathCommand::LineTo(offset(p)),
                    PathCommand::QuadTo { ctrl, to } => PathCommand::QuadTo {
                        ctrl: offset(ctrl),
                        to: offset(to),
                    },
                    PathCommand::CubicTo { c1, c2, to } => PathCommand::CubicTo {
                        c1: offset(c1),
                        c2: offset(c2),
                        to: offset(to),
                    },
                    PathCommand::Close => PathCommand::Close,
                    PathCommand::Arc { .. } => {
                        unreachable!("font outlines only contain lines and Beziers")
                    }
                })
                .collect();
            // Scaling/position addition can overflow even when all inputs are finite.
            if path.iter().any(|c| match *c {
                PathCommand::MoveTo(p) | PathCommand::LineTo(p) => !finite(p),
                PathCommand::QuadTo { ctrl, to } => !finite(ctrl) || !finite(to),
                PathCommand::CubicTo { c1, c2, to } => !finite(c1) || !finite(c2) || !finite(to),
                _ => false,
            }) {
                return Err(FontError::InvalidPosition);
            }
            paths.push(path);
        }
        Ok(Self {
            paths: paths.into(),
            unique_glyphs: outlines.len(),
        })
    }
    pub fn glyph_count(&self) -> usize {
        self.paths.len()
    }
    pub fn unique_glyph_count(&self) -> usize {
        self.unique_glyphs
    }
    /// Positioned paths are read-only and retain individual glyph painter order.
    pub fn paths(&self) -> impl ExactSizeIterator<Item = &[PathCommand]> {
        self.paths.iter().map(Vec::as_slice)
    }
}
fn finite(p: (f32, f32)) -> bool {
    p.0.is_finite() && p.1.is_finite()
}
struct Pen {
    commands: Vec<PathCommand>,
    scale: f32,
}
impl Pen {
    fn point(&self, x: f32, y: f32) -> (f32, f32) {
        (x * self.scale, -y * self.scale)
    }
}
impl ttf_parser::OutlineBuilder for Pen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.commands.push(PathCommand::MoveTo(self.point(x, y)));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.commands.push(PathCommand::LineTo(self.point(x, y)));
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.commands.push(PathCommand::QuadTo {
            ctrl: self.point(x1, y1),
            to: self.point(x, y),
        });
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.commands.push(PathCommand::CubicTo {
            c1: self.point(x1, y1),
            c2: self.point(x2, y2),
            to: self.point(x, y),
        });
    }
    fn close(&mut self) {
        self.commands.push(PathCommand::Close);
    }
}
