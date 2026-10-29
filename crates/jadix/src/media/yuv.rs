//! BT.709 limited-range YUV → RGBA (WP-M.3).
//!
//! CPU kernel is the runtime path (feeds [`crate::queue_image_rgba8`]). The same
//! matrix lives in `renderer/yuv_convert.wgsl` (compiled when `image-stream` is on).
//! Not a decoder.

use super::stream::{MediaFrame, MediaPlanes, PixelLayout};

/// ITU-R BT.709, limited range (8-bit Y 16–235, C 16–240).
#[inline]
pub fn bt709_yuv8_to_rgb(y: u8, u: u8, v: u8) -> [u8; 3] {
    bt709_yuv_to_rgb(
        (f32::from(y) - 16.0) / 219.0,
        (f32::from(u) - 128.0) / 224.0,
        (f32::from(v) - 128.0) / 224.0,
    )
}

/// Analog Y′CbCr after limited-range expand → 8-bit sRGB.
#[inline]
pub fn bt709_yuv_to_rgb(y: f32, cb: f32, cr: f32) -> [u8; 3] {
    let r = y + 1.5748 * cr;
    let g = y - 0.187324 * cb - 0.468124 * cr;
    let b = y + 1.8556 * cb;
    [
        (r.clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
        (g.clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
        (b.clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
    ]
}

#[inline]
fn chroma_wh(width: u32, height: u32) -> (u32, u32) {
    (width.div_ceil(2), height.div_ceil(2))
}

#[cfg(feature = "image-stream")]
fn sample_plane(plane: &[u8], stride: u32, x: u32, y: u32) -> u8 {
    let i = (y as usize)
        .saturating_mul(stride as usize)
        .saturating_add(x as usize);
    plane.get(i).copied().unwrap_or(128)
}

#[cfg(feature = "image-stream")]
fn sample_plane16(plane: &[u8], stride: u32, x: u32, y: u32) -> u16 {
    let i = (y as usize)
        .saturating_mul(stride as usize)
        .saturating_add((x as usize).saturating_mul(2));
    let lo = plane.get(i).copied().unwrap_or(0);
    let hi = plane.get(i.saturating_add(1)).copied().unwrap_or(0);
    u16::from_le_bytes([lo, hi])
}

/// 10-bit left-aligned in a 16-bit LE word (P010).
#[inline]
#[cfg(feature = "image-stream")]
fn p010_to_u8(sample: u16) -> u8 {
    ((u32::from(sample) >> 6) * 255 / 1023) as u8
}

/// Convert a validated [`MediaFrame`] to packed RGBA8. `None` = drop (keep last GPU frame).
pub fn media_frame_to_rgba8(frame: &MediaFrame<'_>) -> Option<Vec<u8>> {
    match frame.layout {
        PixelLayout::Rgba8Unorm => match frame.planes {
            MediaPlanes::Packed(bytes) => {
                let expected = (frame.width as usize)
                    .saturating_mul(frame.height as usize)
                    .saturating_mul(4);
                (bytes.len() == expected).then(|| bytes.to_vec())
            }
            MediaPlanes::Yuv { .. } => None,
        },
        PixelLayout::Nv12 | PixelLayout::I420 | PixelLayout::P010 => {
            #[cfg(feature = "image-stream")]
            {
                yuv_frame_to_rgba8(frame)
            }
            #[cfg(not(feature = "image-stream"))]
            {
                let _ = frame;
                None
            }
        }
    }
}

#[cfg(feature = "image-stream")]
fn yuv_frame_to_rgba8(frame: &MediaFrame<'_>) -> Option<Vec<u8>> {
    let MediaPlanes::Yuv {
        y,
        u,
        v,
        y_stride,
        uv_stride,
    } = frame.planes
    else {
        return None;
    };
    let w = frame.width;
    let h = frame.height;
    if w == 0 || h == 0 {
        return None;
    }
    let (cw, ch) = chroma_wh(w, h);
    match frame.layout {
        PixelLayout::I420 => {
            let y_stride = if y_stride == 0 { w } else { y_stride };
            let uv_stride = if uv_stride == 0 { cw } else { uv_stride };
            if (y_stride as usize).saturating_mul(h as usize) > y.len() {
                return None;
            }
            if (uv_stride as usize).saturating_mul(ch as usize) > u.len()
                || (uv_stride as usize).saturating_mul(ch as usize) > v.len()
            {
                return None;
            }
            let mut rgba = vec![0u8; (w as usize) * (h as usize) * 4];
            for py in 0..h {
                for px in 0..w {
                    let ys = sample_plane(y, y_stride, px, py);
                    let cx = px / 2;
                    let cy = py / 2;
                    let us = sample_plane(u, uv_stride, cx, cy);
                    let vs = sample_plane(v, uv_stride, cx, cy);
                    let [r, g, b] = bt709_yuv8_to_rgb(ys, us, vs);
                    let i = ((py * w + px) * 4) as usize;
                    rgba[i] = r;
                    rgba[i + 1] = g;
                    rgba[i + 2] = b;
                    rgba[i + 3] = 255;
                }
            }
            Some(rgba)
        }
        PixelLayout::Nv12 => {
            let y_stride = if y_stride == 0 { w } else { y_stride };
            let uv_stride = if uv_stride == 0 { w } else { uv_stride };
            if (y_stride as usize).saturating_mul(h as usize) > y.len() {
                return None;
            }
            if (uv_stride as usize).saturating_mul(ch as usize) > u.len() {
                return None;
            }
            let mut rgba = vec![0u8; (w as usize) * (h as usize) * 4];
            for py in 0..h {
                for px in 0..w {
                    let ys = sample_plane(y, y_stride, px, py);
                    let cx = px / 2;
                    let cy = py / 2;
                    let uv_i = (cy as usize)
                        .saturating_mul(uv_stride as usize)
                        .saturating_add((cx as usize).saturating_mul(2));
                    let us = *u.get(uv_i).unwrap_or(&128);
                    let vs = *u.get(uv_i.saturating_add(1)).unwrap_or(&128);
                    let [r, g, b] = bt709_yuv8_to_rgb(ys, us, vs);
                    let i = ((py * w + px) * 4) as usize;
                    rgba[i] = r;
                    rgba[i + 1] = g;
                    rgba[i + 2] = b;
                    rgba[i + 3] = 255;
                }
            }
            Some(rgba)
        }
        PixelLayout::P010 => {
            let y_stride = if y_stride == 0 {
                w.saturating_mul(2)
            } else {
                y_stride
            };
            let uv_stride = if uv_stride == 0 {
                cw.saturating_mul(4)
            } else {
                uv_stride
            };
            if (y_stride as usize).saturating_mul(h as usize) > y.len() {
                return None;
            }
            if (uv_stride as usize).saturating_mul(ch as usize) > u.len() {
                return None;
            }
            let mut rgba = vec![0u8; (w as usize) * (h as usize) * 4];
            for py in 0..h {
                for px in 0..w {
                    let ys = p010_to_u8(sample_plane16(y, y_stride, px, py));
                    let cx = px / 2;
                    let cy = py / 2;
                    let uv_i = (cy as usize)
                        .saturating_mul(uv_stride as usize)
                        .saturating_add((cx as usize).saturating_mul(4));
                    let us = p010_to_u8(u16::from_le_bytes([
                        *u.get(uv_i).unwrap_or(&0),
                        *u.get(uv_i.saturating_add(1)).unwrap_or(&0),
                    ]));
                    let vs = p010_to_u8(u16::from_le_bytes([
                        *u.get(uv_i.saturating_add(2)).unwrap_or(&0),
                        *u.get(uv_i.saturating_add(3)).unwrap_or(&0),
                    ]));
                    let [r, g, b] = bt709_yuv8_to_rgb(ys, us, vs);
                    let i = ((py * w + px) * 4) as usize;
                    rgba[i] = r;
                    rgba[i + 1] = g;
                    rgba[i + 2] = b;
                    rgba[i + 3] = 255;
                }
            }
            Some(rgba)
        }
        PixelLayout::Rgba8Unorm => None,
    }
}

/// RGB8 → BT.709 limited I420 (tight strides). Used by Gallery when `image-stream` is on.
pub fn rgba8_to_i420(width: u32, height: u32, rgba: &[u8]) -> Option<(Vec<u8>, Vec<u8>, Vec<u8>)> {
    let expected = (width as usize)
        .saturating_mul(height as usize)
        .saturating_mul(4);
    if rgba.len() != expected || width == 0 || height == 0 {
        return None;
    }
    let (cw, ch) = chroma_wh(width, height);
    let mut y = vec![0u8; (width as usize) * (height as usize)];
    let mut u = vec![0u8; (cw as usize) * (ch as usize)];
    let mut v = vec![0u8; (cw as usize) * (ch as usize)];
    let mut usum = vec![0u32; (cw as usize) * (ch as usize)];
    let mut vsum = vec![0u32; (cw as usize) * (ch as usize)];
    let mut nsum = vec![0u32; (cw as usize) * (ch as usize)];
    for py in 0..height {
        for px in 0..width {
            let i = ((py * width + px) * 4) as usize;
            let r = f32::from(rgba[i]);
            let g = f32::from(rgba[i + 1]);
            let b = f32::from(rgba[i + 2]);
            let yy = 16.0 + (65.738 * r + 129.057 * g + 25.064 * b) / 256.0;
            let cb = 128.0 + (-37.945 * r - 74.494 * g + 112.439 * b) / 256.0;
            let cr = 128.0 + (112.439 * r - 94.154 * g - 18.285 * b) / 256.0;
            y[(py * width + px) as usize] = yy.round().clamp(16.0, 235.0) as u8;
            let ci = ((py / 2) * cw + px / 2) as usize;
            usum[ci] = usum[ci].saturating_add(cb.round().clamp(16.0, 240.0) as u32);
            vsum[ci] = vsum[ci].saturating_add(cr.round().clamp(16.0, 240.0) as u32);
            nsum[ci] = nsum[ci].saturating_add(1);
        }
    }
    for i in 0..u.len() {
        let n = nsum[i].max(1);
        u[i] = (usum[i] / n) as u8;
        v[i] = (vsum[i] / n) as u8;
    }
    Some((y, u, v))
}

/// Borrow I420 planes as a [`MediaFrame`] (tight strides).
pub fn i420_frame<'a>(
    width: u32,
    height: u32,
    y: &'a [u8],
    u: &'a [u8],
    v: &'a [u8],
) -> MediaFrame<'a> {
    MediaFrame {
        width,
        height,
        layout: PixelLayout::I420,
        color: super::stream::TextureColorSpace::DisplaySrgb,
        pts: None,
        planes: MediaPlanes::Yuv {
            y,
            u,
            v,
            y_stride: width,
            uv_stride: width.div_ceil(2),
        },
    }
}

#[cfg(test)]
#[path = "tests/yuv.rs"]
mod tests;
