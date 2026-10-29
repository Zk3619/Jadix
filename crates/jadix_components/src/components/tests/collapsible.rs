use super::*;
use crate::core::reactivity::create_root;

#[test]
fn scope_sets_slot_and_template() {
    let (_o, el): (_, Element) = create_root(|| {
        collapsible("col-test")
            .default_open(true)
            .trigger(|| Element::new().text("Toggle"))
            .content(|| Element::new().text("Body"))
            .into()
    });
    assert_eq!(el.attrs.get(ATTR_SLOT).map(|s| s.as_str()), Some(SLOT_ROOT));
    assert_eq!(el.template_id.as_deref(), Some("col-test"));
}

#[test]
fn content_does_not_add_unrequested_bottom_spacing() {
    let (_owner, root): (_, Element) = create_root(|| {
        collapsible("col-spacing")
            .default_open(true)
            .content_with(|content| content.child(Element::new().text("Body")))
            .into()
    });
    let inner = find_slot(&root, SLOT_CONTENT_INNER).expect("content inner");
    assert_eq!(
        crate::layout::lp_length_px(inner.style.padding.bottom),
        Some(0.0)
    );
}

#[test]
fn builder_trigger_class_and_style() {
    let (_o, el): (_, Element) = create_root(|| {
        collapsible("col-chrome")
            .trigger_with(|t| t.class("px-2").style(|b| b.text_size(20.0)).text("Toggle"))
            .content(|| Element::new().text("Body"))
            .into()
    });
    let trigger = find_slot(&el, SLOT_TRIGGER).expect("trigger");
    fn find_size(el: &Element, want: f32) -> bool {
        el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01)
            || (el.text_style.size_resolved() - want).abs() < 0.01
            || el.children.iter().any(|c| find_size(c, want))
    }
    assert!(
        find_size(trigger, 20.0),
        "trigger .style(|b| b.text_size(20)) must land on the trigger tree"
    );
}

fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
    if el.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(slot) {
        return Some(el);
    }
    el.children.iter().find_map(|c| find_slot(c, slot))
}

fn find_icon_name<'a>(el: &'a Element, name: &str) -> Option<&'a Element> {
    if el
        .get_attr(crate::utils::ATTR_ICON_NAME)
        .map(|s| s.as_str())
        == Some(name)
    {
        return Some(el);
    }
    el.children.iter().find_map(|c| find_icon_name(c, name))
}

#[test]
fn trigger_chevron_is_named_lucide_for_square_svg_rotate() {
    let (_o, el): (_, Element) = create_root(|| {
        collapsible("col-chev")
            .trigger(|| Element::new().text("Toggle"))
            .content(|| Element::new().text("Body"))
            .into()
    });
    let icon = find_icon_name(&el, "chevron-down").expect("collapsible chevron");
    assert_eq!(
        icon.get_attr(crate::utils::ATTR_ICON_NAME)
            .map(String::as_str),
        Some("chevron-down"),
        "named SVG texture keeps ink centered under 180° rotate"
    );
    assert_eq!(icon.text_style.font_family.as_deref(), Some("lucide"));
    assert!((icon.text_style.size_resolved() - CHEVRON_PX).abs() < 0.01);
}

#[test]
fn dual_surface_compiles_shape() {
    let _b: Element = collapsible("x").default_open(false).into();
    let el = Collapsible(CollapsibleProps {
        id: "y".into(),
        open: None,
        default_open: Some(true),
        disabled: Some(false),
        exclusive: Some(false),
        group: None,
        children: None,
        base: None,
        ..Default::default()
    });
    let _: Element = el;
}

#[test]
fn closed_content_is_display_none() {
    let (_o, el): (_, Element) = create_root(|| {
        collapsible("col-hidden")
            .default_open(false)
            .trigger(|| Element::new().text("Toggle"))
            .content(|| Element::new().text("Body"))
            .into()
    });
    let panel = find_slot(&el, SLOT_CONTENT).expect("content");
    assert_eq!(
        panel.style.display,
        Display::None,
        "closed panel matches CSS hidden — not a 0×0 flex sibling"
    );
}

#[test]
fn open_content_placeholder_stays_in_flow() {
    let (_o, el): (_, Element) = create_root(|| {
        collapsible("col-open")
            .default_open(true)
            .trigger(|| Element::new().text("Toggle"))
            .content(|| Element::new().text("Body"))
            .into()
    });
    let panel = find_slot(&el, SLOT_CONTENT).expect("content");
    assert_ne!(
        panel.style.display,
        Display::None,
        "open panel must stay in-flow so height animation can run"
    );
}

#[test]
fn exclusive_group_keeps_one_open() {
    let (_o, ()) = create_root(|| {
        let a = use_signal("col_excl_a", || true);
        let b = use_signal("col_excl_b", || true);
        let _: Element = collapsible("col-excl-a")
            .open(a)
            .exclusive(true)
            .group("col-excl-g")
            .default_open(true)
            .trigger(|| Element::new().text("A"))
            .content(|| Element::new().text("a"))
            .into();
        let _: Element = collapsible("col-excl-b")
            .open(b)
            .exclusive(true)
            .group("col-excl-g")
            .default_open(true)
            .trigger(|| Element::new().text("B"))
            .content(|| Element::new().text("b"))
            .into();
        assert!(
            a.peek() ^ b.peek(),
            "first default-open panel should occupy the exclusive group"
        );
        a.set(true);
        assert!(a.peek(), "forcing A open");
        assert!(!b.peek(), "B must close when A claims the group");
        b.set(true);
        assert!(b.peek());
        assert!(!a.peek());
    });
}

#[test]
fn enter_anim_seeds_after_measure_not_on_empty_bounds() {
    use crate::layout::layout_tree::LayoutTree;
    let (_o, ()) = create_root(|| {
        let p = use_collapse_panel_presence("col_enter_measure");
        sync_collapse_panel_presence(&p, false);
        sync_collapse_panel_presence(&p, true);
        assert!(p.needs_enter());
        let mut el = Element::new();
        el.layout_bounds = [0.0, 0.0, 100.0, 0.0];
        let mut tree = LayoutTree::new();
        let _ = apply_collapse_panel_motion(&mut el, &mut tree, &p);
        assert!(
            p.needs_enter(),
            "must keep needs_enter until height is known"
        );
        assert!(
            !el.style
                .animations
                .iter()
                .any(|a| a.name == CollapseKeyframes::COLLAPSIBLE.down),
            "must not seed down with 0 height"
        );
        el.layout_bounds = [0.0, 0.0, 100.0, 64.0];
        let _ = apply_collapse_panel_motion(&mut el, &mut tree, &p);
        assert!(!p.needs_enter());
        assert!(
            el.style
                .animations
                .iter()
                .any(|a| a.name == CollapseKeyframes::COLLAPSIBLE.down),
            "first real expand must run collapsible-down"
        );
    });
}
