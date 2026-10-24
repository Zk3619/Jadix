use std::ops::{Deref, DerefMut};
use taffy::prelude::*;
use winit::event::KeyEvent;

use crate::core::reactivity::Signal;

pub use crate::layout::dimension::{
    css_root_font_size_px, dim_kind, dim_length_px, dimension_to_lp, dimension_to_lpa, em, lp_kind,
    lp_length_px, lpa_kind, lpa_length_px, px, rem, CssDim, CssRectPair, DimKind, IntoBorderWidths,
    IntoCssDim, IntoCssRectPair, IntoDimension, IntoF32, IntoF32Raw, IntoLengthPercentage,
    IntoOpacity, PercentAxis, UnitCtx, CSS_ROOT_FONT_SIZE_PX,
};
pub use crate::layout::layout_tree::{LayoutProfileMs, LayoutTree, RenderResult, TextLayoutInfo};
pub use crate::style::{
    parse_color, parse_text_color, CursorKind, CursorValue, IntoAlignItems, IntoCursor,
    IntoFlexWrap, IntoJustifyContent, IntoOverflow, IntoPaintColor, IntoTextColor, LineStyle,
    TextAlignDeclaration, TextDecorationStyle, TextOverflow, TextStyle,
};
pub use crate::style::{Style, UnitSlot};

/// Pointer hit policy for [`Element`] (`jadix_pointer_events`).
///
/// Aligns with CSS [`pointer-events`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/pointer-events):
///
/// **通用（所有元素）**
/// - [`Auto`](Self::Auto) — 默认；未指定时的行为
/// - [`None`](Self::None) — 自身与子树均不作为目标（事件穿透）
///
/// **布局扩展（Jadix / RN，非 CSS 标准）**
/// - [`BoxNone`](Self::BoxNone) — 自身不命中，子节点仍可命中
///
/// **SVG 专用**（[MDN：SVG only](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/pointer-events#svg_only_experimental_for_html)；
/// 在非 Svg/Icon 叶上仍可写入，命中语义回退为布局盒）
/// - [`BoundingBox`](Self::BoundingBox) — 命中元素边界盒（Icon / 公开 `<svg>` 叶推荐默认）
/// - [`VisiblePainted`](Self::VisiblePainted) … [`All`](Self::All) — 标准 SVG 关键字；
///   有 path 场景时按 fill/stroke 几何命中；无几何（纯纹理 Icon）回退边界盒
///
/// | Mode | Self | Children |
/// |------|------|----------|
/// | Auto / SVG 几何类 | yes | yes |
/// | None | no | no |
/// | BoxNone | no | yes |
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PointerEvents {
    /// CSS / HTML：`auto`。SVG 中与 `visiblePainted` 同效（MDN）。
    #[default]
    Auto,
    /// CSS：`none`。
    None,
    /// Jadix / React Native：`box-none`（非 CSS 关键字）。
    BoxNone,
    // —— SVG only （MDN）——
    /// SVG：`bounding-box` — 命中边界盒。
    BoundingBox,
    /// SVG：`visiblePainted`。
    VisiblePainted,
    /// SVG：`visibleFill`。
    VisibleFill,
    /// SVG：`visibleStroke`。
    VisibleStroke,
    /// SVG：`visible`。
    Visible,
    /// SVG：`painted`。
    Painted,
    /// SVG：`fill`。
    Fill,
    /// SVG：`stroke`。
    Stroke,
    /// SVG：`all`。
    All,
}

pub const ATTR_POINTER_EVENTS: &str = "jadix_pointer_events";

impl PointerEvents {
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            // —— 通用 CSS ——
            "none" => Self::None,
            "auto" | "" => Self::Auto,
            // —— Jadix / RN ——
            "box-none" | "box_none" => Self::BoxNone,
            // —— SVG only（MDN）——
            "bounding-box" | "boundingbox" => Self::BoundingBox,
            "visiblepainted" => Self::VisiblePainted,
            "visiblefill" => Self::VisibleFill,
            "visiblestroke" => Self::VisibleStroke,
            "visible" => Self::Visible,
            "painted" => Self::Painted,
            "fill" => Self::Fill,
            "stroke" => Self::Stroke,
            "all" => Self::All,
            _ => Self::Auto,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::None => "none",
            Self::BoxNone => "box-none",
            Self::BoundingBox => "bounding-box",
            Self::VisiblePainted => "visiblePainted",
            Self::VisibleFill => "visibleFill",
            Self::VisibleStroke => "visibleStroke",
            Self::Visible => "visible",
            Self::Painted => "painted",
            Self::Fill => "fill",
            Self::Stroke => "stroke",
            Self::All => "all",
        }
    }

    /// 是否属于 MDN 标注的 SVG-only 关键字（不含通用 `auto` / `none`）。
    #[inline]
    pub fn is_svg_only(self) -> bool {
        !matches!(self, Self::Auto | Self::None | Self::BoxNone)
    }

    /// 自身是否可作为指针目标（布局盒命中；路径级细分后续）。
    #[inline]
    pub fn targets_self(self) -> bool {
        !matches!(self, Self::None | Self::BoxNone)
    }

    /// 是否允许命中测试进入子树。
    #[inline]
    pub fn targets_children(self) -> bool {
        self != Self::None
    }

    /// MDN：`visible*` 族要求 `visibility: visible`（Jadix 用 `opacity > 0` 近似）。
    #[inline]
    pub fn requires_visibility(self) -> bool {
        matches!(
            self,
            Self::VisiblePainted | Self::VisibleFill | Self::VisibleStroke | Self::Visible
        )
    }

    /// 是否需对 fill/stroke 几何做路径级命中（不含 `auto`：SVG 叶上 `auto` 在命中处映射为 `visiblePainted`）。
    #[inline]
    pub fn uses_path_geometry(self) -> bool {
        matches!(
            self,
            Self::VisiblePainted
                | Self::VisibleFill
                | Self::VisibleStroke
                | Self::Visible
                | Self::Painted
                | Self::Fill
                | Self::Stroke
                | Self::All
        )
    }

    /// SVG 路径级命中策略（MDN）。`auto` / `bounding-box` / `none` / `box-none` 不走此表。
    #[inline]
    pub fn path_hit_policy(self) -> Option<PathHitPolicy> {
        match self {
            Self::VisiblePainted => Some(PathHitPolicy {
                test_fill: true,
                test_stroke: true,
                require_fill_painted: true,
                require_stroke_painted: true,
            }),
            Self::VisibleFill | Self::Fill => Some(PathHitPolicy {
                test_fill: true,
                test_stroke: false,
                require_fill_painted: false,
                require_stroke_painted: false,
            }),
            Self::VisibleStroke | Self::Stroke => Some(PathHitPolicy {
                test_fill: false,
                test_stroke: true,
                require_fill_painted: false,
                require_stroke_painted: false,
            }),
            Self::Visible | Self::All => Some(PathHitPolicy {
                test_fill: true,
                test_stroke: true,
                require_fill_painted: false,
                require_stroke_painted: false,
            }),
            Self::Painted => Some(PathHitPolicy {
                test_fill: true,
                test_stroke: true,
                require_fill_painted: true,
                require_stroke_painted: true,
            }),
            Self::Auto | Self::BoundingBox | Self::None | Self::BoxNone => None,
        }
    }
}

impl From<&str> for PointerEvents {
    fn from(s: &str) -> Self {
        Self::parse(s)
    }
}

impl From<String> for PointerEvents {
    fn from(s: String) -> Self {
        Self::parse(&s)
    }
}

impl From<&String> for PointerEvents {
    fn from(s: &String) -> Self {
        Self::parse(s)
    }
}

/// SVG 路径级 `pointer-events` 几何策略（对齐 MDN）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PathHitPolicy {
    pub test_fill: bool,
    pub test_stroke: bool,
    /// `visiblePainted` / `painted`：仅当 fill ≠ none 时测填充。
    pub require_fill_painted: bool,
    /// `visiblePainted` / `painted`：仅当 stroke ≠ none 时测描边。
    pub require_stroke_painted: bool,
}

/// Parse HTML-ish bool literals for RSX attrs (`clearable="true"`, `absolute="1"`, …).
pub fn parse_bool_like(value: impl ParseBoolLike) -> bool {
    value.to_bool()
}

pub trait ParseBoolLike {
    fn to_bool(self) -> bool;
}

impl ParseBoolLike for bool {
    fn to_bool(self) -> bool {
        self
    }
}

impl ParseBoolLike for &str {
    fn to_bool(self) -> bool {
        match self.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => true,
            "false" | "0" | "no" | "off" | "" => false,
            _ => false,
        }
    }
}

impl ParseBoolLike for String {
    fn to_bool(self) -> bool {
        self.as_str().to_bool()
    }
}

impl ParseBoolLike for &String {
    fn to_bool(self) -> bool {
        self.as_str().to_bool()
    }
}

/// Parse CSS `aspect-ratio` ([MDN](https://developer.mozilla.org/en-US/docs/Web/CSS/aspect-ratio)):
/// `auto` | `<ratio>` | `auto <ratio>` | `<ratio> auto`.
///
/// `<ratio>` is `width [/ height]`（省略 height 时为 1）。返回 `width/height`；
/// `auto` / 空 / 全局关键字 / 非法值 → [`None`]（无首选比例）。
pub fn parse_css_aspect_ratio(s: &str) -> Option<f32> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let lower = s.to_ascii_lowercase();
    match lower.as_str() {
        "auto" | "initial" | "unset" | "inherit" | "revert" | "revert-layer" => return None,
        _ => {}
    }
    // Drop optional `auto` when paired with a ratio (`auto 3/4`, `9/6 auto`).
    let mut parts: Vec<&str> = s.split_whitespace().collect();
    parts.retain(|p| !p.eq_ignore_ascii_case("auto"));
    if parts.is_empty() {
        return None;
    }
    parse_css_aspect_ratio_token(&parts.join(" "))
}

fn parse_css_aspect_ratio_token(s: &str) -> Option<f32> {
    let s = s.trim();
    if let Some((a, b)) = s.split_once('/') {
        let num: f32 = a.trim().parse().ok()?;
        let den: f32 = b.trim().parse().ok()?;
        if !num.is_finite() || !den.is_finite() || num < 0.0 || den <= 0.0 {
            return None;
        }
        Some(num / den)
    } else {
        let v: f32 = s.parse().ok()?;
        if !v.is_finite() || v < 0.0 {
            return None;
        }
        Some(v)
    }
}

/// Values accepted by [`Element::aspect_ratio`] / RSX `aspect_ratio` / `aspect-ratio`.
pub trait IntoAspectRatioValue {
    fn into_aspect_ratio_value(self) -> Option<f32>;
}

impl IntoAspectRatioValue for f32 {
    fn into_aspect_ratio_value(self) -> Option<f32> {
        if self.is_finite() && self >= 0.0 {
            Some(self)
        } else {
            None
        }
    }
}

impl IntoAspectRatioValue for f64 {
    fn into_aspect_ratio_value(self) -> Option<f32> {
        (self as f32).into_aspect_ratio_value()
    }
}

impl IntoAspectRatioValue for i32 {
    fn into_aspect_ratio_value(self) -> Option<f32> {
        (self as f32).into_aspect_ratio_value()
    }
}

impl IntoAspectRatioValue for u32 {
    fn into_aspect_ratio_value(self) -> Option<f32> {
        (self as f32).into_aspect_ratio_value()
    }
}

impl IntoAspectRatioValue for &str {
    fn into_aspect_ratio_value(self) -> Option<f32> {
        parse_css_aspect_ratio(self)
    }
}

impl IntoAspectRatioValue for String {
    fn into_aspect_ratio_value(self) -> Option<f32> {
        parse_css_aspect_ratio(&self)
    }
}

impl IntoAspectRatioValue for &String {
    fn into_aspect_ratio_value(self) -> Option<f32> {
        parse_css_aspect_ratio(self)
    }
}

/// Parse CSS time value like `"300ms"` or `"0.3s"` into milliseconds.
pub fn parse_time_ms(s: &str) -> Option<f32> {
    let s = s.trim();
    if let Some(ms) = s.strip_suffix("ms") {
        ms.trim().parse::<f32>().ok()
    } else if let Some(secs) = s.strip_suffix('s') {
        secs.trim().parse::<f32>().ok().map(|v| v * 1000.0)
    } else {
        s.parse::<f32>().ok()
    }
}

/// Parse CSS time with **required** unit (`ms` / `s`). Bare numbers are not times.
pub fn parse_css_time_ms_strict(s: &str) -> Option<f32> {
    let s = s.trim();
    if let Some(ms) = s.strip_suffix("ms").or_else(|| s.strip_suffix("MS")) {
        ms.trim().parse::<f32>().ok()
    } else if let Some(secs) = s.strip_suffix('s').or_else(|| s.strip_suffix('S')) {
        // `forwards`.strip_suffix('s') → `forward` → parse fails (good).
        secs.trim().parse::<f32>().ok().map(|v| v * 1000.0)
    } else {
        None
    }
}

fn ensure_animation_slots(list: &mut Vec<crate::style::AnimationEntry>, n: usize) {
    while list.len() < n {
        list.push(crate::style::AnimationEntry::default());
    }
}

/// Parse one or more CSS `animation` shorthand declarations (comma-separated).
pub fn parse_animation_shorthand(value: &str) -> Vec<crate::style::AnimationEntry> {
    use crate::style::{
        split_comma_list, split_ws_tokens, AnimationComposition, AnimationDirection,
        AnimationEntry, AnimationFillMode, AnimationIterationCount, AnimationPlayState,
        AnimationTimeline, TimingFunction,
    };
    let mut out = Vec::new();
    for segment in split_comma_list(value) {
        let parts = split_ws_tokens(segment);
        if parts.is_empty() {
            continue;
        }
        let mut entry = AnimationEntry::default();
        let mut dur_set = false;
        let mut delay_set = false;
        for p in parts {
            let pl = p.to_ascii_lowercase();
            if let Some(ms) = parse_css_time_ms_strict(p) {
                if !dur_set {
                    entry.duration_ms = ms;
                    dur_set = true;
                } else if !delay_set {
                    entry.delay_ms = ms;
                    delay_set = true;
                }
                continue;
            }
            if pl.starts_with("cubic-bezier(")
                || pl.starts_with("steps(")
                || pl.starts_with("linear(")
                || pl.starts_with("spring(")
                || pl == "spring"
                || matches!(
                    pl.as_str(),
                    "linear"
                        | "ease"
                        | "ease-in"
                        | "ease-out"
                        | "ease-in-out"
                        | "step-start"
                        | "step-end"
                )
            {
                entry.timing = TimingFunction::from_css(p);
                continue;
            }
            if let Some(d) = AnimationDirection::from_css(p) {
                entry.direction = d;
                continue;
            }
            if matches!(pl.as_str(), "forwards" | "backwards" | "both") {
                if let Some(f) = AnimationFillMode::from_css(p) {
                    entry.fill_mode = f;
                    continue;
                }
            }
            if let Some(ps) = AnimationPlayState::from_css(p) {
                entry.play_state = ps;
                continue;
            }
            if let Some(c) = AnimationComposition::from_css(p) {
                entry.composition = c;
                continue;
            }
            if pl == "infinite" {
                entry.iteration_count = AnimationIterationCount::Infinite;
                continue;
            }
            if let Ok(n) = pl.parse::<f32>() {
                // Bare number → iteration-count (times require units).
                entry.iteration_count = AnimationIterationCount::Finite(n.max(0.0));
                continue;
            }
            if pl == "auto" || pl.starts_with("scroll(") || pl.starts_with("view(") {
                entry.timeline = AnimationTimeline::from_css(p);
                continue;
            }
            // Remaining token → animation-name (`none` clears).
            if pl == "none" {
                entry.name.clear();
            } else {
                entry.name = p.to_string();
            }
        }
        out.push(entry);
    }
    out
}

pub fn pointer_events_targets_self(el: &Element) -> bool {
    el.pointer_events_mode().targets_self()
}

/// Whether hit-testing may descend into children.
#[inline]
pub fn pointer_events_targets_children(el: &Element) -> bool {
    el.pointer_events_mode().targets_children()
}

/// `animate-spin` / `animation: spin` must hit the unrotated layout box.
///
/// Lucide icons default to SVG `bounding-box`, which uses the visual AABB. A
/// 1s spin grows that box to the circumcircle every 45°, so a still cursor on
/// a Marker loader-circle enters/leaves every tick — CursorMoved then storms
/// extra `dt≈0` paints and the icon looks frozen.
#[inline]
pub(crate) fn css_spin_locks_pointer_to_layout_box(el: &Element) -> bool {
    el.attrs.get("jadix_animate").map(String::as_str) == Some("spin")
        || el.style.animations.iter().any(|a| a.name == "spin")
}

/// [`Element::button_hover_paint`]：用于无整树重建的按钮 hover / focus 环同步（见 `components/button.rs`）。
#[derive(Debug, Clone)]
pub struct ButtonHoverPaint {
    pub variant: String,
    pub disabled: bool,
    /// `"spring"`（欠阻尼弹簧）或 `"cubic"`（`cubic-bezier` 时长型过渡）。
    pub hover_easing: String,
    /// 显式字色（如 Calendar 非本月 `muted-foreground`）；`None` 跟变体 chrome。
    pub text_color: Option<[u8; 4]>,
}

/// [`Element::input_paint_sync`]：每帧按信号 `peek` 刷新 Input/Textarea 边框、阴影、文本与光标（避免根 `create_effect` 订阅焦点与按键）。
#[derive(Debug, Clone)]
pub struct TextEditPaintSync {
    pub base_id: String,
    pub placeholder: String,
    pub invalid: bool,
    pub disabled: bool,
    pub text_size_px: f32,
    pub font_weight: u16,
    pub line_height_px: Option<f32>,
    pub leading_scale: Option<f32>,
    /// HTML 风格 `type`（`text` / `password` / `email` / …），影响显示掩码与输入过滤。
    pub input_type: String,
    /// 非空时可点的清除（叉号）按钮。
    pub clearable: bool,
    /// `type=password` 时显示睁眼/闭眼切换明文。
    pub password_toggle: bool,
    /// `true` → `textarea:{id}` 焦点与多行编辑；`false` → `input:{id}`。
    pub multiline: bool,
    /// 多行内部滚动视口 rest id（`{id}__ta_scroll`）；单行为 `None`。
    pub scroll_rest: Option<String>,
    /// 多行可拖拽改高。
    pub resizable: bool,
    pub resize_min_h: f32,
    pub resize_max_h: f32,
    /// Command 过滤框等：无边框/阴影/焦点环，视觉壳由外层提供。
    pub plain_chrome: bool,
    /// CSS `text-align` (see [`crate::style::TextAlign`]).
    pub text_align: crate::style::TextAlign,
    /// Textarea 单行溢出：软换行（默认）或横向滚动。
    pub wrap_mode: crate::utils::text_field::TextareaWrapMode,
    /// CSS `caret-color`. `None` → theme token `caret`.
    pub caret_color: Option<[u8; 4]>,
    /// Selection highlight. `None` → theme token `selection-background`.
    pub selection_bg: Option<[u8; 4]>,
    /// Selected glyph color (CSS `::selection { color }`). `None` → theme token `selection-foreground`.
    pub selection_foreground: Option<[u8; 4]>,
    pub selection_on_blur: crate::utils::text_field::SelectionOnBlur,
}

/// 兼容旧名：与 [`TextEditPaintSync`] 相同。
pub type InputPaintSync = TextEditPaintSync;

/// Generic signal-to-element synchronization hook.
///
/// Stateful components attach one of these to their root when a signal only needs to update the
/// existing element tree (style/layout/text) rather than reconstruct a subtree through
/// [`crate::app::reactive`]. The callback runs during the frame prepass and reports its layout
/// impact. This is the reusable component-level reactive capability; structural changes such as
/// mounting an overlay still use a reactive subtree boundary.
///
/// Hover, caret blink, invalid rings, and other high-frequency chrome belong here (or on
/// [`TextEditPaintSync`]) with [`crate::core::reactivity::Signal::peek`] — not a new `bind_*`
/// effect per frame (N.7).
#[derive(Clone)]
pub struct ReactiveSync {
    callback: std::rc::Rc<dyn Fn(&mut Element, &mut LayoutTree) -> LayoutStatus>,
}

impl ReactiveSync {
    pub fn new(callback: impl Fn(&mut Element, &mut LayoutTree) -> LayoutStatus + 'static) -> Self {
        Self {
            callback: std::rc::Rc::new(callback),
        }
    }

    pub fn run(&self, element: &mut Element, layout_tree: &mut LayoutTree) -> LayoutStatus {
        (self.callback)(element, layout_tree)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutStatus {
    /// No changes needed.
    Clean,
    /// Styles changed (color, etc.) - only repaint needed, layout is valid.
    StyleChanged,
    /// Layout properties changed (width, margin, etc.) - need Taffy recompute, but tree structure is same.
    LayoutChanged,
    /// Tree structure changed (nodes added/removed) - need full Taffy rebuild.
    StructureChanged,
}

impl LayoutStatus {
    /// Strongest-wins composition for merging per-frame sync sources (hover paint, toggles, etc.).
    pub fn merge(self, other: Self) -> Self {
        use LayoutStatus::*;
        match (self, other) {
            (StructureChanged, _) | (_, StructureChanged) => StructureChanged,
            (LayoutChanged, _) | (_, LayoutChanged) => LayoutChanged,
            (StyleChanged, _) | (_, StyleChanged) => StyleChanged,
            _ => Clean,
        }
    }
}

/// Queue Taffy nodes after an in-place bind `apply`.
///
/// Primary window no longer full-rebuilds Taffy on [`LayoutStatus::StructureChanged`]
/// (only cold start / portal hoist). Bind patches that restyle children in place
/// (Marker separator hairlines: `Display::None` ↔ flex) must dirty the whole
/// patched subtree so `mark_dirty` pushes current Element styles into Taffy.
/// [`LayoutStatus::LayoutChanged`] still dirties only the patched node.
pub(crate) fn collect_bind_layout_dirty(
    el: &Element,
    status: LayoutStatus,
    out: &mut Vec<taffy::prelude::NodeId>,
) {
    match status {
        LayoutStatus::LayoutChanged => {
            if let Some(n) = el.layout_node {
                out.push(n);
            }
        }
        LayoutStatus::StructureChanged => collect_subtree_layout_nodes(el, out),
        LayoutStatus::Clean | LayoutStatus::StyleChanged => {}
    }
}

fn collect_subtree_layout_nodes(el: &Element, out: &mut Vec<taffy::prelude::NodeId>) {
    if let Some(n) = el.layout_node {
        out.push(n);
    }
    for child in &el.children {
        collect_subtree_layout_nodes(child, out);
    }
}

/// 挂在 [`Element::on_unmount`]：最后一个 `Rc` 释放时执行一次（树节点丢弃即卸载）。
pub struct UnmountHook {
    f: std::cell::RefCell<Option<Box<dyn FnOnce()>>>,
}

impl UnmountHook {
    pub fn new(f: impl FnOnce() + 'static) -> std::rc::Rc<Self> {
        std::rc::Rc::new(Self {
            f: std::cell::RefCell::new(Some(Box::new(f))),
        })
    }

    /// 取消回调（例如已手动 [`Navigator::dispose`](crate::app::navigator::Navigator::dispose)）。
    pub fn disarm(&self) {
        *self.f.borrow_mut() = None;
    }

    /// Run the hook early (e.g. when chaining multiple dispose callbacks on one element).
    pub fn fire(&self) {
        if let Some(f) = self.f.borrow_mut().take() {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        }
    }
}

impl Drop for UnmountHook {
    fn drop(&mut self) {
        let Some(f) = self.f.borrow_mut().take() else {
            return;
        };
        // Skip if reactive TLS is already tearing down (HashMap/TLS dtor re-entrancy).
        if !crate::core::reactivity::runtime_tls_alive() {
            return;
        }
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    }
}

impl Drop for Element {
    fn drop(&mut self) {
        // Default recursive `Vec<Element>` Drop still overflows on tall trees: each
        // handle drops `Box<ElementInner>` which owns `children`. Drain iteratively
        // and recycle detached inners into the TLS slab (retained-tree analog of
        // GPUI's arena — the tree outlives a frame, so this is a free-list, not
        // a per-frame bump).
        //
        // `Element: Drop` forbids moving `inner` by destructure; `ptr::read` +
        // `forget` takes the box after children have been moved onto the worklist.
        let mut stack = std::mem::take(&mut self.children);
        while let Some(mut node) = stack.pop() {
            stack.append(&mut node.children);
            let inner = unsafe { std::ptr::read(&node.inner) };
            std::mem::forget(node);
            recycle_inner_box(inner);
        }
    }
}

/// Pre-order heap DFS. Tree depth does not consume OS stack frames.
pub(crate) fn walk_elements<'a>(root: &'a Element, mut visit: impl FnMut(&'a Element)) {
    let mut stack: Vec<&'a Element> = vec![root];
    while let Some(el) = stack.pop() {
        visit(el);
        stack.extend(el.children.iter().rev());
    }
}

/// Mutable pre-order heap DFS. `visit` must not restructure `children` in a way
/// that invalidates pointers still on the worklist (same constraint as recursion).
pub(crate) fn walk_elements_mut(root: &mut Element, mut visit: impl FnMut(&mut Element)) {
    let mut stack: Vec<*mut Element> = vec![root as *mut Element];
    while let Some(ptr) = stack.pop() {
        // SAFETY: pointers stay inside `root`'s exclusive subtree for this pass.
        let el = unsafe { &mut *ptr };
        visit(el);
        let n = el.children.len();
        for i in (0..n).rev() {
            stack.push(std::ptr::addr_of_mut!(el.children[i]));
        }
    }
}

/// Pre-order heap DFS. `visit` returning `false` skips `el.children` (shortcut
/// slots, chevron subtrees, …).
pub(crate) fn walk_elements_pruned(root: &Element, mut visit: impl FnMut(&Element) -> bool) {
    let mut stack: Vec<&Element> = vec![root];
    while let Some(el) = stack.pop() {
        if !visit(el) {
            continue;
        }
        stack.extend(el.children.iter().rev());
    }
}

/// Mutable counterpart of [`walk_elements_pruned`].
pub(crate) fn walk_elements_mut_pruned(
    root: &mut Element,
    mut visit: impl FnMut(&mut Element) -> bool,
) {
    let mut stack: Vec<*mut Element> = vec![root as *mut Element];
    while let Some(ptr) = stack.pop() {
        // SAFETY: exclusive subtree walk; skip means children stay off the worklist.
        let el = unsafe { &mut *ptr };
        if !visit(el) {
            continue;
        }
        let n = el.children.len();
        for i in (0..n).rev() {
            stack.push(std::ptr::addr_of_mut!(el.children[i]));
        }
    }
}

pub(crate) fn any_in_tree(root: &Element, pred: impl FnMut(&Element) -> bool) -> bool {
    find_in_tree(root, pred).is_some()
}

pub(crate) fn find_by_attr<'a>(root: &'a Element, key: &str, value: &str) -> Option<&'a Element> {
    find_in_tree(root, |el| {
        el.attrs.get(key).map(String::as_str) == Some(value)
    })
}

