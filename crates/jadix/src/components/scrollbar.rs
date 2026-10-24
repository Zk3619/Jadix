//! # Scrollbar
//!
//! 可挂到某滚动视口（`target` = viewport rest id，如 滚动视口的 `id`）的独立滚动条。
//! **多数场景不必单独用**——`ScrollArea` 已内建横/纵条；本组件用于自定义摆放或拆分布局。
//!
//! ## Usage
//! ```ignore
//! <Scrollbar target="sa-main" orientation="vertical" visibility="auto" thickness="10" />
//! // Element::from(scrollbar("sb").target("sa-main").orientation(Vertical).variant("capsule"))
//! ```
//!
//! ## Notes
//! - `target` 必须与对应视口的 `id` 一致。
//! - `visibility`: auto | always | never（never 仍可能可滚内容）。
//! - `children` 当前忽略。
//! - 主题色：`scrollbar-track` / `scrollbar-thumb` / `scrollbar-track-border` / `scrollbar-thumb-border`。
//! - `auto` 显隐：仅拇指（及有色的边框）参与淡入；透明轨道不参与。

use crate::core::scrollbar::{
    configure_scrollbar_parts, ATTR_SCROLLBAR_TRACK_FADES, CHROME_ALPHA_EPS,
};
pub use crate::core::scrollbar::{
    scrollbar_visibility_signal_key, SCROLLBAR_DEFAULT_CROSS_PX, SCROLLBAR_THUMB_FOCUS_PREFIX,
    SCROLLBAR_TRACK_FOCUS_PREFIX,
};
use crate::core::tree::{parse_color, Element, IntoDimension, IntoF32};
use jadix_macros::component;
use taffy::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollbarOrientation {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ScrollbarVariant {
    #[default]
    Capsule,
    Rect,
}

impl ScrollbarVariant {
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "rect" | "rectangle" => Self::Rect,
            _ => Self::Capsule,
        }
    }
}

impl From<&str> for ScrollbarVariant {
    fn from(s: &str) -> Self {
        Self::parse(s)
    }
}

impl From<String> for ScrollbarVariant {
    fn from(s: String) -> Self {
        Self::parse(&s)
    }
}

impl From<&String> for ScrollbarVariant {
    fn from(s: &String) -> Self {
        Self::parse(s)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ScrollbarVisibility {
    #[default]
    Auto,
    Always,
    Never,
}

impl ScrollbarVisibility {
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "always" => Self::Always,
            "never" => Self::Never,
            _ => Self::Auto,
        }
    }
}

impl From<&str> for ScrollbarVisibility {
    fn from(s: &str) -> Self {
        Self::parse(s)
    }
}

impl From<String> for ScrollbarVisibility {
    fn from(s: String) -> Self {
        Self::parse(&s)
    }
}

impl From<&String> for ScrollbarVisibility {
    fn from(s: &String) -> Self {
        Self::parse(s)
    }
}

#[inline]
fn scrollbar_chrome_has_alpha(color: [f32; 4]) -> bool {
    color[3] > CHROME_ALPHA_EPS
}

#[inline]
fn scrollbar_track_fades(track_bg: [f32; 4], track_border: [f32; 4]) -> bool {
    scrollbar_chrome_has_alpha(track_bg) || scrollbar_chrome_has_alpha(track_border)
}

#[inline]
fn scrollbar_border_width(color: [f32; 4], explicit: Option<f32>) -> f32 {
    match explicit {
        Some(w) => w.max(0.0),
        None => {
            if scrollbar_chrome_has_alpha(color) {
                1.0
            } else {
                0.0
            }
        }
    }
}

pub struct ScrollbarBuilder {
    _id: String,
    orientation: ScrollbarOrientation,
    target_id: Option<String>,
    variant: ScrollbarVariant,
    visibility: ScrollbarVisibility,
    thickness: f32,
    track_color: [f32; 4],
    thumb_color: [f32; 4],
    track_border_color: [f32; 4],
    thumb_border_color: [f32; 4],
    track_border_width: Option<f32>,
    thumb_border_width: Option<f32>,
    flex_grow: f32,
    flex_shrink: f32,
    position: Position,
    inset: Rect<LengthPercentageAuto>,
    z_index: i32,
    /// When true, track is absolute-positioned and does not consume flex cross size.
    overlay: bool,
    inline_end: Option<f32>,
}

