//! 与 [`super::prepare_text`] 一致的 glyphon 文本测量，供输入命中测试与拖选使用。

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use crate::style::TextStyle;

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
    let key = measure_key(text, scaled_size, line_height, weight, family, max_width);
    if let Some(hit) = MEASURE_CACHE.with(|cache| cache.borrow().get(&key).copied()) {
        return hit;
    }
    let mut buffer = glyphon::Buffer::new(
        font_system,
        glyphon::Metrics::new(scaled_size, line_height),
    );
    buffer.set_size(
        font_system,
        Some(max_width),
        Some(line_height + 2.0),
    );
    buffer.set_text(
        font_system,
        text,
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

pub fn best_font_family(font_system: &glyphon::FontSystem) -> glyphon::Family<'static> {
    let db = font_system.db();
    let preferred = ["Inter", "Segoe UI", "Roboto", "Helvetica Neue", "Arial"];
    for &name in &preferred {
        if db
            .query(&glyphon::fontdb::Query {
                families: &[glyphon::fontdb::Family::Name(name)],
                weight: glyphon::fontdb::Weight::NORMAL,
                stretch: glyphon::fontdb::Stretch::Normal,
                style: glyphon::fontdb::Style::Normal,
            })
            .is_some()
        {
            return glyphon::Family::Name(name);
        }
    }
    glyphon::Family::SansSerif
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
    let _my = my.clamp(y, y + h.max(0.0));
    let scaled_size = style.size * ui_scale;
    let line_height = line_height_for_style(style, ui_scale);
    let family_owned = style.font_family.clone();
    let family = if let Some(ref f) = family_owned {
        glyphon::Family::Name(f.as_str())
    } else {
        best_font_family(font_system)
    };
    let box_w = w * ui_scale;
    let rel_in_box = (mx - x) * ui_scale;
    let measured = measure_prefix_width(
        font_system,
        display,
        scaled_size,
        line_height,
        style.weight,
        family,
        box_w,
    );
    let content_w = measured.min(box_w);
    let offset = match style.align {
        glyphon::cosmic_text::Align::Center => (box_w - content_w) * 0.5,
        glyphon::cosmic_text::Align::Right => box_w - content_w,
        _ => 0.0,
    };
    let rel_x = (rel_in_box - offset).clamp(0.0, content_w);
    hit_test_line_x(
        font_system,
        display,
        rel_x,
        scaled_size,
        line_height,
        style.weight,
        family,
        box_w,
    )
}

fn line_height_for_style(style: &TextStyle, ui_scale: f32) -> f32 {
    if let Some(lh) = style.line_height_px {
        lh * ui_scale
    } else {
        scaled_size_for_style(style, ui_scale) * style.line_height_scale.unwrap_or(1.2)
    }
}

fn scaled_size_for_style(style: &TextStyle, ui_scale: f32) -> f32 {
    style.size * ui_scale
}
