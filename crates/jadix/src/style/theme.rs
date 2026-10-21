//! 设计令牌、可组合样式和 shadcn / Fluent / Material 3 / Liquid Glass 主题。
//!
//! 语义色源：`apps/v4/app/globals.css`（`:root` / `.dark`）；`chart-*` 引用 Tailwind v4
//! `theme.css` 的 `--color-blue-*`。组件 chrome（如 Button `/10` 叠底）在组件层用 token
//! 做透明度合成，**不在此硬编码采样 hex**。
//!
//! 支持运行时浅色 / 深色切换；[`get_theme`] 读取当前主题，[`set_theme_mode`] 切换。
//! 焦点环由 [`FocusRingStyle`]（Hard / Soft）+ [`Theme::focus_ring_width`] 配置，颜色走 token `ring`。

use std::cell::{Cell, RefCell};

use super::color_space::oklch_to_display_srgba;
use super::element_style::TimingFunction;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ThemeMode {
    #[default]
    Light,
    Dark,
}

/// Default Hard spread / Soft blur in logical px (ShadCN `ring-[3px]`).
pub const DEFAULT_FOCUS_RING_WIDTH: f32 = 3.0;

/// Global transition used when a theme rebuild changes component colors.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct ThemeTransitionConfig {
    pub enabled: bool,
    #[serde(alias = "duration-ms")]
    pub duration_ms: f32,
    pub easing: String,
}

impl Default for ThemeTransitionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            duration_ms: 180.0,
            easing: "ease-in-out".into(),
        }
    }
}

impl ThemeTransitionConfig {
    pub(crate) fn timing(&self) -> TimingFunction {
        TimingFunction::from_css(&self.easing)
    }
}

/// Theme-level focus-ring geometry (ShadCN v4 Hard vs Jadix Soft glow).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FocusRingStyle {
    /// Hard edge: `spread = `[`Theme::focus_ring_width`], `blur = 0`.
    #[default]
    Hard,
    /// Soft glow: `blur = `[`Theme::focus_ring_width`], `spread = 0`.
    Soft,
}

/// ShadCN v4 `@theme inline` 语义色（`globals.css` → `--color-*`）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DesignTokens {
    pub background: [f32; 4],
    pub foreground: [f32; 4],
    pub card: [f32; 4],
    pub card_foreground: [f32; 4],
    pub popover: [f32; 4],
    pub popover_foreground: [f32; 4],
    pub primary: [f32; 4],
    pub primary_foreground: [f32; 4],
    pub secondary: [f32; 4],
    pub secondary_foreground: [f32; 4],
    pub muted: [f32; 4],
    pub muted_foreground: [f32; 4],
    pub accent: [f32; 4],
    pub accent_foreground: [f32; 4],
    pub destructive: [f32; 4],
    pub destructive_foreground: [f32; 4],
    /// Caption / fill seed; default Tailwind `blue-600`. Lighter chrome is `info/10` not `blue-300`.
    pub info: [f32; 4],
    /// Caption / fill seed; default Tailwind `green-600`. Lighter chrome is `success/10` not `green-300`.
    pub success: [f32; 4],
    /// Caption / fill seed; default Tailwind `orange-600`. `warn` parses here. Lighter chrome is `warning/10`.
    pub warning: [f32; 4],
    pub border: [f32; 4],
    pub input: [f32; 4],
    /// `ring` 色（几何由 [`FocusRingStyle`] + [`Theme::focus_ring_width`] 决定）。
    pub ring: [f32; 4],
    pub chart_1: [f32; 4],
    pub chart_2: [f32; 4],
    pub chart_3: [f32; 4],
    pub chart_4: [f32; 4],
    pub chart_5: [f32; 4],
    pub sidebar: [f32; 4],
    pub sidebar_foreground: [f32; 4],
    pub sidebar_primary: [f32; 4],
    pub sidebar_primary_foreground: [f32; 4],
    pub sidebar_accent: [f32; 4],
    pub sidebar_accent_foreground: [f32; 4],
    pub sidebar_border: [f32; 4],
    pub sidebar_ring: [f32; 4],
    pub surface: [f32; 4],
    pub surface_foreground: [f32; 4],
    pub code: [f32; 4],
    pub code_foreground: [f32; 4],
    pub code_highlight: [f32; 4],
    pub code_number: [f32; 4],
    /// I-beam fill (`caret` / `caret-color`). Defaults to `foreground`.
    pub caret: [f32; 4],
    /// Selected-range fill (`selection-background`; alias `selection`).
    /// Inverse pair with [`Self::selection_foreground`].
    pub selection_background: [f32; 4],
    /// Selected glyph color (`selection-foreground`). Inverse of
    /// [`Self::selection_background`].
    pub selection_foreground: [f32; 4],
    /// Scrollbar track fill (`scrollbar-track`).
    pub scrollbar_track: [f32; 4],
    /// Scrollbar thumb fill (`scrollbar-thumb`).
    pub scrollbar_thumb: [f32; 4],
    /// Scrollbar track border (`scrollbar-track-border`); default transparent (overlay).
    pub scrollbar_track_border: [f32; 4],
    /// Scrollbar thumb border (`scrollbar-thumb-border`); default transparent.
    pub scrollbar_thumb_border: [f32; 4],
    /// `--radius: 0.625rem`
    pub radius: f32,
}

