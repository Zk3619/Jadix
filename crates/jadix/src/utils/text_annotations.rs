//! Shared source-byte range styles for Input, Textarea and plain/code editors.
use crate::{
    core::{
        reactivity::{self, use_signal},
        tree::Element,
    },
    style::TextSpan,
};
use jadix_document::{AnnotatedText, TextDocument};
use std::{cell::RefCell, ops::Range, rc::Rc};

thread_local! { static REVISION: std::cell::Cell<u64> = const { std::cell::Cell::new(0) }; }

pub use jadix_document::TextAnnotation;

/// Metrics-neutral styles. Later colors win; decoration lines are additive.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextRangeStyle {
    pub color: Option<[u8; 4]>,
    pub background: Option<[u8; 4]>,
    pub underline: bool,
    pub strikethrough: bool,
}
pub type TextDecoration = TextAnnotation<TextRangeStyle>;

/// UI-side publication state. Parsing is owned by the backend-independent
/// document worker; only line indexing and element updates happen on the UI.
pub struct DocumentSyntax {
    worker: Option<jadix_document::syntax::SyntaxWorker>,
    language: Option<String>,
    key: Option<(u64, bool)>,
    colors: Option<AnnotatedText<[u8; 4]>>,
    generation: u64,
}

impl DocumentSyntax {
    pub fn new(language: Option<String>) -> Self {
        #[cfg(feature = "syntax-highlighting")]
        let worker = language.as_ref().and_then(|_| {
            jadix_document::syntax::SyntaxWorker::new(|| {
                crate::app::run_on_ui_thread(crate::app::runtime::request_redraw);
            })
            .ok()
        });
        #[cfg(not(feature = "syntax-highlighting"))]
        let worker = None;
        Self {
            worker,
            language,
            key: None,
            colors: None,
            generation: 0,
        }
    }

    /// Poll once before rendering a virtual window. Generation changes invalidate
    /// mounted rows, including when the source itself has not changed.
    pub fn update(&mut self, document: &TextDocument, dark: bool) -> u64 {
        let key = (document.revision(), self.language.is_some() && dark);
        if self.key != Some(key) {
            self.key = Some(key);
            self.colors = None;
            self.generation = self.generation.wrapping_add(1);
            if let (Some(worker), Some(language)) = (&mut self.worker, &self.language) {
                worker.request(document.clone(), language, dark);
            }
        }
        if let Some(colors) = self.worker.as_mut().and_then(|worker| worker.poll()) {
            self.colors = Some(colors);
            self.generation = self.generation.wrapping_add(1);
        }
        self.generation
    }

    pub fn colors(&self) -> Option<&AnnotatedText<[u8; 4]>> {
        self.colors.as_ref()
    }

    pub fn cancel(&mut self) {
        self.worker = None;
        self.colors = None;
    }
}

/// Source-byte styles indexed once by logical line and bound to a snapshot.
/// Viewers can render visible rows without scanning every annotation each frame.
#[derive(Clone, Debug)]
pub struct DocumentHighlights {
    revision: u64,
    lines: std::collections::HashMap<usize, Vec<TextDecoration>>,
}

impl DocumentHighlights {
    /// Parse syntax once per snapshot, then apply explicit styles over it.
    pub fn with_syntax(
        document: &TextDocument,
        language: Option<&str>,
        dark: bool,
        decorations: Vec<TextDecoration>,
    ) -> Self {
        let syntax = language.map(|language| {
            jadix_document::syntax::highlight_document(document.clone(), language, dark)
        });
        Self::from_syntax(document, syntax.as_ref(), decorations)
    }

