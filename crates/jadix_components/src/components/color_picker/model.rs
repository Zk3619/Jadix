//! ColorPicker 色空间与字符串编解码（产品层）。

use super::*;

pub(super) const PRESET_SWATCHES: &[&str] = &[
    "#000000", "#ffffff", "#ef4444", "#f97316", "#eab308", "#22c55e", "#14b8a6", "#3b82f6",
    "#8b5cf6", "#ec4899", "#64748b", "#0f172a",
];

/// 0–255 RGB + 0–1 alpha。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: f32,
}

impl Rgba {
    pub fn opaque(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 1.0 }
    }
    pub fn to_f32(self) -> [f32; 4] {
        [
            self.r as f32 / 255.0,
            self.g as f32 / 255.0,
            self.b as f32 / 255.0,
            self.a.clamp(0.0, 1.0),
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorFormat {
    #[default]
    Hex,
    Hex8,
    Rgb,
    Rgba,
    Hsl,
    Hsla,
}

impl ColorFormat {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "hex8" => Self::Hex8,
            "rgb" => Self::Rgb,
            "rgba" => Self::Rgba,
            "hsl" => Self::Hsl,
            "hsla" => Self::Hsla,
            _ => Self::Hex,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hex => "hex",
            Self::Hex8 => "hex8",
            Self::Rgb => "rgb",
            Self::Rgba => "rgba",
            Self::Hsl => "hsl",
            Self::Hsla => "hsla",
        }
    }
    pub fn supports_alpha(self) -> bool {
        matches!(self, Self::Hex8 | Self::Rgba | Self::Hsla)
    }
}

pub(super) const FORMAT_OPTIONS: &[(ColorFormat, &str)] = &[
    (ColorFormat::Hex, "HEX"),
    (ColorFormat::Hex8, "HEX8"),
    (ColorFormat::Rgb, "RGB"),
    (ColorFormat::Rgba, "RGBA"),
    (ColorFormat::Hsl, "HSL"),
    (ColorFormat::Hsla, "HSLA"),
];

