//! Explicit tmux DCS passthrough for bounded Kitty APC packets.
use std::io::{self, Write};
pub struct TmuxPassthrough<W> {
    inner: W,
    pending: Vec<u8>,
    enabled: bool,
    failed: bool,
    depth: u8,
}
impl<W: Write> TmuxPassthrough<W> {
    pub fn new(inner: W) -> Self {
        Self {
            inner,
            pending: Vec::new(),
            enabled: true,
            failed: false,
            depth: 1,
        }
    }
    pub(crate) fn direct(inner: W) -> Self {
        Self {
            inner,
            pending: Vec::new(),
            enabled: false,
            failed: false,
            depth: 1,
        }
    }
    pub(crate) fn enable(&mut self, depth: u8) -> io::Result<()> {
        if !(1..=8).contains(&depth) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "tmux depth must be 1..8",
            ));
        }
        self.depth = depth;
        self.enabled = true;
        Ok(())
    }
    pub fn with_depth(inner: W, depth: u8) -> io::Result<Self> {
        let mut value = Self::new(inner);
        value.enable(depth)?;
        Ok(value)
    }
    pub fn get_ref(&self) -> &W {
        &self.inner
    }
    pub fn into_inner(self) -> W {
        self.inner
    }
    fn write_packets(&mut self, mut bytes: &[u8]) -> io::Result<()> {
        while !bytes.is_empty() {
            if self.pending.is_empty() {
                let n = bytes.iter().position(|b| *b == 0x1b).unwrap_or(bytes.len());
                self.inner.write_all(&bytes[..n])?;
                bytes = &bytes[n..];
                if bytes.is_empty() {
                    break;
                }
                self.pending.push(0x1b);
                bytes = &bytes[1..];
            } else if self.pending.len() == 1 {
                if bytes[0] == b'_' {
                    self.pending.push(b'_');
                    bytes = &bytes[1..];
                } else {
                    self.inner.write_all(&self.pending)?;
                    self.pending.clear();
                }
            } else {
                let byte = bytes[0];
                bytes = &bytes[1..];
                if byte == 0x18 {
                    self.pending.clear();
                    self.inner.write_all(&[byte])?;
                    continue;
                }
                self.pending.push(byte);
                if self.pending.len() == 3 && byte != b'G' {
                    self.inner.write_all(&self.pending)?;
                    self.pending.clear();
                    continue;
                }
                if self.pending.ends_with(b"\x1b\\") {
                    let mut packet = self.pending.clone();
                    for _ in 0..self.depth {
                        let mut wrapped = Vec::with_capacity(packet.len() + 32);
                        wrapped.extend_from_slice(b"\x1bPtmux;");
                        for byte in &packet {
                            if *byte == 0x1b {
                                wrapped.push(0x1b);
                            }
                            wrapped.push(*byte);
                        }
                        wrapped.extend_from_slice(b"\x1b\\");
                        packet = wrapped;
                    }
                    self.inner.write_all(&packet)?;
                    self.pending.clear();
                } else if self.pending.len() > 8192 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "tmux Kitty packet exceeds 8 KiB",
                    ));
                }
            }
        }
        Ok(())
    }
}
impl<W: Write> Write for TmuxPassthrough<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.failed {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        if !self.enabled {
            return self.inner.write(bytes);
        }
        match self.write_packets(bytes) {
            Ok(()) => Ok(bytes.len()),
            Err(error) => {
                self.failed = true;
                Err(error)
            }
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        if self.failed {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        self.inner.flush()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_tmux_envelopes_decode_at_each_layer_without_wrapping_modes() {
        fn unwrap(packet: &[u8]) -> Vec<u8> {
            let body = packet
                .strip_prefix(b"\x1bPtmux;")
                .unwrap()
                .strip_suffix(b"\x1b\\")
                .unwrap();
            let mut result = vec![];
            let mut i = 0;
            while i < body.len() {
                result.push(body[i]);
                i += if body[i] == 27 {
                    assert_eq!(body[i + 1], 27);
                    2
                } else {
                    1
                };
            }
            result
        }
        for depth in 1..=8 {
            let mut out = TmuxPassthrough::with_depth(Vec::new(), depth).unwrap();
            out.write_all(b"\x1b[?25l").unwrap();
            out.write_all(b"\x1b_Ga=q,i=1;AA==\x1b\\").unwrap();
            let mut packet = out.into_inner();
            assert!(packet.starts_with(b"\x1b[?25l"));
            packet.drain(..6);
            for _ in 0..depth {
                packet = unwrap(&packet);
            }
            assert_eq!(packet, b"\x1b_Ga=q,i=1;AA==\x1b\\");
        }
        assert!(TmuxPassthrough::with_depth(Vec::new(), 9).is_err());
    }
    #[test]
    fn fragmented_apc_wraps_once_and_modes_remain_local_to_tmux() {
        let stream =
            b"\x1b[?1049htext\x1b_Ga=q,i=42;AA==\x1b\\\x1b_Ga=d,d=I,i=42;\x1b\\\x1b[?1049l";
        for size in 1..=stream.len() {
            let mut out = TmuxPassthrough::new(Vec::new());
            for chunk in stream.chunks(size) {
                out.write_all(chunk).unwrap();
            }
            out.flush().unwrap();
            assert_eq!(out.into_inner(),b"\x1b[?1049htext\x1bPtmux;\x1b\x1b_Ga=q,i=42;AA==\x1b\x1b\\\x1b\\\x1bPtmux;\x1b\x1b_Ga=d,d=I,i=42;\x1b\x1b\\\x1b\\\x1b[?1049l");
        }
        let mut out = TmuxPassthrough::new(Vec::new());
        out.write_all(b"\x1b_Gpartial").unwrap();
        out.write_all(b"\x18\x1b\\").unwrap();
        assert_eq!(out.into_inner(), b"\x18\x1b\\");
        let mut out = TmuxPassthrough::new(Vec::new());
        assert!(out
            .write_all(&[b"\x1b_G".as_slice(), &vec![b'x'; 8192]].concat())
            .is_err());
        assert!(out.write_all(b"late").is_err());
    }
}
