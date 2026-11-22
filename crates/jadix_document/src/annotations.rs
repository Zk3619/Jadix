//! UTF-8 byte annotations bound to an immutable text snapshot.

use crate::{DocumentEditError, TextDocument};
use std::ops::Range;

/// Smallest enclosing replacement. Disjoint transactions should keep each edit.
pub fn replacement_between<'a>(before: &str, after: &'a str) -> Option<(Range<usize>, &'a str)> {
    if before == after {
        return None;
    }
    let mut prefix = before
        .bytes()
        .zip(after.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    prefix = before.floor_char_boundary(prefix);
    let mut suffix = before[prefix..]
        .bytes()
        .rev()
        .zip(after[prefix..].bytes().rev())
        .take_while(|(a, b)| a == b)
        .count();
    while !before.is_char_boundary(before.len() - suffix)
        || !after.is_char_boundary(after.len() - suffix)
    {
        suffix -= 1;
    }
    Some((
        prefix..before.len() - suffix,
        &after[prefix..after.len() - suffix],
    ))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextAnnotation<T> {
    pub range: Range<usize>,
    pub value: T,
}

/// Ordered annotation layers. Later entries may override earlier entries when rendered.
/// Empty/reversed ranges are discarded; partial UTF-8 characters expand outwards.
#[derive(Clone, Debug)]
pub struct AnnotatedText<T> {
    document: TextDocument,
    annotations: Vec<TextAnnotation<T>>,
}

impl<T> AnnotatedText<T> {
    pub fn new(document: TextDocument, annotations: Vec<TextAnnotation<T>>) -> Self {
        let mut result = Self {
            document,
            annotations: Vec::new(),
        };
        result.set_annotations(annotations);
        result
    }

    pub fn document(&self) -> &TextDocument {
        &self.document
    }

    pub fn annotations(&self) -> &[TextAnnotation<T>] {
        &self.annotations
    }

    pub fn set_annotations(&mut self, annotations: Vec<TextAnnotation<T>>) {
        let text = self.document.text();
        self.annotations = annotations
            .into_iter()
            .filter_map(|mut annotation| {
                let range = &annotation.range;
                if range.start >= range.end || range.start >= text.len() {
                    return None;
                }
                annotation.range = text.floor_char_boundary(range.start)
                    ..text.ceil_char_boundary(range.end.min(text.len()));
                Some(annotation)
            })
            .collect();
    }

    /// Replacing the document clears annotations tied to the previous content.
    pub fn reset(&mut self, document: TextDocument) {
        self.document = document;
        self.annotations.clear();
    }

    /// Replace a validated UTF-8 range and move annotations in the same transaction.
    /// Insertions at a range's start stay outside it; insertions inside extend it;
    /// insertions at its end stay outside it. Full replacement inherits the range.
    /// A rejected edit leaves both text and annotations untouched.
    pub fn replace(&mut self, range: Range<usize>, insert: &str) -> Result<(), DocumentEditError> {
        self.replace_many(std::iter::once((range, insert)))
    }

    /// Applies sequential edit coordinates atomically, indexing the final snapshot once.
    /// Invalid later edits leave all preceding text and annotation changes unapplied.
    pub fn replace_many<'a>(
        &mut self,
        edits: impl IntoIterator<Item = (Range<usize>, &'a str)>,
    ) -> Result<(), DocumentEditError> {
        let mut edits = edits.into_iter().peekable();
        if edits.peek().is_none() {
            return Ok(());
        }
        let mut text = self.document.text().to_string();
        let mut ranges = Vec::new();
        for (range, insert) in edits {
            if range.start > range.end || range.end > text.len() {
                return Err(DocumentEditError::InvalidRange);
            }
            if !text.is_char_boundary(range.start) || !text.is_char_boundary(range.end) {
                return Err(DocumentEditError::NotCharBoundary);
            }
            text.replace_range(range.clone(), insert);
            ranges.push((range, insert.len()));
        }
        for (range, inserted) in ranges {
            self.annotations.retain_mut(|annotation| {
                annotation.range = range_after_edit(&annotation.range, &range, inserted);
                !annotation.range.is_empty()
            });
        }
        self.document = TextDocument::new(text);
        Ok(())
    }
}

fn range_after_edit(range: &Range<usize>, edit: &Range<usize>, inserted: usize) -> Range<usize> {
    let shift = |offset: usize| offset - edit.len() + inserted;
    if edit.is_empty() {
        let start = if range.start >= edit.start {
            shift(range.start)
        } else {
            range.start
        };
        let end = if range.end > edit.start {
            shift(range.end)
        } else {
            range.end
        };
        return start..end;
    }
    let start = if range.start <= edit.start {
        range.start
    } else if range.start >= edit.end {
        shift(range.start)
    } else {
        edit.start
    };
    let end = if range.end <= edit.start {
        range.end
    } else if range.end >= edit.end {
        shift(range.end)
    } else {
        edit.start + inserted
    };
    start..end
}

#[cfg(test)]
mod tests {
    use super::*;

    fn annotation(range: Range<usize>, value: u8) -> TextAnnotation<u8> {
        TextAnnotation { range, value }
    }

    #[test]
    fn unicode_annotations_follow_edits_and_reset_atomically() {
        let mut text = AnnotatedText::new(
            TextDocument::new("héllo world"),
            vec![
                annotation(2..4, 1),
                annotation(7..12, 2),
                annotation(100..200, 3),
                annotation(4..2, 4),
                annotation(2..2, 5),
            ],
        );
        assert_eq!(
            text.annotations(),
            &[annotation(1..4, 1), annotation(7..12, 2)]
        );
        text.replace(0..0, "ab").unwrap();
        text.replace(5..5, "x").unwrap();
        text.replace(9..14, "").unwrap();
        assert_eq!(text.document().text(), "abhéxllod");
        assert_eq!(
            text.annotations(),
            &[annotation(3..7, 1), annotation(9..10, 2)]
        );
        let before = text.document().clone();
        assert_eq!(
            text.replace(4..5, "!"),
            Err(DocumentEditError::NotCharBoundary)
        );
        assert_eq!(text.document(), &before);
        assert_eq!(
            text.annotations(),
            &[annotation(3..7, 1), annotation(9..10, 2)]
        );
        assert_eq!(
            text.replace_many([(0..0, "!"), (5..6, "?")]),
            Err(DocumentEditError::NotCharBoundary)
        );
        assert_eq!(text.document(), &before);
        assert_eq!(
            text.annotations(),
            &[annotation(3..7, 1), annotation(9..10, 2)]
        );
        text.reset(TextDocument::new(""));
        assert!(text.annotations().is_empty());
    }

    #[test]
    fn annotation_boundaries_and_disjoint_edits_preserve_the_middle() {
        let mut text =
            AnnotatedText::new(TextDocument::new("abcdefghij"), vec![annotation(2..6, 1)]);
        text.replace(2..2, "中").unwrap();
        assert_eq!(text.annotations()[0].range, 5..9);
        text.replace(9..9, "!").unwrap();
        assert_eq!(text.annotations()[0].range, 5..9);
        text.replace(5..9, "é").unwrap();
        assert_eq!(text.annotations()[0].range, 5..7);
        let end = text.document().len();
        text.replace_many([(0..1, "AA"), (end..end + 1, "尾")])
            .unwrap();
        assert_eq!(
            &text.document().text()[text.annotations()[0].range.clone()],
            "é"
        );
        text.replace(6..8, "").unwrap();
        assert!(text.annotations().is_empty());
    }
}
