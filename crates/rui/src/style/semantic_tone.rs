//! 跨组件语义语气（`info` / `success` / `warning` / `destructive`）解析与配色。
//!
//! [`SemanticPalette`] / [`soft_semantic_mix_alphas`] 为共享 tone→色板入口：
//! [`semantic_chrome`]（Alert / Badge / Attachment）、[`button_soft_semantic_chrome`]（Button soft tint）、
//! [`semantic_media_wash`]（Attachment media）均从此取 accent / mix 比例。
//! 混色原语见 [`crate::style::color_mix_over_surface`] / [`crate::style::color_mix_over_background`]。
//! 边框、阴影仅在 [`SemanticChromeDecor`] 启用时写入 [`SemanticChrome`]；组件侧对 `Option` 再落盒模型。
//! 别名：`error`≡`destructive` · `warn`≡`warning` · 未知→[`SemanticTone::Default`].

use crate::core::tree::Element;
use crate::style::{is_dark_theme, parse_color, parse_text_color};

/// 语义语气（非 Button `outline`/`ghost` 等产品 variant）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SemanticTone {
    #[default]
    Default,
    Info,
    Success,
    Warning,
    Destructive,
}

impl SemanticTone {
    /// `error`→Destructive · `warn`→Warning · 未知→Default
    pub fn parse(s: &str) -> Self {
        s.parse().unwrap_or(Self::Default)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Info => "info",
            Self::Success => "success",
            Self::Warning => "warning",
            Self::Destructive => "destructive",
        }
    }

    pub fn is_semantic(self) -> bool {
        !matches!(self, Self::Default)
    }

    /// 前景 / soft fill 用的 accent 令牌（与 Badge / Button 语义色同源）。
    pub fn accent_fg_token(self) -> &'static str {
        match self {
            Self::Default => "foreground",
            Self::Info => "blue-600",
            Self::Success => "green-600",
            Self::Warning => "orange-600",
            Self::Destructive => "destructive",
        }
    }

    /// 不填充背景时使用的语义边框色（与 Toast `tone_colors` 同源）。
    pub fn accent_border_token(self) -> &'static str {
        match self {
            Self::Default => "border",
            Self::Info => "blue-300",
            Self::Success => "green-300",
            Self::Warning => "orange-300",
            Self::Destructive => "red-300",
        }
    }

    /// Toast 等浮层语义阴影色相（强度由 [`semantic_shadow`] 统一）。
    pub fn accent_shadow_token(self) -> &'static str {
        match self {
            Self::Default => "foreground",
            Self::Info => "blue-500",
            Self::Success => "green-500",
            Self::Warning => "orange-500",
            Self::Destructive => "red-500",
        }
    }

    /// Toast / Alert 默认图标名。
    pub fn default_icon(self) -> Option<&'static str> {
        match self {
            Self::Default => None,
            Self::Info => Some("info"),
            Self::Success => Some("circle-check"),
            Self::Warning => Some("circle-alert"),
            Self::Destructive => Some("circle-x"),
        }
    }
}

impl From<&str> for SemanticTone {
    fn from(s: &str) -> Self {
        Self::parse(s)
    }
}

impl From<String> for SemanticTone {
    fn from(s: String) -> Self {
        Self::parse(&s)
    }
}

impl std::str::FromStr for SemanticTone {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_ascii_lowercase().as_str() {
            "info" => Self::Info,
            "success" => Self::Success,
            "warning" | "warn" => Self::Warning,
            "destructive" | "error" => Self::Destructive,
            _ => Self::Default,
        })
    }
}

/// 不填充时使用的表面令牌（Alert=`card` · Toast=`popover`）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SemanticSurface {
    #[default]
    Card,
    Popover,
}

impl SemanticSurface {
    fn bg_token(self) -> &'static str {
        match self {
            Self::Card => "card",
            Self::Popover => "popover",
        }
    }

    fn fg_token(self) -> &'static str {
        match self {
            Self::Card => "card-foreground",
            Self::Popover => "foreground",
        }
    }
}

/// 变体 chrome 中输出哪些盒模型装饰（组件仅对 `Some` 字段落样式）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SemanticChromeDecor {
    pub border: bool,
    pub shadow: bool,
}

impl SemanticChromeDecor {
    pub const NONE: Self = Self {
        border: false,
        shadow: false,
    };
    pub const BORDERED: Self = Self {
        border: true,
        shadow: false,
    };
    pub const ELEVATED: Self = Self {
        border: true,
        shadow: true,
    };
}

