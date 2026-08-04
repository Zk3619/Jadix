use super::text::EmojiRaster;
use super::twemoji_name::twemoji_stem;

pub fn svg_filename(sequence: &str) -> String {
    format!("{}.svg", twemoji_stem(sequence))
}

#[cfg(feature = "emoji-svg")]
fn svg_dir() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("RUI_TWEMOJI_SVG_DIR") {
        let p = std::path::PathBuf::from(dir);
        if p.exists() {
            return p;
        }
    }
    let candidates = [
        std::path::PathBuf::from("assets/twemoji/svg"),
        std::path::PathBuf::from("crates/rui/assets/twemoji/svg"),
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/twemoji/svg"),
    ];
    for c in candidates {
        if c.is_dir() {
            return c;
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let p = parent.join("assets/twemoji/svg");
            if p.is_dir() {
                return p;
            }
        }
    }
    std::path::PathBuf::from("assets/twemoji/svg")
}

#[cfg(feature = "emoji-svg")]
pub(crate) fn load_svg_raster(sequence: &str, pixel_size: u16) -> Option<EmojiRaster> {
    use resvg::{tiny_skia, usvg};
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

#[cfg(not(feature = "emoji-svg"))]
pub(crate) fn load_svg_raster(sequence: &str, pixel_size: u16) -> Option<EmojiRaster> {
    let _ = (sequence, pixel_size);
    None
}
