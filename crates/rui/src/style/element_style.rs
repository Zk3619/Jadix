//! 元素盒模型与 Taffy 映射。
//!
//! 契约与缺口总览见 [`docs/box_model.md`](../../../../docs/box_model.md)；执行 [`AUTHORITY.md`](../../../../docs/AUTHORITY.md) WP-2.9。
//!
//! ## 百分比尺寸（对齐 CSS）
//! - `width/height: N%` 相对**包含块的 content box**（父级内容区），不含父级 padding / border / margin。
//! - 本库默认 [`BoxSizing::BorderBox`]（同 Tailwind / 常见 CSS reset）：元素自身的
//!   `width: 100%` 含其 padding + border；子项可用宽 = 父 border-box − padding − border。
//! - `outline` / 阴影仍仅绘制，不进布局。

use taffy::prelude::*;

use crate::layout::dimension::{
    dim_kind, dimension_to_lp, dimension_to_lpa, lp_kind, lpa_kind, CssDim, DimKind, PercentAxis,
    UnitCtx,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxShadow {
    pub color: [f32; 4],
    pub offset: [f32; 2],
    pub blur: f32,
    pub spread: f32,
    pub inset: bool,
}

impl Default for BoxShadow {
    fn default() -> Self {
        Self {
            color: [0.0, 0.0, 0.0, 0.0],
            offset: [0.0, 0.0],
            blur: 0.0,
            spread: 0.0,
            inset: false,
        }
    }
}

fn parse_css_len_px(token: &str, root_font_size_px: f32) -> Option<f32> {
    let t = token.trim();
    if t.is_empty() {
        return None;
    }
    if let Ok(v) = t.parse::<f32>() {
        // Unsuffixed numeric values default to rem (align with IntoF32/IntoDimension).
        return Some(v * root_font_size_px);
    }
    if let Some(raw) = t.strip_suffix("px") {
        return raw.trim().parse::<f32>().ok();
    }
    if let Some(raw) = t.strip_suffix("rem") {
        return raw
            .trim()
            .parse::<f32>()
            .ok()
            .map(|v| v * root_font_size_px);
    }
    if let Some(raw) = t.strip_suffix("em") {
        return raw
            .trim()
            .parse::<f32>()
            .ok()
            .map(|v| v * root_font_size_px);
    }
    None
}

/// Parse a single `box-shadow` (no comma-separated multi-shadow yet; one shadow covers component roots).
///
/// Semantics follow MDN: `<color>? && <length>{2,4} && inset?`.
/// `color` uses this project's semantic color system (`parse_color`).
pub fn parse_box_shadow(s: impl AsRef<str>) -> BoxShadow {
    let raw = s.as_ref().trim();
    if raw.is_empty() || raw.eq_ignore_ascii_case("none") {
        return BoxShadow::default();
    }

    // Keep in sync with `CSS_ROOT_FONT_SIZE_PX` (we avoid depending on layout module here).
    let root_font_size_px = 16.0_f32;

    let mut out = BoxShadow::default();
    let mut lens: Vec<f32> = Vec::with_capacity(4);
    for tok in raw.split_whitespace() {
        if tok.eq_ignore_ascii_case("inset") {
            out.inset = true;
            continue;
        }
        if let Some(v) = parse_css_len_px(tok, root_font_size_px) {
            if lens.len() < 4 {
                lens.push(v);
            }
            continue;
        }
        // Assume it's a color keyword/token. Keep the first color-like token.
        if out.color[3] == 0.0 {
            out.color = super::parse_color(tok);
        }
    }

    // MDN length rules:
    // - 2 lengths => offset-x/offset-y
    // - 3 lengths => + blur (negative not allowed → 0)
    // - 4 lengths => + spread (can be negative)
    if !lens.is_empty() {
        out.offset[0] = lens[0];
    }
    if lens.len() >= 2 {
        out.offset[1] = lens[1];
    }
    if lens.len() >= 3 {
        out.blur = lens[2].max(0.0);
    } else {
        out.blur = 0.0;
    }
    if lens.len() >= 4 {
        out.spread = lens[3];
    } else {
        out.spread = 0.0;
    }
    out
}

/// Split multi-layer `box-shadow` on commas (ignore commas inside parentheses for `rgb(...)`).
pub fn parse_box_shadows(s: impl AsRef<str>) -> Vec<BoxShadow> {
    let raw = s.as_ref().trim();
    if raw.is_empty() || raw.eq_ignore_ascii_case("none") {
        return Vec::new();
    }
    let mut parts: Vec<&str> = Vec::new();
    let mut start = 0usize;
    let mut depth = 0i32;
    for (i, ch) in raw.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                let part = raw[start..i].trim();
                if !part.is_empty() {
                    parts.push(part);
                }
                start = i + ch.len_utf8();
            }
            _ => {}
        }
    }
    let tail = raw[start..].trim();
    if !tail.is_empty() {
        parts.push(tail);
    }
    if parts.is_empty() {
        return vec![parse_box_shadow(raw)];
    }
    parts.into_iter().map(parse_box_shadow).collect()
}

#[cfg(test)]
mod tests {
    use super::parse_box_shadow;

    fn approx_eq(a: f32, b: f32) {
        assert!((a - b).abs() < 0.001, "expected {a} ~= {b}");
    }

    #[test]
    fn box_shadow_unsuffixed_defaults_to_rem() {
        let sh = parse_box_shadow("1 0.5 0.25 0.125");
        approx_eq(sh.offset[0], 16.0);
        approx_eq(sh.offset[1], 8.0);
        approx_eq(sh.blur, 4.0);
        approx_eq(sh.spread, 2.0);
    }

    #[test]
    fn box_shadow_supports_px_rem_em_conversion() {
        let sh = parse_box_shadow("4px 0.5rem 0.25em 2px");
        approx_eq(sh.offset[0], 4.0);
        approx_eq(sh.offset[1], 8.0);
        approx_eq(sh.blur, 4.0);
        approx_eq(sh.spread, 2.0);
    }

