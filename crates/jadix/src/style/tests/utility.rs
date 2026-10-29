use super::*;
use jadix_style_tokens::parse_class_token;

#[test]
#[cfg(feature = "scale-preview")]
fn scaled_preview_scroll_hover_keeps_its_responsive_layout() {
    crate::core::reactivity::create_root(|| {
        let mut tile = Element::new().focus_id("scaled-hover-layout");
        apply_class_list(&mut tile, "w-24 h-12 p-2 md:p-8 hover:bg-red-500");
        let mut preview = Element::new().child(tile);
        crate::core::scaled_viewport::configure_viewport(
            &mut preview,
            [390.0, 240.0],
            0.57,
            "scaled-hover-portals",
        );
        let mut root = Element::new().w(1600.0).h(900.0).child(preview);
        let mut layout = crate::layout::LayoutTree::new();
        let mut impact = LayoutStatus::Clean;
        sync_class_variants_subtree(
            &mut root,
            [-1.0; 2],
            "",
            false,
            1600.0,
            900.0,
            &mut layout,
            &mut impact,
        );
        layout.build_and_layout(&mut root, 1600.0, 900.0, [-1.0; 2]);
        let bounds = crate::app::tree_hit::find_paint_aabb_by_focus_id(
            &root,
            "scaled-hover-layout",
            [0.0; 2],
        )
        .unwrap()
        .0;
        let pointer = [bounds[0] + bounds[2] * 0.5, bounds[1] + bounds[3] * 0.5];
        impact = LayoutStatus::Clean;
        sync_class_variants_subtree(
            &mut root,
            pointer,
            "",
            false,
            1600.0,
            900.0,
            &mut layout,
            &mut impact,
        );
        let padding = root.children[0].children[0].style.padding;
        for mouse in [[-1.0; 2], pointer, [-1.0; 2]] {
            impact = LayoutStatus::Clean;
            tick_active_css_transitions_subtree(
                &mut root,
                mouse,
                "",
                false,
                1600.0,
                900.0,
                [0.0; 2],
                &mut layout,
                &mut impact,
            );
            assert_eq!(
                root.children[0].children[0].style.padding, padding,
                "scroll hover must use the preview width, not the window breakpoint"
            );
            assert!(!matches!(
                impact,
                LayoutStatus::LayoutChanged | LayoutStatus::StructureChanged
            ));
        }
    });
}

#[test]
#[cfg(feature = "product-tests")]
fn scroll_hover_keeps_button_caption_size_and_padding() {
    crate::core::reactivity::create_root(|| {
        let mut root = Element::new()
            .flex_col()
            .w(400.0)
            .h(200.0)
            .text_size(22.0)
            .child(Element::from(
                crate::components::button("scroll-caption")
                    .variant("outline")
                    .size("sm")
                    .text("separator"),
            ));
        let mut layout = crate::layout::LayoutTree::new();
        layout.build_and_layout(&mut root, 400.0, 200.0, [-1.0; 2]);
        let size = root.children[0].children[0].text_style.size;
        let padding = root.children[0].style.padding;
        for mouse in [[20.0, 10.0], [-1.0; 2], [20.0, 10.0], [-1.0; 2]] {
            let mut impact = LayoutStatus::Clean;
            tick_active_css_transitions_subtree(
                &mut root,
                mouse,
                "",
                false,
                400.0,
                200.0,
                [0.0; 2],
                &mut layout,
                &mut impact,
            );
            assert_eq!(
                root.children[0].children[0].text_style.size, size,
                "scroll hover must preserve the authored caption size"
            );
            assert_eq!(root.children[0].style.padding, padding);
            layout.build_and_layout_fast(&mut root, 400.0, 200.0, mouse, true, false);
        }
    });
}

#[test]
fn classes_joins_skips_empty_and_class_list_tuples() {
    assert_eq!(classes(["flex", "gap-2"]), "flex gap-2");
    assert_eq!(classes(["flex", "", "p-4"]), "flex p-4");
    assert_eq!(classes([Some("flex"), None, Some("p-4")]), "flex p-4");
    assert_eq!(
        crate::classes!("flex", ("hidden", false), ("p-4", true)),
        "flex p-4"
    );
    assert_eq!(crate::classes!(), "");
    assert_eq!(crate::classes!("  "), "");
}

