//! Geometry and frame ownership for one terminal image region.
use crate::{KittyView, RgbaFrame};
use crossterm::event::{Event, KeyEvent, KeyModifiers, MouseEventKind};
use ratatui::layout::{Rect, Size};
use std::{io, io::Write, num::NonZeroU32, sync::Arc};

/// Kitty fits source pixels into `cells` preserving aspect ratio. Source pixels
/// must match that region's physical aspect ratio for pointer mapping to agree.
/// `current` reads real terminal geometry; `new` supports explicit host metrics.
/// `scale` is explicit, never inferred from terminal DPI.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerminalViewport {
    cells: Rect,
    pixels: [u32; 2],
    scale: f64,
}

impl TerminalViewport {
    /// Read pane dimensions and cell pixels from tmux; never use the enclosing
    /// client's full pixel area as the pane's pointer coordinate space.
    #[cfg(unix)]
    pub fn current_tmux(scale: f64) -> io::Result<Self> {
        let pane =
            std::env::var("TMUX_PANE").map_err(|_| io::Error::other("TMUX_PANE unavailable"))?;
        if !pane.starts_with('%')
            || pane.len() < 2
            || pane.len() > 32
            || !pane[1..].bytes().all(|b| b.is_ascii_digit())
        {
            return Err(io::Error::other("invalid TMUX_PANE"));
        }
        let result = std::process::Command::new("tmux")
            .args([
                "display-message",
                "-p",
                "-t",
                &pane,
                "#{pane_width} #{pane_height} #{window_cell_width} #{window_cell_height}",
            ])
            .output()?;
        let dimensions: Vec<u16> = String::from_utf8_lossy(&result.stdout)
            .split_whitespace()
            .filter_map(|n| n.parse().ok())
            .collect();
        if !result.status.success() || dimensions.len() != 4 || dimensions.contains(&0) {
            return Err(io::Error::other("tmux cell pixel dimensions unavailable"));
        }
        let [w, h, cw, ch] = dimensions[..] else {
            unreachable!()
        };
        Self::new(
            Rect::new(0, 0, w, h),
            [u32::from(w) * u32::from(cw), u32::from(h) * u32::from(ch)],
            scale,
        )
    }
    /// Read both cell and pixel dimensions without consuming input. A TTY/SSH
    /// peer reporting zero pixel dimensions cannot establish pointer geometry.
    pub fn current(scale: f64) -> io::Result<Self> {
        let size = crossterm::terminal::window_size()?;
        if size.width == 0 || size.height == 0 {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "terminal pixel dimensions unavailable",
            ));
        }
        Self::new(
            Rect::new(0, 0, size.columns, size.rows),
            [u32::from(size.width), u32::from(size.height)],
            scale,
        )
    }
    pub fn new(cells: Rect, pixels: [u32; 2], scale: f64) -> io::Result<Self> {
        if cells.is_empty()
            || pixels.contains(&0)
            || u64::from(pixels[0]) * u64::from(pixels[1]) > (RgbaFrame::MAX_BYTES / 4) as u64
            || !scale.is_finite()
            || scale <= 0.0
            || pixels.iter().any(|n| !(f64::from(*n) / scale).is_finite())
            || u32::from(cells.x) + u32::from(cells.width) > u32::from(u16::MAX)
            || u32::from(cells.y) + u32::from(cells.height) > u32::from(u16::MAX)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid terminal viewport",
            ));
        }
        Ok(Self {
            cells,
            pixels,
            scale,
        })
    }

    pub fn cells(self) -> Rect {
        self.cells
    }
    pub fn pixels(self) -> [u32; 2] {
        self.pixels
    }
    pub fn scale(self) -> f64 {
        self.scale
    }
    pub fn logical_size(self) -> [f64; 2] {
        self.pixels.map(|n| f64::from(n) / self.scale)
    }

    /// Crossterm supplies cell positions, not pixel-precision mouse reports.
    /// Use each cell's center, with half-open bounds; outside events stay outside.
    pub fn logical_position(self, column: u16, row: u16) -> Option<[f64; 2]> {
        if column < self.cells.x
            || row < self.cells.y
            || column >= self.cells.right()
            || row >= self.cells.bottom()
        {
            return None;
        }
        let size = self.logical_size();
        Some([
            ((f64::from(column - self.cells.x) + 0.5) / f64::from(self.cells.width)) * size[0],
            ((f64::from(row - self.cells.y) + 0.5) / f64::from(self.cells.height)) * size[1],
        ])
    }

    /// Normalize reported input without inventing IME, key-release or precision
    /// events that the terminal did not supply. Wheel kinds keep step semantics.
    pub fn input(self, event: Event) -> Option<TerminalInput> {
        TerminalInput::from_event(event, Some(self))
    }
}

