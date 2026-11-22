//! Fixed-height logical source lines, sharing document snapshots and syntax with
//! the other renderers. Selection is native per text leaf; editing uses editors.
use jadix::{
    portable::{clock::Timer, create_signal, on_cleanup, virtual_list::virtual_list_indexed,
        Element, IntoElementList, Signal},
    presentation::style::{InlineStyle, TextAppearance, TextOverflow},
};
pub use jadix_document::TextDocument;
use jadix_document::{syntax::SyntaxWorker, AnnotatedText};
use std::{cell::RefCell, ops::Range, rc::Rc, time::Duration};

pub struct TextViewBuilder {
    id: String,
    document: TextDocument,
    source: Option<Signal<TextDocument>>,
    language: Option<String>,
    dark: bool,
    line_numbers: bool,
    selectable: bool,
    row_height: f32,
    height: f32,
    overscan: usize,
}

pub fn text_view(id: impl Into<String>) -> TextViewBuilder {
    TextViewBuilder { id: id.into(), document: TextDocument::default(), source: None,
        language: None, dark: true, line_numbers: false, selectable: true,
        row_height: 22., height: 320., overscan: 3 }
}

struct SyntaxState {
    worker: Option<SyntaxWorker>,
    timer: Option<Timer>,
    colors: Option<AnnotatedText<[u8; 4]>>,
    revision: Signal<u64>,
    visible: Range<usize>,
    language: Option<String>,
    dark: bool,
}

impl SyntaxState {
    fn request(state: &Rc<RefCell<Self>>, document: TextDocument) {
        let mut current = state.borrow_mut();
        let visible = current.visible.clone();
        let dark = current.dark;
        let language = current.language.clone();
        let changed = match (&mut current.worker, language) {
            (Some(worker), Some(language)) => worker.request_visible(document, &language, dark, visible),
            _ => false,
        };
        if changed {
            current.colors = None;
            let revision = current.revision;
            drop(current);
            revision.set(revision.get().wrapping_add(1));
            Self::arm(state);
        }
    }

    fn arm(state: &Rc<RefCell<Self>>) {
        let timer = state.borrow().timer.clone();
        if let Some(timer) = timer {
            let weak = Rc::downgrade(state);
            // Only poll while a parse is in flight. The shared timer participates
            // in the native/Shell wake deadline and is cancelled with its owner.
            let _ = timer.arm(Duration::from_millis(16), move || {
                let Some(state) = weak.upgrade() else { return; };
                let mut current = state.borrow_mut();
                let result = current.worker.as_mut().and_then(SyntaxWorker::poll);
                let published = result.is_some();
                if published { current.colors = result; }
                let revision = current.revision;
                let pending = current.worker.as_ref().is_some_and(SyntaxWorker::is_pending);
                drop(current);
                if published { revision.set(revision.get().wrapping_add(1)); }
                if pending { Self::arm(&state); }
            });
        }
    }
}

