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

/// Wide enough that glyphon will not wrap; used for nowrap / ellipsis intrinsic width.
const UNBOUNDED_WRAP: f32 = 1.0e6;

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
    // `Some(box_w)` soft-wraps: CJK/latin then reports `max(line_w) ≈ box_w`, so ellipsis
    // thought the string already fit. Unbounded width keeps a true single-line advance.
    let width_opt = if max_width >= UNBOUNDED_WRAP * 0.5 {
        None
    } else {
        Some(max_width.max(1.0))
    };
    buffer.set_size(width_opt, Some(line_height + 2.0));
    let attrs = glyphon::Attrs::new()
        .family(family)
        .weight(glyphon::fontdb::Weight(weight));
    crate::renderer::emoji_layout::set_buffer_with_emoji_slots(
        &mut buffer,
        layout.as_ref(),
        &attrs,
        glyphon::Shaping::Advanced,
        scaled_size,
        line_height,
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
        UNBOUNDED_WRAP,
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
        UNBOUNDED_WRAP,
    );
    if ellipsis_w >= max_width {
        return ELLIPSIS.to_string();
    }
    let budget = max_width - ellipsis_w;
    // Binary-search *character* ends. Byte midpoints can land inside a UTF-8 scalar
    // (`floor_char_boundary` snaps back to `lo`) so `lo` never advances — CJK freeze.
    let mut ends: Vec<usize> = Vec::new();
    let mut i = 0usize;
    while i < text.len() {
        let ch = text[i..].chars().next().unwrap();
        i += ch.len_utf8();
        ends.push(i);
    }
    let mut lo_i = 0usize;
    let mut hi_i = ends.len();
    while lo_i < hi_i {
        let mid_i = (lo_i + hi_i).div_ceil(2);
        let end = ends[mid_i - 1];
        let w = measure_prefix_width(
            font_system,
            &text[..end],
            scaled_size,
            line_height,
            weight,
            family,
            UNBOUNDED_WRAP,
        );
        if w <= budget {
            lo_i = mid_i;
        } else {
            hi_i = mid_i.saturating_sub(1);
        }
    }
    let lo = if lo_i == 0 { 0 } else { ends[lo_i - 1] };
    format!("{}{}", &text[..lo], ELLIPSIS)
}

pub fn best_font_family(font_system: &glyphon::FontSystem) -> glyphon::Family<'static> {
    let db = font_system.db();
    let name = crate::utils::font::resolved_default_sans_family(db);
    if db
        .faces()
        .any(|face| face.families.iter().any(|(f, _)| f == name))
    {
        glyphon::Family::Name(name)
    } else {
        crate::utils::font::resolve_family(db, crate::utils::font::default_ui_sans_family_list())
    }
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
    let mx = if style.overflow.allows_caret_scroll()
        || style.white_space == crate::style::WhiteSpace::Nowrap
    {
        mx
    } else {
        mx.clamp(x, x + w.max(0.0))
    };
    let my = my.clamp(y, y + h.max(0.0));
    let scaled_size = style.size_resolved() * ui_scale;
    let line_height = line_height_for_style(style, ui_scale);
    let line_height_logical = line_height_for_style(style, 1.0).max(1.0);
    let family = if let Some(ref f) = style.font_family {
        crate::utils::font::resolve_family(font_system.db(), f)
    } else {
        best_font_family(font_system)
    };
    let is_icon = style.font_family.as_deref() == Some("lucide");
    let requested_weight = if is_icon {
        400
    } else {
        style.weight_resolved()
    };
    let font_weight =
        crate::utils::font::normalize_weight_for_family(font_system.db(), family, requested_weight);
    let box_w = w * ui_scale;
    let soft_wrap = style.white_space != crate::style::WhiteSpace::Nowrap
        && !style.overflow.allows_caret_scroll();
    let (line_start, line_body, wrap_limit) = if soft_wrap {
        let avail = (w.max(1.0)).max(1.0);
        let ranges = crate::text::soft_visual_line_byte_ranges(
            display,
            style.size_resolved(),
            style.weight_resolved(),
            line_height_for_style(style, 1.0).max(1.0),
            avail,
            style.wrap_mode.to_line_break_source(),
        );
        let line_i = ((my - y) / line_height_logical)
            .floor()
            .clamp(0.0, (ranges.len().saturating_sub(1)) as f32) as usize;
        let (start, end) = ranges[line_i];
        let body = &display[start..end];
        (start, body, box_w)
    } else {
        let lines = hard_line_slices(display);
        let line_i = if lines.len() > 1 {
            ((my - y) / line_height_logical)
                .floor()
                .clamp(0.0, (lines.len() - 1) as f32) as usize
        } else {
            0
        };
        let (line_start, line_body) = lines[line_i];
        let wrap_limit = if style.overflow.allows_caret_scroll() {
            UNBOUNDED_WRAP
        } else {
            box_w
        };
        (line_start, line_body, wrap_limit)
    };
    // Padding / chrome just outside the leaf: native `<input>` jumps to that
    // line's start or end (and lets caret-scroll reveal it). Mapping through
    // `scroll_x` would land on a mid-glyph at the clip edge instead.
    let pan_or_nowrap = style.overflow.allows_caret_scroll()
        || style.white_space == crate::style::WhiteSpace::Nowrap;
    if pan_or_nowrap {
        if mx < x {
            return line_start;
        }
        if mx > x + w.max(0.0) {
            return line_start + line_body.len();
        }
    }
    let rel_in_box = (mx - x + style.scroll_x.max(0.0)) * ui_scale;
    let measured = measure_prefix_width(
        font_system,
        line_body,
        scaled_size,
        line_height,
        font_weight,
        family,
        wrap_limit,
    );
    let content_w = if pan_or_nowrap {
        measured
    } else {
        measured.min(box_w)
    };
    let offset = style.align_inline_offset(content_w, box_w);
    let rel_x = (rel_in_box - offset).clamp(0.0, content_w.max(0.0));
    let local = hit_test_line_x(
        font_system,
        line_body,
        rel_x,
        scaled_size,
        line_height,
        font_weight,
        family,
        wrap_limit,
    );
    line_start + local
}

