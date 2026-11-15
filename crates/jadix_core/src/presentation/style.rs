use crate::environment::LayoutDirection;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    Row,
    #[default]
    Column,
}

/// Text presentation policy. Editing always keeps the complete logical value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextOverflow {
    #[default]
    Wrap,
    Clip,
    Ellipsis,
}

impl TextOverflow {
    fn is_wrap(&self) -> bool {
        *self == Self::Wrap
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
pub struct NodeStyle {
    #[serde(skip_serializing_if = "TextOverflow::is_wrap")]
    pub text_overflow: TextOverflow,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_lines: Option<u16>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub inline_styles: Vec<InlineStyle>,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub min_width: f32,
    pub min_height: f32,
    pub max_width: Option<f32>,
    pub max_height: Option<f32>,
    pub axis: Axis,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<LayoutDirection>,
    pub gap: f32,
    pub padding: [f32; 4],
    pub grow: f32,
    pub shrink: f32,
    pub scrollbars: [bool; 2],
    pub foreground: [u8; 4],
    pub background: [u8; 4],
    pub font_family: String,
    pub font_size: f32,
    pub font_weight: u16,
}

impl Default for NodeStyle {
    fn default() -> Self {
        Self {
            text_overflow: TextOverflow::Wrap,
            max_lines: None,
            inline_styles: Vec::new(),
            width: None,
            height: None,
            min_width: 0.0,
            min_height: 0.0,
            max_width: None,
            max_height: None,
            axis: Axis::Column,
            direction: None,
            gap: 0.0,
            padding: [0.0; 4],
            grow: 0.0,
            shrink: 1.0,
            scrollbars: [true; 2],
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
        self.max_lines != Some(0)
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
