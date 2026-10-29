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
