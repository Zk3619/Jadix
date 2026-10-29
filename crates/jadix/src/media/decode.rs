//! RGBA 解码：[`super::image`] 处理光栅；`.cur` 热点与 SVG 保留专用路径。

use std::path::{Path, PathBuf};

/// 解码选项（保留类型以稳定公开 API；当前无额外约束）。
#[derive(Clone, Copy, Debug, Default)]
pub struct DecodeOptions;

impl DecodeOptions {
    pub const fn cursor() -> Self {
        Self
    }

    pub const fn image() -> Self {
        Self
    }
}

/// 解码后的 RGBA8 缓冲。
#[derive(Clone, Debug)]
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// `.cur` 内嵌热点（`image` 不提供此元数据）。
    pub intrinsic_hotspot: Option<(u16, u16)>,
    /// 多帧动画（GIF 等）。`rgba` 为当前/首帧。
    pub animation: Option<super::animation::DecodedAnimation>,
}

impl DecodedImage {
    pub fn is_animated(&self) -> bool {
        self.animation.as_ref().is_some_and(|a| a.is_animated())
    }
}

/// 从字节解码。
///
/// - **ICO**：[`image`](https://github.com/image-rs/image)
/// - **CUR**：目录解析热点 + `image` 解码条目位图
/// - **GIF**：全帧动画
/// - **SVG**：`resvg`（`svg` feature）
/// - **其他光栅**：`image`
pub fn decode_bytes(bytes: &[u8], _opts: DecodeOptions) -> Option<DecodedImage> {
    if let Some(typ) = ico_file_type(bytes) {
        return match typ {
            2 => decode_cur(bytes),
            _ => super::image::decode_raster(bytes),
        }
        .and_then(accept_valid);
    }
    if super::gif::is_gif(bytes) {
        return super::gif::decode_gif(bytes).and_then(accept_valid);
    }
    if looks_like_svg(bytes) {
        return decode_svg(bytes);
    }
    super::image::decode_raster(bytes).and_then(accept_valid)
}

/// 解析 `data:`、本地路径。`http(s)` 始终返回 `None`（远程拉取见 feature `remote-asset`）。
pub fn load_src(src: &str, opts: DecodeOptions) -> Option<DecodedImage> {
    let bytes = read_src_bytes(src)?;
    decode_bytes(&bytes, opts)
}

pub fn read_src_bytes(src: &str) -> Option<Vec<u8>> {
    let src = src.trim();
    if src.is_empty() {
        return None;
    }
    let lower = src.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return None;
    }
    if lower.starts_with("data:") {
        return decode_data_url(src);
    }
    let path = resolve_asset_path(src)?;
    std::fs::read(path).ok()
}

pub fn resolve_asset_path(src: &str) -> Option<PathBuf> {
    let src = src
        .strip_prefix("file://")
        .or_else(|| src.strip_prefix("file:"))
        .unwrap_or(src);
    let p = Path::new(src);
    if p.is_file() {
        return Some(p.to_path_buf());
    }
    let mut candidates = vec![
        PathBuf::from("assets").join(src),
        PathBuf::from(src),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(src),
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("assets")
            .join(src),
    ];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            candidates.push(parent.join(src));
            candidates.push(parent.join("assets").join(src));
        }
    }
    candidates.into_iter().find(|c| c.is_file())
}

pub fn decode_data_url(src: &str) -> Option<Vec<u8>> {
    let rest = src.strip_prefix("data:")?;
    let (meta, data) = rest.split_once(',')?;
    if meta.to_ascii_lowercase().contains(";base64") {
        decode_base64(data)
    } else {
        Some(percent_decode(data))
    }
}

fn accept_valid(d: DecodedImage) -> Option<DecodedImage> {
    if d.width == 0 || d.height == 0 {
        return None;
    }
    Some(d)
}

fn ico_file_type(bytes: &[u8]) -> Option<u16> {
    if bytes.len() < 6 {
        return None;
    }
    let reserved = u16::from_le_bytes([bytes[0], bytes[1]]);
    let typ = u16::from_le_bytes([bytes[2], bytes[3]]);
    if reserved == 0 && (typ == 1 || typ == 2) {
        Some(typ)
    } else {
        None
    }
}

fn looks_like_svg(bytes: &[u8]) -> bool {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    let Ok(s) = std::str::from_utf8(bytes) else {
        return false;
    };
    let t = s.trim_start();
    let lower = t.to_ascii_lowercase();
    lower.starts_with("<svg") || lower.starts_with("<?xml")
}

fn percent_decode(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = (from_hex(b[i + 1]), from_hex(b[i + 2])) {
                out.push((h << 4) | l);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    out
}

fn from_hex(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

fn decode_base64(s: &str) -> Option<Vec<u8>> {
    let mut vals = Vec::new();
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' | b' ' | b'\n' | b'\r' | b'\t' => continue,
            _ => return None,
        };
        if c != b'=' {
            vals.push(v);
        }
    }
    let mut out = Vec::with_capacity(vals.len() * 3 / 4);
    for chunk in vals.chunks(4) {
        if chunk.len() < 2 {
            break;
        }
        out.push((chunk[0] << 2) | (chunk[1] >> 4));
        if chunk.len() >= 3 {
            out.push((chunk[1] << 4) | (chunk[2] >> 2));
        }
        if chunk.len() >= 4 {
            out.push((chunk[2] << 6) | chunk[3]);
        }
    }
    Some(out)
}

