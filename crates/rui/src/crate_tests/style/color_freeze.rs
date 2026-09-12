//! Color / theme freeze contract.
//!
//! Anchors [`docs/engine/color.md`]: OKLCH token pipeline, semantic tone chrome,
//! theme overrides, parse_color syntax, mix/lerp selection, sRGB swapchain linearization.

use crate::style::{
    clear_theme_overrides, color_mix_over_background, compose_theme, get_theme,
    install_theme_overrides, lerp_display_srgb_linear, lerp_premultiplied_display_srgba,
    oklch_to_display_srgba, parse_color, parse_text_color, semantic_chrome, semantic_palette,
    semantic_shadow, set_theme_mode, soft_semantic_fill, soft_semantic_fill_hover,
    soft_semantic_mix_alphas, try_parse_color, SemanticChromeDecor, SemanticChromeOpts,
    SemanticSurface, SemanticTone, Theme, ThemeMode, ThemeOverrides,
};

#[test]
fn semantic_success_uses_green_not_chart_tokens() {
    let p = semantic_palette(SemanticTone::Success);
    assert_eq!(p.accent, parse_color("success"));
    assert_eq!(p.accent, parse_color("green-600"));
    assert_eq!(p.fg, parse_text_color("success"));
    assert_ne!(p.accent, get_theme().colors.chart_2);
}

#[test]
fn semantic_warning_uses_orange_not_chart_tokens() {
    let p = semantic_palette(SemanticTone::Warning);
    assert_eq!(p.accent, parse_color("warning"));
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
    assert_eq!(chrome.fg, parse_text_color("success"));
    assert_eq!(chrome.border.unwrap().color, parse_color("success/50"));
}

