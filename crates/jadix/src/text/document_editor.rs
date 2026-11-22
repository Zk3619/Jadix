//! Document-coordinate editing, selection, search, formatting and bounded delta history.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::ops::Range;
use std::rc::{Rc, Weak};

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentListKind {
    Bullet,
    Ordered,
}

#[derive(Clone, Copy)]
struct ListPrefixEdit {
    start: usize,
    end: usize,
    replacement_len: usize,
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

/// A synchronous paste stage. Stages run in registration order; `None` consumes
/// the paste (for example after an application handles it itself).
pub type DocumentPasteTransform = Rc<dyn Fn(&DocumentEditorSnapshot, String) -> Option<String>>;

/// Schema-aware paste stages. Applications may inspect, replace or consume an
/// imported rich-text fragment before it becomes one undoable edit.
pub type DocumentFragmentTransform =
    Rc<dyn Fn(&DocumentEditorSnapshot, Document) -> Option<Document>>;

/// Clipboard content after the host has chosen literal text or imported rich text.
#[derive(Clone, Debug)]
pub enum DocumentPasteContent {
    Text(String),
    RichText(Document),
}

/// A one-shot, UI-thread completion for application-owned asynchronous paste work.
/// Dropping it consumes the paste. It does not keep the editor session alive.
/// Edits, selection/mode changes, a newer paste or explicit cancellation fence
/// late results. Call `accept` or `complete` on the UI thread after the work ends.
pub struct DocumentPasteRequest {
    session: Weak<Session>,
    snapshot: DocumentEditorSnapshot,
    generation: u64,
    content: DocumentPasteContent,
    text_transforms: Vec<DocumentPasteTransform>,
    fragment_transforms: Vec<DocumentFragmentTransform>,
}

impl DocumentPasteRequest {
    pub fn snapshot(&self) -> &DocumentEditorSnapshot {
        &self.snapshot
    }

    pub fn content(&self) -> &DocumentPasteContent {
        &self.content
    }

    pub fn is_current(&self) -> bool {
        self.session.upgrade().is_some_and(|session| {
            session.snapshot.is_alive()
                && session.paste_generation.get() == self.generation
                && session.snapshot.peek().epoch == self.snapshot.epoch
        })
    }

    /// Replace the clipboard content, then apply the registered synchronous stages.
    pub fn complete(mut self, content: DocumentPasteContent) -> Result<bool, DocumentEditError> {
        self.content = content;
        self.accept()
    }

    /// Apply the original content as one undoable edit, if the request is still live.
    pub fn accept(self) -> Result<bool, DocumentEditError> {
        if !self.is_current() {
            return Ok(false);
        }
        let Some(session) = self.session.upgrade() else {
            return Ok(false);
        };
        let state = DocumentEditorState(session);
        match self.content {
            DocumentPasteContent::Text(text) => state.paste_text(&text, &self.text_transforms),
            DocumentPasteContent::RichText(fragment) => {
                state.paste_document(fragment, &self.fragment_transforms)
            }
        }
    }
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
    paste_generation: Cell<u64>,
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
            paste_generation: Cell::new(0),
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

