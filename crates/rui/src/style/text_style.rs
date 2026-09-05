//! Text layout / paint style (size, weight, color, decoration, shadow, align).
//! Separated from [`super::Style`] (box model / Taffy).

/// Text overflow: CSS `clip` / `ellipsis` / `hidden`, plus `newline` for wrapping.
///
/// - [`Self::Hidden`]: native `<input>` — crop the line; caret / selection may pan.
///   **Default for Input and Combobox.** Not for static captions.
/// - [`Self::Clip`]: static single-line crop (no `…`, no pan). Label / Button.
/// - [`Self::Ellipsis`]: static single-line trailing `…` (`truncate`). Label / Button.
/// - [`Self::Newline`]: soft-wrap (Label / Textarea default).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TextOverflow {
    #[default]
    Clip,
    Hidden,
    Ellipsis,
    Newline,
}

impl TextOverflow {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "hidden" => Self::Hidden,
            "ellipsis" | "truncate" => Self::Ellipsis,
            "newline" | "wrap" | "break" => Self::Newline,
            _ => Self::Clip,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Clip => "clip",
            Self::Hidden => "hidden",
            Self::Ellipsis => "ellipsis",
            Self::Newline => "newline",
        }
    }

    /// Clip / hidden / ellipsis stay one line; newline wraps.
    pub fn nowrap(self) -> bool {
        !matches!(self, Self::Newline)
    }

    /// Native single-line field (`<input>` / Combobox trigger): crop + caret pan.
    pub fn editable_single_line() -> Self {
        Self::Hidden
    }

    /// Input-like fields: overflow is clipped, but the caret may pan the line.
    pub fn allows_caret_scroll(self) -> bool {
        matches!(self, Self::Hidden)
    }

    /// CSS `text-overflow` on an overflowing nowrap line: keep inline-start in view
    /// (`clip` / `ellipsis` / `hidden`). Fitting lines still honor `text-align`.
    pub fn start_align_when_line_overflows(self) -> bool {
        matches!(self, Self::Clip | Self::Hidden | Self::Ellipsis)
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

/// Paragraph direction MVP: Unicode RLM for explicit RTL + align; bidi still handled by cosmic-text.
/// `Ltr` / `Auto` do **not** inject U+200E (some fonts render LRM as a visible glyph).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TextDirection {
    #[default]
    Auto,
    Ltr,
    Rtl,
}

/// CSS [`text-align`](https://developer.mozilla.org/docs/Web/CSS/text-align) specified value.
///
/// Keywords follow MDN: `start` | `end` | `left` | `right` | `center` | `justify` |
/// `justify-all` | `match-parent`, plus vendor `-moz-center` / `-webkit-center` → [`Self::Center`].
/// Global keywords (`inherit` / `initial` / `revert` / `revert-layer` / `unset`) are handled by
/// [`Self::parse_declaration`] — not stored on this enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TextAlign {
    /// CSS initial / writing-mode start edge.
    #[default]
    Start,
    End,
    Left,
    Right,
    Center,
    Justify,
    /// Treated like [`Self::Justify`] for glyphon (no per-last-line force yet).
    JustifyAll,
    /// Resolve against the parent's `text-align` + direction at cascade time.
    MatchParent,
}

/// Result of parsing a CSS `text-align` declaration (including global keywords).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextAlignDeclaration {
    Value(TextAlign),
    /// `inherit` / `unset` / `revert` / `revert-layer` — clear local declaration, cascade inherits.
    Inherit,
    /// `initial` → [`TextAlign::Start`].
    Initial,
}

impl TextAlign {
    /// Parse a keyword (case-insensitive). Unknown tokens fall back to [`Self::Start`].
    pub fn parse(s: &str) -> Self {
        match Self::parse_declaration(s) {
            TextAlignDeclaration::Value(v) => v,
            TextAlignDeclaration::Initial | TextAlignDeclaration::Inherit => Self::Start,
        }
    }

