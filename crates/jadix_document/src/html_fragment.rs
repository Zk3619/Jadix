//! Clipboard HTML exchanged with Jadix's rich-text schema without a browser.
//! Unsupported wrappers preserve text; active content and external assets are omitted.

use crate::{
    Document, DocumentBlockKind, DocumentEditError, DocumentFormat, DocumentInlineStyle,
    RichTextBlock, RichTextSpan, DocumentInlineKind,
};
use html5ever::{
    buffer_queue::BufferQueue,
    tokenizer::{states::RawKind, StartTag, Token, TokenSink, TokenSinkResult, Tokenizer},
};
use std::{cell::RefCell, fmt::Write, ops::Range};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HtmlFragmentError {
    TooLarge,
    TooDeep,
}

impl std::fmt::Display for HtmlFragmentError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::TooLarge => "HTML fragment exceeds 4 MiB",
            Self::TooDeep => "HTML fragment exceeds 256 nested elements",
        })
    }
}
impl std::error::Error for HtmlFragmentError {}

impl Document {
    /// Import semantic tags: paragraphs, headings, lists, quotes, pre/code,
    /// bold, italic, strike, sub/sup and safe links. HTML entities are decoded by
    /// html5ever. Only CSS white-space preservation is recognized; images,
    /// tables and browser DOM repair are not imported.
    /// No scripts, resource fetches or event handlers are executed.
    pub fn from_html_fragment(html: &str) -> Result<Self, HtmlFragmentError> {
        if html.len() > 4 * 1024 * 1024 {
            return Err(HtmlFragmentError::TooLarge);
        }
        let html = html
            .split_once("<!--StartFragment-->")
            .and_then(|(_, body)| body.split_once("<!--EndFragment-->").map(|(body, _)| body))
            .unwrap_or(html);
        let tokenizer = Tokenizer::new(Sink(RefCell::new(Import::default())), Default::default());
        let input = BufferQueue::default();
        input.push_back(html.into());
        let _ = tokenizer.feed(&input);
        tokenizer.end();
        let mut import = tokenizer.sink.0.into_inner();
        if import.too_deep {
            return Err(HtmlFragmentError::TooDeep);
        }
        import.finish_block();
        Ok(Self::rich_text(import.blocks))
    }

    /// Export the supported RichText block and inline schema as inert HTML.
    /// Raw HTML, formula and image payloads become escaped visible text. Other
    /// CSS attributes and custom nodes are not serialized. This is not a
    /// browser DOM roundtrip. Markdown must be converted to RichText first.
    pub fn to_html_fragment(&self) -> Result<String, DocumentEditError> {
        self.html_fragment(0..self.source().len())
    }

