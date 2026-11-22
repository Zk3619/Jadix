//! Unified and side-by-side views over a precomputed diff snapshot.
//! Both split sides share one virtual list, so alignment cannot drift.

use std::{cell::RefCell, rc::Rc};

use crate::components::base::{SlotChrome, paint_slot};
use crate::components::text_view::{
    cancel_syntax_on_unmount, highlight_source_line, redraw_on_snapshot, source_line, source_width,
};
use crate::components::virtual_list::virtual_list;
use crate::core::reactivity::{Signal, use_signal};
use crate::core::scroll_viewport::{scroll_offset_x_signal_key, scroll_viewport_w_signal_key};
use crate::core::tree::{Element, LayoutStatus};
use crate::text::{DiffDocument, DiffLineKind, DiffMode, DiffRow};
use crate::utils::text_annotations::{
    DocumentHighlights, DocumentSyntax, TextDecoration, TextRangeStyle,
};

pub fn diff_view(id: impl Into<String>, document: DiffDocument) -> DiffViewBuilder {
    DiffViewBuilder {
        id: id.into(),
        document,
        document_signal: None,
        mode: DiffMode::Unified,
        viewport_h: 320.0,
        row_height: 22.0,
        selectable: true,
        file_names: None,
        highlights: (Vec::new(), Vec::new()),
        inline_changes: true,
        language: None,
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
    file_names: Option<(String, String)>,
    highlights: (Vec<TextDecoration>, Vec<TextDecoration>),
    inline_changes: bool,
    language: Option<String>,
    chrome: SlotChrome,
}

impl DiffViewBuilder {
    crate::impl_slot_chrome_methods!();

    /// Ordered highlights in each side's original UTF-8 source coordinates.
    /// Bound to the initial snapshots; replaced sources discard stale highlights.
    pub fn highlights(
        mut self,
        original: Vec<TextDecoration>,
        modified: Vec<TextDecoration>,
    ) -> Self {
        self.highlights = (original, modified);
        self
    }

    /// Highlight precomputed grapheme changes within paired replacement lines.
    /// Enabled by default; explicit highlights override these background colors.
    pub fn inline_changes(mut self, enabled: bool) -> Self {
        self.inline_changes = enabled;
        self
    }

    /// Parse both complete sources in the background, preserving multiline state.
    /// Requires `syntax-highlighting`; explicit styles override syntax colors.
    pub fn language(mut self, language: impl Into<String>) -> Self {
        self.language = Some(language.into());
        self
    }

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

    /// Show the original and modified file names above the scrollable content.
    /// The header remains inside the viewport in both diff modes.
    pub fn file_names(mut self, original: impl Into<String>, modified: impl Into<String>) -> Self {
        self.file_names = Some((original.into(), modified.into()));
        self
    }
}

fn diff_highlights(
    document: &DiffDocument,
    inline_changes: bool,
    explicit: &(Vec<TextDecoration>, Vec<TextDecoration>),
    source_revisions: (u64, u64),
    syntax: (
        Option<&jadix_document::AnnotatedText<[u8; 4]>>,
        Option<&jadix_document::AnnotatedText<[u8; 4]>>,
    ),
) -> (DocumentHighlights, DocumentHighlights) {
    let side = |source: &crate::text::TextDocument,
                ranges: &[std::ops::Range<usize>],
                color: &str,
                explicit: &[TextDecoration],
                revision,
                syntax| {
        let mut decorations = Vec::new();
        if inline_changes {
            let background = crate::style::parse_color(color)
                .map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8);
            decorations.extend(ranges.iter().cloned().map(|range| TextDecoration {
                range,
                value: TextRangeStyle {
                    background: Some(background),
                    ..Default::default()
                },
            }));
        }
        if source.revision() == revision {
            decorations.extend_from_slice(explicit);
        }
        DocumentHighlights::from_syntax(source, syntax, decorations)
    };
    (
        side(
            document.original(),
            document.original_inline_changes(),
            "destructive/25",
            &explicit.0,
            source_revisions.0,
            syntax.0,
        ),
        side(
            document.modified(),
            document.modified_inline_changes(),
            "success/25",
            &explicit.1,
            source_revisions.1,
            syntax.1,
        ),
    )
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

// Keep the two panes in the viewport while the shared scrollport supplies one
// horizontal offset. Only code moves inside each clip; signs and numbers stay put.
fn split_viewport(mut row: Element, viewport: Signal<f32>, offset: Signal<f32>) -> Element {
    for cell in &mut row.children {
        let content = cell.children.split_off(2);
        let mut code = Element::new()
            .display_flex()
            .flex_row()
            .gap(8.0)
            .w("fit-content")
            .flex_shrink(0.0)
            .translate(-offset.peek(), 0.0)
            .reactive_sync(move |element, _| {
                let x = -offset.peek();
                if element.style.translate[0] == x {
                    return LayoutStatus::Clean;
                }
                element.style.translate[0] = x;
                LayoutStatus::StyleChanged
            });
        code.children = content;
        let mut clip = Element::new()
            .min_w(0.0)
            .flex_grow(1.0)
            .overflow("hidden")
            .child(code);
        clip.set_attr("data-diff-code-clip", "true");
        cell.children.push(clip);
    }
    row = row
        .w(viewport.peek().max(1.0))
        .translate(offset.peek(), 0.0);
    row.set_attr("data-diff-split-viewport", "true");
    let width = std::cell::Cell::new(viewport.peek().max(1.0));
    row = row.reactive_sync(move |element, layout| {
        let next_width = viewport.peek().max(1.0);
        let x = offset.peek();
        if width.replace(next_width) != next_width {
            *element = std::mem::take(element).w(next_width).translate(x, 0.0);
            layout.mark_element_layout_dirty(element);
            LayoutStatus::LayoutChanged
        } else if element.style.translate[0] != x {
            element.style.translate[0] = x;
            LayoutStatus::StyleChanged
        } else {
            LayoutStatus::Clean
        }
    });
    // VirtualList sizes its immediate rows to the complete content strip.
    Element::new().child(row)
}

fn diff_row(
    id: &str,
    document: &DiffDocument,
    row: DiffRow,
    mode: DiffMode,
    selectable: bool,
    height: f32,
    highlights: &(DocumentHighlights, DocumentHighlights),
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
            highlight_source_line(
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
                &highlights.0,
            ),
            document.original(),
            row.original_line,
            row.kind != DiffLineKind::Equal,
        )
        .w("50%")
        .flex_shrink(0.0)
        .overflow_x("hidden");
        let right = ending_hint(
            highlight_source_line(
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
                &highlights.1,
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
        highlight_source_line(
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
            if side == "old" {
                &highlights.0
            } else {
                &highlights.1
            },
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
            file_names,
            highlights,
            inline_changes,
            language,
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
        let initial = source();
        let source_revisions = (initial.original().revision(), initial.modified().revision());
        let themed = language.is_some() || inline_changes;
        let syntax = (
            Rc::new(RefCell::new(DocumentSyntax::new(language.clone()))),
            Rc::new(RefCell::new(DocumentSyntax::new(language))),
        );
        let indexed = Rc::new(RefCell::new((
            (initial.revision(), crate::style::is_dark_theme(), 0, 0),
            diff_highlights(
                &initial,
                inline_changes,
                &highlights,
                source_revisions,
                (None, None),
            ),
        )));
        let key_syntax = (syntax.0.clone(), syntax.1.clone());
        let key_index = indexed.clone();
        let key = source.clone();
        let width = source.clone();
        let row_id = id.clone();
        let viewport = use_signal(&scroll_viewport_w_signal_key(&id), || 0.0_f32);
        let offset = use_signal(&scroll_offset_x_signal_key(&id), || 0.0_f32);
        let mut root: Element = virtual_list(&id)
            .height(viewport_h)
            .item_height(row_height)
            .overscan(3)
            .item_count_fn(move || count().rows(mode).len())
            .data_key_fn(move || {
                let document = key();
                let dark = crate::style::is_dark_theme();
                let mut old = key_syntax.0.borrow_mut();
                let mut new = key_syntax.1.borrow_mut();
                let generation = (
                    document.revision(),
                    themed && dark,
                    old.update(document.original(), dark),
                    new.update(document.modified(), dark),
                );
                let mut indexed = key_index.borrow_mut();
                if indexed.0 != generation {
                    *indexed = (
                        generation,
                        diff_highlights(
                            &document,
                            inline_changes,
                            &highlights,
                            source_revisions,
                            (old.colors(), new.colors()),
                        ),
                    );
                }
                format!("{generation:?}")
            })
            .content_width_fn(move || {
                let document = width();
                let widest = source_width(document.original(), true)
                    .max(source_width(document.modified(), true));
                if mode == DiffMode::Split {
                    widest + viewport.peek().max(1.0) / 2.0
                } else {
                    widest + 20.0
                }
            })
            .render_item(move |index| {
                let document = source();
                let row = diff_row(
                    &row_id,
                    &document,
                    document.rows(mode)[index],
                    mode,
                    selectable,
                    row_height,
                    &indexed.borrow().1,
                );
                if mode == DiffMode::Split {
                    split_viewport(row, viewport, offset)
                } else {
                    row
                }
            })
            .into();
        if let Some((original, modified)) = file_names {
            let label = |name| {
                Element::new()
                    .text(name)
                    .w("50%")
                    .min_w(0.0)
                    .flex_shrink(1.0)
                    .overflow("hidden")
                    .white_space("nowrap")
                    .text_overflow("ellipsis")
                    .text_line_height(row_height)
            };
            let mut header = Element::new()
                .display_flex()
                .flex_row()
                .w("100%")
                .min_w(0.0)
                .h(row_height)
                .flex_shrink(0.0)
                .overflow("hidden")
                .child(label(original))
                .child(label(modified));
            header.set_attr("data-diff-file-header", "true");
            root = Element::new()
                .display_flex()
                .flex_col()
                .w("100%")
                .min_w(0.0)
                .child(header)
                .child(root);
        }
        root.set_attr(
            "data-mode",
            if mode == DiffMode::Split {
                "split"
            } else {
                "unified"
            },
        );
        redraw_on_snapshot(
            cancel_syntax_on_unmount(
                paint_slot(
                    root.font_family("monospace")
                        .text_size(14.0)
                        .accesskit_role("group"),
                    "diff-view",
                    &chrome,
                ),
                vec![syntax.0, syntax.1],
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
plain [] file_names: Option<(String, String)>,
plain [] highlights: Option<(Vec<TextDecoration>, Vec<TextDecoration>)>,
plain [] inline_changes: Option<bool>,
plain [] language: Option<String>,
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
    if let Some((original, modified)) = file_names {
        builder = builder.file_names(original, modified);
    }
    if let Some((original, modified)) = highlights {
        builder = builder.highlights(original, modified);
    }
    if let Some(enabled) = inline_changes {
        builder = builder.inline_changes(enabled);
    }
    if let Some(language) = language {
        builder = builder.language(language);
    }
    builder.into()
}
}

#[cfg(all(test, feature = "product-tests"))]
#[path = "tests/diff_view.rs"]
mod tests;