pub(super) const SWATCHES_DEFAULT: &[&str] = PRESET_SWATCHES;
pub(super) const SWATCHES_TWITTER: &[&str] = &[
    "#FF6900", "#FCB900", "#7BDCB5", "#00D084", "#8ED1FC", "#0693E3", "#ABB8C3", "#EB144C",
    "#F78DA7", "#9900EF",
];
pub(super) const SWATCHES_GITHUB: &[&str] = &[
    "#B80000", "#DB3E00", "#FCCB00", "#008B02", "#006B76", "#1273DE", "#004DCF", "#5300EB",
    "#EB9694", "#FAD0C3", "#FEF3BD", "#C1E1C5", "#BEDADC", "#C4DEF6", "#BED3F3", "#D4C4FB",
];
pub(super) const SWATCHES_MATERIAL: &[&str] = &[
    "#F44336", "#E91E63", "#9C27B0", "#673AB7", "#3F51B5", "#2196F3", "#03A9F4", "#00BCD4",
    "#009688", "#4CAF50", "#8BC34A", "#CDDC39", "#FFEB3B", "#FFC107", "#FF9800", "#FF5722",
    "#795548", "#9E9E9E", "#607D8B", "#000000",
];
pub(super) const SWATCHES_BLOCK: &[&str] = &[
    "#D9E3F0", "#F47373", "#697689", "#37D67A", "#2CCCE4", "#555555", "#DCE775", "#FF8A65",
    "#BA68C8",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorPickerVariant {
    #[default]
    Default,
    Inline,
    Compact,
    Sketch,
    Chrome,
    Github,
    Twitter,
    Material,
    Block,
    Circle,
    Slider,
}

impl ColorPickerVariant {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "inline" => Self::Inline,
            "compact" => Self::Compact,
            "sketch" => Self::Sketch,
            "chrome" => Self::Chrome,
            "github" => Self::Github,
            "twitter" => Self::Twitter,
            "material" => Self::Material,
            "block" => Self::Block,
            "circle" => Self::Circle,
            "slider" => Self::Slider,
            _ => Self::Default,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Inline => "inline",
            Self::Compact => "compact",
            Self::Sketch => "sketch",
            Self::Chrome => "chrome",
            Self::Github => "github",
            Self::Twitter => "twitter",
            Self::Material => "material",
            Self::Block => "block",
            Self::Circle => "circle",
            Self::Slider => "slider",
        }
    }

    pub(super) fn panel_width(self) -> &'static str {
        match self {
            Self::Compact | Self::Twitter => "15.75rem",
            Self::Sketch => "18.75rem",
            Self::Github => "16.75rem",
            Self::Block => "11.5rem",
            Self::Circle => "15rem",
            _ => "17.5rem",
        }
    }

    pub(super) fn area_height(self) -> &'static str {
        match self {
            Self::Compact | Self::Github | Self::Material => "6rem",
            _ => "9rem",
        }
    }

    pub(super) fn close_on_swatch(self) -> bool {
        matches!(self, Self::Twitter | Self::Block | Self::Circle)
    }

    pub(super) fn circle_swatches(self) -> bool {
        matches!(self, Self::Github | Self::Circle)
    }

    pub(super) fn swatches(self) -> &'static [&'static str] {
        match self {
            Self::Twitter => SWATCHES_TWITTER,
            Self::Github => SWATCHES_GITHUB,
            Self::Material => SWATCHES_MATERIAL,
            Self::Block | Self::Circle => SWATCHES_BLOCK,
            Self::Chrome => SWATCHES_TWITTER,
            _ => SWATCHES_DEFAULT,
        }
    }

    pub(super) fn swatch_columns(self) -> u16 {
        match self {
            Self::Twitter | Self::Chrome => 5,
            Self::Github => 8,
            Self::Material => 5,
            Self::Block => 3,
            Self::Circle => 5,
            _ => 6,
        }
    }

    /// Fixed chip edge length. `aspect-ratio` is not applied by layout yet, so
    /// swatches must set both axes explicitly (react-color Github ≈ 25px).
    pub(super) fn swatch_chip_size(self) -> &'static str {
        match self {
            Self::Circle => "1.75rem",
            Self::Block => "2rem",
            Self::Github => "1.5rem",
            _ => "1.5rem",
        }
    }
}

pub fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let r = r as f32 / 255.0;
    let g = g as f32 / 255.0;
    let b = b as f32 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d < 1e-6 {
        0.0
    } else if (max - r).abs() < 1e-6 {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if (max - g).abs() < 1e-6 {
        60.0 * (((b - r) / d) + 2.0)
    } else {
        60.0 * (((r - g) / d) + 4.0)
    };
    let h = h.rem_euclid(360.0);
    let s = if max < 1e-6 { 0.0 } else { d / max };
    (h, s, max)
}

pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
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
    (
        ((r1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((g1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((b1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
    )
}

pub fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let r = r as f32 / 255.0;
    let g = g as f32 / 255.0;
    let b = b as f32 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) * 0.5;
    let d = max - min;
    if d < 1e-6 {
        return (0.0, 0.0, l * 100.0);
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let h = if (max - r).abs() < 1e-6 {
        60.0 * (((g - b) / d) % 6.0)
    } else if (max - g).abs() < 1e-6 {
        60.0 * (((b - r) / d) + 2.0)
    } else {
        60.0 * (((r - g) / d) + 4.0)
    };
    let h = if h < 0.0 { h + 360.0 } else { h };
    (h, s * 100.0, l * 100.0)
}

pub fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let s = (s / 100.0).clamp(0.0, 1.0);
    let l = (l / 100.0).clamp(0.0, 1.0);
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
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
    (
        ((r1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((g1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((b1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
    )
}

pub fn parse_color_str(raw: &str) -> Option<Rgba> {
    let s = raw.trim();
    if let Some(hex) = s.strip_prefix('#') {
        return parse_hex(hex);
    }
    let lower = s.to_ascii_lowercase();
    if let Some(inner) = lower
        .strip_prefix("rgba(")
        .or_else(|| lower.strip_prefix("rgb("))
        .and_then(|t| t.strip_suffix(')'))
    {
        let parts: Vec<_> = inner.split(',').map(|t| t.trim()).collect();
        if parts.len() >= 3 {
            let r = parts[0].parse().ok()?;
            let g = parts[1].parse().ok()?;
            let b = parts[2].parse().ok()?;
            let a = if parts.len() >= 4 {
                parts[3].parse().unwrap_or(1.0)
            } else {
                1.0
            };
            return Some(Rgba { r, g, b, a });
        }
    }
    if let Some(inner) = lower
        .strip_prefix("hsla(")
        .or_else(|| lower.strip_prefix("hsl("))
        .and_then(|t| t.strip_suffix(')'))
    {
        let parts: Vec<_> = inner
            .split(',')
            .map(|t| t.trim().trim_end_matches('%'))
            .collect();
        if parts.len() >= 3 {
            let h: f32 = parts[0].parse().ok()?;
            let s: f32 = parts[1].parse().ok()?;
            let l: f32 = parts[2].parse().ok()?;
            let a = if parts.len() >= 4 {
                parts[3].parse().unwrap_or(1.0)
            } else {
                1.0
            };
            let (r, g, b) = hsl_to_rgb(h, s, l);
            return Some(Rgba { r, g, b, a });
        }
    }
    parse_hex(s.trim_start_matches('#'))
}

fn parse_hex(hex: &str) -> Option<Rgba> {
    let h = hex.trim();
    match h.len() {
        3 => {
            let r = u8::from_str_radix(&h[0..1].repeat(2), 16).ok()?;
            let g = u8::from_str_radix(&h[1..2].repeat(2), 16).ok()?;
            let b = u8::from_str_radix(&h[2..3].repeat(2), 16).ok()?;
            Some(Rgba::opaque(r, g, b))
        }
        6 => {
            let r = u8::from_str_radix(&h[0..2], 16).ok()?;
            let g = u8::from_str_radix(&h[2..4], 16).ok()?;
            let b = u8::from_str_radix(&h[4..6], 16).ok()?;
            Some(Rgba::opaque(r, g, b))
        }
        8 => {
            let r = u8::from_str_radix(&h[0..2], 16).ok()?;
            let g = u8::from_str_radix(&h[2..4], 16).ok()?;
            let b = u8::from_str_radix(&h[4..6], 16).ok()?;
            let a = u8::from_str_radix(&h[6..8], 16).ok()? as f32 / 255.0;
            Some(Rgba { r, g, b, a })
        }
        _ => None,
    }
}

pub fn format_color(c: Rgba, fmt: ColorFormat) -> String {
    match fmt {
        ColorFormat::Hex => format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b),
        ColorFormat::Hex8 => {
            let a = (c.a.clamp(0.0, 1.0) * 255.0).round() as u8;
            format!("#{:02x}{:02x}{:02x}{:02x}", c.r, c.g, c.b, a)
        }
        ColorFormat::Rgb => format!("rgb({}, {}, {})", c.r, c.g, c.b),
        ColorFormat::Rgba => format!("rgba({}, {}, {}, {:.2})", c.r, c.g, c.b, c.a),
        ColorFormat::Hsl => {
            let (h, s, l) = rgb_to_hsl(c.r, c.g, c.b);
            format!("hsl({:.0}, {:.0}%, {:.0}%)", h, s, l)
        }
        ColorFormat::Hsla => {
            let (h, s, l) = rgb_to_hsl(c.r, c.g, c.b);
            format!("hsla({:.0}, {:.0}%, {:.0}%, {:.2})", h, s, l, c.a)
        }
    }
}

pub(super) fn sync_string_from_rgba(out: Signal<String>, c: Rgba, fmt: ColorFormat) {
    out.set(format_color(c, fmt));
}
