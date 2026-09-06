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
mod tests {
    use super::*;
    use crate::app::pending_images::take_pending_key;

    #[test]
    fn rgba8_helper_enqueues() {
        let px = [10u8, 20, 30, 255];
        queue_media_frame("rui-test-media-rgba", MediaFrame::rgba8(1, 1, &px));
        let got = take_pending_key("rui-test-media-rgba");
        assert_eq!(
            got.as_ref().map(|(b, w, h)| (b.as_slice(), *w, *h)),
            Some((px.as_slice(), 1, 1))
        );
    }

    #[test]
    fn short_buffer_is_dropped() {
        queue_media_frame(
            "rui-test-media-short",
            MediaFrame::rgba8(2, 2, &[0, 0, 0, 255]),
        );
        assert!(
            take_pending_key("rui-test-media-short").is_none(),
            "short buffer must not enqueue"
        );
    }

    #[test]
    fn zero_size_is_dropped() {
        queue_media_frame("rui-test-media-zero", MediaFrame::rgba8(0, 1, &[]));
        assert!(take_pending_key("rui-test-media-zero").is_none());
    }

    #[test]
    fn yuv_i420_feature_gate() {
        let y = [126u8; 4];
        let u = [128u8; 1];
        let v = [128u8; 1];
        queue_media_frame("rui-test-media-i420", MediaFrame::i420(2, 2, &y, &u, &v));
        let got = take_pending_key("rui-test-media-i420");
        #[cfg(feature = "image-stream")]
        {
            let (bytes, w, h) = got.expect("i420 uploads when image-stream is on");
            assert_eq!((w, h), (2, 2));
            assert_eq!(bytes.len(), 16);
            assert_eq!(bytes[3], 255);
        }
        #[cfg(not(feature = "image-stream"))]
        {
            assert!(got.is_none(), "YUV must drop without image-stream");
        }
    }
}
