use super::*;

#[cfg(feature = "document-editing")]
#[test]
fn structured_fragment_splice_preserves_unicode_styles_and_block_boundaries() {
    let original = Document::rich_text([RichTextBlock::paragraph([RichTextSpan::styled(
        "left中right",
        DocumentInlineStyle {
            italic: true,
            ..Default::default()
        },
    )])]);
    let fragment = Document::from_html_fragment("<b>新</b><h2>标题</h2>").unwrap();
    let pasted = original.replace_fragment(4..7, &fragment).unwrap();
    assert_eq!(pasted.source().text(), "left新\n标题right");
    assert_eq!(pasted.blocks()[1].kind, DocumentBlockKind::Heading(2));
    assert!(pasted.blocks()[0]
        .inlines
        .iter()
        .any(|inline| inline.text == "新" && inline.style.bold && !inline.style.italic));
    assert!(pasted.blocks()[1]
        .inlines
        .iter()
        .any(|inline| inline.text == "right" && inline.style.italic));
    assert_eq!(
        original.replace_fragment(5..7, &fragment).unwrap_err(),
        DocumentEditError::NotCharBoundary
    );
    let deleted = original
        .replace_fragment(4..7, &Document::rich_text([]))
        .unwrap();
    assert_eq!(deleted.source().text(), "leftright");
    assert_eq!(original.source().text(), "left中right");
}
#[test]
fn rich_atomic_blocks_keep_multiline_source_and_surrounding_paragraphs() {
    let document = Document::rich_text([
        RichTextBlock::paragraph([RichTextSpan::new("before")]),
        RichTextBlock {
            kind: DocumentBlockKind::Chemistry,
            spans: vec![RichTextSpan::new("H2 + O2\n-> H2O")],
        },
        RichTextBlock::paragraph([RichTextSpan::new("after")]),
    ]);
    assert_eq!(document.blocks().len(), 3);
    assert_eq!(document.blocks()[1].text(), "H2 + O2\n-> H2O");
    let changed = document.replace(8..9, "3").unwrap();
    assert_eq!(changed.blocks().len(), 3);
    assert_eq!(changed.blocks()[2].text(), "after");
}
#[test]
fn inline_math_survives_native_source_edits_and_json_roundtrip() {
    let document = Document::rich_text([RichTextBlock::paragraph([
        RichTextSpan::new("equation "),
        RichTextSpan::math("x^2"),
    ])]);
    let changed = document.replace(11..12, "3").unwrap();
    let math = changed.blocks()[0]
        .inlines
        .iter()
        .find(|inline| inline.kind == DocumentInlineKind::Math)
        .unwrap();
    assert_eq!(math.text, "x^3");
    let serialized = serde_json::to_string(&changed.rich_text_blocks()).unwrap();
    let restored =
        Document::rich_text(serde_json::from_str::<Vec<RichTextBlock>>(&serialized).unwrap());
    assert_eq!(
        restored.blocks()[0].inlines.last().unwrap().kind,
        DocumentInlineKind::Math
    );
}
#[test]
fn rich_styles_survive_edits_and_serialization() {
    let style = DocumentInlineStyle {
        bold: true,
        color: Some([200, 10, 20, 255]),
        ..Default::default()
    };
    let document = Document::rich_text([RichTextBlock::paragraph([
        RichTextSpan::styled("hello", style.clone()),
        RichTextSpan::new(" world"),
    ])]);
    let changed = document.replace(2..3, "XX").unwrap();
    assert_eq!(changed.source().text(), "heXXlo world");
    assert!(changed.blocks()[0]
        .inlines
        .iter()
        .filter(|inline| inline.source.end <= 6)
        .all(|inline| inline.style == style));
    let serialized = serde_json::to_string(&changed.rich_text_blocks()).unwrap();
    let blocks: Vec<RichTextBlock> = serde_json::from_str(&serialized).unwrap();
    assert_eq!(
        Document::rich_text(blocks.clone()).source().text(),
        changed.source().text()
    );
    assert_eq!(
        Document::rich_text(blocks).blocks()[0].inlines[0]
            .style
            .color,
        style.color
    );
}