pub(crate) fn default_scrollbar_cross_px() -> f32 {
    if crate::style::theme_preset() == crate::style::ThemePreset::Fluent {
        8.0
    } else {
        SCROLLBAR_DEFAULT_CROSS_PX
    }
}

pub fn scrollbar(id: &str) -> ScrollbarBuilder {
    ScrollbarBuilder {
        _id: id.to_string(),
        orientation: ScrollbarOrientation::Vertical,
        target_id: None,
        variant: ScrollbarVariant::Capsule,
        visibility: ScrollbarVisibility::Auto,
        thickness: default_scrollbar_cross_px(),
        track_color: parse_color("scrollbar-track"),
        thumb_color: parse_color("scrollbar-thumb"),
        track_border_color: parse_color("scrollbar-track-border"),
        thumb_border_color: parse_color("scrollbar-thumb-border"),
        track_border_width: None,
        thumb_border_width: None,
        flex_grow: 0.0,
        flex_shrink: 0.0, // Scrollbars usually shouldn't shrink by default if they have a fixed cross size.
        position: Position::Relative,
        inset: Rect::auto(),
        z_index: 0,
        overlay: false,
        inline_end: None,
    }
}

// Alias for macro lowercase mapping: <Scrollbar> -> components::scrollbar(...)
pub fn scrollbarbuilder() -> ScrollbarBuilder {
    scrollbar("scrollbar")
}

/// Scrollbar component for fine-grained scroll control.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Scrollbar(
    target: String,
    orientation: String,
    visibility: Option<String>,
    thickness: Option<f32>,
    children: Option<Element>,
) -> Element {
    let mut builder = scrollbar("scrollbar").target(target);

    if orientation.eq_ignore_ascii_case("horizontal") {
        builder = builder.orientation(ScrollbarOrientation::Horizontal);
    } else {
        builder = builder.orientation(ScrollbarOrientation::Vertical);
    }

    if let Some(v) = visibility {
        builder = builder.visibility(v);
    }
    if let Some(t) = thickness {
        builder = builder.thickness(t);
    }

    if let Some(c) = children {
        builder = builder.child(c);
    }

    builder.into()
}

impl ScrollbarBuilder {
    pub fn orientation(mut self, orientation: ScrollbarOrientation) -> Self {
        self.orientation = orientation;
        self
    }

    pub fn target(mut self, id: impl Into<String>) -> Self {
        self.target_id = Some(id.into());
        self
    }

    pub fn variant(mut self, variant: impl Into<ScrollbarVariant>) -> Self {
        self.variant = variant.into();
        self
    }

    pub fn capsule(mut self) -> Self {
        self.variant = ScrollbarVariant::Capsule;
        self
    }

    pub fn rect(mut self) -> Self {
        self.variant = ScrollbarVariant::Rect;
        self
    }

    pub fn visibility(mut self, mode: impl Into<ScrollbarVisibility>) -> Self {
        self.visibility = mode.into();
        self
    }

    pub fn auto(mut self) -> Self {
        self.visibility = ScrollbarVisibility::Auto;
        self
    }

    pub fn always(mut self) -> Self {
        self.visibility = ScrollbarVisibility::Always;
        self
    }

    pub fn never(mut self) -> Self {
        self.visibility = ScrollbarVisibility::Never;
        self
    }

    pub fn thickness(mut self, thickness: impl IntoF32) -> Self {
        self.thickness = thickness.into_f32().max(2.0);
        self
    }

    pub fn track_color(mut self, color: [f32; 4]) -> Self {
        self.track_color = color;
        self
    }

    pub fn thumb_color(mut self, color: [f32; 4]) -> Self {
        self.thumb_color = color;
        self
    }

    pub fn track_border_color(mut self, color: [f32; 4]) -> Self {
        self.track_border_color = color;
        self
    }

