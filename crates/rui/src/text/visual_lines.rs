//! Official **LineBreakSource** for soft-wrap visual lines.
//!
//! # Contract
//!
//! Soft wrap never inserts `\n` into the underlying string — only visual rows.
//! Measure / caret / hit-test / ensure-scroll must share [`build_visual_lines`] output
//! (or its cached clone), not re-break with a different algorithm.
//!
//! ## Width buckets
//!
//! - **Editor caret / hit / ensure**: exact leaf `avail_width`.
//! - **BiDi prewrap paint**: [`crate::text::layout_wrap_target_prewrap`] (floor-8 ≤ leaf).
//!
//! ## Char mode
//!
//! [`LineBreakSource::SoftChar`] uses glyphon `Wrap::WordOrGlyph` only when
//! [`crate::text::needs_logical_prewrap`] is false; otherwise it silently falls back to
//! [`LineBreakSource::SoftWord`] (BiDi / Word Joiner safety).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use glyphon::{Attrs, Buffer, FontSystem, Metrics, Shaping, Wrap};

use crate::renderer::font_loader::ensure_coverage_for_text;
use crate::text::measure::{with_layout_font_system, GlyphonFitSoftWrap};
use crate::text::uax14_wrap::needs_logical_prewrap;

/// Soft-wrap (and hard-`\n`) visual lines with per-caret advances for Preferred X.
#[derive(Clone, Debug, PartialEq)]
pub struct VisualLineModel {
    /// `[start, end)` UTF-8 byte ranges into the source text. Hard `\n` is not in either side.
    pub ranges: Vec<(usize, usize)>,
    /// Per visual line: `(byte_offset_in_text, x_from_line_start)` sorted by byte.
    /// Includes line-start `(start, 0.0)` and line-end `(end, line_w)` when possible.
    pub caret_xs: Vec<Vec<(usize, f32)>>,
    pub wrap_width: f32,
    pub line_height: f32,
    /// Source actually used after Char→Word / BiDi fallbacks.
    pub source: LineBreakSource,
}

impl VisualLineModel {
    pub fn line_count(&self) -> usize {
        self.ranges.len().max(1)
    }

    pub fn content_height(&self, ink_pad: f32) -> f32 {
        self.line_height.max(1.0) * self.line_count() as f32 + ink_pad.max(0.0)
    }

    /// Advance from the start of the visual line that owns `caret`.
    pub fn caret_x_on_line(&self, caret: usize) -> f32 {
        let i = caret_visual_line_index(&self.ranges, caret);
        let xs = self.caret_xs.get(i).map(|v| v.as_slice()).unwrap_or(&[]);
        caret_x_from_table(xs, caret)
    }

    /// Byte index on visual line `line_i` nearest to `preferred_x` (px from line start).
    pub fn byte_at_preferred_x(&self, line_i: usize, preferred_x: f32) -> usize {
        let Some(&(start, end)) = self.ranges.get(line_i) else {
            return 0;
        };
        if start == end {
            return start;
        }
        let xs = self
            .caret_xs
            .get(line_i)
            .map(|v| v.as_slice())
            .unwrap_or(&[]);
        if xs.is_empty() {
            return start;
        }
        let mut best = start;
        let mut best_d = f32::MAX;
        for &(b, x) in xs {
            let d = (x - preferred_x).abs();
            if d < best_d {
                best_d = d;
                best = b;
            }
        }
        best.clamp(start, end)
    }
}

/// How to produce visual lines for a plain string.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LineBreakSource {
    /// glyphon `Wrap::Word` at exact width (editor soft wrap).
    SoftWord,
    /// glyphon `Wrap::WordOrGlyph` when safe; else SoftWord.
    SoftChar,
    /// Hard `\n` only (nowrap / horizontal scroll).
    None,
    /// UAX#14 prewrap + `Wrap::None` (BiDi / WJ static + editor when needed).
    BiDiPrewrap,
}

impl LineBreakSource {
    /// Resolve Char against BiDi needs for `text`.
    ///
    /// SoftWord stays SoftWord even for BiDi (editor caret uses Wrap::Word on the original
    /// string). BiDiPrewrap is only for explicit static-label paint paths.
    pub fn resolve_for_text(self, text: &str) -> Self {
        match self {
            Self::SoftChar if needs_logical_prewrap(text) => Self::SoftWord,
            other => other,
        }
    }
}

