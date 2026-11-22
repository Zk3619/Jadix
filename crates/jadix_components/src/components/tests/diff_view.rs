use super::*;

#[test]
fn file_headers_stay_in_viewport_when_long_code_scrolls() {
    use crate::app::test_support::sync_reactive_elements_subtree;
    use crate::core::reactivity::create_root;
    use crate::core::tree::LayoutTree;

    let (_owner, ()) = create_root(|| {
        for mode in [DiffMode::Unified, DiffMode::Split] {
            let id = format!("fixed-file-header-{mode:?}");
            let mut view: Element =
                diff_view(&id, DiffDocument::new("old".repeat(500), "new".repeat(500)))
                    .mode(mode)
                    .file_names("original/".repeat(50), "modified/".repeat(50))
                    .viewport_h(180.0)
                    .into();
            let viewport = use_signal(&scroll_viewport_w_signal_key(&id), || 360.0_f32);
            let offset = use_signal(&scroll_offset_x_signal_key(&id), || 0.0_f32);
            let mut tree = LayoutTree::new();
            for width in [360.0, 240.0] {
                viewport.set_silent(width);
                let mut impact = LayoutStatus::Clean;
                sync_reactive_elements_subtree(&mut view, &mut tree, &mut impact);
                let result = tree.build_and_layout(&mut view, width, 202.0, [-1.0; 2]);
                assert!(
                    result
                        .scroll_metrics
                        .iter()
                        .any(|(_, _, max, _)| max[0] > 500.0)
                );
                let header = &view.children[0];
                assert!(header.attrs.contains_key("data-diff-file-header"));
                let bounds = header.layout_bounds;
                assert!((bounds[2] - width).abs() < 0.1, "{bounds:?}");
                for label in &header.children {
                    assert!(label.layout_bounds[2] <= width / 2.0 + 0.1);
                }
                offset.set_silent(500.0);
                sync_reactive_elements_subtree(&mut view, &mut tree, &mut impact);
                assert_eq!(view.children[0].layout_bounds, bounds);
                assert_eq!(view.children[0].style.translate, [0.0, 0.0]);
                offset.set_silent(0.0);
            }
        }
    });
}

fn split_rows(element: &Element) -> Vec<&Element> {
    if element.attrs.contains_key("data-diff-split-viewport") {
        return vec![element];
    }
    element.children.iter().flat_map(split_rows).collect()
}

#[test]
fn automatic_inline_highlights_refresh_and_allow_explicit_overrides() {
    use crate::app::test_support::sync_reactive_elements_subtree;
    use crate::core::{reactivity::create_root, tree::LayoutTree};

    fn leaf<'a>(element: &'a Element, text: &str) -> Option<&'a Element> {
        if element.text.as_deref() == Some(text) {
            Some(element)
        } else {
            element.children.iter().find_map(|child| leaf(child, text))
        }
    }
    let (_owner, ()) = create_root(|| {
        for mode in [DiffMode::Unified, DiffMode::Split] {
            let id = format!("automatic-inline-{mode:?}");
            let document = DiffDocument::new("\t旧🙂\r\n", "\t新🙂\r\n");
            let signal = use_signal(&id, || document.clone());
            let explicit_color = [123, 45, 67, 255];
            let mut view: Element = diff_view(&id, document)
                .document_signal(signal)
                .mode(mode)
                .highlights(
                    vec![TextDecoration {
                        range: 1..4,
                        value: TextRangeStyle {
                            background: Some(explicit_color),
                            underline: true,
                            ..Default::default()
                        },
                    }],
                    Vec::new(),
                )
                .viewport_h(180.0)
                .into();
            let mut layout = LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            sync_reactive_elements_subtree(&mut view, &mut layout, &mut impact);
            for source in ["\t旧🙂", "\t新🙂"] {
                let text = leaf(&view, source).unwrap();
                assert!(text.text_style.selectable && text.focus_id.is_some());
                let spans = text.text_spans.as_ref().unwrap();
                assert_eq!(
                    spans.iter().map(|s| s.text.as_str()).collect::<String>(),
                    source
                );
                assert_eq!(spans[0].text, "\t");
                assert_eq!(spans[0].background, None);
                assert_eq!(spans[1].text, &source[1..4]);
                assert!(spans[1].background.is_some());
                assert_eq!(spans[2].text, "🙂");
                assert_eq!(spans[2].background, None);
                if source == "\t旧🙂" {
                    assert_eq!(spans[1].background, Some(explicit_color));
                    assert!(spans[1].underline);
                }
            }
            let before = leaf(&view, "\t新🙂").unwrap().text_spans.as_ref().unwrap()[1].background;
            let saved_theme = crate::style::get_theme();
            crate::style::set_theme(if crate::style::is_dark_theme() {
                crate::style::Theme::light()
            } else {
                crate::style::Theme::dark()
            });
            sync_reactive_elements_subtree(&mut view, &mut layout, &mut impact);
            assert_ne!(
                leaf(&view, "\t新🙂").unwrap().text_spans.as_ref().unwrap()[1].background,
                before
            );
            assert_eq!(
                leaf(&view, "\t旧🙂").unwrap().text_spans.as_ref().unwrap()[1].background,
                Some(explicit_color)
            );
            crate::style::set_theme(saved_theme);
            signal.set(DiffDocument::new("\t替🙂\r\n", "\t换🙂\r\n"));
            sync_reactive_elements_subtree(&mut view, &mut layout, &mut impact);
            for source in ["\t替🙂", "\t换🙂"] {
                let spans = leaf(&view, source).unwrap().text_spans.as_ref().unwrap();
                let color = spans[1].background.expect("new automatic range");
                assert_ne!(color, explicit_color, "discard stale explicit ranges");
                assert!(!spans[1].underline);
                assert_eq!(spans[1].text, &source[1..4]);
            }
        }
    });
}