impl TerminalInput {
    /// A hidden/empty region has no pointer target, but still receives lifecycle,
    /// keyboard and paste events. No synthetic one-cell viewport is created.
    pub fn from_event(event: Event, viewport: Option<TerminalViewport>) -> Option<Self> {
        Some(match event {
            Event::Key(key) => TerminalInput::Key(key),
            Event::Mouse(mouse) => TerminalInput::Pointer {
                kind: mouse.kind,
                logical: viewport?.logical_position(mouse.column, mouse.row)?,
                modifiers: mouse.modifiers,
            },
            Event::Paste(text) => TerminalInput::Paste(text),
            Event::FocusGained => TerminalInput::Focus(true),
            Event::FocusLost => TerminalInput::Focus(false),
            Event::Resize(width, height) => TerminalInput::Resize(Size::new(width, height)),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum TerminalInput {
    Key(KeyEvent),
    Pointer {
        kind: MouseEventKind,
        logical: [f64; 2],
        modifiers: KeyModifiers,
    },
    Paste(String),
    Focus(bool),
    Resize(Size),
}

/// A render request belongs to exactly one surface and revision. May cross a
/// worker boundary along with owned pixels; it owns no UI/GPU state.
#[derive(Clone, Debug)]
pub struct FrameTicket {
    owner: Arc<()>,
    revision: u64,
    viewport: TerminalViewport,
}
impl FrameTicket {
    pub fn viewport(&self) -> TerminalViewport {
        self.viewport
    }
}

/// One retained image and at most one pending source frame. New requests
/// invalidate earlier work, including across resize; visible pixels remain until
/// replacement except when geometry changes. This is source-side coalescing,
/// NOT a terminal display ACK or an asynchronous transport queue.
pub struct KittySurface {
    id: NonZeroU32,
    owner: Arc<()>,
    revision: u64,
    viewport: TerminalViewport,
    requested: bool,
    closed: bool,
    pending: Option<RgbaFrame>,
    view: Option<KittyView>,
    compressed: bool,
}

impl KittySurface {
    pub fn new(id: NonZeroU32, viewport: TerminalViewport) -> Self {
        Self {
            id,
            owner: Arc::new(()),
            revision: 0,
            viewport,
            requested: false,
            closed: false,
            pending: None,
            view: None,
            compressed: false,
        }
    }

    /// Call only for a real state/geometry change, not every terminal redraw.
    pub fn request_frame(&mut self, viewport: TerminalViewport) -> io::Result<FrameTicket> {
        if self.closed {
            return Err(io::Error::other("terminal surface closed"));
        }
        if viewport.cells.width > 297 || viewport.cells.height > 297 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Kitty placement must have at most 297 columns and rows",
            ));
        }
        self.revision = self
            .revision
            .checked_add(1)
            .ok_or_else(|| io::Error::other("terminal revision exhausted"))?;
        if self.viewport != viewport {
            self.view = None;
        }
        self.viewport = viewport;
        self.pending = None;
        self.requested = true;
        Ok(FrameTicket {
            owner: self.owner.clone(),
            revision: self.revision,
            viewport,
        })
    }

    /// False means stale, foreign, duplicate or closed; no newer state changes.
    /// A malformed current frame is an error and leaves the ticket retryable.
    pub fn submit(&mut self, ticket: &FrameTicket, frame: RgbaFrame) -> io::Result<bool> {
        if self.closed
            || !self.requested
            || !Arc::ptr_eq(&self.owner, &ticket.owner)
            || self.revision != ticket.revision
        {
            return Ok(false);
        }
        if frame.size() != (self.viewport.pixels[0], self.viewport.pixels[1]) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "frame dimensions differ from request",
            ));
        }
        self.pending = Some(frame);
        self.requested = false;
        Ok(true)
    }

    /// Encode only the latest accepted source, once. No IO or redraw happens here.
    pub fn prepare(&mut self) -> io::Result<bool> {
        let Some(frame) = self.pending.take() else {
            return Ok(false);
        };
        self.view = Some(KittyView::encode(
            self.id,
            frame,
            Size::new(self.viewport.cells.width, self.viewport.cells.height),
            self.compressed,
        )?);
        Ok(true)
    }

    pub fn viewport(&self) -> TerminalViewport {
        self.viewport
    }
    pub(crate) fn set_compressed(&mut self, compressed: bool) {
        self.compressed = compressed;
    }
    pub fn view(&self) -> Option<&KittyView> {
        self.view.as_ref()
    }

    /// Invalidate callbacks before IO, even when cleanup fails. May be called
    /// again to retry deletion; cannot reopen the disposed surface.
    pub fn close(&mut self, output: &mut impl Write) -> io::Result<()> {
        self.closed = true;
        self.requested = false;
        self.pending = None;
        self.view = None;
        crate::delete_image(self.id, output)
    }
}
