//! Read-only, fixed-height logical lines, independent of Input/Textarea.
//!
//! Only viewport rows are materialized. No wrapping, editing or block layout;
//! selection currently uses Label's per-line selection, not a document selection.

use std::rc::Rc;

use crate::components::base::{paint_slot, SlotChrome};
use crate::components::label::enable_label_selection;
use crate::components::virtual_list::virtual_list;
use crate::core::reactivity::{create_effect, dispose_effect, Signal};
use crate::core::tree::Element;
use crate::text::TextDocument;

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
    chrome: SlotChrome,
}

impl TextViewBuilder {
    crate::impl_slot_chrome_methods!();

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
        let key = source.clone();
        let width = source.clone();
        let row_id = id.clone();
        let root: Element = virtual_list(&id)
            .height(viewport_h)
            .item_height(row_height)
            .overscan(overscan)
            .item_count_fn(move || count().line_count())
            .data_key_fn(move || key().revision().to_string())
            .content_width_fn(move || source_width(&width(), line_numbers))
            .render_item(move |line| {
                source_line(
                    &row_id,
                    &source(),
                    Some(line),
                    line_numbers,
                    selectable,
                    row_height,
                )
            })
            .into();
        redraw_on_snapshot(
            paint_slot(
                root.font_family("monospace")
                    .text_size(14.0)
                    .accesskit_role("group"),
                "text-view",
                &chrome,
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
    builder.into()
}
}

#[cfg(all(test, feature = "product-tests"))]
mod tests {
    use super::*;

    fn node_count(element: &Element) -> usize {
        1 + element.children.iter().map(node_count).sum::<usize>()
    }

    #[test]
    fn text_view_builder_materializes_only_viewport_rows() {
        let document = TextDocument::new("long document\n".repeat(100_000));
        let view: Element = text_view("text-view-large")
            .document(document)
            .line_numbers(true)
            .selectable(false)
            .viewport_h(220.0)
            .row_height(22.0)
            .overscan(2)
            .class("rounded-md")
            .into();
        assert_eq!(
            view.attrs.get("data-slot").map(String::as_str),
            Some("text-view")
        );
        assert!(node_count(&view) < 100);
        assert!(view.text_style.input_caret.is_none());
    }

    #[test]
    fn source_rows_keep_empty_lines_and_selectable_source_separate_from_gutter() {
        let document = TextDocument::new("first\n\nlast");
        let row = source_line("row", &document, Some(1), true, true, 22.0);
        assert_eq!(row.children[0].text.as_deref(), Some("2"));
        assert!(!row.children[0].text_style.selectable);
        assert_eq!(row.children[1].text.as_deref(), Some(""));
        assert!(row.children[1].text_style.selectable);
        assert_eq!(
            source_line("gap", &document, None, true, true, 22.0)
                .children
                .len(),
            1
        );
    }

    #[test]
    fn snapshot_updates_and_stale_scroll_are_safe_without_root_rebuild() {
        use crate::app::test_support::sync_reactive_elements_subtree;
        use crate::core::reactivity::{create_root, use_signal};
        use crate::core::scroll_viewport::scroll_offset_y_signal_key;
        use crate::core::tree::{LayoutStatus, LayoutTree};

        let (_owner, ()) = create_root(|| {
            let signal = use_signal("text-view-revisions", || TextDocument::new("old\nold"));
            let mut view: Element = text_view("text-view-live").document_signal(signal).into();
            let mut layout = LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            signal.set_silent(TextDocument::new("new\nnew"));
            sync_reactive_elements_subtree(&mut view, &mut layout, &mut impact);
            assert_eq!(impact, LayoutStatus::StructureChanged);
            fn contains(element: &Element, text: &str) -> bool {
                element.text.as_deref() == Some(text)
                    || element.children.iter().any(|child| contains(child, text))
            }
            assert!(contains(&view, "new"));
            assert!(!contains(&view, "old"));
            use_signal(&scroll_offset_y_signal_key("text-view-live"), || 0.0_f32)
                .set_silent(2_000_000.0);
            signal.set_silent(TextDocument::new("short"));
            sync_reactive_elements_subtree(&mut view, &mut layout, &mut impact);
            use_signal(&scroll_offset_y_signal_key("text-view-live"), || 0.0_f32).set_silent(0.0);
            sync_reactive_elements_subtree(&mut view, &mut layout, &mut impact);
            assert!(contains(&view, "short"));
            let mut unchanged = LayoutStatus::Clean;
            sync_reactive_elements_subtree(&mut view, &mut layout, &mut unchanged);
            assert_eq!(unchanged, LayoutStatus::Clean);
        });
    }
}
