use super::*;
use crate::style::apply_class_list;

fn slot_of(el: &Element) -> Option<&str> {
    el.attrs.get(ATTR_SLOT).map(String::as_str)
}

fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
    if slot_of(el) == Some(slot) {
        return Some(el);
    }
    el.children.iter().find_map(|c| find_slot(c, slot))
}

fn rules_of(el: &Element) -> Vec<&Element> {
    el.children
        .iter()
        .filter(|c| slot_of(c) == Some(SLOT_RULE))
        .collect()
}

#[test]
fn marker_default_slot_and_muted_copy() {
    let el: Element = marker().content("Switched to a new branch").into();
    assert_eq!(slot_of(&el), Some(SLOT_ROOT));
    assert_eq!(
        el.attrs.get(ATTR_VARIANT).map(String::as_str),
        Some("default")
    );
    assert!(el.template_id.is_none());
    let content = find_slot(&el, SLOT_CONTENT).expect("content slot");
    let leaf = content
        .children
        .iter()
        .find(|c| c.text.as_deref() == Some("Switched to a new branch"))
        .expect("caption leaf");
    assert_eq!(
        leaf.text_style.color_resolved(),
        parse_text_color("muted-foreground")
    );
    assert_eq!(leaf.text_style.size_resolved(), CONTENT_SIZE_PX);
    for rule in rules_of(&el) {
        assert_eq!(rule.style.display, Display::None);
    }
}

#[test]
fn marker_separator_shows_flex_rules() {
    let el: Element = marker()
        .variant("separator")
        .content("Conversation compacted")
        .into();
    assert_eq!(
        el.attrs.get(ATTR_VARIANT).map(String::as_str),
        Some("separator")
    );
    let rules = rules_of(&el);
    assert_eq!(rules.len(), 2);
    assert!(rules
        .iter()
        .all(|r| r.style.display != Display::None && r.style.flex_grow > 0.0));
    let content = find_slot(&el, SLOT_CONTENT).expect("content");
    assert_eq!(content.style.flex_grow, 0.0);
    assert_eq!(content.style.flex_shrink, 0.0);
}

#[test]
fn marker_border_paints_bottom_rule() {
    let el: Element = marker()
        .variant("border")
        .content("Opened implementation notes")
        .into();
    assert_eq!(
        el.attrs.get(ATTR_VARIANT).map(String::as_str),
        Some("border")
    );
    assert!(el.style.resolved_border_widths()[2] > 0.0);
    assert_eq!(el.style.resolved_border_colors()[2], parse_color("border"));
}

#[test]
fn marker_icon_is_aria_hidden() {
    let el = MarkerIcon(MarkerIconProps {
        children: Some(Element::new().text("branch")),
        base: None,
        base_builder: None,
        ..Default::default()
    });
    assert_eq!(slot_of(&el), Some(SLOT_ICON));
    assert_eq!(
        el.attrs.get("aria-hidden").map(String::as_str),
        Some("true")
    );
}

#[test]
fn builder_assembles_icon_and_content() {
    let el: Element = marker()
        .icon(Element::new().text("⌕"))
        .content("Explored 4 files")
        .into();
    assert_eq!(slot_of(&el), Some(SLOT_ROOT));
    assert!(find_slot(&el, SLOT_ICON).is_some());
    assert!(find_slot(&el, SLOT_CONTENT).is_some());
    assert!(find_slot(&el, SLOT_CLUSTER).is_some());
}

#[test]
fn apply_bound_marker_variant_preserves_template_id() {
    let mut el: Element = marker().content("Today").into();
    el.template_id = Some("marker-bind-test".into());
    assert_eq!(
        apply_bound_marker_variant(&mut el, "separator"),
        LayoutStatus::StructureChanged
    );
    assert_eq!(el.template_id.as_deref(), Some("marker-bind-test"));
    assert_eq!(
        el.attrs.get(ATTR_VARIANT).map(String::as_str),
        Some("separator")
    );
    assert!(rules_of(&el)
        .iter()
        .all(|r| r.style.display != Display::None));
    for rule in rules_of(&el) {
        assert!(
            crate::layout::dim_length_px(rule.style.size.width).is_none(),
            "separator bind must clear hidden w(0), got {:?}",
            rule.style.size.width
        );
        assert!(rule.style.flex_grow > 0.0);
    }
    assert_eq!(
        apply_bound_marker_variant(&mut el, "separator"),
        LayoutStatus::Clean
    );
}

