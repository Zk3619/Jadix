use crate::{parse_color, Theme};

pub(super) fn palette(dark: bool) -> [&'static str; 8] {
    if dark {
        [
            "#1f1f1f", "#ffffff", "#292929", "#115ea3", "#ffffff", "#213951", "#bdbdbd", "#525252",
        ]
    } else {
        [
            "#fafafa", "#242424", "#ffffff", "#0f6cbd", "#ffffff", "#ebf3fc", "#616161", "#d1d1d1",
        ]
    }
}

pub(super) fn configure(theme: &mut Theme, dark: bool) {
    let colors = &mut theme.colors;
    colors.card = parse_color(if dark { "#5a5a64" } else { "#ffffff" });
    colors.secondary = parse_color(if dark { "#383838" } else { "#f5f5f5" });
    colors.muted = colors.secondary;
    colors.surface = colors.secondary;
    colors.scrollbar_track = [0.0; 4];
    colors.scrollbar_track_border = [0.0; 4];
    colors.scrollbar_thumb[3] = 0.72;
    colors.scrollbar_thumb_border = [0.0; 4];
    colors.sidebar = parse_color(if dark { "#242424" } else { "#f5f5f5" });
    colors.radius = 8.0 / 16.0;
    theme.styles.font_family = Some("Segoe UI Variable, Segoe UI, sans-serif".into());
    theme.styles.control_height = Some(32.0);
    theme.styles.control_padding_x = Some(12.0);
    theme.styles.backdrop_blur = Some(20.0);
    theme.styles.surface_opacity = Some(if dark { 0.18 } else { 0.35 });
}