    pub fn select(
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

    /// Fence pending paste completions without changing the document or history.
    /// Hosts call this when their editing surface unmounts or is replaced.
    pub fn cancel_pending_paste(&self) {
        self.0
            .paste_generation
            .set(self.0.paste_generation.get().wrapping_add(1));
    }

    /// Hand off a paste for later completion. No executor or background thread is
    /// created. Stages are retained and run only when the application completes
    /// the request; a new request supersedes the previous one, even if cancelled.
    pub fn defer_paste(
        &self,
        content: DocumentPasteContent,
        text_transforms: &[DocumentPasteTransform],
        fragment_transforms: &[DocumentFragmentTransform],
    ) -> Option<DocumentPasteRequest> {
        if !self.signal().is_alive() {
            return None;
        }
        self.cancel_pending_paste();
        let snapshot = self.snapshot();
        if snapshot.mode == DocumentEditMode::Preview {
            return None;
        }
        Some(DocumentPasteRequest {
            session: Rc::downgrade(&self.0),
            snapshot,
            generation: self.0.paste_generation.get(),
            content,
            text_transforms: text_transforms.to_vec(),
            fragment_transforms: fragment_transforms.to_vec(),
        })
    }

    /// Copy or cut the global selection through a host-provided clipboard writer.
    /// Rich text includes safe HTML outside source mode; unsupported HTML export
    /// falls back to plain text. A failed write or a stale cut never deletes text.
    /// Successful cuts form one undoable edit, preserving rich-text history.
    pub fn transfer_selection(
        &self,
        cut: bool,
        write: impl FnOnce(&str, Option<&str>) -> bool,
    ) -> Result<bool, DocumentEditError> {
        if !self.signal().is_alive() {
            return Ok(false);
        }
        let snapshot = self.snapshot();
        let range = snapshot.selection.range();
        if range.is_empty() || (cut && snapshot.mode == DocumentEditMode::Preview) {
            return Ok(false);
        }
        let html = (snapshot.mode != DocumentEditMode::Source
            && snapshot.document.format() == DocumentFormat::RichText)
            .then(|| snapshot.document.html_fragment(range.clone()).ok())
            .flatten();
        if !write(
            &snapshot.document.source().text()[range.clone()],
            html.as_deref(),
        ) {
            return Ok(false);
        }
        if cut {
            if !self.signal().is_alive() || self.snapshot().epoch != snapshot.epoch {
                return Ok(false);
            }
            self.replace(range, "")?;
        }
        Ok(true)
    }

    /// Transform a paste and insert it as one undoable document edit.
    ///
    /// Callbacks receive document coordinates, not the currently mounted line.
    /// A callback that changes the document, selection or mode consumes the old
    /// request: remaining stages and default insertion must not use stale state.
    /// Returns `false` for an empty, consumed, stale or read-only request.
    pub fn paste_text(
        &self,
        text: &str,
        transforms: &[DocumentPasteTransform],
    ) -> Result<bool, DocumentEditError> {
        self.cancel_pending_paste();
        let generation = self.0.paste_generation.get();
        if !self.signal().is_alive() || text.is_empty() {
            return Ok(false);
        }
        let snapshot = self.snapshot();
        if snapshot.mode == DocumentEditMode::Preview {
            return Ok(false);
        }
        let mut text = text.to_string();
        for transform in transforms {
            let Some(next) = transform(&snapshot, text) else {
                return Ok(false);
            };
            if !self.signal().is_alive()
                || self.0.paste_generation.get() != generation
                || self.snapshot().epoch != snapshot.epoch
            {
                return Ok(false);
            }
            text = next;
        }
        if text.is_empty() {
            return Ok(false);
        }
        self.replace(snapshot.selection.range(), &text)?;
        Ok(true)
    }

    /// Paste a rich-text fragment without flattening its styles or links.
    /// Both documents must be rich text. Stages run in order; a callback that
    /// changes editor state consumes the request, as in [`Self::paste_text`].
    pub fn paste_document(
        &self,
        mut fragment: Document,
        transforms: &[DocumentFragmentTransform],
    ) -> Result<bool, DocumentEditError> {
        self.cancel_pending_paste();
        let generation = self.0.paste_generation.get();
        if !self.signal().is_alive() {
            return Ok(false);
        }
        let mut snapshot = self.snapshot();
        if snapshot.mode == DocumentEditMode::Preview {
            return Ok(false);
        }
        for transform in transforms {
            let Some(next) = transform(&snapshot, fragment) else {
                return Ok(false);
            };
            if !self.signal().is_alive()
                || self.0.paste_generation.get() != generation
                || self.snapshot().epoch != snapshot.epoch
            {
                return Ok(false);
            }
            fragment = next;
        }
        if fragment.source().is_empty() {
            return Ok(false);
        }
        let range = snapshot.selection.range();
        let document = snapshot
            .document
            .replace_fragment(range.clone(), &fragment)?;
        let after = DocumentSelection::caret(range.start + fragment.source().len());
        self.record(Edit {
            start: range.start,
            removed: String::new(),
            inserted: String::new(),
            before: snapshot.selection,
            after,
            documents: Some((snapshot.document.clone(), document.clone())),
        });
        snapshot.document = document;
        snapshot.selection = after;
        snapshot.typing_style = None;
        snapshot.editing = true;
        snapshot.epoch += 1;
        self.publish(snapshot, false);
        Ok(true)
    }

    pub fn insert_newline(&self) -> Result<(), DocumentEditError> {
        self.insert_newline_with_list_continuation(true)
    }

    pub fn insert_newline_with_list_continuation(
        &self,
        continue_markdown_lists: bool,
    ) -> Result<(), DocumentEditError> {
        let snapshot = self.snapshot();
        let newline = if snapshot.document.source().text().contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        if !continue_markdown_lists
            || snapshot.document.format() != DocumentFormat::Markdown
            || !snapshot.selection.is_empty()
        {
            return self.replace_selection(newline);
        }

        let source = snapshot.document.source();
        let Some(line) = source.line_at_byte(snapshot.selection.caret) else {
            return self.replace_selection(newline);
        };
        let Some(range) = source.line_range(line) else {
            return self.replace_selection(newline);
        };
        let line_text = &source.text()[range.clone()];
        let Some(item) = markdown_list_continuation(line_text) else {
            return self.replace_selection(newline);
        };
        if snapshot.selection.caret < range.start + item.content_start {
            return self.replace_selection(newline);
        }

        if line_text[item.content_start..].trim().is_empty()
            && snapshot.selection.caret == range.end
        {
            let indent = line_text
                .as_bytes()
                .iter()
                .take_while(|byte| matches!(byte, b' ' | b'\t'))
                .count();
            self.replace(range.start + indent..snapshot.selection.caret, newline)
        } else {
            self.replace(
                snapshot.selection.caret..snapshot.selection.caret,
                &format!("{newline}{}", item.prefix),
            )
        }
    }

    pub fn replace(&self, range: Range<usize>, insert: &str) -> Result<(), DocumentEditError> {
        let after = DocumentSelection::caret(range.start + insert.len());
        self.apply(range, insert, after, false)
    }

    pub fn apply(
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

    /// Toggle a Markdown list over the selected lines. Non-Markdown documents are unchanged.
    pub fn toggle_list(&self, kind: DocumentListKind) -> Result<(), DocumentEditError> {
        let snapshot = self.snapshot();
        if snapshot.document.format() != DocumentFormat::Markdown {
            return Ok(());
        }

        let source = snapshot.document.source();
        let text = source.text();
        let selection = snapshot.selection;
        let selected = selection.range();
        let first = source
            .line_at_byte(selected.start)
            .ok_or(DocumentEditError::InvalidRange)?;
        let last_byte = if selected.is_empty() {
            selected.start
        } else if selected.end > 0 && text.as_bytes()[selected.end - 1] == b'\n' {
            selected.end - 1
        } else {
            selected.end
        };
        let last = source
            .line_at_byte(last_byte)
            .ok_or(DocumentEditError::InvalidRange)?;
        let all_target = (first..=last).all(|line| {
            let content = source.line(line).unwrap_or_default();
            let indent = content
                .bytes()
                .take_while(|byte| matches!(*byte, b' ' | b'\t'))
                .count();
            list_marker(&content[indent..]).is_some_and(|(existing, _)| existing == kind)
        });

        let mut replacement = String::new();
        let mut prefix_edits = Vec::with_capacity(last - first + 1);
        let mut ordered_index = 1;
        for line in first..=last {
            let full = source.line_full_range(line).unwrap();
            let content_range = source.line_range(line).unwrap();
            let content = &text[content_range.clone()];
            let indent = content
                .bytes()
                .take_while(|byte| matches!(*byte, b' ' | b'\t'))
                .count();
            let (marker_len, marker) = if all_target {
                (
                    list_marker(&content[indent..]).map_or(0, |(_, len)| len),
                    String::new(),
                )
            } else {
                let marker_len = list_marker(&content[indent..]).map_or(0, |(_, len)| len);
                let marker = match kind {
                    DocumentListKind::Bullet => "- ".to_string(),
                    DocumentListKind::Ordered => {
                        let marker = format!("{ordered_index}. ");
                        ordered_index += 1;
                        marker
                    }
                };
                (marker_len, marker)
            };
            let prefix_start = content_range.start + indent;
            let prefix_end = prefix_start + marker_len;
            replacement.push_str(&text[full.start..prefix_start]);
            replacement.push_str(&marker);
            replacement.push_str(&text[prefix_end..full.end]);
            prefix_edits.push(ListPrefixEdit {
                start: prefix_start,
                end: prefix_end,
                replacement_len: marker.len(),
            });
        }

        let map_offset = |offset: usize| {
            let mut delta = 0isize;
            for edit in &prefix_edits {
                if offset < edit.start {
                    break;
                }
                if offset <= edit.end {
                    return edit.start.saturating_add_signed(delta) + edit.replacement_len;
                }
                delta += edit.replacement_len as isize - (edit.end - edit.start) as isize;
            }
            offset.saturating_add_signed(delta)
        };
        let after = DocumentSelection {
            anchor: map_offset(selection.anchor),
            caret: map_offset(selection.caret),
        };
        let range =
            source.line_full_range(first).unwrap().start..source.line_full_range(last).unwrap().end;
        if text[range.clone()] == replacement {
            return Ok(());
        }
        self.apply(range, &replacement, after, false)
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

struct MarkdownListContinuation {
    prefix: String,
    content_start: usize,
}

fn markdown_list_continuation(line: &str) -> Option<MarkdownListContinuation> {
    let bytes = line.as_bytes();
    let indent = bytes
        .iter()
        .take_while(|byte| matches!(byte, b' ' | b'\t'))
        .count();
    let marker_start = indent;
    let (marker_end, next_marker) = match bytes.get(marker_start).copied()? {
        b'-' | b'+' | b'*' => (marker_start + 1, None),
        first if first.is_ascii_digit() => {
            let digits_end = bytes[marker_start..]
                .iter()
                .position(|byte| !byte.is_ascii_digit())?
                + marker_start;
            let delimiter = *bytes.get(digits_end)?;
            if !matches!(delimiter, b'.' | b')') {
                return None;
            }
            let number = line[marker_start..digits_end].parse::<u64>().ok()?;
            (
                digits_end + 1,
                Some(format!("{}{}", number.saturating_add(1), delimiter as char)),
            )
        }
        _ => return None,
    };
    if !bytes
        .get(marker_end)
        .is_some_and(|byte| matches!(byte, b' ' | b'\t'))
    {
        return None;
    }
    let mut content_start = marker_end;
    while bytes
        .get(content_start)
        .is_some_and(|byte| matches!(byte, b' ' | b'\t'))
    {
        content_start += 1;
    }

    let mut prefix = String::from(&line[..indent]);
    if let Some(next_marker) = next_marker {
        prefix.push_str(&next_marker);
    } else {
        prefix.push_str(&line[marker_start..marker_end]);
    }
    prefix.push_str(&line[marker_end..content_start]);

    if bytes
        .get(content_start..content_start + 3)
        .is_some_and(|task| {
            task[0] == b'[' && matches!(task[1], b' ' | b'x' | b'X') && task[2] == b']'
        })
    {
        let task_end = content_start + 3;
        if bytes
            .get(task_end)
            .is_some_and(|byte| matches!(byte, b' ' | b'\t'))
        {
            prefix.push_str("[ ]");
            content_start = task_end;
            while bytes
                .get(content_start)
                .is_some_and(|byte| matches!(byte, b' ' | b'\t'))
            {
                content_start += 1;
            }
            prefix.push_str(&line[task_end..content_start]);
        }
    }

    Some(MarkdownListContinuation {
        prefix,
        content_start,
    })
}

fn list_marker(text: &str) -> Option<(DocumentListKind, usize)> {
    let bytes = text.as_bytes();
    if bytes.len() >= 2
        && matches!(bytes[0], b'-' | b'+' | b'*')
        && matches!(bytes[1], b' ' | b'\t')
    {
        return Some((DocumentListKind::Bullet, 2));
    }
    let digits = bytes
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    if (1..=9).contains(&digits)
        && bytes
            .get(digits)
            .is_some_and(|byte| matches!(*byte, b'.' | b')'))
        && bytes
            .get(digits + 1)
            .is_some_and(|byte| matches!(*byte, b' ' | b'\t'))
    {
        return Some((DocumentListKind::Ordered, digits + 2));
    }
    None
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
#[path = "tests/document_editor.rs"]
mod tests;