    #[test]
    fn box_shadows_split_on_comma() {
        let list = super::parse_box_shadows("0 1px 2px rgb(0,0,0), 0 4px 8px #00000080");
        assert_eq!(list.len(), 2);
        approx_eq(list[0].blur, 2.0);
        approx_eq(list[1].blur, 8.0);
    }
}

/// CSS `border-image-repeat` value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BorderImageRepeat {
    #[default]
    Stretch,
    Repeat,
    Round,
    Space,
}

impl BorderImageRepeat {
    pub fn from_css_keyword(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "repeat" => Self::Repeat,
            "round" => Self::Round,
            "space" => Self::Space,
            _ => Self::Stretch,
        }
    }
}

/// CSS `border-shape` / `corner-shape` (CSS Borders and Box Decorations Level 4).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum CornerShape {
    #[default]
    Round,
    Angle,
    /// Superellipse exponent (`n`). `n=2` is equivalent to `Round`.
    Superellipse(f32),
    Notch,
    Scoop,
}

impl CornerShape {
    pub fn from_css_keyword(s: &str) -> Self {
        let s = s.trim().to_ascii_lowercase();
        if let Some(inner) = s
            .strip_prefix("superellipse(")
            .and_then(|r| r.strip_suffix(')'))
        {
            return Self::Superellipse(inner.trim().parse::<f32>().unwrap_or(2.0));
        }
        match s.as_str() {
            "round" => Self::Round,
            "angle" | "bevel" | "chamfer" => Self::Angle,
            "notch" => Self::Notch,
            "scoop" => Self::Scoop,
            _ => Self::Round,
        }
    }

    /// Float encoding for the GPU shader.
    pub fn to_shader_f32(self) -> f32 {
        match self {
            Self::Round => 0.0,
            Self::Angle => 1.0,
            Self::Superellipse(_) => 2.0,
            Self::Notch => 3.0,
            Self::Scoop => 4.0,
        }
    }

    /// Superellipse exponent (only meaningful when `Superellipse`).
    pub fn shader_param(self) -> f32 {
        match self {
            Self::Superellipse(n) => n,
            _ => 2.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LineStyle {
    #[default]
    None,
    Hidden,
    Solid,
    Dotted,
    Dashed,
    Double,
    Groove,
    Ridge,
    Inset,
    Outset,
}

impl LineStyle {
    pub fn from_css_keyword(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "" | "none" => Self::None,
            "hidden" => Self::Hidden,
            "solid" => Self::Solid,
            "dotted" => Self::Dotted,
            "dashed" => Self::Dashed,
            "double" => Self::Double,
            "groove" => Self::Groove,
            "ridge" => Self::Ridge,
            "inset" => Self::Inset,
            "outset" => Self::Outset,
            _ => Self::Solid,
        }
    }

    /// Returns `true` when the style produces no visible border.
    pub fn is_invisible(self) -> bool {
        matches!(self, Self::None | Self::Hidden)
    }

    /// Float encoding for the GPU shader (matches `rounded_rect.wgsl` constants).
    pub fn to_shader_f32(self) -> f32 {
        match self {
            Self::None | Self::Hidden => 0.0,
            Self::Solid => 1.0,
            Self::Dotted => 2.0,
            Self::Dashed => 3.0,
            Self::Double => 4.0,
            Self::Groove => 5.0,
            Self::Ridge => 6.0,
            Self::Inset => 7.0,
            Self::Outset => 8.0,
        }
    }
}

/// Deferred CSS unit slot (vw/vh/em/calc) applied in [`Style::to_taffy_style`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitSlot {
    Width,
    Height,
    MinWidth,
    MinHeight,
    MaxWidth,
    MaxHeight,
    FlexBasis,
    PadL,
    PadR,
    PadT,
    PadB,
    MarginL,
    MarginR,
    MarginT,
    MarginB,
    GapX,
    GapY,
    InsetL,
    InsetR,
    InsetT,
    InsetB,
}

/// 元素布局与部分视觉属性（背景/边框/阴影等由渲染路径消费，不进入 Taffy）。
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub display: Display,
    pub flex_direction: FlexDirection,
    pub flex_wrap: FlexWrap,
    pub justify_content: Option<JustifyContent>,
    pub align_items: Option<AlignItems>,
    /// Flex 子项 / Grid 子项交叉轴对齐（覆盖父级 `align_items`）。
    pub align_self: Option<AlignSelf>,
    /// Grid 子项行向对齐（覆盖父级 `justify_items`）。
    pub justify_self: Option<AlignSelf>,
    /// 多行 Flex / Grid 容器：`align-content`。
    pub align_content: Option<AlignContent>,
    /// Grid 容器：单元格内子项的默认行向对齐。
    pub justify_items: Option<AlignItems>,
    /// 盒模型：默认 BorderBox（Tailwind）；文案叶若自带 padding 需 ContentBox，
    /// 否则测宽被当成边框宽、padding 吃掉内容区。
    pub box_sizing: BoxSizing,
    pub size: Size<Dimension>,
    pub min_size: Size<Dimension>,
    pub max_size: Size<Dimension>,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub flex_basis: Dimension,
    pub padding: Rect<LengthPercentage>,
    pub margin: Rect<LengthPercentageAuto>,
    pub gap: taffy::geometry::Size<taffy::style::LengthPercentage>,
    pub overflow: taffy::geometry::Point<taffy::style::Overflow>,
    pub position: Position,
    pub inset: Rect<LengthPercentageAuto>,
    /// Viewport / `em` / `calc` specs resolved at layout against [`UnitCtx`].
    pub unit_specs: Vec<(UnitSlot, CssDim)>,
    pub z_index: i32,

