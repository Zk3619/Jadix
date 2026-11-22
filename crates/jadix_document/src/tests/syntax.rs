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

#[test]
fn snapshot_colors_preserve_multiline_byte_coordinates_and_bounded_fallback() {
    let document = crate::TextDocument::new("/* 中文\r\nlet value = 1;\r\n*/\nlet value = 1;\n");
    let highlighted = highlight_document(document.clone(), "rs", false);
    assert_eq!(highlighted.document(), &document);
    for annotation in highlighted.annotations() {
        assert!(document.text().get(annotation.range.clone()).is_some());
    }
    #[cfg(feature = "syntax")]
    {
        let color_at = |highlighted: &crate::AnnotatedText<[u8; 4]>, byte| {
            highlighted
                .annotations()
                .iter()
                .find(|a| a.range.contains(&byte))
                .unwrap()
                .value
        };
        let comment = document.line_range(1).unwrap().start;
        let code = document.line_range(3).unwrap().start;
        assert_ne!(
            color_at(&highlighted, comment),
            color_at(&highlighted, code)
        );
        let dark = highlight_document(document.clone(), "rs", true);
        assert_ne!(color_at(&highlighted, comment), color_at(&dark, comment));
    }
    #[cfg(not(feature = "syntax"))]
    assert!(highlighted.annotations().is_empty());

    let oversized_line = crate::TextDocument::new("x".repeat(64 * 1024 + 1));
    assert!(
        highlight_document(oversized_line, "rs", false)
            .annotations()
            .is_empty()
    );
}
