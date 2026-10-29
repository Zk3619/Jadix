//! resvg / usvg 光栅化 — 与浏览器 SVG 渲染同栈。
//!
//! 本模块仅在 feature `svg` 下编译：供 Image 解码、Twemoji、Lucide Icon、公开 `<svg>` 叶共用。

use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Mutex;

use crate::app::pending_images::queue_image_rgba8;

const MAX_SVG_RASTER_CACHE: usize = 64;

struct SvgCpuEntry {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

thread_local! {
    static RASTER_CPU: RefCell<HashMap<String, SvgCpuEntry>> = RefCell::new(HashMap::new());
    static RASTER_NEEDED: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
}

thread_local! {
    static SVG_OPT: RefCell<Option<resvg::usvg::Options<'static>>> = const { RefCell::new(None) };
}

fn parse_usvg_tree(bytes: &[u8]) -> Option<resvg::usvg::Tree> {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    SVG_OPT.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_none() {
            let mut opt = resvg::usvg::Options::default();
            opt.fontdb_mut().load_system_fonts();
            opt.image_href_resolver.resolve_string = Box::new(|_, _| None);
            *slot = Some(opt);
        }
        resvg::usvg::Tree::from_data(bytes, slot.as_ref().unwrap()).ok()
    })
}

/// Parse SVG bytes and rasterize to RGBA8 at exact `out_w`×`out_h`.
pub fn rasterize_svg_to_rgba(bytes: &[u8], out_w: u32, out_h: u32) -> Option<Vec<u8>> {
    use resvg::tiny_skia;

    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    let tree = parse_usvg_tree(bytes)?;
    let size = tree.size();
    let nw = size.width().max(1.0);
    let nh = size.height().max(1.0);
    let out_w = out_w.max(1);
    let out_h = out_h.max(1);
    let mut pixmap = tiny_skia::Pixmap::new(out_w, out_h)?;
    let transform = tiny_skia::Transform::from_scale(out_w as f32 / nw, out_h as f32 / nh);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    // tiny-skia pixmap is premultiplied. Textured draws (`flags=0`) treat RGBA as
    // straight and premultiply in the shader when the swapchain is compositor-alpha.
    // Leaving premul here made Lucide strokes look like the wrong (washed) foreground
    // on frameless glass / transparent secondary windows.
    let mut rgba = pixmap.data().to_vec();
    unpremultiply_rgba8(&mut rgba);
    Some(rgba)
}

/// Convert premul RGBA8 → straight alpha so `textured_rect` can premul once.
fn unpremultiply_rgba8(pixels: &mut [u8]) {
    for px in pixels.chunks_exact_mut(4) {
        let a = px[3];
        if a == 0 {
            px[0] = 0;
            px[1] = 0;
            px[2] = 0;
            continue;
        }
        if a == 255 {
            continue;
        }
        let inv = 255.0 / a as f32;
        px[0] = (px[0] as f32 * inv).round().clamp(0.0, 255.0) as u8;
        px[1] = (px[1] as f32 * inv).round().clamp(0.0, 255.0) as u8;
        px[2] = (px[2] as f32 * inv).round().clamp(0.0, 255.0) as u8;
    }
}

/// Rasterize UTF-8 SVG markup to RGBA8 at exact output size.
pub fn rasterize_svg_str(svg: &str, out_w: u32, out_h: u32) -> Option<Vec<u8>> {
    rasterize_svg_to_rgba(svg.as_bytes(), out_w, out_h)
}

/// Rasterize at the SVG's intrinsic size (no scaling cap).
pub fn rasterize_svg_natural(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    let tree = parse_usvg_tree(bytes)?;
    let size = tree.size();
    let out_w = size.width().round().max(1.0) as u32;
    let out_h = size.height().round().max(1.0) as u32;
    let rgba = rasterize_svg_to_rgba(bytes, out_w, out_h)?;
    Some((out_w, out_h, rgba))
}

/// Fit longest edge to `max_edge`, preserving aspect ratio.
pub fn rasterize_svg_fit(bytes: &[u8], max_edge: u32) -> Option<(u32, u32, Vec<u8>)> {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    let tree = parse_usvg_tree(bytes)?;
    let size = tree.size();
    let nw = size.width().max(1.0);
    let nh = size.height().max(1.0);
    let longest = nw.max(nh);
    let max = max_edge.max(1) as f32;
    let scale = if longest > max { max / longest } else { 1.0 };
    let out_w = (nw * scale).round().clamp(1.0, max) as u32;
    let out_h = (nh * scale).round().clamp(1.0, max) as u32;
    let rgba = rasterize_svg_to_rgba(bytes, out_w, out_h)?;
    Some((out_w, out_h, rgba))
}

