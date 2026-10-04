//! # ScrollViewport（滚动能力层）
//!
//! 引擎级可滚动视口契约：稳定 `id`、`scroll:` focus、offset/target/metrics 信号、
//! overflow 模式信号。不包含滚动条 UI。
//!
//! - 布局原语：`Element` / `Box` 的 `overflow`（visible/hidden/clip/scroll/auto）
//! - **本模块**：把 `scroll|auto` + `id` 登记为可交互视口
//! - 产品层：[`crate::components::ScrollArea`] = viewport + 内建 Scrollbar
//! - RSX 标签：[`crate::components::ScrollViewport`]
//!
//! ## Usage
//! ```ignore
//! scroll_viewport("list")
//!   .overflow_y("auto")
//!   .w("100%")
//!   .h(200.0)
//!   .child(long_content);
//!
//! // 或布局原语 + id（overflow 为 scroll|auto 时激活）
//! <VStack id="list" overflow_y="auto" h="200">…</VStack>
//! ```
//!
//! 默认 [`DEFAULT_SCROLL_BEHAVIOR`] 为 [`SCROLL_BEHAVIOR_SMOOTH`]；瞬时跳转用
//! [`SCROLL_BEHAVIOR_JUMP`]。

use std::collections::hash_map::Entry;
use std::collections::HashMap;

use super::reactivity::use_signal;
use super::tree::{Element, IntoCssDim, IntoF32Raw};

/// Focus-id 前缀：引擎用此前缀识别可滚动视口（与组件名无关）。
pub const SCROLL_VIEWPORT_FOCUS_PREFIX: &str = "scroll:";

/// 兼容旧名；字面值与 [`SCROLL_VIEWPORT_FOCUS_PREFIX`] 相同。
pub const SCROLL_AREA_FOCUS_PREFIX: &str = SCROLL_VIEWPORT_FOCUS_PREFIX;

/// 平滑追靶（滚轮 / 程序改 `target_*` 时指数靠近）。
pub const SCROLL_BEHAVIOR_SMOOTH: &str = "smooth";
/// 瞬时跳转（无缓动）。
pub const SCROLL_BEHAVIOR_JUMP: &str = "jump";
/// 未显式指定时的滚动行为。
pub const DEFAULT_SCROLL_BEHAVIOR: &str = SCROLL_BEHAVIOR_SMOOTH;

/// 规范化 `scroll_behavior`：仅 `smooth` / `jump`；其它（含空串）→ 默认 smooth。
pub fn normalize_scroll_behavior(mode: &str) -> &'static str {
    let m = mode.trim();
    if m.eq_ignore_ascii_case(SCROLL_BEHAVIOR_SMOOTH) {
        SCROLL_BEHAVIOR_SMOOTH
    } else if m.eq_ignore_ascii_case(SCROLL_BEHAVIOR_JUMP) {
        SCROLL_BEHAVIOR_JUMP
    } else {
        DEFAULT_SCROLL_BEHAVIOR
    }
}

#[inline]
pub fn scroll_behavior_is_smooth(mode: &str) -> bool {
    normalize_scroll_behavior(mode) == SCROLL_BEHAVIOR_SMOOTH
}

#[inline]
pub fn scroll_behavior_is_jump(mode: &str) -> bool {
    normalize_scroll_behavior(mode) == SCROLL_BEHAVIOR_JUMP
}

/// Pre-formatted signal id strings for one viewport `rest` id.
/// Caching avoids per-frame `format!` when many nested viewports exist.
#[derive(Clone)]
pub struct ScrollViewportSignalKeys {
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

/// 兼容旧名。
pub type ScrollAreaSignalKeys = ScrollViewportSignalKeys;

impl ScrollViewportSignalKeys {
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
            scrollbar_visibility_x: format!("__rui_scrollbar_visibility_{rest}:x"),
            scrollbar_visibility_y: format!("__rui_scrollbar_visibility_{rest}:y"),
            spring_x: format!("scrollbar_spring_{rest}:x"),
            spring_y: format!("scrollbar_spring_{rest}:y"),
        }
    }
}

pub fn signal_keys_for_rest_cached<'a>(
    rest: &str,
    cache: &'a mut HashMap<String, ScrollViewportSignalKeys>,
) -> &'a ScrollViewportSignalKeys {
    match cache.entry(rest.to_string()) {
        Entry::Occupied(e) => e.into_mut(),
        Entry::Vacant(v) => v.insert(ScrollViewportSignalKeys::new(rest)),
    }
}

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

/// `scroll` / `auto`（及未知可滚模式）为 true；`visible` / `hidden` / `clip` 为 false。
pub fn scroll_overflow_allows_scrollbar(overflow: &str) -> bool {
    let o = overflow.trim();
    !o.eq_ignore_ascii_case("hidden")
        && !o.eq_ignore_ascii_case("clip")
        && !o.eq_ignore_ascii_case("visible")
}

