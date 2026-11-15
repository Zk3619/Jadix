//! Terminal presentation alongside Jadix's self-drawn and native-view products.
//! Ratatui owns character layout; Kitty presents straight sRGB RGBA images.
//! No GPUI dependency or native OS-view emulation is involved.

use image::{DynamicImage, RgbaImage};
use ratatui::{
    buffer::Buffer,
    layout::{Rect, Size},
    widgets::Widget,
};
use ratatui_image::{
    protocol::{kitty::Kitty, Protocol},
    Image,
};
use std::{io, io::Write, num::NonZeroU32};

pub use crossterm;
pub use ratatui;
pub use ratatui_image::picker::{Capability, Picker, ProtocolType};

mod encode;
mod surface;
pub use surface::{FrameTicket, KittySurface, TerminalInput, TerminalViewport};
mod flow;
pub use flow::{KittyFrameFlow, KittyReply, KittyReplyDecoder, TerminalPacket};
mod braille;
mod cells;
#[cfg(feature = "shell")]
mod shell;
pub use braille::BrailleImage;
#[cfg(feature = "shell")]
pub use shell::ShellApplication;
mod ansi;
pub use ansi::AnsiColorWriter;
mod default_colors;
pub use default_colors::{DefaultColorQuery, DefaultColors};
mod colors;
pub use colors::CellColors;
mod checkpoint;
pub use cells::{CellSearchMatch, CellSearchSummary, CellSurface};
#[cfg(feature = "portable")]
mod application;
#[cfg(feature = "portable")]
pub use application::{CellApplication, Shortcut};
mod commands;
mod menu;
pub use commands::{CommandBinding, CommandContext, CommandInput, CommandRouter};
mod metrics;
pub use menu::{MenuInput, MenuItem, TerminalMenu};
pub use metrics::{FrameMeter, FrameSample, MeasuredBackend, MeasuredWriter};
mod clipboard;
mod editor;
mod input;
mod mailbox;
mod request;
#[cfg(unix)]
mod session_unix;
#[cfg(windows)]
mod session_windows;
mod terminal;
mod text_grid;
mod transport;
pub use transport::TmuxPassthrough;
mod tiles;
mod wake;
pub use checkpoint::EditorCheckpoint;
pub use clipboard::TerminalClipboard;
pub use editor::TerminalEditor;
pub use input::{InputDecoder, StdinEvents, WireEvent};
#[cfg(feature = "native-clipboard")]
pub use jadix_clipboard::{ClipboardContext, TextClipboard};
pub use jadix_core::environment;
pub use jadix_document::checkpoint::{Checkpoint, CheckpointStore};
pub use jadix_tasks::{CancellationToken, EventSendError, EventSink};
pub use mailbox::{HostEvent, Mailbox};
pub use request::{RequestSlot, RequestTicket, TaskUpdate};
pub use terminal::{KittyTerminal, TerminalModes, TerminalOutput};
pub use tiles::DamageMetrics;

#[cfg(feature = "jadix-gpu")]
mod scene;
#[cfg(feature = "jadix-gpu")]
pub use scene::{JadixScene, SceneInput};

/// Straight-alpha, display-sRGB pixels, tightly packed, top row first.
/// Budget checks happen before constructing the image protocol.
pub struct RgbaFrame(RgbaImage);
impl RgbaFrame {
    pub const MAX_BYTES: usize = 64 * 1024 * 1024;

    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> io::Result<Self> {
        let bytes = (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| n.checked_mul(4));
        if width == 0
            || height == 0
            || bytes != Some(pixels.len())
            || pixels.len() > Self::MAX_BYTES
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid or over-budget RGBA frame",
            ));
        }
        Ok(Self(RgbaImage::from_raw(width, height, pixels).unwrap()))
    }

    /// Export Jadix's CPU Canvas pixels (premultiplied display-sRGB floats).
    pub fn from_canvas(canvas: jadix_2d::RasterCanvas) -> io::Result<Self> {
        use jadix_2d::Canvas2D;
        let size = canvas.size();
        if u64::from(size.w as u32) * u64::from(size.h as u32) > (Self::MAX_BYTES / 4) as u64 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "canvas exceeds terminal budget",
            ));
        }
        let pixels = canvas
            .into_pixels()
            .chunks_exact(4)
            .flat_map(|p| {
                let a = p[3].clamp(0.0, 1.0);
                let channel = |v: f32| {
                    if a > 0.0 {
                        ((v / a).clamp(0.0, 1.0) * 255.0).round() as u8
                    } else {
                        0
                    }
                };
                [
                    channel(p[0]),
                    channel(p[1]),
                    channel(p[2]),
                    (a * 255.0).round() as u8,
                ]
            })
            .collect();
        Self::new(size.w as u32, size.h as u32, pixels)
    }

    pub fn pixels(&self) -> &[u8] {
        self.0.as_raw()
    }
    pub fn size(&self) -> (u32, u32) {
        self.0.dimensions()
    }
}

