//! Lucide glyph → cached RGBA texture (for CSS transform / `animate-spin`).
//!
//! Glyphon `TextArea` cannot rotate. When an Icon needs `rotate_deg` / spin, we rasterize
//! the lucide outline once (size+color keyed) and draw via [`TexturedDraw`] + element
//! transform — same path SVG/path leaves will use once icons ship as vectors.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Mutex;

use swash::scale::{Render, ScaleContext, Source};
use swash::zeno::Format;
use swash::FontRef;

use crate::app::pending_images::queue_image_rgba8;

#[cfg(feature = "bundled-lucide")]
static LUCIDE_TTF: &[u8] = include_bytes!("../../assets/lucide.ttf");

struct GlyphCpuEntry {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

static GLYPH_CPU: Mutex<Option<HashMap<String, GlyphCpuEntry>>> = Mutex::new(None);
const MAX_GLYPH_CACHE: usize = 256;

fn font_ref() -> Option<FontRef<'static>> {
    #[cfg(feature = "bundled-lucide")]
    {
        FontRef::from_index(LUCIDE_TTF, 0)
    }
    #[cfg(not(feature = "bundled-lucide"))]
    {
        None
    }
}

fn rasterize_glyph_rgba(ch: char, size_px: f32, color: [u8; 4]) -> Option<(u32, u32, Vec<u8>)> {
    let font = font_ref()?;
    let glyph_id = font.charmap().map(ch);
    if glyph_id == 0 {
        return None;
    }
    let size = size_px.clamp(8.0, 256.0);
    // Unhinted: hinted stems shift the ink bbox; spin then wobbles about a false center.
    let mut context = ScaleContext::new();
    let mut scaler = context.builder(font).size(size).hint(false).build();
    let image = Render::new(&[Source::Outline])
        .format(Format::Alpha)
        .render(&mut scaler, glyph_id)?;
    let w = image.placement.width.max(1);
    let h = image.placement.height.max(1);
    if w > 512 || h > 512 {
        return None;
    }
    // Lucide EM is square (ascent=upem, descent=0). Place by swash bearings so the
    // *design* center stays at the texture center — do NOT center the tight ink bbox
    // (loader-circle's gap makes that bbox off-center → vertical bob while spinning).
    let side = size.ceil().max(1.0) as u32;
    let mut rgba = vec![0u8; (side * side * 4) as usize];
    // Pen at (0, side) in top-down; placement.top is above baseline (y-up).
    let ox0 = image.placement.left;
    let oy0 = side as i32 - image.placement.top;
    let mask = image.data;
    for y in 0..h {
        for x in 0..w {
            let a = mask[(y * w + x) as usize];
            if a == 0 {
                continue;
            }
            let dx = ox0 + x as i32;
            let dy = oy0 + y as i32;
            if dx < 0 || dy < 0 || dx >= side as i32 || dy >= side as i32 {
                continue;
            }
            let i = ((dy as u32 * side + dx as u32) * 4) as usize;
            let cover = a as u32;
            rgba[i] = ((color[0] as u32 * cover) / 255) as u8;
            rgba[i + 1] = ((color[1] as u32 * cover) / 255) as u8;
            rgba[i + 2] = ((color[2] as u32 * cover) / 255) as u8;
            rgba[i + 3] = ((color[3] as u32 * cover) / 255) as u8;
        }
    }
    Some((side, side, rgba))
}

/// Ensure a GPU-queued lucide glyph texture. Returns `(texture_key, side_px)`.
pub fn ensure_lucide_glyph_texture(
    glyph: &str,
    size_px: f32,
    color: [u8; 4],
) -> Option<(String, f32)> {
    let ch = glyph.chars().next()?;
    let size_key = (size_px * 100.0).round() as i32;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (ch as u32).hash(&mut hasher);
    size_key.hash(&mut hasher);
    color.hash(&mut hasher);
    // v2: EM-bearing placement (not ink-bbox center / placement.left hack).
    let key = format!("rui_lucide_v2:{:016x}", hasher.finish());

    let mut guard = GLYPH_CPU.lock().ok()?;
    let map = guard.get_or_insert_with(HashMap::new);
    if !map.contains_key(&key) {
        if map.len() >= MAX_GLYPH_CACHE {
            map.clear();
        }
        let (w, h, rgba) = rasterize_glyph_rgba(ch, size_px, color)?;
        map.insert(
            key.clone(),
            GlyphCpuEntry {
                width: w,
                height: h,
                rgba: rgba.clone(),
            },
        );
        queue_image_rgba8(&key, w, h, rgba);
    } else if let Some(entry) = map.get(&key) {
        // Re-queue in case TexturePool evicted the GPU copy.
        queue_image_rgba8(&key, entry.width, entry.height, entry.rgba.clone());
    }
    let side = map.get(&key)?.width as f32;
    Some((key, side))
}

#[cfg(all(test, feature = "bundled-lucide"))]
mod tests {
    use super::*;

    #[test]
    fn loader_circle_ink_bbox_near_em_center() {
        let hex = "e10a"; // lucide.json loader-circle
        let ch = char::from_u32(u32::from_str_radix(hex, 16).unwrap()).unwrap();
        let (side, _, rgba) = rasterize_glyph_rgba(ch, 32.0, [255, 255, 255, 255]).expect("raster");
        assert_eq!(side, 32);
        let mut min_x = side;
        let mut min_y = side;
        let mut max_x = 0u32;
        let mut max_y = 0u32;
        for y in 0..side {
            for x in 0..side {
                if rgba[((y * side + x) * 4 + 3) as usize] == 0 {
                    continue;
                }
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
        assert!(max_x >= min_x);
        let cx = (min_x + max_x) as f32 * 0.5;
        let cy = (min_y + max_y) as f32 * 0.5;
        let mid = (side as f32 - 1.0) * 0.5;
        assert!(
            (cx - mid).abs() < 1.0 && (cy - mid).abs() < 1.0,
            "ink bbox center ({cx:.2},{cy:.2}) far from EM center {mid}"
        );
    }
}