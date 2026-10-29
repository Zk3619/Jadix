use super::*;
use crate::components::button::button;

fn segments(el: &Element) -> Vec<&Element> {
    el.children
        .iter()
        .filter(|c| c.attrs.contains_key(ATTR_GROUP_EDGE))
        .collect()
}

fn fill_radius() -> f32 {
    JOIN_RADIUS_PX - 1.0
}

#[test]
fn root_slot_role_and_orientation() {
    let el = ButtonGroup(ButtonGroupProps {
        id: String::new(),
        orientation: Some("vertical".into()),
        children: Some(Element::new().text("x")),
        base: None,
        ..Default::default()
    });
    assert_eq!(el.attrs.get(ATTR_SLOT).map(String::as_str), Some(SLOT_ROOT));
    assert_eq!(
        el.attrs.get("data-orientation").map(String::as_str),
        Some("vertical")
    );
}

#[test]
fn vertical_pill_groups_use_segment_radius_not_whole_group_radius() {
    use crate::style::{set_theme, set_theme_preset, theme_preset, ThemePreset};
    let previous = crate::style::get_theme();
    for preset in [ThemePreset::LiquidGlass, ThemePreset::Material3]
        .into_iter()
        .filter(|preset| preset.is_enabled())
    {
        set_theme_preset(preset);
        crate::core::reactivity::create_root(|| {
            let group: Element = button_group()
                .orientation("vertical")
                .child(button("pill-top").variant("outline").text("Top"))
                .child(button("pill-middle").variant("outline").text("Middle"))
                .child(button("pill-bottom").variant("outline").text("Bottom"))
                .into();
            let height =
                crate::layout::dim_length_px(segments(&group)[0].style.size.height).unwrap();
            assert!(
                group.style.radii[0] <= height * 0.5,
                "{preset:?} group radius must match its end segment, not the entire stack: {:?}",
                group.style.radii
            );
            assert!(segments(&group)[0].style.radii[0] <= height * 0.5);
            assert_eq!(theme_preset(), preset);
        });
    }
    set_theme(previous);
}

#[test]
fn hovered_joined_segment_preserves_adjacent_seam() {
    crate::core::reactivity::create_root(|| {
        let mut root = Element::new().w(240.0).h(160.0).child(Element::from(
            button_group()
                .orientation("vertical")
                .child(button("seam-top").variant("outline").text("Top"))
                .child(button("seam-middle").variant("outline").text("Middle")),
        ));
        let mut layout = crate::layout::layout_tree::LayoutTree::new();
        layout.build_and_layout(&mut root, 240.0, 160.0, [-1.0; 2]);
        let bounds = root.children[0].children[0].layout_bounds;
        let mouse = [bounds[0] + bounds[2] * 0.5, bounds[1] + bounds[3] * 0.5];
        let mut impact = crate::core::tree::LayoutStatus::Clean;
        crate::style::sync_class_variants_subtree(
            &mut root,
            mouse,
            "",
            false,
            240.0,
            160.0,
            &mut layout,
            &mut impact,
        );
        let group = &root.children[0];
        assert_eq!(
            group.children[1].style.bg_color, group.children[1].style.border_color,
            "hover must preserve the connecting border"
        );
    });
}

#[test]
fn hovering_selected_segment_repaints_group_even_with_unchanged_seam_color() {
    crate::core::reactivity::create_root(|| {
        let mut group: Element = button_group()
            .child(button("selected-top").variant("outline").text("Top"))
            .child(button("selected-middle").variant("outline").text("Middle"))
            .into();
        group.children[2].set_attr("data-state", "on");
        group.children[2].style.bg_color = [0.25, 0.25, 0.25, 1.0];
        let idle = std::collections::HashSet::<*const Element>::new();
        assert!(!sync_join_seams(&mut group, &|element| idle
            .contains(&(element as *const Element))));
        assert!(!sync_join_seams(&mut group, &|element| idle
            .contains(&(element as *const Element))));
        let color = group.children[1].style.bg_color;
        let hovered = std::collections::HashSet::from([&group.children[2] as *const Element]);
        assert!(sync_join_seams(&mut group, &|element| hovered
            .contains(&(element as *const Element))));
        assert_eq!(group.children[1].style.bg_color, color);
        assert!(!sync_join_seams(&mut group, &|element| hovered
            .contains(&(element as *const Element))));
        assert!(sync_join_seams(&mut group, &|element| idle
            .contains(&(element as *const Element))));
    });
}

