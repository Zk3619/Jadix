//! Color / theme freeze contract.
//!
//! Anchors [`docs/engine/color.md`]: OKLCH token pipeline, semantic tone chrome,
//! theme overrides, sRGB swapchain linearization.

use crate::style::{
    clear_theme_overrides, color_mix_over_background, compose_theme, get_theme,
    install_theme_overrides, oklch_to_display_srgba, parse_color, parse_text_color,
    semantic_chrome, semantic_palette, set_theme_mode, SemanticChromeDecor, SemanticChromeOpts,
    SemanticSurface, SemanticTone, Theme, ThemeMode, ThemeOverrides,
};

#[test]
fn semantic_success_uses_green_not_chart_tokens() {
    let p = semantic_palette(SemanticTone::Success);
    assert_eq!(p.accent, parse_color("green-600"));
    assert_eq!(p.fg, parse_text_color("green-600"));
    assert_ne!(p.accent, get_theme().colors.chart_2);
}

#[test]
fn semantic_warning_uses_orange_not_chart_tokens() {
    let p = semantic_palette(SemanticTone::Warning);
    assert_eq!(p.accent, parse_color("orange-600"));
    assert_ne!(p.accent, get_theme().colors.chart_4);
}

#[test]
fn toast_semantic_chrome_delegates_to_semantic_tone() {
    let chrome = semantic_chrome(SemanticChromeOpts {
        tone: SemanticTone::Success,
        filled: false,
        surface: SemanticSurface::Popover,
        decor: SemanticChromeDecor::ELEVATED,
    });
    assert_eq!(chrome.fg, parse_text_color("green-600"));
    assert_eq!(chrome.border.unwrap().color, parse_color("green-300"));
}

#[test]
fn theme_override_partial_destructive_only() {
    clear_theme_overrides();
    let base = Theme::light().colors.destructive;
    let patch = ThemeOverrides {
        light: crate::style::DesignTokensPatch {
            destructive: Some("oklch(0.62 0.25 30)".into()),
            ..Default::default()
        },
        dark: Default::default(),
        focus_ring_style: None,
    };
    install_theme_overrides(patch).expect("valid oklch patch");
    let themed = compose_theme(ThemeMode::Light);
    assert_ne!(themed.colors.destructive, base);
    assert_eq!(themed.colors.background, Theme::light().colors.background);
    clear_theme_overrides();
}

#[test]
fn set_theme_mode_preserves_user_override() {
    clear_theme_overrides();
    let patch = ThemeOverrides {
        light: crate::style::DesignTokensPatch {
            primary: Some("oklch(0.45 0.2 250)".into()),
            ..Default::default()
        },
        dark: crate::style::DesignTokensPatch {
            primary: Some("oklch(0.85 0.05 250)".into()),
            ..Default::default()
        },
        focus_ring_style: None,
    };
    install_theme_overrides(patch).expect("valid patch");
    set_theme_mode(ThemeMode::Light);
    let light_primary = get_theme().colors.primary;
    set_theme_mode(ThemeMode::Dark);
    let dark_primary = get_theme().colors.primary;
    assert_ne!(light_primary, dark_primary);
    set_theme_mode(ThemeMode::Light);
    assert_eq!(get_theme().colors.primary, light_primary);
    clear_theme_overrides();
}

#[test]
fn light_theme_primary_equals_foreground_per_shadcn_v4() {
    let c = Theme::light().colors;
    assert_eq!(c.primary, c.foreground);
    assert_eq!(c.primary, oklch_to_display_srgba(0.0, 0.0, 0.0, 1.0));
}

#[test]
fn color_mix_over_background_uses_theme_background() {
    clear_theme_overrides();
    let fg = parse_color("primary");
    let mixed = color_mix_over_background(fg, 0.5);
    let bg = get_theme().colors.background;
    assert!(
        (mixed[0] - (fg[0] * 0.5 + bg[0] * 0.5)).abs() < 0.02,
        "50% mix should land between fg and background"
    );
}
