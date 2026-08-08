//! 元素盒模型与 Taffy 映射；部分绘制属性由 GPU 路径消费，不进入 Taffy。

use taffy::prelude::*;

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

/// 解析单个 `box-shadow`（不支持逗号多阴影；先做单阴影满足组件根依赖）。
///
/// 语义参考 MDN：`<color>? && <length>{2,4} && inset?`。
/// `color` 与本项目语义色系统对齐（`parse_color`）。
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

/// Two-stop linear gradient. `angle_deg`: 0 = left→right, 90 = top→bottom.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LinearGradient {
    pub from: [f32; 4],
    pub to: [f32; 4],
    pub angle_deg: f32,
}

impl Default for LinearGradient {
    fn default() -> Self {
        Self {
            from: [0.0, 0.0, 0.0, 0.0],
            to: [0.0, 0.0, 0.0, 0.0],
            angle_deg: 90.0,
        }
    }
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LineStyle {
    #[default]
    None,
    Solid,
}

impl LineStyle {
    pub fn from_css_keyword(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "" | "none" | "hidden" => Self::None,
            // Shader currently supports solid only.
            "solid" | "double" | "dotted" | "dashed" | "groove" | "ridge" | "inset" | "outset" => {
                Self::Solid
            }
            _ => Self::Solid,
        }
    }
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
    pub z_index: i32,

    /// Grid 容器：显式列轨（`grid-template-columns`）。
    pub grid_template_columns: Vec<TrackSizingFunction>,
    /// Grid 容器：显式行轨（`grid-template-rows`）。
    pub grid_template_rows: Vec<TrackSizingFunction>,
    /// Grid 隐式行尺寸。
    pub grid_auto_rows: Vec<NonRepeatedTrackSizingFunction>,
    /// Grid 隐式列尺寸。
    pub grid_auto_columns: Vec<NonRepeatedTrackSizingFunction>,
    pub grid_auto_flow: GridAutoFlow,
    /// Grid 子项：行占位。
    pub grid_row: Line<GridPlacement>,
    /// Grid 子项：列占位。
    pub grid_column: Line<GridPlacement>,

    // Visual properties (not passed to Taffy)
    pub bg_color: [f32; 4],
    pub border_style: LineStyle,
    pub border_color: [f32; 4],
    pub border_width: f32,
    pub outline_style: LineStyle,
    pub outline_color: [f32; 4],
    pub outline_width: f32,
    pub radii: [f32; 4],
    pub shadow_color: [f32; 4],
    pub shadow_offset: [f32; 2],
    pub shadow_blur: f32,
    pub shadow_spread: f32,
    pub shadow_inset: bool,
    /// Additional box-shadow layers beyond the primary `shadow_*` fields.
    pub extra_shadows: Vec<BoxShadow>,
    /// Optional two-stop linear gradient fill (painted as strips; layout unchanged).
    pub bg_gradient: Option<LinearGradient>,
    /// Element opacity multiplier for drawn colors (layout unchanged).
    pub opacity: f32,
    /// Visual translate offset in logical px (MVP transform; does not affect layout).
    pub translate: [f32; 2],
    /// Visual rotation in degrees around the element center (does not affect layout).
    pub rotate_deg: f32,
    /// Visual uniform scale around the element center (does not affect layout). Default 1.0.
    pub scale: f32,
    /// CSS `aspect-ratio` (width / height). Passed to Taffy.
    pub aspect_ratio: Option<f32>,
    /// Emulate CSS `position: fixed` (taffy has no Fixed): Absolute layout + ignore ancestor scroll.
    pub position_fixed: bool,
    /// CSS-like `backdrop-filter: blur(Npx)` radius in logical px. `0` = off.
    /// Consumed by fullscreen modal underlays ([`crate::primitives::overlay`]); general elements TBD.
    pub backdrop_blur: f32,
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
            size: Size::auto(),
            min_size: Size {
                width: Dimension::Auto,
                height: Dimension::Auto,
            },
            max_size: Size {
                width: Dimension::Auto,
                height: Dimension::Auto,
            },
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_basis: Dimension::Auto,
            padding: Rect::zero(),
            margin: Rect::zero(),
            gap: taffy::geometry::Size {
                width: taffy::style::LengthPercentage::Length(0.0),
                height: taffy::style::LengthPercentage::Length(0.0),
            },
            overflow: taffy::geometry::Point {
                x: taffy::style::Overflow::Visible,
                y: taffy::style::Overflow::Visible,
            },
            position: Position::Relative,
            inset: Rect::auto(),
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
            outline_style: LineStyle::None,
            outline_color: [0.0, 0.0, 0.0, 0.0],
            outline_width: 0.0,
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
            rotate_deg: 0.0,
            scale: 1.0,
            aspect_ratio: None,
            position_fixed: false,
            backdrop_blur: 0.0,
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
        taffy::style::Style {
            display: self.display,
            flex_direction: self.flex_direction,
            flex_wrap: self.flex_wrap,
            justify_content: self.justify_content,
            align_items: self.align_items,
            align_self: self.align_self,
            justify_self: self.justify_self,
            align_content: self.align_content,
            justify_items: self.justify_items,
            size: self.size,
            min_size: self.min_size,
            max_size: self.max_size,
            flex_grow: self.flex_grow,
            flex_shrink: self.flex_shrink,
            flex_basis: self.flex_basis,
            padding: self.padding,
            margin: self.margin,
            gap: self.gap,
            overflow: self.overflow,
            position: self.position,
            inset: self.inset,
            aspect_ratio: self.aspect_ratio,
            grid_template_columns: self.grid_template_columns.clone(),
            grid_template_rows: self.grid_template_rows.clone(),
            grid_auto_rows: self.grid_auto_rows.clone(),
            grid_auto_columns: self.grid_auto_columns.clone(),
            grid_auto_flow: self.grid_auto_flow,
            grid_row: self.grid_row,
            grid_column: self.grid_column,
            ..Default::default()
        }
    }
}