fn layout_marker_hairlines(
    marker_el: Element,
) -> (Element, crate::layout::layout_tree::RenderResult) {
    let mut root = Element::new().w(320.0).h(64.0).child(marker_el);
    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 320.0, 64.0, [-1.0, -1.0]);
    (root, result)
}

fn assert_visible_hairlines(root: &Element, result: &crate::layout::layout_tree::RenderResult) {
    let marker = root
        .children
        .iter()
        .find(|c| slot_of(c) == Some(SLOT_ROOT))
        .expect("marker root");
    let rules: Vec<&Element> = marker
        .children
        .iter()
        .filter(|c| slot_of(c) == Some(SLOT_RULE))
        .collect();
    assert_eq!(rules.len(), 2);
    let card = tokens::card();
    for rule in &rules {
        let [_, _, w, h] = rule.layout_bounds;
        assert!(
            w > 24.0,
            "separator hairline should take leftover width, got w={w} bounds={:?}",
            rule.layout_bounds
        );
        assert!(
            h >= 1.5 && h <= 4.0,
            "separator hairline should survive SDF AA, got h={h}"
        );
        assert!(
            (rule.style.bg_color[3] - 1.0).abs() < 0.01,
            "separator hairline must be opaque, bg={:?}",
            rule.style.bg_color
        );
        assert!(
            (luma(rule.style.bg_color) - luma(card)).abs() > 0.2,
            "hairline contrast vs card too low: {:?} card={:?}",
            rule.style.bg_color,
            card
        );
    }
    let hairlines = result
        .instances
        .iter()
        .filter(|i| i.rect_size[1] <= 4.0 && i.rect_size[0] > 24.0 && i.color[3] > 0.5)
        .count();
    assert!(
        hairlines >= 2,
        "expected painted hairline instances, got {hairlines} (instances={})",
        result.instances.len()
    );
}

#[test]
fn bound_variant_default_to_separator_keeps_hairline_geometry() {
    let mut marker_el: Element = marker().content("Today").into();
    assert_eq!(
        apply_bound_marker_variant(&mut marker_el, "separator"),
        LayoutStatus::StructureChanged
    );
    let (root, result) = layout_marker_hairlines(marker_el);
    assert_visible_hairlines(&root, &result);
}

#[test]
fn bound_variant_separator_hairlines_survive_incremental_layout() {
    let marker_el: Element = marker().content("Today").into();
    let mut root = Element::new().w(320.0).h(64.0).child(marker_el);
    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 320.0, 64.0, [-1.0, -1.0]);

    let mut dirty = Vec::new();
    {
        let marker = root
            .children
            .iter_mut()
            .find(|c| slot_of(c) == Some(SLOT_ROOT))
            .expect("marker root");
        assert_eq!(
            apply_bound_marker_variant(marker, "separator"),
            LayoutStatus::StructureChanged
        );
        crate::core::tree::collect_bind_layout_dirty(
            marker,
            crate::app::reactive::sanitize_leaf_bind_status(LayoutStatus::StructureChanged),
            &mut dirty,
        );
    }
    assert!(
        !dirty.is_empty(),
        "separator bind must dirty the marker subtree"
    );
    let dirty_set: std::collections::HashSet<_> = dirty.into_iter().collect();
    tree.mark_dirty_nodes_in_tree(&root, &dirty_set);
    let result =
        tree.build_and_layout_fast_ex(&mut root, 320.0, 64.0, [-1.0, -1.0], false, false, None);
    assert_visible_hairlines(&root, &result);
}

