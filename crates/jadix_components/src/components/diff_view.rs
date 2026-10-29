//! Unified and side-by-side views over a precomputed diff snapshot.
//! Both split sides share one virtual list, so alignment cannot drift.

use std::rc::Rc;

use crate::components::base::{paint_slot, SlotChrome};
use crate::components::text_view::{redraw_on_snapshot, source_line, source_width};
use crate::components::virtual_list::virtual_list;
use crate::core::reactivity::Signal;
use crate::core::tree::Element;
use crate::text::{DiffDocument, DiffLineKind, DiffMode, DiffRow};

pub fn diff_view(id: impl Into<String>, document: DiffDocument) -> DiffViewBuilder {
    DiffViewBuilder {
        id: id.into(),
        document,
        document_signal: None,
        mode: DiffMode::Unified,
        viewport_h: 320.0,
        row_height: 22.0,
        selectable: true,
        chrome: SlotChrome::default(),
    }
}

pub struct DiffViewBuilder {
    id: String,
    document: DiffDocument,
    document_signal: Option<Signal<DiffDocument>>,
    mode: DiffMode,
    viewport_h: f32,
    row_height: f32,
    selectable: bool,
    chrome: SlotChrome,
}

impl DiffViewBuilder {
    crate::impl_slot_chrome_methods!();

    pub fn document_signal(mut self, document: Signal<DiffDocument>) -> Self {
        self.document_signal = Some(document);
        self
    }

    pub fn mode(mut self, mode: DiffMode) -> Self {
        self.mode = mode;
        self
    }
    pub fn viewport_h(mut self, height: f32) -> Self {
        self.viewport_h = height.max(1.0);
        self
    }
    pub fn row_height(mut self, height: f32) -> Self {
        self.row_height = height.max(1.0);
        self
    }
    pub fn selectable(mut self, enabled: bool) -> Self {
        self.selectable = enabled;
        self
    }
}

fn mark_change(mut element: Element, kind: DiffLineKind) -> Element {
    let (name, class) = match kind {
        DiffLineKind::Equal => ("equal", ""),
        DiffLineKind::Delete => ("delete", "bg-destructive/10"),
        DiffLineKind::Insert => ("insert", "bg-success/10"),
        DiffLineKind::Replace => ("replace", ""),
    };
    element.set_attr("data-change", name);
    if class.is_empty() {
        element
    } else {
        element.class(class)
    }
}

fn ending_hint(
    mut element: Element,
    document: &crate::text::TextDocument,
    line: Option<usize>,
    changed: bool,
) -> Element {
    let Some(range) = line.and_then(|line| document.line_full_range(line)) else {
        return element;
    };
    let raw = &document.text()[range];
    let hint = if !raw.ends_with('\n') {
        Some(" [no newline]")
    } else if changed && raw.ends_with("\r\n") {
        Some(" [CRLF]")
    } else {
        None
    };
    if let Some(hint) = hint {
        element = element.child(
            Element::new()
                .text(hint)
                .white_space("pre")
                .class("text-muted-foreground")
                .flex_shrink(0.0),
        );
    }
    element
}

fn split_marker(mut cell: Element, kind: DiffLineKind, height: f32) -> Element {
    let marker = match kind {
        DiffLineKind::Delete => "-",
        DiffLineKind::Insert => "+",
        _ => " ",
    };
    cell.children.insert(
        0,
        Element::new()
            .text(marker)
            .w(20.0)
            .flex_shrink(0.0)
            .text_line_height(height),
    );
    mark_change(cell, kind)
}

