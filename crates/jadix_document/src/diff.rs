//! Precomputed source-coordinate diffs. No diff work happens during painting.

use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use similar::{Algorithm, ChangeTag, TextDiff};
use unicode_segmentation::UnicodeSegmentation;

use super::TextDocument;

static NEXT_DIFF_REVISION: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DiffMode {
    #[default]
    Unified,
    Split,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffLineKind {
    Equal,
    Delete,
    Insert,
    Replace,
}

/// Zero-based logical source lines; missing sides are gaps, not empty lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiffRow {
    pub kind: DiffLineKind,
    pub original_line: Option<usize>,
    pub modified_line: Option<usize>,
}

#[derive(Debug)]
struct DiffSnapshot {
    revision: u64,
    original: TextDocument,
    modified: TextDocument,
    unified: Vec<DiffRow>,
    split: Vec<DiffRow>,
    original_inline: Vec<Range<usize>>,
    modified_inline: Vec<Range<usize>>,
    coarse_inline: bool,
}

/// O(1) clone. Build on a worker before publishing large comparisons to a view.
#[derive(Clone, Debug)]
pub struct DiffDocument(Arc<DiffSnapshot>);

impl PartialEq for DiffDocument {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for DiffDocument {}

impl Default for DiffDocument {
    fn default() -> Self {
        Self::new("", "")
    }
}

impl DiffDocument {
    /// Patience line diff with a 200 ms algorithm budget. A budget overrun may
    /// produce a coarser, but still source-correct diff; indexing is not timed.
    pub fn new(original: impl Into<TextDocument>, modified: impl Into<TextDocument>) -> Self {
        let original = original.into();
        let modified = modified.into();
        let original_lines: Vec<_> = original.text().split_inclusive('\n').collect();
        let modified_lines: Vec<_> = modified.text().split_inclusive('\n').collect();
        let unified: Vec<_> = TextDiff::configure()
            .algorithm(Algorithm::Patience)
            .timeout(Duration::from_millis(200))
            .diff_slices(&original_lines, &modified_lines)
            .iter_all_changes()
            .map(|change| DiffRow {
                kind: match change.tag() {
                    ChangeTag::Equal => DiffLineKind::Equal,
                    ChangeTag::Delete => DiffLineKind::Delete,
                    ChangeTag::Insert => DiffLineKind::Insert,
                },
                original_line: change.old_index(),
                modified_line: change.new_index(),
            })
            .collect();
        let mut split = Vec::with_capacity(unified.len());
        let mut cursor = 0;
        while cursor < unified.len() {
            if unified[cursor].kind == DiffLineKind::Equal {
                split.push(unified[cursor]);
                cursor += 1;
                continue;
            }
            let start = cursor;
            while cursor < unified.len() && unified[cursor].kind != DiffLineKind::Equal {
                cursor += 1;
            }
            let removed: Vec<_> = unified[start..cursor]
                .iter()
                .filter_map(|row| row.original_line)
                .collect();
            let added: Vec<_> = unified[start..cursor]
                .iter()
                .filter_map(|row| row.modified_line)
                .collect();
            for index in 0..removed.len().max(added.len()) {
                let original_line = removed.get(index).copied();
                let modified_line = added.get(index).copied();
                let kind = match (original_line, modified_line) {
                    (Some(_), Some(_)) => DiffLineKind::Replace,
                    (Some(_), None) => DiffLineKind::Delete,
                    _ => DiffLineKind::Insert,
                };
                split.push(DiffRow {
                    kind,
                    original_line,
                    modified_line,
                });
            }
        }
        let (original_inline, modified_inline, coarse_inline) =
            inline_changes(&original, &modified, &split);
        Self(Arc::new(DiffSnapshot {
            revision: NEXT_DIFF_REVISION.fetch_add(1, Ordering::Relaxed),
            original,
            modified,
            unified,
            split,
            original_inline,
            modified_inline,
            coarse_inline,
        }))
    }

    pub fn original(&self) -> &TextDocument {
        &self.0.original
    }

