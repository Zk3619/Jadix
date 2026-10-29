use super::*;

#[test]
fn tabs_scope_wires_trigger_for_and_selected() {
    let value = use_signal("tabs_test_value", || "account".to_string());
    let el = tabs_scope(
        "settings",
        Some(value),
        "account",
        "horizontal",
        false,
        || {
            let list = TabsList(TabsListProps {
                id: String::new(),
                variant: None,
                value: None,
                default_value: None,
                options: None,
                children: Some({
                    let mut frag = Element::new();
                    frag.set_attr("jadix_fragment", "1");
                    frag.children(vec![
                        TabsTrigger(TabsTriggerProps {
                            value: Some("account".into()),
                            disabled: None,
                            children: Some(Element::new().text("Account")),
                            base: None,
                        }),
                        TabsTrigger(TabsTriggerProps {
                            value: Some("password".into()),
                            disabled: None,
                            children: Some(Element::new().text("Password")),
                            base: None,
                        }),
                    ])
                }),
                base: None,
            });
            let mut root = Element::new();
            root.set_attr("jadix_fragment", "1");
            root.children(vec![list])
        },
    );
    assert_eq!(el.attrs.get(ATTR_SLOT).map(|s| s.as_str()), Some(SLOT_TABS));
    let list = el
        .children
        .iter()
        .find(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_LIST))
        .expect("list");
    let triggers: Vec<_> = list
        .children
        .iter()
        .filter(|c| is_tabs_trigger(c))
        .collect();
    assert_eq!(triggers.len(), 2);
    assert_eq!(triggers[0].focus_id.as_deref(), Some("settings:account"));
    assert_eq!(
        triggers[0].attrs.get(ATTR_TABS_VALUE).map(|s| s.as_str()),
        Some("account")
    );
    assert!(triggers[0].on_click.is_some());
    assert_eq!(
        triggers[0].style.box_sizing,
        taffy::style::BoxSizing::ContentBox
    );
    assert_eq!(triggers[0].text.as_deref(), Some("Account"));
    assert!(
        triggers[0].children.iter().all(|c| c.text.is_none()),
        "label on ContentBox trigger — not a shrink-wrapped caption child"
    );
    assert_eq!(
        triggers[0].text_style.line_height_scale,
        Some(TAILWIND_TEXT_SM_LEADING_SCALE),
        "chip root is text-sm leading, not leading-none"
    );
    assert_ne!(triggers[0].text_style.line_height_scale, Some(1.0));
}

