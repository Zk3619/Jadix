use std::path::PathBuf;

use resvg::{tiny_skia, usvg};

use super::text::EmojiRaster;
use super::twemoji_name::twemoji_stem;

fn svg_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("RUI_TWEMOJI_SVG_DIR") {
        let p = PathBuf::from(dir);
        if p.exists() {
            return p;
        }
    }
    PathBuf::from("assets/twemoji/svg")
}

pub fn svg_filename(sequence: &str) -> String {
    format!("{}.svg", twemoji_stem(sequence))
}

pub(crate) fn load_svg_raster(sequence: &str, pixel_size: u16) -> Option<EmojiRaster> {
    let path = svg_dir().join(svg_filename(sequence));
    if !path.exists() {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_data(&bytes, &opt).ok()?;
    let size = tree.size();
    let longest = size.width().max(size.height()).max(1.0);
    let target = pixel_size.max(16) as f32;
    let scale = target / longest;
    let out_w = (size.width() * scale).ceil().max(1.0) as u32;
    let out_h = (size.height() * scale).ceil().max(1.0) as u32;
    let mut pixmap = tiny_skia::Pixmap::new(out_w, out_h)?;
    let transform = tiny_skia::Transform::from_scale(scale, scale);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    Some(EmojiRaster::new(
        out_w as u16,
        out_h as u16,
        pixmap.data().to_vec(),
    ))
}
