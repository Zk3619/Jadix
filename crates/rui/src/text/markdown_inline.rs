//! Lightweight markdown-inspired **inline** markup → [`TextSpan`] list.
//!
//! Supports: `**bold**` / `__bold__`, `*italic*` / `_italic_`, `` `code` ``,
//! `[label](url)`, and plain text (including `\n`). Not a full CommonMark parser.

use crate::style::TextSpan;

/// Parse a single paragraph (or multi-line string) of inline markdown into rich spans.
pub fn parse_markdown_inline(input: &str) -> Vec<TextSpan> {
    let mut out = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0usize;
    let mut plain = String::new();

    let flush_plain = |plain: &mut String, out: &mut Vec<TextSpan>| {
        if !plain.is_empty() {
            out.push(TextSpan::new(std::mem::take(plain)));
        }
    };

    while i < chars.len() {
        let c = chars[i];

        // Escape: \* \_ \` \[
        if c == '\\' && i + 1 < chars.len() {
            plain.push(chars[i + 1]);
            i += 2;
            continue;
        }

        // Code: `...`
        if c == '`' {
            if let Some(end) = find_close(&chars, i + 1, |ch| ch == '`') {
                flush_plain(&mut plain, &mut out);
                let body: String = chars[i + 1..end].iter().collect();
                out.push(code_span(body));
                i = end + 1;
                continue;
            }
        }

        // Bold: **...** or __...__
        if matches_delim(&chars, i, &['*', '*']) || matches_delim(&chars, i, &['_', '_']) {
            let d0 = chars[i];
            if let Some(end) = find_double_close(&chars, i + 2, d0) {
                flush_plain(&mut plain, &mut out);
                let body: String = chars[i + 2..end].iter().collect();
                out.push(TextSpan::new(body).weight(700));
                i = end + 2;
                continue;
            }
        }

        // Link: [label](url)
        if c == '[' {
            if let Some((label_end, url_end)) = find_link(&chars, i) {
                flush_plain(&mut plain, &mut out);
                let label: String = chars[i + 1..label_end].iter().collect();
                let url: String = chars[label_end + 2..url_end].iter().collect();
                out.push(link_span(label, url));
                i = url_end + 1;
                continue;
            }
        }

        // Italic: *...* or _..._ (single, not part of **)
        if (c == '*' || c == '_') && !matches_delim(&chars, i, &[c, c]) && i + 1 < chars.len() {
            if let Some(end) = find_single_close(&chars, i + 1, c) {
                flush_plain(&mut plain, &mut out);
                let body: String = chars[i + 1..end].iter().collect();
                out.push(TextSpan::new(body).italic());
                i = end + 1;
                continue;
            }
        }

        plain.push(c);
        i += 1;
    }
    flush_plain(&mut plain, &mut out);
    if out.is_empty() {
        out.push(TextSpan::new(""));
    }
    out
}

fn code_span(body: String) -> TextSpan {
    TextSpan::new(body)
        .monospace()
        .background([15, 23, 42, 28])
        .padding_x(3.0)
}

fn link_span(label: String, url: String) -> TextSpan {
    TextSpan::new(label)
        .color([37, 99, 235, 255])
        .underline()
        .href(url)
}

fn matches_delim(chars: &[char], i: usize, delim: &[char]) -> bool {
    if i + delim.len() > chars.len() {
        return false;
    }
    chars[i..i + delim.len()] == *delim
}

fn find_close(chars: &[char], start: usize, pred: impl Fn(char) -> bool) -> Option<usize> {
    let mut i = start;
    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() {
            i += 2;
            continue;
        }
        if pred(chars[i]) {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn find_double_close(chars: &[char], start: usize, d: char) -> Option<usize> {
    let mut i = start;
    while i + 1 < chars.len() {
        if chars[i] == '\\' {
            i += 2;
            continue;
        }
        if chars[i] == d && chars[i + 1] == d {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn find_single_close(chars: &[char], start: usize, d: char) -> Option<usize> {
    let mut i = start;
    while i < chars.len() {
        if chars[i] == '\\' {
            i += 2;
            continue;
        }
        // Don't treat ** as italic close.
        if chars[i] == d && !(i + 1 < chars.len() && chars[i + 1] == d) {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn find_link(chars: &[char], start: usize) -> Option<(usize, usize)> {
    // start at '['
    let label_end = find_close(chars, start + 1, |c| c == ']')?;
    if label_end + 1 >= chars.len() || chars[label_end + 1] != '(' {
        return None;
    }
    let url_end = find_close(chars, label_end + 2, |c| c == ')')?;
    Some((label_end, url_end))
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