#[test]
fn tabs_trigger_hover_uses_shadcn_foreground() {
    let value = use_signal("tabs_hover_value", || "account".to_string());
    let mut el = tabs_scope(
        "hover-tabs",
        Some(value),
        "account",
        "horizontal",
        false,
        || {
            let list = TabsList(TabsListProps {
                id: String::new(),
                variant: None,
                value: None,
                default_value: None,
                options: None,
                children: Some({
                    let mut frag = Element::new();
                    frag.set_attr("jadix_fragment", "1");
                    frag.children(vec![
                        TabsTrigger(TabsTriggerProps {
                            value: Some("account".into()),
                            disabled: None,
                            children: Some(Element::new().text("Account")),
                            base: None,
                        }),
                        TabsTrigger(TabsTriggerProps {
                            value: Some("password".into()),
                            disabled: None,
                            children: Some(Element::new().text("Password")),
                            base: None,
                        }),
                    ])
                }),
                base: None,
            });
            let mut root = Element::new();
            root.set_attr("jadix_fragment", "1");
            root.children(vec![list])
        },
    );
    let mut layout_tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_tabs_trigger_subtree(&mut el, &mut layout_tree, &mut impact);

    let list = el
        .children
        .iter_mut()
        .find(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_LIST))
        .expect("list");
    let password = list
        .children
        .iter_mut()
        .find(|c| c.attrs.get(ATTR_TABS_VALUE).map(|s| s.as_str()) == Some("password"))
        .expect("password trigger");
    assert!(
        password
            .class_set
            .as_ref()
            .is_some_and(ClassSet::has_hover_color),
        "TabsTrigger must carry hover:text-foreground"
    );
    let rest_fg = password.text_style.color_resolved();
    assert_eq!(rest_fg, parse_text_color("muted-foreground"));
    let attrs = password.attrs.clone();
    let _ = crate::style::resolve_class_variants(
        password,
        &crate::style::VariantContext {
            hovered: true,
            attrs: Some(&attrs),
            ..Default::default()
        },
    );
    assert_eq!(
        password.text_style.color_resolved(),
        parse_text_color("foreground"),
        "unselected trigger hover:text-foreground"
    );
    let hover_bg = parse_color("muted/30");
    let selected_bg = parse_color("background");
    let list_bg = parse_color("muted");
    assert_eq!(
        password.style.bg_color, hover_bg,
        "unselected trigger hover:bg-muted/30 (slightly darker than selected chip)"
    );
    let dist =
        |a: [f32; 4], b: [f32; 4]| (a[0] - b[0]).abs() + (a[1] - b[1]).abs() + (a[2] - b[2]).abs();
    assert!(
        dist(hover_bg, selected_bg) > 0.01,
        "hover chip must not match selected background"
    );
    assert!(
        dist(hover_bg, selected_bg) < dist(list_bg, selected_bg),
        "hover chip sits between selected background and the muted list"
    );

    apply_trigger_chrome(password, false, false);
    assert_eq!(
        password.style.bg_color,
        parse_color("transparent"),
        "chrome restamps rest; class variants must redraw hover after that"
    );
    let attrs = password.attrs.clone();
    let _ = crate::style::resolve_class_variants(
        password,
        &crate::style::VariantContext {
            hovered: true,
            attrs: Some(&attrs),
            ..Default::default()
        },
    );
    assert_eq!(
        password.style.bg_color, hover_bg,
        "hover:bg-muted/30 must re-apply after chrome restamp"
    );
    assert_eq!(
        password.text_style.color_resolved(),
        parse_text_color("foreground"),
        "hover:text-foreground must re-apply after chrome restamp"
    );

    let account = list
        .children
        .iter_mut()
        .find(|c| c.attrs.get(ATTR_TABS_VALUE).map(|s| s.as_str()) == Some("account"))
        .expect("account trigger");
    let attrs = account.attrs.clone();
    let _ = crate::style::resolve_class_variants(
        account,
        &crate::style::VariantContext {
            hovered: true,
            attrs: Some(&attrs),
            ..Default::default()
        },
    );
    assert_eq!(
        account.style.bg_color, selected_bg,
        "active trigger keeps data-[state=active]:bg-background on hover"
    );
}