    /// Grid 容器：显式列轨（`grid-template-columns`）。
    pub grid_template_columns: Vec<crate::layout::GridTrack>,
    /// Grid 容器：显式行轨（`grid-template-rows`）。
    pub grid_template_rows: Vec<crate::layout::GridTrack>,
    /// Grid 隐式行尺寸。
    pub grid_auto_rows: Vec<TrackSizingFunction>,
    /// Grid 隐式列尺寸。
    pub grid_auto_columns: Vec<TrackSizingFunction>,
    pub grid_auto_flow: GridAutoFlow,
    /// Grid 子项：行占位。
    pub grid_row: Line<GridPlacement>,
    /// Grid 子项：列占位。
    pub grid_column: Line<GridPlacement>,

    // Visual properties
    pub bg_color: [f32; 4],
    /// Layout + paint：非 `None` 且任一边宽度 > 0 时写入 Taffy `border`。
    pub border_style: LineStyle,
    pub border_color: [f32; 4],
    /// Uniform shorthand / max of per-side widths (compat for `border_width > 0` checks).
    pub border_width: f32,
    /// Per-side widths **T R B L**. `None` = all sides use [`Self::border_width`].
    pub border_width_sides: Option<[f32; 4]>,
    /// Per-side colors **T R B L**. `None` = all sides use [`Self::border_color`].
    pub border_color_sides: Option<[[f32; 4]; 4]>,
    /// Per-side styles **T R B L**. `None` = all sides use [`Self::border_style`].
    pub border_style_sides: Option<[LineStyle; 4]>,
    /// CSS `border-collapse` (table layout).
    pub border_collapse: bool,
    /// CSS `border-spacing` `[horizontal, vertical]` in logical px.
    pub border_spacing: [f32; 2],
    /// CSS `border-image-source` — texture key for the image pipeline.
    pub border_image_source: Option<String>,
    /// CSS `border-image-slice` **T R B L** (unitless number or percentage of image size).
    pub border_image_slice: [f32; 4],
    /// CSS `border-image-slice: ... fill`.
    pub border_image_slice_fill: bool,
    /// CSS `border-image-width` **T R B L**. `None` = use `border-width`.
    pub border_image_width: Option<[f32; 4]>,
    /// CSS `border-image-outset` **T R B L** in logical px.
    pub border_image_outset: [f32; 4],
    /// CSS `border-image-repeat` `[horizontal, vertical]`.
    pub border_image_repeat: [BorderImageRepeat; 2],
    /// CSS `border-shape` / `corner-shape` (all corners).
    pub corner_shape: CornerShape,
    /// Per-corner shapes **TL TR BR BL**. `None` = all use [`Self::corner_shape`].
    pub corner_shape_sides: Option<[CornerShape; 4]>,
    /// 仅绘制，不进 Taffy。
    pub outline_style: LineStyle,
    pub outline_color: [f32; 4],
    pub outline_width: f32,
    /// Distance between the outline and the border edge (CSS `outline-offset`).
    /// Positive = outward, negative = inward. Default 0.
    pub outline_offset: f32,
    /// Corner radii in CSS `border-radius` order: **`[TL, TR, BR, BL]`**.
    pub radii: [f32; 4],
    pub shadow_color: [f32; 4],
    pub shadow_offset: [f32; 2],
    pub shadow_blur: f32,
    pub shadow_spread: f32,
    pub shadow_inset: bool,
    /// Additional box-shadow layers beyond the primary `shadow_*` fields.
    pub extra_shadows: Vec<BoxShadow>,
    /// Optional CSS gradient fill (linear / radial / conic; painted as strips or texture).
    pub bg_gradient: Option<super::gradient::Gradient>,
    /// Element opacity multiplier for drawn colors (layout unchanged).
    pub opacity: f32,
    /// Visual translate offset in logical px (MVP transform; does not affect layout).
    pub translate: [f32; 2],
    /// Visual translate offset as fraction of own bounding box (CSS `translate: 50%`).
    /// `[0.5, 0.0]` = shift right by half the element's width.
    pub translate_pct: [f32; 2],
    /// Visual rotation in degrees around the element center (does not affect layout).
    pub rotate_deg: f32,
    /// Visual scale around the element center (does not affect layout).
    /// `[x, y]`; CSS `scale` property supports 1–3 values (Z stored separately).
    pub scale: [f32; 2],
    /// Visual Z-axis scale (CSS `scale` 3-value form). Default 1.0. Reserved for 3D transforms.
    pub scale_z: f32,
    /// CSS `transform`: ordered list of transform functions. `None` = `none`.
    pub transform: Option<super::transform::TransformList>,
    /// CSS `transform-origin`. Default: `50% 50% 0` (center).
    pub transform_origin: super::transform::TransformOrigin,
    /// CSS `transform-box`. Default: `border-box` (for CSS elements).
    pub transform_box: super::transform::TransformBox,
    /// CSS `transform-style`. Default: `flat`.
    pub transform_style: super::transform::TransformStyle,
    /// CSS `zoom`: magnification factor that **affects layout** (scales size/padding/margin).
    /// `1.0` = normal. Values > 1 enlarge, < 1 shrink. Applied before Taffy compute.
    pub zoom: f32,
    /// CSS `aspect-ratio` (width / height). Passed to Taffy.
    pub aspect_ratio: Option<f32>,
    /// Emulate CSS `position: fixed` (taffy has no Fixed): Absolute layout + ignore ancestor scroll.
    pub position_fixed: bool,
    /// Emulate CSS `position: sticky`: Relative layout + scroll-aware offset clamping.
    pub position_sticky: bool,
    /// CSS-like `backdrop-filter: blur(Npx)` radius in logical px. `0` = off.
    /// Consumed by fullscreen modal underlays ([`crate::primitives::overlay`]); general elements TBD.
    pub backdrop_blur: f32,
    /// CSS `filter`: ordered filter functions. `None` = `none`.
    pub filter: Option<super::filter::FilterList>,
    /// CSS `transition` entries. Each entry specifies which property to animate,
    /// duration, delay, and timing function.
    pub transition: Vec<TransitionEntry>,
    /// CSS `animation` entries (MDN shorthand / longhands). Driven by
    /// [`crate::style::tick_element_animations`].
    pub animations: Vec<AnimationEntry>,
}

