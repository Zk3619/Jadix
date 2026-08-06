//! Pretext-inspired text measure: `prepare` once (segment + shape widths), `layout` is pure math.
//!
//! Layout-time Taffy leaf measure and (later) render share the same segment widths so we no longer
//! estimate with `char_intrinsic_width_em` while glyphon shapes for paint.

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use glyphon::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping};

use crate::renderer::font_loader::{self, ensure_coverage_for_text};

/// Opaque prepared paragraph: segment widths cached for arbitrary available widths.
#[derive(Clone, Debug)]
pub struct PreparedText {
    segments: Vec<PreparedSegment>,
    pub size: f32,
    pub weight: u16,
    pub line_height: f32,
    /// Max segment width (single-line intrinsic when no wrap).
    pub max_segment_width: f32,
}

#[derive(Clone, Debug)]
struct PreparedSegment {
    /// Segment text (owned so prepared values can live in a cache).
    text: String,
    width: f32,
    /// Soft wrap opportunity after this segment (whitespace / CJK).
    break_after: bool,
}

/// Result of laying out a prepared paragraph into a fixed width.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextLayoutMetrics {
    pub width: f32,
    pub height: f32,
    pub line_count: u32,
    pub max_line_width: f32,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct PrepareKey {
    text_hash: u64,
    size_q: u16,
    line_height_q: u16,
    weight: u16,
    family_tag: u8,
}

thread_local! {
    static PREPARE_CACHE: RefCell<HashMap<PrepareKey, PreparedText>> =
        RefCell::new(HashMap::new());
    static LAYOUT_FONT_SYSTEM: RefCell<Option<FontSystem>> = const { RefCell::new(None) };
}

/// Drop cached segment widths after fonts are (lazily) added so measure/render stay aligned.
pub fn invalidate_prepare_cache() {
    PREPARE_CACHE.with(|c| c.borrow_mut().clear());
}

/// Release the layout-only [`FontSystem`] and measure cache (call on app shutdown).
pub fn shutdown_layout_text() {
    invalidate_prepare_cache();
    LAYOUT_FONT_SYSTEM.with(|cell| {
        *cell.borrow_mut() = None;
    });
}

fn with_layout_font_system<R>(f: impl FnOnce(&mut FontSystem) -> R) -> R {
    LAYOUT_FONT_SYSTEM.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            let mut fs = font_loader::create_font_system();
            // Re-apply in case create path skipped a default (empty DB / missing faces).
            font_loader::apply_default_families(fs.db_mut());
            *slot = Some(fs);
        }
        f(slot.as_mut().expect("layout font system"))
    })
}

fn hash_text(text: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

fn quantize(v: f32) -> u16 {
    (v * 10.0).clamp(0.0, u16::MAX as f32) as u16
}

fn is_cjk(c: char) -> bool {
    matches!(
        c,
        '\u{3000}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}' | '\u{FF00}'..='\u{FFEF}'
    )
}

/// Split into measure/break segments: whitespace tokens, CJK chars, Latin runs.
fn split_segments(text: &str) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let flush_word = |buf: &mut String, out: &mut Vec<(String, bool)>| {
        if !buf.is_empty() {
            out.push((std::mem::take(buf), false));
        }
    };

    for c in text.chars() {
        if c == '\n' {
            flush_word(&mut buf, &mut out);
            out.push(("\n".to_string(), true));
        } else if c.is_whitespace() || is_cjk(c) {
            flush_word(&mut buf, &mut out);
            out.push((c.to_string(), true));
        } else {
            buf.push(c);
        }
    }
    flush_word(&mut buf, &mut out);
    out
}

fn heuristic_segment_width(text: &str, size: f32, weight: u16) -> f32 {
    let mut w = 0.0_f32;
    for c in text.chars() {
        let em = if c.is_ascii_whitespace() {
            0.35
        } else if c.is_ascii() {
            0.58
        } else {
            let u = c as u32;
            if (0x0300..=0x036F).contains(&u) {
                0.2
            } else {
                0.95
            }
        };
        w += em * size;
    }
    let wf = 1.0 + ((weight as f32 - 400.0).max(0.0)) * 0.0003;
    (w * wf * 1.03).max(size * 0.5)
}

