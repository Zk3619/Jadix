//! Source-preserving CommonMark documents. UI nodes are produced only by a viewport.

use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use pulldown_cmark::{Alignment, CodeBlockKind, Event, Options, Parser, Tag};
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

#[derive(Clone, Debug)]
pub struct DocumentBlock {
    pub id: u64,
    pub source: Range<usize>,
    pub edit_range: Option<Range<usize>>,
    pub kind: DocumentBlockKind,
    pub inlines: Arc<[DocumentInline]>,
    pub cells: Arc<[Arc<[DocumentInline]>]>,
    pub alignments: Arc<[Alignment]>,
    pub payload: Option<Arc<str>>,
    pub(super) independent: bool,
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
        let inline = self.inlines.get(inline)?;
        let local = inline
            .text
            .floor_char_boundary(display_byte.min(inline.text.len()));
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
    pub fn plain_text(source: impl Into<TextDocument>) -> Self {
        Self::new(source.into(), DocumentFormat::PlainText)
    }

    pub fn new(source: TextDocument, format: DocumentFormat) -> Self {
        let blocks = match format {
            DocumentFormat::Markdown => parse_blocks(&source),
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
        }))
    }

    pub fn source(&self) -> &TextDocument {
        &self.0.source
    }
    pub fn format(&self) -> DocumentFormat {
        self.0.format
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
                let parsed = Self::new(TextDocument::new(fragment), self.format());
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
                    })));
                }
            }
        }
        Ok(Self::new(source, self.format()))
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

fn markdown_options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_MATH
}

