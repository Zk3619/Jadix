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