fn hash_svg_doc(svg: &str, width: u32, height: u32) -> String {
    let mut hasher = DefaultHasher::new();
    svg.hash(&mut hasher);
    width.hash(&mut hasher);
    height.hash(&mut hasher);
    format!("jadix_svg:{:016x}", hasher.finish())
}

/// Rasterize SVG document string, cache CPU RGBA, queue GPU upload. Same lifecycle as path textures.
pub fn ensure_svg_document_texture(svg: &str, width: u32, height: u32) -> Option<String> {
    ensure_svg_document_texture_with_key(hash_svg_doc(svg, width, height), svg, width, height)
}

/// The key must identify the document, dimensions and paint parameters.
/// Animation callers keep their frame-key namespace while sharing raster/upload reuse.
pub fn ensure_svg_document_texture_with_key(
    tex_id: String,
    svg: &str,
    width: u32,
    height: u32,
) -> Option<String> {
    if svg.trim().is_empty() || width < 1 || height < 1 {
        return None;
    }
    if width.saturating_mul(height) > 1024 * 1024 {
        return None;
    }
    RASTER_CPU.with(|c| {
        let mut g = c.borrow_mut();
        if !g.contains_key(&tex_id) {
            if g.len() >= MAX_SVG_RASTER_CACHE {
                // Do not discard rasters referenced earlier in this collect: flush
                // still needs them to upload the complete frame. Like TexturePool,
                // allow a temporary overshoot when all entries are in use.
                let evict = RASTER_NEEDED.with(|n| {
                    let needed = n.borrow();
                    g.keys().find(|key| !needed.contains(*key)).cloned()
                });
                if let Some(key) = evict {
                    g.remove(&key);
                }
            }
            let rgba = rasterize_svg_str(svg, width, height)?;
            g.insert(
                tex_id.clone(),
                SvgCpuEntry {
                    width,
                    height,
                    rgba,
                },
            );
        }
        Some(())
    })?;
    RASTER_NEEDED.with(|n| {
        n.borrow_mut().insert(tex_id.clone());
    });
    Some(tex_id)
}

/// Re-queue SVG rasters evicted from the GPU pool.
pub fn flush_svg_textures(mut has_gpu: impl FnMut(&str) -> bool) -> bool {
    let needed: Vec<String> = RASTER_NEEDED
        .with(|n| std::mem::take(&mut *n.borrow_mut()))
        .into_iter()
        .collect();
    let mut any = false;
    RASTER_CPU.with(|c| {
        let map = c.borrow();
        for key in needed {
            if has_gpu(&key) {
                continue;
            }
            if let Some(entry) = map.get(&key) {
                queue_image_rgba8(&key, entry.width, entry.height, entry.rgba.clone());
                any = true;
            }
        }
    });
    any
}

/// Lucide default root + inner markup → browser-grade SVG document.
pub fn lucide_svg_document(inner: &str, color: [u8; 4]) -> String {
    let stroke = if color[3] == 255 {
        format!("rgb({},{},{})", color[0], color[1], color[2])
    } else {
        format!(
            "rgba({},{},{},{:.4})",
            color[0],
            color[1],
            color[2],
            color[3] as f32 / 255.0
        )
    };
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" color="{stroke}" stroke="{stroke}" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">{inner}</svg>"#
    )
}

static LUCIDE_CPU: Mutex<Option<HashMap<String, SvgCpuEntry>>> = Mutex::new(None);

/// Lucide icon name → cached RGBA via resvg (DPR supersample).
pub fn rasterize_lucide_icon_rgba(
    inner: &str,
    color: [u8; 4],
    side: u32,
) -> Option<(u32, u32, Vec<u8>)> {
    let doc = lucide_svg_document(inner, color);
    let rgba = rasterize_svg_str(&doc, side, side)?;
    Some((side, side, rgba))
}

/// Lucide icon cache (shared with icon_texture when `svg` feature on).
pub fn lucide_icon_cache_get(key: &str) -> Option<(u32, u32, Vec<u8>)> {
    let guard = LUCIDE_CPU.lock().ok()?;
    let map = guard.as_ref()?;
    let e = map.get(key)?;
    Some((e.width, e.height, e.rgba.clone()))
}

pub fn lucide_icon_cache_put(key: String, w: u32, h: u32, rgba: Vec<u8>) {
    if let Ok(mut guard) = LUCIDE_CPU.lock() {
        let map = guard.get_or_insert_with(HashMap::new);
        if map.len() >= 256 {
            map.clear();
        }
        map.insert(
            key,
            SvgCpuEntry {
                width: w,
                height: h,
                rgba,
            },
        );
    }
}

#[cfg(test)]
#[path = "tests/svg_raster.rs"]
mod tests;