/// 语义配色选项。
#[derive(Clone, Copy, Debug)]
pub struct SemanticChromeOpts {
    pub tone: SemanticTone,
    /// `true`：Badge 式 soft tint 填充 `bg`；`false`：主题表面色 + 语义边框/字色（Toast 式）。
    pub filled: bool,
    pub surface: SemanticSurface,
    pub decor: SemanticChromeDecor,
}

impl Default for SemanticChromeOpts {
    fn default() -> Self {
        Self {
            tone: SemanticTone::Default,
            filled: false,
            surface: SemanticSurface::Card,
            decor: SemanticChromeDecor::BORDERED,
        }
    }
}

/// 解析后的边框（仅 decor.border 时存在）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SemanticBorder {
    pub color: [f32; 4],
    pub width: f32,
}

/// 解析后的阴影（仅 decor.shadow 时存在）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SemanticShadow {
    pub color: [f32; 4],
    pub blur: f32,
    pub spread: f32,
}

/// 解析后的容器色（bg / 主字色 / 副字色 + 可选 border / shadow）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SemanticChrome {
    pub bg: [f32; 4],
    pub fg: [u8; 4],
    pub muted_fg: [u8; 4],
    pub border: Option<SemanticBorder>,
    pub shadow: Option<SemanticShadow>,
}

/// Resolved accent colors for a tone (shared by [`semantic_chrome`], Button soft tint, Attachment media wash).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SemanticPalette {
    pub tone: SemanticTone,
    /// Accent in linear display RGBA (`accent_fg_token` resolved).
    pub accent: [f32; 4],
    /// Opaque caption / icon foreground (same token as accent).
    pub fg: [u8; 4],
    /// Bordered chrome outline (`accent_border_token`).
    pub border: [f32; 4],
    /// Shadow hue (`accent_shadow_token`, alpha unset).
    pub shadow: [f32; 4],
}

/// Soft-fill mix alphas for rest / hover (light vs dark) — Badge, Button semantic variants.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SoftSemanticMix {
    pub rest: f32,
    pub hover: f32,
}

/// Shared rest/hover mix ratios for soft semantic fills (Button hover, Badge tint).
pub fn soft_semantic_mix_alphas() -> SoftSemanticMix {
    if is_dark_theme() {
        SoftSemanticMix {
            rest: 0.2,
            hover: 0.3,
        }
    } else {
        SoftSemanticMix {
            rest: 0.1,
            hover: 0.2,
        }
    }
}

/// Resolve tone → accent / fg / border / shadow tokens (single palette source).
pub fn semantic_palette(tone: SemanticTone) -> SemanticPalette {
    let accent_token = tone.accent_fg_token();
    SemanticPalette {
        tone,
        accent: parse_color(accent_token),
        fg: parse_text_color(accent_token),
        border: parse_color(tone.accent_border_token()),
        shadow: parse_color(tone.accent_shadow_token()),
    }
}

/// Attachment / chip media box wash (`accent/10` over background).
pub const SEMANTIC_MEDIA_WASH_ALPHA: f32 = 0.1;

/// Media-slot accent wash; `None` for [`SemanticTone::Default`].
pub fn semantic_media_wash(tone: SemanticTone) -> Option<[f32; 4]> {
    if !tone.is_semantic() {
        return None;
    }
    let palette = semantic_palette(tone);
    Some(crate::style::color_mix_over_surface(
        palette.accent,
        parse_color("background"),
        SEMANTIC_MEDIA_WASH_ALPHA,
    ))
}

/// Badge / Button 语义静止态 soft tint（浅 rest / 深 rest — 见 [`soft_semantic_mix_alphas`]）。
pub fn soft_semantic_fill(accent: [f32; 4]) -> [f32; 4] {
    crate::style::color_mix_over_background(accent, soft_semantic_mix_alphas().rest)
}

/// Badge / Button 语义 hover 端点 soft tint。
pub fn soft_semantic_fill_hover(accent: [f32; 4]) -> [f32; 4] {
    crate::style::color_mix_over_background(accent, soft_semantic_mix_alphas().hover)
}

