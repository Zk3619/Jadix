//! RSX 标签 `<ScrollViewport>`：无滚动条的可滚视口。
//!
//! 引擎契约（信号键、`scroll:` focus、activate）见 [`crate::core::scroll_viewport`]。
//! 带滚动条请用 [`crate::components::ScrollArea`]。

pub use crate::core::scroll_viewport::SCROLL_VIEWPORT_FOCUS_PREFIX;
use crate::core::scroll_viewport::{activate_scroll_viewport, DEFAULT_SCROLL_BEHAVIOR};

use crate::components::scroll_area::{
    scroll_into_view, scroll_into_view_when, scroll_item_into_view, scroll_item_into_view_when,
};
use crate::core::tree::{Element, IntoCssDim, IntoF32Raw};
use jadix_macros::component;

pub struct ScrollViewportBuilder {
    pub(crate) id: String,
    el: Element,
    overflow_x: String,
    overflow_y: String,
    scroll_behavior: String,
    hover_mode: String,
}

pub fn scroll_viewport(id: &str) -> ScrollViewportBuilder {
    ScrollViewportBuilder {
        id: id.to_string(),
        el: Element::new().w("100%").h("100%"),
        overflow_x: "auto".to_string(),
        overflow_y: "auto".to_string(),
        scroll_behavior: DEFAULT_SCROLL_BEHAVIOR.to_string(),
        hover_mode: "scrollbar".to_string(),
    }
}

pub fn scrollviewport(id: &str) -> ScrollViewportBuilder {
    scroll_viewport(id)
}

impl ScrollViewportBuilder {
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn overflow(mut self, mode: impl Into<String>) -> Self {
        let mode = mode.into();
        self.overflow_x = mode.clone();
        self.overflow_y = mode;
        self
    }

    pub fn overflow_x(mut self, mode: impl Into<String>) -> Self {
        self.overflow_x = mode.into();
        self
    }

    pub fn overflow_y(mut self, mode: impl Into<String>) -> Self {
        self.overflow_y = mode.into();
        self
    }

    /// `"smooth"` (default) or `"jump"` (instant).
    pub fn scroll_behavior(mut self, mode: impl Into<String>) -> Self {
        self.scroll_behavior = mode.into();
        self
    }

    pub fn hover_mode(mut self, mode: impl Into<String>) -> Self {
        self.hover_mode = mode.into();
        self
    }

    pub fn w(mut self, width: impl IntoCssDim) -> Self {
        self.el = self.el.w(width);
        self
    }

    pub fn h(mut self, height: impl IntoCssDim) -> Self {
        self.el = self.el.h(height);
        self
    }

    pub fn flex_grow(mut self, grow: impl IntoF32Raw) -> Self {
        self.el = self.el.flex_grow(grow);
        self
    }

    pub fn flex_shrink(mut self, shrink: impl IntoF32Raw) -> Self {
        self.el = self.el.flex_shrink(shrink);
        self
    }

    pub fn min_w(mut self, w: impl IntoCssDim) -> Self {
        self.el = self.el.min_w(w);
        self
    }

    pub fn min_h(mut self, h: impl IntoCssDim) -> Self {
        self.el = self.el.min_h(h);
        self
    }

    pub fn p(mut self, padding: impl IntoCssDim) -> Self {
        self.el = self.el.p(padding);
        self
    }

    pub fn bg(mut self, bg: [f32; 4]) -> Self {
        self.el = self.el.bg(bg);
        self
    }

    pub fn flex_col(mut self) -> Self {
        self.el = self.el.flex_col();
        self
    }

    pub fn flex_row(mut self) -> Self {
        self.el = self.el.flex_row();
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.el = self.el.child(child);
        self
    }

    pub fn children(mut self, children: impl IntoIterator<Item = Element>) -> Self {
        for c in children {
            self = self.child(c);
        }
        self
    }

    /// 取出内部 Element（已含 children / 尺寸），供 ScrollArea 等再包一层。
    pub fn into_inner_element(self) -> (String, Element, String, String, String, String) {
        (
            self.id,
            self.el,
            self.overflow_x,
            self.overflow_y,
            self.scroll_behavior,
            self.hover_mode,
        )
    }
}

impl From<ScrollViewportBuilder> for Element {
    fn from(builder: ScrollViewportBuilder) -> Element {
        let ScrollViewportBuilder {
            id,
            el,
            overflow_x,
            overflow_y,
            scroll_behavior,
            hover_mode,
        } = builder;
        activate_scroll_viewport(el, &id, overflow_x, overflow_y, scroll_behavior, hover_mode)
    }
}

impl crate::core::tree::IntoElementList for ScrollViewportBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl ScrollViewportBuilder {
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
    let mut builder = scroll_viewport(&id);
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