/// Tailwind v3 `*-600` as display sRGB (variant caption seed).
fn tw_600(r: u8, g: u8, b: u8) -> [f32; 4] {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
}

/// Tailwind v4 default palette — `theme.css` `--color-blue-*`（chart-1..5 引用）。
mod tw_blue {
    use super::oklch_to_display_srgba;

    #[inline]
    pub fn chart_1() -> [f32; 4] {
        oklch_to_display_srgba(0.809, 0.105, 251.813, 1.0) // blue-300
    }
    #[inline]
    pub fn chart_2() -> [f32; 4] {
        oklch_to_display_srgba(0.623, 0.214, 259.815, 1.0) // blue-500
    }
    #[inline]
    pub fn chart_3() -> [f32; 4] {
        oklch_to_display_srgba(0.546, 0.245, 262.881, 1.0) // blue-600
    }
    #[inline]
    pub fn chart_4() -> [f32; 4] {
        oklch_to_display_srgba(0.488, 0.243, 264.376, 1.0) // blue-700
    }
    #[inline]
    pub fn chart_5() -> [f32; 4] {
        oklch_to_display_srgba(0.424, 0.199, 265.638, 1.0) // blue-800
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub mode: ThemeMode,
    pub preset: super::ThemePreset,
    pub styles: super::ThemeStyles,
    pub components: std::collections::BTreeMap<String, String>,
    pub colors: DesignTokens,
    pub focus_ring_style: FocusRingStyle,
    /// Logical px used as Hard `spread` or Soft `blur`. Default [`DEFAULT_FOCUS_RING_WIDTH`].
    pub focus_ring_width: f32,
}

impl Theme {
    pub fn light() -> Self {
        // `apps/v4/app/globals.css` `:root`
        let background = oklch_to_display_srgba(1.0, 0.0, 0.0, 1.0);
        let foreground = oklch_to_display_srgba(0.0, 0.0, 0.0, 1.0);
        let surface = oklch_to_display_srgba(0.98, 0.0, 0.0, 1.0);
        Self {
            mode: ThemeMode::Light,
            preset: super::ThemePreset::Shadcn,
            styles: super::ThemeStyles::default(),
            components: Default::default(),
            focus_ring_style: FocusRingStyle::Hard,
            focus_ring_width: DEFAULT_FOCUS_RING_WIDTH,
            colors: DesignTokens {
                background,
                foreground,
                card: background,
                card_foreground: foreground,
                popover: background,
                popover_foreground: foreground,
                primary: foreground,
                primary_foreground: oklch_to_display_srgba(0.985, 0.0, 0.0, 1.0),
                secondary: oklch_to_display_srgba(0.97, 0.0, 0.0, 1.0),
                secondary_foreground: oklch_to_display_srgba(0.205, 0.0, 0.0, 1.0),
                muted: oklch_to_display_srgba(0.97, 0.0, 0.0, 1.0),
                muted_foreground: oklch_to_display_srgba(0.556, 0.0, 0.0, 1.0),
                accent: oklch_to_display_srgba(0.97, 0.0, 0.0, 1.0),
                accent_foreground: oklch_to_display_srgba(0.205, 0.0, 0.0, 1.0),
                destructive: oklch_to_display_srgba(0.577, 0.245, 27.325, 1.0),
                destructive_foreground: oklch_to_display_srgba(0.97, 0.01, 17.0, 1.0),
                info: tw_600(37, 99, 235),
                success: tw_600(22, 163, 74),
                warning: tw_600(234, 88, 12),
                border: oklch_to_display_srgba(0.922, 0.0, 0.0, 1.0),
                input: oklch_to_display_srgba(0.922, 0.0, 0.0, 1.0),
                ring: oklch_to_display_srgba(0.708, 0.0, 0.0, 0.5),
                chart_1: tw_blue::chart_1(),
                chart_2: tw_blue::chart_2(),
                chart_3: tw_blue::chart_3(),
                chart_4: tw_blue::chart_4(),
                chart_5: tw_blue::chart_5(),
                sidebar: oklch_to_display_srgba(0.985, 0.0, 0.0, 1.0),
                sidebar_foreground: foreground,
                sidebar_primary: oklch_to_display_srgba(0.205, 0.0, 0.0, 1.0),
                sidebar_primary_foreground: oklch_to_display_srgba(0.985, 0.0, 0.0, 1.0),
                sidebar_accent: oklch_to_display_srgba(0.97, 0.0, 0.0, 1.0),
                sidebar_accent_foreground: oklch_to_display_srgba(0.205, 0.0, 0.0, 1.0),
                sidebar_border: oklch_to_display_srgba(0.922, 0.0, 0.0, 1.0),
                sidebar_ring: oklch_to_display_srgba(0.708, 0.0, 0.0, 1.0),
                surface,
                surface_foreground: foreground,
                code: surface,
                code_foreground: foreground,
                code_highlight: oklch_to_display_srgba(0.96, 0.0, 0.0, 1.0),
                code_number: oklch_to_display_srgba(0.56, 0.0, 0.0, 1.0),
                caret: foreground,
                selection_background: foreground,
                selection_foreground: background,
                scrollbar_track: oklch_to_display_srgba(0.97, 0.0, 0.0, 1.0),
                scrollbar_thumb: foreground,
                scrollbar_track_border: [0.0, 0.0, 0.0, 0.0],
                scrollbar_thumb_border: [0.0, 0.0, 0.0, 0.0],
                radius: 0.625,
            },
        }
    }

