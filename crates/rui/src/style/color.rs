use crate::style::theme::get_theme;

/// Fully transparent fill. Product paths should use this instead of `[0,0,0,0]`.
pub const TRANSPARENT: [f32; 4] = [0.0, 0.0, 0.0, 0.0];

pub(crate) fn f32_to_u8(c: [f32; 4]) -> [u8; 4] {
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

/// Tailwind v3 default palette (common families × 50–950). Returns `None` for unknown.
fn parse_tailwind_family_shade_token(s: &str) -> Option<[u8; 4]> {
    let (family, shade_s) = s.rsplit_once('-')?;
    if !shade_s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let shade: u32 = shade_s.parse().ok()?;
    let (r, g, b) = tailwind_rgb(family, shade)?;
    Some([r, g, b, 255])
}

fn tailwind_rgb(family: &str, shade: u32) -> Option<(u8, u8, u8)> {
    // Compact tables — shades 50,100,200,300,400,500,600,700,800,900,950 where available.
    macro_rules! row {
        ($($s:expr => $rgb:expr),* $(,)?) => {
            match shade {
                $($s => Some($rgb),)*
                _ => None,
            }
        };
    }
    match family {
        "slate" => row!(
            50 => (248, 250, 252), 100 => (241, 245, 249), 200 => (226, 232, 240),
            300 => (203, 213, 225), 400 => (148, 163, 184), 500 => (100, 116, 139),
            600 => (71, 85, 105), 700 => (51, 65, 85), 800 => (30, 41, 59),
            900 => (15, 23, 42), 950 => (2, 6, 23),
        ),
        "gray" => row!(
            50 => (249, 250, 251), 100 => (243, 244, 246), 200 => (229, 231, 235),
            300 => (209, 213, 219), 400 => (156, 163, 175), 500 => (107, 114, 128),
            600 => (75, 85, 99), 700 => (55, 65, 81), 800 => (31, 41, 55),
            900 => (17, 24, 39), 950 => (3, 7, 18),
        ),
        "zinc" => row!(
            50 => (250, 250, 250), 100 => (244, 244, 245), 200 => (228, 228, 231),
            300 => (212, 212, 216), 400 => (161, 161, 170), 500 => (113, 113, 122),
            600 => (82, 82, 91), 700 => (63, 63, 70), 800 => (39, 39, 42),
            900 => (24, 24, 27), 950 => (9, 9, 11),
        ),
        "neutral" => row!(
            50 => (250, 250, 250), 100 => (245, 245, 245), 200 => (229, 229, 229),
            300 => (212, 212, 212), 400 => (163, 163, 163), 500 => (115, 115, 115),
            600 => (82, 82, 82), 700 => (64, 64, 64), 800 => (38, 38, 38),
            900 => (23, 23, 23), 950 => (10, 10, 10),
        ),
        "stone" => row!(
            50 => (250, 250, 249), 100 => (245, 245, 244), 200 => (231, 229, 228),
            300 => (214, 211, 209), 400 => (168, 162, 158), 500 => (120, 113, 108),
            600 => (87, 83, 78), 700 => (68, 64, 60), 800 => (41, 37, 36),
            900 => (28, 25, 23), 950 => (12, 10, 9),
        ),
        "red" => row!(
            50 => (254, 242, 242), 100 => (254, 226, 226), 200 => (254, 202, 202),
            300 => (252, 165, 165), 400 => (248, 113, 113), 500 => (239, 68, 68),
            600 => (220, 38, 38), 700 => (185, 28, 28), 800 => (153, 27, 27),
            900 => (127, 29, 29), 950 => (69, 10, 10),
        ),
        "orange" => row!(
            50 => (255, 247, 237), 100 => (255, 237, 213), 200 => (254, 215, 170),
            300 => (253, 186, 116), 400 => (251, 146, 60), 500 => (249, 115, 22),
            600 => (234, 88, 12), 700 => (194, 65, 12), 800 => (154, 52, 18),
            900 => (124, 45, 18), 950 => (67, 20, 7),
        ),
        "amber" => row!(
            50 => (255, 251, 235), 100 => (254, 243, 199), 200 => (253, 230, 138),
            300 => (252, 211, 77), 400 => (251, 191, 36), 500 => (245, 158, 11),
            600 => (217, 119, 6), 700 => (180, 83, 9), 800 => (146, 64, 14),
            900 => (120, 53, 15), 950 => (69, 26, 3),
        ),
        "yellow" => row!(
            50 => (254, 252, 232), 100 => (254, 249, 195), 200 => (254, 240, 138),
            300 => (253, 224, 71), 400 => (250, 204, 21), 500 => (234, 179, 8),
            600 => (202, 138, 4), 700 => (161, 98, 7), 800 => (133, 77, 14),
            900 => (113, 63, 18), 950 => (66, 32, 6),
        ),
        "lime" => row!(
            50 => (247, 254, 231), 100 => (236, 252, 203), 200 => (217, 249, 157),
            300 => (190, 242, 100), 400 => (163, 230, 53), 500 => (132, 204, 22),
            600 => (101, 163, 13), 700 => (77, 124, 15), 800 => (63, 98, 18),
            900 => (54, 83, 20), 950 => (26, 46, 5),
        ),
        "green" => row!(
            50 => (240, 253, 244), 100 => (220, 252, 231), 200 => (187, 247, 208),
            300 => (134, 239, 172), 400 => (74, 222, 128), 500 => (34, 197, 94),
            600 => (22, 163, 74), 700 => (21, 128, 61), 800 => (22, 101, 52),
            900 => (20, 83, 45), 950 => (5, 46, 22),
        ),
        "emerald" => row!(
            50 => (236, 253, 245), 100 => (209, 250, 229), 200 => (167, 243, 208),
            300 => (110, 231, 183), 400 => (52, 211, 153), 500 => (16, 185, 129),
            600 => (5, 150, 105), 700 => (4, 120, 87), 800 => (6, 95, 70),
            900 => (6, 78, 59), 950 => (2, 44, 34),
        ),
        "teal" => row!(
            50 => (240, 253, 250), 100 => (204, 251, 241), 200 => (153, 246, 228),
            300 => (94, 234, 212), 400 => (45, 212, 191), 500 => (20, 184, 166),
            600 => (13, 148, 136), 700 => (15, 118, 110), 800 => (17, 94, 89),
            900 => (19, 78, 74), 950 => (4, 47, 46),
        ),
        "cyan" => row!(
            50 => (236, 254, 255), 100 => (207, 250, 254), 200 => (165, 243, 252),
            300 => (103, 232, 249), 400 => (34, 211, 238), 500 => (6, 182, 212),
            600 => (8, 145, 178), 700 => (14, 116, 144), 800 => (21, 94, 117),
            900 => (22, 78, 99), 950 => (8, 51, 68),
        ),
        "sky" => row!(
            50 => (240, 249, 255), 100 => (224, 242, 254), 200 => (186, 230, 253),
            300 => (125, 211, 252), 400 => (56, 189, 248), 500 => (14, 165, 233),
            600 => (2, 132, 199), 700 => (3, 105, 161), 800 => (7, 89, 133),
            900 => (12, 74, 110), 950 => (8, 47, 73),
        ),
        "blue" => row!(
            50 => (239, 246, 255), 100 => (219, 234, 254), 200 => (191, 219, 254),
            300 => (147, 197, 253), 400 => (96, 165, 250), 500 => (59, 130, 246),
            600 => (37, 99, 235), 700 => (29, 78, 216), 800 => (30, 64, 175),
            900 => (30, 58, 138), 950 => (23, 37, 84),
        ),
        "indigo" => row!(
            50 => (238, 242, 255), 100 => (224, 231, 255), 200 => (199, 210, 254),
            300 => (165, 180, 252), 400 => (129, 140, 248), 500 => (99, 102, 241),
            600 => (79, 70, 229), 700 => (67, 56, 202), 800 => (55, 48, 163),
            900 => (49, 46, 129), 950 => (30, 27, 75),
        ),
        "violet" => row!(
            50 => (245, 243, 255), 100 => (237, 233, 254), 200 => (221, 214, 254),
            300 => (196, 181, 253), 400 => (167, 139, 250), 500 => (139, 92, 246),
            600 => (124, 58, 237), 700 => (109, 40, 217), 800 => (91, 33, 182),
            900 => (76, 29, 149), 950 => (46, 16, 101),
        ),
        "purple" => row!(
            50 => (250, 245, 255), 100 => (243, 232, 255), 200 => (233, 213, 255),
            300 => (216, 180, 254), 400 => (192, 132, 252), 500 => (168, 85, 247),
            600 => (147, 51, 234), 700 => (126, 34, 206), 800 => (107, 33, 168),
            900 => (88, 28, 135), 950 => (59, 7, 100),
        ),
        "fuchsia" => row!(
            50 => (253, 244, 255), 100 => (250, 232, 255), 200 => (245, 208, 254),
            300 => (240, 171, 252), 400 => (232, 121, 249), 500 => (217, 70, 239),
            600 => (192, 38, 211), 700 => (162, 28, 175), 800 => (134, 25, 143),
            900 => (112, 26, 117), 950 => (74, 4, 78),
        ),
        "pink" => row!(
            50 => (253, 242, 248), 100 => (252, 231, 243), 200 => (251, 207, 232),
            300 => (249, 168, 212), 400 => (244, 114, 182), 500 => (236, 72, 153),
            600 => (219, 39, 119), 700 => (190, 24, 93), 800 => (157, 23, 77),
            900 => (131, 24, 67), 950 => (80, 7, 36),
        ),
        "rose" => row!(
            50 => (255, 241, 242), 100 => (255, 228, 230), 200 => (254, 205, 211),
            300 => (253, 164, 175), 400 => (251, 113, 133), 500 => (244, 63, 94),
            600 => (225, 29, 72), 700 => (190, 18, 60), 800 => (159, 18, 57),
            900 => (136, 19, 55), 950 => (76, 5, 25),
        ),
        _ => None,
    }
}

pub(crate) fn rgba_u8_to_f32(c: [u8; 4]) -> [f32; 4] {
    [
        c[0] as f32 / 255.0,
        c[1] as f32 / 255.0,
        c[2] as f32 / 255.0,
        c[3] as f32 / 255.0,
    ]
}

/// Parse CSS functional colors: `rgb()`/`rgba()`/`hsl()`/`hsla()`/`hwb()`/
/// `oklch()`/`oklab()`/`color-mix()`, including modern space-separated /
/// slash-alpha forms and relative syntax `rgb(from …)`.
fn parse_css_color_function(s: &str) -> Option<[f32; 4]> {
    let s = s.trim();
    let open = s.find('(')?;
    let close = s.rfind(')')?;
    if close <= open {
        return None;
    }
    let name = s[..open].trim().to_ascii_lowercase();
    let inner = s[open + 1..close].trim();
    match name.as_str() {
        "rgb" | "rgba" => {
            if inner.to_ascii_lowercase().starts_with("from ") {
                parse_relative_rgb_hsl(inner, true)
            } else {
                parse_rgb_args(inner)
            }
        }
        "hsl" | "hsla" => {
            if inner.to_ascii_lowercase().starts_with("from ") {
                parse_relative_rgb_hsl(inner, false)
            } else {
                parse_hsl_args(inner)
            }
        }
        "hwb" => parse_hwb_args(inner),
        "oklch" => parse_oklch_args(inner),
        "oklab" => parse_oklab_args(inner),
        "color-mix" => parse_color_mix_args(inner),
        _ => None,
    }
}

fn split_css_color_args(inner: &str) -> Vec<String> {
    // Support both `r, g, b[, a]` and `r g b[/ a]`.
    let t = inner.trim();
    if t.contains(',') {
        return t
            .split(',')
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect();
    }
    let mut parts = Vec::new();
    let mut alpha: Option<String> = None;
    let mut rest = t;
    if let Some((before, after)) = t.split_once('/') {
        rest = before.trim();
        let a = after.trim();
        if !a.is_empty() {
            alpha = Some(a.to_string());
        }
    }
    for tok in rest.split_whitespace() {
        parts.push(tok.to_string());
    }
    if let Some(a) = alpha {
        parts.push(a);
    }
    parts
}

fn parse_css_number_or_percent(tok: &str) -> Option<f32> {
    let t = tok.trim();
    if let Some(raw) = t.strip_suffix('%') {
        let v: f32 = raw.trim().parse().ok()?;
        return Some(v / 100.0);
    }
    t.parse().ok()
}

fn parse_rgb_channel(tok: &str) -> Option<f32> {
    let t = tok.trim();
    if let Some(raw) = t.strip_suffix('%') {
        let v: f32 = raw.trim().parse().ok()?;
        return Some((v / 100.0).clamp(0.0, 1.0));
    }
    let v: f32 = t.parse().ok()?;
    // CSS rgb: bare numbers are 0–255 (legacy + modern without none/relative).
    Some((v / 255.0).clamp(0.0, 1.0))
}

fn parse_alpha_component(tok: &str) -> Option<f32> {
    parse_css_number_or_percent(tok).map(|v| v.clamp(0.0, 1.0))
}

fn parse_rgb_args(inner: &str) -> Option<[f32; 4]> {
    let parts = split_css_color_args(inner);
    if parts.len() < 3 {
        return None;
    }
    let r = parse_rgb_channel(&parts[0])?;
    let g = parse_rgb_channel(&parts[1])?;
    let b = parse_rgb_channel(&parts[2])?;
    let a = if parts.len() >= 4 {
        parse_alpha_component(&parts[3])?
    } else {
        1.0
    };
    Some([r, g, b, a])
}

fn parse_hue_degrees(tok: &str) -> Option<f32> {
    let t = tok.trim().to_ascii_lowercase();
    if let Some(raw) = t.strip_suffix("deg") {
        return raw.trim().parse().ok();
    }
    if let Some(raw) = t.strip_suffix("turn") {
        let v: f32 = raw.trim().parse().ok()?;
        return Some(v * 360.0);
    }
    if let Some(raw) = t.strip_suffix("rad") {
        let v: f32 = raw.trim().parse().ok()?;
        return Some(v.to_degrees());
    }
    if let Some(raw) = t.strip_suffix("grad") {
        let v: f32 = raw.trim().parse().ok()?;
        return Some(v * 0.9);
    }
    t.parse().ok()
}

fn hsl_to_srgb(h_deg: f32, s: f32, l: f32) -> [f32; 3] {
    let s = s.clamp(0.0, 1.0);
    let l = l.clamp(0.0, 1.0);
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let h = ((h_deg % 360.0) + 360.0) % 360.0;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c * 0.5;
    let (r1, g1, b1) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    [r1 + m, g1 + m, b1 + m]
}

fn parse_hsl_args(inner: &str) -> Option<[f32; 4]> {
    let parts = split_css_color_args(inner);
    if parts.len() < 3 {
        return None;
    }
    let h = parse_hue_degrees(&parts[0])?;
    let s = parse_css_number_or_percent(&parts[1])?;
    let l = parse_css_number_or_percent(&parts[2])?;
    // `s`/`l` from percent → 0..=1; bare numbers in modern syntax are also 0..=1
    // but legacy `hsl(120, 100%, 50%)` uses percent. If someone writes `100` without
    // `%` for S/L, treat >1 as percent-like 0–100.
    let s = if s > 1.0 {
        (s / 100.0).clamp(0.0, 1.0)
    } else {
        s
    };
    let l = if l > 1.0 {
        (l / 100.0).clamp(0.0, 1.0)
    } else {
        l
    };
    let a = if parts.len() >= 4 {
        parse_alpha_component(&parts[3])?
    } else {
        1.0
    };
    let [r, g, b] = hsl_to_srgb(h, s, l);
    Some([r, g, b, a])
}

/// `hwb(h w% b% [/ a])` → display sRGB.
fn parse_hwb_args(inner: &str) -> Option<[f32; 4]> {
    let parts = split_css_color_args(inner);
    if parts.len() < 3 {
        return None;
    }
    let h = parse_hue_degrees(&parts[0])?;
    let mut w = parse_css_number_or_percent(&parts[1])?;
    let mut bl = parse_css_number_or_percent(&parts[2])?;
    if w > 1.0 {
        w = (w / 100.0).clamp(0.0, 1.0);
    }
    if bl > 1.0 {
        bl = (bl / 100.0).clamp(0.0, 1.0);
    }
    let a = if parts.len() >= 4 {
        parse_alpha_component(&parts[3])?
    } else {
        1.0
    };
    let sum = w + bl;
    let (w, bl) = if sum > 1.0 {
        (w / sum, bl / sum)
    } else {
        (w, bl)
    };
    // HWB → RGB: start from pure hue (HSL with S=100% L=50%), then mix white/black.
    let [hr, hg, hb] = hsl_to_srgb(h, 1.0, 0.5);
    let r = hr * (1.0 - w - bl) + w;
    let g = hg * (1.0 - w - bl) + w;
    let b = hb * (1.0 - w - bl) + w;
    Some([r, g, b, a])
}

/// `color-mix(in <space>, <color> [<%>], <color> [<%>])`
fn parse_color_mix_args(inner: &str) -> Option<[f32; 4]> {
    let inner = inner.trim();
    let rest = if inner.len() >= 3 && inner[..3].eq_ignore_ascii_case("in ") {
        inner[3..].trim_start()
    } else {
        return None;
    };
    // rest: `<space>, <color…>, <color…>`
    let comma = find_top_level_comma(rest)?;
    let space = rest[..comma].trim().to_ascii_lowercase();
    let colors_part = rest[comma + 1..].trim();
    let comma2 = find_top_level_comma(colors_part)?;
    let (c1, p1) = parse_color_mix_leg(colors_part[..comma2].trim())?;
    let (c2, p2) = parse_color_mix_leg(colors_part[comma2 + 1..].trim())?;

    let (_w1, w2) = match (p1, p2) {
        (Some(a), Some(b)) => {
            let s = a + b;
            if s <= 1e-6 {
                return None;
            }
            (a / s, b / s)
        }
        (Some(a), None) => (a.clamp(0.0, 1.0), 1.0 - a.clamp(0.0, 1.0)),
        (None, Some(b)) => (1.0 - b.clamp(0.0, 1.0), b.clamp(0.0, 1.0)),
        (None, None) => (0.5, 0.5),
    };
    // mix weight t = weight of second color.
    let t = w2;
    match space.as_str() {
        "oklab" | "oklch" => Some(super::color_space::mix_display_srgb_in_oklab(c1, c2, t)),
        "srgb-linear" | "linear-srgb" => {
            Some(super::color_space::lerp_display_srgb_linear(c1, c2, t))
        }
        // Default / `srgb` / `hsl` / `hwb`: channel lerp in display sRGB (MVP).
        _ => Some([
            c1[0] + (c2[0] - c1[0]) * t,
            c1[1] + (c2[1] - c1[1]) * t,
            c1[2] + (c2[2] - c1[2]) * t,
            c1[3] + (c2[3] - c1[3]) * t,
        ]),
    }
}

fn find_top_level_comma(s: &str) -> Option<usize> {
    let mut depth = 0i32;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

fn parse_color_mix_leg(s: &str) -> Option<([f32; 4], Option<f32>)> {
    let s = s.trim();
    // Trailing percentage: `red 40%` or `rgb(...) 40%`
    let (color_s, pct) = if let Some((before, after)) = rsplit_color_and_percent(s) {
        (before, Some(after))
    } else {
        (s, None)
    };
    let color = try_parse_color_recursive(color_s)?;
    Some((color, pct))
}

fn rsplit_color_and_percent(s: &str) -> Option<(&str, f32)> {
    let s = s.trim();
    let mut depth = 0i32;
    let mut last_ws = None;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            c if c.is_whitespace() && depth == 0 => last_ws = Some(i),
            _ => {}
        }
    }
    let i = last_ws?;
    let (left, right) = s.split_at(i);
    let right = right.trim();
    if !right.ends_with('%') {
        return None;
    }
    let v: f32 = right.trim_end_matches('%').trim().parse().ok()?;
    Some((left.trim(), (v / 100.0).clamp(0.0, 1.0)))
}

