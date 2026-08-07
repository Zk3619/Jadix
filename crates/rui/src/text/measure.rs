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

impl PreparedText {
    /// Number of measure/break segments (for tests and render hints).
    pub fn segment_count(&self) -> usize {
        self.segments.len()
    }

    /// Slice of segments `[start, end)` as a new prepared paragraph (for variable-width wrap).
    pub(crate) fn sub_prepared(&self, start: usize, end: usize) -> PreparedText {
        let end = end.min(self.segments.len());
        let start = start.min(end);
        let segments = self.segments[start..end].to_vec();
        let max_segment_width = segments
            .iter()
            .map(|s| s.width)
            .fold(0.0_f32, f32::max);
        PreparedText {
            segments,
            size: self.size,
            weight: self.weight,
            line_height: self.line_height,
            max_segment_width,
        }
    }
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

/// One soft-wrapped (or hard-broken) line from [`layout_lines`].
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedLine {
    /// Inclusive start segment index into [`PreparedText`] segments.
    pub start_seg: usize,
    /// Exclusive end segment index.
    pub end_seg: usize,
    pub width: f32,
    pub text: String,
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
    static LAYOUT_FONT_SYSTEM: RefCell<LayoutFsSlot> = const { RefCell::new(LayoutFsSlot::Empty) };
}

/// Layout measure FontSystem slot: owned (tests / headless) or shared with the renderer.
enum LayoutFsSlot {
    Empty,
    Owned(Box<FontSystem>),
    /// Non-owning pointer to [`crate::renderer::text::RuiTextRenderer::font_system`].
    /// Valid from [`install_shared_font_system`] until [`clear_shared_font_system`] / shutdown.
    Shared(*mut FontSystem),
}

/// Drop cached segment widths after fonts are (lazily) added so measure/render stay aligned.
pub fn invalidate_prepare_cache() {
    PREPARE_CACHE.with(|c| c.borrow_mut().clear());
}

/// Point layout measure at the renderer's [`FontSystem`] so fontdb / coverage stay single-sourced.
///
/// Call after creating [`crate::renderer::text::RuiTextRenderer`], before layout/measure.
/// Clears the prepare cache so widths are recomputed against the shared DB.
pub fn install_shared_font_system(font_system: &mut FontSystem) {
    invalidate_prepare_cache();
    LAYOUT_FONT_SYSTEM.with(|cell| {
        *cell.borrow_mut() = LayoutFsSlot::Shared(font_system as *mut FontSystem);
    });
}

/// Drop the shared pointer (does not free the renderer FontSystem). Call before dropping GPU text.
pub fn clear_shared_font_system() {
    invalidate_prepare_cache();
    LAYOUT_FONT_SYSTEM.with(|cell| {
        if matches!(*cell.borrow(), LayoutFsSlot::Shared(_)) {
            *cell.borrow_mut() = LayoutFsSlot::Empty;
        }
    });
}

/// Raw pointer to the install shared FontSystem (for secondary window shaping). UI thread only.
pub fn shared_font_system_ptr() -> Option<*mut FontSystem> {
    LAYOUT_FONT_SYSTEM.with(|cell| match *cell.borrow() {
        LayoutFsSlot::Shared(ptr) if !ptr.is_null() => Some(ptr),
        _ => None,
    })
}

/// Release owned layout FontSystem (if any) and the measure cache (call on app shutdown).
pub fn shutdown_layout_text() {
    invalidate_prepare_cache();
    LAYOUT_FONT_SYSTEM.with(|cell| {
        *cell.borrow_mut() = LayoutFsSlot::Empty;
    });
}

fn with_layout_font_system<R>(f: impl FnOnce(&mut FontSystem) -> R) -> R {
    LAYOUT_FONT_SYSTEM.with(|cell| {
        let mut slot = cell.borrow_mut();
        match &mut *slot {
            LayoutFsSlot::Shared(ptr) => {
                // SAFETY: pointer installed for AppState lifetime; UI is single-threaded.
                let fs = unsafe { &mut **ptr };
                f(fs)
            }
            LayoutFsSlot::Owned(fs) => f(fs),
            LayoutFsSlot::Empty => {
                let mut fs = font_loader::create_font_system();
                font_loader::apply_default_families(fs.db_mut());
                *slot = LayoutFsSlot::Owned(Box::new(fs));
                match &mut *slot {
                    LayoutFsSlot::Owned(fs) => f(fs),
                    _ => unreachable!("just installed Owned"),
                }
            }
        }
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
    // Align with `prepare_text`:
    // - default UI face is `best_font_family` (not a bare `Family::SansSerif` enum tag)
    // - weight is snapped to an installed face (`500` → Segoe Semibold `600`)
    // - ASCII non-input text uses Basic shaping in the default perf mode
    let family = match family {
        Family::SansSerif => crate::renderer::text_hit::best_font_family(font_system),
        other => other,
    };
    let weight = crate::utils::font::normalize_weight_for_family(font_system.db(), family, weight);
    let shaping = if text.is_ascii() {
        Shaping::Basic
    } else {
        Shaping::Advanced
    };
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
            shaping,
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

/// Prepare rich-inline spans: plain Text runs use [`split_segments`]; Chip/Mention are
/// one atomic segment with `width = measure(text) + 2 * padding_x` and `break_after = true`.
pub fn prepare_rich(
    spans: &[crate::style::TextSpan],
    default_size: f32,
    default_weight: u16,
    line_height: f32,
) -> PreparedText {
    use crate::style::TextSpanKind;

    let mut segments = Vec::new();
    let mut max_seg = 0.0_f32;
    let family = Family::SansSerif;
    with_layout_font_system(|font_system| {
        for span in spans {
            let size = span.size.unwrap_or(default_size);
            let weight = span.weight.unwrap_or(default_weight);
            let lh = if (size - default_size).abs() > f32::EPSILON {
                line_height * (size / default_size.max(1.0))
            } else {
                line_height
            };
            match span.kind {
                TextSpanKind::Text => {
                    for (seg, break_after) in split_segments(&span.text) {
                        let width =
                            measure_run_width(font_system, &seg, size, lh, weight, family);
                        max_seg = max_seg.max(width);
                        segments.push(PreparedSegment {
                            text: seg,
                            width,
                            break_after,
                        });
                    }
                }
                TextSpanKind::Chip | TextSpanKind::Mention => {
                    let text_w =
                        measure_run_width(font_system, &span.text, size, lh, weight, family);
                    let width = text_w + 2.0 * span.padding_x;
                    max_seg = max_seg.max(width);
                    segments.push(PreparedSegment {
                        text: span.text.clone(),
                        width,
                        break_after: true,
                    });
                }
            }
        }
    });

    PreparedText {
        segments,
        size: default_size,
        weight: default_weight,
        line_height,
        max_segment_width: max_seg,
    }
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

/// Soft-wrap prepared segments into concrete lines (segment ranges + concatenated text).
///
/// Mirrors [`layout`] break decisions for the common case (soft wrap + hard `\n`).
/// Oversized single segments that exceed `avail_width` stay on one logical line
/// (glyph-level split remains render-side).
pub fn layout_lines(prepared: &PreparedText, avail_width: f32) -> Vec<PreparedLine> {
    let avail = avail_width.max(1.0);
    let mut lines: Vec<PreparedLine> = Vec::new();
    let mut start = 0usize;
    let mut line_w = 0.0_f32;
    let mut line_text = String::new();
    let mut can_break_after = false;

    let flush = |lines: &mut Vec<PreparedLine>,
                 start: &mut usize,
                 end: usize,
                 line_w: &mut f32,
                 line_text: &mut String| {
        lines.push(PreparedLine {
            start_seg: *start,
            end_seg: end,
            width: *line_w,
            text: std::mem::take(line_text),
        });
        *start = end;
        *line_w = 0.0;
    };

    for (i, seg) in prepared.segments.iter().enumerate() {
        if seg.text == "\n" {
            flush(&mut lines, &mut start, i, &mut line_w, &mut line_text);
            // Skip the newline segment itself.
            start = i + 1;
            continue;
        }
        // Soft-wrap only at a prior break opportunity (`break_after` on previous segment).
        if line_w > 0.0
            && can_break_after
            && line_w + seg.width > avail
            && seg.width <= avail
        {
            flush(&mut lines, &mut start, i, &mut line_w, &mut line_text);
        }
        line_w += seg.width;
        line_text.push_str(&seg.text);
        can_break_after = seg.break_after;
    }
    if start < prepared.segments.len() || lines.is_empty() {
        flush(
            &mut lines,
            &mut start,
            prepared.segments.len(),
            &mut line_w,
            &mut line_text,
        );
    }
    lines
}

/// Pure arithmetic wrap over prepared segment widths.
///
/// Soft breaks occur only after a segment with [`PreparedSegment::break_after`]
/// (whitespace / CJK / chip·mention); hard `\n` always breaks.
pub fn layout(prepared: &PreparedText, avail_width: f32) -> TextLayoutMetrics {
    let avail = avail_width.max(1.0);
    let mut line_count = 1u32;
    let mut line_w = 0.0_f32;
    let mut max_line_width = 0.0_f32;
    let mut can_break_after = false;
    // Hard cap: prevents a single pathological segment from claiming millions of lines.
    const MAX_EXTRA_LINES_PER_SEG: u32 = 64;
    const MAX_TOTAL_LINES: u32 = 4_096;

    for seg in &prepared.segments {
        if seg.text == "\n" {
            max_line_width = max_line_width.max(line_w);
            line_count = (line_count + 1).min(MAX_TOTAL_LINES);
            line_w = 0.0;
            can_break_after = false;
            continue;
        }
        if line_w > 0.0
            && can_break_after
            && line_w + seg.width > avail
            && seg.width <= avail
        {
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
        }
        can_break_after = seg.break_after;
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
    // Min-content: longest unbreakable segment. Shrinking below this forces mid-word wrap in
    // glyphon (last-letter spill) and fake multi-line height from `layout`'s ceil heuristic.
    let min_content = prepared.max_segment_width.min(intrinsic_w).max(1.0);
    let width = if let Some(w) = known_width {
        w.max(1.0)
    } else if let Some(w) = avail_width {
        intrinsic_w.min(w.max(1.0)).max(min_content)
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
    fn measure_leaf_does_not_shrink_below_unbreakable_word() {
        let text = "Secondary";
        let size = 14.0;
        let lh = size * 1.5;
        let prepared = prepare(text, size, 500, lh);
        let intrinsic = intrinsic_width(&prepared);
        assert!(intrinsic > 2.0, "sanity intrinsic={intrinsic}");
        // Slightly narrower than intrinsic — old path wrapped mid-word and grew height.
        let avail = (intrinsic - 1.0).max(1.0);
        let (w, h) = measure_leaf(text, size, 500, lh, None, Some(avail), 0.0);
        assert!(
            w + 0.01 >= prepared.max_segment_width.min(intrinsic),
            "width {w} should respect min-content {}",
            prepared.max_segment_width
        );
        assert!(
            (h - lh).abs() < 0.5,
            "single unbreakable word should stay one line, got h={h} lh={lh}"
        );
    }

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

    #[test]
    fn layout_lines_matches_line_count_for_latin_wrap() {
        let p = prepare("hello world from rui text measure", 14.0, 400, 14.0 * 1.5);
        let narrow = layout(&p, 80.0);
        let lines = layout_lines(&p, 80.0);
        assert_eq!(lines.len() as u32, narrow.line_count);
        assert!(lines.iter().all(|l| !l.text.is_empty() || l.width == 0.0));
    }

    #[test]
    fn layout_lines_hard_newline() {
        let p = prepare("a\nb\nc", 12.0, 400, 16.0);
        let lines = layout_lines(&p, 10_000.0);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].text, "a");
        assert_eq!(lines[1].text, "b");
        assert_eq!(lines[2].text, "c");
    }

    #[test]
    fn prepare_rich_chip_wider_than_plain() {
        let text = "HelloChip";
        let pad = 6.0;
        let plain = prepare(text, 14.0, 400, 21.0);
        let rich = prepare_rich(
            &[crate::style::TextSpan::new(text).chip().padding_x(pad)],
            14.0,
            400,
            21.0,
        );
        assert_eq!(rich.segment_count(), 1);
        assert!(
            rich.max_segment_width > plain.max_segment_width + pad,
            "chip width {} should exceed plain {} by ~2*pad",
            rich.max_segment_width,
            plain.max_segment_width
        );
        let expected_extra = 2.0 * pad;
        assert!(
            (rich.max_segment_width - plain.max_segment_width - expected_extra).abs() < 0.5
                || rich.max_segment_width >= plain.max_segment_width + expected_extra - 0.5,
            "expected +{expected_extra} padding, plain={} chip={}",
            plain.max_segment_width,
            rich.max_segment_width
        );
    }

    #[test]
    fn prepare_rich_chip_is_atomic_under_narrow_layout() {
        let text = "Hello World Chip";
        let plain = prepare(text, 14.0, 400, 21.0);
        assert!(
            plain.segment_count() > 1,
            "plain should split on spaces, got {}",
            plain.segment_count()
        );
        let rich = prepare_rich(
            &[crate::style::TextSpan::new(text).chip().padding_x(8.0)],
            14.0,
            400,
            21.0,
        );
        assert_eq!(rich.segment_count(), 1);
        let lines = layout_lines(&rich, 40.0);
        assert_eq!(lines.len(), 1, "atomic chip must stay on one logical line");
        assert_eq!(lines[0].text, text);
        // Mid-segment split would produce a line whose text is a proper prefix of the chip.
        assert!(
            !lines.iter().any(|l| text.starts_with(&l.text)
                && l.text != text
                && !l.text.is_empty()),
            "chip must not split mid-segment"
        );
    }

    #[test]
    fn prepare_rich_text_then_chip_wraps_after_break_opportunity() {
        // "Hi " (word + space break_after) + chip — narrow width forces wrap after the space.
        let spans = [
            crate::style::TextSpan::new("Hi "),
            crate::style::TextSpan::new("ChipLabel").chip().padding_x(4.0),
        ];
        let rich = prepare_rich(&spans, 14.0, 400, 21.0);
        assert!(
            rich.segments.len() >= 2,
            "expected text segments + chip, got {}",
            rich.segments.len()
        );
        assert!(
            rich.segments.last().is_some_and(|s| s.break_after),
            "chip must advertise break_after"
        );
        let chip_w = rich.segments.last().unwrap().width;
        // Width just under "Hi "+chip so wrap must occur at the space opportunity.
        let avail = chip_w + 1.0;
        let lines = layout_lines(&rich, avail);
        assert!(
            lines.len() >= 2,
            "expected wrap before chip, got {} lines (avail={avail})",
            lines.len()
        );
        assert!(
            lines.last().is_some_and(|l| l.text.contains("ChipLabel")),
            "chip should sit on its own line after wrap"
        );
    }

    #[test]
    fn shared_font_system_measure_works() {
        let mut render_fs = font_loader::create_font_system();
        font_loader::apply_default_families(render_fs.db_mut());
        install_shared_font_system(&mut render_fs);
        let p = prepare("shared-fs-probe", 14.0, 400, 21.0);
        assert!(p.max_segment_width > 0.0);
        let m = layout(&p, 10_000.0);
        assert_eq!(m.line_count, 1);
        clear_shared_font_system();
        // After clear, measure falls back to owned FontSystem.
        let p2 = prepare("owned-fs-probe", 14.0, 400, 21.0);
        assert!(p2.max_segment_width > 0.0);
        shutdown_layout_text();
    }
}
