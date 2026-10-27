//! Block-level markdown → [`Element`] tree (MVP, not full CommonMark).
//!
//! Supports: ATX headings (`#`–`###`), paragraphs, unordered/ordered lists,
//! fenced \`\`\` code blocks, and `>` blockquotes. Inline markup inside blocks
//! reuses [`crate::text::parse_markdown_inline`].

use crate::core::tree::Element;

/// Parse a document through the framework-independent model and render its blocks.
pub fn parse_markdown_blocks(src: &str) -> Element {
    use jadix_document::{Document, DocumentBlockKind};

    let document = Document::markdown(src.to_owned());
    let blocks = document.blocks();
    let mut children = Vec::new();
    let mut index = 0;

    while index < blocks.len() {
        let block = &blocks[index];
        match &block.kind {
            DocumentBlockKind::Blank | DocumentBlockKind::Syntax => index += 1,
            DocumentBlockKind::Heading(level) => {
                children.push(heading_element(*level, block));
                index += 1;
            }
            DocumentBlockKind::Paragraph => {
                children.push(paragraph_element(block));
                index += 1;
            }
            DocumentBlockKind::Quote { depth } => {
                children.push(blockquote_element(block, *depth));
                index += 1;
            }
            DocumentBlockKind::ListItem {
                marker, depth: _, ..
            } if marker.is_empty() => {
                children.push(paragraph_element(block));
                index += 1;
            }
            DocumentBlockKind::ListItem { marker, depth, .. } => {
                let ordered = marker.chars().next().is_some_and(|ch| ch.is_ascii_digit());
                let start = index;
                index += 1;
                while index < blocks.len() {
                    match &blocks[index].kind {
                        DocumentBlockKind::ListItem {
                            marker: next_marker,
                            depth: next_depth,
                            ..
                        } if !next_marker.is_empty()
                            && *next_depth == *depth
                            && next_marker
                                .chars()
                                .next()
                                .is_some_and(|ch| ch.is_ascii_digit())
                                == ordered =>
                        {
                            index += 1;
                        }
                        _ => break,
                    }
                }
                children.push(list_element(&blocks[start..index], ordered));
            }
            DocumentBlockKind::Code { language } => {
                if block.edit_range.as_ref().map_or(true, |range| {
                    index == 0
                        || blocks[index - 1].edit_range.as_ref() != Some(range)
                        || !matches!(blocks[index - 1].kind, DocumentBlockKind::Code { .. })
                }) {
                    let body = block
                        .payload
                        .as_deref()
                        .map(str::to_owned)
                        .unwrap_or_else(|| block.text());
                    children.push(code_block_element(&body, language));
                }
                index += 1;
            }
            DocumentBlockKind::TableRow { header } => {
                children.push(table_row_element(block, *header));
                index += 1;
            }
            DocumentBlockKind::Rule => {
                let mut element = Element::new().h(1.0).bg([0.5, 0.5, 0.5, 0.5]);
                element.set_attr("jadix_md_rule", "1");
                children.push(element);
                index += 1;
            }
            DocumentBlockKind::Math
            | DocumentBlockKind::Chemistry
            | DocumentBlockKind::Molecule { .. }
            | DocumentBlockKind::Html => {
                let text = block
                    .payload
                    .as_deref()
                    .map(str::to_owned)
                    .unwrap_or_else(|| block.text());
                children.push(paragraph_element_with_text(&text));
                index += 1;
            }
        }
    }

    Element::new().flex_col().gap(8.0).children(children)
}

fn inline_spans(inlines: &[jadix_document::DocumentInline]) -> Vec<crate::style::TextSpan> {
    inlines
        .iter()
        .map(|inline| {
            use crate::style::TextSpan;

            let mut span = TextSpan::new(inline.text.clone());
            if inline.style.bold {
                span = span.weight(700);
            }
            if inline.style.italic {
                span = span.italic();
            }
            if inline.style.code {
                span = span.monospace().background([15, 23, 42, 28]).padding_x(3.0);
            }
            if let Some(color) = inline.style.color {
                span = span.color(color);
            }
            if let Some(background) = inline.style.background {
                span = span.background(background);
            }
            if let Some(size) = inline.style.font_size {
                span = span.size(size as f32);
            }
            if let Some(link) = &inline.style.link {
                span = span
                    .color([37, 99, 235, 255])
                    .underline()
                    .href(link.clone());
            }
            span
        })
        .collect()
}

pub(crate) fn inline_spans_for_source(src: &str) -> Vec<crate::style::TextSpan> {
    let mut spans = Vec::new();
    let mut rest = src;
    while let Some(newline) = rest.find('\n') {
        let (line, tail) = rest.split_at(newline);
        let document = jadix_document::Document::markdown(line.to_owned());
        spans.extend(
            document
                .blocks()
                .iter()
                .flat_map(|block| inline_spans(&block.inlines)),
        );
        spans.push(crate::style::TextSpan::new("\n"));
        rest = &tail[1..];
    }
    let document = jadix_document::Document::markdown(rest.to_owned());
    spans.extend(
        document
            .blocks()
            .iter()
            .flat_map(|block| inline_spans(&block.inlines)),
    );
    if spans.is_empty() {
        vec![crate::style::TextSpan::new("")]
    } else {
        spans
    }
}

