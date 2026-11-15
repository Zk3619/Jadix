//! Actual Ratatui/Kitty session: probe, credit, dirty coalescing and cleanup.
#[cfg(unix)]
use crate::session_unix as output_session;
#[cfg(windows)]
use crate::session_windows as output_session;
use crate::{KittyFrameFlow, KittyReply, KittySurface, RgbaFrame, TerminalViewport};
use ratatui::{backend::CrosstermBackend, layout::Size, Terminal, TerminalOptions, Viewport};
use std::{
    cell::RefCell,
    io::{self, Write},
    num::NonZeroU32,
    rc::Rc,
    time::{Duration, Instant},
};

struct Output<W>(Rc<RefCell<crate::TmuxPassthrough<W>>>);
impl<W> Clone for Output<W> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<W: Write> Write for Output<W> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.0.borrow_mut().write(b)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.borrow_mut().flush()
    }
}

/// One image region driven by real replies. Every image upload in this host is
/// fenced; blocked frames retain only dirty state, without rendering/encoding.
/// Call `close` on all exit paths. Keep one input owner and pass its Graphics
/// events to `acknowledge`; no private Crossterm parser is used for raw VT.
pub struct KittyTerminal<W: Write> {
    output: Output<W>,
    terminal: Terminal<CrosstermBackend<Output<W>>>,
    surface: KittySurface,
    viewport: TerminalViewport,
    flow: KittyFrameFlow,
    probe: Option<NonZeroU32>,
    ready: bool,
    dirty: bool,
    closed: bool,
    submitted: u64,
    compression_requested: bool,
    probing_compression: bool,
    compressed: bool,
    image_id: NonZeroU32,
    first_fence: u32,
    tiles: Option<crate::tiles::KittyTiles>,
}
impl<W: Write> KittyTerminal<W> {
    pub fn new(
        output: W,
        viewport: TerminalViewport,
        image_id: NonZeroU32,
        first_fence: NonZeroU32,
        timeout: Duration,
    ) -> io::Result<Self> {
        if image_id.get() >= first_fence.get() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "image ID must precede reserved fence ID range",
            ));
        }
        let output = Output(Rc::new(RefCell::new(crate::TmuxPassthrough::direct(
            output,
        ))));
        let terminal = Terminal::with_options(
            CrosstermBackend::new(output.clone()),
            TerminalOptions {
                viewport: Viewport::Fixed(viewport.cells()),
            },
        )?;
        Ok(Self {
            output,
            terminal,
            surface: KittySurface::new(image_id, viewport),
            viewport,
            flow: KittyFrameFlow::new(first_fence, timeout)?,
            probe: None,
            ready: false,
            dirty: true,
            closed: false,
            submitted: 0,
            compression_requested: false,
            probing_compression: false,
            compressed: false,
            image_id,
            first_fence: first_fence.get(),
            tiles: None,
        })
    }
    pub fn start(&mut self, now: Instant) -> io::Result<()> {
        if self.closed || self.probe.is_some() || self.ready {
            return Err(io::Error::other("terminal probe already started or closed"));
        }
        self.probe = Some(self.flow.submit(now, &mut self.output, |_| Ok(()))?);
        Ok(())
    }
    /// Explicit opt-in. The surrounding tmux server must allow passthrough;
    /// the ordinary Kitty probe still must succeed before any image is sent.
    pub fn request_tmux_passthrough(&mut self) -> io::Result<()> {
        self.request_tmux_passthrough_depth(1)
    }
    /// Explicit nested multiplexer depth; every surrounding server must allow
    /// passthrough. This does not infer nesting from the TMUX environment.
    pub fn request_tmux_passthrough_depth(&mut self, depth: u8) -> io::Result<()> {
        if self.closed || self.ready || self.probe.is_some() {
            return Err(io::Error::other("configure tmux before start"));
        }
        self.output.0.borrow_mut().enable(depth)?;
        Ok(())
    }
    /// Opt in before start. A rejected zlib probe falls back to raw RGBA;
    /// silence still fails the session deadline instead of assuming support.
    pub fn request_compression(&mut self) -> io::Result<()> {
        if self.closed || self.ready || self.probe.is_some() {
            return Err(io::Error::other("configure compression before start"));
        }
        self.compression_requested = true;
        Ok(())
    }
    pub fn compressed(&self) -> bool {
        self.compressed
    }
    /// Opt in before start. At most 256 fixed IDs, within the reserved image
    /// range. Sparse damage updates tiles, dense damage sends one keyframe;
    /// identical pixels write nothing. One fence covers the whole batch.
    pub fn request_tiling(&mut self, cells: Size) -> io::Result<()> {
        if self.closed || self.ready || self.probe.is_some() {
            return Err(io::Error::other("configure tiling before start"));
        }
        let first = NonZeroU32::new(
            self.image_id
                .get()
                .checked_add(1)
                .ok_or_else(|| io::Error::other("image IDs exhausted"))?,
        )
        .unwrap();
        self.tiles = Some(crate::tiles::KittyTiles::new(
            first,
            self.first_fence,
            self.viewport,
            cells,
        )?);
        Ok(())
    }
    pub fn damage_metrics(&self) -> crate::DamageMetrics {
        self.tiles
            .as_ref()
            .map_or(crate::DamageMetrics::default(), |tiles| tiles.metrics())
    }
    pub fn acknowledge(&mut self, reply: &KittyReply, now: Instant) -> io::Result<bool> {
        if self.closed {
            return Ok(false);
        }
        let accepted = self.flow.acknowledge(reply, now)?;
        if accepted && self.probe == Some(reply.id) {
            if self.compression_requested && !self.probing_compression {
                self.probe = Some(self.flow.probe_zlib(now, &mut self.output)?);
                self.probing_compression = true;
                return Ok(true);
            }
            self.compressed = self.probing_compression && reply.message == "OK";
            self.surface.set_compressed(self.compressed);
            if let Some(tiles) = &mut self.tiles {
                tiles.set_compressed(self.compressed);
            }
            self.ready = true;
            self.probe = None;
        }
        Ok(accepted)
    }
    pub fn invalidate(&mut self) {
        if !self.closed {
            self.dirty = true;
        }
    }
    pub fn ready(&self) -> bool {
        self.ready && !self.closed
    }
    pub fn in_flight(&self) -> usize {
        self.flow.in_flight()
    }
    pub fn submitted_frames(&self) -> u64 {
        self.submitted
    }
    pub fn viewport(&self) -> TerminalViewport {
        self.viewport
    }
    pub fn deadline(&self) -> Option<Instant> {
        self.flow.deadline()
    }
    pub fn check(&mut self, now: Instant) -> io::Result<()> {
        self.flow.check(now)
    }
    pub fn resize(&mut self, viewport: TerminalViewport) -> io::Result<()> {
        if self.closed {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        if self.viewport != viewport {
            if let Some(tiles) = &mut self.tiles {
                tiles.resize(viewport)?;
            } else {
                self.surface.request_frame(viewport)?;
            }
            self.terminal.resize(viewport.cells())?;
            self.viewport = viewport;
            self.dirty = true;
        }
        Ok(())
    }
    pub fn present(
        &mut self,
        now: Instant,
        render: impl FnOnce(TerminalViewport) -> io::Result<RgbaFrame>,
    ) -> io::Result<bool> {
        if self.closed {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        self.flow.check(now)?;
        if !self.ready || !self.dirty || self.flow.in_flight() == 2 {
            return Ok(false);
        }
        // Preparing may fail (e.g. asynchronously loading texture). Preserve dirty
        // state and let the host choose its resource retry deadline.
        let frame = render(self.viewport)?;
        if let Some(tiles) = &mut self.tiles {
            if !tiles.prepare(frame)? {
                self.dirty = false;
                return Ok(false);
            }
        } else {
            let ticket = self.surface.request_frame(self.viewport)?;
            self.surface.submit(&ticket, frame)?;
            self.surface.prepare()?;
        }
        self.flow
            .submit(Instant::now().max(now), &mut self.output, |output| {
                if let Some(tiles) = &mut self.tiles {
                    tiles.cleanup(output)?;
                    self.terminal
                        .draw(|f| tiles.draw(f.buffer_mut()))
                        .map(|_| ())
                } else {
                    self.terminal
                        .draw(|f| {
                            f.render_widget(self.surface.view().unwrap(), self.viewport.cells())
                        })
                        .map(|_| ())
                }
            })?;
        self.dirty = false;
        self.submitted += 1;
        Ok(true)
    }
    pub fn close(&mut self) -> io::Result<()> {
        self.closed = true;
        self.ready = false;
        self.dirty = false;
        // Abort a fragmented APC held by the optional tmux wrapper as well as
        // any control string already delivered to the enclosing terminal.
        let abort = self.output.write_all(b"\x18\x1b\\");
        // Restore before image teardown while the session writer is live.
        // Ratatui's Drop must not write through the later closed session.
        let cursor = self.terminal.show_cursor();
        let tiles = self
            .tiles
            .as_mut()
            .map_or(Ok(()), |tiles| tiles.close(&mut self.output));
        let full = self.surface.close(&mut self.output);
        full.and(tiles).and(cursor).and(abort)
    }
}
impl<W: Write> Drop for KittyTerminal<W> {
    fn drop(&mut self) {
        if !self.closed {
            let _ = self.close();
        }
    }
}

/// Alternate screen / raw / mouse / paste / focus / keyboard mode ownership.
/// Drop restores modes during normal errors and panic unwind. Strong kill cannot
/// execute cleanup. Graphics image lifetime is cleaned by its host separately.
pub struct TerminalModes {
    active: bool,
    #[cfg(any(unix, windows))]
    session: Rc<output_session::Session>,
    #[cfg(windows)]
    output_mode: Option<u32>,
    #[cfg(windows)]
    input_mode: Option<u32>,
    #[cfg(windows)]
    output_code_page: Option<u32>,
}
impl TerminalModes {
    pub fn enter() -> io::Result<Self> {
        Self::enter_with_output_timeout(Duration::from_secs(10))
    }
    pub fn enter_with_output_timeout(timeout: Duration) -> io::Result<Self> {
        if timeout.is_zero() || timeout > Duration::from_secs(3600) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "output timeout must be positive and at most one hour",
            ));
        }
        use std::io::IsTerminal;
        if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
            return Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "interactive terminal input/output is closed or redirected",
            ));
        }
        #[cfg(any(unix, windows))]
        let session = output_session::Session::new(timeout)?;
        #[cfg(not(any(unix, windows)))]
        let _ = timeout;
        #[cfg(windows)]
        let input_mode = {
            use std::os::windows::io::AsRawHandle;
            let mut mode = 0;
            // Crossterm's Windows disable_raw_mode sets conventional flags;
            // it does not restore every preexisting Console input flag.
            // SAFETY: borrowed Console handle and initialized mode output.
            if unsafe {
                windows_sys::Win32::System::Console::GetConsoleMode(
                    io::stdin().as_raw_handle(),
                    &mut mode,
                )
            } == 0
            {
                return Err(io::Error::last_os_error());
            }
            Some(mode)
        };
        crossterm::terminal::enable_raw_mode()?;
        let mut guard = Self {
            active: true,
            #[cfg(any(unix, windows))]
            session,
            #[cfg(windows)]
            output_mode: None,
            #[cfg(windows)]
            input_mode,
            #[cfg(windows)]
            output_code_page: None,
        };
        guard.configure_console()?;
        // Raw VT is intentional: Crossterm's Windows MouseCapture command
        // changes console mode bits and would overwrite raw input configuration.
        let mut output = guard.output();
        output.write_all(
            b"\x1b[?1049h\x1b[?25l\x1b[?1003h\x1b[?1006h\x1b[?2004h\x1b[?1004h\x1b[>3u",
        )?;
        output.flush()?;
        Ok(guard)
    }
    /// Use this writer for interactive output; ordinary stdout can still block
    /// if an application bypasses this session. Stdio peers use `stdio()`.
    pub fn output(&self) -> TerminalOutput {
        TerminalOutput {
            #[cfg(any(unix, windows))]
            buffered: Some(std::io::BufWriter::with_capacity(
                16 * 1024,
                SessionWriter(self.session.clone()),
            )),
            #[cfg(any(unix, windows))]
            cleaning: false,
        }
    }
    /// One-way teardown phase. Discard pending buffered frame bytes;
    /// subsequent output attempts write immediately without polling/waiting.
    /// Stop rendering first, close owned graphics, then call `restore` even if
    /// this best-effort control-string abort or graphics cleanup fails.
    pub fn begin_output_cleanup(&self) -> io::Result<()> {
        #[cfg(any(unix, windows))]
        {
            self.session.begin_cleanup()
        }
        #[cfg(not(any(unix, windows)))]
        {
            Ok(())
        }
    }
    pub fn exit_signal(&self) -> Option<i32> {
        #[cfg(any(unix, windows))]
        {
            self.session.signal()
        }
        #[cfg(not(any(unix, windows)))]
        {
            None
        }
    }
    /// Successful OS write bytes, including mode setup/cleanup. Unlike a
    /// meter above BufWriter, this excludes unsent buffered bytes after errors.
    pub fn written_bytes(&self) -> Option<u64> {
        #[cfg(any(unix, windows))]
        {
            Some(self.session.written_bytes())
        }
        #[cfg(not(any(unix, windows)))]
        {
            None
        }
    }
    /// Process-owned terminal termination intent, for cooperative workers.
    /// No callback, IO or cancellation work executes in the signal handler.
    pub fn shutdown_requested() -> bool {
        #[cfg(any(unix, windows))]
        {
            output_session::exit_signal().is_some()
        }
        #[cfg(not(any(unix, windows)))]
        {
            false
        }
    }
    fn configure_console(&mut self) -> io::Result<()> {
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::System::Console::*;
            // SAFETY: query/change only this console's code page; restore saves
            // the original even when a later write or configuration step fails.
            self.output_code_page = Some(unsafe { GetConsoleOutputCP() });
            // SAFETY: UTF-8 is required by our byte-oriented Console writer.
            if unsafe { SetConsoleOutputCP(65001) } == 0 {
                return Err(io::Error::last_os_error());
            }
            let mut output = 0;
            // SAFETY: console handles are borrowed; mode output is initialized.
            unsafe {
                let handle = io::stdout().as_raw_handle();
                if GetConsoleMode(handle, &mut output) == 0 {
                    return Err(io::Error::last_os_error());
                }
                self.output_mode = Some(output);
                if SetConsoleMode(handle, output | ENABLE_VIRTUAL_TERMINAL_PROCESSING) == 0 {
                    return Err(io::Error::last_os_error());
                }
                let handle = io::stdin().as_raw_handle();
                let mut input = 0;
                if GetConsoleMode(handle, &mut input) == 0
                    || SetConsoleMode(
                        handle,
                        (input | ENABLE_WINDOW_INPUT | ENABLE_MOUSE_INPUT | ENABLE_EXTENDED_FLAGS)
                            & !ENABLE_QUICK_EDIT_MODE,
                    ) == 0
                {
                    return Err(io::Error::last_os_error());
                }
            }
        }
        Ok(())
    }
    pub fn restore(&mut self) -> io::Result<()> {
        if !self.active {
            #[cfg(windows)]
            return self.session.console_restore_status();
            #[cfg(not(windows))]
            return Ok(());
        }
        self.active = false;
        let bytes =
            b"\x18\x1b\\\x1b[<u\x1b[?1004l\x1b[?2004l\x1b[?1006l\x1b[?1003l\x1b[?25h\x1b[?1049l";
        #[cfg(any(unix, windows))]
        let write = self.session.cleanup(bytes);
        #[cfg(not(any(unix, windows)))]
        let write = io::stdout().write_all(bytes);
        #[cfg(not(windows))]
        let flush = io::stdout().flush();
        #[cfg(windows)]
        let flush: io::Result<()> = Ok(());
        #[cfg(not(windows))]
        let raw = crossterm::terminal::disable_raw_mode();
        #[cfg(unix)]
        let session = self.session.finish();
        #[cfg(windows)]
        let console = self.session.restore_console(
            self.input_mode.take(),
            self.output_mode.take(),
            self.output_code_page.take(),
        );
        write?;
        flush?;
        #[cfg(not(windows))]
        raw?;
        #[cfg(unix)]
        session?;
        #[cfg(windows)]
        console?;
        Ok(())
    }
}