    pub fn thumb_border_color(mut self, color: [f32; 4]) -> Self {
        self.thumb_border_color = color;
        self
    }

    pub fn track_border_width(mut self, width: impl IntoF32) -> Self {
        self.track_border_width = Some(width.into_f32().max(0.0));
        self
    }

    pub fn thumb_border_width(mut self, width: impl IntoF32) -> Self {
        self.thumb_border_width = Some(width.into_f32().max(0.0));
        self
    }

    pub fn flex_grow(mut self, grow: impl crate::core::tree::IntoF32Raw) -> Self {
        self.flex_grow = grow.into_f32_raw();
        self
    }

    pub fn flex_shrink(mut self, shrink: impl crate::core::tree::IntoF32Raw) -> Self {
        self.flex_shrink = shrink.into_f32_raw();
        self
    }

    pub fn child(self, _child: impl Into<Element>) -> Self {
        // Scrollbar currently doesn't support children
        self
    }
    pub fn absolute(mut self) -> Self {
        self.position = Position::Absolute;
        self
    }
    pub fn relative(mut self) -> Self {
        self.position = Position::Relative;
        self
    }
    pub fn top(mut self, val: impl IntoDimension) -> Self {
        self.inset.top = Element::into_margin_auto(val);
        self
    }
    pub fn right(mut self, val: impl IntoDimension) -> Self {
        self.inset.right = Element::into_margin_auto(val);
        self
    }
    pub fn bottom(mut self, val: impl IntoDimension) -> Self {
        self.inset.bottom = Element::into_margin_auto(val);
        self
    }
    pub fn left(mut self, val: impl IntoDimension) -> Self {
        self.inset.left = Element::into_margin_auto(val);
        self
    }
    pub fn inset_inline_end(mut self, val: impl IntoF32) -> Self {
        self.inline_end = Some(val.into_f32());
        self
    }
    pub fn inset(mut self, val: impl IntoDimension) -> Self {
        let dim = Element::into_margin_auto(val);
        self.inset = Rect {
            left: dim,
            right: dim,
            top: dim,
            bottom: dim,
        };
        self
    }
    pub fn z_index(mut self, z: i32) -> Self {
        self.z_index = z;
        self
    }

    /// Overlay mode: absolute track that does not consume sibling flex width/height.
    ///
    /// Also sets attr `jadix_scrollbar_overlay=1` on the built element for runtime detection.
    pub fn overlay(mut self, enabled: bool) -> Self {
        self.overlay = enabled;
        if enabled {
            self.position = Position::Absolute;
            self.flex_grow = 0.0;
            self.flex_shrink = 0.0;
            if self.z_index == 0 {
                self.z_index = 10;
            }
        }
        self
    }
}

