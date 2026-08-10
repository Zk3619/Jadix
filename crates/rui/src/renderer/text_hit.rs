//! 与 [`super::prepare_text`] 一致的 glyphon 文本测量，供输入命中测试与拖选使用。

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use crate::style::{tailwind_default_leading_scale, TextStyle};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct MeasureKey {
    text_hash: u64,
    scaled_size_q: u16,
    line_height_q: u16,
    weight: u16,
    max_width_q: u16,
    family_hash: u64,
}

thread_local! {
    static MEASURE_CACHE: RefCell<HashMap<MeasureKey, f32>> = RefCell::new(HashMap::new());
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

fn measure_key(
    text: &str,
    scaled_size: f32,
    line_height: f32,
    weight: u16,
    family: glyphon::Family,
    max_width: f32,
) -> MeasureKey {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    MeasureKey {
        text_hash: h.finish(),
        scaled_size_q: (scaled_size * 10.0).clamp(0.0, u16::MAX as f32) as u16,
        line_height_q: (line_height * 10.0).clamp(0.0, u16::MAX as f32) as u16,
        weight,
        max_width_q: (max_width * 10.0).clamp(0.0, u16::MAX as f32) as u16,
        family_hash: family_hash(family),
    }
}

/// 测量单行前缀在指定盒宽内的逻辑宽度（与主文本同一套 Metrics / Attrs）。
pub fn measure_prefix_width(
    font_system: &mut glyphon::FontSystem,
    text: &str,
    scaled_size: f32,
    line_height: f32,
    weight: u16,
    family: glyphon::Family,
    max_width: f32,
) -> f32 {
    if text.is_empty() {
        return 0.0;
    }
    crate::renderer::font_loader::ensure_coverage_for_text(font_system, text);
    let key = measure_key(text, scaled_size, line_height, weight, family, max_width);
    if let Some(hit) = MEASURE_CACHE.with(|cache| cache.borrow().get(&key).copied()) {
        return hit;
    }
    let layout = crate::renderer::emoji_layout::shape_text_for_layout(text);
    let mut buffer =
        glyphon::Buffer::new(font_system, glyphon::Metrics::new(scaled_size, line_height));
    buffer.set_size(font_system, Some(max_width), Some(line_height + 2.0));
    buffer.set_text(
        font_system,
        layout.as_ref(),
        glyphon::Attrs::new()
            .family(family)
            .weight(glyphon::fontdb::Weight(weight)),
        glyphon::Shaping::Advanced,
    );
    buffer.shape_until_scroll(font_system, false);
    let measured = buffer
        .layout_runs()
        .map(|run| run.line_w)
        .fold(0.0_f32, f32::max);
    MEASURE_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() > 4096 {
            cache.clear();
        }
        cache.insert(key, measured);
    });
    measured
}

const ELLIPSIS: &str = "…";

/// True when the layout box is tall enough for at most one line of text.
pub fn is_single_line_text_box(line_height: f32, box_height: f32) -> bool {
    line_height > 0.0 && box_height <= line_height * 1.25
}

/// Truncate `text` with a trailing ellipsis so shaped width fits within `max_width`.
pub fn truncate_with_ellipsis(
    font_system: &mut glyphon::FontSystem,
    text: &str,
    scaled_size: f32,
    line_height: f32,
    weight: u16,
    family: glyphon::Family,
    max_width: f32,
) -> String {
    if text.is_empty() || max_width <= 0.0 {
        return text.to_string();
    }
    let full_w = measure_prefix_width(
        font_system,
        text,
        scaled_size,
        line_height,
        weight,
        family,
        max_width,
    );
    if full_w <= max_width {
        return text.to_string();
    }
    let ellipsis_w = measure_prefix_width(
        font_system,
        ELLIPSIS,
        scaled_size,
        line_height,
        weight,
        family,
        max_width,
    );
    if ellipsis_w >= max_width {
        return ELLIPSIS.to_string();
    }
    let budget = max_width - ellipsis_w;
    let mut lo = 0usize;
    let mut hi = text.len();
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        let end = text.floor_char_boundary(mid);
        let prefix = &text[..end];
        let w = measure_prefix_width(
            font_system,
            prefix,
            scaled_size,
            line_height,
            weight,
            family,
            max_width,
        );
        if w <= budget {
            lo = end;
        } else if end == 0 {
            break;
        } else {
            hi = text.floor_char_boundary(end - 1);
        }
    }
    format!("{}{}", &text[..lo], ELLIPSIS)
}

pub fn best_font_family(font_system: &glyphon::FontSystem) -> glyphon::Family<'static> {
    let db = font_system.db();
    let preferred = "Noto Sans CJK, Microsoft YaHei UI, Microsoft YaHei, Malgun Gothic, PingFang SC, Leelawadee UI, Segoe UI, sans-serif";
    crate::utils::font::resolve_family(db, preferred)
}

