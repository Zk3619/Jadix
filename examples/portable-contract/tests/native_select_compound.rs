use jadix_components::{
    authoring::native::{ElementProjection, ProjectionError},
    prelude::*,
};
use jadix_native::{presentation::*, HeadlessRoot};

fn compound(value: Signal<String>) -> Element {
    ui! {
        <Select id="fruit" value={value} placeholder="Choose fruit">
            <SelectTrigger><SelectValue /></SelectTrigger>
            <SelectContent>
                <SelectItem value="apple">"Apple / 苹果"</SelectItem>
                <SelectItem value="banana">"Banana / 香蕉"</SelectItem>
                <SelectItem value="blocked" disabled=true>"Disabled option"</SelectItem>
            </SelectContent>
        </Select>
    }
}

#[test]
fn compound_rsx_and_options_builder_publish_identical_native_leaf_semantics() {
    let (_owner, (mut rsx, mut builder)) = create_root(|| {
        let value = create_signal("apple".to_string());
        let builder = select("builder")
            .value_signal(value)
            .placeholder("Choose fruit")
            .item(SelectItemData::new("apple", "Apple / 苹果"))
            .item(SelectItemData::new("banana", "Banana / 香蕉"))
            .item(SelectItemData::new("blocked", "Disabled option").disabled(true))
            .into();
        (compound(value), builder)
    });
    let projection = ElementProjection::new(RootId::new());
    let authored = projection.snapshot(&mut rsx).unwrap();
    let options = projection.snapshot(&mut builder).unwrap();
    assert_eq!(authored.descriptor.content, options.descriptor.content);
    assert!(authored.children.is_empty());
    assert_eq!(
        jadix_native::wire::descriptor(&authored.descriptor)["options"][2]["enabled"],
        false
    );
}

#[test]
fn compound_events_share_controlled_state_revisions_identity_and_disposal() {
    let (_owner, (mut element, value)) = create_root(|| {
        let value = create_signal(String::new());
        (compound(value), value)
    });
    let mut root = HeadlessRoot::new();
    let projection = ElementProjection::new(root.id());
    let first = projection.snapshot(&mut element).unwrap();
    root.reconcile(std::slice::from_ref(&first)).unwrap();
    let event = PlatformEvent {
        node: first.id,
        payload: EventPayload::Select {
            value: "banana".into(),
            revision: 1,
        },
    };
    root.accept_event(&event).unwrap();
    projection.dispatch(&mut element, &event).unwrap();
    assert_eq!(value.get(), "banana");
    assert!(projection.dispatch(&mut element, &event).is_err());
    for selected in ["blocked", "missing", ""] {
        let invalid = PlatformEvent {
            node: first.id,
            payload: EventPayload::Select {
                value: selected.into(),
                revision: 2,
            },
        };
        assert!(root.accept_event(&invalid).is_err());
        assert!(projection.dispatch(&mut element, &invalid).is_err());
        assert_eq!(value.get(), "banana");
    }
    value.set(String::new());
    let reset = projection.snapshot(&mut element).unwrap();
    assert_eq!(reset.id, first.id);
    assert!(
        matches!(reset.descriptor.content, NodeContent::Select { value, .. } if value.is_empty())
    );
    projection.dispose();
    assert_eq!(
        projection.dispatch(&mut element, &event),
        Err(ProjectionError::Disposed)
    );
}

#[test]
fn value_placeholder_and_trigger_numeric_chrome_apply_to_the_native_control() {
    let (_owner, mut element) = create_root(|| {
        ui! {
            <Select id="placeholder" placeholder="Root prompt">
                <SelectTrigger w={240.0} h={44.0}><SelectValue placeholder="Pick one" /></SelectTrigger>
                <SelectContent><SelectItem value="apple" /></SelectContent>
            </Select>
        }
    });
    let node = ElementProjection::new(RootId::new())
        .snapshot(&mut element)
        .unwrap();
    assert_eq!(node.descriptor.style.width, Some(240.0));
    assert_eq!(node.descriptor.style.height, Some(44.0));
    assert!(
        matches!(node.descriptor.content, NodeContent::Select { placeholder, options, .. } if placeholder == "Pick one" && options[0].label == "apple")
    );
}

#[test]
fn slots_outside_select_are_rejected_instead_of_mounting_empty_containers() {
    let (_owner, mut elements) = create_root(|| {
        vec![
            ui! { <SelectTrigger /> },
            ui! { <SelectValue /> },
            ui! { <SelectItem value="apple">"Apple"</SelectItem> },
            ui! { <SelectContent><SelectItem value="apple">"Apple"</SelectItem></SelectContent> },
        ]
    });
    let projection = ElementProjection::new(RootId::new());
    for element in &mut elements {
        assert_eq!(
            projection.snapshot(element),
            Err(ProjectionError::Unsupported("Select slot outside Select"))
        );
    }
}

