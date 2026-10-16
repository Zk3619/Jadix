//! Variable-width line wrap MVP (single obstacle).
//!
//! First `first_lines` lines use `width_a`; remaining lines use `width_b`.
//! Useful for float/obstacle wrap where an image occupies the top-right for N lines.

use super::measure::{layout_lines, PreparedLine, PreparedText};

/// Soft-wrap with a two-width obstacle model.
///
/// Lines `0..first_lines` (or until the paragraph ends) are laid out against `width_a`.
/// After `first_lines` lines have been produced, subsequent wrapping uses `width_b`.
///
/// Implementation: sequentially wrap remaining segments with the active width, advancing
/// one line at a time so the width can switch mid-paragraph.
pub fn layout_lines_variable_width(
    prepared: &PreparedText,
    width_a: f32,
    width_b: f32,
    first_lines: usize,
) -> Vec<PreparedLine> {
    if prepared.segment_count() == 0 {
        return layout_lines(prepared, width_a.max(width_b));
    }
    if first_lines == 0 {
        return layout_lines(prepared, width_b);
    }

    let mut out: Vec<PreparedLine> = Vec::new();
    let mut seg_start = 0usize;
    let total = prepared.segment_count();

    while seg_start < total {
        let avail = if out.len() < first_lines {
            width_a
        } else {
            width_b
        };
        let slice = prepared.sub_prepared(seg_start, total);
        let lines = layout_lines(&slice, avail);
        if lines.is_empty() {
            break;
        }
        let first = &lines[0];
        // Remap segment indices to the full prepared text.
        let abs_end = seg_start + first.end_seg;
        out.push(PreparedLine {
            start_seg: seg_start + first.start_seg,
            end_seg: abs_end,
            width: first.width,
            text: first.text.clone(),
        });
        if abs_end <= seg_start {
            // Hard newline-only flush: layout_lines may leave end_seg == 0 while advancing
            // past a `\n`. Consume at least one segment to make progress.
            seg_start += 1;
            continue;
        }
        seg_start = abs_end;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::measure::prepare;

    #[test]
    fn first_n_lines_use_narrow_width() {
        // Latin words that wrap at soft spaces.
        let prepared = prepare(
            "alpha bravo charlie delta echo foxtrot golf hotel",
            14.0,
            400,
            21.0,
        );
        let wide = layout_lines(&prepared, 10_000.0);
        assert_eq!(wide.len(), 1, "unwrapped should be one line");

        let narrow = layout_lines(&prepared, 80.0);
        assert!(
            narrow.len() >= 3,
            "narrow wrap should produce several lines, got {}",
            narrow.len()
        );

        // Obstacle: first 2 lines narrower, rest wider → fewer total lines than all-narrow.
        let mixed = layout_lines_variable_width(&prepared, 80.0, 10_000.0, 2);
        assert!(
            mixed.len() >= 2,
            "expected at least the obstacle lines, got {}",
            mixed.len()
        );
        assert!(
            mixed.len() < narrow.len() || mixed.len() == narrow.len(),
            "wider rest should not produce more lines than all-narrow ({} vs {})",
            mixed.len(),
            narrow.len()
        );
        // After first_lines, remaining text should fit on one wide line when width_b is huge.
        if mixed.len() > 2 {
            assert_eq!(
                mixed.len(),
                3,
                "two narrow + one wide rest expected, got {}",
                mixed.len()
            );
        }
    }

    #[test]
    fn zero_first_lines_uses_width_b_only() {
        let prepared = prepare("one two three four five six", 14.0, 400, 21.0);
        let a = layout_lines(&prepared, 60.0);
        let b = layout_lines_variable_width(&prepared, 60.0, 10_000.0, 0);
        let wide = layout_lines(&prepared, 10_000.0);
        assert_eq!(b.len(), wide.len());
        assert!(a.len() >= b.len());
    }
}
