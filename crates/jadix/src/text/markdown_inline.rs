//! Lightweight markdown-inspired **inline** markup → [`TextSpan`] list.
//!
//! Supports: `**bold**` / `__bold__`, `*italic*` / `_italic_`, `` `code` ``,
//! `[label](url)`, and plain text (including `\n`). Not a full CommonMark parser.

use crate::style::TextSpan;

/// Parse inline Markdown into framework text spans using the document parser.
pub fn parse_markdown_inline(input: &str) -> Vec<TextSpan> {
    crate::text::markdown::inline_spans_for_source(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bold_italic_code_link() {
        let spans = parse_markdown_inline("Hi **bold** and *it* plus `x` and [a](https://e.dev)");
        let texts: Vec<_> = spans.iter().map(|s| s.text.as_str()).collect();
        assert!(texts.contains(&"bold"));
        assert!(texts.contains(&"it"));
        assert!(texts.contains(&"x"));
        assert!(texts.contains(&"a"));
        let bold = spans.iter().find(|s| s.text == "bold").unwrap();
        assert_eq!(bold.weight, Some(700));
        let it = spans.iter().find(|s| s.text == "it").unwrap();
        assert!(it.italic);
        let code = spans.iter().find(|s| s.text == "x").unwrap();
        assert!(code.monospace);
        assert!(code.background.is_some());
        let link = spans.iter().find(|s| s.text == "a").unwrap();
        assert!(link.underline);
        assert_eq!(link.href.as_deref(), Some("https://e.dev"));
    }

    #[test]
    fn escape_and_newline() {
        let spans = parse_markdown_inline("a\\*b\nc");
        let concat: String = spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(concat, "a*b\nc");
    }

    #[test]
    fn underscore_bold() {
        let spans = parse_markdown_inline("__go__");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].text, "go");
        assert_eq!(spans[0].weight, Some(700));
    }
}
