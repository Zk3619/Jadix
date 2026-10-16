//! Block-level markdown → [`Element`] tree (MVP, not full CommonMark).
//!
//! Supports: ATX headings (`#`–`###`), paragraphs, unordered/ordered lists,
//! fenced \`\`\` code blocks, and `>` blockquotes. Inline markup inside blocks
//! reuses [`crate::text::parse_markdown_inline`].

use crate::core::tree::Element;
use crate::text::parse_markdown_inline;

/// Parse a markdown document into a vertical stack of block elements.
pub fn parse_markdown_blocks(src: &str) -> Element {
    let mut children = Vec::new();
    let lines: Vec<&str> = src.lines().collect();
    let mut i = 0usize;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_end();

        if trimmed.trim().is_empty() {
            i += 1;
            continue;
        }

        if let Some(lang) = fence_open(trimmed) {
            i += 1;
            let mut body = String::new();
            while i < lines.len() && !is_fence_close(lines[i]) {
                if !body.is_empty() {
                    body.push('\n');
                }
                body.push_str(lines[i]);
                i += 1;
            }
            if i < lines.len() {
                i += 1;
            }
            children.push(code_block_element(&body, lang.as_deref()));
            continue;
        }

        if let Some((level, text)) = parse_heading(trimmed) {
            children.push(heading_element(level, text));
            i += 1;
            continue;
        }

        if trimmed.trim_start().starts_with('>') {
            let mut quote_body = String::new();
            while i < lines.len() {
                let t = lines[i].trim_end();
                let ts = t.trim_start();
                if !ts.starts_with('>') {
                    break;
                }
                let rest = ts[1..].strip_prefix(' ').unwrap_or(&ts[1..]);
                if !quote_body.is_empty() {
                    quote_body.push('\n');
                }
                quote_body.push_str(rest);
                i += 1;
            }
            children.push(blockquote_element(&quote_body));
            continue;
        }

        if is_ul_item(trimmed) {
            let mut items = Vec::new();
            while i < lines.len() && is_ul_item(lines[i].trim_end()) {
                items.push(list_item_text(lines[i].trim_end()));
                i += 1;
            }
            children.push(ul_element(&items));
            continue;
        }

        if is_ol_item(trimmed) {
            let mut items = Vec::new();
            while i < lines.len() && is_ol_item(lines[i].trim_end()) {
                items.push(list_item_text(lines[i].trim_end()));
                i += 1;
            }
            children.push(ol_element(&items));
            continue;
        }

        let mut para = String::new();
        while i < lines.len() {
            let t = lines[i].trim_end();
            if t.trim().is_empty()
                || fence_open(t).is_some()
                || parse_heading(t).is_some()
                || t.trim_start().starts_with('>')
                || is_ul_item(t)
                || is_ol_item(t)
            {
                break;
            }
            if !para.is_empty() {
                para.push(' ');
            }
            para.push_str(t.trim());
            i += 1;
        }
        if !para.is_empty() {
            children.push(paragraph_element(&para));
        }
    }

    Element::new().flex_col().gap(8.0).children(children)
}

fn fence_open(line: &str) -> Option<Option<String>> {
    let t = line.trim_start();
    if !t.starts_with("```") {
        return None;
    }
    let rest = t[3..].trim();
    if rest.is_empty() {
        Some(None)
    } else {
        Some(Some(rest.to_string()))
    }
}

fn is_fence_close(line: &str) -> bool {
    let t = line.trim();
    t == "```" || (t.starts_with("```") && t.chars().all(|c| c == '`'))
}

fn parse_heading(line: &str) -> Option<(u8, &str)> {
    let t = line.trim_start();
    let mut level = 0u8;
    for c in t.chars() {
        if c == '#' {
            level = level.saturating_add(1);
            if level > 3 {
                return None;
            }
        } else {
            break;
        }
    }
    if !(1..=3).contains(&level) {
        return None;
    }
    let after = &t[level as usize..];
    if !after.is_empty() && !after.starts_with(' ') {
        return None;
    }
    Some((level, after.trim()))
}

fn is_ul_item(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("- ") || t.starts_with("* ") || t.starts_with("+ ")
}

fn is_ol_item(line: &str) -> bool {
    let t = line.trim_start();
    let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits == 0 {
        return false;
    }
    let rest = &t[digits..];
    rest.starts_with(". ") || rest.starts_with(") ")
}

fn list_item_text(line: &str) -> String {
    let t = line.trim_start();
    if is_ul_item(line) {
        return t[2..].trim().to_string();
    }
    let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
    let rest = &t[digits..];
    let body = if rest.starts_with(". ") || rest.starts_with(") ") {
        &rest[2..]
    } else {
        rest
    };
    body.trim().to_string()
}

fn inline_rich(text: &str) -> Element {
    Element::new().text_spans(parse_markdown_inline(text))
}

fn heading_element(level: u8, text: &str) -> Element {
    let size = match level {
        1 => 28.0,
        2 => 22.0,
        _ => 18.0,
    };
    let weight = if level <= 2 { 700.0 } else { 600.0 };
    let mut el = inline_rich(text).text_size(size).font_weight(weight);
    el.set_attr("jadix_md_heading", level.to_string());
    el
}

fn paragraph_element(text: &str) -> Element {
    let mut el = inline_rich(text).text_size(14.0);
    el.set_attr("jadix_md_paragraph", "1");
    el
}

fn code_block_element(body: &str, lang: Option<&str>) -> Element {
    let mut el = Element::new()
        .text(body.to_string())
        .text_size(13.0)
        .p(8.0)
        .rounded(6.0)
        .bg([15.0 / 255.0, 23.0 / 255.0, 42.0 / 255.0, 1.0]);
    el.text_style.font_family = Some("monospace".into());
    el.set_attr("jadix_md_code", "1");
    if let Some(l) = lang {
        el.set_attr("jadix_md_code_lang", l);
    }
    el
}

fn blockquote_element(body: &str) -> Element {
    let mut el = Element::new()
        .flex_col()
        .p(8.0)
        .ml(4.0)
        .bg([148.0 / 255.0, 163.0 / 255.0, 184.0 / 255.0, 0.15])
        .child(inline_rich(body).text_size(14.0));
    el.set_attr("jadix_md_blockquote", "1");
    el
}

fn ul_element(items: &[String]) -> Element {
    let children: Vec<Element> = items
        .iter()
        .map(|t| {
            Element::new()
                .flex_row()
                .gap(6.0)
                .child(Element::new().text("•").text_size(14.0))
                .child(inline_rich(t).text_size(14.0))
        })
        .collect();
    let mut el = Element::new().flex_col().gap(4.0).children(children);
    el.set_attr("jadix_md_ul", "1");
    el
}

fn ol_element(items: &[String]) -> Element {
    let children: Vec<Element> = items
        .iter()
        .enumerate()
        .map(|(idx, t)| {
            Element::new()
                .flex_row()
                .gap(6.0)
                .child(Element::new().text(format!("{}.", idx + 1)).text_size(14.0))
                .child(inline_rich(t).text_size(14.0))
        })
        .collect();
    let mut el = Element::new().flex_col().gap(4.0).children(children);
    el.set_attr("jadix_md_ol", "1");
    el
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
