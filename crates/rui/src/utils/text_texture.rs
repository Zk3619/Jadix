//! Axis-aligned text → cached RGBA texture for CSS rotate / skew (smooth bilinear).
//!
//! Glyphon glyphs are hinted at integer pixels; rotating those quads looks jagged.
//! Bake coverage into a **linear** (`Rgba8Unorm`) texture — sRGB upload + bilinear
//! rotation turns thin strokes into fog (see gallery 3b).

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Mutex;

use crate::app::pending_images::queue_image_rgba8;

struct Entry {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    last_used: u64,
}

struct CpuCache {
    map: HashMap<String, Entry>,
    /// Keys `ensure_*` requested since the last [`flush_text_xform_textures`].
    needed: HashSet<String>,
    clock: u64,
}

impl CpuCache {
    fn bytes(&self) -> usize {
        self.map.values().map(|e| e.rgba.len()).sum()
    }

    fn mark_needed(&mut self, key: &str) {
        self.clock = self.clock.saturating_add(1);
        self.needed.insert(key.to_string());
        if let Some(e) = self.map.get_mut(key) {
            e.last_used = self.clock;
        }
    }

    /// Drop cold entries. Never drop keys still requested this paint cycle.
    fn evict_if_needed(&mut self) {
        while self.map.len() > MAX_CACHE || self.bytes() > MAX_CACHE_BYTES {
            let victim = self
                .map
                .iter()
                .filter(|(k, _)| !self.needed.contains(*k))
                .min_by_key(|(_, e)| e.last_used)
                .map(|(k, _)| k.clone());
            let Some(k) = victim else {
                break;
            };
            self.map.remove(&k);
        }
    }
}

static CACHE: Mutex<Option<CpuCache>> = Mutex::new(None);
/// Entry cap — sticky Gallery pages can mint many unique bakes; LRU drops the rest.
const MAX_CACHE: usize = 128;
/// Byte cap on CPU RGBA (3× bake of a 400×80 leaf ≈ 1.1 MiB). GPU still LRU via TexturePool.
const MAX_CACHE_BYTES: usize = 16 * 1024 * 1024;

/// Key prefix — coverage bake; `upload_rgba8_linear_with_mips` (linear Unorm).
/// v3: Twemoji color composite for emoji leaves (never font outline / MSDF).
pub const TEXT_XFORM_TEX_PREFIX: &str = "rui_text_xform_v3:";

/// Key prefix — 3-channel MSDF (edge-pair coloring). Linear mip-less upload.
/// Emoji-bearing strings must **not** use this path (color would collapse to wireframe).
pub const TEXT_MSDF_TEX_PREFIX: &str = "rui_text_msdf_v2:";

/// SDF spread in supersampled pixels (normalize: 128 = edge).
const SDF_SPREAD_PX: f32 = 8.0;

fn with_cache<R>(f: impl FnOnce(&mut CpuCache) -> R) -> Option<R> {
    let mut guard = CACHE.lock().ok()?;
    let cache = guard.get_or_insert_with(|| CpuCache {
        map: HashMap::new(),
        needed: HashSet::new(),
        clock: 0,
    });
    Some(f(cache))
}

/// Touch + pin for this paint cycle. Does **not** clone RGBA (layout collect is hot).
fn cache_touch(key: &str) -> bool {
    with_cache(|c| {
        let hit = c.map.contains_key(key);
        if hit {
            c.mark_needed(key);
            c.evict_if_needed();
        }
        hit
    })
    .unwrap_or(false)
}

fn cache_put(key: String, width: u32, height: u32, rgba: Vec<u8>) {
    let _ = with_cache(|c| {
        c.mark_needed(&key);
        c.map.insert(
            key,
            Entry {
                width,
                height,
                rgba,
                last_used: c.clock,
            },
        );
        c.evict_if_needed();
    });
}

/// CPU bake/MSDF cache occupancy `(entries, bytes)`.
pub fn text_xform_cpu_cache_stats() -> (usize, usize) {
    with_cache(|c| (c.map.len(), c.bytes())).unwrap_or((0, 0))
}

/// `rui_text_xform_*` / `rui_text_msdf_*` GPU keys (version-agnostic).
pub fn is_text_xform_texture_key(key: &str) -> bool {
    key.starts_with("rui_text_xform_") || key.starts_with("rui_text_msdf_")
}

#[cfg(test)]
pub(crate) fn cache_peek(key: &str) -> Option<(u32, u32, Vec<u8>)> {
    with_cache(|c| c.map.get(key).map(|e| (e.width, e.height, e.rgba.clone()))).flatten()
}

/// Axis-aligned opaque ink bbox in texture pixels (`[x, y, w, h]`).
#[cfg(test)]
pub(crate) fn opaque_ink_rect(rgba: &[u8], w: u32, h: u32) -> Option<[u32; 4]> {
    let mut min_x = w;
    let mut min_y = h;
    let mut max_x = 0u32;
    let mut max_y = 0u32;
    let mut any = false;
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            if rgba.get(i + 3).copied().unwrap_or(0) < 24 {
                continue;
            }
            any = true;
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    }
    any.then_some([
        min_x,
        min_y,
        max_x.saturating_sub(min_x) + 1,
        max_y.saturating_sub(min_y) + 1,
    ])
}

#[cfg(test)]
fn cache_clear() {
    let _ = with_cache(|c| {
        c.map.clear();
        c.needed.clear();
        c.clock = 0;
    });
}

#[cfg(test)]
static CACHE_TEST_SERIAL: Mutex<()> = Mutex::new(());

#[cfg(test)]
fn cache_test_serial() -> std::sync::MutexGuard<'static, ()> {
    CACHE_TEST_SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Logical padding so swash ink (ascent / bearing) is not clipped at the bake edge.
pub fn text_xform_pad_px(font_size_px: f32) -> f32 {
    (font_size_px * 0.22).max(2.0)
}

/// Tailwind default line box when the leaf did not set `leading-*` / `line-height`.
pub(crate) fn bake_default_line_height_px(font_size_px: f32) -> f32 {
    (font_size_px * crate::style::tailwind_default_leading_scale(font_size_px)).max(1.0)
}

/// Expand layout text box to a bake/draw size that fits line-height + ink pad.
/// Returns `(bake_w, bake_h, pad)`.
pub fn text_xform_bake_box(box_w: f32, box_h: f32, font_size_px: f32) -> (f32, f32, f32) {
    text_xform_bake_box_ex(
        box_w,
        box_h,
        font_size_px,
        0.0,
        bake_default_line_height_px(font_size_px),
    )
}

/// Extra pad so CSS `text-shadow` offset/blur is not clipped when baking under transform.
pub fn text_xform_shadow_pad(offset: [f32; 2], blur: f32) -> f32 {
    (blur.max(0.0) + offset[0].abs().max(offset[1].abs()) + 2.0).max(0.0)
}

/// Like [`text_xform_bake_box`] with additional uniform pad (e.g. text-shadow extent)
/// and an explicit line box (must match Coverage `coverage_line_height`).
pub fn text_xform_bake_box_ex(
    box_w: f32,
    box_h: f32,
    font_size_px: f32,
    extra_pad: f32,
    line_height_px: f32,
) -> (f32, f32, f32) {
    let pad = text_xform_pad_px(font_size_px) + extra_pad.max(0.0);
    let h = box_h.max(line_height_px.max(1.0)) + pad * 2.0;
    let w = box_w.max(1.0) + pad * 2.0;
    (w, h, pad)
}

/// Rasterize `text` into an RGBA texture at `dpr` supersample. Returns `(texture_key, logical_w)`.
///
/// When `as_sdf`, converts alpha coverage to a **3-channel MSDF** (edge-pair coloring;
/// `median(RGB)` in the FS). Convention: **128 = edge, >128 inside, <128 outside**.
///
/// Callers should pass sizes from [`text_xform_bake_box`] so glyphs are not clipped.
pub fn ensure_text_xform_texture(
    text: &str,
    font_size_px: f32,
    font_weight: u16,
    color: [u8; 4],
    layout_w: f32,
    layout_h: f32,
    bake_w: f32,
    bake_h: f32,
    pad: f32,
    align: glyphon::cosmic_text::Align,
    dpr: f32,
    as_sdf: bool,
    glyph_dy: f32,
) -> Option<(String, f32)> {
    ensure_text_xform_texture_with_shadow(
        text,
        font_size_px,
        font_weight,
        color,
        layout_w,
        layout_h,
        bake_w,
        bake_h,
        pad,
        align,
        dpr,
        as_sdf,
        None,
        glyph_dy,
        bake_default_line_height_px(font_size_px),
    )
}

/// Coverage `font-synthesis: weight` halo, or `None` for lucide / true Medium / synth off.
fn resolve_synthetic_bold_bake_shadow(
    font_weight: u16,
    color: [u8; 4],
    scale: f32,
    font_family: Option<&str>,
) -> Option<([u8; 4], [f32; 2], f32)> {
    if font_family == Some("lucide") {
        return None;
    }
    crate::text::measure::with_layout_font_system(|fs| {
        let family = match font_family {
            Some(f) => crate::utils::font::resolve_family(fs.db(), f),
            None => crate::renderer::text_hit::best_font_family(fs),
        };
        let resolved = crate::utils::font::resolve_face_weight(fs.db(), family, font_weight);
        resolved
            .synthetic_bold
            .then(|| crate::utils::font::synthetic_bold_shadow(color, scale))
    })
}

/// User `text-shadow` wins; otherwise the Coverage fake-bold halo (scale = paint/text scale).
pub(crate) fn text_xform_effective_shadow(
    user_shadow: Option<([u8; 4], [f32; 2], f32)>,
    font_weight: u16,
    color: [u8; 4],
    text_scale: f32,
    is_icon: bool,
    font_family: Option<&str>,
) -> Option<([u8; 4], [f32; 2], f32)> {
    if user_shadow.is_some() || is_icon {
        return user_shadow;
    }
    resolve_synthetic_bold_bake_shadow(font_weight, color, text_scale, font_family)
}

