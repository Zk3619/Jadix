//! Virtualized semantic documents and reusable native block rendering.

use std::rc::Rc;

use crate::components::base::{paint_slot, SlotChrome};
use crate::components::document_viewport::{document_viewport, DocumentSurface};
use crate::components::math_view::{chemistry_view, math_view, render_diagnostic};
use crate::components::text_view::redraw_on_snapshot;
use crate::core::reactivity::Signal;
use crate::core::tree::Element;
use crate::style::TextSpan;
use crate::text::{
    document_inline_spans, is_safe_document_destination, Document, DocumentBlock,
    DocumentBlockKind, DocumentInline, DocumentInlineKind,
};

pub type DocumentBlockRenderer = Rc<dyn Fn(&Document, &DocumentBlock) -> Option<Element>>;

/// A controlled task change request against the current document snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentTaskToggle {
    pub block_id: u64,
    pub revision: u64,
    pub source: std::ops::Range<usize>,
    pub checked: bool,
}

pub type DocumentTaskHandler = Rc<dyn Fn(DocumentTaskToggle)>;
pub type DocumentLinkHandler = Rc<dyn Fn(&str)>;

/// A view-local link hover transition. `None` dismisses the host's preview.
/// Coordinates are window logical pixels; the component does not fetch the URL.
#[derive(Clone, Debug, PartialEq)]
pub struct DocumentLinkHover {
    pub view_id: String,
    pub href: Option<String>,
    pub position: [f32; 2],
}

pub type DocumentLinkHoverHandler = Rc<dyn Fn(DocumentLinkHover)>;

pub(crate) fn is_table_separator(document: &Document, block: &DocumentBlock) -> bool {
    block.kind == DocumentBlockKind::Syntax
        && block
            .edit_range
            .as_ref()
            .and_then(|range| document.block_at(range.start))
            .is_some_and(|index| {
                matches!(
                    document.blocks()[index].kind,
                    DocumentBlockKind::TableRow { .. }
                )
            })
}

pub fn document_view(id: impl Into<String>) -> DocumentViewBuilder {
    DocumentViewBuilder {
        id: id.into(),
        document: Document::default(),
        signal: None,
        viewport_h: 480.0,
        overscan: 3,
        renderer: None,
        on_task_toggle: None,
        on_link: None,
        on_link_hover: None,
        selectable: true,
        asset_root: None,
        chrome: SlotChrome::default(),
    }
}

pub struct DocumentViewBuilder {
    id: String,
    document: Document,
    signal: Option<Signal<Document>>,
    viewport_h: f32,
    overscan: usize,
    renderer: Option<DocumentBlockRenderer>,
    on_task_toggle: Option<DocumentTaskHandler>,
    on_link: Option<DocumentLinkHandler>,
    on_link_hover: Option<DocumentLinkHoverHandler>,
    selectable: bool,
    asset_root: Option<std::path::PathBuf>,
    chrome: SlotChrome,
}

impl DocumentViewBuilder {
    crate::impl_slot_chrome_methods!();
    pub fn asset_root(mut self, path: impl Into<std::path::PathBuf>) -> Self {
        self.asset_root = Some(path.into());
        self
    }
    pub fn document(mut self, document: impl Into<Document>) -> Self {
        self.document = document.into();
        self.signal = None;
        self
    }
    pub fn document_signal(mut self, document: Signal<Document>) -> Self {
        self.signal = Some(document);
        self
    }
    pub fn viewport_h(mut self, height: f32) -> Self {
        self.viewport_h = height.max(1.0);
        self
    }
    pub fn overscan(mut self, blocks: usize) -> Self {
        self.overscan = blocks;
        self
    }
    pub fn selectable(mut self, selectable: bool) -> Self {
        self.selectable = selectable;
        self
    }
    /// Enable focusable task checkboxes. The application owns document updates;
    /// requests identify a live block rather than its virtual row position.
    pub fn on_task_toggle(mut self, handler: impl Fn(DocumentTaskToggle) + 'static) -> Self {
        self.on_task_toggle = Some(Rc::new(handler));
        self
    }
    /// Activate an inline destination. The application decides how to navigate.
    pub fn on_link(mut self, handler: impl Fn(&str) + 'static) -> Self {
        self.on_link = Some(Rc::new(handler));
        self
    }