/// Which CSS property a transition applies to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TransitionProperty {
    All,
    Opacity,
    Transform,
    /// CSS 独立 `translate` 属性（也包含在 `transform` 过渡中）。
    Translate,
    Filter,
    BackgroundColor,
    BorderColor,
    OutlineColor,
    OutlineWidth,
    OutlineOffset,
    BoxShadow,
    Color,
    MaxHeight,
    /// A property this engine cannot interpolate (`height`, `padding`, …).
    ///
    /// CSS treats an unknown / non-animatable `transition-property` as "no transition"
    /// for that entry. Folding it into [`Self::All`] instead would silently attach
    /// tweens to every supported property — including `max-height`, which then drives
    /// an `auto` box to zero.
    Unsupported,
}

impl TransitionProperty {
    pub fn from_css(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "all" => Self::All,
            "opacity" => Self::Opacity,
            "transform" => Self::Transform,
            "translate" => Self::Translate,
            "filter" => Self::Filter,
            "background-color" | "background" | "bg" => Self::BackgroundColor,
            "border-color" => Self::BorderColor,
            "outline-color" => Self::OutlineColor,
            "outline-width" => Self::OutlineWidth,
            "outline-offset" => Self::OutlineOffset,
            "box-shadow" | "shadow" => Self::BoxShadow,
            "color" => Self::Color,
            "max-height" | "max_height" => Self::MaxHeight,
            "none" => Self::Unsupported,
            other => {
                #[cfg(debug_assertions)]
                log::debug!("[transition] unsupported transition-property `{other}` — ignored");
                let _ = other;
                Self::Unsupported
            }
        }
    }

    /// Whether this property matches a specific property (All matches everything).
    pub fn matches(self, specific: Self) -> bool {
        self == Self::All || self == specific
    }
}

/// CSS `transition-timing-function`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TimingFunction {
    Linear,
    Ease,
    EaseIn,
    EaseOut,
    EaseInOut,
    CubicBezier(f32, f32, f32, f32),
}

impl Default for TimingFunction {
    fn default() -> Self {
        Self::Ease
    }
}

impl TimingFunction {
    pub fn eval(&self, t: f32) -> f32 {
        use super::animation::cubic_bezier_y_at_x;
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Linear => t,
            Self::Ease => cubic_bezier_y_at_x(t, 0.25, 0.1, 0.25, 1.0),
            Self::EaseIn => cubic_bezier_y_at_x(t, 0.42, 0.0, 1.0, 1.0),
            Self::EaseOut => cubic_bezier_y_at_x(t, 0.0, 0.0, 0.58, 1.0),
            Self::EaseInOut => cubic_bezier_y_at_x(t, 0.42, 0.0, 0.58, 1.0),
            Self::CubicBezier(x1, y1, x2, y2) => cubic_bezier_y_at_x(t, *x1, *y1, *x2, *y2),
        }
    }

    pub fn from_css(s: &str) -> Self {
        let s = s.trim().to_ascii_lowercase();
        match s.as_str() {
            "linear" => Self::Linear,
            "ease" => Self::Ease,
            "ease-in" => Self::EaseIn,
            "ease-out" => Self::EaseOut,
            "ease-in-out" => Self::EaseInOut,
            _ => {
                if let Some(inner) = s
                    .strip_prefix("cubic-bezier(")
                    .and_then(|r| r.strip_suffix(')'))
                {
                    let parts: Vec<f32> = inner
                        .split(',')
                        .filter_map(|p| p.trim().parse().ok())
                        .collect();
                    if parts.len() == 4 {
                        return Self::CubicBezier(parts[0], parts[1], parts[2], parts[3]);
                    }
                }
                Self::Ease
            }
        }
    }
}

/// A single CSS transition entry.
#[derive(Clone, Debug, PartialEq)]
pub struct TransitionEntry {
    pub property: TransitionProperty,
    pub duration_ms: f32,
    pub delay_ms: f32,
    pub timing: TimingFunction,
}

/// CSS `animation-direction`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AnimationDirection {
    #[default]
    Normal,
    Reverse,
    Alternate,
    AlternateReverse,
}

impl AnimationDirection {
    pub fn from_css(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "normal" => Some(Self::Normal),
            "reverse" => Some(Self::Reverse),
            "alternate" => Some(Self::Alternate),
            "alternate-reverse" => Some(Self::AlternateReverse),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Reverse => "reverse",
            Self::Alternate => "alternate",
            Self::AlternateReverse => "alternate-reverse",
        }
    }
}

/// CSS `animation-fill-mode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AnimationFillMode {
    #[default]
    None,
    Forwards,
    Backwards,
    Both,
}

impl AnimationFillMode {
    pub fn from_css(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "none" => Some(Self::None),
            "forwards" => Some(Self::Forwards),
            "backwards" => Some(Self::Backwards),
            "both" => Some(Self::Both),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Forwards => "forwards",
            Self::Backwards => "backwards",
            Self::Both => "both",
        }
    }

    pub fn apply_backwards(self) -> bool {
        matches!(self, Self::Backwards | Self::Both)
    }

    pub fn apply_forwards(self) -> bool {
        matches!(self, Self::Forwards | Self::Both)
    }
}

/// CSS `animation-play-state`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AnimationPlayState {
    #[default]
    Running,
    Paused,
}

impl AnimationPlayState {
    pub fn from_css(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "running" => Some(Self::Running),
            "paused" => Some(Self::Paused),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Paused => "paused",
        }
    }
}

