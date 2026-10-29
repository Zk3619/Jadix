//! 布局原语：flex 容器、间距、背景、溢出等，最终落成一棵 [`Element`]。
//!
//! RSX：`VStack` / `HStack` / `GStack`（别名 `Grid`）/ `Box`（别名 `div` / `Div`）由宏映射到
//! 本 Builder；SVG 形状用小写/Pascal `<rect>` / `<Rect>`（见 `primitives::svg`）。
//! [`vstack`] / [`hstack`] / [`gstack`] / [`div`]（兼容 [`r#box`] / [`boxed`]）。

use crate::layout::GridTrack;

use crate::core::scroll_viewport::maybe_activate_scroll_viewport;
use crate::core::tree::{
    Element, IntoBorderWidths, IntoCssDim, IntoCssRectPair, IntoF32, IntoNodeBg, IntoNodeClass,
    IntoNodeFg, IntoNodeOpacity, IntoNodeSize,
};
use crate::style::{
    IntoAlignItems, IntoCursor, IntoFlexWrap, IntoJustifyContent, IntoOverflow, IntoPaintColor,
};

pub struct BoxBuilder {
    pub el: Element,
    /// When set and overflow is `scroll`/`auto` on an axis, registers as a scroll viewport.
    scroll_id: Option<String>,
    scroll_behavior: String,
    hover_mode: String,
}

impl Default for BoxBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl BoxBuilder {
    pub fn hot_reload_declaration_base(mut self, names: &[&str], opaque: bool) -> Self {
        self.el = self.el.hot_reload_declaration_base(names, opaque);
        self
    }

    pub fn new() -> Self {
        Self {
            el: Element::new(),
            scroll_id: None,
            scroll_behavior: crate::core::scroll_viewport::DEFAULT_SCROLL_BEHAVIOR.to_string(),
            hover_mode: "scrollbar".to_string(),
        }
    }

    /// Stable id for scroll viewport registration (`scroll:{id}`) and tree identity
    /// (`template_id`, e.g. Portal `mount`). RSX `id` on Box / VStack / HStack / GStack / Rect.
    pub fn id(mut self, id: impl Into<String>) -> Self {
        let id = id.into();
        self.scroll_id = Some(id.clone());
        if self.el.template_id.is_none() {
            self.el = self.el.template_id(id);
        }
        self
    }

    pub fn scroll_id(self, id: impl Into<String>) -> Self {
        self.id(id)
    }

    pub fn scroll_behavior(mut self, mode: impl Into<String>) -> Self {
        self.scroll_behavior = mode.into();
        self
    }

    pub fn hover_mode(mut self, mode: impl Into<String>) -> Self {
        self.hover_mode = mode.into();
        self
    }

    /// Static CSS dim, [`crate::core::reactivity::Signal`], or `|| dim` compute.
    /// `w(sig.get())` is still a snapshot — pass `sig` or `|| sig.get()`.
    #[track_caller]
    pub fn w(mut self, w: impl IntoNodeSize) -> Self {
        self.el = w.apply_node_size(self.el, "w");
        self
    }