fn diff_row(
    id: &str,
    document: &DiffDocument,
    row: DiffRow,
    mode: DiffMode,
    selectable: bool,
    height: f32,
) -> Element {
    if mode == DiffMode::Split {
        let left_kind = if row.kind == DiffLineKind::Replace {
            DiffLineKind::Delete
        } else {
            row.kind
        };
        let right_kind = if row.kind == DiffLineKind::Replace {
            DiffLineKind::Insert
        } else {
            row.kind
        };
        let left = ending_hint(
            source_line(
                &format!("{id}:old"),
                document.original(),
                row.original_line,
                true,
                selectable,
                height,
            ),
            document.original(),
            row.original_line,
            row.kind != DiffLineKind::Equal,
        )
        .w("50%")
        .flex_shrink(0.0)
        .overflow_x("hidden");
        let right = ending_hint(
            source_line(
                &format!("{id}:new"),
                document.modified(),
                row.modified_line,
                true,
                selectable,
                height,
            ),
            document.modified(),
            row.modified_line,
            row.kind != DiffLineKind::Equal,
        )
        .w("50%")
        .flex_shrink(0.0)
        .overflow_x("hidden");
        let mut root = Element::new()
            .display_flex()
            .flex_row()
            .child(split_marker(
                left,
                if row.original_line.is_some() {
                    left_kind
                } else {
                    DiffLineKind::Equal
                },
                height,
            ))
            .child(split_marker(
                right,
                if row.modified_line.is_some() {
                    right_kind
                } else {
                    DiffLineKind::Equal
                },
                height,
            ));
        root.set_attr(
            "data-change",
            match row.kind {
                DiffLineKind::Equal => "equal",
                DiffLineKind::Delete => "delete",
                DiffLineKind::Insert => "insert",
                DiffLineKind::Replace => "replace",
            },
        );
        return root;
    }
    let (source, line, marker, side) = if row.kind == DiffLineKind::Delete {
        (document.original(), row.original_line, "-", "old")
    } else {
        (
            document.modified(),
            row.modified_line,
            if row.kind == DiffLineKind::Insert {
                "+"
            } else {
                " "
            },
            "new",
        )
    };
    let content = ending_hint(
        source_line(
            &format!("{id}:{side}"),
            source,
            line,
            true,
            selectable,
            height,
        ),
        source,
        line,
        row.kind != DiffLineKind::Equal,
    );
    let sign = Element::new()
        .text(marker)
        .w(20.0)
        .flex_shrink(0.0)
        .text_line_height(height);
    mark_change(
        Element::new()
            .display_flex()
            .flex_row()
            .child(sign)
            .child(content),
        row.kind,
    )
}

impl From<DiffViewBuilder> for Element {
    fn from(builder: DiffViewBuilder) -> Self {
        let DiffViewBuilder {
            id,
            document,
            document_signal,
            mode,
            viewport_h,
            row_height,
            selectable,
            chrome,
        } = builder;
        let id = crate::utils::resolve_instance_id(id, "diff-view");
        let source: Rc<dyn Fn() -> DiffDocument> = Rc::new(move || {
            document_signal
                .as_ref()
                .and_then(|signal| signal.try_peek())
                .unwrap_or_else(|| document.clone())
        });
        let count = source.clone();
        let key = source.clone();
        let width = source.clone();
        let row_id = id.clone();
        let mut root: Element = virtual_list(&id)
            .height(viewport_h)
            .item_height(row_height)
            .overscan(3)
            .item_count_fn(move || count().rows(mode).len())
            .data_key_fn(move || key().revision().to_string())
            .content_width_fn(move || {
                let document = width();
                let widest = source_width(document.original(), true)
                    .max(source_width(document.modified(), true));
                if mode == DiffMode::Split {
                    widest * 2.0
                } else {
                    widest + 20.0
                }
            })
            .render_item(move |index| {
                let document = source();
                diff_row(
                    &row_id,
                    &document,
                    document.rows(mode)[index],
                    mode,
                    selectable,
                    row_height,
                )
            })
            .into();
        root.set_attr(
            "data-mode",
            if mode == DiffMode::Split {
                "split"
            } else {
                "unified"
            },
        );
        redraw_on_snapshot(
            paint_slot(
                root.font_family("monospace")
                    .text_size(14.0)
                    .accesskit_role("group"),
                "diff-view",
                &chrome,
            ),
            document_signal,
        )
    }
}

crate::__component_contract! {
DiffViewProps [];
pub fn DiffView(
identity [] id: String,
default [] document: DiffDocument,
plain [] document_signal: Option<Signal<DiffDocument>>,
plain [] mode: Option<DiffMode>,
plain [] selectable: Option<bool>,
plain [] row_height: Option<f32>,
plain [] viewport_h: Option<f32>,
plain [] children: Option<crate::core::tree::Element>,
) -> Element {
    let mut builder = diff_view(id, document);
    if let Some(signal) = document_signal {
        builder = builder.document_signal(signal);
    }
    if let Some(mode) = mode {
        builder = builder.mode(mode);
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
    builder.into()
}
}

#[cfg(all(test, feature = "product-tests"))]
#[path = "tests/diff_view.rs"]
mod tests;