/// True when `(mx, my)` sits below the last hard/visual line of a textarea leaf.
/// Native behavior: click empty space under the last line → caret at document end.
pub fn hit_test_textarea_below_last_line(
    display: &str,
    my: f32,
    bounds: [f32; 4],
    style: &TextStyle,
) -> bool {
    let y = bounds[1];
    let lh = line_height_for_style(style, 1.0).max(1.0);
    let line_count = textarea_line_count(display, bounds, style);
    let content_bottom = y + line_count as f32 * lh;
    // Strictly below the last line box — clicks on the last line keep precise hit-test.
    my > content_bottom + 1.0
}

/// Textarea press: line-start / line-end padding → that line; other empty chrome → doc end.
pub fn hit_test_textarea_click_caret_index(
    font_system: &mut glyphon::FontSystem,
    display: &str,
    pointer_x: f32,
    pointer_y: f32,
    leaf_bounds: [f32; 4],
    style: &TextStyle,
    ui_scale: f32,
    hit_mx: f32,
    hit_my: f32,
) -> usize {
    let y = leaf_bounds[1];
    let lh = line_height_for_style(style, 1.0).max(1.0);
    let content_bottom = y + textarea_line_count(display, leaf_bounds, style) as f32 * lh;
    // Use the unclamped pointer Y. Soft-wrap pointer space clamps `hit_my` onto the
    // leaf, which would otherwise map empty chrome onto the last line and start a drag.
    if pointer_y > content_bottom + 1.0 {
        return display.len();
    }
    let on_a_line_band = pointer_y >= y - 1.0 && pointer_y <= content_bottom + 1.0;
    if on_a_line_band
        && (pointer_x < leaf_bounds[0]
            || pointer_x > leaf_bounds[0] + leaf_bounds[2].max(0.0)
            || hit_test_input_is_line_side_blank(
                font_system,
                display,
                pointer_x,
                pointer_y,
                leaf_bounds,
                style,
                ui_scale,
            ))
    {
        return hit_test_input_text_index(
            font_system,
            display,
            pointer_x,
            pointer_y,
            leaf_bounds,
            style,
            ui_scale,
        );
    }
    hit_test_input_text_index(
        font_system,
        display,
        hit_mx,
        hit_my,
        leaf_bounds,
        style,
        ui_scale,
    )
}

fn textarea_line_count(display: &str, bounds: [f32; 4], style: &TextStyle) -> usize {
    use crate::style::WhiteSpace;
    let soft_wrap =
        style.white_space != WhiteSpace::Nowrap && !style.overflow.allows_caret_scroll();
    if soft_wrap {
        crate::text::soft_visual_line_byte_ranges(
            display,
            style.size_resolved(),
            style.weight_resolved(),
            line_height_for_style(style, 1.0).max(1.0),
            bounds[2].max(1.0),
            style.wrap_mode.to_line_break_source(),
        )
        .len()
        .max(1)
    } else {
        display.split('\n').count().max(1)
    }
}

