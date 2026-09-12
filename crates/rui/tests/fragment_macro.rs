//! Smoke-tests for `<Fragment>`, `<Dynamic>`, and `IntoElementList` unpacking.

#[test]
fn fragment_sets_attr_and_children() {
    let frag = rui::ui! {
        <Fragment>
            <Text>"one"</Text>
            <Text>"two"</Text>
        </Fragment>
    };
    assert_eq!(frag.get_attr("rui_fragment").map(|s| s.as_str()), Some("1"));
    assert_eq!(frag.children.len(), 2);
    let kids = rui::core::tree::Element::unpack_fragment_children(Some(frag));
    assert_eq!(kids.len(), 2);
}

#[test]
fn product_component_multi_child_packs_as_fragment_not_vstack() {
    // ui! children slot is single-root; product components must pack as rui_fragment
    // (not VStack) so take_packed can hoist row captions without wrong flex direction.
    use rui::prelude::*;

    let el = ui! {
        <Button variant="outline">
            "Search"
            <Kbd>"⌘"</Kbd>
        </Button>
    };
    // After Button::take_packed, caption + Kbd are siblings on the button row.
    assert!(
        el.children
            .iter()
            .any(|c| c.text.as_deref() == Some("Search")),
        "Search caption must be unpacked onto the button"
    );
    assert!(
        el.children
            .iter()
            .any(|c| { c.attrs.get("data-slot").map(|s| s.as_str()) == Some("kbd") }),
        "Kbd shell must remain intact"
    );
    let search = el
        .children
        .iter()
        .find(|c| c.text.as_deref() == Some("Search"))
        .unwrap();
    assert_eq!(
        search.text_style.color,
        Some(rui::style::parse_text_color("foreground")),
        "outline caption uses foreground"
    );
}

#[test]
fn rsx_anonymous_button_keeps_id_across_rebuild() {
    use std::cell::RefCell;
    use std::rc::Rc;

    use rui::core::reactivity::{create_effect, create_root, use_signal};
    use rui::prelude::*;

    let (_owner, ids) = create_root(|| {
        let tick = use_signal("anon-btn-tick", || 0u32);
        let ids = Rc::new(RefCell::new(Vec::<Option<String>>::new()));
        let ids_eff = Rc::clone(&ids);
        create_effect(move || {
            let _ = tick.get();
            let el = ui! {
                <Button>"Allow"</Button>
            };
            ids_eff.borrow_mut().push(el.template_id.clone());
        });
        tick.set(1);
        ids
    });
    let ids = ids.borrow();
    assert_eq!(ids.len(), 2, "effect must run twice");
    let first = ids[0].as_deref().expect("first template_id");
    let second = ids[1].as_deref().expect("second template_id");
    assert!(
        first.starts_with("__rui_button_auto_"),
        "omitted Button id must be a compile-time auto key, got {first}"
    );
    assert_eq!(
        first, second,
        "rebuild must keep the same Button instance key"
    );
}

#[test]
fn into_element_list_flattens_fragment() {
    use rui::core::tree::IntoElementList;

    let frag = rui::ui! {
        <Fragment>
            <Text>"a"</Text>
            <Text>"b"</Text>
        </Fragment>
    };
    let kids = frag.into_element_list();
    assert_eq!(kids.len(), 2);
}

#[test]
fn dynamic_calls_component_closure() {
    let el = rui::ui! {
        <Dynamic component={|| rui::ui! { <Text>"dyn"</Text> }} />
    };
    assert_eq!(el.text.as_deref(), Some("dyn"));
}

#[test]
fn ref_binds_template_id_to_signal() {
    let slot = rui::core::reactivity::create_signal(None::<String>);
    // Nest under VStack so the outer `ui!` root `template_id` does not overwrite the ref target.
    let el = rui::ui! {
        <VStack>
            <Box ref={slot} />
        </VStack>
    };
    let child = &el.children[0];
    assert!(child.template_id.is_some());
    assert_eq!(slot.get(), child.template_id);
}
