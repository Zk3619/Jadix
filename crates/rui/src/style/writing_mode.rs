//! CSS [`writing-mode`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/writing-mode)
//! (Writing Modes Level 4) plus logical-edge → physical mapping.
//!
//! Taffy 0.13 has no `writing-mode` (only `direction` for horizontal-tb). Vertical
//! modes remap flex axes and logical properties at bake time; text is measured on
//! the inline axis then painted with a ±90° bake rotation.

use taffy::prelude::*;
use taffy::style::Direction;

use super::text_style::TextDirection;

/// CSS `writing-mode` used value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum WritingMode {
    /// `horizontal-tb` (initial). Inline = X, block = +Y.
    #[default]
    HorizontalTb,
    /// `vertical-rl`. Inline = Y, block = −X.
    VerticalRl,
    /// `vertical-lr`. Inline = Y, block = +X.
    VerticalLr,
    /// `sideways-rl`. Same axes as `vertical-rl`; glyphs always sideways.
    SidewaysRl,
    /// `sideways-lr`. Inline start is the bottom for LTR.
    SidewaysLr,
}

impl WritingMode {
    /// Parse CSS keywords including SVG1 aliases (`lr`, `tb-rl`, …).
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "horizontal-tb" | "lr" | "lr-tb" | "rl" | "rl-tb" => Some(Self::HorizontalTb),
            "vertical-rl" | "tb-rl" => Some(Self::VerticalRl),
            "vertical-lr" | "tb" | "tb-lr" => Some(Self::VerticalLr),
            "sideways-rl" => Some(Self::SidewaysRl),
            "sideways-lr" => Some(Self::SidewaysLr),
            _ => None,
        }
    }

    pub fn as_css(self) -> &'static str {
        match self {
            Self::HorizontalTb => "horizontal-tb",
            Self::VerticalRl => "vertical-rl",
            Self::VerticalLr => "vertical-lr",
            Self::SidewaysRl => "sideways-rl",
            Self::SidewaysLr => "sideways-lr",
        }
    }

    #[inline]
    pub fn is_horizontal(self) -> bool {
        matches!(self, Self::HorizontalTb)
    }

    #[inline]
    pub fn is_vertical(self) -> bool {
        !self.is_horizontal()
    }

    /// Glyph rotation applied after horizontal shaping (Bake path).
    pub fn glyph_rotation_rad(self) -> f32 {
        match self {
            Self::HorizontalTb => 0.0,
            Self::VerticalRl | Self::SidewaysRl => std::f32::consts::FRAC_PI_2,
            Self::VerticalLr | Self::SidewaysLr => -std::f32::consts::FRAC_PI_2,
        }
    }

    fn rtl(dir: TextDirection) -> bool {
        matches!(dir, TextDirection::Rtl)
    }

    /// Physical side for `*-inline-start`.
    pub fn inline_start(self, dir: TextDirection) -> PhysicalSide {
        let rtl = Self::rtl(dir);
        match self {
            Self::HorizontalTb => {
                if rtl {
                    PhysicalSide::Right
                } else {
                    PhysicalSide::Left
                }
            }
            Self::VerticalRl | Self::VerticalLr | Self::SidewaysRl => {
                if rtl {
                    PhysicalSide::Bottom
                } else {
                    PhysicalSide::Top
                }
            }
            Self::SidewaysLr => {
                if rtl {
                    PhysicalSide::Top
                } else {
                    PhysicalSide::Bottom
                }
            }
        }
    }

    pub fn inline_end(self, dir: TextDirection) -> PhysicalSide {
        self.inline_start(dir).opposite()
    }

    /// Physical side for `*-block-start` (`direction` does not flip the block axis).
    pub fn block_start(self) -> PhysicalSide {
        match self {
            Self::HorizontalTb => PhysicalSide::Top,
            Self::VerticalRl | Self::SidewaysRl => PhysicalSide::Right,
            Self::VerticalLr | Self::SidewaysLr => PhysicalSide::Left,
        }
    }

    pub fn block_end(self) -> PhysicalSide {
        self.block_start().opposite()
    }

    /// Map specified `flex-direction` to Taffy's physical main axis.
    ///
    /// `horizontal-tb` keeps the specified value — Taffy `direction: rtl` handles row reverse.
    /// Vertical modes encode start edges as Row/Column ± Reverse and should keep Taffy `direction` LTR.
    pub fn map_flex_direction(self, dir: TextDirection, flex: FlexDirection) -> FlexDirection {
        let rtl = Self::rtl(dir);
        match self {
            Self::HorizontalTb => flex,
            Self::VerticalRl | Self::SidewaysRl => match flex {
                FlexDirection::Row => {
                    if rtl {
                        FlexDirection::ColumnReverse
                    } else {
                        FlexDirection::Column
                    }
                }
                FlexDirection::RowReverse => {
                    if rtl {
                        FlexDirection::Column
                    } else {
                        FlexDirection::ColumnReverse
                    }
                }
                FlexDirection::Column => FlexDirection::RowReverse,
                FlexDirection::ColumnReverse => FlexDirection::Row,
            },
            Self::VerticalLr => match flex {
                FlexDirection::Row => {
                    if rtl {
                        FlexDirection::ColumnReverse
                    } else {
                        FlexDirection::Column
                    }
                }
                FlexDirection::RowReverse => {
                    if rtl {
                        FlexDirection::Column
                    } else {
                        FlexDirection::ColumnReverse
                    }
                }
                FlexDirection::Column => FlexDirection::Row,
                FlexDirection::ColumnReverse => FlexDirection::RowReverse,
            },
            Self::SidewaysLr => match flex {
                FlexDirection::Row => {
                    if rtl {
                        FlexDirection::Column
                    } else {
                        FlexDirection::ColumnReverse
                    }
                }
                FlexDirection::RowReverse => {
                    if rtl {
                        FlexDirection::ColumnReverse
                    } else {
                        FlexDirection::Column
                    }
                }
                FlexDirection::Column => FlexDirection::Row,
                FlexDirection::ColumnReverse => FlexDirection::RowReverse,
            },
        }
    }

    /// Taffy Column cross-start is physical left; CSS `vertical-rl` block-start is right.
    pub fn flip_flex_cross_start(self, flex: FlexDirection) -> bool {
        match self {
            Self::VerticalRl | Self::SidewaysRl => {
                matches!(flex, FlexDirection::Row | FlexDirection::RowReverse)
            }
            _ => false,
        }
    }
}

