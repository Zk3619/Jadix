//! Smoke-tests for `<Fragment>`, `<Dynamic>`, and `IntoElementList` unpacking.

#[test]
fn fragment_sets_attr_and_children() {
    let frag = rui::ui! {
        <Fragment>
            <Text>"one"</Text>
            <Text>"two"</Text>
        </Fragment>
    };
    assert_eq!(
        frag.get_attr("rui_fragment").map(|s| s.as_str()),
        Some("1")
    );
    assert_eq!(frag.children.len(), 2);
    let kids = rui::core::tree::Element::unpack_fragment_children(Some(frag));
    assert_eq!(kids.len(), 2);
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
