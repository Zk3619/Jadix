//! 按魔数分派的 RGBA 解码。

use std::io::Cursor;
use std::path::{Path, PathBuf};

/// 自定义光标最长边（浏览器常见上限）。
pub const MAX_CURSOR_EDGE: u32 = 128;
/// 贴图 / `<Image>` 最长边，避免异常大图撑爆 GPU。
pub const MAX_IMAGE_EDGE: u32 = 4096;

/// 解码策略：光标拒绝过大位图、SVG 缩放到上限；普通贴图上限更大。
#[derive(Clone, Copy, Debug)]
pub struct DecodeOptions {
    pub max_edge: u32,
}

impl DecodeOptions {
    pub const fn cursor() -> Self {
        Self {
            max_edge: MAX_CURSOR_EDGE,
        }
    }

    pub const fn image() -> Self {
        Self {
            max_edge: MAX_IMAGE_EDGE,
        }
    }
}

/// 解码后的 RGBA8 缓冲。
#[derive(Clone, Debug)]
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// CUR 文件内嵌热点。
    pub intrinsic_hotspot: Option<(u16, u16)>,
}

/// 从字节解码。位图超过 `max_edge` 则失败；SVG 等比缩进上限内。
pub fn decode_bytes(bytes: &[u8], opts: DecodeOptions) -> Option<DecodedImage> {
    if bytes.len() >= 8 && bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return decode_png(bytes).and_then(|d| accept_size(d, opts));
    }
    if is_ico(bytes) {
        return decode_ico(bytes, opts.max_edge).and_then(|d| accept_size(d, opts));
    }
    if is_jpeg(bytes) {
        return decode_jpeg(bytes).and_then(|d| accept_size(d, opts));
    }
    if is_gif(bytes) {
        return decode_gif(bytes).and_then(|d| accept_size(d, opts));
    }
    if is_webp(bytes) {
        return decode_webp(bytes).and_then(|d| accept_size(d, opts));
    }
    if is_bmp(bytes) {
        return decode_bmp(bytes).and_then(|d| accept_size(d, opts));
    }
    if looks_like_svg(bytes) {
        return decode_svg(bytes, opts.max_edge);
    }
    decode_png(bytes).and_then(|d| accept_size(d, opts))
}

/// 解析 `data:`、本地路径（禁止 http(s)）。
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

fn accept_size(d: DecodedImage, opts: DecodeOptions) -> Option<DecodedImage> {
    if d.width == 0
        || d.height == 0
        || d.width > opts.max_edge
        || d.height > opts.max_edge
        || d.width > u16::MAX as u32
        || d.height > u16::MAX as u32
    {
        return None;
    }
    Some(d)
}

fn is_ico(bytes: &[u8]) -> bool {
    if bytes.len() < 6 {
        return false;
    }
    let reserved = u16::from_le_bytes([bytes[0], bytes[1]]);
    let typ = u16::from_le_bytes([bytes[2], bytes[3]]);
    reserved == 0 && (typ == 1 || typ == 2)
}

fn is_jpeg(bytes: &[u8]) -> bool {
    bytes.len() >= 3 && bytes[0] == 0xFF && bytes[1] == 0xD8 && bytes[2] == 0xFF
}

fn is_gif(bytes: &[u8]) -> bool {
    bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")
}

fn is_webp(bytes: &[u8]) -> bool {
    bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP"
}

