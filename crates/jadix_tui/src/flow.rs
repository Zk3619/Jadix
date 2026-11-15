//! Bounded Kitty query fences and incremental reply demultiplexing.
//! The host must feed bytes from its *single* input owner; never add a second
//! stdin reader alongside Crossterm. `terminal_app` connects these fences through
//! `StdinEvents`; the legacy character examples retain their own event loop.
use std::{
    collections::VecDeque,
    io::{self, Write},
    num::NonZeroU32,
    time::{Duration, Instant},
};

/// A parsed graphics response, separate from ordinary keyboard/paste bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KittyReply {
    pub id: NonZeroU32,
    pub message: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum TerminalPacket {
    Input(Vec<u8>),
    Graphics(KittyReply),
}

/// Incremental APC parser. At most 1024 bytes are held across reads. Unknown,
/// malformed and oversized sequences pass through unchanged as ordinary input.
#[derive(Default)]
pub struct KittyReplyDecoder {
    pending: Vec<u8>,
}
impl KittyReplyDecoder {
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<TerminalPacket> {
        let mut packets = Vec::new();
        let mut input = Vec::new();
        for &byte in bytes {
            self.pending.push(byte);
            loop {
                let prefix = b"\x1b_G";
                if self.pending.len() < prefix.len() && prefix.starts_with(&self.pending) {
                    break;
                }
                if !self.pending.starts_with(prefix) {
                    input.push(self.pending.remove(0));
                    if self.pending.is_empty() {
                        break;
                    }
                    continue;
                }
                if self.pending.ends_with(b"\x1b\\") {
                    let reply = parse_reply(&self.pending[3..self.pending.len() - 2]);
                    if let Some(reply) = reply {
                        if !input.is_empty() {
                            packets.push(TerminalPacket::Input(std::mem::take(&mut input)));
                        }
                        packets.push(TerminalPacket::Graphics(reply));
                    } else {
                        input.extend_from_slice(&self.pending);
                    }
                    self.pending.clear();
                    break;
                }
                if self.pending.len() >= 1024 {
                    input.extend_from_slice(&self.pending);
                    self.pending.clear();
                }
                break;
            }
        }
        if !input.is_empty() {
            packets.push(TerminalPacket::Input(input));
        }
        packets
    }

    /// End-of-stream / Escape timeout: preserve an incomplete input sequence.
    pub fn finish(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.pending)
    }
}

fn parse_reply(bytes: &[u8]) -> Option<KittyReply> {
    let text = std::str::from_utf8(bytes).ok()?;
    let (control, message) = text.split_once(';')?;
    if message.is_empty() || message.chars().any(char::is_control) {
        return None;
    }
    let mut id = None;
    for field in control.split(',') {
        let (key, value) = field.split_once('=')?;
        match key {
            "i" if id.is_none() => id = Some(NonZeroU32::new(value.parse().ok()?)?),
            "p" => {
                let _: u32 = value.parse().ok()?;
            }
            _ => return None,
        }
    }
    Some(KittyReply {
        id: id?,
        message: message.to_owned(),
    })
}

