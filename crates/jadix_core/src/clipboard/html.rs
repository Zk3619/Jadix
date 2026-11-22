//! Windows HTML Clipboard Format uses UTF-8 byte offsets, including its header.
//! Keep this codec portable so native hosts and importers share the same bounds.
use super::MAX_BYTES;

pub fn encode(fragment: &str) -> Result<Vec<u8>, String> {
    const PREFIX: &str = "<html><body><!--StartFragment-->";
    const SUFFIX: &str = "<!--EndFragment--></body></html>";
    let header = |start, end, first, last| format!(
        "Version:1.0\r\nStartHTML:{start:010}\r\nEndHTML:{end:010}\r\nStartFragment:{first:010}\r\nEndFragment:{last:010}\r\n"
    );
    let start = header(0, 0, 0, 0).len();
    let first = start + PREFIX.len();
    let last = first.checked_add(fragment.len()).ok_or("HTML clipboard overflow")?;
    let end = last.checked_add(SUFFIX.len()).ok_or("HTML clipboard overflow")?;
    if end >= MAX_BYTES || fragment.contains('\0') { return Err("HTML clipboard payload invalid or too large".into()); }
    let mut bytes = header(start, end, first, last).into_bytes();
    bytes.extend_from_slice(PREFIX.as_bytes());
    bytes.extend_from_slice(fragment.as_bytes());
    bytes.extend_from_slice(SUFFIX.as_bytes());
    bytes.push(0);
    Ok(bytes)
}

pub fn decode(bytes: &[u8]) -> Result<&str, String> {
    if bytes.len() > MAX_BYTES { return Err("HTML clipboard payload too large".into()); }
    // The header is ASCII; fragments can contain arbitrary UTF-8 and newlines.
    let header = &bytes[..bytes.len().min(4096)];
    let field = |name: &[u8]| -> Result<usize, String> {
        let start = header.windows(name.len()).position(|v| v == name).ok_or("missing HTML clipboard offset")? + name.len();
        let end = header[start..].iter().position(|b| !b.is_ascii_digit()).map(|n| start + n).unwrap_or(header.len());
        std::str::from_utf8(&header[start..end]).ok().and_then(|n| n.parse().ok()).ok_or_else(|| "invalid HTML clipboard offset".into())
    };
    let start = field(b"StartHTML:")?;
    let end = field(b"EndHTML:")?;
    let first = field(b"StartFragment:")?;
    let last = field(b"EndFragment:")?;
    if !(start <= first && first <= last && last <= end && end <= bytes.len()) {
        return Err("HTML clipboard offsets outside payload".into());
    }
    let html = std::str::from_utf8(&bytes[start..end]).map_err(|e| e.to_string())?;
    html.get(first - start..last - start).ok_or_else(|| "HTML clipboard offsets split UTF-8".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_fragments_use_bytes_and_untrusted_offsets_are_checked() {
        let fragment = "<b>中文🦀</b>\n<p>שלום</p>";
        let bytes = encode(fragment).unwrap();
        assert_eq!(decode(&bytes).unwrap(), fragment);
        for n in [0, 5, bytes.len() - 2] { assert!(decode(&bytes[..n]).is_err()); }
        let mut corrupt = bytes.clone();
        let index = corrupt.windows(14).position(|v| v == b"StartFragment:").unwrap() + 14;
        corrupt[index..index + 10].copy_from_slice(b"9999999999");
        assert!(decode(&corrupt).is_err());
        assert!(encode("a\0b").is_err());
    }
}
