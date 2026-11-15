use super::*;

#[test]
fn literal_html_preserves_tags_autolinks_source_positions_and_edit_policy() {
    let source = "a <b>bold</b> <img  src=\"x.png\"> <https://example.com>\n\n<div>block</div>\n  literal\n</div>\n\nlast";
    assert!(Document::markdown(source)
        .blocks()
        .iter()
        .any(|block| block.kind == DocumentBlockKind::Html));

    let document = Document::markdown_with_raw_html(source, false);
    assert!(!document.raw_html_enabled());
    assert!(document
        .blocks()
        .iter()
        .all(|block| { block.kind != DocumentBlockKind::Html && block.payload.is_none() }));
    assert_eq!(
        document.blocks()[0].text(),
        "a <b>bold</b> <img  src=\"x.png\"> https://example.com"
    );
    let inlines = &document.blocks()[0].inlines;
    assert!(inlines
        .iter()
        .any(|inline| { inline.style.link.as_deref() == Some("https://example.com") }));
    assert!(inlines
        .iter()
        .all(|inline| inline.kind == DocumentInlineKind::Text));
    let tag = inlines
        .iter()
        .position(|inline| inline.text == "<b>")
        .unwrap();
    assert_eq!(
        document.blocks()[0].source_position(tag, 2),
        source.find("<b>").map(|start| start + 2)
    );
    assert_eq!(document.blocks()[2].text(), "<div>block</div>");
    assert_eq!(document.blocks()[3].text(), "  literal");

    let untouched = document.blocks().last().unwrap().id;
    let local = document.replace(0..1, "A").unwrap();
    assert!(!local.raw_html_enabled());
    assert_eq!(local.blocks().last().unwrap().id, untouched);
    assert!(local.blocks()[0].text().contains("<b>bold</b>"));
    let reparsed = local.replace(0..0, "prefix\n\n").unwrap();
    assert!(!reparsed.raw_html_enabled());
    assert!(reparsed
        .blocks()
        .iter()
        .all(|block| block.kind != DocumentBlockKind::Html));
    assert!(reparsed
        .blocks()
        .iter()
        .any(|block| block.text() == "  literal"));
    let in_block = source.find("literal").unwrap();
    let edited_block = document.replace(in_block..in_block + 7, "new").unwrap();
    assert!(edited_block
        .blocks()
        .iter()
        .any(|block| block.text() == "  new"));
}

#[test]
fn source_ranges_cover_unicode_crlf_and_empty_lines() {
    let document = Document::markdown("# \u{6807}\u{9898}\r\n\r\n**bold** and $x^2$\r\n");
    assert_eq!(document.blocks().len(), 4);
    assert_eq!(document.blocks()[0].kind, DocumentBlockKind::Heading(1));
    assert!(document.blocks()[2].inlines[0].style.bold);
    assert!(document.blocks()[2]
        .inlines
        .iter()
        .any(|inline| inline.kind == DocumentInlineKind::Math));
    for block in document.blocks() {
        assert!(document
            .source()
            .text()
            .is_char_boundary(block.source.start));
        assert!(document.source().text().is_char_boundary(block.source.end));
    }
    assert_eq!(document.block_at(document.source().len()), Some(3));
}

#[test]
fn markdown_folds_text_whitespace_and_maps_display_positions_to_source() {
    let document = Document::markdown("two   spaces and `keep  these`\n");
    let block = &document.blocks()[0];
    assert_eq!(block.text(), "two spaces and keep  these");

    let folded = block
        .inlines
        .iter()
        .position(|inline| inline.text == "two spaces and ")
        .unwrap();
    assert_eq!(block.source_position(folded, 3), Some(3));
    assert_eq!(block.source_position(folded, 4), Some(6));

    let code = block
        .inlines
        .iter()
        .find(|inline| inline.style.code)
        .unwrap();
    assert_eq!(code.text, "keep  these");
}

#[test]
fn markdown_normalizes_whitespace_across_removed_html_and_break_boundaries() {
    let source = "Text <!-- note --> after.\n\nline one<br>\n line two";
    let document = Document::markdown(source);
    let block = &document.blocks()[0];
    assert_eq!(block.text(), "Text after.");
    assert_eq!(document.blocks()[2].text(), "line one\nline two");

    let after = block
        .inlines
        .iter()
        .position(|inline| inline.text == "after.")
        .unwrap();
    assert_eq!(block.source_position(after, 0), source.find("after."));
}

#[test]
fn tables_tasks_code_and_math_are_semantic_blocks() {
    let document = Document::markdown(
        "- [x] done\n- pending\n\n| a | b |\n| :- | -: |\n| 1 | 2 |\n\n```rust\nlet a = 1;\n```\n\n$$\n\\frac{1}{2}\n$$\n",
    );
    assert!(document.blocks().iter().any(|block| matches!(
        block.kind,
        DocumentBlockKind::ListItem {
            checked: Some(true),
            ..
        }
    )));
    assert_eq!(
        document
            .blocks()
            .iter()
            .filter(|block| matches!(block.kind, DocumentBlockKind::TableRow { .. }))
            .count(),
        2
    );
    assert!(document
        .blocks()
        .iter()
        .any(|block| matches!(block.kind, DocumentBlockKind::Code { .. })));
    assert!(document
        .blocks()
        .iter()
        .any(|block| block.kind == DocumentBlockKind::Math));
}

#[test]
fn local_edit_reuses_unaffected_blocks_and_structural_edit_reparses() {
    let document = Document::markdown("alpha\n\nbeta\n");
    let old_id = document.blocks()[2].id;
    let changed = document.replace(1..2, "XX").unwrap();
    assert_eq!(changed.blocks()[2].id, old_id);
    assert_eq!(
        changed.blocks()[2].source.start,
        document.blocks()[2].source.start + 1
    );
    let changed = changed.replace(0..0, "# ").unwrap();
    assert_eq!(changed.blocks()[0].kind, DocumentBlockKind::Heading(1));
}

#[test]
fn large_code_fence_is_split_before_ui_materialization() {
    let document = Document::markdown(format!("```rust\n{}```\n", "let x = 1;\n".repeat(100_000)));
    assert!(document.blocks().len() >= 100_002);
    assert!(document
        .blocks()
        .iter()
        .all(|block| block.source.len() < 40));
}

#[test]
fn code_edit_reuses_rows_and_preserves_whole_fence_range() {
    let original = Document::markdown("```rust\nlet one = 1;\nlet two = 2;\n```\n\nafter");
    let byte = original.source().text().find('1').unwrap();
    let changed = original.replace(byte..byte + 1, "100").unwrap();
    assert_eq!(changed.blocks()[2].id, original.blocks()[2].id);
    assert_eq!(
        changed.blocks()[0].edit_range,
        changed.blocks()[2].edit_range
    );
    assert!(changed.blocks()[1].text().contains("100"));
    assert_eq!(
        changed.blocks()[2].source.start,
        original.blocks()[2].source.start + 2
    );
    assert_eq!(changed.blocks()[2].edit_range.as_ref().unwrap().start, 0);
}