fn is_bmp(bytes: &[u8]) -> bool {
    bytes.len() >= 14 && bytes.starts_with(b"BM")
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

fn decode_png(bytes: &[u8]) -> Option<DecodedImage> {
    let decoder = png::Decoder::new(Cursor::new(bytes));
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let data = &buf[..info.buffer_size()];
    let (w, h) = (info.width as usize, info.height as usize);
    let mut rgba = vec![0u8; w * h * 4];
    match info.color_type {
        png::ColorType::Rgba => rgba.copy_from_slice(data),
        png::ColorType::Rgb => {
            for i in 0..(w * h) {
                let si = i * 3;
                let di = i * 4;
                rgba[di] = data[si];
                rgba[di + 1] = data[si + 1];
                rgba[di + 2] = data[si + 2];
                rgba[di + 3] = 255;
            }
        }
        png::ColorType::GrayscaleAlpha => {
            for i in 0..(w * h) {
                let si = i * 2;
                let di = i * 4;
                rgba[di] = data[si];
                rgba[di + 1] = data[si];
                rgba[di + 2] = data[si];
                rgba[di + 3] = data[si + 1];
            }
        }
        png::ColorType::Grayscale => {
            for (i, &gray) in data.iter().enumerate().take(w * h) {
                let di = i * 4;
                rgba[di] = gray;
                rgba[di + 1] = gray;
                rgba[di + 2] = gray;
                rgba[di + 3] = 255;
            }
        }
        png::ColorType::Indexed => {
            let palette = reader.info().palette.as_ref()?;
            let trns = reader.info().trns.as_ref().map(|t| t.as_ref());
            for (i, &idx) in data.iter().enumerate().take(w * h) {
                let di = i * 4;
                let pi = idx as usize * 3;
                if pi + 2 >= palette.len() {
                    continue;
                }
                rgba[di] = palette[pi];
                rgba[di + 1] = palette[pi + 1];
                rgba[di + 2] = palette[pi + 2];
                rgba[di + 3] = trns
                    .and_then(|t| t.get(idx as usize).copied())
                    .unwrap_or(255);
            }
        }
    }
    Some(DecodedImage {
        width: info.width,
        height: info.height,
        rgba,
        intrinsic_hotspot: None,
    })
}

fn decode_ico(bytes: &[u8], max_edge: u32) -> Option<DecodedImage> {
    if bytes.len() < 6 {
        return None;
    }
    let typ = u16::from_le_bytes([bytes[2], bytes[3]]);
    let count = u16::from_le_bytes([bytes[4], bytes[5]]) as usize;
    if count == 0 {
        return None;
    }
    struct Entry {
        w: u32,
        h: u32,
        bitcount: u16,
        bytes_in_res: u32,
        offset: u32,
        hotspot: Option<(u16, u16)>,
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
        if w > max_edge || h > max_edge {
            continue;
        }
        let hotspot = if typ == 2 {
            Some((planes, bitcount))
        } else {
            None
        };
        let e = Entry {
            w,
            h,
            bitcount,
            bytes_in_res,
            offset,
            hotspot,
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
    let start = e.offset as usize;
    let end = start.saturating_add(e.bytes_in_res as usize);
    if start >= bytes.len() {
        return None;
    }
    let slice = &bytes[start..end.min(bytes.len())];
    if slice.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        let mut d = decode_png(slice)?;
        d.intrinsic_hotspot = e.hotspot;
        return Some(d);
    }
    decode_dib_xor_and(slice, e.hotspot)
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
    let xor_h = if abs_h >= width && abs_h % 2 == 0 && abs_h / 2 == width || abs_h == width * 2 {
        abs_h / 2
    } else if abs_h > width {
        abs_h / 2
    } else {
        abs_h
    };
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
    let and_stride = ((width as usize + 31) / 32) * 4;
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
    })
}

fn decode_bmp(bytes: &[u8]) -> Option<DecodedImage> {
    if bytes.len() < 54 || !bytes.starts_with(b"BM") {
        return None;
    }
    let off = u32::from_le_bytes(bytes[10..14].try_into().ok()?) as usize;
    let dib = bytes.get(14..)?;
    if dib.len() < 40 {
        return None;
    }
    let header_size = u32::from_le_bytes(dib[0..4].try_into().ok()?) as usize;
    if header_size < 40 {
        return None;
    }
    let width = i32::from_le_bytes(dib[4..8].try_into().ok()?);
    let height_raw = i32::from_le_bytes(dib[8..12].try_into().ok()?);
    let bitcount = u16::from_le_bytes(dib[14..16].try_into().ok()?);
    let compression = u32::from_le_bytes(dib[16..20].try_into().ok()?);
    if compression != 0 || width <= 0 {
        return None;
    }
    let w = width as usize;
    let top_down = height_raw < 0;
    let h = height_raw.unsigned_abs() as usize;
    if w == 0 || h == 0 {
        return None;
    }
    let pixels = bytes.get(off..)?;
    let mut rgba = vec![0u8; w * h * 4];
    match bitcount {
        32 => {
            let stride = w * 4;
            if pixels.len() < stride * h {
                return None;
            }
            for y in 0..h {
                let src_y = if top_down { y } else { h - 1 - y };
                let row = &pixels[src_y * stride..];
                for x in 0..w {
                    let si = x * 4;
                    let di = (y * w + x) * 4;
                    rgba[di] = row[si + 2];
                    rgba[di + 1] = row[si + 1];
                    rgba[di + 2] = row[si];
                    rgba[di + 3] = row[si + 3];
                }
            }
        }
        24 => {
            let stride = (w * 3).div_ceil(4) * 4;
            if pixels.len() < stride * h {
                return None;
            }
            for y in 0..h {
                let src_y = if top_down { y } else { h - 1 - y };
                let row = &pixels[src_y * stride..];
                for x in 0..w {
                    let si = x * 3;
                    let di = (y * w + x) * 4;
                    rgba[di] = row[si + 2];
                    rgba[di + 1] = row[si + 1];
                    rgba[di + 2] = row[si];
                    rgba[di + 3] = 255;
                }
            }
        }
        8 => {
            let colors = u32::from_le_bytes(dib[32..36].try_into().ok()?);
            let n = if colors == 0 { 256 } else { colors as usize };
            let pal_off = header_size;
            let pal = dib.get(pal_off..pal_off + n * 4)?;
            let stride = w.div_ceil(4) * 4;
            if pixels.len() < stride * h {
                return None;
            }
            for y in 0..h {
                let src_y = if top_down { y } else { h - 1 - y };
                let row = &pixels[src_y * stride..];
                for x in 0..w {
                    let idx = row[x] as usize;
                    let pi = idx * 4;
                    if pi + 2 >= pal.len() {
                        continue;
                    }
                    let di = (y * w + x) * 4;
                    rgba[di] = pal[pi + 2];
                    rgba[di + 1] = pal[pi + 1];
                    rgba[di + 2] = pal[pi];
                    rgba[di + 3] = 255;
                }
            }
        }
        _ => return None,
    }
    Some(DecodedImage {
        width: w as u32,
        height: h as u32,
        rgba,
        intrinsic_hotspot: None,
    })
}

