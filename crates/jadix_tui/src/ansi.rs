//! Explicit basic-SGR transport for a 16-color character backend.
use crate::{colors::palette, CellColors};
use ratatui::style::Color;
use std::io::{self, Write};

/// Wrap the Crossterm writer when using `Palette16`. Converts extended color
/// SGR to 30..37/40..47/90..97/100..107. Other policies pass bytes unchanged.
/// CSI is bounded to 128 bytes; OSC/DCS/APC payloads are opaque. An incomplete
/// CSI at flush or any I/O error poisons this frame writer; recreate it after
/// the host has restored its terminal session.
pub struct AnsiColorWriter<W> {
    inner: W,
    basic: bool,
    state: State,
    csi: Vec<u8>,
    failed: bool,
}
#[derive(Clone, Copy)]
enum State {
    Ground,
    Escape,
    Csi,
    String { osc: bool, escape: bool },
}
impl<W: Write> AnsiColorWriter<W> {
    pub fn new(inner: W, colors: CellColors) -> Self {
        Self {
            inner,
            basic: colors == CellColors::Palette16,
            state: State::Ground,
            csi: Vec::new(),
            failed: false,
        }
    }
    pub fn get_ref(&self) -> &W {
        &self.inner
    }
    pub fn finish(mut self) -> io::Result<W> {
        self.flush()?;
        Ok(self.inner)
    }
    fn convert(&self, out: &mut Vec<u8>) -> io::Result<()> {
        if self.csi.last() != Some(&b'm') {
            out.extend_from_slice(&self.csi);
            return Ok(());
        }
        let body = &self.csi[2..self.csi.len() - 1];
        let values: Vec<u16> = body
            .split(|b| *b == b';')
            .map(|p| {
                if p.is_empty() {
                    return Ok(0);
                }
                std::str::from_utf8(p)
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "unsupported basic SGR")
                    })
            })
            .collect::<io::Result<_>>()?;
        let mut converted = Vec::new();
        let mut i = 0;
        while i < values.len() {
            let value = values[i];
            if value == 38 || value == 48 || value == 58 {
                let rgba = match values.get(i + 1) {
                    Some(5) if values.get(i + 2).is_some_and(|n| *n <= 255) => {
                        let rgb = palette(values[i + 2] as u8);
                        i += 3;
                        [rgb[0], rgb[1], rgb[2], 255]
                    }
                    Some(2)
                        if values
                            .get(i + 2..i + 5)
                            .is_some_and(|rgb| rgb.iter().all(|n| *n <= 255)) =>
                    {
                        let rgba = [
                            values[i + 2] as u8,
                            values[i + 3] as u8,
                            values[i + 4] as u8,
                            255,
                        ];
                        i += 5;
                        rgba
                    }
                    _ => {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "invalid extended SGR",
                        ))
                    }
                };
                if value == 58 {
                    continue;
                } // basic ANSI has no underline color
                let color = CellColors::Palette16.resolve(rgba, Color::Reset);
                let index = crate::colors::ANSI
                    .iter()
                    .position(|c| *c == color)
                    .unwrap();
                let base = if value == 38 { 30 } else { 40 };
                converted.push(base + (index % 8) as u16 + if index >= 8 { 60 } else { 0 });
            } else {
                // Drop underline-color reset along with its unsupported color.
                if value != 59 {
                    converted.push(value);
                }
                i += 1;
            }
        }
        if !converted.is_empty() {
            out.extend_from_slice(b"\x1b[");
            for (i, value) in converted.iter().enumerate() {
                if i != 0 {
                    out.push(b';');
                }
                write!(out, "{value}")?;
            }
            out.push(b'm');
        }
        Ok(())
    }
    fn transform(&mut self, bytes: &[u8], out: &mut Vec<u8>) -> io::Result<()> {
        for &byte in bytes {
            match self.state {
                State::Ground if byte == 27 => self.state = State::Escape,
                State::Ground => out.push(byte),
                State::Escape => {
                    self.state = match byte {
                        b'[' => {
                            self.csi.extend_from_slice(b"\x1b[");
                            State::Csi
                        }
                        b']' | b'P' | b'_' | b'^' | b'X' => {
                            out.extend_from_slice(&[27, byte]);
                            State::String {
                                osc: byte == b']',
                                escape: false,
                            }
                        }
                        _ => {
                            out.extend_from_slice(&[27, byte]);
                            State::Ground
                        }
                    };
                }
                State::Csi => {
                    if self.csi.len() >= 128 || !(0x20..=0x7e).contains(&byte) {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "invalid or oversized CSI",
                        ));
                    }
                    self.csi.push(byte);
                    if (0x40..=0x7e).contains(&byte) {
                        self.convert(out)?;
                        self.csi.clear();
                        self.state = State::Ground;
                    }
                }
                State::String { osc, escape } => {
                    out.push(byte);
                    self.state = if (osc && byte == 7) || (escape && byte == b'\\') {
                        State::Ground
                    } else {
                        State::String {
                            osc,
                            escape: byte == 27,
                        }
                    };
                }
            }
        }
        Ok(())
    }
}
impl<W: Write> Write for AnsiColorWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.failed {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        if !self.basic {
            return self.inner.write(bytes);
        }
        let result = (|| {
            // Bounded scratch; no whole-frame copy, even for a large caller write.
            for chunk in bytes.chunks(4096) {
                let mut out = Vec::with_capacity(4224);
                self.transform(chunk, &mut out)?;
                self.inner.write_all(&out)?;
            }
            Ok(bytes.len())
        })();
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn flush(&mut self) -> io::Result<()> {
        if self.failed {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        let result = if self.basic && !matches!(self.state, State::Ground) {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unfinished ANSI sequence",
            ))
        } else {
            self.inner.flush()
        };
        if result.is_err() {
            self.failed = true;
        }
        result
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fragmented_sgr_preserves_opaque_payloads_and_uses_basic_colors() {
        let input = b"\x1b[1;38;5;9;48;5;4mred\x1b[38;2;0;255;0mgreen\x1b[0m\x1b[2;3H\x1b]52;c;\x1b[38;5;9m\x07\x1b_G\x1b[38;5;9m\x1b\\";
        let expected = b"\x1b[1;91;44mred\x1b[92mgreen\x1b[0m\x1b[2;3H\x1b]52;c;\x1b[38;5;9m\x07\x1b_G\x1b[38;5;9m\x1b\\";
        for chunk in 1..=input.len() {
            let mut writer = AnsiColorWriter::new(Vec::new(), CellColors::Palette16);
            for part in input.chunks(chunk) {
                writer.write_all(part).unwrap();
            }
            assert_eq!(writer.finish().unwrap(), expected);
        }
        for index in 0..=255 {
            let mut writer = AnsiColorWriter::new(Vec::new(), CellColors::Palette16);
            write!(writer, "\x1b[38;5;{index};48;5;{index}m").unwrap();
            let bytes = writer.finish().unwrap();
            assert!(!String::from_utf8(bytes).unwrap().contains(";5;"));
        }
        let mut writer = AnsiColorWriter::new(Vec::new(), CellColors::TrueColor);
        writer.write_all(input).unwrap();
        assert_eq!(writer.finish().unwrap(), input);
    }
    #[test]
    fn malformed_incomplete_and_io_failures_cannot_resume_a_partial_frame() {
        for input in [
            b"\x1b[38;5;256m".to_vec(),
            b"\x1b[38;2;1m".to_vec(),
            [b"\x1b[".as_slice(), &[b'1'; 128]].concat(),
        ] {
            let mut writer = AnsiColorWriter::new(Vec::new(), CellColors::Palette16);
            assert!(writer.write_all(&input).is_err());
            assert!(writer.write_all(b"late").is_err());
        }
        let mut writer = AnsiColorWriter::new(Vec::new(), CellColors::Palette16);
        writer.write_all(b"\x1b[").unwrap();
        assert!(writer.flush().is_err());
        assert!(writer.write_all(b"0m").is_err());
        let mut writer =
            AnsiColorWriter::new(std::io::Cursor::new([0u8; 1]), CellColors::Palette16);
        assert!(writer.write_all(b"abc").is_err());
        assert!(writer.flush().is_err());
    }
}