    /// Full declaration parse including global CSS keywords and vendor centers.
    pub fn parse_declaration(s: &str) -> TextAlignDeclaration {
        let t = s.trim();
        if t.is_empty() {
            return TextAlignDeclaration::Value(Self::Start);
        }
        match t.to_ascii_lowercase().as_str() {
            "inherit" | "unset" | "revert" | "revert-layer" => TextAlignDeclaration::Inherit,
            "initial" => TextAlignDeclaration::Initial,
            "start" => TextAlignDeclaration::Value(Self::Start),
            "end" => TextAlignDeclaration::Value(Self::End),
            "left" => TextAlignDeclaration::Value(Self::Left),
            "right" => TextAlignDeclaration::Value(Self::Right),
            "center" | "-moz-center" | "-webkit-center" => {
                TextAlignDeclaration::Value(Self::Center)
            }
            "justify" => TextAlignDeclaration::Value(Self::Justify),
            "justify-all" => TextAlignDeclaration::Value(Self::JustifyAll),
            "match-parent" => TextAlignDeclaration::Value(Self::MatchParent),
            _ => TextAlignDeclaration::Value(Self::Start),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::End => "end",
            Self::Left => "left",
            Self::Right => "right",
            Self::Center => "center",
            Self::Justify => "justify",
            Self::JustifyAll => "justify-all",
            Self::MatchParent => "match-parent",
        }
    }

    /// CSS `match-parent`: inherit parent's align, but resolve `start`/`end` against the
    /// **parent's** direction (then store absolute left/right).
    pub fn resolve_match_parent(parent_align: Self, parent_dir: TextDirection) -> Self {
        match parent_align {
            Self::Start | Self::MatchParent => match parent_dir {
                TextDirection::Rtl => Self::Right,
                _ => Self::Left,
            },
            Self::End => match parent_dir {
                TextDirection::Rtl => Self::Left,
                _ => Self::Right,
            },
            other => other,
        }
    }

    /// Used value → glyphon / cosmic-text `Align`.
    pub fn to_glyphon(self, direction: TextDirection) -> glyphon::cosmic_text::Align {
        match self {
            Self::Start | Self::MatchParent => match direction {
                TextDirection::Rtl => glyphon::cosmic_text::Align::Right,
                _ => glyphon::cosmic_text::Align::Left,
            },
            Self::End => glyphon::cosmic_text::Align::End,
            Self::Left => glyphon::cosmic_text::Align::Left,
            Self::Right => glyphon::cosmic_text::Align::Right,
            Self::Center => glyphon::cosmic_text::Align::Center,
            Self::Justify | Self::JustifyAll => glyphon::cosmic_text::Align::Justified,
        }
    }
}

impl From<&str> for TextAlign {
    fn from(s: &str) -> Self {
        Self::parse(s)
    }
}