/// `.cur`：仅解析目录热点/选帧；位图由 `image` 解码（失败时回退 DIB XOR/AND）。
fn decode_cur(bytes: &[u8]) -> Option<DecodedImage> {
    let (slice, hotspot) = select_ico_entry(bytes, true)?;
    if let Some(mut d) = super::image::decode_raster(slice) {
        d.intrinsic_hotspot = hotspot;
        return Some(d);
    }
    decode_dib_xor_and(slice, hotspot)
}

fn select_ico_entry(bytes: &[u8], is_cur: bool) -> Option<(&[u8], Option<(u16, u16)>)> {
    if bytes.len() < 6 {
        return None;
    }
    let count = u16::from_le_bytes([bytes[4], bytes[5]]) as usize;
    if count == 0 {
        return None;
    }
    struct Entry<'a> {
        w: u32,
        h: u32,
        bitcount: u16,
        hotspot: Option<(u16, u16)>,
        slice: &'a [u8],
    }
    let mut best: Option<Entry> = None;
    for i in 0..count {
        let o = 6 + i * 16;
        if o + 16 > bytes.len() {
            break;
        }
        let w0 = bytes[o];
        let h0 = bytes[o + 1];
        let w = if w0 == 0 { 256 } else { w0 as u32 };
        let h = if h0 == 0 { 256 } else { h0 as u32 };
        let planes = u16::from_le_bytes([bytes[o + 4], bytes[o + 5]]);
        let bitcount = u16::from_le_bytes([bytes[o + 6], bytes[o + 7]]);
        let bytes_in_res = u32::from_le_bytes(bytes[o + 8..o + 12].try_into().ok()?);
        let offset = u32::from_le_bytes(bytes[o + 12..o + 16].try_into().ok()?);
        let start = offset as usize;
        let end = start.saturating_add(bytes_in_res as usize);
        if start >= bytes.len() {
            continue;
        }
        let slice = &bytes[start..end.min(bytes.len())];
        let hotspot = if is_cur {
            Some((planes, bitcount))
        } else {
            None
        };
        let e = Entry {
            w,
            h,
            bitcount,
            hotspot,
            slice,
        };
        let better = match &best {
            None => true,
            Some(b) => e.w * e.h > b.w * b.h || (e.w * e.h == b.w * b.h && e.bitcount > b.bitcount),
        };
        if better {
            best = Some(e);
        }
    }
    let e = best?;
    Some((e.slice, e.hotspot))
}

fn decode_dib_xor_and(data: &[u8], hotspot: Option<(u16, u16)>) -> Option<DecodedImage> {
    if data.len() < 40 {
        return None;
    }
    let header_size = u32::from_le_bytes(data[0..4].try_into().ok()?);
    if header_size < 40 {
        return None;
    }
    let width = i32::from_le_bytes(data[4..8].try_into().ok()?).unsigned_abs();
    let height_raw = i32::from_le_bytes(data[8..12].try_into().ok()?);
    let bitcount = u16::from_le_bytes(data[14..16].try_into().ok()?);
    if bitcount != 32 && bitcount != 24 {
        return None;
    }
    let abs_h = height_raw.unsigned_abs();
    let xor_h = if abs_h > width { abs_h / 2 } else { abs_h };
    if width == 0 || xor_h == 0 {
        return None;
    }
    let header = header_size as usize;
    let xor_stride = if bitcount == 32 {
        width as usize * 4
    } else {
        let row = width as usize * 3;
        row.div_ceil(4) * 4
    };
    let xor_size = xor_stride * xor_h as usize;
    let xor = data.get(header..header + xor_size)?;
    let mut rgba = vec![0u8; width as usize * xor_h as usize * 4];
    for y in 0..xor_h as usize {
        let src_y = xor_h as usize - 1 - y;
        let row = &xor[src_y * xor_stride..];
        for x in 0..width as usize {
            let di = (y * width as usize + x) * 4;
            if bitcount == 32 {
                let si = x * 4;
                rgba[di] = row[si + 2];
                rgba[di + 1] = row[si + 1];
                rgba[di + 2] = row[si];
                rgba[di + 3] = row[si + 3];
            } else {
                let si = x * 3;
                rgba[di] = row[si + 2];
                rgba[di + 1] = row[si + 1];
                rgba[di + 2] = row[si];
                rgba[di + 3] = 255;
            }
        }
    }
    let and_off = header + xor_size;
    let and_stride = (width as usize).div_ceil(32) * 4;
    if let Some(and) = data.get(and_off..and_off + and_stride * xor_h as usize) {
        for y in 0..xor_h as usize {
            let src_y = xor_h as usize - 1 - y;
            for x in 0..width as usize {
                let byte = and[src_y * and_stride + x / 8];
                let bit = 7 - (x % 8);
                if (byte >> bit) & 1 == 1 {
                    let di = (y * width as usize + x) * 4;
                    rgba[di + 3] = 0;
                }
            }
        }
    }
    Some(DecodedImage {
        width,
        height: xor_h,
        rgba,
        intrinsic_hotspot: hotspot,
        animation: None,
    })
}

#[cfg(feature = "svg")]
fn decode_svg(bytes: &[u8]) -> Option<DecodedImage> {
    let (out_w, out_h, rgba) = crate::media::svg_raster::rasterize_svg_natural(bytes)?;
    Some(DecodedImage {
        width: out_w,
        height: out_h,
        rgba,
        intrinsic_hotspot: None,
        animation: None,
    })
}

#[cfg(not(feature = "svg"))]
fn decode_svg(_bytes: &[u8]) -> Option<DecodedImage> {
    None
}

#[cfg(test)]
#[path = "tests/decode.rs"]
mod tests;