#[test]
fn tabs_trigger_text_paints_in_content_box_centered() {
    let value = use_signal("tabs_center_paint_value", || "list".to_string());
    let mut el = tabs_scope(
        "center-paint",
        Some(value),
        "list",
        "horizontal",
        false,
        || {
            let list = TabsList(TabsListProps {
                id: String::new(),
                variant: None,
                value: None,
                default_value: None,
                options: None,
                children: Some({
                    let mut frag = Element::new();
                    frag.set_attr("jadix_fragment", "1");
                    frag.children(vec![TabsTrigger(TabsTriggerProps {
                        value: Some("list".into()),
                        disabled: None,
                        children: Some(Element::new().text("List")),
                        base: None,
                    })])
                }),
                base: None,
            });
            let mut root = Element::new();
            root.set_attr("jadix_fragment", "1");
            root.children(vec![list])
        },
    );
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut el, 640.0, 200.0, [-1.0, -1.0]);
    let list = el
        .children
        .iter()
        .find(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_LIST))
        .expect("list");
    let trigger = list
        .children
        .iter()
        .find(|c| is_tabs_trigger(c))
        .expect("trigger");
    let tb = trigger.layout_bounds;
    let pad_l = length_px(trigger.style.padding.left);
    let pad_r = length_px(trigger.style.padding.right);
    let pad_t = length_px(trigger.style.padding.top);
    let pad_b = length_px(trigger.style.padding.bottom);
    assert_eq!(pad_l, pad_r);
    assert_eq!(pad_t, pad_b);
    assert!(pad_l > 0.0 && pad_t > 0.0);
    assert_eq!(
        trigger.style.box_sizing,
        taffy::style::BoxSizing::ContentBox
    );
    assert_eq!(
        trigger.text_style.align_resolved(),
        crate::style::TextAlign::Center
    );
    let tn = result
        .text_nodes
        .iter()
        .find(|n| n.text == "List")
        .expect("List text node");
    // Full border box (no paint inset): Center on Wb with equal pad ≡ content-box center.
    assert!((tn.x - tb[0]).abs() < 1.0);
    assert!((tn.y - tb[1]).abs() < 1.0);
    assert!((tn.width - tb[2]).abs() < 1.5);
    assert!((tn.height - tb[3]).abs() < 1.5);
    let chip_cx = tb[0] + tb[2] * 0.5;
    let chip_cy = tb[1] + tb[3] * 0.5;
    let text_cx = tn.x + tn.width * 0.5;
    let text_cy = tn.y + tn.height * 0.5;
    assert!(
        (text_cx - chip_cx).abs() < 1.5,
        "text border-box center matches chip center x"
    );
    assert!(
        (text_cy - chip_cy).abs() < 1.5,
        "text border-box center matches chip center y"
    );
    assert!(
        tn.height > trigger.text_style.size_resolved() + pad_t + pad_b - 1.0,
        "chip taller than bare glyphs so prepare_text dy can vertical-center"
    );
}

#[test]
fn tabs_list_options_builder_still_works() {
    let el: Element = tabs_list("gal-tabs")
        .default_value("list")
        .options(vec![
            ("list".into(), "List".into()),
            ("grid".into(), "Grid".into()),
        ])
        .into();
    assert_eq!(
        el.attrs.get("jadix_tabs_list").map(|s| s.as_str()),
        Some("1")
    );
    assert!(el.children.len() >= 2);
}

#[test]
fn builder_trigger_style() {
    use crate::core::reactivity::create_root;
    let (_o, el): (_, Element) = create_root(|| {
        tabs("tab-chrome").default_value("a").child(|| {
            tabs_list_slot()
                .child(
                    tabs_trigger("a")
                        .style(|b| b.text_size(20.0))
                        .text("Account"),
                )
                .into()
        })
    });
    fn find_size(el: &Element, want: f32) -> bool {
        el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01)
            || (el.text_style.size_resolved() - want).abs() < 0.01
            || el.children.iter().any(|c| find_size(c, want))
    }
    assert!(
        find_size(&el, 20.0),
        "tabs_trigger .style(|b| b.text_size(20)) must land on the trigger tree"
    );
}

