use crate::style::theme::get_theme;

fn f32_to_u8(c: [f32; 4]) -> [u8; 4] {
    [
        (c[0] * 255.0) as u8,
        (c[1] * 255.0) as u8,
        (c[2] * 255.0) as u8,
        (c[3] * 255.0) as u8,
    ]
}

fn parse_hex_channel(pair: &str) -> Option<u8> {
    u8::from_str_radix(pair, 16).ok()
}

/// `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`
pub(crate) fn parse_hex_color_rgba(s: &str) -> Option<[u8; 4]> {
    let t = s.trim();
    let t = t.strip_prefix('#')?;
    match t.len() {
        3 => {
            let mut chars = t.chars();
            let r = chars.next()?;
            let g = chars.next()?;
            let b = chars.next()?;
            let rs = format!("{r}{r}");
            let gs = format!("{g}{g}");
            let bs = format!("{b}{b}");
            Some([
                parse_hex_channel(&rs)?,
                parse_hex_channel(&gs)?,
                parse_hex_channel(&bs)?,
                255,
            ])
        }
        4 => {
            let mut chars = t.chars();
            let r = chars.next()?;
            let g = chars.next()?;
            let b = chars.next()?;
            let a = chars.next()?;
            let rs = format!("{r}{r}");
            let gs = format!("{g}{g}");
            let bs = format!("{b}{b}");
            let a_s = format!("{a}{a}");
            Some([
                parse_hex_channel(&rs)?,
                parse_hex_channel(&gs)?,
                parse_hex_channel(&bs)?,
                parse_hex_channel(&a_s)?,
            ])
        }
        6 => Some([
            parse_hex_channel(&t[0..2])?,
            parse_hex_channel(&t[2..4])?,
            parse_hex_channel(&t[4..6])?,
            255,
        ]),
        8 => Some([
            parse_hex_channel(&t[0..2])?,
            parse_hex_channel(&t[2..4])?,
            parse_hex_channel(&t[4..6])?,
            parse_hex_channel(&t[6..8])?,
        ]),
        _ => None,
    }
}

/// Tailwind-like `hue-shade`（取 v3 常用档位的近似 sRGB，用于装饰色等）。
fn parse_tailwind_family_shade_token(s: &str) -> Option<[u8; 4]> {
    let (family, shade_s) = s.rsplit_once('-')?;
    if !shade_s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let shade: u32 = shade_s.parse().ok()?;
    let (r, g, b) = match (family, shade) {
        ("red", 50) => (254, 242, 242),
        ("red", 100) => (254, 226, 226),
        ("red", 200) => (254, 202, 202),
        ("red", 300) => (252, 165, 165),
        ("red", 400) => (248, 113, 113),
        ("red", 500) => (239, 68, 68),
        ("red", 600) => (220, 38, 38),
        ("red", 700) => (185, 28, 28),
        ("red", 800) => (153, 27, 27),
        ("red", 900) => (127, 29, 29),
        ("emerald", 400) => (52, 211, 153),
        ("emerald", 500) => (16, 185, 129),
        ("emerald", 600) => (5, 150, 105),
        ("violet", 400) => (167, 139, 250),
        ("violet", 500) => (139, 92, 246),
        ("violet", 600) => (124, 58, 237),
        ("blue", 500) => (59, 130, 246),
        ("amber", 500) => (245, 158, 11),
        _ => return None,
    };
    Some([r, g, b, 255])
}

pub fn parse_color(s: &str) -> [f32; 4] {
    let colors = &get_theme().colors;
    let s = s.trim();
    if let Some(c) = parse_hex_color_rgba(s) {
        return [
            c[0] as f32 / 255.0,
            c[1] as f32 / 255.0,
            c[2] as f32 / 255.0,
            c[3] as f32 / 255.0,
        ];
    }
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
    let s = s.trim();
    if let Some(c) = parse_hex_color_rgba(s) {
        return c;
    }
    if let Some(c) = parse_tailwind_family_shade_token(s) {
        return c;
    }
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
        "red" => [255, 77, 77, 255],
        _ => [23, 23, 23, 255],
    }
}