pub(crate) fn find_by_attr_mut<'a>(
    root: &'a mut Element,
    key: &str,
    value: &str,
) -> Option<&'a mut Element> {
    find_in_tree_mut(root, |el| {
        el.attrs.get(key).map(String::as_str) == Some(value)
    })
}

/// First node whose `text` is `Some` and non-empty after trim.
pub(crate) fn first_trimmed_text(root: &Element) -> Option<String> {
    find_in_tree(root, |n| {
        n.text
            .as_deref()
            .map(str::trim)
            .is_some_and(|s| !s.is_empty())
    })
    .map(|n| n.text.as_deref().unwrap().trim().to_string())
}

/// First node with `text: Some(_)` (empty string counts). `""` if none.
pub(crate) fn first_text_leaf(root: &Element) -> String {
    find_in_tree(root, |n| n.text.is_some())
        .and_then(|n| n.text.clone())
        .unwrap_or_default()
}

/// Pre-order heap DFS that does not enter nested `ui!` template roots.
/// `root` is always visited (hot-reload patches a template island from the island root).
/// Callers live in `#[cfg(debug_assertions)]` HMR inject (`app/state.rs`).
#[cfg(debug_assertions)]
pub(crate) fn walk_within_template<'a>(root: &'a Element, mut visit: impl FnMut(&'a Element)) {
    let mut stack: Vec<&'a Element> = vec![root];
    while let Some(el) = stack.pop() {
        visit(el);
        for child in el.children.iter().rev() {
            if child.template_hash.is_some() {
                continue;
            }
            stack.push(child);
        }
    }
}

/// Mutable pre-order heap DFS with the same nested-template boundary as
/// [`walk_within_template`]. `visit` returning `true` stops the walk.
#[cfg(debug_assertions)]
pub(crate) fn walk_within_template_mut(
    root: &mut Element,
    mut visit: impl FnMut(&mut Element) -> bool,
) -> bool {
    let mut stack: Vec<*mut Element> = vec![root as *mut Element];
    while let Some(ptr) = stack.pop() {
        // SAFETY: pointers stay inside `root`'s exclusive subtree for this pass.
        let el = unsafe { &mut *ptr };
        if visit(el) {
            return true;
        }
        let n = el.children.len();
        for i in (0..n).rev() {
            if el.children[i].template_hash.is_some() {
                continue;
            }
            stack.push(std::ptr::addr_of_mut!(el.children[i]));
        }
    }
    false
}

pub(crate) fn find_in_tree<'a>(
    root: &'a Element,
    mut pred: impl FnMut(&Element) -> bool,
) -> Option<&'a Element> {
    let mut stack: Vec<&'a Element> = vec![root];
    while let Some(el) = stack.pop() {
        if pred(el) {
            return Some(el);
        }
        stack.extend(el.children.iter().rev());
    }
    None
}

pub(crate) fn normalize_template_id(id: &str) -> String {
    id.replace('\\', "/").to_ascii_lowercase()
}

fn template_id_eq_normalized(el: &Element, want: &str) -> bool {
    el.template_id
        .as_deref()
        .is_some_and(|t| normalize_template_id(t) == want)
}

/// Heap DFS by `template_id` (path separators + ASCII case folded).
pub(crate) fn find_by_template_id<'a>(root: &'a Element, want: &str) -> Option<&'a Element> {
    let want = normalize_template_id(want);
    find_in_tree(root, |el| template_id_eq_normalized(el, &want))
}

/// Mutable heap DFS by `template_id` (same matching as [`find_by_template_id`]).
pub(crate) fn find_by_template_id_mut<'a>(
    root: &'a mut Element,
    want: &str,
) -> Option<&'a mut Element> {
    let want = normalize_template_id(want);
    find_in_tree_mut(root, |el| template_id_eq_normalized(el, &want))
}

pub(crate) fn find_in_tree_mut<'a>(
    root: &'a mut Element,
    mut pred: impl FnMut(&Element) -> bool,
) -> Option<&'a mut Element> {
    let mut stack: Vec<*mut Element> = vec![root as *mut Element];
    while let Some(ptr) = stack.pop() {
        // SAFETY: exclusive subtree walk; we return immediately on match.
        let el = unsafe { &mut *ptr };
        if pred(el) {
            return Some(el);
        }
        let n = el.children.len();
        for i in (0..n).rev() {
            stack.push(std::ptr::addr_of_mut!(el.children[i]));
        }
    }
    None
}

/// Post-order heap DFS. Children are visited before the parent.
pub(crate) fn walk_elements_post_order_mut(
    root: &mut Element,
    mut visit: impl FnMut(&mut Element),
) {
    enum Phase {
        Enter,
        Leave,
    }
    let mut stack: Vec<(*mut Element, Phase)> = vec![(root as *mut Element, Phase::Enter)];
    while let Some((ptr, phase)) = stack.pop() {
        // SAFETY: pointers stay inside `root`'s exclusive subtree for this pass.
        let el = unsafe { &mut *ptr };
        match phase {
            Phase::Enter => {
                stack.push((ptr, Phase::Leave));
                let n = el.children.len();
                for i in (0..n).rev() {
                    stack.push((std::ptr::addr_of_mut!(el.children[i]), Phase::Enter));
                }
            }
            Phase::Leave => visit(el),
        }
    }
}

/// Child-index path from `root` to the first pre-order match, or `[]` if `root` matches.
pub(crate) fn find_path_in_tree(
    root: &Element,
    mut pred: impl FnMut(&Element) -> bool,
) -> Option<Vec<usize>> {
    if pred(root) {
        return Some(Vec::new());
    }
    struct Frame<'a> {
        el: &'a Element,
        index: usize,
        pop: bool,
    }
    let mut path = Vec::new();
    let mut stack = vec![Frame {
        el: root,
        index: 0,
        pop: false,
    }];
    while let Some(mut frame) = stack.pop() {
        if frame.index >= frame.el.children.len() {
            if frame.pop {
                path.pop();
            }
            continue;
        }
        let i = frame.index;
        frame.index += 1;
        let child = &frame.el.children[i];
        path.push(i);
        if pred(child) {
            return Some(path);
        }
        stack.push(frame);
        stack.push(Frame {
            el: child,
            index: 0,
            pop: true,
        });
    }
    None
}

/// Heap payload of [`Element`]. Public fields stay reachable via `Deref`
/// (`el.style`, `el.children`, …) so builder / layout call sites do not change.
pub struct ElementInner {
    pub style: Box<crate::style::Style>,
    pub children: Vec<Element>,
    pub text: Option<String>,
    /// Rich-text spans; when set, prepare uses set_rich_text. `text` is concat fallback.
    pub text_spans: Option<Vec<crate::style::TextSpan>>,
    /// 仅参与 glyph 排版（`set_text` / 宽度估计），不计入 `text` 的字节长度；用于减轻纯中文 IME 下 cosmic-text 行高随拼音 bbox 抖动。
    pub text_shape_suffix: Option<String>,
    pub text_style: Box<TextStyle>,
    pub(crate) inherited_text_style: Option<Box<TextStyle>>,
    #[cfg(all(debug_assertions, feature = "devtools"))]
    pub(crate) inspector_instance_id: u64,
    pub layout_bounds: [f32; 4],
    /// Last fully collected subtree extent (`max_right`, `max_bottom`) in abs layout px.
    /// `scroll_only` sibling band cull reuses this so content height / thumb geometry stay stable.
    pub(crate) collect_extent: [f32; 2],
    pub(crate) collect_extent_valid: bool,
    /// Bottom-up paint fingerprint for collect skip ([`crate::layout::collect_cache`]).
    pub(crate) collect_paint_hash: u64,
    /// Some descendant (or self) must recollect every frame (spin / shimmer / caret).
    /// Portal side-channels do **not** set this — see [`Self::collect_skip_side_channel`].
    pub(crate) collect_skip_live: bool,
    /// Transient CSS scroll-walk summary, refreshed from live styles each frame.
    pub(crate) css_scroll_active: bool,
    pub(crate) css_hover_armed: bool,
    /// Cached presence of controls/selection needing per-frame chrome sync.
    pub(crate) subtree_paint_sync: bool,
    /// Presence summaries refreshed with the paint fingerprint. Hash zero means unknown.
    pub(crate) subtree_reactive_sync: bool,
    pub(crate) subtree_layout_motion: bool,
    pub(crate) subtree_floating_panel: bool,
    /// Pending collapse measurement/damage, or a callback that can produce it.
    pub(crate) subtree_collapse_work: bool,
    /// Collect summary: hover/cursor payload exists on this node or a descendant.
    pub(crate) subtree_hover_cursor: bool,
    /// Sticky / fixed descendant: scroll-translate replay would move viewport-pinned paint.
    pub(crate) collect_skip_scroll_pin: bool,
    /// This node or a descendant is a scroll viewport / has a nonzero offset.
    /// Stamp reuse must still walk this path so `scroll_offset` fingerprints stay live.
    pub(crate) collect_skip_has_scroll: bool,
    pub(crate) collect_skip_has_canvas: bool,
    /// Portal / stacking side-channel on self or a descendant. Collect must not
    /// skip-replay this node as a unit (overlay cursors live outside the DL), but
    /// stamp `reuse_idle` may still reuse a stable paint hash when only a **sibling**
    /// StyleChanged busted the parent (inline ColorPicker hue↔alpha).
    pub(crate) collect_skip_side_channel: bool,
    /// Visual offset applied by `position: sticky` clamping (computed during layout collect).
    pub sticky_visual_offset: [f32; 2],
    /// Ancestor scroll passed to collect when this node last painted.
    /// `bind_text` StyleChanged must dest-reset in this space, not document `[0,0]`.
    pub(crate) paint_scroll_acc: [f32; 2],
    /// Host portal stamp when last collect painted this node in the overlay suffix.
    /// Hover:scale chips under `__floating_panel` have no chrome template_id; without
    /// this, `paint_kind_for_element` records a page `Node` that occupancy scissors out.
    pub(crate) paint_overlay_stamp: Option<u64>,
    /// Parent CSS `scale` product stamped at last collect (CarouselStack / nested scale).
    /// [`visual_paint_aabb`] bakes this into the vis box the same way collect does.
    pub(crate) paint_ancestor_scale: [f32; 2],
    /// Parent 2×2 linear cascade stamped at last collect. Child hit AABB must rotate
    /// with the VS `ancestor_linear` even when Taffy origin is unchanged (centered child).
    pub(crate) paint_ancestor_linear: crate::style::Mat2d,
    /// Last collect `render_rect` (paint space). DevTools highlight prefers this over abs−scroll.
    #[cfg(all(debug_assertions, feature = "devtools"))]
    pub(crate) paint_visual_bounds: [f32; 4],
    /// Generation of [`Self::paint_visual_bounds`] (matches current highlight collect gen).
    #[cfg(all(debug_assertions, feature = "devtools"))]
    pub(crate) paint_visual_gen: u32,
    /// Per-element CSS transition tween state (persists across frames).
    pub transition_snapshot: crate::style::TransitionSnapshot,
    /// Per-element CSS `@keyframes` animation runtime.
    pub animation_snapshot: crate::style::AnimationSnapshot,
    pub(crate) layout_motion: Option<crate::motion::LayoutMotion>,
    pub(crate) liquid_flow: Option<crate::style::liquid_flow::LiquidFlow>,
    /// Author-specified animatable values (not polluted by tick present writes).
    pub specified_present: crate::style::SpecifiedPresent,
    /// Layout-pass used sizes for `max-height: auto` → content px keyframes.
    pub used_layout: crate::style::UsedLayout,
    pub on_click: Option<std::rc::Rc<std::cell::RefCell<dyn FnMut()>>>,
    pub on_hover: Option<std::rc::Rc<std::cell::RefCell<dyn FnMut(bool)>>>,
    /// DOM [`mousedown`](https://developer.mozilla.org/zh-CN/docs/Web/API/Element/mousedown_event).
    /// [`Self::on_pointer_down`] is a compatibility alias of this slot.
    pub on_mousedown: Option<
        std::rc::Rc<std::cell::RefCell<dyn FnMut(&crate::app::pointer_events::PointerEvent)>>,
    >,
    /// DOM `mouseup`. [`Self::on_pointer_up`] is a compatibility alias.
    pub on_mouseup: Option<
        std::rc::Rc<std::cell::RefCell<dyn FnMut(&crate::app::pointer_events::PointerEvent)>>,
    >,
    /// Right-click / context menu.
    pub on_context_menu: Option<
        std::rc::Rc<std::cell::RefCell<dyn FnMut(&crate::app::pointer_events::PointerEvent)>>,
    >,
    /// Double-click (detail >= 2). DOM `dblclick`. [`Self::on_dblclick`] is an alias.
    pub on_double_click: Option<
        std::rc::Rc<std::cell::RefCell<dyn FnMut(&crate::app::pointer_events::PointerEvent)>>,
    >,
    /// DOM `mousemove`.
    pub on_mousemove: Option<
        std::rc::Rc<std::cell::RefCell<dyn FnMut(&crate::app::pointer_events::PointerEvent)>>,
    >,
    /// DOM `mouseover` (bubbles; [`crate::app::pointer_events::MouseEvent::related_target`] is the node being left).
    pub on_mouseover: Option<
        std::rc::Rc<std::cell::RefCell<dyn FnMut(&crate::app::pointer_events::PointerEvent)>>,
    >,
    /// DOM `mouseout` (bubbles; [`crate::app::pointer_events::MouseEvent::related_target`] is the node being entered).
    pub on_mouseout: Option<
        std::rc::Rc<std::cell::RefCell<dyn FnMut(&crate::app::pointer_events::PointerEvent)>>,
    >,
    /// Keyboard on focused node. Return `true` if the event is consumed.
    pub on_key_down: Option<std::rc::Rc<std::cell::RefCell<dyn FnMut(KeyEvent) -> bool>>>,
    /// 节点从树中丢弃且 `Rc` 计数归零时触发（用于 Navigator Host dispose 等）。
    pub on_unmount: Option<std::rc::Rc<UnmountHook>>,
    pub focus_id: Option<String>,
    pub focusable: bool,
    pub cursor: Option<CursorValue>,
    pub scroll_offset: [f32; 2],
    /// True if this node or any descendant is a scroll viewport (`scroll:` focus id).
    /// Used to prune subtrees in the ViewportsOnly scrollbar prepass (see
    /// [`scroll_viewport_subtree_count`] for nested viewports).
    pub contains_scroll_viewport: bool,
    /// Count of `scroll:` viewport nodes in this subtree (including self). Nested viewports need
    /// scrollbar geometry prepass to recurse past ancestor viewports when this is > 1 on a viewport.
    pub scroll_viewport_subtree_count: u32,
    pub template_hash: Option<String>,
    pub template_id: Option<String>,
    /// 若存在，则每帧由 `sync_button_hover_paint_subtree` 根据 `{focus_id}_hover` 与全局焦点信号刷新样式（避免 `hover.get()` / `focused.get()` 订阅根 effect）。
    pub button_hover_paint: Option<ButtonHoverPaint>,
    /// 若存在，则每帧由 [`crate::utils::text_edit::sync_text_edit_paint_subtree`] 刷新（`peek` 全部输入相关信号）。
    pub input_paint_sync: Option<TextEditPaintSync>,
    /// Switch track: [`crate::components::switch::sync_switch_checked_subtree`] reads `checked` and animates track bg + thumb translate.
    pub switch_checked_sync: Option<Signal<bool>>,
    /// Checkbox root: [`crate::components::checkbox::sync_checkbox_checked_subtree`] updates fill + checkmark color.
    pub checkbox_sync: Option<Signal<bool>>,
    /// Radio item control: group [`Signal<String>`] + attrs `jadix_radio_value`；见 `sync_radio_checked_subtree`。
    pub radio_selected_sync: Option<Signal<String>>,
    /// Shared keyboard focus ring ([`crate::style::sync_focus_ring_subtree`]).
    /// Default-on with [`.focusable(true)`](Self::focusable); Input/Textarea paint their own
    /// always-on-focus ring and skip this flag. Button uses the same helpers in `button.rs`.
    pub focus_ring: bool,
    /// Generic component-owned signal synchronization; see [`ReactiveSync`].
    pub reactive_sync: Option<ReactiveSync>,
    /// GPU 贴图资源键（见 [`crate::queue_image_rgba8`]）；布局收集时生成纹理四边形实例。
    pub image_texture: Option<String>,
    /// Parsed utility class list + base style snapshot for variant re-resolve (`hover:` / `dark:` …).
    pub class_set: Option<crate::style::ClassSet>,
    pub(crate) layout_node: Option<taffy::prelude::NodeId>,
    pub attrs: std::collections::HashMap<String, String>,
}

/// Cap so a huge unmount cannot pin unbounded RSS. Excess boxes return to the
/// system allocator.
const INNER_SLAB_CAP: usize = 4096;

struct InnerSlab {
    free: Vec<Box<ElementInner>>,
}

thread_local! {
    static INNER_SLAB: std::cell::RefCell<InnerSlab> =
        const { std::cell::RefCell::new(InnerSlab { free: Vec::new() }) };
}

fn blank_inner() -> ElementInner {
    ElementInner {
        style: Box::new(crate::style::Style::default()),
        children: Vec::new(),
        text: None,
        text_spans: None,
        text_shape_suffix: None,
        text_style: Box::new(TextStyle::default()),
        inherited_text_style: None,
        #[cfg(all(debug_assertions, feature = "devtools"))]
        inspector_instance_id: {
            static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
            NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        },
        layout_bounds: [0.0, 0.0, 0.0, 0.0],
        collect_extent: [0.0, 0.0],
        collect_extent_valid: false,
        collect_paint_hash: 0,
        collect_skip_live: false,
        css_scroll_active: false,
        css_hover_armed: false,
        subtree_paint_sync: false,
        subtree_reactive_sync: false,
        subtree_layout_motion: false,
        subtree_floating_panel: false,
        subtree_collapse_work: false,
        subtree_hover_cursor: false,
        collect_skip_scroll_pin: false,
        collect_skip_has_scroll: false,
        collect_skip_has_canvas: false,
        collect_skip_side_channel: false,
        sticky_visual_offset: [0.0, 0.0],
        paint_scroll_acc: [0.0, 0.0],
        paint_overlay_stamp: None,
        paint_ancestor_scale: [1.0, 1.0],
        paint_ancestor_linear: crate::style::MAT2D_IDENTITY,
        #[cfg(all(debug_assertions, feature = "devtools"))]
        paint_visual_bounds: [0.0, 0.0, 0.0, 0.0],
        #[cfg(all(debug_assertions, feature = "devtools"))]
        paint_visual_gen: 0,
        transition_snapshot: Default::default(),
        animation_snapshot: Default::default(),
        layout_motion: None,
        liquid_flow: None,
        specified_present: Default::default(),
        used_layout: Default::default(),
        on_click: None,
        on_hover: None,
        on_mousedown: None,
        on_mouseup: None,
        on_context_menu: None,
        on_double_click: None,
        on_mousemove: None,
        on_mouseover: None,
        on_mouseout: None,
        on_key_down: None,
        on_unmount: None,
        focus_id: None,
        focusable: false,
        cursor: None,
        scroll_offset: [0.0, 0.0],
        contains_scroll_viewport: false,
        scroll_viewport_subtree_count: 0,
        template_hash: None,
        template_id: None,
        button_hover_paint: None,
        input_paint_sync: None,
        switch_checked_sync: None,
        checkbox_sync: None,
        radio_selected_sync: None,
        focus_ring: false,
        reactive_sync: None,
        image_texture: None,
        class_set: None,
        layout_node: None,
        attrs: std::collections::HashMap::new(),
    }
}

fn clone_static_node_shallow(src: &Element) -> Element {
    let mut dst = Element::new();
    *dst.style = (*src.style).clone();
    *dst.text_style = (*src.text_style).clone();
    dst.inherited_text_style = src.inherited_text_style.clone();
    dst.text = src.text.clone();
    dst.text_spans = src.text_spans.clone();
    dst.text_shape_suffix = src.text_shape_suffix.clone();
    dst.focus_id = src.focus_id.clone();
    dst.focusable = src.focusable;
    dst.cursor = src.cursor.clone();
    dst.template_hash = src.template_hash.clone();
    dst.template_id = src.template_id.clone();
    dst.image_texture = src.image_texture.clone();
    dst.class_set = src.class_set.clone();
    dst.attrs = src.attrs.clone();
    dst.focus_ring = src.focus_ring;
    dst.specified_present = src.specified_present.clone();
    dst
}

fn alloc_inner() -> Box<ElementInner> {
    INNER_SLAB
        .try_with(|cell| cell.borrow_mut().free.pop())
        .ok()
        .flatten()
        .unwrap_or_else(|| Box::new(blank_inner()))
}

fn recycle_inner_box(mut inner: Box<ElementInner>) {
    let _ = INNER_SLAB.try_with(|cell| {
        let mut slab = cell.borrow_mut();
        if slab.free.len() >= INNER_SLAB_CAP {
            return;
        }
        *inner = blank_inner();
        slab.free.push(inner);
    });
}

#[cfg(test)]
pub(crate) fn element_inner_slab_len() -> usize {
    INNER_SLAB
        .try_with(|cell| cell.borrow().free.len())
        .unwrap_or(0)
}

#[cfg(test)]
pub(crate) fn drain_element_inner_slab() {
    let _ = INNER_SLAB.try_with(|cell| cell.borrow_mut().free.clear());
}

/// UI node handle. Construction allocates [`ElementInner`] on the heap (Iced
/// `Box<dyn Widget>` / GPUI arena-box). `size_of::<Element>()` is one pointer so
/// MSVC debug `ui!` frames do not reserve ~2.4 KiB per temporary.
pub struct Element {
    /// Heap payload. Prefer `el.field` via `Deref`; use `el.inner.a` / `el.inner.b`
    /// when one call must mutably borrow two fields (Deref cannot split-borrow).
    pub inner: Box<ElementInner>,
}

