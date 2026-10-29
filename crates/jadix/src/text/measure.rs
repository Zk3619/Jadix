//! Pretext-inspired text measure: `prepare` once (segment + shape widths), `layout` is pure math.
//!
//! Layout-time Taffy leaf measure and (later) render share the same segment widths so we no longer
//! estimate with `char_intrinsic_width_em` while glyphon shapes for paint.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use glyphon::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Wrap};

use crate::renderer::font_loader::{self, ensure_coverage_for_text};

/// Logical-px epsilon when comparing [`PreparedLine::width`] to `avail` / glyphon run widths
/// (WP-2.1). Line **counts** for soft-wrap at break opportunities must match exactly.
pub const LINE_WIDTH_EPS: f32 = 1.0;

/// Opaque prepared paragraph: segment widths cached for arbitrary available widths.
#[derive(Clone, Debug)]
pub struct PreparedText {
    // Immutable measurement runs are shared by collect/display-list snapshots.
    // Cloning a label must not allocate and copy every word on each scroll frame.
    segments: std::sync::Arc<[PreparedSegment]>,
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

/// Position in an immutable prepared paragraph, reusable across widths and columns.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LayoutCursor {
    pub segment_index: usize,
}

/// Lay out only the next line without copying or laying out the remaining paragraph.
/// The returned cursor consumes a hard newline, while the line's range excludes it.
/// Like [`layout_lines`], unbreakable segments stay whole even if wider than `avail_width`.
pub fn layout_next_line(
    prepared: &PreparedText,
    start: LayoutCursor,
    avail_width: f32,
) -> Option<(PreparedLine, LayoutCursor)> {
    if start.segment_index >= prepared.segments.len() {
        return None;
    }
    let avail = avail_width.max(1.0);
    let mut line = PreparedLine {
        start_seg: start.segment_index,
        end_seg: start.segment_index,
        width: 0.0,
        text: String::new(),
    };
    let mut can_break_after = false;
    let mut last_segment_width = 0.0;
    let mut last_segment_is_whitespace = false;
    for (index, segment) in prepared
        .segments
        .iter()
        .enumerate()
        .skip(start.segment_index)
    {
        if segment.text == "\n" {
            return Some((
                line,
                LayoutCursor {
                    segment_index: index + 1,
                },
            ));
        }
        if line.width > 0.0 && can_break_after && line.width + segment.width > avail {
            if last_segment_is_whitespace {
                line.width = (line.width - last_segment_width).max(0.0);
            }
            break;
        }
        line.width += segment.width;
        line.text.push_str(&segment.text);
        line.end_seg = index + 1;
        can_break_after = segment.break_after;
        last_segment_width = segment.width;
        last_segment_is_whitespace = segment.text.chars().all(char::is_whitespace);
    }
    let next = LayoutCursor {
        segment_index: line.end_seg,
    };
    Some((line, next))
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct PrepareKey {
    text_hash: u64,
    size_q: u16,
    line_height_q: u16,
    weight: u16,
    family_hash: u64,
}

thread_local! {
    static PREPARE_CACHE: RefCell<HashMap<PrepareKey, PreparedText>> =
        RefCell::new(HashMap::new());
    // Basic ASCII shaping has independent per-character advances (no kerning
    // or ligatures). Reuse those exact advances for changing numeric readouts.
    static ASCII_ADVANCES: RefCell<HashMap<PrepareKey, [f32; 95]>> = RefCell::new(HashMap::new());
    static LAYOUT_FONT_SYSTEM: RefCell<LayoutFsSlot> = const { RefCell::new(LayoutFsSlot::Empty) };
}

/// Layout measure FontSystem slot: owned (tests / headless) or shared with the renderer.
enum LayoutFsSlot {
    Empty,
    Owned(Box<FontSystem>),
    /// Non-owning pointer to [`crate::renderer::text::JadixTextRenderer::font_system`].
    /// Valid from [`install_shared_font_system`] until [`clear_shared_font_system`] / shutdown.
    Shared(*mut FontSystem),
}

/// Drop cached segment widths after fonts are (lazily) added so measure/render stay aligned.
pub fn invalidate_prepare_cache() {
    PREPARE_CACHE.with(|c| c.borrow_mut().clear());
    ASCII_ADVANCES.with(|c| c.borrow_mut().clear());
    FIT_CACHE.with(|c| c.borrow_mut().clear());
    IDEOGRAPH_EM.with(|c| c.set(None));
    IDEOGRAPH_PROBED.with(|c| c.set(false));
    crate::text::visual_lines::invalidate_visual_line_cache();
}

/// Ascent, descent, ideograph ink above baseline, ink below baseline — all in ems.
///
/// `None` until the face that shapes U+4E2D is loaded. A miss is cached until
/// [`invalidate_prepare_cache`] (CJK lazy-load clears it).
pub(crate) fn ideograph_vertical_em() -> Option<(f32, f32, f32, f32)> {
    if let Some(v) = IDEOGRAPH_EM.with(|c| c.get()) {
        return Some((v.ascent, v.descent, v.ink_above, v.ink_below));
    }
    if IDEOGRAPH_PROBED.with(|c| c.get()) {
        return None;
    }
    let found =
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(probe_ideograph_em_inner)) {
            Ok(v) => v,
            Err(_) => return None,
        };
    IDEOGRAPH_PROBED.with(|c| c.set(true));
    if let Some(v) = found {
        IDEOGRAPH_EM.with(|c| c.set(Some(v)));
        Some((v.ascent, v.descent, v.ink_above, v.ink_below))
    } else {
        None
    }
}

