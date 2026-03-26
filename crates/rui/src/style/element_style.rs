//! 元素盒模型与 Taffy 映射；部分绘制属性由 GPU 路径消费，不进入 Taffy。

use taffy::prelude::*;

/// 元素布局与部分视觉属性（背景/边框/阴影等由渲染路径消费，不进入 Taffy）。
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub display: Display,
    pub flex_direction: FlexDirection,
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
    pub border_color: [f32; 4],
    pub border_width: f32,
    pub radii: [f32; 4],
    pub shadow_color: [f32; 4],
    pub shadow_blur: f32,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
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
            border_color: [0.0, 0.0, 0.0, 0.0],
            border_width: 0.0,
            radii: [0.0; 4],
            shadow_color: [0.0, 0.0, 0.0, 0.0],
            shadow_blur: 0.0,
        }
    }
}

impl Style {
    pub fn to_taffy_style(&self) -> taffy::style::Style {
        taffy::style::Style {
            display: self.display,
            flex_direction: self.flex_direction,
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