#[test]
fn joins_outline_buttons_horizontal() {
    let a: Element = button("a").variant("outline").text("A").into();
    let b: Element = button("b").variant("outline").text("B").into();
    let el: Element = button_group().child(a).child(b).into();
    let segs = segments(&el);
    assert_eq!(segs.len(), 2);
    assert_eq!(el.children.len(), 3, "one seam strip between the two fills");
    let first = segs[0];
    let second = segs[1];
    assert_eq!(
        first.attrs.get(ATTR_GROUP_EDGE).map(String::as_str),
        Some(EDGE_START)
    );
    assert_eq!(
        second.attrs.get(ATTR_GROUP_EDGE).map(String::as_str),
        Some(EDGE_END)
    );
    let r = fill_radius();
    // CSS [TL, TR, BR, BL]: start = left corners; end = right corners.
    // Fill radius is the frame radius minus the 1px perimeter.
    assert_eq!(first.style.radii, [r, 0.0, 0.0, r]);
    assert_eq!(second.style.radii, [0.0, r, r, 0.0]);
    assert!(
        first.style.border_width < 0.01 && second.style.border_width < 0.01,
        "joined items paint fill only; the group owns the stroke"
    );
    assert!(
        (el.style.border_width - 1.0).abs() < 0.01,
        "group perimeter is the single 1px stroke"
    );
    assert_eq!(el.style.radii, [JOIN_RADIUS_PX; 4]);
    assert!(second.attrs.get(ATTR_PAINT_NUDGE).is_none());
    assert_eq!(
        second.style.margin.left,
        taffy::style::LengthPercentageAuto::length(0.0)
    );
    assert_eq!(second.style.z_index, 0);
    assert_eq!(second.style.translate, [0.0, 0.0]);
    assert_eq!(
        el.children[1].style.z_index, 1,
        "seam paints after both fills"
    );
}

#[test]
fn text_slot_is_flex_shell_with_caption_child() {
    let el = ButtonGroupText(ButtonGroupTextProps {
        children: Some(Element::new().text("Archive")),
        base: None,
        ..Default::default()
    });
    assert!(
        el.text.as_ref().is_none_or(|t| t.is_empty()),
        "caption must not live on the bordered root"
    );
    assert_eq!(el.children.len(), 1);
    assert_eq!(el.children[0].text.as_deref(), Some("Archive"));
    assert!(el.style.border_width > 0.5);
}

#[test]
fn separator_between_buttons_still_joins_both_edges() {
    let el: Element = button_group()
        .button("Paste", || {})
        .separator()
        .button("Cut", || {})
        .into();
    assert_eq!(
        el.children.len(),
        3,
        "author separator is the seam; no extra strip"
    );
    let a = &el.children[0];
    let sep = &el.children[1];
    let b = &el.children[2];
    assert_eq!(
        sep.attrs.get(ATTR_SLOT).map(String::as_str),
        Some(SLOT_SEPARATOR)
    );
    assert_eq!(
        a.attrs.get(ATTR_GROUP_EDGE).map(String::as_str),
        Some(EDGE_START),
        "button before separator is start, not end"
    );
    assert_eq!(
        b.attrs.get(ATTR_GROUP_EDGE).map(String::as_str),
        Some(EDGE_END),
        "button after separator must still join as end"
    );
    let r = fill_radius();
    // Start: left round only; End: right round only (facing sides square → no pill gap).
    assert_eq!(a.style.radii, [r, 0.0, 0.0, r]);
    assert_eq!(b.style.radii, [0.0, r, r, 0.0]);
    assert!(a.style.border_width < 0.01 && b.style.border_width < 0.01);
    assert!((el.style.border_width - 1.0).abs() < 0.01);
    assert!(a.attrs.get(ATTR_PAINT_NUDGE).is_none());
    assert!(b.attrs.get(ATTR_PAINT_NUDGE).is_none());
    assert_eq!(
        b.style.margin.left,
        taffy::style::LengthPercentageAuto::length(0.0)
    );
    assert_eq!(
        sep.style.margin.left,
        taffy::style::LengthPercentageAuto::length(0.0)
    );
    assert_eq!(
        sep.style.margin.right,
        taffy::style::LengthPercentageAuto::length(0.0)
    );
    assert_eq!(b.style.translate, [0.0, 0.0]);
}