    /// Observe link changes for a host-owned preview, scoped to this view ID.
    /// Plain text, pointer leave/press and unmount dismiss an active preview.
    pub fn on_link_hover(mut self, handler: impl Fn(DocumentLinkHover) + 'static) -> Self {
        self.on_link_hover = Some(Rc::new(handler));
        self
    }
    pub fn block_renderer(
        mut self,
        renderer: impl Fn(&Document, &DocumentBlock) -> Option<Element> + 'static,
    ) -> Self {
        self.renderer = Some(Rc::new(renderer));
        self
    }
}

pub(crate) fn inline_spans(inlines: &[DocumentInline]) -> Vec<TextSpan> {
    let mut spans = document_inline_spans(inlines);
    for (inline, span) in inlines.iter().zip(&mut spans) {
        if inline.style.code {
            span.padding_x = 2.0;
            if inline.style.background.is_none() {
                span.background = Some(crate::style::parse_text_color("muted"));
            }
        }
        if span.href.is_some() {
            span.color = Some(crate::style::parse_text_color("primary"));
        }
    }
    spans
}

fn text_leaf(inlines: &[DocumentInline], size: f32) -> Element {
    Element::new()
        .text_spans(inline_spans(inlines))
        .text_size(size)
        .font_weight(400)
        .text_leading(1.5)
        .min_w(0.0)
        .max_w("100%")
}

fn render_inlines(
    inlines: &[DocumentInline],
    size: f32,
    asset_root: Option<&std::path::Path>,
) -> Element {
    if inlines.iter().all(|inline| {
        inline.kind == DocumentInlineKind::Text
            && !inline.style.superscript
            && !inline.style.subscript
    }) {
        return text_leaf(inlines, size).w("100%");
    }
    let mut row = Element::new()
        .display_flex()
        .flex_row()
        .flex_wrap_kw("wrap")
        .items_center()
        .min_w(0.0)
        .gap(2.0);
    for inline in inlines {
        let element = match &inline.kind {
            DocumentInlineKind::Math => math_view(&inline.text).font_size(size).into(),
            DocumentInlineKind::Image { destination, title }
                if is_safe_document_destination(destination)
                    || std::path::Path::new(destination).is_absolute() =>
            {
                let destination =
                    crate::utils::document_assets::image_path(destination, asset_root)
                        .map(|path| path.to_string_lossy().to_string())
                        .unwrap_or_else(|| destination.clone());
                #[cfg(feature = "webview")]
                if destination.starts_with("https://") {
                    let escaped = destination
                        .replace('&', "&amp;")
                        .replace('"', "&quot;")
                        .replace('<', "&lt;");
                    use std::hash::{Hash, Hasher};
                    let mut identity = std::collections::hash_map::DefaultHasher::new();
                    destination.hash(&mut identity);
                    inline.source.start.hash(&mut identity);
                    row = row.child(crate::components::html_document::html_document_fragment(format!("document-image-{:x}", identity.finish()), &format!("<img alt=\"\" src=\"{escaped}\" style=\"max-width:100%;max-height:600px\">" )).w("100%"));
                    continue;
                }
                let (width, height) = image::image_dimensions(&destination)
                    .ok()
                    .filter(|(width, height)| *width > 0 && *height > 0)
                    .map(|(width, height)| {
                        let factor = (640.0 / width as f32).min(480.0 / height as f32).min(1.0);
                        (width as f32 * factor, height as f32 * factor)
                    })
                    .unwrap_or((320.0, 220.0));
                let mut image: Element = crate::components::image::image()
                    .src(&destination)
                    .style(move |style| style.w(width).h(height))
                    .into();
                image = image.max_w("100%");
                image.set_attr(
                    "aria-label",
                    if title.is_empty() {
                        &inline.text
                    } else {
                        title
                    },
                );
                image
            }
            DocumentInlineKind::Image { .. } => {
                render_diagnostic(&inline.text, "Unsafe image destination is blocked")
            }
            _ => {
                let mut leaf = text_leaf(std::slice::from_ref(inline), size)
                    .w("fit-content")
                    .flex_shrink(0.0);
                if inline.style.code {
                    let font_size = inline.style.font_size.map(f32::from).unwrap_or(size);
                    let prepared = crate::text::prepare_with_family(
                        &inline.text,
                        font_size,
                        if inline.style.bold { 700 } else { 400 },
                        font_size * 1.5,
                        glyphon::Family::Monospace,
                    );
                    let width = crate::text::intrinsic_width(&prepared).ceil() + 4.0;
                    // Reserve a short code span's complete width beside inline objects,
                    // but let the shared text layout wrap an oversized span at the
                    // container width. Keep one span so padding is not added per glyph.
                    leaf = leaf.w(width).min_w(0.0).max_w("100%");
                }
                if inline.style.superscript {
                    leaf = leaf.text_size(size * 0.75).translate(0.0, -size * 0.25);
                }
                if inline.style.subscript {
                    leaf = leaf.text_size(size * 0.75).translate(0.0, size * 0.2);
                }
                leaf
            }
        };
        row = row.child(element);
    }
    row
}