/// Parse a color without re-entering theme-only path infinitely — uses the public
/// parser which already handles functions / hex / names.
fn try_parse_color_recursive(s: &str) -> Option<[f32; 4]> {
    try_parse_color(s)
}

/// Relative color: `rgb(from <color> r g b [/ a])` or `hsl(from <color> h s l [/ a])`.
/// Channel tokens: `r`/`g`/`b`/`h`/`s`/`l`/`alpha`/`a`, numbers, percentages.
fn parse_relative_rgb_hsl(inner: &str, as_rgb: bool) -> Option<[f32; 4]> {
    let rest = {
        let t = inner.trim();
        let lower = t.to_ascii_lowercase();
        let stripped = lower.strip_prefix("from ")?;
        // Map back to original slice length.
        let skip = t.len() - stripped.len();
        t[skip..].trim_start()
    };
    let (origin, channels_s) = split_origin_color(rest)?;
    let origin_c = try_parse_color_recursive(origin)?;
    // Normalize `r g b / a` → treat `/` as whitespace separator for the alpha slot.
    let channels_norm = channels_s.replace('/', " ");
    let channel_tokens = split_ws_preserving_parens_local(&channels_norm);
    if channel_tokens.len() < 3 {
        return None;
    }

    if as_rgb {
        let r = eval_rel_channel(&channel_tokens[0], origin_c, RelSpace::Rgb)?;
        let g = eval_rel_channel(&channel_tokens[1], origin_c, RelSpace::Rgb)?;
        let b = eval_rel_channel(&channel_tokens[2], origin_c, RelSpace::Rgb)?;
        let a = if channel_tokens.len() >= 4 {
            eval_rel_channel(&channel_tokens[3], origin_c, RelSpace::Rgb)?
        } else {
            origin_c[3]
        };
        Some([
            r.clamp(0.0, 1.0),
            g.clamp(0.0, 1.0),
            b.clamp(0.0, 1.0),
            a.clamp(0.0, 1.0),
        ])
    } else {
        let (oh, os, ol) = rgb_to_hsl(origin_c[0], origin_c[1], origin_c[2]);
        let h = eval_rel_hsl_channel(&channel_tokens[0], oh, os, ol, origin_c[3], true)?;
        let s = eval_rel_hsl_channel(&channel_tokens[1], oh, os, ol, origin_c[3], false)?;
        let l = eval_rel_hsl_channel(&channel_tokens[2], oh, os, ol, origin_c[3], false)?;
        let a = if channel_tokens.len() >= 4 {
            eval_rel_hsl_channel(&channel_tokens[3], oh, os, ol, origin_c[3], false)?
        } else {
            origin_c[3]
        };
        let [r, g, b] = hsl_to_srgb(h, s.clamp(0.0, 1.0), l.clamp(0.0, 1.0));
        Some([r, g, b, a.clamp(0.0, 1.0)])
    }
}

