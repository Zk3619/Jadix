use super::*;

#[cfg(feature = "product-tests")]
#[test]
fn shared_material_preserves_the_keyboard_focus_ring() {
    let previous = super::super::get_theme();
    super::super::set_theme_preset(ThemePreset::LiquidGlass);
    crate::core::reactivity::create_root(|| {
        crate::core::reactivity::use_signal("__jadix_button_focus_id", String::new)
            .set("flow-keyboard-focus".into());
        crate::core::reactivity::use_signal("__jadix_focus_visible", || false).set(true);
        let mut root = Element::new().w(160.0).h(48.0);
        root.set_attr("data-slot", "tabs-list");
        let mut item: Element = crate::components::button("flow-keyboard-focus")
            .variant("outline")
            .text("Focus")
            .into();
        item.set_attr(ITEM, "1");
        root.children.push(item);
        let mut layout = crate::layout::LayoutTree::new();
        let rendered = layout.build_and_layout(&mut root, 320.0, 160.0, [-1.0; 2]);
        assert!(
            rendered
                .instances
                .iter()
                .any(|instance| instance.shadow_color[3] > 0.1 && instance.shadow_spread > 0.0),
            "shared background must not suppress keyboard focus chrome"
        );
    });
    super::super::set_theme(previous);
}

#[test]
fn multiple_selection_keeps_every_selected_material_visible() {
    let previous = super::super::get_theme();
    super::super::set_theme_preset(ThemePreset::LiquidGlass);
    let mut group = Element::new();
    group.set_attr("data-slot", "toggle-group");
    group.set_attr("data-multiple", "true");
    for selected in [true, false, true] {
        let mut item = Element::new();
        item.set_attr("data-slot", "toggle-group-item");
        item.set_attr("data-state", if selected { "on" } else { "off" });
        group.children.push(item);
    }
    let hits = HashSet::from([&group.children[1] as *const Element]);
    sync(&mut group, &hits);
    assert!(!group.children[0].attrs.contains_key(ITEM));
    assert!(group.children[1].attrs.contains_key(TARGET));
    assert!(!group.children[2].attrs.contains_key(ITEM));
    let hits = HashSet::from([&group.children[0] as *const Element]);
    sync(&mut group, &hits);
    assert!(group
        .children
        .iter()
        .all(|item| !item.attrs.contains_key(TARGET)));
    super::super::set_theme(previous);
}

#[test]
fn selected_surface_stays_visible_while_hover_glass_moves_to_another_item() {
    let previous = super::super::get_theme();
    super::super::set_theme_preset(ThemePreset::LiquidGlass);
    let mut group = Element::new();
    group.set_attr("data-slot", "toggle-group");
    for state in ["on", "off"] {
        let mut item = Element::new();
        item.set_attr("data-slot", "toggle-group-item");
        item.set_attr("data-state", state);
        item.style.bg_color = if state == "on" {
            [0.4, 0.4, 0.4, 1.0]
        } else {
            [0.0; 4]
        };
        group.children.push(item);
    }

    let hovered = HashSet::from([&group.children[1] as *const Element]);
    sync(&mut group, &hovered);

    assert!(
        !group.children[0].attrs.contains_key(ITEM),
        "the selected item must keep its own selected surface while hover glass moves"
    );
    assert!(group.children[1].attrs.contains_key(ITEM));
    assert!(group.children[1].attrs.contains_key(TARGET));
    assert!(!group.liquid_flow.as_ref().unwrap().target_selected());
    assert_eq!(group.children[0].style.bg_color[3], 1.0);

    let selected_hit = HashSet::from([&group.children[0] as *const Element]);
    sync(&mut group, &selected_hit);
    assert!(group.liquid_flow.as_ref().unwrap().target_selected());
    super::super::set_theme(previous);
}

#[test]
fn glass_indicator_moves_between_items_and_retargets_without_moving_text() {
    let previous = super::super::get_theme();
    super::super::set_theme_preset(ThemePreset::LiquidGlass);
    crate::core::reactivity::create_root(|| {
        for (container, item) in [
            ("button-group", "button"),
            ("toggle-group", "toggle-group-item"),
            ("tabs-list", "tabs-trigger"),
            ("sidebar-menu", "sidebar-menu-button"),
        ] {
            for vertical in [false, true] {
                let mut root: Element = if vertical {
                    crate::primitives::vstack().into()
                } else {
                    crate::primitives::hstack().into()
                };
                root.set_attr("data-slot", container);
                for label in ["One", "Two", "Three"] {
                    let mut child = Element::new()
                        .w(80.0)
                        .h(32.0)
                        .child(Element::new().text(label));
                    child.set_attr("data-slot", item);
                    root.children.push(child);
                }
                let mut layout = crate::layout::LayoutTree::new();
                layout.build_and_layout(&mut root, 360.0, 180.0, [-1.0; 2]);
                let indices: Vec<_> = root
                    .children
                    .iter()
                    .enumerate()
                    .filter_map(|(index, child)| (!child.children.is_empty()).then_some(index))
                    .collect();
                assert_eq!(indices.len(), 3, "{container} segments");
                let labels: Vec<_> = indices
                    .iter()
                    .map(|&index| root.children[index].children[0].layout_bounds)
                    .collect();
                let hits = HashSet::from([&root.children[indices[0]] as *const Element]);
                sync(&mut root, &hits);
                measure(&mut root, &layout.taffy);
                for _ in 0..50 {
                    super::super::tick_animations(0.016);
                    sync(&mut root, &hits);
                }
                let start = root.liquid_flow.as_ref().unwrap().sample().unwrap();
                let hits = HashSet::from([&root.children[indices[2]] as *const Element]);
                sync(&mut root, &hits);
                measure(&mut root, &layout.taffy);
                let axis = usize::from(vertical);
                assert_eq!(
                    root.liquid_flow.as_ref().unwrap().sample().unwrap()[axis],
                    start[axis]
                );
                for _ in 0..4 {
                    super::super::tick_animations(0.016);
                    sync(&mut root, &hits);
                }
                let middle = root.liquid_flow.as_ref().unwrap().sample().unwrap();
                let destination =
                    root.children[indices[2]].layout_bounds[axis] - root.layout_bounds[axis];
                assert!(
                    middle[axis] > start[axis] && middle[axis] < destination,
                    "{container}/{vertical}: {start:?} -> {middle:?} -> {destination}"
                );
                assert!(
                    middle[axis + 2] > start[axis + 2],
                    "moving glass must stretch along its travel axis"
                );
                let hits = HashSet::from([&root.children[indices[1]] as *const Element]);
                sync(&mut root, &hits);
                measure(&mut root, &layout.taffy);
                assert_eq!(
                    root.liquid_flow.as_ref().unwrap().sample().unwrap()[axis],
                    middle[axis]
                );
                layout.build_and_layout(&mut root, 360.0, 180.0, [-1.0; 2]);
                assert_eq!(
                    labels,
                    indices
                        .iter()
                        .map(|&index| root.children[index].children[0].layout_bounds)
                        .collect::<Vec<_>>()
                );
            }
        }
    });
    super::super::set_theme(previous);
}