    pub fn dark() -> Self {
        // `apps/v4/app/globals.css` `.dark`
        let background = oklch_to_display_srgba(0.145, 0.0, 0.0, 1.0);
        let foreground = oklch_to_display_srgba(0.985, 0.0, 0.0, 1.0);
        let card = oklch_to_display_srgba(0.205, 0.0, 0.0, 1.0);
        let surface = oklch_to_display_srgba(0.2, 0.0, 0.0, 1.0);
        let surface_fg = oklch_to_display_srgba(0.708, 0.0, 0.0, 1.0);
        Self {
            mode: ThemeMode::Dark,
            preset: super::ThemePreset::Shadcn,
            styles: super::ThemeStyles::default(),
            components: Default::default(),
            focus_ring_style: FocusRingStyle::Hard,
            focus_ring_width: DEFAULT_FOCUS_RING_WIDTH,
            colors: DesignTokens {
                background,
                foreground,
                card,
                card_foreground: foreground,
                popover: card,
                popover_foreground: foreground,
                primary: oklch_to_display_srgba(0.922, 0.0, 0.0, 1.0),
                primary_foreground: oklch_to_display_srgba(0.205, 0.0, 0.0, 1.0),
                secondary: oklch_to_display_srgba(0.269, 0.0, 0.0, 1.0),
                secondary_foreground: foreground,
                muted: oklch_to_display_srgba(0.269, 0.0, 0.0, 1.0),
                muted_foreground: oklch_to_display_srgba(0.708, 0.0, 0.0, 1.0),
                accent: oklch_to_display_srgba(0.371, 0.0, 0.0, 1.0),
                accent_foreground: foreground,
                destructive: oklch_to_display_srgba(0.704, 0.191, 22.216, 1.0),
                destructive_foreground: oklch_to_display_srgba(0.58, 0.22, 27.0, 1.0),
                info: tw_600(37, 99, 235),
                success: tw_600(22, 163, 74),
                warning: tw_600(234, 88, 12),
                border: oklch_to_display_srgba(1.0, 0.0, 0.0, 0.1),
                input: oklch_to_display_srgba(1.0, 0.0, 0.0, 0.15),
                ring: oklch_to_display_srgba(0.556, 0.0, 0.0, 0.5),
                chart_1: tw_blue::chart_1(),
                chart_2: tw_blue::chart_2(),
                chart_3: tw_blue::chart_3(),
                chart_4: tw_blue::chart_4(),
                chart_5: tw_blue::chart_5(),
                sidebar: oklch_to_display_srgba(0.21, 0.0, 0.0, 1.0),
                sidebar_foreground: foreground,
                sidebar_primary: tw_blue::chart_4(),
                sidebar_primary_foreground: foreground,
                sidebar_accent: oklch_to_display_srgba(0.269, 0.0, 0.0, 1.0),
                sidebar_accent_foreground: foreground,
                sidebar_border: oklch_to_display_srgba(0.269, 0.0, 0.0, 1.0),
                sidebar_ring: oklch_to_display_srgba(0.439, 0.0, 0.0, 1.0),
                surface,
                surface_foreground: surface_fg,
                code: surface,
                code_foreground: surface_fg,
                code_highlight: oklch_to_display_srgba(0.27, 0.0, 0.0, 1.0),
                code_number: oklch_to_display_srgba(0.72, 0.0, 0.0, 1.0),
                caret: foreground,
                selection_background: oklch_to_display_srgba(0.922, 0.0, 0.0, 1.0),
                selection_foreground: oklch_to_display_srgba(0.205, 0.0, 0.0, 1.0),
                scrollbar_track: oklch_to_display_srgba(0.269, 0.0, 0.0, 1.0),
                scrollbar_thumb: foreground,
                scrollbar_track_border: [0.0, 0.0, 0.0, 0.0],
                scrollbar_thumb_border: [0.0, 0.0, 0.0, 0.0],
                radius: 0.625,
            },
        }
    }