enum RelSpace {
    Rgb,
}

fn split_origin_color(s: &str) -> Option<(&str, &str)> {
    let s = s.trim_start();
    if s.starts_with('#') {
        let end = s.find(|c: char| c.is_whitespace()).unwrap_or(s.len());
        return Some((s[..end].trim(), s[end..].trim_start()));
    }
    // Functional color: name(...)
    if let Some(open) = s.find('(') {
        let name = s[..open].trim();
        if !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphabetic() || b == b'-') {
            let after = &s[open + 1..];
            let mut depth = 1i32;
            for (i, ch) in after.char_indices() {
                match ch {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            let end = open + 1 + i + 1;
                            return Some((s[..end].trim(), s[end..].trim_start()));
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    // Named color / token (single word).
    let end = s.find(|c: char| c.is_whitespace()).unwrap_or(s.len());
    if end == 0 {
        return None;
    }
    Some((s[..end].trim(), s[end..].trim_start()))
}

fn split_ws_preserving_parens_local(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = None;
    let mut depth = 0i32;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => {
                depth += 1;
                if start.is_none() {
                    start = Some(i);
                }
            }
            ')' => depth = depth.saturating_sub(1),
            c if c.is_whitespace() && depth == 0 => {
                if let Some(st) = start {
                    out.push(s[st..i].trim());
                    start = None;
                }
            }
            _ => {
                if start.is_none() {
                    start = Some(i);
                }
            }
        }
    }
    if let Some(st) = start {
        let t = s[st..].trim();
        if !t.is_empty() {
            out.push(t);
        }
    }
    out
}

fn eval_rel_channel(tok: &str, origin: [f32; 4], _space: RelSpace) -> Option<f32> {
    let t = tok.trim().to_ascii_lowercase();
    match t.as_str() {
        "r" => Some(origin[0]),
        "g" => Some(origin[1]),
        "b" => Some(origin[2]),
        "alpha" | "a" => Some(origin[3]),
        _ => {
            if let Some(raw) = t.strip_suffix('%') {
                let v: f32 = raw.trim().parse().ok()?;
                return Some((v / 100.0).clamp(0.0, 1.0));
            }
            // Absolute 0–255 channel (common in relative examples) or 0–1.
            let v: f32 = t.parse().ok()?;
            if v > 1.0 {
                Some((v / 255.0).clamp(0.0, 1.0))
            } else {
                Some(v)
            }
        }
    }
}

fn eval_rel_hsl_channel(tok: &str, h: f32, s: f32, l: f32, a: f32, is_hue: bool) -> Option<f32> {
    let t = tok.trim().to_ascii_lowercase();
    match t.as_str() {
        "h" => Some(h),
        "s" => Some(s),
        "l" => Some(l),
        "alpha" | "a" => Some(a),
        _ => {
            if is_hue {
                return parse_hue_degrees(tok);
            }
            let mut v = parse_css_number_or_percent(tok)?;
            if v > 1.0 {
                v /= 100.0;
            }
            Some(v)
        }
    }
}

fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) * 0.5;
    if (max - min).abs() < 1e-6 {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = if (max - r).abs() < 1e-6 {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if (max - g).abs() < 1e-6 {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    } * 60.0;
    (h, s, l)
}

fn parse_oklch_args(inner: &str) -> Option<[f32; 4]> {
    let parts = split_css_color_args(inner);
    if parts.len() < 3 {
        return None;
    }
    let l = parse_css_number_or_percent(&parts[0])?;
    let c: f32 = parts[1].trim().parse().ok()?;
    let h = parse_hue_degrees(&parts[2])?;
    let a = if parts.len() >= 4 {
        parse_alpha_component(&parts[3])?
    } else {
        1.0
    };
    Some(super::color_space::oklch_to_display_srgba(l, c, h, a))
}

fn parse_oklab_args(inner: &str) -> Option<[f32; 4]> {
    let parts = split_css_color_args(inner);
    if parts.len() < 3 {
        return None;
    }
    let l = parse_css_number_or_percent(&parts[0])?;
    let a_ch: f32 = parts[1].trim().parse().ok()?;
    let b_ch: f32 = parts[2].trim().parse().ok()?;
    let alpha = if parts.len() >= 4 {
        parse_alpha_component(&parts[3])?
    } else {
        1.0
    };
    let rgb = super::color_space::oklab_to_display_srgb(l, a_ch, b_ch);
    Some([rgb[0], rgb[1], rgb[2], alpha])
}

/// CSS Level 4 `<named-color>` keywords → display sRGB.
pub(crate) fn css_named_color(s: &str) -> Option<[f32; 4]> {
    let t = s.trim();
    // CSS Level 4 named colors commonly used in MDN gradient examples.
    let (r, g, b) = match t.to_ascii_lowercase().as_str() {
        "aliceblue" => (240, 248, 255),
        "antiquewhite" => (250, 235, 215),
        "aqua" | "cyan" => (0, 255, 255),
        "aquamarine" => (127, 255, 212),
        "azure" => (240, 255, 255),
        "beige" => (245, 245, 220),
        "bisque" => (255, 228, 196),
        "blanchedalmond" => (255, 235, 205),
        "blue" => (0, 0, 255),
        "blueviolet" => (138, 43, 226),
        "brown" => (165, 42, 42),
        "burlywood" => (222, 184, 135),
        "cadetblue" => (95, 158, 160),
        "chartreuse" => (127, 255, 0),
        "chocolate" => (210, 105, 30),
        "coral" => (255, 127, 80),
        "cornflowerblue" => (100, 149, 237),
        "cornsilk" => (255, 248, 220),
        "crimson" => (220, 20, 60),
        "darkblue" => (0, 0, 139),
        "darkcyan" => (0, 139, 139),
        "darkgoldenrod" => (184, 134, 11),
        "darkgray" | "darkgrey" => (169, 169, 169),
        "darkgreen" => (0, 100, 0),
        "darkkhaki" => (189, 183, 107),
        "darkmagenta" => (139, 0, 139),
        "darkolivegreen" => (85, 107, 47),
        "darkorange" => (255, 140, 0),
        "darkorchid" => (153, 50, 204),
        "darkred" => (139, 0, 0),
        "darksalmon" => (233, 150, 122),
        "darkseagreen" => (143, 188, 143),
        "darkslateblue" => (72, 61, 139),
        "darkslategray" | "darkslategrey" => (47, 79, 79),
        "darkturquoise" => (0, 206, 209),
        "darkviolet" => (148, 0, 211),
        "deeppink" => (255, 20, 147),
        "deepskyblue" => (0, 191, 255),
        "dimgray" | "dimgrey" => (105, 105, 105),
        "dodgerblue" => (30, 144, 255),
        "firebrick" => (178, 34, 34),
        "floralwhite" => (255, 250, 240),
        "forestgreen" => (34, 139, 34),
        "fuchsia" | "magenta" => (255, 0, 255),
        "gainsboro" => (220, 220, 220),
        "ghostwhite" => (248, 248, 255),
        "gold" => (255, 215, 0),
        "goldenrod" => (218, 165, 32),
        "gray" | "grey" => (128, 128, 128),
        "green" => (0, 128, 0),
        "greenyellow" => (173, 255, 47),
        "honeydew" => (240, 255, 240),
        "hotpink" => (255, 105, 180),
        "indianred" => (205, 92, 92),
        "indigo" => (75, 0, 130),
        "ivory" => (255, 255, 240),
        "khaki" => (240, 230, 140),
        "lavender" => (230, 230, 250),
        "lavenderblush" => (255, 240, 245),
        "lawngreen" => (124, 252, 0),
        "lemonchiffon" => (255, 250, 205),
        "lightblue" => (173, 216, 230),
        "lightcoral" => (240, 128, 128),
        "lightcyan" => (224, 255, 255),
        "lightgoldenrodyellow" => (250, 250, 210),
        "lightgray" | "lightgrey" => (211, 211, 211),
        "lightgreen" => (144, 238, 144),
        "lightpink" => (255, 182, 193),
        "lightsalmon" => (255, 160, 122),
        "lightseagreen" => (32, 178, 170),
        "lightskyblue" => (135, 206, 250),
        "lightslategray" | "lightslategrey" => (119, 136, 153),
        "lightsteelblue" => (176, 196, 222),
        "lightyellow" => (255, 255, 224),
        "lime" => (0, 255, 0),
        "limegreen" => (50, 205, 50),
        "linen" => (250, 240, 230),
        "maroon" => (128, 0, 0),
        "mediumaquamarine" => (102, 205, 170),
        "mediumblue" => (0, 0, 205),
        "mediumorchid" => (186, 85, 211),
        "mediumpurple" => (147, 112, 219),
        "mediumseagreen" => (60, 179, 113),
        "mediumslateblue" => (123, 104, 238),
        "mediumspringgreen" => (0, 250, 154),
        "mediumturquoise" => (72, 209, 204),
        "mediumvioletred" => (199, 21, 133),
        "midnightblue" => (25, 25, 112),
        "mintcream" => (245, 255, 250),
        "mistyrose" => (255, 228, 225),
        "moccasin" => (255, 228, 181),
        "navajowhite" => (255, 222, 173),
        "navy" => (0, 0, 128),
        "oldlace" => (253, 245, 230),
        "olive" => (128, 128, 0),
        "olivedrab" => (107, 142, 35),
        "orange" => (255, 165, 0),
        "orangered" => (255, 69, 0),
        "orchid" => (218, 112, 214),
        "palegoldenrod" => (238, 232, 170),
        "palegreen" => (152, 251, 152),
        "paleturquoise" => (175, 238, 238),
        "palevioletred" => (219, 112, 147),
        "papayawhip" => (255, 239, 213),
        "peachpuff" => (255, 218, 185),
        "peru" => (205, 133, 63),
        "pink" => (255, 192, 203),
        "plum" => (221, 160, 221),
        "powderblue" => (176, 224, 230),
        "purple" => (128, 0, 128),
        "rebeccapurple" => (102, 51, 153),
        "red" => (255, 0, 0),
        "rosybrown" => (188, 143, 143),
        "royalblue" => (65, 105, 225),
        "saddlebrown" => (139, 69, 19),
        "salmon" => (250, 128, 114),
        "sandybrown" => (244, 164, 96),
        "seagreen" => (46, 139, 87),
        "seashell" => (255, 245, 238),
        "sienna" => (160, 82, 45),
        "silver" => (192, 192, 192),
        "skyblue" => (135, 206, 235),
        "slateblue" => (106, 90, 205),
        "slategray" | "slategrey" => (112, 128, 144),
        "snow" => (255, 250, 250),
        "springgreen" => (0, 255, 127),
        "steelblue" => (70, 130, 180),
        "tan" => (210, 180, 140),
        "teal" => (0, 128, 128),
        "thistle" => (216, 191, 216),
        "tomato" => (255, 99, 71),
        "turquoise" => (64, 224, 208),
        "violet" => (238, 130, 238),
        "wheat" => (245, 222, 179),
        "whitesmoke" => (245, 245, 245),
        "yellow" => (255, 255, 0),
        "yellowgreen" => (154, 205, 50),
        _ => return None,
    };
    Some([r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0])
}

/// Tailwind `token/10` · `token/80` · `token/[0.12]` opacity suffix → 0..1 alpha.
fn parse_color_opacity_suffix(tail: &str) -> Option<f32> {
    let t = tail.trim();
    if t.is_empty() {
        return None;
    }
    if let Some(inner) = t.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        let inner = inner.trim();
        if let Some(pct) = inner.strip_suffix('%') {
            return pct
                .trim()
                .parse::<f32>()
                .ok()
                .map(|v| (v / 100.0).clamp(0.0, 1.0));
        }
        return inner.parse::<f32>().ok().map(|v| v.clamp(0.0, 1.0));
    }
    if let Some(pct) = t.strip_suffix('%') {
        return pct
            .trim()
            .parse::<f32>()
            .ok()
            .map(|v| (v / 100.0).clamp(0.0, 1.0));
    }
    t.parse::<f32>().ok().map(|v| {
        if v > 1.0 {
            (v / 100.0).clamp(0.0, 1.0)
        } else {
            v.clamp(0.0, 1.0)
        }
    })
}

