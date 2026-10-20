//! Prepared text routed through changing widths and rectangular/circular exclusions.

use super::measure::{layout_lines, layout_next_line, LayoutCursor, PreparedLine, PreparedText};

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
    let mut cursor = LayoutCursor::default();
    while cursor.segment_index < prepared.segment_count() {
        let avail = if out.len() < first_lines {
            width_a
        } else {
            width_b
        };
        let Some((line, next)) = layout_next_line(prepared, cursor, avail) else {
            break;
        };
        out.push(line);
        cursor = next;
    }
    out
}

/// Logical-pixel region in the same coordinate system as its exclusions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextFlowRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Geometry to avoid. Inflate the shape before layout to add an optical margin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TextExclusion {
    Rect(TextFlowRect),
    Circle { center: [f32; 2], radius: f32 },
}

impl TextExclusion {
    fn interval(self, top: f32, bottom: f32) -> Option<(f32, f32)> {
        match self {
            Self::Rect(rect) => {
                if ![rect.x, rect.y, rect.width, rect.height]
                    .iter()
                    .all(|v| v.is_finite())
                    || rect.width <= 0.0
                    || rect.height <= 0.0
                    || top >= rect.y + rect.height
                    || bottom <= rect.y
                {
                    return None;
                }
                Some((rect.x, rect.x + rect.width))
            }
            Self::Circle { center, radius } => {
                if ![center[0], center[1], radius].iter().all(|v| v.is_finite()) || radius <= 0.0 {
                    return None;
                }
                let distance = (top - center[1]).max(center[1] - bottom).max(0.0);
                if distance >= radius {
                    return None;
                }
                let half_width = radius * (1.0 - (distance / radius).powi(2)).sqrt();
                Some((center[0] - half_width, center[0] + half_width))
            }
        }
    }
}

/// A single-line fragment. Several fragments may share a row around a central obstacle.
#[derive(Clone, Debug, PartialEq)]
pub struct TextFlowLine {
    pub x: f32,
    pub y: f32,
    pub available_width: f32,
    pub line: PreparedLine,
}

/// Positioned lines plus the first unconsumed segment for the next region/column.
#[derive(Clone, Debug, PartialEq)]
pub struct TextFlowLayout {
    pub lines: Vec<TextFlowLine>,
    pub cursor: LayoutCursor,
}