#[test]
fn text_open_sep_delete_assigns_end_on_delete() {
    let kids = vec![
        ButtonGroupText(ButtonGroupTextProps {
            children: Some(Element::new().text("Archive")),
            base: None,
            ..Default::default()
        }),
        Element::from(button("open").variant("outline").text("Open")),
        ButtonGroupSeparator(ButtonGroupSeparatorProps {
            orientation: None,
            children: None,
            base: None,
            ..Default::default()
        }),
        Element::from(button("del").variant("outline").text("Delete")),
    ];
    let el = build_button_group("", ButtonGroupOrientation::Horizontal, kids);
    let segs = segments(&el);
    assert_eq!(segs.len(), 3);
    assert_eq!(
        el.children.len(),
        5,
        "Archive|Open get one strip; the author separator is the Open|Delete seam"
    );
    assert_eq!(
        segs[0].attrs.get(ATTR_GROUP_EDGE).map(String::as_str),
        Some(EDGE_START)
    );
    assert_eq!(
        segs[1].attrs.get(ATTR_GROUP_EDGE).map(String::as_str),
        Some(EDGE_MIDDLE),
        "Open must be middle (square right), not end"
    );
    assert_eq!(
        segs[2].attrs.get(ATTR_GROUP_EDGE).map(String::as_str),
        Some(EDGE_END)
    );
    let r = fill_radius();
    assert_eq!(segs[1].style.radii, [0.0; 4]);
    assert_eq!(segs[2].style.radii, [0.0, r, r, 0.0]);
    assert!(segs.iter().all(|s| s.style.border_width < 0.01));
    assert!((el.style.border_width - 1.0).abs() < 0.01);
}

#[test]
fn separator_layout_width_is_one_px_not_min_rem() {
    use crate::core::reactivity::create_root;
    use crate::core::tree::LayoutTree;

    let (_handle, _) = create_root(|| {
        let mut root = Element::new().w(400.0).h(80.0).child(Element::from(
            button_group()
                .button("Paste", || {})
                .separator()
                .button("Cut", || {}),
        ));
        let mut layout = LayoutTree::new();
        let _ = layout.build_and_layout(&mut root, 400.0, 80.0, [-1.0, -1.0]);
        let group = &root.children[0];
        assert_eq!(group.children.len(), 3);
        let sep = &group.children[1];
        let w = sep.layout_bounds[2];
        let h = sep.layout_bounds[3];
        assert!(
            w > 0.0 && w < 2.5,
            "vertical separator must stay ~1px wide (got {w}); min_w:1rem would open a white seam"
        );
        assert!(
                h > 20.0,
                "separator must stretch to button height (got {h}); zero-height sep paints a white seam"
            );
        let a = &group.children[0];
        let b = &group.children[2];
        let seam = b.layout_bounds[0] - (a.layout_bounds[0] + a.layout_bounds[2]);
        assert!(
                seam > 0.5 && seam < 2.0,
                "Paste|Cut seam should be the 1px separator, not a collapsed overlap or a multi-pixel gap, got {seam}"
            );
    });
}

#[test]
fn three_outline_buttons_layout_seams_collapse() {
    use crate::core::reactivity::create_root;
    use crate::core::tree::LayoutTree;

    let (_handle, _) = create_root(|| {
        let mut root = Element::new().w(600.0).h(80.0).child(Element::from(
            button_group()
                .child(button("a").variant("outline").text("Copy"))
                .child(button("b").variant("outline").text("Paste"))
                .child(button("c").variant("outline").text("Cut")),
        ));
        let mut layout = LayoutTree::new();
        let _ = layout.build_and_layout(&mut root, 600.0, 80.0, [-1.0, -1.0]);
        let group = &root.children[0];
        let segs = segments(group);
        assert_eq!(segs.len(), 3);
        assert_eq!(group.children.len(), 5);
        let a = segs[0];
        let b = segs[1];
        let c = segs[2];
        let seam_ab = b.layout_bounds[0] - (a.layout_bounds[0] + a.layout_bounds[2]);
        let seam_bc = c.layout_bounds[0] - (b.layout_bounds[0] + b.layout_bounds[2]);
        assert!(
            seam_ab > 0.5 && seam_ab < 2.0,
            "Copy|Paste seam should be one 1px strip, got {seam_ab}"
        );
        assert!(
            (seam_bc - seam_ab).abs() < 0.25,
            "Paste|Cut seam must match Copy|Paste, got ab={seam_ab} bc={seam_bc}"
        );
        assert!(segs.iter().all(|s| s.style.border_width < 0.01));
        assert!((group.style.border_width - 1.0).abs() < 0.01);
        assert!(a.attrs.get(ATTR_PAINT_NUDGE).is_none());
        assert!(b.attrs.get(ATTR_PAINT_NUDGE).is_none());
        assert!(c.attrs.get(ATTR_PAINT_NUDGE).is_none());
        assert_eq!(group_paint_bounds(a, a.layout_bounds), a.layout_bounds);
        assert_eq!(group_paint_bounds(b, b.layout_bounds), b.layout_bounds);
        assert_eq!(group_paint_bounds(c, c.layout_bounds), c.layout_bounds);
        assert_eq!(
            b.style.margin.left,
            taffy::style::LengthPercentageAuto::length(0.0)
        );
        assert_eq!(
            c.style.margin.left,
            taffy::style::LengthPercentageAuto::length(0.0)
        );
    });
}