impl Deref for Element {
    type Target = ElementInner;
    #[inline]
    fn deref(&self) -> &ElementInner {
        &self.inner
    }
}

impl DerefMut for Element {
    #[inline]
    fn deref_mut(&mut self) -> &mut ElementInner {
        &mut self.inner
    }
}

impl Element {
    pub(crate) fn specified_text_style(&self) -> TextStyle {
        self.text_style.without_inherited(self.inherited_text_style.as_deref())
    }

    /// Split-borrow style + animation snapshot (Deref cannot).
    #[inline]
    pub(crate) fn tick_css_animations(&mut self, dt: f32) -> bool {
        crate::style::ensure_used_layout_from_bounds(self);
        let active = crate::style::tick_element_animations_ex(
            &mut self.inner.style,
            &mut self.inner.text_style,
            &self.inner.used_layout,
            &mut self.inner.animation_snapshot,
            dt,
        );
        self.inner.specified_present = self.inner.animation_snapshot.specified.clone();
        active
    }

    /// Split-borrow style + transition snapshot (Deref cannot).
    #[inline]
    pub(crate) fn tick_css_transitions(&mut self, dt: f32) -> bool {
        let lock = crate::style::AnimationLock::default();
        crate::style::tick_element_transitions_with_text(
            &mut self.inner.style,
            &mut self.inner.text_style,
            &mut self.inner.transition_snapshot,
            lock,
            dt,
        )
    }

    /// Split-borrow style + transition snapshot; animation lock is copied first.
    #[inline]
    pub(crate) fn tick_css_transitions_locked(&mut self, dt: f32) -> bool {
        let lock = self.inner.animation_snapshot.lock();
        crate::style::tick_element_transitions_with_text(
            &mut self.inner.style,
            &mut self.inner.text_style,
            &mut self.inner.transition_snapshot,
            lock,
            dt,
        )
    }

    /// Split-borrow style + transition snapshot (Deref cannot).
    #[inline]
    pub(crate) fn snap_css_transition_baseline(&mut self) {
        crate::style::snap_element_transitions_to_current(
            &self.inner.style,
            &mut self.inner.transition_snapshot,
        );
    }
}

/// A wrapper for event handlers to allow them to be passed as props in functional components.
pub struct Handler<T = ()>(pub Box<dyn FnMut(T)>);

impl<T> Handler<T> {
    pub fn new(f: impl FnMut(T) + 'static) -> Self {
        Self(Box::new(f))
    }
    pub fn call(&mut self, arg: T) {
        (self.0)(arg);
    }
}

pub trait IntoHandler<T> {
    fn into_handler(self) -> Handler<T>;
}

impl<T, F> From<F> for Handler<T>
where
    F: FnMut(T) + 'static,
{
    fn from(f: F) -> Self {
        Self::new(f)
    }
}

pub struct ZeroArgFn<F>(pub F);
impl<F> From<ZeroArgFn<F>> for Handler<()>
where
    F: FnMut() + 'static,
{
    fn from(mut f: ZeroArgFn<F>) -> Self {
        Handler::new(move |_| (f.0)())
    }
}

impl Default for Handler<()> {
    fn default() -> Self {
        Self(Box::new(|_| {}))
    }
}

impl From<Box<dyn FnMut()>> for Handler<()> {
    fn from(mut f: Box<dyn FnMut()>) -> Self {
        Self(Box::new(move |_| f()))
    }
}

/// A wrapper for key event handlers.
pub struct KeyHandler(pub Box<dyn FnMut(KeyEvent) -> bool>);

impl KeyHandler {
    pub fn new(f: impl FnMut(KeyEvent) -> bool + 'static) -> Self {
        Self(Box::new(f))
    }
    pub fn call(&mut self, ev: KeyEvent) -> bool {
        (self.0)(ev)
    }
}

impl<F> From<F> for KeyHandler
where
    F: FnMut(KeyEvent) -> bool + 'static,
{
    fn from(f: F) -> Self {
        Self::new(f)
    }
}

impl Default for KeyHandler {
    fn default() -> Self {
        Self(Box::new(|_| false))
    }
}

pub trait IntoElement {
    fn into_element(self) -> Element;
}

impl IntoElement for Element {
    fn into_element(self) -> Element {
        self
    }
}

impl IntoElement for String {
    fn into_element(self) -> Element {
        Element::new().text(self)
    }
}

impl IntoElement for &str {
    fn into_element(self) -> Element {
        Element::new().text(self.to_string())
    }
}

pub trait IntoElementList {
    fn into_element_list(self) -> Vec<Element>;
}

impl IntoElementList for Element {
    fn into_element_list(mut self) -> Vec<Element> {
        if self.attrs.get("jadix_fragment").map(|s| s.as_str()) == Some("1") {
            std::mem::take(&mut self.children)
        } else {
            vec![self]
        }
    }
}

/// Bind an element's `template_id` into a reactive slot (`ref={...}` macro DX).
pub fn bind_element_ref(slot: &Signal<Option<String>>, el: &Element) {
    if let Some(tid) = el.template_id.as_ref() {
        slot.set(Some(tid.clone()));
    }
}

/// Resolve `<Dynamic component={...}>`: `FnOnce() -> impl IntoElement` or an [`Element`].
pub trait IntoDynamicComponent {
    fn into_dynamic_component(self) -> Element;
}

impl IntoDynamicComponent for Element {
    fn into_dynamic_component(self) -> Element {
        self
    }
}

impl<F, R> IntoDynamicComponent for F
where
    F: FnOnce() -> R,
    R: IntoElement,
{
    fn into_dynamic_component(self) -> Element {
        self().into_element()
    }
}

impl IntoElementList for Vec<Element> {
    fn into_element_list(self) -> Vec<Element> {
        self
    }
}

impl IntoElementList for Option<Element> {
    fn into_element_list(self) -> Vec<Element> {
        self.map(|e| vec![e]).unwrap_or_default()
    }
}

impl IntoElementList for String {
    fn into_element_list(self) -> Vec<Element> {
        vec![Element::new().text(self)]
    }
}

impl IntoElementList for &str {
    fn into_element_list(self) -> Vec<Element> {
        vec![Element::new().text(self.to_string())]
    }
}

/// Run one `ui!` child on its own stack frame.
///
/// MSVC debug reserves stack for every sibling `Element` temporary in the parent
/// function. A one-line `#[inline(never)]` generic is still inlined; the vtable
/// call keeps each child's expansion in a separate frame so the Windows ~1 MiB
/// main thread does not overflow on wide RSX trees.
pub fn build_ui_child<'a, T: IntoElementList>(f: impl FnOnce() -> T + 'a) -> Vec<Element> {
    trait UiChildBuild {
        fn build(self: Box<Self>) -> Vec<Element>;
    }
    impl<F, U> UiChildBuild for F
    where
        F: FnOnce() -> U,
        U: IntoElementList,
    {
        #[inline(never)]
        fn build(self: Box<Self>) -> Vec<Element> {
            (*self)().into_element_list()
        }
    }
    let boxed: Box<dyn UiChildBuild + 'a> = Box::new(f);
    boxed.build()
}

/// Run a whole `ui!` tree on its own stack frame (parent only stores the `Element`).
///
/// Without this, a large RSX block is inlined into the enclosing function and MSVC
/// debug reserves the entire tree's temporaries in that prologue.
pub fn build_ui_root<'a>(f: impl FnOnce() -> Element + 'a) -> Element {
    trait UiRootBuild {
        fn build(self: Box<Self>) -> Element;
    }
    impl<F> UiRootBuild for F
    where
        F: FnOnce() -> Element,
    {
        #[inline(never)]
        fn build(self: Box<Self>) -> Element {
            (*self)()
        }
    }
    let boxed: Box<dyn UiRootBuild + 'a> = Box::new(f);
    boxed.build()
}

/// Build many independent subtrees (page sections, showcase lists) without holding
/// every child's expansion in one MSVC-debug prologue. Prefer this over `vec![a(), b(), …]`
/// when `a`/`b` are large `ui!` trees **not** already behind `#[component]` / `#[inline(never)]`.
///
/// `#[component]` functions are already `#[inline(never)]`; `vec![Foo(Default::default()), …]`
/// is then fine (only finished `Element` values live in the parent).
pub fn collect_ui(builders: impl IntoIterator<Item = fn() -> Element>) -> Vec<Element> {
    builders.into_iter().flat_map(build_ui_child).collect()
}

/// Argument to [`Element::text`]: a static string, or (via impls in `app/reactive`) a
/// [`crate::core::reactivity::Signal`] / `FnMut() -> impl Into<String>` live binding.
///
/// `.text(sig.get())` receives an already evaluated value; `get()` still tracks
/// its enclosing effect, but this call creates no leaf binding. `Memo` / `StoreField` are
/// not `IntoNodeText` — use `|| memo.get().to_string()` so the leaf effect tracks.
pub trait IntoNodeText {
    fn apply_node_text(self, el: Element) -> Element;
}

impl IntoNodeText for String {
    fn apply_node_text(self, mut el: Element) -> Element {
        el.text = Some(self);
        el.style.flex_shrink = 0.0;
        el
    }
}

impl IntoNodeText for &str {
    fn apply_node_text(self, el: Element) -> Element {
        el.text(self.to_string())
    }
}

impl IntoNodeText for &String {
    fn apply_node_text(self, el: Element) -> Element {
        el.text(self.clone())
    }
}

impl IntoNodeText for std::borrow::Cow<'_, str> {
    fn apply_node_text(self, el: Element) -> Element {
        el.text(self.into_owned())
    }
}

impl IntoNodeText for Box<str> {
    fn apply_node_text(self, el: Element) -> Element {
        el.text(String::from(self))
    }
}

impl IntoNodeText for char {
    fn apply_node_text(self, el: Element) -> Element {
        el.text(self.to_string())
    }
}

/// Argument to [`Element::class`] / [`crate::primitives::BoxBuilder::class`]: a static
/// utility-class string, or (via impls in `app/reactive`) a [`Signal`] / `FnMut() -> String`
/// live binding.
pub trait IntoNodeClass {
    fn apply_node_class(self, el: Element) -> Element;
}

impl IntoNodeClass for String {
    fn apply_node_class(self, el: Element) -> Element {
        self.as_str().apply_node_class(el)
    }
}

impl IntoNodeClass for &str {
    fn apply_node_class(self, el: Element) -> Element {
        let trimmed = self.trim();
        if trimmed.is_empty() {
            el
        } else {
            crate::style::with_class_list(el, trimmed)
        }
    }
}

impl IntoNodeClass for &String {
    fn apply_node_class(self, el: Element) -> Element {
        self.as_str().apply_node_class(el)
    }
}

/// Argument to live [`crate::primitives::BoxBuilder::w`] / `h`: a static CSS dim, or
/// (via impls in `app/reactive`) a [`Signal`] / `FnMut` binding.
pub trait IntoNodeSize {
    fn apply_node_size(self, el: Element, axis: &'static str) -> Element;
}

fn apply_static_node_size(el: Element, axis: &'static str, dim: impl IntoCssDim) -> Element {
    match axis {
        "w" | "width" => el.w(dim),
        "h" | "height" => el.h(dim),
        _ => el,
    }
}

impl IntoNodeSize for CssDim {
    fn apply_node_size(self, el: Element, axis: &'static str) -> Element {
        apply_static_node_size(el, axis, self)
    }
}

impl IntoNodeSize for f32 {
    fn apply_node_size(self, el: Element, axis: &'static str) -> Element {
        apply_static_node_size(el, axis, self)
    }
}

impl IntoNodeSize for i32 {
    fn apply_node_size(self, el: Element, axis: &'static str) -> Element {
        apply_static_node_size(el, axis, self)
    }
}

impl IntoNodeSize for Dimension {
    fn apply_node_size(self, el: Element, axis: &'static str) -> Element {
        apply_static_node_size(el, axis, self)
    }
}

impl IntoNodeSize for &str {
    fn apply_node_size(self, el: Element, axis: &'static str) -> Element {
        apply_static_node_size(el, axis, self)
    }
}

impl IntoNodeSize for String {
    fn apply_node_size(self, el: Element, axis: &'static str) -> Element {
        apply_static_node_size(el, axis, self.as_str())
    }
}

impl IntoNodeSize for &String {
    fn apply_node_size(self, el: Element, axis: &'static str) -> Element {
        apply_static_node_size(el, axis, self.as_str())
    }
}

/// Argument to live [`crate::primitives::BoxBuilder::bg`]: a static paint color, or
/// (via impls in `app/reactive`) a [`Signal`] / `FnMut` binding.
pub trait IntoNodeBg {
    fn apply_node_bg(self, el: Element) -> Element;
}

impl IntoNodeBg for [f32; 4] {
    fn apply_node_bg(self, el: Element) -> Element {
        el.bg(self)
    }
}

impl IntoNodeBg for &[f32; 4] {
    fn apply_node_bg(self, el: Element) -> Element {
        el.bg(*self)
    }
}

impl IntoNodeBg for [u8; 4] {
    fn apply_node_bg(self, el: Element) -> Element {
        el.bg(self)
    }
}

impl IntoNodeBg for &str {
    fn apply_node_bg(self, el: Element) -> Element {
        el.bg(self)
    }
}

impl IntoNodeBg for String {
    fn apply_node_bg(self, el: Element) -> Element {
        el.bg(self.as_str())
    }
}

impl IntoNodeBg for &String {
    fn apply_node_bg(self, el: Element) -> Element {
        el.bg(self.as_str())
    }
}

/// Argument to live [`Element::text_color`] / [`crate::primitives::BoxBuilder::text_color`]:
/// a static color, or (via impls in `app/reactive`) a [`Signal`] / `FnMut` binding.
pub trait IntoNodeFg {
    fn apply_node_fg(self, el: Element) -> Element;
}

impl IntoNodeFg for [u8; 4] {
    fn apply_node_fg(self, el: Element) -> Element {
        el.text_color(self)
    }
}

impl IntoNodeFg for &[u8; 4] {
    fn apply_node_fg(self, el: Element) -> Element {
        el.text_color(*self)
    }
}

impl IntoNodeFg for [f32; 4] {
    fn apply_node_fg(self, el: Element) -> Element {
        el.text_color(self)
    }
}

impl IntoNodeFg for &str {
    fn apply_node_fg(self, el: Element) -> Element {
        el.text_color(self)
    }
}

impl IntoNodeFg for String {
    fn apply_node_fg(self, el: Element) -> Element {
        el.text_color(self.as_str())
    }
}

impl IntoNodeFg for &String {
    fn apply_node_fg(self, el: Element) -> Element {
        el.text_color(self.as_str())
    }
}

/// Argument to live [`crate::primitives::BoxBuilder::opacity`]: a static opacity, or
/// (via impls in `app/reactive`) a [`Signal`] / `FnMut` binding.
pub trait IntoNodeOpacity {
    fn apply_node_opacity(self, el: Element) -> Element;
}

fn apply_static_node_opacity(el: Element, o: impl IntoOpacity) -> Element {
    el.opacity(o)
}

impl IntoNodeOpacity for f32 {
    fn apply_node_opacity(self, el: Element) -> Element {
        apply_static_node_opacity(el, self)
    }
}

impl IntoNodeOpacity for f64 {
    fn apply_node_opacity(self, el: Element) -> Element {
        apply_static_node_opacity(el, self)
    }
}

impl IntoNodeOpacity for &str {
    fn apply_node_opacity(self, el: Element) -> Element {
        apply_static_node_opacity(el, self)
    }
}

impl IntoNodeOpacity for String {
    fn apply_node_opacity(self, el: Element) -> Element {
        apply_static_node_opacity(el, self.as_str())
    }
}

impl IntoNodeOpacity for &String {
    fn apply_node_opacity(self, el: Element) -> Element {
        apply_static_node_opacity(el, self.as_str())
    }
}

macro_rules! impl_into_node_opacity_int {
    ($($t:ty),+ $(,)?) => {$(
        impl IntoNodeOpacity for $t {
            fn apply_node_opacity(self, el: Element) -> Element {
                apply_static_node_opacity(el, self)
            }
        }
    )+};
}
impl_into_node_opacity_int!(i8, i16, i32, i64, isize, u8, u16, u32, u64, usize);

impl Element {
    pub(crate) fn into_margin_auto(val: impl IntoDimension) -> LengthPercentageAuto {
        crate::layout::dimension_to_lpa(val.into_dimension())
    }

    fn set_css(mut self, slot: UnitSlot, dim: impl IntoCssDim) -> Self {
        self.set_css_dim(slot, dim.into_css_dim());
        self
    }

    /// Authored dimensions must also survive the next class-variant restore.
    pub(crate) fn set_css_dim(&mut self, slot: UnitSlot, dim: CssDim) {
        if let Some(classes) = self.class_set.as_mut() {
            classes.set_base_dimension(slot, dim.clone());
        }
        self.style.set_css_dim(slot, dim);
    }

    fn apply_rect_pair(self, start: UnitSlot, end: UnitSlot, pair: impl IntoCssRectPair) -> Self {
        match pair.into_css_rect_pair() {
            CssRectPair::Both(d) => self.set_css(start, d.clone()).set_css(end, d),
            CssRectPair::StartEnd(a, b) => self.set_css(start, a).set_css(end, b),
        }
    }

    /// Allocates from the TLS inner slab, or the heap on a miss. `#[inline(never)]`
    /// keeps the `ElementInner` temporary inside this frame (MSVC debug would
    /// otherwise reserve it in every inlined caller).
    #[inline(never)]
    pub fn new() -> Self {
        Self {
            inner: alloc_inner(),
        }
    }

    /// WP-N.6: copy static structure (style / text / attrs / class / specified
    /// present / children) into a fresh [`ElementInner`] (Solid `_tmpl$.cloneNode`).
    ///
    /// Layout handles, paint snapshots, and instance hooks (`on_*` / unmount / paint
    /// sync) are not copied — remount must re-run `render()` when those exist.
    pub fn clone_static_skeleton(&self) -> Self {
        let mut dst = clone_static_node_shallow(self);
        dst.children = self
            .children
            .iter()
            .map(Self::clone_static_skeleton)
            .collect();
        dst
    }
}

impl Default for Element {
    fn default() -> Self {
        Self::new()
    }
}

impl Element {
    /// 绑定已上传的贴图键；矩形区域由布局样式（`w`/`h` 等）决定。
    pub fn image_texture(mut self, key: impl Into<String>) -> Self {
        self.image_texture = Some(key.into());
        self
    }

    pub fn set_attr(&mut self, key: impl Into<String>, value: impl Into<String>) -> &mut Self {
        self.attrs.insert(key.into(), value.into());
        self
    }

    pub fn get_attr(&self, key: &str) -> Option<&String> {
        self.attrs.get(key)
    }

    /// Viewport-space layout box, with ancestor scroll and sticky offsets applied.
    /// Excludes CSS transforms. Current during `reactive_sync`; otherwise uses
    /// the last paint's scroll offset.
    pub fn viewport_layout_bounds(&self) -> [f32; 4] {
        crate::app::tree_hit::collision_hit_box(self, self.paint_scroll_acc)
    }

    pub fn template_hash(mut self, hash: impl Into<String>) -> Self {
        self.template_hash = Some(hash.into());
        self
    }

    pub fn template_id(mut self, id: impl Into<String>) -> Self {
        self.template_id = Some(id.into());
        self
    }

    pub fn cursor_kind(mut self, c: CursorKind) -> Self {
        self.cursor = Some(c.into());
        self
    }

    /// CSS [`cursor`](https://developer.mozilla.org/zh-CN/docs/Web/CSS/Reference/Properties/cursor)。
    /// `inherit` / `unset` 清除本节点声明（沿用祖先）。
    pub fn cursor(mut self, value: impl IntoCursor) -> Self {
        self.cursor = value.into_cursor();
        self
    }

