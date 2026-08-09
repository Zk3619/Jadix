//! 设计令牌与主题系统（ShadCN v4 风格 OKLCH → display sRGB）。
//!
//! 支持运行时浅色 / 深色切换；[`get_theme`] 读取当前主题，[`set_theme_mode`] 切换。
//! 焦点环几何由 [`FocusRingStyle`] 配置（Hard / Soft），与配色 token 分离。

use std::cell::RefCell;

use super::color_space::oklch_to_display_srgba;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ThemeMode {
    #[default]
    Light,
    Dark,
}

/// Theme-level focus-ring geometry (ShadCN v4 Hard vs RUI Soft glow).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FocusRingStyle {
    /// ShadCN v4：硬边 `spread=3`, `blur=0`.
    #[default]
    Hard,
    /// 柔光：`blur=3`, `spread=0`.
    Soft,
}

#[derive(Clone, Copy, Debug)]
pub struct DesignTokens {
    pub primary: [f32; 4],
    pub primary_foreground: [f32; 4],
    pub secondary: [f32; 4],
    pub secondary_foreground: [f32; 4],
    pub accent: [f32; 4],
    pub accent_foreground: [f32; 4],
    pub muted: [f32; 4],
    pub muted_foreground: [f32; 4],
    pub destructive: [f32; 4],
    pub destructive_foreground: [f32; 4],
    pub background: [f32; 4],
    pub foreground: [f32; 4],
    pub border: [f32; 4],
    pub input: [f32; 4],
    /// `ring-ring/50` 色（几何由 [`FocusRingStyle`] 决定）。
    pub ring: [f32; 4],
    pub radius: f32,
}

#[derive(Clone, Debug)]
pub struct Theme {
    pub mode: ThemeMode,
    pub colors: DesignTokens,
    pub focus_ring_style: FocusRingStyle,
}

impl Theme {
    pub fn light() -> Self {
        // 与 shadcn-ui `apps/v4/app/globals.css` 中 `:root` 对齐（浅色）。
        Self {
            mode: ThemeMode::Light,
            focus_ring_style: FocusRingStyle::Hard,
            colors: DesignTokens {
                background: oklch_to_display_srgba(1.0, 0.0, 0.0, 1.0),
                foreground: oklch_to_display_srgba(0.145, 0.0, 0.0, 1.0),
                primary: oklch_to_display_srgba(0.205, 0.0, 0.0, 1.0),
                primary_foreground: oklch_to_display_srgba(0.985, 0.0, 0.0, 1.0),
                secondary: oklch_to_display_srgba(0.97, 0.0, 0.0, 1.0),
                secondary_foreground: oklch_to_display_srgba(0.205, 0.0, 0.0, 1.0),
                muted: oklch_to_display_srgba(0.97, 0.0, 0.0, 1.0),
                muted_foreground: oklch_to_display_srgba(0.556, 0.0, 0.0, 1.0),
                accent: oklch_to_display_srgba(0.97, 0.0, 0.0, 1.0),
                accent_foreground: oklch_to_display_srgba(0.205, 0.0, 0.0, 1.0),
                destructive: oklch_to_display_srgba(0.577, 0.245, 27.325, 1.0),
                destructive_foreground: oklch_to_display_srgba(0.97, 0.01, 17.0, 1.0),
                border: oklch_to_display_srgba(0.922, 0.0, 0.0, 1.0),
                input: oklch_to_display_srgba(0.922, 0.0, 0.0, 1.0),
                // ring-ring/50; Hard/Soft geometry is separate.
                ring: oklch_to_display_srgba(0.708, 0.0, 0.0, 0.5),
                radius: 0.625,
            },
        }
    }

    pub fn dark() -> Self {
        // 与 shadcn-ui v4 `.dark` 对齐。
        Self {
            mode: ThemeMode::Dark,
            focus_ring_style: FocusRingStyle::Hard,
            colors: DesignTokens {
                background: oklch_to_display_srgba(0.145, 0.0, 0.0, 1.0),
                foreground: oklch_to_display_srgba(0.985, 0.0, 0.0, 1.0),
                primary: oklch_to_display_srgba(0.922, 0.0, 0.0, 1.0),
                primary_foreground: oklch_to_display_srgba(0.205, 0.0, 0.0, 1.0),
                secondary: oklch_to_display_srgba(0.269, 0.0, 0.0, 1.0),
                secondary_foreground: oklch_to_display_srgba(0.985, 0.0, 0.0, 1.0),
                muted: oklch_to_display_srgba(0.269, 0.0, 0.0, 1.0),
                muted_foreground: oklch_to_display_srgba(0.708, 0.0, 0.0, 1.0),
                accent: oklch_to_display_srgba(0.269, 0.0, 0.0, 1.0),
                accent_foreground: oklch_to_display_srgba(0.985, 0.0, 0.0, 1.0),
                destructive: oklch_to_display_srgba(0.704, 0.191, 22.216, 1.0),
                destructive_foreground: oklch_to_display_srgba(0.985, 0.0, 0.0, 1.0),
                border: oklch_to_display_srgba(1.0, 0.0, 0.0, 0.1),
                input: oklch_to_display_srgba(1.0, 0.0, 0.0, 0.15),
                ring: oklch_to_display_srgba(0.556, 0.0, 0.0, 0.5),
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
}

/// Borrow the current theme for a short closure (avoids returning a dangling reference).
pub fn with_theme<R>(f: impl FnOnce(&Theme) -> R) -> R {
    CURRENT_THEME.with(|t| f(&t.borrow()))
}

/// Snapshot of the current theme (cheap: tokens are Copy-ish via Clone).
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

/// Set focus-ring geometry without replacing color tokens.
pub fn set_focus_ring_style(style: FocusRingStyle) {
    CURRENT_THEME.with(|t| t.borrow_mut().focus_ring_style = style);
}

pub fn set_theme(theme: Theme) {
    CURRENT_THEME.with(|t| *t.borrow_mut() = theme);
}

/// Switch light/dark tokens while preserving the current [`FocusRingStyle`].
pub fn set_theme_mode(mode: ThemeMode) {
    let style = focus_ring_style();
    let mut next = match mode {
        ThemeMode::Light => Theme::light(),
        ThemeMode::Dark => Theme::dark(),
    };
    next.focus_ring_style = style;
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
        set_focus_ring_style(FocusRingStyle::Soft);
        set_theme_mode(ThemeMode::Dark);
        assert_eq!(focus_ring_style(), FocusRingStyle::Soft);
        set_theme_mode(ThemeMode::Light);
        assert_eq!(focus_ring_style(), FocusRingStyle::Soft);
        set_focus_ring_style(prev_style);
        set_theme_mode(prev_mode);
    }
}
