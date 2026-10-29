use super::*;

#[test]
fn required_click_keeps_selection() {
    assert_eq!(single_select_after_click("list", "list", true), "list");
    assert_eq!(single_select_after_click("list", "grid", true), "grid");
    assert_eq!(single_select_after_click("list", "list", false), "");
}

#[test]
fn coerce_required_falls_back() {
    let vals = vec!["list".into(), "grid".into()];
    assert_eq!(coerce_required_single("list", &vals, "grid"), None);
    assert_eq!(
        coerce_required_single("", &vals, "grid"),
        Some("grid".into())
    );
    assert_eq!(
        coerce_required_single("nope", &vals, ""),
        Some("list".into())
    );
}

#[test]
fn multi_select_toggles_independently() {
    let cur = vec!["bold".to_string()];
    assert_eq!(
        multi_select_after_click(&cur, "italic"),
        vec!["bold".to_string(), "italic".to_string()]
    );
    assert_eq!(multi_select_after_click(&cur, "bold"), Vec::<String>::new());
}

#[test]
fn spacing_zero_is_joined_layout() {
    assert_eq!(parse_toggle_group_spacing(0), 0);
    assert_eq!(parse_toggle_group_spacing("joined"), 0);
    assert_eq!(parse_toggle_group_spacing("default"), DEFAULT_SPACING);
    assert!(ToggleGroupLayout::from("joined").is_joined());
}

#[test]
fn type_multiple_parses() {
    assert!(parse_multiple_type(Some("multiple"), false));
    assert!(!parse_multiple_type(Some("single"), true));
    assert!(parse_multiple_type(None, true));
}

#[test]
fn joined_marks_button_group_edges() {
    let el: Element = toggle_group("tg-join")
        .spacing(0)
        .variant("outline")
        .options(vec![("a", "A"), ("b", "B"), ("c", "C")])
        .into();
    assert_eq!(el.attrs.get(ATTR_SLOT).map(String::as_str), Some(SLOT_ROOT));
    assert_eq!(el.attrs.get("data-spacing").map(String::as_str), Some("0"));
    let segs: Vec<&Element> = el
        .children
        .iter()
        .filter(|c| c.attrs.contains_key("jadix_button_group_edge"))
        .collect();
    assert_eq!(segs.len(), 3);
    assert_eq!(el.children.len(), 5, "two seam strips between three fills");
    assert_eq!(
        segs[0]
            .attrs
            .get("jadix_button_group_edge")
            .map(String::as_str),
        Some("start")
    );
    assert_eq!(
        segs[1]
            .attrs
            .get("jadix_button_group_edge")
            .map(String::as_str),
        Some("middle")
    );
    assert_eq!(
        segs[2]
            .attrs
            .get("jadix_button_group_edge")
            .map(String::as_str),
        Some("end")
    );
    assert!(segs.iter().all(|s| s.style.border_width < 0.01));
    assert!((el.style.border_width - 1.0).abs() < 0.01);
}

#[test]
fn joined_seams_keep_border_color_through_selection_and_hover() {
    use crate::core::reactivity::create_root;
    use crate::core::tree::LayoutTree;

    for orientation in ["horizontal", "vertical"] {
        create_root(|| {
            let group: Element = toggle_group("tg-seam-states")
                .orientation(orientation)
                .variant("outline")
                .spacing(0)
                .default_value("middle")
                .options([("top", "Top"), ("middle", "Middle"), ("bottom", "Bottom")])
                .into();
            let mut root = Element::new().w(400.0).h(240.0).child(group);
            let mut layout = LayoutTree::new();
            layout.build_and_layout(&mut root, 400.0, 240.0, [-1.0; 2]);
            for selected in ["middle", "top", "bottom", ""] {
                sync_toggle_group_pressed(
                    &mut root.children[0],
                    true,
                    false,
                    false,
                    false,
                    selected,
                    &[],
                );
                for hovered in [None, Some(0), Some(2), Some(4), None] {
                    let mouse = hovered.map_or([-1.0; 2], |index| {
                        let bounds = root.children[0].children[index].layout_bounds;
                        [bounds[0] + bounds[2] * 0.5, bounds[1] + bounds[3] * 0.5]
                    });
                    let mut impact = crate::core::tree::LayoutStatus::Clean;
                    crate::style::sync_class_variants_subtree(
                        &mut root,
                        mouse,
                        "",
                        false,
                        400.0,
                        240.0,
                        &mut layout,
                        &mut impact,
                    );
                    for index in [1, 3] {
                        let seam = &root.children[0].children[index];
                        assert_eq!(seam.style.bg_color, seam.style.border_color,
                                "{orientation}: selected={selected:?} hovered={hovered:?} seam={index} must keep its border color");
                    }
                }
            }
        });
    }
}

