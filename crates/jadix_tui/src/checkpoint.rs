//! Versioned bounded editor state. Clipboard contents are deliberately not saved.
use crate::{editor::Snapshot, CheckpointStore, TerminalEditor};
use jadix_document::TextDocument;
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    io::{self, Write},
    path::PathBuf,
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredSnapshot {
    text: String,
    anchor: usize,
    caret: usize,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredEditor {
    schema: u32,
    current: StoredSnapshot,
    undo: Vec<StoredSnapshot>,
    redo: Vec<StoredSnapshot>,
}
impl StoredSnapshot {
    fn from_snapshot(snapshot: &Snapshot) -> Self {
        Self {
            text: snapshot.document.text().into(),
            anchor: snapshot.anchor,
            caret: snapshot.caret,
        }
    }
    fn validate(&self) -> io::Result<()> {
        if self.text.len() > TerminalEditor::MAX_BYTES
            || [self.anchor, self.caret]
                .iter()
                .any(|at| *at > self.text.len() || !self.text.is_char_boundary(*at))
        {
            return Err(invalid("invalid editor text budget or UTF-8 selection"));
        }
        Ok(())
    }
    fn into_snapshot(self) -> Snapshot {
        Snapshot {
            document: TextDocument::new(self.text),
            anchor: self.anchor,
            caret: self.caret,
        }
    }
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
pub(crate) struct Limited(pub(crate) Vec<u8>);
impl Write for Limited {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.0.len().saturating_add(bytes.len()) > TerminalEditor::MAX_CHECKPOINT_BYTES {
            return Err(invalid("editor checkpoint exceeds encoded byte budget"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl TerminalEditor {
    pub const MAX_CHECKPOINT_BYTES: usize = 16 * 1024 * 1024;
    pub fn checkpoint_bytes(&self) -> io::Result<Vec<u8>> {
        let value = StoredEditor {
            schema: 1,
            current: StoredSnapshot::from_snapshot(&self.current),
            undo: self
                .undo
                .iter()
                .map(StoredSnapshot::from_snapshot)
                .collect(),
            redo: self
                .redo
                .iter()
                .map(StoredSnapshot::from_snapshot)
                .collect(),
        };
        let mut output = Limited(Vec::new());
        serde_json::to_writer(&mut output, &value).map_err(io::Error::other)?;
        Ok(output.0)
    }
    /// Parse/validate everything before mutating state. Fresh document identities
    /// invalidate old parser/resource revisions even when restored text matches.
    pub fn restore_checkpoint(&mut self, expected_revision: u64, bytes: &[u8]) -> io::Result<()> {
        self.check_revision(expected_revision)?;
        if bytes.len() > Self::MAX_CHECKPOINT_BYTES {
            return Err(invalid("oversized editor checkpoint"));
        }
        let value: StoredEditor = serde_json::from_slice(bytes).map_err(io::Error::other)?;
        if value.schema != 1
            || value.undo.len().saturating_add(value.redo.len()) > Self::HISTORY_ENTRIES
        {
            return Err(invalid("unsupported editor schema or history count"));
        }
        value.current.validate()?;
        let mut history_bytes = 0usize;
        for snapshot in value.undo.iter().chain(value.redo.iter()) {
            snapshot.validate()?;
            history_bytes = history_bytes.saturating_add(snapshot.text.len());
        }
        if history_bytes > Self::HISTORY_BYTES {
            return Err(invalid("editor history exceeds byte budget"));
        }
        let current = value.current.into_snapshot();
        let undo: VecDeque<_> = value
            .undo
            .into_iter()
            .map(StoredSnapshot::into_snapshot)
            .collect();
        let redo = value
            .redo
            .into_iter()
            .map(StoredSnapshot::into_snapshot)
            .collect();
        self.current = current;
        self.undo = undo;
        self.redo = redo;
        Ok(())
    }
}
/// Editor-specific use of the same checksum/generation/atomic replacement store
/// exposed by jadix_native. A failed save never changes its remembered generation.
pub struct EditorCheckpoint {
    store: CheckpointStore,
    generation: Option<u64>,
}
impl EditorCheckpoint {
    pub fn new(path: impl Into<PathBuf>) -> io::Result<Self> {
        Ok(Self {
            store: CheckpointStore::new(path, TerminalEditor::MAX_CHECKPOINT_BYTES)
                .map_err(io::Error::other)?,
            generation: None,
        })
    }
    pub fn generation(&self) -> Option<u64> {
        self.generation
    }
    pub fn save(&mut self, editor: &TerminalEditor) -> io::Result<u64> {
        let bytes = editor.checkpoint_bytes()?;
        let text = std::str::from_utf8(&bytes).map_err(io::Error::other)?;
        let generation = self
            .store
            .save(self.generation, text)
            .map_err(io::Error::other)?;
        self.generation = Some(generation);
        Ok(generation)
    }
    pub fn load(
        &mut self,
        editor: &mut TerminalEditor,
        expected_revision: u64,
    ) -> io::Result<bool> {
        let Some(value) = self.store.read().map_err(io::Error::other)? else {
            return Ok(false);
        };
        editor.restore_checkpoint(expected_revision, value.payload.as_bytes())?;
        self.generation = Some(value.generation);
        Ok(true)
    }
}