    pub fn cursor_text(self) -> Self {
        self.cursor_kind(CursorKind::Text)
    }
    pub fn cursor_not_allowed(self) -> Self {
        self.cursor_kind(CursorKind::NotAllowed)
    }
    pub fn min_w(self, w: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::MinWidth, w)
    }
    pub fn min_h(self, h: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::MinHeight, h)
    }
    pub fn max_w(self, w: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::MaxWidth, w)
    }
    pub fn max_h(self, h: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::MaxHeight, h)
    }
    pub fn on_click(mut self, handler: impl FnMut() + 'static) -> Self {
        self.on_click = Some(std::rc::Rc::new(std::cell::RefCell::new(handler)));
        self
    }

    /// DOM [`mousedown`](https://developer.mozilla.org/zh-CN/docs/Web/API/Element/mousedown_event).
    pub fn on_mousedown(
        mut self,
        handler: impl FnMut(&crate::app::pointer_events::PointerEvent) + 'static,
    ) -> Self {
        self.on_mousedown = Some(std::rc::Rc::new(std::cell::RefCell::new(handler)));
        self
    }

    /// DOM [`mouseup`](https://developer.mozilla.org/zh-CN/docs/Web/API/Element/mouseup_event).
    pub fn on_mouseup(
        mut self,
        handler: impl FnMut(&crate::app::pointer_events::PointerEvent) + 'static,
    ) -> Self {
        self.on_mouseup = Some(std::rc::Rc::new(std::cell::RefCell::new(handler)));
        self
    }

    /// Compatibility alias of [`Self::on_mousedown`].
    pub fn on_pointer_down(
        self,
        handler: impl FnMut(&crate::app::pointer_events::PointerEvent) + 'static,
    ) -> Self {
        self.on_mousedown(handler)
    }

    /// Compatibility alias of [`Self::on_mouseup`].
    pub fn on_pointer_up(
        self,
        handler: impl FnMut(&crate::app::pointer_events::PointerEvent) + 'static,
    ) -> Self {
        self.on_mouseup(handler)
    }

    pub fn on_context_menu(
        mut self,
        handler: impl FnMut(&crate::app::pointer_events::PointerEvent) + 'static,
    ) -> Self {
        self.on_context_menu = Some(std::rc::Rc::new(std::cell::RefCell::new(handler)));
        self
    }

    pub fn on_double_click(
        mut self,
        handler: impl FnMut(&crate::app::pointer_events::PointerEvent) + 'static,
    ) -> Self {
        self.on_double_click = Some(std::rc::Rc::new(std::cell::RefCell::new(handler)));
        self
    }

    /// DOM `dblclick` — alias of [`Self::on_double_click`].
    pub fn on_dblclick(
        self,
        handler: impl FnMut(&crate::app::pointer_events::PointerEvent) + 'static,
    ) -> Self {
        self.on_double_click(handler)
    }

    /// DOM [`mousemove`](https://developer.mozilla.org/zh-CN/docs/Web/API/Element/mousemove_event).
    pub fn on_mousemove(
        mut self,
        handler: impl FnMut(&crate::app::pointer_events::PointerEvent) + 'static,
    ) -> Self {
        self.on_mousemove = Some(std::rc::Rc::new(std::cell::RefCell::new(handler)));
        self
    }

    /// DOM [`mouseover`](https://developer.mozilla.org/zh-CN/docs/Web/API/Element/mouseover_event).
    pub fn on_mouseover(
        mut self,
        handler: impl FnMut(&crate::app::pointer_events::PointerEvent) + 'static,
    ) -> Self {
        self.on_mouseover = Some(std::rc::Rc::new(std::cell::RefCell::new(handler)));
        self
    }

    /// DOM [`mouseout`](https://developer.mozilla.org/zh-CN/docs/Web/API/Element/mouseout_event).
    pub fn on_mouseout(
        mut self,
        handler: impl FnMut(&crate::app::pointer_events::PointerEvent) + 'static,
    ) -> Self {
        self.on_mouseout = Some(std::rc::Rc::new(std::cell::RefCell::new(handler)));
        self
    }

    /// 节点从 UI 树移除且钩子 `Rc` 归零时调用一次。
    pub fn on_unmount(mut self, f: impl FnOnce() + 'static) -> Self {
        self.on_unmount = Some(UnmountHook::new(f));
        self
    }

    pub fn on_unmount_hook(mut self, hook: std::rc::Rc<UnmountHook>) -> Self {
        self.on_unmount = Some(hook);
        self
    }

    /// Attach component-owned reactive synchronization without subscribing the root UI effect.
    pub fn reactive_sync(
        mut self,
        callback: impl Fn(&mut Element, &mut LayoutTree) -> LayoutStatus + 'static,
    ) -> Self {
        self.reactive_sync = Some(ReactiveSync::new(callback));
        self
    }

    /// `jadix_pointer_events`：命中策略。
    ///
    /// 对齐 [CSS `pointer-events`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/pointer-events)：
    /// - **通用**：`auto`（默认）| `none`
    /// - **布局扩展**：`box-none`（Jadix / RN）
    /// - **SVG 专用**：`bounding-box` | `visiblePainted` | `visibleFill` | `visibleStroke` |
    ///   `visible` | `painted` | `fill` | `stroke` | `all`  
    ///   （Icon / `<svg>` 叶推荐 `bounding-box`；有 path 场景时 SVG 关键字走 fill/stroke 几何）
    pub fn pointer_events(mut self, mode: impl Into<PointerEvents>) -> Self {
        let parsed = mode.into();
        // Always persist（含 `auto`），以便 Svg 叶区分「显式 auto」与「未设置 → 默认 bounding-box」。
        self.set_attr(ATTR_POINTER_EVENTS, parsed.as_str());
        self
    }

    /// [`PointerEvents::None`] — 整树不命中。
    pub fn pointer_events_none(mut self) -> Self {
        self.set_attr(ATTR_POINTER_EVENTS, PointerEvents::None.as_str());
        self
    }

    /// [`PointerEvents::BoxNone`] — 自身穿透，子节点仍可命中。
    pub fn pointer_events_box_none(mut self) -> Self {
        self.set_attr(ATTR_POINTER_EVENTS, PointerEvents::BoxNone.as_str());
        self
    }

    pub fn pointer_events_mode(&self) -> PointerEvents {
        self.attrs
            .get(ATTR_POINTER_EVENTS)
            .map(|s| PointerEvents::parse(s))
            .unwrap_or(PointerEvents::Auto)
    }

    /// `true` unless [`PointerEvents::None`]（兼容旧调用）。
    pub fn pointer_events_enabled(&self) -> bool {
        self.pointer_events_mode() != PointerEvents::None
    }
    pub fn on_hover(mut self, handler: impl FnMut(bool) + 'static) -> Self {
        self.on_hover = Some(std::rc::Rc::new(std::cell::RefCell::new(handler)));
        self
    }

    /// Freeze page scroll while the pointer is over this node; leaving restores it.
    ///
    /// Composes with an existing [`Self::on_hover`] handler. Same as calling
    /// [`crate::set_page_scroll_locked`] from hover.
    pub fn lock_page_scroll_on_hover(mut self) -> Self {
        let prev = self.on_hover.take();
        self.on_hover(move |inside| {
            crate::app::portal_overlay::set_page_scroll_locked(inside);
            if let Some(ref prev) = prev {
                (prev.borrow_mut())(inside);
            }
        })
    }
    pub fn on_key_down(mut self, handler: impl FnMut(KeyEvent) -> bool + 'static) -> Self {
        self.on_key_down = Some(std::rc::Rc::new(std::cell::RefCell::new(handler)));
        self
    }
    pub fn focus_id(mut self, id: impl Into<String>) -> Self {
        self.focus_id = Some(id.into());
        self
    }
    /// Keyboard-focusable. Also enables the shared `:focus-visible` ring (same chrome as Input).
    /// Call [`Self::focus_ring`]`(false)` afterwards to opt out.
    pub fn focusable(mut self, focusable: bool) -> Self {
        self.set_focusable(focusable);
        self
    }
    /// Keep [`Self::focusable`] and the shared keyboard ring in lockstep (field-assignment sites).
    pub fn set_focusable(&mut self, focusable: bool) {
        self.focusable = focusable;
        self.focus_ring = focusable;
    }
    /// Shared focus-visible ring (theme `ring` + width, overridable via [`Self::ring_width`] /
    /// [`Self::ring_color`] / `ring-*` classes). Keyboard Tab only, except Input-like controls;
    /// see [`crate::style::focus_ring`].
    pub fn focus_ring(mut self, enabled: bool) -> Self {
        self.focus_ring = enabled;
        self
    }
    pub fn scroll_offset(mut self, x: f32, y: f32) -> Self {
        self.scroll_offset = [x.max(0.0), y.max(0.0)];
        self
    }
    pub fn gap(self, g: impl IntoCssDim) -> Self {
        let d = g.into_css_dim();
        self.set_css(UnitSlot::GapX, d.clone())
            .set_css(UnitSlot::GapY, d)
    }

    /// `column-gap` / Tailwind `gap-x-*`。
    pub fn gap_x(self, g: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::GapX, g)
    }

    /// `row-gap` / Tailwind `gap-y-*`。
    pub fn gap_y(self, g: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::GapY, g)
    }

    pub fn column_gap(self, g: impl IntoCssDim) -> Self {
        self.gap_x(g)
    }

    pub fn row_gap(self, g: impl IntoCssDim) -> Self {
        self.gap_y(g)
    }

    pub fn overflow(mut self, mode: impl IntoOverflow) -> Self {
        let o = mode.into_overflow();
        self.style.overflow.x = o;
        self.style.overflow.y = o;
        self
    }
    pub fn overflow_x(mut self, mode: impl IntoOverflow) -> Self {
        self.style.overflow.x = mode.into_overflow();
        self
    }
    pub fn overflow_y(mut self, mode: impl IntoOverflow) -> Self {
        self.style.overflow.y = mode.into_overflow();
        self
    }
    /// Static string, [`crate::core::reactivity::Signal`], or `|| String` compute.
    /// `text(sig.get())` is still a snapshot — pass `sig` or `|| sig.get().to_string()`.
    #[track_caller]
    pub fn text(self, text: impl IntoNodeText) -> Self {
        text.apply_node_text(self)
    }
    /// Utility class string, [`crate::core::reactivity::Signal`], or `|| String` compute.
    /// `class(sig.get())` is still a snapshot — pass `sig` or `|| sig.get()`.
    #[track_caller]
    pub fn class(self, classes: impl IntoNodeClass) -> Self {
        classes.apply_node_class(self)
    }
    pub fn font_family(mut self, family: impl Into<String>) -> Self {
        if let Some(inherited) = self.inherited_text_style.as_mut() {
            inherited.font_family = None;
        }
        self.text_style.font_family = Some(family.into());
        self.attrs
            .insert("font_family".to_string(), "1".to_string());
        self
    }
    /// Static color, [`crate::core::reactivity::Signal`], or `|| color` compute via
    /// [`IntoNodeFg`] on builders. Direct [`IntoTextColor`] here is a snapshot:
    /// `text_color(sig.get())` does not subscribe.
    pub fn text_color(mut self, color: impl IntoTextColor) -> Self {
        if let Some(inherited) = self.inherited_text_style.as_mut() {
            inherited.color = None;
        }
        self.text_style.color = Some(color.into_text_color());
        self
    }
    pub fn text_size(mut self, size: impl IntoF32) -> Self {
        if let Some(inherited) = self.inherited_text_style.as_mut() {
            inherited.size = None;
        }
        let px = size.into_f32();
        self.text_style.size = Some(px);
        if let Some(sync) = self.input_paint_sync.as_mut() {
            sync.text_size_px = px;
        }
        self
    }
    pub fn font_weight(mut self, weight: impl IntoF32Raw) -> Self {
        if let Some(inherited) = self.inherited_text_style.as_mut() {
            inherited.weight = None;
        }
        let w = weight.into_f32_raw().round().clamp(100.0, 900.0) as u16;
        self.text_style.weight = Some(w);
        self
    }
    pub fn underline(mut self, underline: bool) -> Self {
        self.text_style.underline = Some(underline);
        self
    }
    pub fn text_decoration(mut self, decoration: impl Into<String>) -> Self {
        let raw = decoration.into().to_lowercase();
        if raw.contains("none") {
            self.text_style.underline = Some(false);
            self.text_style.overline = Some(false);
            self.text_style.line_through = Some(false);
            return self;
        }
        let has_line_token = raw.contains("underline")
            || raw.contains("overline")
            || raw.contains("line-through")
            || raw.contains("linethrough")
            || raw.contains("strikethrough");
        if has_line_token {
            self.text_style.underline = Some(false);
            self.text_style.overline = Some(false);
            self.text_style.line_through = Some(false);
            if raw.contains("underline") {
                self.text_style.underline = Some(true);
            }
            if raw.contains("overline") {
                self.text_style.overline = Some(true);
            }
            if raw.contains("line-through")
                || raw.contains("linethrough")
                || raw.contains("strikethrough")
            {
                self.text_style.line_through = Some(true);
            }
        }
        if raw.contains("double") {
            self.text_style.decoration_style = Some(TextDecorationStyle::Double);
        } else if raw.contains("dotted") {
            self.text_style.decoration_style = Some(TextDecorationStyle::Dotted);
        } else if raw.contains("dashed") {
            self.text_style.decoration_style = Some(TextDecorationStyle::Dashed);
        } else if raw.contains("wavy") {
            self.text_style.decoration_style = Some(TextDecorationStyle::Wavy);
        } else if raw.contains("solid") {
            self.text_style.decoration_style = Some(TextDecorationStyle::Solid);
        }
        self
    }
    pub fn text_decoration_line(mut self, line: impl Into<String>) -> Self {
        let raw = line.into().to_lowercase();
        if raw.contains("none") {
            self.text_style.underline = Some(false);
            self.text_style.overline = Some(false);
            self.text_style.line_through = Some(false);
            return self;
        }
        if raw.contains("underline") {
            self.text_style.underline = Some(true);
        }
        if raw.contains("overline") {
            self.text_style.overline = Some(true);
        }
        if raw.contains("line-through")
            || raw.contains("linethrough")
            || raw.contains("strikethrough")
        {
            self.text_style.line_through = Some(true);
        }
        self
    }
    pub fn text_decoration_style(mut self, style: impl Into<String>) -> Self {
        let s = style.into().trim().to_lowercase();
        self.text_style.decoration_style = Some(match s.as_str() {
            "double" => TextDecorationStyle::Double,
            "dotted" => TextDecorationStyle::Dotted,
            "dashed" => TextDecorationStyle::Dashed,
            "wavy" => TextDecorationStyle::Wavy,
            _ => TextDecorationStyle::Solid,
        });
        self
    }
    pub fn text_decoration_color(mut self, color: impl IntoTextColor) -> Self {
        self.text_style.decoration_color = Some(color.into_text_color());
        self.text_style.decoration_color_specified = true;
        self
    }
    pub fn text_decoration_thickness(mut self, thickness: impl IntoF32) -> Self {
        self.text_style.decoration_thickness = Some(thickness.into_f32().max(0.0));
        self
    }
    pub fn text_underline_offset(mut self, offset: impl IntoF32) -> Self {
        self.text_style.underline_offset = Some(offset.into_f32());
        self
    }
    pub fn text_shadow(mut self, spec: impl AsRef<str>) -> Self {
        let (c, offset, blur) = crate::style::parse_text_shadow(spec);
        self.text_style.shadow_color = c;
        self.text_style.shadow_offset = offset;
        self.text_style.shadow_blur = blur;
        self
    }
    pub fn text_shadow_color(mut self, color: [u8; 4]) -> Self {
        self.text_style.shadow_color = Some(color);
        self
    }
    pub fn text_shadow_blur(mut self, blur: impl IntoF32) -> Self {
        self.text_style.shadow_blur = blur.into_f32().max(0.0);
        self
    }
    pub fn text_shadow_offset_x(mut self, dx: impl IntoF32) -> Self {
        self.text_style.shadow_offset[0] = dx.into_f32();
        self
    }
    pub fn text_shadow_offset_y(mut self, dy: impl IntoF32) -> Self {
        self.text_style.shadow_offset[1] = dy.into_f32();
        self
    }
    pub fn text_align(mut self, align: impl Into<TextAlignDeclaration>) -> Self {
        use crate::style::TextAlign;
        match align.into() {
            TextAlignDeclaration::Inherit => {
                self.text_style.align = None;
            }
            TextAlignDeclaration::Initial => {
                self.text_style.align = Some(TextAlign::Start);
            }
            TextAlignDeclaration::Value(v) => {
                self.text_style.align = Some(v);
            }
        }
        self
    }
    pub fn text_center(self) -> Self {
        self.text_align("center")
    }
    pub fn text_left(self) -> Self {
        self.text_align("left")
    }
    pub fn text_right(self) -> Self {
        self.text_align("right")
    }
    pub fn text_line_height(mut self, line_height: impl IntoF32) -> Self {
        self.text_style.line_height_px = Some(line_height.into_f32().max(1.0));
        self.text_style.line_height_specified = true;
        self
    }
    pub fn text_leading(mut self, scale: impl IntoF32Raw) -> Self {
        self.text_style.line_height_px = None;
        self.text_style.line_height_scale = Some(scale.into_f32_raw().max(0.01));
        self.text_style.line_height_specified = true;
        self
    }
    pub fn text_leading_none(mut self) -> Self {
        self.text_style.line_height_px = None;
        self.text_style.line_height_scale = Some(1.0);
        self.text_style.line_height_specified = true;
        self
    }
    pub fn text_leading_tight(mut self) -> Self {
        self.text_style.line_height_px = None;
        self.text_style.line_height_scale = Some(1.25);
        self.text_style.line_height_specified = true;
        self
    }
    pub fn text_leading_snug(mut self) -> Self {
        self.text_style.line_height_px = None;
        self.text_style.line_height_scale = Some(1.375);
        self.text_style.line_height_specified = true;
        self
    }
    pub fn text_leading_normal(mut self) -> Self {
        self.text_style.line_height_px = None;
        self.text_style.line_height_scale = Some(1.5);
        self.text_style.line_height_specified = true;
        self
    }
    pub fn text_leading_relaxed(mut self) -> Self {
        self.text_style.line_height_px = None;
        self.text_style.line_height_scale = Some(1.625);
        self.text_style.line_height_specified = true;
        self
    }
    pub fn text_leading_loose(mut self) -> Self {
        self.text_style.line_height_px = None;
        self.text_style.line_height_scale = Some(2.0);
        self.text_style.line_height_specified = true;
        self
    }
    /// Apply `text-overflow`: `clip` / `ellipsis` are nowrap + **inline** `overflow-x: hidden`
    /// (`overflow-y` stays visible so glyph ink can overflow the line box — CSS
    /// `text-overflow` does not scissor CJK / descenders to `leading-none`).
    /// `hidden` is the native `<input>` field: both axes clip.
    /// `newline` restores wrapping (`white-space: normal`) and visible overflow.
    ///
    /// Default [`TextOverflow::Clip`] on a new node does **not** run this — wrapping stays
    /// allowed until clip/hidden/ellipsis is set explicitly (or via `truncate`).
    pub fn apply_text_overflow_mode(&mut self, overflow: TextOverflow) {
        use crate::style::WhiteSpace;
        self.text_style.overflow = overflow;
        self.attrs
            .insert("text_overflow".to_string(), overflow.as_str().to_string());
        match overflow {
            TextOverflow::Clip | TextOverflow::Ellipsis => {
                self.text_style.white_space = WhiteSpace::Nowrap;
                self.attrs
                    .insert("white_space".to_string(), "nowrap".to_string());
                self.style.overflow.x = taffy::style::Overflow::Hidden;
                self.style.overflow.y = taffy::style::Overflow::Visible;
            }
            TextOverflow::Hidden => {
                self.text_style.white_space = WhiteSpace::Nowrap;
                self.attrs
                    .insert("white_space".to_string(), "nowrap".to_string());
                self.style.overflow.x = taffy::style::Overflow::Hidden;
                self.style.overflow.y = taffy::style::Overflow::Hidden;
            }
            TextOverflow::Newline => {
                self.text_style.white_space = WhiteSpace::Normal;
                self.attrs
                    .insert("white_space".to_string(), "normal".to_string());
                self.style.overflow.x = taffy::style::Overflow::Visible;
                self.style.overflow.y = taffy::style::Overflow::Visible;
            }
        }
    }
    pub fn text_overflow(mut self, overflow: impl Into<TextOverflow>) -> Self {
        self.apply_text_overflow_mode(overflow.into());
        self
    }
    pub fn text_overflow_ellipsis(self) -> Self {
        self.text_overflow(TextOverflow::Ellipsis)
    }
    pub fn text_overflow_clip(self) -> Self {
        self.text_overflow(TextOverflow::Clip)
    }
    pub fn text_overflow_hidden(self) -> Self {
        self.text_overflow(TextOverflow::Hidden)
    }
    pub fn text_overflow_newline(self) -> Self {
        self.text_overflow(TextOverflow::Newline)
    }
    /// CSS / Tailwind `whitespace-nowrap`: caption stays one line (ShadCN Button).
    pub fn whitespace_nowrap(mut self) -> Self {
        self.text_style.white_space = crate::style::WhiteSpace::Nowrap;
        self.attrs
            .insert("white_space".to_string(), "nowrap".to_string());
        self
    }
    pub fn white_space(mut self, keyword: impl AsRef<str>) -> Self {
        let s = keyword.as_ref().trim().to_ascii_lowercase();
        self.text_style.white_space = match s.as_str() {
            "nowrap" | "no-wrap" | "pre" => crate::style::WhiteSpace::Nowrap,
            _ => crate::style::WhiteSpace::Normal,
        };
        self.attrs.insert("white_space".to_string(), s);
        self
    }

    /// Text soft-wrap mode ([`crate::text::WrapMode`]): `word` / `char` / `none`.
    /// Also updates `white_space` (None → nowrap; Word/Char → normal).
    pub fn text_wrap(mut self, mode: impl Into<crate::text::WrapMode>) -> Self {
        let mode = mode.into();
        self.attrs
            .insert("text_wrap".to_string(), mode.as_str().to_string());
        self.text_style.wrap_mode = mode;
        match mode {
            crate::text::WrapMode::None => {
                self.text_style.white_space = crate::style::WhiteSpace::Nowrap;
                self.attrs
                    .insert("white_space".to_string(), "nowrap".to_string());
            }
            crate::text::WrapMode::Word | crate::text::WrapMode::Char => {
                self.text_style.white_space = crate::style::WhiteSpace::Normal;
                self.attrs
                    .insert("white_space".to_string(), "normal".to_string());
            }
        }
        self
    }

    pub fn selectable(mut self, selectable: bool) -> Self {
        self.text_style.selectable = selectable;
        if selectable {
            self.attrs
                .insert("text_selectable".to_string(), "1".to_string());
        } else {
            self.attrs.remove("text_selectable");
        }
        self
    }
    /// AccessKit semantic role (e.g. `"group"`, `"button"`, `"label"`).
    pub fn accesskit_role(mut self, role: impl Into<String>) -> Self {
        self.attrs.insert("accesskit_role".to_string(), role.into());
        self
    }
    /// Accessible name (preferred over `aria-label` / text / placeholder).
    pub fn accesskit_name(mut self, name: impl Into<String>) -> Self {
        self.attrs.insert("accesskit_name".to_string(), name.into());
        self
    }
    /// ARIA label (fallback name when `accesskit_name` is absent).
    pub fn aria_label(mut self, name: impl Into<String>) -> Self {
        self.attrs.insert("aria-label".to_string(), name.into());
        self
    }
    /// Stable AccessKit node key when the element has no `focus_id` (e.g. static Label).
    ///
    /// Pair with [`Self::aria_labelledby`] on a control. See `docs/ops/a11y_smoke.md`.
    pub fn accesskit_id(mut self, id: impl Into<String>) -> Self {
        self.attrs.insert("accesskit_id".to_string(), id.into());
        self
    }
    /// Space-separated `accesskit_id` / `focus_id` keys of labeling nodes (`aria-labelledby`).
    pub fn aria_labelledby(mut self, ids: impl Into<String>) -> Self {
        self.attrs.insert("aria-labelledby".to_string(), ids.into());
        self
    }
    /// Static paint, [`crate::core::reactivity::Signal`], or `|| color` compute via
    /// [`IntoNodeBg`] on builders. Direct [`IntoPaintColor`] here is a snapshot:
    /// `bg(sig.get())` does not subscribe.
    pub fn bg(mut self, color: impl IntoPaintColor) -> Self {
        self.style.bg_color = color.into_paint_color();
        self
    }
    pub fn border(mut self, color: impl IntoPaintColor, width: f32) -> Self {
        self.style.border_style = LineStyle::Solid;
        self.style.border_color = color.into_paint_color();
        self.style.border_color_sides = None;
        self.style.set_uniform_border_width(width);
        self
    }
    pub fn border_style(mut self, style: impl Into<LineStyle>) -> Self {
        self.style.border_style = style.into();
        self
    }
    pub fn border_color(mut self, color: impl IntoPaintColor) -> Self {
        self.style.border_color = color.into_paint_color();
        self.style.border_color_sides = None;
        self
    }
    pub fn border_width(mut self, width: impl IntoBorderWidths) -> Self {
        self.style
            .set_border_widths_trbl(width.into_border_widths());
        self
    }
    pub fn border_top_width(mut self, width: impl IntoF32) -> Self {
        self.style.set_border_side_width(0, width.into_f32());
        self
    }
    pub fn border_right_width(mut self, width: impl IntoF32) -> Self {
        self.style.set_border_side_width(1, width.into_f32());
        self
    }
    pub fn border_bottom_width(mut self, width: impl IntoF32) -> Self {
        self.style.set_border_side_width(2, width.into_f32());
        self
    }
    pub fn border_left_width(mut self, width: impl IntoF32) -> Self {
        self.style.set_border_side_width(3, width.into_f32());
        self
    }
    pub fn border_top_color(mut self, color: impl IntoPaintColor) -> Self {
        self.style
            .set_border_side_color(0, color.into_paint_color());
        self
    }
    pub fn border_right_color(mut self, color: impl IntoPaintColor) -> Self {
        self.style
            .set_border_side_color(1, color.into_paint_color());
        self
    }
    pub fn border_bottom_color(mut self, color: impl IntoPaintColor) -> Self {
        self.style
            .set_border_side_color(2, color.into_paint_color());
        self
    }
    pub fn border_left_color(mut self, color: impl IntoPaintColor) -> Self {
        self.style
            .set_border_side_color(3, color.into_paint_color());
        self
    }
    pub fn border_top(self, color: impl IntoPaintColor, width: impl IntoF32) -> Self {
        self.ensure_border_solid()
            .border_top_color(color)
            .border_top_width(width)
    }
    pub fn border_right(self, color: impl IntoPaintColor, width: impl IntoF32) -> Self {
        self.ensure_border_solid()
            .border_right_color(color)
            .border_right_width(width)
    }
    pub fn border_bottom(self, color: impl IntoPaintColor, width: impl IntoF32) -> Self {
        self.ensure_border_solid()
            .border_bottom_color(color)
            .border_bottom_width(width)
    }
    pub fn border_left(self, color: impl IntoPaintColor, width: impl IntoF32) -> Self {
        self.ensure_border_solid()
            .border_left_color(color)
            .border_left_width(width)
    }
    pub fn border_inline_start_width(self, width: impl IntoF32) -> Self {
        self.border_left_width(width)
    }
    pub fn border_inline_end_width(self, width: impl IntoF32) -> Self {
        self.border_right_width(width)
    }
    pub fn border_block_start_width(self, width: impl IntoF32) -> Self {
        self.border_top_width(width)
    }
    pub fn border_block_end_width(self, width: impl IntoF32) -> Self {
        self.border_bottom_width(width)
    }
    // Per-side style
    pub fn border_top_style(mut self, style: impl Into<LineStyle>) -> Self {
        self.style.set_border_side_style(0, style.into());
        self
    }
    pub fn border_right_style(mut self, style: impl Into<LineStyle>) -> Self {
        self.style.set_border_side_style(1, style.into());
        self
    }
    pub fn border_bottom_style(mut self, style: impl Into<LineStyle>) -> Self {
        self.style.set_border_side_style(2, style.into());
        self
    }
    pub fn border_left_style(mut self, style: impl Into<LineStyle>) -> Self {
        self.style.set_border_side_style(3, style.into());
        self
    }
    // Logical property style
    pub fn border_inline_start_style(self, style: impl Into<LineStyle>) -> Self {
        self.border_left_style(style)
    }
    pub fn border_inline_end_style(self, style: impl Into<LineStyle>) -> Self {
        self.border_right_style(style)
    }
    pub fn border_block_start_style(self, style: impl Into<LineStyle>) -> Self {
        self.border_top_style(style)
    }
    pub fn border_block_end_style(self, style: impl Into<LineStyle>) -> Self {
        self.border_bottom_style(style)
    }
    // Logical property color
    pub fn border_inline_start_color(self, color: [f32; 4]) -> Self {
        self.border_left_color(color)
    }
    pub fn border_inline_end_color(self, color: [f32; 4]) -> Self {
        self.border_right_color(color)
    }
    pub fn border_block_start_color(self, color: [f32; 4]) -> Self {
        self.border_top_color(color)
    }
    pub fn border_block_end_color(self, color: [f32; 4]) -> Self {
        self.border_bottom_color(color)
    }
    // Logical compound shortcuts
    pub fn border_inline(self, color: [f32; 4], width: impl IntoF32 + Copy) -> Self {
        self.ensure_border_solid()
            .border_left_color(color)
            .border_right_color(color)
            .border_left_width(width)
            .border_right_width(width)
    }
    pub fn border_block(self, color: [f32; 4], width: impl IntoF32 + Copy) -> Self {
        self.ensure_border_solid()
            .border_top_color(color)
            .border_bottom_color(color)
            .border_top_width(width)
            .border_bottom_width(width)
    }
    pub fn border_inline_start(self, color: [f32; 4], width: impl IntoF32) -> Self {
        self.border_left(color, width)
    }
    pub fn border_inline_end(self, color: [f32; 4], width: impl IntoF32) -> Self {
        self.border_right(color, width)
    }
    pub fn border_block_start(self, color: [f32; 4], width: impl IntoF32) -> Self {
        self.border_top(color, width)
    }
    pub fn border_block_end(self, color: [f32; 4], width: impl IntoF32) -> Self {
        self.border_bottom(color, width)
    }
    // border-radius longhand (CSS order on `Style.radii`: TL TR BR BL)
    pub fn border_top_left_radius(mut self, r: f32) -> Self {
        self.style.radii[0] = r.max(0.0);
        self
    }
    pub fn border_top_right_radius(mut self, r: f32) -> Self {
        self.style.radii[1] = r.max(0.0);
        self
    }
    pub fn border_bottom_right_radius(mut self, r: f32) -> Self {
        self.style.radii[2] = r.max(0.0);
        self
    }
    pub fn border_bottom_left_radius(mut self, r: f32) -> Self {
        self.style.radii[3] = r.max(0.0);
        self
    }
    // border-shape / corner-shape
    pub fn border_shape(mut self, shape: crate::style::CornerShape) -> Self {
        self.style.corner_shape = shape;
        self.style.corner_shape_sides = None;
        self
    }
    pub fn corner_top_left_shape(mut self, shape: crate::style::CornerShape) -> Self {
        self.style.set_corner_side_shape(0, shape);
        self
    }
    pub fn corner_top_right_shape(mut self, shape: crate::style::CornerShape) -> Self {
        self.style.set_corner_side_shape(1, shape);
        self
    }
    pub fn corner_bottom_right_shape(mut self, shape: crate::style::CornerShape) -> Self {
        self.style.set_corner_side_shape(2, shape);
        self
    }
    pub fn corner_bottom_left_shape(mut self, shape: crate::style::CornerShape) -> Self {
        self.style.set_corner_side_shape(3, shape);
        self
    }
    // border-collapse / border-spacing
    pub fn border_collapse(mut self, collapse: bool) -> Self {
        self.style.border_collapse = collapse;
        self
    }
    pub fn border_spacing(mut self, h: f32, v: f32) -> Self {
        self.style.border_spacing = [h, v];
        self
    }
    // border-image
    pub fn border_image_source(mut self, src: impl Into<String>) -> Self {
        let s: String = src.into();
        if s.is_empty() {
            self.style.border_image_source = None;
        } else {
            let key = crate::app::pending_images::texture_key_for_src(&s);
            crate::app::pending_images::queue_image_from_src(&key, &s);
            self.style.border_image_source = Some(key);
        }
        self
    }
    pub fn border_image_slice(mut self, slice: [f32; 4]) -> Self {
        self.style.border_image_slice = slice;
        self
    }
    pub fn border_image_slice_fill(mut self, fill: bool) -> Self {
        self.style.border_image_slice_fill = fill;
        self
    }
    pub fn border_image_width(mut self, widths: [f32; 4]) -> Self {
        self.style.border_image_width = Some(widths);
        self
    }
    pub fn border_image_outset(mut self, outset: [f32; 4]) -> Self {
        self.style.border_image_outset = outset;
        self
    }
    pub fn border_image_repeat(mut self, repeat: [crate::style::BorderImageRepeat; 2]) -> Self {
        self.style.border_image_repeat = repeat;
        self
    }

    fn ensure_border_solid(mut self) -> Self {
        if self.style.border_style.is_invisible() {
            self.style.border_style = LineStyle::Solid;
        }
        self
    }

    pub fn outline(mut self, color: [f32; 4], width: f32) -> Self {
        self.style.outline_style = LineStyle::Solid;
        self.style.outline_color = color;
        self.style.outline_width = width;
        self
    }
    pub fn outline_style(mut self, style: impl Into<LineStyle>) -> Self {
        self.style.outline_style = style.into();
        self
    }
    pub fn outline_color(mut self, color: impl IntoPaintColor) -> Self {
        self.style.outline_color = color.into_paint_color();
        self
    }
    pub fn outline_width(mut self, width: f32) -> Self {
        self.style.outline_width = width;
        self
    }
    pub fn outline_offset(mut self, offset: impl IntoF32) -> Self {
        self.style.outline_offset = offset.into_f32();
        self
    }
    /// Focus-ring thickness in logical px (Hard spread / Soft blur).
    /// Overrides the theme [`crate::style::focus_ring_width`]. `class("ring-2")` sets the same field.
    pub fn ring_width(mut self, width: impl IntoF32) -> Self {
        self.style.focus_ring_width = Some(width.into_f32().max(0.0));
        self
    }
    /// Focus-ring color. Tokens / hex / `[f32; 4]`. Overrides theme token `ring`.
    pub fn ring_color(mut self, color: impl IntoPaintColor) -> Self {
        self.style.focus_ring_color = Some(color.into_paint_color());
        self
    }
    /// CSS `caret-color`. On Input/Textarea this forwards into [`TextEditPaintSync`].
    pub fn caret_color(mut self, color: impl IntoTextColor) -> Self {
        let c = color.into_text_color();
        if let Some(sync) = self.input_paint_sync.as_mut() {
            sync.caret_color = Some(c);
        }
        self
    }
    /// Selection highlight fill. Forwards into [`TextEditPaintSync`] when present.
    pub fn selection_background(mut self, color: impl IntoTextColor) -> Self {
        let c = color.into_text_color();
        if let Some(sync) = self.input_paint_sync.as_mut() {
            sync.selection_bg = Some(c);
        }
        self
    }
    /// Selected glyph color. Forwards into [`TextEditPaintSync`] when present.
    pub fn selection_foreground(mut self, color: impl IntoTextColor) -> Self {
        let c = color.into_text_color();
        if let Some(sync) = self.input_paint_sync.as_mut() {
            sync.selection_foreground = Some(c);
        }
        self
    }
    /// Parse CSS `outline` shorthand: `<line-width> || <line-style> || <color>` (any order).
    pub fn apply_outline_shorthand(mut self, value: impl AsRef<str>) -> Self {
        let parts: Vec<&str> = value.as_ref().split_whitespace().collect();
        for token in &parts {
            let t = token.to_ascii_lowercase();
            if matches!(
                t.as_str(),
                "none"
                    | "auto"
                    | "dotted"
                    | "dashed"
                    | "solid"
                    | "double"
                    | "groove"
                    | "ridge"
                    | "inset"
                    | "outset"
            ) {
                self.style.outline_style = LineStyle::from_css_keyword(&t);
            } else if token
                .bytes()
                .next()
                .map_or(false, |b| b.is_ascii_digit() || b == b'.')
            {
                self.style.outline_width = (*token).into_f32();
            } else {
                self.style.outline_color = parse_color(token);
            }
        }
        if self.style.outline_style == LineStyle::None
            && self.style.outline_width > 0.0
            && parts.iter().any(|p| {
                let t = p.to_ascii_lowercase();
                !matches!(t.as_str(), "none")
            })
        {
            self.style.outline_style = LineStyle::Solid;
        }
        self
    }
    pub fn rounded(mut self, radius: impl IntoF32) -> Self {
        let r = radius.into_f32();
        self.style.radii = [r, r, r, r];
        self
    }
    pub fn rounded_full(mut self) -> Self {
        // Resolved later using actual layout width/height.
        self.style.radii = [-1.0, -1.0, -1.0, -1.0];
        self
    }
    pub fn shadow(mut self, color: [f32; 4], blur: f32) -> Self {
        self.style.shadow_color = color;
        self.style.shadow_blur = blur;
        self
    }

    /// Non-modal popup chrome: [`crate::tokens::Elevation::Md`] blur, offset 0 (halo on all sides).
    pub fn popup_shadow(mut self) -> Self {
        let (color, blur) = crate::tokens::elevation_shadow(crate::tokens::Elevation::Md);
        self.style.shadow_color = color;
        self.style.shadow_blur = blur;
        self.style.shadow_offset = [0.0, 0.0];
        self.style.shadow_spread = 0.0;
        self.style.shadow_inset = false;
        self
    }
    pub fn box_shadow(mut self, shadow: impl AsRef<str>) -> Self {
        let list = crate::style::parse_box_shadows(shadow);
        self.style.set_shadows_from_parsed(&list);
        self
    }
    pub fn box_shadow_color(mut self, color: [f32; 4]) -> Self {
        self.style.shadow_color = color;
        self
    }
    pub fn box_shadow_blur(mut self, blur: f32) -> Self {
        self.style.shadow_blur = blur.max(0.0);
        self
    }

    /// Backdrop blur radius (`0` disables). See [`crate::style::Style::backdrop_blur`].
    /// RSX unitless `"1"` is 1rem (see [`css_root_font_size_px`]); builder `16.0` / `16` is 16px.
    pub fn backdrop_blur(mut self, blur: impl IntoF32) -> Self {
        self.style.backdrop_blur = blur.into_f32().max(0.0);
        self
    }
    /// CSS `filter` property — parse a CSS filter string (e.g. `"blur(5px) grayscale(0.4)"`).
    pub fn filter(mut self, s: impl AsRef<str>) -> Self {
        self.style.filter = crate::style::parse_filter(s.as_ref());
        self
    }
    /// Parse CSS `transition` shorthand, e.g. `"opacity 300ms ease-in-out 100ms"`.
    /// Multiple entries separated by commas.
    pub fn transition(mut self, value: impl AsRef<str>) -> Self {
        use crate::style::{
            split_comma_list, split_ws_tokens, TimingFunction, TransitionEntry, TransitionProperty,
        };
        self.style.transition.clear();
        for segment in split_comma_list(value.as_ref()) {
            let parts = split_ws_tokens(segment);
            if parts.is_empty() {
                continue;
            }
            let mut property = TransitionProperty::All;
            let mut duration_ms: f32 = 0.0;
            let mut delay_ms: f32 = 0.0;
            let mut timing = TimingFunction::Ease;
            let mut dur_set = false;
            for p in &parts {
                if let Some(ms) = parse_time_ms(p) {
                    if !dur_set {
                        duration_ms = ms;
                        dur_set = true;
                    } else {
                        delay_ms = ms;
                    }
                } else if p.starts_with("cubic-bezier(")
                    || p.starts_with("steps(")
                    || p.starts_with("linear(")
                    || p.starts_with("spring(")
                    || *p == "step-start"
                    || *p == "step-end"
                    || *p == "spring"
                    || p.contains("cubic-bezier")
                {
                    timing = TimingFunction::from_css(p);
                } else {
                    match *p {
                        "linear" | "ease" | "ease-in" | "ease-out" | "ease-in-out" => {
                            timing = TimingFunction::from_css(p);
                        }
                        _ => property = TransitionProperty::from_css(p),
                    }
                }
            }
            self.style.transition.push(TransitionEntry {
                property,
                duration_ms,
                delay_ms,
                timing,
            });
        }
        self
    }

    pub fn transition_property(mut self, value: impl AsRef<str>) -> Self {
        use crate::style::TransitionProperty;
        let props: Vec<TransitionProperty> = value
            .as_ref()
            .split(',')
            .map(|s| TransitionProperty::from_css(s))
            .collect();
        if self.style.transition.is_empty() {
            for p in props {
                self.style.transition.push(crate::style::TransitionEntry {
                    property: p,
                    duration_ms: 0.0,
                    delay_ms: 0.0,
                    timing: crate::style::TimingFunction::Ease,
                });
            }
        } else {
            for (i, p) in props.into_iter().enumerate() {
                let idx: usize = i.min(self.style.transition.len() - 1);
                self.style.transition[idx].property = p;
            }
        }
        self
    }

    pub fn transition_duration(mut self, value: impl AsRef<str>) -> Self {
        let durations: Vec<f32> = value
            .as_ref()
            .split(',')
            .filter_map(|s| parse_time_ms(s.trim()))
            .collect();
        for (i, d) in durations.into_iter().enumerate() {
            if i < self.style.transition.len() {
                self.style.transition[i].duration_ms = d;
            }
        }
        self
    }

    pub fn transition_delay(mut self, value: impl AsRef<str>) -> Self {
        let delays: Vec<f32> = value
            .as_ref()
            .split(',')
            .filter_map(|s| parse_time_ms(s.trim()))
            .collect();
        for (i, d) in delays.into_iter().enumerate() {
            if i < self.style.transition.len() {
                self.style.transition[i].delay_ms = d;
            }
        }
        self
    }

    pub fn transition_timing_function(mut self, value: impl AsRef<str>) -> Self {
        use crate::style::TimingFunction;
        let funcs: Vec<TimingFunction> = value
            .as_ref()
            .split(',')
            .map(|s| TimingFunction::from_css(s.trim()))
            .collect();
        for (i, f) in funcs.into_iter().enumerate() {
            if i < self.style.transition.len() {
                self.style.transition[i].timing = f;
            }
        }
        self
    }

    /// CSS `animation` shorthand (comma-separated). See MDN `animation`.
    pub fn animation(mut self, value: impl AsRef<str>) -> Self {
        self.style.animations = parse_animation_shorthand(value.as_ref());
        self
    }

    /// Attach a programmatic [`crate::style::Animation`] handle (play / pause / seek).
    /// Same `playback_id` replaces the existing list item (rebuild-safe).
    pub fn animate(mut self, anim: crate::style::Animation) -> Self {
        let entry = anim.to_entry();
        if let Some(slot) = self
            .style
            .animations
            .iter_mut()
            .find(|e| e.playback_id == entry.playback_id)
        {
            *slot = entry;
        } else {
            self.style.animations.push(entry);
        }
        self
    }

    /// Live handles attached to this node (registry dump, not a DOM API).
    pub fn get_animations(&self) -> Vec<crate::style::Animation> {
        self.style
            .animations
            .iter()
            .filter_map(|e| e.playback_id.and_then(crate::style::Animation::from_id))
            .collect()
    }

    pub fn animation_name(mut self, value: impl AsRef<str>) -> Self {
        let names: Vec<String> = value
            .as_ref()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        ensure_animation_slots(&mut self.style.animations, names.len().max(1));
        for (i, name) in names.into_iter().enumerate() {
            let idx = i.min(self.style.animations.len() - 1);
            self.style.animations[idx].name = if name.eq_ignore_ascii_case("none") {
                String::new()
            } else {
                name
            };
        }
        self
    }

    pub fn animation_duration(mut self, value: impl AsRef<str>) -> Self {
        let durs: Vec<f32> = value
            .as_ref()
            .split(',')
            .filter_map(|s| parse_time_ms(s.trim()))
            .collect();
        ensure_animation_slots(&mut self.style.animations, durs.len().max(1));
        for (i, d) in durs.into_iter().enumerate() {
            let idx = i.min(self.style.animations.len() - 1);
            self.style.animations[idx].duration_ms = d;
        }
        self
    }

    pub fn animation_delay(mut self, value: impl AsRef<str>) -> Self {
        let delays: Vec<f32> = value
            .as_ref()
            .split(',')
            .filter_map(|s| parse_time_ms(s.trim()))
            .collect();
        ensure_animation_slots(&mut self.style.animations, delays.len().max(1));
        for (i, d) in delays.into_iter().enumerate() {
            let idx = i.min(self.style.animations.len() - 1);
            self.style.animations[idx].delay_ms = d;
        }
        self
    }

    pub fn animation_timing_function(mut self, value: impl AsRef<str>) -> Self {
        use crate::style::TimingFunction;
        let funcs: Vec<TimingFunction> = value
            .as_ref()
            .split(',')
            .map(|s| TimingFunction::from_css(s.trim()))
            .collect();
        ensure_animation_slots(&mut self.style.animations, funcs.len().max(1));
        for (i, f) in funcs.into_iter().enumerate() {
            let idx = i.min(self.style.animations.len() - 1);
            self.style.animations[idx].timing = f;
        }
        self
    }

    pub fn animation_iteration_count(mut self, value: impl AsRef<str>) -> Self {
        use crate::style::AnimationIterationCount;
        let counts: Vec<AnimationIterationCount> = value
            .as_ref()
            .split(',')
            .filter_map(|s| AnimationIterationCount::from_css(s.trim()))
            .collect();
        ensure_animation_slots(&mut self.style.animations, counts.len().max(1));
        for (i, c) in counts.into_iter().enumerate() {
            let idx = i.min(self.style.animations.len() - 1);
            self.style.animations[idx].iteration_count = c;
        }
        self
    }

    pub fn animation_direction(mut self, value: impl AsRef<str>) -> Self {
        use crate::style::AnimationDirection;
        let dirs: Vec<AnimationDirection> = value
            .as_ref()
            .split(',')
            .filter_map(|s| AnimationDirection::from_css(s.trim()))
            .collect();
        ensure_animation_slots(&mut self.style.animations, dirs.len().max(1));
        for (i, d) in dirs.into_iter().enumerate() {
            let idx = i.min(self.style.animations.len() - 1);
            self.style.animations[idx].direction = d;
        }
        self
    }

    pub fn animation_fill_mode(mut self, value: impl AsRef<str>) -> Self {
        use crate::style::AnimationFillMode;
        let modes: Vec<AnimationFillMode> = value
            .as_ref()
            .split(',')
            .filter_map(|s| AnimationFillMode::from_css(s.trim()))
            .collect();
        ensure_animation_slots(&mut self.style.animations, modes.len().max(1));
        for (i, m) in modes.into_iter().enumerate() {
            let idx = i.min(self.style.animations.len() - 1);
            self.style.animations[idx].fill_mode = m;
        }
        self
    }

    pub fn animation_play_state(mut self, value: impl AsRef<str>) -> Self {
        use crate::style::AnimationPlayState;
        let states: Vec<AnimationPlayState> = value
            .as_ref()
            .split(',')
            .filter_map(|s| AnimationPlayState::from_css(s.trim()))
            .collect();
        ensure_animation_slots(&mut self.style.animations, states.len().max(1));
        for (i, s) in states.into_iter().enumerate() {
            let idx = i.min(self.style.animations.len() - 1);
            self.style.animations[idx].play_state = s;
        }
        self
    }

    pub fn animation_composition(mut self, value: impl AsRef<str>) -> Self {
        use crate::style::AnimationComposition;
        let comps: Vec<AnimationComposition> = value
            .as_ref()
            .split(',')
            .filter_map(|s| AnimationComposition::from_css(s.trim()))
            .collect();
        ensure_animation_slots(&mut self.style.animations, comps.len().max(1));
        for (i, c) in comps.into_iter().enumerate() {
            let idx = i.min(self.style.animations.len() - 1);
            self.style.animations[idx].composition = c;
        }
        self
    }

    pub fn animation_timeline(mut self, value: impl AsRef<str>) -> Self {
        use crate::style::AnimationTimeline;
        let tls: Vec<AnimationTimeline> = value
            .as_ref()
            .split(',')
            .map(|s| AnimationTimeline::from_css(s.trim()))
            .collect();
        ensure_animation_slots(&mut self.style.animations, tls.len().max(1));
        for (i, t) in tls.into_iter().enumerate() {
            let idx = i.min(self.style.animations.len() - 1);
            self.style.animations[idx].timeline = t;
        }
        self
    }

    pub fn animation_range(mut self, value: impl AsRef<str>) -> Self {
        // `animation-range: <start> <end>` per list item (comma-separated).
        let parts: Vec<&str> = value.as_ref().split(',').collect();
        ensure_animation_slots(&mut self.style.animations, parts.len().max(1));
        for (i, part) in parts.into_iter().enumerate() {
            let idx = i.min(self.style.animations.len() - 1);
            let toks: Vec<&str> = part.split_whitespace().collect();
            if toks.is_empty() {
                continue;
            }
            self.style.animations[idx].range_start = Some(toks[0].to_string());
            if toks.len() >= 2 {
                self.style.animations[idx].range_end = Some(toks[1].to_string());
            }
        }
        self
    }

    pub fn animation_range_start(mut self, value: impl AsRef<str>) -> Self {
        let parts: Vec<String> = value
            .as_ref()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        ensure_animation_slots(&mut self.style.animations, parts.len().max(1));
        for (i, s) in parts.into_iter().enumerate() {
            let idx = i.min(self.style.animations.len() - 1);
            self.style.animations[idx].range_start = Some(s);
        }
        self
    }

    pub fn animation_range_end(mut self, value: impl AsRef<str>) -> Self {
        let parts: Vec<String> = value
            .as_ref()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        ensure_animation_slots(&mut self.style.animations, parts.len().max(1));
        for (i, s) in parts.into_iter().enumerate() {
            let idx = i.min(self.style.animations.len() - 1);
            self.style.animations[idx].range_end = Some(s);
        }
        self
    }

    /// CSS `interpolate-size: numeric-only | allow-keywords`.
    pub fn interpolate_size(mut self, value: impl AsRef<str>) -> Self {
        self.style.interpolate_size = crate::style::InterpolateSize::from_css(value.as_ref());
        self
    }

    /// CSS `clip-path` (`none` / `inset()` / `polygon()` / `circle()` / `ellipse()` / `path()` / `url(#id)`).
    pub fn clip_path(mut self, value: impl AsRef<str>) -> Self {
        if let Some(parsed) = crate::style::parse_clip_path(value.as_ref()) {
            self.style.clip_path = parsed;
        }
        self
    }

    /// Publish this node's `clip-path` as `url(#id)` (see [`crate::style::register_clip_path`]).
    pub fn clip_path_name(mut self, value: impl AsRef<str>) -> Self {
        let s = value.as_ref().trim();
        self.style.clip_path_name = if s.is_empty() {
            None
        } else {
            Some(s.trim_start_matches('#').to_string())
        };
        self
    }

    /// CSS `offset-path` (`none` / `path()` / `url(#id)`).
    pub fn offset_path(mut self, value: impl AsRef<str>) -> Self {
        if let Some(parsed) = crate::style::parse_offset_path(value.as_ref()) {
            self.style.offset_path = parsed;
        }
        self
    }

    /// CSS `offset-distance` (`50%` or 0–1 fraction).
    pub fn offset_distance(mut self, value: impl AsRef<str>) -> Self {
        let s = value.as_ref().trim();
        if let Some(n) = s.strip_suffix('%') {
            if let Ok(v) = n.trim().parse::<f32>() {
                self.style.offset_distance = (v / 100.0).clamp(0.0, 1.0);
            }
        } else if let Ok(v) = s.parse::<f32>() {
            self.style.offset_distance = v.clamp(0.0, 1.0);
        }
        self
    }

    /// CSS `offset-rotate` (`auto` or angle).
    pub fn offset_rotate(mut self, value: impl AsRef<str>) -> Self {
        let s = value.as_ref().trim();
        if s.is_empty() || s.eq_ignore_ascii_case("auto") {
            self.style.offset_rotate_deg = None;
            return self;
        }
        let deg = s
            .strip_suffix("deg")
            .unwrap_or(s)
            .trim()
            .parse::<f32>()
            .ok();
        self.style.offset_rotate_deg = deg;
        self
    }

    /// CSS `scroll-timeline-name`.
    pub fn scroll_timeline_name(mut self, value: impl AsRef<str>) -> Self {
        let s = value.as_ref().trim();
        self.style.scroll_timeline_name = if s.is_empty() || s.eq_ignore_ascii_case("none") {
            None
        } else {
            Some(s.to_string())
        };
        self
    }

    /// CSS `scroll-timeline-axis`: `block` | `inline` | `x` | `y`.
    pub fn scroll_timeline_axis(mut self, value: impl AsRef<str>) -> Self {
        self.style.scroll_timeline_axis =
            crate::style::ScrollTimelineAxis::from_css(value.as_ref());
        self
    }

    /// CSS `view-timeline-name`.
    pub fn view_timeline_name(mut self, value: impl AsRef<str>) -> Self {
        let s = value.as_ref().trim();
        self.style.view_timeline_name = if s.is_empty() || s.eq_ignore_ascii_case("none") {
            None
        } else {
            Some(s.to_string())
        };
        self
    }

    /// CSS `scroll-timeline: <name> <axis>?`.
    pub fn scroll_timeline(mut self, value: impl AsRef<str>) -> Self {
        let mut it = value.as_ref().split_whitespace();
        if let Some(name) = it.next() {
            self = self.scroll_timeline_name(name);
        }
        if let Some(axis) = it.next() {
            self = self.scroll_timeline_axis(axis);
        }
        self
    }

    /// CSS `background-position` (keywords / percentages → 0–1).
    pub fn background_position(mut self, value: impl AsRef<str>) -> Self {
        if let Some(xy) = crate::style::parse_background_position(value.as_ref()) {
            self.style.bg_position = xy;
        }
        self
    }

    pub fn box_shadow_offset_x(mut self, dx: f32) -> Self {
        self.style.shadow_offset[0] = dx;
        self
    }
    pub fn box_shadow_offset_y(mut self, dy: f32) -> Self {
        self.style.shadow_offset[1] = dy;
        self
    }
    pub fn box_shadow_spread(mut self, spread: f32) -> Self {
        self.style.shadow_spread = spread;
        self
    }
    pub fn box_shadow_inset(mut self, inset: bool) -> Self {
        self.style.shadow_inset = inset;
        self
    }
    /// Static opacity. `opacity(sig.get())` is a snapshot — pass `sig` to
    /// [`crate::primitives::BoxBuilder::opacity`] for a live bind.
    pub fn opacity(mut self, o: impl IntoOpacity) -> Self {
        self.style.opacity = o.into_opacity();
        self
    }
    pub fn translate(mut self, x: f32, y: f32) -> Self {
        self.style.translate = [x, y];
        self
    }
    /// Set translate as a fraction of own bounding box (CSS percentage translate).
    /// `translate_pct(0.5, 0.0)` = shift right by 50% of own width.
    pub fn translate_pct(mut self, x: f32, y: f32) -> Self {
        self.style.translate_pct = [x, y];
        self
    }
    /// Static CSS dim. `w(sig.get())` is a snapshot — pass `sig` to
    /// [`crate::primitives::BoxBuilder::w`] for a live bind.
    pub fn w(self, width: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::Width, width)
    }
    pub fn inline_size(self, size: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::InlineSize, size)
    }
    /// Static CSS dim. `h(sig.get())` is a snapshot — pass `sig` to
    /// [`crate::primitives::BoxBuilder::h`] for a live bind.
    pub fn h(self, height: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::Height, height)
    }
    pub fn block_size(self, size: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::BlockSize, size)
    }
    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.push(child.into());
        self
    }
    pub fn children(mut self, children: impl IntoIterator<Item = Element>) -> Self {
        self.children.extend(children);
        self
    }
    pub fn flex_col(mut self) -> Self {
        self.style.flex_direction = FlexDirection::Column;
        self
    }
    pub fn justify_center(mut self) -> Self {
        self.style.justify_content = Some(JustifyContent::CENTER);
        self
    }
    pub fn justify_start(mut self) -> Self {
        self.style.justify_content = Some(JustifyContent::FLEX_START);
        self
    }
    pub fn justify_end(mut self) -> Self {
        self.style.justify_content = Some(JustifyContent::FLEX_END);
        self
    }

    /// `justify-content`，与 CSS / Tailwind `justify-*` 一致（区别于 [`Self::flex_items`] / `align-items`）。
    pub fn justify(mut self, keyword: impl IntoJustifyContent) -> Self {
        self.style.justify_content = Some(keyword.into_justify_content());
        self
    }

    /// `align-items`，与 RSX `items` / `align`、Tailwind `items-*` 一致。
    pub fn flex_items(mut self, keyword: impl IntoAlignItems) -> Self {
        self.style.align_items = Some(keyword.into_align_items());
        self
    }

    /// `flex-wrap`：`wrap` / `nowrap` / `wrap-reverse`。
    pub fn flex_wrap_kw(mut self, keyword: impl IntoFlexWrap) -> Self {
        self.style.flex_wrap = keyword.into_flex_wrap();
        self
    }

    pub fn items_center(mut self) -> Self {
        self.style.align_items = Some(AlignItems::CENTER);
        self
    }
    pub fn p(self, padding: impl IntoCssDim) -> Self {
        let d = padding.into_css_dim();
        self.set_css(UnitSlot::PadL, d.clone())
            .set_css(UnitSlot::PadR, d.clone())
            .set_css(UnitSlot::PadT, d.clone())
            .set_css(UnitSlot::PadB, d)
    }
    pub fn px(self, padding: impl IntoCssDim) -> Self {
        let d = padding.into_css_dim();
        self.set_css(UnitSlot::PadL, d.clone())
            .set_css(UnitSlot::PadR, d)
    }
    pub fn py(self, padding: impl IntoCssDim) -> Self {
        let d = padding.into_css_dim();
        self.set_css(UnitSlot::PadT, d.clone())
            .set_css(UnitSlot::PadB, d)
    }
    pub fn pt(self, padding: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::PadT, padding)
    }
    pub fn pr(self, padding: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::PadR, padding)
    }
    pub fn pb(self, padding: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::PadB, padding)
    }
    pub fn pl(self, padding: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::PadL, padding)
    }
    pub fn padding_inline(self, padding: impl IntoCssRectPair) -> Self {
        self.apply_rect_pair(UnitSlot::PadInlineStart, UnitSlot::PadInlineEnd, padding)
    }
    pub fn padding_block(self, padding: impl IntoCssRectPair) -> Self {
        self.apply_rect_pair(UnitSlot::PadBlockStart, UnitSlot::PadBlockEnd, padding)
    }
    pub fn padding_inline_start(self, padding: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::PadInlineStart, padding)
    }
    pub fn padding_inline_end(self, padding: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::PadInlineEnd, padding)
    }
    pub fn padding_block_start(self, padding: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::PadBlockStart, padding)
    }
    pub fn padding_block_end(self, padding: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::PadBlockEnd, padding)
    }
    pub fn padding_h(self, val: impl IntoCssDim) -> Self {
        self.px(val)
    }
    pub fn flex_row(mut self) -> Self {
        self.style.flex_direction = FlexDirection::Row;
        self
    }
    pub fn m(self, margin: impl IntoCssDim) -> Self {
        let d = margin.into_css_dim();
        self.set_css(UnitSlot::MarginL, d.clone())
            .set_css(UnitSlot::MarginR, d.clone())
            .set_css(UnitSlot::MarginT, d.clone())
            .set_css(UnitSlot::MarginB, d)
    }
    pub fn mx(self, margin: impl IntoCssDim) -> Self {
        let d = margin.into_css_dim();
        self.set_css(UnitSlot::MarginL, d.clone())
            .set_css(UnitSlot::MarginR, d)
    }
    pub fn my(self, margin: impl IntoCssDim) -> Self {
        let d = margin.into_css_dim();
        self.set_css(UnitSlot::MarginT, d.clone())
            .set_css(UnitSlot::MarginB, d)
    }

    /// Cancel parent **horizontal** padding by applying the negated margin, with
    /// `width: auto` so flex/column cross-axis stretch keeps left/right symmetric.
    ///
    /// Prefer this over `w("100%").mx(...)` — percentage width + negative `mx` overflows
    /// asymmetrically under Taffy/CSS flex (same footgun as browsers).
    ///
    /// Pass the parent padding value (positive), e.g. `bleed_x("0.25rem")` for `p-1`.
    /// Parent should stretch children on the cross axis (column `align-items: stretch`).
    pub fn bleed_x(self, pad: impl IntoCssDim) -> Self {
        self.w("auto").mx(pad.into_css_dim().negated())
    }

    /// Cancel parent **vertical** padding via negated `my`, with `height: auto` for
    /// row-direction stretch. Pass the parent padding value (positive).
    pub fn bleed_y(self, pad: impl IntoCssDim) -> Self {
        self.h("auto").my(pad.into_css_dim().negated())
    }

    /// ShadCN / solid-ui menu / popover chrome (`p-1`): `-mx-1 my-1` with safe width.
    ///
    /// Works on **any** node (separator, banner, full-bleed row) — not separator-only.
    pub fn menu_bleed(self) -> Self {
        self.bleed_x("0.25rem").my("0.25rem")
    }

    pub fn mt(self, margin: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::MarginT, margin)
    }
    pub fn mr(self, margin: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::MarginR, margin)
    }
    pub fn mb(self, margin: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::MarginB, margin)
    }
    pub fn ml(self, margin: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::MarginL, margin)
    }
    pub fn margin_inline(self, margin: impl IntoCssRectPair) -> Self {
        self.apply_rect_pair(
            UnitSlot::MarginInlineStart,
            UnitSlot::MarginInlineEnd,
            margin,
        )
    }
    pub fn margin_block(self, margin: impl IntoCssRectPair) -> Self {
        self.apply_rect_pair(UnitSlot::MarginBlockStart, UnitSlot::MarginBlockEnd, margin)
    }
    pub fn margin_inline_start(self, margin: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::MarginInlineStart, margin)
    }
    pub fn margin_inline_end(self, margin: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::MarginInlineEnd, margin)
    }
    pub fn margin_block_start(self, margin: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::MarginBlockStart, margin)
    }
    pub fn margin_block_end(self, margin: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::MarginBlockEnd, margin)
    }
    pub fn flex_grow(mut self, grow: impl IntoF32Raw) -> Self {
        self.style.flex_grow = grow.into_f32_raw();
        // RSX `<Input flex_grow="1">` lands here after `InputBuilder` already set
        // `w(100%)`. Same contract as [`crate::components::input::InputBuilder::flex_grow`]:
        // opting into grow shares a wrap row instead of forcing a full-width line
        // of `overflow:hidden` siblings (gallery forms Placeholder–Search).
        if self.input_paint_sync.is_some() {
            // `flex: 1 1 0` — wrap uses this basis, not placeholder/value max-content.
            self = self.w("auto").min_w("0").flex_basis(0.0);
        }
        self
    }
    pub fn flex_shrink(mut self, shrink: impl IntoF32Raw) -> Self {
        self.style.flex_shrink = shrink.into_f32_raw();
        self
    }

    /// CSS `box-sizing: border-box`（默认）：`width`/`height` 含 padding + border。
    /// 与 Tailwind `box-border` 同义；从 ContentBox 切回时显式写出。
    pub fn border_box(mut self) -> Self {
        self.style.box_sizing = taffy::style::BoxSizing::BorderBox;
        self
    }

    /// CSS `box-sizing: content-box`：测宽为内容宽，padding 加在外侧。
    /// 文案叶若要自带水平 padding 并 `text-align:center`，必须用这个（默认 BorderBox
    /// 会把测宽当成边框宽，padding 吃掉内容区）。
    pub fn content_box(mut self) -> Self {
        self.style.box_sizing = taffy::style::BoxSizing::ContentBox;
        self
    }

    pub fn display_grid(mut self) -> Self {
        self.style.display = Display::Grid;
        self
    }

    pub fn display_flex(mut self) -> Self {
        self.style.display = Display::Flex;
        self
    }

    /// `flex-basis`（主轴初始尺寸）。
    pub fn flex_basis(self, basis: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::FlexBasis, basis)
    }

    /// `align-self`，与 Tailwind `self-*` 一致；`auto` / 未知关键字清除为继承父级 `align-items`。
    pub fn align_self_kw(mut self, keyword: impl AsRef<str>) -> Self {
        let s = keyword.as_ref().trim().to_ascii_lowercase();
        self.style.align_self = match s.as_str() {
            "" | "auto" => None,
            "center" => Some(AlignSelf::CENTER),
            "start" | "flex-start" => Some(AlignSelf::FLEX_START),
            "end" | "flex-end" => Some(AlignSelf::FLEX_END),
            "stretch" => Some(AlignSelf::STRETCH),
            "baseline" => Some(AlignSelf::BASELINE),
            _ => None,
        };
        self
    }

    /// `align-content`（多行 flex / grid 轴上分布）。
    pub fn align_content_kw(mut self, keyword: impl AsRef<str>) -> Self {
        let s = keyword.as_ref().trim().to_ascii_lowercase();
        self.style.align_content = Some(match s.as_str() {
            "start" | "flex-start" => AlignContent::FLEX_START,
            "end" | "flex-end" => AlignContent::FLEX_END,
            "center" => AlignContent::CENTER,
            "stretch" => AlignContent::STRETCH,
            "space-between" | "between" => AlignContent::SPACE_BETWEEN,
            "space-around" | "around" => AlignContent::SPACE_AROUND,
            "space-evenly" | "evenly" => AlignContent::SPACE_EVENLY,
            _ => AlignContent::STRETCH,
        });
        self
    }

    /// Grid 容器 `justify-items`。
    pub fn justify_items_kw(mut self, keyword: impl AsRef<str>) -> Self {
        let s = keyword.as_ref().trim().to_ascii_lowercase();
        self.style.justify_items = Some(match s.as_str() {
            "center" => AlignItems::CENTER,
            "start" | "flex-start" => AlignItems::FLEX_START,
            "end" | "flex-end" => AlignItems::FLEX_END,
            "stretch" => AlignItems::STRETCH,
            "baseline" => AlignItems::BASELINE,
            _ => AlignItems::STRETCH,
        });
        self
    }

    /// Grid 子项 `justify-self`；`auto` / 未知清除为继承父级 `justify-items`。
    pub fn justify_self_kw(mut self, keyword: impl AsRef<str>) -> Self {
        let s = keyword.as_ref().trim().to_ascii_lowercase();
        self.style.justify_self = match s.as_str() {
            "" | "auto" => None,
            "center" => Some(AlignSelf::CENTER),
            "start" | "flex-start" => Some(AlignSelf::FLEX_START),
            "end" | "flex-end" => Some(AlignSelf::FLEX_END),
            "stretch" => Some(AlignSelf::STRETCH),
            "baseline" => Some(AlignSelf::BASELINE),
            _ => None,
        };
        self
    }

    pub fn grid_template_columns(mut self, tracks: Vec<crate::layout::GridTrack>) -> Self {
        self.style.grid_template_columns = tracks;
        self
    }

    pub fn grid_template_rows(mut self, tracks: Vec<crate::layout::GridTrack>) -> Self {
        self.style.grid_template_rows = tracks;
        self
    }

    /// `grid-cols-{n}`：等分 `fr` 列，并启用 `display: grid`。
    pub fn grid_cols_equal(self, n: u16) -> Self {
        self.display_grid()
            .grid_template_columns(crate::layout::cols_equal_fr(n))
    }

    /// `grid-rows-{n}`：等分 `fr` 行，并启用 `display: grid`。
    pub fn grid_rows_equal(self, n: u16) -> Self {
        self.display_grid()
            .grid_template_rows(crate::layout::cols_equal_fr(n))
    }

    /// 展开宏打成的 `jadix_fragment` 多子节点；否则保留 0/1 个根。
    pub fn unpack_fragment_children(children: Option<Element>) -> Vec<Element> {
        match children {
            Some(mut c) if c.attrs.get("jadix_fragment").map(|s| s.as_str()) == Some("1") => {
                std::mem::take(&mut c.children)
            }
            Some(c) => vec![c],
            None => Vec::new(),
        }
    }

    pub fn grid_auto_flow(mut self, flow: GridAutoFlow) -> Self {
        self.style.grid_auto_flow = flow;
        self
    }

    pub fn grid_row(mut self, placement: Line<GridPlacement>) -> Self {
        self.style.grid_row = placement;
        self
    }

    pub fn grid_column(mut self, placement: Line<GridPlacement>) -> Self {
        self.style.grid_column = placement;
        self
    }

    /// `grid-row: <n>`（单条网格线起始）。
    pub fn grid_row_line(mut self, line_index: i16) -> Self {
        self.style.grid_row = line(line_index);
        self
    }

    /// `grid-column: <n>`。
    pub fn grid_column_line(mut self, line_index: i16) -> Self {
        self.style.grid_column = line(line_index);
        self
    }

    /// `grid-column: span <n>`。
    pub fn grid_column_span(mut self, n: u16) -> Self {
        self.style.grid_column = span(n);
        self
    }

    /// `grid-row: span <n>`。
    pub fn grid_row_span(mut self, n: u16) -> Self {
        self.style.grid_row = span(n);
        self
    }

    /// 列表项拖拽手柄：`focus_id` 供运行时识别；`list_id` 中勿含 `:`。
    pub fn drag_reorder_handle(mut self, list_id: impl AsRef<str>, index: usize) -> Self {
        self.focus_id = Some(crate::utils::drag_reorder::drag_item_focus_id(
            list_id.as_ref(),
            index,
        ));
        self.cursor = Some(CursorKind::Grab.into());
        self
    }

    pub fn absolute(mut self) -> Self {
        self.style.position = Position::Absolute;
        self.style.position_fixed = false;
        self.style.position_sticky = false;
        self
    }
    pub fn relative(mut self) -> Self {
        self.style.position = Position::Relative;
        self.style.position_fixed = false;
        self.style.position_sticky = false;
        self
    }
    /// CSS `position: static` — normal flow, inset properties ignored.
    pub fn static_position(mut self) -> Self {
        self.style.position = Position::Relative;
        self.style.position_fixed = false;
        self.style.position_sticky = false;
        self
    }

    pub fn fixed(mut self) -> Self {
        self.style.position = Position::Absolute;
        self.style.position_fixed = true;
        self.style.position_sticky = false;
        self
    }

    /// CSS `position: sticky` — relative layout + scroll-aware offset clamping.
    pub fn sticky(mut self) -> Self {
        self.style.position = Position::Relative;
        self.style.position_fixed = false;
        self.style.position_sticky = true;
        self
    }

    /// Generic CSS `position` setter: `"static"` | `"relative"` | `"absolute"` | `"fixed"` | `"sticky"`.
    pub fn position(self, value: impl AsRef<str>) -> Self {
        match value.as_ref().trim().to_ascii_lowercase().as_str() {
            "absolute" => self.absolute(),
            "fixed" => self.fixed(),
            "sticky" => self.sticky(),
            "static" => self.static_position(),
            _ => self.relative(),
        }
    }
    /// CSS `aspect-ratio`：数字、`"16/9"` / `"1.5"` / `"auto"` 等（见 [`parse_css_aspect_ratio`]）。
    pub fn aspect_ratio(mut self, ratio: impl IntoAspectRatioValue) -> Self {
        self.style.aspect_ratio = ratio.into_aspect_ratio_value();
        self
    }
    pub fn aspect_ratio_opt(mut self, ratio: Option<f32>) -> Self {
        self.style.aspect_ratio = ratio;
        self
    }
    pub fn rotate(mut self, deg: f32) -> Self {
        self.style.rotate_deg = deg;
        self
    }
    /// CSS `scale` — uniform visual scale (does not affect layout).
    pub fn scale(mut self, scale: f32) -> Self {
        self.style.scale = [scale, scale];
        self
    }
    /// CSS `scale: x y` — per-axis visual scale (does not affect layout).
    pub fn scale_xy(mut self, x: f32, y: f32) -> Self {
        self.style.scale = [x, y];
        self
    }
    /// CSS `scale: x y z` — per-axis visual scale with Z (does not affect layout).
    pub fn scale_xyz(mut self, x: f32, y: f32, z: f32) -> Self {
        self.style.scale = [x, y];
        self.style.scale_z = z;
        self
    }
    /// CSS `transform` property — parse a CSS transform string.
    pub fn css_transform(mut self, s: &str) -> Self {
        self.style.transform = crate::style::parse_transform(s);
        self
    }
    /// Set the CSS `transform-origin` from a CSS string (e.g. `"left top"`, `"50% 50%"`).
    pub fn transform_origin(mut self, s: &str) -> Self {
        self.style.transform_origin = crate::style::parse_transform_origin(s);
        self
    }
    /// Set `transform-origin` by pixel offsets.
    pub fn transform_origin_xy(mut self, x: f32, y: f32) -> Self {
        use crate::layout::dimension::CssDim;
        use crate::style::transform::{TransformOriginX, TransformOriginY};
        self.style.transform_origin.x = TransformOriginX::Length(CssDim::Px(x));
        self.style.transform_origin.y = TransformOriginY::Length(CssDim::Px(y));
        self
    }
    /// CSS `transform-style`: `"flat"` or `"preserve-3d"`.
    pub fn transform_style(mut self, s: &str) -> Self {
        self.style.transform_style = crate::style::parse_transform_style(s);
        self
    }
    /// CSS `transform-box`.
    pub fn transform_box(mut self, s: &str) -> Self {
        self.style.transform_box = crate::style::parse_transform_box(s);
        self
    }
    /// Shorthand: apply `skew(xdeg, ydeg)` as a transform function.
    pub fn skew(mut self, x_deg: f32, y_deg: f32) -> Self {
        use crate::layout::dimension::CssAngle;
        use crate::style::TransformFunction;
        let func = TransformFunction::Skew(CssAngle::Deg(x_deg), Some(CssAngle::Deg(y_deg)));
        self.style.transform.get_or_insert_with(Vec::new).push(func);
        self
    }
    /// Shorthand: `skewX(deg)`.
    pub fn skew_x(mut self, deg: f32) -> Self {
        use crate::layout::dimension::CssAngle;
        use crate::style::TransformFunction;
        self.style
            .transform
            .get_or_insert_with(Vec::new)
            .push(TransformFunction::SkewX(CssAngle::Deg(deg)));
        self
    }
    /// Shorthand: `skewY(deg)`.
    pub fn skew_y(mut self, deg: f32) -> Self {
        use crate::layout::dimension::CssAngle;
        use crate::style::TransformFunction;
        self.style
            .transform
            .get_or_insert_with(Vec::new)
            .push(TransformFunction::SkewY(CssAngle::Deg(deg)));
        self
    }
    /// Shorthand: `perspective(px)`.
    pub fn perspective(mut self, px: f32) -> Self {
        use crate::layout::dimension::CssDim;
        use crate::style::TransformFunction;
        self.style
            .transform
            .get_or_insert_with(Vec::new)
            .push(TransformFunction::Perspective(CssDim::Px(px)));
        self
    }
    /// Set a raw `matrix(a,b,c,d,e,f)` transform.
    pub fn matrix(mut self, a: f32, b: f32, c: f32, d: f32, e: f32, f: f32) -> Self {
        use crate::style::TransformFunction;
        self.style
            .transform
            .get_or_insert_with(Vec::new)
            .push(TransformFunction::Matrix([a, b, c, d, e, f]));
        self
    }
    /// CSS `zoom` — magnification factor that **affects layout**.
    pub fn zoom(mut self, factor: f32) -> Self {
        self.style.zoom = factor.max(0.0);
        self
    }
    pub fn linear_gradient(mut self, from: [f32; 4], to: [f32; 4], angle_deg: f32) -> Self {
        self.style.bg_gradient = Some(crate::style::Gradient::Linear(
            crate::style::LinearGradient::two_stop(from, to, angle_deg),
        ));
        self
    }

    /// CSS gradient image: `linear-gradient(...)` / `radial-gradient(...)` /
    /// `conic-gradient(...)` / `repeating-*` (see MDN Using CSS gradients).
    pub fn background_image(mut self, css: impl AsRef<str>) -> Self {
        if let Some(g) = crate::style::parse_gradient(css.as_ref()) {
            self.style.bg_gradient = Some(g);
        }
        self
    }

    /// Set a parsed [`crate::style::Gradient`] as the background fill.
    pub fn gradient(mut self, grad: crate::style::Gradient) -> Self {
        self.style.bg_gradient = Some(grad);
        self
    }
    pub fn text_spans(mut self, spans: Vec<crate::style::TextSpan>) -> Self {
        let concat: String = spans.iter().map(|s| s.text.as_str()).collect();
        self.text_spans = Some(spans);
        self.text = Some(concat);
        self.style.flex_shrink = 0.0;
        self
    }

    /// Parse markdown-inspired inline markup into [`Self::text_spans`].
    pub fn markdown_inline(self, src: impl AsRef<str>) -> Self {
        self.text_spans(crate::text::parse_markdown_inline(src.as_ref()))
    }
    pub fn text_direction(mut self, dir: crate::style::TextDirection) -> Self {
        self.text_style.direction = dir;
        self.style.direction = dir;
        self.style.direction_specified = !matches!(dir, crate::style::TextDirection::Auto);
        self
    }

    /// CSS [`writing-mode`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/writing-mode).
    pub fn writing_mode(mut self, mode: impl AsRef<str>) -> Self {
        if let Some(wm) = crate::style::WritingMode::parse(mode.as_ref()) {
            self.style.writing_mode = wm;
            self.style.writing_mode_specified = true;
        }
        self
    }
    pub fn letter_spacing(mut self, spacing: f32) -> Self {
        self.text_style.letter_spacing = spacing;
        self.text_style.letter_spacing_specified = true;
        self
    }
    pub fn top(self, val: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::InsetT, val)
    }
    pub fn right(self, val: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::InsetR, val)
    }
    pub fn bottom(self, val: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::InsetB, val)
    }
    pub fn left(self, val: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::InsetL, val)
    }
    pub fn inset_inline_start(self, val: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::InsetInlineStart, val)
    }
    pub fn inset_inline_end(self, val: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::InsetInlineEnd, val)
    }
    pub fn inset(self, val: impl IntoCssDim) -> Self {
        let d = val.into_css_dim();
        self.set_css(UnitSlot::InsetL, d.clone())
            .set_css(UnitSlot::InsetR, d.clone())
            .set_css(UnitSlot::InsetT, d.clone())
            .set_css(UnitSlot::InsetB, d)
    }
    pub fn inset_inline(self, val: impl IntoCssRectPair) -> Self {
        self.apply_rect_pair(UnitSlot::InsetInlineStart, UnitSlot::InsetInlineEnd, val)
    }
    pub fn inset_block(self, val: impl IntoCssRectPair) -> Self {
        self.apply_rect_pair(UnitSlot::InsetBlockStart, UnitSlot::InsetBlockEnd, val)
    }
    pub fn z_index(mut self, z: i32) -> Self {
        self.style.z_index = z;
        self
    }
    pub fn inset_rect(mut self, rect: Rect<LengthPercentageAuto>) -> Self {
        self.style.inset = rect;
        self
    }
}

