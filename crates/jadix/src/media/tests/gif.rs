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