#[cfg(feature = "syntax-highlighting")]
#[test]
fn syntax_preserves_each_side_context_and_composes_inline_and_explicit_styles() {
    fn leaf<'a>(element: &'a Element, text: &str) -> Option<&'a Element> {
        if element.text.as_deref() == Some(text) {
            Some(element)
        } else {
            element.children.iter().find_map(|child| leaf(child, text))
        }
    }
    for mode in [DiffMode::Unified, DiffMode::Split] {
        // The same keyword is a comment on the old side and code on the new side.
        let document = DiffDocument::new("/*\nlet value = 1;\n*/", "//\nlet value = 2;\n//");
        let start = document.modified().line_range(1).unwrap().start;
        let explicit = [11, 22, 33, 255];
        let mut view: Element = diff_view(format!("syntax-diff-{mode:?}"), document)
            .mode(mode)
            .language("rs")
            .highlights(
                Vec::new(),
                vec![TextDecoration {
                    range: start..start + 3,
                    value: TextRangeStyle {
                        color: Some(explicit),
                        underline: true,
                        ..Default::default()
                    },
                }],
            )
            .into();
        let mut layout = crate::core::tree::LayoutTree::new();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let mut impact = LayoutStatus::Clean;
            crate::app::test_support::sync_reactive_elements_subtree(
                &mut view,
                &mut layout,
                &mut impact,
            );
            if ["let value = 1;", "let value = 2;"].iter().all(|source| {
                leaf(&view, source)
                    .and_then(|line| line.text_spans.as_ref())
                    .is_some_and(|spans| spans.iter().all(|span| span.color.is_some()))
            }) {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "background syntax did not arrive"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let old = leaf(&view, "let value = 1;")
            .unwrap()
            .text_spans
            .as_ref()
            .unwrap();
        let new = leaf(&view, "let value = 2;")
            .unwrap()
            .text_spans
            .as_ref()
            .unwrap();
        assert!(
            old.iter().all(|s| s.color == old[0].color),
            "preserve old multiline comment"
        );
        assert_eq!(new[0].text, "let");
        assert_eq!(new[0].color, Some(explicit));
        assert!(new[0].underline);
        assert_ne!(old[0].color, new[0].color);
        for spans in [old, new] {
            assert!(spans.iter().all(|s| s.color.is_some()));
            assert!(
                spans.iter().any(|s| s.background.is_some()),
                "inline changes retain their backgrounds"
            );
        }
    }
}

#[test]
fn split_long_lines_keep_both_panes_visible_and_scroll_without_relayout() {
    use crate::app::test_support::sync_reactive_elements_subtree;
    use crate::core::reactivity::create_root;
    use crate::core::tree::LayoutTree;

    let (_owner, ()) = create_root(|| {
        let id = "split-long-lines";
        let original = format!("{}\n", "old ".repeat(200)).repeat(2_000);
        let modified = original.replace("old", "new");
        let mut view: Element = diff_view(id, DiffDocument::new(original, modified))
            .mode(DiffMode::Split)
            .viewport_h(220.0)
            .into();
        let viewport = use_signal(&scroll_viewport_w_signal_key(id), || 0.0_f32);
        let offset = use_signal(&scroll_offset_x_signal_key(id), || 0.0_f32);
        let mut layout = LayoutTree::new();
        for width in [600.0, 360.0] {
            viewport.set_silent(width);
            let mut impact = LayoutStatus::Clean;
            sync_reactive_elements_subtree(&mut view, &mut layout, &mut impact);
            let result = layout.build_and_layout(&mut view, width, 220.0, [-1.0; 2]);
            assert!(
                result
                    .scroll_metrics
                    .iter()
                    .any(|(_, _, max_scroll, _)| max_scroll[0] > 500.0)
            );
            let rows = split_rows(&view);
            assert!(
                !rows.is_empty() && rows.len() < 30,
                "keep vertical virtualization"
            );
            let node = rows[0].layout_node;
            let left = rows[0].children[0].layout_bounds;
            let right = rows[0].children[1].layout_bounds;
            assert!((left[2] - width / 2.0).abs() < 0.1, "{left:?}");
            assert!((right[0] - left[0] - width / 2.0).abs() < 0.1, "{right:?}");
            assert!(right[0] + right[2] <= width + 0.1);

            offset.set_silent(180.0);
            let mut impact = LayoutStatus::Clean;
            sync_reactive_elements_subtree(&mut view, &mut layout, &mut impact);
            assert_eq!(
                impact,
                LayoutStatus::StyleChanged,
                "horizontal scroll is paint-only"
            );
            let rows = split_rows(&view);
            assert_eq!(rows[0].layout_node, node, "reuse the mounted row");
            assert_eq!(rows[0].style.translate[0], 180.0);
            for pane in &rows[0].children {
                assert_eq!(pane.children[0].style.translate[0], 0.0, "fixed sign");
                assert_eq!(pane.children[1].style.translate[0], 0.0, "fixed number");
                let clip = &pane.children[2];
                assert!(clip.layout_bounds[2] < width / 2.0);
                assert_eq!(clip.children[0].style.translate[0], -180.0);
            }
            offset.set_silent(0.0);
            sync_reactive_elements_subtree(&mut view, &mut layout, &mut impact);
        }
    });
}