#[test]
fn apply_p4_is_rem_and_arbitrary_px_keeps_unit() {
    let pad = |el: &Element| match crate::layout::lp_kind(el.style.to_taffy_style().padding.top) {
        crate::layout::DimKind::Length(px) => px,
        other => panic!("{other:?}"),
    };
    let mut el = Element::new();
    apply_class_list(&mut el, "p-4");
    let root = crate::layout::dimension::css_root_font_size_px();
    assert!(
        (pad(&el) - root).abs() < 0.01,
        "p-4 is 1rem = {root}px, got {}",
        pad(&el)
    );
    let mut el_px = Element::new();
    apply_class_list(&mut el_px, "underline-offset-[2px]");
    assert_eq!(el_px.text_style.underline_offset, Some(2.0));
    let mut el_em = Element::new();
    apply_class_list(&mut el_em, "underline-offset-[0.125em]");
    assert!((el_em.text_style.underline_offset.unwrap_or(0.0) - root * 0.125).abs() < 0.01);
}

#[test]
fn apply_base_and_store_variants() {
    let mut el = Element::new();
    let impact = apply_class_list(&mut el, "flex p-4 hover:bg-accent dark:bg-primary");
    assert_ne!(impact, LayoutStatus::Clean);
    let cs = el.class_set.as_ref().unwrap();
    assert!(cs.base.iter().any(|t| t == "flex"));
    assert_eq!(cs.variants.len(), 2);
}

#[test]
fn class_variant_restore_keeps_patched_display_none() {
    use taffy::style::Display;
    let mut el = Element::new();
    apply_class_list(&mut el, "hover:bg-accent");
    el.style.display = Display::None;
    patch_class_set_base_display(&mut el);
    el.attrs
        .insert("data-status".to_string(), "unanswered".to_string());
    el.layout_bounds = [8.0, 8.0, 80.0, 32.0];

    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_class_variants_subtree(
        &mut el,
        [-1.0e9, -1.0e9],
        "",
        false,
        800.0,
        600.0,
        &mut tree,
        &mut impact,
    );
    assert_eq!(el.style.display, Display::None);

    el.attrs
        .insert("data-status".to_string(), "answered".to_string());
    el.style.display = Display::None;
    patch_class_set_base_display(&mut el);
    let mut impact = LayoutStatus::Clean;
    sync_class_variants_subtree(
        &mut el,
        [-1.0e9, -1.0e9],
        "",
        false,
        800.0,
        600.0,
        &mut tree,
        &mut impact,
    );
    assert_eq!(
        el.style.display,
        Display::None,
        "data-* fingerprint change must not restore Flex over Display::None"
    );
}

#[test]
#[cfg(feature = "product-tests")]
fn dimension_writes_survive_variant_restore_without_relayout() {
    use crate::core::tree::LayoutTree;
    use taffy::style::Dimension;

    for button in [false, true] {
        let mut el = if button {
            Element::from(crate::components::button("dimension-restore").child("Go"))
        } else {
            with_class_list(Element::new().text("Go"), "hover:bg-accent")
        };
        el = el.w(68.0).h(32.0).px(12.0).gap(6.0);
        let mut root = Element::new().flex_col().child(el);
        let mut tree = LayoutTree::new();
        tree.build_and_layout(&mut root, 300.0, 200.0, [-1.0; 2]);
        for hovered in [true, false, true, false] {
            resolve_class_variants(
                &mut root.children[0],
                &VariantContext {
                    hovered,
                    ..Default::default()
                },
            );
            let el = &root.children[0];
            assert_eq!(el.style.size.width, Dimension::length(68.0));
            assert_eq!(el.style.size.height, Dimension::length(32.0));
            assert_eq!(
                el.style.padding.left,
                taffy::style::LengthPercentage::length(12.0)
            );
            assert_eq!(
                el.style.gap.width,
                taffy::style::LengthPercentage::length(6.0)
            );
            tree.update_taffy_node_style(el);
            assert!(
                !tree.is_node_dirty(tree.root_node.unwrap()),
                "color variants must preserve clean layout"
            );
            tree.build_and_layout_fast_ex(&mut root, 300.0, 200.0, [-1.0; 2], false, false, None);
            assert_eq!(
                tree.text_measure_calls.get(),
                0,
                "paint change must not remeasure text"
            );
        }
    }
}

