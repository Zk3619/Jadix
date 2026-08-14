//! Text layout / paint style (size, weight, color, decoration, shadow, align).
//! Separated from [`super::Style`] (box model / Taffy).

/// Text overflow: CSS `clip` / `ellipsis`, plus `newline` for wrapping.
///
/// - [`Self::Clip`]: single line, cut overflow (no `…`).
/// - [`Self::Ellipsis`]: single line, trailing `…` (`truncate` / `text-overflow: ellipsis`).
/// - [`Self::Newline`]: soft-wrap onto new lines (`text-overflow: newline`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TextOverflow {
    #[default]
    Clip,
    Ellipsis,
    Newline,
}

impl TextOverflow {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "ellipsis" | "truncate" => Self::Ellipsis,
            "newline" | "wrap" | "break" => Self::Newline,
            _ => Self::Clip,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Clip => "clip",
            Self::Ellipsis => "ellipsis",
            Self::Newline => "newline",
        }
    }

    /// Clip / ellipsis stay one line; newline wraps.
    pub fn nowrap(self) -> bool {
        !matches!(self, Self::Newline)
    }

    /// `truncate=true` wins; otherwise parse `text-overflow` / `text_overflow`.
    pub fn resolve_prop(truncate: bool, attr: Option<&str>) -> Option<Self> {
        if truncate {
            Some(Self::Ellipsis)
        } else {
            attr.filter(|s| !s.trim().is_empty()).map(Self::parse)
        }
    }
}

impl From<&str> for TextOverflow {
    fn from(s: &str) -> Self {
        Self::parse(s)
    }
}

impl From<String> for TextOverflow {
    fn from(s: String) -> Self {
        Self::parse(&s)
    }
}

/// CSS `white-space` MVP for text leaves (`normal` soft-wraps; `nowrap` stays one line).
///
/// Aligns with Tailwind `whitespace-nowrap` / ShadCN Button base classes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum WhiteSpace {
    #[default]
    Normal,
    Nowrap,
}

/// Paragraph direction MVP: Unicode marks + align; bidi still handled by cosmic-text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TextDirection {
    #[default]
    Auto,
    Ltr,
    Rtl,
}

/// Rich-inline span kind. Chip / Mention are atomic measure/break units.
///
/// Cosmic-text `Attrs::metadata` convention (`rui_chip`): for Chip/Mention spans,
/// prepare_text sets `metadata = span_index + 1` (nonzero). Plain Text keeps `0`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TextSpanKind {
    #[default]
    Text,
    Chip,
    Mention,
}

/// Rich-text span with optional color / weight / size overrides and chip/mention chrome.
#[derive(Clone, Debug, PartialEq)]
pub struct TextSpan {
    pub text: String,
    pub color: Option<[u8; 4]>,
    pub weight: Option<u16>,
    pub size: Option<f32>,
    pub kind: TextSpanKind,
    pub background: Option<[u8; 4]>,
    /// Horizontal padding; Chip/Mention measure width adds `2 * padding_x`.
    pub padding_x: f32,
    pub italic: bool,
    pub monospace: bool,
    pub underline: bool,
    /// Optional hyperlink target (markdown `[label](url)`).
    pub href: Option<String>,
}

impl TextSpan {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            color: None,
            weight: None,
            size: None,
            kind: TextSpanKind::Text,
            background: None,
            padding_x: 0.0,
            italic: false,
            monospace: false,
            underline: false,
            href: None,
        }
    }

    pub fn color(mut self, color: [u8; 4]) -> Self {
        self.color = Some(color);
        self
    }

    pub fn weight(mut self, weight: u16) -> Self {
        self.weight = Some(weight);
        self
    }

    pub fn size(mut self, size: f32) -> Self {
        self.size = Some(size);
        self
    }

    pub fn chip(mut self) -> Self {
        self.kind = TextSpanKind::Chip;
        self
    }

    pub fn mention(mut self) -> Self {
        self.kind = TextSpanKind::Mention;
        // Distinct from chip: soft blue pill chrome when caller omits overrides.
        if self.background.is_none() {
            self.background = Some([37, 99, 235, 40]);
        }
        if self.color.is_none() {
            self.color = Some([37, 99, 235, 255]);
        }
        if self.padding_x == 0.0 {
            self.padding_x = 4.0;
        }
        self
    }

    pub fn background(mut self, color: [u8; 4]) -> Self {
        self.background = Some(color);
        self
    }

    pub fn padding_x(mut self, padding_x: f32) -> Self {
        self.padding_x = padding_x;
        self
    }

    pub fn italic(mut self) -> Self {
        self.italic = true;
        self
    }

    pub fn monospace(mut self) -> Self {
        self.monospace = true;
        self
    }

    pub fn underline(mut self) -> Self {
        self.underline = true;
        self
    }

    pub fn href(mut self, url: impl Into<String>) -> Self {
        self.href = Some(url.into());
        self
    }
}