#[test]
fn rsx_vstack_pack_three_buttons_still_overlaps() {
    use crate::core::reactivity::create_root;
    use crate::core::tree::{IntoElementList, LayoutTree};
    use crate::primitives::r#box::vstack;

    let (_handle, _) = create_root(|| {
        // The macro converts multiple child nodes into VStack; take_packed must be split, otherwise the horizontal group will become one column without overlapping edges.
        let packed = {
            let mut list = Vec::new();
            list.extend(
                Element::from(button("a").variant("outline").text("Copy")).into_element_list(),
            );
            list.extend(
                Element::from(button("b").variant("outline").text("Paste")).into_element_list(),
            );
            list.extend(
                Element::from(button("c").variant("outline").text("Cut")).into_element_list(),
            );
            Element::from(vstack().children(list))
        };
        let mut root = Element::new()
            .w(600.0)
            .h(80.0)
            .child(ButtonGroup(ButtonGroupProps {
                id: String::new(),
                orientation: None,
                children: Some(packed),
                base: None,
                ..Default::default()
            }));
        let mut layout = LayoutTree::new();
        let _ = layout.build_and_layout(&mut root, 600.0, 80.0, [-1.0, -1.0]);
        let group = &root.children[0];
        let segs = segments(group);
        assert_eq!(segs.len(), 3, "VStack pack must unpack to 3 buttons");
        let a = segs[0];
        let b = segs[1];
        let c = segs[2];
        let seam_ab = b.layout_bounds[0] - (a.layout_bounds[0] + a.layout_bounds[2]);
        let seam_bc = c.layout_bounds[0] - (b.layout_bounds[0] + b.layout_bounds[2]);
        assert!(
            seam_ab > 0.5 && seam_ab < 2.0,
            "RSX Copy|Paste seam should be one 1px strip, got {seam_ab}"
        );
        assert!(
            (seam_bc - seam_ab).abs() < 0.25,
            "RSX Paste|Cut seam must match Copy|Paste, got ab={seam_ab} bc={seam_bc}"
        );
        assert!(segs.iter().all(|s| s.style.border_width < 0.01));
    });
}