fn probe_ideograph_em_inner() -> Option<IdeographEm> {
    with_layout_font_system(|fs| {
        ensure_coverage_for_text(fs, "\u{4E2D}");
        let mut buffer = Buffer::new(fs, Metrics::new(16.0, 24.0));
        buffer.set_size(Some(256.0), Some(48.0));
        buffer.set_text("\u{4E2D}", &Attrs::new(), Shaping::Advanced, None);
        buffer.shape_until_scroll(fs, false);
        let (font_id, weight, glyph_id) = {
            let run = buffer.layout_runs().next()?;
            let glyph = run.glyphs.iter().find(|g| g.glyph_id != 0)?;
            (glyph.font_id, glyph.font_weight, glyph.glyph_id)
        };
        let font = fs.get_font(font_id, weight)?;
        let face_ref = font.as_swash();
        // Same metrics cosmic stores on the shaped glyph (`FontRef::metrics`).
        let metrics = face_ref.metrics(&[]);
        let upem = f32::from(metrics.units_per_em);
        if upem < 1.0 {
            return None;
        }
        // TTC table offsets are from the start of the collection, so parse the
        // whole file at the face whose directory sits at `face_ref.offset`.
        let face = parse_face_at_swash_offset(face_ref.data, face_ref.offset)?;
        let bbox = face.glyph_bounding_box(ttf_parser::GlyphId(glyph_id))?;
        let ink_above = f32::from(bbox.y_max) / upem;
        let ink_below = f32::from(bbox.y_min.min(0).unsigned_abs()) / upem;
        if !(0.45..=1.35).contains(&ink_above) {
            return None;
        }
        Some(IdeographEm {
            ascent: metrics.ascent / upem,
            descent: metrics.descent / upem,
            ink_above,
            ink_below,
        })
    })
}

/// `swash` keeps the whole file plus the table-directory offset. Collection
/// offsets are file-relative, so a slice at that offset is not a font.
fn parse_face_at_swash_offset(data: &[u8], offset: u32) -> Option<ttf_parser::Face<'_>> {
    if let Some(count) = ttf_parser::fonts_in_collection(data) {
        let index = ttc_face_index(data, offset, count)?;
        return ttf_parser::Face::parse(data, index).ok();
    }
    if offset == 0 {
        return ttf_parser::Face::parse(data, 0).ok();
    }
    let slice = data.get(offset as usize..)?;
    ttf_parser::Face::parse(slice, 0).ok()
}

fn ttc_face_index(data: &[u8], offset: u32, count: u32) -> Option<u32> {
    let count = count as usize;
    for i in 0..count {
        let at = 12 + i * 4;
        let bytes = data.get(at..at + 4)?;
        let face_off = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        if face_off == offset {
            return Some(i as u32);
        }
    }
    None
}

/// Point layout measure at the renderer's [`FontSystem`] so fontdb / coverage stay single-sourced.
///
/// Call after creating [`crate::renderer::text::JadixTextRenderer`], before layout/measure.
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