/// Taffy overflow → 信号字符串。`auto` 在解析时已并入 `Scroll`，故无法区分，统一为 `"scroll"`。
pub fn overflow_style_to_mode_str(o: taffy::style::Overflow) -> &'static str {
    match o {
        taffy::style::Overflow::Visible => "visible",
        taffy::style::Overflow::Hidden => "hidden",
        taffy::style::Overflow::Clip => "clip",
        taffy::style::Overflow::Scroll => "scroll",
    }
}

pub fn overflow_style_is_scrollable(o: taffy::style::Overflow) -> bool {
    matches!(o, taffy::style::Overflow::Scroll)
}

pub fn focus_id_for_scroll_viewport(id: &str) -> String {
    format!("{}{}", SCROLL_VIEWPORT_FOCUS_PREFIX, id)
}

pub fn strip_scroll_viewport_focus_id(focus_id: &str) -> Option<&str> {
    focus_id.strip_prefix(SCROLL_VIEWPORT_FOCUS_PREFIX)
}

/// Remove all named signals associated with one scroll viewport `rest` id.
///
/// Call on deterministic teardown paths (e.g. secondary-window drop) to keep
/// signal tables and animation slots bounded in long-running sessions.
pub fn remove_scroll_viewport_signals(rest: &str) {
    use super::reactivity::remove_named_signal;
    let keys = ScrollViewportSignalKeys::new(rest);
    remove_named_signal(keys.offset_x.as_str());
    remove_named_signal(keys.offset_y.as_str());
    remove_named_signal(keys.target_x.as_str());
    remove_named_signal(keys.target_y.as_str());
    remove_named_signal(keys.viewport_w.as_str());
    remove_named_signal(keys.viewport_h.as_str());
    remove_named_signal(keys.content_w.as_str());
    remove_named_signal(keys.content_h.as_str());
    remove_named_signal(keys.behavior.as_str());
    remove_named_signal(keys.activity.as_str());
    remove_named_signal(keys.overflow_x.as_str());
    remove_named_signal(keys.overflow_y.as_str());
    remove_named_signal(keys.hover_legacy.as_str());
    remove_named_signal(keys.hover_axis_x.as_str());
    remove_named_signal(keys.hover_axis_y.as_str());
    remove_named_signal(keys.scrollbar_visibility_x.as_str());
    remove_named_signal(keys.scrollbar_visibility_y.as_str());
    remove_named_signal(keys.spring_x.as_str());
    remove_named_signal(keys.spring_y.as_str());
    remove_named_signal(scroll_hover_mode_signal_key(rest).as_str());
    remove_named_signal(scroll_scrollbar_inset_signal_key(rest).as_str());
    remove_named_signal(scroll_scrollbar_inset_x_signal_key(rest).as_str());
    remove_named_signal(scroll_scrollbar_inset_y_signal_key(rest).as_str());
    remove_named_signal(scroll_scrollbar_x_side_signal_key(rest).as_str());
    remove_named_signal(scroll_scrollbar_y_side_signal_key(rest).as_str());
}

/// 将已有节点登记为滚动视口：写 focus_id、offset、overflow/behavior/hover 信号。
/// 不含滚动条。
pub fn activate_scroll_viewport(
    mut el: Element,
    id: &str,
    overflow_x: impl AsRef<str>,
    overflow_y: impl AsRef<str>,
    scroll_behavior: impl AsRef<str>,
    hover_mode: impl AsRef<str>,
) -> Element {
    let overflow_x = overflow_x.as_ref();
    let overflow_y = overflow_y.as_ref();
    let scroll_behavior = normalize_scroll_behavior(scroll_behavior.as_ref());
    let hover_mode = hover_mode.as_ref();

    let sx = use_signal(&scroll_offset_x_signal_key(id), || 0.0_f32).peek();
    let sy = use_signal(&scroll_offset_y_signal_key(id), || 0.0_f32).peek();
    use_signal(&scroll_behavior_signal_key(id), || {
        scroll_behavior.to_string()
    })
    .set(scroll_behavior.to_string());
    use_signal(&scroll_hover_mode_signal_key(id), || hover_mode.to_string())
        .set(hover_mode.to_string());
    use_signal(&scroll_overflow_x_signal_key(id), || overflow_x.to_string())
        .set(overflow_x.to_string());
    use_signal(&scroll_overflow_y_signal_key(id), || overflow_y.to_string())
        .set(overflow_y.to_string());

    el = el
        .focus_id(focus_id_for_scroll_viewport(id))
        .overflow_x(overflow_x)
        .overflow_y(overflow_y)
        .scroll_offset(sx, sy);
    el
}

/// Authoritative scroll offset for layout collect and hit-test accumulation.
///
/// `scroll:` viewports read live signals so text glyphs, selection chrome, and
/// pointer hit-testing stay aligned when `Element.scroll_offset` lags a frame.
#[inline]
pub fn scroll_viewport_axis_offset(
    element: &Element,
    is_scroll_viewport: bool,
    axis_x: bool,
) -> f32 {
    if is_scroll_viewport {
        if let Some(rest) = element
            .focus_id
            .as_deref()
            .and_then(|f| f.strip_prefix(SCROLL_VIEWPORT_FOCUS_PREFIX))
        {
            if axis_x {
                return use_signal(&scroll_offset_x_signal_key(rest), || 0.0_f32).peek();
            }
            return use_signal(&scroll_offset_y_signal_key(rest), || 0.0_f32).peek();
        }
    }
    if axis_x {
        element.scroll_offset[0]
    } else {
        element.scroll_offset[1]
    }
}