/// `color-mix` 等价：语义色 `token/N` 叠在主题 `background` 上的不透明结果（与 Button 一致）。
pub fn color_mix_over_background(fg: [f32; 4], alpha: f32) -> [f32; 4] {
    super::color_space::color_mix_over_surface(fg, get_theme().colors.background, alpha)
}

/// Parse a color name / hex / Tailwind shade / CSS color function for backgrounds.
///
/// Unknown tokens return `None` (callers may fall back). Prefer this over silent gray.
pub fn try_parse_color(s: &str) -> Option<[f32; 4]> {
    let theme = get_theme();
    let colors = &theme.colors;
    let s = s.trim();
    if let Some(c) = parse_hex_color_rgba(s) {
        return Some(rgba_u8_to_f32(c));
    }
    if let Some(c) = parse_css_color_function(s) {
        return Some(c);
    }
    if let Some(c) = parse_tailwind_family_shade_token(s) {
        return Some(rgba_u8_to_f32(c));
    }
    if let Some((name, opacity_tok)) = s.split_once('/') {
        if let Some(base) = try_parse_color(name) {
            if let Some(alpha) = parse_color_opacity_suffix(opacity_tok) {
                return Some(color_mix_over_background(base, alpha));
            }
        }
    }
    Some(match s {
        "background" => colors.background,
        // Tailwind v4 `text-foreground` → color token `foreground`.
        "foreground" | "text-foreground" => colors.foreground,
        "card" => colors.card,
        "card-foreground" => colors.card_foreground,
        "popover" => colors.popover,
        "popover-foreground" => colors.popover_foreground,
        "primary" => colors.primary,
        "primary-foreground" => colors.primary_foreground,
        "secondary" => colors.secondary,
        "secondary-foreground" => colors.secondary_foreground,
        "muted" => colors.muted,
        "muted-foreground" => colors.muted_foreground,
        "accent" => colors.accent,
        "accent-foreground" => colors.accent_foreground,
        "destructive" => colors.destructive,
        "destructive-foreground" => colors.destructive_foreground,
        "border" => colors.border,
        "input" => colors.input,
        "ring" => colors.ring,
        "chart-1" => colors.chart_1,
        "chart-2" => colors.chart_2,
        "chart-3" => colors.chart_3,
        "chart-4" => colors.chart_4,
        "chart-5" => colors.chart_5,
        "sidebar" => colors.sidebar,
        "sidebar-foreground" => colors.sidebar_foreground,
        "sidebar-primary" => colors.sidebar_primary,
        "sidebar-primary-foreground" => colors.sidebar_primary_foreground,
        "sidebar-accent" => colors.sidebar_accent,
        "sidebar-accent-foreground" => colors.sidebar_accent_foreground,
        "sidebar-border" => colors.sidebar_border,
        "sidebar-ring" => colors.sidebar_ring,
        "surface" => colors.surface,
        "surface-foreground" => colors.surface_foreground,
        "code" => colors.code,
        "code-foreground" => colors.code_foreground,
        "code-highlight" => colors.code_highlight,
        "code-number" => colors.code_number,
        "selection" => colors.selection,
        "selection-foreground" => colors.selection_foreground,
        "scrollbar-track" => colors.scrollbar_track,
        "scrollbar-thumb" => colors.scrollbar_thumb,
        "scrollbar-track-border" => colors.scrollbar_track_border,
        "scrollbar-thumb-border" => colors.scrollbar_thumb_border,
        "white" => [1.0, 1.0, 1.0, 1.0],
        "black" => [0.0, 0.0, 0.0, 1.0],
        "red" => [1.0, 0.3, 0.3, 1.0],
        "transparent" => [0.0, 0.0, 0.0, 0.0],
        _ => return css_named_color(s),
    })
}

