use crate::{parse_color, GlassMaterial, Theme};

pub(super) fn palette(dark: bool) -> [&'static str; 8] {
    if dark {
        [
            "#1e1e20", "#f5f5f7", "#2c2c2e", "#0a84ff", "#ffffff", "#3a3a3c", "#a1a1a6", "#48484a",
        ]
    } else {
        [
            "#f5f5f7", "#1d1d1f", "#ffffff", "#007aff", "#ffffff", "#e8e8ed", "#6e6e73", "#d2d2d7",
        ]
    }
}

pub(super) fn configure(theme: &mut Theme, dark: bool) {
    theme.colors.sidebar = parse_color(if dark { "#262628" } else { "#ececee" });
    theme.colors.radius = 24.0 / 16.0;
    theme.styles.font_family = Some("SF Pro Text, Segoe UI, sans-serif".into());
    theme.styles.control_height = Some(36.0);
    theme.styles.control_padding_x = Some(16.0);
    theme.styles.backdrop_blur = Some(8.0);
    theme.styles.surface_opacity = Some(if dark { 0.38 } else { 0.44 });
    theme.styles.glass = Some(GlassMaterial::default());
}