fn inline_rich(block: &jadix_document::DocumentBlock) -> Element {
    Element::new().text_spans(inline_spans(&block.inlines))
}

fn heading_element(level: u8, block: &jadix_document::DocumentBlock) -> Element {
    let size = match level {
        1 => 28.0,
        2 => 22.0,
        _ => 18.0,
    };
    let weight = if level <= 2 { 700.0 } else { 600.0 };
    let mut element = inline_rich(block).text_size(size).font_weight(weight);
    element.set_attr("jadix_md_heading", level.to_string());
    element
}

fn paragraph_element(block: &jadix_document::DocumentBlock) -> Element {
    let mut element = inline_rich(block).text_size(14.0);
    element.set_attr("jadix_md_paragraph", "1");
    element
}

fn paragraph_element_with_text(text: &str) -> Element {
    let mut element = Element::new().text(text.to_owned()).text_size(14.0);
    element.set_attr("jadix_md_paragraph", "1");
    element
}

fn code_block_element(body: &str, language: &str) -> Element {
    let mut element = Element::new()
        .text(body.to_owned())
        .text_size(13.0)
        .p(8.0)
        .rounded(6.0)
        .bg([15.0 / 255.0, 23.0 / 255.0, 42.0 / 255.0, 1.0]);
    element.text_style.font_family = Some("monospace".into());
    element.set_attr("jadix_md_code", "1");
    if !language.is_empty() {
        element.set_attr("jadix_md_code_lang", language);
    }
    element
}

fn blockquote_element(block: &jadix_document::DocumentBlock, depth: usize) -> Element {
    let mut element = Element::new()
        .flex_col()
        .p(8.0)
        .ml(4.0 * depth as f32)
        .bg([148.0 / 255.0, 163.0 / 255.0, 184.0 / 255.0, 0.15])
        .child(inline_rich(block).text_size(14.0));
    element.set_attr("jadix_md_blockquote", "1");
    element
}

fn list_element(blocks: &[jadix_document::DocumentBlock], ordered: bool) -> Element {
    let children: Vec<Element> = blocks
        .iter()
        .enumerate()
        .map(|(index, block)| {
            let marker = match &block.kind {
                jadix_document::DocumentBlockKind::ListItem { marker, .. } if ordered => {
                    if marker.is_empty() {
                        format!("{}.", index + 1)
                    } else {
                        marker.clone()
                    }
                }
                _ => "•".to_owned(),
            };
            Element::new()
                .flex_row()
                .gap(6.0)
                .child(Element::new().text(marker).text_size(14.0))
                .child(inline_rich(block).text_size(14.0))
        })
        .collect();
    let mut element = Element::new().flex_col().gap(4.0).children(children);
    element.set_attr(
        if ordered {
            "jadix_md_ol"
        } else {
            "jadix_md_ul"
        },
        "1",
    );
    element
}

fn table_row_element(block: &jadix_document::DocumentBlock, header: bool) -> Element {
    let cells: Vec<Element> = block
        .cells
        .iter()
        .map(|cell| Element::new().text_spans(inline_spans(cell)))
        .collect();
    let mut element = Element::new().flex_row().gap(8.0).children(cells);
    element.set_attr("jadix_md_table_row", if header { "header" } else { "1" });
    element
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attr<'a>(el: &'a Element, key: &str) -> Option<&'a str> {
        el.get_attr(key).map(|s| s.as_str())
    }

    #[test]
    fn parses_heading_list_code_quote() {
        let src = r#"# Title

## Sub

A paragraph with **bold**.

- one
- two

1. first
2. second

```rust
fn main() {}
```

> quoted *text*
"#;
        let root = parse_markdown_blocks(src);
        assert!(root.children.len() >= 6);

        let heading = root
            .children
            .iter()
            .find(|c| attr(c, "jadix_md_heading") == Some("1"))
            .expect("h1");
        let has_title = heading
            .text_spans
            .as_ref()
            .is_some_and(|s| s.iter().any(|sp| sp.text.contains("Title")))
            || heading.text.as_deref().is_some_and(|t| t.contains("Title"));
        assert!(has_title);

        assert!(root
            .children
            .iter()
            .any(|c| attr(c, "jadix_md_ul") == Some("1")));
        assert!(root
            .children
            .iter()
            .any(|c| attr(c, "jadix_md_ol") == Some("1")));

        let code = root
            .children
            .iter()
            .find(|c| attr(c, "jadix_md_code") == Some("1"))
            .expect("code");
        assert!(code.text.as_deref().unwrap_or("").contains("fn main"));
        assert_eq!(attr(code, "jadix_md_code_lang"), Some("rust"));

        assert!(root
            .children
            .iter()
            .any(|c| attr(c, "jadix_md_blockquote") == Some("1")));
    }

    #[test]
    fn heading_only() {
        let root = parse_markdown_blocks("### Small");
        assert_eq!(root.children.len(), 1);
        assert_eq!(attr(&root.children[0], "jadix_md_heading"), Some("3"));
    }

    #[test]
    fn empty_input() {
        let root = parse_markdown_blocks("");
        assert!(root.children.is_empty());
    }
}
