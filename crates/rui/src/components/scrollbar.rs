//! # Scrollbar
//!
//! 可挂到某滚动视口（`target` = viewport rest id，如 [`ScrollArea`](crate::components::ScrollArea) / [`ScrollViewport`](crate::components::ScrollViewport) 的 `id`）的独立滚动条。
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

use crate::core::reactivity::use_signal;
use crate::core::scroll_viewport::{
    scroll_content_h_signal_key, scroll_content_w_signal_key, scroll_offset_x_signal_key,
    scroll_offset_y_signal_key, scroll_viewport_h_signal_key, scroll_viewport_w_signal_key,
};
use crate::core::tree::{Element, IntoDimension, IntoF32, parse_color};
use rui_macros::component;
use taffy::prelude::*;

// Overlay MVP: `overlay(true)` / `rui_scrollbar_overlay=1` sets absolute + zero flex grow/shrink
// so the track paints over the viewport edge instead of consuming cross-axis flex space.
// Deeper ScrollArea-builtin overlay + inset sync remains future work.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollbarOrientation {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollbarVariant {
    Capsule,
    Rect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollbarVisibility {
    Auto,
    Always,
    Never,
}

pub fn scrollbar_visibility_signal_key(target_id: &str, axis: &str) -> String {
    format!("__rui_scrollbar_visibility_{}:{}", target_id, axis)
}

pub const SCROLLBAR_THUMB_FOCUS_PREFIX: &str = "scrollbar-thumb:";
pub const SCROLLBAR_TRACK_FOCUS_PREFIX: &str = "scrollbar-track:";
/// When `"0"`, `auto` fade skips track chrome (overlay thumb-only).
pub(crate) const ATTR_SCROLLBAR_TRACK_FADES: &str = "rui_scrollbar_track_fades";

const CHROME_ALPHA_EPS: f32 = 0.01;

#[inline]
pub(crate) fn scrollbar_chrome_has_alpha(color: [f32; 4]) -> bool {
    color[3] > CHROME_ALPHA_EPS
}

#[inline]
pub(crate) fn scrollbar_track_fades(track_bg: [f32; 4], track_border: [f32; 4]) -> bool {
    scrollbar_chrome_has_alpha(track_bg) || scrollbar_chrome_has_alpha(track_border)
}

