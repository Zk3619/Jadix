//! # ScrollArea
//!
//! 产品层滚动容器：[`ScrollViewport`](crate::components::ScrollViewport) + 内建横/纵滚动条。
//! 无条裁剪或自定义条请用 viewport / `Box`+`overflow`，见 [`crate::core::scroll_viewport`]。
//!
//! ## 用法
//! ```ignore
//! <ScrollArea id="sa-main" w="100%" h="100%"
//!   scrollbar_visibility="auto" overflow_y="auto" overflow_x="auto">
//!   …长内容…
//! </ScrollArea>
//! ```
//! 读偏移：`use_signal(scroll_offset_y_signal_key("sa-main"), 0.0)`。
//!
//! ## 注意
//! - **稳定唯一 `id` 必需**（所有滚动信号键都依赖它）。
//! - 默认 inset inside、visibility auto、条在 right / bottom；`scroll_behavior` 默认
//!   [`smooth`](crate::core::scroll_viewport::SCROLL_BEHAVIOR_SMOOTH)，瞬时跳转用
//!   [`jump`](crate::core::scroll_viewport::SCROLL_BEHAVIOR_JUMP)。
//! - 独立外挂条见 [`crate::components::Scrollbar`]。

use crate::components::base::BaseComponentBuilder;
use crate::components::scrollbar::{scrollbar, ScrollbarBuilder, ScrollbarOrientation};
use crate::core::reactivity::use_signal;
use crate::core::scroll_viewport::activate_scroll_viewport;
use crate::core::tree::{parse_color, Element, IntoDimension, IntoF32};
use rui_macros::component;

// ── re-export viewport 契约（兼容 `scroll_area::…` 旧路径）──────────────────
pub use crate::core::scroll_viewport::{
    normalize_scroll_behavior, scroll_activity_signal_key, scroll_behavior_is_jump,
    scroll_behavior_is_smooth, scroll_behavior_signal_key, scroll_content_h_signal_key,
    scroll_content_w_signal_key, scroll_hover_mode_signal_key, scroll_offset_x_signal_key,
    scroll_offset_y_signal_key, scroll_overflow_allows_scrollbar, scroll_overflow_x_signal_key,
    scroll_overflow_y_signal_key, scroll_scrollbar_inset_signal_key,
    scroll_scrollbar_inset_x_signal_key, scroll_scrollbar_inset_y_signal_key,
    scroll_scrollbar_x_side_signal_key, scroll_scrollbar_y_side_signal_key,
    scroll_target_x_signal_key, scroll_target_y_signal_key, scroll_viewport_h_signal_key,
    scroll_viewport_w_signal_key, signal_keys_for_rest_cached, ScrollAreaSignalKeys,
    ScrollViewportSignalKeys, DEFAULT_SCROLL_BEHAVIOR, SCROLL_AREA_FOCUS_PREFIX,
    SCROLL_BEHAVIOR_JUMP, SCROLL_BEHAVIOR_SMOOTH, SCROLL_VIEWPORT_FOCUS_PREFIX,
};

const SCROLLBAR_EDGE_PAD: f32 = 2.0;

pub struct ScrollAreaBuilder {
    id: String,
    base: BaseComponentBuilder,
    overflow_x: String,
    overflow_y: String,
    scroll_behavior: String,
    hover_mode: String,
    scrollbar_visibility: String,
    scrollbar_inset_x: String,
    scrollbar_inset_y: String,
    scrollbar_y_side: String,
    scrollbar_x_side: String,
    scrollbar_thickness: Option<f32>,
    scrollbar_variant: String,
    scrollbar_track_color: Option<String>,
    scrollbar_thumb_color: Option<String>,
}