/// A retained Kitty image with a caller-owned ID and a cell-grid placement.
/// Reuse an ID for replacement frames; use distinct IDs for simultaneous images.
/// This widget requires Kitty graphics *and Unicode placeholders*. Call `delete`
/// on removal; terminal IO is never performed implicitly from Drop.
pub struct KittyView {
    id: NonZeroU32,
    protocol: Protocol,
    cells: Size,
    upload: std::cell::RefCell<Option<String>>,
}
impl KittyView {
    pub fn new(id: NonZeroU32, frame: RgbaFrame, cells: Size) -> io::Result<Self> {
        Self::encode(id, frame, cells, false)
    }
    pub(crate) fn encode(
        id: NonZeroU32,
        frame: RgbaFrame,
        cells: Size,
        compressed: bool,
    ) -> io::Result<Self> {
        Self::encode_ref(id, &frame, cells, compressed)
    }
    pub(crate) fn encode_ref(
        id: NonZeroU32,
        frame: &RgbaFrame,
        cells: Size,
        compressed: bool,
    ) -> io::Result<Self> {
        // ratatui-image 11.1 Kitty placeholders have 297 row/column diacritics.
        if cells.width == 0 || cells.height == 0 || cells.width > 297 || cells.height > 297 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Kitty placement must have 1..=297 columns and rows",
            ));
        }
        let upload = encode::upload(frame, id.get(), cells, compressed)?;
        // Keep the pinned library's placeholder/color/diff semantics. Its tiny
        // reference upload is replaced below; no source image is cloned.
        let protocol = Kitty::new(DynamicImage::new_rgba8(1, 1), cells, id.get(), false, false)
            .map_err(io::Error::other)?;
        Ok(Self {
            id,
            protocol: Protocol::Kitty(protocol),
            cells,
            upload: std::cell::RefCell::new(Some(upload)),
        })
    }

    /// Delete this image's placements and stored pixel data, never other IDs.
    /// Direct transport only: multiplexer passthrough is not enabled here.
    pub fn delete(&self, output: &mut impl Write) -> io::Result<()> {
        delete_image(self.id, output)
    }
}

fn delete_image(id: NonZeroU32, output: &mut impl Write) -> io::Result<()> {
    write!(output, "\x1b_Ga=d,d=I,i={id},q=2;\x1b\\")?;
    output.flush()
}
impl Widget for &KittyView {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        Image::new(&self.protocol).render(area, buffer);
        if let Some(cell) = buffer.cell_mut((area.x, area.y)) {
            // ratatui-image 11.1 transmits U=1 without placement dimensions.
            // Bind its existing upload to our explicit grid; otherwise Kitty
            // uses intrinsic pixel size while hit testing uses the full grid.
            if let Some(start) = cell.symbol().find('\u{10eeee}') {
                let mut symbol = self.upload.borrow_mut().take().unwrap_or_default();
                symbol.push_str(&cell.symbol()[start..]);
                cell.set_symbol(&symbol);
            }
        }
        // ratatui-image 11.1 only annotates column zero. After a tile interrupts
        // a base-image row, Kitty cannot infer the resumed base column/row.
        // Explicit coordinates also survive Ratatui's sparse cursor jumps.
        let diacritics = kitty_diacritics();
        let extra = usize::from(self.id.get().to_be_bytes()[0]);
        for row in 0..area.height.min(self.cells.height) {
            for col in 0..area.width.min(self.cells.width) {
                if let Some(cell) = buffer.cell_mut((area.x + col, area.y + row)) {
                    if let Some(start) = cell.symbol().rfind('\u{10eeee}') {
                        let mut symbol = cell.symbol()[..start].to_string();
                        symbol.extend([
                            '\u{10eeee}',
                            diacritics[row as usize],
                            diacritics[col as usize],
                            diacritics[extra],
                        ]);
                        cell.set_symbol(&symbol);
                    }
                }
            }
        }
    }
}

// Obtain the pinned encoder's public wire alphabet once instead of copying its
// private 297-entry table. A 1-column reference render exposes every row mark.
fn kitty_diacritics() -> &'static [char; 297] {
    static TABLE: std::sync::OnceLock<[char; 297]> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let image = DynamicImage::ImageRgba8(RgbaImage::new(1, 1));
        let protocol = Protocol::Kitty(
            Kitty::new(image, Size::new(1, 297), 1, false, false).expect("valid reference image"),
        );
        let mut buffer = Buffer::empty(Rect::new(0, 0, 1, 297));
        Image::new(&protocol).render(buffer.area, &mut buffer);
        std::array::from_fn(|row| {
            buffer[(0, row as u16)]
                .symbol()
                .rsplit('\u{10eeee}')
                .next()
                .unwrap()
                .chars()
                .next()
                .expect("pinned Kitty row mark")
        })
    })
}

/// Initial GPU bridge. This intentionally renders only Element chrome; the
/// existing golden helper excludes text glyphs, textures and the full App loop.
#[cfg(feature = "jadix-gpu")]
pub fn render_element_chrome(
    root: &mut jadix::core::tree::Element,
    width: u32,
    height: u32,
) -> io::Result<RgbaFrame> {
    if width == 0
        || height == 0
        || u64::from(width) * u64::from(height) > (RgbaFrame::MAX_BYTES / 4) as u64
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid render dimensions",
        ));
    }
    let image =
        jadix::renderer::offscreen::render_element_scene(root, width, height, wgpu::Color::BLACK)
            .ok_or_else(|| io::Error::other("Jadix offscreen renderer unavailable"))?;
    if image.pixels.chunks_exact(4).any(|p| p[3] != 255) {
        return Err(io::Error::other(
            "GPU bridge requires an opaque composited frame",
        ));
    }
    RgbaFrame::new(image.width, image.height, image.pixels)
}

#[cfg(test)]
mod tests;
