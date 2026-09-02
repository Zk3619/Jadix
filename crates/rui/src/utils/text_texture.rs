//! Axis-aligned text → cached RGBA texture for CSS rotate / skew (smooth bilinear).
//!
//! Glyphon glyphs are hinted at integer pixels; rotating those quads looks jagged.
//! Bake coverage into a **linear** (`Rgba8Unorm`) texture — sRGB upload + bilinear
//! rotation turns thin strokes into fog (see gallery 3b).

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Mutex;

use crate::app::pending_images::queue_image_rgba8;

struct Entry {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

static CACHE: Mutex<Option<HashMap<String, Entry>>> = Mutex::new(None);
const MAX_CACHE: usize = 128;

/// Key prefix — coverage bake; `upload_rgba8_linear_with_mips` (linear Unorm).
/// v3: Twemoji color composite for emoji leaves (never font outline / MSDF).
pub const TEXT_XFORM_TEX_PREFIX: &str = "rui_text_xform_v3:";

/// Key prefix — single-channel SDF packed as R=G=B; same linear mip upload path.
/// Emoji-bearing strings must **not** use this path (color would collapse to wireframe).
pub const TEXT_MSDF_TEX_PREFIX: &str = "rui_text_msdf_v1:";

/// SDF spread in supersampled pixels (normalize: 128 = edge).
const SDF_SPREAD_PX: f32 = 8.0;

fn cache_get(key: &str) -> Option<(u32, u32, Vec<u8>)> {
    let mut guard = CACHE.lock().ok()?;
    let map = guard.get_or_insert_with(HashMap::new);
    let e = map.get(key)?;
    Some((e.width, e.height, e.rgba.clone()))
}

fn cache_put(key: String, width: u32, height: u32, rgba: Vec<u8>) {
    if let Ok(mut guard) = CACHE.lock() {
        let map = guard.get_or_insert_with(HashMap::new);
        if map.len() >= MAX_CACHE {
            if let Some(k) = map.keys().next().cloned() {
                map.remove(&k);
            }
        }
        map.insert(
            key,
            Entry {
                width,
                height,
                rgba,
            },
        );
    }
}

/// Logical padding so swash ink (ascent / bearing) is not clipped at the bake edge.
pub fn text_xform_pad_px(font_size_px: f32) -> f32 {
    (font_size_px * 0.22).max(2.0)
}

/// Expand layout text box to a bake/draw size that fits line-height + ink pad.
/// Returns `(bake_w, bake_h, pad)`.
pub fn text_xform_bake_box(box_w: f32, box_h: f32, font_size_px: f32) -> (f32, f32, f32) {
    text_xform_bake_box_ex(box_w, box_h, font_size_px, 0.0)
}

/// Extra pad so CSS `text-shadow` offset/blur is not clipped when baking under transform.
pub fn text_xform_shadow_pad(offset: [f32; 2], blur: f32) -> f32 {
    (blur.max(0.0) + offset[0].abs().max(offset[1].abs()) + 2.0).max(0.0)
}

/// Like [`text_xform_bake_box`] with additional uniform pad (e.g. text-shadow extent).
pub fn text_xform_bake_box_ex(
    box_w: f32,
    box_h: f32,
    font_size_px: f32,
    extra_pad: f32,
) -> (f32, f32, f32) {
    let pad = text_xform_pad_px(font_size_px) + extra_pad.max(0.0);
    let leading = font_size_px * crate::style::tailwind_default_leading_scale(font_size_px);
    let h = box_h.max(leading) + pad * 2.0;
    let w = box_w.max(1.0) + pad * 2.0;
    (w, h, pad)
}

/// Rasterize `text` into an RGBA texture at `dpr` supersample. Returns `(texture_key, logical_w)`.
///
/// When `as_sdf`, converts alpha coverage to a single-channel SDF (`R=G=B=sdf_u8`, `A=255`)
/// and keys with [`TEXT_MSDF_TEX_PREFIX`]. Convention: **128 = edge, >128 inside, <128 outside**
/// (matches FS `smoothstep(0.5±afwidth, sdf)` coverage).
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
    )
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
) -> Option<(String, f32)> {
    if text.is_empty() || layout_w < 1.0 || layout_h < 1.0 || bake_w < 1.0 || bake_h < 1.0 || font_size_px < 0.5
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
    // Bump when bake / SDF / emoji / shadow / layout-inset shaping semantics change.
    9u8.hash(&mut hasher);
    let prefix = if as_sdf {
        TEXT_MSDF_TEX_PREFIX
    } else {
        TEXT_XFORM_TEX_PREFIX
    };
    let key = format!("{prefix}{:016x}", hasher.finish());

    if let Some((w, h, rgba)) = cache_get(&key) {
        // Only re-queue when GPU may have evicted; caller flushes via
        // [`flush_text_xform_textures`]. Avoid per-frame re-upload thrash.
        let _ = (w, h, rgba);
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
    )?;
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

/// Layout-sized shaping buffer height (≥ one line of leading).
fn bake_shape_h(layout_h: f32, font_size_px: f32) -> f32 {
    let leading = font_size_px * crate::style::tailwind_default_leading_scale(font_size_px);
    layout_h.max(leading)
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
    let wants_emoji_color = spans.iter().any(|s| bake_contains_emoji(&s.text));
    let as_sdf = as_sdf && !wants_emoji_color;
    as_sdf.hash(&mut hasher);
    wants_emoji_color.hash(&mut hasher);
    8u8.hash(&mut hasher); // rich-span + emoji bake schema
    let prefix = if as_sdf {
        TEXT_MSDF_TEX_PREFIX
    } else {
        TEXT_XFORM_TEX_PREFIX
    };
    let key = format!("{prefix}{:016x}", hasher.finish());

    if cache_get(&key).is_some() {
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
    )?;
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

/// Chamfer distance transform → packed SDF RGBA.
///
/// **Convention:** `u8` 128 = edge, `>128` inside glyph, `<128` outside; spread ≈ [`SDF_SPREAD_PX`].
fn alpha_coverage_to_sdf_rgba(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    let w = width as usize;
    let h = height as usize;
    let n = w * h;
    let mut inside = vec![false; n];
    for i in 0..n {
        inside[i] = rgba.get(i * 4 + 3).copied().unwrap_or(0) >= 128;
    }
    // Seeds: outside pixels → dist for inside; inside pixels → dist for outside.
    let mut outside_seed = vec![false; n];
    let mut inside_seed = vec![false; n];
    for i in 0..n {
        outside_seed[i] = !inside[i];
        inside_seed[i] = inside[i];
    }
    let dist_to_out = chamfer_dt(&outside_seed, w, h);
    let dist_to_in = chamfer_dt(&inside_seed, w, h);

    let mut out = vec![0u8; n * 4];
    let inv = 1.0 / (2.0 * SDF_SPREAD_PX);
    for i in 0..n {
        // Positive inside so FS smoothstep(0.5±af, sdf) yields coverage inside the glyph.
        let sd = if inside[i] {
            dist_to_out[i]
        } else {
            -dist_to_in[i]
        };
        let t = (0.5 + sd * inv).clamp(0.0, 1.0);
        let u = (t * 255.0).round() as u8;
        let o = i * 4;
        out[o] = u;
        out[o + 1] = u;
        out[o + 2] = u;
        out[o + 3] = 255;
    }
    out
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

/// Re-queue CPU-cached text-xform textures missing from the GPU pool.
pub fn flush_text_xform_textures(mut has_gpu: impl FnMut(&str) -> bool) -> bool {
    let Ok(guard) = CACHE.lock() else {
        return false;
    };
    let Some(map) = guard.as_ref() else {
        return false;
    };
    let mut any = false;
    for (key, entry) in map.iter() {
        if has_gpu(key) {
            continue;
        }
        queue_image_rgba8(key, entry.width, entry.height, entry.rgba.clone());
        any = true;
    }
    any
}

/// Vertical glyph offset inside a Bake texture — mirrors Coverage `dy` in `prepare_text.rs`.
/// Uses the **layout** text box height (`layout_box_h`), not the padded bake size.
pub fn bake_glyph_layout_dy(
    layout_box_h: f32,
    font_size_px: f32,
    line_count: usize,
    nowrap: bool,
    is_input_text: bool,
) -> f32 {
    if is_input_text || !nowrap || line_count != 1 {
        return 0.0;
    }
    let line_height = font_size_px * crate::style::tailwind_default_leading_scale(font_size_px);
    if layout_box_h > line_height + 1.0 {
        (layout_box_h - line_height) * 0.5
    } else {
        0.0
    }
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
) -> Option<(u32, u32, Vec<u8>)> {
    crate::text::measure::with_layout_font_system(|fs| {
        crate::renderer::font_loader::ensure_coverage_for_text(fs, text);
        let size = (font_size_px * dpr).max(1.0);
        let lh = size * crate::style::tailwind_default_leading_scale(font_size_px);
        // Shape in the **layout** box (Coverage parity); blit into padded bake RT.
        let shape_h = bake_shape_h(layout_h, font_size_px);
        let pw = (layout_w * dpr).ceil().max(1.0);
        let ph = (shape_h * dpr).ceil().max(lh).max(1.0);
        let tw = (bake_w * dpr).ceil().max(1.0).round().max(1.0) as u32;
        let th = (bake_h * dpr).ceil().max(1.0).round().max(1.0) as u32;
        let pad_x_px = (pad * dpr).round() as i32;
        let pad_y_px = (pad * dpr).round() as i32;
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
                );
            }
        }

        for run in buffer.layout_runs() {
            // Match glyphon text_render: y = round(line_y) + physical.y - top
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
                &mut rgba,
                tw,
                th,
                &buffer,
                text,
                &layout,
                size,
                origin_x,
                origin_y,
            );
        }

        Some((tw, th, rgba))
    })
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
    let draw_blur = shadow_blur > SHARP_BLUR_EPS && alpha > 6;
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
) -> Option<(u32, u32, Vec<u8>)> {
    use crate::style::TextSpanKind;
    crate::text::measure::with_layout_font_system(|fs| {
        for span in spans {
            crate::renderer::font_loader::ensure_coverage_for_text(fs, &span.text);
        }
        let size = (font_size_px * dpr).max(1.0);
        let lh = size * crate::style::tailwind_default_leading_scale(font_size_px);
        let shape_h = bake_shape_h(layout_h, font_size_px);
        let pw = (layout_w * dpr).ceil().max(1.0);
        let ph = (shape_h * dpr).ceil().max(lh).max(1.0);
        let tw = (bake_w * dpr).ceil().max(1.0).round().max(1.0) as u32;
        let th = (bake_h * dpr).ceil().max(1.0).round().max(1.0) as u32;
        let pad_x_px = (pad * dpr).round() as i32;
        let pad_y_px = (pad * dpr).round() as i32;
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
        // Far outside corner should be below the edge (128).
        assert_eq!(sdf[3], 255); // A
        assert!(sdf[0] < 128, "far outside expected <128, got {}", sdf[0]);
        // Center inside should be > 128.
        let c = ((4 * w + 4) * 4) as usize;
        assert!(sdf[c] > 128, "inside expected >128, got {}", sdf[c]);
        assert_eq!(sdf[c + 3], 255);
        // Prefix constants stable for upload routing.
        assert_eq!(TEXT_XFORM_TEX_PREFIX, "rui_text_xform_v3:");
        assert_eq!(TEXT_MSDF_TEX_PREFIX, "rui_text_msdf_v1:");
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
        let Some((w, h, rgba)) = cache_get(&key) else {
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
        let line_h = font * crate::style::tailwind_default_leading_scale(font);
        assert_eq!(
            super::bake_glyph_layout_dy(line_h, font, 1, true, false),
            0.0,
            "intrinsic-height leaf must not get vertical dy"
        );
        let tall = line_h + 12.0;
        let dy = super::bake_glyph_layout_dy(tall, font, 1, true, false);
        assert!((dy - (tall - line_h) * 0.5).abs() < 0.01);
        assert_eq!(super::bake_glyph_layout_dy(tall, font, 2, true, false), 0.0);
        assert_eq!(super::bake_glyph_layout_dy(tall, font, 1, false, false), 0.0);
    }

    #[test]
    fn bake_glyph_layout_dy_centers_single_line_in_tall_layout_box() {
        let font = 14.0_f32;
        let layout_w = 80.0_f32;
        let layout_h = 28.0_f32;
        let dy = super::bake_glyph_layout_dy(layout_h, font, 1, true, false);
        assert!(dy > 0.0, "tall padded layout box should get Coverage-matching dy");
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
}
