//! 位图 / SVG 解码：供 [`crate::components::Image`]、`queue_image_*` 与 CSS `cursor: url()` 共用。
//!
//! 光栅格式由 [`image`](https://github.com/image-rs/image) 解码（默认 PNG / JPEG / WebP / GIF / ICO；
//! 更多格式见 `image-*` features）。`.cur` 热点与 SVG 光栅化走专用路径。
//! GIF 多帧动画由 [`gif`] 模块经 `image` 解码并在帧循环中播放。

mod animation;
mod decode;
pub mod gif;
pub mod image;
pub mod svg_raster;

pub use animation::{DecodedAnimation, DecodedFrame};
pub use decode::{
    decode_bytes, decode_data_url, load_src, resolve_asset_path, DecodeOptions, DecodedImage,
};
pub use image::{
    blur, brighten, contrast, crop, encode_jpeg, encode_png, encode_webp, flip_horizontal,
    flip_vertical, grayscale, huerotate, invert, resize, rotate180, rotate270, rotate90, unsharpen,
    to_rgba_image, FilterType,
};
