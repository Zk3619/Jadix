use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{Theme, ThemeMode};

#[cfg(feature = "theme-fluent")]
#[path = "themes/fluent.rs"]
mod fluent;
#[cfg(feature = "theme-liquid-glass")]
#[path = "themes/liquid_glass.rs"]
mod liquid_glass;
#[cfg(feature = "theme-material3")]
#[path = "themes/material3.rs"]
mod material3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub enum ThemePreset {
    #[serde(rename = "shadcn")]
    Shadcn,
    #[serde(
        rename = "fluent",
        alias = "ms-fluent",
        alias = "winui3",
        alias = "winui-3"
    )]
    #[cfg_attr(not(feature = "theme-fluent"), serde(skip_deserializing))]
    Fluent,
    #[serde(rename = "material3", alias = "md3", alias = "google-md3")]
    #[cfg_attr(not(feature = "theme-material3"), serde(skip_deserializing))]
    Material3,
    #[serde(rename = "liquid-glass", alias = "apple-liquid-glass")]
    #[cfg_attr(not(feature = "theme-liquid-glass"), serde(skip_deserializing))]
    LiquidGlass,
}

impl Default for ThemePreset {
    fn default() -> Self {
        Self::ALL.first().copied().unwrap_or(Self::Shadcn)
    }
}

impl ThemePreset {
    #[allow(non_upper_case_globals)]
    #[deprecated(note = "Use Fluent; WinUI 3 is the Windows implementation of Fluent")]
    pub const WinUi3: Self = Self::Fluent;

    pub const ALL: [Self;
        cfg!(feature = "theme-shadcn") as usize
            + cfg!(feature = "theme-fluent") as usize
            + cfg!(feature = "theme-material3") as usize
            + cfg!(feature = "theme-liquid-glass") as usize] = [
        #[cfg(feature = "theme-shadcn")]
        Self::Shadcn,
        #[cfg(feature = "theme-fluent")]
        Self::Fluent,
        #[cfg(feature = "theme-material3")]
        Self::Material3,
        #[cfg(feature = "theme-liquid-glass")]
        Self::LiquidGlass,
    ];