pub fn parse_color(s: &str) -> [f32; 4] {
    try_parse_color(s).unwrap_or([0.5, 0.5, 0.5, 1.0])
}

pub fn try_parse_text_color(s: &str) -> Option<[u8; 4]> {
    try_parse_color(s).map(|c| f32_to_u8(c))
}

pub fn parse_text_color(s: &str) -> [u8; 4] {
    try_parse_text_color(s).unwrap_or([23, 23, 23, 255])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_semantic_token_opacity_suffix() {
        let d10 = try_parse_color("destructive/10").unwrap();
        let destructive = try_parse_color("destructive").unwrap();
        let bg = try_parse_color("background").unwrap();
        let want = [
            destructive[0] * 0.1 + bg[0] * 0.9,
            destructive[1] * 0.1 + bg[1] * 0.9,
            destructive[2] * 0.1 + bg[2] * 0.9,
            1.0,
        ];
        assert!(
            (0..4).all(|i| (d10[i] - want[i]).abs() < 1e-4),
            "destructive/10 should mix over background"
        );
        let p80 = try_parse_color("primary/80").unwrap();
        assert!(p80[0] < destructive[0] || p80[0] > 0.1);
    }

    #[test]
    fn parses_hex_and_rgba_hsl_oklch() {
        let hex = try_parse_color("#ff0000").unwrap();
        assert!((hex[0] - 1.0).abs() < 1e-3);

        let rgb = try_parse_color("rgb(255, 0, 0)").unwrap();
        assert!((rgb[0] - 1.0).abs() < 1e-3 && rgb[1] < 1e-3);

        let modern = try_parse_color("rgb(255 0 0 / 50%)").unwrap();
        assert!((modern[3] - 0.5).abs() < 1e-3);

        let hsl = try_parse_color("hsl(120, 100%, 50%)").unwrap();
        assert!(hsl[1] > 0.9 && hsl[0] < 1e-3);

        let oklch = try_parse_color("oklch(0.63 0.2 25)").unwrap();
        assert!(oklch[0] > 0.5); // reddish
    }

    #[test]
    fn parses_hwb_color_mix_and_relative() {
        let hwb = try_parse_color("hwb(120 20% 10%)").unwrap();
        assert!(hwb[1] > hwb[0] && hwb[1] > hwb[2]); // greenish

        let mixed = try_parse_color("color-mix(in oklab, red 50%, blue)").unwrap();
        assert!(mixed[0] > 0.1 && mixed[2] > 0.1);

        let rel = try_parse_color("rgb(from #ff0000 r g b / 50%)").unwrap();
        assert!((rel[0] - 1.0).abs() < 1e-3);
        assert!((rel[3] - 0.5).abs() < 1e-3);

        let rel_hsl = try_parse_color("hsl(from lime h s l)").unwrap();
        assert!(rel_hsl[1] > 0.8 && rel_hsl[0] < 0.2);
    }

    #[test]
    fn gradient_accepts_functional_colors() {
        use super::super::gradient::parse_gradient;
        let g = parse_gradient(
            "linear-gradient(to right, rgba(255,0,0,0.8), hsl(210 100% 50%), #00ff00)",
        )
        .unwrap();
        assert_eq!(g.stops().len(), 3);

        let g2 = parse_gradient(
            "linear-gradient(90deg, color-mix(in oklab, red, blue), hwb(40 10% 0%), rgb(from pink r g b))",
        )
        .unwrap();
        assert_eq!(g2.stops().len(), 3);
    }
}
