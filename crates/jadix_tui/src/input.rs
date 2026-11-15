//! One input owner for VT keys, paste, mouse and Kitty responses.
use crate::{KittyReply, KittyReplyDecoder, TerminalPacket};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use std::{
    collections::VecDeque,
    io::{self, IsTerminal, Read},
    time::{Duration, Instant},
};

#[derive(Debug, PartialEq, Eq)]
pub enum WireEvent {
    Input(Event),
    Graphics(KittyReply),
    /// Unsupported sequences are retained, not turned into application text.
    Unknown(Vec<u8>),
}

/// Bounded incremental VT decoder. Paste is literal: graphics-looking content
/// inside a bracketed paste can never acknowledge a frame. Escape ambiguity is
/// resolved by the input owner after 40ms, never by a competing stdin reader.
/// Once a packet prefix or UTF-8 character has started, that ambiguity deadline
/// must not split it: late tails remain control data, not application text.
/// Incomplete packets retain their existing byte bounds; the input owner fails
/// closed after 2s without a complete packet. Bracketed paste keeps its own bound.
#[derive(Default)]
pub struct InputDecoder {
    buffer: Vec<u8>,
    paste: Option<Vec<u8>>,
    packet_serial: u64,
}
impl InputDecoder {
    pub const MAX_PASTE_BYTES: usize = 1024 * 1024;

    pub fn is_pending(&self) -> bool {
        !self.buffer.is_empty() || self.paste.is_some()
    }

    pub fn feed(&mut self, bytes: &[u8]) -> io::Result<Vec<WireEvent>> {
        let mut events = Vec::new();
        for byte in bytes {
            if self.buffer.is_empty() && self.paste.is_none() {
                self.packet_serial = self.packet_serial.wrapping_add(1);
            }
            self.buffer.push(*byte);
            loop {
                if let Some(paste) = &mut self.paste {
                    let end = b"\x1b[201~";
                    if self.buffer == end {
                        let text = String::from_utf8(self.paste.take().unwrap())
                            .map_err(|_| invalid("invalid UTF-8 paste"))?;
                        events.push(WireEvent::Input(Event::Paste(text)));
                        self.buffer.clear();
                        continue;
                    }
                    if end.starts_with(&self.buffer) {
                        break;
                    }
                    paste.push(self.buffer.remove(0));
                    if paste.len() > Self::MAX_PASTE_BYTES {
                        return Err(invalid("paste exceeds 1 MiB"));
                    }
                    if self.buffer.is_empty() {
                        break;
                    }
                    continue;
                }
                if self.buffer.is_empty() {
                    break;
                }
                if self.buffer[0] != 27 {
                    let Some((code, n)) = plain(&self.buffer)? else {
                        break;
                    };
                    self.buffer.drain(..n);
                    events.push(key(code.0, code.1, KeyEventKind::Press));
                    continue;
                }
                if self.buffer.len() == 1 {
                    break;
                }
                if matches!(self.buffer[1], b'_' | b']' | b'P' | b'^' | b'X') {
                    if self.buffer.ends_with(b"\x1b\\")
                        || self.buffer[1] == b']' && self.buffer.last() == Some(&7)
                    {
                        if self.buffer[1] == b'_' {
                            let mut decoder = KittyReplyDecoder::default();
                            for packet in decoder.feed(&self.buffer) {
                                events.push(match packet {
                                    TerminalPacket::Graphics(r) => WireEvent::Graphics(r),
                                    TerminalPacket::Input(b) => WireEvent::Unknown(b),
                                });
                            }
                        } else {
                            events.push(WireEvent::Unknown(std::mem::take(&mut self.buffer)));
                        }
                        self.buffer.clear();
                        continue;
                    }
                    if self.buffer.len() > 1024 {
                        return Err(invalid("oversized terminal control string"));
                    }
                    break;
                }
                if self.buffer.starts_with(b"\x1b[") || self.buffer.starts_with(b"\x1bO") {
                    if self.buffer.len() < 3 {
                        break;
                    }
                    let last = *self.buffer.last().unwrap();
                    if !(0x40..=0x7e).contains(&last) {
                        if self.buffer.len() > 128 {
                            return Err(invalid("oversized CSI input"));
                        }
                        break;
                    }
                    let sequence = std::mem::take(&mut self.buffer);
                    if sequence == b"\x1b[200~" {
                        self.paste = Some(Vec::new());
                        continue;
                    }
                    events.push(parse_sequence(&sequence).unwrap_or(WireEvent::Unknown(sequence)));
                    continue;
                }
                if self.buffer[1] == 27 {
                    self.buffer.remove(0);
                    events.push(key(KeyCode::Esc, KeyModifiers::NONE, KeyEventKind::Press));
                    continue;
                }
                let Some(((code, mods), n)) = plain(&self.buffer[1..])? else {
                    break;
                };
                self.buffer.drain(..n + 1);
                events.push(key(code, mods | KeyModifiers::ALT, KeyEventKind::Press));
            }
        }
        Ok(events)
    }

