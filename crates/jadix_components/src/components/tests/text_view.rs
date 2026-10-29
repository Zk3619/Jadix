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
fn snapshot_updates_and_stale_scroll_are_safe_without_root_rebuild() {
    use crate::app::test_support::sync_reactive_elements_subtree;
    use crate::core::reactivity::{create_root, use_signal};
    use crate::core::scroll_viewport::scroll_offset_y_signal_key;
    use crate::core::tree::{LayoutStatus, LayoutTree};

    let (_owner, ()) = create_root(|| {
        let signal = use_signal("text-view-revisions", || TextDocument::new("old\nold"));
        let mut view: Element = text_view("text-view-live").document_signal(signal).into();
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
