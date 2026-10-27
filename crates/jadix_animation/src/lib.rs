pub mod animation;
pub mod clock;
pub mod easing;
pub mod element_style;
pub mod generator;
#[cfg(feature = "animation-path")]
pub mod icon_morph;
#[cfg(feature = "animation-path")]
pub mod path;
pub mod physics;
pub mod timeline;
pub mod timing;
pub use animation::*;
pub use element_style::*;
pub use generator::*;
pub use physics::*;
pub use timing::*;
/// Parse CSS time with **required** unit (`ms` / `s`). Bare numbers are not times.
pub fn parse_css_time_ms_strict(s: &str) -> Option<f32> {
    let s = s.trim();
    if let Some(ms) = s.strip_suffix("ms").or_else(|| s.strip_suffix("MS")) {
        ms.trim().parse::<f32>().ok()
    } else if let Some(secs) = s.strip_suffix('s').or_else(|| s.strip_suffix('S')) {
        // `forwards`.strip_suffix('s') → `forward` → parse fails (good).
        secs.trim().parse::<f32>().ok().map(|v| v * 1000.0)
    } else {
        None
    }
}
