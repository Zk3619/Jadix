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
#[path = "tests/diff.rs"]
mod tests;
