//! RSX 标签 `<ScrollViewport>`：无滚动条的可滚视口。
//!
//! 引擎契约（信号键、`scroll:` focus、activate）见 [`crate::core::scroll_viewport`]。
//! 带滚动条请用 [`crate::components::ScrollArea`]。

pub use crate::core::scroll_viewport::{
    scroll_viewport, scrollviewport, ScrollViewportBuilder, SCROLL_VIEWPORT_FOCUS_PREFIX,
};

use crate::components::scroll_area::{
    scroll_into_view, scroll_into_view_when, scroll_item_into_view, scroll_item_into_view_when,
};
use crate::core::scroll_viewport::scroll_viewport as scroll_viewport_fn;
use crate::core::tree::Element;
use jadix_macros::component;

impl crate::core::scroll_viewport::ScrollViewportBuilder {
    /// 见 [`scroll_into_view`]。
    pub fn scroll_into_view(self, target: impl AsRef<str>, align: impl AsRef<str>) -> Self {
        scroll_into_view(&self.id, target, align);
        self
    }

    /// 见 [`scroll_into_view_when`]。
    pub fn scroll_into_view_when(
        self,
        target: impl AsRef<str>,
        align: impl AsRef<str>,
        when: impl AsRef<str>,
    ) -> Self {
        scroll_into_view_when(&self.id, target, align, when);
        self
    }

    /// 见 [`scroll_item_into_view`]。
    pub fn scroll_item_into_view(
        self,
        index: usize,
        item_count: usize,
        align: impl AsRef<str>,
    ) -> Self {
        scroll_item_into_view(&self.id, index, item_count, align);
        self
    }

    /// 见 [`scroll_item_into_view_when`]。
    pub fn scroll_item_into_view_when(
        self,
        index: usize,
        item_count: usize,
        align: impl AsRef<str>,
        when: impl AsRef<str>,
    ) -> Self {
        scroll_item_into_view_when(&self.id, index, item_count, align, when);
        self
    }
}

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
    scroll_into_view: Option<String>,
    scroll_align: Option<String>,
    scroll_into_view_when: Option<String>,
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
    if let Some(target) = scroll_into_view {
        let align = scroll_align.unwrap_or_else(|| "center".into());
        let when = scroll_into_view_when.unwrap_or_else(|| "before".into());
        builder = builder.scroll_into_view_when(target, align, when);
    }
    if let Some(c) = children {
        builder = builder.child(c);
    }
    builder.into()
}