    /// Static CSS dim, [`crate::core::reactivity::Signal`], or `|| dim` compute.
    /// `h(sig.get())` is still a snapshot — pass `sig` or `|| sig.get()`.
    #[track_caller]
    pub fn h(mut self, h: impl IntoNodeSize) -> Self {
        self.el = h.apply_node_size(self.el, "h");
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

    pub fn max_w(mut self, w: impl IntoCssDim) -> Self {
        self.el = self.el.max_w(w);
        self
    }

    pub fn max_h(mut self, h: impl IntoCssDim) -> Self {
        self.el = self.el.max_h(h);
        self
    }

    /// CSS `box-sizing: border-box`（默认）。Tailwind `box-border`。
    pub fn border_box(mut self) -> Self {
        self.el = self.el.border_box();
        self
    }

    /// CSS `box-sizing: content-box`。Tailwind `box-content`。
    pub fn content_box(mut self) -> Self {
        self.el = self.el.content_box();
        self
    }

    pub fn p(mut self, p: impl IntoCssDim) -> Self {
        self.el = self.el.p(p);
        self
    }

    pub fn px(mut self, p: impl IntoCssDim) -> Self {
        self.el = self.el.px(p);
        self
    }

    pub fn py(mut self, p: impl IntoCssDim) -> Self {
        self.el = self.el.py(p);
        self
    }

    pub fn pt(mut self, p: impl IntoCssDim) -> Self {
        self.el = self.el.pt(p);
        self
    }

    pub fn pr(mut self, p: impl IntoCssDim) -> Self {
        self.el = self.el.pr(p);
        self
    }

    pub fn pb(mut self, p: impl IntoCssDim) -> Self {
        self.el = self.el.pb(p);
        self
    }

    pub fn pl(mut self, p: impl IntoCssDim) -> Self {
        self.el = self.el.pl(p);
        self
    }

    pub fn padding_inline(mut self, p: impl IntoCssRectPair) -> Self {
        self.el = self.el.padding_inline(p);
        self
    }

    pub fn padding_block(mut self, p: impl IntoCssRectPair) -> Self {
        self.el = self.el.padding_block(p);
        self
    }

    pub fn padding_inline_start(mut self, p: impl IntoCssDim) -> Self {
        self.el = self.el.padding_inline_start(p);
        self
    }

    pub fn padding_inline_end(mut self, p: impl IntoCssDim) -> Self {
        self.el = self.el.padding_inline_end(p);
        self
    }

    pub fn padding_block_start(mut self, p: impl IntoCssDim) -> Self {
        self.el = self.el.padding_block_start(p);
        self
    }

    pub fn padding_block_end(mut self, p: impl IntoCssDim) -> Self {
        self.el = self.el.padding_block_end(p);
        self
    }

    pub fn m(mut self, m: impl IntoCssDim) -> Self {
        self.el = self.el.m(m);
        self
    }

    pub fn mx(mut self, m: impl IntoCssDim) -> Self {
        self.el = self.el.mx(m);
        self
    }

    pub fn my(mut self, m: impl IntoCssDim) -> Self {
        self.el = self.el.my(m);
        self
    }

    /// See [`Element::bleed_x`].
    pub fn bleed_x(mut self, pad: impl IntoCssDim) -> Self {
        self.el = self.el.bleed_x(pad);
        self
    }

    /// See [`Element::bleed_y`].
    pub fn bleed_y(mut self, pad: impl IntoCssDim) -> Self {
        self.el = self.el.bleed_y(pad);
        self
    }

    /// See [`Element::menu_bleed`].
    pub fn menu_bleed(mut self) -> Self {
        self.el = self.el.menu_bleed();
        self
    }

    pub fn mt(mut self, m: impl IntoCssDim) -> Self {
        self.el = self.el.mt(m);
        self
    }

    pub fn mr(mut self, m: impl IntoCssDim) -> Self {
        self.el = self.el.mr(m);
        self
    }

    pub fn mb(mut self, m: impl IntoCssDim) -> Self {
        self.el = self.el.mb(m);
        self
    }

    pub fn ml(mut self, m: impl IntoCssDim) -> Self {
        self.el = self.el.ml(m);
        self
    }

    pub fn margin_inline(mut self, m: impl IntoCssRectPair) -> Self {
        self.el = self.el.margin_inline(m);
        self
    }

    pub fn margin_block(mut self, m: impl IntoCssRectPair) -> Self {
        self.el = self.el.margin_block(m);
        self
    }

    pub fn margin_inline_start(mut self, m: impl IntoCssDim) -> Self {
        self.el = self.el.margin_inline_start(m);
        self
    }

    pub fn margin_inline_end(mut self, m: impl IntoCssDim) -> Self {
        self.el = self.el.margin_inline_end(m);
        self
    }

    pub fn margin_block_start(mut self, m: impl IntoCssDim) -> Self {
        self.el = self.el.margin_block_start(m);
        self
    }

    pub fn margin_block_end(mut self, m: impl IntoCssDim) -> Self {
        self.el = self.el.margin_block_end(m);
        self
    }

    pub fn inline_size(mut self, s: impl IntoCssDim) -> Self {
        self.el = self.el.inline_size(s);
        self
    }

    pub fn block_size(mut self, s: impl IntoCssDim) -> Self {
        self.el = self.el.block_size(s);
        self
    }

    /// Static paint, [`crate::core::reactivity::Signal`], or `|| color` compute.
    /// `bg(sig.get())` is still a snapshot — pass `sig` or `|| sig.get()`.
    #[track_caller]
    pub fn bg(mut self, color: impl IntoNodeBg) -> Self {
        self.el = color.apply_node_bg(self.el);
        self
    }

    /// Static opacity, [`crate::core::reactivity::Signal`], or `|| opacity` compute.
    /// `opacity(sig.get())` is still a snapshot — pass `sig` or `|| sig.get()`.
    #[track_caller]
    pub fn opacity(mut self, o: impl IntoNodeOpacity) -> Self {
        self.el = o.apply_node_opacity(self.el);
        self
    }

    pub fn opacity_group(mut self, enabled: bool) -> Self {
        self.el = self.el.opacity_group(enabled);
        self
    }

    /// CSS gradient: `linear-gradient(...)` / `radial-gradient(...)` / `conic-gradient(...)`.
    pub fn background_image(mut self, css: impl AsRef<str>) -> Self {
        self.el = self.el.background_image(css);
        self
    }

    pub fn gradient(mut self, grad: crate::style::Gradient) -> Self {
        self.el = self.el.gradient(grad);
        self
    }

    pub fn linear_gradient(mut self, from: [f32; 4], to: [f32; 4], angle_deg: f32) -> Self {
        self.el = self.el.linear_gradient(from, to, angle_deg);
        self
    }

    pub fn border(mut self, color: impl IntoPaintColor, width: f32) -> Self {
        self.el = self.el.border(color, width);
        self
    }
    pub fn border_style(mut self, style: impl Into<crate::style::LineStyle>) -> Self {
        self.el = self.el.border_style(style);
        self
    }
    pub fn border_color(mut self, color: impl IntoPaintColor) -> Self {
        self.el = self.el.border_color(color);
        self
    }
    pub fn border_width(mut self, width: impl IntoBorderWidths) -> Self {
        self.el = self.el.border_width(width);
        self
    }
    pub fn border_top_width(mut self, width: impl IntoF32) -> Self {
        self.el = self.el.border_top_width(width);
        self
    }
    pub fn border_right_width(mut self, width: impl IntoF32) -> Self {
        self.el = self.el.border_right_width(width);
        self
    }
    pub fn border_bottom_width(mut self, width: impl IntoF32) -> Self {
        self.el = self.el.border_bottom_width(width);
        self
    }
    pub fn border_left_width(mut self, width: impl IntoF32) -> Self {
        self.el = self.el.border_left_width(width);
        self
    }
    pub fn border_top(mut self, color: [f32; 4], width: impl IntoF32) -> Self {
        self.el = self.el.border_top(color, width);
        self
    }
    pub fn border_right(mut self, color: [f32; 4], width: impl IntoF32) -> Self {
        self.el = self.el.border_right(color, width);
        self
    }
    pub fn border_bottom(mut self, color: [f32; 4], width: impl IntoF32) -> Self {
        self.el = self.el.border_bottom(color, width);
        self
    }
    pub fn border_left(mut self, color: [f32; 4], width: impl IntoF32) -> Self {
        self.el = self.el.border_left(color, width);
        self
    }
    pub fn border_top_color(mut self, color: [f32; 4]) -> Self {
        self.el = self.el.border_top_color(color);
        self
    }
    pub fn border_right_color(mut self, color: [f32; 4]) -> Self {
        self.el = self.el.border_right_color(color);
        self
    }
    pub fn border_bottom_color(mut self, color: [f32; 4]) -> Self {
        self.el = self.el.border_bottom_color(color);
        self
    }
    pub fn border_left_color(mut self, color: [f32; 4]) -> Self {
        self.el = self.el.border_left_color(color);
        self
    }
    pub fn border_top_style(mut self, style: impl AsRef<str>) -> Self {
        self.el = self.el.border_top_style(style);
        self
    }
    pub fn border_right_style(mut self, style: impl AsRef<str>) -> Self {
        self.el = self.el.border_right_style(style);
        self
    }
    pub fn border_bottom_style(mut self, style: impl AsRef<str>) -> Self {
        self.el = self.el.border_bottom_style(style);
        self
    }
    pub fn border_left_style(mut self, style: impl AsRef<str>) -> Self {
        self.el = self.el.border_left_style(style);
        self
    }
    pub fn border_top_left_radius(mut self, r: f32) -> Self {
        self.el = self.el.border_top_left_radius(r);
        self
    }
    pub fn border_top_right_radius(mut self, r: f32) -> Self {
        self.el = self.el.border_top_right_radius(r);
        self
    }
    pub fn border_bottom_right_radius(mut self, r: f32) -> Self {
        self.el = self.el.border_bottom_right_radius(r);
        self
    }
    pub fn border_bottom_left_radius(mut self, r: f32) -> Self {
        self.el = self.el.border_bottom_left_radius(r);
        self
    }
    pub fn border_shape(mut self, shape: crate::style::CornerShape) -> Self {
        self.el = self.el.border_shape(shape);
        self
    }
    pub fn border_collapse(mut self, collapse: bool) -> Self {
        self.el = self.el.border_collapse(collapse);
        self
    }
    pub fn border_spacing(mut self, h: f32, v: f32) -> Self {
        self.el = self.el.border_spacing(h, v);
        self
    }
    pub fn border_inline(mut self, color: [f32; 4], width: impl IntoF32 + Copy) -> Self {
        self.el = self.el.border_inline(color, width);
        self
    }
    pub fn border_block(mut self, color: [f32; 4], width: impl IntoF32 + Copy) -> Self {
        self.el = self.el.border_block(color, width);
        self
    }
    pub fn border_inline_start(mut self, color: [f32; 4], width: impl IntoF32) -> Self {
        self.el = self.el.border_inline_start(color, width);
        self
    }
    pub fn border_inline_end(mut self, color: [f32; 4], width: impl IntoF32) -> Self {
        self.el = self.el.border_inline_end(color, width);
        self
    }
    pub fn border_block_start(mut self, color: [f32; 4], width: impl IntoF32) -> Self {
        self.el = self.el.border_block_start(color, width);
        self
    }
    pub fn border_block_end(mut self, color: [f32; 4], width: impl IntoF32) -> Self {
        self.el = self.el.border_block_end(color, width);
        self
    }

    pub fn outline(mut self, color: [f32; 4], width: f32) -> Self {
        self.el = self.el.outline(color, width);
        self
    }
    pub fn outline_style(mut self, style: impl AsRef<str>) -> Self {
        self.el = self.el.outline_style(style);
        self
    }
    pub fn outline_color(mut self, color: [f32; 4]) -> Self {
        self.el = self.el.outline_color(color);
        self
    }
    pub fn outline_width(mut self, width: f32) -> Self {
        self.el = self.el.outline_width(width);
        self
    }
    /// Focus-ring thickness in logical px. Overrides the theme default.
    pub fn ring_width(mut self, width: impl crate::core::tree::IntoF32) -> Self {
        self.el = self.el.ring_width(width);
        self
    }
    /// Focus-ring color. Tokens / hex / `[f32; 4]`. Overrides theme token `ring`.
    pub fn ring_color(mut self, color: impl crate::style::IntoPaintColor) -> Self {
        self.el = self.el.ring_color(color);
        self
    }

    pub fn rounded(mut self, radius: impl IntoF32) -> Self {
        self.el = self.el.rounded(radius);
        self
    }

    pub fn rounded_full(mut self) -> Self {
        self.el = self.el.rounded_full();
        self
    }

    pub fn gap(mut self, g: impl IntoCssDim) -> Self {
        self.el = self.el.gap(g);
        self
    }
    pub fn gap_x(mut self, g: impl IntoCssDim) -> Self {
        self.el = self.el.gap_x(g);
        self
    }
    pub fn gap_y(mut self, g: impl IntoCssDim) -> Self {
        self.el = self.el.gap_y(g);
        self
    }
    pub fn column_gap(mut self, g: impl IntoCssDim) -> Self {
        self.el = self.el.column_gap(g);
        self
    }
    pub fn row_gap(mut self, g: impl IntoCssDim) -> Self {
        self.el = self.el.row_gap(g);
        self
    }

    pub fn drag_reorder_handle(mut self, list_id: impl AsRef<str>, index: usize) -> Self {
        self.el = self.el.drag_reorder_handle(list_id, index);
        self
    }

    pub fn items(mut self, align: impl IntoAlignItems) -> Self {
        self.el = self.el.flex_items(align);
        self
    }
    pub fn wrap(mut self, w: impl IntoFlexWrap) -> Self {
        self.el = self.el.flex_wrap_kw(w);
        self
    }

    pub fn justify(mut self, keyword: impl IntoJustifyContent) -> Self {
        self.el = self.el.justify(keyword);
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

    pub fn display_grid(mut self) -> Self {
        self.el = self.el.display_grid();
        self
    }

    pub fn grid_template_columns(mut self, tracks: Vec<GridTrack>) -> Self {
        self.el = self.el.grid_template_columns(tracks);
        self
    }

    pub fn grid_template_rows(mut self, tracks: Vec<GridTrack>) -> Self {
        self.el = self.el.grid_template_rows(tracks);
        self
    }

    /// `grid-cols-{n}`：等分 `fr` 列。
    pub fn grid_cols_equal(mut self, n: u16) -> Self {
        self.el = self.el.grid_cols_equal(n);
        self
    }

    /// `grid-rows-{n}`：等分 `fr` 行。
    pub fn grid_rows_equal(mut self, n: u16) -> Self {
        self.el = self.el.grid_rows_equal(n);
        self
    }

    pub fn grid_column_span(mut self, n: u16) -> Self {
        self.el = self.el.grid_column_span(n);
        self
    }

    pub fn grid_row_span(mut self, n: u16) -> Self {
        self.el = self.el.grid_row_span(n);
        self
    }

    pub fn align_self_kw(mut self, kw: impl Into<String>) -> Self {
        let s = kw.into();
        self.el = self.el.align_self_kw(&s);
        self
    }

    pub fn align_content_kw(mut self, keyword: impl AsRef<str>) -> Self {
        self.el = self.el.align_content_kw(keyword);
        self
    }

    pub fn justify_items_kw(mut self, keyword: impl AsRef<str>) -> Self {
        self.el = self.el.justify_items_kw(keyword);
        self
    }

    pub fn justify_self_kw(mut self, keyword: impl AsRef<str>) -> Self {
        self.el = self.el.justify_self_kw(keyword);
        self
    }

    pub fn flex_basis(mut self, basis: impl IntoCssDim) -> Self {
        self.el = self.el.flex_basis(basis);
        self
    }

    pub fn flex_grow(mut self, grow: impl crate::core::tree::IntoF32Raw) -> Self {
        self.el = self.el.flex_grow(grow);
        self
    }

    pub fn flex_shrink(mut self, shrink: impl crate::core::tree::IntoF32Raw) -> Self {
        self.el = self.el.flex_shrink(shrink);
        self
    }

    pub fn justify_center(mut self) -> Self {
        self.el = self.el.justify_center();
        self
    }

    pub fn items_center(mut self) -> Self {
        self.el = self.el.items_center();
        self
    }

    pub fn absolute(mut self, abs: bool) -> Self {
        if abs {
            self.el = self.el.absolute();
        }
        self
    }

    pub fn relative(mut self, rel: bool) -> Self {
        if rel {
            self.el = self.el.relative();
        }
        self
    }

    /// CSS `aspect-ratio`（数字 / `"16/9"` / `"auto"`…）。
    pub fn aspect_ratio(mut self, ratio: impl crate::core::tree::IntoAspectRatioValue) -> Self {
        self.el = self.el.aspect_ratio(ratio);
        self
    }

    pub fn transition(mut self, value: impl AsRef<str>) -> Self {
        self.el = self.el.transition(value);
        self
    }
    pub fn transition_property(mut self, value: impl AsRef<str>) -> Self {
        self.el = self.el.transition_property(value);
        self
    }
    pub fn transition_duration(mut self, value: impl AsRef<str>) -> Self {
        self.el = self.el.transition_duration(value);
        self
    }
    pub fn transition_delay(mut self, value: impl AsRef<str>) -> Self {
        self.el = self.el.transition_delay(value);
        self
    }
    pub fn transition_timing_function(mut self, value: impl AsRef<str>) -> Self {
        self.el = self.el.transition_timing_function(value);
        self
    }
    pub fn animation(mut self, value: impl AsRef<str>) -> Self {
        self.el = self.el.animation(value);
        self
    }
    pub fn clip_path(mut self, value: impl AsRef<str>) -> Self {
        self.el = self.el.clip_path(value);
        self
    }
    pub fn clip_path_name(mut self, value: impl AsRef<str>) -> Self {
        self.el = self.el.clip_path_name(value);
        self
    }
    pub fn offset_path(mut self, value: impl AsRef<str>) -> Self {
        self.el = self.el.offset_path(value);
        self
    }
    pub fn offset_distance(mut self, value: impl AsRef<str>) -> Self {
        self.el = self.el.offset_distance(value);
        self
    }
    pub fn offset_rotate(mut self, value: impl AsRef<str>) -> Self {
        self.el = self.el.offset_rotate(value);
        self
    }
    pub fn animate(mut self, anim: crate::style::Animation) -> Self {
        self.el = self.el.animate(anim);
        self
    }
    pub fn animation_name(mut self, value: impl AsRef<str>) -> Self {
        self.el = self.el.animation_name(value);
        self
    }
    pub fn animation_duration(mut self, value: impl AsRef<str>) -> Self {
        self.el = self.el.animation_duration(value);
        self
    }
    pub fn animation_delay(mut self, value: impl AsRef<str>) -> Self {
        self.el = self.el.animation_delay(value);
        self
    }
    pub fn animation_timing_function(mut self, value: impl AsRef<str>) -> Self {
        self.el = self.el.animation_timing_function(value);
        self
    }
    pub fn animation_iteration_count(mut self, value: impl AsRef<str>) -> Self {
        self.el = self.el.animation_iteration_count(value);
        self
    }
    pub fn animation_direction(mut self, value: impl AsRef<str>) -> Self {
        self.el = self.el.animation_direction(value);
        self
    }
    pub fn animation_fill_mode(mut self, value: impl AsRef<str>) -> Self {
        self.el = self.el.animation_fill_mode(value);
        self
    }
    pub fn animation_play_state(mut self, value: impl AsRef<str>) -> Self {
        self.el = self.el.animation_play_state(value);
        self
    }
    pub fn animation_composition(mut self, value: impl AsRef<str>) -> Self {
        self.el = self.el.animation_composition(value);
        self
    }
    pub fn animation_timeline(mut self, value: impl AsRef<str>) -> Self {
        self.el = self.el.animation_timeline(value);
        self
    }
    pub fn animation_range(mut self, value: impl AsRef<str>) -> Self {
        self.el = self.el.animation_range(value);
        self
    }
    pub fn animation_range_start(mut self, value: impl AsRef<str>) -> Self {
        self.el = self.el.animation_range_start(value);
        self
    }
    pub fn animation_range_end(mut self, value: impl AsRef<str>) -> Self {
        self.el = self.el.animation_range_end(value);
        self
    }

    pub fn pointer_events(mut self, mode: impl Into<crate::core::tree::PointerEvents>) -> Self {
        self.el = self.el.pointer_events(mode);
        self
    }

    pub fn cursor(mut self, value: impl IntoCursor) -> Self {
        self.el = self.el.cursor(value);
        self
    }

    pub fn top(mut self, val: impl IntoCssDim) -> Self {
        self.el = self.el.top(val);
        self
    }

    pub fn right(mut self, val: impl IntoCssDim) -> Self {
        self.el = self.el.right(val);
        self
    }

    pub fn bottom(mut self, val: impl IntoCssDim) -> Self {
        self.el = self.el.bottom(val);
        self
    }

    pub fn left(mut self, val: impl IntoCssDim) -> Self {
        self.el = self.el.left(val);
        self
    }
    pub fn inset_inline_start(mut self, val: impl IntoCssDim) -> Self {
        self.el = self.el.inset_inline_start(val);
        self
    }
    pub fn inset_inline_end(mut self, val: impl IntoCssDim) -> Self {
        self.el = self.el.inset_inline_end(val);
        self
    }

    pub fn inset(mut self, val: impl IntoCssDim) -> Self {
        self.el = self.el.inset(val);
        self
    }
    pub fn inset_inline(mut self, val: impl IntoCssRectPair) -> Self {
        self.el = self.el.inset_inline(val);
        self
    }
    pub fn inset_block(mut self, val: impl IntoCssRectPair) -> Self {
        self.el = self.el.inset_block(val);
        self
    }

    pub fn z_index(mut self, z: i32) -> Self {
        self.el = self.el.z_index(z);
        self
    }

    pub fn shadow(mut self, color: [f32; 4], blur: f32) -> Self {
        self.el = self.el.shadow(color, blur);
        self
    }
    pub fn popup_shadow(mut self) -> Self {
        self.el = self.el.popup_shadow();
        self
    }
    pub fn box_shadow(mut self, spec: impl AsRef<str>) -> Self {
        self.el = self.el.box_shadow(spec);
        self
    }
    pub fn box_shadow_color(mut self, color: [f32; 4]) -> Self {
        self.el = self.el.box_shadow_color(color);
        self
    }
    pub fn box_shadow_blur(mut self, blur: f32) -> Self {
        self.el = self.el.box_shadow_blur(blur);
        self
    }

    pub fn backdrop_blur(mut self, blur: impl crate::core::tree::IntoF32) -> Self {
        self.el = self.el.backdrop_blur(blur);
        self
    }
    pub fn filter(mut self, spec: impl AsRef<str>) -> Self {
        self.el = self.el.filter(spec);
        self
    }
    pub fn box_shadow_offset_x(mut self, dx: f32) -> Self {
        self.el = self.el.box_shadow_offset_x(dx);
        self
    }
    pub fn box_shadow_offset_y(mut self, dy: f32) -> Self {
        self.el = self.el.box_shadow_offset_y(dy);
        self
    }
    pub fn box_shadow_spread(mut self, spread: f32) -> Self {
        self.el = self.el.box_shadow_spread(spread);
        self
    }
    pub fn box_shadow_inset(mut self, inset: bool) -> Self {
        self.el = self.el.box_shadow_inset(inset);
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

    /// Utility class string, [`crate::core::reactivity::Signal`], or `|| String` compute.
    /// `class(sig.get())` is still a snapshot — pass `sig` or `|| sig.get()`.
    #[track_caller]
    pub fn class(mut self, classes: impl IntoNodeClass) -> Self {
        self.el = classes.apply_node_class(self.el);
        self
    }

    #[track_caller]
    pub fn text(mut self, text: impl crate::core::tree::IntoNodeText) -> Self {
        self.el = self.el.text(text);
        self
    }

    pub fn text_size(mut self, size: impl IntoF32) -> Self {
        self.el = self.el.text_size(size.into_f32());
        self
    }

    /// Static color, [`crate::core::reactivity::Signal`], or `|| color` compute.
    /// `text_color(sig.get())` is still a snapshot — pass `sig` or `|| sig.get()`.
    #[track_caller]
    pub fn text_color(mut self, color: impl IntoNodeFg) -> Self {
        self.el = color.apply_node_fg(self.el);
        self
    }

    pub fn font_weight(mut self, weight: impl crate::core::tree::IntoF32Raw) -> Self {
        self.el = self.el.font_weight(weight);
        self
    }

    pub fn font_family(mut self, family: impl Into<String>) -> Self {
        self.el = self.el.font_family(family);
        self
    }

    pub fn underline(mut self, underline: bool) -> Self {
        self.el = self.el.underline(underline);
        self
    }

    pub fn text_decoration(mut self, decoration: impl Into<String>) -> Self {
        self.el = self.el.text_decoration(decoration.into());
        self
    }

    pub fn text_decoration_line(mut self, line: impl Into<String>) -> Self {
        self.el = self.el.text_decoration_line(line.into());
        self
    }

    pub fn text_decoration_style(mut self, style: impl Into<String>) -> Self {
        self.el = self.el.text_decoration_style(style.into());
        self
    }

    pub fn text_decoration_color(mut self, color: [u8; 4]) -> Self {
        self.el = self.el.text_decoration_color(color);
        self
    }

    pub fn text_decoration_thickness(mut self, thickness: impl IntoF32) -> Self {
        self.el = self.el.text_decoration_thickness(thickness);
        self
    }

    pub fn text_underline_offset(mut self, offset: impl IntoF32) -> Self {
        self.el = self.el.text_underline_offset(offset);
        self
    }

    pub fn text_line_height(mut self, line_height: impl IntoF32) -> Self {
        self.el = self.el.text_line_height(line_height);
        self
    }

    pub fn text_leading(mut self, scale: impl crate::core::tree::IntoF32Raw) -> Self {
        self.el = self.el.text_leading(scale);
        self
    }

    pub fn text_leading_none(mut self) -> Self {
        self.el = self.el.text_leading_none();
        self
    }

    pub fn text_leading_tight(mut self) -> Self {
        self.el = self.el.text_leading_tight();
        self
    }

    pub fn text_leading_normal(mut self) -> Self {
        self.el = self.el.text_leading_normal();
        self
    }

    pub fn text_leading_relaxed(mut self) -> Self {
        self.el = self.el.text_leading_relaxed();
        self
    }

    pub fn text_align(mut self, align: impl Into<crate::style::TextAlignDeclaration>) -> Self {
        self.el = self.el.text_align(align);
        self
    }

    pub fn text_align_last(
        mut self,
        align: impl Into<crate::style::TextAlignLastDeclaration>,
    ) -> Self {
        self.el = self.el.text_align_last(align);
        self
    }
    pub fn text_center(self) -> Self {
        self.text_align("center")
    }

    pub fn text_shadow(mut self, spec: impl AsRef<str>) -> Self {
        self.el = self.el.text_shadow(spec);
        self
    }
    pub fn text_shadow_color(mut self, color: [u8; 4]) -> Self {
        self.el = self.el.text_shadow_color(color);
        self
    }
    pub fn text_shadow_blur(mut self, blur: impl IntoF32) -> Self {
        self.el = self.el.text_shadow_blur(blur);
        self
    }
    pub fn text_shadow_offset_x(mut self, dx: impl IntoF32) -> Self {
        self.el = self.el.text_shadow_offset_x(dx);
        self
    }
    pub fn text_shadow_offset_y(mut self, dy: impl IntoF32) -> Self {
        self.el = self.el.text_shadow_offset_y(dy);
        self
    }

    pub fn overflow(mut self, mode: impl IntoOverflow) -> Self {
        self.el = self.el.overflow(mode);
        self
    }
    pub fn overflow_x(mut self, mode: impl IntoOverflow) -> Self {
        self.el = self.el.overflow_x(mode);
        self
    }
    pub fn overflow_y(mut self, mode: impl IntoOverflow) -> Self {
        self.el = self.el.overflow_y(mode);
        self
    }

    /// CSS [`writing-mode`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/writing-mode).
    pub fn writing_mode(mut self, mode: impl AsRef<str>) -> Self {
        self.el = self.el.writing_mode(mode);
        self
    }
}

impl From<BoxBuilder> for Element {
    fn from(builder: BoxBuilder) -> Self {
        maybe_activate_scroll_viewport(
            builder.el,
            builder.scroll_id.as_deref(),
            &builder.scroll_behavior,
            &builder.hover_mode,
        )
    }
}

impl crate::core::tree::IntoElementList for BoxBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

pub fn r#box() -> BoxBuilder {
    BoxBuilder::new()
}

/// Rust-friendly alias for [`r#box`]. RSX uses `<div>` / `<Div>`.
#[inline]
pub fn div() -> BoxBuilder {
    r#box()
}

/// Callable alias of [`r#box`] (Rust reserves `box`; RSX uses `<Box>` / `<div>`).
#[inline]
pub fn boxed() -> BoxBuilder {
    r#box()
}

pub fn hstack() -> BoxBuilder {
    BoxBuilder::new().flex_row()
}

pub fn vstack() -> BoxBuilder {
    // 与 CSS column flex 默认一致：交叉轴 stretch，子项（含文本）易获得父级宽度以便换行。
    BoxBuilder::new().flex_col().items("stretch")
}

pub fn gstack() -> BoxBuilder {
    BoxBuilder::new().display_grid()
}