#[test]
fn bound_dimension_updates_rest_value_without_freezing_size_variants() {
    use taffy::style::Dimension;
    let mut el = with_class_list(Element::new(), "w-8 hover:w-32").w(68.0);
    resolve_class_variants(
        &mut el,
        &VariantContext {
            hovered: true,
            ..Default::default()
        },
    );
    assert_eq!(el.style.size.width, Dimension::length(128.0));
    crate::app::reactive::apply_bound_size(&mut el, "w", 96.0);
    resolve_class_variants(
        &mut el,
        &VariantContext {
            hovered: true,
            ..Default::default()
        },
    );
    assert_eq!(el.style.size.width, Dimension::length(128.0));
    resolve_class_variants(&mut el, &VariantContext::default());
    assert_eq!(el.style.size.width, Dimension::length(96.0));
    crate::app::reactive::apply_bound_size(&mut el, "inline_size", "50%");
    resolve_class_variants(&mut el, &VariantContext::default());
    assert!(el
        .style
        .unit_specs
        .iter()
        .any(|(slot, _)| *slot == crate::style::UnitSlot::InlineSize));
}

#[test]
fn resolve_hover() {
    let mut el = Element::new();
    apply_class_list(&mut el, "bg-transparent hover:bg-accent");
    let base_bg = el.style.bg_color;
    let _ = resolve_class_variants(
        &mut el,
        &VariantContext {
            hovered: true,
            ..Default::default()
        },
    );
    assert_ne!(el.style.bg_color, base_bg);
}

#[test]
fn class_variant_color_overlay_hover_bg_last_wins() {
    let mut el = Element::new();
    apply_class_list(&mut el, "hover:bg-muted hover:bg-accent");
    let attrs = el.attrs.clone();
    let (bg, fg) = class_variant_color_overlay(
        &el,
        &VariantContext {
            hovered: true,
            attrs: Some(&attrs),
            ..Default::default()
        },
    );
    assert_eq!(bg, Some(parse_color("accent")));
    assert_eq!(fg, None);

    let (rest_bg, _) = class_variant_color_overlay(
        &el,
        &VariantContext {
            hovered: false,
            attrs: Some(&attrs),
            ..Default::default()
        },
    );
    assert_eq!(rest_bg, None, "hover:bg must not overlay while not hovered");
}

#[test]
fn hover_color_variants_redraw_after_chrome_rest_stamp() {
    let mut el = Element::new();
    apply_class_list(&mut el, "bg-transparent hover:bg-muted");
    el.layout_bounds = [0.0, 0.0, 100.0, 40.0];
    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_class_variants_subtree(
        &mut el,
        [10.0, 10.0],
        "",
        false,
        1024.0,
        800.0,
        &mut tree,
        &mut impact,
    );
    let hover_bg = parse_color("muted");
    assert_eq!(el.style.bg_color, hover_bg);
    el.style.bg_color = parse_color("transparent");
    let mut impact = LayoutStatus::Clean;
    sync_class_variants_subtree(
        &mut el,
        [10.0, 10.0],
        "",
        false,
        1024.0,
        800.0,
        &mut tree,
        &mut impact,
    );
    assert_eq!(
        el.style.bg_color, hover_bg,
        "still-hovered hover:bg must re-apply after chrome writes rest"
    );
}

#[test]
fn claiming_overlay_blocks_class_hover_on_content_below() {
    // Outline Button / DatePicker trigger hover is `hover:bg-accent`, resolved by
    // geometric class sync — not `on_hover`. A claiming overlay must occlude it.
    let mut below = Element::new();
    apply_class_list(&mut below, "bg-transparent hover:bg-accent");
    below.layout_bounds = [0.0, 0.0, 100.0, 100.0];
    below.focus_id = Some("gal-date:trigger".into());

    let mut overlay = Element::new().on_click(|| {});
    overlay.layout_bounds = [0.0, 0.0, 100.0, 100.0];
    overlay.style.z_index = 1000;

    let mut root = Element::new().child(below).child(overlay);
    root.layout_bounds = [0.0, 0.0, 100.0, 100.0];

    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_class_variants_subtree(
        &mut root,
        [50.0, 50.0],
        "",
        false,
        200.0,
        200.0,
        &mut tree,
        &mut impact,
    );
    let below = &root.children[0];
    assert_ne!(
        below
            .class_set
            .as_ref()
            .and_then(|cs| cs.last_pointer_hover),
        Some(true),
        "covered trigger must not sample :hover while a claiming overlay is on top"
    );
    assert!(
        below.style.bg_color[3] < 0.01,
        "covered trigger must keep rest (transparent) bg, got {:?}",
        below.style.bg_color
    );
}