    /// Export a UTF-8 source range, retaining the intersecting blocks' kinds
    /// and inline styles. A partially selected block keeps its block kind.
    /// Quotes/lists deeper than 128 levels, or whose wrappers plus inline marks
    /// exceed the importer's 256-element nesting budget, are rejected before allocation.
    pub fn html_fragment(&self, range: Range<usize>) -> Result<String, DocumentEditError> {
        if self.format() != DocumentFormat::RichText
            || range.start > range.end
            || range.end > self.source().len()
        {
            return Err(DocumentEditError::InvalidRange);
        }
        if !self.source().text().is_char_boundary(range.start)
            || !self.source().text().is_char_boundary(range.end)
        {
            return Err(DocumentEditError::NotCharBoundary);
        }
        let blocks = self.blocks();
        let selected = if range.start == 0 && range.end == self.source().len() {
            // Include empty trailing blocks when exporting the whole document.
            blocks
        } else {
            // RichText blocks have ordered, non-overlapping source ranges.
            // A small clipboard selection should not scan the entire document.
            let start = blocks.partition_point(|block| block.source.end <= range.start);
            let end = blocks.partition_point(|block| block.source.start < range.end);
            &blocks[start..end.max(start)]
        };
        for block in selected {
            if matches!(block.kind, DocumentBlockKind::Quote { depth } | DocumentBlockKind::ListItem { depth, .. } if depth > 128)
            {
                return Err(DocumentEditError::InvalidRange);
            }
            let wrappers = match block.kind {
                DocumentBlockKind::Quote { depth } => depth.max(1) + 1,
                DocumentBlockKind::ListItem { depth, .. } => depth.max(1) * 2 + 1,
                _ => 1,
            };
            if block.inlines.iter().any(|inline| {
                let style = &inline.style;
                let marks = [
                    style.bold,
                    style.italic,
                    style.strike,
                    style.code,
                    style.superscript,
                    style.subscript,
                ]
                .into_iter()
                .filter(|mark| *mark)
                .count();
                wrappers
                    + marks
                    + usize::from(style.link.as_deref().is_some_and(|link| {
                        !link.is_empty() && crate::is_safe_document_destination(link)
                    }))
                    > 256
            }) || wrappers > 256
            {
                return Err(DocumentEditError::InvalidRange);
            }
        }
        let mut output = String::new();
        for block in selected {
            let (open, close) = match &block.kind {
                DocumentBlockKind::Heading(level) => {
                    let level = level.clamp(&1, &6);
                    (
                        format!("<h{level} style=\"white-space:pre-wrap\">"),
                        format!("</h{level}>"),
                    )
                }
                DocumentBlockKind::Code { language } => {
                    let mut open = String::from("<pre data-language=\"");
                    escape_html(&mut open, language, false);
                    open.push_str("\">");
                    (open, "</pre>".into())
                }
                DocumentBlockKind::Quote { depth } => (
                    format!(
                        "{}<p style=\"white-space:pre-wrap\">",
                        "<blockquote>".repeat((*depth).max(1))
                    ),
                    format!("</p>{}", "</blockquote>".repeat((*depth).max(1))),
                ),
                DocumentBlockKind::ListItem {
                    marker,
                    depth,
                    checked,
                } => {
                    let depth = (*depth).max(1);
                    let (list, end) = if let Some(number) = marker
                        .strip_suffix(['.', ')'])
                        .and_then(|value| value.parse::<i64>().ok())
                    {
                        (format!("<ol start=\"{number}\">"), "</ol>")
                    } else {
                        ("<ul>".into(), "</ul>")
                    };
                    let mut open = "<ul><li>".repeat(depth - 1);
                    open.push_str(&list);
                    open.push_str("<li style=\"white-space:pre-wrap\"");
                    if let Some(checked) = checked {
                        let _ = write!(open, " data-checked=\"{checked}\"");
                    }
                    open.push_str("><p>");
                    (
                        open,
                        format!("</p></li>{end}{}", "</li></ul>".repeat(depth - 1)),
                    )
                }
                DocumentBlockKind::Rule => ("<hr>".into(), String::new()),
                _ => ("<p style=\"white-space:pre-wrap\">".into(), "</p>".into()),
            };
            output.push_str(&open);
            for inline in block.inlines.iter() {
                let absolute = block.source.start + inline.source.start
                    ..block.source.start + inline.source.end;
                let start = absolute.start.max(range.start);
                let end = absolute.end.min(range.end);
                if start >= end {
                    continue;
                }
                // RichText source and inline text have identical byte coordinates.
                let text = &inline.text[start - absolute.start..end - absolute.start];
                export_inline(
                    &mut output,
                    text,
                    &inline.style,
                    matches!(block.kind, DocumentBlockKind::Code { .. }),
                );
            }
            output.push_str(&close);
        }
        Ok(output)
    }
}

fn escape_html(output: &mut String, text: &str, line_breaks: bool) {
    for character in text.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\r' => output.push_str("&#13;"),
            '\n' if line_breaks => output.push_str("<br>"),
            _ => output.push(character),
        }
    }
}