pub fn scroll_area(id: &str) -> ScrollAreaBuilder {
    ScrollAreaBuilder {
        id: id.to_string(),
        base: BaseComponentBuilder::new(Element::new().w("100%").h("100%").flex_col()),
        overflow_x: "auto".to_string(),
        overflow_y: "auto".to_string(),
        scroll_behavior: crate::core::scroll_viewport::DEFAULT_SCROLL_BEHAVIOR.to_string(),
        hover_mode: "scrollbar".to_string(),
        scrollbar_visibility: "auto".to_string(),
        scrollbar_inset_x: "inside".to_string(),
        scrollbar_inset_y: "inside".to_string(),
        scrollbar_y_side: "right".to_string(),
        scrollbar_x_side: "bottom".to_string(),
        scrollbar_thickness: None,
        scrollbar_variant: String::new(),
        scrollbar_track_color: None,
        scrollbar_thumb_color: None,
    }
}

pub fn scrollarea(id: &str) -> ScrollAreaBuilder {
    scroll_area(id)
}

/// ScrollArea：viewport + 内建滚动条。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ScrollArea(
    id: String,
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    overflow: Option<String>,
    overflow_x: Option<String>,
    overflow_y: Option<String>,
    scroll_behavior: Option<String>,
    scrollbar_visibility: Option<String>,
    scrollbar_inset: Option<String>,
    scrollbar_inset_x: Option<String>,
    scrollbar_inset_y: Option<String>,
    scrollbar_variant: Option<String>,
    scrollbar_y_side: Option<String>,
    scrollbar_x_side: Option<String>,
    children: Option<Element>,
) -> Element {
    let mut builder = scroll_area(&id);
    if let Some(b) = base_builder {
        builder.base = b(builder.base);
    }
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
    if let Some(v) = scrollbar_visibility {
        builder = builder.scrollbar_visibility(v);
    }
    if let Some(i) = scrollbar_inset {
        builder = builder.scrollbar_inset(i);
    }
    if let Some(ix) = scrollbar_inset_x {
        builder = builder.scrollbar_inset_x(ix);
    }
    if let Some(iy) = scrollbar_inset_y {
        builder = builder.scrollbar_inset_y(iy);
    }
    if let Some(v) = scrollbar_variant {
        builder = builder.scrollbar_variant(v);
    }
    if let Some(s) = scrollbar_y_side {
        builder = builder.scrollbar_y_side(s);
    }
    if let Some(s) = scrollbar_x_side {
        builder = builder.scrollbar_x_side(s);
    }
    if let Some(c) = children {
        builder = builder.child(c);
    }
    builder.into()
}

fn apply_scrollbar_appearance(mut b: ScrollbarBuilder, a: &ScrollAreaBuilder) -> ScrollbarBuilder {
    if let Some(t) = a.scrollbar_thickness {
        b = b.thickness(t);
    }
    if !a.scrollbar_variant.is_empty() {
        b = b.variant(a.scrollbar_variant.as_str());
    }
    if let Some(ref tok) = a.scrollbar_track_color {
        b = b.track_color(parse_color(tok));
    }
    if let Some(ref tok) = a.scrollbar_thumb_color {
        b = b.thumb_color(parse_color(tok));
    }
    b
}

impl ScrollAreaBuilder {
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

    /// `"smooth"`（默认）或 `"jump"`（瞬时）。
    pub fn scroll_behavior(mut self, mode: impl Into<String>) -> Self {
        self.scroll_behavior = mode.into();
        self
    }

    /// - "scrollbar" (default): overlay bars show when hovering track/thumb
    /// - "area": hovering anywhere in the scroll area shows bars
    pub fn hover_mode(mut self, mode: impl Into<String>) -> Self {
        self.hover_mode = mode.into();
        self
    }

    /// `auto` | `always` | `never` (default `auto`). Only `auto` uses hover / scroll fade.
    pub fn scrollbar_visibility(mut self, mode: impl Into<String>) -> Self {
        self.scrollbar_visibility = mode.into();
        self
    }

    /// `inside` | `outside` (default `inside`).
    pub fn scrollbar_inset(mut self, mode: impl Into<String>) -> Self {
        let mode = mode.into();
        self.scrollbar_inset_x = mode.clone();
        self.scrollbar_inset_y = mode;
        self
    }

