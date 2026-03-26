use crate::components::scroll_area::{
    scroll_content_h_signal_key, scroll_content_w_signal_key, scroll_offset_x_signal_key,
    scroll_offset_y_signal_key, scroll_viewport_h_signal_key, scroll_viewport_w_signal_key,
};
use crate::core::reactivity::use_signal;
use crate::core::tree::{Element, IntoDimension, IntoF32};
use taffy::prelude::*;

// TODO(scrollbar-overlay):
// 仅记录方案，不在当前提交中实现：
// 1) 支持“真正内嵌 overlay 滚动条（不占内容宽度）”；
// 2) 需要布局层补充绝对定位/叠层能力，或改为 ScrollArea 内置 `scrollbar=true`；
// 3) 目标是滚动条渲染在滚动区域内部右侧/底部，内容区尺寸不因滚动条缩窄。
//
// Note:
// 先前出现过“滚动条不可见/区域不可滚动但指针可命中”的问题；
// 根因在滚动指标采集与重绘时机，修复位于 `app.rs` 的 scroll metrics 逻辑。

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

pub struct ScrollbarBuilder {
    _id: String,
    orientation: ScrollbarOrientation,
    target_id: Option<String>,
    variant: ScrollbarVariant,
    visibility: ScrollbarVisibility,
    thickness: f32,
    track_color: [f32; 4],
    thumb_color: [f32; 4],
    flex_grow: f32,
    flex_shrink: f32,
    position: Position,
    inset: Rect<LengthPercentageAuto>,
    z_index: i32,
}

pub fn scrollbar(id: &str) -> ScrollbarBuilder {
    ScrollbarBuilder {
        _id: id.to_string(),
        orientation: ScrollbarOrientation::Vertical,
        target_id: None,
        variant: ScrollbarVariant::Capsule,
        visibility: ScrollbarVisibility::Auto,
        thickness: 10.0,
        track_color: crate::core::tree::parse_color("secondary"),
        thumb_color: crate::core::tree::parse_color("foreground"),
        flex_grow: 0.0,
        flex_shrink: 0.0, // Scrollbars usually shouldn't shrink by default if they have a fixed cross size.
        position: Position::Relative,
        inset: Rect::auto(),
        z_index: 0,
    }
}

// Alias for macro lowercase mapping: <Scrollbar> -> components::scrollbar(...)
pub fn scrollbarbuilder() -> ScrollbarBuilder {
    scrollbar("scrollbar")
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
}

impl From<ScrollbarBuilder> for Element {
    fn from(builder: ScrollbarBuilder) -> Element {
        let (vw, vh, cw, ch, sx, sy, can_scroll) = if let Some(id) = builder.target_id.as_ref() {
            let vw = use_signal(&scroll_viewport_w_signal_key(id), || 0.0_f32).peek();
            let vh = use_signal(&scroll_viewport_h_signal_key(id), || 0.0_f32).peek();
            let cw = use_signal(&scroll_content_w_signal_key(id), || 0.0_f32).peek();
            let ch = use_signal(&scroll_content_h_signal_key(id), || 0.0_f32).peek();
            let sx = use_signal(&scroll_offset_x_signal_key(id), || 0.0_f32).peek();
            let sy = use_signal(&scroll_offset_y_signal_key(id), || 0.0_f32).peek();
            let can = match builder.orientation {
                ScrollbarOrientation::Horizontal => cw > vw + 0.5,
                ScrollbarOrientation::Vertical => ch > vh + 0.5,
            };
            (vw, vh, cw, ch, sx, sy, can)
        } else {
            (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, false)
        };

        if builder.visibility == ScrollbarVisibility::Never {
            return Element::new().w(0.0).h(0.0);
        }

        if builder.visibility == ScrollbarVisibility::Auto && !can_scroll {
            return Element::new().w(0.0).h(0.0);
        }

        // Persist visibility mode for runtime updates (avoid rebuild dependency).
        let spring_id = builder.target_id.clone().unwrap_or_else(|| "none".to_string());
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

        // Animation: spring for visibility
        let spring = crate::style::use_spring(&format!("scrollbar_spring_{}:{}", spring_id, axis), 0.0);
        let v = spring.peek().clamp(0.0, 1.0);

        // Auto + can_scroll: keep a real track in the tree even when v≈0 so hit-testing and
        // `update_scrollbar_elements` can attach a hit-slab each frame (see `state.rs`).

        let inset = 1.0_f32;
        // Overlay scrollbar: animation should feel like native (fade first, thickness only subtly).
        // We still scale cross-size a bit so it doesn't "pop", but keep it very restrained.
        let base_cross = builder.thickness.max(2.0);
        let cross_scale = 0.85 + 0.15 * v;
        let track_cross = base_cross * cross_scale;
        let thumb_cross = (track_cross - inset * 2.0).max(1.0);
        let mut track_color = builder.track_color;
        let mut thumb_color = builder.thumb_color;
        track_color[3] *= v;
        thumb_color[3] *= v;
        let (track_w_px, track_h_px, thumb_w, thumb_h, thumb_ml, thumb_mt) =
            match builder.orientation {
                ScrollbarOrientation::Horizontal => {
                    let track = if vw > 1.0 { vw } else { 120.0 };
                    let ratio = (vw / cw.max(vw)).clamp(0.0, 1.0);
                    let min_thumb = track.min(24.0);
                    let tw = (track * ratio).clamp(min_thumb, track.max(min_thumb));
                    let max_offset = (cw - vw).max(0.0);
                    let t = if max_offset > 0.0 { sx / max_offset } else { 0.0 };
                    let ml = (track - tw).max(0.0) * t.clamp(0.0, 1.0);
                    (track, track_cross, tw, thumb_cross, ml, 0.0)
                }
                ScrollbarOrientation::Vertical => {
                    let track = if vh > 1.0 { vh } else { 120.0 };
                    let ratio = (vh / ch.max(vh)).clamp(0.0, 1.0);
                    let min_thumb = track.min(24.0);
                    let th = (track * ratio).clamp(min_thumb, track.max(min_thumb));
                    let max_offset = (ch - vh).max(0.0);
                    let t = if max_offset > 0.0 { sy / max_offset } else { 0.0 };
                    let mt = (track - th).max(0.0) * t.clamp(0.0, 1.0);
                    (track_cross, track, thumb_cross, th, 0.0, mt)
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
            .border(crate::core::tree::parse_color("background"), 1.0 * v)
            .cursor_kind(crate::core::tree::CursorKind::Pointer);
        if let Some(fid) = thumb_focus {
            thumb = thumb.focus_id(fid);
        }

        let mut track = Element::new()
            .flex_grow(builder.flex_grow)
            .flex_shrink(builder.flex_shrink)
            .p("0.0625rem")
            .rounded(track_radius)
            .bg(track_color)
            .border(crate::core::tree::parse_color("input"), 1.0 * v);
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

        track
            .inset_rect(builder.inset)
            .z_index(builder.z_index)
            .child(thumb)
    }
}