#[test]
fn dual_surface_compiles_shape() {
    let _b: Element = marker().variant("border").content("X").into();
    let _u: Element = ui_marker().variant("separator").content("Y").into();
    let el = Marker(MarkerProps {
        id: String::new(),
        variant: Some(MarkerVariant::Default),
        children: None,
        base: None,
        base_builder: None,
        ..Default::default()
    });
    let _: Element = el;
}

fn luma(c: [f32; 4]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

#[test]
fn separator_rules_get_visible_hairline_geometry() {
    let marker_el: Element = marker().variant("separator").content("Today").into();
    let mut root = Element::new().w(320.0).h(64.0).child(marker_el);
    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 320.0, 64.0, [-1.0, -1.0]);
    let marker = root
        .children
        .iter()
        .find(|c| slot_of(c) == Some(SLOT_ROOT))
        .expect("marker root");
    let rules: Vec<&Element> = marker
        .children
        .iter()
        .filter(|c| slot_of(c) == Some(SLOT_RULE))
        .collect();
    assert_eq!(rules.len(), 2);
    let card = tokens::card();
    for rule in &rules {
        let [_, _, w, h] = rule.layout_bounds;
        assert!(
            w > 24.0,
            "separator hairline should take leftover width, got w={w} bounds={:?}",
            rule.layout_bounds
        );
        assert!(
            h >= 1.5 && h <= 4.0,
            "separator hairline should survive SDF AA, got h={h}"
        );
        assert!(
            (rule.style.bg_color[3] - 1.0).abs() < 0.01,
            "separator hairline must be opaque, bg={:?}",
            rule.style.bg_color
        );
        assert!(
            (luma(rule.style.bg_color) - luma(card)).abs() > 0.2,
            "hairline contrast vs card too low: {:?} card={:?}",
            rule.style.bg_color,
            card
        );
    }
    let hairlines = result
        .instances
        .iter()
        .filter(|i| i.rect_size[1] <= 4.0 && i.rect_size[0] > 24.0 && i.color[3] > 0.5)
        .count();
    assert!(
        hairlines >= 2,
        "expected painted hairline instances, got {hairlines} (instances={})",
        result.instances.len()
    );
}

fn text_box(n: &crate::layout::layout_tree::TextLayoutInfo) -> [f32; 4] {
    [n.x, n.y, n.width, n.height]
}

fn aabb_overlap(a: [f32; 4], b: [f32; 4], slack: f32) -> bool {
    a[0] + slack < b[0] + b[2]
        && b[0] + slack < a[0] + a[2]
        && a[1] + slack < b[1] + b[3]
        && b[1] + slack < a[1] + a[3]
}

fn contains_box(outer: [f32; 4], inner: [f32; 4], pad: f32) -> bool {
    inner[0] + 0.5 >= outer[0] - pad
        && inner[1] + 0.5 >= outer[1] - pad
        && inner[0] + inner[2] <= outer[0] + outer[2] + pad + 0.5
        && inner[1] + inner[3] <= outer[1] + outer[3] + pad + 0.5
}

