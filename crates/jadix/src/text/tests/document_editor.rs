use super::*;

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
