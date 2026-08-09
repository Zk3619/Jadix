//! Text stack façade: layout-time measure + render-time glyphon bridge.

pub mod markdown;
pub mod markdown_inline;
pub mod measure;
pub mod wrap_variable;

pub use crate::renderer::RuiTextRenderer;
pub use markdown::parse_markdown_blocks;
pub use markdown_inline::parse_markdown_inline;
pub use measure::{
    clear_shared_font_system, glyphon_soft_wrap_line_count, install_shared_font_system,
    intrinsic_width, invalidate_prepare_cache, layout, layout_lines, measure_leaf, prepare,
    prepare_rich, prepare_with_family, shared_font_system_ptr, shutdown_layout_text, LINE_WIDTH_EPS,
    PreparedLine, PreparedText, TextLayoutMetrics,
};
pub use wrap_variable::layout_lines_variable_width;