#[test]
fn malformed_compound_structure_and_interactive_or_dynamic_labels_fail_closed() {
    let (_owner, mut elements) = create_root(|| {
        vec![
            ui! { <Select id="missing"><SelectContent><SelectItem value="apple" /></SelectContent></Select> },
            ui! { <Select id="order"><SelectContent /><SelectTrigger /></Select> },
            ui! { <Select id="duplicate"><SelectTrigger /><SelectTrigger /><SelectContent /></Select> },
            ui! { <Select id="interactive"><SelectTrigger /><SelectContent><SelectItem value="apple"><Button id="hidden">"Action"</Button></SelectItem></SelectContent></Select> },
            ui! { <Select id="value"><SelectTrigger><SelectValue>"Custom"</SelectValue></SelectTrigger><SelectContent /></Select> },
            {
                Select(SelectProps {
                    id: "dynamic".into(),
                    children: Some({
                        let mut fragment = Element::new();
                        fragment.set_attr("jadix_fragment", "1");
                        fragment.children(vec![
                            SelectTrigger(Default::default()),
                            select_content(Default::default(), move || {
                                SelectItem(SelectItemProps {
                                    value: Some("apple".into()),
                                    children: Some(jadix_components::app::reactive::bind_text(
                                        "dynamic",
                                        || "Changing".to_string(),
                                    )),
                                    ..Default::default()
                                })
                            }),
                        ])
                    }),
                    ..Default::default()
                })
            },
        ]
    });
    let projection = ElementProjection::new(RootId::new());
    for element in &mut elements {
        assert!(matches!(
            projection.snapshot(element),
            Err(ProjectionError::Unsupported(_))
        ));
    }
}

#[test]
fn unsupported_popup_styles_and_root_flags_are_explicit_errors() {
    let (_owner, mut elements) = create_root(|| {
        vec![
            ui! { <Select id="class" class="w-full"><SelectTrigger /><SelectContent /></Select> },
            ui! { <Select id="menu"><SelectTrigger /><SelectContent max_h="12rem" /></Select> },
            ui! { <Select id="ignored-menu-style"><SelectTrigger /><SelectContent w={240.0} /></Select> },
            ui! { <Select id="size"><SelectTrigger size="sm" /><SelectContent /></Select> },
            ui! { <Select id="align"><SelectTrigger /><SelectContent><SelectItem value="apple" align="center" /></SelectContent></Select> },
            ui! { <Select id="disabled" disabled=true><SelectTrigger /><SelectContent /></Select> },
            ui! { <Select id="default" default_value="apple"><SelectTrigger /><SelectContent><SelectItem value="apple" /></SelectContent></Select> },
        ]
    });
    let projection = ElementProjection::new(RootId::new());
    for element in &mut elements {
        assert!(matches!(
            projection.snapshot(element),
            Err(ProjectionError::Unsupported(_))
        ));
    }
}

#[test]
fn duplicate_values_keep_the_shared_backend_validation_boundary() {
    let (_owner, mut element) = create_root(|| {
        ui! {
            <Select id="duplicate-values"><SelectTrigger /><SelectContent>
                <SelectItem value="same">"First"</SelectItem><SelectItem value="same">"Second"</SelectItem>
            </SelectContent></Select>
        }
    });
    let mut root = HeadlessRoot::new();
    let projection = ElementProjection::new(root.id());
    let node = projection.snapshot(&mut element).unwrap();
    assert!(root.reconcile(&[node]).is_err());
}

#[test]
fn empty_native_menu_keeps_its_prompt_and_controlled_empty_value() {
    let (_owner, mut element) = create_root(|| {
        ui! {
            <Select id="empty" placeholder="No options"><SelectTrigger /><SelectContent /></Select>
        }
    });
    let mut root = HeadlessRoot::new();
    let node = ElementProjection::new(root.id())
        .snapshot(&mut element)
        .unwrap();
    assert!(
        matches!(&node.descriptor.content, NodeContent::Select { options, value, placeholder, .. } if options.is_empty() && value.is_empty() && placeholder == "No options")
    );
    root.reconcile(&[node]).unwrap();
}

#[test]
fn compound_normalization_bounds_options_labels_and_authored_depth() {
    let (_owner, mut elements) = create_root(|| {
        let mut elements = Vec::new();
        for mode in ["options", "label", "depth", "mixed"] {
            let content = select_content(Default::default(), move || {
                let mut item = SelectItem(SelectItemProps {
                    value: Some("apple".into()),
                    children: Some(text(if mode == "label" {
                        "中".repeat(1500)
                    } else {
                        "Apple".into()
                    })),
                    ..Default::default()
                });
                if mode == "depth" {
                    for _ in 0..260 {
                        item = Element::new().child(item);
                    }
                }
                let mut content = Element::new();
                content.children.push(item);
                if mode == "options" {
                    for index in 1..257 {
                        content.children.push(SelectItem(SelectItemProps {
                            value: Some(format!("item-{index}")),
                            ..Default::default()
                        }));
                    }
                }
                content
            });
            elements.push(Select(SelectProps {
                id: mode.into(),
                options: (mode == "mixed").then(|| vec![("extra".into(), "Extra".into())]),
                children: Some(
                    Element::new().children(vec![SelectTrigger(Default::default()), content]),
                ),
                ..Default::default()
            }));
        }
        elements
    });
    let projection = ElementProjection::new(RootId::new());
    for element in &mut elements {
        assert!(matches!(
            projection.snapshot(element),
            Err(ProjectionError::Unsupported(_))
        ));
    }
}
