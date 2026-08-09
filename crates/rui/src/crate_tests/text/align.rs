//! WP-2.1: measure `layout_lines` ≡ glyphon soft-wrap (named `text_align_*`).
//!
//! Contract: `docs/text_align.md`.

use glyphon::{fontdb::Database, FontSystem};

use crate::style::TextSpan;
use crate::text::{
    clear_shared_font_system, glyphon_soft_wrap_line_count, install_shared_font_system,
    invalidate_prepare_cache, layout, layout_lines, prepare, prepare_rich, shutdown_layout_text,
    LINE_WIDTH_EPS,
};
use crate::utils::virtual_list::{item_height_for_text, visible_item_range};

/// Soft-wrap lines should stay near `avail`. Slight overflow is allowed for:
/// - unbreakable segments wider than `avail` (glyph split is render-side);
/// - a trailing whitespace/`break_after` token that lands after the last wrap check.
fn assert_lines_fit_avail(lines: &[crate::text::PreparedLine], avail: f32, font_size: f32) {
    let slack = LINE_WIDTH_EPS + font_size.max(1.0);
    for (i, line) in lines.iter().enumerate() {
        assert!(
            line.width <= avail + slack,
            "line {i} width {:.2} exceeds avail {avail} (+slack {slack}): {:?}",
            line.width,
            line.text
        );
    }
}

#[test]
fn text_align_ascii_soft_wrap_lines_match_glyphon() {
    let text = "hello world from rui text measure alignment";
    let size = 14.0;
    let lh = size * 1.5;
    let avail = 80.0;
    let prepared = prepare(text, size, 400, lh);
    let lines = layout_lines(&prepared, avail);
    let metrics = layout(&prepared, avail);
    assert_eq!(lines.len() as u32, metrics.line_count);
    assert!(
        lines.len() >= 2,
        "ASCII soft-wrap should produce ≥2 lines, got {}",
        lines.len()
    );
    assert_lines_fit_avail(&lines, avail, size);
    let glyphon_n = glyphon_soft_wrap_line_count(text, size, 400, lh, avail);
    assert_eq!(
        lines.len(),
        glyphon_n,
        "layout_lines ({}) vs glyphon runs ({}) for ASCII soft-wrap",
        lines.len(),
        glyphon_n
    );
}

#[test]
fn text_align_cjk_soft_wrap_lines_match_glyphon() {
    // Each CJK char is a break opportunity in measure; glyphon should wrap similarly.
    let text = "中文日文测试测宽分行对齐用例足够长";
    let size = 16.0;
    let lh = 24.0;
    let avail = 96.0;
    let prepared = prepare(text, size, 400, lh);
    let lines = layout_lines(&prepared, avail);
    let metrics = layout(&prepared, avail);
    assert_eq!(lines.len() as u32, metrics.line_count);
    assert!(
        lines.len() >= 2,
        "CJK soft-wrap should produce ≥2 lines, got {}",
        lines.len()
    );
    assert_lines_fit_avail(&lines, avail, size);
    let glyphon_n = glyphon_soft_wrap_line_count(text, size, 400, lh, avail);
    assert_eq!(
        lines.len(),
        glyphon_n,
        "layout_lines ({}) vs glyphon runs ({}) for CJK soft-wrap",
        lines.len(),
        glyphon_n
    );
}

#[test]
fn text_align_hard_newline_and_soft_wrap() {
    let text = "alpha beta gamma\ndelta epsilon zeta eta";
    let size = 14.0;
    let lh = size * 1.5;
    let avail = 72.0;
    let prepared = prepare(text, size, 400, lh);
    let lines = layout_lines(&prepared, avail);
    let metrics = layout(&prepared, avail);
    assert_eq!(lines.len() as u32, metrics.line_count);
    assert!(
        lines.len() >= 3,
        "hard newline + soft wrap should yield ≥3 lines, got {}",
        lines.len()
    );
    assert_lines_fit_avail(&lines, avail, size);
    // Wide: only hard breaks.
    let wide = layout_lines(&prepared, 10_000.0);
    assert_eq!(wide.len(), 2);
    assert_eq!(wide[0].text, "alpha beta gamma");
    assert_eq!(wide[1].text, "delta epsilon zeta eta");
}

