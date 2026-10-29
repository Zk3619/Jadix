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
            "100" | "thin" => Self::Thin,
            "200" | "extralight" => Self::ExtraLight,
            "300" | "light" => Self::Light,
            "400" | "normal" => Self::Normal,
            "500" | "medium" => Self::Medium,
            "600" | "semibold" => Self::SemiBold,
            "700" | "bold" => Self::Bold,
            "800" | "extrabold" => Self::ExtraBold,
            "900" | "black" => Self::Black,
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

/// Utility length that still knows its unit. Resolve with the live CSS root (`1rem`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TwLength {
    Px(f32),
    Rem(f32),
    Em(f32),
}

impl TwLength {
    pub fn to_px(self, root_font_size_px: f32) -> f32 {
        match self {
            Self::Px(v) => v,
            Self::Rem(v) | Self::Em(v) => v * root_font_size_px,
        }
    }

    pub fn negated(self) -> Self {
        match self {
            Self::Px(v) => Self::Px(-v),
            Self::Rem(v) => Self::Rem(-v),
            Self::Em(v) => Self::Em(-v),
        }
    }
}

/// Classified utility (without variant prefixes).
#[derive(Clone, Debug, PartialEq)]
pub enum KnownUtility<'a> {
    DisplayNone,
    DisplayFlex,
    FlexRow,
    FlexCol,
    FlexWrap,
    FlexNoWrap,
    DisplayGrid,
    ItemsCenter,
    JustifyCenter,
    JustifyEnd,
    Grow(f32),
    Shrink(f32),
    Overflow(&'a str),
    OverflowX(&'a str),
    OverflowY(&'a str),
    RoundedFull,
    Rounded(TwLength),
    GridCols(u16),
    GridRows(u16),
    GapX(String),
    GapY(String),
    Gap(String),
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
    TextSize(TwLength),
    FontWeight(FontWeightToken),
    /// Tailwind `font-sans` — UI sans stack.
    FontFamilySans,
    /// Tailwind `font-serif`.
    FontFamilySerif,
    /// Tailwind `font-mono` / `font-monospace`.
    FontFamilyMono,
    /// Tailwind `leading-*` unitless scale (`none` / `tight` / …).
    Leading(f32),
    LeadingPx(f32),
    BorderWidth(f32),
    BoxShadow(&'a str),
    AlignSelf(&'a str),
    Opacity(f32),
    /// Visual translate X in logical px (`translate-x-*`).
    TranslateX(TwLength),
    /// Visual translate X as fraction of own width (`translate-x-1/2` → 0.5).
    TranslateXPct(f32),
    /// Visual translate Y in logical px (`translate-y-*`).
    TranslateY(TwLength),
    /// Visual translate Y as fraction of own height (`translate-y-1/2` → 0.5).
    TranslateYPct(f32),
    /// Single-line ellipsis overflow (`truncate`).
    Truncate,
    /// CSS `text-overflow`: `clip` | `hidden` | `ellipsis` | `newline` (`text-overflow-*` classes).
    TextOverflow(&'a str),
    /// CSS / Tailwind `text-align`: `text-left` | `text-center` | `text-right` | `text-start` |
    /// `text-end` | `text-justify` | `text-justify-all`.
    TextAlign(&'a str),
    TextAlignLast(&'a str),
    Alignment(&'a str, &'a str),
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
    LetterSpacing(TwLength),
    TextDirectionLtr,
    TextDirectionRtl,
    /// Tailwind `writing-*` → CSS `writing-mode`.
    WritingMode(&'a str),
    /// `transition-opacity` — marks element for opacity transitions.
    TransitionOpacity,
    /// `transition-transform` — marks element for transform transitions.
    TransitionTransform,
    /// `transition-all` — transition all animatable properties.
    TransitionAll,
    /// `transition-colors` — transition color-related properties.
    TransitionColors,
    /// `transition-padding` — layout padding only (avoid `transition-all` seeding width/height).
    TransitionPadding,
    /// `transition-none` — disable transitions.
    TransitionNone,
    /// `duration-N` — transition duration in ms (e.g. `duration-300`).
    TransitionDuration(u32),
    /// `delay-N` — transition delay in ms (e.g. `delay-150`).
    TransitionDelay(u32),
    /// `ease-*` — transition timing function.
    TransitionEase(&'static str),
    /// Tailwind `will-change-*`.
    WillChange(&'static str),
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
    /// Tailwind `box-border` → `box-sizing: border-box`（Jadix default）。
    BoxBorder,
    /// Tailwind `box-content` → `box-sizing: content-box`。
    BoxContent,
    /// `outline-none` / `outline` / `outline-dashed` / `outline-dotted` / `outline-double` (style).
    OutlineStyle(&'a str),
    /// `outline-0` / `outline-1` / `outline-2` / `outline-4` / `outline-8` (width in px).
    OutlineWidth(f32),
    /// `outline-offset-*` (offset in px).
    OutlineOffset(TwLength),
    /// `outline-<color>` (color token).
    OutlineColor(&'a str),
    /// `ring` / `ring-0` / `ring-1` / `ring-2` / `ring-4` / `ring-8` / `ring-[Npx]` (focus-ring width).
    RingWidth(f32),
    /// `ring-<color>` (focus-ring color token).
    RingColor(&'a str),
    /// Tailwind `underline` / `no-underline` → CSS `text-decoration-line`.
    Underline(bool),
    /// `underline-offset-*` (spacing rem scale: `4` = 1rem) or `underline-offset-[2px]` / `[0.25rem]` / `[1em]`.
    UnderlineOffset(TwLength),
    /// Tailwind `blur-*` → CSS `filter: blur(...)` (v4 `--blur-*`, logical px).
    FilterBlur(TwLength),
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
    /// Tailwind `filter-none` / `blur-none` → clear CSS `filter`.
    FilterNone,
    /// Tailwind `backdrop-blur-*` → `backdrop-filter: blur(...)` (same v4 scale as `blur-*`).
    BackdropBlur(TwLength),
    Glass([f32; 4]),
    /// shadcn `shimmer` — streaming-text highlight (`background-clip: text` analog).
    Shimmer,
    /// `shimmer-once` — one sweep then settle (`shimmer-count-1`).
    ShimmerOnce,
    /// `shimmer-count-N` / `shimmer-count-infinite` — play N sweeps (`None` = loop).
    ShimmerCount(Option<f32>),
    /// `shimmer-box` / `animate-shimmer` — container overlay sheen (above content; not text-clip).
    ShimmerBox,
    /// `shimmer-reverse` — sweep against the reading direction.
    ShimmerReverse,
    /// `shimmer-none` — disable the effect (wins in any class order vs `shimmer`).
    ShimmerNone,
    /// `shimmer-duration-N` — one sweep in milliseconds (default 2000).
    ShimmerDuration(u32),
    /// `shimmer-spread-*` — highlight band width in logical px.
    ShimmerSpread(f32),
    /// `shimmer-angle-*` — band tilt in degrees (default 20).
    ShimmerAngle(f32),
    /// `shimmer-color-*` — explicit highlight color token.
    ShimmerColor(&'a str),
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

/// Tailwind spacing scale as rem (`4` → `1rem`). `px` / `Npx` stay absolute.
fn spacing_token_to_css(token: &str) -> Option<String> {
    if token == "px" {
        return Some("1px".into());
    }
    if token == "0" {
        return Some("0".into());
    }
    if let Some(n) = token.strip_suffix("px") {
        n.parse::<f32>().ok()?;
        return Some(token.to_string());
    }
    if let Some(n) = token.strip_suffix("rem") {
        n.parse::<f32>().ok()?;
        return Some(token.to_string());
    }
    if let Some(n) = token.strip_suffix("em") {
        n.parse::<f32>().ok()?;
        return Some(token.to_string());
    }
    let rem = spacing_token_to_px(token)? / 16.0;
    Some(format_rem_css(rem))
}

fn format_rem_css(v: f32) -> String {
    if v.abs() < 1e-8 {
        return "0".into();
    }
    let s = format!("{v:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    format!("{s}rem")
}

/// `[2px]` / `[0.25rem]` / `[1em]`. Unitless inside brackets is not a length.
fn parse_arbitrary_tw_length(inner: &str) -> Option<TwLength> {
    let v = inner.trim().replace('_', "");
    if v.is_empty() {
        return None;
    }
    if let Some(n) = v.strip_suffix("px") {
        return n.trim().parse().ok().map(TwLength::Px);
    }
    if let Some(n) = v.strip_suffix("rem") {
        return n.trim().parse().ok().map(TwLength::Rem);
    }
    if let Some(n) = v.strip_suffix("em") {
        return n.trim().parse().ok().map(TwLength::Em);
    }
    None
}

fn spacing_to_tw_length(token: &str) -> Option<TwLength> {
    if let Some(inner) = token.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        return parse_arbitrary_tw_length(inner.trim());
    }
    if token == "px" {
        return Some(TwLength::Px(1.0));
    }
    if let Some(n) = token.strip_suffix("px") {
        return n.parse().ok().map(TwLength::Px);
    }
    if let Some(n) = token.strip_suffix("rem") {
        return n.parse().ok().map(TwLength::Rem);
    }
    if let Some(n) = token.strip_suffix("em") {
        return n.parse().ok().map(TwLength::Em);
    }
    spacing_token_to_px(token).map(|px| TwLength::Rem(px / 16.0))
}

pub fn text_size_token_to_px(token: &str) -> Option<f32> {
    text_size_token_to_tw(token).map(|t| t.to_px(16.0))
}

fn text_size_token_to_tw(token: &str) -> Option<TwLength> {
    if let Some(inner) = token.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        return parse_arbitrary_tw_length(inner.trim());
    }
    match token {
        "xs" => Some(TwLength::Rem(0.75)),
        "sm" => Some(TwLength::Rem(0.875)),
        "base" => Some(TwLength::Rem(1.0)),
        "lg" => Some(TwLength::Rem(1.125)),
        "xl" => Some(TwLength::Rem(1.25)),
        "2xl" => Some(TwLength::Rem(1.5)),
        "3xl" => Some(TwLength::Rem(1.875)),
        "4xl" => Some(TwLength::Rem(2.25)),
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
    spacing_token_to_css(rest)
}

/// Tailwind size keywords → CSS dimension strings consumed by builder / class apply.
///
/// | token | CSS |
/// |-------|-----|
/// | `full` | `100%` (parent containing-block content box) |
/// | `auto` | `auto` |
/// | `fit` / `fit-content` | `fit-content` |
/// | `min` / `min-content` | `min-content` |
/// | `max` / `max-content` | `max-content` |
/// | `screen` | `100vw` / `100vh` |
/// | `a/b` | `{a/b}` of the **parent** content box (`w-3/4` → `75%`, `h-1/2` → `50%`) |
fn size_keyword_value(prefix: &str, rest: &str) -> Option<String> {
    match rest {
        "full" => Some("100%".into()),
        "auto" => Some("auto".into()),
        "fit" | "fit-content" => Some("fit-content".into()),
        "min" | "min-content" => Some("min-content".into()),
        "max" | "max-content" => Some("max-content".into()),
        "screen" => match prefix {
            "w-" | "min-w-" | "max-w-" => Some("100vw".into()),
            "h-" | "min-h-" | "max-h-" => Some("100vh".into()),
            _ => None,
        },
        _ => tailwind_fraction_percent_string(rest),
    }
}

/// Parse Tailwind `a/b` fraction tokens (`1/2`, `3/4`, `11/12`…) → ratio in `(0, 1]`.
fn parse_tailwind_fraction_ratio(rest: &str) -> Option<f32> {
    let (num_s, den_s) = rest.split_once('/')?;
    let num: f32 = num_s.trim().parse().ok()?;
    let den: f32 = den_s.trim().parse().ok()?;
    if !num.is_finite() || !den.is_finite() || den <= 0.0 || num < 0.0 || num > den {
        return None;
    }
    Some(num / den)
}

/// Size utility fraction → CSS percentage of the **parent** containing block (`3/4` → `"75%"`).
fn tailwind_fraction_percent_string(rest: &str) -> Option<String> {
    let ratio = parse_tailwind_fraction_ratio(rest)?;
    Some(format!("{}%", ratio * 100.0))
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
        KnownUtility::TranslateX(len) => KnownUtility::TranslateX(len.negated()),
        KnownUtility::TranslateXPct(p) => KnownUtility::TranslateXPct(-p),
        KnownUtility::TranslateY(len) => KnownUtility::TranslateY(len.negated()),
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
    if token == "shadow-none" {
        return Some(KnownUtility::BoxShadow("none"));
    }
    if let Some(inner) = token
        .strip_prefix("shadow-[")
        .and_then(|value| value.strip_suffix(']'))
    {
        if !inner.is_empty() {
            return Some(KnownUtility::BoxShadow(inner));
        }
    }
    if token == "border" {
        return Some(KnownUtility::BorderWidth(1.0));
    }
    if let Some(rest) = token.strip_prefix("border-") {
        let value = rest
            .strip_prefix('[')
            .and_then(|value| value.strip_suffix("px]"))
            .unwrap_or(rest);
        if let Ok(width) = value.parse::<f32>() {
            return (width.is_finite() && width >= 0.0).then_some(KnownUtility::BorderWidth(width));
        }
    }
    // Negative spacing: `-m-4`, `-translate-x-2`, …
    if let Some(rest) = token.strip_prefix('-') {
        if is_negatable_spacing_token(rest) {
            return classify_utility(rest).and_then(negate_margin_or_translate);
        }
    }

    match token {
        "hidden" => return Some(KnownUtility::DisplayNone),
        "flex" => return Some(KnownUtility::DisplayFlex),
        "flex-row" => return Some(KnownUtility::FlexRow),
        "flex-col" => return Some(KnownUtility::FlexCol),
        "flex-wrap" | "wrap" => return Some(KnownUtility::FlexWrap),
        "flex-nowrap" | "nowrap" => return Some(KnownUtility::FlexNoWrap),
        "grid" => return Some(KnownUtility::DisplayGrid),
        "items-center" => return Some(KnownUtility::ItemsCenter),
        "justify-center" => return Some(KnownUtility::JustifyCenter),
        "justify-end" => return Some(KnownUtility::JustifyEnd),
        "grow" => return Some(KnownUtility::Grow(1.0)),
        "grow-0" => return Some(KnownUtility::Grow(0.0)),
        "shrink" => return Some(KnownUtility::Shrink(1.0)),
        "shrink-0" => return Some(KnownUtility::Shrink(0.0)),
        "overflow-hidden" => return Some(KnownUtility::Overflow("hidden")),
        "overflow-auto" => return Some(KnownUtility::Overflow("auto")),
        "overflow-scroll" => return Some(KnownUtility::Overflow("scroll")),
        "rounded" => return Some(KnownUtility::Rounded(TwLength::Rem(0.375))),
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
        "transition-padding" => return Some(KnownUtility::TransitionPadding),
        "transition-none" => return Some(KnownUtility::TransitionNone),
        "will-change-auto" => return Some(KnownUtility::WillChange("auto")),
        "will-change-transform" => return Some(KnownUtility::WillChange("transform")),
        "will-change-opacity" => return Some(KnownUtility::WillChange("opacity")),
        "will-change-scroll" => return Some(KnownUtility::WillChange("scroll-position")),
        "will-change-contents" => return Some(KnownUtility::WillChange("contents")),
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
        "shimmer" => return Some(KnownUtility::Shimmer),
        "shimmer-once" => return Some(KnownUtility::ShimmerOnce),
        "shimmer-box" | "animate-shimmer" => return Some(KnownUtility::ShimmerBox),
        "shimmer-reverse" => return Some(KnownUtility::ShimmerReverse),
        "shimmer-none" => return Some(KnownUtility::ShimmerNone),
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
        return Some(KnownUtility::Gap(v));
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

    for (prefix, property) in [
        ("justify-items-", "justify-items"),
        ("justify-self-", "justify-self"),
        ("justify-", "justify-content"),
        ("items-", "align-items"),
        ("content-", "align-content"),
        ("self-", "align-self"),
    ] {
        if let Some(keyword) = token.strip_prefix(prefix) {
            let content = property.ends_with("content");
            let valid = match keyword {
                "normal" | "start" | "end" | "flex-start" | "flex-end" | "center" | "stretch" => {
                    true
                }
                "auto" => property.ends_with("self"),
                "self-start" | "self-end" | "baseline" | "first-baseline" => !content,
                "between" | "around" | "evenly" | "space-between" | "space-around"
                | "space-evenly" => content,
                _ => {
                    let position = keyword
                        .strip_prefix("safe-")
                        .or_else(|| keyword.strip_prefix("unsafe-"));
                    position.is_some_and(|position| {
                        matches!(
                            position,
                            "start" | "end" | "flex-start" | "flex-end" | "center"
                        ) || (!content && matches!(position, "self-start" | "self-end"))
                    })
                }
            };
            return valid.then_some(KnownUtility::Alignment(property, keyword));
        }
    }

    if let Some(rest) = token.strip_prefix("rounded-") {
        let len = match rest {
            "none" => TwLength::Px(0.0),
            "sm" => TwLength::Rem(0.125),
            "md" => TwLength::Rem(0.375),
            "lg" => TwLength::Rem(0.5),
            "xl" => TwLength::Rem(0.75),
            "2xl" => TwLength::Rem(1.0),
            "full" => return Some(KnownUtility::RoundedFull),
            _ => spacing_to_tw_length(rest)?,
        };
        return Some(KnownUtility::Rounded(len));
    }

    if let Some(rest) = token.strip_prefix("text-") {
        if let Some(keyword) = rest.strip_prefix("last-") {
            if matches!(
                keyword,
                "auto" | "start" | "end" | "left" | "right" | "center" | "justify"
            ) {
                return Some(KnownUtility::TextAlignLast(keyword));
            }
            return None;
        }
        if let Some(mode) = rest.strip_prefix("overflow-") {
            match mode {
                "clip" | "hidden" | "ellipsis" | "newline" | "wrap" => {
                    return Some(KnownUtility::TextOverflow(mode));
                }
                _ => {}
            }
        }
        match rest {
            "left" | "center" | "right" | "start" | "end" | "justify" | "justify-all"
            | "match-parent" => {
                return Some(KnownUtility::TextAlign(rest));
            }
            _ => {}
        }
        if let Some(len) = text_size_token_to_tw(rest) {
            return Some(KnownUtility::TextSize(len));
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
        return match rest {
            "sans" => Some(KnownUtility::FontFamilySans),
            "serif" => Some(KnownUtility::FontFamilySerif),
            "mono" | "monospace" => Some(KnownUtility::FontFamilyMono),
            _ => Some(KnownUtility::FontWeight(FontWeightToken::from_rest(rest)?)),
        };
    }
    if let Some(rest) = token.strip_prefix("leading-") {
        if let Some(pixels) = rest
            .strip_prefix('[')
            .and_then(|value| value.strip_suffix("px]"))
        {
            return pixels
                .parse::<f32>()
                .ok()
                .filter(|value| value.is_finite() && *value > 0.0)
                .map(KnownUtility::LeadingPx);
        }
        return leading_scale_token(rest).map(KnownUtility::Leading);
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
        "writing-horizontal-tb" => return Some(KnownUtility::WritingMode("horizontal-tb")),
        "writing-vertical-rl" => return Some(KnownUtility::WritingMode("vertical-rl")),
        "writing-vertical-lr" => return Some(KnownUtility::WritingMode("vertical-lr")),
        "writing-sideways-rl" => return Some(KnownUtility::WritingMode("sideways-rl")),
        "writing-sideways-lr" => return Some(KnownUtility::WritingMode("sideways-lr")),
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
        let len = spacing_to_tw_length(rest)?;
        return Some(KnownUtility::OutlineOffset(len));
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

    match token {
        "underline" => return Some(KnownUtility::Underline(true)),
        "no-underline" => return Some(KnownUtility::Underline(false)),
        _ => {}
    }
    if let Some(rest) = token.strip_prefix("underline-offset-") {
        return parse_underline_offset(rest).map(KnownUtility::UnderlineOffset);
    }

    // Focus-ring utilities (Tailwind `ring-*` → authored focus-ring width/color).
    if token == "ring" {
        return Some(KnownUtility::RingWidth(3.0));
    }
    if let Some(rest) = token.strip_prefix("ring-") {
        if rest == "inset" || rest.starts_with("offset") {
            return None;
        }
        if let Some(px) = outline_width_token(rest) {
            return Some(KnownUtility::RingWidth(px));
        }
        if !rest.is_empty() {
            return Some(KnownUtility::RingColor(rest));
        }
    }

    if let Some(rest) = token.strip_prefix("shimmer-count-") {
        return parse_shimmer_count(rest).map(KnownUtility::ShimmerCount);
    }
    if let Some(rest) = token.strip_prefix("shimmer-duration-") {
        if let Ok(ms) = rest.parse::<u32>() {
            return Some(KnownUtility::ShimmerDuration(ms));
        }
    }
    if let Some(rest) = token.strip_prefix("shimmer-spread-") {
        if let Some(px) = parse_shimmer_spread_px(rest) {
            return Some(KnownUtility::ShimmerSpread(px));
        }
    }
    if let Some(rest) = token.strip_prefix("shimmer-angle-") {
        if let Some(deg) = parse_shimmer_angle_deg(rest) {
            return Some(KnownUtility::ShimmerAngle(deg));
        }
    }
    if let Some(rest) = token.strip_prefix("shimmer-color-") {
        if !rest.is_empty() {
            return Some(KnownUtility::ShimmerColor(rest));
        }
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
            return parse_arbitrary_tw_length(inner.trim())
                .or_else(|| inner.trim().parse().ok().map(TwLength::Px))
                .map(KnownUtility::LetterSpacing);
        }
        let em = match rest {
            "tighter" => -0.05,
            "tight" => -0.025,
            "normal" => 0.0,
            "wide" => 0.025,
            "wider" => 0.05,
            "widest" => 0.1,
            _ => return None,
        };
        return Some(KnownUtility::LetterSpacing(TwLength::Em(em)));
    }

    // CSS `filter` / `backdrop-filter` — Tailwind v4 `--blur-*`.
    if token == "filter-none" {
        return Some(KnownUtility::FilterNone);
    }
    if token == "blur" {
        // v3 default `blur` ≡ v4 `blur-sm`.
        return Some(KnownUtility::FilterBlur(TwLength::Px(8.0)));
    }
    if let Some(rest) = token.strip_prefix("blur-") {
        return match classify_tw_blur_rest(rest)? {
            TwBlur::None => Some(KnownUtility::FilterNone),
            TwBlur::Len(len) => Some(KnownUtility::FilterBlur(len)),
        };
    }
    if token == "backdrop-blur" {
        return Some(KnownUtility::BackdropBlur(TwLength::Px(8.0)));
    }
    if token == "glass" {
        return Some(KnownUtility::Glass([1.46, 12.0, 0.025, 0.6]));
    }
    if token == "glass-none" {
        return Some(KnownUtility::Glass([0.0; 4]));
    }
    if let Some(rest) = token
        .strip_prefix("glass-[")
        .and_then(|rest| rest.strip_suffix(']'))
    {
        let values = rest
            .split('_')
            .map(str::parse::<f32>)
            .collect::<Result<Vec<_>, _>>()
            .ok()?;
        let values: [f32; 4] = values.try_into().ok()?;
        if values.iter().all(|value| value.is_finite())
            && (1.0..=2.5).contains(&values[0])
            && (0.0..=64.0).contains(&values[1])
            && (0.0..=0.2).contains(&values[2])
            && (0.0..=1.0).contains(&values[3])
        {
            return Some(KnownUtility::Glass(values));
        }
        return None;
    }
    if let Some(rest) = token.strip_prefix("backdrop-blur-") {
        return match classify_tw_blur_rest(rest)? {
            TwBlur::None => Some(KnownUtility::BackdropBlur(TwLength::Px(0.0))),
            TwBlur::Len(len) => Some(KnownUtility::BackdropBlur(len)),
        };
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

/// `underline-offset-4` → spacing rem scale. `underline-offset-[4px]` / `[0.25rem]` / `[1em]` keep the unit.
fn parse_underline_offset(rest: &str) -> Option<TwLength> {
    if rest == "auto" {
        return None;
    }
    spacing_to_tw_length(rest)
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
        _ => {
            if let Some(pct) = parse_tailwind_fraction_ratio(rest) {
                return Some(if is_x {
                    KnownUtility::TranslateXPct(pct)
                } else {
                    KnownUtility::TranslateYPct(pct)
                });
            }
        }
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
        if let Some(len) = parse_arbitrary_tw_length(inner) {
            return Some(if is_x {
                KnownUtility::TranslateX(len)
            } else {
                KnownUtility::TranslateY(len)
            });
        }
        let v: f32 = inner.parse().ok()?;
        return Some(if is_x {
            KnownUtility::TranslateX(TwLength::Px(v))
        } else {
            KnownUtility::TranslateY(TwLength::Px(v))
        });
    }
    spacing_to_tw_length(rest).map(|len| {
        if is_x {
            KnownUtility::TranslateX(len)
        } else {
            KnownUtility::TranslateY(len)
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

/// Tailwind v4 `@theme` `--blur-*` (logical px).
fn tailwind_v4_blur_px(scale: &str) -> Option<f32> {
    Some(match scale {
        "xs" => 4.0,
        "sm" => 8.0,
        "md" => 12.0,
        "lg" => 16.0,
        "xl" => 24.0,
        "2xl" => 40.0,
        "3xl" => 64.0,
        _ => return None,
    })
}

enum TwBlur {
    None,
    Len(TwLength),
}

fn classify_tw_blur_rest(rest: &str) -> Option<TwBlur> {
    if rest == "none" {
        return Some(TwBlur::None);
    }
    if let Some(px) = tailwind_v4_blur_px(rest) {
        return Some(TwBlur::Len(TwLength::Px(px)));
    }
    if let Some(inner) = rest.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        return parse_tw_blur_arbitrary(inner).map(TwBlur::Len);
    }
    if let Some(inner) = rest.strip_prefix('(').and_then(|s| s.strip_suffix(')')) {
        return parse_tw_blur_custom_property_px(inner)
            .map(TwLength::Px)
            .map(TwBlur::Len);
    }
    None
}

/// `blur-[<value>]`: unitless is rem (same as RSX lengths). `1px` / `1rem` / `1em` allowed.
fn parse_tw_blur_arbitrary(inner: &str) -> Option<TwLength> {
    let inner = inner.trim().replace('_', "");
    if inner.contains('%') {
        return None;
    }
    if let Some(len) = parse_arbitrary_tw_length(&inner) {
        return Some(len);
    }
    parse_nonneg_f32(&inner).map(TwLength::Rem)
}

/// `blur-(--blur-sm)` → theme scale. Unknown custom properties are not classified.
fn parse_tw_blur_custom_property_px(inner: &str) -> Option<f32> {
    let name = inner.trim();
    let name = name.strip_prefix("--").unwrap_or(name);
    let scale = name.strip_prefix("blur-").unwrap_or(name);
    tailwind_v4_blur_px(scale)
}

fn parse_nonneg_f32(s: &str) -> Option<f32> {
    s.trim()
        .parse::<f32>()
        .ok()
        .filter(|v| v.is_finite())
        .map(|v| v.max(0.0))
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

fn parse_shimmer_count(rest: &str) -> Option<Option<f32>> {
    let rest = rest
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(rest)
        .trim();
    if rest.eq_ignore_ascii_case("infinite") || rest.eq_ignore_ascii_case("inf") {
        return Some(None);
    }
    let n: f32 = rest.parse().ok()?;
    if n > 0.0 && n.is_finite() {
        Some(Some(n))
    } else {
        None
    }
}

fn parse_shimmer_spread_px(rest: &str) -> Option<f32> {
    if let Some(inner) = rest.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        let v = inner.trim();
        if let Some(n) = v.strip_suffix('%') {
            // Percentage of a ~1-line caption; treat as a generous band width.
            return n.parse::<f32>().ok().map(|p| (p / 100.0) * 80.0);
        }
        return spacing_token_to_px(v);
    }
    spacing_token_to_px(rest)
}

fn parse_shimmer_angle_deg(rest: &str) -> Option<f32> {
    if let Some(inner) = rest.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        let v = inner.trim().trim_end_matches("deg");
        return v.parse().ok();
    }
    rest.parse().ok()
}

fn leading_scale_token(rest: &str) -> Option<f32> {
    match rest {
        "none" => Some(1.0),
        "tight" => Some(1.25),
        "snug" => Some(1.375),
        "normal" => Some(1.5),
        "relaxed" => Some(1.625),
        "loose" => Some(2.0),
        _ => {
            let inner = rest.strip_prefix('[').and_then(|s| s.strip_suffix(']'))?;
            let v = inner.trim().trim_end_matches("em");
            v.parse().ok().filter(|n: &f32| n.is_finite() && *n > 0.0)
        }
    }
}

#[inline]
pub fn is_known_utility(token: &str) -> bool {
    classify_utility(token).is_some()
}

#[cfg(test)]
#[path = "tests/catalog.rs"]
mod tests;