pub(crate) fn render_document_block(
    document: &Document,
    block: &DocumentBlock,
    renderer: &Option<DocumentBlockRenderer>,
    asset_root: Option<&std::path::Path>,
    task: Option<Element>,
) -> Element {
    if let Some(custom) = renderer
        .as_ref()
        .and_then(|renderer| renderer(document, block))
    {
        return custom;
    }
    let mut result = match &block.kind {
        DocumentBlockKind::Blank => Element::new().h(12.0),
        DocumentBlockKind::Syntax => {
            if is_table_separator(document, block) {
                return Element::new().h(0.0);
            }
            let opening = block
                .edit_range
                .as_ref()
                .is_some_and(|range| range.start == block.source.start);
            if opening {
                let language = block
                    .text()
                    .trim()
                    .trim_start_matches(['`', '~'])
                    .trim()
                    .to_string();
                Element::new()
                    .text(if language.is_empty() {
                        "代码".into()
                    } else {
                        language
                    })
                    .text_size(12.0)
                    .text_color("muted-foreground")
                    .bg("muted")
                    .px(12.0)
                    .py(6.0)
            } else {
                Element::new().h(8.0).bg("muted")
            }
        }
        DocumentBlockKind::Rule => Element::new()
            .h(16.0)
            .py(7.0)
            .child(Element::new().h(1.0).w("100%").bg("border")),
        DocumentBlockKind::Heading(level) => {
            #[cfg(feature = "webview")]
            if let Some(html) = &block.payload {
                return crate::components::html_document::html_document_fragment_with_assets(
                    format!("document-html-{}", block.id),
                    html,
                    asset_root,
                );
            }
            let size =
                [32.0, 28.0, 24.0, 21.0, 18.0, 16.0][level.saturating_sub(1).min(5) as usize];
            render_inlines(&block.inlines, size, asset_root)
                .font_weight(700)
                .py(6.0)
        }
        DocumentBlockKind::Quote { depth } => Element::new()
            .display_flex()
            .flex_row()
            .gap(10.0)
            .px((*depth as f32 * 8.0).min(64.0))
            .py(2.0)
            .child(Element::new().w(3.0).min_h(24.0).bg("border"))
            .child(render_inlines(&block.inlines, 16.0, asset_root).flex_grow(1.0)),
        DocumentBlockKind::ListItem {
            marker,
            depth,
            checked,
        } => {
            let marker_element = if let Some(checked) = checked {
                task.unwrap_or_else(|| task_checkbox(*checked, &block.text()))
            } else {
                Element::new()
                    .text(marker)
                    .min_w(24.0)
                    .text_size(16.0)
                    .text_leading(1.5)
            };
            Element::new()
                .display_flex()
                .flex_row()
                .gap(8.0)
                .px((depth.saturating_sub(1) as f32 * 16.0).min(160.0))
                .py(2.0)
                .child(marker_element)
                .child(
                    render_inlines(&block.inlines, 16.0, asset_root)
                        .flex_grow(1.0)
                        .min_w(0.0),
                )
        }
        DocumentBlockKind::Code { language } => {
            #[cfg(feature = "syntax-highlighting")]
            let spans = if let (Some(payload), Some(range)) = (&block.payload, &block.edit_range) {
                let first = document.source().line_at_byte(range.start).unwrap_or(0);
                let current = document
                    .source()
                    .line_at_byte(block.source.start)
                    .unwrap_or(first);
                let fenced = document.source().text()[range.clone()]
                    .trim_start()
                    .starts_with(['`', '~']);
                let line = current
                    .saturating_sub(first)
                    .saturating_sub(usize::from(fenced));
                if payload.len() > 256 * 1024 {
                    vec![TextSpan::new(block.text())]
                } else {
                    crate::text::syntax::highlight_block_line(
                        payload,
                        language,
                        crate::style::is_dark_theme(),
                        line,
                    )
                }
            } else {
                crate::text::syntax::highlight_code(
                    &block.text(),
                    language,
                    crate::style::is_dark_theme(),
                )
            };
            #[cfg(not(feature = "syntax-highlighting"))]
            let spans = {
                let _ = language;
                vec![TextSpan::new(block.text())]
            };
            let mut code = Element::new()
                .text_spans(spans)
                .text_size(14.0)
                .text_leading(1.5)
                .font_family("monospace")
                .white_space("pre")
                .px(12.0)
                .py(2.0)
                .min_h(25.0)
                .bg("muted");
            code.set_attr("data-language", language);
            code
        }
        DocumentBlockKind::TableRow { header } => {
            let mut row = Element::new()
                .display_flex()
                .flex_row()
                .w("100%")
                .min_h(32.0)
                .flex_items("stretch")
                .border("border", 1.0);
            if !header {
                row = row.border_top_width(0.0);
            }
            row = row.accesskit_role("row");
            row.set_attr(
                "data-document-table-row",
                if *header { "header" } else { "body" },
            );
            for (position, cell) in block.cells.iter().enumerate() {
                let mut content = render_inlines(cell, 16.0, asset_root)
                    .w("100%")
                    .text_line_height(24.0);
                if *header {
                    content = content.font_weight(700);
                }
                let alignment = match block.alignments.get(position) {
                    Some(crate::text::DocumentAlignment::Center) => "center",
                    Some(crate::text::DocumentAlignment::Right) => "right",
                    _ => "left",
                };
                content = content.text_align(alignment);
                let mut cell = Element::new()
                    .display_flex()
                    .flex_col()
                    .flex_grow(1.0)
                    .flex_basis(0.0)
                    .min_w(0.0)
                    .px(12.0)
                    .py(8.0)
                    .text_align(alignment)
                    .child(content);
                if position + 1 < block.cells.len() {
                    cell = cell.border_right("border", 1.0);
                }
                cell = cell.accesskit_role(if *header { "columnheader" } else { "cell" });
                if *header {
                    cell = cell.font_weight(700).bg("muted");
                }
                row = row.child(cell);
            }
            row
        }
        DocumentBlockKind::Html => {
            #[cfg(feature = "webview")]
            {
                crate::components::html_document::html_document_fragment_with_assets(
                    format!("document-html-{}", block.id),
                    block.payload.as_deref().unwrap_or(""),
                    asset_root,
                )
            }
            #[cfg(not(feature = "webview"))]
            {
                render_diagnostic(
                    block.payload.as_deref().unwrap_or(""),
                    "HTML layout requires the webview feature",
                )
            }
        }
        DocumentBlockKind::Math => Element::new()
            .display_flex()
            .flex_row()
            .items_center()
            .py(8.0)
            .child(math_view(block.payload.as_deref().unwrap_or("")).display(true)),
        DocumentBlockKind::Chemistry => Element::new()
            .display_flex()
            .flex_row()
            .items_center()
            .py(8.0)
            .child(chemistry_view(block.payload.as_deref().unwrap_or(""))),
        #[cfg(feature = "document-science")]
        DocumentBlockKind::Molecule { format } => {
            crate::components::molecule_view::render_molecule_source(
                block.payload.as_deref().unwrap_or(""),
                format,
            )
        }
        #[cfg(not(feature = "document-science"))]
        DocumentBlockKind::Molecule { .. } => render_diagnostic(
            block.payload.as_deref().unwrap_or(""),
            "Molecule rendering requires the document-science feature",
        ),
        _ => render_inlines(&block.inlines, 16.0, asset_root).py(2.0),
    };
    result.set_attr("data-document-kind", format!("{:?}", block.kind));
    result.set_attr(
        "accesskit_role",
        if matches!(block.kind, DocumentBlockKind::Heading(_)) {
            "heading"
        } else {
            "group"
        },
    );
    result
}

