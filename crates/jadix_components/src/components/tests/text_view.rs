use super::*;

fn node_count(element: &Element) -> usize {
    1 + element.children.iter().map(node_count).sum::<usize>()
}

#[test]
fn text_view_builder_materializes_only_viewport_rows() {
    let document = TextDocument::new("long document\n".repeat(100_000));
    let view: Element = text_view("text-view-large")
        .document(document)
        .line_numbers(true)
        .selectable(false)
        .viewport_h(220.0)
        .row_height(22.0)
        .overscan(2)
        .class("rounded-md")
        .into();
    assert_eq!(
        view.attrs.get("data-slot").map(String::as_str),
        Some("text-view")
    );
    assert!(node_count(&view) < 100);
    assert!(view.text_style.input_caret.is_none());
}

#[test]
fn source_rows_keep_empty_lines_and_selectable_source_separate_from_gutter() {
    let document = TextDocument::new("first\n\nlast");
    let row = source_line("row", &document, Some(1), true, true, 22.0);
    assert_eq!(row.children[0].text.as_deref(), Some("2"));
    assert!(!row.children[0].text_style.selectable);
    assert_eq!(row.children[1].text.as_deref(), Some(""));
    assert!(row.children[1].text_style.selectable);
    assert_eq!(
        source_line("gap", &document, None, true, true, 22.0)
            .children
            .len(),
        1
    );
}

#[test]
fn explicit_cursor_applies_to_viewport_and_selectable_text_rows() {
    use crate::style::CursorKind;

    let document = TextDocument::new("source");
    let view: Element = text_view("text-view-cursor")
        .document(document.clone())
        .cursor(CursorKind::Pointer)
        .into();
    let pointer = crate::core::tree::CursorValue::from(CursorKind::Pointer);
    assert_eq!(view.cursor, Some(pointer.clone()));

    let row = source_line_with_cursor(
        "row",
        &document,
        Some(0),
        false,
        true,
        22.0,
        Some(pointer.clone()),
    );
    assert_eq!(row.children[0].cursor, Some(pointer));
}

#[test]
fn snapshot_updates_and_stale_scroll_are_safe_without_root_rebuild() {
    use crate::app::test_support::sync_reactive_elements_subtree;
    use crate::core::reactivity::{create_root, use_signal};
    use crate::core::scroll_viewport::scroll_offset_y_signal_key;
    use crate::core::tree::{LayoutStatus, LayoutTree};
    use crate::utils::text_annotations::TextRangeStyle;

    let (_owner, ()) = create_root(|| {
        let signal = use_signal("text-view-revisions", || TextDocument::new("old\nold"));
        let mut view: Element = text_view("text-view-live")
            .document_signal(signal)
            .highlights(vec![TextDecoration {
                range: 0..3,
                value: TextRangeStyle {
                    background: Some([80, 90, 100, 255]),
                    ..Default::default()
                },
            }])
            .into();
        fn source<'a>(element: &'a Element, text: &str) -> Option<&'a Element> {
            if element.text.as_deref() == Some(text) {
                Some(element)
            } else {
                element
                    .children
                    .iter()
                    .find_map(|child| source(child, text))
            }
        }
        assert_eq!(
            source(&view, "old").unwrap().text_spans.as_ref().unwrap()[0].background,
            Some([80, 90, 100, 255])
        );
        let mut layout = LayoutTree::new();
        let mut impact = LayoutStatus::Clean;
        signal.set_silent(TextDocument::new("new\nnew"));
        sync_reactive_elements_subtree(&mut view, &mut layout, &mut impact);
        assert_eq!(impact, LayoutStatus::StructureChanged);
        fn contains(element: &Element, text: &str) -> bool {
            element.text.as_deref() == Some(text)
                || element.children.iter().any(|child| contains(child, text))
        }
        assert!(contains(&view, "new"));
        assert!(!contains(&view, "old"));
        assert!(source(&view, "new").unwrap().text_spans.is_none());
        use_signal(&scroll_offset_y_signal_key("text-view-live"), || 0.0_f32)
            .set_silent(2_000_000.0);
        signal.set_silent(TextDocument::new("short"));
        sync_reactive_elements_subtree(&mut view, &mut layout, &mut impact);
        use_signal(&scroll_offset_y_signal_key("text-view-live"), || 0.0_f32).set_silent(0.0);
        sync_reactive_elements_subtree(&mut view, &mut layout, &mut impact);
        assert!(contains(&view, "short"));
        let mut unchanged = LayoutStatus::Clean;
        sync_reactive_elements_subtree(&mut view, &mut layout, &mut unchanged);
        assert_eq!(unchanged, LayoutStatus::Clean);
    });
}