/// CSS `animation-iteration-count`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AnimationIterationCount {
    Finite(f32),
    Infinite,
}

impl Default for AnimationIterationCount {
    fn default() -> Self {
        Self::Finite(1.0)
    }
}

impl AnimationIterationCount {
    pub fn from_css(s: &str) -> Option<Self> {
        let s = s.trim().to_ascii_lowercase();
        if s == "infinite" {
            return Some(Self::Infinite);
        }
        s.parse::<f32>().ok().map(|n| Self::Finite(n.max(0.0)))
    }
}

/// CSS `animation-composition` (MVP: stored; runtime always **replace**).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AnimationComposition {
    #[default]
    Replace,
    Add,
    Accumulate,
}

impl AnimationComposition {
    pub fn from_css(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "replace" => Some(Self::Replace),
            "add" => Some(Self::Add),
            "accumulate" => Some(Self::Accumulate),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Replace => "replace",
            Self::Add => "add",
            Self::Accumulate => "accumulate",
        }
    }
}

/// CSS `animation-timeline` (MVP: only [`Self::Auto`] drives time).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum AnimationTimeline {
    #[default]
    Auto,
    Other(String),
}

impl AnimationTimeline {
    pub fn from_css(s: &str) -> Self {
        let s = s.trim();
        if s.is_empty() || s.eq_ignore_ascii_case("auto") {
            Self::Auto
        } else {
            Self::Other(s.to_string())
        }
    }

    pub fn is_auto(&self) -> bool {
        matches!(self, Self::Auto)
    }
}

/// A single CSS `animation` entry (one list item in the shorthand).
#[derive(Clone, Debug, PartialEq)]
pub struct AnimationEntry {
    pub name: String,
    pub duration_ms: f32,
    pub timing: TimingFunction,
    pub delay_ms: f32,
    pub iteration_count: AnimationIterationCount,
    pub direction: AnimationDirection,
    pub fill_mode: AnimationFillMode,
    pub play_state: AnimationPlayState,
    pub composition: AnimationComposition,
    pub timeline: AnimationTimeline,
    /// Raw CSS `animation-range-start` (stored; not driven in MVP).
    pub range_start: Option<String>,
    /// Raw CSS `animation-range-end` (stored; not driven in MVP).
    pub range_end: Option<String>,
}

impl Default for AnimationEntry {
    fn default() -> Self {
        Self {
            name: String::new(),
            duration_ms: 0.0,
            timing: TimingFunction::Ease,
            delay_ms: 0.0,
            iteration_count: AnimationIterationCount::Finite(1.0),
            direction: AnimationDirection::Normal,
            fill_mode: AnimationFillMode::None,
            play_state: AnimationPlayState::Running,
            composition: AnimationComposition::Replace,
            timeline: AnimationTimeline::Auto,
            range_start: None,
            range_end: None,
        }
    }
}

impl AnimationEntry {
    pub fn is_named(&self) -> bool {
        !self.name.is_empty() && !self.name.eq_ignore_ascii_case("none")
    }

    /// True when this entry should keep requesting frames.
    pub fn needs_frame(&self) -> bool {
        self.is_named()
            && self.timeline.is_auto()
            && self.play_state == AnimationPlayState::Running
            && (self.duration_ms > 0.0
                || matches!(self.iteration_count, AnimationIterationCount::Infinite))
    }
}

impl Default for Style {
    fn default() -> Self {
        Self {
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::NoWrap,
            justify_content: None,
            align_items: None,
            align_self: None,
            justify_self: None,
            align_content: None,
            justify_items: None,
            box_sizing: BoxSizing::BorderBox,
            size: Size::auto(),
            min_size: Size {
                width: Dimension::auto(),
                height: Dimension::auto(),
            },
            max_size: Size {
                width: Dimension::auto(),
                height: Dimension::auto(),
            },
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_basis: Dimension::auto(),
            padding: Rect::zero(),
            margin: Rect::zero(),
            gap: taffy::geometry::Size {
                width: taffy::style::LengthPercentage::length(0.0),
                height: taffy::style::LengthPercentage::length(0.0),
            },
            overflow: taffy::geometry::Point {
                x: taffy::style::Overflow::Visible,
                y: taffy::style::Overflow::Visible,
            },
            position: Position::Relative,
            inset: Rect::auto(),
            unit_specs: Vec::new(),
            z_index: 0,
            grid_template_columns: Vec::new(),
            grid_template_rows: Vec::new(),
            grid_auto_rows: Vec::new(),
            grid_auto_columns: Vec::new(),
            grid_auto_flow: GridAutoFlow::Row,
            grid_row: Line {
                start: GridPlacement::Auto,
                end: GridPlacement::Auto,
            },
            grid_column: Line {
                start: GridPlacement::Auto,
                end: GridPlacement::Auto,
            },
            bg_color: [0.0, 0.0, 0.0, 0.0],
            border_style: LineStyle::None,
            border_color: [0.0, 0.0, 0.0, 0.0],
            border_width: 0.0,
            border_width_sides: None,
            border_color_sides: None,
            border_style_sides: None,
            border_collapse: false,
            border_spacing: [0.0; 2],
            border_image_source: None,
            border_image_slice: [100.0; 4],
            border_image_slice_fill: false,
            border_image_width: None,
            border_image_outset: [0.0; 4],
            border_image_repeat: [BorderImageRepeat::Stretch; 2],
            corner_shape: CornerShape::Round,
            corner_shape_sides: None,
            outline_style: LineStyle::None,
            outline_color: [0.0, 0.0, 0.0, 0.0],
            outline_width: 0.0,
            outline_offset: 0.0,
            radii: [0.0; 4],
            shadow_color: [0.0, 0.0, 0.0, 0.0],
            shadow_offset: [0.0, 0.0],
            shadow_blur: 0.0,
            shadow_spread: 0.0,
            shadow_inset: false,
            extra_shadows: Vec::new(),
            bg_gradient: None,
            opacity: 1.0,
            translate: [0.0, 0.0],
            translate_pct: [0.0, 0.0],
            rotate_deg: 0.0,
            scale: [1.0, 1.0],
            scale_z: 1.0,
            transform: None,
            transform_origin: super::transform::TransformOrigin::default(),
            transform_box: super::transform::TransformBox::default(),
            transform_style: super::transform::TransformStyle::default(),
            zoom: 1.0,
            aspect_ratio: None,
            position_fixed: false,
            position_sticky: false,
            backdrop_blur: 0.0,
            filter: None,
            transition: Vec::new(),
            animations: Vec::new(),
        }
    }
}

