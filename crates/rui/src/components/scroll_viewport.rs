//! RSX 标签 `<ScrollViewport>`：无滚动条的可滚视口。
//!
//! 引擎契约（信号键、`scroll:` focus、activate）见 [`crate::core::scroll_viewport`]。
//! 带滚动条请用 [`crate::components::ScrollArea`]。

pub use crate::core::scroll_viewport::{
    scroll_viewport, scrollviewport, ScrollViewportBuilder, SCROLL_VIEWPORT_FOCUS_PREFIX,
};

use crate::core::scroll_viewport::scroll_viewport as scroll_viewport_fn;
use crate::core::tree::Element;
use rui_macros::component;

/// 无滚动条的滚动视口组件。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ScrollViewport(
    id: String,
    overflow: Option<String>,
    overflow_x: Option<String>,
    overflow_y: Option<String>,
    scroll_behavior: Option<String>,
    hover_mode: Option<String>,
    children: Option<Element>,
) -> Element {
    let mut builder = scroll_viewport_fn(&id);
    if let Some(o) = overflow {
        builder = builder.overflow(o);
    }
    if let Some(ox) = overflow_x {
        builder = builder.overflow_x(ox);
    }
    if let Some(oy) = overflow_y {
        builder = builder.overflow_y(oy);
    }
    if let Some(sb) = scroll_behavior {
        builder = builder.scroll_behavior(sb);
    }
    if let Some(hm) = hover_mode {
        builder = builder.hover_mode(hm);
    }
    if let Some(c) = children {
        builder = builder.child(c);
    }
    builder.into()
}
