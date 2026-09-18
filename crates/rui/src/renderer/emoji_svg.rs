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
    let path = svg_dir().join(svg_filename(sequence));
    if !path.exists() {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let (w, h, rgba) =
        crate::media::svg_raster::rasterize_svg_fit(&bytes, pixel_size.max(16) as u32)?;
    Some(EmojiRaster::new(w as u16, h as u16, rgba))
}

#[cfg(not(feature = "emoji-svg"))]
pub(crate) fn load_svg_raster(sequence: &str, pixel_size: u16) -> Option<EmojiRaster> {
    let _ = (sequence, pixel_size);
    None
}
