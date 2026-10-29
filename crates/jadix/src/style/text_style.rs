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

    /// Placeholder hint on a native single-line field: trailing `…` when the box is narrow.
    pub fn editable_placeholder() -> Self {
        Self::Ellipsis
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
/// Supported global keywords (`inherit` / `initial` / `unset`) are handled by
/// [`Self::parse_declaration`]. Unsupported cascade rollback keywords are rejected.
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
    /// Justify every line, including each paragraph's final line.
    JustifyAll,
    /// Resolve against the parent's `text-align` + direction at cascade time.
    MatchParent,
}

/// Result of parsing a CSS `text-align` declaration (including global keywords).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextAlignDeclaration {
    Value(TextAlign),
    /// `inherit` / `unset` — clear local declaration, cascade inherits.
    Inherit,
    /// `initial` → [`TextAlign::Start`].
    Initial,
    Invalid,
}

impl TextAlign {
    /// Parse a keyword (case-insensitive). Unknown tokens fall back to [`Self::Start`].
    pub fn parse(s: &str) -> Self {
        match Self::parse_declaration(s) {
            TextAlignDeclaration::Value(v) => v,
            TextAlignDeclaration::Initial
            | TextAlignDeclaration::Inherit
            | TextAlignDeclaration::Invalid => Self::Start,
        }
    }

    /// Full declaration parse including global CSS keywords and vendor centers.
    pub fn parse_declaration(s: &str) -> TextAlignDeclaration {
        Self::try_parse_declaration(s).unwrap_or(TextAlignDeclaration::Invalid)
    }

    pub fn try_parse_declaration(s: &str) -> Option<TextAlignDeclaration> {
        let t = s.trim();
        if t.is_empty() {
            return None;
        }
        Some(match t.to_ascii_lowercase().as_str() {
            "inherit" | "unset" => TextAlignDeclaration::Inherit,
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
            _ => return None,
        })
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

impl From<&String> for TextAlign {
    fn from(s: &String) -> Self {
        Self::parse(s)
    }
}

impl From<TextAlign> for TextAlignDeclaration {
    fn from(v: TextAlign) -> Self {
        Self::Value(v)
    }
}

impl From<&str> for TextAlignDeclaration {
    fn from(s: &str) -> Self {
        TextAlign::parse_declaration(s)
    }
}

impl From<String> for TextAlignDeclaration {
    fn from(s: String) -> Self {
        TextAlign::parse_declaration(&s)
    }
}

impl From<&String> for TextAlignDeclaration {
    fn from(s: &String) -> Self {
        TextAlign::parse_declaration(s)
    }
}

/// Rich-inline span kind. Chip / Mention are atomic measure/break units.
///
/// Cosmic-text `Attrs::metadata` convention (`jadix_chip`): for Chip/Mention spans,
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
    /// Extra Alt+click carets (display UTF-8 bytes).
    pub extra_carets: Vec<usize>,
    /// Extra Alt+click selections `[start, end)` in display UTF-8 bytes.
    pub extra_selections: Vec<(usize, usize)>,
    /// Selection background color.
    pub selection_bg: [u8; 4],
    /// Overwrite-mode highlight `[start, end)`.
    pub overwrite_highlight: Option<(usize, usize)>,
    /// IME preedit `[start, end)` in display UTF-8 bytes; spaced underline under composing glyphs.
    pub preedit_underline: Option<(usize, usize)>,
    /// Extra carets' IME preedit underlines in display UTF-8 bytes.
    pub extra_preedit_underlines: Vec<(usize, usize)>,
    /// Styled composition runs in display UTF-8 bytes: `(start, end, style)`.
    /// `0` thin solid, `1` thick solid, `2` wavy. Empty keeps the single solid
    /// [`Self::preedit_underline`].
    pub preedit_ul: Vec<(usize, usize, u8)>,
    /// Draw the I-beam even when a selection highlight is present (selection-move drop caret).
    pub drop_caret: bool,
    /// I-beam fill. `None` uses the node's text color. Input/Textarea default this
    /// to token `caret` (same value as `foreground`) so a placeholder's muted
    /// color cannot tint the caret (CSS `caret-color` is not `::placeholder`).
    /// Override via [`crate::components::input::InputBuilder::caret_color`].
    pub caret_color: Option<[u8; 4]>,
    /// Selected glyph color (CSS `::selection { color }`). `None` uses token
    /// `selection-foreground` (inverse of `selection-background`). Override via
    /// [`crate::components::input::InputBuilder::selection_foreground`].
    pub selection_foreground: Option<[u8; 4]>,
}

impl Default for InputCaretDraw {
    fn default() -> Self {
        Self {
            caret_byte_index: 0,
            selection: None,
            extra_carets: Vec::new(),
            extra_selections: Vec::new(),
            selection_bg: [0; 4],
            overwrite_highlight: None,
            preedit_underline: None,
            extra_preedit_underlines: Vec::new(),
            preedit_ul: Vec::new(),
            drop_caret: false,
            caret_color: None,
            selection_foreground: None,
        }
    }
}

impl InputCaretDraw {
    /// CSS `caret-color` default: theme token `caret`.
    pub fn default_caret_color() -> [u8; 4] {
        super::parse_text_color("caret")
    }

