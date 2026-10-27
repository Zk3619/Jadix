#![cfg(feature = "document-editing")]

use jadix_components::core::reactivity::create_root;
use jadix_components::core::scroll_viewport::scroll_offset_y_signal_key;
use jadix_components::core::tree::LayoutTree;
use jadix_components::prelude::*;

fn find<'a>(element: &'a Element, attribute: &str, value: &str) -> Option<&'a Element> {
    if element
        .attrs
        .get(attribute)
        .is_some_and(|found| found == value)
    {
        return Some(element);
    }
    element
        .children
        .iter()
        .find_map(|child| find(child, attribute, value))
}

fn sync(element: &mut Element, layout: &mut LayoutTree) {
    if let Some(callback) = element.reactive_sync.clone() {
        callback.run(element, layout);
    }
    for child in &mut element.children {
        sync(child, layout);
    }
}

fn texts(element: &Element) -> String {
    let mut result = element.text.clone().unwrap_or_default();
    for child in &element.children {
        result.push_str(&texts(child));
    }
    result
}

#[test]
fn commonmark_blocks_retain_source_and_render_real_semantics() {
    let source = "# title\n\n- [x] done\n- pending\n\n| a | b |\n| :- | -: |\n| 1 | 2 |\n\n$$\n\\frac{1}{2}\n$$\n";
    let document = Document::markdown(source);
    assert_eq!(document.source().text(), source);
    assert!(document
        .blocks()
        .iter()
        .any(|block| matches!(block.kind, DocumentBlockKind::Heading(1))));
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
        .any(|block| block.kind == DocumentBlockKind::Math));
    for block in document.blocks() {
        for inline in block.inlines.iter() {
            let start = block.source.start + inline.source.start;
            let end = block.source.start + inline.source.end;
            assert!(start <= end && end <= source.len());
        }
    }
}

#[test]
fn nested_lists_do_not_duplicate_source_ranges() {
    let document = Document::markdown("- parent\n  - child\n  - child two\n- sibling\n");
    for pair in document.blocks().windows(2) {
        assert!(
            pair[0].source.end <= pair[1].source.start,
            "overlapping blocks: {:?} {:?}",
            pair[0].source,
            pair[1].source
        );
    }
}

