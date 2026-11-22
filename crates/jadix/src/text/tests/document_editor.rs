use super::*;

#[test]
fn deferred_paste_fences_late_results_and_preserves_atomic_rich_history() {
    fn request(state: &DocumentEditorState, text: &str) -> DocumentPasteRequest {
        state
            .defer_paste(DocumentPasteContent::Text(text.into()), &[], &[])
            .unwrap()
    }
    let (owner, state) = crate::core::reactivity::create_root(|| {
        let original = Document::from_html_fragment("<p><b>中ab</b></p><p>cd文</p>").unwrap();
        let state = DocumentEditorState::new(original.clone());
        let selection = DocumentSelection {
            anchor: 8,
            caret: 3,
        };
        state.set_selection(selection).unwrap();
        let first = request(&state, "late");
        let newest = request(&state, "new");
        assert!(!first.is_current());
        assert!(!first.accept().unwrap());
        assert!(newest.is_current());
        assert!(!state.can_undo());
        let fragment = Document::from_html_fragment("<p><i>异步</i></p>").unwrap();
        assert!(newest
            .complete(DocumentPasteContent::RichText(fragment))
            .unwrap());
        assert_eq!(state.document().source().text(), "中异步文");
        assert!(state.document().rich_text_blocks()[0]
            .spans
            .iter()
            .any(|span| span.style.italic));
        assert!(state.undo());
        assert_eq!(
            state.document().rich_text_blocks(),
            original.rich_text_blocks()
        );
        assert_eq!(state.selection(), selection);
        assert!(!state.can_undo());
        assert!(state.redo());
        assert!(state.undo());

        let moved = request(&state, "late selection");
        state.set_selection(DocumentSelection::caret(0)).unwrap();
        assert!(!moved.accept().unwrap());
        let edited = request(&state, "late edit");
        state.replace_selection("x").unwrap();
        assert!(!edited.accept().unwrap());
        let undone = request(&state, "late undo");
        assert!(state.undo());
        assert!(!undone.accept().unwrap());
        let changed = request(&state, "late mode");
        state.set_mode(DocumentEditMode::Preview);
        assert!(!changed.accept().unwrap());
        assert!(state
            .defer_paste(DocumentPasteContent::Text("read only".into()), &[], &[])
            .is_none());
        state.set_mode(DocumentEditMode::LivePreview);
        let cancelled = request(&state, "cancelled");
        state.cancel_pending_paste();
        assert!(!cancelled.accept().unwrap());
        let superseded = request(&state, "old");
        // A newer consumed request still fences the old one.
        drop(request(&state, "consumed"));
        assert!(!superseded.accept().unwrap());
        let superseded = request(&state, "old");
        assert!(!state.paste_text("", &[]).unwrap());
        assert!(!superseded.accept().unwrap());
        let nested_state = state.clone();
        let transform: DocumentPasteTransform = Rc::new(move |_, text| {
            drop(request(&nested_state, "new pending"));
            Some(text)
        });
        assert!(!state.paste_text("stale stage", &[transform]).unwrap());
        assert_eq!(state.document().source().text(), original.source().text());
        state
    });
    let disposed = request(&state, "late disposal");
    owner.dispose();
    assert!(!disposed.is_current());
    assert!(!disposed.accept().unwrap());
    let dropped = {
        let state = DocumentEditorState::new("");
        request(&state, "late session")
    };
    assert!(!dropped.accept().unwrap());
}