impl TextViewBuilder {
    pub fn document(mut self, document: impl Into<TextDocument>) -> Self {
        self.document = document.into(); self.source = None; self
    }
    pub fn document_signal(mut self, document: Signal<TextDocument>) -> Self { self.source = Some(document); self }
    pub fn language(mut self, language: impl Into<String>) -> Self { self.language = Some(language.into()); self }
    pub fn dark(mut self, dark: bool) -> Self { self.dark = dark; self }
    pub fn line_numbers(mut self, enabled: bool) -> Self { self.line_numbers = enabled; self }
    pub fn selectable(mut self, enabled: bool) -> Self { self.selectable = enabled; self }
    pub fn row_height(mut self, height: f32) -> Self {
        assert!(height.is_finite() && height > 0.); self.row_height = height; self
    }
    pub fn viewport_h(mut self, height: f32) -> Self {
        assert!(height.is_finite() && height > 0.); self.height = height; self
    }
    pub fn overscan(mut self, lines: usize) -> Self { self.overscan = lines.min(1024); self }
    pub fn build(self) -> Element {
        let Self { id, document, source, language, dark, line_numbers, selectable, row_height, height, overscan } = self;
        let document: Rc<dyn Fn() -> TextDocument> = Rc::new(move || source.map_or_else(|| document.clone(), |source| source.get()));
        let count = create_signal(document().line_count());
        let worker = if cfg!(feature = "syntax-highlighting") && language.is_some() {
            SyntaxWorker::new(|| {}).ok()
        } else { None };
        let timer = worker.as_ref().map(|_| Timer::new().expect("TextView syntax requires a reactive owner"));
        let state = Rc::new(RefCell::new(SyntaxState { worker, timer, colors: None,
            revision: create_signal(0), visible: 0..((height / row_height).ceil() as usize), language, dark }));
        let cleanup = Rc::downgrade(&state);
        on_cleanup(move || {
            if let Some(state) = cleanup.upgrade() {
                let mut state = state.borrow_mut();
                if let Some(timer) = state.timer.take() { timer.cancel(); }
                state.worker.take();
            }
        });
        let row_state = state.clone();
        let row_document = document.clone();
        let visible_state = state.clone();
        let visible_document = document.clone();
        let mut list = virtual_list_indexed(count.get(), row_height as f64, move |index| {
            let key_document = row_document.clone();
            let render_document = row_document.clone();
            let key_state = row_state.clone();
            let render_state = row_state.clone();
            Element::new().flex_row().bind_children_with(move || {
                let revision = key_state.borrow().revision.get();
                (key_document().revision(), index.get(), revision)
            }, move |(_, line, _)| {
                let document = render_document();
                let mut children = Vec::new();
                if line_numbers {
                    let mut gutter = Element::new().text(format!("{}  ", line + 1)).flex_shrink(0.);
                    gutter.descriptor.style.font_family = "monospace".into();
                    gutter.descriptor.style.font_size = 14.;
                    children.push(gutter);
                }
                if let Some(range) = document.line_range(line) {
                    let state = render_state.borrow();
                    let colors = state.colors.as_ref().filter(|colors| colors.document() == &document);
                    children.extend(source_chunks(&document, range, colors, selectable, row_height));
                }
                children
            }, |region| region.flex_row()).min_w(0.)
        }).id(id).count(count).h(height).overscan(overscan)
            .on_visible_range_change(move |visible| {
                if let Some(visible) = visible { visible_state.borrow_mut().visible = visible; }
                SyntaxState::request(&visible_state, visible_document());
            }).build();
        let width_document = document.clone();
        list.children[0].descriptor_binding = Some(Rc::new(move |descriptor| {
            let document = width_document();
            let gutter = if line_numbers { document.line_count().to_string().len() + 2 } else { 0 };
            // A conservative monospace width keeps distant long lines reachable
            // without mounting them just to measure their horizontal extent.
            descriptor.style.min_width = ((document.max_line_width_units().saturating_add(gutter)) as f64 * 14.)
                .min(16_777_216.) as f32;
        }));
        // Refresh count before the virtual list evaluates its visible keys. No
        // source String or annotation vector is cloned per mounted line.
        list.descriptor_binding = Some(Rc::new(move |_| {
            let document = document();
            if count.get() != document.line_count() { count.set(document.line_count()); }
            SyntaxState::request(&state, document);
        }));
        list
    }
}

fn source_chunks(document: &TextDocument, range: Range<usize>, colors: Option<&AnnotatedText<[u8; 4]>>,
    selectable: bool, row_height: f32) -> Vec<Element> {
    let annotations = colors.map_or(&[][..], |colors| colors.annotations());
    // Syntax output is ordered and nonoverlapping. Avoid scanning the complete
    // document's token vector for each viewport line.
    let first = annotations.partition_point(|annotation| annotation.range.end <= range.start);
    let mut annotations = annotations[first..].iter().take_while(|annotation| annotation.range.start < range.end).peekable();
    let mut start = range.start;
    let mut result = Vec::new();
    loop {
        let mut end = start.saturating_add(1024 * 1024).min(range.end);
        while !document.text().is_char_boundary(end) { end -= 1; }
        let mut styles = Vec::new();
        while let Some(annotation) = annotations.peek() {
            if annotation.range.start >= end { break; }
            if styles.len() == 64 { end = annotation.range.start.max(start); break; }
            styles.push(InlineStyle { range: [annotation.range.start.max(start) - start, annotation.range.end.min(end) - start],
                foreground: Some(annotation.value), ..Default::default() });
            if annotation.range.end <= end { annotations.next(); } else { break; }
        }
        let mut leaf = Element::new().text(&document.text()[start..end]).flex_shrink(0.);
        leaf.descriptor.style.font_family = "monospace".into();
        leaf.descriptor.style.font_size = 14.;
        leaf.descriptor.style.text_overflow = TextOverflow::Clip;
        leaf.descriptor.style.max_lines = Some(1);
        leaf.descriptor.style.text = Some(TextAppearance { selectable: Some(selectable), line_height: Some(row_height), ..Default::default() });
        leaf.descriptor.style.inline_styles = styles;
        result.push(leaf);
        if end == range.end { break; }
        start = end;
    }
    result
}

impl From<TextViewBuilder> for Element { fn from(value: TextViewBuilder) -> Self { value.build() } }
impl IntoElementList for TextViewBuilder { fn into_element_list(self) -> Vec<Element> { vec![self.build()] } }