    pub const fn is_enabled(self) -> bool {
        match self {
            Self::Shadcn => cfg!(feature = "theme-shadcn"),
            Self::Fluent => cfg!(feature = "theme-fluent"),
            Self::Material3 => cfg!(feature = "theme-material3"),
            Self::LiquidGlass => cfg!(feature = "theme-liquid-glass"),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Shadcn => "shadcn",
            Self::Fluent => "fluent",
            Self::Material3 => "material3",
            Self::LiquidGlass => "liquid-glass",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct ThemeStyles {
    pub font_family: Option<String>,
    pub font_size: Option<f32>,
    pub font_weight: Option<u16>,
    pub line_height: Option<f32>,
    pub control_height: Option<f32>,
    pub control_padding_x: Option<f32>,
    pub control_radius: Option<f32>,
    pub panel_radius: Option<f32>,
    pub panel_padding: Option<f32>,
    pub gap: Option<f32>,
    pub border_width: Option<f32>,
    pub shadow: Option<String>,
    pub backdrop_blur: Option<f32>,
    pub surface_opacity: Option<f32>,
    pub disabled_opacity: Option<f32>,
    pub glass: Option<super::GlassMaterial>,
}

impl ThemeStyles {
    pub fn merge(&mut self, patch: &Self) {
        macro_rules! merge {
            ($($field:ident),* $(,)?) => { $(
                if patch.$field.is_some() {
                    self.$field = patch.$field.clone();
                }
            )* };
        }
        merge!(
            font_family,
            font_size,
            font_weight,
            line_height,
            control_height,
            control_padding_x,
            control_radius,
            panel_radius,
            panel_padding,
            gap,
            border_width,
            shadow,
            backdrop_blur,
            surface_opacity,
            disabled_opacity,
            glass
        );
    }

    pub fn validate(&self) -> Result<(), String> {
        if let Some(shadow) = &self.shadow {
            if shadow != "none" && super::parse_box_shadows(shadow).is_empty() {
                return Err("shadow must be a box-shadow value or none".into());
            }
        }
        if let Some(glass) = self.glass {
            glass.validate()?;
        }
        for (name, value, positive) in [
            ("font-size", self.font_size, true),
            ("line-height", self.line_height, true),
            ("control-height", self.control_height, true),
            ("control-padding-x", self.control_padding_x, false),
            ("control-radius", self.control_radius, false),
            ("panel-radius", self.panel_radius, false),
            ("panel-padding", self.panel_padding, false),
            ("gap", self.gap, false),
            ("border-width", self.border_width, false),
            ("backdrop-blur", self.backdrop_blur, false),
        ] {
            if value.is_some_and(|value| {
                !value.is_finite() || value < 0.0 || (positive && value == 0.0)
            }) {
                return Err(format!(
                    "{name} must be a finite {}number",
                    if positive {
                        "positive "
                    } else {
                        "non-negative "
                    }
                ));
            }
        }
        for (name, value) in [
            ("surface-opacity", self.surface_opacity),
            ("disabled-opacity", self.disabled_opacity),
        ] {
            if value.is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value)) {
                return Err(format!("{name} must be between 0 and 1"));
            }
        }
        if self
            .font_weight
            .is_some_and(|weight| !(1..=1000).contains(&weight))
        {
            return Err("font-weight must be between 1 and 1000".into());
        }
        if self
            .font_family
            .as_ref()
            .is_some_and(|family| family.trim().is_empty())
        {
            return Err("font-family must not be empty".into());
        }
        Ok(())
    }
}

pub fn theme_preset() -> ThemePreset {
    super::with_theme(|theme| theme.preset)
}

impl Theme {
    pub fn preset(preset: ThemePreset, mode: ThemeMode) -> Self {
        assert!(
            preset == ThemePreset::Shadcn || preset.is_enabled(),
            "theme preset {} is not enabled; enable its theme-* feature",
            preset.name()
        );
        let mut theme = match mode {
            ThemeMode::Light => Self::light(),
            ThemeMode::Dark => Self::dark(),
        };
        theme.preset = preset;
        if preset == ThemePreset::Shadcn {
            return theme;
        }
        #[cfg(any(
            feature = "theme-fluent",
            feature = "theme-material3",
            feature = "theme-liquid-glass"
        ))]
        theme.apply_preset(preset, mode);
        theme
    }

    #[cfg(any(
        feature = "theme-fluent",
        feature = "theme-material3",
        feature = "theme-liquid-glass"
    ))]
    fn apply_preset(&mut self, preset: ThemePreset, mode: ThemeMode) {
        let theme = self;
        let dark = mode == ThemeMode::Dark;
        let palette = match (preset, dark) {
            #[cfg(feature = "theme-fluent")]
            (ThemePreset::Fluent, dark) => fluent::palette(dark),
            #[cfg(feature = "theme-material3")]
            (ThemePreset::Material3, dark) => material3::palette(dark),
            #[cfg(feature = "theme-liquid-glass")]
            (ThemePreset::LiquidGlass, dark) => liquid_glass::palette(dark),
            _ => unreachable!(),
        }
        .map(super::parse_color);
        let colors = &mut theme.colors;
        colors.background = palette[0];
        colors.foreground = palette[1];
        colors.card = palette[2];
        colors.card_foreground = palette[1];
        colors.popover = palette[2];
        colors.popover_foreground = palette[1];
        colors.primary = palette[3];
        colors.primary_foreground = palette[4];
        colors.secondary = palette[5];
        colors.secondary_foreground = palette[1];
        colors.muted = palette[5];
        colors.muted_foreground = palette[6];
        colors.accent = palette[5];
        colors.accent_foreground = palette[1];
        colors.border = palette[7];
        colors.input = palette[7];
        colors.ring = palette[3];
        colors.ring[3] = 0.6;
        colors.sidebar = palette[2];
        colors.sidebar_foreground = palette[1];
        colors.sidebar_primary = palette[3];
        colors.sidebar_primary_foreground = palette[4];
        colors.sidebar_accent = palette[5];
        colors.sidebar_accent_foreground = palette[1];
        colors.sidebar_border = palette[7];
        colors.sidebar_ring = colors.ring;
        colors.surface = palette[5];
        colors.surface_foreground = palette[1];
        colors.code = palette[2];
        colors.code_foreground = palette[1];
        colors.code_highlight = palette[5];
        colors.code_number = palette[6];
        colors.caret = palette[3];
        colors.selection_background = palette[3];
        colors.selection_foreground = palette[4];
        colors.scrollbar_track = palette[5];
        colors.scrollbar_thumb = palette[6];
        theme.focus_ring_width = 2.0;
        theme.styles = ThemeStyles {
            panel_padding: Some(24.0),
            gap: Some(8.0),
            border_width: Some(1.0),
            ..ThemeStyles::default()
        };
        match preset {
            #[cfg(feature = "theme-fluent")]
            ThemePreset::Fluent => fluent::configure(theme, dark),
            #[cfg(feature = "theme-material3")]
            ThemePreset::Material3 => material3::configure(theme, dark),
            #[cfg(feature = "theme-liquid-glass")]
            ThemePreset::LiquidGlass => liquid_glass::configure(theme, dark),
            _ => unreachable!(),
        }
    }
}