/// Button semantic variant chrome: soft tint rest→hover + opaque accent caption.
pub fn button_soft_semantic_chrome(
    tone: SemanticTone,
    hover_t: f32,
) -> ([f32; 4], [u8; 4], Option<[f32; 4]>) {
    use crate::style::{lerp_paint_chrome, PaintChrome};
    let palette = semantic_palette(tone);
    let mix = soft_semantic_mix_alphas();
    let rest = PaintChrome {
        bg: crate::style::color_mix_over_background(palette.accent, mix.rest),
        fg: palette.fg,
        border: None,
    };
    let hover = PaintChrome {
        bg: crate::style::color_mix_over_background(palette.accent, mix.hover),
        fg: palette.fg,
        border: None,
    };
    let c = lerp_paint_chrome(rest, hover, hover_t);
    (c.bg, c.fg, c.border)
}

/// 语义副文案色：与 Toast 描述一致，accent 前景降 alpha（非主题 `muted-foreground`）。
const SEMANTIC_DESC_FG_ALPHA: f32 = 0.78;

/// 语义副文案色：accent 前景降 alpha（Alert / Toast 描述共用）。
pub fn muted_from_accent(fg: [u8; 4]) -> [u8; 4] {
    let mut c = fg;
    if c[3] > 0 {
        c[3] = (c[3] as f32 * SEMANTIC_DESC_FG_ALPHA) as u8;
    }
    c
}

const SHADOW_BLUR: f32 = 10.0;
const SHADOW_SPREAD: f32 = -2.0;
const SHADOW_ALPHA: f32 = 0.12;
const BORDER_WIDTH: f32 = 1.0;

/// Neutral modal scrim alpha (Dialog / AlertDialog default).
pub const MODAL_OVERLAY_OPACITY_NEUTRAL: f32 = 0.25;
/// Semantic modal scrim alpha when tone is non-default (light theme).
const MODAL_OVERLAY_OPACITY_SEMANTIC_LIGHT: f32 = 0.32;
/// Semantic modal scrim alpha when tone is non-default (dark theme).
const MODAL_OVERLAY_OPACITY_SEMANTIC_DARK: f32 = 0.40;
/// Accent wash mixed into `background` before scrim alpha (light / dark).
const MODAL_OVERLAY_SEMANTIC_WASH_LIGHT: f32 = 0.12;
const MODAL_OVERLAY_SEMANTIC_WASH_DARK: f32 = 0.22;

/// 四边均匀阴影（offset 0,0）；语义变体仅换色相，强度与 default 一致。
pub fn semantic_shadow(tone: SemanticTone) -> SemanticShadow {
    let color = if tone.is_semantic() {
        let mut c = semantic_palette(tone).shadow;
        c[3] = SHADOW_ALPHA;
        c
    } else {
        [0.0, 0.0, 0.0, SHADOW_ALPHA]
    };
    SemanticShadow {
        color,
        blur: SHADOW_BLUR,
        spread: SHADOW_SPREAD,
    }
}

fn resolve_border(
    tone: SemanticTone,
    filled: bool,
    decor: SemanticChromeDecor,
) -> Option<SemanticBorder> {
    if !decor.border {
        return None;
    }
    let color = if filled || !tone.is_semantic() {
        parse_color("border")
    } else {
        semantic_palette(tone).border
    };
    Some(SemanticBorder {
        color,
        width: BORDER_WIDTH,
    })
}

/// Modal underlay scrim: neutral `background` wash, or semantic accent tint when [`SemanticTone::is_semantic`].
///
/// Explicit `color` / `opacity` always win (product override). Token resolution follows the active
/// theme ([`is_dark_theme`]) so light/dark switches repaint with correct hues.
pub fn semantic_modal_overlay_scrim(
    tone: SemanticTone,
    explicit_color: Option<&str>,
    explicit_opacity: Option<f32>,
) -> [f32; 4] {
    if let Some(s) = explicit_color.filter(|s| !s.is_empty()) {
        let mut c = parse_color(s);
        c[3] = explicit_opacity
            .unwrap_or(MODAL_OVERLAY_OPACITY_NEUTRAL)
            .clamp(0.0, 1.0);
        return c;
    }
    if !tone.is_semantic() {
        let mut c = parse_color("background");
        c[3] = explicit_opacity
            .unwrap_or(MODAL_OVERLAY_OPACITY_NEUTRAL)
            .clamp(0.0, 1.0);
        return c;
    }
    let accent = semantic_palette(tone).accent;
    let wash = if is_dark_theme() {
        MODAL_OVERLAY_SEMANTIC_WASH_DARK
    } else {
        MODAL_OVERLAY_SEMANTIC_WASH_LIGHT
    };
    let mut c = crate::style::color_mix_over_background(accent, wash);
    c[3] = explicit_opacity
        .unwrap_or_else(|| {
            if is_dark_theme() {
                MODAL_OVERLAY_OPACITY_SEMANTIC_DARK
            } else {
                MODAL_OVERLAY_OPACITY_SEMANTIC_LIGHT
            }
        })
        .clamp(0.0, 1.0);
    c
}

