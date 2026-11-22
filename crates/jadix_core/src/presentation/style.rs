use crate::environment::LayoutDirection;
mod appearance;
pub use appearance::*;
mod text;
pub use text::*;
mod transform;
pub use transform::*;
mod shape;
pub use shape::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    Row,
    #[default]
    Column,
}

/// Programmatic scrolling policy. Hosts honor reduced motion and user interruption.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScrollBehavior {
    #[default]
    Instant,
    Smooth,
}

/// Whether a scroll viewport lets a tap activate content while the keyboard is visible.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyboardTaps { #[default] Never, Always, Handled }

/// Native keyboard dismissal during a user scroll gesture.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyboardDismissMode { #[default] None, OnDrag, Interactive }

/// Alignment of children on a flex container's cross axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlignItems {
    Start,
    End,
    Center,
    Stretch,
    Baseline,
}

/// Text presentation policy. Editing always keeps the complete logical value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextOverflow {
    #[default]
    Wrap,
    Clip,
    Ellipsis,
    EllipsisStart,
    EllipsisMiddle,
}

impl TextOverflow {
    fn is_wrap(&self) -> bool {
        *self == Self::Wrap
    }
}

/// How a decoded image fits the content box. Intrinsic layout is independent of this setting.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageFit {
    #[default]
    Contain,
    Cover,
    Fill,
    None,
    ScaleDown,
}

impl ImageFit {
    /// Fitted destination rectangle in the same units as `target` and `source`.
    /// Hosts clip this rectangle to the content box; offsets apply after fitting.
    pub fn geometry(self, target: [f64; 2], source: [f64; 2], position: ImagePosition) -> Option<[f64; 4]> {
        if !target.into_iter().chain(source).all(|v| v.is_finite() && v > 0.0)
            || !position.fraction.into_iter().chain(position.offset).all(f32::is_finite) {
            return None;
        }
        let scale = match self {
            Self::Fill => 1.0,
            Self::Contain => (target[0] / source[0]).min(target[1] / source[1]),
            Self::Cover => (target[0] / source[0]).max(target[1] / source[1]),
            Self::None => 1.0,
            Self::ScaleDown => (target[0] / source[0]).min(target[1] / source[1]).min(1.0),
        };
        let size = if self == Self::Fill { target } else { source.map(|v| v * scale) };
        let rect = [
            (target[0] - size[0]) * f64::from(position.fraction[0]) + f64::from(position.offset[0]),
            (target[1] - size[1]) * f64::from(position.fraction[1]) + f64::from(position.offset[1]),
            size[0], size[1],
        ];
        (rect.iter().all(|v| v.is_finite()) && size.iter().all(|v| *v > 0.0)).then_some(rect)
    }
}

/// Places the fitted image within the content box from the top-left corner.
/// Each fraction multiplies the remaining space on its axis. Point offsets
/// are then added, with positive values moving right and down.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "ImagePositionValue")]
pub struct ImagePosition {
    pub fraction: [f32; 2],
    pub offset: [f32; 2],
}

#[derive(serde::Deserialize)]
#[serde(untagged, deny_unknown_fields)]
enum ImagePositionValue {
    Position {
        fraction: [f32; 2],
        offset: [f32; 2],
    },
    Css(String),
}

impl TryFrom<ImagePositionValue> for ImagePosition {
    type Error = &'static str;

    fn try_from(value: ImagePositionValue) -> Result<Self, Self::Error> {
        match value {
            ImagePositionValue::Position { fraction, offset } => Ok(Self::new(fraction, offset)),
            ImagePositionValue::Css(css) => Self::parse(&css).ok_or("invalid image position"),
        }
    }
}

impl ImagePosition {
    pub const CENTER: Self = Self::new([0.5, 0.5], [0.0, 0.0]);

    pub const fn new(fraction: [f32; 2], offset: [f32; 2]) -> Self {
        Self { fraction, offset }
    }