/// Like [`ensure_text_xform_texture`] with optional CSS `text-shadow` baked under the glyphs
/// (so Bake/Msdf transform quads keep shadows without the Coverage glyphon path).
pub fn ensure_text_xform_texture_with_shadow(
    text: &str,
    font_size_px: f32,
    font_weight: u16,
    color: [u8; 4],
    layout_w: f32,
    layout_h: f32,
    bake_w: f32,
    bake_h: f32,
    pad: f32,
    align: glyphon::cosmic_text::Align,
    dpr: f32,
    as_sdf: bool,
    shadow: Option<([u8; 4], [f32; 2], f32)>,
    glyph_dy: f32,
    line_height_px: f32,
) -> Option<(String, f32)> {
    let shadow = text_xform_effective_shadow(shadow, font_weight, color, 1.0, false, None);
    if text.is_empty()
        || layout_w < 1.0
        || layout_h < 1.0
        || bake_w < 1.0
        || bake_h < 1.0
        || font_size_px < 0.5
    {
        return None;
    }
    let dpr = dpr.clamp(1.0, 4.0);
    let size_key = (font_size_px * 100.0).round() as i32;
    let lw_key = (layout_w * 10.0).round() as i32;
    let lh_key = (layout_h * 10.0).round() as i32;
    let bw_key = (bake_w * 10.0).round() as i32;
    let bh_key = (bake_h * 10.0).round() as i32;
    let pad_key = (pad * 10.0).round() as i32;
    let dpr_key = (dpr * 100.0).round() as i32;
    let align_key = match align {
        glyphon::cosmic_text::Align::Left => 0u8,
        glyphon::cosmic_text::Align::Center => 1,
        glyphon::cosmic_text::Align::Right => 2,
        glyphon::cosmic_text::Align::Justified => 3,
        glyphon::cosmic_text::Align::End => 4,
    };
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    size_key.hash(&mut hasher);
    font_weight.hash(&mut hasher);
    color.hash(&mut hasher);
    lw_key.hash(&mut hasher);
    lh_key.hash(&mut hasher);
    bw_key.hash(&mut hasher);
    bh_key.hash(&mut hasher);
    pad_key.hash(&mut hasher);
    dpr_key.hash(&mut hasher);
    align_key.hash(&mut hasher);
    glyph_dy.to_bits().hash(&mut hasher);
    let line_h = line_height_px.max(1.0);
    ((line_h * 100.0).round() as i32).hash(&mut hasher);
    let wants_emoji_color = bake_contains_emoji(text);
    // Color emoji must stay RGBA Bake — MSDF collapses color glyphs to a tinted wireframe.
    let as_sdf = as_sdf && !wants_emoji_color;
    as_sdf.hash(&mut hasher);
    wants_emoji_color.hash(&mut hasher);
    if let Some((sc, so, sb)) = shadow {
        sc.hash(&mut hasher);
        so[0].to_bits().hash(&mut hasher);
        so[1].to_bits().hash(&mut hasher);
        sb.to_bits().hash(&mut hasher);
    } else {
        0u8.hash(&mut hasher);
    }
    // Bump when bake / MSDF / emoji / shadow / synth-bold / oversample stem / line-height change.
    14u8.hash(&mut hasher);
    let prefix = if as_sdf {
        TEXT_MSDF_TEX_PREFIX
    } else {
        TEXT_XFORM_TEX_PREFIX
    };
    let key = format!("{prefix}{:016x}", hasher.finish());

    if cache_touch(&key) {
        return Some((key, bake_w));
    }

    let (w, h, mut rgba) = rasterize_text_rgba(
        text,
        font_size_px,
        font_weight,
        color,
        layout_w,
        layout_h,
        bake_w,
        bake_h,
        pad,
        align,
        dpr,
        shadow,
        glyph_dy,
        line_h,
    )?;
    restore_bake_coverage_after_oversample(&mut rgba, w, h, dpr);
    // Reject empty bitmaps (font/shape failure); caller may fall back to Coverage glyphon.
    if !rgba.chunks_exact(4).any(|p| p[3] > 0) {
        return None;
    }
    if as_sdf {
        rgba = alpha_coverage_to_sdf_rgba(&rgba, w, h);
    }
    cache_put(key.clone(), w, h, rgba.clone());
    queue_image_rgba8(&key, w, h, rgba);
    Some((key, bake_w))
}

/// Layout-sized shaping buffer height (≥ one line of the Coverage line box).
fn bake_shape_h(layout_h: f32, line_height_px: f32) -> f32 {
    layout_h.max(line_height_px.max(1.0))
}

/// Rich-inline spans (chip/mention/color) → cached bake texture for CSS transform.
pub fn ensure_text_xform_texture_spans(
    spans: &[crate::style::TextSpan],
    font_size_px: f32,
    font_weight: u16,
    default_color: [u8; 4],
    layout_w: f32,
    layout_h: f32,
    bake_w: f32,
    bake_h: f32,
    pad: f32,
    align: glyphon::cosmic_text::Align,
    dpr: f32,
    as_sdf: bool,
    glyph_dy: f32,
    line_height_px: f32,
) -> Option<(String, f32)> {
    if spans.is_empty()
        || layout_w < 1.0
        || layout_h < 1.0
        || bake_w < 1.0
        || bake_h < 1.0
        || font_size_px < 0.5
    {
        return None;
    }
    let dpr = dpr.clamp(1.0, 4.0);
    let size_key = (font_size_px * 100.0).round() as i32;
    let lw_key = (layout_w * 10.0).round() as i32;
    let lh_key = (layout_h * 10.0).round() as i32;
    let bw_key = (bake_w * 10.0).round() as i32;
    let bh_key = (bake_h * 10.0).round() as i32;
    let pad_key = (pad * 10.0).round() as i32;
    let dpr_key = (dpr * 100.0).round() as i32;
    let align_key = match align {
        glyphon::cosmic_text::Align::Left => 0u8,
        glyphon::cosmic_text::Align::Center => 1,
        glyphon::cosmic_text::Align::Right => 2,
        glyphon::cosmic_text::Align::Justified => 3,
        glyphon::cosmic_text::Align::End => 4,
    };
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for span in spans {
        span.text.hash(&mut hasher);
        span.color.hash(&mut hasher);
        span.weight.hash(&mut hasher);
        span.size.map(|v| v.to_bits()).hash(&mut hasher);
        (span.kind as u8).hash(&mut hasher);
        span.background.hash(&mut hasher);
        span.padding_x.to_bits().hash(&mut hasher);
        span.italic.hash(&mut hasher);
        span.monospace.hash(&mut hasher);
        span.underline.hash(&mut hasher);
    }
    size_key.hash(&mut hasher);
    font_weight.hash(&mut hasher);
    default_color.hash(&mut hasher);
    lw_key.hash(&mut hasher);
    lh_key.hash(&mut hasher);
    bw_key.hash(&mut hasher);
    bh_key.hash(&mut hasher);
    pad_key.hash(&mut hasher);
    dpr_key.hash(&mut hasher);
    align_key.hash(&mut hasher);
    glyph_dy.to_bits().hash(&mut hasher);
    let line_h = line_height_px.max(1.0);
    ((line_h * 100.0).round() as i32).hash(&mut hasher);
    let wants_emoji_color = spans.iter().any(|s| bake_contains_emoji(&s.text));
    let as_sdf = as_sdf && !wants_emoji_color;
    as_sdf.hash(&mut hasher);
    wants_emoji_color.hash(&mut hasher);
    11u8.hash(&mut hasher); // rich-span + oversample stem + Coverage line-height
    let prefix = if as_sdf {
        TEXT_MSDF_TEX_PREFIX
    } else {
        TEXT_XFORM_TEX_PREFIX
    };
    let key = format!("{prefix}{:016x}", hasher.finish());

    if cache_touch(&key) {
        return Some((key, bake_w));
    }

    let (w, h, mut rgba) = rasterize_rich_text_rgba(
        spans,
        font_size_px,
        font_weight,
        default_color,
        layout_w,
        layout_h,
        bake_w,
        bake_h,
        pad,
        align,
        dpr,
        glyph_dy,
        line_h,
    )?;
    restore_bake_coverage_after_oversample(&mut rgba, w, h, dpr);
    if !rgba.chunks_exact(4).any(|p| p[3] > 0) {
        return None;
    }
    if as_sdf {
        rgba = alpha_coverage_to_sdf_rgba(&rgba, w, h);
    }
    cache_put(key.clone(), w, h, rgba.clone());
    queue_image_rgba8(&key, w, h, rgba);
    Some((key, bake_w))
}

/// 3-channel MSDF from a coverage bitmap (Chlumsky-style edge *pairs*).
///
/// Each silhouette edge is assigned two of {R,G,B} from its gradient angle (120° bins).
/// `median(R,G,B)` then reconstructs the true edge: a straight edge has two correct
/// channels and one far channel; a sharp corner drops the spurious long-way distance.
/// Falls back to identical RGB (classic SDF) when no edges are found.
fn alpha_coverage_to_sdf_rgba(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    let w = width as usize;
    let h = height as usize;
    let n = w * h;
    let mut inside = vec![false; n];
    for i in 0..n {
        inside[i] = rgba.get(i * 4 + 3).copied().unwrap_or(0) >= 128;
    }

    let mut seed_r = vec![false; n];
    let mut seed_g = vec![false; n];
    let mut seed_b = vec![false; n];
    let mut any_edge = false;
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if !is_coverage_edge(&inside, w, h, x, y) {
                continue;
            }
            any_edge = true;
            let (gx, gy) = sobel_mask(&inside, w, h, x, y);
            match angle_pair_bin(gx, gy) {
                Some(0) => {
                    seed_r[i] = true;
                    seed_g[i] = true;
                }
                Some(1) => {
                    seed_g[i] = true;
                    seed_b[i] = true;
                }
                Some(2) => {
                    seed_b[i] = true;
                    seed_r[i] = true;
                }
                _ => {
                    seed_r[i] = true;
                    seed_g[i] = true;
                    seed_b[i] = true;
                }
            }
        }
    }

    if !any_edge {
        // Solid / empty — pack a conventional single-channel field.
        return pack_signed_sdf_rgb(
            &chamfer_signed(&inside, w, h),
            &chamfer_signed(&inside, w, h),
            &chamfer_signed(&inside, w, h),
            n,
        );
    }

    let dist_r = chamfer_dt(&seed_r, w, h);
    let dist_g = chamfer_dt(&seed_g, w, h);
    let dist_b = chamfer_dt(&seed_b, w, h);
    let mut sd_r = vec![0.0f32; n];
    let mut sd_g = vec![0.0f32; n];
    let mut sd_b = vec![0.0f32; n];
    for i in 0..n {
        let sign = if inside[i] { 1.0 } else { -1.0 };
        sd_r[i] = sign * dist_r[i];
        sd_g[i] = sign * dist_g[i];
        sd_b[i] = sign * dist_b[i];
    }
    pack_signed_sdf_rgb(&sd_r, &sd_g, &sd_b, n)
}