#[test]
fn joined_vertical_inner_corners_stay_square_on_hover() {
    use crate::core::reactivity::create_root;
    use crate::core::tree::LayoutTree;

    let (_handle, mut el): (_, Element) = create_root(|| {
        toggle_group("tg-vert-join")
            .orientation("vertical")
            .variant("outline")
            .spacing(0)
            .default_value("top")
            .options(vec![
                ("top", "Top"),
                ("middle", "Middle"),
                ("bottom", "Bottom"),
            ])
            .into()
    });
    let seg_idx: Vec<usize> = el
        .children
        .iter()
        .enumerate()
        .filter_map(|(i, c)| c.attrs.contains_key("jadix_button_group_edge").then_some(i))
        .collect();
    assert_eq!(seg_idx.len(), 3);
    let r = el.children[seg_idx[0]].style.radii[0];
    assert!(r > 0.0, "start chip keeps outer rounds");
    assert_eq!(el.children[seg_idx[0]].style.radii, [r, r, 0.0, 0.0]);
    assert_eq!(el.children[seg_idx[1]].style.radii, [0.0, 0.0, 0.0, 0.0]);
    assert_eq!(el.children[seg_idx[2]].style.radii, [0.0, 0.0, r, r]);
    assert_eq!(
        el.children[seg_idx[1]]
            .class_set
            .as_ref()
            .and_then(|cs| cs.base_style.as_ref())
            .map(|s| s.radii),
        Some([0.0, 0.0, 0.0, 0.0]),
        "join radii must be in the class restore snapshot"
    );
    assert!(el.children[seg_idx[1]].style.border_width < 0.01);
    assert!((el.style.border_width - 1.0).abs() < 0.01);

    for &i in &seg_idx {
        let join = el.children[i].style.radii;
        let attrs = el.children[i].attrs.clone();
        let _ = crate::style::resolve_class_variants(
            &mut el.children[i],
            &crate::style::VariantContext {
                hovered: true,
                attrs: Some(&attrs),
                ..Default::default()
            },
        );
        assert_eq!(
            el.children[i].style.radii, join,
            "item {i} inner corners must not flash rounded-md on hover restore"
        );
        assert!(
            el.children[i].style.border_width < 0.01,
            "item {i} class restore must not put the per-item stroke back"
        );
    }

    let mut layout = LayoutTree::new();
    let mut root = Element::new().w(200.0).h(200.0).child(el);
    let _ = layout.build_and_layout(&mut root, 200.0, 200.0, [-1.0, -1.0]);
    let group = &root.children[0];
    let segs: Vec<&Element> = group
        .children
        .iter()
        .filter(|c| c.attrs.contains_key("jadix_button_group_edge"))
        .collect();
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
        "Middle|Bottom seam must match Top|Middle (got ab={seam_ab} bc={seam_bc})"
    );
}

#[test]
fn separated_has_no_group_edge() {
    let el: Element = toggle_group("tg-sep")
        .options(vec![("a", "A"), ("b", "B")])
        .into();
    assert!(el.children[0]
        .attrs
        .get("jadix_button_group_edge")
        .is_none());
    assert_eq!(el.attrs.get("data-spacing").map(String::as_str), Some("2"));
}

#[test]
fn dual_surface_root_slot_matches() {
    let rsx = ToggleGroup(ToggleGroupProps {
        id: "tg-dual".into(),
        default_value: Some("left".into()),
        options: Some(vec![
            ("left".into(), "L".into()),
            ("right".into(), "R".into()),
        ]),
        variant: Some("outline".into()),
        base: None,
        ..Default::default()
    });
    let built: Element = toggle_group("tg-dual")
        .default_value("left")
        .variant("outline")
        .options(vec![("left", "L"), ("right", "R")])
        .into();
    assert_eq!(
        rsx.attrs.get(ATTR_SLOT).map(String::as_str),
        built.attrs.get(ATTR_SLOT).map(String::as_str)
    );
    assert_eq!(rsx.children.len(), built.children.len());
}

#[test]
fn builder_item_style() {
    use crate::core::reactivity::create_root;
    let (_owner, el): (_, Element) = create_root(|| {
        toggle_group("tg-chrome")
            .child(|| {
                toggle_group_item("left")
                    .style(|b| b.text_size(20.0))
                    .text("Left")
                    .into()
            })
            .into()
    });
    fn find_size(el: &Element, want: f32) -> bool {
        el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01)
            || (el.text_style.size_resolved() - want).abs() < 0.01
            || el.children.iter().any(|c| find_size(c, want))
    }
    assert!(
        find_size(&el, 20.0),
        "toggle_group_item .style(|b| b.text_size(20)) must land on the chip"
    );
}