impl From<DocumentViewBuilder> for Element {
    fn from(builder: DocumentViewBuilder) -> Self {
        let id = crate::utils::resolve_instance_id(builder.id, "document-view");
        let signal = builder.signal;
        let source = Rc::new(move || DocumentSurface {
            document: signal
                .and_then(|signal| signal.try_peek())
                .unwrap_or_else(|| builder.document.clone()),
            active: None,
        });
        let selection_id = id.clone();
        let task_source = source.clone();
        let render = Rc::new(
            move |surface: &DocumentSurface, index: usize, _width: f32, _active: bool| {
                let document_block = &surface.document.blocks()[index];
                let task = match (&document_block.kind, &builder.on_task_toggle) {
                    (
                        DocumentBlockKind::ListItem {
                            checked: Some(checked),
                            ..
                        },
                        Some(handler),
                    ) => {
                        let block_id = document_block.id;
                        let source = task_source.clone();
                        let handler = handler.clone();
                        let mut checkbox = task_checkbox(*checked, &document_block.text())
                            .focus_id(format!("document:{selection_id}:task:{block_id}"))
                            .focusable(true)
                            .cursor("pointer")
                            .on_click(move || {
                                if signal.is_some_and(|signal| signal.try_peek().is_none()) {
                                    return;
                                }
                                let document = source().document;
                                let Some(block) =
                                    document.blocks().iter().find(|block| block.id == block_id)
                                else {
                                    return;
                                };
                                if let DocumentBlockKind::ListItem {
                                    checked: Some(checked),
                                    ..
                                } = block.kind
                                {
                                    handler(DocumentTaskToggle {
                                        block_id,
                                        revision: document.revision(),
                                        source: block.source.clone(),
                                        checked: !checked,
                                    });
                                }
                            });
                        checkbox.set_attr("aria-readonly", "false");
                        Some(checkbox)
                    }
                    _ => None,
                };
                let mut block = render_document_block(
                    &surface.document,
                    &surface.document.blocks()[index],
                    &builder.renderer,
                    builder.asset_root.as_deref(),
                    task,
                );
                if builder.selectable {
                    make_selectable(
                        &mut block,
                        &format!(
                            "document:{selection_id}:{}",
                            surface.document.blocks()[index].id
                        ),
                        &mut 0,
                    );
                }
                block
            },
        );
        let root = document_viewport(
            &id,
            builder.viewport_h,
            builder.overscan,
            source,
            render,
            None,
        );
        let mut root = paint_slot(root, "document-view", &builder.chrome);
        if let Some(on_link) = builder.on_link {
            root = root.on_text_link(move |event| {
                if let Some(link) = &event.text_link {
                    on_link(&link.href);
                    event.stop_propagation();
                }
            });
        }
        if let Some(on_hover) = builder.on_link_hover {
            let active = Rc::new(std::cell::Cell::new(false));
            let position = Rc::new(std::cell::Cell::new([0.0; 2]));
            root = root.on_text_link_hover({
                let on_hover = on_hover.clone();
                let id = id.clone();
                let active = active.clone();
                let position = position.clone();
                move |event| {
                    let href = event
                        .hovered_text_link
                        .as_ref()
                        .map(|link| link.href.clone());
                    if href.is_none() && !active.get() {
                        return;
                    }
                    active.set(href.is_some());
                    position.set([event.x, event.y]);
                    on_hover(DocumentLinkHover {
                        view_id: id.clone(),
                        href,
                        position: position.get(),
                    });
                }
            });
            let previous = root.on_unmount.take();
            root = root.on_unmount(move || {
                if active.replace(false) {
                    on_hover(DocumentLinkHover {
                        view_id: id.clone(),
                        href: None,
                        position: position.get(),
                    });
                }
                if let Some(previous) = previous {
                    previous.fire();
                }
            });
        }
        redraw_on_snapshot(root, signal)
    }
}