pub(crate) fn with_layout_font_system<R>(f: impl FnOnce(&mut FontSystem) -> R) -> R {
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

/// Single-line glyph advance for a UTF-8 prefix (caret / selection chrome).
pub fn measure_prefix_advance(text: &str, size: f32, weight: u16, line_height: f32) -> f32 {
    if text.is_empty() {
        return 0.0;
    }
    with_layout_font_system(|font_system| {
        let family = crate::renderer::text_hit::best_font_family(font_system);
        let weight =
            crate::utils::font::normalize_weight_for_family(font_system.db(), family, weight);
        crate::renderer::text_hit::measure_prefix_width(
            font_system,
            text,
            size.max(1.0),
            line_height.max(1.0),
            weight,
            family,
            1.0e6,
        )
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

fn prepare_key(
    text: &str,
    size: f32,
    weight: u16,
    line_height: f32,
    family: Family<'_>,
) -> PrepareKey {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    family.hash(&mut hasher);
    PrepareKey {
        text_hash: hash_text(text),
        size_q: quantize(size),
        line_height_q: quantize(line_height),
        weight,
        family_hash: hasher.finish(),
    }
}

fn is_cjk(c: char) -> bool {
    matches!(
        c,
        '\u{3000}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}' | '\u{FF00}'..='\u{FFEF}'
    )
}

/// Split into measure/break segments: whitespace tokens, CJK chars, Latin runs.
///
/// Respects UAX#14 glue/soft opportunities that Rust's `char::is_whitespace` misses:
/// - `U+2060` WJ / `U+FEFF` ZWNBSP stay inside the word (no break)
/// - `U+200B` ZWSP is an explicit soft-break opportunity
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
        } else if c == '\u{2060}' || c == '\u{FEFF}' {
            // Word Joiner / ZWNBSP: glue into the current unbreakable run.
            buf.push(c);
        } else if c == '\u{200B}' {
            flush_word(&mut buf, &mut out);
            out.push((c.to_string(), true));
        } else if c.is_whitespace() || is_cjk(c) {
            flush_word(&mut buf, &mut out);
            out.push((c.to_string(), true));
        } else if matches!(c, '-' | '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}') {
            // UAX#14 BA: break *after* hyphen — matches glyphon `Wrap::Word` ("border-│radius").
            buf.push(c);
            out.push((std::mem::take(&mut buf), true));
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
    measure_resolved_run_width(font_system, text, size, line_height, weight, family)
}

fn ascii_advances(
    font_system: &mut FontSystem,
    size: f32,
    line_height: f32,
    weight: u16,
    family: Family<'_>,
) -> [f32; 95] {
    let key = prepare_key("", size, weight, line_height, family);
    let cached = ASCII_ADVANCES.with(|c| c.borrow().get(&key).copied());
    let advances = cached.unwrap_or_else(|| {
        let alphabet: String = (32_u8..=126).map(char::from).collect();
        let mut buffer = Buffer::new(font_system, Metrics::new(size, line_height));
        buffer.set_size(Some(16_384.0), Some(line_height * 2.0));
        buffer.set_text(
            &alphabet,
            &Attrs::new()
                .family(family)
                .weight(glyphon::fontdb::Weight(weight)),
            Shaping::Basic,
            None,
        );
        buffer.shape_until_scroll(font_system, false);
        let mut table = [0.0; 95];
        for run in buffer.layout_runs() {
            for glyph in run.glyphs {
                table[glyph.start] = glyph.w;
            }
        }
        ASCII_ADVANCES.with(|c| {
            let mut cache = c.borrow_mut();
            if cache.len() > 128 {
                cache.clear();
            }
            cache.insert(key, table);
        });
        table
    });
    advances
}

fn ascii_metrics_cacheable(size: f32, line_height: f32) -> bool {
    size.is_finite() && size > 0.0 && line_height.is_finite() && line_height > 0.0
}

fn measure_resolved_run_width(
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
    if ascii_metrics_cacheable(size, line_height) && text.bytes().all(|b| (32..=126).contains(&b)) {
        let advances = ascii_advances(font_system, size, line_height, weight, family);
        return text.bytes().map(|b| advances[(b - 32) as usize]).sum();
    }
    let shaping = if text.is_ascii() {
        Shaping::Basic
    } else {
        Shaping::Advanced
    };
    ensure_coverage_for_text(font_system, text);
    if crate::renderer::emoji_detect::text_contains_emoji(text)
        && crate::renderer::emoji_layout::custom_glyph_layout_enabled()
    {
        ensure_coverage_for_text(
            font_system,
            crate::renderer::emoji_layout::EMOJI_LAYOUT_SLOT.encode_utf8(&mut [0; 4]),
        );
    }
    let shaped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let layout = crate::renderer::emoji_layout::shape_text_for_layout(text);
        let mut buffer = Buffer::new(font_system, Metrics::new(size, line_height));
        buffer.set_size(
            Some(16_384.0),
            Some(line_height + crate::utils::text_field::text_edit_ink_pad_px(size)),
        );
        let attrs = Attrs::new()
            .family(family)
            .weight(glyphon::fontdb::Weight(weight));
        crate::renderer::emoji_layout::set_buffer_with_emoji_slots(
            &mut buffer,
            layout.as_ref(),
            &attrs,
            shaping,
            size,
            line_height,
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
                        let width = measure_run_width(font_system, &seg, size, lh, weight, family);
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
        segments: segments.into(),
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
    let key = prepare_key(text, size, weight, line_height, family);
    if let Some(hit) = PREPARE_CACHE.with(|c| c.borrow().get(&key).cloned()) {
        return hit;
    }

    let parts = split_segments(text);
    let mut segments = Vec::with_capacity(parts.len());
    let mut max_seg = 0.0_f32;
    with_layout_font_system(|font_system| {
        // Resolve once per paragraph, not once for every whitespace/number
        // segment in a changing readout.
        let family = match family {
            Family::SansSerif => crate::renderer::text_hit::best_font_family(font_system),
            other => other,
        };
        let weight =
            crate::utils::font::normalize_weight_for_family(font_system.db(), family, weight);
        // Fetch the shared advance table once, avoiding a cache lookup and
        // copying 95 advances for every whitespace segment.
        let advances = if ascii_metrics_cacheable(size, line_height)
            && text.bytes().all(|b| (32..=126).contains(&b) || b == b'\n')
            && font_system.db().faces().next().is_some()
        {
            Some(ascii_advances(
                font_system,
                size,
                line_height,
                weight,
                family,
            ))
        } else {
            None
        };
        for (seg, break_after) in parts {
            let width = if let Some(advances) = &advances {
                if seg == "\n" {
                    0.0
                } else {
                    seg.bytes().map(|b| advances[(b - 32) as usize]).sum()
                }
            } else {
                measure_resolved_run_width(font_system, &seg, size, line_height, weight, family)
            };
            max_seg = max_seg.max(width);
            segments.push(PreparedSegment {
                text: seg,
                width,
                break_after,
            });
        }
    });

    let prepared = PreparedText {
        segments: segments.into(),
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

/// Soft-wrap line count from a glyphon [`Buffer`] at `avail_width` (logical px), using the
/// same family / weight / shaping rules as [`measure_run_width`].
///
/// Uses [`glyphon_fit_soft_wrap`]: cosmic-text may emit runs wider than the buffer for
/// bidi / mixed-script lines; we tighten wrap width until ink fits `avail_width`.
///
/// Used by WP-2.1 `text_align_*` tests to assert measure [`layout_lines`] ≡ paint soft-wrap.
pub fn glyphon_soft_wrap_line_count(
    text: &str,
    size: f32,
    weight: u16,
    line_height: f32,
    avail_width: f32,
) -> usize {
    glyphon_fit_soft_wrap(text, size, weight, line_height, avail_width).line_count
}

/// Result of shaping with a wrap width tightened until glyph ink fits `target_width`.
#[derive(Clone, Debug, PartialEq)]
pub struct GlyphonFitSoftWrap {
    /// Buffer wrap width actually used (≤ target when bidi overflow required shrink).
    pub wrap_width: f32,
    pub line_count: usize,
    pub max_ink: f32,
    /// Text actually shaped: original, or UAX#14 soft-wrapped (`\n`) for BiDi/WJ.
    pub shape_text: std::sync::Arc<str>,
}

#[derive(Clone)]
struct FitCacheEntry {
    gen: u64,
    value: GlyphonFitSoftWrap,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct FitKey {
    text_hash: u64,
    size_q: u16,
    line_height_q: u16,
    weight: u16,
    target_bits: u32,
}

const FIT_CACHE_SOFT_MAX: usize = 1024;
/// Keep entries touched in the last N frame ticks (egui-style generation GC).
const FIT_CACHE_GEN_KEEP: u64 = 4;

/// Ideograph vertical metrics in ems: `(ascent, descent, ink_above, ink_below)`.
///
/// Cosmic centers the font's ascent+descent box in the line. CJK hhea ascent is
/// taller than the ideograph, so a full-line selection leaves unequal blank
/// above and below 中. These ratios are the face actually used for U+4E2D.
#[derive(Clone, Copy)]
struct IdeographEm {
    ascent: f32,
    descent: f32,
    ink_above: f32,
    ink_below: f32,
}

thread_local! {
    static IDEOGRAPH_EM: Cell<Option<IdeographEm>> = const { Cell::new(None) };
    static IDEOGRAPH_PROBED: Cell<bool> = const { Cell::new(false) };
    static FIT_CACHE: RefCell<HashMap<FitKey, FitCacheEntry>> =
        RefCell::new(HashMap::new());
    static FIT_GEN: Cell<u64> = const { Cell::new(1) };
    /// Resize / scroll fast path: skip expensive glyphon fit shaping in layout + prepare.
    static LAYOUT_TEXT_FAST: Cell<bool> = const { Cell::new(false) };
    /// Paint computed a new FIT_CACHE entry that may need a taller leaf next frame.
    static FIT_NEEDS_RELAYOUT: Cell<bool> = const { Cell::new(false) };
}

/// Advance FIT_CACHE generation and trim stale entries (call once per prepare frame).
pub fn fit_cache_frame_tick() {
    crate::text::visual_lines::visual_line_cache_frame_tick();
    let cur = FIT_GEN.with(|g| {
        let n = g.get().wrapping_add(1).max(1);
        g.set(n);
        n
    });
    FIT_CACHE.with(|c| {
        let mut cache = c.borrow_mut();
        if cache.len() <= FIT_CACHE_SOFT_MAX {
            return;
        }
        let min_gen = cur.saturating_sub(FIT_CACHE_GEN_KEEP);
        cache.retain(|_, e| e.gen >= min_gen);
        if cache.len() > FIT_CACHE_SOFT_MAX {
            let min2 = cur.saturating_sub(1);
            cache.retain(|_, e| e.gen >= min2);
        }
    });
}

fn fit_cache_touch(key: &FitKey) -> Option<GlyphonFitSoftWrap> {
    let gen = FIT_GEN.with(|g| g.get());
    FIT_CACHE.with(|c| {
        let mut cache = c.borrow_mut();
        let entry = cache.get_mut(key)?;
        entry.gen = gen;
        Some(entry.value.clone())
    })
}

fn fit_cache_insert(key: FitKey, value: GlyphonFitSoftWrap) {
    let gen = FIT_GEN.with(|g| g.get());
    FIT_CACHE.with(|c| {
        c.borrow_mut().insert(key, FitCacheEntry { gen, value });
    });
}

/// Enable fast layout/paint text path (window resize drag, scroll). Cleared each frame from render.
pub fn set_layout_text_fast_mode(fast: bool) {
    LAYOUT_TEXT_FAST.with(|c| c.set(fast));
}

#[inline]
pub fn layout_text_fast_mode() -> bool {
    LAYOUT_TEXT_FAST.with(|c| c.get())
}

/// Keep actual width for both measurement and paint, including subpixel wrap thresholds.
#[inline]
pub fn layout_wrap_target(width: f32, _fast: bool) -> f32 {
    width.max(1.0)
}

/// 8px bucket for BiDi/WJ UAX#14 prewrap — always ≤ `width`.
///
/// Rounding **up** then painting at the real leaf with
/// `Wrap::Word` lets cosmic-text re-break inside a hard line and split Word Joiner
/// pairs across BiDi level spans (Hebrew + EN digits). Floor + `Wrap::None` keeps
/// hard `\n` as the only soft breaks.
#[inline]
pub fn layout_wrap_target_prewrap(width: f32) -> f32 {
    let w = width.max(1.0);
    if w > 8.0 {
        ((w / 8.0).floor() * 8.0).max(1.0)
    } else {
        w
    }
}

fn buffer_max_ink(buffer: &Buffer) -> (usize, f32) {
    let mut n = 0usize;
    let mut m = 0.0_f32;
    for run in buffer.layout_runs() {
        n += 1;
        let gr = run.glyphs.iter().map(|g| g.x + g.w).fold(0.0_f32, f32::max);
        m = m.max(gr).max(run.line_w);
    }
    (n, m)
}

fn fit_cache_key(text: &str, size: f32, weight: u16, line_height: f32, target: f32) -> FitKey {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    FitKey {
        text_hash: h.finish(),
        size_q: (size * 10.0).clamp(0.0, u16::MAX as f32) as u16,
        line_height_q: (line_height * 10.0).clamp(0.0, u16::MAX as f32) as u16,
        weight,
        // A bucket may straddle a wrap threshold. Reusing whichever side was
        // measured first makes identical widths depend on resize direction.
        target_bits: target.to_bits(),
    }
}

/// FIT_CACHE lookup only — no glyphon shaping (resize fast path).
pub fn try_glyphon_fit_soft_wrap(
    text: &str,
    size: f32,
    weight: u16,
    line_height: f32,
    target_width: f32,
) -> Option<GlyphonFitSoftWrap> {
    if text.is_empty() {
        return Some(GlyphonFitSoftWrap {
            wrap_width: target_width.max(1.0),
            line_count: 0,
            max_ink: 0.0,
            shape_text: std::sync::Arc::from(""),
        });
    }
    let target = target_width.max(1.0);
    let key = fit_cache_key(text, size, weight, line_height, target);
    fit_cache_touch(&key)
}

pub fn bidi_soft_wrap_height(
    text: &str,
    size: f32,
    weight: u16,
    line_height: f32,
    layout_w: f32,
    max_segment_width: f32,
    metrics: &TextLayoutMetrics,
    nowrap: bool,
) -> f32 {
    // Live resize always uses arithmetic so cache warmth cannot change the measured height.
    // Settle frames may use FIT_CACHE or shape once on a miss for precise bidi height.
    let base = metrics.height.max(line_height);
    if nowrap {
        return base;
    }
    // MinContent / single-segment probe (`layout_w≈0` → max_seg): no soft-wrap fit.
    if layout_w <= 8.0 || layout_w <= max_segment_width + 1.0 {
        return base;
    }
    let fast = layout_text_fast_mode();
    if fast {
        return base;
    }
    // BiDi/WJ prewrap paint always floor-buckets width; keep layout height on the same key.
    let target = if super::uax14_wrap::needs_logical_prewrap(text) {
        layout_wrap_target_prewrap(layout_w)
    } else {
        layout_wrap_target(layout_w, fast)
    };
    if let Some(fit) = try_glyphon_fit_soft_wrap(text, size, weight, line_height, target) {
        return (line_height * fit.line_count.max(1) as f32).max(base);
    }
    if (target - layout_w).abs() > 0.5 {
        if let Some(fit) = try_glyphon_fit_soft_wrap(text, size, weight, line_height, layout_w) {
            return (line_height * fit.line_count.max(1) as f32).max(base);
        }
    }
    let fit = glyphon_fit_soft_wrap(text, size, weight, line_height, target);
    (line_height * fit.line_count.max(1) as f32).max(base)
}

/// Called when paint runs `glyphon_fit` after ink spill so the next frame can remeasure height.
pub fn note_fit_needs_relayout() {
    FIT_NEEDS_RELAYOUT.with(|c| c.set(true));
}

/// Drain the paint→layout height sync flag (one-shot per frame).
pub fn take_fit_needs_relayout() -> bool {
    FIT_NEEDS_RELAYOUT.with(|c| c.replace(false))
}

/// Shape `text` so every layout run's ink stays within `target_width`.
///
/// For BiDi / Word Joiner text, soft breaks are decided with UAX#14 on the **logical**
/// string first (see [`super::uax14_wrap`]), then shaped with `Wrap::None`. That avoids
/// cosmic-text wrapping between BiDi level spans and illegally splitting WJ pairs.
///
/// Results are cached — measure + prepare must not reshape 12×/frame.
pub fn glyphon_fit_soft_wrap(
    text: &str,
    size: f32,
    weight: u16,
    line_height: f32,
    target_width: f32,
) -> GlyphonFitSoftWrap {
    if text.is_empty() {
        return GlyphonFitSoftWrap {
            wrap_width: target_width.max(1.0),
            line_count: 0,
            max_ink: 0.0,
            shape_text: std::sync::Arc::from(""),
        };
    }
    let target = target_width.max(1.0);
    let key = fit_cache_key(text, size, weight, line_height, target);
    if let Some(hit) = fit_cache_touch(&key) {
        return hit;
    }
    let fitted = with_layout_font_system(|font_system| {
        let family = crate::renderer::text_hit::best_font_family(font_system);
        let weight =
            crate::utils::font::normalize_weight_for_family(font_system.db(), family, weight);
        let shaping = if text.is_ascii() {
            Shaping::Basic
        } else {
            Shaping::Advanced
        };
        ensure_coverage_for_text(font_system, text);
        if crate::renderer::emoji_detect::text_contains_emoji(text)
            && crate::renderer::emoji_layout::custom_glyph_layout_enabled()
        {
            ensure_coverage_for_text(
                font_system,
                crate::renderer::emoji_layout::EMOJI_LAYOUT_SLOT.encode_utf8(&mut [0; 4]),
            );
        }
        let layout = crate::renderer::emoji_layout::shape_text_for_layout(text);
        let logical_prewrap = super::uax14_wrap::needs_logical_prewrap(layout.as_ref());

        let mut wrap_w = target;
        let shape_text = if logical_prewrap {
            super::uax14_wrap::soft_wrap_logical(
                font_system,
                layout.as_ref(),
                size,
                weight,
                line_height,
                wrap_w,
            )
        } else {
            std::sync::Arc::from(layout.as_ref())
        };

        let mut buffer = Buffer::new(font_system, Metrics::new(size, line_height));
        if logical_prewrap {
            buffer.set_wrap(Wrap::None);
        }
        let attrs = Attrs::new()
            .family(family)
            .weight(glyphon::fontdb::Weight(weight));
        crate::renderer::emoji_layout::set_buffer_with_emoji_slots(
            &mut buffer,
            shape_text.as_ref(),
            &attrs,
            shaping,
            size,
            line_height,
        );
        let tall = (line_height * 128.0).max(line_height + 2.0);
        buffer.set_size(Some(wrap_w), Some(tall));
        buffer.shape_until_scroll(font_system, false);
        let (mut line_count, mut max_ink) = buffer_max_ink(&buffer);

        // Mild shrink for residual ink spill (emoji / measurement slack).
        // Skip for UAX#14 prewrap: shrinking wrap_w and re-breaking makes the last word
        // jump to the next line on settle (resize flicker).
        if !logical_prewrap && max_ink > target + 0.5 && target > 8.0 {
            let over = (max_ink - target).max(1.0).min((target * 0.15).max(1.0));
            wrap_w = (target - over).max(target * 0.85).max(1.0);
            buffer.set_size(Some(wrap_w), Some(tall));
            buffer.shape_until_scroll(font_system, false);
            let (n, ink) = buffer_max_ink(&buffer);
            line_count = n;
            max_ink = ink;
            if max_ink > target + 1.0 {
                let over2 = (max_ink - target).max(1.0).min((target * 0.2).max(1.0));
                wrap_w = (wrap_w - over2).max(target * 0.75).max(1.0);
                buffer.set_size(Some(wrap_w), Some(tall));
                buffer.shape_until_scroll(font_system, false);
                let (n2, ink2) = buffer_max_ink(&buffer);
                line_count = n2;
                max_ink = ink2;
            }
        }

        GlyphonFitSoftWrap {
            wrap_width: wrap_w,
            line_count,
            max_ink,
            shape_text,
        }
    });
    fit_cache_insert(key, fitted.clone());
    fitted
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
        if line_w > 0.0 && can_break_after && line_w + seg.width > avail && seg.width <= avail {
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

// Soft visual lines live in `visual_lines` (official LineBreakSource). Re-export via `text::mod`.

/// Segment-layout fallback for visual lines when glyphon returns empty runs (rich chips / edge cases).
pub(crate) fn visual_line_ranges_from_segments(
    text: &str,
    size: f32,
    weight: u16,
    line_height: f32,
    avail_width: f32,
) -> Vec<(usize, usize)> {
    let prepared = prepare(text, size, weight, line_height);
    let mut seg_byte_start = Vec::with_capacity(prepared.segments.len());
    let mut byte = 0usize;
    for seg in prepared.segments.iter() {
        seg_byte_start.push(byte);
        byte = byte.saturating_add(seg.text.len()).min(text.len());
    }
    let lines = layout_lines(&prepared, avail_width.max(1.0));
    let mut ranges = Vec::with_capacity(lines.len());
    for line in &lines {
        if line.start_seg >= line.end_seg {
            let start = seg_byte_start
                .get(line.start_seg)
                .copied()
                .unwrap_or(text.len());
            ranges.push((start, start));
            continue;
        }
        let start = seg_byte_start[line.start_seg];
        let last = line.end_seg - 1;
        let end = seg_byte_start[last] + prepared.segments[last].text.len();
        ranges.push((start.min(text.len()), end.min(text.len())));
    }
    if ranges.is_empty() {
        ranges.push((0, text.len()));
    }
    ranges
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

    for seg in prepared.segments.iter() {
        if seg.text == "\n" {
            max_line_width = max_line_width.max(line_w);
            line_count = (line_count + 1).min(MAX_TOTAL_LINES);
            line_w = 0.0;
            can_break_after = false;
            continue;
        }
        if line_w > 0.0 && can_break_after && line_w + seg.width > avail && seg.width <= avail {
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
    for seg in prepared.segments.iter() {
        if seg.text == "\n" {
            max_w = max_w.max(line_w);
            line_w = 0.0;
        } else {
            line_w += seg.width;
        }
    }
    max_w.max(line_w).max(prepared.size * 0.5)
}

/// Snap a content-sized text width up to a whole logical pixel.
///
/// Glyphon `Wrap::Word` compares ink against the buffer width with a hard edge: when the
/// leaf is sized to a fractional intrinsic (e.g. 16.621) but the used box is even 0.5px
/// shorter — or a parent floors the contribution — the last glyph spills onto a new line
/// (`"sun"` → `"su"` / `"n"`). Ceil keeps measure ≥ paint for single-line captions.
#[inline]
pub fn snap_content_text_width(w: f32) -> f32 {
    w.ceil().max(1.0)
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
    nowrap: bool,
) -> (f32, f32) {
    let prepared = prepare(text, size, weight, line_height);
    let mut intrinsic_w = intrinsic_width(&prepared);
    // Approximate letter-spacing inflation: (grapheme_count - 1) * spacing.
    if letter_spacing.abs() > f32::EPSILON {
        let gaps = text.chars().count().saturating_sub(1) as f32;
        intrinsic_w = (intrinsic_w + gaps * letter_spacing).max(0.0);
    }
    let intrinsic_w = snap_content_text_width(intrinsic_w);
    // Min-content: longest unbreakable segment. Shrinking below this forces mid-word wrap in
    // glyphon (last-letter spill) and fake multi-line height from `layout`'s ceil heuristic.
    // `nowrap` (ShadCN Button / Tailwind whitespace-nowrap): min == max == intrinsic.
    let min_content = if nowrap {
        intrinsic_w
    } else {
        snap_content_text_width(prepared.max_segment_width.min(intrinsic_w))
    };
    let width = if nowrap {
        known_width
            .map(|w| w.max(intrinsic_w).max(1.0))
            .unwrap_or(intrinsic_w)
    } else if let Some(w) = known_width {
        w.max(1.0)
    } else if let Some(w) = avail_width {
        intrinsic_w.min(w.max(1.0)).max(min_content)
    } else {
        intrinsic_w
    };
    let layout_w = if nowrap { intrinsic_w } else { width.max(1.0) };
    let wrap_w = layout_wrap_target(layout_w, !nowrap && layout_text_fast_mode());
    let metrics = layout(&prepared, wrap_w);
    let height = bidi_soft_wrap_height(
        text,
        size,
        weight,
        line_height,
        layout_w,
        prepared.max_segment_width,
        &metrics,
        nowrap,
    );
    (width, height)
}

#[cfg(test)]
#[path = "tests/measure.rs"]
mod tests;