#[test]
fn three_outline_buttons_vertical_seams_overlap() {
    use crate::core::reactivity::create_root;
    use crate::core::tree::LayoutTree;

    let (_handle, _) = create_root(|| {
        let mut root = Element::new().w(200.0).h(200.0).child(Element::from(
            button_group()
                .orientation("vertical")
                .child(button("t").variant("outline").text("Top"))
                .child(
                    Element::from(button("m").variant("outline").text("Middle"))
                        .bg([0.8, 0.2, 0.2, 1.0]),
                )
                .child(button("b").variant("outline").text("Bottom")),
        ));
        let mut layout = LayoutTree::new();
        let result = layout.build_and_layout(&mut root, 200.0, 200.0, [-1.0, -1.0]);
        let group = &root.children[0];
        let segs = segments(group);
        assert_eq!(segs.len(), 3);
        let a = segs[0];
        let b = segs[1];
        let c = segs[2];
        let seam_ab = b.layout_bounds[1] - (a.layout_bounds[1] + a.layout_bounds[3]);
        let seam_bc = c.layout_bounds[1] - (b.layout_bounds[1] + b.layout_bounds[3]);
        assert!(
            seam_ab > 0.5 && seam_ab < 2.0,
            "Top|Middle seam should be one 1px strip, got {seam_ab}"
        );
        assert!(
            (seam_bc - seam_ab).abs() < 0.25,
            "Middle|Bottom seam must match Top|Middle, got ab={seam_ab} bc={seam_bc}"
        );
        assert!(segs.iter().all(|s| s.style.border_width < 0.01));
        assert_eq!(button_group_paint_nudge(b), [0.0, 0.0]);
        assert_eq!(button_group_paint_nudge(c), [0.0, 0.0]);
        let b_bounds = b.layout_bounds;
        assert!(
            result.node_paints.iter().any(|paint| {
                (paint.bounds[0] - b_bounds[0]).abs() < 0.01
                    && (paint.bounds[1] - b_bounds[1]).abs() < 0.01
                    && (paint.bounds[2] - b_bounds[2]).abs() < 0.01
                    && (paint.bounds[3] - b_bounds[3]).abs() < 0.01
            }),
            "retained paint bounds stay on the layout box; paints={:?}, layout={b_bounds:?}",
            result
                .node_paints
                .iter()
                .map(|paint| paint.bounds)
                .collect::<Vec<_>>()
        );
        let group_bounds = group.layout_bounds;
        assert!(
            result.instances.iter().any(|instance| {
                (instance.rect_pos[0] - group_bounds[0]).abs() < 0.01
                    && (instance.rect_pos[1] - group_bounds[1]).abs() < 0.01
                    && (instance.rect_size[0] - group_bounds[2]).abs() < 0.01
                    && (instance.rect_size[1] - group_bounds[3]).abs() < 0.01
                    && instance.border_width > 0.5
            }),
            "the group draws the only perimeter stroke; bounds={:?} instances={:?}",
            group.layout_bounds,
            result
                .instances
                .iter()
                .map(|instance| { (instance.rect_pos, instance.rect_size, instance.border_width) })
                .collect::<Vec<_>>()
        );
        let middle_bounds = b.layout_bounds;
        assert!(
                result.instances.iter().any(|instance| {
                    (instance.rect_pos[0] - middle_bounds[0]).abs() < 0.01
                        && (instance.rect_pos[1] - middle_bounds[1]).abs() < 0.01
                        && (instance.rect_size[0] - middle_bounds[2]).abs() < 0.01
                        && (instance.rect_size[1] - middle_bounds[3]).abs() < 0.01
                        && instance.border_width < 0.01
                }),
                "the middle segment is fill only, so a pressed bg cannot recolor one seam; instances={:?}",
                result
                    .instances
                    .iter()
                    .map(|instance| (instance.rect_pos, instance.rect_size, instance.border_width))
                    .collect::<Vec<_>>()
            );
        assert_eq!(c.style.z_index, 0);
        let group_bottom = group.layout_bounds[1] + group.layout_bounds[3];
        let last_bottom = c.layout_bounds[1] + c.layout_bounds[3];
        assert!(
                last_bottom <= group_bottom + 0.5,
                "vertical group must contain Bottom (got last={last_bottom} group={group_bottom}); negative margin used to collapse height and overlay the next Gallery label"
            );
    });
}

#[test]
fn joined_hover_at_125_dpi_replays_the_seam_the_scissor_eats() {
    use crate::app::damage::damage_to_scissor_physical;
    use crate::app::draw_cull::{
        last_present_visual_slots, node_present_replays_shared_seam, node_present_reset_rect,
    };
    use crate::core::reactivity::create_root;
    use crate::core::tree::LayoutTree;

    let (_handle, _) = create_root(|| {
        let mut root = Element::new().w(200.0).h(200.0).child(Element::from(
            button_group()
                .orientation("vertical")
                .child(button("t").variant("outline").text("Top"))
                .child(button("m").variant("outline").text("Middle"))
                .child(button("b").variant("outline").text("Bottom")),
        ));
        let mut layout = LayoutTree::new();
        let result = layout.build_and_layout(&mut root, 200.0, 200.0, [-1.0, -1.0]);
        let group = &root.children[0];
        let segs = segments(group);
        let top = segs[0];
        let middle = segs[1];
        let seam = &group.children[1];
        let key = u64::from(top.layout_node_id().expect("top layout node"));
        let bounds = top.layout_bounds;
        let seam_box = seam.layout_bounds;
        let slots = last_present_visual_slots(
            &result.node_paints,
            &result.instances,
            &[],
            &result.text_nodes,
            &[],
        );
        let prev = slots.iter().find(|(k, _)| *k == key).map(|(_, b)| *b);
        let scale = 1.25_f32;
        let wipe = node_present_reset_rect(
            key,
            bounds,
            &result.node_paints,
            &result.instances,
            &[],
            &[],
            0.0,
            scale,
            prev,
        )
        .expect("joined hover has a dest-reset");
        let logical = [
            wipe[0] / scale,
            wipe[1] / scale,
            wipe[2] / scale,
            wipe[3] / scale,
        ];
        let (x, y, w, h) =
            damage_to_scissor_physical(logical, 0.0, scale, 800, 600).expect("scissor");
        let hole = [
            x as f32 / scale,
            y as f32 / scale,
            w as f32 / scale,
            h as f32 / scale,
        ];
        let overlap_y =
            (hole[1] + hole[3]).min(seam_box[1] + seam_box[3]) - hole[1].max(seam_box[1]);
        assert!(
                overlap_y > 0.25,
                "125% DPI ceil must eat the Top|Middle seam (that is the bug)\n hole={hole:?} seam={seam_box:?} logical={logical:?}"
            );
        let seam_key = u64::from(seam.layout_node_id().expect("seam layout node"));
        let seam_paint = result
            .node_paints
            .iter()
            .find(|p| p.key == seam_key)
            .expect("seam slot");
        assert!(
            node_present_replays_shared_seam(key, bounds, seam_paint, hole, &result.node_paints,),
            "the eaten seam must be drawn again after the hover fill"
        );
        let middle_key = u64::from(middle.layout_node_id().expect("middle layout node"));
        let middle_paint = result
            .node_paints
            .iter()
            .find(|p| p.key == middle_key)
            .expect("middle slot");
        assert!(
            !node_present_replays_shared_seam(key, bounds, middle_paint, hole, &result.node_paints,),
            "the next item is not a hairline seam"
        );
        let lower = &group.children[3];
        let lower_key = u64::from(lower.layout_node_id().expect("lower seam"));
        let lower_paint = result
            .node_paints
            .iter()
            .find(|p| p.key == lower_key)
            .expect("lower seam slot");
        assert!(
            !node_present_replays_shared_seam(key, bounds, lower_paint, hole, &result.node_paints,),
            "Middle|Bottom must stay out of Top's hover hole"
        );
    });
}

