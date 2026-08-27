//! Glyphon provides a simple way to render 2D text with [wgpu], [cosmic-text] and [etagere].
//!
//! [wpgu]: https://github.com/gfx-rs/wgpu
//! [cosmic-text]: https://github.com/pop-os/cosmic-text
//! [etagere]: https://github.com/nical/etagere

mod cache;
mod custom_glyph;
mod error;
mod text_atlas;
mod text_render;
mod viewport;

pub use cache::Cache;
pub use custom_glyph::{
    ContentType, CustomGlyph, CustomGlyphId, RasterizeCustomGlyphRequest, RasterizedCustomGlyph,
};
pub use error::{PrepareError, RenderError};
pub use text_atlas::{ColorMode, TextAtlas};
pub use text_render::TextRenderer;
pub use viewport::Viewport;

// Re-export all top-level types from `cosmic-text` for convenience.
#[doc(no_inline)]
pub use cosmic_text::{
    self, fontdb, Action, Affinity, Attrs, AttrsList, AttrsOwned, Buffer, BufferLine, CacheKey,
    Color, Command, Cursor, Edit, Editor, Family, FamilyOwned, Font, FontSystem, LayoutCursor,
    LayoutGlyph, LayoutLine, LayoutRun, LayoutRunIter, Metrics, ShapeGlyph, ShapeLine, ShapeSpan,
    ShapeWord, Shaping, Stretch, Style, SubpixelBin, SwashCache, SwashContent, SwashImage, Weight,
    Wrap,
};

use etagere::AllocId;
use wgpu::{Device, Queue};

pub(crate) enum GpuCacheStatus {
    InAtlas {
        x: u16,
        y: u16,
        content_type: ContentType,
    },
    SkipRasterization,
}

pub(crate) struct GlyphDetails {
    width: u16,
    height: u16,
    gpu_cache: GpuCacheStatus,
    atlas_id: Option<AllocId>,
    top: i16,
    left: i16,
}

/// Identity 2×2 linear transform `[a, b, c, d]` (column-major: x' = a*x + c*y).
pub const TEXT_TRANSFORM_IDENTITY: [f32; 4] = [1.0, 0.0, 0.0, 1.0];

/// Returns true when `[a,b,c,d]` is the identity linear transform.
#[inline]
pub fn text_transform_is_identity(t: &[f32; 4]) -> bool {
    (t[0] - 1.0).abs() < 1e-5
        && t[1].abs() < 1e-5
        && t[2].abs() < 1e-5
        && (t[3] - 1.0).abs() < 1e-5
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct GlyphToRender {
    /// Top-left of the glyph quad in screen px (float for transformed text).
    pos: [f32; 2],
    /// Screen-space size of the quad (1× geometry even when atlas is oversampled).
    dim: [u16; 2],
    uv: [u16; 2],
    color: u32,
    content_type_with_srgb: [u16; 2],
    depth: f32,
    /// Linear 2×2 `[a, b, c, d]` applied about [`Self::transform_origin`].
    transform: [f32; 4],
    transform_origin: [f32; 2],
    /// Atlas texel size (equals [`Self::dim`] normally; 4× when oversampled).
    uv_size: [u16; 2],
    _pad: [u16; 2],
}

#[cfg(test)]
mod glyph_to_render_layout_tests {
    use super::GlyphToRender;
    use std::mem::{align_of, offset_of, size_of};

    #[test]
    fn glyph_to_render_matches_vertex_layout() {
        assert_eq!(size_of::<GlyphToRender>(), 60);
        assert_eq!(align_of::<GlyphToRender>(), 4);
        assert_eq!(offset_of!(GlyphToRender, pos), 0);
        assert_eq!(offset_of!(GlyphToRender, dim), 8);
        assert_eq!(offset_of!(GlyphToRender, uv), 12);
        assert_eq!(offset_of!(GlyphToRender, color), 16);
        assert_eq!(offset_of!(GlyphToRender, content_type_with_srgb), 20);
        assert_eq!(offset_of!(GlyphToRender, depth), 24);
        assert_eq!(offset_of!(GlyphToRender, transform), 28);
        assert_eq!(offset_of!(GlyphToRender, transform_origin), 44);
        assert_eq!(offset_of!(GlyphToRender, uv_size), 52);
    }

    #[test]
    fn identity_helper() {
        assert!(super::text_transform_is_identity(&super::TEXT_TRANSFORM_IDENTITY));
        assert!(!super::text_transform_is_identity(&[0.7, 0.7, -0.7, 0.7]));
    }
}

/// The screen resolution to use when rendering text.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Resolution {
    /// The width of the screen in pixels.
    pub width: u32,
    /// The height of the screen in pixels.
    pub height: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Params {
    screen_resolution: Resolution,
    _pad: [u32; 2],
}

/// Controls the visible area of the text. Any text outside of the visible area will be clipped.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextBounds {
    /// The position of the left edge of the visible area.
    pub left: i32,
    /// The position of the top edge of the visible area.
    pub top: i32,
    /// The position of the right edge of the visible area.
    pub right: i32,
    /// The position of the bottom edge of the visible area.
    pub bottom: i32,
}

/// The default visible area doesn't clip any text.
impl Default for TextBounds {
    fn default() -> Self {
        Self {
            left: i32::MIN,
            top: i32::MIN,
            right: i32::MAX,
            bottom: i32::MAX,
        }
    }
}

/// A text area containing text to be rendered along with its overflow behavior.
#[derive(Clone)]
pub struct TextArea<'a> {
    /// The buffer containing the text to be rendered.
    pub buffer: &'a Buffer,
    /// The left edge of the buffer.
    pub left: f32,
    /// The top edge of the buffer.
    pub top: f32,
    /// The scaling to apply to the buffer.
    pub scale: f32,
    /// The visible bounds of the text area. This is used to clip the text and doesn't have to
    /// match the `left` and `top` values.
    pub bounds: TextBounds,
    /// The default color of the text area.
    pub default_color: Color,
    /// Additional custom glyphs to render.
    pub custom_glyphs: &'a [CustomGlyph],
    /// 2×2 linear transform `[a, b, c, d]` about [`Self::transform_origin`]
    /// (screen / buffer px, same space as `left`/`top`). Identity = no rotate/skew.
    pub transform: [f32; 4],
    /// Pivot for [`Self::transform`] in the same coordinate space as glyph positions.
    pub transform_origin: [f32; 2],
}

pub(crate) struct State<'a> {
    pub(crate) device: &'a Device,
    pub(crate) queue: &'a Queue,
}
