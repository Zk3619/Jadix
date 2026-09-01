use crate::app::test_support::sync_reactive_elements_subtree;
use crate::components::{radio_group, slider, toggle_group};
use crate::core::reactivity::use_signal;
use crate::core::tree::{Element, LayoutStatus, LayoutTree};

fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
    if el.attrs.get("data-slot").map(|s| s.as_str()) == Some(slot) {
        return Some(el);
    }
    for c in &el.children {
        if let Some(f) = find_slot(c, slot) {
            return Some(f);
        }
    }
    None
}

#[test]
fn slider_value_sync_updates_track_without_rebuilding_component() {
    let value = use_signal("__rui_test_slider_reactive_sync", || 0.2_f32);
    value.set_silent(0.2);
    let mut root: Element = slider("test-slider-sync").value_signal(value).into();
    let left = find_slot(&root, "slider-before").expect("before");
    assert!((left.style.flex_grow - 0.2).abs() < 0.001);
    let fill = find_slot(&root, "slider-fill").expect("fill");
    match crate::layout::dim_kind(fill.style.size.width) {
        crate::layout::DimKind::Percent(p) => assert!((p - 0.2).abs() < 0.001),
        other => panic!("expected percent width, got {other:?}"),
    }

    value.set_silent(0.8);
    let mut layout_tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_reactive_elements_subtree(&mut root, &mut layout_tree, &mut impact);

    let before = find_slot(&root, "slider-before").expect("before");
    let after = find_slot(&root, "slider-after").expect("after");
    assert!((before.style.flex_grow - 0.8).abs() < 0.001);
    assert!((after.style.flex_grow - 0.2).abs() < 0.001);
    let fill = find_slot(&root, "slider-fill").expect("fill");
    match crate::layout::dim_kind(fill.style.size.width) {
        crate::layout::DimKind::Percent(p) => assert!((p - 0.8).abs() < 0.001),
        other => panic!("expected percent width, got {other:?}"),
    }
    assert_eq!(impact, LayoutStatus::LayoutChanged);
}

#[test]
fn radio_group_selection_sync_updates_indicator_without_rebuilding_component() {
    let selected = use_signal("__rui_test_radio_reactive_sync", || "free".to_string());
    selected.set_silent("free".to_string());
    let mut root: Element = radio_group("test-radio-sync")
        .selected_signal(selected)
        .options(vec![
            ("free".into(), "Free".into()),
            ("pro".into(), "Pro".into()),
        ])
        .into();

    let free_ctrl = root.children[0]
        .children
        .iter()
        .find(|c| c.radio_selected_sync.is_some())
        .expect("free control");
    let free_dot = free_ctrl
        .children
        .iter()
        .find(|c| c.attrs.get("data-slot").map(|s| s.as_str()) == Some("radio-indicator"))
        .expect("free indicator");
    assert!(free_dot.style.bg_color[3] > 0.5);

    selected.set_silent("pro".to_string());
    let mut layout_tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    // Radio uses dedicated sync (not reactive_sync on root).
    crate::components::radio_group::sync_radio_checked_subtree(
        &mut root,
        &mut layout_tree,
        &mut impact,
    );

    let free_ctrl = root.children[0]
        .children
        .iter()
        .find(|c| c.radio_selected_sync.is_some())
        .expect("free control");
    let free_dot = free_ctrl
        .children
        .iter()
        .find(|c| c.attrs.get("data-slot").map(|s| s.as_str()) == Some("radio-indicator"))
        .expect("free indicator");
    let pro_ctrl = root.children[1]
        .children
        .iter()
        .find(|c| c.radio_selected_sync.is_some())
        .expect("pro control");
    let pro_dot = pro_ctrl
        .children
        .iter()
        .find(|c| c.attrs.get("data-slot").map(|s| s.as_str()) == Some("radio-indicator"))
        .expect("pro indicator");
    assert!(free_dot.style.bg_color[3] < 0.1);
    assert!(pro_dot.style.bg_color[3] > 0.5);
    assert_eq!(impact, LayoutStatus::StyleChanged);
}

#[test]
fn toggle_group_selection_sync_updates_chips_without_rebuilding_component() {
    let selected = use_signal("__rui_test_toggle_reactive_sync", || "left".to_string());
    selected.set_silent("left".to_string());
    let mut root: Element = toggle_group("test-toggle-sync")
        .value_signal(selected)
        .options(vec![
            ("left".into(), "L".into()),
            ("right".into(), "R".into()),
        ])
        .into();
    let left_before = root.children[0].style.bg_color;

    selected.set_silent("right".to_string());
    let mut layout_tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_reactive_elements_subtree(&mut root, &mut layout_tree, &mut impact);

    let left_after = root.children[0].style.bg_color;
    let right_after = root.children[1].style.bg_color;
    assert_ne!(left_before, left_after);
    assert_ne!(left_after, right_after);
    assert_eq!(impact, LayoutStatus::StyleChanged);
}