#[test]
fn paint_nudge_parses_axis() {
    let mut el = Element::new();
    assert_eq!(button_group_paint_nudge(&el), [0.0, 0.0]);
    el.set_attr(ATTR_PAINT_NUDGE, "x");
    assert_eq!(button_group_paint_nudge(&el), [-1.0, 0.0]);
    el.set_attr(ATTR_PAINT_NUDGE, "y");
    assert_eq!(button_group_paint_nudge(&el), [0.0, -1.0]);
}

#[test]
fn join_chrome_survives_uniform_border_reset() {
    let mut kid: Element = button("b").variant("outline").text("B").into();
    kid.set_attr(ATTR_GROUP_EDGE, EDGE_END);
    kid.set_attr(ATTR_GROUP_ORIENTATION, "horizontal");
    kid.set_attr(ATTR_PAINT_NUDGE, "x");
    apply_button_group_join_chrome(&mut kid);
    assert_eq!(
        kid.attrs.get(ATTR_PAINT_NUDGE).map(String::as_str),
        Some("x"),
        "hover chrome must preserve paint nudge"
    );
    kid.style.set_uniform_border_width(1.0);
    apply_button_group_join_chrome(&mut kid);
    assert_eq!(
        kid.attrs.get(ATTR_PAINT_NUDGE).map(String::as_str),
        Some("x"),
        "join chrome must keep paint nudge after set_uniform"
    );
    assert!(
        kid.style.border_width < 0.01,
        "hover must not put the per-item stroke back"
    );
    let r = fill_radius();
    assert_eq!(kid.style.radii, [0.0, r, r, 0.0]);
}

#[test]
fn dual_surface_compiles_shape() {
    let rsx = ButtonGroup(ButtonGroupProps {
        id: String::new(),
        orientation: None,
        children: Some(ButtonGroupText(ButtonGroupTextProps {
            children: Some(Element::new().text("Label")),
            base: None,
            ..Default::default()
        })),
        base: None,
        ..Default::default()
    });
    let built: Element = button_group().text("Label").into();
    assert_eq!(
        rsx.attrs.get(ATTR_SLOT).map(String::as_str),
        built.attrs.get(ATTR_SLOT).map(String::as_str)
    );
}

#[test]
fn text_with_style_text_size() {
    let el: Element = button_group()
        .text_with(|t| {
            t.style(|s| s.text_size(16.0))
                .child(Element::new().text("Archive"))
        })
        .into();
    let text_slot = &el.children[0];
    assert_eq!(
        text_slot.attrs.get(ATTR_SLOT).map(String::as_str),
        Some(SLOT_TEXT)
    );
    assert_eq!(text_slot.text_style.size_resolved(), 16.0);
}
