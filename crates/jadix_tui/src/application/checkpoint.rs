//! Opt-in automatic recovery for named controlled editors.
use super::*;
use crate::{CheckpointStore, TerminalEditor};
use jadix_core::presentation::NodeId;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    schema: u32,
    fields: BTreeMap<String, String>,
    focus: Option<String>,
}
pub(super) struct Recovery {
    store: CheckpointStore,
    generation: Option<u64>,
    keys: Vec<String>,
    saved: Saved,
    payload: Option<String>,
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
impl CellApplication {
    fn recovery_node(&self, key: &str) -> io::Result<NodeId> {
        let id = self
            .projection
            .node_by_template(&self.element, key)
            .ok_or_else(|| invalid("checkpoint key must name one visible projected editor"))?;
        let mut pending = vec![&self.element];
        while let Some(element) = pending.pop() {
            if element.template_id.as_deref() == Some(key) && element.input.is_none() {
                return Err(invalid(
                    "checkpoint field requires a controlled Signal binding",
                ));
            }
            pending.extend(&element.children);
        }
        self.surface.checkpoint_target(id)?;
        Ok(id)
    }
    /// Attach after first `present`. Restores values/history/selection and focus
    /// through the existing controlled bindings, using explicit stable template
    /// keys (1..64, each <=256 bytes). Password fields are rejected. Hidden pages
    /// retain their last saved editor state; initial restore requires all saved
    /// fields to be mounted and in the active scope. Application business state,
    /// clipboard, passwords and transient modal/capture state are not serialized.
    ///
    /// Successful input automatically saves changed state. This opt-in path uses
    /// synchronous atomic file I/O: a save error is returned after input commits;
    /// it never rolls back accepted input or silently overwrites another writer.
    pub fn enable_checkpoint(
        &mut self,
        path: impl Into<PathBuf>,
        keys: &[&str],
    ) -> io::Result<bool> {
        if self.checkpoint.is_some()
            || self.viewport.is_none()
            || keys.is_empty()
            || keys.len() > 64
        {
            return Err(invalid(
                "checkpoint requires a mounted app and 1..64 unique keys",
            ));
        }
        let mut unique = BTreeSet::new();
        let mut fields = BTreeMap::new();
        let mut encoded_bytes = 0usize;
        for key in keys {
            if key.is_empty()
                || key.len() > 256
                || key.chars().any(char::is_control)
                || !unique.insert(*key)
            {
                return Err(invalid("invalid checkpoint key"));
            }
            let id = self.recovery_node(key)?;
            let bytes = self.surface.checkpoint_field(id)?;
            encoded_bytes = encoded_bytes.saturating_add(bytes.len());
            if encoded_bytes > TerminalEditor::MAX_CHECKPOINT_BYTES {
                return Err(invalid("checkpoint aggregate encoded budget"));
            }
            fields.insert(
                (*key).to_owned(),
                String::from_utf8(bytes).map_err(io::Error::other)?,
            );
        }
        let store = CheckpointStore::new(path, TerminalEditor::MAX_CHECKPOINT_BYTES)
            .map_err(io::Error::other)?;
        let loaded = store.read().map_err(io::Error::other)?;
        let mut recovery = Recovery {
            store,
            generation: None,
            keys: keys.iter().map(|k| (*k).to_owned()).collect(),
            saved: Saved {
                schema: 1,
                fields,
                focus: None,
            },
            payload: None,
        };
        if let Some(value) = &loaded {
            let saved: Saved = serde_json::from_str(&value.payload).map_err(io::Error::other)?;
            if saved.schema != 1
                || saved.fields.keys().any(|k| !unique.contains(k.as_str()))
                || saved
                    .focus
                    .as_ref()
                    .is_some_and(|k| !saved.fields.contains_key(k))
            {
                return Err(invalid("unsupported checkpoint schema or key"));
            }
            let staged: Vec<_> = saved
                .fields
                .iter()
                .map(|(key, bytes)| Ok((self.recovery_node(key)?, bytes.clone())))
                .collect::<io::Result<_>>()?;
            let focus = saved
                .focus
                .as_ref()
                .map(|key| self.recovery_node(key))
                .transpose()?;
            let events = self.surface.restore_checkpoint_fields(&staged)?;
            let owner = self.owner.as_ref().ok_or(io::ErrorKind::BrokenPipe)?.id();
            let applied = with_owner(owner, || {
                for event in events {
                    self.projection
                        .dispatch(&mut self.element, &event)
                        .map_err(|e| io::Error::other(format!("checkpoint binding: {e:?}")))?;
                }
                Ok::<_, io::Error>(())
            });
            if let Err(error) = applied {
                self.dispose();
                return Err(error);
            }
            let (area, environment) = self.viewport.unwrap();
            self.present(area, environment)?;
            if let Some(id) = focus {
                self.surface.focus_node(id)?;
            }
            recovery.saved.fields.extend(saved.fields);
            recovery.saved.focus = saved.focus;
            recovery.generation = Some(value.generation);
            recovery.payload = Some(value.payload.clone());
        }
        self.checkpoint = Some(recovery);
        Ok(loaded.is_some())
    }
    /// Persist after external signal changes or before explicit disposal. Input
    /// uses this same method automatically. Unchanged state does not rewrite disk.
    pub fn save_checkpoint(&mut self) -> io::Result<Option<u64>> {
        let Some(mut recovery) = self.checkpoint.take() else {
            return Ok(None);
        };
        let result = (|| {
            let mut fields = recovery.saved.fields.clone();
            let mut focus = None;
            let mut encoded_bytes: usize = fields.values().map(String::len).sum();
            for key in &recovery.keys {
                if let Some(id) = self.projection.node_by_template(&self.element, key) {
                    let bytes = self.surface.checkpoint_field(id)?;
                    encoded_bytes = encoded_bytes
                        .saturating_sub(fields.get(key).map_or(0, String::len))
                        .saturating_add(bytes.len());
                    if encoded_bytes > TerminalEditor::MAX_CHECKPOINT_BYTES {
                        return Err(invalid("checkpoint aggregate encoded budget"));
                    }
                    fields.insert(
                        key.clone(),
                        String::from_utf8(bytes).map_err(io::Error::other)?,
                    );
                    if self.surface.focused() == Some(id) {
                        focus = Some(key.clone());
                    }
                }
            }
            let saved = Saved {
                schema: 1,
                fields,
                focus,
            };
            let mut output = crate::checkpoint::Limited(Vec::new());
            serde_json::to_writer(&mut output, &saved).map_err(io::Error::other)?;
            let payload = String::from_utf8(output.0).map_err(io::Error::other)?;
            if recovery.payload.as_ref() != Some(&payload) {
                let generation = recovery
                    .store
                    .save(recovery.generation, &payload)
                    .map_err(io::Error::other)?;
                recovery.generation = Some(generation);
                recovery.payload = Some(payload);
                recovery.saved = saved;
            }
            Ok(recovery.generation)
        })();
        self.checkpoint = Some(recovery);
        result
    }
}
