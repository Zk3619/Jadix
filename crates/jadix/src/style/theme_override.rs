//! Application-level theme token overrides (`theme.toml` only).
//!
//! Patches merge onto built-in shadcn v4 defaults per [`ThemeMode`]; [`set_theme_mode`]
//! reapplies the active patch so light/dark toggles keep user branding.

use std::cell::RefCell;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::color::try_parse_color;
use super::theme::{
    get_theme, set_theme, theme_mode, DesignTokens, FocusRingStyle, Theme, ThemeMode,
    ThemeTransitionConfig,
};

thread_local! {
    static USER_OVERRIDES: RefCell<ThemeOverrides> = RefCell::new(ThemeOverrides::default());
}

/// Partial token overrides for one theme mode (`[light]` / `[dark]` in `theme.toml`).
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields, default)]
pub struct DesignTokensPatch {
    pub background: Option<String>,
    pub foreground: Option<String>,
    pub card: Option<String>,
    #[serde(alias = "card-foreground")]
    pub card_foreground: Option<String>,
    pub popover: Option<String>,
    #[serde(alias = "popover-foreground")]
    pub popover_foreground: Option<String>,
    pub primary: Option<String>,
    #[serde(alias = "primary-foreground")]
    pub primary_foreground: Option<String>,
    pub secondary: Option<String>,
    #[serde(alias = "secondary-foreground")]
    pub secondary_foreground: Option<String>,
    pub muted: Option<String>,
    #[serde(alias = "muted-foreground")]
    pub muted_foreground: Option<String>,
    pub accent: Option<String>,
    #[serde(alias = "accent-foreground")]
    pub accent_foreground: Option<String>,
    pub destructive: Option<String>,
    #[serde(alias = "destructive-foreground")]
    pub destructive_foreground: Option<String>,
    pub info: Option<String>,
    pub success: Option<String>,
    #[serde(alias = "warn")]
    pub warning: Option<String>,
    pub border: Option<String>,
    pub input: Option<String>,
    pub ring: Option<String>,
    #[serde(alias = "chart-1")]
    pub chart_1: Option<String>,
    #[serde(alias = "chart-2")]
    pub chart_2: Option<String>,
    #[serde(alias = "chart-3")]
    pub chart_3: Option<String>,
    #[serde(alias = "chart-4")]
    pub chart_4: Option<String>,
    #[serde(alias = "chart-5")]
    pub chart_5: Option<String>,
    pub sidebar: Option<String>,
    #[serde(alias = "sidebar-foreground")]
    pub sidebar_foreground: Option<String>,
    #[serde(alias = "sidebar-primary")]
    pub sidebar_primary: Option<String>,
    #[serde(alias = "sidebar-primary-foreground")]
    pub sidebar_primary_foreground: Option<String>,
    #[serde(alias = "sidebar-accent")]
    pub sidebar_accent: Option<String>,
    #[serde(alias = "sidebar-accent-foreground")]
    pub sidebar_accent_foreground: Option<String>,
    #[serde(alias = "sidebar-border")]
    pub sidebar_border: Option<String>,
    #[serde(alias = "sidebar-ring")]
    pub sidebar_ring: Option<String>,
    pub surface: Option<String>,
    #[serde(alias = "surface-foreground")]
    pub surface_foreground: Option<String>,
    pub code: Option<String>,
    #[serde(alias = "code-foreground")]
    pub code_foreground: Option<String>,
    #[serde(alias = "code-highlight")]
    pub code_highlight: Option<String>,
    #[serde(alias = "code-number")]
    pub code_number: Option<String>,
    #[serde(alias = "caret-color")]
    pub caret: Option<String>,
    #[serde(alias = "selection-background", alias = "selection")]
    pub selection_background: Option<String>,
    #[serde(alias = "selection-foreground")]
    pub selection_foreground: Option<String>,
    #[serde(alias = "scrollbar-track")]
    pub scrollbar_track: Option<String>,
    #[serde(alias = "scrollbar-thumb")]
    pub scrollbar_thumb: Option<String>,
    #[serde(alias = "scrollbar-track-border")]
    pub scrollbar_track_border: Option<String>,
    #[serde(alias = "scrollbar-thumb-border")]
    pub scrollbar_thumb_border: Option<String>,
    pub radius: Option<f32>,
}