fn parse_blocks(source: &TextDocument) -> Vec<DocumentBlock> {
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
        &Context::default(),
        &mut blocks,
        0,
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
    quote: usize,
    list_depth: usize,
    marker: Option<String>,
    checked: Option<bool>,
    alignments: Arc<[Alignment]>,
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
    matches!(node.event, Some(Event::InlineHtml(_)))
        || node.children.iter().any(contains_inline_html)
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
    depth: usize,
) {
    if depth > 128 {
        return;
    }
    for node in nodes {
        if contains_inline_html(node)
            && matches!(
                node.tag,
                Some(Tag::Paragraph | Tag::Heading { .. } | Tag::List(_) | Tag::Table(_))
            )
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
            collect_inline(
                node,
                &DocumentInlineStyle::default(),
                source,
                &mut inlines,
                0,
            );
            inlines.retain(|inline| !inline.text.starts_with('<'));
            for inline in &mut inlines {
                inline.source = inline.source.start.saturating_sub(result.source.start)
                    ..inline.source.end.saturating_sub(result.source.start);
            }
            result.inlines = inlines.into();
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
                lower(&node.children, source, &context, out, depth + 1);
            }
            Some(Tag::List(start)) => {
                for (item_index, item) in node.children.iter().enumerate() {
                    let mut context = context.clone();
                    context.list_depth += 1;
                    context.marker = Some(
                        start
                            .map(|start| format!("{}.", start + item_index as u64))
                            .unwrap_or_else(|| "\u{2022}".into()),
                    );
                    context.checked = task_marker(item);
                    lower(std::slice::from_ref(item), source, &context, out, depth + 1);
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
                let nested: Vec<_> = node
                    .children
                    .iter()
                    .filter(|child| is_block(child))
                    .collect();
                for child in nested {
                    let mut continuation = context.clone();
                    if out
                        .last()
                        .is_some_and(|block| block.source.start >= node.source.start)
                    {
                        continuation.marker = context.marker.as_ref().map(|_| String::new());
                        continuation.checked = None;
                    }
                    lower(
                        std::slice::from_ref(child),
                        source,
                        &continuation,
                        out,
                        depth + 1,
                    );
                }
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
                                source: result.source.start + offset
                                    ..result.source.start + offset + text.len(),
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
                lower(&node.children, source, &context, out, depth + 1);
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
                        collect_inline(
                            cell,
                            &DocumentInlineStyle::default(),
                            source,
                            &mut inlines,
                            0,
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
                    lower(&node.children, source, context, out, depth + 1)
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
    if let Some(Event::TaskListMarker(checked)) = node.event {
        return Some(checked);
    }
    node.children.iter().find_map(task_marker)
}

fn collect_text(node: &Parsed<'_>, text: &mut String) {
    match node.event.as_ref() {
        Some(
            Event::Text(value) | Event::Code(value) | Event::Html(value) | Event::InlineHtml(value),
        ) => text.push_str(value),
        Some(Event::SoftBreak | Event::HardBreak) => text.push('\n'),
        _ => {}
    }
    for child in &node.children {
        collect_text(child, text);
    }
}

fn collect_inline(
    node: &Parsed<'_>,
    inherited: &DocumentInlineStyle,
    source: &TextDocument,
    out: &mut Vec<DocumentInline>,
    depth: usize,
) {
    if depth > 128 {
        return;
    }
    let mut style = inherited.clone();
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
            return;
        }
        _ => {}
    }
    let value = match node.event.as_ref() {
        Some(
            Event::Text(value)
            | Event::Html(value)
            | Event::InlineHtml(value)
            | Event::FootnoteReference(value),
        ) => Some((value.to_string(), DocumentInlineKind::Text)),
        Some(Event::Code(value)) => {
            style.code = true;
            Some((value.to_string(), DocumentInlineKind::Text))
        }
        Some(Event::InlineMath(value) | Event::DisplayMath(value)) => {
            Some((value.to_string(), DocumentInlineKind::Math))
        }
        _ => None,
    };
    if let Some((text, kind)) = value {
        let raw = &source.text()[node.source.clone()];
        let range = raw
            .find(&text)
            .map(|start| node.source.start + start..node.source.start + start + text.len())
            .unwrap_or_else(|| node.source.clone());
        out.push(DocumentInline {
            text,
            source: range,
            kind,
            style,
        });
        return;
    }
    for child in &node.children {
        collect_inline(child, &style, source, out, depth + 1);
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
    for node in nodes {
        collect_inline(
            node,
            &DocumentInlineStyle::default(),
            source,
            &mut inlines,
            0,
        );
    }
    let first = source.line_at_byte(range.start).unwrap_or(0);
    let last = source
        .line_at_byte(range.end.saturating_sub(1).max(range.start))
        .unwrap_or(first);
    let mut by_line: BTreeMap<usize, Vec<DocumentInline>> = BTreeMap::new();
    for inline in inlines {
        let line = source.line_at_byte(inline.source.start).unwrap_or(first);
        by_line.entry(line).or_default().push(inline);
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
        result.independent = first == last;
        out.push(result);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_ranges_cover_unicode_crlf_and_empty_lines() {
        let document = Document::markdown("# \u{6807}\u{9898}\r\n\r\n**bold** and $x^2$\r\n");
        assert_eq!(document.blocks().len(), 4);
        assert_eq!(document.blocks()[0].kind, DocumentBlockKind::Heading(1));
        assert!(document.blocks()[2].inlines[0].style.bold);
        assert!(document.blocks()[2]
            .inlines
            .iter()
            .any(|inline| inline.kind == DocumentInlineKind::Math));
        for block in document.blocks() {
            assert!(document
                .source()
                .text()
                .is_char_boundary(block.source.start));
            assert!(document.source().text().is_char_boundary(block.source.end));
        }
        assert_eq!(document.block_at(document.source().len()), Some(3));
    }

    #[test]
    fn tables_tasks_code_and_math_are_semantic_blocks() {
        let document = Document::markdown("- [x] done\n- pending\n\n| a | b |\n| :- | -: |\n| 1 | 2 |\n\n```rust\nlet a = 1;\n```\n\n$$\n\\frac{1}{2}\n$$\n");
        assert!(document.blocks().iter().any(|block| matches!(
            block.kind,
            DocumentBlockKind::ListItem {
                checked: Some(true),
                ..
            }
        )));
        assert_eq!(
            document
                .blocks()
                .iter()
                .filter(|block| matches!(block.kind, DocumentBlockKind::TableRow { .. }))
                .count(),
            2
        );
        assert!(document
            .blocks()
            .iter()
            .any(|block| matches!(block.kind, DocumentBlockKind::Code { .. })));
        assert!(document
            .blocks()
            .iter()
            .any(|block| block.kind == DocumentBlockKind::Math));
    }

    #[test]
    fn local_edit_reuses_unaffected_blocks_and_structural_edit_reparses() {
        let document = Document::markdown("alpha\n\nbeta\n");
        let old_id = document.blocks()[2].id;
        let changed = document.replace(1..2, "XX").unwrap();
        assert_eq!(changed.blocks()[2].id, old_id);
        assert_eq!(
            changed.blocks()[2].source.start,
            document.blocks()[2].source.start + 1
        );
        let changed = changed.replace(0..0, "# ").unwrap();
        assert_eq!(changed.blocks()[0].kind, DocumentBlockKind::Heading(1));
    }

    #[test]
    fn large_code_fence_is_split_before_ui_materialization() {
        let document =
            Document::markdown(format!("```rust\n{}```\n", "let x = 1;\n".repeat(100_000)));
        assert!(document.blocks().len() >= 100_002);
        assert!(document
            .blocks()
            .iter()
            .all(|block| block.source.len() < 40));
    }

    #[test]
    fn code_edit_reuses_rows_and_preserves_whole_fence_range() {
        let original = Document::markdown("```rust\nlet one = 1;\nlet two = 2;\n```\n\nafter");
        let byte = original.source().text().find('1').unwrap();
        let changed = original.replace(byte..byte + 1, "100").unwrap();
        assert_eq!(changed.blocks()[2].id, original.blocks()[2].id);
        assert_eq!(
            changed.blocks()[0].edit_range,
            changed.blocks()[2].edit_range
        );
        assert!(changed.blocks()[1].text().contains("100"));
        assert_eq!(
            changed.blocks()[2].source.start,
            original.blocks()[2].source.start + 2
        );
        assert_eq!(changed.blocks()[2].edit_range.as_ref().unwrap().start, 0);
    }
}
