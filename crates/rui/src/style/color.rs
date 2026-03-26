use crate::style::theme::get_theme;

fn f32_to_u8(c: [f32; 4]) -> [u8; 4] {
    [
        (c[0] * 255.0) as u8,
        (c[1] * 255.0) as u8,
        (c[2] * 255.0) as u8,
        (c[3] * 255.0) as u8,
    ]
}

pub fn parse_color(s: &str) -> [f32; 4] {
    let colors = &get_theme().colors;
    match s {
        "primary" => colors.primary,
        "primary-foreground" => colors.primary_foreground,
        "secondary" => colors.secondary,
        "secondary-foreground" => colors.secondary_foreground,
        "accent" => colors.accent,
        "accent-foreground" => colors.accent_foreground,
        "muted" => colors.muted,
        "muted-foreground" => colors.muted_foreground,
        "destructive" => colors.destructive,
        "destructive-foreground" => colors.destructive_foreground,
        "background" => colors.background,
        "foreground" => colors.foreground,
        "border" => colors.border,
        "input" => colors.input,
        "ring" => colors.ring,
        "white" => [1.0, 1.0, 1.0, 1.0],
        "black" => [0.0, 0.0, 0.0, 1.0],
        "red" => [1.0, 0.3, 0.3, 1.0],
        "transparent" => [0.0, 0.0, 0.0, 0.0],
        _ => [0.5, 0.5, 0.5, 1.0],
    }
}

pub fn parse_text_color(s: &str) -> [u8; 4] {
    let colors = get_theme().colors;
    match s {
        "primary" => f32_to_u8(colors.primary),
        "primary-foreground" => f32_to_u8(colors.primary_foreground),
        "secondary" => f32_to_u8(colors.secondary),
        "secondary-foreground" => f32_to_u8(colors.secondary_foreground),
        "accent" => f32_to_u8(colors.accent),
        "accent-foreground" => f32_to_u8(colors.accent_foreground),
        "muted" => f32_to_u8(colors.muted),
        "muted-foreground" => f32_to_u8(colors.muted_foreground),
        "destructive" => f32_to_u8(colors.destructive),
        "destructive-foreground" => f32_to_u8(colors.destructive_foreground),
        "background" => f32_to_u8(colors.background),
        "foreground" => f32_to_u8(colors.foreground),
        "white" => [255, 255, 255, 255],
        "black" => [0, 0, 0, 255],
        _ => [23, 23, 23, 255],
    }
}
