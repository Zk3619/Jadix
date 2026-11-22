//! Read-only, fixed-height logical lines, independent of Input/Textarea.
//!
//! Only viewport rows are materialized. No wrapping, editing or block layout;
//! selection currently uses Label's per-line selection, not a document selection.

use std::{cell::RefCell, rc::Rc};

use crate::components::base::{SlotChrome, paint_slot};
use crate::components::label::enable_label_selection;
use crate::components::virtual_list::virtual_list;
use crate::core::reactivity::{Signal, create_effect, dispose_effect};
use crate::core::tree::{CursorValue, Element};
use crate::text::TextDocument;
use crate::utils::text_annotations::{DocumentHighlights, DocumentSyntax, TextDecoration};

pub fn text_view(id: impl Into<String>) -> TextViewBuilder {
    TextViewBuilder {
        id: id.into(),
        document: TextDocument::default(),
        document_signal: None,
        line_numbers: false,
        selectable: true,
        row_height: 22.0,
        viewport_h: 320.0,
        overscan: 3,
        cursor: None,
        highlights: Vec::new(),
        language: None,
        chrome: SlotChrome::default(),
    }
}

pub struct TextViewBuilder {
    id: String,
    document: TextDocument,
    document_signal: Option<Signal<TextDocument>>,
    line_numbers: bool,
    selectable: bool,
    row_height: f32,
    viewport_h: f32,
    overscan: usize,
    cursor: Option<CursorValue>,
    highlights: Vec<TextDecoration>,
    language: Option<String>,
    chrome: SlotChrome,
}

impl TextViewBuilder {
    crate::impl_slot_chrome_methods!();

    /// Ordered UTF-8 source-byte styles, bound to the initial document snapshot.
    /// Publishing a replacement document clears these styles until rebuilt.
    pub fn highlights(mut self, highlights: Vec<TextDecoration>) -> Self {
        self.highlights = highlights;
        self
    }

    /// Enable bounded syntax highlighting with the `syntax-highlighting` feature.
    /// Parsed in the background per snapshot/theme; explicit highlights take precedence.
    pub fn language(mut self, language: impl Into<String>) -> Self {
        self.language = Some(language.into());
        self
    }

    pub fn document(mut self, document: impl Into<TextDocument>) -> Self {
        self.document = document.into();
        self.document_signal = None;
        self
    }

    /// Publish snapshots rather than cloning an entire String every frame.
    pub fn document_signal(mut self, document: Signal<TextDocument>) -> Self {
        self.document_signal = Some(document);
        self
    }

    pub fn line_numbers(mut self, enabled: bool) -> Self {
        self.line_numbers = enabled;
        self
    }

    pub fn selectable(mut self, enabled: bool) -> Self {
        self.selectable = enabled;
        self
    }

    pub fn row_height(mut self, height: f32) -> Self {
        self.row_height = height.max(1.0);
        self
    }

    pub fn viewport_h(mut self, height: f32) -> Self {
        self.viewport_h = height.max(1.0);
        self
    }

    pub fn overscan(mut self, lines: usize) -> Self {
        self.overscan = lines;
        self
    }

    pub fn cursor(mut self, value: impl crate::style::IntoCursor) -> Self {
        self.cursor = value.into_cursor();
        self
    }
}

pub(crate) fn gutter_width(line_count: usize) -> f32 {
    (line_count.max(1).ilog10() + 2) as f32 * 10.0
}

pub(crate) fn source_line(
    id: &str,
    document: &TextDocument,
    line: Option<usize>,
    numbers: bool,
    selectable: bool,
    row_height: f32,
) -> Element {
    source_line_with_cursor(id, document, line, numbers, selectable, row_height, None)
}

pub(crate) fn source_line_with_cursor(
    id: &str,
    document: &TextDocument,
    line: Option<usize>,
    numbers: bool,
    selectable: bool,
    row_height: f32,
    cursor: Option<CursorValue>,
) -> Element {
    let mut row = Element::new()
        .display_flex()
        .flex_row()
        .gap(8.0)
        .flex_items("start")
        .min_w(0.0);
    row.set_attr("data-slot", "text-view-line");
    if let Some(line) = line {
        row.set_attr("data-source-line", (line + 1).to_string());
        let range = document.line_full_range(line).expect("source line exists");
        let raw = &document.text()[range];
        row.set_attr(
            "data-line-ending",
            if raw.ends_with("\r\n") {
                "crlf"
            } else if raw.ends_with('\n') {
                "lf"
            } else {
                "none"
            },
        );
    }
    if numbers {
        let number = Element::new()
            .text(line.map(|line| (line + 1).to_string()).unwrap_or_default())
            .w(gutter_width(document.line_count()))
            .flex_shrink(0.0)
            .text_line_height(row_height)
            .class("text-muted-foreground")
            .text_align("end")
            .pr(8.0);
        row = row.child(number);
    }
    let Some(line) = line else {
        return row;
    };
    let mut text = Element::new()
        .text(document.line(line).unwrap_or_default())
        .white_space("pre")
        .w("fit-content")
        .flex_shrink(0.0)
        .text_line_height(row_height);
    if selectable {
        let selection_id = format!("text-view:{id}:{}:{line}", document.revision());
        let selection_key = crate::components::label::label_selection_signal_key(&selection_id);
        enable_label_selection(&mut text, selection_id);
        text =
            text.on_unmount(move || crate::core::reactivity::remove_named_signal(&selection_key));
    }
    if let Some(cursor) = cursor {
        text = text.cursor(cursor);
    }
    row.child(text)
}

pub(crate) fn source_width(document: &TextDocument, numbers: bool) -> f32 {
    let gutter = if numbers {
        gutter_width(document.line_count())
    } else {
        0.0
    };
    document.max_line_width_units() as f32 * 14.0 + gutter + 160.0
}

