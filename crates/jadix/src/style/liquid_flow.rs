use std::collections::HashSet;

use super::{AnimatedNumber, ThemePreset};
use crate::core::tree::Element;
use crate::motion::{LayoutAnimation, LayoutMotion};

pub(crate) const ITEM: &str = "jadix_liquid_flow_item";
pub(crate) const TARGET: &str = "jadix_liquid_flow_target";

pub(crate) struct LiquidFlow {
    pub motion: LayoutMotion,
    pub visibility: AnimatedNumber,
    visible: bool,
    target_selected: bool,
    target_path: Option<Vec<usize>>,
    sample: Option<[f32; 5]>,
}

impl LiquidFlow {
    fn new() -> Self {
        Self {
            motion: LayoutMotion::new(LayoutAnimation::Both),
            visibility: AnimatedNumber::follow(0.0),
            visible: false,
            target_selected: false,
            target_path: None,
            sample: None,
        }
    }

    pub fn is_active(&self) -> bool {
        self.motion.is_active() || self.visibility.is_active()
    }

    pub fn target_selected(&self) -> bool {
        self.target_selected
    }

    pub fn sample(&self) -> Option<[f32; 5]> {
        let rect = self.motion.bounds()?;
        let velocity = self.motion.velocities();
        let stretch: [f32; 2] =
            std::array::from_fn(|axis| (velocity[axis].abs() * 0.012).min(rect[axis + 2] * 0.24));
        Some([
            rect[0] - stretch[0] * 0.5,
            rect[1] - stretch[1] * 0.5,
            rect[2] + stretch[0],
            rect[3] + stretch[1],
            self.visibility.peek(),
        ])
    }
}

fn slot(element: &Element) -> &str {
    element
        .attrs
        .get("data-slot")
        .map(String::as_str)
        .unwrap_or("")
}

