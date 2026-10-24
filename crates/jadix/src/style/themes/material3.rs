use crate::style::{parse_color, Theme};

pub(super) fn palette(dark: bool) -> [&'static str; 8] {
    if dark {
        [
            "#141218", "#e6e0e9", "#211f26", "#d0bcff", "#381e72", "#4a4458", "#cac4d0", "#938f99",
        ]
    } else {
        [
            "#fef7ff", "#1d1b20", "#f3edf7", "#6750a4", "#ffffff", "#e8def8", "#49454f", "#79747e",
        ]
    }
}

pub(super) fn configure(theme: &mut Theme, dark: bool) {
    let colors = &mut theme.colors;
    colors.popover = parse_color(if dark { "#2b2930" } else { "#ece6f0" });
    colors.border = parse_color(if dark { "#49454f" } else { "#cac4d0" });
    colors.destructive = parse_color(if dark { "#f2b8b5" } else { "#b3261e" });
    colors.destructive_foreground = parse_color(if dark { "#601410" } else { "#ffffff" });
    colors.sidebar = parse_color(if dark { "#1d1b20" } else { "#f7f2fa" });
    colors.radius = 28.0 / 16.0;
    theme.focus_ring_width = 3.0;
    theme.styles.font_family = Some("Roboto, sans-serif".into());
    theme.styles.control_height = Some(40.0);
    theme.styles.control_padding_x = Some(24.0);
    theme.styles.backdrop_blur = Some(0.0);
    theme.styles.surface_opacity = Some(1.0);
}
