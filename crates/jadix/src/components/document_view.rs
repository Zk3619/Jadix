//! Virtualized semantic documents and reusable native block rendering.

use std::rc::Rc;

use crate::components::base::{paint_slot, SlotChrome};
use crate::components::document_viewport::{document_viewport, DocumentSurface};
use crate::components::math_view::{chemistry_view, math_view, render_diagnostic};
use crate::components::text_view::redraw_on_snapshot;
use crate::core::reactivity::Signal;
use crate::core::tree::Element;
use crate::style::TextSpan;
use crate::text::{Document, DocumentBlock, DocumentBlockKind, DocumentInline, DocumentInlineKind};
use jadix_macros::component;

pub type DocumentBlockRenderer = Rc<dyn Fn(&Document, &DocumentBlock) -> Option<Element>>;

pub fn document_view(id: impl Into<String>) -> DocumentViewBuilder {
    DocumentViewBuilder {
        id: id.into(),
        document: Document::default(),
        signal: None,
        viewport_h: 480.0,
        overscan: 3,
        renderer: None,
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
    pub fn block_renderer(
        mut self,
        renderer: impl Fn(&Document, &DocumentBlock) -> Option<Element> + 'static,
    ) -> Self {
        self.renderer = Some(Rc::new(renderer));
        self
    }
}

pub(crate) fn inline_spans(inlines: &[DocumentInline]) -> Vec<TextSpan> {
    inlines
        .iter()
        .map(|inline| {
            let mut span = TextSpan::new(&inline.text);
            if inline.style.bold {
                span = span.weight(700);
            }
            if inline.style.italic {
                span = span.italic();
            }
            if let Some(color) = inline.style.color {
                span = span.color(color);
            }
            if let Some(background) = inline.style.background {
                span = span.background(background);
            }
            if let Some(size) = inline.style.font_size {
                span = span.size(f32::from(size).clamp(8.0, 96.0));
            }
            if inline.style.code {
                span = span
                    .monospace()
                    .background(crate::style::parse_text_color("muted"))
                    .padding_x(2.0);
            }
            if let Some(link) = &inline.style.link {
                if safe_destination(link) {
                    span = span
                        .underline()
                        .color(crate::style::parse_text_color("primary"))
                        .href(link);
                }
            }
            span
        })
        .collect()
}

fn safe_destination(destination: &str) -> bool {
    let normalized = destination.trim().to_ascii_lowercase();
    if normalized.chars().any(|character| character.is_control()) {
        return false;
    }
    !normalized.contains(':')
        || normalized.starts_with("https:")
        || normalized.starts_with("http:")
        || normalized.starts_with("mailto:")
        || normalized.starts_with("data:image/")
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
            && !inline.style.strike
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
                if safe_destination(destination)
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
                    leaf = leaf.whitespace_nowrap().w(width).min_w(width);
                }
                if inline.style.strike {
                    leaf = leaf.text_decoration("line-through");
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
                let mut checkbox = Element::new()
                    .w(16.0)
                    .h(16.0)
                    .min_w(16.0)
                    .mt(4.0)
                    .rounded(3.0)
                    .border("muted-foreground", 1.0)
                    .text(if *checked { "✓" } else { "" })
                    .text_size(13.0)
                    .text_align("center")
                    .text_leading(1.0);
                checkbox.set_attr("accesskit_role", "checkbox");
                checkbox.set_attr("aria-checked", checked.to_string());
                checkbox
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
                .border("border", 1.0);
            for (position, cell) in block.cells.iter().enumerate() {
                let mut cell = render_inlines(cell, 16.0, asset_root)
                    .flex_grow(1.0)
                    .flex_basis(0.0)
                    .min_w(40.0)
                    .px(8.0)
                    .py(4.0);
                if *header {
                    cell = cell.font_weight(700).bg("muted");
                }
                if let Some(alignment) = block.alignments.get(position) {
                    cell = cell.text_align(match alignment {
                        pulldown_cmark::Alignment::Center => "center",
                        pulldown_cmark::Alignment::Right => "right",
                        _ => "left",
                    });
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
        DocumentBlockKind::Molecule { format } => {
            crate::components::molecule_view::render_molecule_source(
                block.payload.as_deref().unwrap_or(""),
                format,
            )
        }
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
        let render = Rc::new(
            move |surface: &DocumentSurface, index: usize, _width: f32, _active: bool| {
                let mut block = render_document_block(
                    &surface.document,
                    &surface.document.blocks()[index],
                    &builder.renderer,
                    builder.asset_root.as_deref(),
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
        redraw_on_snapshot(paint_slot(root, "document-view", &builder.chrome), signal)
    }
}

#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn DocumentView(
    id: String,
    document: Option<Document>,
    value: Option<Signal<Document>>,
    text: Option<String>,
    viewport_h: Option<f32>,
    selectable: Option<bool>,
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
    builder.into()
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
