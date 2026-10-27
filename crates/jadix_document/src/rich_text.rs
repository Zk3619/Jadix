//! Structured rich-text import/export and style-preserving document edits.

use serde::{Deserialize, Serialize};
use std::ops::Range;
use std::sync::Arc;

use super::semantic_document::{block, DocumentSnapshot};
use super::{
    Document, DocumentBlockKind, DocumentEditError, DocumentFormat, DocumentInline,
    DocumentInlineKind, DocumentInlineStyle, TextDocument,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RichTextSpan {
    pub text: String,
    #[serde(default)]
    pub style: DocumentInlineStyle,
    #[serde(default)]
    pub kind: DocumentInlineKind,
}

impl RichTextSpan {
    pub fn new(text: impl Into<String>) -> Self {
        Self::styled(text, DocumentInlineStyle::default())
    }
    pub fn styled(text: impl Into<String>, style: DocumentInlineStyle) -> Self {
        Self {
            text: text.into(),
            style,
            kind: DocumentInlineKind::Text,
        }
    }
    pub fn math(source: impl Into<String>) -> Self {
        Self {
            kind: DocumentInlineKind::Math,
            ..Self::new(source)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RichTextBlock {
    pub kind: DocumentBlockKind,
    pub spans: Vec<RichTextSpan>,
}

impl RichTextBlock {
    pub fn paragraph(spans: impl IntoIterator<Item = RichTextSpan>) -> Self {
        Self {
            kind: DocumentBlockKind::Paragraph,
            spans: spans.into_iter().collect(),
        }
    }
}

impl Document {
    pub fn rich_text(blocks: impl IntoIterator<Item = RichTextBlock>) -> Self {
        let mut source = String::new();
        let mut result = Vec::new();
        let blocks: Vec<_> = blocks.into_iter().collect();
        let count = blocks.len();
        for (position, rich) in blocks.into_iter().enumerate() {
            let start = source.len();
            let mut inlines = Vec::new();
            for span in rich.spans {
                let local = source.len() - start;
                source.push_str(&span.text);
                inlines.push(DocumentInline {
                    source: local..local + span.text.len(),
                    text: span.text,
                    style: span.style,
                    kind: span.kind,
                });
            }
            if position + 1 < count {
                source.push('\n');
            }
            let mut current = block(rich.kind, start..source.len());
            current.inlines = inlines.into();
            result.push(current);
        }
        if result.is_empty() {
            return Self::new(TextDocument::default(), DocumentFormat::RichText);
        }
        let base = Self(Arc::new(DocumentSnapshot {
            source: TextDocument::new(source),
            format: DocumentFormat::RichText,
            blocks: result,
            references: false,
        }));
        replace_rich(&base, base.source().clone(), 0..0, "")
    }

    pub fn rich_text_blocks(&self) -> Vec<RichTextBlock> {
        self.blocks()
            .iter()
            .map(|block| RichTextBlock {
                kind: block.kind.clone(),
                spans: block
                    .inlines
                    .iter()
                    .map(|inline| RichTextSpan {
                        text: inline.text.clone(),
                        style: inline.style.clone(),
                        kind: inline.kind.clone(),
                    })
                    .collect(),
            })
            .collect()
    }

    pub fn with_inline_style(
        &self,
        range: Range<usize>,
        transform: impl Fn(&mut DocumentInlineStyle),
    ) -> Result<Self, DocumentEditError> {
        self.transform_inlines(range, |inline| transform(&mut inline.style))
    }

    pub fn with_inline_kind(
        &self,
        range: Range<usize>,
        kind: DocumentInlineKind,
    ) -> Result<Self, DocumentEditError> {
        self.transform_inlines(range, |inline| inline.kind = kind.clone())
    }

    fn transform_inlines(
        &self,
        range: Range<usize>,
        transform: impl Fn(&mut DocumentInline),
    ) -> Result<Self, DocumentEditError> {
        if self.format() != DocumentFormat::RichText {
            return Err(DocumentEditError::InvalidRange);
        }
        if range.start > range.end || range.end > self.source().len() {
            return Err(DocumentEditError::InvalidRange);
        }
        if !self.source().text().is_char_boundary(range.start)
            || !self.source().text().is_char_boundary(range.end)
        {
            return Err(DocumentEditError::NotCharBoundary);
        }
        let mut blocks = self.blocks().to_vec();
        for current in &mut blocks {
            if current.source.start >= range.end || current.source.end <= range.start {
                continue;
            }
            let mut inlines = Vec::new();
            for inline in current.inlines.iter() {
                let absolute = current.source.start + inline.source.start
                    ..current.source.start + inline.source.end;
                let start = absolute.start.max(range.start);
                let end = absolute.end.min(range.end);
                if start >= end {
                    inlines.push(inline.clone());
                    continue;
                }
                for (part, modify) in [
                    (absolute.start..start, false),
                    (start..end, true),
                    (end..absolute.end, false),
                ] {
                    if part.is_empty() {
                        continue;
                    }
                    let mut fragment = inline.clone();
                    fragment.text = self.source().text()[part.clone()].into();
                    fragment.source =
                        part.start - current.source.start..part.end - current.source.start;
                    if modify {
                        transform(&mut fragment);
                    }
                    inlines.push(fragment);
                }
            }
            current.inlines = inlines.into();
            current.id = block(current.kind.clone(), current.source.clone()).id;
        }
        Ok(Self(Arc::new(DocumentSnapshot {
            source: TextDocument::new(self.source().text()),
            format: self.format(),
            blocks,
            references: false,
        })))
    }

    #[cfg(feature = "document-editing")]
    pub fn with_rich_heading(&self, byte: usize, level: u8) -> Self {
        let mut blocks = self.blocks().to_vec();
        if let Some(index) = self.block_at(byte) {
            blocks[index].kind = if level == 0 {
                DocumentBlockKind::Paragraph
            } else {
                DocumentBlockKind::Heading(level.min(6))
            };
            blocks[index].id = block(blocks[index].kind.clone(), blocks[index].source.clone()).id;
        }
        Self(Arc::new(DocumentSnapshot {
            source: TextDocument::new(self.source().text()),
            format: self.format(),
            blocks,
            references: false,
        }))
    }
}

pub(super) fn replace_rich(
    document: &Document,
    source: TextDocument,
    range: Range<usize>,
    insert: &str,
) -> Document {
    let delta = insert.len() as isize - range.len() as isize;
    let mut runs = Vec::new();
    let mut inserted_style = DocumentInlineStyle::default();
    let mut inserted_kind = DocumentInlineKind::Text;
    let mut kinds = std::collections::BTreeMap::new();
    for current in document.blocks() {
        let start = current.source.start;
        if start <= range.start {
            kinds.insert(start, current.kind.clone());
        } else if start >= range.end {
            kinds.insert(start.saturating_add_signed(delta), current.kind.clone());
        }
        for inline in current.inlines.iter() {
            let absolute = start + inline.source.start..start + inline.source.end;
            if absolute.start <= range.start && absolute.end >= range.start {
                inserted_style = inline.style.clone();
                inserted_kind = inline.kind.clone();
            }
            let before = absolute.start..absolute.end.min(range.start);
            if before.start < before.end {
                runs.push((before, inline.style.clone(), inline.kind.clone()));
            }
            let after = absolute.start.max(range.end)..absolute.end;
            if after.start < after.end {
                runs.push((
                    after.start.saturating_add_signed(delta)
                        ..after.end.saturating_add_signed(delta),
                    inline.style.clone(),
                    inline.kind.clone(),
                ));
            }
        }
    }
    if !insert.is_empty() {
        runs.push((
            range.start..range.start + insert.len(),
            inserted_style,
            inserted_kind,
        ));
    }
    runs.sort_by_key(|(range, _, _)| range.start);
    let mut merged: Vec<(Range<usize>, DocumentInlineStyle, DocumentInlineKind)> = Vec::new();
    for (range, style, kind) in runs {
        if let Some(last) = merged.last_mut() {
            if last.0.end == range.start && last.1 == style && last.2 == kind {
                last.0.end = range.end;
                continue;
            }
        }
        merged.push((range, style, kind));
    }
    let runs = merged;
    let mut result = Vec::with_capacity(source.line_count());
    let mut cursor = 0;
    let mut line = 0;
    while line < source.line_count() {
        let mut full = source.line_full_range(line).unwrap();
        let mut body = source.line_range(line).unwrap();
        let kind = kinds
            .get(&full.start)
            .cloned()
            .unwrap_or(if body.is_empty() {
                DocumentBlockKind::Blank
            } else {
                DocumentBlockKind::Paragraph
            });
        if matches!(
            kind,
            DocumentBlockKind::Math
                | DocumentBlockKind::Chemistry
                | DocumentBlockKind::Html
                | DocumentBlockKind::Molecule { .. }
        ) {
            full.end = kinds
                .range((
                    std::ops::Bound::Excluded(full.start),
                    std::ops::Bound::Unbounded,
                ))
                .next()
                .map(|(start, _)| *start)
                .unwrap_or(source.len());
            body.end = full.start
                + source.text()[full.clone()]
                    .trim_end_matches(['\r', '\n'])
                    .len();
            line = source
                .line_at_byte(full.end)
                .unwrap_or(source.line_count())
                .max(line + 1);
        } else {
            line += 1;
        }
        let mut current = block(kind, full);
        while cursor < runs.len() && runs[cursor].0.end <= body.start {
            cursor += 1;
        }
        let mut scan = cursor;
        let mut inlines = Vec::new();
        while scan < runs.len() && runs[scan].0.start < body.end {
            let (range, style, kind) = &runs[scan];
            let start = range.start.max(body.start);
            let end = range.end.min(body.end);
            if start < end {
                inlines.push(DocumentInline {
                    text: source.text()[start..end].into(),
                    source: start - current.source.start..end - current.source.start,
                    style: style.clone(),
                    kind: kind.clone(),
                });
            }
            scan += 1;
        }
        current.inlines = inlines.into();
        result.push(current);
    }
    Document(Arc::new(DocumentSnapshot {
        source,
        format: DocumentFormat::RichText,
        blocks: result,
        references: false,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rich_atomic_blocks_keep_multiline_source_and_surrounding_paragraphs() {
        let document = Document::rich_text([
            RichTextBlock::paragraph([RichTextSpan::new("before")]),
            RichTextBlock {
                kind: DocumentBlockKind::Chemistry,
                spans: vec![RichTextSpan::new("H2 + O2\n-> H2O")],
            },
            RichTextBlock::paragraph([RichTextSpan::new("after")]),
        ]);
        assert_eq!(document.blocks().len(), 3);
        assert_eq!(document.blocks()[1].text(), "H2 + O2\n-> H2O");
        let changed = document.replace(8..9, "3").unwrap();
        assert_eq!(changed.blocks().len(), 3);
        assert_eq!(changed.blocks()[2].text(), "after");
    }
    #[test]
    fn inline_math_survives_native_source_edits_and_json_roundtrip() {
        let document = Document::rich_text([RichTextBlock::paragraph([
            RichTextSpan::new("equation "),
            RichTextSpan::math("x^2"),
        ])]);
        let changed = document.replace(11..12, "3").unwrap();
        let math = changed.blocks()[0]
            .inlines
            .iter()
            .find(|inline| inline.kind == DocumentInlineKind::Math)
            .unwrap();
        assert_eq!(math.text, "x^3");
        let serialized = serde_json::to_string(&changed.rich_text_blocks()).unwrap();
        let restored =
            Document::rich_text(serde_json::from_str::<Vec<RichTextBlock>>(&serialized).unwrap());
        assert_eq!(
            restored.blocks()[0].inlines.last().unwrap().kind,
            DocumentInlineKind::Math
        );
    }
    #[test]
    fn rich_styles_survive_edits_and_serialization() {
        let style = DocumentInlineStyle {
            bold: true,
            color: Some([200, 10, 20, 255]),
            ..Default::default()
        };
        let document = Document::rich_text([RichTextBlock::paragraph([
            RichTextSpan::styled("hello", style.clone()),
            RichTextSpan::new(" world"),
        ])]);
        let changed = document.replace(2..3, "XX").unwrap();
        assert_eq!(changed.source().text(), "heXXlo world");
        assert!(changed.blocks()[0]
            .inlines
            .iter()
            .filter(|inline| inline.source.end <= 6)
            .all(|inline| inline.style == style));
        let serialized = serde_json::to_string(&changed.rich_text_blocks()).unwrap();
        let blocks: Vec<RichTextBlock> = serde_json::from_str(&serialized).unwrap();
        assert_eq!(
            Document::rich_text(blocks.clone()).source().text(),
            changed.source().text()
        );
        assert_eq!(
            Document::rich_text(blocks).blocks()[0].inlines[0]
                .style
                .color,
            style.color
        );
    }
}
