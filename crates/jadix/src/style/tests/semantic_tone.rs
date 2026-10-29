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
    assert_eq!(p.accent, parse_color("success"));
    assert_eq!(
        p.accent,
        parse_color("green-600"),
        "default success aligns with Tailwind green-600"
    );
    assert_eq!(p.fg, parse_text_color("success"));
    assert_eq!(p.border, parse_color("success/50"));
    assert_ne!(
        p.border,
        parse_color("green-300"),
        "lighter chrome is success/50, not a green-300 shade"
    );
    assert_ne!(
        p.border,
        parse_color("success"),
        "border is mixed /50, not solid success"
    );
    assert_eq!(p.shadow, parse_color("success/90"));
    assert_ne!(
        p.shadow,
        parse_color("success"),
        "shadow is success/90 (500-step), not solid 600-step success"
    );
    assert_ne!(
        p.shadow,
        parse_color("green-500"),
        "shadow is not a Tailwind green-500 shade"
    );
}

#[test]
fn button_soft_rest_matches_soft_semantic_fill() {
    let tone = SemanticTone::Warning;
    let (bg, fg, border) = button_soft_semantic_chrome(tone, 0.0);
    assert_eq!(bg, soft_semantic_fill(semantic_palette(tone).accent));
    assert_eq!(fg, parse_text_color("warning"));
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
    assert_eq!(
        c.muted_fg[3], 255,
        "muted description must be opaque so __modal_zoom capture keeps the variant hue"
    );
}

#[test]
fn destructive_shadow_uses_destructive_opacity_not_red_500() {
    let s = semantic_shadow(SemanticTone::Destructive);
    let mixed = parse_color("destructive/90");
    assert!(
        (s.color[0] - mixed[0]).abs() < 1e-5
            && (s.color[1] - mixed[1]).abs() < 1e-5
            && (s.color[2] - mixed[2]).abs() < 1e-5,
        "destructive shadow hue must track destructive/90, got {:?} vs {:?}",
        s.color,
        mixed
    );
    let solid = parse_color("destructive");
    assert!(
        (s.color[0] - solid[0]).abs() > 1e-4
            || (s.color[1] - solid[1]).abs() > 1e-4
            || (s.color[2] - solid[2]).abs() > 1e-4,
        "shadow is /90 (500-step), not solid destructive (600-step)"
    );
    assert!((s.color[3] - SHADOW_ALPHA).abs() < 0.001);
    let red = parse_color("red-500");
    assert!(
        (s.color[0] - red[0]).abs() > 0.01
            || (s.color[1] - red[1]).abs() > 0.01
            || (s.color[2] - red[2]).abs() > 0.01
            || (solid[0] - red[0]).abs() < 1e-5,
        "when destructive ≠ red-500, shadow must not stay on red-500"
    );
}

#[test]
fn destructive_border_uses_opacity_mix_not_solid_or_red_300() {
    let p = semantic_palette(SemanticTone::Destructive);
    assert_eq!(p.border, parse_color("destructive/50"));
    assert_ne!(
        p.border,
        parse_color("destructive"),
        "border is destructive/50 over background, not solid destructive"
    );
    assert_ne!(
        p.border,
        parse_color("red-300"),
        "border is not a Tailwind red-300 shade"
    );
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
    assert_eq!(border.color, parse_color("info/50"));
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
    assert_eq!(c.bg, soft_semantic_fill(parse_color("success")));
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
    assert_eq!(border.color, parse_color("success/50"));
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

#[test]
fn apply_semantic_border_enables_solid_stroke() {
    let mut el = Element::new();
    apply_semantic_border(
        &mut el,
        Some(SemanticBorder {
            color: [0.1, 0.1, 0.1, 1.0],
            width: 1.0,
        }),
    );
    assert!(
        !el.style.border_style.is_invisible(),
        "semantic border must stroke, not leave LineStyle::None"
    );
    assert!(
        el.style.resolved_border_widths().iter().any(|w| *w > 0.0),
        "resolved widths {:?}",
        el.style.resolved_border_widths()
    );
    apply_semantic_border(&mut el, None);
    assert!(el.style.border_style.is_invisible());
    assert!(el.style.resolved_border_widths().iter().all(|w| *w == 0.0));
}
