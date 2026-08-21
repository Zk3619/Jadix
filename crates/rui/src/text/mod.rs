//! Text stack façade: layout-time measure + render-time glyphon bridge.
//!
//! # Soft wrap vs logical newlines
//!
//! Soft wrap **never** inserts `\n` into the underlying string — only visual rows.
//! Prefer [`visual_lines::build_visual_lines`] / [`VisualLineModel`] as the single source of
//! truth for editor caret, hit-test, and content height. Do **not** invent a third
//! mid-word truncator.
//!
//! - [`WrapMode::Word`] (default) — glyphon `Wrap::Word`
//! - [`WrapMode::Char`] — mid-word when safe; falls back to Word under BiDi / Word Joiner
//! - [`WrapMode::None`] — hard `\n` only; horizontal scroll for textareas
//!
//! Static BiDi labels may still use UAX#14 prewrap + `Wrap::None` via
//! [`LineBreakSource::BiDiPrewrap`].

pub mod markdown;
pub mod markdown_inline;
pub mod measure;
pub mod uax14_wrap;
pub mod visual_lines;
pub mod wrap_variable;

pub use crate::renderer::RuiTextRenderer;
pub use markdown::parse_markdown_blocks;
pub use markdown_inline::parse_markdown_inline;
pub use measure::{
    bidi_soft_wrap_height, clear_shared_font_system, fit_cache_frame_tick, glyphon_fit_soft_wrap,
    glyphon_soft_wrap_line_count, install_shared_font_system, intrinsic_width,
    invalidate_prepare_cache, layout, layout_lines, layout_text_fast_mode, layout_wrap_target,
    layout_wrap_target_prewrap, measure_leaf, measure_prefix_advance, note_fit_needs_relayout,
    prepare, prepare_rich, prepare_with_family, set_layout_text_fast_mode, shared_font_system_ptr,
    shutdown_layout_text, take_fit_needs_relayout, try_glyphon_fit_soft_wrap, GlyphonFitSoftWrap,
    PreparedLine, PreparedText, TextLayoutMetrics, LINE_WIDTH_EPS,
};
pub use uax14_wrap::{needs_logical_prewrap, soft_wrap_logical};
pub use visual_lines::{
    build_visual_lines, build_visual_lines_window, caret_visual_line_index,
    invalidate_visual_line_cache, soft_visual_line_byte_ranges, textarea_visual_content_height,
    visual_line_cache_frame_tick, LineBreakSource, VirtHeightEstimate, VisualLineModel, WrapMode,
    VIRT_CHAR_THRESHOLD, VIRT_LINE_THRESHOLD, VIRT_WINDOW_LINES,
};
pub use wrap_variable::layout_lines_variable_width;