fn measure_run_width(
    font_system: &mut FontSystem,
    text: &str,
    size: f32,
    line_height: f32,
    weight: u16,
    family: Family<'_>,
) -> f32 {
    if text.is_empty() || text == "\n" {
        return 0.0;
    }
    let heuristic = heuristic_segment_width(text, size, weight);
    if font_system.db().faces().next().is_none() {
        return heuristic;
    }
    ensure_coverage_for_text(font_system, text);
    let shaped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut buffer = Buffer::new(font_system, Metrics::new(size, line_height));
        buffer.set_size(font_system, Some(16_384.0), Some(line_height + 2.0));
        buffer.set_text(
            font_system,
            text,
            Attrs::new()
                .family(family)
                .weight(glyphon::fontdb::Weight(weight)),
            Shaping::Advanced,
        );
        buffer.shape_until_scroll(font_system, false);
        buffer
            .layout_runs()
            .map(|run| run.line_w)
            .fold(0.0_f32, f32::max)
    }));
    match shaped {
        Ok(w) if w.is_finite() && w > 0.0 => {
            // Guard against buffer-width / failed-shape blowups that would make
            // `layout()` ceil into near-infinite line counts.
            let max_sane = (text.chars().count() as f32 * size * 2.5).max(size * 2.0);
            if w > max_sane * 4.0 {
                heuristic
            } else {
                w.min(max_sane.max(heuristic))
            }
        }
        _ => heuristic,
    }
}

/// Prepare segment widths once for `(text, size, weight, line_height)`. Cached across frames.
pub fn prepare(text: &str, size: f32, weight: u16, line_height: f32) -> PreparedText {
    prepare_with_family(text, size, weight, line_height, Family::SansSerif)
}

pub fn prepare_with_family(
    text: &str,
    size: f32,
    weight: u16,
    line_height: f32,
    family: Family<'_>,
) -> PreparedText {
    let family_tag = match family {
        Family::Serif => 1,
        Family::SansSerif => 2,
        Family::Monospace => 3,
        Family::Cursive => 4,
        Family::Fantasy => 5,
        Family::Name(_) => 6,
    };
    let key = PrepareKey {
        text_hash: hash_text(text),
        size_q: quantize(size),
        line_height_q: quantize(line_height),
        weight,
        family_tag,
    };
    if let Some(hit) = PREPARE_CACHE.with(|c| c.borrow().get(&key).cloned()) {
        return hit;
    }

    let parts = split_segments(text);
    let mut segments = Vec::with_capacity(parts.len());
    let mut max_seg = 0.0_f32;
    with_layout_font_system(|font_system| {
        for (seg, break_after) in parts {
            let width = measure_run_width(font_system, &seg, size, line_height, weight, family);
            max_seg = max_seg.max(width);
            segments.push(PreparedSegment {
                text: seg,
                width,
                break_after,
            });
        }
    });

    let prepared = PreparedText {
        segments,
        size,
        weight,
        line_height,
        max_segment_width: max_seg,
    };
    PREPARE_CACHE.with(|c| {
        let mut cache = c.borrow_mut();
        if cache.len() > 2048 {
            cache.clear();
        }
        cache.insert(key, prepared.clone());
    });
    prepared
}

/// Pure arithmetic wrap over prepared segment widths.
pub fn layout(prepared: &PreparedText, avail_width: f32) -> TextLayoutMetrics {
    let avail = avail_width.max(1.0);
    let mut line_count = 1u32;
    let mut line_w = 0.0_f32;
    let mut max_line_width = 0.0_f32;
    // Hard cap: prevents a single pathological segment from claiming millions of lines.
    const MAX_EXTRA_LINES_PER_SEG: u32 = 64;
    const MAX_TOTAL_LINES: u32 = 4_096;

    for seg in &prepared.segments {
        if seg.text == "\n" {
            max_line_width = max_line_width.max(line_w);
            line_count = (line_count + 1).min(MAX_TOTAL_LINES);
            line_w = 0.0;
            continue;
        }
        if line_w > 0.0 && line_w + seg.width > avail && seg.width <= avail {
            max_line_width = max_line_width.max(line_w);
            line_count = (line_count + 1).min(MAX_TOTAL_LINES);
            line_w = seg.width;
        } else if seg.width > avail && line_w > 0.0 {
            // Oversized segment on a non-empty line: break first, then place.
            max_line_width = max_line_width.max(line_w);
            line_count = (line_count + 1).min(MAX_TOTAL_LINES);
            line_w = seg.width;
            // Still wider than avail: count extra visual lines (same as old ceil heuristic).
            if seg.width > avail {
                let extra = (seg.width / avail)
                    .ceil()
                    .max(1.0)
                    .min(MAX_EXTRA_LINES_PER_SEG as f32) as u32;
                if extra > 1 {
                    line_count = (line_count + extra - 1).min(MAX_TOTAL_LINES);
                    line_w = seg.width % avail;
                    if line_w == 0.0 {
                        line_w = avail;
                    }
                }
            }
        } else if seg.width > avail {
            let extra = (seg.width / avail)
                .ceil()
                .max(1.0)
                .min(MAX_EXTRA_LINES_PER_SEG as f32) as u32;
            line_count = (line_count + extra - 1).min(MAX_TOTAL_LINES);
            line_w = seg.width % avail;
            if line_w == 0.0 {
                line_w = avail;
            }
            max_line_width = max_line_width.max(avail);
        } else {
            line_w += seg.width;
            let _ = seg.break_after;
        }
    }
    max_line_width = max_line_width.max(line_w);

    TextLayoutMetrics {
        width: max_line_width.min(avail),
        height: prepared.line_height * line_count as f32,
        line_count,
        max_line_width,
    }
}