/// True when `(mx, my)` is in the horizontal blank beside a hard line's ink
/// (left/right of glyphs, including padding outside the text leaf).
///
/// Used for double-click → select whole line. Empty line bodies are never blank
/// hits (there is no non-empty range to select).
pub fn hit_test_input_is_line_side_blank(
    font_system: &mut glyphon::FontSystem,
    display: &str,
    mx: f32,
    my: f32,
    bounds: [f32; 4],
    style: &TextStyle,
    ui_scale: f32,
) -> bool {
    let x = bounds[0];
    let y = bounds[1];
    let w = bounds[2];
    let h = bounds[3];
    // Do not clamp into the leaf: chrome padding outside the box is the blank
    // area we care about (textarea Clip path would otherwise collapse it onto ink).
    let my = my.clamp(y, y + h.max(0.0));
    let scaled_size = style.size_resolved() * ui_scale;
    let line_height = line_height_for_style(style, ui_scale);
    let line_height_logical = line_height_for_style(style, 1.0).max(1.0);
    let family = if let Some(ref f) = style.font_family {
        crate::utils::font::resolve_family(font_system.db(), f)
    } else {
        best_font_family(font_system)
    };
    let is_icon = style.font_family.as_deref() == Some("lucide");
    let requested_weight = if is_icon {
        400
    } else {
        style.weight_resolved()
    };
    let font_weight =
        crate::utils::font::normalize_weight_for_family(font_system.db(), family, requested_weight);
    let box_w = w * ui_scale;
    let soft_wrap = style.white_space != crate::style::WhiteSpace::Nowrap
        && !style.overflow.allows_caret_scroll();
    let (line_body, wrap_limit) = if soft_wrap {
        let avail = w.max(1.0);
        let ranges = crate::text::soft_visual_line_byte_ranges(
            display,
            style.size_resolved(),
            style.weight_resolved(),
            line_height_logical,
            avail,
            style.wrap_mode.to_line_break_source(),
        );
        let line_i = ((my - y) / line_height_logical)
            .floor()
            .clamp(0.0, (ranges.len().saturating_sub(1)) as f32) as usize;
        let (start, end) = ranges[line_i];
        (&display[start..end], box_w)
    } else {
        let lines = hard_line_slices(display);
        let line_i = if lines.len() > 1 {
            ((my - y) / line_height_logical)
                .floor()
                .clamp(0.0, (lines.len() - 1) as f32) as usize
        } else {
            0
        };
        let (_line_start, line_body) = lines[line_i];
        let wrap_limit = if style.overflow.allows_caret_scroll() {
            UNBOUNDED_WRAP
        } else {
            box_w
        };
        (line_body, wrap_limit)
    };
    if line_body.is_empty() {
        return false;
    }
    if mx < x || mx > x + w.max(0.0) {
        return true;
    }
    let measured = measure_prefix_width(
        font_system,
        line_body,
        scaled_size,
        line_height,
        font_weight,
        family,
        wrap_limit,
    );
    let content_w = if style.overflow.allows_caret_scroll()
        || style.white_space == crate::style::WhiteSpace::Nowrap
    {
        measured
    } else {
        measured.min(box_w)
    };
    let offset = style.align_inline_offset(content_w, box_w);
    let rel_in_box = (mx - x + style.scroll_x.max(0.0)) * ui_scale;
    // Half-px slack so sitting on the first/last glyph edge is not treated as blank.
    const EPS: f32 = 0.5;
    rel_in_box < offset - EPS || rel_in_box > offset + content_w + EPS
}

fn line_height_for_style(style: &TextStyle, ui_scale: f32) -> f32 {
    if let Some(lh) = style.line_height_px {
        lh * ui_scale
    } else {
        let scale = style
            .line_height_scale
            .unwrap_or_else(|| tailwind_default_leading_scale(style.size_resolved()));
        scaled_size_for_style(style, ui_scale) * scale
    }
}

fn scaled_size_for_style(style: &TextStyle, ui_scale: f32) -> f32 {
    style.size_resolved() * ui_scale
}

#[cfg(test)]
#[path = "tests/text_hit.rs"]
mod tests;
