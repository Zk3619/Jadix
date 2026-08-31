//! Known utility recognition — single source of truth for macro warnings + runtime apply.

/// Font-weight tokens (`font-*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontWeightToken {
    Thin,
    ExtraLight,
    Light,
    Normal,
    Medium,
    SemiBold,
    Bold,
    ExtraBold,
    Black,
}

impl FontWeightToken {
    pub fn from_rest(rest: &str) -> Option<Self> {
        Some(match rest {
            "thin" => Self::Thin,
            "extralight" => Self::ExtraLight,
            "light" => Self::Light,
            "normal" => Self::Normal,
            "medium" => Self::Medium,
            "semibold" => Self::SemiBold,
            "bold" => Self::Bold,
            "extrabold" => Self::ExtraBold,
            "black" => Self::Black,
            _ => return None,
        })
    }

    pub fn weight_u16(self) -> u16 {
        match self {
            Self::Thin => 100,
            Self::ExtraLight => 200,
            Self::Light => 300,
            Self::Normal => 400,
            Self::Medium => 500,
            Self::SemiBold => 600,
            Self::Bold => 700,
            Self::ExtraBold => 800,
            Self::Black => 900,
        }
    }
}

/// Classified utility (without variant prefixes).
#[derive(Clone, Debug, PartialEq)]
pub enum KnownUtility<'a> {
    FlexRow,
    FlexCol,
    FlexWrap,
    FlexNoWrap,
    DisplayGrid,
    ItemsCenter,
    JustifyCenter,
    Grow(f32),
    Shrink(f32),
    Overflow(&'a str),
    OverflowX(&'a str),
    OverflowY(&'a str),
    RoundedFull,
    Rounded(f32),
    GridCols(u16),
    GridRows(u16),
    GapX(String),
    GapY(String),
    Gap(f32),
    Width(String),
    Height(String),
    MinWidth(String),
    MaxWidth(String),
    MinHeight(String),
    MaxHeight(String),
    Padding(String),
    PaddingX(String),
    PaddingY(String),
    PaddingT(String),
    PaddingR(String),
    PaddingB(String),
    PaddingL(String),
    Margin(String),
    MarginX(String),
    MarginY(String),
    MarginT(String),
    MarginR(String),
    MarginB(String),
    MarginL(String),
    BgColor(&'a str),
    TextColor(&'a str),
    TextSize(f32),
    FontWeight(FontWeightToken),
    AlignSelf(&'a str),
    Opacity(f32),
    /// Visual translate X in logical px (`translate-x-*`).
    TranslateX(f32),
    /// Visual translate X as fraction of own width (`translate-x-1/2` → 0.5).
    TranslateXPct(f32),
    /// Visual translate Y in logical px (`translate-y-*`).
    TranslateY(f32),
    /// Visual translate Y as fraction of own height (`translate-y-1/2` → 0.5).
    TranslateYPct(f32),
    /// Single-line ellipsis overflow (`truncate`).
    Truncate,
    /// CSS `text-overflow`: `clip` | `hidden` | `ellipsis` | `newline` (`text-overflow-*` classes).
    TextOverflow(&'a str),
    /// CSS / Tailwind `text-align`: `text-left` | `text-center` | `text-right` | `text-start` |
    /// `text-end` | `text-justify` | `text-justify-all`.
    TextAlign(&'a str),
    /// Tailwind `whitespace-nowrap` — text leaf stays one line (ShadCN Button).
    WhitespaceNowrap,
    PositionFixed,
    PositionAbsolute,
    PositionRelative,
    PositionSticky,
    PositionStatic,
    /// `inset-*` / `top-*` / `right-*` / `bottom-*` / `left-*` spacing utilities.
    InsetAll(String),
    InsetX(String),
    InsetY(String),
    InsetTop(String),
    InsetRight(String),
    InsetBottom(String),
    InsetLeft(String),
    AspectRatio(f32),
    Rotate(f32),
    /// Uniform scale (`scale-*`) — CSS `scale` single-value.
    Scale(f32),
    /// X-axis scale (`scale-x-*`) — CSS `scale` per-axis.
    ScaleX(f32),
    /// Y-axis scale (`scale-y-*`) — CSS `scale` per-axis.
    ScaleY(f32),
    /// Visual skew X in degrees (`skew-x-*`).
    SkewX(f32),
    /// Visual skew Y in degrees (`skew-y-*`).
    SkewY(f32),
    /// CSS `transform-origin` keyword (`origin-*`).
    TransformOrigin(&'a str),
    /// CSS `zoom` — magnification that affects layout (`zoom-*`).
    Zoom(f32),
    /// `pointer-events-auto`
    PointerEventsAuto,
    /// `pointer-events-none` — block self + descendants
    PointerEventsNone,
    /// `pointer-events-box-none` — pass through self, children remain hittable
    PointerEventsBoxNone,
    /// Linear gradient direction angle (0=to-r, 90=to-b, 180=to-l, 270=to-t).
    BgGradientTo(f32),
    GradientFrom(&'a str),
    GradientVia(&'a str),
    GradientTo(&'a str),
    LetterSpacing(f32),
    TextDirectionLtr,
    TextDirectionRtl,
    /// `transition-opacity` — marks element for opacity transitions.
    TransitionOpacity,
    /// `transition-transform` — marks element for transform transitions.
    TransitionTransform,
    /// `transition-all` — transition all animatable properties.
    TransitionAll,
    /// `transition-colors` — transition color-related properties.
    TransitionColors,
    /// `transition-none` — disable transitions.
    TransitionNone,
    /// `duration-N` — transition duration in ms (e.g. `duration-300`).
    TransitionDuration(u32),
    /// `delay-N` — transition delay in ms (e.g. `delay-150`).
    TransitionDelay(u32),
    /// `ease-*` — transition timing function.
    TransitionEase(&'static str),
    /// `animate-spin` — continuous rotate (`animation: spin 1s linear infinite`).
    AnimateSpin,
    /// `animate-ping` — scale+fade pulse.
    AnimatePing,
    /// `animate-pulse` — opacity breathe.
    AnimatePulse,
    /// `animate-bounce` — translateY bounce.
    AnimateBounce,
    /// `animate-none` — clear animation.
    AnimateNone,
    /// Tailwind `cursor-*` (CSS `cursor` keywords).
    Cursor(&'a str),
    /// Marker: this element is a `group` / `group/name` for `group-hover:` descendants.
    GroupMarker {
        name: Option<&'a str>,
    },
    /// Marker: this element is a `peer` / `peer/name` for following siblings' `peer-hover:`.
    PeerMarker {
        name: Option<&'a str>,
    },
    /// Tailwind `box-border` → `box-sizing: border-box`（RUI default）。
    BoxBorder,
    /// Tailwind `box-content` → `box-sizing: content-box`。
    BoxContent,
    /// `outline-none` / `outline` / `outline-dashed` / `outline-dotted` / `outline-double` (style).
    OutlineStyle(&'a str),
    /// `outline-0` / `outline-1` / `outline-2` / `outline-4` / `outline-8` (width in px).
    OutlineWidth(f32),
    /// `outline-offset-*` (offset in px).
    OutlineOffset(f32),
    /// `outline-<color>` (color token).
    OutlineColor(&'a str),
    /// Tailwind `blur-*` → CSS `filter: blur(...)`.
    FilterBlur(f32),
    /// Tailwind `brightness-*` → CSS `filter: brightness(...)`.
    FilterBrightness(f32),
    /// Tailwind `contrast-*` → CSS `filter: contrast(...)`.
    FilterContrast(f32),
    /// Tailwind `grayscale` / `grayscale-*` → CSS `filter: grayscale(...)`.
    FilterGrayscale(f32),
    /// Tailwind `sepia` / `sepia-*` → CSS `filter: sepia(...)`.
    FilterSepia(f32),
    /// Tailwind `invert` / `invert-*` → CSS `filter: invert(...)`.
    FilterInvert(f32),
    /// Tailwind `saturate-*` → CSS `filter: saturate(...)`.
    FilterSaturate(f32),
    /// Tailwind `hue-rotate-*` → CSS `filter: hue-rotate(...)` (degrees).
    FilterHueRotate(f32),
    /// Tailwind `drop-shadow` / `drop-shadow-*` / `drop-shadow-[...]`.
    FilterDropShadow(&'a str),
    /// Tailwind `filter-none` → clear CSS `filter`.
    FilterNone,
}

pub fn spacing_token_to_px(token: &str) -> Option<f32> {
    match token {
        "0" => Some(0.0),
        "px" => Some(1.0),
        "0.5" => Some(2.0),
        "1" => Some(4.0),
        "1.5" => Some(6.0),
        "2" => Some(8.0),
        "2.5" => Some(10.0),
        "3" => Some(12.0),
        "3.5" => Some(14.0),
        "4" => Some(16.0),
        "5" => Some(20.0),
        "6" => Some(24.0),
        "7" => Some(28.0),
        "8" => Some(32.0),
        "9" => Some(36.0),
        "10" => Some(40.0),
        "11" => Some(44.0),
        "12" => Some(48.0),
        "14" => Some(56.0),
        "16" => Some(64.0),
        "20" => Some(80.0),
        "24" => Some(96.0),
        // `full` is NOT a spacing length — size utilities (`w-full` → 100%) are handled in
        // [`size_keyword_value`]. Do not map it to a sentinel px (that broke web `w-full`).
        _ => {
            if let Some(n) = token.strip_suffix("px") {
                return n.parse().ok();
            }
            if let Some(n) = token.strip_suffix("rem") {
                return n.parse::<f32>().ok().map(|v| v * 16.0);
            }
            // bare number → Tailwind spacing scale (×4px)
            token.parse::<f32>().ok().map(|n| n * 4.0)
        }
    }
}

pub fn text_size_token_to_px(token: &str) -> Option<f32> {
    match token {
        "xs" => Some(12.0),
        "sm" => Some(14.0),
        "base" => Some(16.0),
        "lg" => Some(18.0),
        "xl" => Some(20.0),
        "2xl" => Some(24.0),
        "3xl" => Some(30.0),
        "4xl" => Some(36.0),
        _ => None,
    }
}

fn spacing_value(token: &str, prefix: &str) -> Option<String> {
    let rest = token.strip_prefix(prefix)?;
    if rest.is_empty() {
        return None;
    }
    if let Some(inner) = rest.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        let v = inner.trim();
        if v.is_empty() {
            return None;
        }
        return Some(v.to_string());
    }
    // Size box keywords (Tailwind / CSS): only for w/h/min/max size prefixes.
    if matches!(
        prefix,
        "w-" | "h-" | "min-w-" | "max-w-" | "min-h-" | "max-h-"
    ) {
        if let Some(v) = size_keyword_value(prefix, rest) {
            return Some(v);
        }
    }
    let px = spacing_token_to_px(rest)?;
    Some(format!("{px}px"))
}

/// Tailwind size keywords → CSS dimension strings consumed by [`IntoDimension`].
///
/// | token | CSS |
/// |-------|-----|
/// | `full` | `100%` |
/// | `auto` | `auto` |
/// | `screen` | `100vw` / `100vh` |
///
/// (`fit` / `min` / `max` content keywords omitted until layout supports them.)
fn size_keyword_value(prefix: &str, rest: &str) -> Option<String> {
    match rest {
        "full" => Some("100%".into()),
        "auto" => Some("auto".into()),
        "screen" => match prefix {
            "w-" | "min-w-" | "max-w-" => Some("100vw".into()),
            "h-" | "min-h-" | "max-h-" => Some("100vh".into()),
            _ => None,
        },
        _ => None,
    }
}

fn negate_px_string(v: &str) -> String {
    let v = v.trim();
    if let Some(rest) = v.strip_prefix('-') {
        rest.to_string()
    } else {
        format!("-{v}")
    }
}

fn negate_margin_or_translate(known: KnownUtility<'_>) -> Option<KnownUtility<'_>> {
    Some(match known {
        KnownUtility::Margin(v) => KnownUtility::Margin(negate_px_string(&v)),
        KnownUtility::MarginX(v) => KnownUtility::MarginX(negate_px_string(&v)),
        KnownUtility::MarginY(v) => KnownUtility::MarginY(negate_px_string(&v)),
        KnownUtility::MarginT(v) => KnownUtility::MarginT(negate_px_string(&v)),
        KnownUtility::MarginR(v) => KnownUtility::MarginR(negate_px_string(&v)),
        KnownUtility::MarginB(v) => KnownUtility::MarginB(negate_px_string(&v)),
        KnownUtility::MarginL(v) => KnownUtility::MarginL(negate_px_string(&v)),
        KnownUtility::TranslateX(px) => KnownUtility::TranslateX(-px),
        KnownUtility::TranslateXPct(p) => KnownUtility::TranslateXPct(-p),
        KnownUtility::TranslateY(px) => KnownUtility::TranslateY(-px),
        KnownUtility::TranslateYPct(p) => KnownUtility::TranslateYPct(-p),
        KnownUtility::SkewX(v) => KnownUtility::SkewX(-v),
        KnownUtility::SkewY(v) => KnownUtility::SkewY(-v),
        KnownUtility::Rotate(deg) => KnownUtility::Rotate(-deg),
        _ => return None,
    })
}

fn is_negatable_spacing_token(token: &str) -> bool {
    token.starts_with("m-")
        || token.starts_with("mx-")
        || token.starts_with("my-")
        || token.starts_with("mt-")
        || token.starts_with("mr-")
        || token.starts_with("mb-")
        || token.starts_with("ml-")
        || token.starts_with("translate-x-")
        || token.starts_with("translate-y-")
        || token.starts_with("rotate-")
}

/// Classify a bare utility token (no `hover:` / `dark:` prefixes).
pub fn classify_utility(token: &str) -> Option<KnownUtility<'_>> {
    // Negative spacing: `-m-4`, `-translate-x-2`, …
    if let Some(rest) = token.strip_prefix('-') {
        if is_negatable_spacing_token(rest) {
            return classify_utility(rest).and_then(negate_margin_or_translate);
        }
    }

    match token {
        "flex" | "flex-row" => return Some(KnownUtility::FlexRow),
        "flex-col" => return Some(KnownUtility::FlexCol),
        "flex-wrap" | "wrap" => return Some(KnownUtility::FlexWrap),
        "flex-nowrap" | "nowrap" => return Some(KnownUtility::FlexNoWrap),
        "grid" => return Some(KnownUtility::DisplayGrid),
        "items-center" => return Some(KnownUtility::ItemsCenter),
        "justify-center" => return Some(KnownUtility::JustifyCenter),
        "grow" => return Some(KnownUtility::Grow(1.0)),
        "grow-0" => return Some(KnownUtility::Grow(0.0)),
        "shrink" => return Some(KnownUtility::Shrink(1.0)),
        "shrink-0" => return Some(KnownUtility::Shrink(0.0)),
        "overflow-hidden" => return Some(KnownUtility::Overflow("hidden")),
        "overflow-auto" => return Some(KnownUtility::Overflow("auto")),
        "overflow-scroll" => return Some(KnownUtility::Overflow("scroll")),
        "rounded" => return Some(KnownUtility::Rounded(6.0)),
        "rounded-full" => return Some(KnownUtility::RoundedFull),
        "self-auto" => return Some(KnownUtility::AlignSelf("auto")),
        "self-center" => return Some(KnownUtility::AlignSelf("center")),
        "self-start" => return Some(KnownUtility::AlignSelf("start")),
        "self-end" => return Some(KnownUtility::AlignSelf("end")),
        "self-stretch" => return Some(KnownUtility::AlignSelf("stretch")),
        "self-baseline" => return Some(KnownUtility::AlignSelf("baseline")),
        "transition-opacity" => return Some(KnownUtility::TransitionOpacity),
        "transition-transform" => return Some(KnownUtility::TransitionTransform),
        "transition-all" => return Some(KnownUtility::TransitionAll),
        "transition-colors" => return Some(KnownUtility::TransitionColors),
        "transition-none" => return Some(KnownUtility::TransitionNone),
        "ease-linear" => return Some(KnownUtility::TransitionEase("linear")),
        "ease-in" => return Some(KnownUtility::TransitionEase("ease-in")),
        "ease-out" => return Some(KnownUtility::TransitionEase("ease-out")),
        "ease-in-out" => return Some(KnownUtility::TransitionEase("ease-in-out")),
        "pointer-events-auto" => return Some(KnownUtility::PointerEventsAuto),
        "pointer-events-none" => return Some(KnownUtility::PointerEventsNone),
        "pointer-events-box-none" => return Some(KnownUtility::PointerEventsBoxNone),
        "animate-spin" => return Some(KnownUtility::AnimateSpin),
        "animate-ping" => return Some(KnownUtility::AnimatePing),
        "animate-pulse" => return Some(KnownUtility::AnimatePulse),
        "animate-bounce" => return Some(KnownUtility::AnimateBounce),
        "animate-none" => return Some(KnownUtility::AnimateNone),
        "group" => return Some(KnownUtility::GroupMarker { name: None }),
        "peer" => return Some(KnownUtility::PeerMarker { name: None }),
        "box-border" => return Some(KnownUtility::BoxBorder),
        "box-content" => return Some(KnownUtility::BoxContent),
        _ => {}
    }

    if let Some(rest) = token.strip_prefix("cursor-") {
        if !rest.is_empty() {
            return Some(KnownUtility::Cursor(rest));
        }
    }

    if let Some(name) = token.strip_prefix("group/") {
        let name = name.trim();
        if !name.is_empty() {
            return Some(KnownUtility::GroupMarker { name: Some(name) });
        }
    }
    if let Some(name) = token.strip_prefix("peer/") {
        let name = name.trim();
        if !name.is_empty() {
            return Some(KnownUtility::PeerMarker { name: Some(name) });
        }
    }

    if let Some(raw) = token.strip_prefix("grid-cols-") {
        let n: u16 = raw.parse().ok()?;
        if n > 0 {
            return Some(KnownUtility::GridCols(n));
        }
    }
    if let Some(raw) = token.strip_prefix("grid-rows-") {
        let n: u16 = raw.parse().ok()?;
        if n > 0 {
            return Some(KnownUtility::GridRows(n));
        }
    }

    if let Some(v) = spacing_value(token, "gap-x-") {
        return Some(KnownUtility::GapX(v));
    }
    if let Some(v) = spacing_value(token, "gap-y-") {
        return Some(KnownUtility::GapY(v));
    }
    if let Some(v) = spacing_value(token, "gap-") {
        let px = v
            .strip_suffix("px")
            .and_then(|s| s.parse().ok())
            .or_else(|| spacing_token_to_px(v.trim_end_matches("px")))?;
        return Some(KnownUtility::Gap(px));
    }

    if let Some(v) = spacing_value(token, "w-") {
        return Some(KnownUtility::Width(v));
    }
    if let Some(v) = spacing_value(token, "h-") {
        return Some(KnownUtility::Height(v));
    }
    if let Some(v) = spacing_value(token, "min-w-") {
        return Some(KnownUtility::MinWidth(v));
    }
    if let Some(v) = spacing_value(token, "max-w-") {
        return Some(KnownUtility::MaxWidth(v));
    }
    if let Some(v) = spacing_value(token, "min-h-") {
        return Some(KnownUtility::MinHeight(v));
    }
    if let Some(v) = spacing_value(token, "max-h-") {
        return Some(KnownUtility::MaxHeight(v));
    }
    if let Some(v) = spacing_value(token, "p-") {
        return Some(KnownUtility::Padding(v));
    }
    if let Some(v) = spacing_value(token, "px-") {
        return Some(KnownUtility::PaddingX(v));
    }
    if let Some(v) = spacing_value(token, "py-") {
        return Some(KnownUtility::PaddingY(v));
    }
    if let Some(v) = spacing_value(token, "pt-") {
        return Some(KnownUtility::PaddingT(v));
    }
    if let Some(v) = spacing_value(token, "pr-") {
        return Some(KnownUtility::PaddingR(v));
    }
    if let Some(v) = spacing_value(token, "pb-") {
        return Some(KnownUtility::PaddingB(v));
    }
    if let Some(v) = spacing_value(token, "pl-") {
        return Some(KnownUtility::PaddingL(v));
    }
    if let Some(v) = spacing_value(token, "m-") {
        return Some(KnownUtility::Margin(v));
    }
    if let Some(v) = spacing_value(token, "mx-") {
        return Some(KnownUtility::MarginX(v));
    }
    if let Some(v) = spacing_value(token, "my-") {
        return Some(KnownUtility::MarginY(v));
    }
    if let Some(v) = spacing_value(token, "mt-") {
        return Some(KnownUtility::MarginT(v));
    }
    if let Some(v) = spacing_value(token, "mr-") {
        return Some(KnownUtility::MarginR(v));
    }
    if let Some(v) = spacing_value(token, "mb-") {
        return Some(KnownUtility::MarginB(v));
    }
    if let Some(v) = spacing_value(token, "ml-") {
        return Some(KnownUtility::MarginL(v));
    }

    if let Some(rest) = token.strip_prefix("rounded-") {
        let px = match rest {
            "none" => 0.0,
            "sm" => 2.0,
            "md" => 6.0,
            "lg" => 8.0,
            "xl" => 12.0,
            "2xl" => 16.0,
            "full" => return Some(KnownUtility::RoundedFull),
            _ => spacing_token_to_px(rest)?,
        };
        return Some(KnownUtility::Rounded(px));
    }

    if let Some(rest) = token.strip_prefix("text-") {
        if let Some(mode) = rest.strip_prefix("overflow-") {
            match mode {
                "clip" | "hidden" | "ellipsis" | "newline" | "wrap" => {
                    return Some(KnownUtility::TextOverflow(mode));
                }
                _ => {}
            }
        }
        match rest {
            "left" | "center" | "right" | "start" | "end" | "justify" | "justify-all" => {
                return Some(KnownUtility::TextAlign(rest));
            }
            _ => {}
        }
        if let Some(px) = text_size_token_to_px(rest) {
            return Some(KnownUtility::TextSize(px));
        }
        return Some(KnownUtility::TextColor(rest));
    }
    if let Some(rest) = token.strip_prefix("bg-") {
        if rest == "gradient-to-r" {
            return Some(KnownUtility::BgGradientTo(0.0));
        }
        if rest == "gradient-to-l" {
            return Some(KnownUtility::BgGradientTo(180.0));
        }
        if rest == "gradient-to-b" {
            return Some(KnownUtility::BgGradientTo(90.0));
        }
        if rest == "gradient-to-t" {
            return Some(KnownUtility::BgGradientTo(270.0));
        }
        if rest == "gradient-to-tr" {
            return Some(KnownUtility::BgGradientTo(315.0));
        }
        if rest == "gradient-to-tl" {
            return Some(KnownUtility::BgGradientTo(225.0));
        }
        if rest == "gradient-to-br" {
            return Some(KnownUtility::BgGradientTo(45.0));
        }
        if rest == "gradient-to-bl" {
            return Some(KnownUtility::BgGradientTo(135.0));
        }
        return Some(KnownUtility::BgColor(rest));
    }
    if let Some(rest) = token.strip_prefix("font-") {
        return Some(KnownUtility::FontWeight(FontWeightToken::from_rest(rest)?));
    }
    if let Some(rest) = token.strip_prefix("overflow-x-") {
        return Some(KnownUtility::OverflowX(rest));
    }
    if let Some(rest) = token.strip_prefix("overflow-y-") {
        return Some(KnownUtility::OverflowY(rest));
    }

    if let Some(rest) = token.strip_prefix("opacity-") {
        return parse_opacity_token(rest).map(KnownUtility::Opacity);
    }
    if let Some(rest) = token.strip_prefix("translate-x-") {
        return parse_translate_token(rest, true);
    }
    if let Some(rest) = token.strip_prefix("translate-y-") {
        return parse_translate_token(rest, false);
    }
    if token == "truncate" {
        return Some(KnownUtility::Truncate);
    }
    if token == "whitespace-nowrap" || token == "whitespace-no-wrap" {
        return Some(KnownUtility::WhitespaceNowrap);
    }

    match token {
        "fixed" => return Some(KnownUtility::PositionFixed),
        "absolute" => return Some(KnownUtility::PositionAbsolute),
        "relative" => return Some(KnownUtility::PositionRelative),
        "sticky" => return Some(KnownUtility::PositionSticky),
        "static" => return Some(KnownUtility::PositionStatic),
        "aspect-square" => return Some(KnownUtility::AspectRatio(1.0)),
        "aspect-video" => return Some(KnownUtility::AspectRatio(16.0 / 9.0)),
        "rtl" => return Some(KnownUtility::TextDirectionRtl),
        "ltr" => return Some(KnownUtility::TextDirectionLtr),
        _ => {}
    }

    if let Some(rest) = token.strip_prefix("aspect-") {
        if let Some(inner) = rest.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            let inner = inner.trim();
            if let Some((a, b)) = inner.split_once('/') {
                let num: f32 = a.trim().parse().ok()?;
                let den: f32 = b.trim().parse().ok()?;
                if den != 0.0 {
                    return Some(KnownUtility::AspectRatio(num / den));
                }
            } else {
                let v: f32 = inner.parse().ok()?;
                return Some(KnownUtility::AspectRatio(v));
            }
        }
    }

    if token == "rotate-none" {
        return Some(KnownUtility::Rotate(0.0));
    }
    if let Some(rest) = token.strip_prefix("rotate-") {
        if let Some(inner) = rest.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            let inner = inner.trim();
            return parse_angle_value(inner).map(KnownUtility::Rotate);
        }
        let deg: f32 = match rest {
            "0" => 0.0,
            "1" => 1.0,
            "2" => 2.0,
            "3" => 3.0,
            "6" => 6.0,
            "12" => 12.0,
            "45" => 45.0,
            "90" => 90.0,
            "180" => 180.0,
            "270" => 270.0,
            _ => return rest.parse().ok().map(KnownUtility::Rotate).or(None),
        };
        return Some(KnownUtility::Rotate(deg));
    }

    if let Some(rest) = token.strip_prefix("scale-x-") {
        return parse_scale_value(rest).map(KnownUtility::ScaleX);
    }
    if let Some(rest) = token.strip_prefix("scale-y-") {
        return parse_scale_value(rest).map(KnownUtility::ScaleY);
    }
    if let Some(rest) = token.strip_prefix("scale-") {
        return parse_scale_value(rest).map(KnownUtility::Scale);
    }
    if let Some(rest) = token.strip_prefix("zoom-") {
        return parse_zoom_token(rest).map(KnownUtility::Zoom);
    }

    if let Some(rest) = token.strip_prefix("skew-x-") {
        if let Some(inner) = rest.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            let inner = inner.trim().strip_suffix("deg").unwrap_or(inner.trim());
            let v: f32 = inner.parse().ok()?;
            return Some(KnownUtility::SkewX(v));
        }
        let v: f32 = rest.parse().ok()?;
        return Some(KnownUtility::SkewX(v));
    }
    if let Some(rest) = token.strip_prefix("skew-y-") {
        if let Some(inner) = rest.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            let inner = inner.trim().strip_suffix("deg").unwrap_or(inner.trim());
            let v: f32 = inner.parse().ok()?;
            return Some(KnownUtility::SkewY(v));
        }
        let v: f32 = rest.parse().ok()?;
        return Some(KnownUtility::SkewY(v));
    }
    if let Some(rest) = token.strip_prefix("origin-") {
        return Some(KnownUtility::TransformOrigin(rest));
    }

    if let Some(rest) = token.strip_prefix("from-") {
        return Some(KnownUtility::GradientFrom(rest));
    }
    if let Some(rest) = token.strip_prefix("via-") {
        return Some(KnownUtility::GradientVia(rest));
    }
    if let Some(rest) = token.strip_prefix("to-") {
        // Avoid colliding with unrelated tokens if any; gradient `to-*` color stops only.
        return Some(KnownUtility::GradientTo(rest));
    }

    // Transition duration / delay
    if let Some(rest) = token.strip_prefix("duration-") {
        if let Ok(ms) = rest.parse::<u32>() {
            return Some(KnownUtility::TransitionDuration(ms));
        }
    }
    if let Some(rest) = token.strip_prefix("delay-") {
        if let Ok(ms) = rest.parse::<u32>() {
            return Some(KnownUtility::TransitionDelay(ms));
        }
    }

    // Outline utilities (Tailwind v4 style)
    if let Some(rest) = token.strip_prefix("outline-offset-") {
        let px = spacing_token_to_px(rest)?;
        return Some(KnownUtility::OutlineOffset(px));
    }
    if let Some(rest) = token.strip_prefix("outline-") {
        match rest {
            "none" => return Some(KnownUtility::OutlineStyle("none")),
            "dashed" => return Some(KnownUtility::OutlineStyle("dashed")),
            "dotted" => return Some(KnownUtility::OutlineStyle("dotted")),
            "double" => return Some(KnownUtility::OutlineStyle("double")),
            "solid" => return Some(KnownUtility::OutlineStyle("solid")),
            _ => {}
        }
        if let Some(px) = outline_width_token(rest) {
            return Some(KnownUtility::OutlineWidth(px));
        }
        if !rest.is_empty() {
            return Some(KnownUtility::OutlineColor(rest));
        }
    }
    if token == "outline" {
        return Some(KnownUtility::OutlineStyle("solid"));
    }

    // Inset / top / right / bottom / left utilities
    if let Some(v) = spacing_value(token, "inset-x-") {
        return Some(KnownUtility::InsetX(v));
    }
    if let Some(v) = spacing_value(token, "inset-y-") {
        return Some(KnownUtility::InsetY(v));
    }
    if let Some(v) = spacing_value(token, "inset-") {
        return Some(KnownUtility::InsetAll(v));
    }
    if let Some(v) = spacing_value(token, "top-") {
        return Some(KnownUtility::InsetTop(v));
    }
    if let Some(v) = spacing_value(token, "right-") {
        return Some(KnownUtility::InsetRight(v));
    }
    if let Some(v) = spacing_value(token, "bottom-") {
        return Some(KnownUtility::InsetBottom(v));
    }
    if let Some(v) = spacing_value(token, "left-") {
        return Some(KnownUtility::InsetLeft(v));
    }

    if let Some(rest) = token.strip_prefix("tracking-") {
        if let Some(inner) = rest.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            let inner = inner.trim();
            if let Some(raw) = inner.strip_suffix("em") {
                let em: f32 = raw.trim().parse().ok()?;
                return Some(KnownUtility::LetterSpacing(em * 16.0));
            }
            if let Some(raw) = inner.strip_suffix("px") {
                let px: f32 = raw.trim().parse().ok()?;
                return Some(KnownUtility::LetterSpacing(px));
            }
            let v: f32 = inner.parse().ok()?;
            return Some(KnownUtility::LetterSpacing(v));
        }
        let px = match rest {
            "tighter" => -0.05 * 16.0,
            "tight" => -0.025 * 16.0,
            "normal" => 0.0,
            "wide" => 0.025 * 16.0,
            "wider" => 0.05 * 16.0,
            "widest" => 0.1 * 16.0,
            _ => return None,
        };
        return Some(KnownUtility::LetterSpacing(px));
    }

    // CSS `filter` utilities (Tailwind v3)
    if token == "filter-none" {
        return Some(KnownUtility::FilterNone);
    }
    if token == "blur" {
        return Some(KnownUtility::FilterBlur(8.0));
    }
    if let Some(rest) = token.strip_prefix("blur-") {
        let px = match rest {
            "none" => 0.0,
            "sm" => 4.0,
            "md" => 12.0,
            "lg" => 16.0,
            "xl" => 24.0,
            "2xl" => 40.0,
            "3xl" => 64.0,
            _ => {
                if let Some(inner) = rest.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                    let inner = inner.trim();
                    let raw = inner.strip_suffix("px").unwrap_or(inner);
                    raw.parse().ok()?
                } else {
                    return None;
                }
            }
        };
        return Some(KnownUtility::FilterBlur(px));
    }
    if let Some(rest) = token.strip_prefix("brightness-") {
        return parse_filter_factor_token(rest).map(KnownUtility::FilterBrightness);
    }
    if let Some(rest) = token.strip_prefix("contrast-") {
        return parse_filter_factor_token(rest).map(KnownUtility::FilterContrast);
    }
    if let Some(rest) = token.strip_prefix("saturate-") {
        return parse_filter_factor_token(rest).map(KnownUtility::FilterSaturate);
    }
    if token == "grayscale" {
        return Some(KnownUtility::FilterGrayscale(1.0));
    }
    if let Some(rest) = token.strip_prefix("grayscale-") {
        return parse_filter_amount_token(rest).map(KnownUtility::FilterGrayscale);
    }
    if token == "sepia" {
        return Some(KnownUtility::FilterSepia(1.0));
    }
    if let Some(rest) = token.strip_prefix("sepia-") {
        return parse_filter_amount_token(rest).map(KnownUtility::FilterSepia);
    }
    if token == "invert" {
        return Some(KnownUtility::FilterInvert(1.0));
    }
    if let Some(rest) = token.strip_prefix("invert-") {
        return parse_filter_amount_token(rest).map(KnownUtility::FilterInvert);
    }
    if let Some(rest) = token.strip_prefix("hue-rotate-") {
        if let Some(inner) = rest.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            return parse_angle_value(inner.trim()).map(KnownUtility::FilterHueRotate);
        }
        let deg: f32 = match rest {
            "0" => 0.0,
            "15" => 15.0,
            "30" => 30.0,
            "60" => 60.0,
            "90" => 90.0,
            "180" => 180.0,
            _ => return None,
        };
        return Some(KnownUtility::FilterHueRotate(deg));
    }
    if token == "drop-shadow" {
        return Some(KnownUtility::FilterDropShadow(""));
    }
    if let Some(rest) = token.strip_prefix("drop-shadow-") {
        if let Some(inner) = rest.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            let inner = inner.trim();
            if !inner.is_empty() {
                return Some(KnownUtility::FilterDropShadow(inner));
            }
        } else if matches!(rest, "sm" | "md" | "lg" | "xl" | "2xl" | "none") {
            return Some(KnownUtility::FilterDropShadow(rest));
        }
    }

    None
}

fn outline_width_token(rest: &str) -> Option<f32> {
    match rest {
        "0" => Some(0.0),
        "1" => Some(1.0),
        "2" => Some(2.0),
        "4" => Some(4.0),
        "8" => Some(8.0),
        _ => {
            if let Some(inner) = rest.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                return inner
                    .trim()
                    .strip_suffix("px")
                    .unwrap_or(inner.trim())
                    .parse()
                    .ok();
            }
            None
        }
    }
}

fn parse_scale_value(rest: &str) -> Option<f32> {
    if let Some(inner) = rest.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        let inner = inner.trim();
        if let Some(pct) = inner.strip_suffix('%') {
            return pct.trim().parse::<f32>().ok().map(|v| v / 100.0);
        }
        return inner.parse().ok();
    }
    let pct: f32 = rest.parse().ok()?;
    Some(pct / 100.0)
}

/// Parse `zoom-*` values: `zoom-150` → 1.5, `zoom-[2.5]` → 2.5, `zoom-normal` → 1.0.
fn parse_zoom_token(rest: &str) -> Option<f32> {
    match rest {
        "normal" => return Some(1.0),
        _ => {}
    }
    if let Some(inner) = rest.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        let inner = inner.trim();
        if let Some(pct) = inner.strip_suffix('%') {
            return pct.trim().parse::<f32>().ok().map(|v| (v / 100.0).max(0.0));
        }
        return inner.parse::<f32>().ok().map(|v| v.max(0.0));
    }
    let pct: f32 = rest.parse().ok()?;
    Some((pct / 100.0).max(0.0))
}

/// Parse translate token: px via spacing scale, fractions (`1/2`, `full`), percentage (`[50%]`), arbitrary px (`[20px]`).
fn parse_translate_token(rest: &str, is_x: bool) -> Option<KnownUtility<'_>> {
    match rest {
        "full" => {
            return Some(if is_x {
                KnownUtility::TranslateXPct(1.0)
            } else {
                KnownUtility::TranslateYPct(1.0)
            })
        }
        "1/2" => {
            return Some(if is_x {
                KnownUtility::TranslateXPct(0.5)
            } else {
                KnownUtility::TranslateYPct(0.5)
            })
        }
        "1/3" => {
            return Some(if is_x {
                KnownUtility::TranslateXPct(1.0 / 3.0)
            } else {
                KnownUtility::TranslateYPct(1.0 / 3.0)
            })
        }
        "2/3" => {
            return Some(if is_x {
                KnownUtility::TranslateXPct(2.0 / 3.0)
            } else {
                KnownUtility::TranslateYPct(2.0 / 3.0)
            })
        }
        "1/4" => {
            return Some(if is_x {
                KnownUtility::TranslateXPct(0.25)
            } else {
                KnownUtility::TranslateYPct(0.25)
            })
        }
        "3/4" => {
            return Some(if is_x {
                KnownUtility::TranslateXPct(0.75)
            } else {
                KnownUtility::TranslateYPct(0.75)
            })
        }
        _ => {}
    }
    if let Some(inner) = rest.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        let inner = inner.trim();
        if let Some(pct) = inner.strip_suffix('%') {
            let v: f32 = pct.trim().parse().ok()?;
            return Some(if is_x {
                KnownUtility::TranslateXPct(v / 100.0)
            } else {
                KnownUtility::TranslateYPct(v / 100.0)
            });
        }
        if let Some(px) = inner.strip_suffix("px") {
            let v: f32 = px.trim().parse().ok()?;
            return Some(if is_x {
                KnownUtility::TranslateX(v)
            } else {
                KnownUtility::TranslateY(v)
            });
        }
        if let Some(rem) = inner.strip_suffix("rem") {
            let v: f32 = rem.trim().parse().ok()?;
            return Some(if is_x {
                KnownUtility::TranslateX(v * 16.0)
            } else {
                KnownUtility::TranslateY(v * 16.0)
            });
        }
        let v: f32 = inner.parse().ok()?;
        return Some(if is_x {
            KnownUtility::TranslateX(v)
        } else {
            KnownUtility::TranslateY(v)
        });
    }
    spacing_token_to_px(rest).map(|px| {
        if is_x {
            KnownUtility::TranslateX(px)
        } else {
            KnownUtility::TranslateY(px)
        }
    })
}

/// Parse angle value: `45deg`, `0.5turn`, `1.57rad`, or bare number as degrees.
fn parse_angle_value(s: &str) -> Option<f32> {
    if let Some(deg) = s.strip_suffix("deg") {
        return deg.trim().parse().ok();
    }
    if let Some(turn) = s.strip_suffix("turn") {
        return turn.trim().parse::<f32>().ok().map(|v| v * 360.0);
    }
    if let Some(rad) = s.strip_suffix("rad") {
        return rad.trim().parse::<f32>().ok().map(|v| v.to_degrees());
    }
    s.parse().ok()
}

fn parse_opacity_token(rest: &str) -> Option<f32> {
    match rest {
        "0" => Some(0.0),
        "50" => Some(0.5),
        "75" => Some(0.75),
        "100" => Some(1.0),
        _ => {
            if let Some(inner) = rest.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                return inner.trim().parse::<f32>().ok().map(|v| v.clamp(0.0, 1.0));
            }
            None
        }
    }
}

/// Tailwind filter factor tokens (`brightness-*` / `contrast-*` / `saturate-*`):
/// `100` → 1.0, `50` → 0.5, `[1.25]` → 1.25, `[50%]` → 0.5.
fn parse_filter_factor_token(rest: &str) -> Option<f32> {
    if let Some(inner) = rest.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        let inner = inner.trim();
        if let Some(pct) = inner.strip_suffix('%') {
            return pct.trim().parse::<f32>().ok().map(|v| v / 100.0);
        }
        return inner.parse().ok();
    }
    match rest {
        "0" => Some(0.0),
        "50" => Some(0.5),
        "75" => Some(0.75),
        "90" => Some(0.9),
        "95" => Some(0.95),
        "100" => Some(1.0),
        "105" => Some(1.05),
        "110" => Some(1.1),
        "125" => Some(1.25),
        "150" => Some(1.5),
        "200" => Some(2.0),
        _ => rest.parse::<f32>().ok().map(|v| v / 100.0),
    }
}

