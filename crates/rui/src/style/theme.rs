//! 设计令牌与主题系统（ShadCN v4 风格 OKLCH → display sRGB）。
use std::sync::OnceLock;

use super::color_space::oklch_to_display_srgba;

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
    /// 焦点环色；含 alpha 以贴近 `ring-ring/50` 观感。
    pub ring: [f32; 4],
    pub radius: f32,
}

pub struct Theme {
    pub colors: DesignTokens,
}

impl Default for Theme {
    fn default() -> Self {
        // 与 shadcn-ui `apps/v4/app/globals.css` 中 `:root` 对齐（浅色）。
        Self {
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
                ring: oklch_to_display_srgba(0.708, 0.0, 0.0, 0.5),
                radius: 0.625,
            },
        }
    }
}

static CURRENT_THEME: OnceLock<Theme> = OnceLock::new();

pub fn get_theme() -> &'static Theme {
    CURRENT_THEME.get_or_init(Theme::default)
}