#[test]
fn tabs_content_hidden_slot_is_display_none_zero_shell() {
    let value = use_signal("tabs_hidden_slot_value", || "list".to_string());
    let el = tabs_scope(
        "hidden-slot",
        Some(value),
        "list",
        "horizontal",
        false,
        || {
            let list = TabsList(TabsListProps {
                id: String::new(),
                variant: None,
                value: None,
                default_value: None,
                options: None,
                children: Some({
                    let mut frag = Element::new();
                    frag.set_attr("jadix_fragment", "1");
                    frag.children(vec![
                        TabsTrigger(TabsTriggerProps {
                            value: Some("list".into()),
                            disabled: None,
                            children: Some(Element::new().text("List")),
                            base: None,
                        }),
                        TabsTrigger(TabsTriggerProps {
                            value: Some("grid".into()),
                            disabled: None,
                            children: Some(Element::new().text("Grid")),
                            base: None,
                        }),
                    ])
                }),
                base: None,
            });
            let list_panel = tabs_content("list", || Element::new().text("List panel"));
            let grid_panel = tabs_content("grid", || Element::new().text("Grid panel"));
            let mut root = Element::new();
            root.set_attr("jadix_fragment", "1");
            root.children(vec![list, list_panel, grid_panel])
        },
    );
    let contents: Vec<_> = el
        .children
        .iter()
        .filter(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_CONTENT))
        .collect();
    assert_eq!(contents.len(), 2);
    for panel in &contents {
        // 初始壳：0×0，避免未跑 effect 前就按 100% 撑开列高。
        assert_eq!(panel.style.size.width, taffy::style::Dimension::length(0.0));
        assert_eq!(
            panel.style.size.height,
            taffy::style::Dimension::length(0.0)
        );
    }
}

#[test]
fn tabs_chips_prefer_intrinsic_content_width_in_narrow_parent() {
    let value = use_signal("tabs_intrinsic_w_value", || "sketch".to_string());
    let labels = [
        "Sketch", "Chrome", "GitHub", "Twitter", "Material", "Block", "Circle", "Slider", "Compact",
    ];
    let scope = tabs_scope(
        "intrinsic-w",
        Some(value),
        "sketch",
        "horizontal",
        false,
        || {
            let triggers: Vec<_> = labels
                .iter()
                .map(|lab| {
                    TabsTrigger(TabsTriggerProps {
                        value: Some(lab.to_lowercase()),
                        disabled: None,
                        children: Some(Element::new().text(*lab)),
                        base: None,
                    })
                })
                .collect();
            let list = TabsList(TabsListProps {
                id: String::new(),
                variant: None,
                value: None,
                default_value: None,
                options: None,
                children: Some({
                    let mut frag = Element::new();
                    frag.set_attr("jadix_fragment", "1");
                    frag.children(triggers)
                }),
                base: None,
            });
            let mut root = Element::new();
            root.set_attr("jadix_fragment", "1");
            root.children(vec![list])
        },
    );
    // Gallery MyCard 形状：窄列 + min_w(0) + padding。
    let mut root = Element::new()
        .w(360.0)
        .h(240.0)
        .child(Element::new().w("100%").min_w(0.0).p(16.0).child(scope));
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 360.0, 240.0, [-1.0, -1.0]);

    fn collect_triggers<'a>(el: &'a Element, out: &mut Vec<&'a Element>) {
        if is_tabs_trigger(el) {
            out.push(el);
        }
        for c in &el.children {
            collect_triggers(c, out);
        }
    }
    fn find_list<'a>(el: &'a Element) -> Option<&'a Element> {
        if el.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_LIST) {
            return Some(el);
        }
        el.children.iter().find_map(find_list)
    }
    let mut triggers = Vec::new();
    collect_triggers(&root, &mut triggers);
    assert_eq!(triggers.len(), labels.len());

    let list = find_list(&root).expect("tabs list");
    let list_w = list.layout_bounds[2];
    assert!(
        list_w <= 360.0 - 16.0 * 2.0 + 1.0,
        "TabsList must fit parent content (got {list_w})"
    );
    assert_eq!(list.style.flex_wrap, taffy::style::FlexWrap::Wrap);

    let mut ys: Vec<i32> = triggers
        .iter()
        .map(|t| t.layout_bounds[1].round() as i32)
        .collect();
    ys.sort_unstable();
    ys.dedup();
    assert!(
        ys.len() >= 2,
        "nine content-sized chips in ~328px must wrap to ≥2 rows, ys={ys:?}"
    );

    for (t, lab) in triggers.iter().zip(labels.iter()) {
        let w = t.layout_bounds[2];
        eprintln!(
            "chip '{lab}' x={:.1} y={:.1} w={w:.1} h={:.1}",
            t.layout_bounds[0], t.layout_bounds[1], t.layout_bounds[3]
        );
        assert!(
            (t.style.flex_shrink - 0.0).abs() < f32::EPSILON,
            "{lab}: flex_shrink must stay 0 so content width wins"
        );
        assert!(
            w >= 40.0,
            "{lab}: chip crushed to {w} — should size to label+padding"
        );
        let tn = result
            .text_nodes
            .iter()
            .find(|n| n.text == *lab)
            .unwrap_or_else(|| panic!("missing text node for {lab}"));
        assert!(
            tn.width >= 20.0,
            "{lab}: painted text width lost ({})",
            tn.width
        );
        // Chip right edge must stay inside the list (no clip / 内容丢失).
        let chip_r = t.layout_bounds[0] + t.layout_bounds[2];
        let list_r = list.layout_bounds[0] + list.layout_bounds[2];
        assert!(
            chip_r <= list_r + 1.0,
            "{lab}: chip overflows list (chip_r={chip_r} list_r={list_r})"
        );
    }

    // Gallery symptom: all labels stacked at the same x — must not share one origin.
    let mut xs: Vec<i32> = triggers
        .iter()
        .map(|t| t.layout_bounds[0].round() as i32)
        .collect();
    xs.sort_unstable();
    let unique_x = {
        let mut u = xs.clone();
        u.dedup();
        u
    };
    assert!(
        unique_x.len() >= 3,
        "chips must spread horizontally (or wrap); got xs={xs:?} — gallery overlap bug"
    );
    for i in 1..triggers.len() {
        let a = triggers[i - 1].layout_bounds;
        let b = triggers[i].layout_bounds;
        let same_row = (a[1] - b[1]).abs() < 1.0;
        if same_row {
            assert!(
                b[0] + 0.5 >= a[0] + a[2],
                "same-row chips must not overlap: '{}'@x={}w={} vs '{}'@x={}w={}",
                labels[i - 1],
                a[0],
                a[2],
                labels[i],
                b[0],
                b[2]
            );
        }
    }
}