    /// Default selection highlight: theme token `selection-background`.
    pub fn default_selection_bg() -> [u8; 4] {
        super::parse_text_color("selection-background")
    }

    /// Default selected-text color: theme token `selection-foreground`
    /// (inverse of `selection-background`).
    pub fn default_selection_foreground() -> [u8; 4] {
        super::parse_text_color("selection-foreground")
    }

    /// Byte range + color for recoloring selected glyphs. `None` when there is
    /// no highlight. Missing [`Self::selection_foreground`] uses the token default.
    pub fn selection_recolor(&self) -> Option<((usize, usize), [u8; 4])> {
        self.selection_recolor_ranges().into_iter().next()
    }

    /// All selection ranges (primary + extra) for glyph recolor.
    pub fn selection_recolor_ranges(&self) -> Vec<((usize, usize), [u8; 4])> {
        let fg = self
            .selection_foreground
            .unwrap_or_else(Self::default_selection_foreground);
        let mut out = Vec::new();
        let mut push = |a: usize, b: usize| {
            let lo = a.min(b);
            let hi = a.max(b);
            if lo < hi {
                out.push(((lo, hi), fg));
            }
        };
        if let Some((a, b)) = self.selection {
            push(a, b);
        }
        for &(a, b) in &self.extra_selections {
            push(a, b);
        }
        out
    }
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
    pub computed_align: Option<TextAlign>,
    pub align_last: Option<super::TextAlignLast>,
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
    /// Unspecified (`letter_spacing_specified = false`) inherits from the parent.
    pub letter_spacing: f32,
    /// When false, [`letter_spacing`] is inherited (CSS `letter-spacing`).
    pub letter_spacing_specified: bool,
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
            computed_align: None,
            align_last: None,
            overflow: TextOverflow::Clip,
            white_space: WhiteSpace::Normal,
            wrap_mode: crate::text::WrapMode::Word,
            selectable: false,
            direction: TextDirection::Auto,
            letter_spacing: 0.0,
            letter_spacing_specified: false,
            input_caret: None,
            scroll_x: 0.0,
            line_height_specified: false,
        }
    }
}

impl TextStyle {
    pub fn line_height_specified(&self) -> bool {
        self.line_height_specified
    }
    pub fn set_line_height_specified(&mut self, specified: bool) {
        self.line_height_specified = specified;
    }

    pub(crate) fn without_inherited(&self, inherited: Option<&Self>) -> Self {
        let mut specified = self.clone();
        specified.computed_align = None;
        if let Some(inherited) = inherited {
            macro_rules! restore {
                ($($field:ident),+ $(,)?) => {$(
                    if inherited.$field.is_some() && specified.$field == inherited.$field {
                        specified.$field = None;
                    }
                )+};
            }
            restore!(
                color,
                size,
                weight,
                font_family,
                align,
                align_last,
                underline,
                overline,
                line_through,
                decoration_style,
                decoration_thickness,
                underline_offset
            );
        }
        specified
    }

    pub(crate) fn inherited_fields(&self, resolved: &Self) -> Self {
        let mut inherited = Self::default();
        macro_rules! remember {
            ($($field:ident),+ $(,)?) => {$(
                if self.$field.is_none() {
                    inherited.$field = resolved.$field.clone();
                }
            )+};
        }
        remember!(
            color,
            size,
            weight,
            font_family,
            align,
            align_last,
            underline,
            overline,
            line_through,
            decoration_style,
            decoration_thickness,
            underline_offset
        );
        inherited
    }

    /// Root ancestor defaults when no parent supplies inheritable values.
    pub fn cascade_root() -> Self {
        let defaults = crate::utils::font::text_defaults();
        Self {
            color: Some(super::parse_text_color("foreground")),
            size: Some(defaults.size_px),
            weight: Some(defaults.weight),
            font_family: Some(crate::utils::font::default_ui_sans_family_list().to_string()),
            align: Some(TextAlign::Start),
            align_last: Some(super::TextAlignLast::Auto),
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
        match self.align {
            Some(TextAlign::MatchParent) => self.computed_align.unwrap_or(TextAlign::Start),
            align => align.unwrap_or(TextAlign::Start),
        }
    }

    pub fn align_last_resolved(&self) -> super::TextAlignLast {
        self.align_last.unwrap_or_default()
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
        out.align = Some(if self.align.is_some() {
            self.align_resolved()
        } else {
            parent.align_resolved()
        });
        out.computed_align = None;
        out.align_last = self.align_last.or(parent.align_last);
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
        if !self.letter_spacing_specified {
            out.letter_spacing = parent.letter_spacing;
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
        if self.align_last.is_none() {
            self.align_last = Some(self.align_last_resolved());
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

    let root_font_size_px = crate::layout::dimension::css_root_font_size_px();
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
#[path = "tests/text_style.rs"]
mod tests;