#[test]
fn soft_semantic_mix_alphas_light_rest_and_hover() {
    clear_theme_overrides();
    set_theme_mode(ThemeMode::Light);
    let mix = soft_semantic_mix_alphas();
    assert!((mix.rest - 0.1).abs() < 1e-4);
    assert!((mix.hover - 0.2).abs() < 1e-4);
    let accent = parse_color("green-600");
    assert_eq!(
        soft_semantic_fill(accent),
        color_mix_over_background(accent, mix.rest)
    );
    assert_eq!(
        soft_semantic_fill_hover(accent),
        color_mix_over_background(accent, mix.hover)
    );
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
fn destructive_shadow_follows_theme_toml_destructive_token() {
    clear_theme_overrides();
    set_theme_mode(ThemeMode::Light);
    let base_shadow = semantic_shadow(SemanticTone::Destructive);
    let base_mixed = parse_color("destructive/90");
    assert!(
        (base_shadow.color[0] - base_mixed[0]).abs() < 1e-5
            && (base_shadow.color[1] - base_mixed[1]).abs() < 1e-5
            && (base_shadow.color[2] - base_mixed[2]).abs() < 1e-5,
        "shadow hue must start on destructive/90 (500-step), not solid 600-step"
    );

    let patch = ThemeOverrides {
        light: crate::style::DesignTokensPatch {
            // Gallery theme.toml uses hue 300 (purple) so red-500 shadows are obvious.
            destructive: Some("oklch(0.55 0.22 300)".into()),
            ..Default::default()
        },
        dark: Default::default(),
        focus_ring_style: None,
    };
    install_theme_overrides(patch).expect("valid oklch patch");
    let themed_shadow = semantic_shadow(SemanticTone::Destructive);
    let themed_accent = parse_color("destructive");
    let themed_mixed = parse_color("destructive/90");
    assert_ne!(
        [
            base_shadow.color[0],
            base_shadow.color[1],
            base_shadow.color[2]
        ],
        [
            themed_shadow.color[0],
            themed_shadow.color[1],
            themed_shadow.color[2]
        ],
        "theme.toml destructive must retint the elevated shadow"
    );
    assert!(
        (themed_shadow.color[0] - themed_mixed[0]).abs() < 1e-5
            && (themed_shadow.color[1] - themed_mixed[1]).abs() < 1e-5
            && (themed_shadow.color[2] - themed_mixed[2]).abs() < 1e-5,
        "themed shadow {:?} must match destructive/90 {:?}",
        themed_shadow.color,
        themed_mixed
    );
    assert!(
        (themed_shadow.color[0] - themed_accent[0]).abs() > 1e-4
            || (themed_shadow.color[1] - themed_accent[1]).abs() > 1e-4
            || (themed_shadow.color[2] - themed_accent[2]).abs() > 1e-4,
        "shadow is /90, not solid destructive"
    );
    let red = parse_color("red-500");
    assert!(
        (themed_shadow.color[0] - red[0]).abs() > 0.05
            || (themed_shadow.color[1] - red[1]).abs() > 0.05
            || (themed_shadow.color[2] - red[2]).abs() > 0.05,
        "purple destructive must not keep a red-500 shadow"
    );
    let themed_border = semantic_palette(SemanticTone::Destructive).border;
    let mixed = parse_color("destructive/50");
    assert!(
        (themed_border[0] - mixed[0]).abs() < 1e-5
            && (themed_border[1] - mixed[1]).abs() < 1e-5
            && (themed_border[2] - mixed[2]).abs() < 1e-5,
        "themed border {:?} must match destructive/50 {:?}",
        themed_border,
        mixed
    );
    assert!(
        (themed_border[0] - themed_accent[0]).abs() > 0.02
            || (themed_border[1] - themed_accent[1]).abs() > 0.02
            || (themed_border[2] - themed_accent[2]).abs() > 0.02,
        "border is mixed /50, not solid destructive"
    );
    let red_border = parse_color("red-300");
    assert!(
        (themed_border[0] - red_border[0]).abs() > 0.05
            || (themed_border[1] - red_border[1]).abs() > 0.05
            || (themed_border[2] - red_border[2]).abs() > 0.05,
        "purple destructive must not keep a red-300 border"
    );
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

#[test]
fn semantic_success_follows_theme_token_not_green_shade() {
    clear_theme_overrides();
    set_theme_mode(ThemeMode::Light);
    let green = parse_color("green-600");
    assert_eq!(parse_color("success"), green);
    let patch = ThemeOverrides {
        light: crate::style::DesignTokensPatch {
            success: Some("oklch(0.55 0.22 300)".into()),
            ..Default::default()
        },
        dark: Default::default(),
        focus_ring_style: None,
    };
    install_theme_overrides(patch).expect("valid oklch patch");
    let themed = parse_color("success");
    assert_ne!(themed, green);
    assert_eq!(
        parse_color("green-600"),
        green,
        "Tailwind green-600 is a shade table, not a theme token"
    );
    let p = semantic_palette(SemanticTone::Success);
    assert_eq!(p.accent, themed);
    assert_eq!(p.border, parse_color("success/50"));
    assert_ne!(p.border, parse_color("green-300"));
    assert_eq!(p.shadow, parse_color("success/90"));
    assert_ne!(p.shadow, themed);
    clear_theme_overrides();
}

#[test]
fn parse_token_opacity_suffix_mixes_over_background() {
    clear_theme_overrides();
    let destructive = parse_color("destructive");
    let d10 = try_parse_color("destructive/10").expect("destructive/10");
    let bg = get_theme().colors.background;
    let want = [
        destructive[0] * 0.1 + bg[0] * 0.9,
        destructive[1] * 0.1 + bg[1] * 0.9,
        destructive[2] * 0.1 + bg[2] * 0.9,
        1.0,
    ];
    assert!(
        (0..4).all(|i| (d10[i] - want[i]).abs() < 1e-3),
        "destructive/10 should mix over background"
    );
}

#[test]
fn parse_color_mix_oklab_differs_from_display_srgb_channel_lerp() {
    let oklab = try_parse_color("color-mix(in oklab, red, blue)").expect("oklab mix");
    let srgb = try_parse_color("color-mix(in srgb, red, blue)").expect("srgb mix");
    assert!(
        (oklab[0] - srgb[0]).abs() > 0.05 || (oklab[2] - srgb[2]).abs() > 0.05,
        "oklab vs srgb channel mix should diverge: oklab={oklab:?} srgb={srgb:?}"
    );
}

#[test]
fn parse_color_mix_srgb_linear_matches_lerp_helper() {
    let parsed =
        try_parse_color("color-mix(in srgb-linear, #ff0000, #0000ff)").expect("srgb-linear mix");
    let red = try_parse_color("#ff0000").expect("red");
    let blue = try_parse_color("#0000ff").expect("blue");
    let helper = lerp_display_srgb_linear(red, blue, 0.5);
    assert!(
        (parsed[0] - helper[0]).abs() < 1e-3 && (parsed[2] - helper[2]).abs() < 1e-3,
        "srgb-linear color-mix should match lerp_display_srgb_linear: parsed={parsed:?} helper={helper:?}"
    );
}

#[test]
fn parse_css_oklch_and_relative_rgb() {
    let oklch = try_parse_color("oklch(0.63 0.2 25)").expect("oklch");
    assert!(oklch[0] > 0.4);

    let rel = try_parse_color("rgb(from #ff0000 r g b / 50%)").expect("relative rgb");
    assert!((rel[0] - 1.0).abs() < 1e-3);
    assert!((rel[3] - 0.5).abs() < 1e-3);
}

#[test]
fn lerp_premultiplied_black_transparent_rest_stains_midpoint() {
    let rest = [0.0, 0.0, 0.0, 0.0];
    let hover = [1.0, 0.0, 0.0, 1.0];
    let mid = lerp_premultiplied_display_srgba(rest, hover, 0.5);
    assert!(
        mid[1] < 0.05 && mid[2] < 0.05,
        "premulti from [0,0,0,0] rest darkens mid-transition: {mid:?}"
    );
}

#[test]
fn ghost_rest_preserves_accent_rgb_at_zero_alpha() {
    let hover = parse_color("accent");
    let rest = [hover[0], hover[1], hover[2], 0.0];
    assert_eq!(rest[0..3], hover[0..3]);
    assert_eq!(rest[3], 0.0);
}

#[test]
fn toml_theme_overrides_install_destructive() {
    clear_theme_overrides();
    let patch =
        crate::style::parse_theme_overrides_str("[light]\ndestructive = \"oklch(0.62 0.25 30)\"\n")
            .expect("toml");
    let base = Theme::light().colors.destructive;
    install_theme_overrides(patch).expect("install");
    assert_ne!(get_theme().colors.destructive, base);
    clear_theme_overrides();
}

#[test]
fn elevation_xl_matches_dialog_legacy_alpha() {
    use crate::tokens::{shadow_rgba, Elevation};
    assert!((shadow_rgba(Elevation::Xl)[3] - 0.16).abs() < 1e-6);
}