#[test]
fn global_copy_cut_preserves_formats_history_and_rejects_failed_or_stale_writes() {
    let original = Document::from_html_fragment("<p><b>中ab</b></p><p>cd文</p>").unwrap();
    let state = DocumentEditorState::new(original.clone());
    let selection = DocumentSelection {
        anchor: 8,
        caret: 3,
    };
    state.set_selection(selection).unwrap();
    let epoch = state.snapshot().epoch;
    assert!(state
        .transfer_selection(false, |text, html| {
            assert_eq!(text, "ab\ncd");
            let exported = Document::from_html_fragment(html.unwrap()).unwrap();
            assert_eq!(exported.source().text(), text);
            assert!(exported.rich_text_blocks()[0].spans[0].style.bold);
            true
        })
        .unwrap());
    assert_eq!(state.snapshot().epoch, epoch);
    assert!(!state.can_undo());
    assert!(!state.transfer_selection(true, |_, _| false).unwrap());
    assert_eq!(state.selection(), selection);
    assert_eq!(state.document().source().text(), original.source().text());
    assert!(state
        .transfer_selection(true, |text, html| {
            assert_eq!(text, "ab\ncd");
            assert!(html.is_some());
            true
        })
        .unwrap());
    assert_eq!(state.document().source().text(), "中文");
    assert!(state.undo());
    assert_eq!(
        state.document().rich_text_blocks(),
        original.rich_text_blocks()
    );
    assert_eq!(state.selection(), selection);
    assert!(!state.can_undo());
    assert!(state.redo());
    assert_eq!(state.document().source().text(), "中文");
    assert!(state.undo());
    let stale = state.clone();
    assert!(!state
        .transfer_selection(true, |_, _| {
            stale.set_selection(DocumentSelection::caret(0)).unwrap();
            true
        })
        .unwrap());
    assert_eq!(
        state.document().rich_text_blocks(),
        original.rich_text_blocks()
    );
    assert!(!state
        .transfer_selection(false, |_, _| panic!("empty selection"))
        .unwrap());
    state.set_selection(selection).unwrap();
    state.set_mode(DocumentEditMode::Preview);
    assert!(!state
        .transfer_selection(true, |_, _| panic!("read only cut"))
        .unwrap());
    assert!(state
        .transfer_selection(false, |_, html| html.is_some())
        .unwrap());
    state.set_mode(DocumentEditMode::Source);
    assert!(state
        .transfer_selection(false, |_, html| html.is_none())
        .unwrap());
    let plain = DocumentEditorState::new("one\ntwo");
    plain
        .set_selection(DocumentSelection {
            anchor: 0,
            caret: 7,
        })
        .unwrap();
    assert!(plain
        .transfer_selection(false, |text, html| text == "one\ntwo" && html.is_none())
        .unwrap());
}

#[test]
fn structured_paste_is_atomic_undoable_and_rejects_stale_plugin_results() {
    let original = Document::rich_text([super::super::RichTextBlock::paragraph([
        super::super::RichTextSpan::new("before 中 after"),
    ])]);
    let state = DocumentEditorState::new(original.clone());
    state
        .set_selection(DocumentSelection {
            anchor: 7,
            caret: 10,
        })
        .unwrap();
    let before = state.selection();
    let fragment =
        Document::from_html_fragment("<b>你好</b><a href='https://example.test'>link</a>").unwrap();
    let stages: Vec<DocumentFragmentTransform> = vec![Rc::new(|snapshot, fragment| {
        assert_eq!(snapshot.selection.range(), 7..10);
        assert!(fragment.blocks()[0].inlines[0].style.bold);
        Some(fragment)
    })];
    assert!(state.paste_document(fragment.clone(), &stages).unwrap());
    assert_eq!(state.document().source().text(), "before 你好link after");
    let pasted = state.document().rich_text_blocks();
    assert!(pasted[0]
        .spans
        .iter()
        .any(|span| span.style.link.as_deref() == Some("https://example.test")));
    assert!(state.undo());
    assert_eq!(
        state.document().rich_text_blocks(),
        original.rich_text_blocks()
    );
    assert_eq!(state.selection(), before);
    assert!(!state.can_undo());
    assert!(state.redo());
    assert_eq!(state.document().rich_text_blocks(), pasted);
    let stale = state.clone();
    assert!(!state
        .paste_document(
            fragment.clone(),
            &[Rc::new(move |_, fragment| {
                stale.set_selection(DocumentSelection::caret(0)).unwrap();
                Some(fragment)
            })]
        )
        .unwrap());
    assert_eq!(state.document().rich_text_blocks(), pasted);
    assert!(!state
        .paste_document(fragment.clone(), &[Rc::new(|_, _| None)])
        .unwrap());
    state.set_mode(DocumentEditMode::Preview);
    assert!(!state
        .paste_document(fragment, &[Rc::new(|_, _| panic!("read-only"))])
        .unwrap());
}

#[test]
fn unicode_selection_edit_and_history_use_document_bytes() {
    let state = DocumentEditorState::new("one\n\u{4e2d}\u{6587}\nthree");
    state
        .set_selection(DocumentSelection {
            anchor: 4,
            caret: 10,
        })
        .unwrap();
    assert_eq!(state.selected_text(), "\u{4e2d}\u{6587}");
    state.replace_selection("two").unwrap();
    assert_eq!(state.document().source().text(), "one\ntwo\nthree");
    assert!(state.undo());
    assert_eq!(state.selected_text(), "\u{4e2d}\u{6587}");
    assert!(state.redo());
    assert_eq!(state.selection(), DocumentSelection::caret(7));
    assert!(state.set_selection(DocumentSelection::caret(99)).is_err());
}