#[test]
fn local_edit_matches_full_parse_and_reuses_later_blocks() {
    let original = Document::markdown("alpha\n\nbeta\n");
    let last_id = original.blocks()[2].id;
    let edited = original.replace(1..2, "XYZ").unwrap();
    let reparsed = Document::markdown(edited.source().text());
    assert_eq!(edited.blocks()[2].id, last_id);
    assert_eq!(
        edited
            .blocks()
            .iter()
            .map(|block| (&block.kind, block.text()))
            .collect::<Vec<_>>(),
        reparsed
            .blocks()
            .iter()
            .map(|block| (&block.kind, block.text()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn live_editor_reveals_only_active_line_and_commits_native_input() {
    let (_owner, ()) = create_root(|| {
        let state = DocumentEditorState::new("# title\n\n**bold**\n\nparagraph");
        state.set_selection(DocumentSelection::caret(13)).unwrap();
        let mut root: Element = document_editor("live-doc", state.clone())
            .viewport_h(400.0)
            .into();
        let mut layout = LayoutTree::new();
        layout.build_and_layout(&mut root, 640.0, 400.0, [-1.0, -1.0]);
        assert!(find(&root, "data-document-source", "true").is_some());
        assert!(texts(&root).contains("title"));
        assert!(!texts(&root).contains("# title"));
        let field = "live-doc__source";
        jadix_components::utils::text_field::commit_value(field, "**bold!**".into());
        use_signal(
            &jadix_components::utils::text_field::input_caret_signal_key(field),
            || None::<usize>,
        )
        .set(Some(7));
        sync(&mut root, &mut layout);
        assert_eq!(
            state.document().source().text(),
            "# title\n\n**bold!**\n\nparagraph"
        );
        jadix_components::utils::text_field::commit_value(field, "**bold!!**".into());
        use_signal(
            &jadix_components::utils::text_field::input_caret_signal_key(field),
            || None::<usize>,
        )
        .set(Some(8));
        sync(&mut root, &mut layout);
        assert!(state.document().source().text().contains("**bold!!**"));
        let paragraph = state.document().source().text().find("paragraph").unwrap();
        state
            .set_selection(DocumentSelection::caret(paragraph))
            .unwrap();
        sync(&mut root, &mut layout);
        assert!(texts(&root).contains("bold!!"));
        assert!(!texts(&root).contains("**bold!!**"));
        assert!(state.undo());
        sync(&mut root, &mut layout);
        assert!(state.document().source().text().contains("**bold!**"));
    });
}

#[test]
fn revealing_a_different_block_resets_the_native_source_scroll() {
    let (_owner, ()) = create_root(|| {
        let state = DocumentEditorState::new("paragraph\n\n| A | B |\n| --- | --- |\n| 1 | 2 |\n");
        state.set_selection(DocumentSelection::caret(0)).unwrap();
        let mut root: Element = document_editor("source-scroll-reset", state.clone()).into();
        let mut layout = LayoutTree::new();
        let scroll = jadix_components::utils::text_field::textarea_scroll_rest(
            "source-scroll-reset__source",
        );
        let offset = use_signal(&scroll_offset_y_signal_key(&scroll), || 0.0_f32);
        offset.set_silent(240.0);
        state.set_selection(DocumentSelection::caret(38)).unwrap();
        sync(&mut root, &mut layout);
        assert_eq!(offset.peek(), 0.0);
        let source = find(&root, "data-document-source", "true").unwrap();
        assert!(texts(source).contains("| A | B |"));
        assert!(texts(source).contains("| --- | --- |"));
    });
}

#[test]
fn document_view_virtualizes_large_code_and_handles_stale_scroll() {
    let (_owner, ()) = create_root(|| {
        let document = use_signal("large-semantic-doc", || {
            Document::markdown(format!("```rust\n{}```\n", "let x = 1;\n".repeat(100_000)))
        });
        let mut root: Element = document_view("large-doc-view")
            .document_signal(document)
            .viewport_h(220.0)
            .into();
        let mut layout = LayoutTree::new();
        layout.build_and_layout(&mut root, 640.0, 220.0, [-1.0, -1.0]);
        sync(&mut root, &mut layout);
        fn body(element: &Element) -> Option<&Element> {
            if element.attrs.contains_key("data-document-count") {
                return Some(element);
            }
            element.children.iter().find_map(body)
        }
        let materialized: usize = body(&root).unwrap().attrs["data-document-materialized"]
            .parse()
            .unwrap();
        assert!(materialized < 40);
        use_signal(&scroll_offset_y_signal_key("large-doc-view"), || 0.0_f32)
            .set_silent(1_300_000.0);
        sync(&mut root, &mut layout);
        assert!(
            body(&root).unwrap().attrs["data-document-materialized"]
                .parse::<usize>()
                .unwrap()
                < 40
        );
        document.set(Document::markdown("short"));
        sync(&mut root, &mut layout);
        assert_eq!(body(&root).unwrap().attrs["data-document-count"], "1");
        assert!(texts(&root).contains("short"));
    });
}

#[test]
fn selection_search_and_undo_cover_unmounted_content() {
    let text = format!("start\n{}\u{4e2d}\u{6587} end", "middle\n".repeat(10_000));
    let state = DocumentEditorState::new(text.clone());
    let range = state.find_next("\u{4e2d}\u{6587}", false, true).unwrap();
    assert_eq!(state.selected_text(), "\u{4e2d}\u{6587}");
    state.replace_selection("tail").unwrap();
    assert!(state.document().source().text().ends_with("tail end"));
    assert!(state.undo());
    assert_eq!(state.selection().range(), range);
    assert_eq!(state.document().source().text(), text);
}

#[test]
fn rich_text_formatting_preserves_source_and_is_undoable() {
    let rich = Document::rich_text([RichTextBlock::paragraph([RichTextSpan::new("hello world")])]);
    let state = DocumentEditorState::new(rich);
    state
        .set_selection(DocumentSelection {
            anchor: 0,
            caret: 5,
        })
        .unwrap();
    state.toggle_mark(DocumentMark::Bold).unwrap();
    assert_eq!(state.document().source().text(), "hello world");
    assert!(state.document().blocks()[0].inlines[0].style.bold);
    assert!(state.undo());
    assert!(!state.document().blocks()[0].inlines[0].style.bold);
    assert!(state.redo());
    assert!(state.document().blocks()[0].inlines[0].style.bold);
    state.set_heading(2).unwrap();
    assert_eq!(
        state.document().blocks()[0].kind,
        DocumentBlockKind::Heading(2)
    );
    assert_eq!(state.document().source().text(), "hello world");
}

#[test]
fn builders_and_rsx_expose_document_and_native_text_editors() {
    let state = DocumentEditorState::new("# title");
    let editor = ui! { <DocumentEditor id="rsx-rich-editor" state={state} viewport_h={300.0} /> };
    assert_eq!(editor.attrs["data-slot"], "document-editor");
    let viewer = ui! { <DocumentView id="rsx-rich-view" text="# title" viewport_h={300.0} /> };
    assert_eq!(viewer.attrs["data-slot"], "document-view");
    fn has_selectable(element: &Element) -> bool {
        element.text_style.selectable || element.children.iter().any(has_selectable)
    }
    assert!(has_selectable(&viewer));
    let plain = ui! { <TextEditor id="rsx-native-editor" rows={4} /> };
    assert_eq!(plain.attrs["data-slot"], "text-editor");
    #[cfg(feature = "document-science")]
    {
        let molecule = Molecule::from_xyz("1\natom\nC 0 0 0\n").unwrap();
        let graphic = ui! { <MoleculeView molecule={molecule} viewport_h={200.0} /> };
        assert_eq!(graphic.attrs["data-slot"], "molecule-view");
    }
}

#[test]
#[cfg(feature = "document-science")]
fn molecular_json_and_xyz_are_typed_graphs() {
    let molecule = Molecule::from_xyz("3\nwater\nO 0 0 0\nH 1 0 0\nH 0 1 0\n").unwrap();
    assert_eq!(
        Molecule::from_json(&molecule.to_json())
            .unwrap()
            .atoms()
            .len(),
        3
    );
    assert!(Molecule::from_xyz("1\ninvalid\nH NaN 0 0").is_err());
}

#[test]
fn height_index_maintains_anchor_across_measurements() {
    let mut heights = DocumentHeightIndex::new([20.0, 80.0, 30.0]);
    let anchor = heights.anchor(35.0);
    heights.measure(0, 50.0);
    assert_eq!(heights.restore(anchor), 65.0);
    assert_eq!(heights.visible(55.0, 10.0, 0), 1..2);
}

#[test]
fn rich_typing_link_and_math_commands_preserve_metadata() {
    let state = DocumentEditorState::new(Document::rich_text([RichTextBlock::paragraph([
        RichTextSpan::new("hello"),
    ])]));
    state.set_selection(DocumentSelection::caret(5)).unwrap();
    state.toggle_mark(DocumentMark::Bold).unwrap();
    state.replace_selection("!").unwrap();
    assert!(
        state.document().blocks()[0]
            .inlines
            .last()
            .unwrap()
            .style
            .bold
    );
    state
        .set_selection(DocumentSelection {
            anchor: 0,
            caret: 5,
        })
        .unwrap();
    state.insert_link("https://example.com").unwrap();
    assert_eq!(state.document().source().text(), "hello!");
    assert_eq!(
        state.document().blocks()[0].inlines[0]
            .style
            .link
            .as_deref(),
        Some("https://example.com")
    );
    state.toggle_mark(DocumentMark::Math).unwrap();
    assert_eq!(
        state.document().blocks()[0].inlines[0].kind,
        DocumentInlineKind::Math
    );
    assert!(state.undo());
    assert_eq!(
        state.document().blocks()[0].inlines[0].kind,
        DocumentInlineKind::Text
    );
}

#[test]
fn huge_unicode_source_uses_safe_sampled_height() {
    let source = "\u{4e2d}\u{6587}\u{1f680}\n".repeat(10_000);
    let estimate = jadix_components::text::textarea_visual_content_height(
        &source,
        16.0,
        400,
        24.0,
        600.0,
        jadix_components::text::WrapMode::Word,
        4.0,
    );
    assert!(estimate.estimated);
    assert!(estimate.content_height.is_finite());
    assert!(estimate.exact_line_count >= 10_001);
}

#[test]
fn inline_code_reserves_its_full_width_next_to_math() {
    let (_owner, ()) = create_root(|| {
        let mut root: Element = document_view("inline-code-math")
            .document("before `inline code`, or $x^2$.")
            .into();
        let mut layout = LayoutTree::new();
        layout.build_and_layout(&mut root, 640.0, 480.0, [-1.0, -1.0]);
        fn code(element: &Element) -> Option<&Element> {
            if element.text.as_deref() == Some("inline code") {
                return Some(element);
            }
            element.children.iter().find_map(code)
        }
        let code = code(&root).unwrap();
        assert!(
            code.layout_bounds[2] > 80.0,
            "code box must reserve glyph width: {:?}",
            code.layout_bounds
        );
    });
}