/// Text decoration style (CSS `text-decoration-style`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextDecorationStyle {
    Solid,
    Double,
    Dotted,
    Dashed,
    Wavy,
}

/// Caret draw params for single-line inputs.
#[derive(Clone, Debug, PartialEq)]
pub struct InputCaretDraw {
    /// UTF-8 byte index in display text; caret is drawn before this index.
    pub caret_byte_index: usize,
    /// Selection `[start, end)` in UTF-8 bytes; `None`/empty skips highlight.
    pub selection: Option<(usize, usize)>,
    /// Selection background color.
    pub selection_bg: [u8; 4],
    /// Overwrite-mode highlight `[start, end)`.
    pub overwrite_highlight: Option<(usize, usize)>,
}

/// Per-run text style shared by Element and TextLayoutInfo.
#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    pub font_family: Option<String>,
    pub color: [u8; 4],
    pub size: f32,
    pub weight: u16,
    pub underline: bool,
    pub overline: bool,
    pub line_through: bool,
    pub decoration_style: TextDecorationStyle,
    /// `None` = CSS `currentColor`（绘制时用本节点 `color`）；`Some` = 显式装饰色。
    pub decoration_color: Option<[u8; 4]>,
    pub decoration_thickness: f32,
    pub underline_offset: f32,
    pub shadow_color: Option<[u8; 4]>,
    pub shadow_offset: [f32; 2],
    pub shadow_blur: f32,
    /// Absolute line height in logical px.
    pub line_height_px: Option<f32>,
    /// Line-height scale relative to font size. `None` = default policy.
    pub line_height_scale: Option<f32>,
    pub align: glyphon::cosmic_text::Align,
    /// Overflow when the box is too small (`clip` / `ellipsis` / `newline`).
    pub overflow: TextOverflow,
    /// Soft-wrap policy (`normal` | `nowrap`). Button captions use `Nowrap` (ShadCN).
    pub white_space: WhiteSpace,
    /// Whether static text allows drag-select.
    pub selectable: bool,
    /// Paragraph direction (Auto / LTR / RTL).
    pub direction: TextDirection,
    /// Letter spacing in logical px (hair-space approx on cosmic-text 0.12).
    pub letter_spacing: f32,
    /// Optional caret drawn in prepare_text.
    pub input_caret: Option<InputCaretDraw>,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font_family: None,
            color: [255, 255, 255, 255],
            size: 16.0,
            weight: 400,
            underline: false,
            overline: false,
            line_through: false,
            decoration_style: TextDecorationStyle::Solid,
            decoration_color: None,
            decoration_thickness: 0.0,
            underline_offset: 1.0,
            shadow_color: None,
            shadow_offset: [0.0, 0.0],
            shadow_blur: 0.0,
            line_height_px: None,
            line_height_scale: None,
            align: glyphon::cosmic_text::Align::Left,
            overflow: TextOverflow::Clip,
            white_space: WhiteSpace::Normal,
            selectable: false,
            direction: TextDirection::Auto,
            letter_spacing: 0.0,
            input_caret: None,
        }
    }
}

