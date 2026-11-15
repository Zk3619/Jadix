//! Explicit CPU fallback for raster/Canvas output when Kitty is unavailable.
use crate::{CellColors, RgbaFrame};
use ratatui::{buffer::Buffer, layout::Rect, style::Color, widgets::Widget};

/// Samples the source image into 2×4 Braille dots per cell, filling the supplied
/// Rect. The caller owns aspect ratio/size. Pixels composite over `background`;
/// foreground is the average of enabled dots, never an image protocol upload.
pub struct BrailleImage<'a> {
    frame: &'a RgbaFrame,
    background: [u8; 3],
    contrast: u8,
    colors: CellColors,
}
impl<'a> BrailleImage<'a> {
    pub fn new(frame: &'a RgbaFrame) -> Self {
        Self {
            frame,
            background: [0; 3],
            contrast: 32,
            colors: CellColors::TrueColor,
        }
    }
    pub fn background(mut self, rgb: [u8; 3]) -> Self {
        self.background = rgb;
        self
    }
    /// Maximum-channel difference must exceed this threshold (0..=255).
    pub fn contrast(mut self, value: u8) -> Self {
        self.contrast = value;
        self
    }
    pub fn colors(mut self, colors: CellColors) -> Self {
        self.colors = colors;
        self
    }
}
impl Widget for BrailleImage<'_> {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        let clip = area.intersection(buffer.area);
        if clip.is_empty() {
            return;
        }
        let (width, height) = self.frame.size();
        let rgba = self.frame.pixels();
        let bg = Color::Rgb(self.background[0], self.background[1], self.background[2]);
        let background = self.colors.resolve(
            [
                self.background[0],
                self.background[1],
                self.background[2],
                255,
            ],
            bg,
        );
        for y in clip.y..clip.bottom() {
            for x in clip.x..clip.right() {
                let mut mask = 0;
                let mut sum = [0u32; 3];
                let mut count = 0;
                for (dy, row) in [[0, 3], [1, 4], [2, 5], [6, 7]].iter().enumerate() {
                    for (dx, bit) in row.iter().enumerate() {
                        // Center samples stay within source bounds, including
                        // non-divisible dimensions and very small images.
                        let sx = ((u64::from(x - area.x) * 4 + dx as u64 * 2 + 1)
                            * u64::from(width)
                            / (u64::from(area.width) * 4))
                            .min(u64::from(width - 1));
                        let sy = ((u64::from(y - area.y) * 8 + dy as u64 * 2 + 1)
                            * u64::from(height)
                            / (u64::from(area.height) * 8))
                            .min(u64::from(height - 1));
                        let p = &rgba[(sy * u64::from(width) + sx) as usize * 4..][..4];
                        let a = u32::from(p[3]);
                        let rgb: [u8; 3] = std::array::from_fn(|i| {
                            ((u32::from(p[i]) * a
                                + u32::from(self.background[i]) * (255 - a)
                                + 127)
                                / 255) as u8
                        });
                        if a > 0
                            && (0..3)
                                .map(|i| rgb[i].abs_diff(self.background[i]))
                                .max()
                                .unwrap()
                                > self.contrast
                        {
                            mask |= 1 << bit;
                            count += 1;
                            for i in 0..3 {
                                sum[i] += u32::from(rgb[i]);
                            }
                        }
                    }
                }
                let cell = &mut buffer[(x, y)];
                let symbol = char::from_u32(0x2800 + mask).unwrap();
                cell.set_char(if mask == 0 { ' ' } else { symbol })
                    .set_bg(background);
                if count > 0 {
                    let rgb = sum.map(|v| (v / count) as u8);
                    cell.set_fg(self.colors.resolve([rgb[0], rgb[1], rgb[2], 255], bg));
                } else {
                    cell.set_fg(background);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_eight_dot_positions_alpha_and_color_reduction_match_braille_coordinates() {
        for (index, bit) in [0, 3, 1, 4, 2, 5, 6, 7].into_iter().enumerate() {
            let mut pixels = vec![0; 32];
            pixels[index * 4..index * 4 + 4].copy_from_slice(&[255; 4]);
            let frame = RgbaFrame::new(2, 4, pixels).unwrap();
            let mut buffer = Buffer::empty(Rect::new(0, 0, 4, 4));
            BrailleImage::new(&frame)
                .colors(CellColors::Palette16)
                .render(Rect::new(2, 1, 1, 1), &mut buffer);
            assert_eq!(
                buffer[(2, 1)].symbol(),
                char::from_u32(0x2800 + (1 << bit)).unwrap().to_string()
            );
            assert_eq!(buffer[(2, 1)].fg, Color::White);
            assert_eq!(buffer[(0, 0)].symbol(), " ");
        }
        let frame = RgbaFrame::new(1, 1, vec![255, 0, 0, 0]).unwrap();
        let mut buffer = Buffer::empty(Rect::new(0, 0, 1, 1));
        BrailleImage::new(&frame).render(buffer.area, &mut buffer);
        assert_eq!(buffer[(0, 0)].symbol(), " ");
    }
    #[test]
    fn clipping_keeps_source_coordinates_and_cpu_canvas_can_render_without_kitty() {
        use jadix_2d::{Canvas2D, RasterCanvas};
        let mut canvas = RasterCanvas::new_linear(4, 4);
        canvas.clear("#000000");
        canvas.set_fill("#ffffff");
        canvas.fill_rect(0., 0., 2., 4.);
        let raster = RgbaFrame::from_canvas(canvas).unwrap();
        let frame = RgbaFrame::new(4, 4, [vec![255; 8], vec![0; 8]].concat().repeat(4)).unwrap();
        let mut full = Buffer::empty(Rect::new(0, 0, 2, 1));
        BrailleImage::new(&frame).render(full.area, &mut full);
        let mut clipped = Buffer::empty(Rect::new(1, 0, 1, 1));
        BrailleImage::new(&frame).render(Rect::new(0, 0, 2, 1), &mut clipped);
        assert_eq!(full[(1, 0)], clipped[(1, 0)]);
        assert_eq!(full[(0, 0)].symbol(), "⣿");
        assert_eq!(full[(1, 0)].symbol(), " ");
        let mut actual = Buffer::empty(full.area);
        BrailleImage::new(&raster).render(actual.area, &mut actual);
        assert_eq!(actual[(0, 0)].symbol(), full[(0, 0)].symbol());
    }
}