    /// Project an already-parsed snapshot, ignoring colors from another revision.
    pub fn from_syntax(
        document: &TextDocument,
        syntax: Option<&AnnotatedText<[u8; 4]>>,
        decorations: Vec<TextDecoration>,
    ) -> Self {
        let mut styles = syntax
            .filter(|syntax| syntax.document().revision() == document.revision())
            .map(|syntax| {
                syntax
                    .annotations()
                    .iter()
                    .map(|annotation| TextDecoration {
                        range: annotation.range.clone(),
                        value: TextRangeStyle {
                            color: Some(annotation.value),
                            ..Default::default()
                        },
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        styles.extend(decorations);
        Self::new(document, styles)
    }

    pub fn new(document: &TextDocument, decorations: Vec<TextDecoration>) -> Self {
        let annotations = AnnotatedText::new(document.clone(), decorations);
        let mut lines = std::collections::HashMap::<usize, Vec<TextDecoration>>::new();
        for decoration in annotations.annotations() {
            let first = document.line_at_byte(decoration.range.start).unwrap();
            let last = document.line_at_byte(decoration.range.end).unwrap();
            for line in first..=last {
                let range = document.line_range(line).unwrap();
                let start = decoration.range.start.max(range.start);
                let end = decoration.range.end.min(range.end);
                if start < end {
                    lines.entry(line).or_default().push(TextDecoration {
                        range: start - range.start..end - range.start,
                        value: decoration.value.clone(),
                    });
                }
            }
        }
        Self {
            revision: document.revision(),
            lines,
        }
    }

    /// Stale annotations never migrate to a replacement document. Ranges refer
    /// to original UTF-8 bytes, including tabs, before any display expansion.
    /// Optional syntax is the base layer; later highlight colors take precedence.
    pub fn line_spans(
        &self,
        document: &TextDocument,
        line: usize,
        syntax: Option<&[TextSpan]>,
    ) -> Option<Vec<TextSpan>> {
        let text = document.line(line)?;
        let decorations = if document.revision() == self.revision {
            self.lines.get(&line).map(Vec::as_slice).unwrap_or_default()
        } else {
            &[]
        };
        compose(text, syntax, decorations, &[])
    }
}

#[cfg(test)]
mod document_highlight_tests {
    use super::*;

    #[test]
    fn document_highlights_layer_unicode_ranges_and_reject_stale_snapshots() {
        let document = TextDocument::new("\t旧🙂\r\nnext");
        let background = [30, 50, 70, 255];
        let foreground = [90, 110, 130, 255];
        let highlights = DocumentHighlights::new(
            &document,
            vec![
                TextDecoration {
                    range: 2..12,
                    value: TextRangeStyle {
                        background: Some(background),
                        ..Default::default()
                    },
                },
                TextDecoration {
                    range: 5..7,
                    value: TextRangeStyle {
                        color: Some(foreground),
                        ..Default::default()
                    },
                },
                TextDecoration {
                    range: 100..usize::MAX,
                    value: TextRangeStyle::default(),
                },
            ],
        );
        let syntax = [TextSpan::new("\t旧🙂").underline()];
        let spans = highlights.line_spans(&document, 0, Some(&syntax)).unwrap();
        assert_eq!(
            spans
                .iter()
                .map(|span| span.text.as_str())
                .collect::<String>(),
            "\t旧🙂"
        );
        assert!(spans.iter().all(|span| span.underline));
        assert_eq!(spans[0].background, None, "tab retains its source position");
        assert_eq!(spans[1].text, "旧");
        assert_eq!(spans[1].background, Some(background));
        assert_eq!(spans[2].text, "🙂");
        assert_eq!(spans[2].background, Some(background));
        assert_eq!(spans[2].color, Some(foreground));
        let second = highlights.line_spans(&document, 1, None).unwrap();
        assert_eq!(second[0].text, "ne");
        assert_eq!(second[0].background, Some(background));
        assert_eq!(second[1].text, "xt");
        assert_eq!(second[1].background, None);
        let replacement = TextDocument::new(document.text());
        assert!(highlights.line_spans(&replacement, 0, None).is_none());
        assert_eq!(
            highlights.line_spans(&replacement, 0, Some(&syntax)),
            Some(syntax.to_vec())
        );
    }
}

struct FieldAnnotations {
    text: AnnotatedText<TextRangeStyle>,
    syntax: Option<(String, Vec<TextSpan>)>,
    dirty: bool,
    display: String,
    preedit: Vec<(usize, usize)>,
    hidden: bool,
    spans: Option<Vec<TextSpan>>,
    revision: u64,
}

fn key(field: &str) -> String {
    format!("__jadix_text_annotations_{field}")
}

fn state(field: &str) -> Rc<RefCell<FieldAnnotations>> {
    use_signal(&key(field), || {
        Rc::new(RefCell::new(FieldAnnotations {
            text: AnnotatedText::new(
                TextDocument::new(
                    use_signal(
                        &super::text_field::input_value_signal_key(field),
                        String::new,
                    )
                    .peek(),
                ),
                Vec::new(),
            ),
            syntax: None,
            dirty: true,
            display: String::new(),
            preedit: Vec::new(),
            hidden: false,
            spans: None,
            revision: 0,
        }))
    })
    .peek()
}

/// Invalid ranges are discarded and UTF-8 edges expand outwards. Identical updates
/// do not notify or invalidate shaping. Setting the whole value clears decorations.
pub fn set_text_decorations(field: &str, decorations: Vec<TextDecoration>) {
    let state = state(field);
    let changed = {
        let mut state = state.borrow_mut();
        let value = use_signal(
            &super::text_field::input_value_signal_key(field),
            String::new,
        )
        .peek();
        let reset = state.text.document().text() != value;
        if reset {
            state.text.reset(TextDocument::new(value));
            state.syntax = None;
        }
        let old = state.text.annotations().to_vec();
        state.text.set_annotations(decorations);
        let changed = reset || state.text.annotations() != old;
        state.dirty |= changed;
        changed
    };
    if changed {
        crate::app::runtime::request_redraw();
    }
}

pub fn text_decorations(field: &str) -> Vec<TextDecoration> {
    if !reactivity::named_signal_exists(&key(field)) {
        return Vec::new();
    }
    state(field).borrow().text.annotations().to_vec()
}

pub(crate) fn remove(field: &str) {
    reactivity::remove_named_signal(&key(field));
}

pub(crate) fn reset(field: &str, value: &str) {
    if !reactivity::named_signal_exists(&key(field)) {
        return;
    }
    let state = state(field);
    let mut state = state.borrow_mut();
    state.text.reset(TextDocument::new(value));
    state.syntax = None;
    state.dirty = true;
}

pub(crate) fn value_changed(field: &str, before: &str, after: &str) {
    if before == after || !reactivity::named_signal_exists(&key(field)) {
        return;
    }
    let state = state(field);
    let mut state = state.borrow_mut();
    if state.text.document().text() == after {
        return;
    }
    if state.text.document().text() != before {
        state.text.reset(TextDocument::new(after));
    } else {
        let (range, insert) =
            jadix_document::annotations::replacement_between(before, after).unwrap();
        state
            .text
            .replace(range, insert)
            .expect("UTF-8 replacement derived from valid strings");
    }
    state.syntax = None;
    state.dirty = true;
}

pub(crate) fn apply_edits(
    field: &str,
    before: &str,
    edits: &super::text_multi_cursor::CursorEdits,
) {
    super::text_edit_history::note_cursor_edits(field, before, edits);
    if !reactivity::named_signal_exists(&key(field)) {
        return;
    }
    let state = state(field);
    let mut state = state.borrow_mut();
    if state.text.document().text() != before {
        state.text.reset(TextDocument::new(before));
    }
    state
        .text
        .replace_many(
            edits
                .iter()
                .map(|(range, insert)| (range.clone(), insert.as_str())),
        )
        .expect("validated cursor edits");
    state.syntax = None;
    state.dirty = true;
}

/// Syntax is a base layer, never a second writer of the rendered leaf spans.
pub fn set_text_field_syntax(field: &str, text: &str, spans: Option<Vec<TextSpan>>) {
    let state = state(field);
    let mut state = state.borrow_mut();
    state.syntax = spans.map(|spans| (text.to_string(), spans));
    state.dirty = true;
}

pub(crate) fn sync(
    field: &str,
    leaf: &mut Element,
    display: &str,
    preedit: &[(usize, usize)],
    hidden: bool,
) -> bool {
    if !reactivity::named_signal_exists(&key(field)) {
        return false;
    }
    let state = state(field);
    let mut state = state.borrow_mut();
    let committed = use_signal(
        &super::text_field::input_value_signal_key(field),
        String::new,
    )
    .peek();
    if state.text.document().text() != committed {
        state.text.reset(TextDocument::new(committed));
        state.syntax = None;
        state.dirty = true;
    }
    if state.dirty || state.display != display || state.preedit != preedit || state.hidden != hidden
    {
        state.spans = if hidden {
            None
        } else {
            let syntax = state
                .syntax
                .as_ref()
                .filter(|(text, _)| text == state.text.document().text());
            let projected = syntax
                .filter(|_| !preedit.is_empty())
                .and_then(|(text, spans)| project_syntax(text, spans, display, preedit));
            let syntax = if preedit.is_empty() {
                syntax.map(|(_, spans)| spans.as_slice())
            } else {
                projected.as_deref()
            };
            compose(display, syntax, state.text.annotations(), preedit)
        };
        state.display = display.to_string();
        state.preedit = preedit.to_vec();
        state.hidden = hidden;
        state.dirty = false;
        state.revision = REVISION.with(|revision| {
            let next = revision.get().wrapping_add(1);
            revision.set(next);
            next
        });
    }
    if leaf
        .attrs
        .get("jadix_text_annotation_revision")
        .and_then(|value| value.parse::<u64>().ok())
        == Some(state.revision)
    {
        return false;
    }
    leaf.attrs.insert(
        "jadix_text_annotation_revision".into(),
        state.revision.to_string(),
    );
    if leaf.text_spans == state.spans {
        return false;
    }
    leaf.text_spans = state.spans.clone();
    true
}

fn display_ranges(range: &Range<usize>, preedit: &[(usize, usize)]) -> Vec<Range<usize>> {
    let mut shift = 0;
    let mut start = range.start;
    let mut ranges = Vec::new();
    for &(lo, hi) in preedit {
        let anchor = lo - shift;
        if anchor >= range.end {
            break;
        }
        if anchor > start {
            ranges.push(start + shift..anchor + shift);
            start = anchor;
        }
        shift += hi - lo;
    }
    ranges.push(start + shift..range.end + shift);
    ranges
}

fn compose(
    text: &str,
    syntax: Option<&[TextSpan]>,
    decorations: &[TextDecoration],
    preedit: &[(usize, usize)],
) -> Option<Vec<TextSpan>> {
    let syntax = syntax.filter(|spans| spans_match_text(text, spans));
    if decorations.is_empty() {
        return syntax.map(<[TextSpan]>::to_vec);
    }
    let mut base = syntax
        .map(<[TextSpan]>::to_vec)
        .unwrap_or_else(|| vec![TextSpan::new(text)]);
    let mut boundaries = vec![0, text.len()];
    let mut offset = 0;
    let mut base_ends = Vec::with_capacity(base.len());
    for span in &mut base {
        offset += span.text.len();
        boundaries.push(offset);
        base_ends.push(offset);
        span.text.clear(); // Clone only the style when splitting a long syntax run.
    }
    let mut events = Vec::new();
    for (index, decoration) in decorations.iter().enumerate() {
        for range in display_ranges(&decoration.range, preedit) {
            boundaries.extend([range.start, range.end]);
            events.extend([(range.start, true, index), (range.end, false, index)]);
        }
    }
    boundaries.sort_unstable();
    boundaries.dedup();
    events.sort_unstable();
    let mut active = std::collections::BTreeSet::new();
    let (mut event, mut base_index) = (0, 0);
    let mut spans = Vec::new();
    for range in boundaries.windows(2) {
        let (start, end) = (range[0], range[1]);
        if start == end {
            continue;
        }
        while event < events.len() && events[event].0 <= start {
            let (_, begin, index) = events[event];
            if begin {
                active.insert(index);
            } else {
                active.remove(&index);
            }
            event += 1;
        }
        while start >= base_ends[base_index] && base_index + 1 < base.len() {
            base_index += 1;
        }
        let mut span = base[base_index].clone();
        span.text = text[start..end].to_string();
        for &index in &active {
            let style = &decorations[index].value;
            if style.color.is_some() {
                span.color = style.color;
            }
            if style.background.is_some() {
                span.background = style.background;
            }
            span.underline |= style.underline;
            span.strikethrough |= style.strikethrough;
        }
        spans.push(span);
    }
    Some(spans)
}

fn spans_match_text(text: &str, spans: &[TextSpan]) -> bool {
    let mut offset = 0;
    for span in spans {
        if !text[offset..].starts_with(&span.text) {
            return false;
        }
        offset += span.text.len();
    }
    offset == text.len()
}

// IME inserts display-only gaps. Keep committed syntax on both sides without
// assigning a committed token's style to text that has not been committed yet.
fn project_syntax(
    committed: &str,
    syntax: &[TextSpan],
    display: &str,
    preedit: &[(usize, usize)],
) -> Option<Vec<TextSpan>> {
    if !spans_match_text(committed, syntax) {
        return None;
    }
    let (mut source, mut end) = (0, 0);
    for &(lo, hi) in preedit {
        if lo < end || hi < lo || !display.is_char_boundary(lo) || !display.is_char_boundary(hi) {
            return None;
        }
        let plain = &display[end..lo];
        if !committed[source..].starts_with(plain) {
            return None;
        }
        source += plain.len();
        end = hi;
    }
    if committed[source..] != display[end..] {
        return None;
    }
    let (mut source, mut end) = (0, 0);
    let mut projected = Vec::with_capacity(syntax.len() + preedit.len() * 2);
    for span in syntax {
        if span.text.is_empty() {
            continue;
        }
        let next = source + span.text.len();
        let mut style = span.clone();
        style.text.clear();
        for range in display_ranges(&(source..next), preedit) {
            if end < range.start {
                projected.push(TextSpan::new(&display[end..range.start]));
            }
            if !range.is_empty() {
                let mut run = style.clone();
                run.text = display[range.clone()].to_string();
                projected.push(run);
            }
            end = range.end;
        }
        source = next;
    }
    if end < display.len() {
        projected.push(TextSpan::new(&display[end..]));
    }
    Some(projected)
}
