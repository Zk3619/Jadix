use jadix_components::core::reactivity::create_root;
use jadix_components::core::scroll_viewport::scroll_offset_y_signal_key;
use jadix_components::core::tree::{LayoutStatus, LayoutTree};
use jadix_components::prelude::*;

fn list_body(element: &mut Element) -> Option<&mut Element> {
    if element.attrs.contains_key("__jadix_vlist_range") {
        return Some(element);
    }
    element.children.iter_mut().find_map(list_body)
}

fn has_line(element: &Element, number: usize) -> bool {
    element
        .attrs
        .get("data-source-line")
        .is_some_and(|line| line == &number.to_string())
        || element.children.iter().any(|child| has_line(child, number))
}

#[test]
fn text_view_rsx_and_builder_accept_the_same_snapshot() {
    let document = TextDocument::new("first\nsecond\n");
    let rsx = ui! {
        <TextView id="rsx-document" document={document.clone()} line_numbers={true}
            viewport_h={220.0} row_height={22.0} selectable={false} />
    };
    let builder: Element = text_view("builder-document")
        .document(document)
        .line_numbers(true)
        .viewport_h(220.0)
        .row_height(22.0)
        .selectable(false)
        .into();
    assert_eq!(rsx.attrs.get("data-slot"), builder.attrs.get("data-slot"));
}

#[test]
fn auto_height_editor_primes_native_viewport_before_caret_follow() {
    let (_owner, ()) = create_root(|| {
        let id = "auto-height-first-frame";
        let scroll = jadix_components::utils::text_field::textarea_scroll_rest(id);
        let viewport = use_signal(
            &jadix_components::core::scroll_viewport::scroll_viewport_h_signal_key(&scroll),
            || 44.0_f32,
        );
        let height = use_signal(
            &jadix_components::utils::text_field::textarea_height_signal_key(id),
            || 44.0_f32,
        );
        let value = Signal::new("| A | B |\n| --- | --- |\n| 1 | 2 |\n| 3 | 4 |".to_string());
        let editor: Element = text_editor(id)
            .value_signal(value)
            .rows(1)
            .font_size(16.0)
            .auto_height(1024.0)
            .into();
        assert!(height.peek() >= 112.0);
        assert_eq!(viewport.peek(), height.peek());
        assert_eq!(
            jadix_components::layout::dim_length_px(editor.style.size.height),
            Some(height.peek())
        );
        assert_eq!(
            jadix_components::utils::text_field::ensure_caret_visible_scroll_y(
                &value.peek(),
                value.peek().len() - 2,
                24.0,
                viewport.peek(),
                0.0,
                1000.0,
                Default::default(),
            ),
            None
        );
    });
}

#[test]
#[cfg(feature = "document-diff")]
fn diff_view_rsx_accepts_a_precomputed_model() {
    let diff = DiffDocument::new("before\n", "after\n");
    let view = ui! {
        <DiffView id="rsx-diff" document={diff} mode={DiffMode::Split} viewport_h={220.0} />
    };
    assert_eq!(
        view.attrs.get("data-mode").map(String::as_str),
        Some("split")
    );
}

#[test]
fn hundred_thousand_line_view_keeps_total_extent_and_scrolls_to_middle_and_tail() {
    let (_owner, ()) = create_root(|| {
        let document = TextDocument::new("source row\n".repeat(100_000));
        let mut view: Element = text_view("document-scroll-integration")
            .document(document)
            .viewport_h(220.0)
            .row_height(22.0)
            .selectable(false)
            .into();
        let mut layout = LayoutTree::new();
        let result = layout.build_and_layout(&mut view, 400.0, 220.0, [-1.0, -1.0]);
        let (_, _, max_scroll, content) = result
            .scroll_metrics
            .iter()
            .find(|(id, ..)| id == "document-scroll-integration")
            .expect("text view scrollport");
        assert!(content[1] >= 100_001.0 * 22.0 - 1.0);
        assert!(max_scroll[1] > 2_000_000.0);
        assert!(has_line(&view, 1));
        for line in [50_000, 99_995] {
            use_signal(
                &scroll_offset_y_signal_key("document-scroll-integration"),
                || 0.0,
            )
            .set_silent(line as f32 * 22.0);
            let body = list_body(&mut view).expect("virtual body");
            let sync = body
                .reactive_sync
                .clone()
                .expect("virtual window synchronizer");
            assert_eq!(sync.run(body, &mut layout), LayoutStatus::StructureChanged);
            assert!(has_line(&view, line + 1));
            assert!(!has_line(&view, 1));
        }
    });
}

#[test]
fn horizontal_extent_update_requests_layout_not_only_repaint() {
    let (_owner, ()) = create_root(|| {
        let width = use_signal("document-width-integration", || 100.0_f32);
        let mut view: Element = virtual_list("document-width-list")
            .item_count(5)
            .content_width_fn(move || width.peek())
            .into();
        let mut layout = LayoutTree::new();
        let body = list_body(&mut view).unwrap();
        let sync = body.reactive_sync.clone().unwrap();
        width.set_silent(800.0);
        assert_eq!(sync.run(body, &mut layout), LayoutStatus::LayoutChanged);
        assert_eq!(sync.run(body, &mut layout), LayoutStatus::Clean);
    });
}

#[test]
#[cfg(feature = "document-diff")]
fn diff_view_same_row_count_snapshot_updates_do_not_reuse_stale_text() {
    let (_owner, ()) = create_root(|| {
        let model = use_signal("diff-snapshot-integration", || {
            DiffDocument::new("a\n", "b\n")
        });
        let mut view: Element = diff_view("diff-live-integration", model.peek())
            .document_signal(model)
            .into();
        model.set_silent(DiffDocument::new("a\n", "c\n"));
        let mut layout = LayoutTree::new();
        let body = list_body(&mut view).unwrap();
        let sync = body.reactive_sync.clone().unwrap();
        assert_eq!(sync.run(body, &mut layout), LayoutStatus::StructureChanged);
        fn has_text(element: &Element, text: &str) -> bool {
            element.text.as_deref() == Some(text)
                || element.children.iter().any(|child| has_text(child, text))
        }
        assert!(has_text(&view, "c"));
        assert!(!has_text(&view, "b"));
    });
}

#[test]
fn publishing_a_snapshot_schedules_a_view_effect_and_unmount_disposes_it() {
    use jadix_components::core::reactivity::take_flushed_effects;

    let (_owner, ()) = create_root(|| {
        let document = use_signal("snapshot-wake-integration", || TextDocument::new("before"));
        let view: Element = text_view("snapshot-wake-view")
            .document_signal(document)
            .into();
        take_flushed_effects();
        document.set(TextDocument::new("after"));
        assert!(!take_flushed_effects().is_empty());
        drop(view);
        take_flushed_effects();
        document.set(TextDocument::new("unmounted"));
        assert!(take_flushed_effects().is_empty());
    });
}

#[test]
fn disposed_external_snapshot_source_falls_back_without_panicking() {
    use jadix_components::core::reactivity::create_signal;

    let (_owner, ()) = create_root(|| {
        let (source_owner, document) =
            create_root(|| create_signal(TextDocument::new("transient")));
        let mut view: Element = text_view("disposed-source-view")
            .document("fallback")
            .document_signal(document)
            .into();
        source_owner.dispose();
        let body = list_body(&mut view).unwrap();
        let sync = body.reactive_sync.clone().unwrap();
        assert_eq!(
            sync.run(body, &mut LayoutTree::new()),
            LayoutStatus::StructureChanged
        );
    });
}
