use super::*;

#[test]
fn devtools_and_text_edit_foundations_do_not_import_product_components() {
    let source_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut sources: Vec<_> = std::fs::read_dir(source_root.join("app/devtools"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "rs"))
        .collect();
    sources.push(source_root.join("utils/text_edit/build.rs"));
    let forbidden = [
        ["components", "::"].concat(),
        ["jadix", "_components"].concat(),
    ];
    for path in sources {
        let source = std::fs::read_to_string(&path).unwrap();
        for dependency in &forbidden {
            assert!(
                !source.contains(dependency),
                "{} depends on {}",
                path.display(),
                dependency
            );
        }
    }
}

#[test]
fn inspector_scroll_chrome_uses_foundation_parts_for_both_axes() {
    let (_owner, _) = crate::core::reactivity::create_root(|| {
        let area: Element = scroll_area("devtools-foundation-scroll")
            .w(200.0)
            .h(120.0)
            .overflow_x("auto")
            .overflow_y("auto")
            .scrollbar_visibility("always")
            .child(Element::new().w(400.0).h(480.0))
            .into();
        assert_eq!(area.children.len(), 3);
        assert_eq!(
            area.children[0].focus_id.as_deref(),
            Some("scroll:devtools-foundation-scroll")
        );
        for (index, axis) in [(1, "y"), (2, "x")] {
            let track = &area.children[index];
            assert_eq!(
                track.focus_id.as_deref(),
                Some(format!("scrollbar-track:devtools-foundation-scroll:{axis}").as_str())
            );
            assert_eq!(track.children.len(), 1);
            assert_eq!(
                track.children[0].focus_id.as_deref(),
                Some(format!("scrollbar-thumb:devtools-foundation-scroll:{axis}").as_str())
            );
            assert_eq!(
                crate::core::scrollbar::auto_scrollbar_live_paint_alpha(track),
                None
            );
        }
    });
}

#[test]
fn tooltip_panel_hugs_its_caption_in_a_secondary_viewport() {
    use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
    use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
    use std::cell::RefCell;
    use std::rc::Rc;

    reset_registered_anchors();
    clear_test_reconcile_root();
    crate::app::portal_overlay::clear_overlay_stack();
    let live = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&live));
    let (_owner, element) = crate::core::reactivity::create_root(|| {
        let tool: Element = button("tooltip-size-test")
            .size("icon-sm")
            .tooltip("Toggle device toolbar")
            .into();
        Element::new().w(800.0).h(600.0).child(tool)
    });
    *live.borrow_mut() = element;
    drain_deferred_view_remounts();
    let hover = crate::core::tree::find_by_template_id(&live.borrow(), "tooltip-size-test")
        .unwrap()
        .on_hover
        .clone()
        .unwrap();
    (hover.borrow_mut())(true);
    drain_deferred_view_remounts();
    let mut tree = crate::core::tree::LayoutTree::new();
    tree.build_and_layout(&mut live.borrow_mut(), 800.0, 600.0, [10.0, 10.0]);
    let root = live.borrow();
    let panel = crate::core::tree::find_by_attr(
        &root,
        crate::utils::popup::ATTR_FLOATING,
        "tooltip-size-test:tooltip",
    )
    .expect("mounted tooltip panel");
    assert!(
        panel.layout_bounds[2] > 80.0 && panel.layout_bounds[2] < 400.0,
        "tooltip must hug its caption, not stretch to the viewport: {:?}",
        panel.layout_bounds
    );
    drop(root);
    clear_test_reconcile_root();
    crate::app::portal_overlay::clear_overlay_stack();
    reset_registered_anchors();
}

#[test]
fn leaving_previous_tooltip_keeps_the_next_tooltip_open() {
    use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
    use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
    use std::cell::RefCell;
    use std::rc::Rc;
    reset_registered_anchors();
    clear_test_reconcile_root();
    crate::app::portal_overlay::clear_overlay_stack();
    let live = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&live));
    let (_owner, element) = crate::core::reactivity::create_root(|| {
        let first: Element = button("tooltip-handoff-first")
            .tooltip("Select an element (Ctrl+Shift+C)")
            .into();
        let second: Element = button("tooltip-handoff-second")
            .tooltip("Toggle device toolbar")
            .into();
        Element::new().children([first, second])
    });
    *live.borrow_mut() = element;
    drain_deferred_view_remounts();
    let first = live.borrow().children[0].on_hover.clone().unwrap();
    let second = live.borrow().children[1].on_hover.clone().unwrap();
    (first.borrow_mut())(true);
    drain_deferred_view_remounts();
    (second.borrow_mut())(true);
    drain_deferred_view_remounts();
    (first.borrow_mut())(false);
    for _ in 0..20 {
        crate::style::tick_animations(0.05);
        crate::utils::floating_motion::tick_floating_presence();
        drain_deferred_view_remounts();
    }
    assert!(
        crate::core::reactivity::use_signal("tooltip-handoff-second:tooltip:open", || false).peek()
    );
    clear_test_reconcile_root();
    crate::app::portal_overlay::clear_overlay_stack();
    reset_registered_anchors();
}

