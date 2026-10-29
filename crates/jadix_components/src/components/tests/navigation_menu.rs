use super::*;

#[test]
fn components_click_mounts_dropdown_in_rsx_and_builder() {
    use crate::app::portal_overlay::{
        clear_overlay_stack, try_close_top, try_dismiss_outside_press,
    };
    use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
    use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
    use crate::core::reactivity::{create_effect, create_root};
    use crate::core::tree::find_by_attr;

    for rsx in [true, false] {
        let id = if rsx {
            "nm-click-rsx"
        } else {
            "nm-click-builder"
        };
        clear_overlay_stack();
        reset_registered_anchors();
        let live = Rc::new(RefCell::new(Element::new()));
        install_test_reconcile_root(live.clone());
        let (owner, (expanded, open, picked)) = create_root(|| {
            let expanded = use_signal(&format!("{id}__navmenu_expanded"), || None::<String>);
            let open = use_signal(&format!("{id}:comp__open"), || false);
            let picked = use_signal(&format!("{id}-picked"), || false);
            let live = live.clone();
            create_effect(move || {
                *live.borrow_mut() = if rsx {
                    crate::ui! {
                        <NavigationMenu id={id}>
                            <NavigationMenuList>
                                <NavigationMenuItem value="comp">
                                    <NavigationMenuTrigger>"Components"</NavigationMenuTrigger>
                                    <NavigationMenuContent>
                                        <NavigationMenuLink id="dialog" on_click={move || picked.set(true)}>
                                            "Dialog"
                                        </NavigationMenuLink>
                                    </NavigationMenuContent>
                                </NavigationMenuItem>
                            </NavigationMenuList>
                        </NavigationMenu>
                    }
                } else {
                    navigation_menu(id)
                        .trigger(
                            "comp",
                            "Components",
                            vec![NavigationMenuLinkData::new("dialog", "Dialog", move || {
                                picked.set(true)
                            })],
                        )
                        .into()
                };
            });
            (expanded, open, picked)
        });
        drain_deferred_view_remounts();
        assert!(find_by_attr(&live.borrow(), ATTR_SLOT, SLOT_CONTENT).is_none());
        let click = find_by_attr(&live.borrow(), ATTR_SLOT, SLOT_TRIGGER)
            .expect("Components trigger must mount")
            .on_click
            .clone()
            .expect("Components click handler");
        click.borrow_mut()();
        drain_deferred_view_remounts();
        assert_eq!(expanded.peek().as_deref(), Some("comp"));
        assert!(open.peek(), "Components must open its portal signal");
        assert!(
            find_by_attr(&live.borrow(), ATTR_SLOT, SLOT_CONTENT).is_some(),
            "Components click must mount its dropdown (rsx={rsx})"
        );

        assert!(try_close_top(), "Escape must close the open dropdown");
        assert!(!open.peek());
        assert!(
            expanded.peek().is_none(),
            "Escape must clear expanded state"
        );
        click.borrow_mut()();
        assert!(open.peek(), "one click after Escape must reopen");

        // The event loop arms outside dismissal at the start of a pointer press.
        crate::utils::popup::arm_underlay_close();
        assert!(try_dismiss_outside_press());
        assert!(!open.peek());
        assert!(
            expanded.peek().is_none(),
            "outside press must clear expanded state"
        );
        click.borrow_mut()();
        assert!(open.peek(), "one click after outside dismissal must reopen");

        let select = crate::core::tree::find_in_tree(&live.borrow(), |el| {
            el.focus_id.as_deref() == Some("dialog")
        })
        .expect("dropdown link must mount")
        .on_click
        .clone()
        .unwrap();
        select.borrow_mut()();
        assert!(
            picked.peek(),
            "dropdown link must run its selection handler"
        );
        assert!(!open.peek());
        assert!(expanded.peek().is_none());
        click.borrow_mut()();
        assert!(open.peek());
        expanded.set(Some("other".into()));
        assert!(!open.peek());
        assert_eq!(expanded.peek().as_deref(), Some("other"));
        click.borrow_mut()();
        assert!(open.peek());
        click.borrow_mut()();
        assert!(!open.peek(), "second trigger click must close");
        clear_test_reconcile_root();
        drop(live);
        owner.dispose();
        clear_overlay_stack();
        reset_registered_anchors();
    }
}

#[test]
fn navigation_menu_scope_exposes_template() {
    let el = navigation_menu_scope("nm-scope", true, || {
        NavigationMenuList(NavigationMenuListProps {
            children: Some(navigation_menu_item(Some("docs".into()), || {
                NavigationMenuLink(NavigationMenuLinkProps {
                    id: "docs".into(),
                    on_click: Some(Handler::default()),
                    children: Some(Element::new().text("Docs")),
                    base: None,
                })
            })),
            base: None,
        })
    });
    assert_eq!(el.template_id.as_deref(), Some("nm-scope__navigation_menu"));
    assert_eq!(el.attrs.get(ATTR_SLOT).map(String::as_str), Some(SLOT_ROOT));
}

#[test]
fn builder_builds_compound() {
    let el: Element = navigation_menu("nm-b")
        .link("docs", "Docs", || {})
        .trigger(
            "comp",
            "Components",
            vec![NavigationMenuLinkData::new("a", "Alert", || {}).description("Modal")],
        )
        .into();
    assert!(
        el.template_id
            .as_deref()
            .is_some_and(|id| id.contains("navigation_menu")),
        "builder must route through scope"
    );
    assert_eq!(el.attrs.get(ATTR_SLOT).map(String::as_str), Some(SLOT_ROOT));
}

#[test]
fn builder_keeps_list_under_root_not_unpacked() {
    let el: Element = navigation_menu("nm-layout")
        .link("a", "A", || {})
        .link("b", "B", || {})
        .into();
    assert_eq!(el.attrs.get(ATTR_SLOT).map(String::as_str), Some(SLOT_ROOT));
    // Root children: List (+ optional Viewport). Items must live under List, not root.
    let list = el
        .children
        .iter()
        .find(|c| c.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_LIST))
        .expect("NavigationMenuList must remain a child of root");
    assert!(
        list.children.len() >= 2,
        "List should hold items; got {} children (root kids={})",
        list.children.len(),
        el.children.len()
    );
    let root_items = el
        .children
        .iter()
        .filter(|c| c.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_ITEM))
        .count();
    assert_eq!(
        root_items, 0,
        "Items must not be unpacked into root vstack (that caused vertical chaos)"
    );
    let vp = el
        .children
        .iter()
        .find(|c| c.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_VIEWPORT))
        .expect("default Viewport slot");
    // Viewport must hug / zero — w(100%) stretches the shell across NavbarCenter.
    let w = crate::layout::dim_length_px(vp.style.size.width);
    assert!(
        w.is_some_and(|px| px < 1.0),
        "Viewport width must be ~0px, got {w:?}"
    );
}