#[test]
fn hover_variant_does_not_lock_chrome_override() {
    let mut el = Element::new();
    apply_class_list(&mut el, "hover:bg-muted");
    let attrs = el.attrs.clone();
    let _ = resolve_class_variants(
        &mut el,
        &VariantContext {
            hovered: true,
            attrs: Some(&attrs),
            ..Default::default()
        },
    );
    assert!(
        !crate::style::is_variant_style_overridden(&el.attrs, VariantStyleField::Bg),
        "hover: utilities must not stamp jadix_override_bg (chrome still owns rest)"
    );
}

#[test]
fn join_radii_survive_class_restore() {
    let mut el = Element::new();
    apply_class_list(&mut el, "rounded-md hover:bg-accent");
    el.set_attr("jadix_button_group_edge", "middle");
    el.style.radii = [0.0; 4];
    let attrs = el.attrs.clone();
    let _ = resolve_class_variants(
        &mut el,
        &VariantContext {
            hovered: true,
            attrs: Some(&attrs),
            ..Default::default()
        },
    );
    assert_eq!(
        el.style.radii, [0.0; 4],
        "joined inner corners must stay square across hover: restore"
    );

    let mut start = Element::new();
    apply_class_list(&mut start, "rounded-md hover:bg-accent");
    start.set_attr("jadix_button_group_edge", "start");
    start.style.radii = [6.0, 6.0, 0.0, 0.0];
    let attrs = start.attrs.clone();
    let _ = resolve_class_variants(
        &mut start,
        &VariantContext {
            hovered: true,
            attrs: Some(&attrs),
            ..Default::default()
        },
    );
    assert_eq!(
        start.style.radii,
        [6.0, 6.0, 0.0, 0.0],
        "joined start must keep outer rounds and inner squares"
    );
}

#[test]
fn data_state_on_wins_over_hover_bg() {
    let mut el = Element::new();
    apply_class_list(&mut el, "hover:bg-muted data-[state=on]:bg-accent");
    el.set_attr("data-state", "on");
    let attrs = el.attrs.clone();
    let _ = resolve_class_variants(
        &mut el,
        &VariantContext {
            hovered: true,
            attrs: Some(&attrs),
            ..Default::default()
        },
    );
    assert_eq!(el.style.bg_color, parse_color("accent"));
}

#[test]
fn resolve_motion_reduce_prefix() {
    crate::style::set_prefers_reduced_motion(Some(true));
    let mut el = Element::new();
    apply_class_list(&mut el, "opacity-100 motion-reduce:opacity-0");
    let _ = resolve_class_variants(&mut el, &VariantContext::default());
    assert!(
        (el.style.opacity - 0.0).abs() < 0.01,
        "motion-reduce: must apply when prefers_reduced_motion; got {}",
        el.style.opacity
    );
    crate::style::set_prefers_reduced_motion(Some(false));
    let _ = resolve_class_variants(&mut el, &VariantContext::default());
    assert!(
        (el.style.opacity
            - if cfg!(feature = "animation-base") {
                1.0
            } else {
                0.0
            })
        .abs()
            < 0.01,
        "base opacity restored when motion-reduce is off; got {}",
        el.style.opacity
    );
    crate::style::set_prefers_reduced_motion(None);
}

#[test]
fn apply_opacity_tokens() {
    let mut el = Element::new();
    let impact = apply_class_list(&mut el, "opacity-50");
    assert_eq!(impact, LayoutStatus::StyleChanged);
    assert_eq!(el.style.opacity, 0.5);

    let mut el = Element::new();
    apply_class_list(&mut el, "opacity-[0.4]");
    assert_eq!(el.style.opacity, 0.4);
}

#[test]
fn apply_filter_utilities() {
    use crate::layout::dimension::CssDim;
    use crate::style::FilterFunction;

    let mut el = Element::new();
    let impact = apply_class_list(&mut el, "blur-md");
    assert_eq!(impact, LayoutStatus::StyleChanged);
    assert_eq!(
        el.style.filter,
        Some(vec![FilterFunction::Blur(Some(CssDim::Px(12.0)))])
    );

    apply_class_list(&mut el, "grayscale");
    assert_eq!(
        el.style.filter,
        Some(vec![FilterFunction::Grayscale(Some(1.0))])
    );

    apply_class_list(&mut el, "filter-none");
    assert!(el.style.filter.is_none());

    apply_class_list(&mut el, "blur-none");
    assert!(el.style.filter.is_none());

    apply_class_list(&mut el, "blur-[1]");
    assert_eq!(
        el.style.filter,
        Some(vec![FilterFunction::Blur(Some(CssDim::Rem(1.0)))])
    );

    apply_class_list(&mut el, "blur-(--blur-xs)");
    assert_eq!(
        el.style.filter,
        Some(vec![FilterFunction::Blur(Some(CssDim::Px(4.0)))])
    );

    apply_class_list(&mut el, "backdrop-blur-lg");
    assert!((el.style.backdrop_blur - 16.0).abs() < 0.01);

    apply_class_list(&mut el, "drop-shadow-sm");
    assert!(el.style.filter.is_some());
    assert!(matches!(
        el.style.filter.as_ref().unwrap().as_slice(),
        [FilterFunction::DropShadow { .. }]
    ));
}