#[test]
fn tooltips_use_foundation_portals_and_preserve_icon_actions() {
    crate::core::reactivity::create_root(|| {
        let clicked = crate::core::reactivity::use_signal("tooltip-clicked", || false);
        let element: Element = button("tooltip-test")
            .size("icon-sm")
            .tooltip("Toggle device toolbar")
            .variant("ghost")
            .child(build_icon_text_node("icon"))
            .on_click(move || clicked.set(true))
            .into();
        assert!(element.attrs.contains_key("aria-label"));
        assert!(
            crate::core::tree::find_by_template_id(&element, "tooltip-test:tooltip__view")
                .is_some()
        );
        let open = crate::core::reactivity::use_signal("tooltip-test:tooltip:open", || false);
        let hover = element.on_hover.as_ref().unwrap();
        (hover.borrow_mut())(true);
        assert!(open.peek());
        (element.on_click.as_ref().unwrap().borrow_mut())();
        assert!(clicked.peek());
        (hover.borrow_mut())(false);
        assert!(!open.peek());
    });
}

#[test]
fn select_uses_retained_framework_overlay_instead_of_inline_menu() {
    use crate::app::reactive::{registered_anchor_count, reset_registered_anchors};
    use crate::core::reactivity::{create_root, use_signal};

    reset_registered_anchors();
    let before = registered_anchor_count();
    let (_owner, element) = create_root(|| {
        let value = use_signal("devtools-select-framework-value", || {
            "responsive".to_string()
        });
        let element: Element = select("devtools-select-framework")
            .value_signal(value)
            .options(vec![("responsive".into(), "Responsive".into())])
            .into();
        assert_eq!(registered_anchor_count(), before + 1);
        element
    });
    fn find<'a>(element: &'a Element, id: &str) -> Option<&'a Element> {
        if element.template_id.as_deref() == Some(id) {
            return Some(element);
        }
        element.children.iter().find_map(|child| find(child, id))
    }
    assert!(find(&element, "devtools-select-framework:trigger").is_some());
    assert!(find(&element, "devtools-select-framework__view").is_some());
    assert!(find(&element, "devtools-select-framework:menu").is_none());
    reset_registered_anchors();
}

#[test]
fn tool_button_tooltip_does_not_shift_icon_center() {
    crate::core::reactivity::create_root(|| {
        let icon = inspector_icon("x", "×")
            .template_id("inspector-test-icon")
            .text_size(16.0);
        let tool: Element = button("centered-tooltip-icon")
            .size("icon-sm")
            .tooltip("Close DevTools")
            .child(icon)
            .into();
        let mut root = Element::new().w(200.0).h(100.0).child(tool);
        let mut tree = crate::core::tree::LayoutTree::new();
        tree.build_and_layout(&mut root, 200.0, 100.0, [-1.0; 2]);
        let button = &root.children[0];
        let icon = crate::core::tree::find_in_tree(button, |node| {
            node.template_id.as_deref() == Some("inspector-test-icon")
        })
        .unwrap();
        for axis in 0..2 {
            let center =
                |node: &Element| node.layout_bounds[axis] + node.layout_bounds[axis + 2] * 0.5;
            assert!(
                (center(icon) - center(button)).abs() < 0.1,
                "button={:?}, icon={:?}",
                button.layout_bounds,
                icon.layout_bounds
            );
        }
    });
}

#[test]
fn local_button_caption_stays_centered_inside_padding() {
    crate::core::reactivity::create_root(|| {
        let mut root = crate::primitives::vstack()
            .child(button("centered-caption").w(80.0).text("EN"))
            .into();
        let mut tree = crate::core::tree::LayoutTree::new();
        let result = tree.build_and_layout(&mut root, 200.0, 100.0, [-1.0; 2]);
        let button = &root.children[0];
        let caption = result
            .text_nodes
            .iter()
            .find(|text| text.text == "EN")
            .unwrap();
        let [left, _, width, _] = button.layout_bounds;
        assert!(
            caption.x >= left + 8.0,
            "caption must respect button padding"
        );
        assert!((caption.x + caption.width / 2.0 - left - width / 2.0).abs() < 1.0);
    });
}

#[test]
fn local_button_variant_has_one_hover_palette_and_retains_keyboard_action() {
    crate::core::reactivity::create_root(|| {
        let element: Element = button("tool-ghost").variant("ghost").text("Pick").into();
        let classes = element.class_set.as_ref().unwrap();
        assert!(classes
            .variants
            .iter()
            .all(|(_, utility)| !utility.contains("primary")));
        assert_eq!(
            element.text_style.align,
            Some(crate::style::TextAlign::Center)
        );
        assert!(element.on_key_down.is_some());
        assert_eq!(element.template_id.as_deref(), Some("tool-ghost"));
        let disabled: Element = button("tool-disabled")
            .disabled(true)
            .on_click(|| panic!("disabled action"))
            .into();
        assert!(disabled.on_click.is_none());
        assert!(!disabled.focusable);
    });
}
