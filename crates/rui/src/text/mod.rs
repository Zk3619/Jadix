//! Text stack façade: layout-time measure + render-time glyphon bridge.

pub mod measure;

pub use crate::renderer::RuiTextRenderer;
pub use measure::{
    intrinsic_width, layout, measure_leaf, prepare, prepare_with_family, PreparedText,
    TextLayoutMetrics,
};
