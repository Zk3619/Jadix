//! Soft-wrap using UAX#14 on the **full logical string** before BiDi span split.
//!
//! cosmic-text shapes BiDi level runs as separate spans, then wraps between those
//! words without consulting linebreak opportunities that cross span boundaries.
//! That drops Word Joiner (`U+2060`) and other no-break pairs between letters and
//! numbers inside RTL isolates (gallery Edge Cases). Computing breaks on the
//! logical string first, then inserting hard `\n`, restores Unicode line-breaking.

use std::sync::Arc;

use glyphon::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Wrap};
use unicode_linebreak::{linebreaks, BreakOpportunity};

use crate::renderer::font_loader::ensure_coverage_for_text;
use crate::renderer::text_hit::best_font_family;

/// True when cosmic-text's per-span wrap can illegally split no-break pairs.
#[inline]
pub fn needs_logical_prewrap(text: &str) -> bool {
    if text.is_ascii() {
        return false;
    }
    text.chars().any(|c| {
        matches!(
            c,
            '\u{2060}' // WJ
                | '\u{FEFF}' // ZWNBSP
                | '\u{2066}' // LRI
                | '\u{2067}' // RLI
                | '\u{2068}' // FSI
                | '\u{2069}' // PDI
                | '\u{200B}' // ZWSP (explicit soft opportunity; keep with WJ/FSI path)
                | '\u{200E}'
                | '\u{200F}'
        ) || {
            let u = c as u32;
            // Arabic / Hebrew blocks — BiDi letters often sit beside AN/EN at another level.
            (0x0590..=0x05FF).contains(&u)
                || (0x0600..=0x06FF).contains(&u)
                || (0xFB1D..=0xFDFF).contains(&u)
                || (0xFE70..=0xFEFF).contains(&u)
        }
    })
}

fn measure_nowrap_ink(
    font_system: &mut FontSystem,
    text: &str,
    size: f32,
    line_height: f32,
    weight: u16,
    family: Family<'_>,
    shaping: Shaping,
) -> f32 {
    if text.is_empty() {
        return 0.0;
    }
    let mut buffer = Buffer::new(font_system, Metrics::new(size, line_height));
    buffer.set_wrap(Wrap::None);
    buffer.set_text(
        text,
        &Attrs::new()
            .family(family)
            .weight(glyphon::fontdb::Weight(weight)),
        shaping,
        None,
    );
    buffer.set_size(None, Some(line_height + 2.0));
    buffer.shape_until_scroll(font_system, false);
    buffer
        .layout_runs()
        .map(|run| {
            run.glyphs
                .iter()
                .map(|g| g.x + g.w)
                .fold(0.0_f32, f32::max)
                .max(run.line_w)
        })
        .fold(0.0_f32, f32::max)
}

fn trim_break_tail(s: &str) -> &str {
    s.trim_end_matches(|c: char| c == '\u{200B}' || (c.is_whitespace() && c != '\n'))
}

fn skip_break_head(text: &str, mut i: usize) -> usize {
    while i < text.len() {
        let c = text[i..].chars().next().unwrap();
        if c == '\u{200B}' || (c.is_whitespace() && c != '\n') {
            i += c.len_utf8();
        } else {
            break;
        }
    }
    i
}

/// Insert soft `\n` only at UAX#14-allowed positions so each hard line fits `target_width`.
///
/// Unbreakable clusters wider than `target_width` stay on one line (may ink-overflow);
/// that is preferred over splitting a Word Joiner pair.
pub fn soft_wrap_logical(
    font_system: &mut FontSystem,
    text: &str,
    size: f32,
    weight: u16,
    line_height: f32,
    target_width: f32,
) -> Arc<str> {
    if text.is_empty() || !needs_logical_prewrap(text) {
        return Arc::from(text);
    }
    let target = target_width.max(1.0);
    let family = best_font_family(font_system);
    let weight =
        crate::utils::font::normalize_weight_for_family(font_system.db(), family, weight);
    let shaping = Shaping::Advanced;
    ensure_coverage_for_text(font_system, text);

    if measure_nowrap_ink(
        font_system,
        text,
        size,
        line_height,
        weight,
        family,
        shaping,
    ) <= target + 0.5
    {
        return Arc::from(text);
    }

    // Allowed (and mandatory) break byte indices; always includes `text.len()`.
    let mut breaks: Vec<usize> = linebreaks(text)
        .filter_map(|(i, op)| match op {
            BreakOpportunity::Allowed | BreakOpportunity::Mandatory => Some(i),
        })
        .collect();
    if breaks.last().copied() != Some(text.len()) {
        breaks.push(text.len());
    }

    let mut out = String::with_capacity(text.len() + 8);
    let mut line_start = 0usize;

    while line_start < text.len() {
        // Mandatory `\n` already in the source: keep as hard break.
        if text[line_start..].starts_with('\n') {
            if !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
            line_start += 1;
            continue;
        }

        let mut best: Option<usize> = None;
        for &br in breaks.iter().filter(|&&b| b > line_start) {
            let chunk = trim_break_tail(&text[line_start..br]);
            if chunk.is_empty() {
                best = Some(br);
                continue;
            }
            let ink = measure_nowrap_ink(
                font_system,
                chunk,
                size,
                line_height,
                weight,
                family,
                shaping,
            );
            if ink <= target + 0.5 {
                // Prefer an earlier ZWSP break over extending the line with only
                // `-\s*` (gallery: avoid orphan `-` after Arabic before Hebrew).
                let orphan_hyphen_after_zwsp = chunk.ends_with('-')
                    && text[line_start..br].contains('\u{200B}')
                    && best.is_some_and(|b| text[..b].ends_with('\u{200B}'));
                if !orphan_hyphen_after_zwsp {
                    best = Some(br);
                }
                if text.as_bytes().get(br.saturating_sub(1)) == Some(&b'\n') {
                    break;
                }
            } else {
                // First overflowing candidate: keep previous fit, or take this if nothing fitted
                // (single unbreakable cluster).
                if best.is_none() {
                    best = Some(br);
                }
                break;
            }
        }
        let br = best.unwrap_or(text.len());
        let chunk = trim_break_tail(&text[line_start..br]);
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(chunk);
        line_start = skip_break_head(text, br);
        if line_start <= br && br < text.len() && text[br..].starts_with('\n') {
            line_start = br + 1;
        }
        // Safety: always advance.
        if line_start <= br && chunk.is_empty() && br < text.len() {
            let ch = text[br..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
            line_start = br + ch;
        }
    }

    if out.is_empty() {
        Arc::from(text)
    } else {
        Arc::from(out)
    }
}
