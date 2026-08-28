//! GIF 全帧解码与合成（供 `<Image src="*.gif">` 动画播放）。
//!
//! 经 [`image::codecs::gif::GifDecoder`] 解码；`gif` crate 仅作 `image` 传递依赖。

use std::io::Cursor;
use std::time::Duration;

use image::codecs::gif::GifDecoder;
use image::{AnimationDecoder, ImageDecoder};

use super::animation::{DecodedAnimation, DecodedFrame};
use super::DecodedImage;

pub fn is_gif(bytes: &[u8]) -> bool {
    bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")
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

fn frame_delay_cs(delay: image::Delay) -> u16 {
    let ms = Duration::from(delay).as_millis();
    let cs = ms / 10;
    cs.min(u16::MAX as u128) as u16
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::codecs::gif::{GifEncoder, Repeat};
    use image::{Delay, Frame, ImageBuffer, Rgba};

    fn encode_test_gif(frames: &[(u8, u8, u8, u16)]) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut enc = GifEncoder::new(&mut buf);
            enc.set_repeat(Repeat::Infinite).unwrap();
            for &(r, g, b, delay_cs) in frames {
                let buffer = ImageBuffer::from_pixel(2, 1, Rgba([r, g, b, 255]));
                let delay = Delay::from_numer_denom_ms(u32::from(delay_cs) * 10, 1);
                enc.encode_frame(Frame::from_parts(buffer, 0, 0, delay))
                    .unwrap();
            }
        }
        buf
    }

    #[test]
    fn multi_frame_gif_decodes() {
        let bytes = encode_test_gif(&[(255, 0, 0, 2), (0, 255, 0, 2)]);
        let anim = decode_gif_animation(&bytes).unwrap();
        assert_eq!(anim.frames.len(), 2);
        assert!(anim.is_animated());
        let img = decode_gif(&bytes).unwrap();
        assert!(img.is_animated());
    }
}
