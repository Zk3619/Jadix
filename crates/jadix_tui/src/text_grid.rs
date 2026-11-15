//! Unicode logical bytes and terminal visual cells share one geometry model.
use std::borrow::Cow;
use unicode_bidi::{BidiInfo, Level};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Caret {
    pub byte: usize,
    pub col: usize,
    pub row: usize,
}
pub(crate) struct Glyph<'a> {
    pub start: usize,
    pub end: usize,
    pub symbol: Cow<'a, str>,
    pub col: usize,
    pub row: usize,
    pub width: usize,
    pub rtl: bool,
    pub base_rtl: bool,
}
pub(crate) struct Layout<'a> {
    pub glyphs: Vec<Glyph<'a>>,
    pub stops: Vec<Caret>,
    pub end: (usize, usize),
}
pub(crate) fn needs_bidi(text: &str) -> bool {
    if text.is_ascii() {
        return false;
    }
    text.chars().any(|c| {
        formatting(c)
            || matches!(
                unicode_bidi::bidi_class(c),
                unicode_bidi::BidiClass::R
                    | unicode_bidi::BidiClass::AL
                    | unicode_bidi::BidiClass::RLE
                    | unicode_bidi::BidiClass::RLO
                    | unicode_bidi::BidiClass::RLI
                    | unicode_bidi::BidiClass::LRI
                    | unicode_bidi::BidiClass::FSI
                    | unicode_bidi::BidiClass::PDF
                    | unicode_bidi::BidiClass::PDI
                    | unicode_bidi::BidiClass::LRE
                    | unicode_bidi::BidiClass::LRO
            )
    })
}
fn formatting(c: char) -> bool {
    matches!(c, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
}
impl<'a> Layout<'a> {
    // ponytail: bounded document/labels use O(text bytes) UBA and glyph storage;
    // retain paragraph results if profiling calls for incremental shaping.
    pub fn new(text: &'a str, password: bool, width: usize, rows: usize) -> Self {
        let width = width.max(1);
        let bidi = BidiInfo::new(text, password.then_some(Level::ltr()));
        let mut out = Self {
            glyphs: vec![],
            stops: vec![],
            end: (0, 0),
        };
        let mut row = 0;
        for para in &bidi.paragraphs {
            let mut line = Vec::new();
            let mut col = 0;
            let mut line_start = para.range.start;
            for (offset, cluster) in text[para.range.clone()].grapheme_indices(true) {
                if row >= rows {
                    break;
                }
                let start = para.range.start + offset;
                let end = start + cluster.len();
                let newline = matches!(cluster, "\n" | "\r\n" | "\r");
                let symbol = if newline {
                    ""
                } else if password {
                    "•"
                } else if cluster == "\t" {
                    "    "
                } else if cluster.chars().all(formatting) {
                    ""
                } else if cluster.chars().any(char::is_control) {
                    "�"
                } else {
                    cluster
                };
                let size = ratatui::text::Span::raw(symbol).width();
                if !line.is_empty() && col + size > width {
                    out.line(
                        &bidi,
                        para,
                        line_start..start,
                        std::mem::take(&mut line),
                        row,
                        password,
                    );
                    row += 1;
                    col = 0;
                    line_start = start;
                    if row >= rows {
                        break;
                    }
                }
                line.push(Glyph {
                    start,
                    end,
                    symbol: Cow::Borrowed(symbol),
                    col: 0,
                    row,
                    width: size,
                    rtl: false,
                    base_rtl: false,
                });
                col += size;
                if newline {
                    out.line(
                        &bidi,
                        para,
                        line_start..end,
                        std::mem::take(&mut line),
                        row,
                        password,
                    );
                    row += 1;
                    col = 0;
                    line_start = end;
                }
            }
            if row < rows && !line.is_empty() {
                let end = line.last().unwrap().end;
                out.line(&bidi, para, line_start..end, line, row, password);
                if col >= width {
                    row += 1;
                    col = 0;
                }
            }
            out.end = (col, row);
        }
        if out.stops.is_empty() {
            out.stops.push(Caret {
                byte: 0,
                col: 0,
                row: 0,
            });
        }
        if text.ends_with('\n') || out.end.0 == 0 {
            out.stops.push(Caret {
                byte: text.len(),
                col: out.end.0,
                row: out.end.1,
            });
        }
        // Lines and visual runs already emit nondecreasing visual cells.
        out.stops.dedup();
        out
    }
    fn line(
        &mut self,
        bidi: &BidiInfo<'_>,
        para: &unicode_bidi::ParagraphInfo,
        range: std::ops::Range<usize>,
        logical: Vec<Glyph<'a>>,
        row: usize,
        password: bool,
    ) {
        let offset = range.start;
        let (levels, runs) = if password {
            (Vec::new(), std::iter::once(0..range.len()).collect())
        } else {
            // unicode-bidi::visual_runs clones all levels in its BidiInfo.
            // Preserve the paragraph-resolved levels/classes but scope this
            // view to one line, so wrapping never clones the whole document
            // once per visual row. Rule L1 still uses the original base level.
            let line = BidiInfo {
                text: &bidi.text[range.clone()],
                original_classes: bidi.original_classes[range.clone()].to_vec(),
                levels: bidi.levels[range.clone()].to_vec(),
                paragraphs: Vec::new(),
            };
            let paragraph = unicode_bidi::ParagraphInfo {
                range: 0..range.len(),
                level: para.level,
            };
            line.visual_runs(&paragraph, 0..range.len())
        };
        let starts: Vec<_> = logical.iter().map(|g| g.start).collect();
        let mut ordered: Vec<_> = logical.into_iter().map(Some).collect();
        let mut col = 0;
        for run in runs {
            let rtl = !password && levels[run.start].is_rtl();
            let run = run.start + offset..run.end + offset;
            let indices = starts.partition_point(|start| *start < run.start)
                ..starts.partition_point(|start| *start < run.end);
            let mut emit = |i: usize| {
                let mut g = ordered[i].take().unwrap();
                g.col = col;
                g.row = row;
                g.rtl = rtl;
                g.base_rtl = para.level.is_rtl();
                if rtl
                    && g.symbol
                        .chars()
                        .any(|c| unicode_bidi_mirroring::get_mirrored(c).is_some())
                {
                    g.symbol = Cow::Owned(
                        g.symbol
                            .chars()
                            .map(|c| unicode_bidi_mirroring::get_mirrored(c).unwrap_or(c))
                            .collect(),
                    );
                }
                self.stops.push(Caret {
                    byte: if rtl { g.end } else { g.start },
                    col,
                    row,
                });
                col += g.width;
                self.stops.push(Caret {
                    byte: if rtl { g.start } else { g.end },
                    col,
                    row,
                });
                self.glyphs.push(g);
            };
            if rtl {
                for i in indices.into_iter().rev() {
                    emit(i);
                }
            } else {
                for i in indices {
                    emit(i);
                }
            }
        }
    }
    pub fn caret(&self, byte: usize, affinity: Option<Caret>) -> Caret {
        if let Some(old) = affinity.filter(|old| old.byte == byte && self.stops.contains(old)) {
            return old;
        }
        // Prefer the downstream edge; the other visual edge remains navigable.
        self.glyphs
            .iter()
            .find(|g| g.start == byte)
            .map(|g| Caret {
                byte,
                col: g.col + if g.rtl { g.width } else { 0 },
                row: g.row,
            })
            .or_else(|| self.stops.iter().rev().find(|s| s.byte == byte).copied())
            .unwrap_or(Caret {
                byte,
                col: self.end.0,
                row: self.end.1,
            })
    }
    pub fn hit(&self, col: usize, row: usize) -> Caret {
        self.stops
            .iter()
            .min_by_key(|s| (s.row.abs_diff(row), s.col.abs_diff(col)))
            .copied()
            .unwrap()
    }
    pub fn step(&self, caret: Caret, right: bool) -> Caret {
        let i = self.stops.iter().position(|s| *s == caret).unwrap_or(0);
        self.stops[if right {
            (i + 1).min(self.stops.len() - 1)
        } else {
            i.saturating_sub(1)
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scoped_line_reordering_matches_library_paragraph_oracle() {
        for text in [
            "abc אבג   123 def   אבג",
            "אבג abc 123 אבג \nsecond אבג",
            "x \u{2067}אב 123\u{2069} y",
            "a\u{200e}b\u{200e}c",
        ] {
            let bidi = BidiInfo::new(text, None);
            for width in [1, 3, 9, 80] {
                let layout = Layout::new(text, false, width, usize::MAX);
                for row in 0..=layout.end.1 {
                    let glyphs: Vec<_> = layout.glyphs.iter().filter(|g| g.row == row).collect();
                    if glyphs.is_empty() {
                        continue;
                    }
                    let start = glyphs.iter().map(|g| g.start).min().unwrap();
                    let end = glyphs.iter().map(|g| g.end).max().unwrap();
                    let paragraph = bidi
                        .paragraphs
                        .iter()
                        .find(|p| p.range.contains(&start))
                        .unwrap();
                    let expected: String = bidi
                        .reorder_line(paragraph, start..end)
                        .chars()
                        .filter(|c| !formatting(*c) && *c != '\n')
                        .collect();
                    let actual: String = glyphs.iter().map(|g| g.symbol.as_ref()).collect();
                    assert_eq!(actual, expected, "{text:?} width={width} row={row}");
                }
            }
        }
    }
    #[test]
    fn unicode_visual_order_mirroring_and_caret_affinity_keep_logical_bytes() {
        let text = "abc אבג 123";
        let layout = Layout::new(text, false, 80, usize::MAX);
        assert_eq!(
            layout
                .glyphs
                .iter()
                .map(|g| g.symbol.as_ref())
                .collect::<String>(),
            "abc 123 גבא"
        );
        for g in &layout.glyphs {
            assert!(text.is_char_boundary(g.start) && text.is_char_boundary(g.end));
            let c = layout.caret(g.start, None);
            assert!(layout.stops.contains(&c));
        }
        let rtl = Layout::new("(אב)", false, 20, 10);
        assert_eq!(
            rtl.glyphs
                .iter()
                .map(|g| g.symbol.as_ref())
                .collect::<String>(),
            "(בא)"
        );
        let wrapped = Layout::new("ab👨‍👩‍👧‍👦cd", false, 4, 10);
        assert_eq!(wrapped.glyphs.len(), 5);
        assert_eq!(wrapped.glyphs[2].width, 2);
        assert_eq!(wrapped.glyphs[3].row, 1);
        let boundary = wrapped.glyphs[3].start;
        let upstream = wrapped
            .stops
            .iter()
            .find(|c| c.byte == boundary && c.row == 0)
            .copied()
            .unwrap();
        assert_eq!(wrapped.caret(boundary, Some(upstream)), upstream);
        assert_eq!(wrapped.caret(boundary, None).row, 1);
        let isolated = Layout::new("x \u{2067}אב\u{2069} y", false, 20, 10);
        assert_eq!(
            isolated
                .glyphs
                .iter()
                .map(|g| g.symbol.as_ref())
                .collect::<String>(),
            "x בא y"
        );
    }
}