fn is_coverage_edge(inside: &[bool], w: usize, h: usize, x: usize, y: usize) -> bool {
    let i = y * w + x;
    let me = inside[i];
    if x > 0 && inside[i - 1] != me {
        return true;
    }
    if x + 1 < w && inside[i + 1] != me {
        return true;
    }
    if y > 0 && inside[i - w] != me {
        return true;
    }
    if y + 1 < h && inside[i + w] != me {
        return true;
    }
    false
}

fn mask_at(inside: &[bool], w: usize, h: usize, x: i32, y: i32) -> f32 {
    if x < 0 || y < 0 || x as usize >= w || y as usize >= h {
        return 0.0;
    }
    if inside[y as usize * w + x as usize] {
        1.0
    } else {
        0.0
    }
}

fn sobel_mask(inside: &[bool], w: usize, h: usize, x: usize, y: usize) -> (f32, f32) {
    let x = x as i32;
    let y = y as i32;
    let gx = mask_at(inside, w, h, x + 1, y - 1)
        + 2.0 * mask_at(inside, w, h, x + 1, y)
        + mask_at(inside, w, h, x + 1, y + 1)
        - mask_at(inside, w, h, x - 1, y - 1)
        - 2.0 * mask_at(inside, w, h, x - 1, y)
        - mask_at(inside, w, h, x - 1, y + 1);
    let gy = mask_at(inside, w, h, x - 1, y + 1)
        + 2.0 * mask_at(inside, w, h, x, y + 1)
        + mask_at(inside, w, h, x + 1, y + 1)
        - mask_at(inside, w, h, x - 1, y - 1)
        - 2.0 * mask_at(inside, w, h, x, y - 1)
        - mask_at(inside, w, h, x + 1, y - 1);
    (gx, gy)
}

/// 120° bins → which **pair** of MSDF channels own this edge. `None` = degenerate gradient.
fn angle_pair_bin(gx: f32, gy: f32) -> Option<u8> {
    if gx * gx + gy * gy < 1e-8 {
        return None;
    }
    let t = (gy.atan2(gx) + std::f32::consts::PI) / (2.0 * std::f32::consts::PI);
    Some(((t * 3.0).floor() as i32).rem_euclid(3) as u8)
}

fn chamfer_signed(inside: &[bool], w: usize, h: usize) -> Vec<f32> {
    let n = w * h;
    let mut outside_seed = vec![false; n];
    let mut inside_seed = vec![false; n];
    for i in 0..n {
        outside_seed[i] = !inside[i];
        inside_seed[i] = inside[i];
    }
    let dist_to_out = chamfer_dt(&outside_seed, w, h);
    let dist_to_in = chamfer_dt(&inside_seed, w, h);
    let mut sd = vec![0.0f32; n];
    for i in 0..n {
        sd[i] = if inside[i] {
            dist_to_out[i]
        } else {
            -dist_to_in[i]
        };
    }
    sd
}

fn pack_signed_sdf_rgb(r: &[f32], g: &[f32], b: &[f32], n: usize) -> Vec<u8> {
    let mut out = vec![0u8; n * 4];
    let inv = 1.0 / (2.0 * SDF_SPREAD_PX);
    for i in 0..n {
        let o = i * 4;
        out[o] = pack_sdf_u8(r[i], inv);
        out[o + 1] = pack_sdf_u8(g[i], inv);
        out[o + 2] = pack_sdf_u8(b[i], inv);
        out[o + 3] = 255;
    }
    out
}

fn pack_sdf_u8(sd: f32, inv: f32) -> u8 {
    let t = (0.5 + sd * inv).clamp(0.0, 1.0);
    (t * 255.0).round() as u8
}

/// 3-4 chamfer distance to nearest `true` seed (approx. Euclidean).
fn chamfer_dt(seeds: &[bool], w: usize, h: usize) -> Vec<f32> {
    const INF: f32 = 1.0e10;
    const A: f32 = 1.0;
    const B: f32 = std::f32::consts::SQRT_2;
    let mut d = vec![INF; w * h];
    for i in 0..w * h {
        if seeds[i] {
            d[i] = 0.0;
        }
    }
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if x > 0 {
                d[i] = d[i].min(d[i - 1] + A);
            }
            if y > 0 {
                d[i] = d[i].min(d[i - w] + A);
            }
            if x > 0 && y > 0 {
                d[i] = d[i].min(d[i - w - 1] + B);
            }
            if x + 1 < w && y > 0 {
                d[i] = d[i].min(d[i - w + 1] + B);
            }
        }
    }
    for y in (0..h).rev() {
        for x in (0..w).rev() {
            let i = y * w + x;
            if x + 1 < w {
                d[i] = d[i].min(d[i + 1] + A);
            }
            if y + 1 < h {
                d[i] = d[i].min(d[i + w] + A);
            }
            if x + 1 < w && y + 1 < h {
                d[i] = d[i].min(d[i + w + 1] + B);
            }
            if x > 0 && y + 1 < h {
                d[i] = d[i].min(d[i + w - 1] + B);
            }
        }
    }
    d
}

/// Re-queue CPU-cached Bake/MSDF textures that this paint cycle still needs
/// and that the GPU pool no longer holds. Cold CPU entries are LRU-dropped here.
pub fn flush_text_xform_textures(mut has_gpu: impl FnMut(&str) -> bool) -> bool {
    with_cache(|c| {
        let needed = std::mem::take(&mut c.needed);
        let mut any = false;
        for key in needed {
            if has_gpu(&key) {
                continue;
            }
            if let Some(entry) = c.map.get(&key) {
                queue_image_rgba8(&key, entry.width, entry.height, entry.rgba.clone());
                any = true;
            }
        }
        c.evict_if_needed();
        any
    })
    .unwrap_or(false)
}

/// Vertical glyph offset inside a Bake texture — mirrors Coverage `dy` in `prepare_text.rs`.
/// Uses the **layout** text box height (`layout_box_h`), not the padded bake size.
/// `line_height_px` must be the same Coverage line box passed to rasterize.
pub fn bake_glyph_layout_dy(
    layout_box_h: f32,
    line_height_px: f32,
    line_count: usize,
    nowrap: bool,
    is_input_text: bool,
) -> f32 {
    crate::renderer::prepare_text::coverage_nowrap_glyph_dy(
        is_input_text,
        nowrap,
        line_count,
        layout_box_h,
        line_height_px.max(1.0),
    )
}

/// Screen rect for a Bake/Msdf quad: expanded for ink pad, anchored on the layout text box
/// center so the textured shader's rect-center pivot matches flex/Taffy placement under rotate.
pub fn bake_draw_rect_for_layout(
    text_x: f32,
    text_y0: f32,
    layout_w: f32,
    layout_h: f32,
    bake_w: f32,
    bake_h: f32,
) -> [f32; 4] {
    let cx = text_x + layout_w * 0.5;
    let cy = text_y0 + layout_h * 0.5;
    [cx - bake_w * 0.5, cy - bake_h * 0.5, bake_w, bake_h]
}

fn bake_contains_emoji(text: &str) -> bool {
    crate::renderer::emoji_detect::text_contains_emoji(text)
}

/// Bitmap-override bake: invisible slots + Twemoji/font custom rasters.
fn bake_wants_emoji_bitmap(text: &str) -> bool {
    crate::renderer::emoji_layout::custom_glyph_layout_enabled() && bake_contains_emoji(text)
}

