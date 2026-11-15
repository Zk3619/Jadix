//! Independent 2D path rendering: geometry, gradients, rasterization and GPU mesh preparation.
mod area;
mod geometry;
pub use geometry::{PathBooleanError, PathBooleanOp, PathBooleanOptions, PathGeometry2D};
pub mod gradient;
pub mod path;
mod stroke;
pub use path::*;
pub mod canvas;
mod readback;
pub use canvas::*;
mod blend;
mod region;
pub use blend::{BlendMode, CompositeMode, LayerOptions};
#[cfg(feature = "gpu")]
pub use region::PreparedSceneGpu;
pub use region::{RegionImage, SceneRegion};
mod image;
mod mipmap;
pub use image::{Image2D, ImageSampling};
pub use mipmap::{MipmapError, MipmapLimits};
pub mod color;
pub use ::color as color_types;
#[cfg(feature = "icc")]
pub use color::IccTransform;
pub use color::{Color2D, ColorContext2D, ColorSpaceTag, CssColorError, GamutMapping};
pub use kurbo;
#[cfg(feature = "gpu")]
pub mod gpu;

#[cfg(feature = "gpu")]
pub use wgpu;

mod brush;
pub use brush::{Brush2D, ExtendMode};
mod text;
pub use text::{
    Font2D, FontEmbolden, FontError, FontMetrics2D, GlyphRun, GlyphRunOptions, PositionedGlyph,
};
mod color_glyph;
pub use color_glyph::{ColorGlyphError, ColorGlyphOptions, ColorGlyphRun};
mod blur;
mod filter;
#[cfg(feature = "svg-fonts")]
mod svg_glyph;
pub use filter::{
    Convolution, FilterColorSpace, FilterEdge, FilterError, LayerFilter, Morphology,
    TransferFunction,
};
mod vector_gradient;
pub use vector_gradient::{
    GradientAlpha, GradientError, GradientSpace, GradientStops, HueDirection, VectorGradient,
};
mod atlas;
pub use atlas::{AtlasError, AtlasLimits, AtlasRegion, ImageAtlas};
mod atlas_cache;
pub use atlas_cache::{AtlasCache, AtlasCacheStats, AtlasEntry};
#[cfg(feature = "text-layout")]
pub mod text_layout;
#[cfg(feature = "text-layout")]
pub use text_layout::{
    GlyphAtlasCache, PreparedText2D, TextGlyphInfo, TextLayoutError, TextPrepareOptions,
    TextRenderMode,
};

#[cfg(feature = "text-layout")]
pub use text_layout::cosmic_text::{FontVariations, VariationError};