#[cfg(feature = "jpeg")]
fn decode_jpeg(bytes: &[u8]) -> Option<DecodedImage> {
    let mut decoder = jpeg_decoder::Decoder::new(Cursor::new(bytes));
    let pixels = decoder.decode().ok()?;
    let info = decoder.info()?;
    let w = info.width as usize;
    let h = info.height as usize;
    let mut rgba = vec![0u8; w * h * 4];
    match info.pixel_format {
        jpeg_decoder::PixelFormat::L16 => {
            for i in 0..(w * h) {
                let g = pixels[i * 2 + 1];
                let di = i * 4;
                rgba[di] = g;
                rgba[di + 1] = g;
                rgba[di + 2] = g;
                rgba[di + 3] = 255;
            }
        }
        jpeg_decoder::PixelFormat::L8 => {
            for (i, &g) in pixels.iter().enumerate().take(w * h) {
                let di = i * 4;
                rgba[di] = g;
                rgba[di + 1] = g;
                rgba[di + 2] = g;
                rgba[di + 3] = 255;
            }
        }
        jpeg_decoder::PixelFormat::RGB24 => {
            for i in 0..(w * h) {
                let si = i * 3;
                let di = i * 4;
                rgba[di] = pixels[si];
                rgba[di + 1] = pixels[si + 1];
                rgba[di + 2] = pixels[si + 2];
                rgba[di + 3] = 255;
            }
        }
        jpeg_decoder::PixelFormat::CMYK32 => {
            for i in 0..(w * h) {
                let si = i * 4;
                let di = i * 4;
                let c = pixels[si] as u16;
                let m = pixels[si + 1] as u16;
                let y = pixels[si + 2] as u16;
                let k = pixels[si + 3] as u16;
                rgba[di] = ((255 - c.min(255)) * (255 - k.min(255)) / 255) as u8;
                rgba[di + 1] = ((255 - m.min(255)) * (255 - k.min(255)) / 255) as u8;
                rgba[di + 2] = ((255 - y.min(255)) * (255 - k.min(255)) / 255) as u8;
                rgba[di + 3] = 255;
            }
        }
    }
    Some(DecodedImage {
        width: info.width as u32,
        height: info.height as u32,
        rgba,
        intrinsic_hotspot: None,
    })
}

#[cfg(not(feature = "jpeg"))]
fn decode_jpeg(_bytes: &[u8]) -> Option<DecodedImage> {
    None
}