fn rasterize_text_rgba(
    text: &str,
    font_size_px: f32,
    font_weight: u16,
    color: [u8; 4],
    layout_w: f32,
    layout_h: f32,
    bake_w: f32,
    bake_h: f32,
    pad: f32,
    align: glyphon::cosmic_text::Align,
    dpr: f32,
    shadow: Option<([u8; 4], [f32; 2], f32)>,
    glyph_dy: f32,
    line_height_px: f32,
) -> Option<(u32, u32, Vec<u8>)> {
    crate::text::measure::with_layout_font_system(|fs| {
        crate::renderer::font_loader::ensure_coverage_for_text(fs, text);
        let size = (font_size_px * dpr).max(1.0);
        let line_h = line_height_px.max(1.0);
        let lh = (line_h * dpr).max(1.0);
        // Shape in the **layout** box (Coverage parity); blit into padded bake RT.
        let shape_h = bake_shape_h(layout_h, line_h);
        let pw = (layout_w * dpr).ceil().max(1.0);
        let ph = (shape_h * dpr).ceil().max(lh).max(1.0);
        let tw = (bake_w * dpr).ceil().max(1.0).round().max(1.0) as u32;
        let th = (bake_h * dpr).ceil().max(1.0).round().max(1.0) as u32;
        // Integer-center the shape in the RT so ceil(bake*dpr) leftover is split,
        // not dumped on the bottom (that read as ~1px-down vs Coverage).
        let _ = pad;
        let pad_x_px = ((tw as f32 - pw) * 0.5).round() as i32;
        let pad_y_px = ((th as f32 - ph) * 0.5).round() as i32;
        let v_dy_px = (glyph_dy * dpr).round() as i32;
        let origin_x = pad_x_px;
        let origin_y = pad_y_px + v_dy_px;

        let family = crate::renderer::text_hit::best_font_family(fs);
        let weight = crate::utils::font::normalize_weight_for_family(fs.db(), family, font_weight);
        let mut buffer = glyphon::Buffer::new(fs, glyphon::Metrics::new(size, lh));
        buffer.set_size(Some(pw), Some(ph));
        let attrs = glyphon::Attrs::new()
            .family(family)
            .weight(glyphon::fontdb::Weight(weight))
            .color(glyphon::Color::rgba(color[0], color[1], color[2], color[3]));

        // Sticky Bake used to shape raw emoji codepoints → font Mask/CBDT outlines
        // (lucide-like wireframes). Match Coverage: invisible slots + Twemoji RGBA blit.
        let wants_twemoji = bake_wants_emoji_bitmap(text);
        let layout;
        if wants_twemoji {
            layout = crate::renderer::emoji_layout::plan_emoji_layout_text(text);
            crate::renderer::emoji_layout::set_buffer_with_emoji_slots(
                &mut buffer,
                &layout,
                &attrs,
                glyphon::Shaping::Advanced,
                size,
                lh,
            );
        } else {
            layout = text.to_string();
            buffer.set_text(text, &attrs, glyphon::Shaping::Advanced, None);
        }
        for line in buffer.lines.iter_mut() {
            line.set_align(Some(align));
        }
        buffer.shape_until_scroll(fs, false);

        let mut swash = glyphon::SwashCache::new();
        let mut rgba = vec![0u8; (tw as usize) * (th as usize) * 4];

        // Optional text-shadow under glyphs (local space; rotates with Bake/Msdf quad).
        if let Some((sc, so, sb)) = shadow {
            if sc[3] > 0 && (sb > 0.0 || so[0].abs() > 0.0 || so[1].abs() > 0.0) {
                blit_text_shadow_into_bake(
                    &mut rgba,
                    tw,
                    th,
                    &buffer,
                    fs,
                    &mut swash,
                    sc,
                    so[0] * dpr,
                    so[1] * dpr,
                    (sb * dpr).max(0.0),
                    origin_x,
                    origin_y,
                    dpr,
                );
            }
        }

        for run in buffer.layout_runs() {
            // Match glyphon text_render: y = round(line_y) + physical.y - top
            let line_y_px = run.line_y.round() as i32;
            for glyph in run.glyphs.iter() {
                if crate::renderer::emoji_layout::glyph_is_emoji_layout_slot(&run, glyph) {
                    continue;
                }
                let physical = glyph.physical((0.0, 0.0), 1.0);
                let Some(image) = swash.get_image_uncached(fs, physical.cache_key) else {
                    continue;
                };
                let gw = image.placement.width;
                let gh = image.placement.height;
                if gw == 0 || gh == 0 {
                    continue;
                }
                let gx = physical.x + image.placement.left + origin_x;
                let gy = line_y_px + physical.y - image.placement.top + origin_y;
                let glyph_color = glyph
                    .color_opt
                    .map(|c| {
                        let v = c.0;
                        [
                            ((v >> 16) & 0xff) as u8,
                            ((v >> 8) & 0xff) as u8,
                            (v & 0xff) as u8,
                            ((v >> 24) & 0xff) as u8,
                        ]
                    })
                    .unwrap_or(color);
                // Slot attrs use alpha=0; skip any residual ink so Twemoji isn't underlaid
                // by a CJK/font outline.
                if glyph_color[3] == 0 {
                    continue;
                }
                match image.content {
                    glyphon::SwashContent::Mask => {
                        blit_mask_straight(
                            &mut rgba,
                            tw,
                            th,
                            &image.data,
                            gw,
                            gh,
                            gx,
                            gy,
                            glyph_color,
                        );
                    }
                    glyphon::SwashContent::SubpixelMask => {
                        blit_subpixel_as_mask(
                            &mut rgba,
                            tw,
                            th,
                            &image.data,
                            gw,
                            gh,
                            gx,
                            gy,
                            glyph_color,
                        );
                    }
                    glyphon::SwashContent::Color => {
                        // Prefer Twemoji below; ignore font CBDT when custom glyphs are on.
                        if wants_twemoji {
                            continue;
                        }
                        blit_rgba_straight(&mut rgba, tw, th, &image.data, gw, gh, gx, gy);
                    }
                }
            }
        }

        if wants_twemoji {
            composite_twemoji_into_bake(
                &mut rgba, tw, th, &buffer, text, &layout, size, origin_x, origin_y,
            );
        }

        Some((tw, th, rgba))
    })
}

/// 3× Bake is hinted at a large size then bilinear-minified; 14px glyphon Coverage
/// (and 1× bake) looks heavier. Lift mid-coverage so the ghost matches the source.
fn restore_bake_coverage_after_oversample(rgba: &mut [u8], width: u32, height: u32, dpr: f32) {
    if dpr <= 1.2 || width < 2 || height < 2 {
        return;
    }
    // 3× box-down of font-medium CJK was ~91% of 1× ink. exp ~0.89 restores ~1.0.
    let exp = (1.0 - 0.055 * (dpr - 1.0)).clamp(0.84, 0.96);
    for px in rgba.chunks_exact_mut(4) {
        let a = px[3];
        if a == 0 || a == 255 {
            continue;
        }
        let boosted = (a as f32 / 255.0).powf(exp) * 255.0;
        px[3] = boosted.round().clamp(0.0, 255.0) as u8;
    }
}

/// Soft/hard text-shadow stamp into a Bake RT (under subsequent glyph blits).
fn blit_text_shadow_into_bake(
    rgba: &mut [u8],
    tw: u32,
    th: u32,
    buffer: &glyphon::Buffer,
    fs: &mut glyphon::FontSystem,
    swash: &mut glyphon::SwashCache,
    shadow_color: [u8; 4],
    shadow_dx: f32,
    shadow_dy: f32,
    shadow_blur: f32,
    origin_x: i32,
    origin_y: i32,
    dpr: f32,
) {
    struct GlyphEntry {
        px: i32,
        py: i32,
        data: Vec<u8>,
        width: u32,
        height: u32,
        is_color: bool,
    }
    let mut glyphs: Vec<GlyphEntry> = Vec::new();
    for run in buffer.layout_runs() {
        let line_y_px = run.line_y.round() as i32;
        for g in run.glyphs.iter() {
            if crate::renderer::emoji_layout::glyph_is_emoji_layout_slot(&run, g) {
                continue;
            }
            let physical = g.physical((0.0, 0.0), 1.0);
            let Some(img) = swash.get_image_uncached(fs, physical.cache_key) else {
                continue;
            };
            if img.placement.width == 0 || img.placement.height == 0 {
                continue;
            }
            glyphs.push(GlyphEntry {
                px: physical.x + img.placement.left + origin_x,
                py: line_y_px + physical.y - img.placement.top + origin_y,
                data: img.data,
                width: img.placement.width,
                height: img.placement.height,
                is_color: matches!(img.content, glyphon::SwashContent::Color),
            });
        }
    }
    if glyphs.is_empty() {
        return;
    }

    let alpha = shadow_color[3];
    const SHARP_BLUR_EPS: f32 = 0.75;
    // Threshold is logical px (Coverage `TEXTURED_SHADOW_MIN_BLUR`). Incoming
    // `shadow_blur` is already * dpr; 0.32 synth at 3× is 0.96 bake-px and must
    // stay a sharp duplicate, not a soft glow.
    let draw_blur = shadow_blur > SHARP_BLUR_EPS * dpr.max(1.0) && alpha > 6;
    let [sr, sg, sb] = [shadow_color[0], shadow_color[1], shadow_color[2]];

    let mut stamp = |ox: f32, oy: f32, sample_alpha: u8| {
        if sample_alpha == 0 {
            return;
        }
        for ge in &glyphs {
            let px0 = ge.px + ox.round() as i32;
            let py0 = ge.py + oy.round() as i32;
            for row in 0..ge.height as i32 {
                let by = py0 + row;
                if by < 0 || by >= th as i32 {
                    continue;
                }
                let row_off = (row as u32 * ge.width) as usize;
                let dst_row = by as usize * tw as usize;
                for col in 0..ge.width as i32 {
                    let bx = px0 + col;
                    if bx < 0 || bx >= tw as i32 {
                        continue;
                    }
                    let src_a = if ge.is_color {
                        ge.data[(row_off + col as usize) * 4 + 3]
                    } else {
                        ge.data[row_off + col as usize]
                    };
                    let a = ((src_a as u16 * sample_alpha as u16) / 255) as u8;
                    if a == 0 {
                        continue;
                    }
                    let di = (dst_row + bx as usize) * 4;
                    rgba[di] = sr;
                    rgba[di + 1] = sg;
                    rgba[di + 2] = sb;
                    rgba[di + 3] = rgba[di + 3].max(a);
                }
            }
        }
    };

    if !draw_blur {
        stamp(shadow_dx, shadow_dy, alpha);
    } else {
        const GOLDEN_ANGLE: f32 = 2.399_963_1;
        let sample_count = (8.0 + shadow_blur * 4.0).round().clamp(8.0, 32.0) as usize;
        let sigma = (shadow_blur * 0.58).max(0.75);
        let two_sigma2 = 2.0 * sigma * sigma;
        let mut weight_sum = 1.0_f32;
        for idx in 0..sample_count {
            let t = (idx as f32 + 0.5) / sample_count as f32;
            let r = shadow_blur * t.sqrt();
            weight_sum += (-(r * r) / two_sigma2).exp();
        }
        let inv = 1.0 / weight_sum.max(0.001);
        let ca = ((alpha as f32) * inv).round().clamp(0.0, 255.0) as u8;
        stamp(shadow_dx, shadow_dy, ca);
        for idx in 0..sample_count {
            let t = (idx as f32 + 0.5) / sample_count as f32;
            let r = shadow_blur * t.sqrt();
            let theta = idx as f32 * GOLDEN_ANGLE;
            let w = (-(r * r) / two_sigma2).exp();
            let wa = ((alpha as f32) * w * inv).round().clamp(0.0, 255.0) as u8;
            stamp(shadow_dx + theta.cos() * r, shadow_dy + theta.sin() * r, wa);
        }
    }
}

