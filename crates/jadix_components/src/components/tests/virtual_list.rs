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
fn focused_window_mounts_neighbors_and_reveals_offscreen_rows() {
    let (start, end, reveal) = focused_item_window(0.0, 100.0, 20.0, 100, 1, Some(3));
    assert_eq!((start, end, reveal), (0, 6, None));

    let (start, end, reveal) = focused_item_window(0.0, 100.0, 20.0, 100, 1, Some(20));
    assert_eq!(reveal, Some("start"));
    assert!(start <= 19);
    assert!(20 < end);
    assert!(end - start <= 9);

    let (start, end, reveal) = focused_item_window(200.0, 100.0, 20.0, 100, 1, Some(3));
    assert_eq!(reveal, Some("end"));
    assert!(start <= 2);
    assert!(3 < end);
    assert!(end - start <= 9);
}

#[test]
fn virtual_list_builds() {
    let _el: Element = virtual_list("vl1")
        .item_count(100)
        .item_height(24.0)
        .focused_index_fn(|| Some(0))
        .render_item(|i| Element::new().text(format!("#{i}")))
        .into();
}

#[test]
fn virtual_list_item_motion_is_opt_in_and_uses_stable_keys() {
    let plain: Element = virtual_list("vl-motion-plain")
        .item_count(2)
        .item_key_fn(|index| format!("item-{index}"))
        .render_item(|index| Element::new().text(format!("{index}")))
        .into();
    let plain_row = crate::core::tree::find_in_tree(&plain, |node| {
        node.attrs
            .get(ROW_KEY_ATTR)
            .is_some_and(|key| key == "k:item-0")
    })
    .expect("stable-key row");
    assert!(!plain_row.attrs.contains_key("jadix_layout_id"));

    let animated: Element = virtual_list("vl-motion")
        .item_count(2)
        .item_key_fn(|index| format!("item-{index}"))
        .animate_item_changes(LayoutAnimation::Position)
        .render_item(|index| Element::new().text(format!("{index}")))
        .into();
    let animated_row = crate::core::tree::find_in_tree(&animated, |node| {
        node.attrs
            .get(ROW_KEY_ATTR)
            .is_some_and(|key| key == "k:item-0")
    })
    .expect("animated stable-key row");
    assert_eq!(
        animated_row
            .attrs
            .get("jadix_layout_id")
            .map(String::as_str),
        Some("vl-motion__vlist_item_6974656d2d30")
    );
    assert!(animated_row.layout_motion.is_some());

    let unkeyed: Element = virtual_list("vl-motion-unkeyed")
        .item_count(2)
        .animate_item_changes(LayoutAnimation::Both)
        .render_item(|index| Element::new().text(format!("{index}")))
        .into();
    let unkeyed_row = crate::core::tree::find_in_tree(&unkeyed, |node| {
        node.attrs.get(ROW_KEY_ATTR).is_some_and(|key| key == "i:0")
    })
    .expect("unkeyed row");
    assert!(!unkeyed_row.attrs.contains_key("jadix_layout_id"));
}

#[test]
fn initial_scroll_at_end_builds_the_trailing_window() {
    let el: Element = virtual_list("vl-end")
        .item_count(100)
        .item_height(20.0)
        .height(100.0)
        .overscan(0)
        .initial_scroll_at_end()
        .render_item(|i| Element::new().text(format!("{i}")))
        .into();
    let body = crate::core::tree::find_in_tree(&el, |node| node.attrs.contains_key(RANGE_ATTR))
        .expect("virtual list body");
    let rows: Vec<_> = body
        .children
        .iter()
        .filter_map(|child| {
            child
                .template_id
                .as_deref()
                .and_then(|id| parse_row_index("vl-end", id))
        })
        .collect();
    assert_eq!(rows, (95..100).collect::<Vec<_>>());
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
    let body = build_body(
        "vl",
        120,
        21.0,
        11,
        28,
        &|i| Element::new().text(format!("{i}")),
        None,
        None,
        None,
    );
    match crate::layout::dim_kind(body.style.size.height) {
        crate::layout::DimKind::Length(h) => assert!((h - 120.0 * 21.0).abs() < 0.01),
        other => panic!("expected Length total height, got {other:?}"),
    }
}

#[test]
fn apply_body_window_is_idempotent_for_same_range() {
    let mut root = build_body(
        "vl",
        50,
        20.0,
        0,
        10,
        &|i| Element::new().text(format!("{i}")),
        None,
        None,
        None,
    );
    root.attrs
        .insert(RANGE_ATTR.to_string(), window_key(50, "", 0, 10));
    assert_eq!(
        apply_body_window(
            &mut root,
            "vl",
            50,
            "",
            20.0,
            0,
            10,
            &|i| { Element::new().text(format!("{i}")) },
            None,
            None,
            None,
            &|| 50,
        ),
        LayoutStatus::Clean
    );
}

#[test]
fn apply_body_window_updates_when_item_count_changes_same_range() {
    let mut root = build_body(
        "vl",
        50,
        20.0,
        0,
        10,
        &|i| Element::new().text(format!("{i}")),
        None,
        None,
        None,
    );
    root.attrs
        .insert(RANGE_ATTR.to_string(), window_key(50, "", 0, 10));
    assert_eq!(
        apply_body_window(
            &mut root,
            "vl",
            20,
            "",
            20.0,
            0,
            10,
            &|i| { Element::new().text(format!("n{i}")) },
            None,
            None,
            None,
            &|| 20,
        ),
        LayoutStatus::StructureChanged
    );
    match crate::layout::dim_kind(root.style.size.height) {
        crate::layout::DimKind::Length(h) => assert!((h - 20.0 * 20.0).abs() < 0.01),
        other => panic!("expected Length total height, got {other:?}"),
    }
}

#[test]
fn apply_body_window_reuses_overlap_and_drops_old() {
    let mut root = build_body(
        "vl",
        100,
        20.0,
        0,
        10,
        &|i| Element::new().text(format!("r{i:03}")),
        None,
        None,
        None,
    );
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
        apply_body_window(
            &mut root,
            "vl",
            100,
            "",
            20.0,
            5,
            15,
            &|i| { Element::new().text(format!("r{i:03}")) },
            None,
            None,
            None,
            &|| 100,
        ),
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
