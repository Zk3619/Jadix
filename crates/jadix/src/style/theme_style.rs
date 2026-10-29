use crate::core::tree::Element;

#[cfg(all(
    test,
    feature = "theme-shadcn",
    feature = "theme-fluent",
    feature = "theme-material3",
    feature = "theme-liquid-glass"
))]
#[path = "tests/theme_style.rs"]
mod tests;

pub use jadix_theme::theme_style::*;

pub(crate) fn slot_theme_classes(el: &mut Element) -> String {
    let Some(slot) = el.attrs.get("data-slot").cloned() else {
        return String::new();
    };
    super::with_theme(|theme| {
        let mut styles = preset_slot_styles(theme.preset, &slot);
        styles.merge(&theme.styles);
        let styles = &styles;
        let control = matches!(
            slot.as_str(),
            "button"
                | "input"
                | "textarea"
                | "select-trigger"
                | "combobox-trigger"
                | "toggle"
                | "badge"
        );
        let panel = matches!(
            slot.as_str(),
            "card"
                | "dialog-content"
                | "alert-dialog-content"
                | "sheet-content"
                | "drawer-content"
                | "popover-content"
                | "hover-card-content"
                | "select-content"
                | "combobox-content"
                | "dropdown-menu-content"
                | "dropdown-menu-sub-content"
                | "context-menu-content"
                | "context-menu-sub-content"
                | "menubar-content"
                | "menubar-sub-content"
                | "tooltip-content"
                | "command-overlay-content"
                | "command"
        );
        let inline = el.attrs.get("data-size").map(String::as_str) == Some("inline");
        let glass_chrome = theme.preset == ThemePreset::LiquidGlass
            && !(slot == "button"
                && el.attrs.get("data-variant").map(String::as_str) == Some("outline"))
            && (control
                || matches!(
                    slot.as_str(),
                    "sidebar-inset"
                        | "sidebar-inner"
                        | "tabs-list"
                        | "button-group"
                        | "toggle-group"
                ));
        let material_surface =
            (panel && !matches!(slot.as_str(), "tooltip-content" | "command")) || glass_chrome;
        let opaque = styles.surface_opacity == Some(1.0);
        let mut classes = Vec::new();
        let mut length = |prefix: &str, value: Option<f32>| {
            if let Some(value) = value {
                classes.push(format!("{prefix}-[{value}px]"));
            }
        };
        if control {
            length("rounded", styles.control_radius);
            length("text", styles.font_size);
            length("leading", styles.line_height);
            if !inline && slot != "badge" {
                let size = el
                    .attrs
                    .get("data-size")
                    .map(String::as_str)
                    .unwrap_or("default");
                let scale = match size {
                    "xs" | "icon-xs" => 0.75,
                    "sm" | "icon-sm" => 0.875,
                    "lg" | "icon-lg" => 1.125,
                    _ => 1.0,
                };
                let height = styles.control_height.map(|value| value * scale);
                if slot != "textarea" {
                    length("h", height);
                    if height.is_some() {
                        length("min-h", Some(0.0));
                    }
                }
                if size.starts_with("icon") {
                    length("w", height);
                    length("px", Some(0.0));
                } else {
                    length("px", styles.control_padding_x.map(|value| value * scale));
                }
            }
            length("gap", styles.gap);
            if el.style.border_width > 0.0 {
                length("border", styles.border_width);
            }
        }
        if panel {
            length("rounded", styles.panel_radius);
            length("gap", styles.gap);
            length("border", styles.border_width);
            if material_surface {
                length(
                    "backdrop-blur",
                    if opaque {
                        Some(0.0)
                    } else {
                        styles.backdrop_blur
                    },
                );
            }
            if slot == "card" {
                length("py", styles.panel_padding);
            } else if matches!(
                slot.as_str(),
                "dialog-content" | "alert-dialog-content" | "sheet-content" | "drawer-content"
            ) {
                length("p", styles.panel_padding);
            }
        }
        if matches!(
            slot.as_str(),
            "card-header" | "card-content" | "card-footer"
        ) {
            length("px", styles.panel_padding);
        }
        if slot == "checkbox" {
            length("rounded", styles.control_radius.map(|radius| radius * 0.25));
        }
        if control {
            apply_typography(el, styles);
            if slot == "button" {
                classes.push(button_theme_classes(
                    theme,
                    el.attrs
                        .get("data-variant")
                        .map(String::as_str)
                        .unwrap_or("default"),
                ));
            }
            if let Some(opacity) = styles.disabled_opacity {
                classes.push(format!("disabled:opacity-{}", opacity * 100.0));
            }
        }
        if matches!(
            slot.as_str(),
            "checkbox" | "switch" | "checkbox-group" | "switch-group"
        ) {
            if let Some(opacity) = styles.disabled_opacity {
                classes.push(format!("disabled:opacity-{}", opacity * 100.0));
            }
        }
        if panel {
            if let Some(glass) = styles.glass.filter(|_| material_surface && !opaque) {
                let params = glass.gpu_params();
                classes.push(format!(
                    "glass-[{}_{}_{}_{}]",
                    params[0], params[1], params[2], params[3]
                ));
            }
            if let Some(opacity) = styles.surface_opacity.filter(|_| material_surface) {
                let color = if slot == "card" {
                    theme.colors.card
                } else {
                    theme.colors.popover
                };
                // Component constructors provide a direct `bg-*` value. Keep
                // that value reactive to preset changes before utility classes
                // are applied, otherwise the constructor's opaque fill wins
                // over the theme's translucent surface utility.
                if theme.preset != ThemePreset::Shadcn {
                    el.style.bg_color = [color[0], color[1], color[2], color[3] * opacity];
                }
                classes.push(format!(
                    "bg-rgba({},{},{},{})",
                    color[0] * 255.0,
                    color[1] * 255.0,
                    color[2] * 255.0,
                    color[3] * opacity
                ));
            }
            let modal = matches!(
                slot.as_str(),
                "dialog-content"
                    | "alert-dialog-content"
                    | "sheet-content"
                    | "drawer-content"
                    | "command-overlay-content"
            );
            let shadow = styles.shadow.as_deref().or_else(|| match theme.preset {
                ThemePreset::Fluent if modal => {
                    Some("0 24px 48px 0 #0000003d, 0 0 8px 0 #00000033")
                }
                ThemePreset::Fluent if slot == "card" => Some("0 1px 2px 0 #00000024"),
                ThemePreset::Fluent => Some("0 8px 16px 0 #00000024, 0 0 2px 0 #0000001f"),
                ThemePreset::Material3 if matches!(slot.as_str(), "card" | "command") => {
                    Some("none")
                }
                ThemePreset::Material3 => Some("0 2px 6px 0 #00000026"),
                ThemePreset::LiquidGlass if !material_surface => Some("none"),
                ThemePreset::LiquidGlass => {
                    Some("0 8px 32px 0 #00000024, inset 0 1px 0 0 #ffffff66")
                }
                _ => None,
            });
            if let Some(shadow) = shadow {
                classes.push(if shadow == "none" {
                    "shadow-none".into()
                } else {
                    format!("shadow-[{}]", shadow.replace(' ', "_"))
                });
            }
        }
        if theme.preset != ThemePreset::Shadcn {
            match slot.as_str() {
                "sidebar"
                    if matches!(theme.preset, ThemePreset::Fluent | ThemePreset::LiquidGlass) =>
                {
                    el.style.bg_color[3] = 0.0;
                    classes.push("bg-transparent".into());
                    if theme.preset == ThemePreset::Fluent {
                        el.style.set_uniform_border_width(0.0);
                        classes.push("border-0".into());
                    }
                }
                "sidebar-wrapper" => classes.push(
                    if matches!(theme.preset, ThemePreset::Fluent | ThemePreset::LiquidGlass) {
                        "bg-transparent"
                    } else {
                        "bg-background"
                    }
                    .into(),
                ),
                "sidebar-inset" => {
                    if el.style.radii.iter().any(|radius| *radius > 0.0) {
                        if let Some(radius) = styles.panel_radius {
                            classes.push(format!("rounded-[{radius}px] max-md:rounded-none"));
                        }
                    }
                    if matches!(theme.preset, ThemePreset::Fluent | ThemePreset::LiquidGlass) {
                        el.style.bg_color[3] = 0.0;
                        classes.push("bg-transparent shadow-none".into());
                    } else {
                        classes.push("bg-background shadow-none".into());
                    }
                }
                "sidebar-inner" => {
                    if theme.preset == ThemePreset::Fluent {
                        el.style.bg_color[3] = 0.0;
                        el.style.set_uniform_border_width(0.0);
                        classes
                            .push("bg-transparent border-0 shadow-none backdrop-blur-none".into());
                    } else {
                        classes.push("bg-sidebar".into());
                    }
                    if el.style.radii.iter().any(|radius| *radius > 0.0) {
                        if let Some(radius) = styles.panel_radius {
                            classes.push(format!("rounded-[{radius}px]"));
                        }
                    }
                }
                "sidebar-menu-button" | "tabs-trigger" => {
                    let radius = match theme.preset {
                        ThemePreset::Fluent => 4.0,
                        ThemePreset::Material3 => 9999.0,
                        _ => 8.0,
                    };
                    classes.push(format!(
                        "rounded-[{}px]",
                        theme.styles.control_radius.unwrap_or(radius)
                    ));
                }
                "tabs-list" => classes.push("bg-muted".into()),
                "dropdown-menu-item"
                | "dropdown-menu-checkbox-item"
                | "dropdown-menu-radio-item"
                | "context-menu-item"
                | "context-menu-checkbox-item"
                | "context-menu-radio-item"
                | "menubar-item"
                | "menubar-checkbox-item"
                | "menubar-radio-item"
                | "select-item"
                | "combobox-item"
                | "command-item" => {
                    let radius = match theme.preset {
                        ThemePreset::LiquidGlass => 12.0,
                        ThemePreset::Material3 => 0.0,
                        _ => 4.0,
                    };
                    classes.push(format!(
                        "rounded-[{}px]",
                        theme.styles.control_radius.unwrap_or(radius)
                    ));
                }
                _ => {}
            }
        }
        if glass_chrome {
            classes.push(format!(
                "backdrop-blur-[{}px]",
                if opaque {
                    0.0
                } else {
                    styles.backdrop_blur.unwrap_or(8.0)
                }
            ));
            if !opaque {
                if let Some(mut glass) = styles.glass {
                    if control {
                        glass.thickness = glass.thickness.min(4.0);
                    }
                    let params = glass.gpu_params();
                    classes.push(format!(
                        "glass-[{}_{}_{}_{}]",
                        params[0], params[1], params[2], params[3]
                    ));
                }
            }
            let variant = el
                .attrs
                .get("data-variant")
                .map(String::as_str)
                .unwrap_or("default");
            let color = if slot == "button" && matches!(variant, "default" | "destructive") {
                if variant == "destructive" {
                    theme.colors.destructive
                } else {
                    theme.colors.primary
                }
            } else {
                theme.colors.popover
            };
            let opacity = styles.surface_opacity.unwrap_or(0.0);
            let alpha = if slot == "button" && matches!(variant, "default" | "destructive") {
                opacity.max(0.82)
            } else {
                opacity
            };
            if !matches!(variant, "ghost" | "link") {
                classes.push(format!(
                    "bg-rgba({},{},{},{})",
                    color[0] * 255.0,
                    color[1] * 255.0,
                    color[2] * 255.0,
                    color[3] * alpha
                ));
            }
        }
        if theme.preset == ThemePreset::LiquidGlass
            && matches!(
                slot.as_str(),
                "button" | "sidebar-menu-button" | "tabs-trigger" | "toggle-group-item"
            )
        {
            classes.push("transition-colors duration-180 ease-out".into());
        }
        if let Some(custom) = theme.components.get(&slot) {
            classes.push(custom.clone());
        }
        classes.join(" ")
    })
}

fn apply_typography(el: &mut Element, styles: &ThemeStyles) {
    if el.text_style.font_family.as_deref() == Some("lucide") {
        return;
    }
    if let Some(family) = &styles.font_family {
        if el.text_style.font_family.is_none() {
            el.text_style.font_family = Some(family.clone());
        }
    }
    if let Some(size) = styles.font_size {
        el.text_style.size = Some(size);
    }
    if let Some(weight) = styles.font_weight {
        el.text_style.weight = Some(weight);
    }
    if let Some(height) = styles.line_height {
        el.text_style.line_height_px = Some(height);
    }
    let text_style = (*el.text_style).clone();
    if let Some(classes) = el.class_set.as_mut() {
        if classes.base_text_style.is_some() {
            classes.base_text_style = Some(text_style);
        }
    }
    if let Some(sync) = el.input_paint_sync.as_mut() {
        if let Some(size) = styles.font_size {
            sync.text_size_px = size;
        }
        if let Some(weight) = styles.font_weight {
            sync.font_weight = weight;
        }
        if let Some(height) = styles.line_height {
            sync.line_height_px = Some(height);
        }
    }
    for child in &mut el.children {
        if !child.attrs.contains_key("data-slot") {
            apply_typography(child, styles);
        }
    }
}
