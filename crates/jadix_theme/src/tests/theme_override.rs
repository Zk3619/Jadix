use super::*;

#[test]
fn toml_patch_parses_kebab_keys() {
    let raw = r#"
            [light]
            primary-foreground = "oklch(0.985 0 0)"
            destructive = "oklch(0.577 0.245 27.325)"
            warn = "oklch(0.75 0.15 70)"
            [dark]
            primary = "oklch(0.922 0 0)"
        "#;
    let patch = parse_theme_overrides_str(raw).expect("toml");
    assert!(patch.light.primary_foreground.is_some());
    assert!(patch.light.destructive.is_some());
    assert!(patch.light.warning.is_some());
    assert!(patch.dark.primary.is_some());
}

#[test]
fn toml_focus_ring_width_and_style() {
    let raw = r#"
            focus_ring_style = "soft"
            focus-ring-width = 8
            [light]
            ring = "oklch(0.5 0.1 250 / 0.5)"
        "#;
    let patch = parse_theme_overrides_str(raw).expect("toml");
    assert_eq!(patch.focus_ring_style.as_deref(), Some("soft"));
    assert_eq!(patch.focus_ring_width, Some(8.0));
    assert!(patch.light.ring.is_some());
}

#[test]
fn toml_theme_transition_defaults_and_overrides() {
    let defaults = parse_theme_overrides_str("").unwrap().transition;
    assert!(defaults.enabled);
    assert_eq!(defaults.duration_ms, 180.0);
    assert_eq!(defaults.easing, "ease-in-out");
    let raw = r#"
            [transition]
            enabled = false
            duration-ms = 320
            easing = "linear"
        "#;
    let parsed = parse_theme_overrides_str(raw)
        .expect("transition config")
        .transition;
    assert!(!parsed.enabled);
    assert_eq!(parsed.duration_ms, 320.0);
    assert_eq!(parsed.easing, "linear");
}

#[test]
fn invalid_transition_keeps_theme_and_settings() {
    let prior_theme = get_theme();
    let prior_overrides = snapshot_theme_overrides();
    for (duration_ms, easing) in [
        (-1.0, "linear"),
        (f32::INFINITY, "ease"),
        (180.0, "invalid-curve"),
    ] {
        let mut bad = ThemeOverrides::default();
        bad.transition.duration_ms = duration_ms;
        bad.transition.easing = easing.into();
        bad.light.background = Some("#123456".into());
        assert!(install_theme_overrides(bad).is_err());
        assert_eq!(get_theme(), prior_theme);
        assert_eq!(snapshot_theme_overrides(), prior_overrides);
    }
}

#[test]
fn install_focus_ring_width_updates_theme() {
    crate::clear_theme_overrides();
    let prev_style = crate::focus_ring_style();
    let prev_width = crate::focus_ring_width();
    let overrides = ThemeOverrides {
        focus_ring_style: Some("soft".into()),
        focus_ring_width: Some(8.0),
        ..Default::default()
    };
    install_theme_overrides(overrides).expect("install");
    assert_eq!(crate::focus_ring_style(), FocusRingStyle::Soft);
    assert!((crate::focus_ring_width() - 8.0).abs() < f32::EPSILON);
    crate::clear_theme_overrides();
    crate::set_focus_ring_style(prev_style);
    crate::set_focus_ring_width(prev_width);
}

#[test]
fn apply_theme_patch_only_changes_destructive() {
    let patch = DesignTokensPatch {
        destructive: Some("oklch(0.62 0.25 30)".into()),
        ..Default::default()
    };
    let theme = apply_theme_patch(&patch, ThemeMode::Light);
    let base = Theme::light();
    assert_ne!(theme.colors.destructive, base.colors.destructive);
    assert_eq!(theme.colors.primary, base.colors.primary);
}

#[test]
fn reject_non_toml_extension() {
    let err = load_theme_overrides("theme.json").unwrap_err();
    assert!(matches!(err, ThemeOverrideError::UnsupportedExtension(_)));
}
