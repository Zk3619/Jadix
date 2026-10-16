//! Experimental document editor: Textarea chrome + [`DocumentRegistry`] session.
//!
//! **Not** an IDE (no LSP / minimap / multi-cursor). Z/Y use the same
//! `input_controller` path as Textarea; history survives tab unmount until
//! [`DocumentRegistry::unregister`]. See `docs/plans/text_undo_redo.md` §12.

use crate::components::base::SlotChrome;
use crate::components::input::InputType;
use crate::components::internal::text_edit::{build_text_edit_element, TextEditBuildParams};
use crate::core::reactivity::Signal;
use crate::core::tree::Element;
use crate::utils::document_registry::{
    default_document_registry, document_field_id, DocumentRegistry,
};
use crate::utils::text_field::TextareaWrapMode;
use jadix_macros::component;

const DEFAULT_TEXT_SIZE_PX: f32 = 0.875 * 16.0;
const DEFAULT_FONT_WEIGHT: u16 = 400;

pub fn code_editor(uri: impl Into<String>) -> CodeEditorBuilder {
    CodeEditorBuilder {
        uri: uri.into(),
        registry: None,
        rows: 16,
        wrap_mode: TextareaWrapMode::None,
        value: None,
        initial: None,
        chrome: SlotChrome::default(),
        #[cfg(feature = "text-undo")]
        undo: crate::utils::text_edit_history::TextUndoConfig::textarea_default(),
    }
}

pub struct CodeEditorBuilder {
    uri: String,
    registry: Option<DocumentRegistry>,
    rows: u32,
    wrap_mode: TextareaWrapMode,
    value: Option<Signal<String>>,
    initial: Option<String>,
    chrome: SlotChrome,
    #[cfg(feature = "text-undo")]
    undo: crate::utils::text_edit_history::TextUndoConfig,
}

impl CodeEditorBuilder {
    crate::impl_slot_chrome_methods!();

    pub fn registry(mut self, registry: DocumentRegistry) -> Self {
        self.registry = Some(registry);
        self
    }

    pub fn rows(mut self, n: u32) -> Self {
        self.rows = n.max(1);
        self
    }

    pub fn wrap(mut self, mode: impl Into<TextareaWrapMode>) -> Self {
        self.wrap_mode = mode.into();
        self
    }

    pub fn value_signal(mut self, s: Signal<String>) -> Self {
        self.value = Some(s);
        self
    }

    /// Seed text on **first** [`DocumentRegistry::register`] only.
    pub fn initial(mut self, text: impl Into<String>) -> Self {
        self.initial = Some(text.into());
        self
    }

    #[cfg(feature = "text-undo")]
    pub fn undo_enabled(mut self, enabled: bool) -> Self {
        self.undo.enabled = enabled;
        self
    }
}

impl From<CodeEditorBuilder> for Element {
    fn from(b: CodeEditorBuilder) -> Element {
        let CodeEditorBuilder {
            uri,
            registry,
            rows,
            wrap_mode,
            value,
            initial,
            chrome,
            #[cfg(feature = "text-undo")]
            undo,
        } = b;

        let registry = registry.unwrap_or_else(default_document_registry);
        let seed = value.as_ref().map(|s| s.peek()).or(initial);
        let handle = registry.register(&uri, seed);
        let field_id = handle
            .as_ref()
            .map(|h| h.field_id().to_string())
            .unwrap_or_else(|| document_field_id(&uri));

        chrome.apply(build_text_edit_element(TextEditBuildParams {
            field_id,
            placeholder: String::new(),
            disabled: false,
            invalid: false,
            input_type: InputType::Text,
            clearable: false,
            password_toggle: false,
            number_steppers: false,
            step: 1.0,
            text_size_px: DEFAULT_TEXT_SIZE_PX,
            font_weight: DEFAULT_FONT_WEIGHT,
            line_height_px: None,
            leading_scale: None,
            value,
            text_align: crate::style::TextAlign::Start,
            single_line_container: None,
            multiline: true,
            rows,
            resizable: true,
            resize_max_h: 720.0,
            wrap_mode,
            plain_chrome: false,
            drop_undo_on_unmount: false,
            caret_color: None,
            selection_bg: None,
            selection_foreground: None,
            selection_on_blur: crate::utils::text_field::SelectionOnBlur::Retain,
            undo: {
                #[cfg(feature = "text-undo")]
                {
                    undo
                }
                #[cfg(not(feature = "text-undo"))]
                {
                    crate::utils::text_edit_history::TextUndoConfig::textarea_default()
                }
            },
        }))
    }
}

#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn CodeEditor(
    uri: String,
    rows: Option<i32>,
    wrap: Option<String>,
    value: Option<Signal<String>>,
    children: Option<Element>,
) -> Element {
    let _ = children;
    let mut b = code_editor(uri);
    if let Some(n) = rows {
        b = b.rows(n.max(1) as u32);
    }
    if let Some(w) = wrap {
        b = b.wrap(TextareaWrapMode::parse(&w));
    }
    if let Some(v) = value {
        b = b.value_signal(v);
    }
    b.into()
}

#[cfg(all(test, feature = "text-undo"))]
mod tests {
    use super::*;
    use crate::utils::document_registry::DocumentRegistry;
    use crate::utils::text_edit_history::{
        note_after_edit, undo, undo_slot_exists, will_user_edit, EditKind, TextEditSnapshot,
    };

    fn snap(value: &str, caret: usize) -> TextEditSnapshot {
        TextEditSnapshot {
            value: value.into(),
            caret,
            ..Default::default()
        }
    }

    #[test]
    fn unmount_keeps_history_until_unregister() {
        let reg = DocumentRegistry::new();
        let uri = "file:///tab.rs";
        let field_id = document_field_id(uri);
        let el: Element = code_editor(uri).registry(reg.clone()).initial("x").into();
        will_user_edit(&field_id, snap("x", 1), EditKind::Insert);
        note_after_edit(&field_id, snap("xy", 2));
        drop(el);
        assert!(
            undo_slot_exists(&field_id),
            "tab unmount must not drop L3 history"
        );
        assert_eq!(undo(&field_id).unwrap().value, "x");
        assert!(reg.unregister(uri));
        assert!(!undo_slot_exists(&field_id));
    }
}
