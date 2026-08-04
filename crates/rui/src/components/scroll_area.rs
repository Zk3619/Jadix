//! # ScrollArea
//!
//! 溢出滚动视口：裁剪内容、跟踪偏移，并内建横/纵滚动条。为虚拟列表等提供
//! `scroll_offset_*` / `scroll_viewport_*` 等信号键。
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
//! - 默认 inset inside、visibility auto、条在 right / bottom；`scroll_behavior="smooth"` 可选。
//! - 独立外挂条见 [`crate::components::Scrollbar`]；多数场景用本组件内建即可。

use std::collections::hash_map::Entry;
use std::collections::HashMap;

use crate::components::base::BaseComponentBuilder;
use crate::components::scrollbar::{
    scrollbar, scrollbar_visibility_signal_key, ScrollbarBuilder, ScrollbarOrientation,
};
use crate::core::reactivity::use_signal;
use crate::core::tree::{parse_color, Element, IntoDimension, IntoF32};
use rui_macros::component;

const SCROLLBAR_EDGE_PAD: f32 = 2.0;

/// Pre-formatted signal id strings for one scroll-area `rest` id. Caching avoids per-frame
/// `format!` allocations when many nested `ScrollArea`s exist (outer scroll hot path).
pub struct ScrollAreaSignalKeys {
    pub offset_x: String,
    pub offset_y: String,
    pub target_x: String,
    pub target_y: String,
    pub viewport_w: String,
    pub viewport_h: String,
    pub content_w: String,
    pub content_h: String,
    pub behavior: String,
    pub activity: String,
    pub overflow_x: String,
    pub overflow_y: String,
    /// Legacy `__rui_scroll_hover_{rest}` (content hover).
    pub hover_legacy: String,
    pub hover_axis_x: String,
    pub hover_axis_y: String,
    pub scrollbar_visibility_x: String,
    pub scrollbar_visibility_y: String,
    pub spring_x: String,
    pub spring_y: String,
}

impl ScrollAreaSignalKeys {
    pub fn new(rest: &str) -> Self {
        Self {
            offset_x: scroll_offset_x_signal_key(rest),
            offset_y: scroll_offset_y_signal_key(rest),
            target_x: scroll_target_x_signal_key(rest),
            target_y: scroll_target_y_signal_key(rest),
            viewport_w: scroll_viewport_w_signal_key(rest),
            viewport_h: scroll_viewport_h_signal_key(rest),
            content_w: scroll_content_w_signal_key(rest),
            content_h: scroll_content_h_signal_key(rest),
            behavior: scroll_behavior_signal_key(rest),
            activity: scroll_activity_signal_key(rest),
            overflow_x: scroll_overflow_x_signal_key(rest),
            overflow_y: scroll_overflow_y_signal_key(rest),
            hover_legacy: format!("__rui_scroll_hover_{rest}"),
            hover_axis_x: format!("__rui_scroll_hover_{rest}:x"),
            hover_axis_y: format!("__rui_scroll_hover_{rest}:y"),
            scrollbar_visibility_x: scrollbar_visibility_signal_key(rest, "x"),
            scrollbar_visibility_y: scrollbar_visibility_signal_key(rest, "y"),
            spring_x: format!("scrollbar_spring_{rest}:x"),
            spring_y: format!("scrollbar_spring_{rest}:y"),
        }
    }
}

pub fn signal_keys_for_rest_cached<'a>(
    rest: &str,
    cache: &'a mut HashMap<String, ScrollAreaSignalKeys>,
) -> &'a ScrollAreaSignalKeys {
    match cache.entry(rest.to_string()) {
        Entry::Occupied(e) => e.into_mut(),
        Entry::Vacant(v) => v.insert(ScrollAreaSignalKeys::new(rest)),
    }
}

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
        scroll_behavior: "auto".to_string(),
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

/// ScrollArea component for overflow management.
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

pub const SCROLL_AREA_FOCUS_PREFIX: &str = "scroll:";

pub fn scroll_offset_x_signal_key(id: &str) -> String {
    format!("__rui_scroll_x_{}", id)
}

pub fn scroll_offset_y_signal_key(id: &str) -> String {
    format!("__rui_scroll_y_{}", id)
}

pub fn scroll_viewport_w_signal_key(id: &str) -> String {
    format!("__rui_scroll_viewport_w_{}", id)
}

pub fn scroll_viewport_h_signal_key(id: &str) -> String {
    format!("__rui_scroll_viewport_h_{}", id)
}

pub fn scroll_content_w_signal_key(id: &str) -> String {
    format!("__rui_scroll_content_w_{}", id)
}

pub fn scroll_content_h_signal_key(id: &str) -> String {
    format!("__rui_scroll_content_h_{}", id)
}

pub fn scroll_target_x_signal_key(id: &str) -> String {
    format!("__rui_scroll_target_x_{}", id)
}

pub fn scroll_target_y_signal_key(id: &str) -> String {
    format!("__rui_scroll_target_y_{}", id)
}

pub fn scroll_behavior_signal_key(id: &str) -> String {
    format!("__rui_scroll_behavior_{}", id)
}

pub fn scroll_activity_signal_key(id: &str) -> String {
    format!("__rui_scroll_activity_{}", id)
}

pub fn scroll_hover_mode_signal_key(id: &str) -> String {
    format!("__rui_scroll_hover_mode_{}", id)
}

pub fn scroll_overflow_x_signal_key(id: &str) -> String {
    format!("__rui_scroll_overflow_x_{}", id)
}

pub fn scroll_overflow_y_signal_key(id: &str) -> String {
    format!("__rui_scroll_overflow_y_{}", id)
}

