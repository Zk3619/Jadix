//! Document-coordinate editing, selection, search, formatting and bounded delta history.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::ops::Range;
use std::rc::Rc;

use super::{Document, DocumentEditError, DocumentFormat, DocumentInlineStyle};
use crate::core::reactivity::Signal;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DocumentSelection {
    pub anchor: usize,
    pub caret: usize,
}

impl DocumentSelection {
    pub fn caret(byte: usize) -> Self {
        Self {
            anchor: byte,
            caret: byte,
        }
    }
    pub fn range(self) -> Range<usize> {
        self.anchor.min(self.caret)..self.anchor.max(self.caret)
    }
    pub fn is_empty(self) -> bool {
        self.anchor == self.caret
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DocumentEditMode {
    #[default]
    LivePreview,
    Source,
    Preview,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentMark {
    Bold,
    Italic,
    Code,
    Strike,
    Math,
}

impl DocumentMark {
    fn delimiter(self) -> &'static str {
        match self {
            Self::Bold => "**",
            Self::Italic => "*",
            Self::Code => "`",
            Self::Strike => "~~",
            Self::Math => "$",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentEditorSnapshot {
    pub document: Document,
    pub selection: DocumentSelection,
    pub mode: DocumentEditMode,
    pub editing: bool,
    pub epoch: u64,
    pub typing_style: Option<DocumentInlineStyle>,
}

#[derive(Clone, Debug)]
struct Edit {
    start: usize,
    removed: String,
    inserted: String,
    before: DocumentSelection,
    after: DocumentSelection,
    documents: Option<(Document, Document)>,
}

impl Edit {
    fn bytes(&self) -> usize {
        self.documents
            .as_ref()
            .map(|(before, after)| before.source().len() + after.source().len())
            .unwrap_or(self.removed.len() + self.inserted.len())
    }
}

#[derive(Default)]
struct History {
    undo: VecDeque<Edit>,
    redo: Vec<Edit>,
    bytes: usize,
}

struct Session {
    snapshot: Signal<DocumentEditorSnapshot>,
    history: RefCell<History>,
    max_bytes: usize,
    max_entries: usize,
}

#[derive(Clone)]
pub struct DocumentEditorState(Rc<Session>);

impl PartialEq for DocumentEditorState {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for DocumentEditorState {}
impl Default for DocumentEditorState {
    fn default() -> Self {
        Self::new(Document::default())
    }
}

impl DocumentEditorState {
    pub fn new(document: impl Into<Document>) -> Self {
        Self::with_history_limit(document, 1000, 8 * 1024 * 1024)
    }

    pub fn with_history_limit(document: impl Into<Document>, entries: usize, bytes: usize) -> Self {
        Self(Rc::new(Session {
            snapshot: Signal::new(DocumentEditorSnapshot {
                document: document.into(),
                selection: DocumentSelection::default(),
                mode: DocumentEditMode::LivePreview,
                editing: false,
                epoch: 0,
                typing_style: None,
            }),
            history: RefCell::new(History::default()),
            max_bytes: bytes,
            max_entries: entries,
        }))
    }

    pub fn signal(&self) -> Signal<DocumentEditorSnapshot> {
        self.0.snapshot
    }
    pub fn snapshot(&self) -> DocumentEditorSnapshot {
        self.0.snapshot.peek()
    }
    pub fn document(&self) -> Document {
        self.snapshot().document
    }
    pub fn selection(&self) -> DocumentSelection {
        self.snapshot().selection
    }
    pub fn can_undo(&self) -> bool {
        !self.0.history.borrow().undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.0.history.borrow().redo.is_empty()
    }
    pub fn history_bytes(&self) -> usize {
        self.0.history.borrow().bytes
    }

    pub fn set_document(&self, document: impl Into<Document>) {
        let mut snapshot = self.snapshot();
        snapshot.document = document.into();
        let caret = snapshot.document.source().text().floor_char_boundary(
            snapshot
                .selection
                .caret
                .min(snapshot.document.source().len()),
        );
        snapshot.selection = DocumentSelection::caret(caret);
        snapshot.epoch += 1;
        snapshot.typing_style = None;
        *self.0.history.borrow_mut() = History::default();
        self.publish(snapshot, false);
    }

    pub fn set_mode(&self, mode: DocumentEditMode) {
        let mut snapshot = self.snapshot();
        snapshot.mode = mode;
        snapshot.epoch += 1;
        if mode == DocumentEditMode::Preview {
            snapshot.editing = false;
        } else if mode == DocumentEditMode::Source {
            snapshot.editing = true;
        }
        self.publish(snapshot, false);
    }

    pub fn set_selection(&self, selection: DocumentSelection) -> Result<(), DocumentEditError> {
        self.select(selection, false)
    }

    pub(crate) fn select(
        &self,
        selection: DocumentSelection,
        silent: bool,
    ) -> Result<(), DocumentEditError> {
        let mut snapshot = self.snapshot();
        validate(&snapshot.document, selection.range())?;
        snapshot.selection = selection;
        snapshot.editing = true;
        snapshot.epoch += 1;
        snapshot.typing_style = None;
        self.publish(snapshot, silent);
        Ok(())
    }

    pub fn finish_editing(&self) {
        let mut snapshot = self.snapshot();
        snapshot.editing = false;
        snapshot.epoch += 1;
        self.publish(snapshot, false);
    }

    pub fn selected_text(&self) -> String {
        let snapshot = self.snapshot();
        snapshot.document.source().text()[snapshot.selection.range()].to_string()
    }

    pub fn replace_selection(&self, insert: &str) -> Result<(), DocumentEditError> {
        self.replace(self.selection().range(), insert)
    }

    pub fn replace(&self, range: Range<usize>, insert: &str) -> Result<(), DocumentEditError> {
        let after = DocumentSelection::caret(range.start + insert.len());
        self.apply(range, insert, after, false)
    }

    pub(crate) fn apply(
        &self,
        range: Range<usize>,
        insert: &str,
        after: DocumentSelection,
        silent: bool,
    ) -> Result<(), DocumentEditError> {
        let mut snapshot = self.snapshot();
        validate(&snapshot.document, range.clone())?;
        let mut document = snapshot.document.replace(range.clone(), insert)?;
        if document.format() == DocumentFormat::RichText && !insert.is_empty() {
            if let Some(style) = &snapshot.typing_style {
                document = document
                    .with_inline_style(range.start..range.start + insert.len(), |target| {
                        *target = style.clone()
                    })?;
            }
        }
        validate(&document, after.range())?;
        if snapshot.document.source().text()[range.clone()] != *insert {
            let edit = Edit {
                start: range.start,
                removed: snapshot.document.source().text()[range].to_string(),
                inserted: insert.into(),
                before: snapshot.selection,
                after,
                documents: (document.format() == DocumentFormat::RichText)
                    .then(|| (snapshot.document.clone(), document.clone())),
            };
            self.record(edit);
        }
        snapshot.document = document;
        snapshot.selection = after;
        snapshot.editing = true;
        snapshot.epoch += 1;
        self.publish(snapshot, silent);
        Ok(())
    }

    fn record(&self, edit: Edit) {
        let mut history = self.0.history.borrow_mut();
        let redo_bytes: usize = history.redo.iter().map(Edit::bytes).sum();
        history.bytes = history.bytes.saturating_sub(redo_bytes);
        history.redo.clear();
        if edit.bytes() > self.0.max_bytes || self.0.max_entries == 0 {
            *history = History::default();
        } else {
            history.bytes += edit.bytes();
            history.undo.push_back(edit);
            while history.undo.len() > self.0.max_entries || history.bytes > self.0.max_bytes {
                if let Some(old) = history.undo.pop_front() {
                    history.bytes -= old.bytes();
                } else {
                    break;
                }
            }
        }
    }

    fn format_document(&self, document: Document) {
        let mut snapshot = self.snapshot();
        self.record(Edit {
            start: 0,
            removed: String::new(),
            inserted: String::new(),
            before: snapshot.selection,
            after: snapshot.selection,
            documents: Some((snapshot.document.clone(), document.clone())),
        });
        snapshot.document = document;
        snapshot.epoch += 1;
        self.publish(snapshot, false);
    }

    pub fn undo(&self) -> bool {
        self.history_step(false)
    }
    pub fn redo(&self) -> bool {
        self.history_step(true)
    }

    fn history_step(&self, redo: bool) -> bool {
        let mut snapshot = self.snapshot();
        let edit = {
            let mut history = self.0.history.borrow_mut();
            if redo {
                history.redo.pop()
            } else {
                history.undo.pop_back()
            }
        };
        let Some(edit) = edit else { return false };
        let (old, replacement, selection) = if redo {
            (&edit.removed, &edit.inserted, edit.after)
        } else {
            (&edit.inserted, &edit.removed, edit.before)
        };
        let range = edit.start..edit.start + old.len();
        let document = if let Some((before, after)) = &edit.documents {
            if redo {
                after.clone()
            } else {
                before.clone()
            }
        } else {
            snapshot
                .document
                .replace(range, replacement)
                .expect("history ranges belong to the current document")
        };
        snapshot.document = document;
        snapshot.selection = selection;
        snapshot.editing = true;
        snapshot.epoch += 1;
        {
            let mut history = self.0.history.borrow_mut();
            if redo {
                history.undo.push_back(edit);
            } else {
                history.redo.push(edit);
            }
        }
        self.publish(snapshot, false);
        true
    }

    pub fn toggle_mark(&self, mark: DocumentMark) -> Result<(), DocumentEditError> {
        let snapshot = self.snapshot();
        let range = snapshot.selection.range();
        let delimiter = mark.delimiter();
        if snapshot.document.format() == DocumentFormat::RichText {
            if mark == DocumentMark::Math {
                let is_math = snapshot
                    .document
                    .blocks()
                    .iter()
                    .flat_map(|block| {
                        block
                            .inlines
                            .iter()
                            .map(move |inline| (block.source.start, inline))
                    })
                    .filter(|(start, inline)| {
                        start + inline.source.start < range.end
                            && start + inline.source.end > range.start
                    })
                    .all(|(_, inline)| inline.kind == super::DocumentInlineKind::Math);
                let kind = if is_math {
                    super::DocumentInlineKind::Text
                } else {
                    super::DocumentInlineKind::Math
                };
                self.format_document(snapshot.document.with_inline_kind(range, kind)?);
                return Ok(());
            }
            let value = |style: &DocumentInlineStyle| match mark {
                DocumentMark::Bold => style.bold,
                DocumentMark::Italic => style.italic,
                DocumentMark::Code => style.code,
                DocumentMark::Strike => style.strike,
                DocumentMark::Math => false,
            };
            let apply = |style: &mut DocumentInlineStyle, enabled: bool| match mark {
                DocumentMark::Bold => style.bold = enabled,
                DocumentMark::Italic => style.italic = enabled,
                DocumentMark::Code => style.code = enabled,
                DocumentMark::Strike => style.strike = enabled,
                DocumentMark::Math => {}
            };
            if range.is_empty() {
                let mut updated = snapshot.clone();
                let mut style = snapshot.typing_style.clone().unwrap_or_default();
                let enabled = !value(&style);
                apply(&mut style, enabled);
                updated.typing_style = Some(style);
                updated.epoch += 1;
                self.publish(updated, false);
                return Ok(());
            }
            let enabled = !snapshot
                .document
                .blocks()
                .iter()
                .flat_map(|block| {
                    block
                        .inlines
                        .iter()
                        .map(move |inline| (block.source.start, inline))
                })
                .filter(|(start, inline)| {
                    start + inline.source.start < range.end
                        && start + inline.source.end > range.start
                })
                .all(|(_, inline)| value(&inline.style));
            let document = snapshot
                .document
                .with_inline_style(range, |style| apply(style, enabled))?;
            self.format_document(document);
            return Ok(());
        }
        let text = snapshot.document.source().text();
        if range.start >= delimiter.len()
            && text[..range.start].ends_with(delimiter)
            && text[range.end..].starts_with(delimiter)
        {
            let start = range.start - delimiter.len();
            let selected = &text[range.clone()];
            self.apply(
                start..range.end + delimiter.len(),
                selected,
                DocumentSelection {
                    anchor: start,
                    caret: start + selected.len(),
                },
                false,
            )
        } else {
            let selected = &text[range.clone()];
            let replacement = format!("{delimiter}{selected}{delimiter}");
            let start = range.start + delimiter.len();
            self.apply(
                range,
                &replacement,
                DocumentSelection {
                    anchor: start,
                    caret: start + selected.len(),
                },
                false,
            )
        }
    }

    pub fn set_heading(&self, level: u8) -> Result<(), DocumentEditError> {
        let snapshot = self.snapshot();
        let source = snapshot.document.source();
        if snapshot.document.format() == DocumentFormat::RichText {
            self.format_document(
                snapshot
                    .document
                    .with_rich_heading(snapshot.selection.caret, level),
            );
            return Ok(());
        }
        let line = source
            .line_at_byte(snapshot.selection.caret)
            .ok_or(DocumentEditError::InvalidRange)?;
        let range = source.line_range(line).unwrap();
        let text = &source.text()[range.clone()];
        let hashes = text.bytes().take_while(|byte| *byte == b'#').count();
        let prefix = if (1..=6).contains(&hashes) && text.as_bytes().get(hashes) == Some(&b' ') {
            hashes + 1
        } else {
            0
        };
        let marker = if level == 0 {
            String::new()
        } else {
            format!("{} ", "#".repeat(level.min(6) as usize))
        };
        let caret = range.start
            + marker.len()
            + snapshot
                .selection
                .caret
                .saturating_sub(range.start + prefix);
        self.apply(
            range.start..range.start + prefix,
            &marker,
            DocumentSelection::caret(caret),
            false,
        )
    }

    pub fn insert_link(&self, destination: &str) -> Result<(), DocumentEditError> {
        let snapshot = self.snapshot();
        if snapshot.document.format() == DocumentFormat::RichText {
            if snapshot.selection.is_empty() {
                self.replace_selection("link")?;
            }
            let range = if snapshot.selection.is_empty() {
                snapshot.selection.caret..snapshot.selection.caret + 4
            } else {
                snapshot.selection.range()
            };
            let document = self
                .document()
                .with_inline_style(range, |style| style.link = Some(destination.into()))?;
            self.format_document(document);
            return Ok(());
        }
        let label = self.selected_text();
        let label = if label.is_empty() { "link" } else { &label };
        let destination = destination
            .replace('\\', "\\\\")
            .replace('(', "\\(")
            .replace(')', "\\)")
            .replace(['\n', '\r'], "");
        self.replace_selection(&format!("[{label}]({destination})"))
    }

    pub fn find_next(&self, query: &str, backwards: bool, wrap: bool) -> Option<Range<usize>> {
        if query.is_empty() {
            return None;
        }
        let snapshot = self.snapshot();
        let text = snapshot.document.source().text();
        let selection = snapshot.selection.range();
        let found = if backwards {
            text[..selection.start]
                .rfind(query)
                .or_else(|| wrap.then(|| text.rfind(query)).flatten())
        } else {
            text[selection.end..]
                .find(query)
                .map(|start| selection.end + start)
                .or_else(|| wrap.then(|| text.find(query)).flatten())
        }?;
        let range = found..found + query.len();
        self.set_selection(DocumentSelection {
            anchor: range.start,
            caret: range.end,
        })
        .ok()?;
        Some(range)
    }

    pub fn replace_all(&self, query: &str, replacement: &str) -> Result<usize, DocumentEditError> {
        if query.is_empty() {
            return Ok(0);
        }
        let document = self.document();
        let text = document.source().text();
        let count = text.matches(query).count();
        if count != 0 {
            self.replace(0..text.len(), &text.replace(query, replacement))?;
        }
        Ok(count)
    }

    fn publish(&self, snapshot: DocumentEditorSnapshot, silent: bool) {
        if silent {
            self.0.snapshot.set_silent(snapshot);
        } else {
            self.0.snapshot.set(snapshot);
        }
        crate::app::runtime::request_redraw();
    }
}

fn validate(document: &Document, range: Range<usize>) -> Result<(), DocumentEditError> {
    if range.start > range.end || range.end > document.source().len() {
        return Err(DocumentEditError::InvalidRange);
    }
    if !document.source().text().is_char_boundary(range.start)
        || !document.source().text().is_char_boundary(range.end)
    {
        return Err(DocumentEditError::NotCharBoundary);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_selection_edit_and_history_use_document_bytes() {
        let state = DocumentEditorState::new("one\n\u{4e2d}\u{6587}\nthree");
        state
            .set_selection(DocumentSelection {
                anchor: 4,
                caret: 10,
            })
            .unwrap();
        assert_eq!(state.selected_text(), "\u{4e2d}\u{6587}");
        state.replace_selection("two").unwrap();
        assert_eq!(state.document().source().text(), "one\ntwo\nthree");
        assert!(state.undo());
        assert_eq!(state.selected_text(), "\u{4e2d}\u{6587}");
        assert!(state.redo());
        assert_eq!(state.selection(), DocumentSelection::caret(7));
        assert!(state.set_selection(DocumentSelection::caret(99)).is_err());
    }

    #[test]
    fn formatting_and_find_are_undoable() {
        let state = DocumentEditorState::new("hello world\n");
        state
            .set_selection(DocumentSelection {
                anchor: 0,
                caret: 5,
            })
            .unwrap();
        state.toggle_mark(DocumentMark::Bold).unwrap();
        assert_eq!(state.document().source().text(), "**hello** world\n");
        state.toggle_mark(DocumentMark::Bold).unwrap();
        assert_eq!(state.document().source().text(), "hello world\n");
        state.set_heading(2).unwrap();
        assert!(state.document().source().text().starts_with("## "));
        assert_eq!(state.find_next("world", false, true), Some(9..14));
        assert_eq!(state.replace_all("world", "earth").unwrap(), 1);
        assert!(state.undo());
        assert!(state.document().source().text().contains("world"));
    }

    #[test]
    fn history_is_bounded_and_redo_is_cleared_after_branching() {
        let state = DocumentEditorState::with_history_limit("", 2, 8);
        for text in ["a", "b", "c"] {
            state.replace_selection(text).unwrap();
        }
        assert!(state.history_bytes() <= 8);
        assert!(state.undo());
        assert!(state.undo());
        assert!(!state.undo());
        assert_eq!(state.document().source().text(), "a");
        state.replace_selection("z").unwrap();
        assert!(!state.redo());
        state.replace(0..2, "longer than budget").unwrap();
        assert!(!state.can_undo());
    }
}
