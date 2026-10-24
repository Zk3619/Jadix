//! Shared immutable UTF-8 snapshots. Cloning is O(1); construction and edits
//! are O(bytes + lines). Line content excludes LF and the CR in CRLF.

use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

static NEXT_REVISION: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
struct Snapshot {
    text: String,
    line_starts: Vec<usize>,
    max_line_width_units: usize,
    revision: u64,
}

/// Indexed source shared by viewers, diff models and background parsers.
#[derive(Clone, Debug)]
pub struct TextDocument(Arc<Snapshot>);

/// Snapshot identity equality is O(1), including when used by Signal setters.
impl PartialEq for TextDocument {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for TextDocument {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentEditError {
    InvalidRange,
    NotCharBoundary,
}

impl TextDocument {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let mut line_starts = vec![0];
        line_starts.extend(
            text.bytes()
                .enumerate()
                .filter_map(|(offset, byte)| (byte == b'\n').then_some(offset + 1)),
        );
        let max_line_width_units = text
            .split('\n')
            .map(|line| {
                line.bytes()
                    .map(|byte| if byte == b'\t' { 8 } else { 1 })
                    .sum()
            })
            .max()
            .unwrap_or(0);
        Self(Arc::new(Snapshot {
            text,
            line_starts,
            max_line_width_units,
            revision: NEXT_REVISION.fetch_add(1, Ordering::Relaxed),
        }))
    }

    pub fn text(&self) -> &str {
        &self.0.text
    }
    pub fn len(&self) -> usize {
        self.0.text.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.text.is_empty()
    }

    /// Cache identity, not a saved-file revision number.
    pub fn revision(&self) -> u64 {
        self.0.revision
    }

    /// Includes the empty document line and the empty line after trailing LF.
    pub fn line_count(&self) -> usize {
        self.0.line_starts.len()
    }

    /// Conservative width input: UTF-8 bytes, with eight units per tab.
    /// Not an exact pixel width or a Unicode display-column count.
    pub fn max_line_width_units(&self) -> usize {
        self.0.max_line_width_units
    }

    pub fn line_full_range(&self, line: usize) -> Option<Range<usize>> {
        let start = *self.0.line_starts.get(line)?;
        let end = self
            .0
            .line_starts
            .get(line + 1)
            .copied()
            .unwrap_or(self.len());
        Some(start..end)
    }

    pub fn line_range(&self, line: usize) -> Option<Range<usize>> {
        let mut range = self.line_full_range(line)?;
        if range.end > range.start && self.text().as_bytes()[range.end - 1] == b'\n' {
            range.end -= 1;
            if range.end > range.start && self.text().as_bytes()[range.end - 1] == b'\r' {
                range.end -= 1;
            }
        }
        Some(range)
    }

    pub fn line(&self, line: usize) -> Option<&str> {
        self.line_range(line).map(|range| &self.text()[range])
    }

    /// Maps an in-bounds UTF-8 byte boundary to a logical line, including EOF.
    pub fn line_at_byte(&self, offset: usize) -> Option<usize> {
        if offset > self.len() || !self.text().is_char_boundary(offset) {
            return None;
        }
        Some(self.0.line_starts.partition_point(|start| *start <= offset) - 1)
    }

    /// Returns a new snapshot without changing existing readers.
    pub fn replace(&self, range: Range<usize>, insert: &str) -> Result<Self, DocumentEditError> {
        if range.start > range.end || range.end > self.len() {
            return Err(DocumentEditError::InvalidRange);
        }
        if !self.text().is_char_boundary(range.start) || !self.text().is_char_boundary(range.end) {
            return Err(DocumentEditError::NotCharBoundary);
        }
        let mut text = String::with_capacity(self.len() - range.len() + insert.len());
        text.push_str(&self.text()[..range.start]);
        text.push_str(insert);
        text.push_str(&self.text()[range.end..]);
        Ok(Self::new(text))
    }
}

impl Default for TextDocument {
    fn default() -> Self {
        Self::new(String::new())
    }
}

impl From<String> for TextDocument {
    fn from(text: String) -> Self {
        Self::new(text)
    }
}

impl From<&str> for TextDocument {
    fn from(text: &str) -> Self {
        Self::new(text)
    }
}

impl std::fmt::Display for DocumentEditError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidRange => "document edit range is reversed or out of bounds",
            Self::NotCharBoundary => "document edit range splits a UTF-8 character",
        })
    }
}

impl std::error::Error for DocumentEditError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexes_empty_trailing_crlf_and_unicode_lines() {
        let document = TextDocument::new("alpha\r\n\u{4e2d}\u{6587}\n\n");
        assert_eq!(document.line_count(), 4);
        assert_eq!(document.line(0), Some("alpha"));
        assert_eq!(document.line(1), Some("\u{4e2d}\u{6587}"));
        assert_eq!(document.line(2), Some(""));
        assert_eq!(document.line(3), Some(""));
        assert_eq!(document.line(4), None);
        assert_eq!(document.line_full_range(0), Some(0..7));
        assert_eq!(document.line_at_byte(7), Some(1));
        assert_eq!(document.line_at_byte(8), None);
        assert_eq!(document.line_at_byte(document.len()), Some(3));
        assert_eq!(document.line_at_byte(document.len() + 1), None);
        assert_eq!(TextDocument::default().line(0), Some(""));
        assert_eq!(TextDocument::new("a\rb").line(0), Some("a\rb"));
    }

    #[test]
    fn replacement_preserves_snapshot_and_checks_byte_boundaries() {
        let original = TextDocument::new("a\n\u{4e2d}\u{6587}\n");
        assert!(Arc::ptr_eq(&original.0, &original.clone().0));
        let edited = original.replace(2..8, "next\nlast").unwrap();
        assert_eq!(original.text(), "a\n\u{4e2d}\u{6587}\n");
        assert_eq!(edited.text(), "a\nnext\nlast\n");
        assert_eq!(edited.line_count(), 4);
        assert_ne!(original.revision(), edited.revision());
        assert_eq!(
            original.replace(3..4, "").unwrap_err(),
            DocumentEditError::NotCharBoundary
        );
        let reversed = Range { start: 8, end: 2 };
        assert_eq!(
            original.replace(reversed, "").unwrap_err(),
            DocumentEditError::InvalidRange
        );
        assert_eq!(
            original.replace(0..99, "").unwrap_err(),
            DocumentEditError::InvalidRange
        );
    }
}