impl From<ScrollbarBuilder> for Element {
    fn from(builder: ScrollbarBuilder) -> Element {
        if builder.visibility == ScrollbarVisibility::Never {
            return Element::new().w(0.0).h(0.0);
        }

        // IMPORTANT:
        // Do not short-circuit `auto && !can_scroll` at build time.
        // First frame metrics are often still zero; collapsing here makes the track node disappear,
        // then later hover/scroll updates cannot drive visibility/geometry until an unrelated rebuild.
        // Runtime path (`update_scrollbar_elements`) owns can_scroll gating.

        // Persist visibility mode for runtime updates (avoid rebuild dependency).
        let axis = match builder.orientation {
            ScrollbarOrientation::Horizontal => "x",
            ScrollbarOrientation::Vertical => "y",
        };
        // Auto + can_scroll: keep a real track in the tree even when v≈0 so hit-testing and
        // `update_scrollbar_elements` can attach a hit-slab each frame (see `state.rs`).
        let initial_alpha = match builder.visibility {
            ScrollbarVisibility::Always => 1.0_f32,
            ScrollbarVisibility::Auto | ScrollbarVisibility::Never => 0.0_f32,
        };
        let track_fades = scrollbar_track_fades(builder.track_color, builder.track_border_color);
        let track_border_w =
            scrollbar_border_width(builder.track_border_color, builder.track_border_width);
        let thumb_border_w =
            scrollbar_border_width(builder.thumb_border_color, builder.thumb_border_width);
        let mut track_color = builder.track_color;
        if track_fades {
            track_color[3] *= initial_alpha;
        }
        let mut thumb_color = builder.thumb_color;
        thumb_color[3] *= initial_alpha;
        let mut track_border_color = builder.track_border_color;
        if track_fades {
            track_border_color[3] *= initial_alpha;
        }
        let mut thumb_border_color = builder.thumb_border_color;
        if scrollbar_chrome_has_alpha(thumb_border_color) {
            thumb_border_color[3] *= initial_alpha;
        }

        let track_cross = builder.thickness.max(2.0);
        let thumb_cross = (track_cross - 2.0).max(1.0);
        let (track_radius, thumb_radius) = match builder.variant {
            // Capsule radius derives from the actual cross size.
            ScrollbarVariant::Capsule => (track_cross * 0.5, thumb_cross * 0.5),
            ScrollbarVariant::Rect => (0.0_f32, 0.0_f32),
        };

        let mut thumb = Element::new()
            // The track owns its size. Moving its thumb must not invalidate
            // intrinsic measurements of the surrounding scrolling content.
            .absolute()
            .rounded(thumb_radius)
            .bg(thumb_color)
            .border(thumb_border_color, thumb_border_w)
            .cursor_kind(crate::core::tree::CursorKind::Pointer);

        let mut track = Element::new()
            .flex_grow(builder.flex_grow)
            .flex_shrink(builder.flex_shrink)
            .rounded(track_radius)
            .bg(track_color)
            .border(track_border_color, track_border_w);
        let visibility = match builder.visibility {
            ScrollbarVisibility::Always => "always",
            ScrollbarVisibility::Never => "never",
            ScrollbarVisibility::Auto => "auto",
        };
        configure_scrollbar_parts(
            &mut track,
            &mut thumb,
            builder.target_id.as_deref(),
            axis,
            visibility,
            track_cross,
        );

        let track = match builder.position {
            Position::Absolute => track.absolute(),
            Position::Relative | Position::Sticky => track.relative(),
            Position::Static => track.position("static"),
            Position::Fixed => track.fixed(),
        };

        let mut track = track
            .inset_rect(builder.inset)
            .z_index(builder.z_index)
            .child(thumb);
        if builder.overlay {
            track.set_attr("jadix_scrollbar_overlay", "1");
            match builder.orientation {
                ScrollbarOrientation::Vertical => {
                    track = track.absolute().inset_inline_end(0.0).top(0.0).bottom(0.0);
                    track.style.size.height = taffy::style::Dimension::auto();
                    track.style.min_size.height = taffy::style::Dimension::length(0.0);
                }
                ScrollbarOrientation::Horizontal => {
                    track = track.absolute().left(0.0).right(0.0).bottom(0.0);
                    track.style.size.width = taffy::style::Dimension::auto();
                    track.style.min_size.width = taffy::style::Dimension::length(0.0);
                }
            }
            // Explicit caller insets override overlay defaults.
            if !builder.inset.left.is_auto() {
                track.style.inset.left = builder.inset.left;
            }
            if !builder.inset.right.is_auto() {
                track.style.inset.right = builder.inset.right;
            }
            if !builder.inset.top.is_auto() {
                track.style.inset.top = builder.inset.top;
            }
            if !builder.inset.bottom.is_auto() {
                track.style.inset.bottom = builder.inset.bottom;
            }
        }
        if let Some(end) = builder.inline_end {
            track = track.inset_inline_end(end);
        }
        track.set_attr(
            ATTR_SCROLLBAR_TRACK_FADES,
            if track_fades { "1" } else { "0" },
        );
        track
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transparent_border_has_zero_width_by_default() {
        assert_eq!(scrollbar_border_width([0.0; 4], None), 0.0);
        assert_eq!(scrollbar_border_width([1.0, 1.0, 1.0, 1.0], None), 1.0);
    }
}
