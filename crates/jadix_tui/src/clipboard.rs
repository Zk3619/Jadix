//! Explicit opt-in OSC52 writes. No clipboard read/query or automatic export.
use base64::Engine;
use std::io::{self, Write};
/// The host must opt in; constructing a disabled writer never emits bytes.
/// A completed write proves delivery to stdout, not host clipboard permission.
pub struct TerminalClipboard {
    enabled: bool,
}
impl TerminalClipboard {
    pub const MAX_BYTES: usize = 100 * 1024;
    pub fn new(enabled: bool) -> Self {
        Self { enabled }
    }
    pub fn copy(&self, text: &str, output: &mut impl Write) -> io::Result<()> {
        if !self.enabled {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "terminal clipboard is disabled",
            ));
        }
        if text.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "no text selected",
            ));
        }
        if text.len() > Self::MAX_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "terminal clipboard exceeds 100 KiB",
            ));
        }
        let encoded = base64::engine::general_purpose::STANDARD.encode(text);
        write!(output, "\x1b]52;c;{encoded}\x1b\\")?;
        output.flush()
    }
}

#[cfg(feature = "native-clipboard")]
pub(crate) fn shortcut(event: &crate::crossterm::event::Event) -> Option<char> {
    use crate::crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
    match event {
        Event::Key(key)
            if key.kind != KeyEventKind::Release
                && key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::SUPER) =>
        {
            match key.code {
                KeyCode::Char(c @ ('c' | 'x' | 'v')) => Some(c),
                _ => None,
            }
        }
        _ => None,
    }
}
#[cfg(feature = "native-clipboard")]
pub(crate) fn check_transfer(text: &str) -> io::Result<()> {
    if text.len() > TerminalClipboard::MAX_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "clipboard transfer exceeds 100 KiB",
        ));
    }
    Ok(())
}
