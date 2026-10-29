use super::*;

#[test]
fn replaced_editor_cleanup_preserves_new_mount_buffer() {
    let (_owner, ()) = crate::core::reactivity::create_root(|| {
        let state = DocumentEditorState::new("first\n\nsecond");
        let old: Element = document_editor("overlap-mount", state.clone()).into();
        let mut replacement: Element = document_editor("overlap-mount", state.clone()).into();
        let buffer = use_signal("overlap-mount__source_buffer", String::new);
        drop(old);
        assert!(buffer.is_alive());
        state.set_selection(DocumentSelection::caret(8)).unwrap();
        let mut layout = crate::core::tree::LayoutTree::new();
        let mut impact = crate::core::tree::LayoutStatus::Clean;
        crate::app::test_support::sync_reactive_elements_subtree(
            &mut replacement,
            &mut layout,
            &mut impact,
        );
        assert_eq!(buffer.peek(), "second");
        drop(replacement);
        assert!(!buffer.is_alive());
    });
}

fn named_key(named: NamedKey) -> KeyEvent {
    KeyEvent {
        physical_key: winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Enter),
        logical_key: Key::Named(named),
        text: None,
        location: winit::keyboard::KeyLocation::Standard,
        state: ElementState::Pressed,
        repeat: false,
        text_with_all_modifiers: None,
        key_without_modifiers: Key::Named(named),
    }
}

#[test]
fn switching_preview_back_to_live_retains_click_to_edit_on_reused_rows() {
    let (_owner, ()) = crate::core::reactivity::create_root(|| {
        let state = DocumentEditorState::new("**bold**");
        state.set_mode(DocumentEditMode::Preview);
        let mut root: Element = document_editor("mode-switch-test", state.clone()).into();
        let mut layout = crate::core::tree::LayoutTree::new();
        fn sync(root: &mut Element, layout: &mut crate::core::tree::LayoutTree) {
            if let Some(callback) = root.reactive_sync.clone() {
                callback.run(root, layout);
            }
            for child in &mut root.children {
                sync(child, layout);
            }
        }
        fn row(root: &Element) -> Option<&Element> {
            if root.attrs.contains_key("data-document-key") {
                return Some(root);
            }
            root.children.iter().find_map(row)
        }
        sync(&mut root, &mut layout);
        let click = row(&root).unwrap().on_mousedown.clone().unwrap();
        let event = crate::app::pointer_events::MouseEvent::new(
            1.0,
            1.0,
            winit::event::MouseButton::Left,
            1,
        );
        click.borrow_mut()(&event);
        assert!(!state.snapshot().editing);
        state.set_mode(DocumentEditMode::LivePreview);
        sync(&mut root, &mut layout);
        row(&root)
            .unwrap()
            .on_mousedown
            .as_ref()
            .unwrap()
            .borrow_mut()(&event);
        assert!(state.snapshot().editing);
        sync(&mut root, &mut layout);
        drop(root);
        assert!(!use_signal("__jadix_button_focus_id", String::new)
            .peek()
            .contains("mode-switch-test__source"));
    });
}

#[test]
fn reused_preview_click_uses_current_document_offsets_after_earlier_edit() {
    let (_owner, ()) = crate::core::reactivity::create_root(|| {
        let state = DocumentEditorState::new("first\n\n**next**");
        let mut root: Element = document_editor("shifted-click-test", state.clone()).into();
        let mut layout = crate::core::tree::LayoutTree::new();
        let mut impact = crate::core::tree::LayoutStatus::Clean;
        crate::app::test_support::sync_reactive_elements_subtree(
            &mut root,
            &mut layout,
            &mut impact,
        );
        fn last_row(root: &Element) -> Option<&Element> {
            root.children
                .iter()
                .rev()
                .find_map(last_row)
                .or_else(|| root.attrs.contains_key("data-document-key").then_some(root))
        }
        let callback = last_row(&root).unwrap().on_mousedown.clone().unwrap();
        state.replace(0..0, "long ").unwrap();
        crate::app::test_support::sync_reactive_elements_subtree(
            &mut root,
            &mut layout,
            &mut impact,
        );
        let event = crate::app::pointer_events::MouseEvent::new(
            0.0,
            0.0,
            winit::event::MouseButton::Left,
            1,
        );
        callback.borrow_mut()(&event);
        assert!(
            state.selection().caret >= state.document().source().text().find("**next**").unwrap()
        );
    });
}

