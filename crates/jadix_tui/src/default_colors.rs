//! Default-color discovery through the existing single input owner.
use crate::WireEvent;
use std::{
    io::{self, Write},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DefaultColors {
    pub foreground: Option<[u8; 3]>,
    pub background: Option<[u8; 3]>,
}
impl DefaultColors {
    pub fn complete(self) -> bool {
        self.foreground.is_some() && self.background.is_some()
    }
}
/// One bounded startup query per terminal session. Feed `WireEvent::Unknown`
/// from the normal input owner to `observe`, and bound its wait by `deadline`.
/// OSC replies carry no request ID: they cannot authenticate the terminal or
/// distinguish unsolicited replies from requested ones. Do not reuse a query
/// after timeout/cancellation; theme changes can supply new defaults explicitly.
#[derive(Default)]
pub struct DefaultColorQuery {
    started: bool,
    deadline: Option<Instant>,
    colors: DefaultColors,
}
impl DefaultColorQuery {
    pub fn start(
        &mut self,
        output: &mut impl Write,
        now: Instant,
        timeout: Duration,
    ) -> io::Result<()> {
        if self.started || timeout.is_zero() || timeout > Duration::from_secs(2) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "color query is once per session, timeout 1ns..2s",
            ));
        }
        self.started = true;
        output.write_all(b"\x1b]10;?\x1b\\\x1b]11;?\x1b\\")?;
        output.flush()?;
        self.deadline = Some(now + timeout);
        Ok(())
    }
    pub fn deadline(&self) -> Option<Instant> {
        self.deadline
    }
    pub fn cancel(&mut self) {
        self.deadline = None;
        self.colors = DefaultColors::default();
    }
    /// True only for a valid, still-pending color reply. Other input remains
    /// available to the host, including graphics replies and bracketed paste.
    pub fn observe(&mut self, event: &WireEvent, now: Instant) -> bool {
        if self.deadline.is_none_or(|at| now >= at) {
            return false;
        }
        let WireEvent::Unknown(bytes) = event else {
            return false;
        };
        let Some((foreground, rgb)) = parse(bytes) else {
            return false;
        };
        let target = if foreground {
            &mut self.colors.foreground
        } else {
            &mut self.colors.background
        };
        // Duplicate replies cannot change a value already accepted in this query.
        if target.is_none() {
            *target = Some(rgb);
        }
        true
    }
    /// Complete or timed out, exactly once. Missing values remain unknown.
    pub fn poll(&mut self, now: Instant) -> Option<DefaultColors> {
        if self
            .deadline
            .is_some_and(|at| now >= at || self.colors.complete())
        {
            self.deadline = None;
            Some(self.colors)
        } else {
            None
        }
    }
}
fn parse(bytes: &[u8]) -> Option<(bool, [u8; 3])> {
    if bytes.len() > 128 {
        return None;
    }
    let body = bytes.strip_prefix(b"\x1b]")?;
    let body = body
        .strip_suffix(b"\x1b\\")
        .or_else(|| body.strip_suffix(b"\x07"))?;
    let (role, rgb) = std::str::from_utf8(body).ok()?.split_once(";rgb:")?;
    let foreground = match role {
        "10" => true,
        "11" => false,
        _ => return None,
    };
    let mut parts = rgb.split('/');
    let mut output = [0; 3];
    for channel in &mut output {
        let value = parts.next()?;
        if value.is_empty() || value.len() > 4 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let maximum = (1u32 << (value.len() * 4)) - 1;
        let value = u32::from_str_radix(value, 16).ok()?;
        *channel = ((value * 255 + maximum / 2) / maximum) as u8;
    }
    if parts.next().is_some() {
        return None;
    }
    Some((foreground, output))
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{crossterm::event::Event, InputDecoder};
    #[test]
    fn fragmented_replies_share_input_with_literal_paste_and_expire_without_invention() {
        let now = Instant::now();
        let mut query = DefaultColorQuery::default();
        let mut request = Vec::new();
        query
            .start(&mut request, now, Duration::from_millis(100))
            .unwrap();
        assert_eq!(request, b"\x1b]10;?\x1b\\\x1b]11;?\x1b\\");
        let mut decoder = InputDecoder::default();
        let wire = b"\x1b]11;rgb:ffff/8080/0000\x07\x1b[200~\x1b]10;rgb:0/0/0\x07\x1b[201~\x1b]10;rgb:f/ab/1234\x1b\\";
        let mut pasted = false;
        for byte in wire {
            for event in decoder.feed(&[*byte]).unwrap() {
                if let WireEvent::Input(Event::Paste(ref text)) = event {
                    pasted = true;
                    assert!(text.contains("rgb:0/0/0"));
                    assert!(!query.observe(&event, now));
                } else {
                    assert!(query.observe(&event, now));
                }
            }
        }
        assert!(pasted);
        let colors = query.poll(now).unwrap();
        assert_eq!(colors.background, Some([255, 128, 0]));
        assert_eq!(colors.foreground, Some([255, 171, 18]));
        assert!(colors.complete());
        assert_eq!(query.poll(now), None);
        assert!(query
            .start(&mut request, now, Duration::from_millis(100))
            .is_err());
        let mut query = DefaultColorQuery::default();
        query
            .start(&mut Vec::new(), now, Duration::from_millis(100))
            .unwrap();
        assert!(!query.observe(
            &WireEvent::Unknown(b"\x1b]11;rgb:bad/xx/1\x07".to_vec()),
            now
        ));
        let late = now + Duration::from_millis(101);
        assert!(!query.observe(&WireEvent::Unknown(b"\x1b]11;rgb:f/f/f\x07".to_vec()), late));
        assert_eq!(query.poll(late), Some(DefaultColors::default()));
        let mut query = DefaultColorQuery::default();
        query
            .start(&mut Vec::new(), now, Duration::from_millis(100))
            .unwrap();
        query.cancel();
        assert_eq!(query.poll(late), None);
    }
}
