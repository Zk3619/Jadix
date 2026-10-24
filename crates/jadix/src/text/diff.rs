//! Precomputed source-coordinate diffs. No diff work happens during painting.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use similar::{Algorithm, ChangeTag, TextDiff};

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
        Self(Arc::new(DiffSnapshot {
            revision: NEXT_DIFF_REVISION.fetch_add(1, Ordering::Relaxed),
            original,
            modified,
            unified,
            split,
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

    pub fn is_identical(&self) -> bool {
        self.0
            .unified
            .iter()
            .all(|row| row.kind == DiffLineKind::Equal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unified_and_split_keep_source_line_coordinates() {
        let diff = DiffDocument::new("a\nb\nc\n", "a\nx\ny\nc\n");
        assert_eq!(diff.rows(DiffMode::Unified).len(), 5);
        let split = diff.rows(DiffMode::Split);
        assert_eq!(split.len(), 4);
        assert_eq!(
            split[1],
            DiffRow {
                kind: DiffLineKind::Replace,
                original_line: Some(1),
                modified_line: Some(1)
            }
        );
        assert_eq!(
            split[2],
            DiffRow {
                kind: DiffLineKind::Insert,
                original_line: None,
                modified_line: Some(2)
            }
        );
        assert_eq!(split[3].original_line, Some(2));
        assert_eq!(split[3].modified_line, Some(3));
        assert!(!diff.is_identical());
    }

    #[test]
    fn empty_sources_and_final_newline_are_not_phantom_lines() {
        assert!(DiffDocument::new("", "").rows(DiffMode::Unified).is_empty());
        assert_eq!(DiffDocument::new("", "\n").rows(DiffMode::Split).len(), 1);
        assert!(!DiffDocument::new("a", "a\n").is_identical());
        assert!(!DiffDocument::new("a\r\n", "a\n").is_identical());
        assert!(DiffDocument::new("\u{4e2d}\n", "\u{4e2d}\n").is_identical());
        let removed = DiffDocument::new("a\nb\n", "");
        assert!(removed
            .rows(DiffMode::Split)
            .iter()
            .all(|row| row.modified_line.is_none()));
    }

    #[test]
    fn rows_reconstruct_both_sources_including_line_endings() {
        let originals = [
            "",
            "a",
            "a\n",
            "a\r\n\n",
            "a\rb\rc",
            "a\nb\na\n",
            "\u{4e2d}\n\u{6587}",
        ];
        for original in originals {
            for modified in originals {
                let diff = DiffDocument::new(original, modified);
                for mode in [DiffMode::Unified, DiffMode::Split] {
                    let old: String = diff
                        .rows(mode)
                        .iter()
                        .filter_map(|row| row.original_line)
                        .map(|line| {
                            &diff.original().text()[diff.original().line_full_range(line).unwrap()]
                        })
                        .collect();
                    let new: String = diff
                        .rows(mode)
                        .iter()
                        .filter_map(|row| row.modified_line)
                        .map(|line| {
                            &diff.modified().text()[diff.modified().line_full_range(line).unwrap()]
                        })
                        .collect();
                    assert_eq!(old, original);
                    assert_eq!(new, modified);
                }
            }
        }
    }
}
