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
        return Some(v);
    }
    if let Some(raw) = t.strip_suffix("px") {
        return raw.trim().parse::<f32>().ok();
    }
    if let Some(raw) = t.strip_suffix("rem") {
        return raw.trim().parse::<f32>().ok().map(|v| v * root_font_size_px);
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
    if lens.len() >= 1 {
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineStyle {
    None,
    Solid,
}

impl Default for LineStyle {
    fn default() -> Self {
        Self::None
    }
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
    pub size: Size<Dimension>,
    pub min_size: Size<Dimension>,
    pub max_size: Size<Dimension>,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub padding: Rect<LengthPercentage>,
    pub margin: Rect<LengthPercentageAuto>,
    pub gap: taffy::geometry::Size<taffy::style::LengthPercentage>,
    pub overflow: taffy::geometry::Point<taffy::style::Overflow>,
    pub position: Position,
    pub inset: Rect<LengthPercentageAuto>,
    pub z_index: i32,

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
}

impl Default for Style {
    fn default() -> Self {
        Self {
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::NoWrap,
            justify_content: None,
            align_items: None,
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
        }
    }
}

impl Style {
    pub fn to_taffy_style(&self) -> taffy::style::Style {
        taffy::style::Style {
            display: self.display,
            flex_direction: self.flex_direction,
            flex_wrap: self.flex_wrap,
            justify_content: self.justify_content,
            align_items: self.align_items,
            size: self.size,
            min_size: self.min_size,
            max_size: self.max_size,
            flex_grow: self.flex_grow,
            flex_shrink: self.flex_shrink,
            padding: self.padding,
            margin: self.margin,
            gap: self.gap,
            overflow: self.overflow,
            position: self.position,
            inset: self.inset,
            ..Default::default()
        }
    }
}
