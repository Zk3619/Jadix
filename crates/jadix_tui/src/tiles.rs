//! Cell-aligned, bounded Kitty image damage. One frame fence covers the batch.
use crate::{KittyView, RgbaFrame, TerminalViewport};
use ratatui::{
    buffer::Buffer,
    layout::{Rect, Size},
    widgets::Widget,
};
use std::{
    io::{self, Write},
    num::NonZeroU32,
};

struct Tile {
    cells: Rect,
    pixels: [u32; 4],
    view: Option<KittyView>,
}
/// Encoding work, not an assertion that the terminal scanned out these bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DamageMetrics {
    pub compared_frames: u64,
    pub identical_frames: u64,
    pub encoded_tiles: u64,
    pub encoded_rgba_bytes: u64,
    pub full_frames: u64,
}
pub(crate) struct KittyTiles {
    first: NonZeroU32,
    fence: u32,
    viewport: TerminalViewport,
    tile_cells: Size,
    tiles: Vec<Tile>,
    previous: Option<RgbaFrame>,
    base: Option<KittyView>,
    cleanup: usize,
    compressed: bool,
    metrics: DamageMetrics,
}
impl KittyTiles {
    pub fn new(
        first: NonZeroU32,
        fence: u32,
        viewport: TerminalViewport,
        tile_cells: Size,
    ) -> io::Result<Self> {
        let tiles = Self::layout(first, fence, viewport, tile_cells)?;
        Ok(Self {
            first,
            fence,
            viewport,
            tile_cells,
            tiles,
            previous: None,
            base: None,
            cleanup: 0,
            compressed: false,
            metrics: DamageMetrics::default(),
        })
    }
    fn layout(
        first: NonZeroU32,
        fence: u32,
        vp: TerminalViewport,
        size: Size,
    ) -> io::Result<Vec<Tile>> {
        if first.get() == 1
            || size.width == 0
            || size.height == 0
            || size.width > 297
            || size.height > 297
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "tile dimensions must be 1..=297 cells",
            ));
        }
        let cells = vp.cells();
        let cols = u32::from(cells.width).div_ceil(u32::from(size.width));
        let rows = u32::from(cells.height).div_ceil(u32::from(size.height));
        let count = cols * rows;
        if count > 256
            || first.get().checked_add(count).is_none_or(|end| end > fence)
            || vp.pixels()[0] < u32::from(cells.width)
            || vp.pixels()[1] < u32::from(cells.height)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "tile count, image ID range or pixel geometry exceeds budget",
            ));
        }
        let [width, height] = vp.pixels();
        let mut result = Vec::with_capacity(count as usize);
        for row in 0..rows {
            for col in 0..cols {
                let x = col * u32::from(size.width);
                let y = row * u32::from(size.height);
                let w = u32::from(size.width).min(u32::from(cells.width) - x);
                let h = u32::from(size.height).min(u32::from(cells.height) - y);
                let px = (u64::from(width) * u64::from(x) / u64::from(cells.width)) as u32;
                let py = (u64::from(height) * u64::from(y) / u64::from(cells.height)) as u32;
                let right = (u64::from(width) * u64::from(x + w) / u64::from(cells.width)) as u32;
                let bottom =
                    (u64::from(height) * u64::from(y + h) / u64::from(cells.height)) as u32;
                result.push(Tile {
                    cells: Rect::new(cells.x + x as u16, cells.y + y as u16, w as u16, h as u16),
                    pixels: [px, py, right - px, bottom - py],
                    view: None,
                });
            }
        }
        Ok(result)
    }
    pub fn resize(&mut self, viewport: TerminalViewport) -> io::Result<()> {
        let tiles = Self::layout(self.first, self.fence, viewport, self.tile_cells)?;
        self.cleanup = self.cleanup.max(self.tiles.len());
        self.tiles = tiles;
        self.viewport = viewport;
        self.previous = None;
        self.base = None;
        Ok(())
    }
    pub fn set_compressed(&mut self, value: bool) {
        self.compressed = value;
    }
    pub fn metrics(&self) -> DamageMetrics {
        self.metrics
    }
    pub fn prepare(&mut self, frame: RgbaFrame) -> io::Result<bool> {
        if frame.size() != (self.viewport.pixels()[0], self.viewport.pixels()[1]) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "frame differs from tiled viewport",
            ));
        }
        self.metrics.compared_frames += 1;
        if self
            .previous
            .as_ref()
            .is_some_and(|old| old.pixels() == frame.pixels())
        {
            self.metrics.identical_frames += 1;
            return Ok(false);
        }
        let stride = self.viewport.pixels()[0] as usize * 4;
        let mut changed = Vec::new();
        for (index, tile) in self.tiles.iter().enumerate() {
            let [x, y, w, h] = tile.pixels.map(|v| v as usize);
            let equal = self.previous.as_ref().is_some_and(|old| {
                (y..y + h).all(|row| {
                    let start = row * stride + x * 4;
                    old.pixels()[start..start + w * 4] == frame.pixels()[start..start + w * 4]
                })
            });
            if equal {
                continue;
            }
            changed.push(index);
            if changed.len() * 2 >= self.tiles.len() {
                break;
            }
        }
        // Dense damage is cheaper as a keyframe. Sparse changes overlay only
        // the affected cells; a keyframe retires every old tile placement.
        if changed.len() * 2 >= self.tiles.len() {
            let view = KittyView::encode_ref(
                NonZeroU32::new(self.first.get() - 1).unwrap(),
                &frame,
                self.viewport.cells().as_size(),
                self.compressed,
            )?;
            if self.tiles.iter().any(|tile| tile.view.is_some()) {
                self.cleanup = self.cleanup.max(self.tiles.len());
            }
            for tile in &mut self.tiles {
                tile.view = None;
            }
            self.base = Some(view);
            self.metrics.encoded_tiles += 1;
            self.metrics.encoded_rgba_bytes += frame.pixels().len() as u64;
            self.metrics.full_frames += 1;
            self.previous = Some(frame);
            return Ok(true);
        }
        let mut updates = Vec::with_capacity(changed.len());
        let mut bytes = 0;
        for index in changed {
            let tile = &self.tiles[index];
            let [x, y, w, h] = tile.pixels.map(|v| v as usize);
            let mut pixels = Vec::with_capacity(w * h * 4);
            for row in y..y + h {
                pixels.extend_from_slice(
                    &frame.pixels()[row * stride + x * 4..row * stride + (x + w) * 4],
                );
            }
            let rgba = RgbaFrame::new(w as u32, h as u32, pixels)?;
            let id = NonZeroU32::new(self.first.get() + index as u32).unwrap();
            let view = KittyView::encode(id, rgba, tile.cells.as_size(), self.compressed)?;
            bytes += (w * h * 4) as u64;
            updates.push((index, view));
        }
        self.metrics.encoded_tiles += updates.len() as u64;
        self.metrics.encoded_rgba_bytes += bytes;
        for (index, view) in updates {
            self.tiles[index].view = Some(view);
        }
        self.previous = Some(frame);
        Ok(true)
    }
    pub fn draw(&self, buffer: &mut Buffer) {
        if let Some(base) = &self.base {
            base.render(self.viewport.cells(), buffer);
        }
        for tile in &self.tiles {
            if let Some(view) = &tile.view {
                view.render(tile.cells, buffer);
            }
        }
    }
    pub fn cleanup(&mut self, output: &mut impl Write) -> io::Result<()> {
        for index in 0..self.cleanup {
            write!(
                output,
                "\x1b_Ga=d,d=I,i={},q=2;\x1b\\",
                self.first.get() + index as u32
            )?;
        }
        self.cleanup = 0;
        Ok(())
    }
    pub fn close(&mut self, output: &mut impl Write) -> io::Result<()> {
        self.cleanup = self.cleanup.max(self.tiles.len());
        self.previous = None;
        self.base = None;
        self.tiles.clear();
        self.cleanup(output)?;
        output.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn vp(cells: Rect, pixels: [u32; 2]) -> TerminalViewport {
        TerminalViewport::new(cells, pixels, 1.0).unwrap()
    }
    #[test]
    fn uneven_pixels_reassemble_exactly_and_only_changed_tiles_encode() {
        let viewport = vp(Rect::new(3, 2, 5, 3), [17, 11]);
        let mut surface = KittyTiles::new(
            NonZeroU32::new(100).unwrap(),
            1000,
            viewport,
            Size::new(2, 2),
        )
        .unwrap();
        let pixels: Vec<_> = (0..17 * 11 * 4).map(|n| (n % 251) as u8).collect();
        assert!(surface
            .prepare(RgbaFrame::new(17, 11, pixels.clone()).unwrap())
            .unwrap());
        let mut restored = vec![0; pixels.len()];
        for tile in &surface.tiles {
            let [x, y, w, h] = tile.pixels.map(|v| v as usize);
            for row in y..y + h {
                restored[row * 17 * 4 + x * 4..row * 17 * 4 + (x + w) * 4]
                    .copy_from_slice(&pixels[row * 17 * 4 + x * 4..row * 17 * 4 + (x + w) * 4]);
            }
        }
        assert_eq!(restored, pixels);
        assert_eq!(surface.metrics().encoded_tiles, 1);
        assert_eq!(surface.metrics().full_frames, 1);
        assert!(!surface
            .prepare(RgbaFrame::new(17, 11, pixels.clone()).unwrap())
            .unwrap());
        let mut pixels = pixels;
        pixels[0] ^= 1;
        assert!(surface
            .prepare(RgbaFrame::new(17, 11, pixels).unwrap())
            .unwrap());
        assert_eq!(surface.metrics().encoded_tiles, 2);
        assert_eq!(surface.metrics().identical_frames, 1);
        let mut out = Vec::new();
        surface.close(&mut out).unwrap();
        assert!(String::from_utf8(out).unwrap().contains("i=105"));
    }
    #[test]
    fn resize_budget_and_id_ranges_fail_before_replacing_live_state() {
        let viewport = vp(Rect::new(0, 0, 80, 32), [480, 384]);
        assert!(KittyTiles::new(
            NonZeroU32::new(990).unwrap(),
            1000,
            viewport,
            Size::new(8, 4)
        )
        .is_err());
        assert!(
            KittyTiles::new(NonZeroU32::new(1).unwrap(), 1000, viewport, Size::new(1, 1)).is_err()
        );
        let mut surface = KittyTiles::new(
            NonZeroU32::new(100).unwrap(),
            1000,
            viewport,
            Size::new(16, 8),
        )
        .unwrap();
        let invalid = vp(Rect::new(0, 0, 600, 600), [600, 600]);
        assert!(surface.resize(invalid).is_err());
        assert_eq!(surface.viewport, viewport);
        surface
            .resize(vp(Rect::new(0, 0, 20, 8), [120, 96]))
            .unwrap();
        let mut out = Vec::new();
        surface.cleanup(&mut out).unwrap();
        assert_eq!(String::from_utf8(out).unwrap().matches("a=d").count(), 20);
    }
}