/// Route text left-to-right through every free interval of each full line-height band.
/// Preparation is reused; resizing and moving obstacles only redo arithmetic and line strings.
/// Narrow slots never split or clip an unbreakable segment: they are skipped without consuming
/// text. A region too narrow for the next word returns it in `cursor` for a wider region.
/// Empty/invalid regions return no lines and leave the cursor unchanged.
pub fn layout_text_flow(
    prepared: &PreparedText,
    start: LayoutCursor,
    region: TextFlowRect,
    exclusions: &[TextExclusion],
) -> TextFlowLayout {
    let mut result = TextFlowLayout {
        lines: Vec::new(),
        cursor: start,
    };
    let line_height = prepared.line_height;
    if ![
        region.x,
        region.y,
        region.width,
        region.height,
        line_height,
        region.x + region.width,
        region.y + region.height,
    ]
    .iter()
    .all(|v| v.is_finite())
        || region.width < 1.0
        || line_height <= 0.0
        || region.height < line_height
    {
        return result;
    }
    let row_count = (region.height / line_height).floor() as usize;
    for row in 0..row_count {
        if result.cursor.segment_index >= prepared.segment_count() {
            break;
        }
        let top = region.y + row as f32 * line_height;
        let mut slots = vec![(region.x, region.x + region.width)];
        for exclusion in exclusions {
            if let Some((left, right)) = exclusion.interval(top, top + line_height) {
                slots = slots
                    .into_iter()
                    .flat_map(|(start, end)| {
                        let before = (start, end.min(left));
                        let after = (start.max(right), end);
                        [before, after]
                            .into_iter()
                            .filter(|(left, right)| right - left >= 1.0)
                    })
                    .collect();
            }
        }
        for (left, right) in slots {
            let Some((line, next)) = layout_next_line(prepared, result.cursor, right - left) else {
                return result;
            };
            if line.width > right - left {
                continue;
            }
            let hard_break = next.segment_index > line.end_seg;
            result.lines.push(TextFlowLine {
                x: left,
                y: top,
                available_width: right - left,
                line,
            });
            result.cursor = next;
            if hard_break {
                break;
            }
        }
    }
    result
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

    #[test]
    fn cursor_consumes_hard_breaks_once() {
        for source in [
            "alpha\n\nbravo\n",
            "\nalpha",
            "中文测试\n第二段",
            "alpha bravo charlie",
        ] {
            let prepared = prepare(source, 14.0, 400, 21.0);
            let expected = layout_lines(&prepared, 90.0);
            let mut cursor = LayoutCursor::default();
            let mut actual = Vec::new();
            while let Some((line, next)) = layout_next_line(&prepared, cursor, 90.0) {
                assert!(next.segment_index > cursor.segment_index);
                actual.push(line);
                cursor = next;
            }
            assert_eq!(
                actual
                    .iter()
                    .map(|line| (&line.start_seg, &line.end_seg, &line.text))
                    .collect::<Vec<_>>(),
                expected
                    .iter()
                    .map(|line| (&line.start_seg, &line.end_seg, &line.text))
                    .collect::<Vec<_>>(),
                "{source}"
            );
            assert_eq!(
                layout_lines_variable_width(&prepared, 90.0, 90.0, 2)
                    .iter()
                    .map(|line| (&line.start_seg, &line.end_seg, &line.text))
                    .collect::<Vec<_>>(),
                expected
                    .iter()
                    .map(|line| (&line.start_seg, &line.end_seg, &line.text))
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn flow_resumes_across_columns_without_losing_text_or_crossing_shapes() {
        let source = "alpha bravo charlie delta echo foxtrot 中文排版绕过图形。\n\n第二段 text continues across columns. ".repeat(8);
        let prepared = prepare(&source, 14.0, 400, 21.0);
        let exclusions = [
            TextExclusion::Circle {
                center: [150.0, 84.0],
                radius: 52.0,
            },
            TextExclusion::Rect(TextFlowRect {
                x: 20.0,
                y: 160.0,
                width: 90.0,
                height: 45.0,
            }),
            TextExclusion::Rect(TextFlowRect {
                x: 40.0,
                y: 180.0,
                width: 90.0,
                height: 45.0,
            }),
        ];
        let mut cursor = LayoutCursor::default();
        let mut actual = String::new();
        let mut saw_split_row = false;
        let mut previous_y = None;
        for column in 0..20 {
            let region = TextFlowRect {
                x: 0.0,
                y: 0.0,
                width: 300.0,
                height: 252.0,
            };
            let flow = layout_text_flow(&prepared, cursor, region, &exclusions);
            assert!(flow.cursor.segment_index > cursor.segment_index);
            for fragment in &flow.lines {
                assert!(fragment.line.width <= fragment.available_width);
                assert!(fragment.x >= 0.0 && fragment.x + fragment.line.width <= 300.0);
                assert!(fragment.y >= 0.0 && fragment.y + 21.0 <= 252.0);
                for exclusion in exclusions {
                    if let Some((left, right)) = exclusion.interval(fragment.y, fragment.y + 21.0) {
                        assert!(fragment.x + fragment.line.width <= left || fragment.x >= right);
                    }
                }
                saw_split_row |= previous_y == Some((column, fragment.y));
                previous_y = Some((column, fragment.y));
                actual.push_str(&fragment.line.text);
            }
            cursor = flow.cursor;
            if cursor.segment_index == prepared.segment_count() {
                break;
            }
        }
        assert_eq!(cursor.segment_index, prepared.segment_count());
        assert_eq!(actual, source.replace('\n', ""));
        assert!(saw_split_row);
    }

    #[test]
    fn blocked_rows_and_unbreakable_words_do_not_consume_text() {
        let prepared = prepare("unbreakableword", 14.0, 400, 21.0);
        let region = TextFlowRect {
            x: 0.0,
            y: 0.0,
            width: 400.0,
            height: 84.0,
        };
        let blocked = layout_text_flow(
            &prepared,
            LayoutCursor::default(),
            region,
            &[TextExclusion::Rect(TextFlowRect {
                height: 42.0,
                ..region
            })],
        );
        assert_eq!(blocked.lines.len(), 1);
        assert_eq!(blocked.lines[0].y, 42.0);
        assert_eq!(blocked.lines[0].line.text, "unbreakableword");
        for region in [
            TextFlowRect {
                width: 1.0,
                ..region
            },
            TextFlowRect {
                width: f32::NAN,
                ..region
            },
            TextFlowRect {
                height: 20.0,
                ..region
            },
        ] {
            let flow = layout_text_flow(&prepared, LayoutCursor::default(), region, &[]);
            assert!(flow.lines.is_empty());
            assert_eq!(flow.cursor, LayoutCursor::default());
        }
    }

    #[test]
    fn circular_exclusion_uses_the_whole_line_band() {
        let circle = TextExclusion::Circle {
            center: [100.0, 100.0],
            radius: 50.0,
        };
        assert_eq!(circle.interval(80.0, 110.0), Some((50.0, 150.0)));
        let (left, right) = circle.interval(50.0, 70.0).unwrap();
        assert!((left - 60.0).abs() < 0.001);
        assert!((right - 140.0).abs() < 0.001);
        assert_eq!(circle.interval(150.0, 171.0), None);
    }
}
