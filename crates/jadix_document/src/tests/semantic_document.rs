use super::*;

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
fn tables_tasks_code_and_math_are_semantic_blocks() {
    let document = Document::markdown("- [x] done\n- pending\n\n| a | b |\n| :- | -: |\n| 1 | 2 |\n\n```rust\nlet a = 1;\n```\n\n$$\n\\frac{1}{2}\n$$\n");
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