/// Light/dark tables from `theme.toml`.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct ThemeOverrides {
    pub preset: super::ThemePreset,
    pub styles: super::ThemeStyles,
    #[serde(rename = "light-styles", alias = "light_styles")]
    pub light_styles: super::ThemeStyles,
    #[serde(rename = "dark-styles", alias = "dark_styles")]
    pub dark_styles: super::ThemeStyles,
    pub components: std::collections::BTreeMap<String, String>,
    pub light: DesignTokensPatch,
    pub dark: DesignTokensPatch,
    pub focus_ring_style: Option<String>,
    /// Logical px; Hard spread / Soft blur. Alias `focus-ring-width`.
    #[serde(alias = "focus-ring-width")]
    pub focus_ring_width: Option<f32>,
    /// Smooth presentation settings; default enabled, 180 ms, ease-in-out.
    pub transition: ThemeTransitionConfig,
}

/// Current theme switch presentation settings.
pub fn theme_transition_config() -> ThemeTransitionConfig {
    USER_OVERRIDES.with(|o| o.borrow().transition.clone())
}

/// Change presentation settings without rebuilding component styles.
/// A zero duration or reduced-motion preference makes switches immediate.
pub fn set_theme_transition_config(mut config: ThemeTransitionConfig) {
    config.duration_ms = if config.duration_ms.is_finite() {
        config.duration_ms.max(0.0)
    } else {
        0.0
    };
    USER_OVERRIDES.with(|o| o.borrow_mut().transition = config);
    crate::app::runtime::request_redraw_all();
}

/// Compose built-in defaults + stored user patch for `mode`.
pub fn compose_theme(mode: ThemeMode) -> Theme {
    let overrides = USER_OVERRIDES.with(|o| o.borrow().clone());
    let mut theme = Theme::preset(overrides.preset, mode);
    theme.styles.merge(&overrides.styles);
    theme.styles.merge(match mode {
        ThemeMode::Light => &overrides.light_styles,
        ThemeMode::Dark => &overrides.dark_styles,
    });
    theme.components = overrides.components.clone();
    let patch = match mode {
        ThemeMode::Light => &overrides.light,
        ThemeMode::Dark => &overrides.dark,
    };
    if let Err(err) = apply_design_tokens_patch(&mut theme.colors, patch) {
        log::warn!("theme override apply failed: {err}");
    }
    theme
}

/// Validate, store overrides, and refresh the active [`Theme`].
pub fn install_theme_overrides(overrides: ThemeOverrides) -> Result<(), ThemeOverrideError> {
    if overrides.preset != super::ThemePreset::Shadcn && !overrides.preset.is_enabled() {
        return Err(ThemeOverrideError::InvalidStyles(format!(
            "theme preset {} is not enabled",
            overrides.preset.name()
        )));
    }
    validate_patch(&Theme::preset(overrides.preset, ThemeMode::Light).colors, &overrides.light)?;
    validate_patch(&Theme::preset(overrides.preset, ThemeMode::Dark).colors, &overrides.dark)?;
    for styles in [&overrides.styles, &overrides.light_styles, &overrides.dark_styles] {
        styles.validate().map_err(ThemeOverrideError::InvalidStyles)?;
    }
    super::theme_style::validate_component_styles(&overrides.components)
        .map_err(ThemeOverrideError::InvalidStyles)?;
    if overrides.focus_ring_style.is_some()
        && parse_focus_ring_style(overrides.focus_ring_style.as_deref()).is_none()
    {
        return Err(ThemeOverrideError::UnknownFocusRingStyle {
            value: overrides.focus_ring_style.clone().unwrap_or_default(),
        });
    }
    if !overrides.transition.duration_ms.is_finite() || overrides.transition.duration_ms < 0.0 {
        return Err(ThemeOverrideError::InvalidTransitionDuration(
            overrides.transition.duration_ms,
        ));
    }
    let easing = overrides.transition.easing.trim().to_ascii_lowercase();
    let timing = overrides.transition.timing();
    if (timing == super::TimingFunction::Ease && easing != "ease")
        || matches!(timing, super::TimingFunction::CubicBezier(x1, y1, x2, y2)
            if ![x1, y1, x2, y2].iter().all(|n| n.is_finite()) || !(0.0..=1.0).contains(&x1) || !(0.0..=1.0).contains(&x2))
    {
        return Err(ThemeOverrideError::InvalidTransitionEasing(
            overrides.transition.easing.clone(),
        ));
    }
    USER_OVERRIDES.with(|o| *o.borrow_mut() = overrides);
    refresh_active_theme();
    crate::app::runtime::request_redraw_all();
    Ok(())
}

/// Load `theme.toml` and install.
pub fn load_and_install_theme_overrides(path: impl AsRef<Path>) -> Result<(), ThemeOverrideError> {
    let overrides = load_theme_overrides(path)?;
    install_theme_overrides(overrides)
}

pub fn load_theme_overrides(path: impl AsRef<Path>) -> Result<ThemeOverrides, ThemeOverrideError> {
    let path = path.as_ref();
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext != "toml" {
        return Err(ThemeOverrideError::UnsupportedExtension(ext.into()));
    }
    let raw = fs::read_to_string(path).map_err(ThemeOverrideError::Io)?;
    parse_theme_overrides_str(&raw)
}