    fn escape_pending(&self) -> bool {
        self.paste.is_none() && self.buffer == b"\x1b"
    }

    /// Resolve only a standalone Escape after the host's ambiguity deadline.
    /// Complete packets are emitted by `feed`; partial control/UTF-8 data stays
    /// buffered even if a query has timed out. It must never become typed text.
    pub fn flush_escape(&mut self) -> Vec<WireEvent> {
        if self.escape_pending() {
            self.buffer.clear();
            vec![key(KeyCode::Esc, KeyModifiers::NONE, KeyEventKind::Press)]
        } else {
            Vec::new()
        }
    }

    /// Windows console records expose terminal responses as ordinary character
    /// key records. Only a pending escape sequence is reassembled; other native
    /// records keep their reported modifiers/repeat/release and resize/focus.
    pub fn feed_native(&mut self, event: Event) -> io::Result<Vec<WireEvent>> {
        if let Event::Key(k) = event {
            if k.kind == KeyEventKind::Release && self.is_pending() {
                return Ok(Vec::new());
            }
            if k.kind != KeyEventKind::Release && k.code == KeyCode::Esc && k.modifiers.is_empty() {
                return self.feed(b"\x1b");
            }
            if self.is_pending() && k.kind != KeyEventKind::Release && k.modifiers.is_empty() {
                let mut encoded = [0; 4];
                match k.code {
                    KeyCode::Char(c) => return self.feed(c.encode_utf8(&mut encoded).as_bytes()),
                    KeyCode::Enter => return self.feed(b"\r"),
                    KeyCode::Tab => return self.feed(b"\t"),
                    KeyCode::Backspace => return self.feed(b"\x7f"),
                    _ => {}
                }
            }
        }
        let mut events = self.flush_escape();
        events.push(WireEvent::Input(event));
        Ok(events)
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn key(code: KeyCode, modifiers: KeyModifiers, kind: KeyEventKind) -> WireEvent {
    WireEvent::Input(Event::Key(KeyEvent::new_with_kind(code, modifiers, kind)))
}
fn plain(bytes: &[u8]) -> io::Result<Option<((KeyCode, KeyModifiers), usize)>> {
    let b = bytes[0];
    let code = match b {
        0 => KeyCode::Null,
        9 => KeyCode::Tab,
        10 | 13 => KeyCode::Enter,
        8 | 127 => KeyCode::Backspace,
        1..=26 => {
            return Ok(Some((
                (KeyCode::Char((b + b'a' - 1) as char), KeyModifiers::CONTROL),
                1,
            )))
        }
        _ => {
            let len = if b < 128 {
                1
            } else if b < 224 {
                2
            } else if b < 240 {
                3
            } else {
                4
            };
            if bytes.len() < len {
                return Ok(None);
            }
            let c = std::str::from_utf8(&bytes[..len])
                .map_err(|_| invalid("invalid UTF-8 input"))?
                .chars()
                .next()
                .unwrap();
            return Ok(Some(((KeyCode::Char(c), KeyModifiers::NONE), len)));
        }
    };
    Ok(Some(((code, KeyModifiers::NONE), 1)))
}
fn modifiers(number: u32) -> Option<KeyModifiers> {
    let bits = number.checked_sub(1)?;
    if bits > 255 {
        return None;
    }
    let mut mods = KeyModifiers::NONE;
    for (bit, m) in [
        (1, KeyModifiers::SHIFT),
        (2, KeyModifiers::ALT),
        (4, KeyModifiers::CONTROL),
        (8, KeyModifiers::SUPER),
        (16, KeyModifiers::HYPER),
        (32, KeyModifiers::META),
    ] {
        if bits & bit != 0 {
            mods |= m;
        }
    }
    Some(mods)
}
fn parse_sequence(bytes: &[u8]) -> Option<WireEvent> {
    let last = *bytes.last()?;
    let body = std::str::from_utf8(&bytes[2..bytes.len() - 1]).ok()?;
    if bytes.starts_with(b"\x1b[") {
        if body.is_empty() && (last == b'I' || last == b'O') {
            return Some(WireEvent::Input(if last == b'I' {
                Event::FocusGained
            } else {
                Event::FocusLost
            }));
        }
        if let Some(mouse) = body.strip_prefix('<') {
            let values: Vec<u16> = mouse
                .split(';')
                .map(str::parse)
                .collect::<Result<_, _>>()
                .ok()?;
            let [b, x, y] = values.as_slice() else {
                return None;
            };
            if *b > 127 || b & 3 == 3 && b & 96 == 0 {
                return None;
            }
            let mut mods = KeyModifiers::NONE;
            if b & 4 != 0 {
                mods |= KeyModifiers::SHIFT;
            }
            if b & 8 != 0 {
                mods |= KeyModifiers::ALT;
            }
            if b & 16 != 0 {
                mods |= KeyModifiers::CONTROL;
            }
            let button = match b & 3 {
                0 => MouseButton::Left,
                1 => MouseButton::Middle,
                2 => MouseButton::Right,
                _ => MouseButton::Left,
            };
            let kind = if b & 64 != 0 {
                match b & 3 {
                    0 => MouseEventKind::ScrollUp,
                    1 => MouseEventKind::ScrollDown,
                    2 => MouseEventKind::ScrollLeft,
                    _ => MouseEventKind::ScrollRight,
                }
            } else if last == b'm' {
                MouseEventKind::Up(button)
            } else if last != b'M' {
                return None;
            } else if b & 32 != 0 {
                if b & 3 == 3 {
                    MouseEventKind::Moved
                } else {
                    MouseEventKind::Drag(button)
                }
            } else {
                MouseEventKind::Down(button)
            };
            return Some(WireEvent::Input(Event::Mouse(MouseEvent {
                kind,
                column: x.checked_sub(1)?,
                row: y.checked_sub(1)?,
                modifiers: mods,
            })));
        }
        // Explicit xterm window-size report, used by pipe/PTY peers. Native
        // terminal resize is also supplied by the input owner below.
        if last == b't' {
            let v: Vec<u16> = body
                .split(';')
                .map(str::parse)
                .collect::<Result<_, _>>()
                .ok()?;
            if let [8, h, w] = v.as_slice() {
                return Some(WireEvent::Input(Event::Resize(*w, *h)));
            }
            return None;
        }
    }
    let parts: Vec<&str> = body.split(';').collect();
    let mut kind = KeyEventKind::Press;
    let mods = if let Some(part) = parts.get(1) {
        let mut fields = part.split(':');
        let mods = modifiers(fields.next()?.parse().ok()?)?;
        if let Some(event) = fields.next() {
            kind = match event {
                "1" => KeyEventKind::Press,
                "2" => KeyEventKind::Repeat,
                "3" => KeyEventKind::Release,
                _ => return None,
            };
        }
        mods
    } else {
        KeyModifiers::NONE
    };
    let code = match last {
        b'A' => KeyCode::Up,
        b'B' => KeyCode::Down,
        b'C' => KeyCode::Right,
        b'D' => KeyCode::Left,
        b'H' => KeyCode::Home,
        b'F' => KeyCode::End,
        b'Z' => return Some(key(KeyCode::BackTab, mods | KeyModifiers::SHIFT, kind)),
        b'P' => KeyCode::F(1),
        b'Q' => KeyCode::F(2),
        b'R' => KeyCode::F(3),
        b'S' => KeyCode::F(4),
        b'~' => match parts[0] {
            "1" | "7" => KeyCode::Home,
            "2" => KeyCode::Insert,
            "3" => KeyCode::Delete,
            "4" | "8" => KeyCode::End,
            "5" => KeyCode::PageUp,
            "6" => KeyCode::PageDown,
            "11" => KeyCode::F(1),
            "12" => KeyCode::F(2),
            "13" => KeyCode::F(3),
            "14" => KeyCode::F(4),
            "15" => KeyCode::F(5),
            "17" => KeyCode::F(6),
            "18" => KeyCode::F(7),
            "19" => KeyCode::F(8),
            "20" => KeyCode::F(9),
            "21" => KeyCode::F(10),
            "23" => KeyCode::F(11),
            "24" => KeyCode::F(12),
            _ => return None,
        },
        b'u' => match parts[0].split(':').next()?.parse::<u32>().ok()? {
            13 => KeyCode::Enter,
            27 => KeyCode::Esc,
            9 => KeyCode::Tab,
            127 => KeyCode::Backspace,
            c @ 57364..=57398 => KeyCode::F((c - 57364 + 1) as u8),
            57344..=63743 => return None,
            c => KeyCode::Char(char::from_u32(c)?),
        },
        _ => return None,
    };
    // Like Crossterm's CSI-u mapping, honor an explicitly reported shifted
    // codepoint; never guess a keyboard layout from the unshifted character.
    let code = if last == b'u' && mods.contains(KeyModifiers::SHIFT) {
        parts[0]
            .split(':')
            .nth(1)
            .and_then(|c| c.parse::<u32>().ok())
            .and_then(char::from_u32)
            .map_or(code, KeyCode::Char)
    } else {
        code
    };
    let mut result = key(code, mods, kind);
    if let (b'u', Some(mods), WireEvent::Input(Event::Key(event))) =
        (last, parts.get(1), &mut result)
    {
        let bits = mods
            .split(':')
            .next()?
            .parse::<u32>()
            .ok()?
            .checked_sub(1)?;
        if bits & 64 != 0 {
            event.state |= crossterm::event::KeyEventState::CAPS_LOCK;
        }
        if bits & 128 != 0 {
            event.state |= crossterm::event::KeyEventState::NUM_LOCK;
        }
    }
    Some(result)
}

/// The sole stdin reader. Windows interactive consoles keep native key/mouse
/// records; Unix and redirected pipe sessions read VT bytes. No helper thread
/// remains blocked after cleanup. `None` from poll means a deadline, never EOF.
pub struct StdinEvents {
    decoder: InputDecoder,
    pending: VecDeque<WireEvent>,
    input_deadline: Option<(Instant, u64, bool)>,
    native: bool,
    size: Option<(u16, u16)>,
    task_turn: bool,
}
impl StdinEvents {
    pub fn new() -> io::Result<Self> {
        let native = std::io::stdin().is_terminal();
        Ok(Self {
            decoder: InputDecoder::default(),
            pending: VecDeque::new(),
            input_deadline: None,
            native,
            task_turn: true,
            size: if native {
                crossterm::terminal::size().ok()
            } else {
                None
            },
        })
    }

    pub fn poll(&mut self, timeout: Duration) -> io::Result<Option<WireEvent>> {
        self.poll_inner(timeout, None)
    }

    /// Wait on stdin and a bounded mailbox using the same input owner. Alternate
    /// ready input/tasks so a producer burst cannot starve keyboard or replies.
    pub fn poll_with<T: Send + 'static>(
        &mut self,
        mailbox: &crate::Mailbox<T>,
        timeout: Duration,
    ) -> io::Result<Option<crate::HostEvent<T>>> {
        if self.task_turn {
            if let Some(task) = mailbox.try_recv() {
                self.task_turn = false;
                return Ok(Some(crate::HostEvent::Task(task)));
            }
        }
        if let Some(event) = self.poll_inner(Duration::ZERO, Some(mailbox.wake()))? {
            self.task_turn = true;
            return Ok(Some(crate::HostEvent::Input(event)));
        }
        if let Some(task) = mailbox.try_recv() {
            self.task_turn = false;
            return Ok(Some(crate::HostEvent::Task(task)));
        }
        if mailbox.is_closed() {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        if let Some(event) = self.poll_inner(timeout, Some(mailbox.wake()))? {
            self.task_turn = true;
            return Ok(Some(crate::HostEvent::Input(event)));
        }
        self.task_turn = false;
        if mailbox.is_closed() {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        Ok(mailbox.try_recv().map(crate::HostEvent::Task))
    }

    fn poll_inner(
        &mut self,
        timeout: Duration,
        wake: Option<&crate::wake::Wake>,
    ) -> io::Result<Option<WireEvent>> {
        #[cfg(unix)]
        if crate::session_unix::exit_signal().is_some() {
            return Err(io::ErrorKind::Interrupted.into());
        }
        if let Some(event) = self.pending.pop_front() {
            return Ok(Some(event));
        }
        let end = Instant::now() + timeout;
        loop {
            #[cfg(unix)]
            if crate::session_unix::exit_signal().is_some() {
                return Err(io::ErrorKind::Interrupted.into());
            }
            let now = Instant::now();
            if let Some((_, _, escape_only)) = self.input_deadline.filter(|(at, _, _)| now >= *at) {
                if !escape_only {
                    return Err(invalid("incomplete terminal packet after 2s"));
                }
                self.pending.extend(self.decoder.flush_escape());
                self.input_deadline = None;
                if let Some(event) = self.pending.pop_front() {
                    return Ok(Some(event));
                }
            }
            let wait = self
                .input_deadline
                .map(|(at, _, _)| at)
                .unwrap_or(end)
                .min(end)
                .saturating_duration_since(now);
            #[cfg(windows)]
            if self.native {
                let ready = if let Some(wake) = wake {
                    use std::os::windows::io::AsRawHandle;
                    if crossterm::event::poll(Duration::ZERO)? {
                        true
                    } else {
                        if wake.wait(Some(std::io::stdin().as_raw_handle()), wait)? {
                            return Ok(None);
                        }
                        crossterm::event::poll(Duration::ZERO)?
                    }
                } else {
                    crossterm::event::poll(wait)?
                };
                if ready {
                    self.pending
                        .extend(self.decoder.feed_native(crossterm::event::read()?)?);
                }
            } else {
                if self.read_bytes(wait, wake)? {
                    return Ok(None);
                }
            }
            #[cfg(unix)]
            if self.read_bytes(
                if self.native {
                    wait.min(Duration::from_millis(250))
                } else {
                    wait
                },
                wake,
            )? {
                return Ok(None);
            }
            if self.native {
                if let Ok(size) = crossterm::terminal::size() {
                    if self.size != Some(size) {
                        self.size = Some(size);
                        self.pending
                            .push_back(WireEvent::Input(Event::Resize(size.0, size.1)));
                    }
                }
            }
            if self.decoder.paste.is_some() || self.decoder.buffer.is_empty() {
                self.input_deadline = None;
            } else {
                let serial = self.decoder.packet_serial;
                let escape_only = self.decoder.escape_pending();
                if self
                    .input_deadline
                    .is_none_or(|(_, old_serial, old_escape)| {
                        old_serial != serial || old_escape != escape_only
                    })
                {
                    let timeout = if escape_only {
                        Duration::from_millis(40)
                    } else {
                        Duration::from_secs(2)
                    };
                    self.input_deadline = Some((Instant::now() + timeout, serial, escape_only));
                }
            }
            if let Some(event) = self.pending.pop_front() {
                return Ok(Some(event));
            }
            if Instant::now() >= end {
                return Ok(None);
            }
        }
    }

    // true means a mailbox wake; no stdin bytes have been consumed in that case.
    fn read_bytes(&mut self, wait: Duration, wake: Option<&crate::wake::Wake>) -> io::Result<bool> {
        #[cfg(unix)]
        {
            let mut fd = libc::pollfd {
                fd: libc::STDIN_FILENO,
                events: libc::POLLIN,
                revents: 0,
            };
            let timeout = if let Some(wake) = wake {
                // The nonblocking fairness pass must check stdin even while
                // mailbox credit is signaled; otherwise a task burst wins all
                // ready turns on Unix just because the wake fd is first.
                if !wait.is_zero() && wake.wait(Some(libc::STDIN_FILENO), wait)? {
                    return Ok(true);
                }
                0
            } else {
                wait.as_millis().min(i32::MAX as u128) as i32
            };
            // SAFETY: one initialized pollfd, borrowed only for this call.
            let result = unsafe { libc::poll(&mut fd, 1, timeout) };
            if result < 0 {
                let e = io::Error::last_os_error();
                if e.kind() == io::ErrorKind::Interrupted {
                    return Ok(false);
                }
                return Err(e);
            }
            if result == 0 {
                return Ok(false);
            }
        }
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            let end = Instant::now() + wait;
            loop {
                let mut available = 0;
                // SAFETY: borrowed stdin pipe handle; output points to a u32.
                let ok = unsafe {
                    windows_sys::Win32::System::Pipes::PeekNamedPipe(
                        std::io::stdin().as_raw_handle(),
                        std::ptr::null_mut(),
                        0,
                        std::ptr::null_mut(),
                        &mut available,
                        std::ptr::null_mut(),
                    )
                };
                if ok == 0 {
                    return Err(io::Error::last_os_error());
                }
                if available > 0 {
                    break;
                }
                if Instant::now() >= end {
                    return Ok(false);
                }
                let wait =
                    Duration::from_millis(2).min(end.saturating_duration_since(Instant::now()));
                if let Some(wake) = wake {
                    if wake.wait(None, wait)? {
                        return Ok(true);
                    }
                } else {
                    std::thread::sleep(wait);
                }
            }
        }
        let mut bytes = [0; 4096];
        let n = std::io::stdin().read(&mut bytes)?;
        if n == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "terminal input closed",
            ));
        }
        self.pending.extend(self.decoder.feed(&bytes[..n])?);
        Ok(false)
    }
}