/// Blit Twemoji PNGs into a Bake RT at the same centers Coverage uses for custom glyphs.
fn composite_twemoji_into_bake(
    rgba: &mut [u8],
    tw: u32,
    th: u32,
    buffer: &glyphon::Buffer,
    original: &str,
    layout: &str,
    font_size_px: f32,
    origin_x: i32,
    origin_y: i32,
) {
    use crate::renderer::emoji_layout;
    use unicode_segmentation::UnicodeSegmentation;

    let (ink, _advance, _) = emoji_layout::emoji_metrics(font_size_px);
    let pixel_size = emoji_layout::emoji_ink_pixel_size(font_size_px);
    let ink_draw = (pixel_size as f32).min(ink);

    let emojis: Vec<&str> = original
        .graphemes(true)
        .filter(|g| crate::renderer::emoji_detect::grapheme_is_emoji(g))
        .collect();
    if emojis.is_empty() {
        return;
    }

    let mut emoji_i = 0usize;
    let mut pos = 0usize;
    for c in layout.chars() {
        let start = pos;
        pos += c.len_utf8();
        if c != emoji_layout::EMOJI_LAYOUT_SLOT {
            continue;
        }
        let Some(seq) = emojis.get(emoji_i).copied() else {
            break;
        };
        emoji_i += 1;
        let Some((left, top)) =
            emoji_layout::custom_emoji_left_centered_top(buffer, layout, start, ink_draw, ink_draw)
        else {
            continue;
        };
        let raster =
            crate::renderer::prepare_text::load_twemoji_raster(seq, pixel_size).or_else(|| {
                let over = pixel_size.saturating_mul(2).clamp(pixel_size, 128);
                crate::renderer::prepare_text::load_font_fallback_raster(seq, over, 400).map(|r| {
                    crate::renderer::prepare_text::fit_emoji_to_pixel_square(&r, pixel_size.max(1))
                })
            });
        let Some(raster) = raster else {
            log::warn!("emoji bake: no Twemoji/font raster for {seq:?}");
            continue;
        };
        blit_rgba_straight(
            rgba,
            tw,
            th,
            &raster.rgba,
            raster.width as u32,
            raster.height as u32,
            left.round() as i32 + origin_x,
            top.round() as i32 + origin_y,
        );
    }
}

/// Fill chip/mention pill backgrounds into `rgba` before glyph blits (metadata = span_index+1).
fn paint_span_backgrounds(
    rgba: &mut [u8],
    tw: u32,
    th: u32,
    buffer: &glyphon::Buffer,
    spans: &[crate::style::TextSpan],
    dpr: f32,
    origin_x: i32,
    origin_y: i32,
) {
    if !spans.iter().any(|s| s.background.is_some()) {
        return;
    }
    for run in buffer.layout_runs() {
        let line_top = run.line_top;
        let line_height = run.line_height;
        let mut cur_meta: Option<usize> = None;
        let mut min_x = 0.0_f32;
        let mut max_x = 0.0_f32;
        let mut flush = |meta: usize, min_x: f32, max_x: f32| {
            let Some(span) = spans.get(meta.saturating_sub(1)) else {
                return;
            };
            let Some(bg) = span.background else {
                return;
            };
            if max_x <= min_x {
                return;
            }
            let pad = span.padding_x * dpr;
            let bx = ((min_x - pad) * dpr).round() as i32 + origin_x;
            let by = (line_top * dpr).round() as i32 + origin_y;
            let bw = ((max_x - min_x) + 2.0 * pad) * dpr;
            let bh = line_height.max(1.0) * dpr;
            let rw = bw.round().max(1.0) as i32;
            let rh = bh.round().max(1.0) as i32;
            fill_rect_rgba(rgba, tw, th, bx, by, rw, rh, bg);
        };
        for g in run.glyphs.iter() {
            let meta = g.metadata;
            if meta == 0 {
                if let Some(m) = cur_meta.take() {
                    flush(m, min_x, max_x);
                }
                continue;
            }
            if cur_meta != Some(meta) {
                if let Some(m) = cur_meta.take() {
                    flush(m, min_x, max_x);
                }
                cur_meta = Some(meta);
                min_x = g.x;
                max_x = g.x + g.w;
            } else {
                min_x = min_x.min(g.x);
                max_x = max_x.max(g.x + g.w);
            }
        }
        if let Some(m) = cur_meta.take() {
            flush(m, min_x, max_x);
        }
    }
}

fn fill_rect_rgba(
    dst: &mut [u8],
    dw: u32,
    dh: u32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    color: [u8; 4],
) {
    if color[3] == 0 {
        return;
    }
    for row in 0..h {
        let py = y + row;
        if py < 0 || py >= dh as i32 {
            continue;
        }
        for col in 0..w {
            let px = x + col;
            if px < 0 || px >= dw as i32 {
                continue;
            }
            let i = ((py as u32 * dw + px as u32) * 4) as usize;
            let sa = color[3] as u32;
            let da = dst[i + 3] as u32;
            let out_a = sa + ((255 - sa) * da) / 255;
            if out_a == 0 {
                continue;
            }
            let inv = 255 - sa;
            dst[i] = (((color[0] as u32 * sa) + (dst[i] as u32 * inv * da) / 255) / out_a) as u8;
            dst[i + 1] =
                (((color[1] as u32 * sa) + (dst[i + 1] as u32 * inv * da) / 255) / out_a) as u8;
            dst[i + 2] =
                (((color[2] as u32 * sa) + (dst[i + 2] as u32 * inv * da) / 255) / out_a) as u8;
            dst[i + 3] = out_a as u8;
        }
    }
}

fn rasterize_rich_text_rgba(
    spans: &[crate::style::TextSpan],
    font_size_px: f32,
    font_weight: u16,
    default_color: [u8; 4],
    layout_w: f32,
    layout_h: f32,
    bake_w: f32,
    bake_h: f32,
    pad: f32,
    align: glyphon::cosmic_text::Align,
    dpr: f32,
    glyph_dy: f32,
    line_height_px: f32,
) -> Option<(u32, u32, Vec<u8>)> {
    use crate::style::TextSpanKind;
    crate::text::measure::with_layout_font_system(|fs| {
        for span in spans {
            crate::renderer::font_loader::ensure_coverage_for_text(fs, &span.text);
        }
        let size = (font_size_px * dpr).max(1.0);
        let line_h = line_height_px.max(1.0);
        let lh = (line_h * dpr).max(1.0);
        let shape_h = bake_shape_h(layout_h, line_h);
        let pw = (layout_w * dpr).ceil().max(1.0);
        let ph = (shape_h * dpr).ceil().max(lh).max(1.0);
        let tw = (bake_w * dpr).ceil().max(1.0).round().max(1.0) as u32;
        let th = (bake_h * dpr).ceil().max(1.0).round().max(1.0) as u32;
        let _ = pad;
        let pad_x_px = ((tw as f32 - pw) * 0.5).round() as i32;
        let pad_y_px = ((th as f32 - ph) * 0.5).round() as i32;
        let v_dy_px = (glyph_dy * dpr).round() as i32;
        let origin_x = pad_x_px;
        let origin_y = pad_y_px + v_dy_px;

        let family = crate::renderer::text_hit::best_font_family(fs);
        let weight = crate::utils::font::normalize_weight_for_family(fs.db(), family, font_weight);
        let mut buffer = glyphon::Buffer::new(fs, glyphon::Metrics::new(size, lh));
        buffer.set_size(Some(pw), Some(ph));
        let base_attrs = glyphon::Attrs::new()
            .family(family)
            .weight(glyphon::fontdb::Weight(weight))
            .color(glyphon::Color::rgba(
                default_color[0],
                default_color[1],
                default_color[2],
                default_color[3],
            ));
        let mut owned: Vec<(String, glyphon::Attrs)> = Vec::with_capacity(spans.len());
        for (si, span) in spans.iter().enumerate() {
            let mut a = base_attrs.clone();
            if let Some(c) = span.color {
                a = a.color(glyphon::Color::rgba(c[0], c[1], c[2], c[3]));
            }
            if let Some(w) = span.weight {
                a = a.weight(glyphon::fontdb::Weight(w));
            }
            if let Some(sz) = span.size {
                let span_lh = lh * (sz / font_size_px.max(1.0));
                a = a.metrics(glyphon::Metrics::new((sz * dpr).max(1.0), span_lh.max(1.0)));
            }
            if span.italic {
                a = a.style(glyphon::Style::Italic);
            }
            if span.monospace {
                a = a.family(glyphon::Family::Monospace);
            }
            if span.background.is_some()
                || matches!(span.kind, TextSpanKind::Chip | TextSpanKind::Mention)
            {
                a = a.metadata(si + 1);
            }
            owned.push((span.text.clone(), a));
        }
        let rich: Vec<(&str, glyphon::Attrs)> =
            owned.iter().map(|(t, a)| (t.as_str(), a.clone())).collect();
        buffer.set_rich_text(rich, &base_attrs, glyphon::Shaping::Advanced, None);
        for line in buffer.lines.iter_mut() {
            line.set_align(Some(align));
        }
        buffer.shape_until_scroll(fs, false);

        let mut rgba = vec![0u8; (tw as usize) * (th as usize) * 4];
        paint_span_backgrounds(&mut rgba, tw, th, &buffer, spans, dpr, origin_x, origin_y);

        let mut swash = glyphon::SwashCache::new();
        for run in buffer.layout_runs() {
            let line_y_px = run.line_y.round() as i32;
            for glyph in run.glyphs.iter() {
                let physical = glyph.physical((0.0, 0.0), 1.0);
                let Some(image) = swash.get_image_uncached(fs, physical.cache_key) else {
                    continue;
                };
                let gw = image.placement.width;
                let gh = image.placement.height;
                if gw == 0 || gh == 0 {
                    continue;
                }
                let gx = physical.x + image.placement.left + origin_x;
                let gy = line_y_px + physical.y - image.placement.top + origin_y;
                let glyph_color = glyph
                    .color_opt
                    .map(|c| {
                        let v = c.0;
                        [
                            ((v >> 16) & 0xff) as u8,
                            ((v >> 8) & 0xff) as u8,
                            (v & 0xff) as u8,
                            ((v >> 24) & 0xff) as u8,
                        ]
                    })
                    .unwrap_or(default_color);
                match image.content {
                    glyphon::SwashContent::Mask => {
                        blit_mask_straight(
                            &mut rgba,
                            tw,
                            th,
                            &image.data,
                            gw,
                            gh,
                            gx,
                            gy,
                            glyph_color,
                        );
                    }
                    glyphon::SwashContent::SubpixelMask => {
                        blit_subpixel_as_mask(
                            &mut rgba,
                            tw,
                            th,
                            &image.data,
                            gw,
                            gh,
                            gx,
                            gy,
                            glyph_color,
                        );
                    }
                    glyphon::SwashContent::Color => {
                        blit_rgba_straight(&mut rgba, tw, th, &image.data, gw, gh, gx, gy);
                    }
                }
            }
        }
        Some((tw, th, rgba))
    })
}