#[test]
fn formatting_and_find_are_undoable() {
    let state = DocumentEditorState::new("hello world\n");
    state
        .set_selection(DocumentSelection {
            anchor: 0,
            caret: 5,
        })
        .unwrap();
    state.toggle_mark(DocumentMark::Bold).unwrap();
    assert_eq!(state.document().source().text(), "**hello** world\n");
    state.toggle_mark(DocumentMark::Bold).unwrap();
    assert_eq!(state.document().source().text(), "hello world\n");
    state.set_heading(2).unwrap();
    assert!(state.document().source().text().starts_with("## "));
    assert_eq!(state.find_next("world", false, true), Some(9..14));
    assert_eq!(state.replace_all("world", "earth").unwrap(), 1);
    assert!(state.undo());
    assert!(state.document().source().text().contains("world"));
}

#[test]
fn markdown_list_toggle_preserves_selection_line_endings_and_history() {
    let state = DocumentEditorState::new("  alpha\r\n  中文\r\n");
    state
        .set_selection(DocumentSelection {
            anchor: 2,
            caret: 17,
        })
        .unwrap();

    state.toggle_list(DocumentListKind::Ordered).unwrap();
    assert_eq!(
        state.document().source().text(),
        "  1. alpha\r\n  2. 中文\r\n"
    );
    assert_eq!(state.selected_text(), "alpha\r\n  2. 中文");

    state.toggle_list(DocumentListKind::Ordered).unwrap();
    assert_eq!(state.document().source().text(), "  alpha\r\n  中文\r\n");
    assert!(state.undo());
    assert_eq!(
        state.document().source().text(),
        "  1. alpha\r\n  2. 中文\r\n"
    );
}

#[test]
fn markdown_list_toggle_converts_selected_line_and_ignores_rich_text() {
    let state = DocumentEditorState::new("first\n- second\nthird");
    state
        .set_selection(DocumentSelection {
            anchor: 8,
            caret: 14,
        })
        .unwrap();
    state.toggle_list(DocumentListKind::Ordered).unwrap();
    assert_eq!(state.document().source().text(), "first\n1. second\nthird");
    assert_eq!(state.selected_text(), "second");

    let rich = Document::new(
        crate::text::TextDocument::new("content"),
        DocumentFormat::RichText,
    );
    let rich = DocumentEditorState::new(rich);
    rich.toggle_list(DocumentListKind::Bullet).unwrap();
    assert_eq!(rich.document().source().text(), "content");
}

#[test]
fn markdown_enter_continues_lists_and_exits_empty_items() {
    for (source, caret, expected) in [
        ("  * before after", 12, "  * before a\n  * fter"),
        ("  * ", 4, "  \n"),
        ("3) before", 9, "3) before\n4) "),
        ("- [x] ", 6, "\n"),
    ] {
        let state = DocumentEditorState::new(source);
        state
            .set_selection(DocumentSelection::caret(caret))
            .unwrap();
        state.insert_newline().unwrap();
        assert_eq!(state.document().source().text(), expected);
    }

    let checked_task = DocumentEditorState::new("- [x] done");
    checked_task
        .set_selection(DocumentSelection::caret(10))
        .unwrap();
    checked_task.insert_newline().unwrap();
    assert_eq!(
        checked_task.document().source().text(),
        "- [x] done\n- [ ] "
    );

    let crlf = DocumentEditorState::new("- item\r\n");
    crlf.set_selection(DocumentSelection::caret(6)).unwrap();
    crlf.insert_newline().unwrap();
    assert_eq!(crlf.document().source().text(), "- item\r\n- \r\n");

    let shifted = DocumentEditorState::new("- item");
    shifted.set_selection(DocumentSelection::caret(6)).unwrap();
    shifted
        .insert_newline_with_list_continuation(false)
        .unwrap();
    assert_eq!(shifted.document().source().text(), "- item\n");
}

#[test]
fn history_is_bounded_and_redo_is_cleared_after_branching() {
    let state = DocumentEditorState::with_history_limit("", 2, 8);
    for text in ["a", "b", "c"] {
        state.replace_selection(text).unwrap();
    }
    assert!(state.history_bytes() <= 8);
    assert!(state.undo());
    assert!(state.undo());
    assert!(!state.undo());
    assert_eq!(state.document().source().text(), "a");
    state.replace_selection("z").unwrap();
    assert!(!state.redo());
    state.replace(0..2, "longer than budget").unwrap();
    assert!(!state.can_undo());
}