#[test]
fn builder_option_style_reaches_chip() {
    use crate::core::reactivity::create_root;
    let (_owner, el): (_, Element) = create_root(|| {
        toggle_group("tg-opt-chrome")
            .options([toggle_option("left", "Left").style(|b| b.text_size(20.0))])
            .into()
    });
    fn find_size(el: &Element, want: f32) -> bool {
        el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01)
            || (el.text_style.size_resolved() - want).abs() < 0.01
            || el.children.iter().any(|c| find_size(c, want))
    }
    assert!(
        find_size(&el, 20.0),
        "toggle_option .style must land on the assembled chip"
    );
}

#[test]
fn toggle_group_item_hover_uses_shadcn_muted() {
    let mut el: Element = toggle_group("tg-hover")
        .default_value("a")
        .options(vec![("a", "A"), ("b", "B")])
        .into();
    let off_i = el
        .children
        .iter()
        .position(|c| c.attrs.get(ATTR_VALUE).map(String::as_str) == Some("b"))
        .expect("off chip");
    let on_i = el
        .children
        .iter()
        .position(|c| c.attrs.get(ATTR_VALUE).map(String::as_str) == Some("a"))
        .expect("on chip");
    assert!(el.children[off_i]
        .class_set
        .as_ref()
        .is_some_and(ClassSet::has_hover_color));
    let attrs = el.children[off_i].attrs.clone();
    let _ = crate::style::resolve_class_variants(
        &mut el.children[off_i],
        &crate::style::VariantContext {
            hovered: true,
            attrs: Some(&attrs),
            ..Default::default()
        },
    );
    assert_eq!(el.children[off_i].style.bg_color, parse_color("muted"));
    let mut off_fg = None;
    fn child_fg(el: &Element, acc: &mut Option<[u8; 4]>) {
        if el.text.is_some() {
            *acc = el.text_style.color;
        }
        for c in &el.children {
            child_fg(c, acc);
        }
    }
    child_fg(&el.children[off_i], &mut off_fg);
    assert_eq!(
        off_fg,
        Some(parse_text_color("foreground")),
        "off-chip hover keeps foreground, matching observed ShadCN ghost Button"
    );

    let attrs = el.children[on_i].attrs.clone();
    let _ = crate::style::resolve_class_variants(
        &mut el.children[on_i],
        &crate::style::VariantContext {
            hovered: true,
            attrs: Some(&attrs),
            ..Default::default()
        },
    );
    assert_eq!(el.children[on_i].style.bg_color, parse_color("accent"));
}

#[test]
fn lucide_icon_child_keeps_font_family() {
    let mut icon = Element::new().text("\u{ea0e}").font_family("lucide");
    icon.set_attr(crate::utils::ATTR_ICON_NAME, "heart");
    let nodes = nodes_from_label_or_children("", vec![icon], [1, 2, 3, 255], 14.0);
    assert_eq!(nodes.len(), 1);
    assert_eq!(
        nodes[0].text_style.font_family.as_deref(),
        Some("lucide"),
        "PUA glyph must keep the lucide face"
    );
    assert_eq!(
        nodes[0]
            .attrs
            .get(crate::utils::ATTR_ICON_NAME)
            .map(String::as_str),
        Some("heart")
    );
    assert_eq!(nodes[0].text_style.color, Some([1, 2, 3, 255]));

    let plain = Element::new().text("Hello");
    let nodes = nodes_from_label_or_children("", vec![plain], [0, 0, 0, 255], 14.0);
    assert_eq!(nodes[0].text.as_deref(), Some("Hello"));
    assert!(
        nodes[0].text_style.font_family.is_none(),
        "plain caption still uses the chip typeface"
    );
}

#[test]
fn icon_only_item_is_square_like_icon_button() {
    use taffy::style::Dimension;

    let el: Element = toggle_group("tg-icon-sq")
        .size("sm")
        .variant("outline")
        .child(|| {
            let mut icon = Element::new().text("\u{ea0e}").font_family("lucide");
            icon.set_attr(crate::utils::ATTR_ICON_NAME, "heart");
            toggle_group_item("heart").child(icon).into()
        })
        .into();
    let chip = &el.children[0];
    let side = ToggleGroupSize::Sm.min_px();
    assert_eq!(chip.style.size.width, Dimension::length(side));
    assert_eq!(chip.style.size.height, Dimension::length(side));
    assert_eq!(chip.style.padding.left, chip.style.padding.right);
    assert_eq!(chip.style.padding.top, chip.style.padding.bottom);
    assert_eq!(chip.style.padding.left, chip.style.padding.top);

    let text: Element = toggle_group("tg-text-rect")
        .size("sm")
        .item("a", "snappy")
        .into();
    assert!(
        text.children[0].style.size.width == Dimension::auto(),
        "caption chips stay content-width, not a forced square"
    );
    assert_eq!(text.children[0].style.size.height, Dimension::length(side));
}