/// Straight alpha for `ALPHA_BLENDING`: RGB = color, A = coverage × color.a.
fn blit_mask_straight(
    dst: &mut [u8],
    dw: u32,
    dh: u32,
    mask: &[u8],
    mw: u32,
    mh: u32,
    dx: i32,
    dy: i32,
    color: [u8; 4],
) {
    for row in 0..mh as i32 {
        let py = dy + row;
        if py < 0 || py >= dh as i32 {
            continue;
        }
        for col in 0..mw as i32 {
            let px = dx + col;
            if px < 0 || px >= dw as i32 {
                continue;
            }
            let m = mask[(row as u32 * mw + col as u32) as usize] as u32;
            if m == 0 {
                continue;
            }
            let sa = ((color[3] as u32 * m) / 255) as u8;
            if sa == 0 {
                continue;
            }
            let i = ((py as u32 * dw + px as u32) * 4) as usize;
            // Non-premultiplied source-over (same as prepare_text fallback blit).
            let da = dst[i + 3] as u32;
            let out_a = sa as u32 + ((255 - sa as u32) * da) / 255;
            if out_a == 0 {
                continue;
            }
            let inv = 255 - sa as u32;
            dst[i] =
                (((color[0] as u32 * sa as u32) + (dst[i] as u32 * inv * da) / 255) / out_a) as u8;
            dst[i + 1] = (((color[1] as u32 * sa as u32) + (dst[i + 1] as u32 * inv * da) / 255)
                / out_a) as u8;
            dst[i + 2] = (((color[2] as u32 * sa as u32) + (dst[i + 2] as u32 * inv * da) / 255)
                / out_a) as u8;
            dst[i + 3] = out_a as u8;
        }
    }
}

fn blit_subpixel_as_mask(
    dst: &mut [u8],
    dw: u32,
    dh: u32,
    data: &[u8],
    mw: u32,
    mh: u32,
    dx: i32,
    dy: i32,
    color: [u8; 4],
) {
    // Swash SubpixelMask is tightly packed RGB (3 bytes/px) or RGBA depending on version.
    let bpp = if data.len() >= (mw * mh * 4) as usize {
        4
    } else {
        3
    };
    let mut mask = Vec::with_capacity((mw * mh) as usize);
    for i in 0..(mw * mh) as usize {
        let o = i * bpp;
        if o + 2 >= data.len() {
            mask.push(0);
            continue;
        }
        let r = data[o] as u32;
        let g = data[o + 1] as u32;
        let b = data[o + 2] as u32;
        mask.push(r.max(g).max(b) as u8);
    }
    blit_mask_straight(dst, dw, dh, &mask, mw, mh, dx, dy, color);
}