impl From<String> for TextAlign {
    fn from(s: String) -> Self {
        Self::parse(&s)
    }
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
///
/// Inheritable fields use `None` = CSS **unset** (inherit from ancestor during
/// [`crate::layout::layout_tree::cascade_inheritable_text_style`]). Setting
/// `text_style.color = Some(c)` is equivalent to Web `color: c` — no parallel
/// `text_color` attrs required.
#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    pub font_family: Option<String>,
    /// `None` = inherit.
    pub color: Option<[u8; 4]>,
    /// `None` = inherit.
    pub size: Option<f32>,
    /// `None` = inherit.
    pub weight: Option<u16>,
    /// `None` = inherit.
    pub underline: Option<bool>,
    /// `None` = inherit.
    pub overline: Option<bool>,
    /// `None` = inherit.
    pub line_through: Option<bool>,
    /// `None` = inherit.
    pub decoration_style: Option<TextDecorationStyle>,
    /// `None` = CSS `currentColor`（绘制时用本节点 `color`）；`Some` = 显式装饰色。
    pub decoration_color: Option<[u8; 4]>,
    /// When false, [`decoration_color`] is inherited from the ancestor cascade.
    pub decoration_color_specified: bool,
    /// `None` = inherit.
    pub decoration_thickness: Option<f32>,
    /// CSS `text-underline-offset`（逻辑 px）：正值让下划线离文字更远（向下）。
    /// `None` = inherit.
    pub underline_offset: Option<f32>,
    pub shadow_color: Option<[u8; 4]>,
    pub shadow_offset: [f32; 2],
    pub shadow_blur: f32,
    /// Absolute line height in logical px.
    pub line_height_px: Option<f32>,
    /// Line-height scale relative to font size. `None` = default policy.
    pub line_height_scale: Option<f32>,
    /// CSS `text-align` (see [`TextAlign`]). `None` = inherit.
    pub align: Option<TextAlign>,
    /// Overflow when the box is too small (`clip` / `hidden` / `ellipsis` / `newline`).
    pub overflow: TextOverflow,
    /// Soft-wrap policy (`normal` | `nowrap`). Button captions use `Nowrap` (ShadCN).
    pub white_space: WhiteSpace,
    /// Mid-line break mode when soft-wrapping (`word` / `char` / `none`).
    /// Must match paint + caret chrome (`wrap=char` → SoftChar visual lines).
    pub wrap_mode: crate::text::WrapMode,
    /// Whether static text allows drag-select.
    pub selectable: bool,
    /// Paragraph direction (Auto / LTR / RTL).
    pub direction: TextDirection,
    /// Letter spacing in logical px (hair-space approx on cosmic-text 0.12).
    pub letter_spacing: f32,
    /// Optional caret drawn in prepare_text.
    pub input_caret: Option<InputCaretDraw>,
    /// Horizontal pan (logical px) for [`TextOverflow::Hidden`] single-line fields.
    pub scroll_x: f32,
    /// Set by [`crate::core::tree::Element::text_line_height`] / `text_leading*`.
    pub(crate) line_height_specified: bool,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font_family: None,
            color: None,
            size: None,
            weight: None,
            underline: None,
            overline: None,
            line_through: None,
            decoration_style: None,
            decoration_color: None,
            decoration_color_specified: false,
            decoration_thickness: None,
            underline_offset: None,
            shadow_color: None,
            shadow_offset: [0.0, 0.0],
            shadow_blur: 0.0,
            line_height_px: None,
            line_height_scale: None,
            align: None,
            overflow: TextOverflow::Clip,
            white_space: WhiteSpace::Normal,
            wrap_mode: crate::text::WrapMode::Word,
            selectable: false,
            direction: TextDirection::Auto,
            letter_spacing: 0.0,
            input_caret: None,
            scroll_x: 0.0,
            line_height_specified: false,
        }
    }
}

impl TextStyle {
    /// Root ancestor defaults when no parent supplies inheritable values.
    pub fn cascade_root() -> Self {
        let defaults = crate::utils::font::text_defaults();
        Self {
            color: Some(super::parse_text_color("foreground")),
            size: Some(defaults.size_px),
            weight: Some(defaults.weight),
            font_family: Some(crate::utils::font::default_ui_sans_family_list().to_string()),
            align: Some(TextAlign::Start),
            underline: Some(false),
            overline: Some(false),
            line_through: Some(false),
            decoration_style: Some(TextDecorationStyle::Solid),
            decoration_thickness: Some(0.0),
            underline_offset: Some(1.0),
            ..Default::default()
        }
    }

    /// Resolved values after cascade (or for paint when cascade already ran).
    #[inline]
    pub fn color_resolved(&self) -> [u8; 4] {
        self.color
            .unwrap_or_else(|| super::parse_text_color("foreground"))
    }

    #[inline]
    pub fn size_resolved(&self) -> f32 {
        self.size
            .unwrap_or_else(|| crate::utils::font::text_defaults().size_px)
    }

    #[inline]
    pub fn weight_resolved(&self) -> u16 {
        self.weight
            .unwrap_or_else(|| crate::utils::font::text_defaults().weight)
    }

    #[inline]
    pub fn align_resolved(&self) -> TextAlign {
        self.align.unwrap_or(TextAlign::Start)
    }

    #[inline]
    pub fn underline_resolved(&self) -> bool {
        self.underline.unwrap_or(false)
    }