#[test]
fn radio_sync_must_not_nest_indicator_under_tabs_trigger() {
    // TabsTrigger reuses `radio_selected_sync` for the selected value signal but is not a
    // Radio control (no `jadix_radio_value`). Global radio sync must not inject the size-4
    // Indicator — that destroys the text leaf and piles chip labels at x≈0 in gallery.
    let value = use_signal("tabs_radio_sync_guard", || "date".to_string());
    let mut el = tabs_scope(
        "radio-guard",
        Some(value),
        "date",
        "horizontal",
        false,
        || {
            let list = TabsList(TabsListProps {
                id: String::new(),
                variant: None,
                value: None,
                default_value: None,
                options: None,
                children: Some({
                    let mut frag = Element::new();
                    frag.set_attr("jadix_fragment", "1");
                    frag.children(vec![
                        TabsTrigger(TabsTriggerProps {
                            value: Some("date".into()),
                            disabled: None,
                            children: Some(Element::new().text("Date")),
                            base: None,
                        }),
                        TabsTrigger(TabsTriggerProps {
                            value: Some("datetime".into()),
                            disabled: None,
                            children: Some(Element::new().text("DateTime")),
                            base: None,
                        }),
                        TabsTrigger(TabsTriggerProps {
                            value: Some("time".into()),
                            disabled: None,
                            children: Some(Element::new().text("Time")),
                            base: None,
                        }),
                    ])
                }),
                base: None,
            });
            let mut root = Element::new();
            root.set_attr("jadix_fragment", "1");
            root.children(vec![list])
        },
    );

    let mut layout_tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    crate::components::radio_group::sync_radio_checked_subtree(
        &mut el,
        &mut layout_tree,
        &mut impact,
    );
    sync_tabs_trigger_subtree(&mut el, &mut layout_tree, &mut impact);

    fn collect_triggers<'a>(node: &'a Element, out: &mut Vec<&'a Element>) {
        if is_tabs_trigger(node) {
            out.push(node);
        }
        for c in &node.children {
            collect_triggers(c, out);
        }
    }
    let mut triggers = Vec::new();
    collect_triggers(&el, &mut triggers);
    assert_eq!(triggers.len(), 3);
    for t in &triggers {
        assert!(
            t.children.iter().all(|c| {
                c.attrs.get("data-slot").map(|s| s.as_str()) != Some("radio-indicator")
            }),
            "TabsTrigger must stay a text leaf — radio Indicator would crush content width"
        );
        assert!(
            t.text.as_deref().is_some_and(|s| !s.is_empty()),
            "chip label must remain on the trigger element"
        );
    }

    let mut root = Element::new().w(480.0).h(80.0).child(el);
    let _ = layout_tree.build_and_layout(&mut root, 480.0, 80.0, [-1.0, -1.0]);
    let mut laid = Vec::new();
    collect_triggers(&root, &mut laid);
    let mut xs: Vec<f32> = laid.iter().map(|t| t.layout_bounds[0]).collect();
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert!(
        (xs[2] - xs[0]).abs() > 40.0,
        "after radio+tabs sync, chips must still spread (xs={xs:?})"
    );
    for w in laid.windows(2) {
        let a = w[0].layout_bounds;
        let b = w[1].layout_bounds;
        if (a[1] - b[1]).abs() < 1.0 {
            assert!(
                b[0] + 0.5 >= a[0] + a[2].min(8.0),
                "chips must not pile: {:?} vs {:?}",
                a,
                b
            );
        }
    }
}

