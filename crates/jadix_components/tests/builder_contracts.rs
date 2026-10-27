use jadix_components::prelude::*;

#[test]
fn builder_and_props_share_runtime_without_proc_macros() {
    let (_owner, ()) = jadix_components::core::reactivity::create_root(|| {
        let builder: Element = button("builder-only").text("Save").into();
        let props = Button(ButtonProps {
            id: "props-only".into(),
            children: Some(Element::new().text("Save")),
            ..Default::default()
        });
        assert_eq!(builder.attrs.get("data-slot"), props.attrs.get("data-slot"));
        assert_eq!(builder.template_id.as_deref(), Some("builder-only"));
        assert_eq!(props.template_id.as_deref(), Some("props-only"));
    });
}