    #[inline]
    pub fn overline_resolved(&self) -> bool {
        self.overline.unwrap_or(false)
    }

    #[inline]
    pub fn line_through_resolved(&self) -> bool {
        self.line_through.unwrap_or(false)
    }

    #[inline]
    pub fn decoration_style_resolved(&self) -> TextDecorationStyle {
        self.decoration_style.unwrap_or(TextDecorationStyle::Solid)
    }

    #[inline]
    pub fn decoration_thickness_resolved(&self) -> f32 {
        self.decoration_thickness.unwrap_or(0.0)
    }

    #[inline]
    pub fn underline_offset_resolved(&self) -> f32 {
        self.underline_offset.unwrap_or(1.0)
    }

    /// Effective inheritable values to pass as the parent for descendants.
    pub fn effective_ancestry(&self, parent: &Self) -> Self {
        let mut out = self.clone();
        out.color = self.color.or(parent.color);
        out.size = self.size.or(parent.size);
        out.weight = self.weight.or(parent.weight);
        out.align = self.align.or(parent.align);
        out.underline = self.underline.or(parent.underline);
        out.overline = self.overline.or(parent.overline);
        out.line_through = self.line_through.or(parent.line_through);
        out.decoration_style = self.decoration_style.or(parent.decoration_style);
        out.decoration_thickness = self.decoration_thickness.or(parent.decoration_thickness);
        out.underline_offset = self.underline_offset.or(parent.underline_offset);
        out.font_family = self
            .font_family
            .clone()
            .or_else(|| parent.font_family.clone());
        if !self.decoration_color_specified {
            out.decoration_color = parent.decoration_color;
        }
        if !self.line_height_specified {
            out.line_height_px = parent.line_height_px;
            out.line_height_scale = parent.line_height_scale;
        }
        out
    }

    /// Fill unset inheritable fields with resolved values (post-cascade paint / measure).
    pub fn materialize_in_place(&mut self) {
        if self.color.is_none() {
            self.color = Some(self.color_resolved());
        }
        if self.size.is_none() {
            self.size = Some(self.size_resolved());
        }
        if self.weight.is_none() {
            self.weight = Some(self.weight_resolved());
        }
        if self.align.is_none() {
            self.align = Some(self.align_resolved());
        }
        if self.underline.is_none() {
            self.underline = Some(self.underline_resolved());
        }
        if self.overline.is_none() {
            self.overline = Some(self.overline_resolved());
        }
        if self.line_through.is_none() {
            self.line_through = Some(self.line_through_resolved());
        }
        if self.decoration_style.is_none() {
            self.decoration_style = Some(self.decoration_style_resolved());
        }
        if self.decoration_thickness.is_none() {
            self.decoration_thickness = Some(self.decoration_thickness_resolved());
        }
        if self.underline_offset.is_none() {
            self.underline_offset = Some(self.underline_offset_resolved());
        }
    }

    /// Clone with all inheritable fields materialized to [`Some`] for paint/measure code paths.
    pub fn materialized(&self) -> Self {
        let mut s = self.clone();
        s.materialize_in_place();
        s
    }