/// Gallery pickers/color-picker: chips OK but panel empty when tabs_content stole
/// nested `reactive_component_live` `template_id` (reconcile then miss forever).
#[test]
fn tabs_content_preserves_nested_reactive_component_anchor() {
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::app::reactive::{reactive_component_live, reset_registered_anchors};
    use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
    use crate::core::reactivity::{create_effect, create_root};

    reset_registered_anchors();
    clear_test_reconcile_root();

    let root: Rc<RefCell<Element>> = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&root));

    let (handle, ()) = create_root(|| {
        let root_e = Rc::clone(&root);
        create_effect(move || {
            let el = tabs_scope("tc-nest", None, "date", "horizontal", false, || {
                tabs_content("date", || {
                    reactive_component_live("tc-nest-picker__view", (), |_| {
                        Element::new().text("Pick a date")
                    })
                })
            });
            *root_e.borrow_mut() = el;
        });
    });

    fn find_tid(el: &Element, want: &str) -> bool {
        if el.template_id.as_deref() == Some(want) {
            return true;
        }
        el.children.iter().any(|c| find_tid(c, want))
    }
    fn find_text(el: &Element, want: &str) -> bool {
        if el.text.as_deref() == Some(want) {
            return true;
        }
        el.children.iter().any(|c| find_text(c, want))
    }

    {
        let tree = root.borrow();
        assert!(
            find_tid(&tree, "tc-nest:content:date"),
            "tabs content slot must remain the outer shell"
        );
        assert!(
            find_tid(&tree, "tc-nest-picker__view"),
            "nested reactive_component template_id must not be overwritten by tabs_content"
        );
        assert!(
            find_text(&tree, "Pick a date"),
            "nested component body must mount (panel must not stay empty)"
        );
    }

    handle.dispose();
    clear_test_reconcile_root();
    reset_registered_anchors();
}