pub fn set_theme_preset(preset: ThemePreset) {
    assert!(
        preset == ThemePreset::Shadcn || preset.is_enabled(),
        "theme preset {} is not enabled; enable its theme-* feature",
        preset.name()
    );
    super::theme_override::change_theme_preset(preset);
}

pub fn validate_component_styles(components: &BTreeMap<String, String>) -> Result<(), String> {
    for (slot, classes) in components {
        if slot.trim().is_empty() || slot.chars().any(char::is_whitespace) {
            return Err(format!("invalid component slot: {slot:?}"));
        }
        for token in classes.split_whitespace() {
            let parsed = jadix_style_tokens::parse_class_token(token);
            if !jadix_style_tokens::is_known_utility(&parsed.utility) {
                return Err(format!("unknown utility for {slot}: {token}"));
            }
        }
    }
    Ok(())
}

pub fn has_theme_styles() -> bool {
    super::with_theme(|theme| {
        theme.styles != ThemeStyles::default() || !theme.components.is_empty()
    })
}

pub fn button_theme_classes(theme: &Theme, variant: &str) -> String {
    if !matches!(
        theme.preset,
        ThemePreset::Fluent | ThemePreset::Material3 | ThemePreset::LiquidGlass
    ) {
        return String::new();
    }
    let colors = &theme.colors;
    let material = theme.preset == ThemePreset::Material3;
    let (background, foreground) = match variant {
        "default" => (colors.primary, colors.primary_foreground),
        "secondary" => (colors.secondary, colors.secondary_foreground),
        "destructive" => (colors.destructive, colors.destructive_foreground),
        "outline" => (
            [0.0; 4],
            if material {
                colors.primary
            } else {
                colors.foreground
            },
        ),
        "ghost" | "link" => (
            [0.0; 4],
            if material || variant == "link" {
                colors.primary
            } else {
                colors.foreground
            },
        ),
        _ => return String::new(),
    };
    let blend = |base: [f32; 4], overlay: [f32; 4], amount: f32| {
        if base[3] == 0.0 {
            [overlay[0], overlay[1], overlay[2], amount]
        } else {
            std::array::from_fn(|channel| {
                if channel == 3 {
                    1.0
                } else {
                    base[channel] * (1.0 - amount) + overlay[channel] * amount
                }
            })
        }
    };
    let (hover, pressed) = if material {
        (
            blend(background, foreground, 0.08),
            blend(background, foreground, 0.10),
        )
    } else {
        let overlay = if theme.mode == ThemeMode::Dark {
            [1.0; 4]
        } else {
            [0.0, 0.0, 0.0, 1.0]
        };
        (
            blend(background, overlay, 0.08),
            blend(background, [0.0, 0.0, 0.0, 1.0], 0.20),
        )
    };
    let transparent = matches!(variant, "ghost" | "link" | "outline");
    let (disabled_background, disabled_foreground) = if material {
        let mut fill = colors.foreground;
        fill[3] = if transparent { 0.0 } else { 0.12 };
        let mut text = colors.foreground;
        text[3] = 0.38;
        (fill, text)
    } else {
        let dark = theme.mode == ThemeMode::Dark;
        (
            if transparent {
                [0.0; 4]
            } else {
                super::parse_color(if dark { "#141414" } else { "#f0f0f0" })
            },
            super::parse_color(if dark { "#5c5c5c" } else { "#bdbdbd" }),
        )
    };
    let mut classes = Vec::new();
    for (prefix, color) in [
        ("bg", background),
        ("text", foreground),
        ("hover:bg", hover),
        ("active:bg", pressed),
        ("disabled:bg", disabled_background),
        ("disabled:text", disabled_foreground),
    ] {
        classes.push(format!(
            "{prefix}-rgba({},{},{},{})",
            color[0] * 255.0,
            color[1] * 255.0,
            color[2] * 255.0,
            color[3]
        ));
    }
    classes.push("disabled:opacity-100".into());
    if variant == "link" {
        classes.push("hover:underline".into());
    }
    classes.join(" ")
}

