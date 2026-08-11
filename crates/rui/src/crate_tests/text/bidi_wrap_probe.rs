//! Gallery bidi: fit is cached; paint shapes at cached wrap width (not full leaf).
use crate::renderer::font_loader;
use crate::renderer::text_hit::best_font_family;
use crate::text::{
    glyphon_fit_soft_wrap, invalidate_prepare_cache, prepare, shutdown_layout_text,
};
use glyphon::{Attrs, Buffer, Metrics, Shaping};

/// Same controls as the Text Compatibility gallery Edge Cases sample.
const GALLERY_BIDI: &str = "Bidirectional sample: English 123 - \u{2068}العربية\u{2060}٤٥٦\u{2069}\u{200B}- \u{2068}עברית\u{2060}789\u{2069}";
const GALLERY_RTL_EMOJI: &str = "RTL+emoji+Latin: عربي 😀 English مرحبا 👍🏽 — 固定回归样例";

fn run_logical_slices(buffer: &Buffer, text: &str) -> Vec<String> {
    let mut runs = Vec::new();
    for run in buffer.layout_runs() {
        let mut starts: Vec<usize> = run.glyphs.iter().map(|g| g.start).collect();
        starts.sort_unstable();
        starts.dedup();
        let mut s = String::new();
        for start in starts {
            if start >= text.len() || !text.is_char_boundary(start) {
                continue;
            }
            if let Some(ch) = text[start..].chars().next() {
                if !matches!(
                    ch,
                    '\u{200B}'
                        | '\u{200E}'
                        | '\u{200F}'
                        | '\u{2060}'
                        | '\u{2066}'
                        | '\u{2067}'
                        | '\u{2068}'
                        | '\u{2069}'
                ) {
                    s.push(ch);
                }
            }
        }
        runs.push(s);
    }
    runs
}

fn max_ink(buffer: &Buffer) -> f32 {
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

#[test]
fn gallery_bidi_fit_soft_wrap_keeps_ink_near_target() {
    invalidate_prepare_cache();
    let samples = [GALLERY_BIDI, GALLERY_RTL_EMOJI];
    let size = 14.0f32;
    let lh = size * 1.5;
    for text in samples {
        let _p = prepare(text, size, 500, lh);
        for target in [200.0f32, 240.0, 280.0] {
            let a = glyphon_fit_soft_wrap(text, size, 500, lh, target);
            let b = glyphon_fit_soft_wrap(text, size, 500, lh, target);
            assert_eq!(a, b, "fit must be cached / stable");
            assert!(
                a.max_ink <= target * 1.08 + 2.0,
                "ink {:.1} too far past target {target} for {:?}",
                a.max_ink,
                &text.chars().take(24).collect::<String>()
            );
            assert!(a.line_count >= 1);
        }
    }
    shutdown_layout_text();
}

#[test]
fn gallery_bidi_paint_uses_fit_wrap_width_not_full_leaf() {
    invalidate_prepare_cache();
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    font_loader::ensure_coverage_for_text(&mut fs, GALLERY_BIDI);
    let family = best_font_family(&fs);
    let size = 14.0f32;
    let lh = size * 1.5;
    let leaf_w = 240.0f32;
    let fit = glyphon_fit_soft_wrap(GALLERY_BIDI, size, 500, lh, leaf_w);

    let mut buffer = Buffer::new(&mut fs, Metrics::new(size, lh));
    buffer.set_text(
        &mut fs,
        GALLERY_BIDI,
        Attrs::new()
            .family(family)
            .weight(glyphon::fontdb::Weight(500)),
        Shaping::Advanced,
    );
    // Mirrors prepare_text: shape at cached fit wrap, not full leaf width.
    buffer.set_size(&mut fs, Some(fit.wrap_width), Some(lh * 16.0));
    buffer.shape_until_scroll(&mut fs, false);
    assert!(
        fit.wrap_width + 0.01 < leaf_w || fit.max_ink > leaf_w,
        "sanity: fit should tighten when ink spills leaf"
    );
    assert!(
        max_ink(&buffer) <= leaf_w * 1.08 + 2.0,
        "ink {:.1} past leaf {leaf_w} at wrap {:.1}",
        max_ink(&buffer),
        fit.wrap_width
    );

    let runs = run_logical_slices(&buffer, GALLERY_BIDI);
    assert!(runs.len() >= 2, "expected wrap at ~{leaf_w}px, got {runs:?}");
    assert!(
        runs
            .iter()
            .any(|r| r.contains('ع') && r.contains('٤') && r.contains('٦')),
        "Arabic isolate split: {runs:?}"
    );
    assert!(
        runs
            .iter()
            .any(|r| r.contains('ע') && r.contains('7') && r.contains('9')),
        "Hebrew isolate split: {runs:?}"
    );
    shutdown_layout_text();
}

#[test]
fn gallery_rtl_emoji_paint_wrap_stays_inside_leaf() {
    invalidate_prepare_cache();
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    font_loader::ensure_coverage_for_text(&mut fs, GALLERY_RTL_EMOJI);
    let family = best_font_family(&fs);
    let size = 14.0f32;
    let lh = size * 1.5;
    let leaf_w = 280.0f32;
    let fit = glyphon_fit_soft_wrap(GALLERY_RTL_EMOJI, size, 500, lh, leaf_w);

    let mut buffer = Buffer::new(&mut fs, Metrics::new(size, lh));
    buffer.set_text(
        &mut fs,
        GALLERY_RTL_EMOJI,
        Attrs::new()
            .family(family)
            .weight(glyphon::fontdb::Weight(500)),
        Shaping::Advanced,
    );
    buffer.set_size(&mut fs, Some(fit.wrap_width), Some(lh * 16.0));
    buffer.shape_until_scroll(&mut fs, false);
    assert!(
        max_ink(&buffer) <= leaf_w * 1.08 + 2.0,
        "emoji/bidi ink {:.1} past leaf {leaf_w}",
        max_ink(&buffer)
    );
    shutdown_layout_text();
}