#[cfg(feature = "syntax-highlighting")]
#[test]
fn syntax_survives_virtual_scroll_and_refreshes_theme_and_snapshot() {
    use crate::app::test_support::sync_reactive_elements_subtree;
    use crate::core::scroll_viewport::scroll_offset_y_signal_key;
    use crate::core::{
        reactivity::{create_root, use_signal},
        tree::{LayoutStatus, LayoutTree},
    };
    use crate::utils::text_annotations::TextRangeStyle;

    fn leaf(element: &Element) -> Option<&Element> {
        if element.text.as_deref() == Some("let value = 1;") {
            Some(element)
        } else {
            element.children.iter().find_map(leaf)
        }
    }
    fn sync_until(view: &mut Element, layout: &mut LayoutTree, ready: impl Fn(&Element) -> bool) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let mut impact = LayoutStatus::Clean;
            sync_reactive_elements_subtree(view, layout, &mut impact);
            if ready(view) {
                return;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "background syntax was not published"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
    let (_owner, ()) = create_root(|| {
        let source = format!("/*\n{}let value = 1;\n*/", "comment\n".repeat(100));
        let document = TextDocument::new(source);
        let range = document.line_range(101).unwrap();
        let signal = use_signal("syntax-view-source", || document);
        let explicit = [11, 22, 33, 255];
        let mut view: Element = text_view("syntax-view")
            .document_signal(signal)
            .language("rs")
            .highlights(vec![TextDecoration {
                range,
                value: TextRangeStyle {
                    background: Some(explicit),
                    ..Default::default()
                },
            }])
            .viewport_h(44.0)
            .overscan(0)
            .into();
        assert!(
            leaf(&view).is_none(),
            "opening comment is outside the target viewport"
        );
        use_signal(&scroll_offset_y_signal_key("syntax-view"), || 0.0_f32).set_silent(101.0 * 22.0);
        let mut layout = LayoutTree::new();
        sync_until(&mut view, &mut layout, |view| {
            leaf(view)
                .and_then(|leaf| leaf.text_spans.as_ref())
                .is_some_and(|spans| spans[0].color.is_some())
        });
        let spans = leaf(&view).unwrap().text_spans.as_ref().unwrap();
        let comment_color = spans[0].color.unwrap();
        assert!(
            spans
                .iter()
                .all(|s| s.color == Some(comment_color) && s.background == Some(explicit))
        );
        assert!(node_count(&view) < 60);

        let saved_theme = crate::style::get_theme();
        let next_theme = if crate::style::is_dark_theme() {
            crate::style::Theme::light()
        } else {
            crate::style::Theme::dark()
        };
        crate::style::set_theme(next_theme);
        sync_until(&mut view, &mut layout, |view| {
            leaf(view)
                .and_then(|leaf| leaf.text_spans.as_ref())
                .is_some_and(|spans| {
                    spans[0].color.is_some() && spans[0].color != Some(comment_color)
                })
        });
        assert_ne!(
            leaf(&view).unwrap().text_spans.as_ref().unwrap()[0].color,
            Some(comment_color)
        );
        crate::style::set_theme(saved_theme);

        signal.set_silent(TextDocument::new(format!(
            "{}let value = 1;",
            "\n".repeat(101)
        )));
        sync_until(&mut view, &mut layout, |view| {
            leaf(view)
                .and_then(|leaf| leaf.text_spans.as_ref())
                .is_some_and(|spans| {
                    spans[0].color.is_some() && spans[0].color != Some(comment_color)
                })
        });
        let spans = leaf(&view).unwrap().text_spans.as_ref().unwrap();
        assert_ne!(
            spans[0].color,
            Some(comment_color),
            "new snapshot starts outside a comment"
        );
        assert!(
            spans.iter().all(|s| s.background.is_none()),
            "stale explicit styles are discarded"
        );
        let mut unchanged = LayoutStatus::Clean;
        sync_reactive_elements_subtree(&mut view, &mut layout, &mut unchanged);
        assert_eq!(unchanged, LayoutStatus::Clean);
    });
}