    pub fn revision(&self) -> u64 {
        self.0.revision
    }
    pub fn modified(&self) -> &TextDocument {
        &self.0.modified
    }

    pub fn rows(&self, mode: DiffMode) -> &[DiffRow] {
        match mode {
            DiffMode::Unified => &self.0.unified,
            DiffMode::Split => &self.0.split,
        }
    }

    /// Precomputed changes within positionally paired replacement lines, in
    /// absolute UTF-8 source bytes. Ranges never split a grapheme or include CRLF.
    /// Unpaired insertions/deletions retain their existing whole-line treatment.
    pub fn original_inline_changes(&self) -> &[Range<usize>] {
        &self.0.original_inline
    }

    /// Modified-side counterpart of [`Self::original_inline_changes`].
    pub fn modified_inline_changes(&self) -> &[Range<usize>] {
        &self.0.modified_inline
    }

    /// True if any paired line exceeded the inline work budget and was marked
    /// as a whole line. This does not describe the separate line-diff timeout.
    pub fn has_coarse_inline_changes(&self) -> bool {
        self.0.coarse_inline
    }

    pub fn is_identical(&self) -> bool {
        self.0
            .unified
            .iter()
            .all(|row| row.kind == DiffLineKind::Equal)
    }
}

fn inline_changes(
    original: &TextDocument,
    modified: &TextDocument,
    rows: &[DiffRow],
) -> (Vec<Range<usize>>, Vec<Range<usize>>, bool) {
    let (mut old_ranges, mut new_ranges, mut coarse) = (Vec::new(), Vec::new(), false);
    let (mut bytes_left, mut work_left) = (256 * 1024, 1024 * 1024);
    for row in rows.iter().filter(|row| row.kind == DiffLineKind::Replace) {
        let old_range = original.line_range(row.original_line.unwrap()).unwrap();
        let new_range = modified.line_range(row.modified_line.unwrap()).unwrap();
        let old = &original.text()[old_range.clone()];
        let new = &modified.text()[new_range.clone()];
        if old == new {
            continue; // Only the line ending changed.
        }
        let bytes = old.len().saturating_add(new.len());
        let units = if bytes <= 8192 && bytes <= bytes_left {
            bytes_left -= bytes;
            Some((
                old.graphemes(true).collect::<Vec<_>>(),
                new.graphemes(true).collect::<Vec<_>>(),
            ))
        } else {
            None
        };
        let units = units.filter(|(old, new)| {
            let work = old
                .len()
                .saturating_add(1)
                .saturating_mul(new.len().saturating_add(1));
            if work > 65_536 || work > work_left {
                return false;
            }
            work_left -= work;
            true
        });
        // ponytail: positional line pairing with bounded grapheme refinement;
        // semantic line matching belongs in a separate diff algorithm if needed.
        let Some((old_units, new_units)) = units else {
            coarse = true;
            if !old_range.is_empty() {
                old_ranges.push(old_range);
            }
            if !new_range.is_empty() {
                new_ranges.push(new_range);
            }
            continue;
        };
        let diff = TextDiff::configure().diff_slices(&old_units, &new_units);
        let (mut old_offset, mut new_offset) = (old_range.start, new_range.start);
        for change in diff.iter_all_changes() {
            let length = change.value().len();
            match change.tag() {
                ChangeTag::Equal => {
                    old_offset += length;
                    new_offset += length;
                }
                ChangeTag::Delete => {
                    append_inline_range(&mut old_ranges, old_offset..old_offset + length);
                    old_offset += length;
                }
                ChangeTag::Insert => {
                    append_inline_range(&mut new_ranges, new_offset..new_offset + length);
                    new_offset += length;
                }
            }
        }
    }
    (old_ranges, new_ranges, coarse)
}

fn append_inline_range(ranges: &mut Vec<Range<usize>>, range: Range<usize>) {
    if let Some(last) = ranges.last_mut().filter(|last| last.end == range.start) {
        last.end = range.end;
    } else {
        ranges.push(range);
    }
}

#[cfg(test)]
#[path = "tests/diff.rs"]
mod tests;
