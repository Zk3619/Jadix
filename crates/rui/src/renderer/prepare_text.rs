//! 将布局得到的 `TextLayoutInfo` 转为 glyphon 文本区与下划线实例，并执行 `prepare`。
//!
//! 调试：设置环境变量 `RUI_TEXT_DEBUG`（见 [`super::text_prepare_debug`]）。

use std::borrow::Cow;
use std::collections::hash_map::Entry;
use std::sync::OnceLock;
use std::time::Instant;
use std::hash::{Hash, Hasher};
use unicode_segmentation::UnicodeSegmentation;

use wgpu::{Device, Queue, SurfaceConfiguration};

use crate::style::{tailwind_default_leading_scale, TextDecorationStyle};
use crate::core::tree::TextLayoutInfo;

use super::text::RuiTextRenderer;
use super::text_hit::{best_font_family, measure_prefix_width};
use super::InstanceInput;

#[derive(Clone)]
struct FrameTextNodeMeta {
    x: f32,
    y: f32,
    box_w: f32,
    box_h: f32,
    dy: f32,
    text_size: f32,
    align: glyphon::cosmic_text::Align,
    measured_text_w: f32,
    underline: bool,
    overline: bool,
    line_through: bool,
    decoration_style: TextDecorationStyle,
    decoration_color: Option<[u8; 4]>,
    decoration_thickness: f32,
    underline_offset: f32,
    color: [u8; 4],
    clip_bounds: Option<[f32; 4]>,
    shadow_col: Option<[u8; 4]>,
    shadow_dx: f32,
    shadow_dy: f32,
    shadow_blur: f32,
    emoji_cache_key: Option<super::text::EmojiCacheKey>,
}

fn align_to_u8(align: glyphon::cosmic_text::Align) -> u8 {
    match align {
        glyphon::cosmic_text::Align::Left => 0,
        glyphon::cosmic_text::Align::Center => 1,
        glyphon::cosmic_text::Align::Right => 2,
        glyphon::cosmic_text::Align::Justified => 3,
        glyphon::cosmic_text::Align::End => 4,
    }
}

fn family_hash(family: glyphon::Family) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    match family {
        glyphon::Family::Name(name) => {
            1u8.hash(&mut h);
            name.hash(&mut h);
        }
        glyphon::Family::Serif => 2u8.hash(&mut h),
        glyphon::Family::SansSerif => 3u8.hash(&mut h),
        glyphon::Family::Monospace => 4u8.hash(&mut h),
        glyphon::Family::Cursive => 5u8.hash(&mut h),
        glyphon::Family::Fantasy => 6u8.hash(&mut h),
    }
    h.finish()
}

fn node_shape_hash(
    text: &str,
    scaled_size: f32,
    line_height: f32,
    scaled_w: f32,
    scaled_h: f32,
    weight: u16,
    family: glyphon::Family,
    align: glyphon::cosmic_text::Align,
    shaping: glyphon::Shaping,
) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    ((scaled_size * 10.0).clamp(0.0, u16::MAX as f32) as u16).hash(&mut h);
    ((line_height * 10.0).clamp(0.0, u16::MAX as f32) as u16).hash(&mut h);
    ((scaled_w * 10.0).clamp(0.0, u16::MAX as f32) as u16).hash(&mut h);
    ((scaled_h * 10.0).clamp(0.0, u16::MAX as f32) as u16).hash(&mut h);
    weight.hash(&mut h);
    family_hash(family).hash(&mut h);
    align_to_u8(align).hash(&mut h);
    let shaping_mode: u8 = match shaping {
        glyphon::Shaping::Advanced => 1,
        _ => 0,
    };
    shaping_mode.hash(&mut h);
    h.finish()
}