    /// Parses one or two positions, or two edges with offsets.
    pub fn parse(css: &str) -> Option<Self> {
        fn length(value: &str) -> Option<(f32, f32)> {
            let (number, factor, percent) = if let Some(number) = value.strip_suffix('%') {
                (number, 0.01, true)
            } else if let Some(number) = value.strip_suffix("rem") {
                (number, 16.0, false)
            } else if let Some(number) = value.strip_suffix("pt") {
                (number, 4.0 / 3.0, false)
            } else {
                (value.strip_suffix("px").unwrap_or(value), 1.0, false)
            };
            let amount = number.parse::<f32>().ok()? * factor;
            amount.is_finite().then_some(if percent {
                (amount, 0.0)
            } else {
                (0.0, amount)
            })
        }

        fn axis(word: &str, start: &str, end: &str) -> Option<(f32, f32)> {
            match word {
                "center" => Some((0.5, 0.0)),
                word if word == start => Some((0.0, 0.0)),
                word if word == end => Some((1.0, 0.0)),
                _ => length(word),
            }
        }

        let mut words = css.split_ascii_whitespace();
        let first = words.next()?;
        let horizontal = |word| axis(word, "left", "right");
        let vertical = |word| axis(word, "top", "bottom");
        let is_horizontal = |word| matches!(word, "left" | "right");
        let is_vertical = |word| matches!(word, "top" | "bottom");
        let center = (0.5, 0.0);
        let (x, y) = match (words.next(), words.next(), words.next(), words.next()) {
            (None, None, None, None) if is_vertical(first) => (center, vertical(first)?),
            (None, None, None, None) => (horizontal(first)?, center),
            (Some(second), None, None, None)
                if (is_vertical(first) && (is_horizontal(second) || second == "center"))
                    || (first == "center" && is_horizontal(second)) =>
            {
                (horizontal(second)?, vertical(first)?)
            }
            (Some(second), None, None, None) => (horizontal(first)?, vertical(second)?),
            (Some(second), Some(third), Some(fourth), None) => {
                let edge = |word, value| {
                    let (fraction, offset) = length(value)?;
                    Some(match word {
                        "left" | "top" => (fraction, offset),
                        "right" | "bottom" => (1.0 - fraction, -offset),
                        _ => return None,
                    })
                };
                if is_horizontal(first) && is_vertical(third) {
                    (edge(first, second)?, edge(third, fourth)?)
                } else if is_vertical(first) && is_horizontal(third) {
                    (edge(third, fourth)?, edge(first, second)?)
                } else {
                    return None;
                }
            }
            _ => return None,
        };
        Some(Self::new([x.0, y.0], [x.1, y.1]))
    }
}

impl Default for ImagePosition {
    fn default() -> Self {
        Self::CENTER
    }
}

/// Sorted, nonoverlapping UTF-8 ranges within a Text node. A host paints whole
/// graphemes using the style at their first byte; layout metrics remain shared.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct InlineStyle {
    pub range: [usize; 2],
    pub foreground: Option<[u8; 4]>,
    pub background: Option<[u8; 4]>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub strikethrough: Option<bool>,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PopoverAppearance {
    /// Complete visual endpoint. When set, supersedes the legacy shorthand fields.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub view: Option<ViewAppearance>,
    pub opacity: f32,
    pub scale: f32,
    /// Translation in logical points along the right and down axes.
    pub offset: [f32; 2],
    /// Optional RGBA background color for AppKit popover transitions.
    pub background: Option<[u8; 4]>,
    /// Optional clipping radius for AppKit popover transitions, in logical points.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub corner_radius: Option<f32>,
}

impl Default for PopoverAppearance {
    fn default() -> Self {
        Self {
            view: None,
            opacity: 0.0,
            scale: 1.0,
            offset: [0.0; 2],
            background: None,
            corner_radius: None,
        }
    }
}

