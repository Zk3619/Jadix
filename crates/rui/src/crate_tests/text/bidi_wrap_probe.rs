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

fn run_logical_slices(buffer: &Buffer, _text: &str) -> Vec<String> {
    let mut runs = Vec::new();
    for run in buffer.layout_runs() {
        // Glyph `start` is into this buffer line (`run.text`), not the full paragraph —
        // UAX#14 prewrap inserts hard `\n`, so each soft line is its own BufferLine.
        let line = run.text;
        let mut starts: Vec<usize> = run.glyphs.iter().map(|g| g.start).collect();
        starts.sort_unstable();
        starts.dedup();
        let mut s = String::new();
        for start in starts {
            if start >= line.len() || !line.is_char_boundary(start) {
                continue;
            }
            if let Some(ch) = line[start..].chars().next() {
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
    // Mirror prepare_text settle path: UAX#14 hard lines + Wrap::None (never Wrap::Word —
    // that re-breaks BiDi spans and splits WJ; never Wrap::None + raw text either).
    buffer.set_wrap(glyphon::Wrap::None);
    buffer.set_text(
        fit.shape_text.as_ref(),
        &Attrs::new()
            .family(family)
            .weight(glyphon::fontdb::Weight(500)),
        Shaping::Advanced,
        None,
    );
    // Mirrors prepare_text: shape at cached fit wrap + UAX#14 prewrap, not full leaf width.
    buffer.set_size(Some(fit.wrap_width), Some(lh * 16.0));
    buffer.shape_until_scroll(&mut fs, false);
    // cosmic-text 0.19 often fits this sample at the leaf width; older versions overflowed
    // (~253px ink in a 240px buffer) and required wrap_width < leaf_w.
    assert!(
        fit.wrap_width <= leaf_w + 0.01,
        "fit wrap {:.1} must not exceed leaf {leaf_w}",
        fit.wrap_width
    );
    assert!(
        max_ink(&buffer) <= leaf_w * 1.08 + 2.0,
        "ink {:.1} past leaf {leaf_w} at wrap {:.1}",
        max_ink(&buffer),
        fit.wrap_width
    );

    let runs = run_logical_slices(&buffer, fit.shape_text.as_ref());
    assert!(
        runs.len() >= 2,
        "expected wrap at ~{leaf_w}px, got {runs:?}"
    );
    assert!(
        runs.iter()
            .any(|r| r.contains('ع') && r.contains('٤') && r.contains('٦')),
        "Arabic isolate split: {runs:?}"
    );
    assert!(
        runs.iter()
            .any(|r| r.contains('ע') && r.contains('7') && r.contains('9')),
        "Hebrew isolate split: {runs:?}"
    );
    shutdown_layout_text();
}

#[test]
fn gallery_bidi_wj_not_split_across_widths() {
    // Scan wrap widths: WJ must keep Arabic+digits and Hebrew+digits on the same visual line.
    invalidate_prepare_cache();
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    font_loader::ensure_coverage_for_text(&mut fs, GALLERY_BIDI);
    let family = best_font_family(&fs);
    let size = 14.0f32;
    let lh = size * 1.5;
    let mut bad = Vec::new();
    for w in (120..=360).step_by(4) {
        let fit = glyphon_fit_soft_wrap(GALLERY_BIDI, size, 500, lh, w as f32);
        let mut buffer = Buffer::new(&mut fs, Metrics::new(size, lh));
        buffer.set_wrap(glyphon::Wrap::None);
        buffer.set_text(
            fit.shape_text.as_ref(),
            &Attrs::new()
                .family(family)
                .weight(glyphon::fontdb::Weight(500)),
            Shaping::Advanced,
            None,
        );
        buffer.set_size(Some(fit.wrap_width), Some(lh * 16.0));
        buffer.shape_until_scroll(&mut fs, false);
        let runs = run_logical_slices(&buffer, fit.shape_text.as_ref());
        let ar_split = runs.iter().any(|r| r.contains('ع') && !r.contains('٤'))
            && runs.iter().any(|r| r.contains('٤') && !r.contains('ع'));
        let he_split = runs.iter().any(|r| r.contains('ע') && !r.contains('7'))
            && runs.iter().any(|r| r.contains('7') && !r.contains('ע'));
        if ar_split || he_split {
            bad.push((w, ar_split, he_split, fit.shape_text.to_string(), runs));
        }
    }
    assert!(
        bad.is_empty(),
        "WJ-glued clusters split at widths: {bad:#?}"
    );
    shutdown_layout_text();
}

#[test]
fn fast_miss_without_fit_cache_must_not_disable_wrap() {
    // Regression: Wrap::None + raw BiDi text (old fast Miss) overflowed the card forever.
    invalidate_prepare_cache();
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    font_loader::ensure_coverage_for_text(&mut fs, GALLERY_BIDI);
    let family = best_font_family(&fs);
    let size = 14.0f32;
    let lh = size * 1.5;
    let leaf = 220.0f32;

    // Simulate fast Miss: no FIT_CACHE, WordOrGlyph + original text (must still wrap).
    let mut buffer = Buffer::new(&mut fs, Metrics::new(size, lh));
    buffer.set_wrap(glyphon::Wrap::WordOrGlyph);
    buffer.set_text(
        GALLERY_BIDI,
        &Attrs::new()
            .family(family)
            .weight(glyphon::fontdb::Weight(500)),
        Shaping::Advanced,
        None,
    );
    buffer.set_size(Some(leaf), Some(lh * 16.0));
    buffer.shape_until_scroll(&mut fs, false);
    assert!(
        max_ink(&buffer) <= leaf * 1.12 + 2.0,
        "fast path must soft-wrap, ink={} leaf={}",
        max_ink(&buffer),
        leaf
    );

    // Settle: UAX#14 prewrap must keep WJ clusters and stay inside leaf.
    let fit = glyphon_fit_soft_wrap(GALLERY_BIDI, size, 500, lh, leaf);
    buffer.set_wrap(glyphon::Wrap::None);
    buffer.set_text(
        fit.shape_text.as_ref(),
        &Attrs::new()
            .family(family)
            .weight(glyphon::fontdb::Weight(500)),
        Shaping::Advanced,
        None,
    );
    buffer.set_size(Some(fit.wrap_width), Some(lh * 16.0));
    buffer.shape_until_scroll(&mut fs, false);
    let runs = run_logical_slices(&buffer, fit.shape_text.as_ref());
    assert!(
        runs.iter()
            .any(|r| r.contains('ع') && r.contains('٤') && r.contains('٦')),
        "Arabic WJ split after settle: {runs:?}"
    );
    assert!(
        runs.iter()
            .any(|r| r.contains('ע') && r.contains('7') && r.contains('9')),
        "Hebrew WJ split after settle: {runs:?}"
    );
    assert!(max_ink(&buffer) <= leaf * 1.08 + 2.0);
    shutdown_layout_text();
}

#[test]
fn paint_leaf_narrower_than_round_bucket_keeps_hebrew_wj() {
    // Regression: prewrap at round(237)=240 then Wrap::Word at leaf 237 re-broke
    // `עברית\u{2060}789` across lines. Paint must floor-bucket + Wrap::None.
    invalidate_prepare_cache();
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    font_loader::ensure_coverage_for_text(&mut fs, GALLERY_BIDI);
    let family = best_font_family(&fs);
    let size = 14.0f32;
    let lh = size * 1.5;
    let leaf = 237.0f32;
    let prewrap_w = crate::text::layout_wrap_target_prewrap(leaf);
    assert!(
        prewrap_w <= leaf + 0.01,
        "prewrap bucket {prewrap_w} must not exceed leaf {leaf}"
    );
    let fit = glyphon_fit_soft_wrap(GALLERY_BIDI, size, 500, lh, prewrap_w);

    let mut buffer = Buffer::new(&mut fs, Metrics::new(size, lh));
    buffer.set_wrap(glyphon::Wrap::None);
    buffer.set_text(
        fit.shape_text.as_ref(),
        &Attrs::new()
            .family(family)
            .weight(glyphon::fontdb::Weight(500)),
        Shaping::Advanced,
        None,
    );
    buffer.set_size(Some(leaf), Some(lh * 16.0));
    buffer.shape_until_scroll(&mut fs, false);
    let runs = run_logical_slices(&buffer, fit.shape_text.as_ref());
    let he_split = runs.iter().any(|r| r.contains('ע') && !r.contains('7'))
        && runs.iter().any(|r| r.contains('7') && !r.contains('ע'));
    let ar_split = runs.iter().any(|r| r.contains('ع') && !r.contains('٤'))
        && runs.iter().any(|r| r.contains('٤') && !r.contains('ع'));
    assert!(
        !he_split && !ar_split,
        "WJ split at leaf={leaf} prewrap={prewrap_w} runs={runs:?} shape={:?}",
        fit.shape_text
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
        GALLERY_RTL_EMOJI,
        &Attrs::new()
            .family(family)
            .weight(glyphon::fontdb::Weight(500)),
        Shaping::Advanced,
        None,
    );
    buffer.set_size(Some(fit.wrap_width), Some(lh * 16.0));
    buffer.shape_until_scroll(&mut fs, false);
    assert!(
        max_ink(&buffer) <= leaf_w * 1.08 + 2.0,
        "emoji/bidi ink {:.1} past leaf {leaf_w}",
        max_ink(&buffer)
    );
    shutdown_layout_text();
}

#[test]
fn raw_word_wrap_buffer_must_not_count_as_prewrap() {
    // Simulates Bounds reuse of a fast WordOrGlyph buffer: ink still fits the leaf but
    // Hebrew WJ is split. Paint must detect mismatch vs UAX#14 shape_text and re-Miss.
    invalidate_prepare_cache();
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    font_loader::ensure_coverage_for_text(&mut fs, GALLERY_BIDI);
    let family = best_font_family(&fs);
    let size = 14.0f32;
    let lh = size * 1.5;
    let leaf = 300.0f32;

    let mut raw = Buffer::new(&mut fs, Metrics::new(size, lh));
    raw.set_wrap(glyphon::Wrap::WordOrGlyph);
    raw.set_text(
        GALLERY_BIDI,
        &Attrs::new()
            .family(family)
            .weight(glyphon::fontdb::Weight(500)),
        Shaping::Advanced,
        None,
    );
    raw.set_size(Some(leaf), Some(lh * 16.0));
    raw.shape_until_scroll(&mut fs, false);
    let raw_runs = run_logical_slices(&raw, GALLERY_BIDI);
    let he_split = raw_runs.iter().any(|r| r.contains('ע') && !r.contains('7'))
        && raw_runs.iter().any(|r| r.contains('7') && !r.contains('ע'));
    // Fixture: cosmic-text WordOrGlyph must be capable of the illegal split at this width.
    // If fonts change and it no longer splits, still assert prewrap path is clean.
    let fit = glyphon_fit_soft_wrap(
        GALLERY_BIDI,
        size,
        500,
        lh,
        crate::text::layout_wrap_target_prewrap(leaf),
    );
    let mut good = Buffer::new(&mut fs, Metrics::new(size, lh));
    good.set_wrap(glyphon::Wrap::None);
    good.set_text(
        fit.shape_text.as_ref(),
        &Attrs::new()
            .family(family)
            .weight(glyphon::fontdb::Weight(500)),
        Shaping::Advanced,
        None,
    );
    good.set_size(Some(leaf), Some(lh * 16.0));
    good.shape_until_scroll(&mut fs, false);
    let good_runs = run_logical_slices(&good, fit.shape_text.as_ref());
    assert!(
        good_runs
            .iter()
            .any(|r| r.contains('ע') && r.contains('7') && r.contains('9')),
        "prewrap paint must keep Hebrew WJ: {good_runs:?}"
    );
    let raw_joined: String = {
        let mut out = String::new();
        for (i, line) in raw.lines.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            out.push_str(line.text());
        }
        out
    };
    assert_ne!(
        raw_joined.as_str(),
        fit.shape_text.as_ref(),
        "raw WordOrGlyph buffer must differ from UAX#14 shape_text (he_split={he_split})"
    );
    shutdown_layout_text();
}

#[test]
fn ascii_blurb_reflows_when_leaf_grows_past_stale_wrap() {
    // Regression: Bounds reused old narrow wrap when measured still fit the wider leaf,
    // leaving empty space on the right (gallery Checkbox description after widen).
    invalidate_prepare_cache();
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    let family = best_font_family(&fs);
    let text = "Checkbox: embed text / Label inside, or sibling Label with Solid id / for. Tab / Space on the box.";
    let size = 12.0f32;
    let lh = size * 1.5;
    let narrow = 160.0f32;
    let wide = 320.0f32;

    let mut buffer = Buffer::new(&mut fs, Metrics::new(size, lh));
    buffer.set_text(
        text,
        &Attrs::new()
            .family(family)
            .weight(glyphon::fontdb::Weight(500)),
        Shaping::Advanced,
        None,
    );
    buffer.set_size(Some(narrow), Some(lh * 16.0));
    buffer.shape_until_scroll(&mut fs, false);
    let runs_narrow = buffer.layout_runs().count();
    assert!(
        runs_narrow >= 3,
        "fixture must wrap tightly at {narrow}px, got {runs_narrow} runs"
    );

    // Grow leaf: must set_size to wide (not keep narrow wrap just because ink fits).
    buffer.set_size(Some(wide), Some(lh * 16.0));
    buffer.shape_until_scroll(&mut fs, false);
    let runs_wide = buffer.layout_runs().count();
    assert!(
        runs_wide < runs_narrow,
        "widening wrap must reflow to fewer lines ({runs_narrow} → {runs_wide})"
    );
    let ink = max_ink(&buffer);
    assert!(
        ink > narrow + 8.0,
        "reflowed lines should use space past the old wrap; ink={ink} narrow={narrow}"
    );
    shutdown_layout_text();
}