    pub fn scrollbar_inset_x(mut self, mode: impl Into<String>) -> Self {
        self.scrollbar_inset_x = mode.into();
        self
    }

    pub fn scrollbar_inset_y(mut self, mode: impl Into<String>) -> Self {
        self.scrollbar_inset_y = mode.into();
        self
    }

    pub fn scrollbar_y_side(mut self, side: impl Into<String>) -> Self {
        self.scrollbar_y_side = side.into();
        self
    }

    pub fn scrollbar_x_side(mut self, side: impl Into<String>) -> Self {
        self.scrollbar_x_side = side.into();
        self
    }

    pub fn scrollbar_thickness(mut self, px: impl IntoF32) -> Self {
        self.scrollbar_thickness = Some(px.into_f32());
        self
    }

    pub fn scrollbar_variant(mut self, v: impl Into<String>) -> Self {
        self.scrollbar_variant = v.into();
        self
    }

    pub fn scrollbar_track_color(mut self, token: impl Into<String>) -> Self {
        self.scrollbar_track_color = Some(token.into());
        self
    }

    pub fn scrollbar_thumb_color(mut self, token: impl Into<String>) -> Self {
        self.scrollbar_thumb_color = Some(token.into());
        self
    }

    pub fn w(mut self, width: impl IntoDimension) -> Self {
        self.base = self.base.w(width);
        self
    }

    pub fn h(mut self, height: impl IntoDimension) -> Self {
        self.base = self.base.h(height);
        self
    }

    pub fn max_h(mut self, height: impl IntoDimension) -> Self {
        self.base.el = self.base.el.max_h(height);
        self
    }

    pub fn max_w(mut self, width: impl IntoDimension) -> Self {
        self.base.el = self.base.el.max_w(width);
        self
    }

    pub fn min_w(mut self, width: impl IntoDimension) -> Self {
        self.base = self.base.min_w(width);
        self
    }

    pub fn min_h(mut self, height: impl IntoDimension) -> Self {
        self.base.el = self.base.el.min_h(height);
        self
    }

    pub fn flex_grow(mut self, grow: impl crate::core::tree::IntoF32Raw) -> Self {
        self.base = self.base.flex_grow(grow);
        self
    }

    pub fn flex_shrink(mut self, shrink: impl crate::core::tree::IntoF32Raw) -> Self {
        self.base = self.base.flex_shrink(shrink);
        self
    }

    pub fn p(mut self, padding: impl IntoDimension) -> Self {
        self.base = self.base.p(padding);
        self
    }

    pub fn bg(mut self, bg: [f32; 4]) -> Self {
        self.base = self.base.bg(bg);
        self
    }

    pub fn rounded(mut self, radius: impl crate::core::tree::IntoF32) -> Self {
        self.base = self.base.rounded(radius);
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.base = self.base.child(child);
        self
    }

    pub fn children(mut self, children: impl IntoIterator<Item = Element>) -> Self {
        for c in children {
            self = self.child(c);
        }
        self
    }
}