/// Parse a `theme.toml` body.
pub fn parse_theme_overrides_str(raw: &str) -> Result<ThemeOverrides, ThemeOverrideError> {
    toml::from_str(raw).map_err(|e| ThemeOverrideError::Toml(e.to_string()))
}

/// Merge a single-mode patch onto built-in tokens for `mode` (does not store USER_OVERRIDES).
pub fn apply_theme_patch(patch: &DesignTokensPatch, mode: ThemeMode) -> Theme {
    let mut theme = match mode {
        ThemeMode::Light => Theme::light(),
        ThemeMode::Dark => Theme::dark(),
    };
    if let Err(err) = apply_design_tokens_patch(&mut theme.colors, patch) {
        log::warn!("apply_theme_patch failed: {err}");
    }
    theme
}

/// Clear user overrides and restore built-in tokens for the current mode.
pub fn clear_theme_overrides() {
    USER_OVERRIDES.with(|o| *o.borrow_mut() = ThemeOverrides::default());
    refresh_active_theme();
}

pub(crate) fn snapshot_theme_overrides() -> ThemeOverrides {
    USER_OVERRIDES.with(|o| o.borrow().clone())
}

pub fn theme_overrides() -> ThemeOverrides {
    snapshot_theme_overrides()
}

/// Restore a previous [`snapshot_theme_overrides`] and recompose the active theme.
pub(crate) fn restore_theme_overrides(overrides: ThemeOverrides) {
    USER_OVERRIDES.with(|o| *o.borrow_mut() = overrides);
    refresh_active_theme();
}

pub(crate) fn refresh_active_theme() {
    let t = get_theme();
    let fallback_ring = t.focus_ring_style;
    let fallback_width = t.focus_ring_width;
    let mode = theme_mode();
    let mut next = compose_theme(mode);
    if next.preset == t.preset {
        apply_stored_focus_ring(&mut next, fallback_ring, fallback_width);
    } else {
        let ring = next.focus_ring_style;
        let width = next.focus_ring_width;
        apply_stored_focus_ring(&mut next, ring, width);
    }
    set_theme(next);
}

pub(crate) fn change_theme_preset(preset: super::ThemePreset) {
    USER_OVERRIDES.with(|overrides| overrides.borrow_mut().preset = preset);
    let mut next = compose_theme(theme_mode());
    let ring = next.focus_ring_style;
    let width = next.focus_ring_width;
    apply_stored_focus_ring(&mut next, ring, width);
    set_theme(next);
}

pub(crate) fn apply_stored_focus_ring(
    theme: &mut Theme,
    fallback_style: FocusRingStyle,
    fallback_width: f32,
) {
    let (override_style, override_width) = USER_OVERRIDES.with(|o| {
        let o = o.borrow();
        (o.focus_ring_style.clone(), o.focus_ring_width)
    });
    theme.focus_ring_style =
        parse_focus_ring_style(override_style.as_deref()).unwrap_or(fallback_style);
    theme.focus_ring_width = override_width
        .map(|w| w.max(0.0))
        .unwrap_or(fallback_width.max(0.0));
}

fn validate_patch(
    base: &DesignTokens,
    patch: &DesignTokensPatch,
) -> Result<(), ThemeOverrideError> {
    let mut scratch = *base;
    apply_design_tokens_patch(&mut scratch, patch)?;
    Ok(())
}

