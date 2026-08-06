//! Text layout / paint style (size, weight, color, decoration, shadow, align).
//! Separated from [`super::Style`] (box model / Taffy).

/// Text overflow strategy (clip / ellipsis).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TextOverflow {
    #[default]
    Clip,
    Ellipsis,
}

/// Paragraph direction MVP: Unicode marks + align; bidi still handled by cosmic-text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TextDirection {
    #[default]
    Auto,
    Ltr,
    Rtl,
}

/// Rich-text span with optional color / weight / size overrides.
#[derive(Clone, Debug, PartialEq)]
pub struct TextSpan {
    pub text: String,
    pub color: Option<[u8; 4]>,
    pub weight: Option<u16>,
    pub size: Option<f32>,
}

impl TextSpan {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            color: None,
            weight: None,
            size: None,
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
    /// Overflow when the single-line box is too narrow.
    pub overflow: TextOverflow,
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
    use super::{parse_text_shadow, TextOverflow, TextStyle};

    #[test]
    fn text_style_overflow_and_selectable_defaults() {
        let s = TextStyle::default();
        assert_eq!(s.overflow, TextOverflow::Clip);
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