/// 统一语义配色入口。
pub fn semantic_chrome(opts: SemanticChromeOpts) -> SemanticChrome {
    let SemanticChromeOpts {
        tone,
        filled,
        surface,
        decor,
    } = opts;

    let border = resolve_border(tone, filled, decor);
    let shadow = if decor.shadow {
        Some(semantic_shadow(tone))
    } else {
        None
    };

    if !tone.is_semantic() {
        return SemanticChrome {
            bg: parse_color(surface.bg_token()),
            fg: parse_text_color(surface.fg_token()),
            muted_fg: parse_text_color("muted-foreground"),
            border,
            shadow,
        };
    }

    let palette = semantic_palette(tone);
    let accent = palette.accent;
    let fg = palette.fg;

    if filled {
        SemanticChrome {
            bg: soft_semantic_fill(accent),
            fg,
            muted_fg: muted_from_accent(fg),
            border,
            shadow,
        }
    } else {
        SemanticChrome {
            bg: parse_color(surface.bg_token()),
            fg,
            muted_fg: muted_from_accent(fg),
            border,
            shadow,
        }
    }
}

/// 仅当 chrome 配置了边框时写入 Element 盒模型。
pub fn apply_semantic_border(el: &mut Element, border: Option<SemanticBorder>) {
    if let Some(b) = border {
        el.style.border_color = b.color;
        el.style.border_width = b.width;
    } else {
        el.style.border_width = 0.0;
        el.style.border_color = [0.0, 0.0, 0.0, 0.0];
    }
}

/// 仅当 chrome 配置了阴影时写入 Element 盒模型。
pub fn apply_semantic_shadow(el: &mut Element, shadow: Option<SemanticShadow>) {
    if let Some(s) = shadow {
        el.style.shadow_color = s.color;
        el.style.shadow_blur = s.blur;
        el.style.shadow_spread = s.spread;
    } else {
        el.style.shadow_color = [0.0, 0.0, 0.0, 0.0];
        el.style.shadow_blur = 0.0;
        el.style.shadow_spread = 0.0;
    }
}

