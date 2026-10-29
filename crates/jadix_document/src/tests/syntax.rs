use super::*;
#[test]
fn highlighted_spans_preserve_every_source_byte() {
    let source = "/* 中文\n comment */\r\nfn main() { let value = \"🚀\"; }\n";
    let spans = highlight_code(source, "rs", false);
    assert_eq!(
        spans
            .iter()
            .map(|span| span.text.as_str())
            .collect::<String>(),
        source
    );
    #[cfg(feature = "syntax")]
    assert!(spans.windows(2).any(|pair| pair[0].color != pair[1].color));
}