#[test]
fn text_align_chip_atomic_and_mention_break() {
    let mixed = prepare_rich(
        &[
            TextSpan::new("Hi "),
            TextSpan::new("ChipLabel").chip().padding_x(4.0),
            TextSpan::new(" "),
            TextSpan::new("@bob").mention(),
        ],
        14.0,
        400,
        21.0,
    );
    assert!(
        mixed.segment_count() >= 3,
        "text + chip + mention segments, got {}",
        mixed.segment_count()
    );

    // Atomic chip under tiny avail (spaces inside chip must not soft-split).
    let chip_only = prepare_rich(
        &[TextSpan::new("Hello World Chip").chip().padding_x(8.0)],
        14.0,
        400,
        21.0,
    );
    assert_eq!(chip_only.segment_count(), 1);
    let chip_lines = layout_lines(&chip_only, 40.0);
    assert_eq!(chip_lines.len(), 1);
    assert_eq!(chip_lines[0].text, "Hello World Chip");

    // Mention is also a single atomic segment.
    let mention_only = prepare_rich(&[TextSpan::new("@bob").mention()], 14.0, 400, 21.0);
    assert_eq!(mention_only.segment_count(), 1);
    assert_eq!(layout_lines(&mention_only, 8.0).len(), 1);

    // Wrap before chip when avail is just above chip width.
    let ahead = prepare_rich(
        &[
            TextSpan::new("Hi "),
            TextSpan::new("ChipLabel").chip().padding_x(4.0),
        ],
        14.0,
        400,
        21.0,
    );
    let chip_w = ahead.max_segment_width;
    let lines = layout_lines(&ahead, chip_w + 1.0);
    assert!(lines.len() >= 2, "wrap before chip, got {}", lines.len());
    assert!(lines.last().unwrap().text.contains("ChipLabel"));
}

#[test]
fn text_align_layout_lines_widths_within_tolerance() {
    let text = "the quick brown fox jumps over the lazy dog again";
    let size = 14.0;
    let lh = 21.0;
    let avail = 100.0;
    let prepared = prepare(text, size, 400, lh);
    let lines = layout_lines(&prepared, avail);
    assert_lines_fit_avail(&lines, avail, size);
    let max_w = lines.iter().map(|l| l.width).fold(0.0_f32, f32::max);
    let metrics = layout(&prepared, avail);
    assert!(
        (max_w - metrics.max_line_width).abs() <= LINE_WIDTH_EPS
            || max_w <= metrics.max_line_width + LINE_WIDTH_EPS,
        "max line width layout_lines={max_w} layout()={}",
        metrics.max_line_width
    );
}

#[test]
fn text_align_no_font_heuristic_no_panic() {
    // Empty fontdb → measure_run_width uses heuristic; must not panic.
    let db = Database::new();
    let mut fs = FontSystem::new_with_locale_and_db("en-US".into(), db);
    assert!(
        fs.db().faces().next().is_none(),
        "test requires empty fontdb"
    );
    install_shared_font_system(&mut fs);
    invalidate_prepare_cache();
    let prepared = prepare("hello world 中文", 14.0, 400, 21.0);
    assert!(
        prepared.max_segment_width.is_finite() && prepared.max_segment_width > 0.0,
        "heuristic width must be finite positive"
    );
    let lines = layout_lines(&prepared, 40.0);
    assert!(!lines.is_empty());
    assert!(lines.iter().all(|l| l.width.is_finite()));
    let metrics = layout(&prepared, 40.0);
    assert!(metrics.height.is_finite() && metrics.line_count >= 1);
    clear_shared_font_system();
    shutdown_layout_text();
}

#[test]
fn text_align_virtual_list_fixed_font_row_height() {
    let font_size = 14.0;
    let leading = 1.5;
    let row_h = item_height_for_text(font_size, leading);
    assert!((row_h - 21.0).abs() < f32::EPSILON);
    // Single-line prepare height for the same metrics equals row height.
    let prepared = prepare("Ay", font_size, 400, row_h);
    let metrics = layout(&prepared, 10_000.0);
    assert_eq!(metrics.line_count, 1);
    assert!((metrics.height - row_h).abs() < LINE_WIDTH_EPS);
    // Range math: 210px viewport / 21px rows → ceil(10) visible; overscan=0 → [0, 10).
    let (start, end) = visible_item_range(0.0, 210.0, row_h, 100, 0);
    assert_eq!(start, 0);
    assert_eq!(end, 10);
}
