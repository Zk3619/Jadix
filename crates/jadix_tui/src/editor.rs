//! Host-neutral terminal editing over Jadix's immutable document snapshots.
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use jadix_document::document::TextDocument;
use std::{collections::VecDeque, io, ops::Range};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone)]
pub(super) struct Snapshot {
    pub(super) document: TextDocument,
    pub(super) anchor: usize,
    pub(super) caret: usize,
}

#[cfg(test)]
thread_local! {
    pub(super) static RETAINED_QUERIES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// UTF-8/grapheme editing, directional selection and bounded undo/redo. State
/// survives view rebuild, resize and focus loss. No fabricated IME preedit or
/// OS clipboard is implied; copy/cut/paste use the session-local clipboard.
#[derive(Clone)]
pub struct TerminalEditor {
    pub(super) current: Snapshot,
    pub(super) undo: VecDeque<Snapshot>,
    pub(super) redo: Vec<Snapshot>,
    clipboard: String,
    max_length: Option<usize>,
}
impl TerminalEditor {
    pub const MAX_BYTES: usize = 1024 * 1024;
    pub(super) const HISTORY_BYTES: usize = 8 * 1024 * 1024;
    pub(super) const HISTORY_ENTRIES: usize = 128;
    pub fn new(text: impl Into<String>) -> io::Result<Self> {
        let text = text.into();
        if text.len() > Self::MAX_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "editor exceeds 1 MiB",
            ));
        }
        Ok(Self {
            current: Snapshot {
                document: TextDocument::new(text),
                anchor: 0,
                caret: 0,
            },
            undo: VecDeque::new(),
            redo: Vec::new(),
            clipboard: String::new(),
            max_length: None,
        })
    }
    pub fn document(&self) -> &TextDocument {
        &self.current.document
    }
    pub fn caret(&self) -> usize {
        self.current.caret
    }
    pub fn anchor(&self) -> usize {
        self.current.anchor
    }
    pub fn selected_text(&self) -> &str {
        &self.document().text()[self.selection()]
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    /// User insertion limit in UTF-16 code units, matching DOM maxLength.
    /// Controlled values and undo history are never silently truncated.
    pub fn set_max_length(&mut self, limit: Option<usize>) -> io::Result<()> {
        if limit.is_some_and(|n| n > Self::MAX_BYTES) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "maxLength exceeds editor budget",
            ));
        }
        self.max_length = limit;
        Ok(())
    }
    pub fn max_length(&self) -> Option<usize> {
        self.max_length
    }
    /// Apply a controlled replacement only to the snapshot the caller read.
    /// Keep valid selection offsets, clear cross-document history, and retain
    /// the session-local clipboard. Equal text is a no-op, preserving history.
    pub fn apply_external(
        &mut self,
        expected_revision: u64,
        text: impl Into<String>,
    ) -> io::Result<bool> {
        self.check_revision(expected_revision)?;
        let text = text.into();
        if text.len() > Self::MAX_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "editor exceeds 1 MiB",
            ));
        }
        if self.document().text() == text {
            return Ok(false);
        }
        let boundary = |at: usize| {
            let mut at = at.min(text.len());
            while !text.is_char_boundary(at) {
                at -= 1;
            }
            at
        };
        self.current.anchor = boundary(self.anchor());
        self.current.caret = boundary(self.caret());
        self.current.document = TextDocument::new(text);
        self.undo.clear();
        self.redo.clear();
        Ok(true)
    }
    pub(super) fn check_revision(&self, expected: u64) -> io::Result<()> {
        if self.document().revision() != expected {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "document revision conflict; local edits preserved",
            ));
        }
        Ok(())
    }
    pub fn selection(&self) -> Range<usize> {
        self.current.anchor.min(self.current.caret)..self.current.anchor.max(self.current.caret)
    }
    pub fn history_bytes(&self) -> usize {
        self.undo
            .iter()
            .chain(self.redo.iter())
            .map(|s| s.document.len())
            .sum()
    }
    /// Conservative retained text bytes, counting shared history snapshots in
    /// full. Hosts can enforce a budget across several editors transactionally.
    pub fn retained_bytes(&self) -> usize {
        #[cfg(test)]
        RETAINED_QUERIES.with(|n| n.set(n.get() + 1));
        self.document().len() + self.history_bytes() + self.clipboard.len()
    }
    fn trim_history(&mut self) {
        while self.undo.len() + self.redo.len() > Self::HISTORY_ENTRIES
            || self.history_bytes() > Self::HISTORY_BYTES
        {
            if self.undo.pop_front().is_none() {
                self.redo.remove(0);
            }
        }
    }
    pub fn set_selection(&mut self, anchor: usize, caret: usize) -> io::Result<()> {
        let text = self.current.document.text();
        if [anchor, caret]
            .iter()
            .any(|i| *i > text.len() || !text.is_char_boundary(*i))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "selection splits UTF-8",
            ));
        }
        self.current.anchor = anchor;
        self.current.caret = caret;
        Ok(())
    }
    pub fn insert(&mut self, text: &str) -> io::Result<bool> {
        let insert = text.replace("\r\n", "\n").replace('\r', "\n");
        let insert: String = insert
            .chars()
            .filter(|c| *c == '\n' || *c == '\t' || !c.is_control())
            .collect();
        // Empty clipboard data (including filtered controls) is not a delete.
        if insert.is_empty() {
            return Ok(false);
        }
        let prefix = jadix_core::presentation::input::limited_insertion(
            self.document().text(),
            self.selection(),
            &insert,
            self.max_length,
        )
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "invalid insertion constraint")
        })?;
        let end = prefix.len();
        if !insert.is_empty() && end == 0 {
            return Ok(false);
        }
        self.replace(self.selection(), &insert[..end])
    }
    fn replace(&mut self, range: Range<usize>, insert: &str) -> io::Result<bool> {
        // ponytail: snapshot edits are O(bytes + lines), bounded to 1 MiB;
        // use a shared incremental document model if larger files are needed.
        if self.current.document.len() - (range.end - range.start) + insert.len() > Self::MAX_BYTES
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "editor exceeds 1 MiB",
            ));
        }
        if &self.current.document.text()[range.clone()] == insert {
            return Ok(false);
        }
        let document = self
            .current
            .document
            .replace(range.clone(), insert)
            .map_err(io::Error::other)?;
        self.undo.push_back(self.current.clone());
        self.redo.clear();
        self.current = Snapshot {
            document,
            caret: range.start + insert.len(),
            anchor: range.start + insert.len(),
        };
        self.trim_history();
        Ok(true)
    }
    fn move_to(&mut self, at: usize, select: bool) {
        self.current.caret = at;
        if !select {
            self.current.anchor = at;
        }
    }
    fn prev(&self) -> usize {
        self.current.document.text()[..self.caret()]
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(i, _)| i)
    }
    fn next(&self) -> usize {
        self.current.document.text()[self.caret()..]
            .graphemes(true)
            .next()
            .map_or(self.caret(), |g| self.caret() + g.len())
    }
    fn vertical(&self, delta: isize) -> usize {
        let d = self.document();
        let line = d.line_at_byte(self.caret()).unwrap();
        let start = d.line_range(line).unwrap().start;
        let column = d.text()[start..self.caret()].graphemes(true).count();
        let target = line.saturating_add_signed(delta).min(d.line_count() - 1);
        let range = d.line_range(target).unwrap();
        range.start
            + d.text()[range]
                .grapheme_indices(true)
                .nth(column)
                .map_or_else(|| d.line(target).unwrap().len(), |(i, _)| i)
    }
    /// Returns whether visible text/selection changed. Release is a no-op.
    pub fn handle(&mut self, event: &Event) -> io::Result<bool> {
        if let Event::Paste(text) = event {
            return self.insert(text);
        }
        let Event::Key(k) = event else {
            return Ok(false);
        };
        if k.kind == KeyEventKind::Release {
            return Ok(false);
        }
        let before = (self.caret(), self.anchor());
        let shift = k.modifiers.contains(KeyModifiers::SHIFT);
        let command = k
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::SUPER);
        if command {
            match k.code {
                KeyCode::Char('a') => self.set_selection(0, self.document().len())?,
                KeyCode::Char('c') => {
                    self.clipboard = self.document().text()[self.selection()].to_owned();
                    return Ok(false);
                }
                KeyCode::Char('x') => {
                    self.clipboard = self.document().text()[self.selection()].to_owned();
                    return self.replace(self.selection(), "");
                }
                KeyCode::Char('v') => return self.insert(&self.clipboard.clone()),
                KeyCode::Char('z') if !shift => {
                    if let Some(s) = self.undo.pop_back() {
                        self.redo.push(self.current.clone());
                        self.current = s;
                        self.trim_history();
                        return Ok(true);
                    }
                    return Ok(false);
                }
                KeyCode::Char('y') | KeyCode::Char('z') => {
                    if let Some(s) = self.redo.pop() {
                        self.undo.push_back(self.current.clone());
                        self.current = s;
                        self.trim_history();
                        return Ok(true);
                    }
                    return Ok(false);
                }
                KeyCode::Home => self.move_to(0, shift),
                KeyCode::End => self.move_to(self.document().len(), shift),
                _ => return Ok(false),
            }
        } else {
            match k.code {
                KeyCode::Char(c)
                    if !k
                        .modifiers
                        .intersects(KeyModifiers::ALT | KeyModifiers::META) =>
                {
                    return self.insert(&c.to_string());
                }
                KeyCode::Enter => return self.insert("\n"),
                KeyCode::Tab => return self.insert("\t"),
                KeyCode::Backspace => {
                    return self.replace(
                        if self.selection().is_empty() {
                            self.prev()..self.caret()
                        } else {
                            self.selection()
                        },
                        "",
                    );
                }
                KeyCode::Delete => {
                    return self.replace(
                        if self.selection().is_empty() {
                            self.caret()..self.next()
                        } else {
                            self.selection()
                        },
                        "",
                    );
                }
                KeyCode::Left => self.move_to(
                    if !shift && !self.selection().is_empty() {
                        self.selection().start
                    } else {
                        self.prev()
                    },
                    shift,
                ),
                KeyCode::Right => self.move_to(
                    if !shift && !self.selection().is_empty() {
                        self.selection().end
                    } else {
                        self.next()
                    },
                    shift,
                ),
                KeyCode::Up => self.move_to(self.vertical(-1), shift),
                KeyCode::Down => self.move_to(self.vertical(1), shift),
                KeyCode::Home => {
                    let line = self.document().line_at_byte(self.caret()).unwrap();
                    self.move_to(self.document().line_range(line).unwrap().start, shift);
                }
                KeyCode::End => {
                    let line = self.document().line_at_byte(self.caret()).unwrap();
                    self.move_to(self.document().line_range(line).unwrap().end, shift);
                }
                _ => return Ok(false),
            }
        }
        Ok(before != (self.caret(), self.anchor()))
    }
}
