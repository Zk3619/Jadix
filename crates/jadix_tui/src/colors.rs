//! Explicit character color reduction; terminal palette discovery is separate.
use ratatui::style::Color;
use std::{io, str::FromStr};

/// Character output color policy. Indexed/named colors use the conventional
/// xterm palette for approximation; actual terminal themes can redefine 0..15.
/// Default/reset backgrounds are assumed black when compositing alpha.
/// Pair `Palette16` with `AnsiColorWriter` for a basic-SGR transport.
/// The color policy alone only changes buffer colors.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CellColors {
    #[default]
    TrueColor,
    Indexed256,
    Palette16,
    /// Preserve terminal default foreground/background; keep semantic modifiers.
    Monochrome,
}
impl FromStr for CellColors {
    type Err = io::Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "rgb" => Ok(Self::TrueColor),
            "256" => Ok(Self::Indexed256),
            "16" => Ok(Self::Palette16),
            "mono" => Ok(Self::Monochrome),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "colors must be rgb, 256, 16 or mono",
            )),
        }
    }
}
pub(crate) const ANSI: [Color; 16] = [
    Color::Black,
    Color::Red,
    Color::Green,
    Color::Yellow,
    Color::Blue,
    Color::Magenta,
    Color::Cyan,
    Color::Gray,
    Color::DarkGray,
    Color::LightRed,
    Color::LightGreen,
    Color::LightYellow,
    Color::LightBlue,
    Color::LightMagenta,
    Color::LightCyan,
    Color::White,
];
const BASE: [[u8; 3]; 16] = [
    [0, 0, 0],
    [128, 0, 0],
    [0, 128, 0],
    [128, 128, 0],
    [0, 0, 128],
    [128, 0, 128],
    [0, 128, 128],
    [192, 192, 192],
    [128, 128, 128],
    [255, 0, 0],
    [0, 255, 0],
    [255, 255, 0],
    [0, 0, 255],
    [255, 0, 255],
    [0, 255, 255],
    [255, 255, 255],
];
const CUBE: [u8; 6] = [0, 95, 135, 175, 215, 255];
pub(crate) fn palette(index: u8) -> [u8; 3] {
    match index {
        0..=15 => BASE[usize::from(index)],
        16..=231 => {
            let i = usize::from(index - 16);
            [CUBE[i / 36], CUBE[i / 6 % 6], CUBE[i % 6]]
        }
        _ => [8 + (index - 232) * 10; 3],
    }
}
fn channels(color: Color) -> [u8; 3] {
    match color {
        Color::Rgb(r, g, b) => [r, g, b],
        Color::Indexed(i) => palette(i),
        _ => ANSI
            .iter()
            .position(|c| *c == color)
            .map_or([0; 3], |i| BASE[i]),
    }
}
fn distance(a: [u8; 3], b: [u8; 3]) -> u32 {
    (0..3)
        .map(|i| (i32::from(a[i]) - i32::from(b[i])).pow(2) as u32)
        .sum()
}
impl CellColors {
    pub(crate) fn resolve(self, rgba: [u8; 4], background: Color) -> Color {
        if self == Self::Monochrome {
            return Color::Reset;
        }
        if rgba[3] == 0 {
            return background;
        }
        let base = channels(background);
        let a = u32::from(rgba[3]);
        let value = std::array::from_fn(|i| {
            ((u32::from(rgba[i]) * a + u32::from(base[i]) * (255 - a) + 127) / 255) as u8
        });
        if self == Self::TrueColor {
            return Color::Rgb(value[0], value[1], value[2]);
        }
        let mut best = (u32::MAX, 0u8);
        let mut consider = |index| {
            let candidate = (distance(value, palette(index)), index);
            if candidate < best {
                best = candidate;
            }
        };
        for index in 0..16 {
            consider(index);
        }
        if self == Self::Indexed256 {
            // The Cartesian cube's nearest point is the nearest channel in
            // each dimension. Only one cube point and one gray need scoring.
            let cube: [u8; 3] = std::array::from_fn(|i| {
                (0..6)
                    .min_by_key(|c| (i32::from(value[i]) - i32::from(CUBE[*c])).pow(2))
                    .unwrap() as u8
            });
            consider(16 + cube[0] * 36 + cube[1] * 6 + cube[2]);
            let sum: i32 = value.into_iter().map(i32::from).sum();
            let gray = ((sum - 24 + 14).div_euclid(30)).clamp(0, 23) as u8;
            consider(232 + gray);
        }
        ANSI.get(usize::from(best.1))
            .copied()
            .unwrap_or(Color::Indexed(best.1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reduced_colors_match_exhaustive_palette_and_alpha_uses_named_backgrounds() {
        for r in (0..=255).step_by(17) {
            for g in (0..=255).step_by(17) {
                for b in (0..=255).step_by(17) {
                    let rgb = [r, g, b];
                    for (mode, count) in [
                        (CellColors::Palette16, 16u16),
                        (CellColors::Indexed256, 256),
                    ] {
                        let best = (0..count)
                            .min_by_key(|i| (distance(rgb, palette(*i as u8)), *i))
                            .unwrap() as u8;
                        let expected = ANSI
                            .get(usize::from(best))
                            .copied()
                            .unwrap_or(Color::Indexed(best));
                        assert_eq!(mode.resolve([r, g, b, 255], Color::Reset), expected);
                    }
                }
            }
        }
        assert_eq!(
            CellColors::TrueColor.resolve([255, 255, 255, 128], Color::Red),
            Color::Rgb(192, 128, 128)
        );
        assert_eq!(
            CellColors::TrueColor.resolve([255, 255, 255, 128], Color::Indexed(21)),
            Color::Rgb(128, 128, 255)
        );
        assert_eq!(
            CellColors::Palette16.resolve([0; 4], Color::Reset),
            Color::Reset
        );
        assert!("auto".parse::<CellColors>().is_err());
    }
}
