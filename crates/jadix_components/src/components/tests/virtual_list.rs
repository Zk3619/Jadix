use super::*;
use crate::utils::virtual_list::visible_item_range;

#[test]
fn visible_range_helper() {
    let (a, b) = visible_item_range(0.0, 100.0, 20.0, 100, 1);
    assert_eq!(a, 0);
    assert!(b > a);
    let (a2, b2) = visible_item_range(40.0, 100.0, 20.0, 100, 2);
    assert_eq!(a2, 0);
    assert_eq!(b2, 9);
}

#[test]
fn virtual_list_builds() {
    let _el: Element = virtual_list("vl1")
        .item_count(100)
        .item_height(24.0)
        .render_item(|i| Element::new().text(format!("#{i}")))
        .into();
}

#[test]
fn builder_style_reaches_list_root() {
    let el: Element = virtual_list("vl-chrome")
        .item_count(3)
        .item_height(24.0)
        .style(|b| b.text_size(20.0))
        .render_item(|i| Element::new().text(format!("#{i}")))
        .into();
    fn find_size(el: &Element, want: f32) -> bool {
        el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01)
            || (el.text_style.size_resolved() - want).abs() < 0.01
            || el.children.iter().any(|c| find_size(c, want))
    }
    assert!(
        find_size(&el, 20.0),
        "virtual_list .style must land on the scroll root"
    );
}

#[test]
fn build_body_total_height_matches_item_count() {
    let body = build_body("vl", 120, 21.0, 11, 28, &|i| {
        Element::new().text(format!("{i}"))
    });
    match crate::layout::dim_kind(body.style.size.height) {
        crate::layout::DimKind::Length(h) => assert!((h - 120.0 * 21.0).abs() < 0.01),
        other => panic!("expected Length total height, got {other:?}"),
    }
}

#[test]
fn apply_body_window_is_idempotent_for_same_range() {
    let mut root = build_body("vl", 50, 20.0, 0, 10, &|i| {
        Element::new().text(format!("{i}"))
    });
    root.attrs
        .insert(RANGE_ATTR.to_string(), window_key(50, "", 0, 10));
    assert_eq!(
        apply_body_window(&mut root, "vl", 50, "", 20.0, 0, 10, &|i| {
            Element::new().text(format!("{i}"))
        }),
        LayoutStatus::Clean
    );
}

#[test]
fn apply_body_window_updates_when_item_count_changes_same_range() {
    let mut root = build_body("vl", 50, 20.0, 0, 10, &|i| {
        Element::new().text(format!("{i}"))
    });
    root.attrs
        .insert(RANGE_ATTR.to_string(), window_key(50, "", 0, 10));
    assert_eq!(
        apply_body_window(&mut root, "vl", 20, "", 20.0, 0, 10, &|i| {
            Element::new().text(format!("n{i}"))
        }),
        LayoutStatus::StructureChanged
    );
    match crate::layout::dim_kind(root.style.size.height) {
        crate::layout::DimKind::Length(h) => assert!((h - 20.0 * 20.0).abs() < 0.01),
        other => panic!("expected Length total height, got {other:?}"),
    }
}

#[test]
fn apply_body_window_reuses_overlap_and_drops_old() {
    let mut root = build_body("vl", 100, 20.0, 0, 10, &|i| {
        Element::new().text(format!("r{i:03}"))
    });
    // Stamp fake layout nodes on rows 5..10 so we can assert identity after reuse.
    for child in &mut root.children {
        if let Some(i) = child
            .template_id
            .as_deref()
            .and_then(|id| parse_row_index("vl", id))
        {
            if (5..10).contains(&i) {
                child.set_layout_node_id(Some(taffy::prelude::NodeId::from(1000u64 + i as u64)));
            }
        }
    }
    root.attrs
        .insert(RANGE_ATTR.to_string(), window_key(100, "", 0, 10));

    assert_eq!(
        apply_body_window(&mut root, "vl", 100, "", 20.0, 5, 15, &|i| {
            Element::new().text(format!("r{i:03}"))
        }),
        LayoutStatus::StructureChanged
    );
    assert_eq!(root.attrs.get(REUSED_ATTR).map(String::as_str), Some("5"));
    assert_eq!(root.attrs.get(CREATED_ATTR).map(String::as_str), Some("5"));
    assert_eq!(root.attrs.get(DROPPED_ATTR).map(String::as_str), Some("5"));

    let mut texts = Vec::new();
    for c in &root.children {
        if let Some(t) = &c.text {
            texts.push(t.clone());
        }
    }
    assert!(texts.iter().any(|t| t == "r005"));
    assert!(texts.iter().any(|t| t == "r014"));
    assert!(!texts.iter().any(|t| t == "r000"));

    let node5 = root
        .children
        .iter()
        .find(|c| c.template_id.as_deref() == Some("vl__row_5"))
        .and_then(|c| c.layout_node_id());
    assert_eq!(node5, Some(taffy::prelude::NodeId::from(1005u64)));
}
