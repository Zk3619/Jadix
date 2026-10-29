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
#[path = "tests/markdown_inline.rs"]
mod tests;
