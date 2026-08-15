//! 位图 / SVG 解码：供 [`crate::components::Image`]、`queue_image_*` 与 CSS `cursor: url()` 共用。
//!
//! PNG / ICO / CUR / BMP 默认可用。JPEG / GIF / WebP / SVG 见 crate features
//! `jpeg` / `gif` / `webp` / `svg`（`images` 打开前三者；`emoji-svg` 依赖 `svg`）。
//! 不发起 `http(s)` 请求。

mod decode;

pub use decode::{
    decode_bytes, decode_data_url, load_src, resolve_asset_path, DecodeOptions, DecodedImage,
    MAX_CURSOR_EDGE, MAX_IMAGE_EDGE,
};
