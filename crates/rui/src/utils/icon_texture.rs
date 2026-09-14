//! Lucide Icon → cached RGBA texture (for CSS transform / `animate-spin`).
//!
//! Glyphon `TextArea` cannot rotate. With `svg` feature: resvg (browser-grade curves).
//! Without: fall back to [`crate::primitives::path`] or font outlines.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Mutex;

use swash::scale::{Render, ScaleContext, Source};
use swash::zeno::Format;
use swash::FontRef;

use crate::app::pending_images::queue_image_rgba8;
use crate::utils::lucide::lucide_svg_inner;

#[cfg(not(feature = "svg"))]
use crate::primitives::path::{
    decode_path_d, filled_circle, filled_rect, polyline, rasterize_layers_rgba, PathCommand,
    PathStyle, Point2, StrokeLinecap, StrokeLinejoin,
};

#[cfg(feature = "bundled-lucide")]
static LUCIDE_TTF: &[u8] = include_bytes!("../../assets/lucide.ttf");

struct GlyphCpuEntry {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

static GLYPH_CPU: Mutex<Option<HashMap<String, GlyphCpuEntry>>> = Mutex::new(None);
const MAX_GLYPH_CACHE: usize = 256;
#[cfg(not(feature = "svg"))]
const LUCIDE_VB: f32 = 24.0;

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

#[cfg(not(feature = "svg"))]
fn color_f32(c: [u8; 4]) -> [f32; 4] {
    [
        c[0] as f32 / 255.0,
        c[1] as f32 / 255.0,
        c[2] as f32 / 255.0,
        c[3] as f32 / 255.0,
    ]
}

#[cfg(not(feature = "svg"))]
fn scale_cmds(cmds: &[PathCommand], s: f32) -> Vec<PathCommand> {
    cmds.iter()
        .cloned()
        .map(|c| match c {
            PathCommand::Arc {
                radii,
                x_rot_deg,
                large,
                sweep,
                to,
            } => PathCommand::Arc {
                radii: (radii.0 * s, radii.1 * s),
                x_rot_deg,
                large,
                sweep,
                to: (to.0 * s, to.1 * s),
            },
            other => other.map_xy(|(x, y)| (x * s, y * s)),
        })
        .collect()
}

#[cfg(not(feature = "svg"))]
fn attr_map(tag_attrs: &str) -> HashMap<String, String> {
    let mut m = HashMap::new();
    let mut rest = tag_attrs;
    while let Some(eq) = rest.find('=') {
        let key = rest[..eq].trim();
        let after = rest[eq + 1..].trim_start();
        let (val, next) = if let Some(stripped) = after.strip_prefix('"') {
            if let Some(end) = stripped.find('"') {
                (&stripped[..end], &stripped[end + 1..])
            } else {
                break;
            }
        } else if let Some(stripped) = after.strip_prefix('\'') {
            if let Some(end) = stripped.find('\'') {
                (&stripped[..end], &stripped[end + 1..])
            } else {
                break;
            }
        } else {
            break;
        };
        if let Some(k) = key.split_whitespace().last() {
            m.insert(k.to_string(), val.to_string());
        }
        rest = next;
    }
    m
}

#[cfg(not(feature = "svg"))]
fn parse_f(attrs: &HashMap<String, String>, key: &str, default: f32) -> f32 {
    attrs
        .get(key)
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

/// Parse Lucide inner markup into path layers (fallback when `svg` feature off).
#[cfg(not(feature = "svg"))]
pub fn lucide_inner_to_layers(
    inner: &str,
    color: [f32; 4],
    stroke_width: f32,
) -> Vec<(Vec<PathCommand>, PathStyle)> {
    let style = PathStyle::stroke(color, stroke_width)
        .linecap(StrokeLinecap::Round)
        .linejoin(StrokeLinejoin::Round);
    let mut layers = Vec::new();
    let mut rest = inner;
    while let Some(start) = rest.find('<') {
        rest = &rest[start + 1..];
        if rest.starts_with('/') {
            if let Some(gt) = rest.find('>') {
                rest = &rest[gt + 1..];
            }
            continue;
        }
        let name_end = rest
            .find(|c: char| c.is_whitespace() || c == '/' || c == '>')
            .unwrap_or(rest.len());
        let tag = rest[..name_end].to_ascii_lowercase();
        let after_name = &rest[name_end..];
        let self_close = after_name.find("/>");
        let open_end = after_name.find('>');
        let (attrs_src, next) = if let Some(sc) = self_close {
            if open_end.map(|o| o < sc).unwrap_or(false) {
                let o = open_end.unwrap();
                (&after_name[..o], &after_name[o + 1..])
            } else {
                (&after_name[..sc], &after_name[sc + 2..])
            }
        } else if let Some(o) = open_end {
            (&after_name[..o], &after_name[o + 1..])
        } else {
            break;
        };
        rest = next;
        let attrs = attr_map(attrs_src);
        match tag.as_str() {
            "path" => {
                if let Some(d) = attrs.get("d") {
                    let cmds = decode_path_d(d);
                    if !cmds.is_empty() {
                        let fill_none = attrs
                            .get("fill")
                            .map(|s| s.eq_ignore_ascii_case("none"))
                            .unwrap_or(true);
                        let mut st = style.clone();
                        if !fill_none {
                            st.fill = Some(color);
                        }
                        layers.push((cmds, st));
                    }
                }
            }
            "circle" => {
                let cx = parse_f(&attrs, "cx", 0.0);
                let cy = parse_f(&attrs, "cy", 0.0);
                let r = parse_f(&attrs, "r", 0.0);
                if r > 0.0 {
                    layers.push((filled_circle(cx, cy, r, 48), style.clone()));
                }
            }
            "ellipse" => {
                let cx = parse_f(&attrs, "cx", 0.0);
                let cy = parse_f(&attrs, "cy", 0.0);
                let rx = parse_f(&attrs, "rx", 0.0);
                let ry = parse_f(&attrs, "ry", 0.0);
                if rx > 0.0 && ry > 0.0 {
                    let n = 48usize;
                    let mut pts = Vec::with_capacity(n);
                    for i in 0..n {
                        let a = std::f32::consts::TAU * (i as f32) / (n as f32);
                        pts.push((cx + rx * a.cos(), cy + ry * a.sin()));
                    }
                    let mut cmds = polyline(&pts);
                    cmds.push(PathCommand::Close);
                    layers.push((cmds, style.clone()));
                }
            }
            "rect" => {
                let x = parse_f(&attrs, "x", 0.0);
                let y = parse_f(&attrs, "y", 0.0);
                let w = parse_f(&attrs, "width", 0.0);
                let h = parse_f(&attrs, "height", 0.0);
                if w > 0.0 && h > 0.0 {
                    layers.push((filled_rect(x, y, w, h), style.clone()));
                }
            }
            "line" => {
                let x1 = parse_f(&attrs, "x1", 0.0);
                let y1 = parse_f(&attrs, "y1", 0.0);
                let x2 = parse_f(&attrs, "x2", 0.0);
                let y2 = parse_f(&attrs, "y2", 0.0);
                layers.push((polyline(&[(x1, y1), (x2, y2)]), style.clone()));
            }
            "polyline" | "polygon" => {
                if let Some(pts) = attrs.get("points") {
                    let nums: Vec<f32> = pts
                        .replace(',', " ")
                        .split_whitespace()
                        .filter_map(|n| n.parse().ok())
                        .collect();
                    let points: Vec<Point2> = nums
                        .chunks(2)
                        .filter_map(|c| (c.len() == 2).then_some((c[0], c[1])))
                        .collect();
                    if points.len() >= 2 {
                        let mut cmds = polyline(&points);
                        if tag == "polygon" {
                            cmds.push(PathCommand::Close);
                        }
                        layers.push((cmds, style.clone()));
                    }
                }
            }
            _ => {}
        }
    }
    layers
}

fn rasterize_svg_rgba(
    name: &str,
    size_px: f32,
    color: [u8; 4],
    dpr: f32,
) -> Option<(u32, u32, Vec<u8>)> {
    let inner = lucide_svg_inner(name)?;
    let size = size_px.clamp(8.0, 256.0);
    let dpr = dpr.clamp(1.0, 3.0);
    let side = (size * dpr).ceil().clamp(16.0, 512.0) as u32;

    #[cfg(feature = "svg")]
    {
        return crate::media::svg_raster::rasterize_lucide_icon_rgba(inner, color, side);
    }

    #[cfg(not(feature = "svg"))]
    {
        let scale = side as f32 / LUCIDE_VB;
        let stroke_w = (2.0 * scale).max(1.0);
        let layers_vb = lucide_inner_to_layers(inner, color_f32(color), 2.0);
        if layers_vb.is_empty() {
            return None;
        }
        let layers: Vec<_> = layers_vb
            .into_iter()
            .map(|(cmds, mut st)| {
                st.stroke_width = stroke_w;
                (scale_cmds(&cmds, scale), st)
            })
            .collect();
        let rgba = rasterize_layers_rgba(&layers, &[], 0.0, side, side, 2);
        Some((side, side, rgba))
    }
}

fn rasterize_glyph_rgba(
    ch: char,
    size_px: f32,
    color: [u8; 4],
    dpr: f32,
) -> Option<(u32, u32, Vec<u8>)> {
    let font = font_ref()?;
    let glyph_id = font.charmap().map(ch);
    if glyph_id == 0 {
        return None;
    }
    let size = size_px.clamp(8.0, 256.0);
    let dpr = dpr.clamp(1.0, 3.0);
    // Match SVG path: raster into a square at `size * dpr`, then the layout quad scales it down.
    let px = (size * dpr).clamp(16.0, 512.0);
    let mut context = ScaleContext::new();
    let mut scaler = context.builder(font).size(px).hint(false).build();
    let image = Render::new(&[Source::Outline])
        .format(Format::Alpha)
        .render(&mut scaler, glyph_id)?;
    let w = image.placement.width.max(1);
    let h = image.placement.height.max(1);
    if w > 512 || h > 512 {
        return None;
    }
    let side = px.ceil().max(1.0) as u32;
    let mut rgba = vec![0u8; (side * side * 4) as usize];
    // Center ink in the square. Font bearings (`placement.left` / baseline `top`) are for
    // text runs; rotating that around the quad center orbits the glyph (accordion chevron
    // jumps sideways) and the 1× Linear sample looks thicker/softer than glyphon Coverage.
    let ox0 = (side as i32 - w as i32) / 2;
    let oy0 = (side as i32 - h as i32) / 2;
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

fn cache_put(key: String, w: u32, h: u32, rgba: Vec<u8>) -> Option<(String, f32)> {
    let mut guard = GLYPH_CPU.lock().ok()?;
    let map = guard.get_or_insert_with(HashMap::new);
    if map.len() >= MAX_GLYPH_CACHE {
        map.clear();
    }
    map.insert(
        key.clone(),
        GlyphCpuEntry {
            width: w,
            height: h,
            rgba: rgba.clone(),
        },
    );
    queue_image_rgba8(&key, w, h, rgba);
    Some((key, w as f32))
}

/// Prefer SVG via resvg when `svg` feature on; else path flatten or font glyph.
pub fn ensure_lucide_icon_texture(
    name: Option<&str>,
    glyph: &str,
    size_px: f32,
    color: [u8; 4],
    dpr: f32,
) -> Option<(String, f32)> {
    let size_key = (size_px * 100.0).round() as i32;
    let dpr_key = (dpr * 100.0).round() as i32;
    if let Some(n) = name.filter(|s| !s.is_empty()) {
        if lucide_svg_inner(n).is_some() {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            n.hash(&mut hasher);
            size_key.hash(&mut hasher);
            dpr_key.hash(&mut hasher);
            color.hash(&mut hasher);
            #[cfg(feature = "svg")]
            {
                "resvg_v1".hash(&mut hasher);
            }
            let key = format!("rui_lucide_svg:{:016x}", hasher.finish());
            let mut guard = GLYPH_CPU.lock().ok()?;
            let map = guard.get_or_insert_with(HashMap::new);
            if let Some(entry) = map.get(&key) {
                queue_image_rgba8(&key, entry.width, entry.height, entry.rgba.clone());
                return Some((key, entry.width as f32));
            }
            drop(guard);
            let (w, h, rgba) = rasterize_svg_rgba(n, size_px, color, dpr)?;
            return cache_put(key, w, h, rgba);
        }
    }

    let ch = glyph.chars().next()?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (ch as u32).hash(&mut hasher);
    size_key.hash(&mut hasher);
    dpr_key.hash(&mut hasher);
    color.hash(&mut hasher);
    let key = format!("rui_lucide_font_v3:{:016x}", hasher.finish());
    let mut guard = GLYPH_CPU.lock().ok()?;
    let map = guard.get_or_insert_with(HashMap::new);
    if let Some(entry) = map.get(&key) {
        queue_image_rgba8(&key, entry.width, entry.height, entry.rgba.clone());
        return Some((key, entry.width as f32));
    }
    drop(guard);
    let (w, h, rgba) = rasterize_glyph_rgba(ch, size_px, color, dpr)?;
    cache_put(key, w, h, rgba)
}

/// Back-compat: glyph-only entry (font path).
pub fn ensure_lucide_glyph_texture(
    glyph: &str,
    size_px: f32,
    color: [u8; 4],
) -> Option<(String, f32)> {
    ensure_lucide_icon_texture(None, glyph, size_px, color, 1.0)
}

#[cfg(all(test, feature = "bundled-lucide"))]
mod tests {
    use super::*;

    fn ink_bbox_center(side: u32, rgba: &[u8]) -> (f32, f32) {
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
        assert!(max_x >= min_x, "expected ink");
        ((min_x + max_x) as f32 * 0.5, (min_y + max_y) as f32 * 0.5)
    }

    #[test]
    fn loader_circle_svg_raster_nonempty_square() {
        let (side, h, rgba) =
            rasterize_svg_rgba("loader-circle", 32.0, [255, 255, 255, 255], 2.0).expect("svg");
        assert_eq!(side, h);
        assert!(side >= 32);
        let ink = rgba.chunks(4).filter(|p| p[3] > 0).count();
        assert!(ink > 40, "expected stroke ink, got {ink}");
    }

    #[test]
    fn circle_icon_ink_bbox_near_center() {
        let (side, _, rgba) =
            rasterize_svg_rgba("circle", 32.0, [255, 255, 255, 255], 2.0).expect("svg");
        let (cx, cy) = ink_bbox_center(side, &rgba);
        let mid = (side as f32 - 1.0) * 0.5;
        assert!(
            (cx - mid).abs() < 2.5 && (cy - mid).abs() < 2.5,
            "ink bbox center ({cx:.2},{cy:.2}) far from {mid}"
        );
    }

    #[test]
    fn font_chevron_ink_bbox_near_center() {
        let glyph = crate::utils::lucide::lucide_glyph("chevron-down").expect("glyph");
        let ch = glyph.chars().next().expect("char");
        let (side, h, rgba) =
            rasterize_glyph_rgba(ch, 32.0, [255, 255, 255, 255], 1.0).expect("font");
        assert_eq!(side, h);
        let (cx, cy) = ink_bbox_center(side, &rgba);
        let mid = (side as f32 - 1.0) * 0.5;
        assert!(
            (cx - mid).abs() < 2.5 && (cy - mid).abs() < 2.5,
            "font chevron ink ({cx:.2},{cy:.2}) far from square center {mid} — rotation would offset"
        );
    }

    #[test]
    fn font_glyph_honors_dpr_supersample() {
        let glyph = crate::utils::lucide::lucide_glyph("chevron-down").expect("glyph");
        let ch = glyph.chars().next().expect("char");
        let (side, _, _) = rasterize_glyph_rgba(ch, 16.0, [255, 255, 255, 255], 2.0).expect("font");
        assert_eq!(
            side, 32,
            "dpr 2 at 16px must raster 32px square, got {side}"
        );
    }

    #[cfg(not(feature = "svg"))]
    #[test]
    fn lucide_inner_parses_path_d() {
        let layers = lucide_inner_to_layers(
            r#"<path d="M21 12a9 9 0 1 1-6.219-8.56" />"#,
            [0.0, 0.0, 0.0, 1.0],
            2.0,
        );
        assert!(!layers.is_empty());
        assert!(layers[0].0.len() > 4);
    }
}
