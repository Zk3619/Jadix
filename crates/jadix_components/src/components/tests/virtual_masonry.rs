use super::*;

#[test]
fn virtual_masonry_builds() {
    let _el: Element = virtual_masonry("vm1")
        .item_count(50)
        .columns(3)
        .item_height(|i, _| 40.0 + (i % 3) as f32 * 10.0)
        .render_item(|i, w, h| Element::from(r#box().w(w).h(h).text(format!("#{i}"))))
        .into();
}

#[test]
fn builder_style_reaches_masonry_root() {
    let el: Element = virtual_masonry("vm-chrome")
        .item_count(4)
        .columns(2)
        .item_height(|i, _| 40.0 + (i % 2) as f32 * 10.0)
        .style(|b| b.text_size(20.0))
        .render_item(|i, w, h| Element::from(r#box().w(w).h(h).text(format!("#{i}"))))
        .into();
    fn find_size(el: &Element, want: f32) -> bool {
        el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01)
            || (el.text_style.size_resolved() - want).abs() < 0.01
            || el.children.iter().any(|c| find_size(c, want))
    }
    assert!(
        find_size(&el, 20.0),
        "virtual_masonry .style must land on the scroll root"
    );
}

#[test]
fn apply_body_window_reuses_overlap() {
    let heights = vec![40.0; 30];
    let layout = pack_masonry(&heights, 2, 200.0, 8.0);
    let vis_a: Vec<usize> = (0..8).collect();
    let mut root = build_body("vm", &layout, &vis_a, &|i, w, h| {
        Element::from(r#box().w(w).h(h).text(format!("c{i}")))
    });
    root.attrs.insert(
        RANGE_ATTR.to_string(),
        window_key(&vis_a, layout.total_height, layout.column_width),
    );
    for child in &mut root.children {
        if let Some(i) = child
            .template_id
            .as_deref()
            .and_then(|id| parse_cell_index("vm", id))
        {
            if (4..8).contains(&i) {
                child.set_layout_node_id(Some(taffy::prelude::NodeId::from(2000u64 + i as u64)));
            }
        }
    }

    let vis_b: Vec<usize> = (4..12).collect();
    assert_eq!(
        apply_body_window(&mut root, "vm", &layout, &vis_b, &|i, w, h| {
            Element::from(r#box().w(w).h(h).text(format!("c{i}")))
        }),
        LayoutStatus::StructureChanged
    );
    assert_eq!(root.attrs.get(REUSED_ATTR).map(String::as_str), Some("4"));
    assert_eq!(root.attrs.get(CREATED_ATTR).map(String::as_str), Some("4"));
    assert_eq!(root.attrs.get(DROPPED_ATTR).map(String::as_str), Some("4"));

    let node4 = root
        .children
        .iter()
        .find(|c| c.template_id.as_deref() == Some("vm__cell_4"))
        .and_then(|c| c.layout_node_id());
    assert_eq!(node4, Some(taffy::prelude::NodeId::from(2004u64)));
}