/// Last-frame layout box is fully outside the window (plus pad for shadows).
/// Unlaid-out / zero-size / fixed / portal roots return false so we still walk them.
#[inline]
pub(crate) fn element_is_offscreen(
    el: &Element,
    scroll_acc: [f32; 2],
    viewport_w: f32,
    viewport_h: f32,
) -> bool {
    if el.style.position_fixed || el.template_id.as_deref() == Some("__jadix_portal_layer") {
        return false;
    }
    let b = el.layout_bounds;
    if b[2] <= 0.0 || b[3] <= 0.0 || viewport_w <= 0.0 || viewport_h <= 0.0 {
        return false;
    }
    const PAD: f32 = 32.0;
    let vx = b[0] - scroll_acc[0] + el.sticky_visual_offset[0];
    let vy = b[1] - scroll_acc[1] + el.sticky_visual_offset[1];
    vx + b[2] < -PAD || vy + b[3] < -PAD || vx > viewport_w + PAD || vy > viewport_h + PAD
}

/// Focused `input:` / `textarea:` wrap, or a leaf that still carries `input_caret`.
///
/// Fine-grained clip-cull / offscreen paint-sync skip used to drop this subtree.
/// Losing the caret from `text_nodes` clears `caret_wake_at` and the event loop
/// goes to bare `Wait` — animations and later clicks sit in state until a scroll
/// or blank click pumps a frame (gallery forms: first field not fully visible).
pub(crate) fn subtree_holds_live_text_edit(el: &Element, focused: &str) -> bool {
    fn walk(el: &Element, focused: &str) -> bool {
        if el.text_style.input_caret.is_some() {
            return true;
        }
        if !focused.is_empty()
            && crate::utils::text_field::text_edit_rest(focused).is_some()
            && el.focus_id.as_deref() == Some(focused)
        {
            return true;
        }
        el.children.iter().any(|c| walk(c, focused))
    }
    walk(el, focused)
}

