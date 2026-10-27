//! [`image`](https://github.com/image-rs/image) 核心：全格式 RGBA 解码、编码与 `imageops` 处理。

use std::io::Cursor;

use image::{imageops, DynamicImage, ImageFormat, ImageReader, RgbaImage};

use super::DecodedImage;

/// 用 `image` 解码任意已启用格式为 RGBA8（不含 CUR 热点语义）。
pub fn decode_raster(bytes: &[u8]) -> Option<DecodedImage> {
    let img = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?;
    dynamic_to_decoded(img)
}

/// 将 [`DecodedImage`] 转为 `image` 的 RGBA 缓冲。
pub fn to_rgba_image(img: &DecodedImage) -> Option<RgbaImage> {
    if img.width == 0
        || img.height == 0
        || img.rgba.len() != img.width as usize * img.height as usize * 4
    {
        return None;
    }
    RgbaImage::from_raw(img.width, img.height, img.rgba.clone())
}

pub(crate) fn dynamic_to_decoded(img: DynamicImage) -> Option<DecodedImage> {
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    if width == 0 || height == 0 {
        return None;
    }
    Some(DecodedImage {
        width,
        height,
        rgba: rgba.into_raw(),
        intrinsic_hotspot: None,
        animation: None,
    })
}

fn map_rgba(img: &DecodedImage, f: impl FnOnce(&RgbaImage) -> RgbaImage) -> Option<DecodedImage> {
    let src = to_rgba_image(img)?;
    dynamic_to_decoded(DynamicImage::ImageRgba8(f(&src)))
}

/// 重采样滤波器（re-export）。
pub use image::imageops::FilterType;

pub fn resize(
    img: &DecodedImage,
    width: u32,
    height: u32,
    filter: FilterType,
) -> Option<DecodedImage> {
    map_rgba(img, |src| imageops::resize(src, width, height, filter))
}

pub fn blur(img: &DecodedImage, sigma: f32) -> Option<DecodedImage> {
    map_rgba(img, |src| imageops::blur(src, sigma))
}

pub fn unsharpen(img: &DecodedImage, sigma: f32, threshold: i32) -> Option<DecodedImage> {
    map_rgba(img, |src| imageops::unsharpen(src, sigma, threshold))
}

pub fn brighten(img: &DecodedImage, value: i32) -> Option<DecodedImage> {
    map_rgba(img, |src| imageops::brighten(src, value))
}

pub fn contrast(img: &DecodedImage, value: f32) -> Option<DecodedImage> {
    map_rgba(img, |src| imageops::contrast(src, value))
}

pub fn huerotate(img: &DecodedImage, degrees: i32) -> Option<DecodedImage> {
    map_rgba(img, |src| imageops::huerotate(src, degrees))
}

pub fn grayscale(img: &DecodedImage) -> Option<DecodedImage> {
    let src = to_rgba_image(img)?;
    let gray = imageops::grayscale(&src);
    dynamic_to_decoded(DynamicImage::ImageLuma8(gray))
}

pub fn invert(img: &DecodedImage) -> Option<DecodedImage> {
    let mut rgba = to_rgba_image(img)?;
    imageops::invert(&mut rgba);
    dynamic_to_decoded(DynamicImage::ImageRgba8(rgba))
}

pub fn flip_horizontal(img: &DecodedImage) -> Option<DecodedImage> {
    map_rgba(img, imageops::flip_horizontal)
}

pub fn flip_vertical(img: &DecodedImage) -> Option<DecodedImage> {
    map_rgba(img, imageops::flip_vertical)
}

pub fn rotate90(img: &DecodedImage) -> Option<DecodedImage> {
    map_rgba(img, imageops::rotate90)
}

pub fn rotate180(img: &DecodedImage) -> Option<DecodedImage> {
    map_rgba(img, imageops::rotate180)
}

pub fn rotate270(img: &DecodedImage) -> Option<DecodedImage> {
    map_rgba(img, imageops::rotate270)
}

pub fn crop(img: &DecodedImage, x: u32, y: u32, width: u32, height: u32) -> Option<DecodedImage> {
    let src = to_rgba_image(img)?;
    let sub = imageops::crop_imm(&src, x, y, width, height).to_image();
    dynamic_to_decoded(DynamicImage::ImageRgba8(sub))
}

pub fn encode_png(img: &DecodedImage) -> Option<Vec<u8>> {
    encode(img, ImageFormat::Png)
}

#[cfg(feature = "image-jpeg")]
pub fn encode_jpeg(img: &DecodedImage, quality: u8) -> Option<Vec<u8>> {
    let rgba = to_rgba_image(img)?;
    let mut buf = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, quality);
    encoder
        .encode(
            &rgba,
            rgba.width(),
            rgba.height(),
            image::ExtendedColorType::Rgba8,
        )
        .ok()?;
    Some(buf)
}

#[cfg(not(feature = "image-jpeg"))]
pub fn encode_jpeg(_img: &DecodedImage, _quality: u8) -> Option<Vec<u8>> {
    None
}

#[cfg(feature = "image-webp")]
pub fn encode_webp(img: &DecodedImage) -> Option<Vec<u8>> {
    encode(img, ImageFormat::WebP)
}

#[cfg(not(feature = "image-webp"))]
pub fn encode_webp(_img: &DecodedImage) -> Option<Vec<u8>> {
    None
}

fn encode(img: &DecodedImage, format: ImageFormat) -> Option<Vec<u8>> {
    let rgba = to_rgba_image(img)?;
    let mut buf = Vec::new();
    DynamicImage::ImageRgba8(rgba)
        .write_to(&mut Cursor::new(&mut buf), format)
        .ok()?;
    Some(buf)
}

#[cfg(test)]
mod tests {
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
}
