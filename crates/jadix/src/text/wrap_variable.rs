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
#[path = "tests/wrap_variable.rs"]
mod tests;