/// At most two submitted frames awaiting ordered terminal query responses.
/// `q=2` image uploads alone do not acknowledge completion. Each accepted draw
/// is followed by a separate one-pixel `a=q` query with a never-reused ID.
/// This bounds transport backlog, not monitor scanout/visible frame completion.
pub struct KittyFrameFlow {
    next: Option<NonZeroU32>,
    pending: VecDeque<(NonZeroU32, Instant)>,
    timeout: Duration,
    failed: bool,
    optional_probe: Option<NonZeroU32>,
}
impl KittyFrameFlow {
    /// Reserve a query-ID range separate from all image/other host IDs. A new
    /// terminal session must use a fresh range to reject late old responses.
    pub fn new(first_fence: NonZeroU32, timeout: Duration) -> io::Result<Self> {
        if timeout.is_zero() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "zero Kitty fence timeout",
            ));
        }
        Ok(Self {
            next: Some(first_fence),
            pending: VecDeque::new(),
            timeout,
            failed: false,
            optional_probe: None,
        })
    }

    pub fn in_flight(&self) -> usize {
        self.pending.len()
    }
    pub fn deadline(&self) -> Option<Instant> {
        self.pending.front().map(|(_, sent)| *sent + self.timeout)
    }

    pub fn check(&mut self, now: Instant) -> io::Result<()> {
        if self.failed {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "Kitty flow is invalidated",
            ));
        }
        if self
            .pending
            .front()
            .is_some_and(|(_, sent)| now.saturating_duration_since(*sent) >= self.timeout)
        {
            self.failed = true;
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Kitty query fence timed out",
            ));
        }
        Ok(())
    }

    /// Backpressure is checked before invoking `draw`; the caller can retain
    /// only its latest dirty state until credit returns. Any partial write or
    /// flush failure invalidates the flow: never replay an ambiguous upload.
    pub fn submit<W: Write>(
        &mut self,
        now: Instant,
        output: &mut W,
        draw: impl FnOnce(&mut W) -> io::Result<()>,
    ) -> io::Result<NonZeroU32> {
        self.submit_query(now, output, draw, false)
    }

    /// Optional zlib decoding capability query, still counted against credit.
    /// Rejection consumes this probe without invalidating ordinary frame flow.
    pub fn probe_zlib<W: Write>(&mut self, now: Instant, output: &mut W) -> io::Result<NonZeroU32> {
        if self.optional_probe.is_some() {
            return Err(io::Error::other("capability probe already pending"));
        }
        let id = self.submit_query(now, output, |_| Ok(()), true)?;
        self.optional_probe = Some(id);
        Ok(id)
    }

    fn submit_query<W: Write>(
        &mut self,
        now: Instant,
        output: &mut W,
        draw: impl FnOnce(&mut W) -> io::Result<()>,
        zlib: bool,
    ) -> io::Result<NonZeroU32> {
        self.check(now)?;
        if self.pending.len() == 2 {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "Kitty frames in flight limit",
            ));
        }
        let id = self
            .next
            .ok_or_else(|| io::Error::other("Kitty query IDs exhausted"))?;
        self.next = id.get().checked_add(1).and_then(NonZeroU32::new);
        let result = (|| {
            draw(output)?;
            if zlib {
                // RFC1950 stream for one black RGB pixel, not an uncompressed
                // payload mislabeled o=z. Reuse upstream encoding for frames.
                write!(
                    output,
                    "\x1b_Ga=q,t=d,f=24,s=1,v=1,o=z,i={id};eJxjYGAAAAADAAE=\x1b\\"
                )?;
            } else {
                write!(output, "\x1b_Ga=q,t=d,f=24,s=1,v=1,i={id};AAAA\x1b\\")?;
            }
            output.flush()
        })();
        if let Err(error) = result {
            self.failed = true;
            return Err(error);
        }
        self.pending.push_back((id, now));
        Ok(id)
    }

    /// Only the oldest owned fence can release credit. Unrelated, duplicate or
    /// out-of-order replies cannot advance this session. Owned frame errors
    /// fail it; an optional capability rejection consumes only that probe.
    pub fn acknowledge(&mut self, reply: &KittyReply, now: Instant) -> io::Result<bool> {
        self.check(now)?;
        if self.pending.front().is_none_or(|(id, _)| *id != reply.id) {
            return Ok(false);
        }
        if reply.message != "OK" && self.optional_probe != Some(reply.id) {
            self.failed = true;
            return Err(io::Error::other(format!(
                "Kitty rejected frame fence: {}",
                reply.message
            )));
        }
        self.pending.pop_front();
        if self.optional_probe == Some(reply.id) {
            self.optional_probe = None;
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fragmented_replies_preserve_utf8_paste_keys_and_unknown_sequences() {
        let bytes = "a中文\x1b[200~paste\x1b[201~\x1b_Gi=10;OK\x1b\\z\x1b_Gi=0;OK\x1b\\".as_bytes();
        for chunk in 1..=bytes.len() {
            let mut decoder = KittyReplyDecoder::default();
            let packets: Vec<_> = bytes.chunks(chunk).flat_map(|b| decoder.feed(b)).collect();
            let input: Vec<_> = packets
                .iter()
                .filter_map(|p| match p {
                    TerminalPacket::Input(b) => Some(b.as_slice()),
                    _ => None,
                })
                .flatten()
                .copied()
                .collect();
            assert_eq!(
                input,
                "a中文\x1b[200~paste\x1b[201~z\x1b_Gi=0;OK\x1b\\".as_bytes()
            );
            assert_eq!(
                packets
                    .iter()
                    .filter(|p| matches!(p, TerminalPacket::Graphics(_)))
                    .count(),
                1
            );
            assert!(decoder.finish().is_empty());
        }
        let mut decoder = KittyReplyDecoder::default();
        let oversized = [b"\x1b_G".as_slice(), &vec![b'x'; 4096]].concat();
        let mut recovered: Vec<_> = decoder
            .feed(&oversized)
            .into_iter()
            .flat_map(|p| match p {
                TerminalPacket::Input(b) => b,
                _ => panic!(),
            })
            .collect();
        recovered.extend(decoder.finish());
        assert_eq!(recovered, oversized);
    }

    #[test]
    fn bounded_credit_timeout_errors_and_io_failure_do_not_replay() {
        let now = Instant::now();
        let mut flow =
            KittyFrameFlow::new(NonZeroU32::new(10).unwrap(), Duration::from_secs(1)).unwrap();
        let mut out = Vec::new();
        let first = flow
            .submit(now, &mut out, |w| w.write_all(b"frame1"))
            .unwrap();
        let second = flow
            .submit(now, &mut out, |w| w.write_all(b"frame2"))
            .unwrap();
        assert_eq!(
            flow.submit(now, &mut out, |_| panic!(
                "must not draw under backpressure"
            ))
            .unwrap_err()
            .kind(),
            io::ErrorKind::WouldBlock
        );
        let reply = |id| KittyReply {
            id,
            message: "OK".into(),
        };
        assert!(!flow.acknowledge(&reply(second), now).unwrap());
        assert!(flow.acknowledge(&reply(first), now).unwrap());
        assert!(!flow.acknowledge(&reply(first), now).unwrap());
        assert_eq!(flow.in_flight(), 1);
        assert_eq!(
            flow.check(now + Duration::from_secs(1)).unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
        assert!(flow.acknowledge(&reply(second), now).is_err());
        let mut flow = KittyFrameFlow::new(first, Duration::from_secs(1)).unwrap();
        assert!(flow
            .submit(now, &mut out, |w| {
                w.write_all(b"partial")?;
                Err(io::ErrorKind::BrokenPipe.into())
            })
            .is_err());
        assert!(flow
            .submit(now, &mut out, |_| panic!("invalid flow must not draw"))
            .is_err());
        let mut flow = KittyFrameFlow::new(first, Duration::from_secs(1)).unwrap();
        let id = flow.submit(now, &mut out, |_| Ok(())).unwrap();
        assert!(flow
            .acknowledge(
                &KittyReply {
                    id,
                    message: "ENOTSUP".into()
                },
                now
            )
            .is_err());
    }
}