/// Axis-aligned point-in-rect (`[x, y, w, h]`).
#[inline]
pub(crate) fn point_in_aabb(pos: [f32; 2], rect: [f32; 4]) -> bool {
    pos[0] >= rect[0]
        && pos[1] >= rect[1]
        && pos[0] < rect[0] + rect[2]
        && pos[1] < rect[1] + rect[3]
}

/// Paint vis box + center-pivot affine (same contract as collect / VS).
///
/// `ancestor_*` is either last-collect stamps or a live walk product. Hit
/// testing prefers the walk so a parent rotate this frame is visible before collect.
struct PaintVis {
    vx: f32,
    vy: f32,
    vis: [f32; 4],
    affine: crate::style::Mat2d,
    paint_sx: f32,
    paint_sy: f32,
    origin: [f32; 2],
}

fn resolved_style_scale(el: &Element) -> [f32; 2] {
    let scale = crate::motion::presentation_scale(el);
    let sx = if scale[0].is_finite() && scale[0] > 0.0 {
        scale[0]
    } else {
        1.0
    };
    let sy = if scale[1].is_finite() && scale[1] > 0.0 {
        scale[1]
    } else {
        1.0
    };
    [sx, sy]
}

fn resolved_ancestor_scale(ancestor_scale: [f32; 2]) -> [f32; 2] {
    let asx = if ancestor_scale[0].is_finite() && ancestor_scale[0] > 0.0 {
        ancestor_scale[0]
    } else {
        1.0
    };
    let asy = if ancestor_scale[1].is_finite() && ancestor_scale[1] > 0.0 {
        ancestor_scale[1]
    } else {
        1.0
    };
    [asx, asy]
}