pub fn scroll_scrollbar_inset_signal_key(id: &str) -> String {
    format!("__rui_scroll_scrollbar_inset_{}", id)
}

pub fn scroll_scrollbar_inset_x_signal_key(id: &str) -> String {
    format!("__rui_scroll_scrollbar_inset_x_{}", id)
}

pub fn scroll_scrollbar_inset_y_signal_key(id: &str) -> String {
    format!("__rui_scroll_scrollbar_inset_y_{}", id)
}

pub fn scroll_scrollbar_y_side_signal_key(id: &str) -> String {
    format!("__rui_scroll_scrollbar_y_side_{}", id)
}

pub fn scroll_scrollbar_x_side_signal_key(id: &str) -> String {
    format!("__rui_scroll_scrollbar_x_side_{}", id)
}

/// Matches `allow_axis` in `update_scrollbar_elements`: scrollbar widgets only for scrollable overflow modes.
pub fn scroll_overflow_allows_scrollbar(overflow: &str) -> bool {
    let o = overflow.trim();
    !o.eq_ignore_ascii_case("hidden")
        && !o.eq_ignore_ascii_case("clip")
        && !o.eq_ignore_ascii_case("visible")
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

    /// Horizontal scrollbar inset mode: `inside` | `outside` (default `inside`).
    pub fn scrollbar_inset_x(mut self, mode: impl Into<String>) -> Self {
        self.scrollbar_inset_x = mode.into();
        self
    }

    /// Vertical scrollbar inset mode: `inside` | `outside` (default `inside`).
    pub fn scrollbar_inset_y(mut self, mode: impl Into<String>) -> Self {
        self.scrollbar_inset_y = mode.into();
        self
    }

    /// Vertical scrollbar: `left` | `right` (default `right`).
    pub fn scrollbar_y_side(mut self, side: impl Into<String>) -> Self {
        self.scrollbar_y_side = side.into();
        self
    }

    /// Horizontal scrollbar: `top` | `bottom` (default `bottom`).
    pub fn scrollbar_x_side(mut self, side: impl Into<String>) -> Self {
        self.scrollbar_x_side = side.into();
        self
    }

    pub fn scrollbar_thickness(mut self, px: impl IntoF32) -> Self {
        self.scrollbar_thickness = Some(px.into_f32());
        self
    }

    /// `capsule` (default) or `rect`.
    pub fn scrollbar_variant(mut self, v: impl Into<String>) -> Self {
        self.scrollbar_variant = v.into();
        self
    }

    /// Theme token, e.g. `"secondary"` (see `parse_color`).
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
    fn from(builder: ScrollAreaBuilder) -> Element {
        let sx_key = scroll_offset_x_signal_key(&builder.id);
        let sy_key = scroll_offset_y_signal_key(&builder.id);
        let behavior_key = scroll_behavior_signal_key(&builder.id);
        let hover_mode_key = scroll_hover_mode_signal_key(&builder.id);
        let overflow_x_key = scroll_overflow_x_signal_key(&builder.id);
        let overflow_y_key = scroll_overflow_y_signal_key(&builder.id);
        let inset_key = scroll_scrollbar_inset_signal_key(&builder.id);
        let inset_x_key = scroll_scrollbar_inset_x_signal_key(&builder.id);
        let inset_y_key = scroll_scrollbar_inset_y_signal_key(&builder.id);
        let yside_key = scroll_scrollbar_y_side_signal_key(&builder.id);
        let xside_key = scroll_scrollbar_x_side_signal_key(&builder.id);

        let sx = use_signal(&sx_key, || 0.0_f32).peek();
        let sy = use_signal(&sy_key, || 0.0_f32).peek();
        use_signal(&behavior_key, || builder.scroll_behavior.clone())
            .set(builder.scroll_behavior.clone());
        use_signal(&hover_mode_key, || builder.hover_mode.clone()).set(builder.hover_mode.clone());
        use_signal(&overflow_x_key, || builder.overflow_x.clone()).set(builder.overflow_x.clone());
        use_signal(&overflow_y_key, || builder.overflow_y.clone()).set(builder.overflow_y.clone());
        // Keep legacy combined inset signal for compatibility.
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
        // Corner reservation is runtime-driven in `update_scrollbar_elements` using actual
        // scrollability metrics, so we keep static inset anchors here minimal.

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
        // Reserve exactly the inside scrollbar occupied lane:
        // track thickness + edge inset from the viewport boundary.
        let inside_reserve = base_thickness + SCROLLBAR_EDGE_PAD;
        let add_padding = |p: &mut taffy::style::LengthPercentage, delta: f32| {
            *p = match *p {
                taffy::style::LengthPercentage::Length(v) => {
                    taffy::style::LengthPercentage::Length((v + delta).max(0.0))
                }
                // Keep percentage paddings unchanged; we only add deterministic pixel reserve
                // for the scrollbar slot when original padding is absolute.
                taffy::style::LengthPercentage::Percent(v) => {
                    taffy::style::LengthPercentage::Percent(v)
                }
            };
        };
        let mut viewport = Element::new()
            .w("100%")
            .h("100%")
            .focus_id(format!("{}{}", SCROLL_AREA_FOCUS_PREFIX, builder.id))
            .overflow_x(builder.overflow_x.clone())
            .overflow_y(builder.overflow_y.clone())
            .scroll_offset(sx, sy)
            .children(content_children);
        viewport.style.padding = padding;
        // Inside scrollbar must reserve a transparent slot so hidden bars don't overlap
        // content hit-testing (buttons should never accidentally hit scrollbar tracks).
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
            // Keep viewport lane pinned to start in outside mode.
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