/// 若节点任一轴为可滚 overflow，且提供了 `id`，则登记为视口；否则原样返回。
pub fn maybe_activate_scroll_viewport(
    el: Element,
    id: Option<&str>,
    scroll_behavior: &str,
    hover_mode: &str,
) -> Element {
    let Some(id) = id.filter(|s| !s.is_empty()) else {
        return el;
    };
    let ox = overflow_style_to_mode_str(el.style.overflow.x);
    let oy = overflow_style_to_mode_str(el.style.overflow.y);
    if !overflow_style_is_scrollable(el.style.overflow.x)
        && !overflow_style_is_scrollable(el.style.overflow.y)
    {
        return el;
    }
    activate_scroll_viewport(el, id, ox, oy, scroll_behavior, hover_mode)
}

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

impl super::tree::IntoElementList for ScrollViewportBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

/// `(bounds, max_scroll, content_size)` — shared by wheel cache and collect seed.
pub(crate) type ScrollMetricsSample = ([f32; 4], [f32; 2], [f32; 2]);

/// True when a fresh `max≈0` sample is a band/clip cull (or similar) over a known
/// scrollable rest — not a real expand-to-fit. Used by wheel-cache merge, post-render
/// signal publish, and same-frame overlay seed (thumb must not snap to track start).
#[inline]
pub(crate) fn is_stale_zero_max_sample(
    fresh_max: [f32; 2],
    fresh_bounds: [f32; 4],
    fresh_content: [f32; 2],
    prev: Option<&ScrollMetricsSample>,
) -> bool {
    if fresh_max[0] >= 0.5 || fresh_max[1] >= 0.5 {
        return false;
    }
    let Some((prev_bounds, prev_max, prev_content)) = prev else {
        return false;
    };
    if prev_max[0] < 0.5 && prev_max[1] < 0.5 {
        return false;
    }
    const EPS: f32 = 0.5;
    let y_was_scrollable = prev_max[1] > EPS;
    let x_was_scrollable = prev_max[0] > EPS;
    let grew_h = fresh_bounds[3] > prev_bounds[3] + EPS;
    let grew_w = fresh_bounds[2] > prev_bounds[2] + EPS;
    // Content did not collapse to the new viewport (cull samples drop content_h
    // down to vh). Stretching with the viewport (`fresh >= prev`) is still a grow.
    let content_kept_y = fresh_content[1] + EPS >= prev_content[1];
    let content_kept_x = fresh_content[0] + EPS >= prev_content[0];
    // Maximize/`scroll_only` can also collapse content_h to the new vh. If last
    // frame's content already fits the grown viewport, accept max=0 so a nested
    // MessageScroller leftover cannot keep the sidebar overlay lit.
    let old_y_fits = prev_content[1] <= fresh_bounds[3] + EPS;
    let old_x_fits = prev_content[0] <= fresh_bounds[2] + EPS;
    if (y_was_scrollable && grew_h && (content_kept_y || old_y_fits))
        || (x_was_scrollable && grew_w && (content_kept_x || old_x_fits))
    {
        return false;
    }
    true
}

#[cfg(test)]
mod scroll_behavior_tests {
    use super::*;

    #[test]
    fn normalize_accepts_smooth_and_jump_only() {
        assert_eq!(normalize_scroll_behavior("smooth"), SCROLL_BEHAVIOR_SMOOTH);
        assert_eq!(normalize_scroll_behavior("SMOOTH"), SCROLL_BEHAVIOR_SMOOTH);
        assert_eq!(normalize_scroll_behavior("jump"), SCROLL_BEHAVIOR_JUMP);
        assert_eq!(normalize_scroll_behavior(""), DEFAULT_SCROLL_BEHAVIOR);
        assert_eq!(normalize_scroll_behavior("nope"), DEFAULT_SCROLL_BEHAVIOR);
        assert!(scroll_behavior_is_smooth("smooth"));
        assert!(scroll_behavior_is_jump("jump"));
        assert!(!scroll_behavior_is_jump("nope"));
    }

    #[test]
    fn stale_zero_max_detects_cull_not_expand_to_fit() {
        let prev = ([0.0, 100.0, 320.0, 384.0], [0.0, 100.0], [320.0, 484.0]);
        assert!(is_stale_zero_max_sample(
            [0.0, 0.0],
            [0.0, 80.0, 320.0, 384.0],
            [320.0, 384.0],
            Some(&prev),
        ));
        assert!(!is_stale_zero_max_sample(
            [0.0, 0.0],
            [0.0, 48.0, 208.0, 600.0],
            [208.0, 500.0],
            Some(&([0.0, 48.0, 208.0, 300.0], [0.0, 200.0], [208.0, 500.0])),
        ));
    }
}
