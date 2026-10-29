use super::*;
use crate::media::{decode_bytes, DecodeOptions};

fn solid_png(w: u32, h: u32, r: u8, g: u8, b: u8) -> Vec<u8> {
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for _ in 0..(w * h) {
        rgba.extend_from_slice(&[r, g, b, 255]);
    }
    encode_png(&DecodedImage {
        width: w,
        height: h,
        rgba,
        intrinsic_hotspot: None,
        animation: None,
    })
    .unwrap()
}

#[cfg(not(feature = "image-jpeg"))]
#[test]
fn disabled_jpeg_encoder_returns_none() {
    let image = DecodedImage {
        width: 1,
        height: 1,
        rgba: vec![255; 4],
        intrinsic_hotspot: None,
        animation: None,
    };
    assert!(encode_jpeg(&image, 80).is_none());
}

#[cfg(not(feature = "image-webp"))]
#[test]
fn disabled_webp_encoder_returns_none() {
    let image = DecodedImage {
        width: 1,
        height: 1,
        rgba: vec![255; 4],
        intrinsic_hotspot: None,
        animation: None,
    };
    assert!(encode_webp(&image).is_none());
}

#[cfg(not(feature = "image-gif"))]
#[test]
fn disabled_gif_decoder_returns_none_without_loading_frames() {
    let image = b"GIF89a\x01\x00\x01\x00\x80\x00\x00\x00\x00\x00\xff\xff\xff\x21\xf9\x04\x01\x00\x00\x00\x00\x2c\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02\x44\x01\x00\x3b";
    assert!(super::super::gif::is_gif(image));
    assert!(super::super::gif::decode_gif_animation(image).is_none());
}

#[test]
fn png_roundtrip_via_image_core() {
    let png = solid_png(2, 2, 255, 0, 0);
    let d = decode_bytes(&png, DecodeOptions::image()).unwrap();
    assert_eq!((d.width, d.height), (2, 2));
    assert_eq!(&d.rgba[0..4], &[255, 0, 0, 255]);
}

#[test]
fn resize_and_grayscale() {
    let src = DecodedImage {
        width: 2,
        height: 1,
        rgba: vec![255, 0, 0, 255, 0, 255, 0, 255],
        intrinsic_hotspot: None,
        animation: None,
    };
    let small = resize(&src, 1, 1, FilterType::Nearest).unwrap();
    assert_eq!((small.width, small.height), (1, 1));
    let gray = grayscale(&src).unwrap();
    assert_eq!(gray.rgba.len(), src.rgba.len());
}