/// 在水平方向命中：返回 UTF-8 字节下标（落在某字符簇边界，与光标语义一致）。
pub fn hit_test_line_x(
    font_system: &mut glyphon::FontSystem,
    text: &str,
    rel_x: f32,
    scaled_size: f32,
    line_height: f32,
    weight: u16,
    family: glyphon::Family,
    max_width: f32,
) -> usize {
    if text.is_empty() {
        return 0;
    }
    let rel_x = rel_x.max(0.0);
    let mut best = 0usize;
    for ix in text
        .char_indices()
        .map(|(i, _)| i)
        .chain(std::iter::once(text.len()))
    {
        let w = measure_prefix_width(
            font_system,
            &text[..ix],
            scaled_size,
            line_height,
            weight,
            family,
            max_width,
        );
        if w <= rel_x {
            best = ix;
        } else {
            break;
        }
    }
    let next = if best < text.len() {
        text[best..]
            .chars()
            .next()
            .map(|c| best + c.len_utf8())
            .unwrap_or(text.len())
    } else {
        text.len()
    };
    let w_best = measure_prefix_width(
        font_system,
        &text[..best],
        scaled_size,
        line_height,
        weight,
        family,
        max_width,
    );
    let w_next = measure_prefix_width(
        font_system,
        &text[..next],
        scaled_size,
        line_height,
        weight,
        family,
        max_width,
    );
    if next > best && rel_x > (w_best + w_next) * 0.5 {
        next
    } else {
        best
    }
}

/// Hard-`\n` line bodies with global byte starts (includes trailing empty line).
fn hard_line_slices(text: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut start = 0usize;
    for (i, ch) in text.char_indices() {
        if ch == '\n' {
            out.push((start, &text[start..i]));
            start = i + 1;
        }
    }
    out.push((start, &text[start..]));
    out
}

/// `bounds` 为逻辑像素 `[x,y,w,h]`，与布局树一致；`mx`/`my` 为逻辑坐标。
pub fn hit_test_input_text_index(
    font_system: &mut glyphon::FontSystem,
    display: &str,
    mx: f32,
    my: f32,
    bounds: [f32; 4],
    style: &TextStyle,
    ui_scale: f32,
) -> usize {
    let x = bounds[0];
    let y = bounds[1];
    let w = bounds[2];
    let h = bounds[3];
    let mx = mx.clamp(x, x + w.max(0.0));
    let my = my.clamp(y, y + h.max(0.0));
    let scaled_size = style.size * ui_scale;
    let line_height = line_height_for_style(style, ui_scale);
    let line_height_logical = line_height_for_style(style, 1.0).max(1.0);
    let family = if let Some(ref f) = style.font_family {
        crate::utils::font::resolve_family(font_system.db(), f)
    } else {
        best_font_family(font_system)
    };
    let is_icon = style.font_family.as_deref() == Some("lucide");
    let requested_weight = if is_icon { 400 } else { style.weight };
    let font_weight =
        crate::utils::font::normalize_weight_for_family(font_system.db(), family, requested_weight);
    let box_w = w * ui_scale;
    let lines = hard_line_slices(display);
    let line_i = if lines.len() > 1 {
        ((my - y) / line_height_logical)
            .floor()
            .clamp(0.0, (lines.len() - 1) as f32) as usize
    } else {
        0
    };
    let (line_start, line_body) = lines[line_i];
    let rel_in_box = (mx - x) * ui_scale;
    let measured = measure_prefix_width(
        font_system,
        line_body,
        scaled_size,
        line_height,
        font_weight,
        family,
        box_w,
    );
    let content_w = measured.min(box_w);
    let offset = match style.align {
        glyphon::cosmic_text::Align::Center => (box_w - content_w) * 0.5,
        glyphon::cosmic_text::Align::Right => box_w - content_w,
        _ => 0.0,
    };
    let rel_x = (rel_in_box - offset).clamp(0.0, content_w.max(0.0));
    let local = hit_test_line_x(
        font_system,
        line_body,
        rel_x,
        scaled_size,
        line_height,
        font_weight,
        family,
        box_w,
    );
    line_start + local
}

fn line_height_for_style(style: &TextStyle, ui_scale: f32) -> f32 {
    if let Some(lh) = style.line_height_px {
        lh * ui_scale
    } else {
        let scale = style
            .line_height_scale
            .unwrap_or_else(|| tailwind_default_leading_scale(style.size));
        scaled_size_for_style(style, ui_scale) * scale
    }
}

fn scaled_size_for_style(style: &TextStyle, ui_scale: f32) -> f32 {
    style.size * ui_scale
}