#[cfg(feature = "gif")]
fn decode_gif(bytes: &[u8]) -> Option<DecodedImage> {
    let mut opts = gif::DecodeOptions::new();
    opts.set_color_output(gif::ColorOutput::RGBA);
    let mut decoder = opts.read_info(Cursor::new(bytes)).ok()?;
    let screen_w = decoder.width() as usize;
    let screen_h = decoder.height() as usize;
    if screen_w == 0 || screen_h == 0 {
        return None;
    }
    let frame = decoder.read_next_frame().ok()??;
    let mut rgba = vec![0u8; screen_w * screen_h * 4];
    let fw = frame.width as usize;
    let fh = frame.height as usize;
    let left = frame.left as usize;
    let top = frame.top as usize;
    let src = &frame.buffer;
    for y in 0..fh {
        for x in 0..fw {
            let dx = left + x;
            let dy = top + y;
            if dx >= screen_w || dy >= screen_h {
                continue;
            }
            let si = (y * fw + x) * 4;
            let di = (dy * screen_w + dx) * 4;
            if si + 3 >= src.len() {
                continue;
            }
            rgba[di..di + 4].copy_from_slice(&src[si..si + 4]);
        }
    }
    Some(DecodedImage {
        width: screen_w as u32,
        height: screen_h as u32,
        rgba,
        intrinsic_hotspot: None,
    })
}

#[cfg(not(feature = "gif"))]
fn decode_gif(_bytes: &[u8]) -> Option<DecodedImage> {
    None
}

#[cfg(feature = "webp")]
fn decode_webp(bytes: &[u8]) -> Option<DecodedImage> {
    let mut decoder = image_webp::WebPDecoder::new(Cursor::new(bytes)).ok()?;
    let (w, h) = decoder.dimensions();
    if w == 0 || h == 0 {
        return None;
    }
    let mut rgba = vec![0u8; w as usize * h as usize * 4];
    decoder.read_image(&mut rgba).ok()?;
    Some(DecodedImage {
        width: w,
        height: h,
        rgba,
        intrinsic_hotspot: None,
    })
}

#[cfg(not(feature = "webp"))]
fn decode_webp(_bytes: &[u8]) -> Option<DecodedImage> {
    None
}

#[cfg(feature = "svg")]
fn decode_svg(bytes: &[u8], max_edge: u32) -> Option<DecodedImage> {
    let (out_w, out_h, rgba) = crate::media::svg_raster::rasterize_svg_fit(bytes, max_edge)?;
    Some(DecodedImage {
        width: out_w,
        height: out_h,
        rgba,
        intrinsic_hotspot: None,
    })
}

#[cfg(not(feature = "svg"))]
fn decode_svg(_bytes: &[u8], _max_edge: u32) -> Option<DecodedImage> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode_rgba_png(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut buf, w, h);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(rgba).unwrap();
        }
        buf
    }

    #[test]
    fn png_image_allows_over_cursor_cap() {
        let big = vec![0u8; 129 * 1 * 4];
        let png = encode_rgba_png(129, 1, &big);
        assert!(decode_bytes(&png, DecodeOptions::cursor()).is_none());
        let d = decode_bytes(&png, DecodeOptions::image()).unwrap();
        assert_eq!((d.width, d.height), (129, 1));
    }

    #[test]
    fn bmp24_roundtrip() {
        let w = 2u32;
        let h = 1u32;
        let stride = (w as usize * 3).div_ceil(4) * 4;
        let off = 54u32;
        let mut bytes = vec![0u8; off as usize + stride];
        bytes[0..2].copy_from_slice(b"BM");
        let file_size = bytes.len() as u32;
        bytes[2..6].copy_from_slice(&file_size.to_le_bytes());
        bytes[10..14].copy_from_slice(&off.to_le_bytes());
        bytes[14..18].copy_from_slice(&40u32.to_le_bytes());
        bytes[18..22].copy_from_slice(&(w as i32).to_le_bytes());
        bytes[22..26].copy_from_slice(&(h as i32).to_le_bytes());
        bytes[26..28].copy_from_slice(&1u16.to_le_bytes());
        bytes[28..30].copy_from_slice(&24u16.to_le_bytes());
        bytes[off as usize] = 0;
        bytes[off as usize + 1] = 0;
        bytes[off as usize + 2] = 255;
        let d = decode_bytes(&bytes, DecodeOptions::image()).unwrap();
        assert_eq!(&d.rgba[0..4], &[255, 0, 0, 255]);
    }

    #[cfg(feature = "svg")]
    #[test]
    fn svg_scales_to_option_cap() {
        let big = br#"<svg xmlns="http://www.w3.org/2000/svg" width="256" height="128"><rect width="256" height="128" fill="lime"/></svg>"#;
        let d = decode_bytes(big, DecodeOptions::cursor()).unwrap();
        assert_eq!(d.width.max(d.height), MAX_CURSOR_EDGE);
    }
}
