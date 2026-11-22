use super::*;

#[test]
fn appended_markdown_matches_full_parse_through_syntax_and_reference_changes() {
    let cases = [
        "reply\ncontinuation\n\nnext **bold**",
        "heading\n===\n\nnext",
        // GPUI Kit markdown-rs panic regression (markdown-rs#275).
        "a\n===\n---\nb\n===",
        "a\r\n===\r\n---\r\nb\r\n===",
        "| a | b |\n| - | - |\n| c | d |\n\nnext",
        "- list\n\n  continuation\n  - nested\n\nnext",
        "> quote\n\n> continued\n\nnext",
        "```rust\ncode\n\nnot a boundary\n```\n\nnext",
        "~~~\ncode\n\nmore\n~~~\n\nnext",
        "<div>\n\nhtml\n</div>\n\nnext",
        "<!-- comment\n\ncontinued -->\n\nnext",
        "a <span\n title=\"x\">text</span>\n\nnext",
        "$$\nx^2\n\n+y\n$$\n\nnext",
        "    indented\n\n    continued\n\nnext",
        "[late]\n\n[late]: https://example.com\n\nnext",
        "note[^id]\n\n[^id]: footnote\n\n    continued\n\nnext",
        "中文 &amp; `  code`\r\n\r\nnext",
    ];
    for raw_html in [false, true] {
        for tail in cases {
            let prefix = "stable 中文 &amp; text\n\nanchor\n\n";
            let mut document = Document::markdown_with_raw_html(prefix, raw_html);
            for character in tail.chars() {
                document = document.append(character.encode_utf8(&mut [0; 4])).unwrap();
                let full = Document::markdown_with_raw_html(document.source().text(), raw_html);
                assert_eq!(document.blocks().len(), full.blocks().len(), "{tail:?}");
                for (actual, expected) in document.blocks().iter().zip(full.blocks()) {
                    assert_eq!(actual.kind, expected.kind, "{}", document.source().text());
                    assert_eq!(actual.source, expected.source);
                    assert_eq!(actual.edit_range, expected.edit_range);
                    assert_eq!(actual.inlines, expected.inlines);
                    assert_eq!(actual.cells, expected.cells);
                    assert_eq!(actual.alignments, expected.alignments);
                    assert_eq!(actual.payload, expected.payload);
                    assert_eq!(actual.source_positions, expected.source_positions);
                }
            }
        }
    }
    let document = Document::markdown("[late]\n\nanchor\n\nreply");
    let updated = document.append("\n\n[late]: https://example.com").unwrap();
    assert_eq!(
        updated.blocks()[0].inlines[0].style.link.as_deref(),
        Some("https://example.com")
    );
    assert_ne!(updated.blocks()[0].id, document.blocks()[0].id);
}

#[test]
fn streamed_append_reuses_prefix_and_bounds_parser_work() {
    let prefix = "stable paragraph\n\n".repeat(2_000);
    let mut document = Document::markdown(format!("{prefix}reply"));
    let retained = document.blocks()[0].clone();
    PARSED_MARKDOWN_BYTES.with(|bytes| bytes.set(0));
    for index in 0..200 {
        document = document.append(&format!(" chunk {index}\n\nnext")).unwrap();
        assert_eq!(document.blocks()[0].id, retained.id);
        assert!(Arc::ptr_eq(
            &document.blocks()[0].inlines,
            &retained.inlines
        ));
    }
    let bytes = PARSED_MARKDOWN_BYTES.with(|bytes| bytes.get());
    assert!(
        bytes < 20_000,
        "reparsed {bytes} bytes of a {} byte document",
        document.source().len()
    );
    assert!(document.source().len() > 36_000);
}

#[test]
fn deeply_nested_markdown_keeps_content_and_source_positions_on_a_small_stack() {
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(|| {
            let depth = 8192;
            let source = format!(
                "{}**deep** &amp; <b>tail</b>\n\noutside",
                "> ".repeat(depth)
            );
            for raw_html in [false, true] {
                let document = Document::markdown_with_raw_html(source.as_str(), raw_html);
                let block = document
                    .blocks()
                    .iter()
                    .find(|block| block.text().contains("deep"))
                    .unwrap();
                assert!(block.text().contains("&"));
                assert!(block.text().contains("tail"));
                assert!(block
                    .inlines
                    .iter()
                    .any(|inline| inline.text == "deep" && inline.style.bold));
                let deep = block
                    .inlines
                    .iter()
                    .position(|inline| inline.text == "deep")
                    .unwrap();
                assert_eq!(
                    block.source_position(deep, 2),
                    source.find("deep").map(|start| start + 2)
                );
                if !raw_html {
                    assert_eq!(block.kind, DocumentBlockKind::Quote { depth });
                    assert!(block.text().contains("<b>tail</b>"));
                }
                let offset = source.find("deep").unwrap();
                let edited = document.replace(offset..offset + 4, "edited").unwrap();
                assert!(edited
                    .blocks()
                    .iter()
                    .any(|block| block.text().contains("edited")));
                assert_eq!(edited.blocks().last().unwrap().text(), "outside");
                assert_eq!(edited.raw_html_enabled(), raw_html);
            }

            let mut source = String::new();
            for depth in 0..192 {
                source.push_str(&format!("{}- item{depth}\n", "  ".repeat(depth)));
            }
            source.push_str("\noutside");
            let document = Document::markdown(source.as_str());
            let deepest = document
                .blocks()
                .iter()
                .find(|block| block.text() == "item191")
                .unwrap();
            assert!(matches!(
                deepest.kind,
                DocumentBlockKind::ListItem { depth: 192, .. }
            ));
            assert_eq!(document.blocks().last().unwrap().text(), "outside");
        })
        .unwrap()
        .join()
        .unwrap();
}

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

    for (source, expected) in [
        ("x `  y`", "x   y"),
        ("`  y` x", "  y x"),
        ("x *`  y`*", "x   y"),
    ] {
        for raw_html in [false, true] {
            let document = Document::markdown_with_raw_html(source, raw_html);
            let block = &document.blocks()[0];
            assert_eq!(block.text(), expected, "source {source:?}");
            let index = block
                .inlines
                .iter()
                .position(|inline| inline.style.code)
                .unwrap();
            assert_eq!(block.inlines[index].text, "  y");
            assert_eq!(block.source_position(index, 2), source.find('y'));
        }
    }
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