#[test]
fn apply_box_sizing_utilities() {
    let mut el = Element::new();
    assert_eq!(el.style.box_sizing, taffy::style::BoxSizing::BorderBox);
    let impact = apply_class_list(&mut el, "box-content");
    assert_eq!(impact, LayoutStatus::LayoutChanged);
    assert_eq!(el.style.box_sizing, taffy::style::BoxSizing::ContentBox);

    let impact = apply_class_list(&mut el, "box-border");
    assert_eq!(impact, LayoutStatus::LayoutChanged);
    assert_eq!(el.style.box_sizing, taffy::style::BoxSizing::BorderBox);

    let impact = apply_class_list(&mut el, "box-border");
    assert_eq!(impact, LayoutStatus::Clean);
}

#[test]
fn rounded_full_reapply_is_paint_only() {
    let mut el = Element::new();
    let first = apply_class_list(&mut el, "rounded-full translate-x-0");
    assert_eq!(first, LayoutStatus::StyleChanged);
    assert_eq!(el.style.radii, [-1.0, -1.0, -1.0, -1.0]);
    let again = apply_class_list(&mut el, "rounded-full translate-x-40");
    assert_eq!(
        again,
        LayoutStatus::StyleChanged,
        "re-applying rounded-full must not LayoutChanged (Go/Reset hitch)"
    );
    let same = apply_class_list(&mut el, "rounded-full translate-x-40");
    assert_eq!(same, LayoutStatus::Clean);
}

#[test]
fn apply_translate_and_truncate() {
    let mut el = Element::new();
    apply_class_list(&mut el, "translate-x-4 translate-y-2");
    assert_eq!(el.style.translate, [16.0, 8.0]);

    let mut el = Element::new().text("long");
    apply_class_list(&mut el, "truncate");
    assert_eq!(el.text_style.overflow, crate::style::TextOverflow::Ellipsis);
    assert_eq!(el.style.overflow.x, taffy::style::Overflow::Hidden);
    assert_eq!(el.text_style.white_space, crate::style::WhiteSpace::Nowrap);

    apply_class_list(&mut el, "text-overflow-newline");
    assert_eq!(el.text_style.overflow, crate::style::TextOverflow::Newline);
    assert_eq!(el.text_style.white_space, crate::style::WhiteSpace::Normal);
    assert_eq!(el.style.overflow.x, taffy::style::Overflow::Visible);

    apply_class_list(&mut el, "text-overflow-clip");
    assert_eq!(el.text_style.overflow, crate::style::TextOverflow::Clip);
    assert_eq!(el.text_style.white_space, crate::style::WhiteSpace::Nowrap);
    assert_eq!(el.style.overflow.x, taffy::style::Overflow::Hidden);

    apply_class_list(&mut el, "text-overflow-hidden");
    assert_eq!(el.text_style.overflow, crate::style::TextOverflow::Hidden);
    assert_eq!(el.text_style.white_space, crate::style::WhiteSpace::Nowrap);
    assert_eq!(el.style.overflow.x, taffy::style::Overflow::Hidden);
}

#[test]
fn apply_text_align_utilities() {
    let mut el = Element::new();
    apply_class_list(&mut el, "text-center");
    assert_eq!(
        el.text_style.align_resolved(),
        crate::style::TextAlign::Center
    );
    assert_eq!(
        el.attrs.get("text_align").map(String::as_str),
        Some("center")
    );

    apply_class_list(&mut el, "text-start");
    assert_eq!(
        el.text_style.align_resolved(),
        crate::style::TextAlign::Start
    );

    apply_class_list(&mut el, "text-justify");
    assert_eq!(
        el.text_style.align_resolved(),
        crate::style::TextAlign::Justify
    );

    apply_class_list(&mut el, "text-end");
    assert_eq!(el.text_style.align_resolved(), crate::style::TextAlign::End);
}

#[test]
fn parse_stacked() {
    let t = parse_class_token("dark:hover:bg-muted");
    assert_eq!(t.utility, "bg-muted");
    assert_eq!(t.variants.len(), 2);
}
