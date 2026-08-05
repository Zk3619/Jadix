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
    /// Visual translate Y in logical px (`translate-y-*`).
    TranslateY(f32),
    /// Single-line ellipsis overflow (`truncate`).
    Truncate,
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
        "full" => Some(9999.0),
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
    let px = spacing_token_to_px(rest)?;
    Some(format!("{px}px"))
}

/// Classify a bare utility token (no `hover:` / `dark:` prefixes).
pub fn classify_utility(token: &str) -> Option<KnownUtility<'_>> {
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
        _ => {}
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
        if let Some(px) = text_size_token_to_px(rest) {
            return Some(KnownUtility::TextSize(px));
        }
        return Some(KnownUtility::TextColor(rest));
    }
    if let Some(rest) = token.strip_prefix("bg-") {
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
        return spacing_token_to_px(rest).map(KnownUtility::TranslateX);
    }
    if let Some(rest) = token.strip_prefix("translate-y-") {
        return spacing_token_to_px(rest).map(KnownUtility::TranslateY);
    }
    if token == "truncate" {
        return Some(KnownUtility::Truncate);
    }

    None
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
        assert!(!is_known_utility("not-a-real-class"));
    }
}