pub(crate) fn sync(root: &mut Element, hits: &HashSet<*const Element>) -> bool {
    let container = matches!(
        slot(root),
        "button-group" | "toggle-group" | "tabs-list" | "sidebar-menu"
    );
    if !container {
        return false;
    }
    let enabled = super::with_theme(|theme| {
        theme.preset == ThemePreset::LiquidGlass && theme.styles.surface_opacity != Some(1.0)
    });
    fn visit(
        element: &mut Element,
        enabled: bool,
        hits: &HashSet<*const Element>,
        path: &mut Vec<usize>,
        candidates: &mut Vec<(Vec<usize>, bool, bool)>,
    ) {
        let candidate = matches!(
            slot(element),
            "button"
                | "toggle-group-item"
                | "tabs-trigger"
                | "sidebar-menu-button"
                | "button-group-text"
        );
        if candidate {
            let hovered = hits.contains(&(element as *const Element));
            let selected = matches!(
                element.attrs.get("data-state").map(String::as_str),
                Some("active" | "on")
            ) || element.attrs.get("data-active").map(String::as_str)
                == Some("true");
            let changed = element.attrs.contains_key(ITEM) != enabled;
            if enabled {
                element.set_attr(ITEM, "1");
            } else {
                element.attrs.remove(ITEM);
            }
            element.attrs.remove(TARGET);
            if changed {
                element.collect_paint_hash = 0;
            }
            candidates.push((path.clone(), hovered, selected));
        } else if !matches!(
            slot(element),
            "button-group" | "toggle-group" | "tabs-list" | "sidebar-menu"
        ) {
            if slot(element) == "button-group-separator" {
                if enabled {
                    element.set_attr(ITEM, "1");
                } else {
                    element.attrs.remove(ITEM);
                }
            }
            for (index, child) in element.children.iter_mut().enumerate() {
                path.push(index);
                visit(child, enabled, hits, path, candidates);
                path.pop();
            }
        }
    }
    let mut candidates = Vec::new();
    for (index, child) in root.children.iter_mut().enumerate() {
        visit(child, enabled, hits, &mut vec![index], &mut candidates);
    }
    if !enabled {
        return root.liquid_flow.take().is_some();
    }
    let multiple = root.attrs.get("data-multiple").map(String::as_str) == Some("true");
    if multiple {
        for (path, _, selected) in &candidates {
            if *selected {
                let mut element = &mut *root;
                for &index in path {
                    element = &mut element.children[index];
                }
                if element.attrs.remove(ITEM).is_some() {
                    element.collect_paint_hash = 0;
                }
            }
        }
    }
    let target = candidates
        .iter()
        .find(|(_, hovered, _)| *hovered)
        .or_else(|| {
            candidates
                .iter()
                .find(|(_, _, selected)| !multiple && *selected)
        })
        .filter(|(_, _, selected)| !multiple || !selected);
    let target = target.map(|(path, _, selected)| (path.clone(), *selected));
    let visible = target.is_some();
    for (path, _, selected) in &candidates {
        let is_target = target
            .as_ref()
            .is_some_and(|(target_path, _)| target_path == path);
        if !multiple && *selected && !is_target {
            let mut element = &mut *root;
            for &index in path {
                element = &mut element.children[index];
            }
            if element.attrs.remove(ITEM).is_some() {
                element.collect_paint_hash = 0;
            }
        }
    }
    if let Some((path, _)) = &target {
        let mut element = &mut *root;
        for &index in path {
            element = &mut element.children[index];
        }
        element.set_attr(TARGET, "1");
    }
    let flow = root.liquid_flow.get_or_insert_with(LiquidFlow::new);
    let target_path = target.as_ref().map(|(path, _)| path.clone());
    let target_changed = flow.target_path != target_path;
    let selected_changed =
        flow.target_selected != target.as_ref().is_some_and(|(_, selected)| *selected);
    flow.target_path = target_path;
    flow.target_selected = target.as_ref().is_some_and(|(_, selected)| *selected);
    if flow.visible != visible {
        let opacity = if visible { 1.0 } else { 0.0 };
        if super::prefers_reduced_motion() {
            flow.visibility.jump(opacity);
        } else {
            flow.visibility.set(opacity);
        }
        flow.visible = visible;
    }
    let sample = flow.sample();
    let changed = flow.sample != sample;
    flow.sample = sample;
    if flow.is_active() {
        crate::app::invalidation::request_frame();
    }
    target_changed || selected_changed || changed || flow.is_active()
}

pub(crate) fn measure(root: &mut Element, layout: &taffy::TaffyTree<()>) {
    if root.collect_paint_hash != 0 && !root.subtree_layout_motion && root.liquid_flow.is_none() {
        return;
    }
    fn target(
        element: &Element,
        layout: &taffy::TaffyTree<()>,
        origin: [f32; 2],
    ) -> Option<[f32; 4]> {
        let bounds = layout.layout(element.layout_node?).ok()?;
        let origin = [origin[0] + bounds.location.x, origin[1] + bounds.location.y];
        if element.attrs.contains_key(TARGET) {
            return Some([origin[0], origin[1], bounds.size.width, bounds.size.height]);
        }
        if element.liquid_flow.is_some() {
            return None;
        }
        element
            .children
            .iter()
            .find_map(|child| target(child, layout, origin))
    }
    if root.liquid_flow.is_some() {
        if let Some(bounds) = root
            .children
            .iter()
            .find_map(|child| target(child, layout, [0.0; 2]))
        {
            let flow = root.liquid_flow.as_ref().unwrap();
            flow.motion.update(bounds);
        }
        let sample = root.liquid_flow.as_ref().and_then(LiquidFlow::sample);
        if root.liquid_flow.as_ref().unwrap().sample != sample {
            root.collect_paint_hash = 0;
        }
    }
    for child in &mut root.children {
        measure(child, layout);
    }
}

#[cfg(all(test, feature = "theme-liquid-glass"))]
mod tests {
    use super::*;

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
}