#[inline]
pub(crate) fn scrollbar_border_width(color: [f32; 4], explicit: Option<f32>) -> f32 {
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

/// Restore base alpha after build-time `auto` zeroing; skip intentionally transparent track chrome.
#[inline]
pub(crate) fn normalize_scrollbar_base_for_auto(
    vis: &str,
    track_fades: bool,
    track_border_width: f32,
    thumb_border_width: f32,
    track_bg: &mut [f32; 4],
    track_border: &mut [f32; 4],
    thumb_bg: &mut [f32; 4],
    thumb_border: &mut [f32; 4],
) {
    if vis != "auto" {
        return;
    }
    if track_fades && track_bg[3] <= CHROME_ALPHA_EPS {
        track_bg[3] = 1.0;
    }
    if thumb_bg[3] <= CHROME_ALPHA_EPS {
        thumb_bg[3] = 1.0;
    }
    if track_fades && track_border_width > 0.0 && track_border[3] <= CHROME_ALPHA_EPS {
        track_border[3] = 1.0;
    }
    if thumb_border_width > 0.0 && thumb_border[3] <= CHROME_ALPHA_EPS {
        thumb_border[3] = 1.0;
    }
}

/// Apply `auto` / `always` visibility alpha; track and thumb fade independently.
#[inline]
pub(crate) fn apply_scrollbar_visible_alpha(
    visible_alpha: f32,
    track_fades: bool,
    thumb_border_fades: bool,
    track_bg: &mut [f32; 4],
    thumb_bg: &mut [f32; 4],
    track_border: &mut [f32; 4],
    thumb_border: &mut [f32; 4],
) {
    let v = visible_alpha.clamp(0.0, 1.0);
    if track_fades {
        track_bg[3] *= v;
        track_border[3] *= v;
    }
    thumb_bg[3] *= v;
    if thumb_border_fades {
        thumb_border[3] *= v;
    }
}

/// 沿轨道方向的最小滑块长度。
///
/// 旧实现用 `track_inner.min(24)`：当嵌套在小盒子里 `track_inner < 24` 时，`min_thumb == track_inner`，
/// `clamp` 后滑块恒为满轨，比例严重失真。
#[inline]
pub(crate) fn scrollbar_min_thumb_along_track(track_inner: f32) -> f32 {
    let t = track_inner.max(1.0);
    let cap = (t * 0.18).min(20.0);
    cap.max(3.0).min(t * 0.9)
}

/// Track border-box → content-box origin (where Taffy lays out the thumb).
///
/// The track uses a 1px layout border and **no padding**: `thumb_cross = thickness - 2`
/// then sits in the remaining 8px inside a 10px BorderBox. Adding padding on top of the
/// border double-counts and shifts the thumb ~1px down-right.
#[inline]
pub(crate) fn scrollbar_track_content_inset_px(
    padding: f32,
    border_width: f32,
    has_border: bool,
) -> f32 {
    let pad = padding.max(0.0);
    let border = if has_border {
        border_width.max(0.0)
    } else {
        0.0
    };
    pad + border
}

#[inline]
pub(crate) fn scrollbar_track_content_inset(track: &Element) -> f32 {
    let pad = crate::layout::lp_length_px(track.style.padding.left)
        .filter(|px| px.is_finite())
        .unwrap_or(0.0)
        .max(0.0);
    let has_border = !track.style.border_style.is_invisible() && track.style.border_width > 0.0;
    scrollbar_track_content_inset_px(pad, track.style.border_width, has_border)
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
}

pub fn scrollbar(id: &str) -> ScrollbarBuilder {
    ScrollbarBuilder {
        _id: id.to_string(),
        orientation: ScrollbarOrientation::Vertical,
        target_id: None,
        variant: ScrollbarVariant::Capsule,
        visibility: ScrollbarVisibility::Auto,
        thickness: 10.0,
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

    pub fn variant(mut self, variant: impl AsRef<str>) -> Self {
        self.variant = match variant.as_ref().to_ascii_lowercase().as_str() {
            "rect" | "rectangle" => ScrollbarVariant::Rect,
            _ => ScrollbarVariant::Capsule,
        };
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

    pub fn visibility(mut self, mode: impl AsRef<str>) -> Self {
        self.visibility = match mode.as_ref().to_ascii_lowercase().as_str() {
            "always" => ScrollbarVisibility::Always,
            "never" => ScrollbarVisibility::Never,
            _ => ScrollbarVisibility::Auto,
        };
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
    /// Also sets attr `rui_scrollbar_overlay=1` on the built element for runtime detection.
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
        let (vw, vh, cw, ch, sx, sy) = if let Some(id) = builder.target_id.as_ref() {
            let vw = use_signal(&scroll_viewport_w_signal_key(id), || 0.0_f32).peek();
            let vh = use_signal(&scroll_viewport_h_signal_key(id), || 0.0_f32).peek();
            let cw = use_signal(&scroll_content_w_signal_key(id), || 0.0_f32).peek();
            let ch = use_signal(&scroll_content_h_signal_key(id), || 0.0_f32).peek();
            let sx = use_signal(&scroll_offset_x_signal_key(id), || 0.0_f32).peek();
            let sy = use_signal(&scroll_offset_y_signal_key(id), || 0.0_f32).peek();
            (vw, vh, cw, ch, sx, sy)
        } else {
            (0.0, 0.0, 0.0, 0.0, 0.0, 0.0)
        };

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
        if let Some(id) = builder.target_id.as_ref() {
            let vk = scrollbar_visibility_signal_key(id, axis);
            let raw = match builder.visibility {
                ScrollbarVisibility::Always => "always",
                ScrollbarVisibility::Never => "never",
                ScrollbarVisibility::Auto => "auto",
            };
            use_signal(&vk, || raw.to_string()).set(raw.to_string());
        }

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

        let inset = 1.0_f32;
        let base_cross = builder.thickness.max(2.0);
        // Keep constant cross size; visibility is alpha-only.
        let track_cross = base_cross;
        let thumb_cross = (track_cross - inset * 2.0).max(1.0);
        let (track_w_px, track_h_px, thumb_w, thumb_h, thumb_ml, thumb_mt) =
            match builder.orientation {
                ScrollbarOrientation::Horizontal => {
                    let track = if vw > 1.0 { vw } else { 120.0 };
                    let ratio = (vw / cw.max(vw)).clamp(0.0, 1.0);
                    let min_thumb = scrollbar_min_thumb_along_track(track);
                    let tw = (track * ratio).clamp(min_thumb, track.max(min_thumb));
                    let max_offset = (cw - vw).max(0.0);
                    let t = if max_offset > 0.0 {
                        sx / max_offset
                    } else {
                        0.0
                    };
                    let ml = (track - tw).max(0.0) * t.clamp(0.0, 1.0);
                    (track, track_cross, tw, thumb_cross, ml, inset)
                }
                ScrollbarOrientation::Vertical => {
                    let track = if vh > 1.0 { vh } else { 120.0 };
                    let ratio = (vh / ch.max(vh)).clamp(0.0, 1.0);
                    let min_thumb = scrollbar_min_thumb_along_track(track);
                    let th = (track * ratio).clamp(min_thumb, track.max(min_thumb));
                    let max_offset = (ch - vh).max(0.0);
                    let t = if max_offset > 0.0 {
                        sy / max_offset
                    } else {
                        0.0
                    };
                    let mt = (track - th).max(0.0) * t.clamp(0.0, 1.0);
                    (track_cross, track, thumb_cross, th, inset, mt)
                }
            };

        let thumb_focus = builder
            .target_id
            .as_ref()
            .map(|id| format!("{SCROLLBAR_THUMB_FOCUS_PREFIX}{id}:{axis}"));
        let track_focus = builder
            .target_id
            .as_ref()
            .map(|id| format!("{SCROLLBAR_TRACK_FOCUS_PREFIX}{id}:{axis}"));

        let (track_radius, thumb_radius) = match builder.variant {
            // Capsule radius derives from the actual cross size.
            ScrollbarVariant::Capsule => (track_cross * 0.5, thumb_cross * 0.5),
            ScrollbarVariant::Rect => (0.0_f32, 0.0_f32),
        };

        let mut thumb = Element::new()
            .w(thumb_w)
            .h(thumb_h)
            .min_w(thumb_cross)
            .min_h(thumb_cross)
            .ml(thumb_ml)
            .mt(thumb_mt)
            .rounded(thumb_radius)
            .bg(thumb_color)
            .border(thumb_border_color, thumb_border_w)
            .cursor_kind(crate::core::tree::CursorKind::Pointer);
        if let Some(fid) = thumb_focus {
            thumb = thumb.focus_id(fid);
        }

        let mut track = Element::new()
            .flex_grow(builder.flex_grow)
            .flex_shrink(builder.flex_shrink)
            .rounded(track_radius)
            .bg(track_color)
            .border(track_border_color, track_border_w);
        match builder.orientation {
            ScrollbarOrientation::Horizontal => {
                let w = if track_w_px > 0.0 { track_w_px } else { 120.0 };
                // Store base thickness in max block size for runtime updates.
                track = track
                    .w(w)
                    .min_w(w)
                    .h(track_h_px)
                    .min_h(track_h_px)
                    .max_h(base_cross);
            }
            ScrollbarOrientation::Vertical => {
                let h = if track_h_px > 0.0 { track_h_px } else { 120.0 };
                track = track
                    .w(track_w_px)
                    .min_w(track_w_px)
                    .h(h)
                    .min_h(h)
                    .max_w(base_cross);
            }
        }
        if let Some(fid) = track_focus {
            track = track.focus_id(fid);
        }

        let track = match builder.position {
            Position::Absolute => track.absolute(),
            Position::Relative => track.relative(),
        };

        let mut track = track
            .inset_rect(builder.inset)
            .z_index(builder.z_index)
            .child(thumb);
        if builder.overlay {
            track.set_attr("rui_scrollbar_overlay", "1");
            match builder.orientation {
                ScrollbarOrientation::Vertical => {
                    track = track.absolute().right(0.0).top(0.0).bottom(0.0);
                }
                ScrollbarOrientation::Horizontal => {
                    track = track.absolute().left(0.0).right(0.0).bottom(0.0);
                }
            }
        }
        track.set_attr(
            ATTR_SCROLLBAR_TRACK_FADES,
            if track_fades { "1" } else { "0" },
        );
        track
    }
}

#[cfg(test)]
mod chrome_tests {
    use super::*;

    #[test]
    fn transparent_track_does_not_fade_with_thumb() {
        let mut track = [0.0, 0.0, 0.0, 0.0];
        let mut track_border = [0.0, 0.0, 0.0, 0.0];
        let mut thumb = [0.2, 0.2, 0.2, 1.0];
        let mut thumb_border = [0.0, 0.0, 0.0, 0.0];
        normalize_scrollbar_base_for_auto(
            "auto",
            false,
            0.0,
            0.0,
            &mut track,
            &mut track_border,
            &mut thumb,
            &mut thumb_border,
        );
        apply_scrollbar_visible_alpha(0.5, false, false, &mut track, &mut thumb, &mut track_border, &mut thumb_border);
        assert_eq!(track[3], 0.0);
        assert!((thumb[3] - 0.5).abs() < 1e-4);
    }

    #[test]
    fn opaque_track_and_thumb_both_fade() {
        let mut track = [0.9, 0.9, 0.9, 1.0];
        let mut track_border = [0.0, 0.0, 0.0, 0.0];
        let mut thumb = [0.2, 0.2, 0.2, 1.0];
        let mut thumb_border = [0.0, 0.0, 0.0, 0.0];
        apply_scrollbar_visible_alpha(0.5, true, false, &mut track, &mut thumb, &mut track_border, &mut thumb_border);
        assert!((track[3] - 0.5).abs() < 1e-4);
        assert!((thumb[3] - 0.5).abs() < 1e-4);
    }

    #[test]
    fn transparent_border_has_zero_width_by_default() {
        assert_eq!(scrollbar_border_width([0.0; 4], None), 0.0);
        assert_eq!(scrollbar_border_width([1.0, 1.0, 1.0, 1.0], None), 1.0);
    }
}

#[cfg(test)]
mod content_inset_tests {
    use super::scrollbar_track_content_inset_px;

    #[test]
    fn track_content_inset_is_border_when_padding_is_zero() {
        assert!((scrollbar_track_content_inset_px(0.0, 1.0, true) - 1.0).abs() < f32::EPSILON);
        assert!((scrollbar_track_content_inset_px(1.0, 1.0, true) - 2.0).abs() < f32::EPSILON);
        assert!((scrollbar_track_content_inset_px(1.0, 1.0, false) - 1.0).abs() < f32::EPSILON);
        assert!((scrollbar_track_content_inset_px(0.0, 0.0, true) - 0.0).abs() < f32::EPSILON);
    }
}