/// Intrinsic (unwrapped) width: longest hard-line's sum of segment widths.
pub fn intrinsic_width(prepared: &PreparedText) -> f32 {
    let mut line_w = 0.0_f32;
    let mut max_w = 0.0_f32;
    for seg in &prepared.segments {
        if seg.text == "\n" {
            max_w = max_w.max(line_w);
            line_w = 0.0;
        } else {
            line_w += seg.width;
        }
    }
    max_w.max(line_w).max(prepared.size * 0.5)
}

/// Measure a text leaf the way Taffy needs: constrained width + wrapped height.
pub fn measure_leaf(
    text: &str,
    size: f32,
    weight: u16,
    line_height: f32,
    known_width: Option<f32>,
    avail_width: Option<f32>,
    letter_spacing: f32,
) -> (f32, f32) {
    let prepared = prepare(text, size, weight, line_height);
    let mut intrinsic_w = intrinsic_width(&prepared);
    // Approximate letter-spacing inflation: (grapheme_count - 1) * spacing.
    if letter_spacing.abs() > f32::EPSILON {
        let gaps = text.chars().count().saturating_sub(1) as f32;
        intrinsic_w = (intrinsic_w + gaps * letter_spacing).max(0.0);
    }
    let width = if let Some(w) = known_width {
        w
    } else if let Some(w) = avail_width {
        intrinsic_w.min(w.max(1.0))
    } else {
        intrinsic_w
    };
    let metrics = layout(&prepared, width.max(1.0));
    let height = metrics.height.max(line_height);
    (width, height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepare_layout_wraps_latin() {
        let p = prepare("hello world from rui text measure", 14.0, 400, 14.0 * 1.5);
        let wide = layout(&p, 10_000.0);
        assert_eq!(wide.line_count, 1);
        let narrow = layout(&p, 80.0);
        assert!(
            narrow.line_count >= 2,
            "expected wrap, got {} lines (max_line={})",
            narrow.line_count,
            narrow.max_line_width
        );
    }

    #[test]
    fn prepare_is_cached() {
        let a = prepare("cache-me", 12.0, 400, 18.0);
        let b = prepare("cache-me", 12.0, 400, 18.0);
        assert_eq!(a.segments.len(), b.segments.len());
        assert!((a.max_segment_width - b.max_segment_width).abs() < f32::EPSILON);
    }

    #[test]
    fn hard_newline_increments_lines() {
        let p = prepare("a\nb\nc", 12.0, 400, 16.0);
        let m = layout(&p, 10_000.0);
        assert_eq!(m.line_count, 3);
    }

    #[test]
    fn layout_caps_pathological_segment_width() {
        let prepared = PreparedText {
            segments: vec![PreparedSegment {
                text: "爆炸".into(),
                width: 16_384.0,
                break_after: false,
            }],
            size: 14.0,
            weight: 400,
            line_height: 21.0,
            max_segment_width: 16_384.0,
        };
        let m = layout(&prepared, 40.0);
        assert!(
            m.line_count <= 64,
            "line_count blew up: {}",
            m.line_count
        );
        assert!(m.height < 21.0 * 65.0);
    }

    #[test]
    fn cjk_measure_stays_bounded() {
        let text = "中文日文한국어 हिन्दी ไทย";
        let (w, h) = measure_leaf(text, 16.0, 400, 24.0, None, Some(320.0), 0.0);
        assert!(w.is_finite() && h.is_finite(), "non-finite {w}x{h}");
        assert!(h < 24.0 * 64.0, "height too large: {h}");
    }
}
