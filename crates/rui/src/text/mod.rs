//! Text stack façade: layout-time measure + render-time glyphon bridge.

pub mod markdown;
pub mod markdown_inline;
pub mod measure;
pub mod wrap_variable;

pub use crate::renderer::RuiTextRenderer;
pub use markdown::parse_markdown_blocks;
pub use markdown_inline::parse_markdown_inline;
pub use measure::{
    bidi_soft_wrap_height, clear_shared_font_system, fit_cache_frame_tick, glyphon_fit_soft_wrap,
    glyphon_soft_wrap_line_count, install_shared_font_system, intrinsic_width,
    invalidate_prepare_cache, layout, layout_lines, layout_text_fast_mode, layout_wrap_target,
    measure_leaf, note_fit_needs_relayout, prepare, prepare_rich, prepare_with_family,
    set_layout_text_fast_mode, shared_font_system_ptr, shutdown_layout_text,
    take_fit_needs_relayout, try_glyphon_fit_soft_wrap, GlyphonFitSoftWrap, PreparedLine,
    PreparedText, TextLayoutMetrics, LINE_WIDTH_EPS,
};
pub use wrap_variable::layout_lines_variable_width;
