use super::*;

#[cfg(feature = "theme-fluent")]
#[test]
fn system_preference_follows_changes_and_manual_modes_override_them() {
    let previous_theme = get_theme();
    let previous_preference = theme_preference();
    let previous_system = SYSTEM_THEME.with(Cell::get);
    let previous_overrides = crate::snapshot_theme_overrides();
    crate::set_theme_preset(crate::ThemePreset::Fluent);
    let mut overrides = crate::theme_overrides();
    overrides.styles.control_height = Some(39.0);
    crate::install_theme_overrides(overrides).unwrap();
    set_focus_ring_style(FocusRingStyle::Soft);
    set_focus_ring_width(7.0);
    set_theme_preference(ThemePreference::System);
    for mode in [ThemeMode::Dark, ThemeMode::Light, ThemeMode::Dark] {
        update_system_theme(mode);
        assert_eq!(theme_preference(), ThemePreference::System);
        assert_eq!(theme_mode(), mode);
        assert_eq!(get_theme().preset, crate::ThemePreset::Fluent);
        assert_eq!(get_theme().styles.control_height, Some(39.0));
        assert_eq!(focus_ring_style(), FocusRingStyle::Soft);
        assert_eq!(focus_ring_width(), 7.0);
    }
    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        set_theme_mode(mode);
        update_system_theme(if mode == ThemeMode::Light {
            ThemeMode::Dark
        } else {
            ThemeMode::Light
        });
        assert_eq!(theme_mode(), mode);
        assert_ne!(theme_preference(), ThemePreference::System);
    }
    set_theme_preference(ThemePreference::System);
    assert!(take_system_theme_refresh());
    assert!(!take_system_theme_refresh());
    assert_eq!(theme_mode(), ThemeMode::Light);
    let generation = theme_generation();
    update_system_theme(ThemeMode::Light);
    assert_eq!(theme_generation(), generation);
    crate::restore_theme_overrides(previous_overrides);
    SYSTEM_THEME.with(|current| current.set(previous_system));
    set_theme_preference(previous_preference);
    set_theme(previous_theme);
}

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