impl PopoverAppearance {
    fn is_valid(&self) -> bool {
        self.view.as_ref().is_none_or(ViewAppearance::is_valid)
            && (0.0..=1.0).contains(&self.opacity)
            && (0.0..=16.0).contains(&self.scale)
            && self.offset.iter().all(|value| value.abs() <= 1_000_000.0)
            && self
                .corner_radius
                .is_none_or(|radius| (0.0..=1_000_000.0).contains(&radius))
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NodeStyle {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<TextAppearance>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub view: Option<ViewAppearance>,
    #[serde(skip_serializing_if = "TextOverflow::is_wrap")]
    pub text_overflow: TextOverflow,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_lines: Option<u16>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub inline_styles: Vec<InlineStyle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub popover_margin: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub popover_close_on_escape: Option<bool>,
    /// Hover surfaces keep focus on their trigger; menus and dialogs focus content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub popover_focus_on_open: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub popover_enter_ms: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub popover_exit_ms: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub popover_starting: Option<PopoverAppearance>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub popover_ending: Option<PopoverAppearance>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_drag: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_fit: Option<ImageFit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_position: Option<ImagePosition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_placeholder: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_fade_ms: Option<u32>,
    /// Gaussian image-only blur in logical pixels.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_blur_radius: Option<f32>,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub min_width: f32,
    pub min_height: f32,
    pub max_width: Option<f32>,
    pub max_height: Option<f32>,
    pub axis: Axis,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub align_items: Option<AlignItems>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<LayoutDirection>,
    pub gap: f32,
    pub padding: [f32; 4],
    pub grow: f32,
    pub shrink: f32,
    pub scrollbars: [bool; 2],
    /// Native indicator insets in logical points, ordered top/right/bottom/left.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scrollbar_inset: Option<[f32; 4]>,
    /// Allow scrolling past the document bounds on hosts with elastic scrolling.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scroll_bounces: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keyboard_taps: Option<KeyboardTaps>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scroll_refresh: Option<super::scroll::RefreshControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scroll_snap: Option<super::scroll::ScrollSnap>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scroll_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keyboard_dismiss_mode: Option<KeyboardDismissMode>,
    /// Optional document viewport magnification; does not change layout size.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scroll_zoom: Option<crate::scroll_zoom::ScrollZoom>,
    /// One-shot document offset. Reapplying an unchanged target preserves user scrolling.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scroll_to: Option<[f64; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scroll_behavior: Option<ScrollBehavior>,
    pub foreground: [u8; 4],
    pub background: [u8; 4],
    pub font_family: String,
    pub font_size: f32,
    pub font_weight: u16,
}

impl Default for NodeStyle {
    fn default() -> Self {
        Self {
            text: None,
            view: None,
            text_overflow: TextOverflow::Wrap,
            max_lines: None,
            inline_styles: Vec::new(),
            popover_margin: None,
            popover_close_on_escape: None,
            popover_focus_on_open: None,
            popover_enter_ms: None,
            popover_exit_ms: None,
            popover_starting: None,
            popover_ending: None,
            window_drag: None,
            image_fit: None,
            image_position: None,
            image_placeholder: None,
            image_fade_ms: None,
            image_blur_radius: None,
            width: None,
            height: None,
            min_width: 0.0,
            min_height: 0.0,
            max_width: None,
            max_height: None,
            axis: Axis::Column,
            align_items: None,
            direction: None,
            gap: 0.0,
            padding: [0.0; 4],
            grow: 0.0,
            shrink: 1.0,
            scrollbars: [true; 2],
            scrollbar_inset: None,
            scroll_bounces: None,
            keyboard_taps: None,
            scroll_refresh: None,
            scroll_snap: None,
            scroll_enabled: None,
            keyboard_dismiss_mode: None,
            scroll_zoom: None,
            scroll_to: None,
            scroll_behavior: None,
            foreground: [0, 0, 0, 255],
            background: [0, 0, 0, 0],
            font_family: String::from("system-ui"),
            font_size: 16.0,
            font_weight: 400,
        }
    }
}

impl NodeStyle {
    pub fn inline_valid(&self, text: &str) -> bool {
        self.inline_styles.iter().all(|s| {
            s.range[1] <= text.len()
                && text.is_char_boundary(s.range[0])
                && text.is_char_boundary(s.range[1])
        })
    }
    pub fn is_valid(&self) -> bool {
        let nonnegative = |value: f32| value.is_finite() && value >= 0.0;
        self.image_blur_radius
            .is_none_or(|radius| radius.is_finite() && (0.0..=1024.0).contains(&radius))
            && self.scrollbar_inset.is_none_or(|edges| edges.into_iter().all(|v| v.is_finite() && v.abs() <= 1_000_000.0))
            && self.scroll_refresh.is_none_or(super::scroll::RefreshControl::is_valid)
            && self.scroll_snap.as_ref().is_none_or(super::scroll::ScrollSnap::is_valid)
            && self.scroll_zoom.is_none_or(crate::scroll_zoom::ScrollZoom::is_valid)
            && self.scroll_to.is_none_or(|point| point.iter().all(|v| v.is_finite() && *v >= 0.0 && *v <= f32::MAX as f64))
            && self.max_lines != Some(0)
            && self.view.as_ref().is_none_or(ViewAppearance::is_valid)
            && self.text.as_ref().is_none_or(TextAppearance::is_valid)
            && self
                .popover_starting
                .iter()
                .chain(&self.popover_ending)
                .all(PopoverAppearance::is_valid)
            && self.image_position.is_none_or(|position| {
                position
                    .fraction
                    .into_iter()
                    .chain(position.offset)
                    .all(f32::is_finite)
            })
            && self.inline_styles.len() <= 64
            && self
                .inline_styles
                .iter()
                .all(|s| s.range[0] < s.range[1] && s.range[1] <= 1024 * 1024)
            && self
                .inline_styles
                .windows(2)
                .all(|s| s[0].range[1] <= s[1].range[0])
            && [self.width, self.height, self.max_width, self.max_height]
                .into_iter()
                .flatten()
                .all(nonnegative)
            && [
                self.min_width,
                self.min_height,
                self.gap,
                self.grow,
                self.shrink,
            ]
            .into_iter()
            .chain(self.padding)
            .all(nonnegative)
            && self.max_width.is_none_or(|max| max >= self.min_width)
            && self.max_height.is_none_or(|max| max >= self.min_height)
            && self.font_size.is_finite()
            && self.font_size > 0.0
            && (1..=1000).contains(&self.font_weight)
            && !self.font_family.is_empty()
            && self.font_family.len() <= 1024
    }
}
