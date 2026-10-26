//! 位图解码：供 [`crate::components::Image`]、`queue_image_*` 与 CSS `cursor: url()` 共用。
//! SVG 字节解码见 feature `svg`（`media::svg_raster`）。
//!
//! 光栅格式由 [`image`](https://github.com/image-rs/image) 解码（默认 PNG / JPEG / WebP / GIF / ICO；
//! 更多格式见 `image-*` features）。`.cur` 热点与 SVG 光栅化走专用路径。
//! GIF 多帧动画由 [`gif`] 模块经 `image` 解码并在帧循环中播放。
//! 动态纹理槽（不解码）见 [`stream`]。

mod animation;
mod decode;
pub mod gif;
pub mod image;
pub mod playback;
pub(crate) mod remote;
pub mod stream;
pub mod texture;
#[cfg(feature = "svg")]
pub mod svg_raster;
pub mod yuv;

pub use animation::{DecodedAnimation, DecodedFrame};
pub use decode::{
    decode_bytes, decode_data_url, load_src, resolve_asset_path, DecodeOptions, DecodedImage,
};
pub use image::{
    blur, brighten, contrast, crop, encode_jpeg, encode_png, encode_webp, flip_horizontal,
    flip_vertical, grayscale, huerotate, invert, resize, rotate180, rotate270, rotate90,
    to_rgba_image, unsharpen, FilterType,
};
pub use remote::{
    clear_remote_asset_fetcher, fetch_remote_bytes, is_remote_src, remote_asset_enabled,
    remote_asset_fetcher_installed, set_remote_asset_fetcher, RemoteAssetFetcher,
};
pub use stream::{queue_media_frame, MediaFrame, MediaPlanes, PixelLayout, TextureColorSpace};
pub use playback::*;
pub use yuv::{bt709_yuv8_to_rgb, i420_frame, rgba8_to_i420};