impl Style {
    pub fn primary_shadow(&self) -> BoxShadow {
        BoxShadow {
            color: self.shadow_color,
            offset: self.shadow_offset,
            blur: self.shadow_blur,
            spread: self.shadow_spread,
            inset: self.shadow_inset,
        }
    }

    pub fn set_shadows_from_parsed(&mut self, shadows: &[BoxShadow]) {
        self.extra_shadows.clear();
        if let Some(first) = shadows.first() {
            self.shadow_color = first.color;
            self.shadow_offset = first.offset;
            self.shadow_blur = first.blur;
            self.shadow_spread = first.spread;
            self.shadow_inset = first.inset;
            self.extra_shadows.extend_from_slice(&shadows[1..]);
        } else {
            self.shadow_color = [0.0, 0.0, 0.0, 0.0];
            self.shadow_offset = [0.0, 0.0];
            self.shadow_blur = 0.0;
            self.shadow_spread = 0.0;
            self.shadow_inset = false;
        }
    }

    pub fn to_taffy_style(&self) -> taffy::style::Style {
        self.to_taffy_style_with(&UnitCtx::new(0.0, 0.0, CSS_FALLBACK_FONT))
    }

    pub fn to_taffy_style_with(&self, ctx: &UnitCtx) -> taffy::style::Style {
        let [bt, br, bb, bl] = self.resolved_border_widths();
        let border = Rect {
            top: LengthPercentage::length(bt),
            right: LengthPercentage::length(br),
            bottom: LengthPercentage::length(bb),
            left: LengthPercentage::length(bl),
        };
        let mut size = self.size;
        let mut min_size = self.min_size;
        let mut max_size = self.max_size;
        let mut flex_basis = self.flex_basis;
        let mut padding = self.padding;
        let mut margin = self.margin;
        let mut gap = self.gap;
        let mut inset = self.inset;
        for (slot, dim) in &self.unit_specs {
            match slot {
                UnitSlot::Width => size.width = dim.to_dimension_axis(ctx, PercentAxis::Inline),
                UnitSlot::Height => size.height = dim.to_dimension_axis(ctx, PercentAxis::Block),
                UnitSlot::MinWidth => {
                    min_size.width = dim.to_dimension_axis(ctx, PercentAxis::Inline)
                }
                UnitSlot::MinHeight => {
                    min_size.height = dim.to_dimension_axis(ctx, PercentAxis::Block)
                }
                UnitSlot::MaxWidth => {
                    max_size.width = dim.to_dimension_axis(ctx, PercentAxis::Inline)
                }
                UnitSlot::MaxHeight => {
                    max_size.height = dim.to_dimension_axis(ctx, PercentAxis::Block)
                }
                UnitSlot::FlexBasis => flex_basis = dim.to_dimension_axis(ctx, PercentAxis::Inline),
                // CSS: padding/margin % is always relative to containing-block **width**.
                UnitSlot::PadL => padding.left = dim.to_length_percentage(ctx, PercentAxis::Inline),
                UnitSlot::PadR => {
                    padding.right = dim.to_length_percentage(ctx, PercentAxis::Inline)
                }
                UnitSlot::PadT => padding.top = dim.to_length_percentage(ctx, PercentAxis::Inline),
                UnitSlot::PadB => {
                    padding.bottom = dim.to_length_percentage(ctx, PercentAxis::Inline)
                }
                UnitSlot::MarginL => {
                    margin.left = dim.to_length_percentage_auto(ctx, PercentAxis::Inline)
                }
                UnitSlot::MarginR => {
                    margin.right = dim.to_length_percentage_auto(ctx, PercentAxis::Inline)
                }
                UnitSlot::MarginT => {
                    margin.top = dim.to_length_percentage_auto(ctx, PercentAxis::Inline)
                }
                UnitSlot::MarginB => {
                    margin.bottom = dim.to_length_percentage_auto(ctx, PercentAxis::Inline)
                }
                UnitSlot::GapX => gap.width = dim.to_length_percentage(ctx, PercentAxis::Inline),
                UnitSlot::GapY => gap.height = dim.to_length_percentage(ctx, PercentAxis::Block),
                UnitSlot::InsetL => {
                    inset.left = dim.to_length_percentage_auto(ctx, PercentAxis::Inline)
                }
                UnitSlot::InsetR => {
                    inset.right = dim.to_length_percentage_auto(ctx, PercentAxis::Inline)
                }
                UnitSlot::InsetT => {
                    inset.top = dim.to_length_percentage_auto(ctx, PercentAxis::Block)
                }
                UnitSlot::InsetB => {
                    inset.bottom = dim.to_length_percentage_auto(ctx, PercentAxis::Block)
                }
            }
        }
        // CSS `zoom`: scale layout dimensions by the zoom factor.
        let z =
            if self.zoom.is_finite() && self.zoom > 0.0 && (self.zoom - 1.0).abs() > f32::EPSILON {
                self.zoom
            } else {
                1.0
            };
        if z != 1.0 {
            size.width = zoom_dimension(size.width, z);
            size.height = zoom_dimension(size.height, z);
            min_size.width = zoom_dimension(min_size.width, z);
            min_size.height = zoom_dimension(min_size.height, z);
            max_size.width = zoom_dimension(max_size.width, z);
            max_size.height = zoom_dimension(max_size.height, z);
            flex_basis = zoom_dimension(flex_basis, z);
            padding = zoom_rect_lp(padding, z);
            margin = zoom_rect_lpa(margin, z);
            gap.width = zoom_lp(gap.width, z);
            gap.height = zoom_lp(gap.height, z);
        }
        let border = if z != 1.0 {
            zoom_rect_lp_from(border, z)
        } else {
            border
        };

        taffy::style::Style {
            box_sizing: self.box_sizing,
            display: self.display,
            flex_direction: self.flex_direction,
            flex_wrap: self.flex_wrap,
            justify_content: self.justify_content,
            align_items: self.align_items,
            align_self: self.align_self,
            justify_self: self.justify_self,
            align_content: self.align_content,
            justify_items: self.justify_items,
            size,
            min_size,
            max_size,
            flex_grow: self.flex_grow,
            flex_shrink: self.flex_shrink,
            flex_basis,
            padding,
            border,
            margin,
            gap,
            overflow: self.overflow,
            position: self.position,
            inset,
            aspect_ratio: self.aspect_ratio,
            grid_template_columns: self.grid_template_columns.clone(),
            grid_template_rows: self.grid_template_rows.clone(),
            grid_auto_rows: self.grid_auto_rows.clone(),
            grid_auto_columns: self.grid_auto_columns.clone(),
            grid_auto_flow: self.grid_auto_flow,
            grid_row: self.grid_row.clone(),
            grid_column: self.grid_column.clone(),
            ..Default::default()
        }
    }