fn make_dummy_cache_entry(font_system: &mut glyphon::FontSystem) -> super::text::ShapedTextEntry {
    let mut buffer = glyphon::Buffer::new(font_system, glyphon::Metrics::new(1.0, 1.0));
    buffer.set_size(font_system, Some(1.0), Some(1.0));
    super::text::ShapedTextEntry {
        key_hash: 0,
        buffer,
        measured_text_w: 0.0,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TextPerfMode {
    HighQuality,
    Balanced,
    Fast,
}

fn text_perf_mode() -> TextPerfMode {
    static MODE: OnceLock<TextPerfMode> = OnceLock::new();
    *MODE.get_or_init(|| {
        let raw = std::env::var("RUI_TEXT_PERF")
            .unwrap_or_else(|_| "balanced".to_string())
            .to_ascii_lowercase();
        match raw.as_str() {
            "fast" | "low" => TextPerfMode::Fast,
            "high" | "quality" => TextPerfMode::HighQuality,
            _ => TextPerfMode::Balanced,
        }
    })
}

fn caret_blink_visible() -> bool {
    caret_blink_phase() % 2 == 0
}

pub(crate) fn caret_blink_phase() -> u128 {
    static START: OnceLock<Instant> = OnceLock::new();
    let start = START.get_or_init(Instant::now);
    start.elapsed().as_millis() / 520
}

fn input_line_height(style: &crate::style::TextStyle, scaled_size: f32, ui_scale: f32) -> f32 {
    if let Some(lh) = style.line_height_px {
        lh * ui_scale
    } else {
        let scale = style
            .line_height_scale
            .unwrap_or_else(|| tailwind_default_leading_scale(style.size));
        scaled_size * scale
    }
}

fn clip_rect_to_bounds(rect: [f32; 4], clip: Option<[f32; 4]>) -> Option<[f32; 4]> {
    let Some(c) = clip else {
        return Some(rect);
    };
    let x0 = rect[0].max(c[0]);
    let y0 = rect[1].max(c[1]);
    let x1 = (rect[0] + rect[2]).min(c[0] + c[2]);
    let y1 = (rect[1] + rect[3]).min(c[1] + c[3]);
    let w = (x1 - x0).max(0.0);
    let h = (y1 - y0).max(0.0);
    if w <= 0.0 || h <= 0.0 {
        None
    } else {
        Some([x0, y0, w, h])
    }
}

fn text_bounds_from_clip(clip: Option<[f32; 4]>, config: &SurfaceConfiguration) -> glyphon::TextBounds {
    if let Some(b) = clip {
        glyphon::TextBounds {
            left: b[0].floor() as i32,
            top: b[1].floor() as i32,
            right: (b[0] + b[2]).ceil() as i32,
            bottom: (b[1] + b[3]).ceil() as i32,
        }
    } else {
        glyphon::TextBounds {
            left: 0,
            top: 0,
            right: config.width as i32,
            bottom: config.height as i32,
        }
    }
}

fn contains_emoji_grapheme(text: &str) -> bool {
    text.graphemes(true).any(|g| {
        g.chars().any(|c| {
            let cp = c as u32;
            (0x1F300..=0x1FAFF).contains(&cp)
                || (0x2600..=0x27BF).contains(&cp)
                || (0xFE00..=0xFE0F).contains(&cp)
        })
    })
}

/// 文本阴影层会把整块 buffer 平移；若仍用正文同款 `TextBounds`，glyphon 会按边界裁字，
/// 多圈模糊采样时会出现缺块/不对称。对阴影扩大裁剪并与视口求交。
fn shadow_clip_for_glyphon(
    node_clip: Option<[f32; 4]>,
    outset: f32,
    viewport: [f32; 4],
) -> Option<[f32; 4]> {
    let Some(c) = node_clip else {
        return None;
    };
    let expanded = [
        c[0] - outset,
        c[1] - outset,
        c[2] + 2.0 * outset,
        c[3] + 2.0 * outset,
    ];
    clip_rect_to_bounds(expanded, Some(viewport))
}

/// 排版文本、构建下划线 `InstanceInput`，并调用 glyphon `prepare`。
pub fn prepare_text_frame(
    text_renderer: &mut RuiTextRenderer,
    device: &Device,
    queue: &Queue,
    config: &SurfaceConfiguration,
    text_nodes: &[TextLayoutInfo],
    ui_scale: f32,
    force_fast_mode: bool,
) -> Vec<InstanceInput> {
    let perf_mode = if force_fast_mode {
        TextPerfMode::Fast
    } else {
        text_perf_mode()
    };
    if text_nodes.is_empty() {
        text_renderer.prepare(device, queue, config, std::iter::empty());
        return Vec::new();
    }

    let mut frame_meta: Vec<Option<FrameTextNodeMeta>> = Vec::with_capacity(text_nodes.len());
    let best = best_font_family(&text_renderer.font_system);
    let estimated = text_nodes.len().saturating_mul(3);
    let cap = text_renderer.text_area_capacity_hint.max(estimated);
    let mut text_areas = Vec::with_capacity(cap);
    let viewport_clip = [0.0, 0.0, config.width as f32, config.height as f32];

    if text_renderer.shaped_cache.len() > text_nodes.len() {
        text_renderer.shaped_cache.truncate(text_nodes.len());
    }

    for (i, info) in text_nodes.iter().enumerate() {
        let st = &info.style;
        let has_emoji = contains_emoji_grapheme(info.text.as_str())
            || info
                .shape_suffix
                .as_deref()
                .is_some_and(|s| !s.is_empty() && contains_emoji_grapheme(s));
        let family = if has_emoji {
            crate::utils::font::resolve_family(
                text_renderer.font_system.db(),
                "Noto Emoji, Noto Color Emoji, Noto Sans CJK SC, sans-serif",
            )
        } else if let Some(f) = &st.font_family {
            crate::utils::font::resolve_family(text_renderer.font_system.db(), f)
        } else {
            best
        };

        let is_icon = st.font_family.as_deref() == Some("lucide");
        let requested_weight = if is_icon {
            400
        } else {
            st.weight
        };
        let font_weight = crate::utils::font::normalize_weight_for_family(
            text_renderer.font_system.db(),
            family,
            requested_weight,
        );
        let scaled_size = st.size * ui_scale;
        let line_height = if is_icon {
            scaled_size
        } else if let Some(line_height_px) = st.line_height_px {
            line_height_px * ui_scale
        } else {
            let scale = st
                .line_height_scale
                .unwrap_or_else(|| tailwind_default_leading_scale(st.size));
            scaled_size * scale
        };

        let scaled_w = info.width * ui_scale;
        let scaled_h = info.height * ui_scale;
        let x = info.x * ui_scale;
        let y = info.y * ui_scale;
        let scaled_clip = info.clip_bounds.map(|b| {
            [
                b[0] * ui_scale,
                b[1] * ui_scale,
                b[2] * ui_scale,
                b[3] * ui_scale,
            ]
        });
        let node_rect = [x, y, scaled_w.max(0.0), scaled_h.max(0.0)];
        let Some(clipped) = clip_rect_to_bounds(node_rect, scaled_clip) else {
            frame_meta.push(None);
            continue;
        };
        if clip_rect_to_bounds(clipped, Some([0.0, 0.0, config.width as f32, config.height as f32])).is_none()
        {
            frame_meta.push(None);
            continue;
        }
        let is_ascii_basic = st.input_caret.is_none() && info.text.is_ascii();
        let shaping = match perf_mode {
            // Editing/caret keeps advanced shaping for cursor fidelity.
            TextPerfMode::HighQuality => {
                if force_fast_mode && st.input_caret.is_none() {
                    glyphon::Shaping::Basic
                } else if is_ascii_basic {
                    glyphon::Shaping::Basic
                } else {
                    glyphon::Shaping::Advanced
                }
            }
            // Balanced: prioritize frame stability while scrolling.
            TextPerfMode::Balanced | TextPerfMode::Fast => {
                if is_ascii_basic {
                    glyphon::Shaping::Basic
                } else {
                    glyphon::Shaping::Advanced
                }
            }
        };
        // Scroll/drag frame: only ASCII non-input text is downgraded to Basic.
        let shaping = if force_fast_mode && st.input_caret.is_none() && info.text.is_ascii() {
            glyphon::Shaping::Basic
        } else {
            shaping
        };
        let shape_text: Cow<'_, str> = match info.shape_suffix.as_deref() {
            Some(s) if !s.is_empty() => Cow::Owned(format!("{}{}", info.text, s)),
            _ => Cow::Borrowed(info.text.as_str()),
        };
        let key_hash = node_shape_hash(
            shape_text.as_ref(),
            scaled_size,
            line_height,
            scaled_w,
            scaled_h,
            font_weight,
            family,
            st.align,
            shaping,
        );

        while text_renderer.shaped_cache.len() <= i {
            text_renderer
                .shaped_cache
                .push(make_dummy_cache_entry(&mut text_renderer.font_system));
        }
        let entry = &mut text_renderer.shaped_cache[i];
        let cache_hit = entry.key_hash == key_hash;
        let measured_text_w = if cache_hit {
            entry.measured_text_w
        } else {
            let mut buffer = glyphon::Buffer::new(
                &mut text_renderer.font_system,
                glyphon::Metrics::new(scaled_size.max(1.0), line_height.max(1.0)),
            );
            buffer.set_size(
                &mut text_renderer.font_system,
                Some(scaled_w),
                Some(scaled_h + 2.0),
            );
            buffer.set_text(
                &mut text_renderer.font_system,
                shape_text.as_ref(),
                glyphon::Attrs::new()
                    .family(family)
                    .weight(glyphon::fontdb::Weight(font_weight)),
                shaping,
            );
            for line in buffer.lines.iter_mut() {
                line.set_align(Some(st.align));
            }
            buffer.shape_until_scroll(&mut text_renderer.font_system, false);
            let measured = buffer
                .layout_runs()
                .map(|run| run.line_w)
                .fold(0.0_f32, f32::max);
            entry.key_hash = key_hash;
            entry.buffer = buffer;
            entry.measured_text_w = measured;
            measured
        };

        if super::text_prepare_debug::enabled()
            && super::text_prepare_debug::should_log(
                info.template_id.as_deref(),
                st.input_caret.is_some(),
            )
            && (!super::text_prepare_debug::reshape_only() || !cache_hit)
        {
            let lh_log = line_height / ui_scale.max(0.001);
            super::text_prepare_debug::log_line(
                i,
                info,
                family,
                font_weight,
                lh_log,
                st.input_caret.is_some(),
                &entry.buffer,
                &text_renderer.font_system,
                cache_hit,
                shaping,
            );
        }
        super::text_prepare_debug::maybe_log_family_mismatch(
            info,
            family,
            font_weight,
            &entry.buffer,
            &text_renderer.font_system,
        );

        let dy = 0.0;
        let emoji_cache_key = if has_emoji || contains_emoji_grapheme(shape_text.as_ref()) {
            Some(super::text::EmojiCacheKey {
                sequence: shape_text.to_string(),
                pixel_size: scaled_size.round().clamp(1.0, u16::MAX as f32) as u16,
                scale_bits: ui_scale.to_bits(),
                color_mode: 1,
            })
        } else {
            None
        };
        if let Some(key) = emoji_cache_key.clone() {
            if let Entry::Vacant(v) = text_renderer.emoji_cache.entries.entry(key) {
                // MVP: reserve custom-glyph slot for emoji runs; later phase fills atlas textures.
                v.insert(Vec::new());
            }
        }
        frame_meta.push(Some(FrameTextNodeMeta {
            x,
            y,
            box_w: scaled_w,
            box_h: scaled_h,
            dy,
            text_size: scaled_size,
            align: st.align,
            measured_text_w,
            underline: st.underline,
            overline: st.overline,
            line_through: st.line_through,
            decoration_style: st.decoration_style,
            decoration_color: st.decoration_color,
            decoration_thickness: st.decoration_thickness,
            underline_offset: st.underline_offset,
            color: st.color,
            shadow_col: st.shadow_color,
            shadow_dx: st.shadow_offset[0] * ui_scale,
            shadow_dy: st.shadow_offset[1] * ui_scale,
            shadow_blur: st.shadow_blur * ui_scale,
            clip_bounds: scaled_clip,
            emoji_cache_key,
        }));
    }

    for (i, tb) in frame_meta.iter().enumerate() {
        let Some(tb) = tb.as_ref() else {
            continue;
        };
        let entry = &text_renderer.shaped_cache[i];
        if let Some(sc) = tb.shadow_col {
            let radius = tb.shadow_blur;
            let alpha = sc[3];
            if alpha == 0 {
                // skip
            } else {
                // 阴影质量勿随滚动 `force_fast_mode` 降级：与 shaping 分离，仅按 `RUI_TEXT_PERF`。
                let shadow_perf = text_perf_mode();
                if !matches!(shadow_perf, TextPerfMode::Fast) {
                    let (min_radius, min_alpha, base_samples, radial_scale, max_samples) = match shadow_perf {
                        TextPerfMode::HighQuality => (0.35_f32, 6_u8, 20_usize, 9.0_f32, 72_usize),
                        TextPerfMode::Balanced => (0.8_f32, 20_u8, 12_usize, 6.5_f32, 48_usize),
                        TextPerfMode::Fast => (0.0, 0, 0, 0.0, 0),
                    };
                    // 小于此模糊半径（已乘 ui_scale）视为无模糊：只画偏移后的单层阴影。
                    const SHARP_BLUR_EPS: f32 = 0.75;
                    let shadow_bounds_clip = shadow_clip_for_glyphon(
                        tb.clip_bounds,
                        radius + tb.shadow_dx.abs() + tb.shadow_dy.abs() + 3.0,
                        viewport_clip,
                    );
                    let bounds = text_bounds_from_clip(shadow_bounds_clip, config);

                    let mut push_shadow_layer = |left: f32, top: f32, layer_alpha: u8| {
                        if layer_alpha == 0 {
                            return;
                        }
                        let shadow_rect = [left, top, tb.box_w, tb.box_h];
                        let Some(shadow_visible) = clip_rect_to_bounds(shadow_rect, tb.clip_bounds) else {
                            return;
                        };
                        if clip_rect_to_bounds(shadow_visible, Some(viewport_clip)).is_none() {
                            return;
                        }
                        text_areas.push(glyphon::TextArea {
                            buffer: &entry.buffer,
                            left,
                            top,
                            scale: 1.0,
                            bounds,
                            default_color: glyphon::Color::rgba(sc[0], sc[1], sc[2], layer_alpha),
                            custom_glyphs: tb
                                .emoji_cache_key
                                .as_ref()
                                .and_then(|k| text_renderer.emoji_cache.entries.get(k))
                                .map_or(&[], |v| v.as_slice()),
                        });
                    };

                    // 高密度圆盘采样（Vogel spiral）：比固定少点核更接近连续模糊，减少“多重字影”。
                    let center_left = tb.x + tb.shadow_dx;
                    let center_top = tb.y + tb.dy + tb.shadow_dy;
                    let draw_blur = radius > SHARP_BLUR_EPS && radius > min_radius && alpha > min_alpha;
                    if !draw_blur {
                        push_shadow_layer(center_left, center_top, alpha);
                    } else {
                        const GOLDEN_ANGLE: f32 = 2.399_963_1;
                        let sample_count = (base_samples as f32 + radius * radial_scale)
                            .round()
                            .clamp(base_samples as f32, max_samples as f32)
                            as usize;
                        let sigma = (radius * 0.58).max(0.75);
                        let two_sigma2 = 2.0 * sigma * sigma;

                        let mut weight_sum = 1.0_f32; // center sample
                        for idx in 0..sample_count {
                            let t = (idx as f32 + 0.5) / (sample_count as f32);
                            let r = radius * t.sqrt();
                            let w = (-(r * r) / two_sigma2).exp();
                            weight_sum += w;
                        }
                        let inv = 1.0 / weight_sum.max(0.001);
                        let center_alpha = ((alpha as f32) * inv).round().clamp(0.0, 255.0) as u8;
                        push_shadow_layer(center_left, center_top, center_alpha);

                        for idx in 0..sample_count {
                            let t = (idx as f32 + 0.5) / (sample_count as f32);
                            let r = radius * t.sqrt();
                            let theta = idx as f32 * GOLDEN_ANGLE;
                            let rx = theta.cos() * r;
                            let ry = theta.sin() * r;
                            let w = (-(r * r) / two_sigma2).exp();
                            let wa = ((alpha as f32) * w * inv).round().clamp(0.0, 255.0) as u8;
                            push_shadow_layer(center_left + rx, center_top + ry, wa);
                        }
                    }
                }
            }
        }

        let bounds = text_bounds_from_clip(tb.clip_bounds, config);
        text_areas.push(glyphon::TextArea {
            buffer: &entry.buffer,
            left: tb.x,
            top: tb.y,
            scale: 1.0,
            bounds,
            default_color: glyphon::Color::rgba(tb.color[0], tb.color[1], tb.color[2], tb.color[3]),
            custom_glyphs: tb
                .emoji_cache_key
                .as_ref()
                .and_then(|k| text_renderer.emoji_cache.entries.get(k))
                .map_or(&[], |v| v.as_slice()),
        });
    }

    let mut underline_instances: Vec<InstanceInput> = Vec::new();
    for maybe_tb in &frame_meta {
        let Some(tb) = maybe_tb.as_ref() else {
            continue;
        };
        if tb.underline || tb.overline || tb.line_through {
            let ul_h = if tb.decoration_thickness > 0.0 {
                tb.decoration_thickness
            } else {
                (tb.text_size * 0.07).max(1.0)
            };
            let ul_w = tb.measured_text_w.min(tb.box_w);
            let ul_x = match tb.align {
                glyphon::cosmic_text::Align::Center => tb.x + (tb.box_w - ul_w) * 0.5,
                glyphon::cosmic_text::Align::Right => tb.x + (tb.box_w - ul_w),
                _ => tb.x,
            };
            let dc = tb.decoration_color.unwrap_or(tb.color);
            let line_color = [
                dc[0] as f32 / 255.0,
                dc[1] as f32 / 255.0,
                dc[2] as f32 / 255.0,
                dc[3] as f32 / 255.0,
            ];
            let mut push_line = |x: f32, y: f32, w: f32, h: f32| {
                if w <= 0.0 || h <= 0.0 {
                    return;
                }
                let Some([cx, cy, cw, ch]) = clip_rect_to_bounds([x, y, w, h], tb.clip_bounds) else {
                    return;
                };
                underline_instances.push(InstanceInput {
                    rect_pos: [cx, cy],
                    rect_size: [cw, ch],
                    color: line_color,
                    border_color: [0.0, 0.0, 0.0, 0.0],
                    radii: [0.0, 0.0, 0.0, 0.0],
                    border_width: 0.0,
                    shadow_color: [0.0, 0.0, 0.0, 0.0],
                    shadow_offset: [0.0, 0.0],
                    shadow_blur: 0.0,
                    shadow_spread: 0.0,
                    shadow_inset: 0.0,
                    _padding: [0.0; 2],
                });
            };
            let mut draw_at = |line_y: f32| match tb.decoration_style {
                TextDecorationStyle::Solid => {
                    push_line(ul_x, line_y, ul_w, ul_h);
                }
                TextDecorationStyle::Double => {
                    let gap = (ul_h * 0.8).max(1.0);
                    push_line(ul_x, line_y - gap, ul_w, ul_h);
                    push_line(ul_x, line_y + gap, ul_w, ul_h);
                }
                TextDecorationStyle::Dotted => {
                    // 明显留出间隔，避免细线 + 密集采样看起来像 solid
                    let dot_w = ((ul_h * 1.35).max(2.0)).min(ul_w.max(1.0));
                    let gap = (ul_h * 1.1).max(2.0);
                    let step = dot_w + gap;
                    let mut cx = ul_x;
                    while cx < ul_x + ul_w {
                        let w = dot_w.min(ul_x + ul_w - cx);
                        if w > 0.0 {
                            push_line(cx, line_y, w, ul_h);
                        }
                        cx += step;
                    }
                }
                TextDecorationStyle::Dashed => {
                    let dash = (ul_h * 4.0).max(4.0);
                    let gap = (ul_h * 2.0).max(2.0);
                    let mut cx = ul_x;
                    while cx < ul_x + ul_w {
                        push_line(cx, line_y, dash.min(ul_x + ul_w - cx), ul_h);
                        cx += dash + gap;
                    }
                }
                TextDecorationStyle::Wavy => {
                    // 沿正弦路径采样短横条，避免“上下交替”被看成两条虚线。
                    let wavelength = (ul_h * 6.0).max(8.0);
                    let amp = (ul_h * 0.75).max(1.0);
                    let step = (ul_h * 0.65).max(1.0);
                    let mut cx = ul_x;
                    while cx < ul_x + ul_w {
                        let t = ((cx - ul_x) / wavelength) * std::f32::consts::TAU;
                        let y = line_y + amp * t.sin();
                        let w = step.min(ul_x + ul_w - cx);
                        push_line(cx, y, w, ul_h);
                        cx += step;
                    }
                }
            };
            if tb.underline {
                // Keep underline visually closer to the text bottom edge.
                let y = tb.y + tb.box_h - (ul_h * 0.45) - (tb.underline_offset * ui_scale);
                draw_at(y);
            }
            if tb.overline {
                let y = tb.y + (tb.underline_offset * ui_scale);
                draw_at(y);
            }
            if tb.line_through {
                let y = tb.y + (tb.box_h - ul_h) * 0.5;
                draw_at(y);
            }
        }
    }

    // 文本选区高亮（`::selection` / `selection:bg-primary`；实例 pass 先于 glyphon 文本，故在字下）
    // 改写模式下闪烁高亮下一字符（`overwrite_highlight`）与选区共用同一套几何。
    for (i, info) in text_nodes.iter().enumerate() {
        let Some(caret) = info.style.input_caret.as_ref() else {
            continue;
        };
        let display = info.text.as_str();
        let range_opt: Option<(usize, usize)> = if let Some((sa, sb)) = caret.selection {
            let mut s = sa.min(sb).min(display.len());
            let mut e = sa.max(sb).min(display.len());
            s = display.floor_char_boundary(s);
            e = display.floor_char_boundary(e);
            if s < e {
                Some((s, e))
            } else {
                None
            }
        } else if let Some((a, b)) = caret.overwrite_highlight {
            if !caret_blink_visible() {
                None
            } else {
                let mut s = a.min(b).min(display.len());
                let mut e = a.max(b).min(display.len());
                s = display.floor_char_boundary(s);
                e = display.floor_char_boundary(e);
                if s < e {
                    Some((s, e))
                } else {
                    None
                }
            }
        } else {
            None
        };
        let Some((s, e)) = range_opt else {
            continue;
        };
        let Some(tb) = frame_meta.get(i).and_then(|x| x.as_ref()) else {
            continue;
        };
        let x = tb.x;
        let y = tb.y;
        let box_w = tb.box_w;
        let box_h = tb.box_h;
        let scaled_size = tb.text_size;
        let align = tb.align;
        let clip_bounds = tb.clip_bounds;
        let st = &info.style;

        let family = if let Some(f) = &st.font_family {
            crate::utils::font::resolve_family(text_renderer.font_system.db(), f)
        } else {
            best
        };
        let is_icon = st.font_family.as_deref() == Some("lucide");
        let requested_weight = if is_icon { 400 } else { st.weight };
        let font_weight = crate::utils::font::normalize_weight_for_family(
            text_renderer.font_system.db(),
            family,
            requested_weight,
        );
        let line_height = input_line_height(st, scaled_size, ui_scale);

        let prefix_to_sel = measure_prefix_width(
            &mut text_renderer.font_system,
            &display[..s],
            scaled_size,
            line_height,
            font_weight,
            family,
            box_w,
        );
        let sel_w = measure_prefix_width(
            &mut text_renderer.font_system,
            &display[s..e],
            scaled_size,
            line_height,
            font_weight,
            family,
            box_w,
        );
        let text_w = tb.measured_text_w;
        let content_w = text_w.min(box_w);
        let text_left = match align {
            glyphon::cosmic_text::Align::Center => x + (box_w - content_w) * 0.5,
            glyphon::cosmic_text::Align::Right => x + (box_w - content_w),
            _ => x,
        };
        let sel_x0 = text_left + prefix_to_sel.min(content_w);
        let max_sel = (text_left + content_w - sel_x0).max(0.0);
        let sel_w = sel_w.min(max_sel);

        // 选区高度与文本布局使用同一 line-height，避免中文/下行字母时视觉不匹配。
        let caret_h = line_height.min(box_h).max(1.0);
        let sel_y = y + (box_h - caret_h) * 0.5;

        let bg = caret.selection_bg;
        let line_color = [
            bg[0] as f32 / 255.0,
            bg[1] as f32 / 255.0,
            bg[2] as f32 / 255.0,
            bg[3] as f32 / 255.0,
        ];
        if sel_w > 0.0 && caret_h > 0.0 {
            if let Some([cx, cy, cw, ch]) =
                clip_rect_to_bounds([sel_x0, sel_y, sel_w, caret_h], clip_bounds)
            {
                underline_instances.push(InstanceInput {
                    rect_pos: [cx, cy],
                    rect_size: [cw, ch],
                    color: line_color,
                    border_color: [0.0, 0.0, 0.0, 0.0],
                    radii: [0.0, 0.0, 0.0, 0.0],
                    border_width: 0.0,
                    shadow_color: [0.0, 0.0, 0.0, 0.0],
                    shadow_offset: [0.0, 0.0],
                    shadow_blur: 0.0,
                    shadow_spread: 0.0,
                    shadow_inset: 0.0,
                    _padding: [0.0; 2],
                });
            }
        }
    }

    // 文本输入插入符（与下划线共用实例管线）
    for (i, info) in text_nodes.iter().enumerate() {
        let Some(caret) = info.style.input_caret.as_ref() else {
            continue;
        };
        let display = info.text.as_str();
        if let Some((sa, sb)) = caret.selection {
            let mut s = sa.min(sb).min(display.len());
            let mut e = sa.max(sb).min(display.len());
            s = display.floor_char_boundary(s);
            e = display.floor_char_boundary(e);
            if s < e {
                continue;
            }
        }
        if let Some((a, b)) = caret.overwrite_highlight {
            let s = a.min(b).min(display.len());
            let e = a.max(b).min(display.len());
            let s = display.floor_char_boundary(s);
            let e = display.floor_char_boundary(e);
            if s < e {
                continue;
            }
        }
        if !caret_blink_visible() {
            continue;
        }
        let Some(tb) = frame_meta.get(i).and_then(|x| x.as_ref()) else {
            continue;
        };
        let x = tb.x;
        let y = tb.y;
        let box_w = tb.box_w;
        let box_h = tb.box_h;
        let scaled_size = tb.text_size;
        let align = tb.align;
        let clip_bounds = tb.clip_bounds;
        let st = &info.style;

        let mut ix = caret.caret_byte_index.min(display.len());
        ix = display.floor_char_boundary(ix);
        let prefix = &display[..ix];

        let family = if let Some(f) = &st.font_family {
            crate::utils::font::resolve_family(text_renderer.font_system.db(), f)
        } else {
            best
        };
        let is_icon = st.font_family.as_deref() == Some("lucide");
        let requested_weight = if is_icon { 400 } else { st.weight };
        let font_weight = crate::utils::font::normalize_weight_for_family(
            text_renderer.font_system.db(),
            family,
            requested_weight,
        );
        let line_height = input_line_height(st, scaled_size, ui_scale);

        let prefix_w = measure_prefix_width(
            &mut text_renderer.font_system,
            prefix,
            scaled_size,
            line_height,
            font_weight,
            family,
            box_w,
        );

        let text_w = tb.measured_text_w;
        let content_w = text_w.min(box_w);
        let text_left = match align {
            glyphon::cosmic_text::Align::Center => x + (box_w - content_w) * 0.5,
            glyphon::cosmic_text::Align::Right => x + (box_w - content_w),
            _ => x,
        };
        let caret_w = (1.0_f32 * ui_scale).max(1.0);
        let mut caret_x = text_left + prefix_w.min(content_w);
        caret_x = caret_x.min(x + box_w - caret_w);

        // 光标高度与选区一致：直接跟随 line-height。
        let caret_h = line_height.min(box_h).max(1.0);
        let caret_y = y + (box_h - caret_h) * 0.5;

        let c = st.color;
        let line_color = [
            c[0] as f32 / 255.0,
            c[1] as f32 / 255.0,
            c[2] as f32 / 255.0,
            c[3] as f32 / 255.0,
        ];
        if let Some([cx, cy, cw, ch]) =
            clip_rect_to_bounds([caret_x, caret_y, caret_w, caret_h], clip_bounds)
        {
            underline_instances.push(InstanceInput {
                rect_pos: [cx, cy],
                rect_size: [cw, ch],
                color: line_color,
                border_color: [0.0, 0.0, 0.0, 0.0],
                radii: [0.0, 0.0, 0.0, 0.0],
                border_width: 0.0,
                shadow_color: [0.0, 0.0, 0.0, 0.0],
                shadow_offset: [0.0, 0.0],
                shadow_blur: 0.0,
                shadow_spread: 0.0,
                shadow_inset: 0.0,
                _padding: [0.0; 2],
            });
        }
    }

    text_renderer.viewport.update(
        queue,
        glyphon::Resolution {
            width: config.width,
            height: config.height,
        },
    );
    let text_area_count = text_areas.len();
    text_renderer
        .text_renderer
        .prepare(
            device,
            queue,
            &mut text_renderer.font_system,
            &mut text_renderer.text_atlas,
            &text_renderer.viewport,
            text_areas.into_iter(),
            &mut text_renderer.swash_cache,
        )
        .unwrap();
    text_renderer.text_area_capacity_hint = text_area_count;

    underline_instances
}
