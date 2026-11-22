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
fn selectable_document_links_reach_the_public_callback_once() {
    use jadix_components::app::pointer_events::{dispatch_pointer, hit_path, PointerKind};
    use std::{cell::RefCell, rc::Rc};
    use winit::event::MouseButton;
    let (_owner, ()) = create_root(|| {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let mut root: Element = document_view("document-links")
            .document(Document::markdown(
                "before [guide](https://example.com/guide) after",
            ))
            .selectable(true)
            .on_link({
                let calls = calls.clone();
                move |href| calls.borrow_mut().push(href.to_owned())
            })
            .into();
        LayoutTree::new().build_and_layout(&mut root, 400.0, 200.0, [-1.0; 2]);
        let leaf = find(&root, "data-document-kind", "Paragraph").unwrap();
        assert!(leaf.text_style.selectable);
        let [x, y, _, _] = leaf.layout_bounds;
        let path = hit_path(&root, x + 2.0, y + 2.0);
        assert_eq!(path.last().unwrap().node_id, leaf.layout_node_id());
        let mut event = MouseEvent::new(x + 2.0, y + 2.0, MouseButton::Left, 1);
        event.text_link = Some(TextLinkHit {
            href: "https://example.com/guide".into(),
            span_index: 1,
        });
        dispatch_pointer(&path, &mut event, PointerKind::Up);
        assert_eq!(&*calls.borrow(), &["https://example.com/guide".to_owned()]);
        assert!(event.is_consumed());
        let mut event = MouseEvent::new(x + 2.0, y + 2.0, MouseButton::Left, 1);
        dispatch_pointer(&path, &mut event, PointerKind::Up);
        assert_eq!(
            calls.borrow().len(),
            1,
            "ordinary selection has no link activation"
        );
    });
}

#[test]
fn document_link_hover_ownership_is_per_view_and_clears_on_unmount() {
    use jadix_components::app::pointer_events::{dispatch_pointer, hit_path, PointerKind};
    use std::{cell::RefCell, rc::Rc};
    use winit::event::MouseButton;
    let (_owner, ()) = create_root(|| {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let callback: DocumentLinkHoverHandler = Rc::new({
            let calls = calls.clone();
            move |hover| calls.borrow_mut().push(hover)
        });
        let document = Document::markdown("[guide](https://example.com/guide)");
        let mut first: Element = document_view("first-view")
            .document(document.clone())
            .on_link_hover({
                let callback = callback.clone();
                move |event| callback(event)
            })
            .on_link(|_| panic!("hover must not activate"))
            .into();
        let mut second = ui! {
            <DocumentView id="second-view" document={document} on_link_hover={callback} />
        };
        for view in [&mut first, &mut second] {
            LayoutTree::new().build_and_layout(view, 400.0, 200.0, [-1.0; 2]);
            let leaf = find(view, "data-document-kind", "Paragraph").unwrap();
            let [x, y, _, _] = leaf.layout_bounds;
            let mut path = hit_path(view, x + 2.0, y + 2.0);
            path.last_mut().unwrap().text_link = Some(TextLinkHit {
                href: "https://example.com/guide".into(),
                span_index: 0,
            });
            for _ in 0..2 {
                let mut event = MouseEvent::new(x + 2.0, y + 2.0, MouseButton::Left, 0);
                dispatch_pointer(&path, &mut event, PointerKind::Move);
            }
        }
        assert_eq!(calls.borrow().len(), 2, "one transition per view");
        first.on_unmount.take().unwrap().fire();
        {
            let calls = calls.borrow();
            assert_eq!(
                calls
                    .iter()
                    .map(|event| event.view_id.as_str())
                    .collect::<Vec<_>>(),
                ["first-view", "second-view", "first-view"]
            );
            assert!(calls[0].href.is_some() && calls[1].href.is_some());
            assert!(calls[2].href.is_none());
            assert_eq!(calls[0].position, calls[2].position);
        }
        second.on_unmount.take().unwrap().fire();
        assert_eq!(calls.borrow().last().unwrap().view_id, "second-view");
        assert!(calls.borrow().last().unwrap().href.is_none());
    });
}