#[test]
fn tabs_sync_strips_stray_radio_indicator_from_trigger() {
    let value = use_signal("tabs_strip_indicator", || "a".to_string());
    let mut el = tabs_scope("strip-ind", Some(value), "a", "horizontal", false, || {
        TabsTrigger(TabsTriggerProps {
            value: Some("a".into()),
            disabled: None,
            children: Some(Element::new().text("Alpha")),
            base: None,
        })
    });
    fn find_trigger_mut(el: &mut Element) -> Option<&mut Element> {
        if is_tabs_trigger(el) {
            return Some(el);
        }
        for c in &mut el.children {
            if let Some(t) = find_trigger_mut(c) {
                return Some(t);
            }
        }
        None
    }
    let trigger = find_trigger_mut(&mut el).expect("trigger");
    let mut ind = Element::new().w(16.0).h(16.0);
    ind.attrs
        .insert("data-slot".into(), "radio-indicator".into());
    trigger.children.insert(0, ind);
    assert_eq!(trigger.children.len(), 1);

    let mut layout_tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_tabs_trigger_subtree(&mut el, &mut layout_tree, &mut impact);
    assert!(matches!(
        impact,
        LayoutStatus::StructureChanged | LayoutStatus::LayoutChanged
    ));
    let trigger = find_trigger_mut(&mut el).expect("trigger");
    assert!(
        trigger.children.is_empty(),
        "tabs sync must remove stray radio Indicator so text leaf measure returns"
    );
}

#[test]
fn trigger_chrome_respects_author_padding_bg_and_size() {
    use crate::style::{mark_variant_style_override, VariantStyleField};
    let mut el = Element::new();
    el.text = Some("Account".into());
    el.text_style.size = Some(18.0);
    el.style.bg_color = [1.0, 0.0, 0.0, 1.0];
    mark_variant_style_override(&mut el, VariantStyleField::Bg);
    el.style.padding.left = taffy::style::LengthPercentage::length(20.0);
    el.style.padding.right = taffy::style::LengthPercentage::length(20.0);
    el.style.padding.top = taffy::style::LengthPercentage::length(20.0);
    el.style.padding.bottom = taffy::style::LengthPercentage::length(20.0);
    mark_variant_style_override(&mut el, VariantStyleField::Pad);
    apply_trigger_chrome(&mut el, true, false);
    assert_eq!(el.style.bg_color, [1.0, 0.0, 0.0, 1.0]);
    assert!((length_px(el.style.padding.left) - 20.0).abs() < 0.01);
    assert_eq!(el.text_style.size, Some(18.0));
}

fn tabs_with_n_contents(id: &str, selected: &str, n: usize) -> Element {
    let selected = selected.to_string();
    tabs_scope(id, None, selected, "horizontal", false, move || {
        let mut kids = vec![Element::new().w(80.0).h(24.0)];
        for i in 0..n {
            let value = if i == 0 {
                "a".to_string()
            } else {
                format!("t{i}")
            };
            kids.push(tabs_content(value, || Element::new().text("panel")));
        }
        let mut frag = Element::new();
        frag.set_attr("jadix_fragment", "1");
        frag.children(kids)
    })
}

#[test]
fn unselected_content_is_display_none_and_does_not_stack_root_gap() {
    let mut many = tabs_with_n_contents("tabs-gap-many", "a", 3);
    let panels: Vec<_> = many
        .children
        .iter()
        .filter(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_CONTENT))
        .collect();
    assert_eq!(panels.len(), 3);
    assert_ne!(
        panels[0].style.display,
        Display::None,
        "selected panel stays in-flow (ShadCN gap between List and active Content)"
    );
    assert_eq!(panels[1].style.display, Display::None);
    assert_eq!(panels[2].style.display, Display::None);

    let mut one = tabs_with_n_contents("tabs-gap-one", "a", 1);
    let mut layout = LayoutTree::new();
    let _ = layout.build_and_layout(&mut one, 400.0, 400.0, [-1.0, -1.0]);
    let _ = layout.build_and_layout(&mut many, 400.0, 400.0, [-1.0, -1.0]);
    assert!(
        (many.layout_bounds[3] - one.layout_bounds[3]).abs() < 1.0,
        "hidden Contents must not add extra Root gap (one={} many={})",
        one.layout_bounds[3],
        many.layout_bounds[3]
    );
}