#[inline]
pub(crate) fn paint_scale_with_ancestor(el: &Element, ancestor_scale: [f32; 2]) -> [f32; 2] {
    let s = resolved_style_scale(el);
    let a = resolved_ancestor_scale(ancestor_scale);
    [
        (a[0] * s[0]).clamp(0.01, 8.0),
        (a[1] * s[1]).clamp(0.01, 8.0),
    ]
}

/// Parent → child paint cascade (same as collect `child_ancestor_linear/scale`).
#[inline]
pub(crate) fn child_paint_ancestor(
    parent: &Element,
    ancestor_linear: crate::style::Mat2d,
    ancestor_scale: [f32; 2],
) -> (crate::style::Mat2d, [f32; 2]) {
    let paint_s = paint_scale_with_ancestor(parent, ancestor_scale);
    let b = parent.layout_bounds;
    let vis_wh = [b[2] * paint_s[0], b[3] * paint_s[1]];
    let offset = crate::style::offset_path::sample(&parent.style, [b[2], b[3]]);
    let local_unwrapped = crate::style::transform::rotate_and_list_mat2d_in_paint_space(
        parent.style.rotate_deg + offset.rotate_deg,
        parent.style.transform.as_ref(),
        vis_wh[0],
        vis_wh[1],
        ancestor_scale,
    );
    let local_linear = crate::style::mat2d_linear(&local_unwrapped);
    (
        crate::style::mat2d_multiply(&ancestor_linear, &local_linear),
        paint_s,
    )
}

fn paint_vis(
    el: &Element,
    scroll_acc: [f32; 2],
    ancestor_linear: crate::style::Mat2d,
    ancestor_scale: [f32; 2],
) -> PaintVis {
    let b = crate::renderer::emoji_layout::layout_bounds_with_visible_emoji_ink(
        el.layout_bounds,
        el.text.as_deref(),
        el.text_style.size_resolved(),
        el.style.overflow.x == taffy::style::Overflow::Visible,
        el.style.overflow.y == taffy::style::Overflow::Visible,
    );
    let offset = crate::style::offset_path::sample(&el.style, [b[2], b[3]]);
    let translate = crate::motion::presentation_translate_in_paint_space(el, ancestor_scale);
    let tx = translate[0] + (b[2] * el.style.translate_pct[0] + offset.translate[0]) * ancestor_scale[0];
    let ty = translate[1] + (b[3] * el.style.translate_pct[1] + offset.translate[1]) * ancestor_scale[1];
    let vx = b[0] - scroll_acc[0] + el.sticky_visual_offset[0] + tx;
    let vy = b[1] - scroll_acc[1] + el.sticky_visual_offset[1] + ty;
    let [paint_sx, paint_sy] = paint_scale_with_ancestor(el, ancestor_scale);
    let rotate_deg = el.style.rotate_deg + offset.rotate_deg;
    let has_scale = (paint_sx - 1.0).abs() > f32::EPSILON || (paint_sy - 1.0).abs() > f32::EPSILON;
    let has_linear = rotate_deg.abs() > 1e-4
        || el
            .style
            .transform
            .as_ref()
            .is_some_and(|list| !list.is_empty())
        || !crate::style::mat2d_is_identity(&ancestor_linear);
    let origin = crate::style::origin_px(&el.style.transform_origin, b[2], b[3]);
    if !has_scale && !has_linear {
        return PaintVis {
            vx,
            vy,
            vis: [vx, vy, b[2], b[3]],
            affine: crate::style::MAT2D_IDENTITY,
            paint_sx,
            paint_sy,
            origin,
        };
    }
    let vis = if has_scale {
        crate::style::scale_box_about_origin([vx, vy, b[2], b[3]], origin, [paint_sx, paint_sy])
    } else {
        [vx, vy, b[2], b[3]]
    };
    if !has_linear {
        return PaintVis {
            vx,
            vy,
            vis,
            affine: crate::style::MAT2D_IDENTITY,
            paint_sx,
            paint_sy,
            origin,
        };
    }
    let origin_in_vis = [origin[0] - (vis[0] - vx), origin[1] - (vis[1] - vy)];
    let local_unwrapped = crate::style::transform::rotate_and_list_mat2d_in_paint_space(
        rotate_deg,
        el.style.transform.as_ref(),
        vis[2],
        vis[3],
        ancestor_scale,
    );
    let local_linear = crate::style::mat2d_linear(&local_unwrapped);
    let list_t = [local_unwrapped[4], local_unwrapped[5]];
    let has_list_t = list_t[0].abs() > 1e-4 || list_t[1].abs() > 1e-4;
    if crate::style::mat2d_is_identity(&local_linear)
        && crate::style::mat2d_is_identity(&ancestor_linear)
        && !has_list_t
    {
        return PaintVis {
            vx,
            vy,
            vis,
            affine: crate::style::MAT2D_IDENTITY,
            paint_sx,
            paint_sy,
            origin,
        };
    }
    let affine = crate::style::mat2d_world_center_pivot_with_shift(
        &ancestor_linear,
        &local_linear,
        origin_in_vis,
        [vis[2], vis[3]],
        list_t,
    );
    PaintVis {
        vx,
        vy,
        vis,
        affine,
        paint_sx,
        paint_sy,
        origin,
    }
}

/// Paint-space border box after CSS `translate` + `scale` + rotate about origin.
///
/// Uses last-collect ancestor stamps. Pointer hit walks should call
/// [`visual_paint_aabb_with_ancestor`] with a live cascade so a parent rotate
/// this frame is visible before collect.
#[inline]
pub(crate) fn visual_paint_aabb(el: &Element, scroll_acc: [f32; 2]) -> [f32; 4] {
    visual_paint_aabb_with_ancestor(
        el,
        scroll_acc,
        el.paint_ancestor_linear,
        el.paint_ancestor_scale,
    )
}

pub(crate) fn rest_paint_aabb(el: &Element, scroll_acc: [f32; 2]) -> [f32; 4] {
    let bounds = el.layout_bounds;
    let origin = crate::style::origin_px(&el.style.transform_origin, bounds[2], bounds[3]);
    let rest = [
        bounds[0] - scroll_acc[0],
        bounds[1] - scroll_acc[1],
        bounds[2].max(0.0),
        bounds[3].max(0.0),
    ];
    let visual = crate::style::scale_box_about_origin(
        rest,
        origin,
        resolved_ancestor_scale(el.paint_ancestor_scale),
    );
    if crate::style::mat2d_is_identity(&el.paint_ancestor_linear) {
        return visual;
    }
    let transform = crate::style::mat2d_world_center_pivot_with_shift(
        &el.paint_ancestor_linear,
        &crate::style::MAT2D_IDENTITY,
        [origin[0] - (visual[0] - rest[0]), origin[1] - (visual[1] - rest[1])],
        [visual[2], visual[3]],
        [0.0; 2],
    );
    crate::style::aabb_of_centered_transform(visual, &transform)
}

#[inline]
pub(crate) fn visual_paint_aabb_with_ancestor(
    el: &Element,
    scroll_acc: [f32; 2],
    ancestor_linear: crate::style::Mat2d,
    ancestor_scale: [f32; 2],
) -> [f32; 4] {
    let g = paint_vis(el, scroll_acc, ancestor_linear, ancestor_scale);
    if crate::style::mat2d_is_identity(&g.affine) {
        return g.vis;
    }
    crate::style::aabb_of_centered_transform(g.vis, &g.affine)
}

/// Map a world pointer into layout-local path space (`path_pointer_hit` / SVG fill).
///
/// Inverts the VS contract `center + R*local + txty`, then undoes origin scale
/// bake. Identity transform → `(world - visual_xy)`.
#[inline]
pub(crate) fn pointer_to_layout_local(
    el: &Element,
    world: [f32; 2],
    scroll_acc: [f32; 2],
    ancestor_linear: crate::style::Mat2d,
    ancestor_scale: [f32; 2],
) -> [f32; 2] {
    let g = paint_vis(el, scroll_acc, ancestor_linear, ancestor_scale);
    let vis = g.vis;
    let cx = vis[0] + vis[2] * 0.5;
    let cy = vis[1] + vis[3] * 0.5;
    let dx = world[0] - cx - g.affine[4];
    let dy = world[1] - cy - g.affine[5];
    let inv = crate::style::mat2d_inverse(&crate::style::mat2d_linear(&g.affine));
    let (lx, ly) = crate::style::mat2d_transform_vec(&inv, dx, dy);
    let vis_pt = [cx + lx, cy + ly];
    let origin_world = [g.vx + g.origin[0], g.vy + g.origin[1]];
    let sx = if g.paint_sx.abs() > 1e-6 {
        g.paint_sx
    } else {
        1.0
    };
    let sy = if g.paint_sy.abs() > 1e-6 {
        g.paint_sy
    } else {
        1.0
    };
    let layout_x = origin_world[0] + (vis_pt[0] - origin_world[0]) / sx;
    let layout_y = origin_world[1] + (vis_pt[1] - origin_world[1]) / sy;
    [layout_x - g.vx, layout_y - g.vy]
}

/// Inverse of [`pointer_to_layout_local`]: layout-local → painted world (VS).
#[inline]
pub(crate) fn layout_local_to_paint_world(
    el: &Element,
    local: [f32; 2],
    scroll_acc: [f32; 2],
    ancestor_linear: crate::style::Mat2d,
    ancestor_scale: [f32; 2],
) -> [f32; 2] {
    let g = paint_vis(el, scroll_acc, ancestor_linear, ancestor_scale);
    let vis = g.vis;
    let cx = vis[0] + vis[2] * 0.5;
    let cy = vis[1] + vis[3] * 0.5;
    let origin_world = [g.vx + g.origin[0], g.vy + g.origin[1]];
    let layout_world = [g.vx + local[0], g.vy + local[1]];
    let vis_pt = [
        origin_world[0] + (layout_world[0] - origin_world[0]) * g.paint_sx,
        origin_world[1] + (layout_world[1] - origin_world[1]) * g.paint_sy,
    ];
    let dx = vis_pt[0] - cx;
    let dy = vis_pt[1] - cy;
    let lin = crate::style::mat2d_linear(&g.affine);
    let (rx, ry) = crate::style::mat2d_transform_vec(&lin, dx, dy);
    [cx + rx + g.affine[4], cy + ry + g.affine[5]]
}

/// `clip-path` is specified in the border box. Hit tests the painted clip by
/// inverting the VS affine first.
#[inline]
pub(crate) fn clip_path_contains_pointer(
    el: &Element,
    world: [f32; 2],
    scroll_acc: [f32; 2],
    ancestor_linear: crate::style::Mat2d,
    ancestor_scale: [f32; 2],
) -> bool {
    if el.style.clip_path.is_none() {
        return true;
    }
    let local = pointer_to_layout_local(el, world, scroll_acc, ancestor_linear, ancestor_scale);
    let b = el.layout_bounds;
    el.style.clip_path.contains_local([b[2], b[3]], local)
}

/// Painted border box: coarse VS AABB, then layout-local inside `[0,w]×[0,h]`.
#[inline]
pub(crate) fn pointer_in_paint_border_box(
    el: &Element,
    world: [f32; 2],
    scroll_acc: [f32; 2],
    ancestor_linear: crate::style::Mat2d,
    ancestor_scale: [f32; 2],
) -> bool {
    let vis = visual_paint_aabb_with_ancestor(el, scroll_acc, ancestor_linear, ancestor_scale);
    if !(world[0] >= vis[0]
        && world[1] >= vis[1]
        && world[0] <= vis[0] + vis[2]
        && world[1] <= vis[1] + vis[3])
    {
        return false;
    }
    let local = pointer_to_layout_local(el, world, scroll_acc, ancestor_linear, ancestor_scale);
    // `pointer_to_layout_local` is origin'd on the paint-vis box (overflow-visible
    // emoji ink is inflated past the leading-none strut). Testing against the
    // uninflated Taffy size drops :hover on the face — chip hover:scale never
    // enters and the glyph stays at rest.
    let b = crate::renderer::emoji_layout::layout_bounds_with_visible_emoji_ink(
        el.layout_bounds,
        el.text.as_deref(),
        el.text_style.size_resolved(),
        el.style.overflow.x == taffy::style::Overflow::Visible,
        el.style.overflow.y == taffy::style::Overflow::Visible,
    );
    local[0] >= -0.5 && local[1] >= -0.5 && local[0] <= b[2] + 0.5 && local[1] <= b[3] + 0.5
}

/// World pointer as glyphon/layout-box `(vx+lx, vy+ly)` after inverting the VS affine.
#[inline]
pub(crate) fn pointer_as_layout_box_xy(
    el: &Element,
    world: [f32; 2],
    scroll_acc: [f32; 2],
    ancestor_linear: crate::style::Mat2d,
    ancestor_scale: [f32; 2],
) -> [f32; 2] {
    let g = paint_vis(el, scroll_acc, ancestor_linear, ancestor_scale);
    let local = pointer_to_layout_local(el, world, scroll_acc, ancestor_linear, ancestor_scale);
    [g.vx + local[0], g.vy + local[1]]
}

/// Axes that clip pointer / paint overflow to the border box.
///
/// Per-axis `hidden`/`clip` stays independent (`overflow-x: hidden; overflow-y: visible`
/// for `text-overflow` must not scissor glyphs to the line box). A **scrollport**
/// (`auto`/`scroll`, or a `scroll:` id whose overflow was cleared to `Visible`) clips
/// both axes — CSS scroll containers clip on both axes, and gallery `scroll:` shells
/// must not leak hits into the sidebar.
#[inline]
pub(crate) fn overflow_clip_axes(el: &Element) -> (bool, bool) {
    let is_scroll_viewport = el
        .focus_id
        .as_deref()
        .is_some_and(|f| f.starts_with(crate::core::scroll_viewport::SCROLL_VIEWPORT_FOCUS_PREFIX));
    let (clip_x, clip_y) =
        overflow_clip_axes_from(el.style.overflow.x, el.style.overflow.y, is_scroll_viewport);
    // CSS scroll containers clip both axes. `overflow-x: auto` is Taffy Scroll, so
    // the generic rule invents a Y clip. table-container is content-height — that
    // Y clip dest-resets / culls invoice rows inside gallery `scroll-data`.
    if !is_scroll_viewport
        && matches!(el.style.overflow.y, taffy::style::Overflow::Visible)
        && el.attrs.get("data-slot").map(|s| s.as_str()) == Some("table-container")
    {
        return (clip_x, false);
    }
    (clip_x, clip_y)
}

#[inline]
pub(crate) fn overflow_clip_axes_from(
    overflow_x: taffy::style::Overflow,
    overflow_y: taffy::style::Overflow,
    is_scroll_viewport: bool,
) -> (bool, bool) {
    use taffy::style::Overflow::{Scroll, Visible};
    let ox = overflow_x != Visible;
    let oy = overflow_y != Visible;
    // CSS `overflow: auto` is stored as Taffy `Scroll`.
    let scrollport =
        is_scroll_viewport || matches!(overflow_x, Scroll) || matches!(overflow_y, Scroll);
    if scrollport {
        (true, true)
    } else {
        (ox, oy)
    }
}

/// Overflow / `scroll:` viewports clip hit-testing and class `:hover` to the layout border box.
///
/// `scroll:` ids still clip when overflow was cleared to `Visible` (ScrollArea outer shells and
/// some viewport rebuilds). Without this, scrolled content can absorb hits that belong to
/// sibling chrome (gallery sidebar) while the main pane scrolls.
#[inline]
pub(crate) fn element_clips_overflow(el: &Element) -> bool {
    let (clip_x, clip_y) = overflow_clip_axes(el);
    clip_x || clip_y
}

/// Overflow clip is the **local** border box (CSS), then the VS affine is applied.
/// Hit inverts that affine first — same contract as [`clip_path_contains_pointer`].
/// Dest-reset / node-present scissor / B8 pins are paint-space AABB unions of that
/// transformed box, not this local test and not an OBB GPU scissor.
#[inline]
pub(crate) fn overflow_clip_contains_pointer(
    el: &Element,
    world: [f32; 2],
    scroll_acc: [f32; 2],
    ancestor_linear: crate::style::Mat2d,
    ancestor_scale: [f32; 2],
) -> bool {
    if !element_clips_overflow(el) {
        return true;
    }
    let b = el.layout_bounds;
    if b[2] <= 0.0 || b[3] <= 0.0 {
        return false;
    }
    let local = pointer_to_layout_local(el, world, scroll_acc, ancestor_linear, ancestor_scale);
    let (clip_x, clip_y) = overflow_clip_axes(el);
    if clip_x && (local[0] < 0.0 || local[0] > b[2]) {
        return false;
    }
    if clip_y && (local[1] < 0.0 || local[1] > b[3]) {
        return false;
    }
    if clip_x && clip_y {
        overflow_clip_contains_point(el, [0.0, 0.0, b[2], b[3]], local)
    } else {
        true
    }
}

/// Overflow clip hit: border-box AABB, then `border-radius` if any corner is rounded.
/// Prefer [`overflow_clip_contains_pointer`] at pointer enter (local + VS inverse).
#[inline]
pub(crate) fn overflow_clip_contains_point(
    el: &Element,
    box_xywh: [f32; 4],
    world: [f32; 2],
) -> bool {
    crate::style::rounded_rect_contains(box_xywh, el.style.radii, world)
}

#[inline]
pub(crate) fn child_scroll_accumulation(el: &Element, scroll_acc: [f32; 2]) -> [f32; 2] {
    // Match collect / `next_hit_scroll_acc`: `scroll:` viewports still contribute offset
    // when overflow was cleared to Visible.
    let is_scroll_viewport = el
        .focus_id
        .as_deref()
        .is_some_and(|f| f.starts_with(crate::core::scroll_viewport::SCROLL_VIEWPORT_FOCUS_PREFIX));
    let mut next = scroll_acc;
    let scale = paint_scale_with_ancestor(el, el.paint_ancestor_scale);
    if is_scroll_viewport || matches!(el.style.overflow.x, taffy::style::Overflow::Scroll) {
        next[0] += el.scroll_offset[0] * scale[0];
    }
    if is_scroll_viewport || matches!(el.style.overflow.y, taffy::style::Overflow::Scroll) {
        next[1] += el.scroll_offset[1] * scale[1];
    }
    next
}