/// Public wrap mode for Textarea / text leaves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Hash)]
pub enum WrapMode {
    /// Soft wrap at word boundaries (default). Alias parse: `soft`, `word`.
    #[default]
    Word,
    /// No soft wrap; horizontal scroll when content overflows.
    None,
    /// Break mid-word when needed (`char`). Falls back to [`Self::Word`] under BiDi.
    Char,
}

impl WrapMode {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "off" | "scroll" | "nowrap" | "no-wrap" | "none" => Self::None,
            "char" | "character" | "glyph" => Self::Char,
            _ => Self::Word, // soft | word | empty | unknown
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Word => "word",
            Self::None => "none",
            Self::Char => "char",
        }
    }

    #[inline]
    pub fn soft_wrap(self) -> bool {
        !matches!(self, Self::None)
    }

    pub fn to_line_break_source(self) -> LineBreakSource {
        match self {
            Self::None => LineBreakSource::None,
            Self::Word => LineBreakSource::SoftWord,
            Self::Char => LineBreakSource::SoftChar,
        }
    }
}

impl From<&str> for WrapMode {
    fn from(s: &str) -> Self {
        Self::parse(s)
    }
}

impl From<String> for WrapMode {
    fn from(s: String) -> Self {
        Self::parse(&s)
    }
}

/// Index of the visual line that owns `caret`.
pub fn caret_visual_line_index(ranges: &[(usize, usize)], caret: usize) -> usize {
    if ranges.is_empty() {
        return 0;
    }
    let caret = caret.min(ranges.last().map(|(_, e)| *e).unwrap_or(0));
    let mut best = 0usize;
    for (i, &(s, e)) in ranges.iter().enumerate() {
        if caret >= s && caret <= e {
            best = i;
        }
        if caret < s {
            break;
        }
    }
    best
}

fn caret_x_from_table(xs: &[(usize, f32)], caret: usize) -> f32 {
    if xs.is_empty() {
        return 0.0;
    }
    let mut best_x = 0.0;
    for &(b, x) in xs {
        if b <= caret {
            best_x = x;
        } else {
            break;
        }
    }
    best_x
}