/// Tailwind amount tokens (`grayscale-*` / `sepia-*` / `invert-*`):
/// `0` → 0.0, `[0.5]` → 0.5, `[50%]` → 0.5.
fn parse_filter_amount_token(rest: &str) -> Option<f32> {
    if let Some(inner) = rest.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        let inner = inner.trim();
        if let Some(pct) = inner.strip_suffix('%') {
            return pct
                .trim()
                .parse::<f32>()
                .ok()
                .map(|v| (v / 100.0).clamp(0.0, 1.0));
        }
        return inner.parse::<f32>().ok().map(|v| v.clamp(0.0, 1.0));
    }
    match rest {
        "0" => Some(0.0),
        "50" => Some(0.5),
        "100" => Some(1.0),
        _ => None,
    }
}

#[inline]
pub fn is_known_utility(token: &str) -> bool {
    classify_utility(token).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_basics() {
        assert!(is_known_utility("flex"));
        assert!(is_known_utility("bg-primary"));
        assert!(is_known_utility(
            "hover:bg-accent".strip_prefix("hover:").unwrap_or("")
        ));
        assert!(is_known_utility("p-4"));
        assert!(is_known_utility("w-[12rem]"));
        assert!(is_known_utility("opacity-50"));
        assert!(is_known_utility("opacity-[0.4]"));
        assert!(is_known_utility("translate-x-4"));
        assert!(is_known_utility("translate-y-2"));
        assert!(is_known_utility("truncate"));
        assert!(is_known_utility("text-overflow-clip"));
        assert!(is_known_utility("text-overflow-hidden"));
        assert!(is_known_utility("text-overflow-ellipsis"));
        assert!(is_known_utility("text-overflow-newline"));
        assert!(matches!(
            classify_utility("text-overflow-clip"),
            Some(KnownUtility::TextOverflow("clip"))
        ));
        assert!(matches!(
            classify_utility("text-overflow-hidden"),
            Some(KnownUtility::TextOverflow("hidden"))
        ));
        assert!(matches!(
            classify_utility("text-overflow-ellipsis"),
            Some(KnownUtility::TextOverflow("ellipsis"))
        ));
        assert!(matches!(
            classify_utility("text-overflow-newline"),
            Some(KnownUtility::TextOverflow("newline"))
        ));
        assert!(matches!(
            classify_utility("text-center"),
            Some(KnownUtility::TextAlign("center"))
        ));
        assert!(matches!(
            classify_utility("text-left"),
            Some(KnownUtility::TextAlign("left"))
        ));
        assert!(matches!(
            classify_utility("text-start"),
            Some(KnownUtility::TextAlign("start"))
        ));
        assert!(matches!(
            classify_utility("text-justify"),
            Some(KnownUtility::TextAlign("justify"))
        ));
        assert!(is_known_utility("whitespace-nowrap"));
        assert!(matches!(
            classify_utility("whitespace-nowrap"),
            Some(KnownUtility::WhitespaceNowrap)
        ));
        assert!(is_known_utility("box-border"));
        assert!(is_known_utility("box-content"));
        assert!(matches!(
            classify_utility("box-border"),
            Some(KnownUtility::BoxBorder)
        ));
        assert!(matches!(
            classify_utility("box-content"),
            Some(KnownUtility::BoxContent)
        ));
        assert!(!is_known_utility("not-a-real-class"));
    }

    #[test]
    fn size_full_auto_screen_match_web() {
        assert!(matches!(
            classify_utility("w-full"),
            Some(KnownUtility::Width(ref v)) if v == "100%"
        ));
        assert!(matches!(
            classify_utility("h-full"),
            Some(KnownUtility::Height(ref v)) if v == "100%"
        ));
        assert!(matches!(
            classify_utility("min-w-full"),
            Some(KnownUtility::MinWidth(ref v)) if v == "100%"
        ));
        assert!(matches!(
            classify_utility("max-w-full"),
            Some(KnownUtility::MaxWidth(ref v)) if v == "100%"
        ));
        assert!(matches!(
            classify_utility("w-auto"),
            Some(KnownUtility::Width(ref v)) if v == "auto"
        ));
        assert!(matches!(
            classify_utility("w-screen"),
            Some(KnownUtility::Width(ref v)) if v == "100vw"
        ));
        assert!(matches!(
            classify_utility("h-screen"),
            Some(KnownUtility::Height(ref v)) if v == "100vh"
        ));
        // `full` must not resolve as a spacing px for gap/padding.
        assert_eq!(spacing_token_to_px("full"), None);
        assert!(matches!(
            classify_utility("w-64"),
            Some(KnownUtility::Width(ref v)) if v == "256px"
        ));
    }

    #[test]
    fn aspect_square_and_arbitrary() {
        assert!(matches!(
            classify_utility("aspect-square"),
            Some(KnownUtility::AspectRatio(r)) if (r - 1.0).abs() < f32::EPSILON
        ));
        assert!(matches!(
            classify_utility("aspect-video"),
            Some(KnownUtility::AspectRatio(r)) if (r - 16.0 / 9.0).abs() < 0.001
        ));
        assert!(matches!(
            classify_utility("aspect-[16/9]"),
            Some(KnownUtility::AspectRatio(r)) if (r - 16.0 / 9.0).abs() < 0.001
        ));
        assert!(matches!(
            classify_utility("aspect-[1.5]"),
            Some(KnownUtility::AspectRatio(r)) if (r - 1.5).abs() < f32::EPSILON
        ));
    }

    #[test]
    fn rotate_scale_tracking_direction() {
        assert!(matches!(
            classify_utility("rotate-45"),
            Some(KnownUtility::Rotate(45.0))
        ));
        assert!(matches!(
            classify_utility("rotate-[12deg]"),
            Some(KnownUtility::Rotate(12.0))
        ));
        assert!(matches!(
            classify_utility("scale-150"),
            Some(KnownUtility::Scale(1.5))
        ));
        assert!(matches!(
            classify_utility("scale-[1.2]"),
            Some(KnownUtility::Scale(1.2))
        ));
        assert!(matches!(
            classify_utility("scale-x-150"),
            Some(KnownUtility::ScaleX(1.5))
        ));
        assert!(matches!(
            classify_utility("scale-y-[0.5]"),
            Some(KnownUtility::ScaleY(0.5))
        ));
        assert!(matches!(
            classify_utility("scale-[50%]"),
            Some(KnownUtility::Scale(0.5))
        ));
        assert!(matches!(
            classify_utility("zoom-200"),
            Some(KnownUtility::Zoom(2.0))
        ));
        assert!(matches!(
            classify_utility("zoom-normal"),
            Some(KnownUtility::Zoom(1.0))
        ));
        assert!(matches!(
            classify_utility("zoom-[1.5]"),
            Some(KnownUtility::Zoom(1.5))
        ));
        assert!(matches!(
            classify_utility("zoom-[75%]"),
            Some(KnownUtility::Zoom(0.75))
        ));
        assert!(matches!(
            classify_utility("tracking-wide"),
            Some(KnownUtility::LetterSpacing(_))
        ));
        assert!(matches!(
            classify_utility("rtl"),
            Some(KnownUtility::TextDirectionRtl)
        ));
        assert!(matches!(
            classify_utility("fixed"),
            Some(KnownUtility::PositionFixed)
        ));
    }

    #[test]
    fn negative_margin_and_translate() {
        assert!(matches!(
            classify_utility("-m-4"),
            Some(KnownUtility::Margin(ref v)) if v == "-16px"
        ));
        assert!(matches!(
            classify_utility("-mx-2"),
            Some(KnownUtility::MarginX(ref v)) if v == "-8px"
        ));
        assert!(matches!(
            classify_utility("-mt-1"),
            Some(KnownUtility::MarginT(ref v)) if v == "-4px"
        ));
        assert!(matches!(
            classify_utility("-translate-x-4"),
            Some(KnownUtility::TranslateX(px)) if (px - (-16.0)).abs() < f32::EPSILON
        ));
        assert!(matches!(
            classify_utility("-translate-y-2"),
            Some(KnownUtility::TranslateY(px)) if (px - (-8.0)).abs() < f32::EPSILON
        ));
    }

    #[test]
    fn filter_utilities_classify() {
        assert!(matches!(
            classify_utility("blur"),
            Some(KnownUtility::FilterBlur(8.0))
        ));
        assert!(matches!(
            classify_utility("blur-lg"),
            Some(KnownUtility::FilterBlur(16.0))
        ));
        assert!(matches!(
            classify_utility("blur-[10px]"),
            Some(KnownUtility::FilterBlur(10.0))
        ));
        assert!(matches!(
            classify_utility("brightness-50"),
            Some(KnownUtility::FilterBrightness(v)) if (v - 0.5).abs() < f32::EPSILON
        ));
        assert!(matches!(
            classify_utility("contrast-150"),
            Some(KnownUtility::FilterContrast(v)) if (v - 1.5).abs() < f32::EPSILON
        ));
        assert!(matches!(
            classify_utility("grayscale"),
            Some(KnownUtility::FilterGrayscale(1.0))
        ));
        assert!(matches!(
            classify_utility("grayscale-0"),
            Some(KnownUtility::FilterGrayscale(0.0))
        ));
        assert!(matches!(
            classify_utility("sepia"),
            Some(KnownUtility::FilterSepia(1.0))
        ));
        assert!(matches!(
            classify_utility("invert-0"),
            Some(KnownUtility::FilterInvert(0.0))
        ));
        assert!(matches!(
            classify_utility("saturate-200"),
            Some(KnownUtility::FilterSaturate(v)) if (v - 2.0).abs() < f32::EPSILON
        ));
        assert!(matches!(
            classify_utility("hue-rotate-90"),
            Some(KnownUtility::FilterHueRotate(90.0))
        ));
        assert!(matches!(
            classify_utility("hue-rotate-[45deg]"),
            Some(KnownUtility::FilterHueRotate(45.0))
        ));
        assert!(matches!(
            classify_utility("drop-shadow"),
            Some(KnownUtility::FilterDropShadow(""))
        ));
        assert!(matches!(
            classify_utility("drop-shadow-md"),
            Some(KnownUtility::FilterDropShadow("md"))
        ));
        assert!(matches!(
            classify_utility("filter-none"),
            Some(KnownUtility::FilterNone)
        ));
    }

    #[test]
    fn transition_and_animate_classify() {
        assert!(matches!(
            classify_utility("transition-opacity"),
            Some(KnownUtility::TransitionOpacity)
        ));
        assert!(matches!(
            classify_utility("transition-transform"),
            Some(KnownUtility::TransitionTransform)
        ));
        assert!(matches!(
            classify_utility("animate-spin"),
            Some(KnownUtility::AnimateSpin)
        ));
        assert!(matches!(
            classify_utility("animate-ping"),
            Some(KnownUtility::AnimatePing)
        ));
        assert!(matches!(
            classify_utility("animate-pulse"),
            Some(KnownUtility::AnimatePulse)
        ));
        assert!(matches!(
            classify_utility("animate-bounce"),
            Some(KnownUtility::AnimateBounce)
        ));
        assert!(matches!(
            classify_utility("animate-none"),
            Some(KnownUtility::AnimateNone)
        ));
    }

    #[test]
    fn cursor_utility_classes() {
        assert!(matches!(
            classify_utility("cursor-pointer"),
            Some(KnownUtility::Cursor("pointer"))
        ));
        assert!(matches!(
            classify_utility("cursor-not-allowed"),
            Some(KnownUtility::Cursor("not-allowed"))
        ));
        assert!(matches!(
            classify_utility("cursor-ew-resize"),
            Some(KnownUtility::Cursor("ew-resize"))
        ));
        assert!(classify_utility("cursor-").is_none());
    }
}