#[test]
fn diff_view_builder_has_one_shared_scrollport_in_split_mode() {
    let document = DiffDocument::new("old\n", "new\n");
    let view: Element = diff_view("diff-split-test", document.clone())
        .mode(DiffMode::Split)
        .viewport_h(220.0)
        .selectable(false)
        .into();
    assert_eq!(
        view.attrs.get("data-mode").map(String::as_str),
        Some("split")
    );
    assert_eq!(
        view.attrs.get("data-slot").map(String::as_str),
        Some("diff-view")
    );
    let row = diff_row(
        "row",
        &document,
        document.rows(DiffMode::Split)[0],
        DiffMode::Split,
        true,
        22.0,
        &(
            DocumentHighlights::new(document.original(), Vec::new()),
            DocumentHighlights::new(document.modified(), Vec::new()),
        ),
    );
    assert_eq!(row.children.len(), 2);
    assert_eq!(
        row.children[0].attrs.get("data-change").map(String::as_str),
        Some("delete")
    );
    assert_eq!(
        row.children[1].attrs.get("data-change").map(String::as_str),
        Some("insert")
    );
}

#[test]
fn diff_source_highlights_keep_side_coordinates_selection_and_snapshot_identity() {
    use crate::app::test_support::sync_reactive_elements_subtree;
    use crate::core::{reactivity::create_root, tree::LayoutTree};
    use crate::utils::text_annotations::TextRangeStyle;

    fn leaf<'a>(element: &'a Element, text: &str) -> Option<&'a Element> {
        if element.text.as_deref() == Some(text) {
            Some(element)
        } else {
            element.children.iter().find_map(|child| leaf(child, text))
        }
    }
    let (_owner, ()) = create_root(|| {
        for mode in [DiffMode::Unified, DiffMode::Split] {
            let id = format!("source-highlights-{mode:?}");
            let old = "\t旧🙂";
            let new = "\t新🙂";
            let document = DiffDocument::new(
                format!("{old}\r\n{}", "same\n".repeat(2_000)),
                format!("{new}\r\n{}", "same\n".repeat(2_000)),
            );
            let signal = use_signal(&id, || document.clone());
            let decoration = |color| {
                vec![TextDecoration {
                    range: 2..8,
                    value: TextRangeStyle {
                        background: Some(color),
                        ..Default::default()
                    },
                }]
            };
            let mut view: Element = diff_view(&id, document)
                .document_signal(signal)
                .mode(mode)
                .inline_changes(false)
                .highlights(decoration([255, 0, 0, 255]), decoration([0, 0, 255, 255]))
                .viewport_h(180.0)
                .into();
            let mut layout = LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            sync_reactive_elements_subtree(&mut view, &mut layout, &mut impact);
            layout.build_and_layout(&mut view, 600.0, 180.0, [-1.0; 2]);
            for (source, color) in [(old, [255, 0, 0, 255]), (new, [0, 0, 255, 255])] {
                let text = leaf(&view, source).expect("visible source line");
                assert!(text.text_style.selectable && text.focus_id.is_some());
                let spans = text.text_spans.as_ref().unwrap();
                assert_eq!(spans[0].text, "\t");
                assert_eq!(spans[0].background, None);
                assert_eq!(spans[1].text, &source[1..]);
                assert_eq!(spans[1].background, Some(color));
            }
            if mode == DiffMode::Split {
                assert!(split_rows(&view).len() < 30, "keep rows virtualized");
            }
            signal.set(DiffDocument::new("\t替🙂\r\n", "\t换🙂\r\n"));
            sync_reactive_elements_subtree(&mut view, &mut layout, &mut impact);
            for source in ["\t替🙂", "\t换🙂"] {
                assert!(
                    leaf(&view, source).unwrap().text_spans.is_none(),
                    "discard stale ranges"
                );
            }
        }
    });
}