/// Collect ink must sit in the caption / rule slots. A full-width root
/// `text_left` used to paint the caption at the Marker origin (叠在发丝线上)
/// and dest-reset then smeared the leftover glyphs.
#[test]
fn marker_collect_ink_stays_in_slots_not_root_origin() {
    let default_el: Element = marker()
        .icon(Element::new().text("⌕"))
        .content("Explored 4 files")
        .into();
    let sep_el: Element = marker()
        .variant("separator")
        .icon(Element::new().text("⌕"))
        .content("Explored 4 files")
        .into();

    for (label, marker_el) in [("default", default_el), ("separator", sep_el)] {
        let mut root = Element::new().w(320.0).h(64.0).child(marker_el.w("100%"));
        let mut tree = crate::layout::layout_tree::LayoutTree::new();
        let result = tree.build_and_layout(&mut root, 320.0, 64.0, [-1.0, -1.0]);
        let marker = root
            .children
            .iter()
            .find(|c| slot_of(c) == Some(SLOT_ROOT))
            .unwrap_or_else(|| panic!("{label}: marker root"));
        let content =
            find_slot(marker, SLOT_CONTENT).unwrap_or_else(|| panic!("{label}: content slot"));
        let icon = find_slot(marker, SLOT_ICON).unwrap_or_else(|| panic!("{label}: icon slot"));
        let caption = result
            .text_nodes
            .iter()
            .find(|n| n.text.contains("Explored 4 files"))
            .unwrap_or_else(|| panic!("{label}: caption collect"));
        let caption_box = text_box(caption);
        assert!(
                contains_box(content.layout_bounds, caption_box, 4.0),
                "{label}: caption collect must sit in the content slot, not the Marker origin\n caption={caption_box:?} content={:?} marker={:?}",
                content.layout_bounds,
                marker.layout_bounds
            );
        assert!(
                caption_box[0] + 0.5 >= icon.layout_bounds[0] + icon.layout_bounds[2] - 1.0,
                "{label}: caption must start after the icon (渲染错位)\n caption={caption_box:?} icon={:?}",
                icon.layout_bounds
            );
        if label == "separator" {
            for rule in rules_of(marker) {
                let hair = result.instances.iter().find(|i| {
                    i.rect_size[1] <= 4.0
                        && i.rect_size[0] > 24.0
                        && (i.rect_pos[0] - rule.layout_bounds[0]).abs() < 4.0
                });
                if let Some(h) = hair {
                    let hb = [h.rect_pos[0], h.rect_pos[1], h.rect_size[0], h.rect_size[1]];
                    assert!(
                            !aabb_overlap(hb, caption_box, 1.0),
                            "{label}: hairline overlaps caption (弄脏)\n hair={hb:?} caption={caption_box:?} rule={:?}",
                            rule.layout_bounds
                        );
                    let cy = caption_box[1] + caption_box[3] * 0.5;
                    let hy = hb[1] + hb[3] * 0.5;
                    assert!(
                            (hy - cy).abs() <= 8.0,
                            "{label}: hairline Y {hy} must stay near caption center {cy} (渲染错位)\n hair={hb:?} caption={caption_box:?}"
                        );
                }
            }
        }
    }
}

#[test]
fn default_and_separator_keep_icon_content_gap() {
    fn gap_after_layout(variant: &str) -> f32 {
        let marker_el: Element = marker()
            .variant(variant)
            .icon(Element::new().text("i"))
            .content("Hello")
            .into();
        let mut root = Element::new().w(320.0).h(64.0).child(marker_el);
        let mut tree = crate::layout::layout_tree::LayoutTree::new();
        let _ = tree.build_and_layout(&mut root, 320.0, 64.0, [-1.0, -1.0]);
        let marker = root
            .children
            .iter()
            .find(|c| slot_of(c) == Some(SLOT_ROOT))
            .expect("marker");
        let icon = find_slot(marker, SLOT_ICON).expect("icon");
        let content = find_slot(marker, SLOT_CONTENT).expect("content");
        content.layout_bounds[0] - (icon.layout_bounds[0] + icon.layout_bounds[2])
    }
    let default_gap = gap_after_layout("default");
    let separator_gap = gap_after_layout("separator");
    assert!(
            (default_gap - separator_gap).abs() < 1.0,
            "separator must not open space between icon and caption: default={default_gap} separator={separator_gap}"
        );
}

#[test]
fn flex_col_class_stacks_cluster() {
    let el: Element = marker()
        .class("flex-col")
        .icon(Element::new().text("i"))
        .content("Syncing completed")
        .into();
    let cluster = find_slot(&el, SLOT_CLUSTER).expect("cluster");
    assert_eq!(
        cluster.style.flex_direction,
        taffy::style::FlexDirection::Column
    );
}

#[test]
fn marker_role_status_is_forwarded() {
    let el: Element = marker().role("status").content("Thinking...").into();
    assert_eq!(el.attrs.get("role").map(String::as_str), Some("status"));
}