fn parse_css_len_px(token: &str, root_font_size_px: f32) -> Option<f32> {
    let t = token.trim();
    if t.is_empty() {
        return None;
    }
    if let Ok(v) = t.parse::<f32>() {
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

/// Parse a single `text-shadow` (no comma-separated layers).
pub fn parse_text_shadow(s: impl AsRef<str>) -> (Option<[u8; 4]>, [f32; 2], f32) {
    let raw = s.as_ref().trim();
    if raw.is_empty() || raw.eq_ignore_ascii_case("none") {
        return (None, [0.0, 0.0], 0.0);
    }

    let root_font_size_px = 16.0_f32;
    let mut lens: Vec<f32> = Vec::with_capacity(3);
    let mut color: Option<[u8; 4]> = None;
    for tok in raw.split_whitespace() {
        if let Some(v) = parse_css_len_px(tok, root_font_size_px) {
            lens.push(v);
        } else {
            color = Some(super::parse_text_color(tok));
        }
    }

    let mut offset = [0.0, 0.0];
    let mut blur = 0.0;
    if lens.len() >= 2 {
        offset = [lens[0], lens[1]];
    }
    if lens.len() >= 3 {
        blur = lens[2].max(0.0);
    }
    (color, offset, blur)
}

#[cfg(test)]
mod tests {
    use super::{parse_text_shadow, TextOverflow, TextSpan, TextSpanKind, TextStyle, WhiteSpace};

    #[test]
    fn text_span_chip_builder() {
        let s = TextSpan::new("alice")
            .chip()
            .background([40, 40, 50, 255])
            .padding_x(6.0)
            .color([200, 200, 255, 255]);
        assert_eq!(s.kind, TextSpanKind::Chip);
        assert_eq!(s.padding_x, 6.0);
        assert_eq!(s.background, Some([40, 40, 50, 255]));
        assert_eq!(s.color, Some([200, 200, 255, 255]));
        let m = TextSpan::new("@bob").mention();
        assert_eq!(m.kind, TextSpanKind::Mention);
        assert_eq!(m.padding_x, 4.0);
        assert_eq!(m.background, Some([37, 99, 235, 40]));
        assert_eq!(m.color, Some([37, 99, 235, 255]));
        let m_override = TextSpan::new("@bob")
            .mention()
            .padding_x(8.0)
            .background([1, 2, 3, 4])
            .color([5, 6, 7, 8]);
        assert_eq!(m_override.padding_x, 8.0);
        assert_eq!(m_override.background, Some([1, 2, 3, 4]));
        assert_eq!(m_override.color, Some([5, 6, 7, 8]));
        let plain = TextSpan::new("hi");
        assert_eq!(plain.kind, TextSpanKind::Text);
        assert_eq!(plain.padding_x, 0.0);
        assert!(plain.background.is_none());
    }

    #[test]
    fn text_style_overflow_and_selectable_defaults() {
        let s = TextStyle::default();
        assert_eq!(s.overflow, TextOverflow::Clip);
        assert_eq!(s.white_space, WhiteSpace::Normal);
        assert!(!s.selectable);
        assert_eq!(s.direction, super::TextDirection::Auto);
        assert_eq!(s.letter_spacing, 0.0);
    }

    #[test]
    fn element_selectable_and_overflow_roundtrip() {
        let el = crate::core::tree::Element::new()
            .text("hi")
            .selectable(true)
            .text_overflow_ellipsis();
        assert!(el.text_style.selectable);
        assert_eq!(el.text_style.overflow, TextOverflow::Ellipsis);
        assert_eq!(el.text_style.white_space, WhiteSpace::Nowrap);
        assert_eq!(el.style.overflow.x, taffy::style::Overflow::Hidden);
    }

    #[test]
    fn text_overflow_parse_clip_ellipsis_newline() {
        assert_eq!(TextOverflow::parse("clip"), TextOverflow::Clip);
        assert_eq!(TextOverflow::parse("ellipsis"), TextOverflow::Ellipsis);
        assert_eq!(TextOverflow::parse("truncate"), TextOverflow::Ellipsis);
        assert_eq!(TextOverflow::parse("newline"), TextOverflow::Newline);
        assert_eq!(TextOverflow::parse("wrap"), TextOverflow::Newline);
        assert!(TextOverflow::Clip.nowrap());
        assert!(TextOverflow::Ellipsis.nowrap());
        assert!(!TextOverflow::Newline.nowrap());
    }

    fn approx_eq(a: f32, b: f32) {
        assert!((a - b).abs() < 0.001, "expected {a} ~= {b}");
    }

    #[test]
    fn text_shadow_unsuffixed_defaults_to_rem() {
        let (_, offset, blur) = parse_text_shadow("1 0.5 0.25");
        approx_eq(offset[0], 16.0);
        approx_eq(offset[1], 8.0);
        approx_eq(blur, 4.0);
    }

    #[test]
    fn text_shadow_supports_px_rem_em_conversion() {
        let (_, offset, blur) = parse_text_shadow("4px 0.5rem 0.25em");
        approx_eq(offset[0], 4.0);
        approx_eq(offset[1], 8.0);
        approx_eq(blur, 4.0);
    }
}