    pub fn is_dark(&self) -> bool {
        self.mode == ThemeMode::Dark
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::light()
    }
}

thread_local! {
    static CURRENT_THEME: RefCell<Theme> = RefCell::new(Theme::light());
    static THEME_GENERATION: Cell<u64> = const { Cell::new(0) };
}

pub(crate) fn theme_generation() -> u64 {
    THEME_GENERATION.with(Cell::get)
}

/// Borrow the current theme for a short closure (avoids returning a dangling reference).
pub fn with_theme<R>(f: impl FnOnce(&Theme) -> R) -> R {
    CURRENT_THEME.with(|t| f(&t.borrow()))
}

/// Snapshot of the current palette, style tokens and component overrides.
pub fn get_theme() -> Theme {
    CURRENT_THEME.with(|t| t.borrow().clone())
}

pub fn theme_mode() -> ThemeMode {
    CURRENT_THEME.with(|t| t.borrow().mode)
}

pub fn is_dark_theme() -> bool {
    CURRENT_THEME.with(|t| t.borrow().is_dark())
}

pub fn focus_ring_style() -> FocusRingStyle {
    CURRENT_THEME.with(|t| t.borrow().focus_ring_style)
}

/// Theme focus-ring thickness in logical px (Hard spread / Soft blur).
pub fn focus_ring_width() -> f32 {
    CURRENT_THEME.with(|t| t.borrow().focus_ring_width)
}

/// Set focus-ring geometry without replacing color tokens.
pub fn set_focus_ring_style(style: FocusRingStyle) {
    let mut theme = get_theme();
    theme.focus_ring_style = style;
    set_theme(theme);
}

/// Set theme focus-ring thickness (logical px; clamped ≥ 0).
pub fn set_focus_ring_width(width: f32) {
    let mut theme = get_theme();
    theme.focus_ring_width = width.max(0.0);
    set_theme(theme);
}

pub fn set_theme(theme: Theme) {
    let changed = CURRENT_THEME.with(|current| {
        let mut current = current.borrow_mut();
        if *current == theme {
            return false;
        }
        *current = theme;
        true
    });
    if !changed {
        return;
    }
    THEME_GENERATION.with(|generation| generation.set(generation.get().wrapping_add(1)));
    crate::platform::glass_sync_jadix_theme();
    super::stylesheet::watch::mark_style_hot_rebuild();
}

/// Resolve override expressions against scratch tokens without notifying live trees.
pub(crate) fn with_temporary_theme<R>(theme: Theme, f: impl FnOnce() -> R) -> R {
    struct Restore(Theme);
    impl Drop for Restore {
        fn drop(&mut self) {
            CURRENT_THEME.with(|t| *t.borrow_mut() = self.0.clone());
        }
    }
    let _restore = Restore(CURRENT_THEME.with(|t| t.replace(theme)));
    f()
}

/// Switch light/dark tokens while preserving the current [`FocusRingStyle`] /
/// width unless [`ThemeOverrides`] sets them.
pub fn set_theme_mode(mode: ThemeMode) {
    let fallback_ring = focus_ring_style();
    let fallback_width = focus_ring_width();
    let mut next = crate::style::theme_override::compose_theme(mode);
    crate::style::theme_override::apply_stored_focus_ring(&mut next, fallback_ring, fallback_width);
    set_theme(next);
}

pub fn toggle_theme_mode() -> ThemeMode {
    let next = match theme_mode() {
        ThemeMode::Light => ThemeMode::Dark,
        ThemeMode::Dark => ThemeMode::Light,
    };
    set_theme_mode(next);
    next
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_theme_mode_preserves_focus_ring_style() {
        let prev_mode = theme_mode();
        let prev_style = focus_ring_style();
        let prev_width = focus_ring_width();
        set_focus_ring_style(FocusRingStyle::Soft);
        set_focus_ring_width(8.0);
        set_theme_mode(ThemeMode::Dark);
        assert_eq!(focus_ring_style(), FocusRingStyle::Soft);
        assert!((focus_ring_width() - 8.0).abs() < f32::EPSILON);
        set_theme_mode(ThemeMode::Light);
        assert_eq!(focus_ring_style(), FocusRingStyle::Soft);
        assert!((focus_ring_width() - 8.0).abs() < f32::EPSILON);
        set_focus_ring_style(prev_style);
        set_focus_ring_width(prev_width);
        set_theme_mode(prev_mode);
    }

    #[test]
    fn light_tokens_match_globals_css() {
        let c = Theme::light().colors;
        assert_eq!(c.background, c.card);
        assert_eq!(c.background, c.popover);
        assert_eq!(c.foreground, c.primary);
        assert_eq!(c.surface, c.code);
        assert_eq!(c.foreground, c.surface_foreground);
    }
}