#[test]
fn marker_content_shimmer_class() {
    let el: Element = marker_content().class("shimmer").text("Thinking...").into();
    assert_eq!(slot_of(&el), Some(SLOT_CONTENT));
    assert_eq!(
        el.attrs
            .get(crate::renderer::ATTR_GLYPH_FX)
            .map(String::as_str),
        Some("shimmer")
    );
    assert!(el
        .style
        .animations
        .iter()
        .any(|a| a.name == "tw-shimmer" && a.duration_ms == 2000.0));
}

#[test]
fn builder_styles_icon_content_and_root_slots() {
    let fg = parse_text_color("foreground");
    let el: Element = marker()
        .style(|b| b.gap(12.0))
        .icon_with(|icon| {
            icon.class("text-primary")
                .style(|b| b.w("1.25rem").h("1.25rem"))
                .child(Element::new().text("⌕"))
        })
        .content_with(|c| {
            c.class("shimmer")
                .style(move |b| b.text_size(16.0).text_color(fg))
                .text("Thinking...")
        })
        .into();

    let icon = find_slot(&el, SLOT_ICON).expect("icon");
    let default_icon: Element = marker_icon().child(Element::new().text("x")).into();
    assert_eq!(
        crate::layout::dim_length_px(icon.style.size.width),
        Some(20.0)
    );
    assert_eq!(
        crate::layout::dim_length_px(default_icon.style.size.width),
        Some(16.0)
    );
    assert_eq!(
        icon.attrs
            .get(crate::style::ATTR_OVERRIDE_FG)
            .map(String::as_str),
        Some("1")
    );
    let glyph = icon
        .children
        .iter()
        .find(|c| c.text.as_deref() == Some("⌕"))
        .expect("glyph");
    assert_eq!(
        glyph.text_style.color_resolved(),
        parse_text_color("primary")
    );

    let content = find_slot(&el, SLOT_CONTENT).expect("content");
    assert_eq!(
        content
            .attrs
            .get(crate::renderer::ATTR_GLYPH_FX)
            .map(String::as_str),
        Some("shimmer")
    );
    let leaf = content
        .children
        .iter()
        .find(|c| c.text.as_deref() == Some("Thinking..."))
        .expect("caption");
    assert_eq!(leaf.text_style.color_resolved(), fg);
    assert_eq!(leaf.text_style.size_resolved(), 16.0);
}

#[test]
fn builder_icon_does_not_double_wrap_existing_slot() {
    let el: Element = marker()
        .icon(
            marker_icon()
                .class("text-primary")
                .child(Element::new().text("i")),
        )
        .content("X")
        .into();
    fn count(el: &Element, slot: &str) -> usize {
        let here = usize::from(slot_of(el) == Some(slot));
        here + el.children.iter().map(|c| count(c, slot)).sum::<usize>()
    }
    assert_eq!(count(&el, SLOT_ICON), 1);
}

#[test]
fn marker_spin_icon_child_is_pointer_events_none() {
    let mut glyph = crate::utils::build_named_icon_text_node("loader-circle", "\u{ea0e}");
    apply_class_list(&mut glyph, "animate-spin");
    assert_eq!(
        glyph.pointer_events_mode(),
        crate::core::tree::PointerEvents::BoundingBox,
        "lucide Icon defaults to bounding-box before the Marker slot stamps none"
    );
    let el: Element = marker()
        .icon(glyph)
        .content_with(|c| c.class("shimmer").text("Thinking..."))
        .into();
    let icon = find_slot(&el, SLOT_ICON).expect("icon slot");
    assert_eq!(
        icon.pointer_events_mode(),
        crate::core::tree::PointerEvents::None
    );
    let spin = icon
        .children
        .iter()
        .find(|c| c.attrs.get("jadix_animate").map(String::as_str) == Some("spin"))
        .expect("spin glyph");
    assert_eq!(
        spin.pointer_events_mode(),
        crate::core::tree::PointerEvents::None,
        "spin AABB must not steal hover from the Marker row"
    );
}