/// The interactive session's buffered, cancellable Unix output. `stdio()`
/// deliberately keeps ordinary stdout semantics for controlled pipe peers.
/// Windows uses an owned synchronous-IO worker with native cancellation.
/// Non-cancellable drivers are reported separately; stdio keeps plain stdout.
pub struct TerminalOutput {
    #[cfg(any(unix, windows))]
    buffered: Option<std::io::BufWriter<SessionWriter>>,
    #[cfg(any(unix, windows))]
    cleaning: bool,
}
#[cfg(any(unix, windows))]
struct SessionWriter(Rc<output_session::Session>);
#[cfg(any(unix, windows))]
impl Write for SessionWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl TerminalOutput {
    /// Shared cancellation intent; cancelling invalidates ordinary frame output.
    /// Cleanup remains best effort and bounded independently. None for stdio.
    pub fn cancellation(&self) -> Option<crate::CancellationToken> {
        #[cfg(any(unix, windows))]
        {
            self.buffered.as_ref().map(|b| b.get_ref().0.cancellation())
        }
        #[cfg(not(any(unix, windows)))]
        {
            None
        }
    }
    pub fn stdio() -> Self {
        Self {
            #[cfg(any(unix, windows))]
            buffered: None,
            #[cfg(any(unix, windows))]
            cleaning: false,
        }
    }
    #[cfg(any(unix, windows))]
    fn prepare_cleanup(&mut self) {
        if !self.cleaning
            && self
                .buffered
                .as_ref()
                .is_some_and(|b| b.get_ref().0.is_cleaning())
        {
            // into_parts never flushes a partially emitted/failed frame.
            let (writer, _) = self.buffered.take().unwrap().into_parts();
            self.buffered = Some(std::io::BufWriter::with_capacity(16 * 1024, writer));
            self.cleaning = true;
        }
    }
}
impl Write for TerminalOutput {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        #[cfg(any(unix, windows))]
        self.prepare_cleanup();
        #[cfg(any(unix, windows))]
        if let Some(buffered) = &mut self.buffered {
            buffered.get_ref().0.check_write()?;
            if self.cleaning {
                return buffered.get_ref().0.write(bytes);
            }
            return buffered.write(bytes);
        }
        io::stdout().write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        #[cfg(any(unix, windows))]
        self.prepare_cleanup();
        #[cfg(any(unix, windows))]
        if let Some(buffered) = &mut self.buffered {
            buffered.get_ref().0.check_write()?;
            if self.cleaning {
                return Ok(());
            }
            return buffered.flush();
        }
        io::stdout().flush()
    }
}
impl Drop for TerminalModes {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}