fn apply_design_tokens_patch(
    tokens: &mut DesignTokens,
    patch: &DesignTokensPatch,
) -> Result<(), ThemeOverrideError> {
    macro_rules! apply_color {
        ($field:ident, $label:literal) => {
            if let Some(raw) = &patch.$field {
                tokens.$field = resolve_patch_color(raw, tokens).ok_or_else(|| {
                    ThemeOverrideError::UnknownColor {
                        field: $label.into(),
                        value: raw.clone(),
                    }
                })?;
            }
        };
    }
    apply_color!(background, "background");
    apply_color!(foreground, "foreground");
    apply_color!(card, "card");
    apply_color!(card_foreground, "card-foreground");
    apply_color!(popover, "popover");
    apply_color!(popover_foreground, "popover-foreground");
    apply_color!(primary, "primary");
    apply_color!(primary_foreground, "primary-foreground");
    apply_color!(secondary, "secondary");
    apply_color!(secondary_foreground, "secondary-foreground");
    apply_color!(muted, "muted");
    apply_color!(muted_foreground, "muted-foreground");
    apply_color!(accent, "accent");
    apply_color!(accent_foreground, "accent-foreground");
    apply_color!(destructive, "destructive");
    apply_color!(destructive_foreground, "destructive-foreground");
    apply_color!(info, "info");
    apply_color!(success, "success");
    apply_color!(warning, "warning");
    apply_color!(border, "border");
    apply_color!(input, "input");
    apply_color!(ring, "ring");
    apply_color!(chart_1, "chart-1");
    apply_color!(chart_2, "chart-2");
    apply_color!(chart_3, "chart-3");
    apply_color!(chart_4, "chart-4");
    apply_color!(chart_5, "chart-5");
    apply_color!(sidebar, "sidebar");
    apply_color!(sidebar_foreground, "sidebar-foreground");
    apply_color!(sidebar_primary, "sidebar-primary");
    apply_color!(sidebar_primary_foreground, "sidebar-primary-foreground");
    apply_color!(sidebar_accent, "sidebar-accent");
    apply_color!(sidebar_accent_foreground, "sidebar-accent-foreground");
    apply_color!(sidebar_border, "sidebar-border");
    apply_color!(sidebar_ring, "sidebar-ring");
    apply_color!(surface, "surface");
    apply_color!(surface_foreground, "surface-foreground");
    apply_color!(code, "code");
    apply_color!(code_foreground, "code-foreground");
    apply_color!(code_highlight, "code-highlight");
    apply_color!(code_number, "code-number");
    apply_color!(caret, "caret");
    apply_color!(selection_background, "selection-background");
    apply_color!(selection_foreground, "selection-foreground");
    apply_color!(scrollbar_track, "scrollbar-track");
    apply_color!(scrollbar_thumb, "scrollbar-thumb");
    apply_color!(scrollbar_track_border, "scrollbar-track-border");
    apply_color!(scrollbar_thumb_border, "scrollbar-thumb-border");
    if let Some(r) = patch.radius {
        tokens.radius = r;
    }
    Ok(())
}

fn resolve_patch_color(raw: &str, tokens: &DesignTokens) -> Option<[f32; 4]> {
    let prior = get_theme();
    super::theme::with_temporary_theme(
        Theme {
            colors: *tokens,
            ..prior
        },
        || try_parse_color(raw),
    )
}

fn parse_focus_ring_style(raw: Option<&str>) -> Option<FocusRingStyle> {
    match raw.map(|s| s.trim().to_ascii_lowercase()) {
        None => None,
        Some(s) if s.is_empty() => None,
        Some(s) if s == "hard" => Some(FocusRingStyle::Hard),
        Some(s) if s == "soft" => Some(FocusRingStyle::Soft),
        Some(_) => None,
    }
}

/// Theme override load/apply errors.
#[derive(Debug)]
pub enum ThemeOverrideError {
    Io(std::io::Error),
    Toml(String),
    UnsupportedExtension(String),
    UnknownColor { field: String, value: String },
    UnknownFocusRingStyle { value: String },
    InvalidTransitionDuration(f32),
    InvalidTransitionEasing(String),
    InvalidStyles(String),
}

impl std::fmt::Display for ThemeOverrideError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidStyles(value) => write!(f, "invalid theme styles: {value}"),
            Self::Io(e) => write!(f, "read theme file: {e}"),
            Self::Toml(e) => write!(f, "parse theme TOML: {e}"),
            Self::UnsupportedExtension(ext) => {
                write!(f, "unsupported theme extension: {ext} (use .toml)")
            }
            Self::UnknownColor { field, value } => {
                write!(f, "unknown color for {field}: {value}")
            }
            Self::UnknownFocusRingStyle { value } => {
                write!(f, "unknown focus_ring_style: {value} (use hard|soft)")
            }
            Self::InvalidTransitionEasing(value) => {
                write!(f, "invalid theme transition easing: {value}")
            }
            Self::InvalidTransitionDuration(value) => {
                write!(
                    f,
                    "invalid theme transition duration: {value} (use a non-negative number)"
                )
            }
        }
    }
}

impl std::error::Error for ThemeOverrideError {}

#[cfg(test)]
mod tests {
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
        crate::style::clear_theme_overrides();
        let prev_style = crate::style::focus_ring_style();
        let prev_width = crate::style::focus_ring_width();
        let overrides = ThemeOverrides {
            focus_ring_style: Some("soft".into()),
            focus_ring_width: Some(8.0),
            ..Default::default()
        };
        install_theme_overrides(overrides).expect("install");
        assert_eq!(crate::style::focus_ring_style(), FocusRingStyle::Soft);
        assert!((crate::style::focus_ring_width() - 8.0).abs() < f32::EPSILON);
        crate::style::clear_theme_overrides();
        crate::style::set_focus_ring_style(prev_style);
        crate::style::set_focus_ring_width(prev_width);
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
}
