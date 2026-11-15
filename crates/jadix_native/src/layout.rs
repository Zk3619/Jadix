//! Native hosts retain their public API over the shared CPU layout engine.
pub use jadix_layout::presentation::{
    LayoutError, LayoutStats, MeasureMode, MeasureRequest, PresentationLayout as NativeLayout,
};