/// 堆叠后方层等场景：在已有 shadow 上淡出 alpha（blur/spread 不变）。
pub fn dim_semantic_shadow(shadow: SemanticShadow, dim: bool) -> SemanticShadow {
    let mut s = shadow;
    if dim {
        s.color[3] = 0.0;
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_aliases_and_unknown() {
        assert_eq!(SemanticTone::parse("error"), SemanticTone::Destructive);
        assert_eq!(
            SemanticTone::parse("destructive"),
            SemanticTone::Destructive
        );
        assert_eq!(SemanticTone::parse("warn"), SemanticTone::Warning);
        assert_eq!(SemanticTone::parse("warning"), SemanticTone::Warning);
        assert_eq!(SemanticTone::parse("INFO"), SemanticTone::Info);
        assert_eq!(SemanticTone::parse("nope"), SemanticTone::Default);
    }

    #[test]
    fn filled_soft_matches_badge_recipe() {
        let accent = parse_color("green-600");
        let filled = soft_semantic_fill(accent);
        let mix = soft_semantic_mix_alphas();
        let bg = parse_color("background");
        let expect = [
            accent[0] * mix.rest + bg[0] * (1.0 - mix.rest),
            accent[1] * mix.rest + bg[1] * (1.0 - mix.rest),
            accent[2] * mix.rest + bg[2] * (1.0 - mix.rest),
            1.0,
        ];
        assert_eq!(filled, expect);
    }

    #[test]
    fn semantic_palette_resolves_tone_tokens() {
        let p = semantic_palette(SemanticTone::Success);
        assert_eq!(p.accent, parse_color("green-600"));
        assert_eq!(p.fg, parse_text_color("green-600"));
        assert_eq!(p.border, parse_color("green-300"));
        assert_eq!(p.shadow, parse_color("green-500"));
    }

    #[test]
    fn button_soft_rest_matches_soft_semantic_fill() {
        let tone = SemanticTone::Warning;
        let (bg, fg, border) = button_soft_semantic_chrome(tone, 0.0);
        assert_eq!(bg, soft_semantic_fill(semantic_palette(tone).accent));
        assert_eq!(fg, parse_text_color("orange-600"));
        assert!(border.is_none());
    }

    #[test]
    fn button_soft_hover_endpoint_matches_fill_hover() {
        let tone = SemanticTone::Destructive;
        let (bg, ..) = button_soft_semantic_chrome(tone, 1.0);
        assert_eq!(bg, soft_semantic_fill_hover(semantic_palette(tone).accent));
    }

    #[test]
    fn semantic_media_wash_matches_attachment_recipe() {
        let wash = semantic_media_wash(SemanticTone::Info).expect("info wash");
        let accent = semantic_palette(SemanticTone::Info).accent;
        assert_eq!(
            wash,
            crate::style::color_mix_over_background(accent, SEMANTIC_MEDIA_WASH_ALPHA)
        );
    }

    #[test]
    fn unfilled_semantic_description_uses_accent_muted_not_theme_muted() {
        let c = semantic_chrome(SemanticChromeOpts {
            tone: SemanticTone::Info,
            filled: false,
            surface: SemanticSurface::Card,
            decor: SemanticChromeDecor::BORDERED,
        });
        assert_ne!(c.muted_fg, parse_text_color("muted-foreground"));
        assert_eq!(c.muted_fg, muted_from_accent(c.fg));
    }

    #[test]
    fn unfilled_bordered_uses_theme_surface_and_semantic_border() {
        let c = semantic_chrome(SemanticChromeOpts {
            tone: SemanticTone::Info,
            filled: false,
            surface: SemanticSurface::Card,
            decor: SemanticChromeDecor::BORDERED,
        });
        assert_eq!(c.bg, parse_color("card"));
        let border = c.border.expect("border configured");
        assert_eq!(border.color, parse_color("blue-300"));
        assert!(c.shadow.is_none());
    }

    #[test]
    fn filled_without_decor_has_no_border_or_shadow() {
        let c = semantic_chrome(SemanticChromeOpts {
            tone: SemanticTone::Success,
            filled: true,
            surface: SemanticSurface::Card,
            decor: SemanticChromeDecor::NONE,
        });
        assert_eq!(c.bg, soft_semantic_fill(parse_color("green-600")));
        assert!(c.border.is_none());
        assert!(c.shadow.is_none());
    }

    #[test]
    fn elevated_toast_chrome_includes_border_and_shadow() {
        let c = semantic_chrome(SemanticChromeOpts {
            tone: SemanticTone::Success,
            filled: false,
            surface: SemanticSurface::Popover,
            decor: SemanticChromeDecor::ELEVATED,
        });
        assert_eq!(c.bg, parse_color("popover"));
        let border = c.border.expect("border");
        assert_eq!(border.color, parse_color("green-300"));
        let shadow = c.shadow.expect("shadow");
        assert!((shadow.color[3] - SHADOW_ALPHA).abs() < 0.001);
        assert!(shadow.color[1] > shadow.color[0]);
    }

    #[test]
    fn semantic_modal_overlay_scrim_uses_neutral_for_default() {
        let c = semantic_modal_overlay_scrim(SemanticTone::Default, None, None);
        let mut neutral = parse_color("background");
        neutral[3] = MODAL_OVERLAY_OPACITY_NEUTRAL;
        assert_eq!(c, neutral);
    }

    #[test]
    fn semantic_modal_overlay_scrim_tints_info() {
        let c = semantic_modal_overlay_scrim(SemanticTone::Info, None, None);
        let neutral = semantic_modal_overlay_scrim(SemanticTone::Default, None, None);
        assert_ne!(c, neutral);
        assert!(c[2] > c[0], "info scrim should be blue-tinted, got {c:?}");
    }

    #[test]
    fn semantic_modal_overlay_scrim_respects_explicit_color() {
        let c = semantic_modal_overlay_scrim(SemanticTone::Warning, Some("red-500"), Some(0.5));
        let mut expect = parse_color("red-500");
        expect[3] = 0.5;
        assert_eq!(c, expect);
    }

    #[test]
    fn semantic_shadow_success_is_green_tinted() {
        let s = semantic_shadow(SemanticTone::Success);
        assert!(
            s.color[1] > s.color[0]
                && s.color[1] > s.color[2]
                && (s.color[3] - SHADOW_ALPHA).abs() < 0.001,
            "success shadow must be green-tinted, got {:?}",
            s.color
        );
    }
}