fn export_inline(output: &mut String, text: &str, style: &DocumentInlineStyle, pre: bool) {
    let link = style
        .link
        .as_deref()
        .filter(|link| !link.is_empty() && crate::is_safe_document_destination(link));
    if let Some(link) = link {
        output.push_str("<a href=\"");
        escape_html(output, link, false);
        output.push_str("\">");
    }
    let marks = [
        (style.bold, "strong"),
        (style.italic, "em"),
        (style.strike, "s"),
        (style.code, "code"),
        (style.superscript, "sup"),
        (style.subscript, "sub"),
    ];
    for (_, tag) in marks.iter().filter(|(enabled, _)| *enabled) {
        let _ = write!(output, "<{tag}>");
    }
    escape_html(output, text, !pre);
    for (_, tag) in marks.iter().rev().filter(|(enabled, _)| *enabled) {
        let _ = write!(output, "</{tag}>");
    }
    if link.is_some() {
        output.push_str("</a>");
    }
}

#[derive(Clone)]
struct Frame {
    tag: String,
    style: DocumentInlineStyle,
    hidden: bool,
    pre: bool,
    block: Option<DocumentBlockKind>,
    next_ordinal: Option<i64>,
}

struct Import {
    stack: Vec<Frame>,
    blocks: Vec<RichTextBlock>,
    spans: Vec<RichTextSpan>,
    kind: DocumentBlockKind,
    pending_space: bool,
    too_deep: bool,
    explicit_empty: bool,
}

impl Default for Import {
    fn default() -> Self {
        Self {
            stack: Vec::new(),
            blocks: Vec::new(),
            spans: Vec::new(),
            kind: DocumentBlockKind::Paragraph,
            pending_space: false,
            too_deep: false,
            explicit_empty: false,
        }
    }
}

impl Import {
    fn finish_block(&mut self) {
        if !self.spans.is_empty() || self.explicit_empty {
            self.blocks.push(RichTextBlock {
                kind: self.kind.clone(),
                spans: std::mem::take(&mut self.spans),
            });
        }
        self.kind = DocumentBlockKind::Paragraph;
        self.pending_space = false;
        self.explicit_empty = false;
    }

    fn text(&mut self, text: &str) {
        if self.too_deep || self.stack.last().is_some_and(|frame| frame.hidden) {
            return;
        }
        let style = self
            .stack
            .last()
            .map(|frame| frame.style.clone())
            .unwrap_or_default();
        let pre = self.stack.last().is_some_and(|frame| frame.pre);
        let mut output = String::new();
        for character in text.chars() {
            if !pre && matches!(character, ' ' | '\t' | '\n' | '\r' | '\u{000c}') {
                self.pending_space = !self.spans.is_empty() || !output.is_empty();
            } else {
                if self.pending_space {
                    output.push(' ');
                    self.pending_space = false;
                }
                output.push(character);
            }
        }
        if output.is_empty() {
            return;
        }
        if let Some(last) = self.spans.last_mut().filter(|span| span.style == style) {
            last.text.push_str(&output);
        } else {
            self.spans.push(RichTextSpan::styled(output, style));
        }
    }
}

struct Sink(RefCell<Import>);

fn block_tag(tag: &str) -> bool {
    matches!(
        tag,
        "p" | "div"
            | "section"
            | "article"
            | "header"
            | "footer"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "li"
            | "blockquote"
            | "pre"
            | "table"
            | "tbody"
            | "thead"
            | "tr"
            | "td"
            | "th"
    )
}

impl TokenSink for Sink {
    type Handle = ();

