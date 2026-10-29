//! Dynamic texture / ImageStream slot (WP-M.2 / WP-M.3).
//!
//! Does **not** decode video. Applications push frames; [`crate::components::Image`]
//! displays them via `texture`. See [`docs/engine/media.md`](../../../docs/engine/media.md).

use std::time::Duration;

use crate::app::pending_images::queue_image_rgba8;

use super::yuv::media_frame_to_rgba8;

/// Color tag on a streamed or static GPU texture. P3/HDR variants arrive with later WPs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextureColorSpace {
    /// Today's default; unmarked bitmaps.
    #[default]
    DisplaySrgb,
}

/// Packed pixel layout accepted by [`queue_media_frame`].
///
/// YUV variants require feature `image-stream`; without it those frames are dropped.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PixelLayout {
    #[default]
    Rgba8Unorm,
    Nv12,
    I420,
    P010,
}

/// Plane storage for [`MediaFrame`].
#[derive(Clone, Copy, Debug)]
pub enum MediaPlanes<'a> {
    Packed(&'a [u8]),
    /// I420: `y`/`u`/`v` separate. NV12 / P010: interleaved chroma in `u`, `v` unused.
    Yuv {
        y: &'a [u8],
        u: &'a [u8],
        v: &'a [u8],
        y_stride: u32,
        uv_stride: u32,
    },
}

/// One frame to upload. Invalid payloads are dropped (previous GPU contents kept).
#[derive(Clone, Copy, Debug)]
pub struct MediaFrame<'a> {
    pub width: u32,
    pub height: u32,
    pub layout: PixelLayout,
    pub color: TextureColorSpace,
    pub pts: Option<Duration>,
    pub planes: MediaPlanes<'a>,
}

impl<'a> MediaFrame<'a> {
    /// Packed RGBA8, display sRGB, no timestamp.
    pub fn rgba8(width: u32, height: u32, rgba: &'a [u8]) -> Self {
        Self {
            width,
            height,
            layout: PixelLayout::Rgba8Unorm,
            color: TextureColorSpace::DisplaySrgb,
            pts: None,
            planes: MediaPlanes::Packed(rgba),
        }
    }

    /// I420 (BT.709 limited). Tight strides. Requires `image-stream` to upload.
    pub fn i420(width: u32, height: u32, y: &'a [u8], u: &'a [u8], v: &'a [u8]) -> Self {
        Self {
            width,
            height,
            layout: PixelLayout::I420,
            color: TextureColorSpace::DisplaySrgb,
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

    /// NV12 (BT.709 limited). `uv` is interleaved chroma. Requires `image-stream`.
    pub fn nv12(width: u32, height: u32, y: &'a [u8], uv: &'a [u8]) -> Self {
        Self {
            width,
            height,
            layout: PixelLayout::Nv12,
            color: TextureColorSpace::DisplaySrgb,
            pts: None,
            planes: MediaPlanes::Yuv {
                y,
                u: uv,
                v: &[],
                y_stride: width,
                uv_stride: width,
            },
        }
    }
}

/// Queue a frame for GPU upload. Same `key` keeps only the latest pending frame.
///
/// Not a decoder or player. YUV layouts need feature `image-stream` (BT.709 limited
/// → RGBA, then the same in-place `write_texture` path as RGBA). HDR tags are dropped.
/// Short or oversized buffers are dropped without panicking.
pub fn queue_media_frame(key: impl Into<String>, frame: MediaFrame<'_>) {
    if frame.width == 0 || frame.height == 0 {
        return;
    }
    if frame.color != TextureColorSpace::DisplaySrgb {
        return;
    }
    let _ = frame.pts;
    let Some(rgba) = media_frame_to_rgba8(&frame) else {
        return;
    };
    queue_image_rgba8(key, frame.width, frame.height, rgba);
}

#[cfg(test)]
#[path = "tests/stream.rs"]
mod tests;