crate::__component_contract! {
DocumentViewProps [];
#[allow(non_snake_case)]
pub fn DocumentView(
identity [] id: String,
plain [] document: Option<Document>,
plain [] value: Option<Signal<Document>>,
plain [] text: Option<String>,
plain [] viewport_h: Option<f32>,
plain [] selectable: Option<bool>,
plain [] on_task_toggle: Option<DocumentTaskHandler>,
plain [] on_link: Option<DocumentLinkHandler>,
plain [] on_link_hover: Option<DocumentLinkHoverHandler>,
plain [] children: Option<crate::core::tree::Element>,
) -> Element {
    let mut builder = document_view(id);
    if let Some(document) = document {
        builder = builder.document(document);
    }
    if let Some(text) = text {
        builder = builder.document(text);
    }
    if let Some(value) = value {
        builder = builder.document_signal(value);
    }
    if let Some(height) = viewport_h {
        builder = builder.viewport_h(height);
    }
    if let Some(selectable) = selectable {
        builder = builder.selectable(selectable);
    }
    builder.on_task_toggle = on_task_toggle;
    builder.on_link = on_link;
    builder.on_link_hover = on_link_hover;
    builder.into()
}
}

fn task_checkbox(checked: bool, label: &str) -> Element {
    let mut checkbox = Element::new()
        .w(16.0)
        .h(16.0)
        .min_w(16.0)
        .mt(4.0)
        .rounded(3.0)
        .border("muted-foreground", 1.0)
        .text(if checked { "✓" } else { "" })
        .text_size(13.0)
        .text_align("center")
        .text_leading(1.0);
    checkbox.set_attr("accesskit_role", "checkbox");
    checkbox.set_attr("aria-checked", checked.to_string());
    checkbox.set_attr("aria-readonly", "true");
    checkbox.set_attr("aria-label", label);
    checkbox
}

fn make_selectable(element: &mut Element, prefix: &str, counter: &mut usize) {
    if element.text.is_some()
        && element.text_style.input_caret.is_none()
        && element.focus_id.is_none()
    {
        let id = format!("{prefix}:{}", *counter);
        *counter += 1;
        crate::components::label::enable_label_selection(element, &id);
        let selection = crate::components::label::label_selection_signal_key(&id);
        let previous = element.on_unmount.take();
        *element = std::mem::take(element).on_unmount(move || {
            crate::core::reactivity::remove_named_signal(&selection);
            if let Some(previous) = previous {
                previous.fire();
            }
        });
    }
    for child in &mut element.children {
        make_selectable(child, prefix, counter);
    }
}
