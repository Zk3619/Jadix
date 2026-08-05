use crate::app::test_support::sync_reactive_elements_subtree;
use crate::components::{radio_group, slider, toggle_group};
use crate::core::reactivity::use_signal;
use crate::core::tree::{Element, LayoutStatus, LayoutTree};
use crate::tokens;

#[test]
fn slider_value_sync_updates_track_without_rebuilding_component() {
    let value = use_signal("__rui_test_slider_reactive_sync", || 0.2_f32);
    value.set_silent(0.2);
    let mut root: Element = slider("test-slider-sync").value_signal(value).into();
    assert!((root.children[0].style.flex_grow - 0.2).abs() < 0.001);

    value.set_silent(0.8);
    let mut layout_tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_reactive_elements_subtree(&mut root, &mut layout_tree, &mut impact);

    assert!((root.children[0].style.flex_grow - 0.8).abs() < 0.001);
    assert!((root.children[2].style.flex_grow - 0.2).abs() < 0.001);
    assert_eq!(impact, LayoutStatus::LayoutChanged);
}

#[test]
fn radio_group_selection_sync_updates_pills_without_rebuilding_component() {
    let selected = use_signal("__rui_test_radio_reactive_sync", || "free".to_string());
    selected.set_silent("free".to_string());
    let mut root: Element = radio_group("test-radio-sync")
        .selected_signal(selected)
        .options(vec![
            ("free".into(), "Free".into()),
            ("pro".into(), "Pro".into()),
        ])
        .into();
    assert_eq!(root.children[0].style.bg_color, tokens::primary());

    selected.set_silent("pro".to_string());
    let mut layout_tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_reactive_elements_subtree(&mut root, &mut layout_tree, &mut impact);

    assert_eq!(root.children[0].style.bg_color, tokens::muted());
    assert_eq!(root.children[1].style.bg_color, tokens::primary());
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