fn blit_rgba_straight(
    dst: &mut [u8],
    dw: u32,
    dh: u32,
    src: &[u8],
    sw: u32,
    sh: u32,
    dx: i32,
    dy: i32,
) {
    for row in 0..sh as i32 {
        let py = dy + row;
        if py < 0 || py >= dh as i32 {
            continue;
        }
        for col in 0..sw as i32 {
            let px = dx + col;
            if px < 0 || px >= dw as i32 {
                continue;
            }
            let si = ((row as u32 * sw + col as u32) * 4) as usize;
            if si + 3 >= src.len() {
                continue;
            }
            let sa = src[si + 3];
            if sa == 0 {
                continue;
            }
            let i = ((py as u32 * dw + px as u32) * 4) as usize;
            let da = dst[i + 3] as u32;
            let out_a = sa as u32 + ((255 - sa as u32) * da) / 255;
            if out_a == 0 {
                continue;
            }
            let inv = 255 - sa as u32;
            dst[i] =
                (((src[si] as u32 * sa as u32) + (dst[i] as u32 * inv * da) / 255) / out_a) as u8;
            dst[i + 1] = (((src[si + 1] as u32 * sa as u32) + (dst[i + 1] as u32 * inv * da) / 255)
                / out_a) as u8;
            dst[i + 2] = (((src[si + 2] as u32 * sa as u32) + (dst[i + 2] as u32 * inv * da) / 255)
                / out_a) as u8;
            dst[i + 3] = out_a as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_texture_sdf_edge_is_mid_gray() {
        // 8x8: solid 4x4 block in the center.
        let w = 8u32;
        let h = 8u32;
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for y in 2..6 {
            for x in 2..6 {
                let i = ((y * w + x) * 4) as usize;
                rgba[i + 3] = 255;
            }
        }
        let sdf = alpha_coverage_to_sdf_rgba(&rgba, w, h);
        fn median3(r: u8, g: u8, b: u8) -> u8 {
            let mut v = [r, g, b];
            v.sort_unstable();
            v[1]
        }
        // Far outside corner should be below the edge (128).
        assert_eq!(sdf[3], 255); // A
        let outside = median3(sdf[0], sdf[1], sdf[2]);
        assert!(
            outside < 128,
            "far outside median expected <128, got {outside}"
        );
        // Center inside should be > 128.
        let c = ((4 * w + 4) * 4) as usize;
        let inside = median3(sdf[c], sdf[c + 1], sdf[c + 2]);
        assert!(inside > 128, "inside median expected >128, got {inside}");
        assert_eq!(sdf[c + 3], 255);
        // Prefix constants stable for upload routing.
        assert_eq!(TEXT_XFORM_TEX_PREFIX, "rui_text_xform_v3:");
        assert_eq!(TEXT_MSDF_TEX_PREFIX, "rui_text_msdf_v2:");
    }

    #[test]
    fn text_texture_msdf_channels_diverge_near_box_corner() {
        let w = 32u32;
        let h = 32u32;
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for y in 8..24 {
            for x in 8..24 {
                let i = ((y * w + x) * 4) as usize;
                rgba[i + 3] = 255;
            }
        }
        let sdf = alpha_coverage_to_sdf_rgba(&rgba, w, h);
        let mut diverged = 0u32;
        for px in sdf.chunks_exact(4) {
            let max = px[0].max(px[1]).max(px[2]);
            let min = px[0].min(px[1]).min(px[2]);
            if (max as i32 - min as i32) > 6 {
                diverged += 1;
            }
        }
        assert!(
            diverged > 8,
            "true MSDF must split RGB near oriented edges, diverged={diverged}"
        );
        // Center median still inside.
        let c = ((16 * w + 16) * 4) as usize;
        let mut v = [sdf[c], sdf[c + 1], sdf[c + 2]];
        v.sort_unstable();
        assert!(v[1] > 128, "center median {}", v[1]);
    }

    #[test]
    fn bake_emoji_composites_twemoji_color_not_wireframe() {
        // Repro: sticky Bake without Twemoji used font Mask → lucide-like outline.
        let font = 28.0_f32;
        let layout_w = 48.0_f32;
        let layout_h = 48.0_f32;
        let (bw, bh, pad) = text_xform_bake_box(layout_w, layout_h, font);
        let Some((key, _)) = ensure_text_xform_texture(
            "👍",
            font,
            400,
            [0, 0, 0, 255], // black text color — wireframe path would be near-monochrome black
            layout_w,
            layout_h,
            bw,
            bh,
            pad,
            glyphon::cosmic_text::Align::Center,
            3.0,
            true, // would be MSDF; must be forced to RGBA Bake for emoji
            0.0,
        ) else {
            // Twemoji assets missing in some CI sandboxes — skip soft.
            return;
        };
        assert!(
            key.starts_with(TEXT_XFORM_TEX_PREFIX),
            "emoji must not use MSDF prefix, got {key}"
        );
        let Some((w, h, rgba)) = cache_peek(&key) else {
            panic!("bake cache miss after ensure");
        };
        assert!(w > 0 && h > 0);
        // Twemoji has chromatic pixels; a tinted outline bake is nearly R≈G≈B.
        let mut chromatic = 0u32;
        let mut opaque = 0u32;
        for px in rgba.chunks_exact(4) {
            if px[3] < 32 {
                continue;
            }
            opaque += 1;
            let max = px[0].max(px[1]).max(px[2]);
            let min = px[0].min(px[1]).min(px[2]);
            if (max as i32 - min as i32) > 24 {
                chromatic += 1;
            }
        }
        assert!(opaque > 20, "expected opaque emoji ink, opaque={opaque}");
        assert!(
            chromatic > 5,
            "expected Twemoji color variance (not wireframe), chromatic={chromatic}/opaque={opaque}"
        );
    }

    #[test]
    fn reaction_chip_hover_scale_twemoji_bake_is_not_clipped_in_rt() {
        // Reaction leaf: text-xs + leading-none in a 1rem chip. Mid hover:scale-125
        // must not clip Twemoji against the leading-none strut (square crop).
        let _g = cache_test_serial();
        cache_clear();
        fn bake_at(scale: f32) -> Option<(u32, u32, Vec<u8>, [u32; 4])> {
            let font = 12.0 * scale;
            let layout_w = 16.0 * scale;
            let layout_h = 12.0 * scale;
            let line_h = 12.0 * scale;
            let (bw, bh, pad) = text_xform_bake_box_ex(layout_w, layout_h, font, 0.0, line_h);
            let (key, _) = ensure_text_xform_texture(
                "😮",
                font,
                400,
                [0, 0, 0, 255],
                layout_w,
                layout_h,
                bw,
                bh,
                pad,
                glyphon::cosmic_text::Align::Center,
                crate::renderer::BAKE_OVERSAMPLE,
                false,
                0.0,
            )?;
            let (w, h, rgba) = cache_peek(&key)?;
            let ink = opaque_ink_rect(&rgba, w, h)?;
            Some((w, h, rgba, ink))
        }
        let Some((rw, rh, _, rest_ink)) = bake_at(1.0) else {
            return;
        };
        let Some((hw, hh, _, hover_ink)) = bake_at(1.17) else {
            return;
        };
        let rest_touch = rest_ink[0] == 0
            || rest_ink[1] == 0
            || rest_ink[0] + rest_ink[2] >= rw
            || rest_ink[1] + rest_ink[3] >= rh;
        let hover_touch = hover_ink[0] == 0
            || hover_ink[1] == 0
            || hover_ink[0] + hover_ink[2] >= hw
            || hover_ink[1] + hover_ink[3] >= hh;
        assert!(
            !rest_touch,
            "rest reaction Bake already clips Twemoji (square crop, then hover:scale enlarges the cut); rest_rt={rw}x{rh} rest_ink={rest_ink:?}"
        );
        assert!(
            !hover_touch,
            "scaled reaction Bake must keep Twemoji inside the RT; rest_rt={rw}x{rh} rest_ink={rest_ink:?} hover_rt={hw}x{hh} hover_ink={hover_ink:?}"
        );
        let ink_ratio_h = hover_ink[3] as f32 / rest_ink[3] as f32;
        assert!(
            ink_ratio_h > 1.17 * 0.88,
            "hover Twemoji ink must grow with scale (crop to rest strut keeps ink flat); rest_ink={rest_ink:?} hover_ink={hover_ink:?} ratio_h={ink_ratio_h}"
        );
        let rest_frac_w = rest_ink[2] as f32 / rw as f32;
        let hover_frac_w = hover_ink[2] as f32 / hw as f32;
        assert!(
            (hover_frac_w - rest_frac_w).abs() < 0.08,
            "ink/RT fraction must stay similar under near-identity scale (crop shrinks pad); rest={rest_frac_w} hover={hover_frac_w} rest_ink={rest_ink:?} hover_ink={hover_ink:?}"
        );
    }

    #[test]
    fn text_texture_reject_empty_alpha() {
        let empty = vec![0u8; 4 * 4 * 4];
        assert!(!empty.chunks_exact(4).any(|p| p[3] > 0));
    }

    #[test]
    fn text_xform_bake_box_grows_tight_height_to_leading_plus_pad() {
        let font = 14.0_f32;
        let (w, h, pad) = text_xform_bake_box(40.0, font, font);
        let leading = font * crate::style::tailwind_default_leading_scale(font);
        assert!(pad >= 2.0);
        assert!(
            h + 0.01 >= leading + pad * 2.0,
            "bake h={h} must fit leading={leading} + 2*pad={pad}"
        );
        assert!((w - (40.0 + pad * 2.0)).abs() < 0.01);
    }

    #[test]
    fn bake_glyph_layout_dy_only_when_layout_taller_than_line() {
        let font = 14.0_f32;
        let line_h = super::bake_default_line_height_px(font);
        assert_eq!(
            super::bake_glyph_layout_dy(line_h, line_h, 1, true, false),
            0.0,
            "intrinsic-height leaf must not get vertical dy"
        );
        let tall = line_h + 12.0;
        let dy = super::bake_glyph_layout_dy(tall, line_h, 1, true, false);
        assert!((dy - (tall - line_h) * 0.5).abs() < 0.01);
        assert_eq!(
            super::bake_glyph_layout_dy(tall, line_h, 2, true, false),
            0.0
        );
        assert_eq!(
            super::bake_glyph_layout_dy(tall, line_h, 1, false, false),
            0.0
        );
        // Label `leading-none`: 1em line box in a slightly taller leaf → ~1px dy,
        // not the old Tailwind-default (~20px) which skipped dy and sat low.
        let none = font;
        let chip = font + 2.0;
        let dy_none = super::bake_glyph_layout_dy(chip, none, 1, true, false);
        assert!(
            (dy_none - 1.0).abs() < 0.01,
            "leading-none in a 16px leaf must dy by 1px, got {dy_none}"
        );
    }

    #[test]
    fn bake_glyph_layout_dy_centers_single_line_in_tall_layout_box() {
        let font = 14.0_f32;
        let line_h = super::bake_default_line_height_px(font);
        let layout_w = 80.0_f32;
        let layout_h = 28.0_f32;
        let dy = super::bake_glyph_layout_dy(layout_h, line_h, 1, true, false);
        assert!(
            dy > 0.0,
            "tall padded layout box should get Coverage-matching dy"
        );
        let (bw, bh, pad) = text_xform_bake_box(layout_w, layout_h, font);
        let Some((w, h, rgba)) = super::rasterize_text_rgba(
            "居中",
            font,
            600,
            [0, 0, 0, 255],
            layout_w,
            layout_h,
            bw,
            bh,
            pad,
            glyphon::cosmic_text::Align::Center,
            2.0,
            None,
            dy,
            line_h,
        ) else {
            return;
        };
        let mut min_y = h as i32;
        let mut max_y = 0i32;
        for y in 0..h {
            for x in 0..w {
                if rgba[((y * w + x) * 4 + 3) as usize] > 16 {
                    min_y = min_y.min(y as i32);
                    max_y = max_y.max(y as i32);
                }
            }
        }
        let ink_cy = (min_y + max_y) as f32 * 0.5;
        let layout_cy = (dy * 2.0 + font) * 0.5 * 2.0; // rough: ink should sit near layout mid
        let _ = layout_cy;
        let tex_cy = h as f32 * 0.5;
        assert!(
            (ink_cy - tex_cy).abs() < h as f32 * 0.15,
            "ink cy={ink_cy} should be near texture center {tex_cy} when dy={dy}"
        );
    }

    #[test]
    fn bake_right_align_ink_ends_in_layout_region_not_bake_edge() {
        let font = 14.0_f32;
        let layout_w = 96.0_f32;
        let layout_h = 20.0_f32;
        let (bw, bh, pad) = text_xform_bake_box(layout_w, layout_h, font);
        let dpr = 2.0_f32;
        let Some((w, h, rgba)) = super::rasterize_text_rgba(
            "OK",
            font,
            600,
            [0, 0, 0, 255],
            layout_w,
            layout_h,
            bw,
            bh,
            pad,
            glyphon::cosmic_text::Align::Right,
            dpr,
            None,
            0.0,
            super::bake_default_line_height_px(font),
        ) else {
            return;
        };
        let layout_right_px = ((pad + layout_w) * dpr).round() as i32;
        let bake_right_px = (bw * dpr).round() as i32;
        let mut max_ink_x = 0i32;
        for y in 0..h {
            for x in 0..w {
                if rgba[((y * w + x) * 4 + 3) as usize] > 16 {
                    max_ink_x = max_ink_x.max(x as i32);
                }
            }
        }
        let _ = bake_right_px;
        assert!(
            max_ink_x <= layout_right_px + 2,
            "right-aligned ink max_x={max_ink_x} should be near layout right {layout_right_px}"
        );
        assert!(
            max_ink_x < bake_right_px - 2,
            "ink must not hug bake texture edge when layout is inset by pad"
        );
    }

    #[test]
    fn bake_tight_box_no_longer_clips_cjk_ink() {
        // Repro: layout height == font-size (no leading) used to clip glyph tops/bottoms.
        let font = 12.0_f32;
        let tight_h = font;
        let (bw, bh, pad) = text_xform_bake_box(64.0, tight_h, font);
        let Some((_, _)) = ensure_text_xform_texture(
            "旋转中",
            font,
            600,
            [255, 255, 255, 255],
            64.0,
            tight_h,
            bw,
            bh,
            pad,
            glyphon::cosmic_text::Align::Center,
            2.0,
            false,
            0.0,
        ) else {
            // Font missing in CI — skip rather than false-fail.
            return;
        };
        // Pull RGBA from cache via second ensure (same key) by re-rasterizing check:
        // top/bottom border rows of a padded bake must stay mostly empty.
        let (w, h, rgba) = super::rasterize_text_rgba(
            "旋转中",
            font,
            600,
            [255, 255, 255, 255],
            64.0,
            tight_h,
            bw,
            bh,
            pad,
            glyphon::cosmic_text::Align::Center,
            2.0,
            None,
            0.0,
            super::bake_default_line_height_px(font),
        )
        .expect("rasterize");
        let border = 1usize;
        let mut top_ink = 0u32;
        let mut bot_ink = 0u32;
        for y in 0..border {
            for x in 0..w as usize {
                if rgba[(y * w as usize + x) * 4 + 3] > 16 {
                    top_ink += 1;
                }
            }
        }
        for y in (h as usize - border)..h as usize {
            for x in 0..w as usize {
                if rgba[(y * w as usize + x) * 4 + 3] > 16 {
                    bot_ink += 1;
                }
            }
        }
        assert_eq!(top_ink, 0, "top pad row must not contain glyph ink");
        assert_eq!(bot_ink, 0, "bottom pad row must not contain glyph ink");
    }

    #[test]
    fn bake_leading_none_ink_matches_coverage_not_default_leading() {
        // Gallery Label is `leading-none` (1em). Bake used to shape with Tailwind
        // text-sm leading (~1.43em) and grow the quad — cosmic half-leading plus
        // pad-ceil leftover read as ~1px down vs Coverage.
        let font = 14.0_f32;
        let layout_w = 96.0_f32;
        let layout_h = font;
        let none = font;
        let def = super::bake_default_line_height_px(font);
        assert!(
            def - none > 4.0,
            "text-sm default leading must be distinctly taller than 1em"
        );

        let (_, bh_none, _) = super::text_xform_bake_box_ex(layout_w, layout_h, font, 0.0, none);
        let (_, bh_def, _) = super::text_xform_bake_box_ex(layout_w, layout_h, font, 0.0, def);
        assert!(
            bh_none + 0.5 < bh_def,
            "leading-none bake box must be shorter than text-sm leading ({bh_none} vs {bh_def})"
        );

        // Metrics must actually change Cosmic line_y (same RT, origin from `pad`
        // not integer-centered shape — otherwise extra shape_h cancels half-leading).
        let dpr = crate::renderer::BAKE_OVERSAMPLE;
        let (bw, bh, pad) = super::text_xform_bake_box_ex(layout_w, layout_h, font, 0.0, none);
        let line_y = |line_h: f32| -> Option<f32> {
            crate::text::measure::with_layout_font_system(|fs| {
                crate::renderer::font_loader::ensure_coverage_for_text(fs, "拖到四角");
                let size = (font * dpr).max(1.0);
                let lh = (line_h * dpr).max(1.0);
                let mut buffer = glyphon::Buffer::new(fs, glyphon::Metrics::new(size, lh));
                buffer.set_size(Some((layout_w * dpr).ceil().max(1.0)), None);
                let family = crate::renderer::text_hit::best_font_family(fs);
                let attrs = glyphon::Attrs::new().family(family);
                buffer.set_text("拖到四角", &attrs, glyphon::Shaping::Advanced, None);
                buffer.shape_until_scroll(fs, false);
                buffer.layout_runs().next().map(|r| r.line_y / dpr)
            })
        };
        let Some(y_none) = line_y(none) else {
            return;
        };
        let Some(y_def) = line_y(def) else {
            return;
        };
        assert!(
            y_def > y_none + 0.35,
            "taller Cosmic line box must half-lead downward (none={y_none:.2} def={y_def:.2})"
        );
        let _ = (bw, bh, pad);
    }

    #[test]
    fn text_xform_cpu_cache_lru_keeps_hot_drops_cold() {
        let _serial = cache_test_serial();
        cache_clear();
        let _ = crate::app::pending_images::take_pending();
        let px = vec![255u8, 0, 0, 255];
        for i in 0..MAX_CACHE {
            cache_put(format!("lru-k{i}"), 1, 1, px.clone());
        }
        // Pretend GPU holds everything; clear `needed` like a completed frame.
        let _ = flush_text_xform_textures(|_| true);
        assert!(cache_touch("lru-k0"));
        cache_put("lru-knew".into(), 1, 1, px);
        let _ = flush_text_xform_textures(|_| true);
        assert!(cache_peek("lru-k0").is_some(), "hot key must survive LRU");
        assert!(
            cache_peek("lru-knew").is_some(),
            "newest insert must remain"
        );
        assert!(
            cache_peek("lru-k1").is_none(),
            "oldest cold key should be evicted"
        );
        cache_clear();
    }

    #[test]
    fn text_xform_flush_requeues_only_needed_missing_from_gpu() {
        let _serial = cache_test_serial();
        cache_clear();
        let _ = crate::app::pending_images::take_pending();
        let px = vec![0u8, 255, 0, 255];
        cache_put("need-a".into(), 1, 1, px.clone());
        cache_put("cold-b".into(), 1, 1, px);
        let _ = crate::app::pending_images::take_pending();
        let _ = flush_text_xform_textures(|_| true);
        assert!(cache_touch("need-a"));
        let any = flush_text_xform_textures(|_| false);
        assert!(any, "needed key missing from GPU must re-queue");
        let pending = crate::app::pending_images::take_pending();
        let keys: Vec<&str> = pending.iter().map(|(k, _, _, _)| k.as_str()).collect();
        assert!(keys.contains(&"need-a"), "got {keys:?}");
        assert!(
            !keys.contains(&"cold-b"),
            "cold CPU entries must not refill GPU, got {keys:?}"
        );
        cache_clear();
    }

    #[test]
    fn text_xform_cpu_cache_byte_budget_evicts() {
        let _serial = cache_test_serial();
        cache_clear();
        let _ = crate::app::pending_images::take_pending();
        // 4 MiB × 5 > 16 MiB cap.
        let fat = vec![0u8; 1024 * 1024 * 4];
        for i in 0..5 {
            cache_put(format!("fat-{i}"), 1024, 1024, fat.clone());
            let _ = flush_text_xform_textures(|_| true);
        }
        let (n, bytes) = text_xform_cpu_cache_stats();
        assert!(
            bytes <= MAX_CACHE_BYTES,
            "byte budget exceeded: {bytes} bytes, n={n}"
        );
        assert!(n < 5, "expected eviction under byte cap, n={n}");
        cache_clear();
    }

    #[test]
    fn is_text_xform_texture_key_matches_prefixes() {
        assert!(is_text_xform_texture_key(TEXT_XFORM_TEX_PREFIX));
        assert!(is_text_xform_texture_key(TEXT_MSDF_TEX_PREFIX));
        assert!(is_text_xform_texture_key("rui_text_xform_v3:deadbeef"));
        assert!(!is_text_xform_texture_key("rui_path:x"));
    }

    fn bake_rgba(text: &str, weight: u16, dpr: f32) -> Option<(u32, u32, Vec<u8>)> {
        let font = 14.0_f32;
        let layout_w = 96.0_f32;
        let layout_h = 20.0_f32;
        let (bw, bh, pad) = text_xform_bake_box(layout_w, layout_h, font);
        let (key, _) = ensure_text_xform_texture(
            text,
            font,
            weight,
            [20, 20, 20, 255],
            layout_w,
            layout_h,
            bw,
            bh,
            pad,
            glyphon::cosmic_text::Align::Left,
            dpr,
            false,
            0.0,
        )?;
        cache_peek(&key)
    }

    fn bake_alpha_mass(text: &str, weight: u16) -> Option<u32> {
        bake_rgba(text, weight, 2.0)
            .map(|(_, _, rgba)| rgba.chunks_exact(4).map(|p| p[3] as u32).sum())
    }

    fn bake_fringe_ratio(rgba: &[u8]) -> f32 {
        let mut solid = 0u32;
        let mut fringe = 0u32;
        for px in rgba.chunks_exact(4) {
            let a = px[3];
            if a > 160 {
                solid += 1;
            } else if a > 12 {
                fringe += 1;
            }
        }
        let den = (solid + fringe).max(1) as f32;
        fringe as f32 / den
    }

    #[test]
    fn bake_font_medium_applies_synthetic_bold_when_face_missing() {
        // Gallery Label is font-medium (500). Coverage thickens Regular with a
        // 0.18/0.32 halo; Bake (rotate / DragOverlay 替身) must do the same or
        // the ghost looks thinner than the source.
        let _serial = cache_test_serial();
        cache_clear();
        let needs_synth = crate::text::measure::with_layout_font_system(|fs| {
            let family = crate::renderer::text_hit::best_font_family(fs);
            crate::utils::font::resolve_face_weight(fs.db(), family, 500).synthetic_bold
        });
        if !needs_synth {
            return;
        }
        let mass400 = bake_alpha_mass("拖到四角", 400).expect("regular bake");
        let mass500 = bake_alpha_mass("拖到四角", 500).expect("medium bake");
        assert!(
            mass500 > mass400 + mass400 / 80,
            "Bake font-medium must apply font-synthesis:weight (400={mass400} 500={mass500})"
        );
    }

    #[test]
    fn bake_synthetic_bold_stays_sharp_at_oversample() {
        // Coverage fake-bold is a 0.18px duplicate TextArea (blur 0.32 < 0.75).
        // Bake stamps in supersampled px; 0.32 * BAKE_OVERSAMPLE (3) = 0.96 used to
        // trip the 0.75 bake-pixel blur threshold and become a soft glow — thinner
        // than the source after bilinear minify (DragOverlay rotate ghost).
        let _serial = cache_test_serial();
        cache_clear();
        let needs_synth = crate::text::measure::with_layout_font_system(|fs| {
            let family = crate::renderer::text_hit::best_font_family(fs);
            crate::utils::font::resolve_face_weight(fs.db(), family, 500).synthetic_bold
        });
        if !needs_synth {
            return;
        }
        let (_, _, rgba1) = bake_rgba("拖到四角", 500, 1.0).expect("dpr1");
        let (_, _, rgba3) =
            bake_rgba("拖到四角", 500, crate::renderer::BAKE_OVERSAMPLE).expect("dpr3");
        let r1 = bake_fringe_ratio(&rgba1);
        let r3 = bake_fringe_ratio(&rgba3);
        assert!(
            r3 <= r1 + 0.06,
            "3× Bake synth must stay a sharp duplicate like Coverage, not a soft glow (fringe dpr1={r1:.3} dpr3={r3:.3})"
        );
    }

    fn box_downscale(rgba: &[u8], w: u32, h: u32, factor: u32) -> (u32, u32, Vec<u8>) {
        let f = factor.max(1);
        let nw = (w / f).max(1);
        let nh = (h / f).max(1);
        let mut out = vec![0u8; (nw * nh * 4) as usize];
        let f2 = (f * f) as u32;
        for y in 0..nh {
            for x in 0..nw {
                let mut a = 0u32;
                let mut r = 0u32;
                let mut g = 0u32;
                let mut b = 0u32;
                for dy in 0..f {
                    for dx in 0..f {
                        let sx = x * f + dx;
                        let sy = y * f + dy;
                        if sx >= w || sy >= h {
                            continue;
                        }
                        let i = ((sy * w + sx) * 4) as usize;
                        r += rgba[i] as u32;
                        g += rgba[i + 1] as u32;
                        b += rgba[i + 2] as u32;
                        a += rgba[i + 3] as u32;
                    }
                }
                let o = ((y * nw + x) * 4) as usize;
                out[o] = (r / f2) as u8;
                out[o + 1] = (g / f2) as u8;
                out[o + 2] = (b / f2) as u8;
                out[o + 3] = (a / f2) as u8;
            }
        }
        (nw, nh, out)
    }

    #[test]
    fn bake_oversample_minified_keeps_label_weight() {
        // DragOverlay rotate bakes at 3× then bilinear-minifies. Small-size glyphon
        // Coverage (本体) is hinted at 14px and looks heavier; the ghost used to
        // read as Regular. Minified 3× ink must stay close to 1× bake of the same label.
        let _serial = cache_test_serial();
        cache_clear();
        let (_, _, r1) = bake_rgba("拖到四角", 500, 1.0).expect("dpr1");
        let (w3, h3, r3) =
            bake_rgba("拖到四角", 500, crate::renderer::BAKE_OVERSAMPLE).expect("dpr3");
        let factor = crate::renderer::BAKE_OVERSAMPLE.round() as u32;
        let (_, _, down) = box_downscale(&r3, w3, h3, factor);
        let ink = |rgba: &[u8]| rgba.chunks_exact(4).map(|p| p[3] as u32).sum::<u32>() as f32;
        let m1 = ink(&r1);
        let md = ink(&down);
        assert!(
            md >= m1 * 0.95 && md <= m1 * 1.08,
            "3× Bake minified must match 1× (Coverage-like) weight: 1x={m1:.0} down={md:.0} ratio={:.3}",
            md / m1.max(1.0)
        );
    }
}