#[test]
fn document_inline_styles_share_core_semantics_and_one_wrapping_selection_leaf() {
    use jadix_components::text::{
        document_inline_spans, is_safe_document_destination, parse_markdown_blocks,
        parse_markdown_inline,
    };
    let source = "before **bold** *italic* ~~deleted words across the line~~ `code` [good](HTTPS://example.com) [bad](javascript:run) after";
    let document = Document::markdown(source);
    let paragraph = document
        .blocks()
        .iter()
        .find(|block| block.kind == DocumentBlockKind::Paragraph)
        .unwrap();
    let expected = paragraph.text();
    let core = document_inline_spans(&paragraph.inlines);
    assert_eq!(parse_markdown_inline(source), core);
    let blocks = parse_markdown_blocks(source);
    assert_eq!(
        find_text(&blocks, &expected)
            .unwrap()
            .text_spans
            .as_ref()
            .unwrap(),
        &core
    );
    assert!(core.iter().any(|span| span.strikethrough));
    assert!(core.iter().any(|span| span.weight == Some(700)));
    assert!(core.iter().any(|span| span.italic));
    assert!(core.iter().any(|span| span.monospace));
    assert_eq!(
        core.iter()
            .find(|span| span.text == "good")
            .unwrap()
            .href
            .as_deref(),
        Some("HTTPS://example.com")
    );
    let blocked = core.iter().find(|span| span.text == "bad").unwrap();
    assert!(blocked.href.is_none() && !blocked.underline);
    for target in [
        "#heading",
        "../image.png",
        "https://example.com",
        "HTTP://example.com",
        "MAILTO:a@example.com",
        "DATA:IMAGE/png;base64,AA==",
    ] {
        assert!(is_safe_document_destination(target), "{target}");
    }
    for target in [
        "javascript:run",
        "VbScript:run",
        "data:text/html,body",
        "file:///private",
        "\nhttps://example.com",
        "https://example.com\t",
    ] {
        assert!(!is_safe_document_destination(target), "{target:?}");
    }
    let (_owner, ()) = create_root(|| {
        let mut heights = Vec::new();
        for width in [640.0, 180.0] {
            let mut root: Element = document_view(format!("shared-inline-{width}"))
                .document(document.clone())
                .selectable(true)
                .into();
            let output = LayoutTree::new().build_and_layout(&mut root, width, 600.0, [-1.0; 2]);
            let leaf = find(&root, "data-document-kind", "Paragraph").unwrap();
            assert_eq!(
                leaf.text.as_deref(),
                Some(expected.as_str()),
                "ordinary styled text must remain one selection leaf"
            );
            assert!(leaf.children.is_empty());
            let spans = leaf.text_spans.as_ref().unwrap();
            assert_eq!(spans.len(), core.len());
            for (actual, expected) in spans.iter().zip(&core) {
                assert_eq!(
                    (
                        &actual.text,
                        actual.weight,
                        actual.size,
                        actual.italic,
                        actual.monospace,
                        actual.strikethrough,
                        actual.underline,
                        &actual.href
                    ),
                    (
                        &expected.text,
                        expected.weight,
                        expected.size,
                        expected.italic,
                        expected.monospace,
                        expected.strikethrough,
                        expected.underline,
                        &expected.href
                    )
                );
            }
            assert_eq!(output.text_nodes.len(), 1);
            assert_eq!(output.text_nodes[0].spans.as_ref().unwrap(), spans);
            assert!(leaf.layout_bounds[2] <= width);
            heights.push(leaf.layout_bounds[3]);
        }
        assert!(heights[1] > heights[0], "narrow paragraphs must reflow");
    });
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
fn deferred_document_blocks_inherit_their_original_context() {
    use jadix_components::core::reactivity::{create_context, provide_context, use_context};
    use std::{cell::RefCell, rc::Rc};

    let context = create_context::<&'static str>();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let rendered = seen.clone();
    let (_owner, mut root) = create_root(|| {
        provide_context(context, "document");
        document_view("deferred-context-document")
            .document("paragraph\n\n".repeat(300))
            .viewport_h(220.0)
            .block_renderer(move |_, block| {
                rendered.borrow_mut().push(use_context(context));
                Some(Element::new().text(block.text()).h(24.0))
            })
            .into()
    });
    let (_other, ()) = create_root(|| {
        provide_context(context, "unrelated");
        let mut layout = LayoutTree::new();
        layout.build_and_layout(&mut root, 640.0, 220.0, [-1.0, -1.0]);
        sync(&mut root, &mut layout);
        seen.borrow_mut().clear();
        use_signal(
            &scroll_offset_y_signal_key("deferred-context-document"),
            || 0.0_f32,
        )
        .set_silent(2_500.0);
        sync(&mut root, &mut layout);
        let seen = seen.borrow();
        assert!(
            !seen.is_empty(),
            "scrolling must mount previously unseen blocks"
        );
        assert!(seen.iter().all(|value| *value == Some("document")));
        assert_eq!(use_context(context), Some("unrelated"));
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
fn paste_pipeline_uses_document_selection_and_one_undo() {
    use std::{cell::RefCell, rc::Rc};
    let (_owner, ()) = create_root(|| {
        let original = "before\n\n中文\n\nafter";
        let state = DocumentEditorState::new(original);
        let selection = DocumentSelection {
            anchor: 14,
            caret: 8,
        };
        state.set_selection(selection).unwrap();
        let seen = Rc::new(RefCell::new(Vec::new()));
        let first = seen.clone();
        let second = seen.clone();
        let transforms: Vec<DocumentPasteTransform> = vec![
            Rc::new(move |snapshot, text| {
                assert_eq!(snapshot.selection, selection);
                first.borrow_mut().push(text.clone());
                Some(text.trim().to_uppercase())
            }),
            Rc::new(move |_, text| {
                second.borrow_mut().push(text.clone());
                Some(format!("**{text}**\nnext"))
            }),
        ];
        assert!(state.paste_text(" hello ", &transforms).unwrap());
        assert_eq!(*seen.borrow(), [" hello ", "HELLO"]);
        assert_eq!(
            state.document().source().text(),
            "before\n\n**HELLO**\nnext\n\nafter"
        );
        assert!(state.undo());
        assert_eq!(state.document().source().text(), original);
        assert_eq!(state.selection(), selection);
        assert!(!state.undo());
        assert!(state.redo());
        assert_eq!(state.selected_text(), "");

        // Both authoring surfaces accept the same shared callback type.
        let editor = ui! { <DocumentEditor id="paste-rsx" state={state.clone()} paste_transforms={transforms} /> };
        assert_eq!(editor.attrs["data-slot"], "document-editor");
        let _: Element = document_editor("paste-builder", state)
            .paste_transform(|_, text| Some(text))
            .into();
    });
}

#[test]
fn paste_pipeline_stops_on_consumption_or_state_change() {
    use std::rc::Rc;
    let (_owner, ()) = create_root(|| {
        for change in ["consume", "document", "selection", "mode", "empty"] {
            let state = DocumentEditorState::new("abc");
            state.set_selection(DocumentSelection::caret(1)).unwrap();
            let changed = state.clone();
            let transforms: Vec<DocumentPasteTransform> = vec![
                Rc::new(move |_, text| {
                    match change {
                        "consume" => return None,
                        "document" => changed.replace(0..0, "!").unwrap(),
                        "selection" => changed.set_selection(DocumentSelection::caret(3)).unwrap(),
                        "mode" => changed.set_mode(DocumentEditMode::Preview),
                        "empty" => return Some(String::new()),
                        _ => unreachable!(),
                    }
                    Some(text)
                }),
                Rc::new(|_, text| {
                    assert!(
                        text.is_empty(),
                        "stale/consumed requests must stop before the next stage"
                    );
                    Some(text)
                }),
            ];
            assert!(!state.paste_text("paste", &transforms).unwrap());
            assert_eq!(
                state.document().source().text(),
                if change == "document" { "!abc" } else { "abc" }
            );
        }
        let readonly = DocumentEditorState::new("readonly");
        readonly.set_mode(DocumentEditMode::Preview);
        assert!(!readonly
            .paste_text("paste", &[Rc::new(|_, _| panic!("read-only"))])
            .unwrap());
        let (owner, dead) = create_root(|| DocumentEditorState::new("dead"));
        drop(owner);
        assert!(!dead.paste_text("paste", &[]).unwrap());
    });
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
        let word = "0123456789".repeat(8);
        let mut narrow: Element = document_view("oversized-inline-code")
            .document(format!("`{word}` $x$"))
            .into();
        layout.build_and_layout(&mut narrow, 150.0, 480.0, [-1.0, -1.0]);
        let code = find_text(&narrow, &word).unwrap();
        assert!(code.layout_bounds[2] <= 150.0, "{:?}", code.layout_bounds);
        assert!(code.layout_bounds[3] > 24.0, "long code must wrap");
        assert_eq!(code.text.as_deref(), Some(word.as_str()));
    });
}

fn find_text<'a>(element: &'a Element, text: &str) -> Option<&'a Element> {
    if element.text.as_deref() == Some(text) {
        return Some(element);
    }
    element
        .children
        .iter()
        .find_map(|child| find_text(child, text))
}

