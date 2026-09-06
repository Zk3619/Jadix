//! Application-level theme token overrides (`theme.toml` / `theme.json`).
//!
//! Patches merge onto built-in shadcn v4 defaults per [`ThemeMode`]; [`set_theme_mode`]
//! reapplies the active patch so light/dark toggles keep user branding.

use std::cell::RefCell;
use std::fs;
use std::path::Path;

use serde::Deserialize;

use super::color::try_parse_color;
use super::theme::{
    get_theme, set_theme, theme_mode, DesignTokens, FocusRingStyle, Theme, ThemeMode,
};

thread_local! {
    static USER_OVERRIDES: RefCell<ThemeOverrides> = RefCell::new(ThemeOverrides::default());
}

/// Partial token overrides for one theme mode (`[light]` / `[dark]` in `theme.toml`).
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
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
    pub selection: Option<String>,
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

/// Light/dark tables from `theme.toml` / `theme.json`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct ThemeOverrides {
    pub light: DesignTokensPatch,
    pub dark: DesignTokensPatch,
    pub focus_ring_style: Option<String>,
}

/// Compose built-in defaults + stored user patch for `mode`.
pub fn compose_theme(mode: ThemeMode) -> Theme {
    let overrides = USER_OVERRIDES.with(|o| o.borrow().clone());
    let mut theme = match mode {
        ThemeMode::Light => Theme::light(),
        ThemeMode::Dark => Theme::dark(),
    };
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
    validate_patch(&Theme::light().colors, &overrides.light)?;
    validate_patch(&Theme::dark().colors, &overrides.dark)?;
    if overrides.focus_ring_style.is_some()
        && parse_focus_ring_style(overrides.focus_ring_style.as_deref()).is_none()
    {
        return Err(ThemeOverrideError::UnknownFocusRingStyle {
            value: overrides.focus_ring_style.clone().unwrap_or_default(),
        });
    }
    USER_OVERRIDES.with(|o| *o.borrow_mut() = overrides);
    refresh_active_theme();
    Ok(())
}

/// Load `theme.toml` or `theme.json` and install.
pub fn load_and_install_theme_overrides(path: impl AsRef<Path>) -> Result<(), ThemeOverrideError> {
    let overrides = load_theme_overrides(path)?;
    install_theme_overrides(overrides)
}

pub fn load_theme_overrides(path: impl AsRef<Path>) -> Result<ThemeOverrides, ThemeOverrideError> {
    let path = path.as_ref();
    let raw = fs::read_to_string(path).map_err(ThemeOverrideError::Io)?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "toml" => toml::from_str(&raw).map_err(ThemeOverrideError::Toml),
        "json" => serde_json::from_str(&raw).map_err(ThemeOverrideError::Json),
        _ => Err(ThemeOverrideError::UnsupportedExtension(ext)),
    }
}

/// Clear user overrides and restore built-in tokens for the current mode.
pub fn clear_theme_overrides() {
    USER_OVERRIDES.with(|o| *o.borrow_mut() = ThemeOverrides::default());
    refresh_active_theme();
}

pub(crate) fn refresh_active_theme() {
    let fallback_ring = get_theme().focus_ring_style;
    let mode = theme_mode();
    let mut next = compose_theme(mode);
    apply_stored_focus_ring(&mut next, fallback_ring);
    set_theme(next);
}

pub(crate) fn apply_stored_focus_ring(theme: &mut Theme, fallback: FocusRingStyle) {
    let override_style = USER_OVERRIDES.with(|o| o.borrow().focus_ring_style.clone());
    theme.focus_ring_style = parse_focus_ring_style(override_style.as_deref()).unwrap_or(fallback);
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
    apply_color!(selection, "selection");
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
    set_theme(Theme {
        mode: prior.mode,
        focus_ring_style: prior.focus_ring_style,
        colors: *tokens,
    });
    let resolved = try_parse_color(raw);
    set_theme(prior);
    resolved
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
    Toml(toml::de::Error),
    Json(serde_json::Error),
    UnsupportedExtension(String),
    UnknownColor { field: String, value: String },
    UnknownFocusRingStyle { value: String },
}

impl std::fmt::Display for ThemeOverrideError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "read theme file: {e}"),
            Self::Toml(e) => write!(f, "parse theme TOML: {e}"),
            Self::Json(e) => write!(f, "parse theme JSON: {e}"),
            Self::UnsupportedExtension(ext) => write!(f, "unsupported theme extension: {ext}"),
            Self::UnknownColor { field, value } => {
                write!(f, "unknown color for {field}: {value}")
            }
            Self::UnknownFocusRingStyle { value } => {
                write!(f, "unknown focus_ring_style: {value} (use hard|soft)")
            }
        }
    }
}

impl std::error::Error for ThemeOverrideError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_patch_roundtrip() {
        let raw = r#"{
            "light": { "destructive": "oklch(0.577 0.245 27.325)" },
            "dark": { "primary": "oklch(0.922 0 0)" }
        }"#;
        let patch: ThemeOverrides = serde_json::from_str(raw).expect("json");
        assert!(patch.light.destructive.is_some());
        assert!(patch.dark.primary.is_some());
    }

    #[test]
    fn toml_patch_parses_kebab_keys() {
        let raw = r#"
            [light]
            primary-foreground = "oklch(0.985 0 0)"
        "#;
        let patch: ThemeOverrides = toml::from_str(raw).expect("toml");
        assert!(patch.light.primary_foreground.is_some());
    }
}