pub(crate) fn highlight_source_line(
    mut row: Element,
    document: &TextDocument,
    line: Option<usize>,
    highlights: &DocumentHighlights,
) -> Element {
    if let (Some(line), Some(text)) = (line, row.children.last_mut()) {
        text.text_spans = highlights.line_spans(document, line, text.text_spans.as_deref());
    }
    row
}

pub(crate) fn redraw_on_snapshot<T: Clone + 'static>(
    mut root: Element,
    signal: Option<Signal<T>>,
) -> Element {
    let Some(signal) = signal else {
        return root;
    };
    let effect = create_effect(move || {
        if signal.try_get().is_some() {
            crate::app::runtime::request_redraw();
        }
    });
    let previous = root.on_unmount.take();
    root.on_unmount(move || {
        dispose_effect(effect);
        if let Some(previous) = previous {
            previous.fire();
        }
    })
}

pub(crate) fn cancel_syntax_on_unmount(
    mut root: Element,
    syntax: Vec<Rc<RefCell<DocumentSyntax>>>,
) -> Element {
    let previous = root.on_unmount.take();
    root.on_unmount(move || {
        for syntax in &syntax {
            syntax.borrow_mut().cancel();
        }
        if let Some(previous) = &previous {
            previous.fire();
        }
    })
}

impl From<TextViewBuilder> for Element {
    fn from(builder: TextViewBuilder) -> Self {
        let TextViewBuilder {
            id,
            document,
            document_signal,
            line_numbers,
            selectable,
            row_height,
            viewport_h,
            overscan,
            cursor,
            highlights,
            language,
            chrome,
        } = builder;
        let id = crate::utils::resolve_instance_id(id, "text-view");
        let source: Rc<dyn Fn() -> TextDocument> = Rc::new(move || {
            document_signal
                .as_ref()
                .and_then(|signal| signal.try_peek())
                .unwrap_or_else(|| document.clone())
        });
        let count = source.clone();
        let initial = source();
        let initial_revision = initial.revision();
        let syntax = Rc::new(RefCell::new(DocumentSyntax::new(language)));
        let indexed = Rc::new(RefCell::new((
            0,
            DocumentHighlights::new(&initial, highlights.clone()),
        )));
        let key_syntax = syntax.clone();
        let key_index = indexed.clone();
        let key = source.clone();
        let width = source.clone();
        let row_id = id.clone();
        let item_cursor = cursor.clone();
        let mut root: Element = virtual_list(&id)
            .height(viewport_h)
            .item_height(row_height)
            .overscan(overscan)
            .item_count_fn(move || count().line_count())
            .data_key_fn(move || {
                let document = key();
                let mut syntax = key_syntax.borrow_mut();
                let generation = syntax.update(&document, crate::style::is_dark_theme());
                let mut indexed = key_index.borrow_mut();
                if indexed.0 != generation {
                    let explicit = if document.revision() == initial_revision {
                        highlights.clone()
                    } else {
                        Vec::new()
                    };
                    *indexed = (
                        generation,
                        DocumentHighlights::from_syntax(&document, syntax.colors(), explicit),
                    );
                }
                format!("{}:{generation}", document.revision())
            })
            .content_width_fn(move || source_width(&width(), line_numbers))
            .render_item(move |line| {
                let document = source();
                highlight_source_line(
                    source_line_with_cursor(
                        &row_id,
                        &document,
                        Some(line),
                        line_numbers,
                        selectable,
                        row_height,
                        item_cursor.clone(),
                    ),
                    &document,
                    Some(line),
                    &indexed.borrow().1,
                )
            })
            .into();
        if let Some(cursor) = cursor {
            root = root.cursor(cursor);
        }
        redraw_on_snapshot(
            cancel_syntax_on_unmount(
                paint_slot(
                    root.font_family("monospace")
                        .text_size(14.0)
                        .accesskit_role("group"),
                    "text-view",
                    &chrome,
                ),
                vec![syntax],
            ),
            document_signal,
        )
    }
}

crate::__component_contract! {
TextViewProps [];
pub fn TextView(
identity [] id: String,
default [] text: String,
plain [] document: Option<TextDocument>,
plain [] document_signal: Option<Signal<TextDocument>>,
default [] line_numbers: bool,
plain [] selectable: Option<bool>,
plain [] row_height: Option<f32>,
plain [] viewport_h: Option<f32>,
plain [] overscan: Option<usize>,
plain [] cursor: Option<CursorValue>,
plain [] highlights: Option<Vec<TextDecoration>>,
plain [] language: Option<String>,
plain [] children: Option<crate::core::tree::Element>,
) -> Element {
    let mut builder = text_view(id)
        .document(document.unwrap_or_else(|| TextDocument::new(text)))
        .line_numbers(line_numbers);
    if let Some(signal) = document_signal {
        builder = builder.document_signal(signal);
    }
    if let Some(enabled) = selectable {
        builder = builder.selectable(enabled);
    }
    if let Some(height) = row_height {
        builder = builder.row_height(height);
    }
    if let Some(height) = viewport_h {
        builder = builder.viewport_h(height);
    }
    if let Some(lines) = overscan {
        builder = builder.overscan(lines);
    }
    if let Some(cursor) = cursor {
        builder = builder.cursor(cursor);
    }
    if let Some(highlights) = highlights {
        builder = builder.highlights(highlights);
    }
    if let Some(language) = language {
        builder = builder.language(language);
    }
    builder.into()
}
}

#[cfg(all(test, feature = "product-tests"))]
#[path = "tests/text_view.rs"]
mod tests;