fn hash_text(text: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

fn quantize(v: f32) -> u16 {
    (v * 10.0).clamp(0.0, u16::MAX as f32) as u16
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct VisualKey {
    text_hash: u64,
    size_q: u16,
    weight: u16,
    lh_q: u16,
    wrap_w_q: u16,
    source: LineBreakSource,
}

thread_local! {
    static VISUAL_CACHE: RefCell<HashMap<VisualKey, Arc<VisualLineModel>>> =
        RefCell::new(HashMap::new());
    static VISUAL_GEN: Cell<u32> = const { Cell::new(1) };
    static VISUAL_ENTRY_GEN: RefCell<HashMap<VisualKey, u32>> = RefCell::new(HashMap::new());
}

const VISUAL_CACHE_CAP: usize = 256;

/// Drop cached visual lines (tests / font system shutdown).
pub fn invalidate_visual_line_cache() {
    VISUAL_CACHE.with(|c| c.borrow_mut().clear());
    VISUAL_ENTRY_GEN.with(|c| c.borrow_mut().clear());
    VISUAL_GEN.with(|g| g.set(g.get().wrapping_add(1)));
}

/// Per-frame LRU touch for visual line cache (call alongside fit cache tick).
pub fn visual_line_cache_frame_tick() {
    let gen = VISUAL_GEN.with(|g| {
        let n = g.get().wrapping_add(1);
        g.set(n);
        n
    });
    VISUAL_CACHE.with(|cache| {
        VISUAL_ENTRY_GEN.with(|gens| {
            let mut cache = cache.borrow_mut();
            let mut gens = gens.borrow_mut();
            if cache.len() <= VISUAL_CACHE_CAP {
                return;
            }
            let stale: Vec<_> = gens
                .iter()
                .filter(|(_, g)| gen.wrapping_sub(**g) > 2)
                .map(|(k, _)| *k)
                .collect();
            for k in stale {
                cache.remove(&k);
                gens.remove(&k);
            }
            while cache.len() > VISUAL_CACHE_CAP {
                if let Some(k) = cache.keys().next().copied() {
                    cache.remove(&k);
                    gens.remove(&k);
                } else {
                    break;
                }
            }
        });
    });
}

/// Build (or fetch) the official visual-line model for `text`.
pub fn build_visual_lines(
    text: &str,
    size: f32,
    weight: u16,
    line_height: f32,
    avail_width: f32,
    source: LineBreakSource,
) -> Arc<VisualLineModel> {
    let avail = avail_width.max(1.0);
    let lh = line_height.max(1.0);
    let size = size.max(1.0);
    let resolved = source.resolve_for_text(text);
    let key = VisualKey {
        text_hash: hash_text(text),
        size_q: quantize(size),
        weight,
        lh_q: quantize(lh),
        wrap_w_q: quantize(avail),
        source: resolved,
    };
    if let Some(hit) = VISUAL_CACHE.with(|c| c.borrow().get(&key).cloned()) {
        let gen = VISUAL_GEN.with(|g| g.get());
        VISUAL_ENTRY_GEN.with(|g| {
            g.borrow_mut().insert(key, gen);
        });
        return hit;
    }
    let model = Arc::new(build_visual_lines_uncached(
        text, size, weight, lh, avail, resolved,
    ));
    let gen = VISUAL_GEN.with(|g| g.get());
    VISUAL_CACHE.with(|c| {
        let mut cache = c.borrow_mut();
        if cache.len() >= VISUAL_CACHE_CAP {
            if let Some(k) = cache.keys().next().copied() {
                cache.remove(&k);
                VISUAL_ENTRY_GEN.with(|g| {
                    g.borrow_mut().remove(&k);
                });
            }
        }
        cache.insert(key, Arc::clone(&model));
    });
    VISUAL_ENTRY_GEN.with(|g| {
        g.borrow_mut().insert(key, gen);
    });
    model
}

fn build_visual_lines_uncached(
    text: &str,
    size: f32,
    weight: u16,
    line_height: f32,
    avail: f32,
    source: LineBreakSource,
) -> VisualLineModel {
    if text.is_empty() {
        return VisualLineModel {
            ranges: vec![(0, 0)],
            caret_xs: vec![vec![(0, 0.0)]],
            wrap_width: avail,
            line_height,
            source,
        };
    }
    match source {
        LineBreakSource::None => build_hard_lines_only(text, size, weight, line_height, avail),
        LineBreakSource::BiDiPrewrap => {
            build_bidi_prewrap_lines(text, size, weight, line_height, avail)
        }
        LineBreakSource::SoftWord => {
            build_glyphon_wrap_lines(text, size, weight, line_height, avail, Wrap::Word, source)
        }
        LineBreakSource::SoftChar => build_glyphon_wrap_lines(
            text,
            size,
            weight,
            line_height,
            avail,
            Wrap::WordOrGlyph,
            source,
        ),
    }
}

fn build_hard_lines_only(
    text: &str,
    size: f32,
    weight: u16,
    line_height: f32,
    avail: f32,
) -> VisualLineModel {
    let mut ranges = Vec::new();
    let mut caret_xs = Vec::new();
    let mut start = 0usize;
    for (i, _) in text.match_indices('\n') {
        ranges.push((start, i));
        caret_xs.push(line_caret_xs_prefix(text, start, i, size, weight, line_height));
        start = i + 1;
    }
    ranges.push((start, text.len()));
    caret_xs.push(line_caret_xs_prefix(
        text,
        start,
        text.len(),
        size,
        weight,
        line_height,
    ));
    if text.ends_with('\n') {
        let n = text.len();
        ranges.push((n, n));
        caret_xs.push(vec![(n, 0.0)]);
    }
    VisualLineModel {
        ranges,
        caret_xs,
        wrap_width: avail,
        line_height,
        source: LineBreakSource::None,
    }
}

fn line_caret_xs_prefix(
    text: &str,
    start: usize,
    end: usize,
    size: f32,
    weight: u16,
    line_height: f32,
) -> Vec<(usize, f32)> {
    let mut out = vec![(start, 0.0)];
    if start >= end {
        return out;
    }
    let slice = &text[start..end];
    for (i, ch) in slice.char_indices() {
        let next = i + ch.len_utf8();
        let adv = crate::text::measure::measure_prefix_advance(
            &slice[..next],
            size,
            weight,
            line_height,
        );
        out.push((start + next, adv));
    }
    out
}

fn build_bidi_prewrap_lines(
    text: &str,
    size: f32,
    weight: u16,
    line_height: f32,
    avail: f32,
) -> VisualLineModel {
    let fit: GlyphonFitSoftWrap =
        crate::text::measure::glyphon_fit_soft_wrap(text, size, weight, line_height, avail);
    // Prewrapped text may insert `\n`; map layout runs back onto original when possible.
    // For caret/ranges we use the **original** text hard+soft map via shaping the prewrapped
    // buffer, then projecting run ends through the original by scanning.
    let model = with_layout_font_system(|font_system| {
        shape_buffer_to_model(
            font_system,
            fit.shape_text.as_ref(),
            size,
            weight,
            line_height,
            fit.wrap_width.max(1.0),
            Wrap::None,
            LineBreakSource::BiDiPrewrap,
        )
    });
    // If shape_text == original (no extra breaks), ranges are correct for `text`.
    // If prewrap inserted `\n`, ranges are into shape_text — remap via soft_wrap_logical
    // identity when lengths match; otherwise fall back to segment layout on original.
    if fit.shape_text.as_ref() == text {
        return model;
    }
    // Remap: treat prewrapped newlines as soft breaks in original byte space using
    // segment fallback on original (safe for caret) when lengths differ.
    let mut fallback = build_glyphon_wrap_lines(
        text,
        size,
        weight,
        line_height,
        avail,
        Wrap::Word,
        LineBreakSource::BiDiPrewrap,
    );
    // Prefer prewrap line count for height consistency with paint.
    if fallback.ranges.len() != model.ranges.len() && !model.ranges.is_empty() {
        // Keep glyphon Word ranges on original for caret; height consumers may use
        // fit.line_count separately. Annotate wrap_width from fit.
        fallback.wrap_width = fit.wrap_width;
        return fallback;
    }
    fallback.wrap_width = fit.wrap_width;
    fallback
}

fn build_glyphon_wrap_lines(
    text: &str,
    size: f32,
    weight: u16,
    line_height: f32,
    avail: f32,
    wrap: Wrap,
    source: LineBreakSource,
) -> VisualLineModel {
    let mut model = with_layout_font_system(|font_system| {
        shape_buffer_to_model(
            font_system,
            text,
            size,
            weight,
            line_height,
            avail,
            wrap,
            source,
        )
    });
    if model.ranges.is_empty() {
        model = soft_visual_line_byte_ranges_segments_model(text, size, weight, line_height, avail);
        model.source = source;
    }
    if text.ends_with('\n') {
        let n = text.len();
        if model.ranges.last() != Some(&(n, n)) {
            model.ranges.push((n, n));
            model.caret_xs.push(vec![(n, 0.0)]);
        }
    }
    model
}

fn shape_buffer_to_model(
    font_system: &mut FontSystem,
    text: &str,
    size: f32,
    weight: u16,
    line_height: f32,
    avail: f32,
    wrap: Wrap,
    source: LineBreakSource,
) -> VisualLineModel {
    let family = crate::renderer::text_hit::best_font_family(font_system);
    let weight =
        crate::utils::font::normalize_weight_for_family(font_system.db(), family, weight);
    ensure_coverage_for_text(font_system, text);
    let shaping = if text.is_ascii() {
        Shaping::Basic
    } else {
        Shaping::Advanced
    };
    let mut buffer = Buffer::new(font_system, Metrics::new(size, line_height));
    buffer.set_wrap(wrap);
    buffer.set_text(
        text,
        &Attrs::new()
            .family(family)
            .weight(glyphon::fontdb::Weight(weight)),
        shaping,
        None,
    );
    let tall = (line_height * (text.lines().count().max(1) as f32 + 4.0)).max(line_height + 2.0);
    buffer.set_size(Some(avail), Some(tall));
    buffer.shape_until_scroll(font_system, false);

    let mut hard_starts: Vec<usize> = Vec::with_capacity(buffer.lines.len().max(1));
    let mut byte = 0usize;
    for (li, line) in buffer.lines.iter().enumerate() {
        hard_starts.push(byte);
        let line_len = line.text().len();
        byte = byte.saturating_add(line_len);
        if li + 1 < buffer.lines.len() {
            if text.as_bytes().get(byte) == Some(&b'\n') {
                byte += 1;
            }
        }
    }

    let mut ranges = Vec::new();
    let mut caret_xs = Vec::new();
    for run in buffer.layout_runs() {
        let base = hard_starts.get(run.line_i).copied().unwrap_or(text.len());
        if run.glyphs.is_empty() {
            let b = base.min(text.len());
            ranges.push((b, b));
            caret_xs.push(vec![(b, 0.0)]);
            continue;
        }
        let start = base.saturating_add(run.glyphs[0].start).min(text.len());
        let end = base
            .saturating_add(run.glyphs[run.glyphs.len() - 1].end)
            .min(text.len());
        ranges.push((start, end.max(start)));
        let mut xs = Vec::with_capacity(run.glyphs.len() + 2);
        xs.push((start, 0.0_f32));
        for g in run.glyphs.iter() {
            let b = base.saturating_add(g.start).min(text.len());
            xs.push((b, g.x));
            let be = base.saturating_add(g.end).min(text.len());
            xs.push((be, g.x + g.w));
        }
        // Dedup by byte keeping last x.
        xs.sort_by_key(|(b, _)| *b);
        let mut dedup: Vec<(usize, f32)> = Vec::with_capacity(xs.len());
        for (b, x) in xs {
            if let Some(last) = dedup.last_mut() {
                if last.0 == b {
                    *last = (b, x);
                    continue;
                }
            }
            dedup.push((b, x));
        }
        caret_xs.push(dedup);
    }
    VisualLineModel {
        ranges,
        caret_xs,
        wrap_width: avail,
        line_height,
        source,
    }
}

fn soft_visual_line_byte_ranges_segments_model(
    text: &str,
    size: f32,
    weight: u16,
    line_height: f32,
    avail_width: f32,
) -> VisualLineModel {
    let ranges = crate::text::measure::visual_line_ranges_from_segments(
        text,
        size,
        weight,
        line_height,
        avail_width,
    );
    let mut caret_xs = Vec::with_capacity(ranges.len());
    for &(s, e) in &ranges {
        caret_xs.push(line_caret_xs_prefix(text, s, e, size, weight, line_height));
    }
    VisualLineModel {
        ranges,
        caret_xs,
        wrap_width: avail_width,
        line_height,
        source: LineBreakSource::SoftWord,
    }
}

/// Compatibility: ranges only (same as historical `soft_visual_line_byte_ranges`).
pub fn soft_visual_line_byte_ranges(
    text: &str,
    size: f32,
    weight: u16,
    line_height: f32,
    avail_width: f32,
) -> Vec<(usize, usize)> {
    build_visual_lines(
        text,
        size,
        weight,
        line_height,
        avail_width,
        LineBreakSource::SoftWord,
    )
    .ranges
    .clone()
}

// --- Textarea virtualization MVP -------------------------------------------------

/// Character / visual-line thresholds that enable virtualized height estimation.
pub const VIRT_CHAR_THRESHOLD: usize = 64 * 1024;
pub const VIRT_LINE_THRESHOLD: usize = 2048;
pub const VIRT_WINDOW_LINES: usize = 40;

#[derive(Clone, Debug)]
pub struct VirtHeightEstimate {
    pub content_height: f32,
    pub exact_line_count: usize,
    pub estimated: bool,
}

/// Content height for textarea: exact visual lines when small; estimated when huge.
pub fn textarea_visual_content_height(
    text: &str,
    size: f32,
    weight: u16,
    line_height: f32,
    avail_width: f32,
    wrap: WrapMode,
    ink_pad: f32,
) -> VirtHeightEstimate {
    let lh = line_height.max(1.0);
    if !wrap.soft_wrap() {
        let hard = text.split('\n').count().max(1);
        return VirtHeightEstimate {
            content_height: lh * hard as f32 + ink_pad,
            exact_line_count: hard,
            estimated: false,
        };
    }
    if text.len() < VIRT_CHAR_THRESHOLD {
        let model = build_visual_lines(
            text,
            size,
            weight,
            lh,
            avail_width,
            wrap.to_line_break_source(),
        );
        if model.line_count() < VIRT_LINE_THRESHOLD {
            return VirtHeightEstimate {
                content_height: model.content_height(ink_pad),
                exact_line_count: model.line_count(),
                estimated: false,
            };
        }
    }
    // Virtualized: sample first / middle / last hard paragraphs for avg visual lines/char.
    let sample = sample_virt_avg_line_height(text, size, weight, lh, avail_width, wrap);
    let hard_n = text.split('\n').count().max(1);
    let est_lines = ((text.len() as f32) * sample.lines_per_byte).ceil().max(hard_n as f32) as usize;
    VirtHeightEstimate {
        content_height: lh * est_lines as f32 + ink_pad,
        exact_line_count: est_lines,
        estimated: true,
    }
}

struct VirtSample {
    lines_per_byte: f32,
}

fn sample_virt_avg_line_height(
    text: &str,
    size: f32,
    weight: u16,
    lh: f32,
    avail: f32,
    wrap: WrapMode,
) -> VirtSample {
    let src = wrap.to_line_break_source();
    let take = |slice: &str| {
        if slice.is_empty() {
            return (1usize, 1usize);
        }
        let m = build_visual_lines(slice, size, weight, lh, avail, src);
        (m.line_count(), slice.len().max(1))
    };
    let bytes = text.len().max(1);
    let chunk = (bytes / 8).clamp(256, 4096);
    let (l0, b0) = take(&text[..chunk.min(text.len())]);
    let mid = bytes / 2;
    let mid_end = (mid + chunk).min(text.len());
    let mid_start = mid.saturating_sub(chunk / 2).min(mid_end);
    let (l1, b1) = take(&text[mid_start..mid_end]);
    let tail_start = text.len().saturating_sub(chunk);
    let (l2, b2) = take(&text[tail_start..]);
    let lines = (l0 + l1 + l2) as f32;
    let bs = (b0 + b1 + b2) as f32;
    VirtSample {
        lines_per_byte: (lines / bs.max(1.0)).max(1.0 / 80.0),
    }
}

/// Shape only the visual-line window around `focus_line` for a large document.
/// Returns a model covering `[window_start, window_end)` visual lines of the full text
/// by shaping the hard-paragraph span that contains that window (MVP: shape that span fully).
pub fn build_visual_lines_window(
    text: &str,
    size: f32,
    weight: u16,
    line_height: f32,
    avail_width: f32,
    source: LineBreakSource,
    focus_byte: usize,
) -> Arc<VisualLineModel> {
    if text.len() < VIRT_CHAR_THRESHOLD {
        return build_visual_lines(text, size, weight, line_height, avail_width, source);
    }
    // Locate hard paragraph containing focus_byte, expand ± neighboring paragraphs.
    let focus = focus_byte.min(text.len());
    let mut para_start = text[..focus].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let mut para_end = text[focus..]
        .find('\n')
        .map(|i| focus + i + 1)
        .unwrap_or(text.len());
    // Expand by character budget (~window).
    let budget = VIRT_WINDOW_LINES * 40;
    while para_start > 0 && (para_end - para_start) < budget {
        let prev = text[..para_start.saturating_sub(1)]
            .rfind('\n')
            .map(|i| i + 1)
            .unwrap_or(0);
        if prev == para_start {
            break;
        }
        para_start = prev;
    }
    while para_end < text.len() && (para_end - para_start) < budget {
        let next = text[para_end..]
            .find('\n')
            .map(|i| para_end + i + 1)
            .unwrap_or(text.len());
        if next == para_end {
            break;
        }
        para_end = next;
    }
    let slice = &text[para_start..para_end];
    let inner = build_visual_lines(slice, size, weight, line_height, avail_width, source);
    // Offset ranges into full-text byte space.
    let mut model = (*inner).clone();
    for (s, e) in &mut model.ranges {
        *s += para_start;
        *e += para_start;
    }
    for row in &mut model.caret_xs {
        for (b, _) in row.iter_mut() {
            *b += para_start;
        }
    }
    Arc::new(model)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_mode_parse() {
        assert_eq!(WrapMode::parse("soft"), WrapMode::Word);
        assert_eq!(WrapMode::parse("word"), WrapMode::Word);
        assert_eq!(WrapMode::parse("off"), WrapMode::None);
        assert_eq!(WrapMode::parse("char"), WrapMode::Char);
    }

    #[test]
    fn soft_word_wraps_latin() {
        let text = "hello world hello world hello world";
        let model = build_visual_lines(text, 14.0, 400, 21.0, 80.0, LineBreakSource::SoftWord);
        assert!(model.line_count() > 1, "expected wrap, got {:?}", model.ranges);
    }

    #[test]
    fn none_is_hard_lines_only() {
        let text = "aaaa bbbb cccc";
        let model = build_visual_lines(text, 14.0, 400, 21.0, 40.0, LineBreakSource::None);
        assert_eq!(model.line_count(), 1);
    }

    #[test]
    fn preferred_x_roundtrip() {
        let text = "abc";
        let model = build_visual_lines(text, 14.0, 400, 21.0, 400.0, LineBreakSource::SoftWord);
        let x = model.caret_x_on_line(text.len());
        let b = model.byte_at_preferred_x(0, x);
        assert_eq!(b, text.len());
    }

    #[test]
    fn char_falls_back_on_bidi() {
        let text = "עברית\u{2060}789";
        let src = LineBreakSource::SoftChar.resolve_for_text(text);
        assert_eq!(src, LineBreakSource::SoftWord);
    }
}