#[test]
fn native_keys_cross_source_lines_preserve_crlf_and_stop_on_escape() {
    let (_owner, ()) = crate::core::reactivity::create_root(|| {
        let state = DocumentEditorState::new("one\r\n\u{4e2d}\u{6587}\r\nthree");
        state.set_selection(DocumentSelection::caret(5)).unwrap();
        let mut bridge = EditorBridge {
            state: state.clone(),
            field: "doc-key-test".into(),
            value: Signal::new(String::new()),
            active: None,
            next_mount: 0,
        };
        bridge.sync();
        assert!(bridge.key(&named_key(NamedKey::Backspace), ModifiersState::empty()));
        assert_eq!(
            state.document().source().text(),
            "one\u{4e2d}\u{6587}\r\nthree"
        );
        assert!(state.undo());
        bridge.sync();
        assert!(bridge.key(&named_key(NamedKey::Enter), ModifiersState::empty()));
        assert_eq!(
            state.document().source().text(),
            "one\r\n\r\n\u{4e2d}\u{6587}\r\nthree"
        );
        bridge.sync();
        assert!(bridge.key(&named_key(NamedKey::ArrowUp), ModifiersState::empty()));
        assert!(state.selection().caret < 7);
        bridge.sync();
        assert!(bridge.key(&named_key(NamedKey::Escape), ModifiersState::empty()));
        bridge.sync();
        assert!(bridge.active.is_none());
        assert_ne!(
            use_signal("__jadix_button_focus_id", String::new).peek(),
            text_field::textarea_focus_id(&bridge.field)
        );
    });
}
#[test]
fn reveal_uses_complete_table_and_code_blocks() {
    for source in [
        "| A | B |\n| --- | --- |\n| 1 | 2 |\n",
        "```rust\nlet first = 1;\nlet second = 2;\n```\n",
    ] {
        let state = DocumentEditorState::new(format!("before\n\n{source}\nafter"));
        state
            .set_selection(DocumentSelection::caret(8 + source.find('1').unwrap()))
            .unwrap();
        let snapshot = state.snapshot();
        let range = reveal_range(&snapshot).unwrap();
        assert_eq!(
            snapshot.document.source().text()[range].trim_end(),
            source.trim_end()
        );
    }
}

#[test]
fn reveal_is_line_scoped_except_atomic_formula_blocks() {
    let state = DocumentEditorState::new("# title\n\n**bold**\n\n$$\nx^2\n$$\n");
    state.set_selection(DocumentSelection::caret(11)).unwrap();
    assert_eq!(reveal_range(&state.snapshot()), Some(9..17));
    let math = state.document().source().text().find("x^2").unwrap();
    state.set_selection(DocumentSelection::caret(math)).unwrap();
    let snapshot = state.snapshot();
    let range = reveal_range(&snapshot).unwrap();
    assert!(snapshot.document.source().text()[range].starts_with("$$"));
}

#[test]
fn edit_delta_does_not_split_unicode() {
    let (range, insert) = replacement_delta("a\u{4e2d}\u{6587}b", "a\u{4e2d}\u{6587}\u{5b57}b");
    assert_eq!(range, 7..7);
    assert_eq!(insert, "\u{5b57}");
}

#[test]
fn ime_preedit_does_not_parse_or_replace_native_composition() {
    let (_owner, ()) = crate::core::reactivity::create_root(|| {
        let state = DocumentEditorState::new("# title\n\ntext");
        state.set_selection(DocumentSelection::caret(10)).unwrap();
        let mut bridge = EditorBridge {
            state: state.clone(),
            field: "doc-ime-test".into(),
            value: Signal::new(String::new()),
            active: None,
            next_mount: 0,
        };
        bridge.sync();
        let preedit = use_signal(&input_preedit_signal_key(&bridge.field), String::new);
        preedit.set_silent("\u{4e2d}".into());
        text_field::commit_value(&bridge.field, "t\u{4e2d}ext".into());
        use_signal(&input_caret_signal_key(&bridge.field), || None::<usize>).set_silent(Some(4));
        bridge.sync();
        assert_eq!(state.document().source().text(), "# title\n\ntext");
        assert_eq!(preedit.peek(), "\u{4e2d}");
        preedit.set_silent(String::new());
        bridge.sync();
        assert_eq!(state.document().source().text(), "# title\n\nt\u{4e2d}ext");
        assert!(state.undo());
        bridge.sync();
        assert_eq!(bridge.active.as_ref().unwrap().text, "text");
    });
}