    /// Horizontal offset of a line's ink origin inside `box_w` for CSS `text-align`.
    ///
    /// Matches glyphon / [`crate::renderer::prepare_text`]: when a nowrap/`hidden` line
    /// overflows, `text-overflow` keeps the inline-start edge in view (offset = 0).
    pub fn align_inline_offset(&self, line_content_w: f32, box_w: f32) -> f32 {
        let nowrap = self.white_space == WhiteSpace::Nowrap || self.overflow.nowrap();
        if self.overflow.start_align_when_line_overflows() && nowrap && line_content_w > box_w + 0.5
        {
            return 0.0;
        }
        match self.align_resolved().to_glyphon(self.direction) {
            glyphon::cosmic_text::Align::Center => (box_w - line_content_w) * 0.5,
            glyphon::cosmic_text::Align::Right | glyphon::cosmic_text::Align::End => {
                box_w - line_content_w
            }
            _ => 0.0,
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
    use super::{
        TextAlign, TextAlignDeclaration, TextDirection, TextOverflow, TextSpan, TextSpanKind,
        TextStyle, WhiteSpace, parse_text_shadow,
    };

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
        assert_eq!(s.wrap_mode, crate::text::WrapMode::Word);
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
    fn text_align_parse_mdn_keywords() {
        assert_eq!(TextAlign::parse("start"), TextAlign::Start);
        assert_eq!(TextAlign::parse("end"), TextAlign::End);
        assert_eq!(TextAlign::parse("left"), TextAlign::Left);
        assert_eq!(TextAlign::parse("right"), TextAlign::Right);
        assert_eq!(TextAlign::parse("center"), TextAlign::Center);
        assert_eq!(TextAlign::parse("JUSTIFY"), TextAlign::Justify);
        assert_eq!(TextAlign::parse("justify-all"), TextAlign::JustifyAll);
        assert_eq!(TextAlign::parse("match-parent"), TextAlign::MatchParent);
        assert_eq!(TextAlign::parse("-moz-center"), TextAlign::Center);
        assert_eq!(TextAlign::parse("-webkit-center"), TextAlign::Center);
        assert_eq!(
            TextAlign::parse_declaration("inherit"),
            TextAlignDeclaration::Inherit
        );
        assert_eq!(
            TextAlign::parse_declaration("initial"),
            TextAlignDeclaration::Initial
        );
        assert_eq!(
            TextAlign::parse_declaration("unset"),
            TextAlignDeclaration::Inherit
        );
        assert_eq!(
            TextAlign::parse_declaration("revert"),
            TextAlignDeclaration::Inherit
        );
        assert_eq!(
            TextAlign::parse_declaration("revert-layer"),
            TextAlignDeclaration::Inherit
        );
        assert_eq!(
            TextAlign::Start.to_glyphon(TextDirection::Ltr),
            glyphon::cosmic_text::Align::Left
        );
        assert_eq!(
            TextAlign::Start.to_glyphon(TextDirection::Rtl),
            glyphon::cosmic_text::Align::Right
        );
        assert_eq!(
            TextAlign::Left.to_glyphon(TextDirection::Rtl),
            glyphon::cosmic_text::Align::Left
        );
        assert_eq!(
            TextAlign::End.to_glyphon(TextDirection::Auto),
            glyphon::cosmic_text::Align::End
        );
        assert_eq!(
            TextAlign::Justify.to_glyphon(TextDirection::Auto),
            glyphon::cosmic_text::Align::Justified
        );
        assert_eq!(
            TextAlign::resolve_match_parent(TextAlign::Start, TextDirection::Rtl),
            TextAlign::Right
        );
        assert_eq!(TextStyle::default().align, None);
    }

    #[test]
    fn text_overflow_parse_clip_ellipsis_newline() {
        assert_eq!(TextOverflow::parse("clip"), TextOverflow::Clip);
        assert_eq!(TextOverflow::parse("hidden"), TextOverflow::Hidden);
        assert_eq!(TextOverflow::parse("ellipsis"), TextOverflow::Ellipsis);
        assert_eq!(TextOverflow::parse("truncate"), TextOverflow::Ellipsis);
        assert_eq!(TextOverflow::parse("newline"), TextOverflow::Newline);
        assert_eq!(TextOverflow::parse("wrap"), TextOverflow::Newline);
        assert!(TextOverflow::Clip.nowrap());
        assert!(TextOverflow::Hidden.nowrap());
        assert!(TextOverflow::Ellipsis.nowrap());
        assert!(!TextOverflow::Newline.nowrap());
        assert_eq!(TextOverflow::editable_single_line(), TextOverflow::Hidden);
        assert!(TextOverflow::Hidden.allows_caret_scroll());
        assert!(!TextOverflow::Clip.allows_caret_scroll());
        assert!(TextOverflow::Ellipsis.start_align_when_line_overflows());
        assert!(!TextOverflow::Newline.start_align_when_line_overflows());
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