/// Clip box of the nearest overflow/scroll ancestor of `target` (or `target` itself).
/// Identity overflow stays the layout border (Wave 18). Rotated overflow is the
/// AABB of the transformed local box so island FullClear covers painted pixels.
/// `None` when the island sits in unclipped page space (route Show → page FullClear).
pub(crate) fn overflow_clip_bounds_for(root: &Element, target: *const Element) -> Option<[f32; 4]> {
    fn rec(
        el: &Element,
        clip: Option<[f32; 4]>,
        ancestor_linear: crate::style::Mat2d,
        ancestor_scale: [f32; 2],
        target: *const Element,
    ) -> Option<Option<[f32; 4]>> {
        let clip = if element_clips_overflow(el) {
            let b =
                visual_paint_aabb_with_ancestor(el, [0.0, 0.0], ancestor_linear, ancestor_scale);
            if b[2] > 0.0 && b[3] > 0.0 {
                Some(b)
            } else {
                clip
            }
        } else {
            clip
        };
        if std::ptr::eq(el as *const Element, target) {
            return Some(clip);
        }
        let (child_lin, child_sc) = child_paint_ancestor(el, ancestor_linear, ancestor_scale);
        for child in &el.children {
            if let Some(found) = rec(child, clip, child_lin, child_sc, target) {
                return Some(found);
            }
        }
        None
    }
    rec(root, None, crate::style::MAT2D_IDENTITY, [1.0, 1.0], target).flatten()
}

/// Scratch filled by [`sync_reactive_elements_subtree_ex`] so the frame path can
/// mark specific Taffy nodes dirty (instead of remasuring every text leaf) and
/// distinguish portal-local geometry from page layout.
#[derive(Debug, Default)]
pub(crate) struct ReactiveSyncCollect {
    /// Nodes whose style/measure must be pushed into Taffy after sync.
    pub layout_dirty: Vec<NodeId>,
    /// `LayoutChanged` / page `StructureChanged` under the page tree — FullClear.
    pub page_layout_changed: bool,
    /// `LayoutChanged` or overlay `StructureChanged` under the portal layer / a
    /// `jadix_portal` subtree (bubble +N expand) — not a page FullClear.
    pub portal_layout_changed: bool,
    /// `display: none` → shown while the leaf still has no visual box.
    /// The ancestor clip is the scrollport; damage the laid-out box after layout.
    pub display_reveal_keys: Vec<u64>,
}

/// Run generic component-owned reactive synchronizers for the mounted tree.
///
/// Prefer [`sync_reactive_elements_subtree_ex`] on the frame path; this wrapper is
/// kept for crate tests / `test_support`.
#[allow(dead_code)]
pub(crate) fn sync_reactive_elements_subtree(
    element: &mut Element,
    layout_tree: &mut LayoutTree,
    impact: &mut LayoutStatus,
    viewport_w: f32,
    viewport_h: f32,
    scroll_acc: [f32; 2],
) {
    let mut collect = ReactiveSyncCollect::default();
    sync_reactive_elements_subtree_ex(
        element,
        layout_tree,
        impact,
        viewport_w,
        viewport_h,
        scroll_acc,
        false,
        &mut collect,
    );
}

/// Like [`sync_reactive_elements_subtree`], but records dirty nodes + portal vs page geometry.
pub(crate) fn sync_reactive_elements_subtree_ex(
    element: &mut Element,
    layout_tree: &mut LayoutTree,
    impact: &mut LayoutStatus,
    viewport_w: f32,
    viewport_h: f32,
    scroll_acc: [f32; 2],
    in_portal: bool,
    collect: &mut ReactiveSyncCollect,
) {
    let root_ptr = element as *mut Element;
    // DFS siblings share their ancestor spine. Store depth/index in the work
    // stack instead of allocating and copying a complete path for every node.
    let mut path = Vec::new();
    let mut stack: Vec<(
        *mut Element,
        [f32; 2],
        bool,
        Option<[f32; 4]>,
        usize,
        Option<usize>,
    )> = vec![(root_ptr, scroll_acc, in_portal, None, 0, None)];
    while let Some((ptr, scroll_acc, in_portal, clip_bounds, depth, index)) = stack.pop() {
        path.truncate(depth);
        if let Some(index) = index {
            path.push(index);
        }
        // Exclusive node borrow ends before any root-wide hash bust (overlapping
        // `&mut` through `root_ptr` would alias `ptr` when `path` reaches this node).
        let (child_acc, in_portal, clip_bounds, child_count, paint_changed) = {
            // SAFETY: exclusive subtree; children pointers are taken after this node's sync.
            let element = unsafe { &mut *ptr };
            if element.collect_paint_hash != 0 && !element.subtree_reactive_sync {
                continue;
            }
            // Reactive paint synchronizers can move a floating panel during the
            // same frame as the pointer event. Its retained layout box may still
            // contain the previous offscreen position, so skipping it here leaves
            // controls such as the ColorPicker SV knob one frame behind. Run the
            // small synchronizer once; ordinary subtree work remains culled.
            if element_is_offscreen(element, scroll_acc, viewport_w, viewport_h)
                && element.reactive_sync.is_none()
            {
                continue;
            }
            let in_portal = in_portal
                || element.template_id.as_deref()
                    == Some(crate::layout::layout_tree::PORTAL_LAYER_ID);
            let ancestor_clip = clip_bounds;
            let in_clip = clip_bounds.is_some() || element_clips_overflow(element);
            let clip_bounds = if element_clips_overflow(element) {
                let b = visual_paint_aabb(element, scroll_acc);
                if b[2] > 0.0 && b[3] > 0.0 {
                    Some(b)
                } else {
                    clip_bounds
                }
            } else {
                clip_bounds
            };
            let mut paint_changed = false;
            // Clone the Rc-backed descriptor so the callback may mutate its owning Element.
            if let Some(sync) = element.reactive_sync.clone() {
                // Damage and in-callback paints must share this walk's scroll.
                // `paint_scroll_acc` is still the previous collect. InputOTP
                // pushes a focus hole from it, then StructureChanged pushes the
                // live box under the same key; the union is a slot-wide column
                // from the visual row down through the stale document position.
                element.paint_scroll_acc = scroll_acc;
                let before = visual_paint_aabb(element, scroll_acc);
                let before_display = element.style.display;
                let before_opacity = element.style.opacity;
                let is_scroll_viewport = element.focus_id.as_deref().is_some_and(|id| {
                    id.starts_with(crate::core::scroll_viewport::SCROLL_VIEWPORT_FOCUS_PREFIX)
                });
                let before_style = is_scroll_viewport.then(|| element.style.clone());
                let before_scroll_offset = element.scroll_offset;
                let status = sync.run(element, layout_tree);
                paint_changed = status != LayoutStatus::Clean;
                match status {
                    LayoutStatus::LayoutChanged => {
                        push_subtree_layout_nodes(element, &mut collect.layout_dirty);
                        if in_portal {
                            collect.portal_layout_changed = true;
                            crate::app::damage::push_frame_paint_element(element, scroll_acc);
                        } else if layout_is_local_display_toggle(
                            before_display,
                            element.style.display,
                        ) {
                            // `display: none` ↔ shown is a local panel (TabsContent,
                            // Collapsible hide). Page FullClear was the Gallery 8b
                            // ToggleGroup one-frame text flash — not a product slot.
                            if element.style.position == taffy::style::Position::Absolute {
                                crate::app::damage::push_frame_paint_key(
                                    crate::app::damage::paint_element_key(element),
                                    estimate_geom_damage(element, before),
                                );
                            } else {
                                // Island, not the ancestor scrollport. QuestionnaireError
                                // sits in the forms ScrollArea; that clip is the whole page.
                                // Recording it under the error leaf clears every caption
                                // above. A box that is already shown is damaged now. A
                                // reveal still has a 0×0 box — layout fills it, then
                                // [`push_display_reveal_regions`] names that box.
                                let own = estimate_geom_damage(element, before);
                                if own[2] > 0.5 && own[3] > 0.5 {
                                    let d = match clip_bounds {
                                        Some(clip) => intersect_damage(own, clip).unwrap_or(own),
                                        None => own,
                                    };
                                    if d[2] > 0.5 && d[3] > 0.5 {
                                        crate::app::damage::push_frame_region_paint(
                                            crate::app::damage::paint_element_key(element),
                                            d,
                                        );
                                    }
                                } else if element.layout_node.is_some() {
                                    collect
                                        .display_reveal_keys
                                        .push(crate::app::damage::paint_element_key(element));
                                }
                            }
                        } else {
                            collect.page_layout_changed = true;
                        }
                    }
                    LayoutStatus::StructureChanged => {
                        // Page remount (route / Show) → FullClear. Overlay Show-mount
                        // (bubble +N / popover) is portal membership — scissor to the hoisted box.
                        // Nested For inside a scroll/overflow clip (MessageScroller Send) is
                        // island-local: do not mark page geometry or the sidebar Footer is
                        // on the FullClear present path.
                        let portalish = in_portal
                            || (crate::layout::layout_tree::subtree_declares_portal(element)
                                && crate::layout::layout_tree::subtree_is_portal_only(element));
                        if portalish {
                            collect.portal_layout_changed = true;
                            crate::app::damage::push_frame_paint_damage(estimate_geom_damage(
                                element, before,
                            ));
                        } else if in_clip && element.attrs.remove("jadix_halo_dirty").is_some() {
                            // Focus-ring outset is already a Node dest-reset. A
                            // layout-box Region window-clears and page-band-replays;
                            // that composite paints the ring stroke off the cell
                            // (grouped OTP type/delete). Layout still rebuilds below.
                        } else if in_clip {
                            // Island, not the ancestor scrollport. InputOTP's caret is
                            // StructureChanged on a 36px slot inside the page scroller.
                            // Recording that scroller under the slot key clears every
                            // other field and own-chrome redraws only the slot.
                            // A list taller than the port still intersects down to the
                            // visible island. An unlaid node keeps the clip.
                            let own = estimate_geom_damage(element, before);
                            let d = match clip_bounds {
                                Some(clip) if own[2] > 0.5 && own[3] > 0.5 => {
                                    intersect_damage(own, clip)
                                }
                                Some(clip) => Some(clip),
                                None => Some(own),
                            };
                            if let Some(d) = d {
                                if d[2] > 0.0 && d[3] > 0.0 {
                                    crate::app::damage::push_frame_region_paint(
                                        crate::app::damage::paint_element_key(element),
                                        d,
                                    );
                                }
                            }
                        } else {
                            collect.page_layout_changed = true;
                        }
                        push_subtree_layout_nodes(element, &mut collect.layout_dirty);
                    }
                    LayoutStatus::StyleChanged => {
                        // Same as bind_text paint-only: StyleChanged is a local dest-reset
                        // + recapture, not a layout rebuild. Leaving this a no-op kept
                        // previous ink in PartialLoad (in-page stacking chrome hide/hover).
                        // Halo-only (InputAdornment blur): strips already queued —
                        // a second full-box Node wipe flashes placeholder + ⌘K.
                        // A scroll viewport also reports StyleChanged while its reactive
                        // synchronizer only advances `scroll_offset` and bookkeeping
                        // attributes. The scroll-layer clear/blit path owns that movement;
                        // emitting the viewport's full visual box here clears sibling page
                        // text and creates the white rectangle seen during Latest entry.
                        let scroll_only = is_scroll_viewport
                            && element.scroll_offset != before_scroll_offset
                            && before_style
                                .as_ref()
                                .is_some_and(|style| *style == element.style);
                        let halo_dirty = element.attrs.remove("jadix_halo_dirty").is_some();
                        if !scroll_only && !halo_dirty {
                            crate::app::damage::push_frame_paint_element_change_from_before(
                                element,
                                scroll_acc,
                                before_opacity,
                                Some(before),
                                ancestor_clip,
                            );
                        }
                    }
                    LayoutStatus::Clean => {}
                }
                *impact = impact.merge(status);
            }
            let child_acc = child_scroll_accumulation(element, scroll_acc);
            let child_count = element.children.len();
            (
                child_acc,
                in_portal,
                clip_bounds,
                child_count,
                paint_changed,
            )
        };
        if paint_changed {
            // ViewportsOnly `reuse_idle` would skip-replay idle ancestors (Gallery
            // InputAdornment Hard ring after scroll settle: placeholder flicker only).
            // Same spine bust as bind StyleChanged / Item variant.
            // SAFETY: exclusive sync root; node borrow above has ended.
            crate::layout::collect_cache::invalidate_collect_paint_hash_path(
                unsafe { &mut *root_ptr },
                &path,
            );
        }
        // SAFETY: exclusive subtree; push child pointers after hash bust.
        let element = unsafe { &mut *ptr };
        for i in (0..child_count).rev() {
            stack.push((
                std::ptr::addr_of_mut!(element.children[i]),
                child_acc,
                in_portal,
                clip_bounds,
                path.len(),
                Some(i),
            ));
        }
    }
}

/// After layout, damage each `display: none` → shown leaf at its real box.
///
/// Sync cannot see that box (the leaf was 0×0). The ancestor overflow clip is
/// the scrollport and must not be the clear.
pub(crate) fn push_display_reveal_regions(root: &Element, keys: &[u64]) {
    if keys.is_empty() {
        return;
    }
    walk_elements(root, |el| {
        let key = crate::app::damage::paint_element_key(el);
        if !keys.contains(&key) {
            return;
        }
        let b = visual_paint_aabb(el, el.paint_scroll_acc);
        if b[2] > 0.5 && b[3] > 0.5 {
            crate::app::damage::push_frame_region_paint(key, b);
        }
    });
}

fn push_subtree_layout_nodes(el: &Element, out: &mut Vec<NodeId>) {
    walk_elements(el, |node| {
        if let Some(n) = node.layout_node {
            out.push(n);
        }
    });
}

/// `display: none` ↔ shown: dest-reset the panel, do not mark page geometry.
fn layout_is_local_display_toggle(before: Display, after: Display) -> bool {
    before != after && (before == Display::None || after == Display::None)
}

/// Old layout box ∪ authored length size after sync (covers expand and collapse).
fn intersect_damage(a: [f32; 4], b: [f32; 4]) -> Option<[f32; 4]> {
    let x0 = a[0].max(b[0]);
    let y0 = a[1].max(b[1]);
    let x1 = (a[0] + a[2]).min(b[0] + b[2]);
    let y1 = (a[1] + a[3]).min(b[1] + b[3]);
    let w = x1 - x0;
    let h = y1 - y0;
    (w > 0.5 && h > 0.5).then_some([x0, y0, w, h])
}

fn estimate_geom_damage(el: &Element, before: [f32; 4]) -> [f32; 4] {
    let mut d = before;
    if let DimKind::Length(w) = dim_kind(el.style.size.width) {
        if w > 0.0 {
            d[2] = d[2].max(w);
        }
    }
    if let DimKind::Length(h) = dim_kind(el.style.size.height) {
        if h > 0.0 {
            d[3] = d[3].max(h);
        }
    }
    d
}

impl From<String> for Element {
    fn from(s: String) -> Self {
        Element::new().text(s)
    }
}

impl From<&str> for Element {
    fn from(s: &str) -> Self {
        Element::new().text(s)
    }
}

impl From<Vec<Element>> for Element {
    fn from(children: Vec<Element>) -> Self {
        let mut el = Element::new();
        for child in children {
            el = el.child(child);
        }
        el
    }
}

impl From<Option<Element>> for Element {
    fn from(opt: Option<Element>) -> Self {
        opt.unwrap_or_default()
    }
}

#[cfg(test)]
mod p1_builder_tests {
    use super::*;
    use crate::style::{TextDirection, TextSpan};

    #[test]
    fn reactive_sync_keeps_the_correct_spine_across_dfs_siblings() {
        let changed = || Element::new().reactive_sync(|_, _| LayoutStatus::StyleChanged);
        let mut root = Element::new()
            .child(
                Element::new()
                    .child(Element::new().child(changed()))
                    .child(Element::new()),
            )
            .child(Element::new().child(Element::new()))
            .child(changed());
        crate::layout::collect_cache::stamp_subtree_paint_hashes(&mut root, false);
        walk_elements_post_order_mut(&mut root, |el| el.collect_paint_hash = 42);
        let mut tree = LayoutTree::new();
        let mut impact = LayoutStatus::Clean;
        sync_reactive_elements_subtree(&mut root, &mut tree, &mut impact, 400.0, 300.0, [0.0; 2]);
        assert_eq!(impact, LayoutStatus::StyleChanged);
        assert_eq!(root.collect_paint_hash, 0);
        assert_eq!(root.children[0].collect_paint_hash, 0);
        assert_eq!(root.children[0].children[0].collect_paint_hash, 0);
        assert_eq!(
            root.children[0].children[0].children[0].collect_paint_hash,
            0
        );
        assert_eq!(root.children[0].children[1].collect_paint_hash, 42);
        assert_eq!(root.children[1].collect_paint_hash, 42);
        assert_eq!(root.children[1].children[0].collect_paint_hash, 42);
        assert_eq!(root.children[2].collect_paint_hash, 0);
    }

    #[test]
    fn reactive_hover_damage_stays_inside_scrolled_viewport() {
        crate::core::reactivity::create_root(|| {
            let mut row = Element::new().reactive_sync(|element, _| {
                element.style.bg_color = [0.2, 0.3, 0.4, 1.0];
                LayoutStatus::StyleChanged
            });
            row.layout_bounds = [100.0, 210.0, 600.0, 22.0];
            let mut root = Element::new()
                .focus_id(crate::core::scroll_viewport::focus_id_for_scroll_viewport(
                    "hover-damage",
                ))
                .overflow("hidden")
                .child(row);
            root.layout_bounds = [100.0, 100.0, 160.0, 100.0];
            root.scroll_offset = [80.0, 100.0];
            let mut tree = LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            crate::app::damage::take_frame_paint_nodes();
            for (offset, expected) in [
                ([80.0, 100.0], Some([100.0, 110.0, 160.0, 22.0])),
                ([400.0, 90.0], Some([100.0, 120.0, 160.0, 22.0])),
                ([400.0, 125.0], Some([100.0, 100.0, 160.0, 7.0])),
                ([400.0, 400.0], None),
            ] {
                root.scroll_offset = offset;
                sync_reactive_elements_subtree(
                    &mut root,
                    &mut tree,
                    &mut impact,
                    800.0,
                    600.0,
                    [0.0; 2],
                );
                let damage = crate::app::damage::take_frame_paint_nodes();
                if let Some(bounds) = expected {
                    assert_eq!(damage.len(), 1);
                    assert_eq!(damage[0].bounds, bounds);
                    assert_eq!(damage[0].kind, crate::app::damage::PaintDirtyKind::Region);
                } else {
                    assert!(damage.is_empty());
                }
            }
        });
    }

    #[test]
    fn reactive_sync_summary_tracks_invalidated_mounts() {
        let calls = std::rc::Rc::new(std::cell::Cell::new(0));
        let mut root = Element::new().child(Element::new().child(Element::new()));
        let mut tree = LayoutTree::new();
        crate::layout::collect_cache::stamp_subtree_paint_hashes(&mut root, false);
        assert!(!root.subtree_reactive_sync);
        let counter = calls.clone();
        root.children[0]
            .children
            .push(Element::new().reactive_sync(move |_, _| {
                counter.set(counter.get() + 1);
                LayoutStatus::Clean
            }));
        crate::layout::collect_cache::invalidate_collect_paint_hash_path(&mut root, &[0]);
        let sync = |root: &mut Element, tree: &mut LayoutTree| {
            let mut impact = LayoutStatus::Clean;
            sync_reactive_elements_subtree(root, tree, &mut impact, 400.0, 300.0, [0.0; 2]);
            assert_eq!(impact, LayoutStatus::Clean);
        };
        // A newly mounted callback must run even before the next stamp.
        sync(&mut root, &mut tree);
        assert_eq!(calls.get(), 1);
        crate::layout::collect_cache::stamp_subtree_paint_hashes(&mut root, true);
        assert!(root.subtree_reactive_sync);
        assert!(!root.children[0].children[0].subtree_reactive_sync);
        sync(&mut root, &mut tree);
        assert_eq!(calls.get(), 2);
        root.children[0].children.pop();
        crate::layout::collect_cache::invalidate_collect_paint_hash_path(&mut root, &[0]);
        crate::layout::collect_cache::stamp_subtree_paint_hashes(&mut root, true);
        assert!(!root.subtree_reactive_sync);
        sync(&mut root, &mut tree);
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn text_spans_builder_sets_concat_text() {
        let el = Element::new().text_spans(vec![
            TextSpan::new("Hello").weight(700),
            TextSpan::new(" "),
            TextSpan::new("world").color([255, 0, 0, 255]),
        ]);
        assert_eq!(el.text.as_deref(), Some("Hello world"));
        assert_eq!(el.text_spans.as_ref().map(|s| s.len()), Some(3));
    }

    #[test]
    fn text_direction_builder() {
        let el = Element::new().text("hi").text_direction(TextDirection::Rtl);
        assert_eq!(el.text_style.direction, TextDirection::Rtl);
    }

    #[test]
    fn text_overflow_modes_set_whitespace_and_box_overflow() {
        let clip = Element::new().text("hi").text_overflow("clip");
        assert_eq!(clip.text_style.overflow, TextOverflow::Clip);
        assert_eq!(
            clip.text_style.white_space,
            crate::style::WhiteSpace::Nowrap
        );
        assert_eq!(clip.style.overflow.x, taffy::style::Overflow::Hidden);
        assert_eq!(clip.style.overflow.y, taffy::style::Overflow::Visible);

        let ellipsis = Element::new().text("hi").text_overflow_ellipsis();
        assert_eq!(ellipsis.style.overflow.x, taffy::style::Overflow::Hidden);
        assert_eq!(ellipsis.style.overflow.y, taffy::style::Overflow::Visible);

        let hidden = Element::new().text("hi").text_overflow_hidden();
        assert_eq!(hidden.text_style.overflow, TextOverflow::Hidden);
        assert_eq!(
            hidden.text_style.white_space,
            crate::style::WhiteSpace::Nowrap
        );
        assert_eq!(hidden.style.overflow.x, taffy::style::Overflow::Hidden);
        assert_eq!(hidden.style.overflow.y, taffy::style::Overflow::Hidden);

        let wrap = Element::new()
            .text("hi")
            .text_overflow_ellipsis()
            .text_overflow_newline();
        assert_eq!(wrap.text_style.overflow, TextOverflow::Newline);
        assert_eq!(
            wrap.text_style.white_space,
            crate::style::WhiteSpace::Normal
        );
        assert_eq!(wrap.style.overflow.x, taffy::style::Overflow::Visible);
    }

    #[test]
    fn fixed_sets_absolute_and_flag() {
        let el = Element::new().fixed();
        assert_eq!(el.style.position, Position::Absolute);
        assert!(el.style.position_fixed);
    }

    #[test]
    fn unpack_fragment_children_expands_jadix_fragment() {
        let mut frag = Element::new();
        frag.set_attr("jadix_fragment", "1");
        let frag = frag
            .child(Element::new().text("a"))
            .child(Element::new().text("b"));
        let kids = Element::unpack_fragment_children(Some(frag));
        assert_eq!(kids.len(), 2);
        assert_eq!(kids[0].text.as_deref(), Some("a"));
        assert_eq!(kids[1].text.as_deref(), Some("b"));
    }

    #[test]
    fn unpack_fragment_children_keeps_plain_element() {
        let el = Element::new().text("only");
        let kids = Element::unpack_fragment_children(Some(el));
        assert_eq!(kids.len(), 1);
        assert_eq!(kids[0].text.as_deref(), Some("only"));
    }

    #[test]
    fn unpack_fragment_children_none_is_empty() {
        assert!(Element::unpack_fragment_children(None).is_empty());
    }

    #[test]
    fn into_element_list_flattens_jadix_fragment() {
        let mut frag = Element::new();
        frag.set_attr("jadix_fragment", "1");
        let frag = frag
            .child(Element::new().text("a"))
            .child(Element::new().text("b"));
        let kids = frag.into_element_list();
        assert_eq!(kids.len(), 2);
        assert_eq!(kids[0].text.as_deref(), Some("a"));
        assert_eq!(kids[1].text.as_deref(), Some("b"));
    }

    #[test]
    fn bind_element_ref_writes_template_id() {
        let slot = crate::core::reactivity::create_signal(None::<String>);
        let el = Element::new().template_id("tid:ref");
        bind_element_ref(&slot, &el);
        assert_eq!(slot.get().as_deref(), Some("tid:ref"));
    }

    #[test]
    fn into_dynamic_component_calls_closure() {
        let el = (|| Element::new().text("dyn")).into_dynamic_component();
        assert_eq!(el.text.as_deref(), Some("dyn"));
        let el2 = Element::new().text("plain").into_dynamic_component();
        assert_eq!(el2.text.as_deref(), Some("plain"));
    }
}