impl From<ScrollAreaBuilder> for Element {
    fn from(mut builder: ScrollAreaBuilder) -> Element {
        builder.id = crate::utils::resolve_instance_id(std::mem::take(&mut builder.id), "scroll");
        let inset_key = scroll_scrollbar_inset_signal_key(&builder.id);
        let inset_x_key = scroll_scrollbar_inset_x_signal_key(&builder.id);
        let inset_y_key = scroll_scrollbar_inset_y_signal_key(&builder.id);
        let yside_key = scroll_scrollbar_y_side_signal_key(&builder.id);
        let xside_key = scroll_scrollbar_x_side_signal_key(&builder.id);

        // Product-only scrollbar chrome signals (viewport signals set in activate_scroll_viewport).
        use_signal(&inset_key, || builder.scrollbar_inset_y.clone())
            .set(builder.scrollbar_inset_y.clone());
        use_signal(&inset_x_key, || builder.scrollbar_inset_x.clone())
            .set(builder.scrollbar_inset_x.clone());
        use_signal(&inset_y_key, || builder.scrollbar_inset_y.clone())
            .set(builder.scrollbar_inset_y.clone());
        use_signal(&yside_key, || builder.scrollbar_y_side.clone())
            .set(builder.scrollbar_y_side.clone());
        use_signal(&xside_key, || builder.scrollbar_x_side.clone())
            .set(builder.scrollbar_x_side.clone());

        let show_v = scroll_overflow_allows_scrollbar(&builder.overflow_y);
        let show_h = scroll_overflow_allows_scrollbar(&builder.overflow_x);

        let vis = builder.scrollbar_visibility.trim().to_ascii_lowercase();
        let inset_outside_x = builder
            .scrollbar_inset_x
            .trim()
            .eq_ignore_ascii_case("outside");
        let inset_outside_y = builder
            .scrollbar_inset_y
            .trim()
            .eq_ignore_ascii_case("outside");
        let v_right = !builder.scrollbar_y_side.trim().eq_ignore_ascii_case("left");
        let h_top = builder.scrollbar_x_side.trim().eq_ignore_ascii_case("top");
        let base_thickness = builder.scrollbar_thickness.unwrap_or(10.0).max(2.0);
        let vis_x = if vis == "auto" && inset_outside_x {
            "always"
        } else {
            vis.as_str()
        };
        let vis_y = if vis == "auto" && inset_outside_y {
            "always"
        } else {
            vis.as_str()
        };

        let z_overlay = 1000_i32;
        let mut v_scroll = apply_scrollbar_appearance(
            scrollbar(&format!("__rui_v_scrollbar_{}", builder.id))
                .target(builder.id.clone())
                .orientation(ScrollbarOrientation::Vertical)
                .visibility(vis_y)
                .z_index(z_overlay),
            &builder,
        );
        let mut h_scroll = apply_scrollbar_appearance(
            scrollbar(&format!("__rui_h_scrollbar_{}", builder.id))
                .target(builder.id.clone())
                .orientation(ScrollbarOrientation::Horizontal)
                .visibility(vis_x)
                .z_index(z_overlay),
            &builder,
        );

        if inset_outside_y {
            v_scroll = v_scroll.relative().flex_shrink(0.0).flex_grow(0.0);
        } else {
            v_scroll = v_scroll
                .absolute()
                .top(SCROLLBAR_EDGE_PAD)
                .bottom(SCROLLBAR_EDGE_PAD);
            if v_right {
                v_scroll = v_scroll.right(SCROLLBAR_EDGE_PAD);
            } else {
                v_scroll = v_scroll.left(SCROLLBAR_EDGE_PAD);
            }
        }
        if inset_outside_x {
            h_scroll = h_scroll.relative().flex_shrink(0.0).flex_grow(0.0);
        } else {
            h_scroll = h_scroll
                .absolute()
                .left(SCROLLBAR_EDGE_PAD)
                .right(SCROLLBAR_EDGE_PAD);
            if h_top {
                h_scroll = h_scroll.top(SCROLLBAR_EDGE_PAD);
            } else {
                h_scroll = h_scroll.bottom(SCROLLBAR_EDGE_PAD);
            }
        }

        let mut outer = builder.base.el;

        let padding = outer.style.padding;
        let gap = outer.style.gap;
        let flex_direction = outer.style.flex_direction;
        let justify_content = outer.style.justify_content;
        let align_items = outer.style.align_items;

        outer.style.padding = taffy::prelude::Rect::zero();
        outer.style.gap = taffy::geometry::Size {
            width: taffy::style::LengthPercentage::Length(0.0),
            height: taffy::style::LengthPercentage::Length(0.0),
        };
        outer.style.overflow.x = taffy::style::Overflow::Visible;
        outer.style.overflow.y = taffy::style::Overflow::Visible;
        outer.style.position = taffy::style::Position::Relative;

        let content_children = std::mem::take(&mut outer.children);
        let inside_reserve = base_thickness + SCROLLBAR_EDGE_PAD;
        let add_padding = |p: &mut taffy::style::LengthPercentage, delta: f32| {
            *p = match *p {
                taffy::style::LengthPercentage::Length(v) => {
                    taffy::style::LengthPercentage::Length((v + delta).max(0.0))
                }
                taffy::style::LengthPercentage::Percent(v) => {
                    taffy::style::LengthPercentage::Percent(v)
                }
            };
        };

        let mut viewport = activate_scroll_viewport(
            Element::new()
                .w("100%")
                .h("100%")
                .children(content_children),
            &builder.id,
            &builder.overflow_x,
            &builder.overflow_y,
            &builder.scroll_behavior,
            &builder.hover_mode,
        );
        viewport.style.padding = padding;
        if show_v && !inset_outside_y && !v_right {
            add_padding(&mut viewport.style.padding.left, inside_reserve);
        }
        if show_h && !inset_outside_x && h_top {
            add_padding(&mut viewport.style.padding.top, inside_reserve);
        }
        viewport.style.gap = gap;
        viewport.style.flex_direction = flex_direction;
        viewport.style.justify_content = justify_content;
        viewport.style.align_items = align_items;

        let mut v_opt: Option<Element> = if show_v { Some(v_scroll.into()) } else { None };
        let mut h_opt: Option<Element> = if show_h { Some(h_scroll.into()) } else { None };

        if inset_outside_x || inset_outside_y {
            outer.style.flex_direction = taffy::style::FlexDirection::Column;
            let mut inner_col = Element::new()
                .flex_col()
                .flex_grow(1.0)
                .min_w(0.0)
                .min_h(0.0)
                .w("100%");
            inner_col.style.justify_content = Some(taffy::style::JustifyContent::FlexStart);
            inner_col.style.align_items = Some(taffy::style::AlignItems::Stretch);
            let vp = viewport
                .flex_grow(1.0)
                .flex_shrink(1.0)
                .min_h(0.0)
                .min_w(0.0);
            let mut h_left = if inset_outside_x { h_opt.take() } else { None };
            if show_h && inset_outside_x && h_top {
                if let Some(h) = h_left.take() {
                    inner_col = inner_col.child(h);
                }
            }
            inner_col = inner_col.child(vp);
            if show_h && inset_outside_x && !h_top {
                if let Some(h) = h_left.take() {
                    inner_col = inner_col.child(h);
                }
            }

            let mut row = Element::new()
                .flex_row()
                .flex_grow(1.0)
                .min_w(0.0)
                .min_h(0.0)
                .w("100%")
                .h("100%");
            let mut v_left = if inset_outside_y { v_opt.take() } else { None };
            if show_v && inset_outside_y && !v_right {
                if let Some(v) = v_left.take() {
                    row = row.child(v);
                }
            }
            row = row.child(inner_col);
            if show_v && inset_outside_y && v_right {
                if let Some(v) = v_left.take() {
                    row = row.child(v);
                }
            }
            outer = outer.child(row);
            if show_v && !inset_outside_y {
                if let Some(v) = v_opt.take() {
                    outer = outer.child(v);
                }
            }
            if show_h && !inset_outside_x {
                if let Some(h) = h_opt.take() {
                    outer = outer.child(h);
                }
            }
            outer.template_id = Some(builder.id.clone());
            outer
        } else {
            outer = outer.child(viewport);
            if let Some(v) = v_opt.take() {
                outer = outer.child(v);
            }
            if let Some(h) = h_opt.take() {
                outer = outer.child(h);
            }
            outer.template_id = Some(builder.id);
            outer
        }
    }
}

impl crate::core::tree::IntoElementList for ScrollAreaBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}
