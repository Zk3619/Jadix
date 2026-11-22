//! Source-preserving CommonMark documents. UI nodes are produced only by a viewport.

use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub use pulldown_cmark::Alignment as DocumentAlignment;
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag};
use serde::{Deserialize, Serialize};

use super::{DocumentEditError, TextDocument};

static NEXT_BLOCK: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DocumentFormat {
    #[default]
    Markdown,
    PlainText,
    RichText,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DocumentBlockKind {
    Paragraph,
    Heading(u8),
    Quote {
        depth: usize,
    },
    ListItem {
        marker: String,
        depth: usize,
        checked: Option<bool>,
    },
    Code {
        language: String,
    },
    TableRow {
        header: bool,
    },
    Math,
    Chemistry,
    Html,
    Molecule {
        format: String,
    },
    Rule,
    Blank,
    Syntax,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DocumentInlineStyle {
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
    pub superscript: bool,
    pub subscript: bool,
    pub link: Option<String>,
    pub color: Option<[u8; 4]>,
    pub background: Option<[u8; 4]>,
    pub font_size: Option<u16>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DocumentInlineKind {
    #[default]
    Text,
    Math,
    Image {
        destination: String,
        title: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentInline {
    pub text: String,
    pub source: Range<usize>,
    pub kind: DocumentInlineKind,
    pub style: DocumentInlineStyle,
}

pub use crate::is_safe_document_destination;

#[derive(Clone, Debug)]
pub struct DocumentBlock {
    pub id: u64,
    pub source: Range<usize>,
    pub edit_range: Option<Range<usize>>,
    pub kind: DocumentBlockKind,
    pub inlines: Arc<[DocumentInline]>,
    pub cells: Arc<[Arc<[DocumentInline]>]>,
    pub alignments: Arc<[DocumentAlignment]>,
    pub payload: Option<Arc<str>>,
    pub(super) independent: bool,
    source_positions: Arc<[Arc<[(usize, usize)]>]>,
}

impl DocumentBlock {
    pub fn text(&self) -> String {
        if !self.cells.is_empty() {
            return self
                .cells
                .iter()
                .map(|cell| {
                    cell.iter()
                        .map(|inline| inline.text.as_str())
                        .collect::<String>()
                })
                .collect::<Vec<_>>()
                .join("\t");
        }
        self.inlines
            .iter()
            .map(|inline| inline.text.as_str())
            .collect()
    }

    pub fn source_position(&self, inline: usize, display_byte: usize) -> Option<usize> {
        let inline_index = inline;
        let inline = self.inlines.get(inline)?;
        let local = inline
            .text
            .floor_char_boundary(display_byte.min(inline.text.len()));
        if let Some(positions) = self.source_positions.get(inline_index) {
            if let Some((display_offset, source_offset)) =
                positions.iter().rev().find(|(offset, _)| *offset <= local)
            {
                return Some(self.source.start + source_offset + local - display_offset);
            }
            if !positions.is_empty() {
                return Some(self.source.start + inline.source.start + local);
            }
        }
        let offset = if inline.text.len() == inline.source.len() {
            local
        } else if local == inline.text.len() {
            inline.source.len()
        } else {
            0
        };
        Some(self.source.start + inline.source.start + offset)
    }
}

#[derive(Debug)]
pub(super) struct DocumentSnapshot {
    pub(super) source: TextDocument,
    pub(super) format: DocumentFormat,
    pub(super) blocks: Vec<DocumentBlock>,
    pub(super) references: bool,
    pub(super) raw_html: bool,
}

#[derive(Clone, Debug)]
pub struct Document(pub(super) Arc<DocumentSnapshot>);

impl PartialEq for Document {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for Document {}
impl Default for Document {
    fn default() -> Self {
        Self::markdown("")
    }
}
impl From<&str> for Document {
    fn from(source: &str) -> Self {
        Self::markdown(source)
    }
}
impl From<String> for Document {
    fn from(source: String) -> Self {
        Self::markdown(source)
    }
}

impl Document {
    pub fn markdown(source: impl Into<TextDocument>) -> Self {
        Self::new(source.into(), DocumentFormat::Markdown)
    }

    /// Keep raw HTML tags and blocks as literal text when `enabled` is false.
    /// Markdown outside HTML blocks, including autolinks, remains semantic.
    /// HTML block contents remain literal; this is not an HTML sanitizer.
    pub fn markdown_with_raw_html(source: impl Into<TextDocument>, enabled: bool) -> Self {
        Self::new_with_raw_html(source.into(), DocumentFormat::Markdown, enabled)
    }
    pub fn plain_text(source: impl Into<TextDocument>) -> Self {
        Self::new(source.into(), DocumentFormat::PlainText)
    }

    pub fn new(source: TextDocument, format: DocumentFormat) -> Self {
        Self::new_with_raw_html(source, format, true)
    }

    fn new_with_raw_html(source: TextDocument, format: DocumentFormat, raw_html: bool) -> Self {
        let blocks = match format {
            DocumentFormat::Markdown => parse_blocks(&source, raw_html),
            DocumentFormat::PlainText | DocumentFormat::RichText => (0..source.line_count())
                .map(|line| plain_line(&source, line))
                .collect(),
        };
        let references = source.text().contains("]:");
        Self(Arc::new(DocumentSnapshot {
            source,
            format,
            blocks,
            references,
            raw_html,
        }))
    }

    pub fn source(&self) -> &TextDocument {
        &self.0.source
    }
    pub fn format(&self) -> DocumentFormat {
        self.0.format
    }
    pub fn raw_html_enabled(&self) -> bool {
        self.0.raw_html
    }
    pub fn revision(&self) -> u64 {
        self.source().revision()
    }
    pub fn blocks(&self) -> &[DocumentBlock] {
        &self.0.blocks
    }

    pub fn block_at(&self, byte: usize) -> Option<usize> {
        self.source().line_at_byte(byte)?;
        let position = self
            .blocks()
            .partition_point(|block| block.source.start <= byte);
        Some(
            position
                .saturating_sub(1)
                .min(self.blocks().len().saturating_sub(1)),
        )
    }

    pub fn replace(&self, range: Range<usize>, insert: &str) -> Result<Self, DocumentEditError> {
        let source = self.source().replace(range.clone(), insert)?;
        if self.format() == DocumentFormat::RichText {
            return Ok(super::rich_text::replace_rich(self, source, range, insert));
        }
        if self.format() == DocumentFormat::Markdown
            && range.is_empty()
            && range.start == self.source().len()
            && !self.0.references
            && !source.text().contains("]:")
        {
            if let Some(updated) = self.reparse_markdown_tail(&source) {
                return Ok(updated);
            }
        }
        if let Some(index) = self.block_at(range.start) {
            let block = &self.blocks()[index];
            let old = &self.source().text()[range.clone()];
            let fenced_code = matches!(block.kind, DocumentBlockKind::Code { .. })
                && block.edit_range.as_ref().is_some_and(|group| {
                    let opening = self.source().text()[group.clone()].trim_start();
                    opening.starts_with("```") || opening.starts_with("~~~")
                })
                && range.end <= block.source.end
                && !old.contains(['\r', '\n', '`', '~'])
                && !insert.contains(['\r', '\n', '`', '~']);
            if fenced_code {
                let delta = insert.len() as isize - range.len() as isize;
                let edit_range = block.edit_range.as_ref().unwrap();
                let updated_range = edit_range.start..edit_range.end.saturating_add_signed(delta);
                let mut contents = String::new();
                for event in Parser::new_ext(&source.text()[updated_range], markdown_options()) {
                    if let Event::Text(text) = event {
                        contents.push_str(&text);
                    }
                }
                let payload: Arc<str> = contents.into();
                let line = source.line_at_byte(range.start).unwrap();
                let mut replacement = plain_line(&source, line);
                replacement.kind = block.kind.clone();
                replacement.edit_range = block.edit_range.clone();
                let mut blocks = self.blocks().to_vec();
                blocks[index] = replacement;
                for (position, block) in blocks.iter_mut().enumerate() {
                    if block.edit_range.as_ref() == Some(edit_range) {
                        block.payload = Some(payload.clone());
                    }
                    if position > index {
                        block.source.start = block.source.start.saturating_add_signed(delta);
                        block.source.end = block.source.end.saturating_add_signed(delta);
                    }
                    if let Some(group) = &mut block.edit_range {
                        if group.end > range.start {
                            if group.start >= range.end {
                                group.start = group.start.saturating_add_signed(delta);
                            }
                            group.end = group.end.saturating_add_signed(delta);
                        }
                    }
                }
                return Ok(Self(Arc::new(DocumentSnapshot {
                    source,
                    format: self.format(),
                    blocks,
                    references: self.0.references,
                    raw_html: self.0.raw_html,
                })));
            }
            let local_edit = block.independent
                && !self.0.references
                && range.end <= block.source.end
                && !old.contains(['\r', '\n'])
                && !insert.contains(['\r', '\n', '[', ']'])
                && matches!(
                    block.kind,
                    DocumentBlockKind::Paragraph
                        | DocumentBlockKind::Heading(_)
                        | DocumentBlockKind::Quote { .. }
                        | DocumentBlockKind::ListItem { .. }
                );
            if local_edit {
                let delta = insert.len() as isize - range.len() as isize;
                let end = block.source.end.saturating_add_signed(delta);
                let fragment = &source.text()[block.source.start..end];
                let parsed = Self::new_with_raw_html(
                    TextDocument::new(fragment),
                    self.format(),
                    self.0.raw_html,
                );
                let candidates: Vec<_> = parsed
                    .blocks()
                    .iter()
                    .filter(|block| !matches!(block.kind, DocumentBlockKind::Blank))
                    .collect();
                if candidates.len() == 1 && candidates[0].kind == block.kind {
                    let mut blocks = self.blocks().to_vec();
                    let mut replacement = candidates[0].clone();
                    replacement.source = block.source.start..end;
                    blocks[index] = replacement;
                    for after in &mut blocks[index + 1..] {
                        after.source.start = after.source.start.saturating_add_signed(delta);
                        after.source.end = after.source.end.saturating_add_signed(delta);
                        if let Some(range) = &mut after.edit_range {
                            range.start = range.start.saturating_add_signed(delta);
                            range.end = range.end.saturating_add_signed(delta);
                        }
                    }
                    return Ok(Self(Arc::new(DocumentSnapshot {
                        source,
                        format: self.format(),
                        blocks,
                        references: false,
                        raw_html: self.0.raw_html,
                    })));
                }
            }
        }
        Ok(Self::new_with_raw_html(
            source,
            self.format(),
            self.0.raw_html,
        ))
    }

    /// Append a reply using the same source-preserving edit path as the editor.
    /// Reference definitions require a whole parse because they affect earlier text.
    pub fn append(&self, text: &str) -> Result<Self, DocumentEditError> {
        let end = self.source().len();
        self.replace(end..end, text)
    }

    fn reparse_markdown_tail(&self, source: &TextDocument) -> Option<Self> {
        // Start at a previously parsed top-level, single-line paragraph after a
        // blank line, not at an arbitrary blank inside a fence/list/HTML block.
        // Reparse that paragraph too: appended setext/table syntax may change it.
        let index = self.blocks().iter().rposition(|block| {
            block.independent
                && block.kind == DocumentBlockKind::Paragraph
                && block.edit_range.is_none()
                && self
                    .source()
                    .line_at_byte(block.source.start)
                    .is_some_and(|line| {
                        line > 0
                            && self
                                .source()
                                .line(line - 1)
                                .is_some_and(|text| text.trim().is_empty())
                    })
        })?;
        let start = self.blocks()[index].source.start;
        if start == 0
            || self.blocks()[..index].iter().any(|block| {
                block.source.end > start
                    || block
                        .edit_range
                        .as_ref()
                        .is_some_and(|range| range.end > start)
            })
        {
            return None;
        }
        let mut blocks = self.blocks()[..index].to_vec();
        for mut block in parse_blocks(&TextDocument::new(&source.text()[start..]), self.0.raw_html)
        {
            block.source.start += start;
            block.source.end += start;
            if let Some(range) = &mut block.edit_range {
                range.start += start;
                range.end += start;
            }
            blocks.push(block);
        }
        Some(Self(Arc::new(DocumentSnapshot {
            source: source.clone(),
            format: self.format(),
            blocks,
            references: false,
            raw_html: self.0.raw_html,
        })))
    }
}

pub(super) fn block(kind: DocumentBlockKind, source: Range<usize>) -> DocumentBlock {
    DocumentBlock {
        id: NEXT_BLOCK.fetch_add(1, Ordering::Relaxed),
        source,
        edit_range: None,
        kind,
        inlines: Arc::from([]),
        cells: Arc::from([]),
        alignments: Arc::from([]),
        payload: None,
        independent: false,
        source_positions: Arc::from([]),
    }
}

fn plain_line(source: &TextDocument, line: usize) -> DocumentBlock {
    let range = source.line_full_range(line).unwrap();
    let text = source.line(line).unwrap();
    let mut result = block(
        if text.is_empty() {
            DocumentBlockKind::Blank
        } else {
            DocumentBlockKind::Paragraph
        },
        range,
    );
    result.inlines = Arc::from([DocumentInline {
        text: text.into(),
        source: 0..text.len(),
        kind: DocumentInlineKind::Text,
        style: DocumentInlineStyle::default(),
    }]);
    result
}

#[derive(Debug)]
struct Parsed<'a> {
    tag: Option<Tag<'a>>,
    event: Option<Event<'a>>,
    source: Range<usize>,
    children: Vec<Parsed<'a>>,
}

impl Drop for Parsed<'_> {
    fn drop(&mut self) {
        // Parser events can nest far beyond the native thread stack. Empty each
        // child's descendants before Rust drops it, including on early returns.
        let mut pending = std::mem::take(&mut self.children);
        while let Some(mut node) = pending.pop() {
            pending.append(&mut node.children);
        }
    }
}

fn markdown_options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_MATH
}

#[cfg(test)]
thread_local! {
    static PARSED_MARKDOWN_BYTES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn parse_blocks(source: &TextDocument, raw_html: bool) -> Vec<DocumentBlock> {
    #[cfg(test)]
    PARSED_MARKDOWN_BYTES.with(|bytes| bytes.set(bytes.get() + source.len()));
    let options = markdown_options();
    let mut stack = vec![Parsed {
        tag: None,
        event: None,
        source: 0..source.len(),
        children: Vec::new(),
    }];
    for (event, range) in Parser::new_ext(source.text(), options).into_offset_iter() {
        match event {
            Event::Start(tag) => stack.push(Parsed {
                tag: Some(tag),
                event: None,
                source: range,
                children: Vec::new(),
            }),
            Event::End(_) => {
                if stack.len() > 1 {
                    let node = stack.pop().unwrap();
                    stack.last_mut().unwrap().children.push(node);
                }
            }
            event => stack.last_mut().unwrap().children.push(Parsed {
                tag: None,
                event: Some(event),
                source: range,
                children: Vec::new(),
            }),
        }
    }
    let mut blocks = Vec::new();
    lower(
        &stack[0].children,
        source,
        &Context {
            text_only_html: !raw_html,
            ..Context::default()
        },
        &mut blocks,
    );
    blocks.sort_by_key(|block| block.source.start);
    let mut complete = Vec::with_capacity(blocks.len());
    let mut line = 0;
    for block in blocks {
        let first = source.line_at_byte(block.source.start).unwrap_or(line);
        while line < first {
            let mut gap = gap_line(source, line);
            gap.edit_range = block
                .edit_range
                .clone()
                .filter(|range| range.contains(&gap.source.start));
            complete.push(gap);
            line += 1;
        }
        let last = source
            .line_at_byte(block.source.end.saturating_sub(1).max(block.source.start))
            .unwrap_or(first);
        line = line.max(last + 1);
        complete.push(block);
    }
    while line < source.line_count() {
        complete.push(gap_line(source, line));
        line += 1;
    }
    complete
}

fn gap_line(source: &TextDocument, line: usize) -> DocumentBlock {
    let mut result = plain_line(source, line);
    if source
        .line(line)
        .is_some_and(|text| !text.trim().is_empty())
    {
        result.kind = DocumentBlockKind::Syntax;
    }
    result
}

#[derive(Clone, Default)]
struct Context {
    text_only_html: bool,
    quote: usize,
    list_depth: usize,
    marker: Option<String>,
    checked: Option<bool>,
    alignments: Arc<[DocumentAlignment]>,
    edit_range: Option<Range<usize>>,
}

fn ordinary_kind(context: &Context) -> DocumentBlockKind {
    if let Some(marker) = &context.marker {
        DocumentBlockKind::ListItem {
            marker: marker.clone(),
            depth: context.list_depth,
            checked: context.checked,
        }
    } else if context.quote > 0 {
        DocumentBlockKind::Quote {
            depth: context.quote,
        }
    } else {
        DocumentBlockKind::Paragraph
    }
}

fn contains_inline_html(node: &Parsed<'_>) -> bool {
    let mut pending = vec![node];
    while let Some(node) = pending.pop() {
        if matches!(node.event, Some(Event::InlineHtml(_))) {
            return true;
        }
        pending.extend(node.children.iter().rev());
    }
    false
}

fn render_html_math(event: Event<'_>) -> Event<'_> {
    #[cfg(feature = "document-math")]
    if let Event::InlineMath(source) | Event::DisplayMath(source) = &event {
        let display = matches!(event, Event::DisplayMath(_));
        if let Ok(rendered) = super::math::render_svg(source, display, 18.0, [34, 34, 34, 255]) {
            let svg = rendered.svg.replacen(
                "<svg",
                &format!(
                    "<svg style=\"width:{}px;height:{}px;vertical-align:middle\"",
                    rendered.width, rendered.height
                ),
                1,
            );
            return Event::Html(svg.into());
        }
    }
    event
}

fn lower(
    nodes: &[Parsed<'_>],
    source: &TextDocument,
    context: &Context,
    out: &mut Vec<DocumentBlock>,
) {
    let mut pending: Vec<_> = nodes
        .iter()
        .rev()
        .map(|node| (node, context.clone(), None::<usize>))
        .collect();
    while let Some((node, mut context, item_start)) = pending.pop() {
        // Decide list continuation after the previous child has been lowered.
        if item_start.is_some_and(|start| out.last().is_some_and(|b| b.source.start >= start)) {
            context.marker = context.marker.as_ref().map(|_| String::new());
            context.checked = None;
        }
        let context = &context;
        if !context.text_only_html
            && matches!(
                node.tag,
                Some(Tag::Paragraph | Tag::Heading { .. } | Tag::List(_) | Tag::Table(_))
            )
            && contains_inline_html(node)
        {
            let mut html = String::new();
            pulldown_cmark::html::push_html(
                &mut html,
                Parser::new_ext(&source.text()[node.source.clone()], markdown_options())
                    .map(render_html_math),
            );
            let kind = if let Some(Tag::Heading { level, .. }) = node.tag {
                DocumentBlockKind::Heading(level as u8)
            } else {
                DocumentBlockKind::Html
            };
            let mut result = block(kind, node.source.clone());
            let mut inlines = Vec::new();
            let mut source_positions = Vec::new();
            collect_inline(
                node,
                &DocumentInlineStyle::default(),
                source,
                &mut inlines,
                &mut source_positions,
            );
            let mut retained_inlines = Vec::new();
            let mut retained_positions = Vec::new();
            for (mut inline, mut positions) in inlines.into_iter().zip(source_positions) {
                if inline.text.starts_with('<') {
                    continue;
                }
                inline.source = inline.source.start.saturating_sub(result.source.start)
                    ..inline.source.end.saturating_sub(result.source.start);
                Arc::make_mut(&mut positions)
                    .iter_mut()
                    .for_each(|position| {
                        position.1 = position.1.saturating_sub(result.source.start);
                    });
                retained_inlines.push(inline);
                retained_positions.push(positions);
            }
            normalize_inline_boundaries(&mut retained_inlines, &mut retained_positions);
            result.inlines = retained_inlines.into();
            result.source_positions = retained_positions.into();
            result.edit_range = Some(node.source.clone());
            result.payload = Some(html.into());
            out.push(result);
            continue;
        }
        match node.tag.as_ref() {
            Some(Tag::Paragraph) => {
                let math = node
                    .children
                    .iter()
                    .find_map(|child| match child.event.as_ref() {
                        Some(Event::DisplayMath(math)) => Some(math),
                        _ => None,
                    });
                if let Some(math) = math.filter(|_| {
                    node.children.iter().all(|child| {
                        matches!(
                            child.event,
                            Some(Event::DisplayMath(_) | Event::SoftBreak | Event::HardBreak)
                        )
                    })
                }) {
                    let mut result = block(DocumentBlockKind::Math, node.source.clone());
                    result.payload = Some(math.as_ref().into());
                    out.push(result);
                } else {
                    push_lines(node, source, ordinary_kind(context), out);
                }
            }
            Some(Tag::Heading { level, .. }) => {
                push_lines(node, source, DocumentBlockKind::Heading(*level as u8), out)
            }
            Some(Tag::BlockQuote(_)) => {
                let mut context = context.clone();
                context.quote += 1;
                pending.extend(
                    node.children
                        .iter()
                        .rev()
                        .map(|child| (child, context.clone(), None)),
                );
            }
            Some(Tag::List(start)) => {
                for (item_index, item) in node.children.iter().enumerate().rev() {
                    let mut context = context.clone();
                    context.list_depth += 1;
                    context.marker = Some(
                        start
                            .map(|start| format!("{}.", start + item_index as u64))
                            .unwrap_or_else(|| "\u{2022}".into()),
                    );
                    context.checked = task_marker(item);
                    pending.push((item, context, None));
                }
            }
            Some(Tag::Item) => {
                let inline: Vec<_> = node
                    .children
                    .iter()
                    .filter(|child| !is_block(child))
                    .collect();
                if let (Some(first), Some(last)) = (inline.first(), inline.last()) {
                    push_inline_lines(
                        &inline,
                        first.source.start..last.source.end,
                        source,
                        ordinary_kind(context),
                        out,
                    );
                }
                pending.extend(
                    node.children
                        .iter()
                        .rev()
                        .filter(|child| is_block(child))
                        .map(|child| (child, context.clone(), Some(node.source.start))),
                );
            }
            Some(Tag::CodeBlock(code)) => {
                let language = match code {
                    CodeBlockKind::Fenced(language) => {
                        language.split_whitespace().next().unwrap_or("")
                    }
                    _ => "",
                };
                let special = match language {
                    "math" | "latex" | "tex" => Some(DocumentBlockKind::Math),
                    "chem" | "chemistry" => Some(DocumentBlockKind::Chemistry),
                    "pdb" | "mol" | "molecule" | "xyz" => Some(DocumentBlockKind::Molecule {
                        format: language.into(),
                    }),
                    _ => None,
                };
                if let Some(kind) = special {
                    let mut result = block(kind, node.source.clone());
                    let mut payload = String::new();
                    collect_text(node, &mut payload);
                    result.payload = Some(payload.into());
                    out.push(result);
                } else {
                    let first = source.line_at_byte(node.source.start).unwrap_or(0);
                    let last = source
                        .line_at_byte(node.source.end.saturating_sub(1))
                        .unwrap_or(first);
                    let mut contents = String::new();
                    collect_text(node, &mut contents);
                    let content_lines: Vec<_> = contents.lines().collect();
                    let payload: Arc<str> = contents.clone().into();
                    for line in first..=last {
                        let mut result = plain_line(source, line);
                        if matches!(code, CodeBlockKind::Indented) {
                            let text = content_lines.get(line - first).copied().unwrap_or("");
                            let raw = source.line(line).unwrap_or("");
                            let offset = raw.find(text).unwrap_or(0);
                            result.inlines = Arc::from([DocumentInline {
                                text: text.into(),
                                source: offset..offset + text.len(),
                                kind: DocumentInlineKind::Text,
                                style: DocumentInlineStyle::default(),
                            }]);
                        }
                        result.edit_range = Some(node.source.clone());
                        result.payload = Some(payload.clone());
                        let text = source.line(line).unwrap_or("").trim_start();
                        let opening = source.line(first).unwrap_or("").trim_start();
                        let marker = opening.chars().next().unwrap_or('`');
                        let fence_length = opening
                            .chars()
                            .take_while(|character| *character == marker)
                            .count();
                        let closing = line == last
                            && line != first
                            && text
                                .chars()
                                .take_while(|character| *character == marker)
                                .count()
                                >= fence_length
                            && text.trim_matches(marker).trim().is_empty();
                        result.kind = if matches!(code, CodeBlockKind::Fenced(_))
                            && (line == first || closing)
                        {
                            DocumentBlockKind::Syntax
                        } else {
                            DocumentBlockKind::Code {
                                language: language.into(),
                            }
                        };
                        out.push(result);
                    }
                }
            }
            Some(Tag::Table(alignments)) => {
                let mut context = context.clone();
                context.alignments = alignments.clone().into();
                context.edit_range = Some(node.source.clone());
                pending.extend(
                    node.children
                        .iter()
                        .rev()
                        .map(|child| (child, context.clone(), None)),
                );
            }
            Some(Tag::TableHead | Tag::TableRow) => {
                let mut result = block(
                    DocumentBlockKind::TableRow {
                        header: matches!(node.tag, Some(Tag::TableHead)),
                    },
                    node.source.clone(),
                );
                result.cells = node
                    .children
                    .iter()
                    .map(|cell| {
                        let mut inlines = Vec::new();
                        let mut source_positions = Vec::new();
                        collect_inline(
                            cell,
                            &DocumentInlineStyle::default(),
                            source,
                            &mut inlines,
                            &mut source_positions,
                        );
                        for inline in &mut inlines {
                            inline.source = inline.source.start.saturating_sub(result.source.start)
                                ..inline.source.end.saturating_sub(result.source.start);
                        }
                        Arc::from(inlines)
                    })
                    .collect::<Vec<_>>()
                    .into();
                result.alignments = context.alignments.clone();
                result.edit_range = context.edit_range.clone();
                out.push(result);
            }
            Some(Tag::HtmlBlock) => {
                if context.text_only_html {
                    let first = source.line_at_byte(node.source.start).unwrap_or(0);
                    let last = source
                        .line_at_byte(node.source.end.saturating_sub(1).max(node.source.start))
                        .unwrap_or(first);
                    for line in first..=last {
                        let mut result = plain_line(source, line);
                        result.kind = ordinary_kind(context);
                        // HTML block boundaries can change when any contained line changes.
                        result.edit_range = Some(node.source.clone());
                        out.push(result);
                    }
                    continue;
                }
                let mut result = block(DocumentBlockKind::Html, node.source.clone());
                result.payload = Some(source.text()[node.source.clone()].into());
                result.edit_range = Some(node.source.clone());
                out.push(result);
            }
            _ => match node.event.as_ref() {
                Some(Event::DisplayMath(math)) => {
                    let mut result = block(DocumentBlockKind::Math, node.source.clone());
                    result.payload = Some(math.as_ref().into());
                    out.push(result);
                }
                Some(Event::Rule) => out.push(block(DocumentBlockKind::Rule, node.source.clone())),
                _ if !node.children.is_empty() => {
                    pending.extend(
                        node.children
                            .iter()
                            .rev()
                            .map(|child| (child, context.clone(), None)),
                    );
                }
                _ => {}
            },
        }
    }
}

fn is_block(node: &Parsed<'_>) -> bool {
    matches!(
        node.tag,
        Some(
            Tag::Paragraph
                | Tag::Heading { .. }
                | Tag::List(_)
                | Tag::BlockQuote(_)
                | Tag::CodeBlock(_)
                | Tag::Table(_)
                | Tag::HtmlBlock
        )
    )
}

fn task_marker(node: &Parsed<'_>) -> Option<bool> {
    let mut pending = vec![node];
    while let Some(node) = pending.pop() {
        if let Some(Event::TaskListMarker(checked)) = node.event {
            return Some(checked);
        }
        pending.extend(node.children.iter().rev());
    }
    None
}

fn collect_text(node: &Parsed<'_>, text: &mut String) {
    let mut pending = vec![node];
    while let Some(node) = pending.pop() {
        match node.event.as_ref() {
            Some(
                Event::Text(value)
                | Event::Code(value)
                | Event::Html(value)
                | Event::InlineHtml(value),
            ) => text.push_str(value),
            Some(Event::SoftBreak | Event::HardBreak) => text.push('\n'),
            _ => {}
        }
        pending.extend(node.children.iter().rev());
    }
}

fn collect_inline(
    node: &Parsed<'_>,
    inherited: &DocumentInlineStyle,
    source: &TextDocument,
    out: &mut Vec<DocumentInline>,
    source_positions: &mut Vec<Arc<[(usize, usize)]>>,
) {
    let mut pending = vec![(node, inherited.clone())];
    while let Some((node, mut style)) = pending.pop() {
        match node.tag.as_ref() {
            Some(Tag::Strong) => style.bold = true,
            Some(Tag::Emphasis) => style.italic = true,
            Some(Tag::Strikethrough) => style.strike = true,
            Some(Tag::Superscript) => style.superscript = true,
            Some(Tag::Subscript) => style.subscript = true,
            Some(Tag::Link { dest_url, .. }) => style.link = Some(dest_url.to_string()),
            Some(Tag::Image {
                dest_url, title, ..
            }) => {
                let mut text = String::new();
                collect_text(node, &mut text);
                out.push(DocumentInline {
                    text,
                    source: node.source.clone(),
                    style,
                    kind: DocumentInlineKind::Image {
                        destination: dest_url.to_string(),
                        title: title.to_string(),
                    },
                });
                source_positions.push(Arc::from([]));
                continue;
            }
            _ => {}
        }
        let value = match node.event.as_ref() {
            Some(Event::Text(value)) => Some((value.to_string(), DocumentInlineKind::Text, true)),
            Some(
                Event::Html(value) | Event::InlineHtml(value) | Event::FootnoteReference(value),
            ) => Some((value.to_string(), DocumentInlineKind::Text, false)),
            Some(Event::Code(value)) => {
                style.code = true;
                Some((value.to_string(), DocumentInlineKind::Text, false))
            }
            Some(Event::SoftBreak | Event::HardBreak) => {
                Some(("\n".into(), DocumentInlineKind::Text, false))
            }
            Some(Event::InlineMath(value) | Event::DisplayMath(value)) => {
                Some((value.to_string(), DocumentInlineKind::Math, false))
            }
            _ => None,
        };
        if let Some((text, kind, fold_whitespace)) = value {
            let raw = &source.text()[node.source.clone()];
            let matched_start = raw.find(&text);
            let range = matched_start
                .map(|start| node.source.start + start..node.source.start + start + text.len())
                .unwrap_or_else(|| node.source.clone());
            let (text, positions) = if fold_whitespace {
                fold_markdown_whitespace(&text, range.start, matched_start.is_some())
            } else {
                (text, Vec::new())
            };
            out.push(DocumentInline {
                text,
                source: if matches!(node.event, Some(Event::SoftBreak | Event::HardBreak)) {
                    node.source.clone()
                } else {
                    range
                },
                kind,
                style,
            });
            if matches!(node.event, Some(Event::SoftBreak | Event::HardBreak)) {
                source_positions.push(Arc::from([(1, node.source.end)]));
            } else {
                source_positions.push(positions.into());
            }
            continue;
        }
        pending.extend(
            node.children
                .iter()
                .rev()
                .map(|child| (child, style.clone())),
        );
    }
}

fn fold_markdown_whitespace(
    text: &str,
    source_start: usize,
    source_is_exact: bool,
) -> (String, Vec<(usize, usize)>) {
    let mut folded = String::with_capacity(text.len());
    let mut positions = Vec::new();
    let mut whitespace_end = None;
    for (offset, ch) in text.char_indices() {
        let end = offset + ch.len_utf8();
        if matches!(ch, ' ' | '\t' | '\r' | '\n') {
            if whitespace_end.is_none() {
                folded.push(' ');
            } else if source_is_exact {
                let source_end = source_start + end;
                if positions
                    .last()
                    .is_some_and(|(display_offset, _)| *display_offset == folded.len())
                {
                    positions.last_mut().unwrap().1 = source_end;
                } else {
                    positions.push((folded.len(), source_end));
                }
            }
            whitespace_end = Some(end);
            continue;
        }
        whitespace_end = None;
        folded.push(ch);
    }
    (folded, positions)
}

fn normalize_inline_boundaries(
    inlines: &mut [DocumentInline],
    source_positions: &mut [Arc<[(usize, usize)]>],
) {
    let mut at_line_start = true;
    let mut previous_whitespace = false;
    for (inline, positions) in inlines.iter_mut().zip(source_positions) {
        if matches!(inline.kind, DocumentInlineKind::Image { .. }) {
            at_line_start = false;
            previous_whitespace = false;
            continue;
        }
        if inline.style.code {
            at_line_start = false;
            previous_whitespace = inline.text.ends_with([' ', '\t']);
            continue;
        }
        if inline.text == "\n" && !inline.style.code {
            at_line_start = true;
            previous_whitespace = false;
            continue;
        }
        let leading = inline.text.len() - inline.text.trim_start_matches([' ', '\t']).len();
        if leading > 0 && (at_line_start || previous_whitespace) {
            let source_start = if positions.is_empty() {
                if inline.text.len() == inline.source.len() {
                    inline.source.start + leading
                } else if leading == inline.text.len() {
                    inline.source.end
                } else {
                    inline.source.start
                }
            } else {
                let source_offset = positions
                    .iter()
                    .rev()
                    .find(|(offset, _)| *offset <= leading)
                    .map(|(offset, source_offset)| *source_offset + leading - offset)
                    .unwrap_or(inline.source.start + leading);
                source_offset
            };
            inline.text.drain(..leading);
            let shifted: Vec<_> = positions
                .iter()
                .filter(|(offset, _)| *offset > leading)
                .map(|(offset, source_offset)| (offset - leading, *source_offset))
                .collect();
            *positions = shifted.into();
            inline.source.start = source_start;
        }
        if inline.text.is_empty() {
            continue;
        }
        at_line_start = false;
        previous_whitespace = inline.text.ends_with([' ', '\t']);
    }
}

fn push_lines(
    node: &Parsed<'_>,
    source: &TextDocument,
    kind: DocumentBlockKind,
    out: &mut Vec<DocumentBlock>,
) {
    push_inline_lines(&[node], node.source.clone(), source, kind, out);
}

fn push_inline_lines(
    nodes: &[&Parsed<'_>],
    range: Range<usize>,
    source: &TextDocument,
    kind: DocumentBlockKind,
    out: &mut Vec<DocumentBlock>,
) {
    let mut inlines = Vec::new();
    let mut source_positions = Vec::new();
    for node in nodes {
        collect_inline(
            node,
            &DocumentInlineStyle::default(),
            source,
            &mut inlines,
            &mut source_positions,
        );
    }
    normalize_inline_boundaries(&mut inlines, &mut source_positions);
    let first = source.line_at_byte(range.start).unwrap_or(0);
    let last = source
        .line_at_byte(range.end.saturating_sub(1).max(range.start))
        .unwrap_or(first);
    let mut by_line: BTreeMap<usize, Vec<DocumentInline>> = BTreeMap::new();
    let mut positions_by_line: BTreeMap<usize, Vec<Arc<[(usize, usize)]>>> = BTreeMap::new();
    for (inline, mut positions) in inlines.into_iter().zip(source_positions) {
        let line = source.line_at_byte(inline.source.start).unwrap_or(first);
        by_line.entry(line).or_default().push(inline);
        let line_start = source.line_full_range(line).unwrap().start;
        Arc::make_mut(&mut positions)
            .iter_mut()
            .for_each(|position| {
                position.1 = position.1.saturating_sub(line_start);
            });
        positions_by_line.entry(line).or_default().push(positions);
    }
    for line in first..=last {
        let mut result = block(kind.clone(), source.line_full_range(line).unwrap());
        if let DocumentBlockKind::ListItem { depth, .. } = &kind {
            if line != first {
                result.kind = DocumentBlockKind::ListItem {
                    marker: String::new(),
                    depth: *depth,
                    checked: None,
                };
            }
        }
        let mut inlines = by_line.remove(&line).unwrap_or_default();
        for inline in &mut inlines {
            inline.source = inline.source.start.saturating_sub(result.source.start)
                ..inline.source.end.saturating_sub(result.source.start);
        }
        result.inlines = inlines.into();
        result.source_positions = positions_by_line.remove(&line).unwrap_or_default().into();
        result.independent = first == last;
        out.push(result);
    }
}

#[cfg(test)]
#[path = "tests/semantic_document.rs"]
mod tests;