#[test]
fn document_tasks_are_controlled_and_resolve_live_block_identity() {
    use std::{cell::RefCell, rc::Rc};
    let (_owner, ()) = create_root(|| {
        let document = use_signal("task-document", || {
            Document::markdown("intro\n\n- [ ] **todo**\n- [x] done\n")
        });
        let requests = Rc::new(RefCell::new(Vec::new()));
        let sink = requests.clone();
        let mut root: Element = document_view("task-view")
            .document_signal(document)
            .on_task_toggle(move |event| sink.borrow_mut().push(event))
            .into();
        let checkbox = find(&root, "aria-label", "todo").unwrap();
        assert_eq!(checkbox.attrs["aria-checked"], "false");
        assert_eq!(checkbox.attrs["aria-readonly"], "false");
        assert!(checkbox.focus_id.is_some());
        assert!(!checkbox.text_style.selectable);
        let click = checkbox.on_click.clone().unwrap();
        let id = document
            .peek()
            .blocks()
            .iter()
            .find(|block| block.text() == "todo")
            .unwrap()
            .id;
        // A retained virtual row must resolve its new source coordinates.
        let changed = document.peek().replace(1..2, "longer").unwrap();
        let block = changed
            .blocks()
            .iter()
            .find(|block| block.id == id)
            .unwrap();
        let expected = DocumentTaskToggle {
            block_id: id,
            revision: changed.revision(),
            source: block.source.clone(),
            checked: true,
        };
        document.set(changed);
        let mut layout = LayoutTree::new();
        sync(&mut root, &mut layout);
        assert!(Rc::ptr_eq(
            &click,
            find(&root, "aria-label", "todo")
                .unwrap()
                .on_click
                .as_ref()
                .unwrap()
        ));
        click.borrow_mut()();
        assert_eq!(*requests.borrow(), vec![expected.clone()]);
        assert!(document.peek().source().text().contains("[ ]"));
        // The application applies the request, creating a fresh checked block.
        let marker = document.peek().source().text()[expected.source.clone()]
            .find("[ ]")
            .unwrap();
        let offset = expected.source.start + marker + 1;
        document.set(document.peek().replace(offset..offset + 1, "x").unwrap());
        sync(&mut root, &mut layout);
        assert_eq!(
            find(&root, "aria-label", "todo").unwrap().attrs["aria-checked"],
            "true"
        );
        click.borrow_mut()();
        assert_eq!(
            requests.borrow().len(),
            1,
            "removed identities must not toggle a new row"
        );
        find(&root, "aria-label", "todo")
            .unwrap()
            .on_click
            .as_ref()
            .unwrap()
            .borrow_mut()();
        assert!(!requests.borrow().last().unwrap().checked);
    });
}

