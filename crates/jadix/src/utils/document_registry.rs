//! L3 document sessions: one undo stack per URI, released only on [`DocumentRegistry::unregister`].
//!
//! Input/Textarea still drop history on widget unmount (P1). A CodeEditor / tab host
//! **must not**: switching tabs unmounts the widget, but VSCode keeps undo until the
//! tab is closed. This registry is that valve. Save does **not** clear history.
//!
//! Not a process-level leaky map: the app (or [`default_document_registry`]) owns the
//! `Rc`; close-tab is explicit `unregister`. See `docs/plans/text_undo_redo.md` §12.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::core::reactivity::use_signal;
use crate::utils::text_edit_history::{self, TextUndoConfig};
use crate::utils::text_field::{commit_value, input_value_signal_key, wipe_text_edit_named_slots};

const DEFAULT_REGISTRY_KEY: &str = "__jadix_document_registry";

/// Named-signal field id so document slots never collide with form Input ids.
pub fn document_field_id(uri: &str) -> String {
    format!("__jadix_doc:{uri}")
}

/// App-wide default registry (named signal, same identity model as Input value).
/// Tests should construct [`DocumentRegistry::new`] so they do not share slots.
pub fn default_document_registry() -> DocumentRegistry {
    use_signal(DEFAULT_REGISTRY_KEY, DocumentRegistry::new).peek()
}

#[derive(Clone, Debug)]
pub struct DocumentRegistry {
    inner: Rc<RefCell<Inner>>,
}

#[derive(Debug, Default)]
struct Inner {
    sessions: HashMap<String, SessionMeta>,
}

#[derive(Clone, Debug)]
struct SessionMeta {
    saved_text: String,
}

impl Default for DocumentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl DocumentRegistry {
    pub fn new() -> Self {
        Self {
            inner: Rc::new(RefCell::new(Inner {
                sessions: HashMap::new(),
            })),
        }
    }

    pub fn len(&self) -> usize {
        self.inner.borrow().sessions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.borrow().sessions.is_empty()
    }

    pub fn contains(&self, uri: &str) -> bool {
        self.inner.borrow().sessions.contains_key(uri)
    }

    /// Open `uri`. First open seeds the buffer with `initial` (not an undo step).
    /// Re-open of an already registered URI keeps the live buffer and history.
    pub fn register(
        &self,
        uri: impl Into<String>,
        initial: Option<String>,
    ) -> Option<DocumentHandle> {
        let uri = uri.into();
        if uri.is_empty() {
            return None;
        }
        let field_id = document_field_id(&uri);
        let is_new = !self.inner.borrow().sessions.contains_key(&uri);
        if is_new {
            let saved = initial.clone().unwrap_or_default();
            if let Some(text) = initial {
                commit_value(&field_id, text);
            }
            self.inner
                .borrow_mut()
                .sessions
                .insert(uri.clone(), SessionMeta { saved_text: saved });
            text_edit_history::install_field_config(&field_id, TextUndoConfig::textarea_default());
        }
        Some(DocumentHandle {
            registry: self.clone(),
            uri,
            field_id,
        })
    }

    pub fn get(&self, uri: &str) -> Option<DocumentHandle> {
        if !self.contains(uri) {
            return None;
        }
        Some(DocumentHandle {
            registry: self.clone(),
            uri: uri.to_string(),
            field_id: document_field_id(uri),
        })
    }

    /// Close-tab: drop undo + value named slots. Other URIs are untouched.
    pub fn unregister(&self, uri: &str) -> bool {
        let Some(_) = self.inner.borrow_mut().sessions.remove(uri) else {
            return false;
        };
        wipe_text_edit_named_slots(&document_field_id(uri));
        true
    }

    /// Mark the current buffer as saved. **Does not** clear undo (VSCode).
    pub fn save(&self, uri: &str) -> bool {
        let field_id = document_field_id(uri);
        let current = use_signal(&input_value_signal_key(&field_id), String::new).peek();
        let mut inner = self.inner.borrow_mut();
        let Some(meta) = inner.sessions.get_mut(uri) else {
            return false;
        };
        meta.saved_text = current;
        true
    }

    pub fn is_dirty(&self, uri: &str) -> bool {
        let saved = {
            let inner = self.inner.borrow();
            let Some(meta) = inner.sessions.get(uri) else {
                return false;
            };
            meta.saved_text.clone()
        };
        let field_id = document_field_id(uri);
        use_signal(&input_value_signal_key(&field_id), String::new).peek() != saved
    }
}

/// Handle for one open URI. Cheap to clone (`Rc` registry + owned keys).
#[derive(Clone, Debug)]
pub struct DocumentHandle {
    registry: DocumentRegistry,
    uri: String,
    field_id: String,
}

impl DocumentHandle {
    pub fn uri(&self) -> &str {
        &self.uri
    }

    /// Field id consumed by `input_controller` / Z/Y (same as a Textarea id).
    pub fn field_id(&self) -> &str {
        &self.field_id
    }

    pub fn save(&self) -> bool {
        self.registry.save(&self.uri)
    }

    pub fn is_dirty(&self) -> bool {
        self.registry.is_dirty(&self.uri)
    }

    pub fn can_undo(&self) -> bool {
        text_edit_history::can_undo(&self.field_id)
    }

    pub fn can_redo(&self) -> bool {
        text_edit_history::can_redo(&self.field_id)
    }

    pub fn undo(&self) -> bool {
        text_edit_history::undo_field(&self.field_id)
    }

    pub fn redo(&self) -> bool {
        text_edit_history::redo_field(&self.field_id)
    }
}

#[cfg(test)]
#[path = "tests/document_registry.rs"]
mod tests;