    pub fn has_percent_calc(&self) -> bool {
        self.unit_specs.iter().any(|(_, d)| d.has_percent())
    }

    /// `vw`/`vh`/`em`/`calc`（含 `%` in calc）须在布局期按视口 / 字号 / 包含块重烘焙。
    pub fn has_deferred_units(&self) -> bool {
        self.unit_specs
            .iter()
            .any(|(_, d)| d.needs_layout_context())
    }

    /// Border widths **T R B L** that participate in layout (0 if `border_style` is invisible).
    pub fn resolved_border_widths(&self) -> [f32; 4] {
        if self.border_style.is_invisible() {
            return [0.0; 4];
        }
        self.border_width_sides.unwrap_or([self.border_width; 4])
    }

    pub fn resolved_border_colors(&self) -> [[f32; 4]; 4] {
        self.border_color_sides.unwrap_or([self.border_color; 4])
    }

    pub fn border_is_uniform(&self) -> bool {
        let w = self.resolved_border_widths();
        let c = self.resolved_border_colors();
        let s = self.resolved_border_styles();
        w[0] == w[1]
            && w[1] == w[2]
            && w[2] == w[3]
            && c[0] == c[1]
            && c[1] == c[2]
            && c[2] == c[3]
            && s[0] == s[1]
            && s[1] == s[2]
            && s[2] == s[3]
    }

    pub fn set_border_side_width(&mut self, side: usize, width: f32) {
        let mut sides = self.border_width_sides.unwrap_or([self.border_width; 4]);
        sides[side.min(3)] = width.max(0.0);
        if sides[0] == sides[1] && sides[1] == sides[2] && sides[2] == sides[3] {
            self.border_width = sides[0];
            self.border_width_sides = None;
        } else {
            self.border_width_sides = Some(sides);
            self.border_width = sides.into_iter().fold(0.0_f32, f32::max);
        }
    }

    pub fn set_border_side_color(&mut self, side: usize, color: [f32; 4]) {
        let mut sides = self.border_color_sides.unwrap_or([self.border_color; 4]);
        sides[side.min(3)] = color;
        if sides[0] == sides[1] && sides[1] == sides[2] && sides[2] == sides[3] {
            self.border_color = color;
            self.border_color_sides = None;
        } else {
            self.border_color_sides = Some(sides);
        }
    }

    pub fn set_uniform_border_width(&mut self, width: f32) {
        self.border_width = width.max(0.0);
        self.border_width_sides = None;
    }

    pub fn set_border_widths_trbl(&mut self, sides: [f32; 4]) {
        let sides = sides.map(|v| v.max(0.0));
        if sides[0] == sides[1] && sides[1] == sides[2] && sides[2] == sides[3] {
            self.set_uniform_border_width(sides[0]);
        } else {
            self.border_width_sides = Some(sides);
            self.border_width = sides.into_iter().fold(0.0_f32, f32::max);
        }
    }

    pub fn resolved_border_styles(&self) -> [LineStyle; 4] {
        self.border_style_sides.unwrap_or([self.border_style; 4])
    }

    pub fn set_border_side_style(&mut self, side: usize, style: LineStyle) {
        let mut sides = self.border_style_sides.unwrap_or([self.border_style; 4]);
        sides[side.min(3)] = style;
        if sides[0] == sides[1] && sides[1] == sides[2] && sides[2] == sides[3] {
            self.border_style = sides[0];
            self.border_style_sides = None;
        } else {
            self.border_style_sides = Some(sides);
        }
    }

    pub fn set_border_styles_trbl(&mut self, sides: [LineStyle; 4]) {
        if sides[0] == sides[1] && sides[1] == sides[2] && sides[2] == sides[3] {
            self.border_style = sides[0];
            self.border_style_sides = None;
        } else {
            self.border_style_sides = Some(sides);
        }
    }

    pub fn resolved_corner_shapes(&self) -> [CornerShape; 4] {
        self.corner_shape_sides.unwrap_or([self.corner_shape; 4])
    }