#[test]
fn document_task_rsx_preserves_readonly_and_custom_renderer_contracts() {
    use std::rc::Rc;
    let (_owner, ()) = create_root(|| {
        let readonly: Element = document_view("readonly-tasks")
            .document("- [ ] todo")
            .into();
        let checkbox = find(&readonly, "accesskit_role", "checkbox").unwrap();
        assert!(checkbox.on_click.is_none());
        assert_eq!(checkbox.attrs["aria-readonly"], "true");
        #[cfg(feature = "a11y")]
        {
            let update = jadix::a11y::bridge::tree_update_from_elements(&readonly, None);
            let node = &update
                .nodes
                .iter()
                .find(|(_, node)| node.label() == Some("todo"))
                .unwrap()
                .1;
            assert_eq!(format!("{:?}", node.toggled()), "Some(False)");
            assert!(node.is_read_only());
        }
        let state = DocumentEditorState::new("intro\n\n- [ ] todo");
        let editor: Element = document_editor("editable-tasks", state.clone()).into();
        assert_eq!(
            find(&editor, "accesskit_role", "checkbox").unwrap().attrs["aria-readonly"],
            "false"
        );
        state.set_mode(DocumentEditMode::Preview);
        let preview: Element = document_editor("preview-tasks", state).into();
        assert_eq!(
            find(&preview, "accesskit_role", "checkbox").unwrap().attrs["aria-readonly"],
            "true"
        );
        let handler: DocumentTaskHandler = Rc::new(|_| {});
        let rsx =
            ui! { <DocumentView id="rsx-tasks" text="- [ ] todo" on_task_toggle={handler} /> };
        assert!(find(&rsx, "accesskit_role", "checkbox")
            .unwrap()
            .on_click
            .is_some());
        #[cfg(feature = "a11y")]
        {
            let checked: Element = document_view("checked-tasks")
                .document("- [x] done")
                .on_task_toggle(|_| {})
                .into();
            let update = jadix::a11y::bridge::tree_update_from_elements(&checked, None);
            let node = &update
                .nodes
                .iter()
                .find(|(_, node)| node.label() == Some("done"))
                .unwrap()
                .1;
            assert_eq!(format!("{:?}", node.toggled()), "Some(True)");
            assert!(!node.is_read_only());
        }
        let custom: Element = document_view("custom-tasks")
            .document("- [ ] todo")
            .on_task_toggle(|_| panic!("custom renderer owns its interactions"))
            .block_renderer(|_, _| Some(Element::new().text("custom")))
            .into();
        assert!(find(&custom, "accesskit_role", "checkbox").is_none());
        assert_eq!(texts(&custom), "custom");
    });
}