    fn process_token(&self, token: Token, _: u64) -> TokenSinkResult<()> {
        let mut import = self.0.borrow_mut();
        if import.too_deep {
            return TokenSinkResult::Continue;
        }
        match token {
            Token::CharacterTokens(text) => import.text(&text),
            Token::TagToken(tag) => {
                let name = tag.name.as_ref();
                if tag.kind != StartTag {
                    if let Some(index) = import.stack.iter().rposition(|frame| frame.tag == name) {
                        let visible = !import.stack.last().is_some_and(|frame| frame.hidden);
                        if visible && block_tag(name) {
                            import.finish_block();
                        }
                        import.stack.truncate(index);
                        if visible && block_tag(name) {
                            import.kind = import
                                .stack
                                .last()
                                .and_then(|frame| frame.block.clone())
                                .unwrap_or(DocumentBlockKind::Paragraph);
                        }
                    }
                    return TokenSinkResult::Continue;
                }
                let mut frame = import.stack.last().cloned().unwrap_or(Frame {
                    tag: String::new(),
                    style: DocumentInlineStyle::default(),
                    hidden: false,
                    pre: false,
                    block: None,
                    next_ordinal: None,
                });
                frame.tag = name.into();
                // Ordinals belong to the list container, never an inherited frame.
                frame.next_ordinal = None;
                frame.hidden |= matches!(
                    name,
                    "head"
                        | "script"
                        | "style"
                        | "iframe"
                        | "object"
                        | "template"
                        | "svg"
                        | "math"
                        | "noscript"
                );
                frame.hidden |= tag
                    .attrs
                    .iter()
                    .any(|attr| attr.name.local.as_ref() == "hidden");
                if !frame.hidden {
                    let attribute = |name: &str| {
                        tag.attrs.iter().find(|attr| attr.name.local.as_ref() == name).map(|attr| attr.value.as_ref())
                    };
                    if name == "img" {
                        if let Some(src) = attribute("src").filter(|src| crate::is_safe_document_destination(src)) {
                            let alt = attribute("alt").unwrap_or("").to_string();
                            import.spans.push(RichTextSpan { text: alt, style: frame.style.clone(), kind: DocumentInlineKind::Image { destination: src.to_string(), title: attribute("title").unwrap_or("").to_string() } });
                        }
                        return TokenSinkResult::Continue;
                    }
                    if name == "ol" {
                        frame.next_ordinal = Some(
                            attribute("start")
                                .and_then(|value| value.parse().ok())
                                .unwrap_or(1),
                        );
                    }
                    if attribute("style").is_some_and(|style| {
                        style
                            .split(';')
                            .filter_map(|rule| rule.split_once(':'))
                            .any(|(name, value)| {
                                name.trim().eq_ignore_ascii_case("white-space")
                                    && ["pre", "pre-wrap", "break-spaces"].iter().any(|candidate| {
                                        value.trim().eq_ignore_ascii_case(candidate)
                                    })
                            })
                    }) {
                        frame.pre = true;
                    }
                    if block_tag(name) {
                        import.finish_block();
                        import.explicit_empty =
                            matches!(name, "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "pre");
                        import.kind = match name {
                            "h1" => DocumentBlockKind::Heading(1),
                            "h2" => DocumentBlockKind::Heading(2),
                            "h3" => DocumentBlockKind::Heading(3),
                            "h4" => DocumentBlockKind::Heading(4),
                            "h5" => DocumentBlockKind::Heading(5),
                            "h6" => DocumentBlockKind::Heading(6),
                            "pre" => DocumentBlockKind::Code {
                                language: attribute("data-language").unwrap_or("").into(),
                            },
                            "blockquote" => DocumentBlockKind::Quote {
                                depth: import
                                    .stack
                                    .iter()
                                    .filter(|frame| frame.tag == "blockquote")
                                    .count()
                                    + 1,
                            },
                            "li" => DocumentBlockKind::ListItem {
                                marker: if let Some(list) = import
                                    .stack
                                    .iter_mut()
                                    .rev()
                                    .find(|frame| matches!(frame.tag.as_str(), "ol" | "ul"))
                                {
                                    if list.tag == "ol" {
                                        let ordinal = attribute("value")
                                            .and_then(|value| value.parse().ok())
                                            .or(list.next_ordinal)
                                            .unwrap_or(1);
                                        list.next_ordinal = Some(ordinal.saturating_add(1));
                                        format!("{ordinal}.")
                                    } else {
                                        "-".into()
                                    }
                                } else {
                                    "-".into()
                                },
                                depth: import
                                    .stack
                                    .iter()
                                    .filter(|frame| matches!(frame.tag.as_str(), "ol" | "ul"))
                                    .count()
                                    .max(1),
                                checked: attribute("data-checked").and_then(|value| match value {
                                    "true" => Some(true),
                                    "false" => Some(false),
                                    _ => None,
                                }),
                            },
                            _ => frame.block.clone().unwrap_or(DocumentBlockKind::Paragraph),
                        };
                        frame.block = Some(import.kind.clone());
                    }
                    match name {
                        "b" | "strong" => frame.style.bold = true,
                        "i" | "em" => frame.style.italic = true,
                        "s" | "del" | "strike" => frame.style.strike = true,
                        "code" => frame.style.code = true,
                        "sup" => frame.style.superscript = true,
                        "sub" => frame.style.subscript = true,
                        "pre" => frame.pre = true,
                        "br" => {
                            import.pending_space = false;
                            import
                                .spans
                                .push(RichTextSpan::styled("\n", frame.style.clone()));
                        }
                        "hr" => {
                            import.finish_block();
                            import.blocks.push(RichTextBlock {
                                kind: DocumentBlockKind::Rule,
                                spans: Vec::new(),
                            });
                        }
                        "a" => {
                            frame.style.link = tag
                                .attrs
                                .iter()
                                .find(|attr| attr.name.local.as_ref() == "href")
                                .map(|attr| attr.value.trim().to_string())
                                .filter(|value| {
                                    !value.is_empty() && crate::is_safe_document_destination(value)
                                })
                        }
                        _ => {}
                    }
                }
                // Honor foreign self-closing elements without swallowing following text.
                let foreign = matches!(name, "svg" | "math")
                    || import
                        .stack
                        .iter()
                        .any(|frame| matches!(frame.tag.as_str(), "svg" | "math"));
                // HTML's self-closing slash does not close non-void HTML elements.
                if !(tag.self_closing && foreign)
                    && !matches!(
                        name,
                        "area"
                            | "base"
                            | "br"
                            | "col"
                            | "embed"
                            | "hr"
                            | "img"
                            | "input"
                            | "link"
                            | "meta"
                            | "param"
                            | "source"
                            | "track"
                            | "wbr"
                    )
                {
                    if import.stack.len() == 256 {
                        import.too_deep = true;
                    } else {
                        import.stack.push(frame);
                    }
                }
                return match name {
                    "script" => TokenSinkResult::RawData(RawKind::ScriptData),
                    "style" | "iframe" | "xmp" => TokenSinkResult::RawData(RawKind::Rawtext),
                    "textarea" | "title" => TokenSinkResult::RawData(RawKind::Rcdata),
                    _ => TokenSinkResult::Continue,
                };
            }
            _ => {}
        }
        TokenSinkResult::Continue
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragment_range_preserves_unicode_empty_blocks_and_edit_boundaries() {
        let document = Document::rich_text([
            RichTextBlock::paragraph([RichTextSpan::new("甲<&")]),
            RichTextBlock::paragraph([]),
            RichTextBlock {
                kind: DocumentBlockKind::Heading(2),
                spans: vec![RichTextSpan::new("尾🙂")],
            },
            RichTextBlock::paragraph([]),
        ]);
        assert_eq!(document.source().text(), "甲<&\n\n尾🙂\n");
        let p = |text| format!("<p style=\"white-space:pre-wrap\">{text}</p>");
        let h = |text| format!("<h2 style=\"white-space:pre-wrap\">{text}</h2>");
        for (range, expected) in [
            (0..3, p("甲")),
            (3..5, p("&lt;&amp;")),
            (5..6, p("")), // The paragraph owns its following newline.
            (6..7, p("")),
            (7..14, h("尾🙂")),
            (10..14, h("🙂")),
            (0..0, String::new()),
            (3..3, p("")), // Preserve the existing interior caret behavior.
            (7..7, String::new()),
            (15..15, String::new()),
            (0..15, p("甲&lt;&amp;") + &p("") + &h("尾🙂") + &p("")),
        ] {
            assert_eq!(
                document.html_fragment(range.clone()).unwrap(),
                expected,
                "{range:?}"
            );
        }
        let edited = document.replace(7..10, "末\n新").unwrap();
        let start = edited.source().text().find('新').unwrap();
        let fragment = edited.html_fragment(start..start + "新🙂".len()).unwrap();
        let restored = Document::from_html_fragment(&fragment).unwrap();
        assert_eq!(restored.source().text(), "新🙂");
        // A line introduced by plain-text Enter uses the existing paragraph default.
        assert_eq!(restored.blocks()[0].kind, DocumentBlockKind::Paragraph);
        let empty = Document::rich_text([RichTextBlock::paragraph([])]);
        assert_eq!(empty.to_html_fragment().unwrap(), p(""));
    }

    #[test]
    fn supported_html_roundtrip_and_selection_keep_structure_without_active_markup() {
        let document = Document::rich_text([
            RichTextBlock {
                kind: DocumentBlockKind::Heading(2),
                spans: vec![RichTextSpan::new(" 标题  <&> ")],
            },
            RichTextBlock::paragraph([]),
            RichTextBlock::paragraph([
                RichTextSpan::styled(
                    "中文\"&",
                    DocumentInlineStyle {
                        bold: true,
                        italic: true,
                        strike: true,
                        code: true,
                        superscript: true,
                        link: Some("https://example.test/?a=1&b=\"q\"".into()),
                        ..Default::default()
                    },
                ),
                RichTextSpan::styled(
                    "x",
                    DocumentInlineStyle {
                        subscript: true,
                        ..Default::default()
                    },
                ),
            ]),
            RichTextBlock {
                kind: DocumentBlockKind::ListItem {
                    marker: "7.".into(),
                    depth: 1,
                    checked: Some(true),
                },
                spans: vec![RichTextSpan::new("item")],
            },
            RichTextBlock {
                kind: DocumentBlockKind::ListItem {
                    marker: "-".into(),
                    depth: 2,
                    checked: Some(false),
                },
                spans: vec![],
            },
            RichTextBlock {
                kind: DocumentBlockKind::Quote { depth: 3 },
                spans: vec![RichTextSpan::new("quote")],
            },
            RichTextBlock {
                kind: DocumentBlockKind::Rule,
                spans: vec![],
            },
            RichTextBlock {
                kind: DocumentBlockKind::Code {
                    language: "rust\"<&".into(),
                },
                spans: vec![RichTextSpan::new("  <script>evil()</script>\nlast\n")],
            },
            RichTextBlock::paragraph([]),
        ]);
        let html = document.to_html_fragment().unwrap();
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
        let restored = Document::from_html_fragment(&html).unwrap();
        assert_eq!(restored.source().text(), document.source().text());
        assert_eq!(restored.rich_text_blocks(), document.rich_text_blocks());
        let start = document.source().text().find("中文").unwrap();
        let selected = Document::from_html_fragment(
            &document.html_fragment(start..start + "中文".len()).unwrap(),
        )
        .unwrap();
        assert_eq!(selected.source().text(), "中文");
        assert!(selected.blocks()[0].inlines[0].style.bold);
        assert_eq!(
            document.html_fragment(start + 1..start + 3),
            Err(DocumentEditError::NotCharBoundary)
        );
        assert_eq!(
            document.html_fragment(0..usize::MAX),
            Err(DocumentEditError::InvalidRange)
        );
        assert_eq!(document.html_fragment(0..0).unwrap(), "");
        assert_eq!(
            Document::markdown("**source**").to_html_fragment(),
            Err(DocumentEditError::InvalidRange)
        );
        let unsafe_link = Document::rich_text([RichTextBlock::paragraph([RichTextSpan::styled(
            "<img onerror=evil()>",
            DocumentInlineStyle {
                link: Some("javascript:evil()".into()),
                ..Default::default()
            },
        )])])
        .to_html_fragment()
        .unwrap();
        assert!(!unsafe_link.contains("href="));
        assert!(!unsafe_link.contains("<img"));
        let list = Document::from_html_fragment(
            "<ol start='3'><li>A</li><li value='9'>B</li><li>C<ul><li>D</li></ul></li></ol>",
        )
        .unwrap();
        for (block, marker, depth) in [(0, "3.", 1), (1, "9.", 1), (2, "10.", 1), (3, "-", 2)] {
            assert!(
                matches!(&list.blocks()[block].kind, DocumentBlockKind::ListItem { marker: actual, depth: actual_depth, .. } if actual == marker && *actual_depth == depth)
            );
        }
        let deep = |depth| {
            Document::rich_text([RichTextBlock {
                kind: DocumentBlockKind::ListItem {
                    marker: "1.".into(),
                    depth,
                    checked: None,
                },
                spans: vec![RichTextSpan::styled(
                    "nested",
                    DocumentInlineStyle {
                        bold: true,
                        italic: true,
                        strike: true,
                        code: true,
                        superscript: true,
                        subscript: true,
                        link: Some("https://example.test/".into()),
                        ..Default::default()
                    },
                )],
            }])
        };
        let deepest = deep(124);
        assert_eq!(
            Document::from_html_fragment(&deepest.to_html_fragment().unwrap())
                .unwrap()
                .rich_text_blocks(),
            deepest.rich_text_blocks()
        );
        assert_eq!(
            deep(125).to_html_fragment(),
            Err(DocumentEditError::InvalidRange)
        );
        assert_eq!(
            deep(usize::MAX).to_html_fragment(),
            Err(DocumentEditError::InvalidRange)
        );
    }

    #[test]
    fn imports_schema_entities_and_omits_active_content() {
        let document = Document::from_html_fragment(r#"<html><head><title>secret</title></head><body>
            <!--StartFragment--><h2>标题 &amp; A</h2><p>Hello <strong>中<em>文</em></strong>
            <a href="https://example.test/?x=1&amp;y=2">link</a> <a href="java&#x73;cript:evil()">safe text</a>
            <script>if (a < b) attack()</script><span hidden>hidden</span><img src="file:secret">
            </p><pre>a  b\n&lt;tag&gt;</pre><!--EndFragment--></body></html>"#).unwrap();
        assert_eq!(
            document.source().text(),
            "标题 & A\nHello 中文 link safe text\na  b\\n<tag>"
        );
        assert_eq!(document.blocks()[0].kind, DocumentBlockKind::Heading(2));
        let inlines = &document.blocks()[1].inlines;
        assert!(inlines
            .iter()
            .any(|span| span.text == "文" && span.style.bold && span.style.italic));
        assert!(inlines
            .iter()
            .any(|span| span.style.link.as_deref() == Some("https://example.test/?x=1&y=2")));
        assert!(!inlines.iter().any(|span| span
            .style
            .link
            .as_deref()
            .is_some_and(|link| link.starts_with("javascript:"))));
        let nested = Document::from_html_fragment(
            "<blockquote><p>quote</p></blockquote><ul><li><p>item</p></li></ul>",
        )
        .unwrap();
        assert_eq!(
            nested.blocks()[0].kind,
            DocumentBlockKind::Quote { depth: 1 }
        );
        assert!(matches!(
            nested.blocks()[1].kind,
            DocumentBlockKind::ListItem { .. }
        ));
        let pre = Document::from_html_fragment("<svg/><pre>a  b\n&lt;tag&gt;\n</pre><p>after</p>")
            .unwrap();
        assert_eq!(pre.blocks().len(), 2);
        assert!(matches!(
            pre.blocks()[0].kind,
            DocumentBlockKind::Code { .. }
        ));
        assert_eq!(pre.blocks()[0].text(), "a  b\n<tag>\n");
        let edited = pre.replace(0..1, "A").unwrap();
        assert_eq!(edited.blocks().len(), 2);
        assert_eq!(edited.blocks()[0].text(), "A  b\n<tag>\n");
        assert_eq!(edited.blocks()[1].text(), "after");
        let terminal = Document::from_html_fragment("<pre>x</pre>")
            .unwrap()
            .replace(1..1, "<b>literal</b>\ny")
            .unwrap();
        assert_eq!(terminal.blocks().len(), 1);
        assert_eq!(terminal.blocks()[0].text(), "x<b>literal</b>\ny");
        assert!(matches!(
            terminal.blocks()[0].kind,
            DocumentBlockKind::Code { .. }
        ));
        assert_eq!(
            Document::from_html_fragment(&"<div>".repeat(257)).unwrap_err(),
            HtmlFragmentError::TooDeep
        );
        assert_eq!(
            Document::from_html_fragment(&"x".repeat(4 * 1024 * 1024 + 1)).unwrap_err(),
            HtmlFragmentError::TooLarge
        );
    }
}