/// Physical box edge (CSS TRBL).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhysicalSide {
    Top,
    Right,
    Bottom,
    Left,
}

impl PhysicalSide {
    pub fn opposite(self) -> Self {
        match self {
            Self::Top => Self::Bottom,
            Self::Right => Self::Left,
            Self::Bottom => Self::Top,
            Self::Left => Self::Right,
        }
    }
}

pub fn flip_align_items(v: AlignItems) -> AlignItems {
    AlignItems {
        keyword: match v.keyword {
            AlignItemsKeyword::Start => AlignItemsKeyword::End,
            AlignItemsKeyword::End => AlignItemsKeyword::Start,
            AlignItemsKeyword::FlexStart => AlignItemsKeyword::FlexEnd,
            AlignItemsKeyword::FlexEnd => AlignItemsKeyword::FlexStart,
            AlignItemsKeyword::SelfStart => AlignItemsKeyword::SelfEnd,
            AlignItemsKeyword::SelfEnd => AlignItemsKeyword::SelfStart,
            other => other,
        },
        safety: v.safety,
    }
}

pub fn flip_align_content(v: AlignContent) -> AlignContent {
    AlignContent {
        keyword: match v.keyword {
            AlignContentKeyword::Start => AlignContentKeyword::End,
            AlignContentKeyword::End => AlignContentKeyword::Start,
            AlignContentKeyword::FlexStart => AlignContentKeyword::FlexEnd,
            AlignContentKeyword::FlexEnd => AlignContentKeyword::FlexStart,
            other => other,
        },
        safety: v.safety,
    }
}

pub fn taffy_direction(wm: WritingMode, dir: TextDirection) -> Direction {
    if wm.is_horizontal() && matches!(dir, TextDirection::Rtl) {
        Direction::Rtl
    } else {
        Direction::Ltr
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_keywords_and_svg_aliases() {
        assert_eq!(
            WritingMode::parse("horizontal-tb"),
            Some(WritingMode::HorizontalTb)
        );
        assert_eq!(
            WritingMode::parse("vertical-rl"),
            Some(WritingMode::VerticalRl)
        );
        assert_eq!(
            WritingMode::parse("vertical-lr"),
            Some(WritingMode::VerticalLr)
        );
        assert_eq!(
            WritingMode::parse("sideways-rl"),
            Some(WritingMode::SidewaysRl)
        );
        assert_eq!(
            WritingMode::parse("sideways-lr"),
            Some(WritingMode::SidewaysLr)
        );
        assert_eq!(WritingMode::parse("tb-rl"), Some(WritingMode::VerticalRl));
        assert_eq!(WritingMode::parse("lr-tb"), Some(WritingMode::HorizontalTb));
        assert_eq!(WritingMode::parse("nope"), None);
    }

    #[test]
    fn vertical_rl_logical_edges() {
        let wm = WritingMode::VerticalRl;
        assert_eq!(wm.inline_start(TextDirection::Ltr), PhysicalSide::Top);
        assert_eq!(wm.inline_end(TextDirection::Ltr), PhysicalSide::Bottom);
        assert_eq!(wm.block_start(), PhysicalSide::Right);
        assert_eq!(wm.block_end(), PhysicalSide::Left);
        assert_eq!(wm.inline_start(TextDirection::Rtl), PhysicalSide::Bottom);
    }

    #[test]
    fn horizontal_rtl_inline_start_is_right() {
        assert_eq!(
            WritingMode::HorizontalTb.inline_start(TextDirection::Rtl),
            PhysicalSide::Right
        );
    }

    #[test]
    fn vertical_rl_flex_row_is_column() {
        let mapped =
            WritingMode::VerticalRl.map_flex_direction(TextDirection::Ltr, FlexDirection::Row);
        assert_eq!(mapped, FlexDirection::Column);
        assert!(WritingMode::VerticalRl.flip_flex_cross_start(FlexDirection::Row));
    }
}
