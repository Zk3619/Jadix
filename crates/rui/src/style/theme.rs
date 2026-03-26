//! 设计令牌与主题系统（ShadCN 风格）。
use std::sync::OnceLock;

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
    pub ring: [f32; 4],
    pub radius: f32,
}

pub struct Theme {
    pub colors: DesignTokens,
}

impl Default for Theme {
    fn default() -> Self {
        // Default Light Theme (ShadCN style)
        Self {
            colors: DesignTokens {
                primary: [0.09, 0.09, 0.09, 1.0],
                primary_foreground: [0.98, 0.98, 0.98, 1.0],
                secondary: [0.96, 0.96, 0.96, 1.0],
                secondary_foreground: [0.09, 0.09, 0.09, 1.0],
                accent: [0.96, 0.96, 0.96, 1.0],
                accent_foreground: [0.09, 0.09, 0.09, 1.0],
                muted: [0.96, 0.96, 0.96, 1.0],
                muted_foreground: [0.45, 0.45, 0.45, 1.0],
                destructive: [0.94, 0.27, 0.27, 1.0],
                destructive_foreground: [0.98, 0.98, 0.98, 1.0], // Fixed from analysis
                background: [1.0, 1.0, 1.0, 1.0],
                foreground: [0.09, 0.09, 0.09, 1.0],
                border: [0.90, 0.90, 0.90, 1.0],
                input: [0.90, 0.90, 0.90, 1.0],
                ring: [0.09, 0.09, 0.09, 0.5],
                radius: 0.5, // rem
            },
        }
    }
}

static CURRENT_THEME: OnceLock<Theme> = OnceLock::new();

pub fn get_theme() -> &'static Theme {
    CURRENT_THEME.get_or_init(Theme::default)
}