    pub fn set_corner_side_shape(&mut self, corner: usize, shape: CornerShape) {
        let mut sides = self.corner_shape_sides.unwrap_or([self.corner_shape; 4]);
        sides[corner.min(3)] = shape;
        if sides[0] == sides[1] && sides[1] == sides[2] && sides[2] == sides[3] {
            self.corner_shape = sides[0];
            self.corner_shape_sides = None;
        } else {
            self.corner_shape_sides = Some(sides);
        }
    }

    /// Best-effort content box from specified size (definite Length / % of `ctx.cb_*`).
    /// Used to resolve descendant `calc(100% - Npx)` before Taffy compute.
    pub fn estimate_content_box(&self, ctx: &UnitCtx) -> (Option<f32>, Option<f32>) {
        let taffy = self.to_taffy_style_with(ctx);
        let [bt, br, bb, bl] = self.resolved_border_widths();
        let pad_x = lp_used(taffy.padding.left, ctx.cb_w) + lp_used(taffy.padding.right, ctx.cb_w);
        let pad_y = lp_used(taffy.padding.top, ctx.cb_w) + lp_used(taffy.padding.bottom, ctx.cb_w);
        let border_box = matches!(self.box_sizing, BoxSizing::BorderBox);
        let content = |dim: Dimension, cb: Option<f32>, chrome: f32| -> Option<f32> {
            let used = match dim_kind(dim) {
                DimKind::Length(l) => Some(l),
                DimKind::Percent(p) => cb.map(|c| p * c),
                DimKind::Auto => None,
            }?;
            if border_box {
                Some((used - chrome).max(0.0))
            } else {
                Some(used.max(0.0))
            }
        };
        (
            content(taffy.size.width, ctx.cb_w, pad_x + bl + br),
            content(taffy.size.height, ctx.cb_h, pad_y + bt + bb),
        )
    }

    pub fn set_css_dim(&mut self, slot: UnitSlot, dim: CssDim) {
        self.unit_specs.retain(|(s, _)| *s != slot);
        if dim.needs_layout_context() {
            if let Some(preview) = dim.preview_dimension() {
                self.apply_taffy_slot(slot, preview);
            }
            self.unit_specs.push((slot, dim));
        } else if let Some(immediate) = dim.as_immediate_dimension() {
            self.apply_taffy_slot(slot, immediate);
        }
    }

    fn apply_taffy_slot(&mut self, slot: UnitSlot, dim: Dimension) {
        let lp = dimension_to_lp(dim);
        let lpa = dimension_to_lpa(dim);
        match slot {
            UnitSlot::Width => self.size.width = dim,
            UnitSlot::Height => self.size.height = dim,
            UnitSlot::MinWidth => self.min_size.width = dim,
            UnitSlot::MinHeight => self.min_size.height = dim,
            UnitSlot::MaxWidth => self.max_size.width = dim,
            UnitSlot::MaxHeight => self.max_size.height = dim,
            UnitSlot::FlexBasis => self.flex_basis = dim,
            UnitSlot::PadL => self.padding.left = lp,
            UnitSlot::PadR => self.padding.right = lp,
            UnitSlot::PadT => self.padding.top = lp,
            UnitSlot::PadB => self.padding.bottom = lp,
            UnitSlot::MarginL => self.margin.left = lpa,
            UnitSlot::MarginR => self.margin.right = lpa,
            UnitSlot::MarginT => self.margin.top = lpa,
            UnitSlot::MarginB => self.margin.bottom = lpa,
            UnitSlot::GapX => self.gap.width = lp,
            UnitSlot::GapY => self.gap.height = lp,
            UnitSlot::InsetL => self.inset.left = lpa,
            UnitSlot::InsetR => self.inset.right = lpa,
            UnitSlot::InsetT => self.inset.top = lpa,
            UnitSlot::InsetB => self.inset.bottom = lpa,
        }
    }
}

/// Scale a `Dimension` by zoom factor (length values only; percent/auto pass through).
fn zoom_dimension(dim: Dimension, z: f32) -> Dimension {
    match dim_kind(dim) {
        DimKind::Length(l) => Dimension::length(l * z),
        _ => dim,
    }
}

fn zoom_lp(lp: LengthPercentage, z: f32) -> LengthPercentage {
    match lp_kind(lp) {
        DimKind::Length(l) => LengthPercentage::length(l * z),
        _ => lp,
    }
}

fn zoom_rect_lp(r: Rect<LengthPercentage>, z: f32) -> Rect<LengthPercentage> {
    Rect {
        top: zoom_lp(r.top, z),
        right: zoom_lp(r.right, z),
        bottom: zoom_lp(r.bottom, z),
        left: zoom_lp(r.left, z),
    }
}

fn zoom_rect_lp_from(r: Rect<LengthPercentage>, z: f32) -> Rect<LengthPercentage> {
    zoom_rect_lp(r, z)
}

fn zoom_lpa_val(lpa: LengthPercentageAuto, z: f32) -> LengthPercentageAuto {
    match lpa_kind(lpa) {
        DimKind::Length(l) => LengthPercentageAuto::length(l * z),
        DimKind::Percent(_) | DimKind::Auto => lpa,
    }
}

fn zoom_rect_lpa(r: Rect<LengthPercentageAuto>, z: f32) -> Rect<LengthPercentageAuto> {
    Rect {
        top: zoom_lpa_val(r.top, z),
        right: zoom_lpa_val(r.right, z),
        bottom: zoom_lpa_val(r.bottom, z),
        left: zoom_lpa_val(r.left, z),
    }
}

const CSS_FALLBACK_FONT: f32 = 16.0;

fn lp_used(lp: LengthPercentage, cb_w: Option<f32>) -> f32 {
    match lp_kind(lp) {
        DimKind::Length(l) => l,
        DimKind::Percent(p) => cb_w.map(|c| p * c).unwrap_or(0.0),
        DimKind::Auto => 0.0,
    }
}
