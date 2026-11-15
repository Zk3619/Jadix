#[path = "../../../examples/portable-contract/src/page.rs"]
mod page;

use jadix::core::reactivity::create_root;
use jadix::{authoring::native::ElementProjection, presentation::*};
use jadix_components::prelude::*;

#[test]
fn password_builders_and_rsx_publish_explicit_native_semantics() {
    let (owner, (mut builder, mut rsx, mut plain)) = create_root(|| {
        let value = create_signal(String::from("test-only"));
        let builder: Element = input("secret-builder")
            .input_type(InputType::Password)
            .value_signal(value)
            .into();
        let rsx = ui! { <Input id="secret-rsx" input_type="password" value={value} /> };
        let plain: Element = input("plain").input_type(InputType::Text).into();
        (builder, rsx, plain)
    });
    let projection = ElementProjection::new(RootId::new());
    for element in [&mut builder, &mut rsx] {
        let node = projection.snapshot(element).unwrap();
        assert!(matches!(
            node.descriptor.content,
            NodeContent::Input { password: true, .. }
        ));
    }
    assert!(matches!(
        projection.snapshot(&mut plain).unwrap().descriptor.content,
        NodeContent::Input {
            password: false,
            ..
        }
    ));
    drop(owner);
}

#[test]
fn portable_page_module_also_builds_against_existing_gpu_author_surface() {
    let (owner, (mut builder, mut rsx)) = create_root(|| {
        let count = create_signal(0usize);
        let value = create_signal(String::new());
        (page::builder_page(count, value), page::page(count, value))
    });
    assert_eq!(builder.children.len(), rsx.children.len());
    let projection = ElementProjection::new(RootId::new());
    fn leaves(node: &PresentationNode, output: &mut Vec<NodeContent>) {
        if node.descriptor.content != NodeContent::Container {
            output.push(node.descriptor.content.clone());
        }
        for child in &node.children {
            leaves(child, output);
        }
    }
    let mut builder_content = vec![];
    let mut rsx_content = vec![];
    leaves(
        &projection.snapshot(&mut builder).unwrap(),
        &mut builder_content,
    );
    leaves(&projection.snapshot(&mut rsx).unwrap(), &mut rsx_content);
    assert_eq!(builder_content.len(), 3);
    assert_eq!(builder_content, rsx_content);
    drop(owner);
}

#[test]
fn compound_select_rsx_preserves_the_existing_gpu_construction_surface() {
    let (owner, element) = create_root(|| {
        let value = create_signal("apple".to_string());
        ui! {
            <Select id="gpu-compound" value={value} placeholder="Choose fruit">
                <SelectTrigger><SelectValue /></SelectTrigger>
                <SelectContent>
                    <SelectItem value="apple">"Apple"</SelectItem>
                    <SelectItem value="banana">"Banana"</SelectItem>
                </SelectContent>
            </Select>
        }
    });
    assert_eq!(element.template_id.as_deref(), Some("gpu-compound__select"));
    assert!(!element.children.is_empty());
    drop(owner);
}

#[test]
fn compound_popover_rsx_preserves_the_existing_gpu_construction_surface() {
    let (owner, element) = create_root(|| {
        let open = create_signal(false);
        ui! {
            <Popover id="gpu-popover" open={open}>
                <PopoverTrigger><Button>"Open"</Button></PopoverTrigger>
                <PopoverContent side="bottom" align="start" sideOffset={4} width="280">
                    <PopoverHeader>
                        <PopoverTitle>"Profile"</PopoverTitle>
                        <PopoverDescription>"Native content"</PopoverDescription>
                    </PopoverHeader>
                </PopoverContent>
            </Popover>
        }
    });
    assert!(!element.children.is_empty());
    drop(owner);
}