pub fn preset_slot_styles(preset: ThemePreset, slot: &str) -> ThemeStyles {
    if preset == ThemePreset::Shadcn {
        return ThemeStyles::default();
    }
    let field = matches!(
        slot,
        "input" | "textarea" | "select-trigger" | "combobox-trigger"
    );
    let label = matches!(slot, "button" | "badge" | "toggle");
    let modal = matches!(
        slot,
        "dialog-content"
            | "alert-dialog-content"
            | "sheet-content"
            | "drawer-content"
            | "command-overlay-content"
    );
    let material_field = preset == ThemePreset::Material3 && field;
    ThemeStyles {
        font_size: Some(if material_field {
            16.0
        } else if preset == ThemePreset::Fluent && slot == "badge" {
            12.0
        } else {
            14.0
        }),
        line_height: Some(if material_field {
            24.0
        } else if preset == ThemePreset::Fluent && slot == "badge" {
            16.0
        } else {
            20.0
        }),
        font_weight: Some(match (preset, label) {
            (ThemePreset::Fluent, true) => 600,
            (ThemePreset::Material3 | ThemePreset::LiquidGlass, true) => 500,
            _ => 400,
        }),
        control_radius: Some(match preset {
            ThemePreset::Fluent => {
                if slot == "checkbox" {
                    8.0
                } else {
                    4.0
                }
            }
            ThemePreset::Material3 => {
                if field {
                    4.0
                } else if matches!(slot, "badge" | "checkbox") {
                    8.0
                } else {
                    9999.0
                }
            }
            _ => {
                if field {
                    8.0
                } else if slot == "badge" {
                    6.0
                } else if slot == "checkbox" {
                    16.0
                } else {
                    9999.0
                }
            }
        }),
        panel_radius: Some(match preset {
            ThemePreset::LiquidGlass if matches!(slot, "sidebar-inset" | "sidebar-inner") => 20.0,
            ThemePreset::Fluent if slot == "sidebar-inset" => 0.0,
            ThemePreset::Fluent if slot == "card" => 4.0,
            ThemePreset::Fluent => {
                if slot == "tooltip-content" {
                    4.0
                } else {
                    8.0
                }
            }
            ThemePreset::Material3 => {
                if modal || slot == "sidebar-inset" {
                    28.0
                } else if slot == "card" {
                    12.0
                } else {
                    4.0
                }
            }
            _ => {
                if modal || matches!(slot, "popover-content" | "hover-card-content") {
                    24.0
                } else if matches!(slot, "card" | "command") {
                    16.0
                } else if slot == "tooltip-content" {
                    8.0
                } else {
                    20.0
                }
            }
        }),
        ..ThemeStyles::default()
    }
}
