//! GIF 全帧解码与合成（供 `<Image src="*.gif">` 动画播放）。
//!
//! 经 [`image::codecs::gif::GifDecoder`] 解码；`gif` crate 仅作 `image` 传递依赖。

#[cfg(feature = "image-gif")]
use std::io::Cursor;
#[cfg(feature = "image-gif")]
use std::time::Duration;

#[cfg(feature = "image-gif")]
use image::codecs::gif::GifDecoder;
#[cfg(feature = "image-gif")]
use image::{AnimationDecoder, ImageDecoder};

#[cfg(feature = "image-gif")]
use super::animation::{DecodedAnimation, DecodedFrame};
use super::DecodedImage;

pub fn is_gif(bytes: &[u8]) -> bool {
    bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")
}

#[cfg(not(feature = "image-gif"))]
pub fn decode_gif_animation(_bytes: &[u8]) -> Option<super::animation::DecodedAnimation> {
    None
}

/// 解码 GIF 为合成后的全画布帧序列。
pub fn decode_gif(bytes: &[u8]) -> Option<DecodedImage> {
    let animation = decode_gif_animation(bytes)?;
    let first = animation.frames.first()?;
    let animated = animation.is_animated();
    Some(DecodedImage {
        width: animation.width,
        height: animation.height,
        rgba: first.rgba.clone(),
        intrinsic_hotspot: None,
        animation: animated.then_some(animation),
    })
}

#[cfg(feature = "image-gif")]
pub fn decode_gif_animation(bytes: &[u8]) -> Option<DecodedAnimation> {
    let decoder = GifDecoder::new(Cursor::new(bytes)).ok()?;
    let (width, height) = decoder.dimensions();
    if width == 0 || height == 0 {
        return None;
    }
    let frames = decoder.into_frames().collect_frames().ok()?;
    if frames.is_empty() {
        return None;
    }
    let decoded: Vec<DecodedFrame> = frames
        .into_iter()
        .map(|frame| {
            let delay_cs = frame_delay_cs(frame.delay());
            DecodedFrame {
                rgba: frame.into_buffer().into_raw(),
                delay_cs,
            }
        })
        .collect();
    Some(DecodedAnimation {
        width,
        height,
        frames: decoded,
    })
}

#[cfg(feature = "image-gif")]
fn frame_delay_cs(delay: image::Delay) -> u16 {
    let ms = Duration::from(delay).as_millis();
    let cs = ms / 10;
    cs.min(u16::MAX as u128) as u16
}

#[cfg(all(test, feature = "image-gif"))]
#[path = "tests/gif.rs"]
mod tests;
