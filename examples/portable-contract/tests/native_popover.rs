use jadix_components::{
    authoring::native::{ElementProjection, ProjectionError},
    prelude::*,
};
use jadix_native::{BackendError, HeadlessRoot, presentation::*};
use std::{cell::Cell, rc::Rc};

fn nodes(tree: &PresentationNode) -> Vec<&PresentationNode> {
    let mut result = Vec::new();
    let mut pending = vec![tree];
    while let Some(node) = pending.pop() {
        result.push(node);
        pending.extend(node.children.iter().rev());
    }
    result
}
fn popup(tree: &PresentationNode) -> &PresentationNode {
    nodes(tree)
        .into_iter()
        .find(|node| matches!(node.descriptor.content, NodeContent::Popover { .. }))
        .unwrap()
}
fn compound(open: Signal<bool>) -> Element {
    ui! { <Popover id="profile" open={open}>
        <PopoverTrigger id="profile-trigger">"Edit profile"</PopoverTrigger>
        <PopoverContent side="right" align="end" sideOffset={6.0} width={240.0}>
            <PopoverHeader><PopoverTitle>"Profile"</PopoverTitle><PopoverDescription>"中文 😀"</PopoverDescription></PopoverHeader>
            <Button id="done" on_click={popover_close}>"Done"</Button>
        </PopoverContent>
    </Popover> }
}

#[test]
fn compound_rsx_projects_one_typed_panel_with_resolved_native_anchor() {
    let (_owner, mut element) = create_root(|| compound(create_signal(true)));
    let mut root = HeadlessRoot::new();
    let projection = ElementProjection::new(root.id());
    let tree = projection.snapshot(&mut element).unwrap();
    let panel = popup(&tree);
    let trigger_id = projection.node_by_template(&element, "profile-trigger").unwrap();
    let trigger = nodes(&tree).into_iter().find(|node| node.id == trigger_id).unwrap();
    assert_eq!(trigger.descriptor.accessible_name.as_deref(), Some("Edit profile"));
    assert!(!trigger.children.is_empty());
    assert!(
        matches!(panel.descriptor.content, NodeContent::Popover { anchor, placement: PopoverPlacement::RightEnd, gap: 6, open: true, .. } if anchor == trigger.id)
    );
    assert_eq!(panel.descriptor.style.width, Some(240.0));
    assert!(nodes(panel).iter().any(
        |node| matches!(&node.descriptor.content, NodeContent::Text(value) if value == "中文 😀")
    ));
    root.reconcile(std::slice::from_ref(&tree)).unwrap();
}

#[test]
fn controlled_trigger_and_dismiss_share_revisions_and_keep_native_identity() {
    let (_owner, (mut element, open)) = create_root(|| {
        let open = create_signal(false);
        (compound(open), open)
    });
    let mut root = HeadlessRoot::new();
    let projection = ElementProjection::new(root.id());
    let first = projection.snapshot(&mut element).unwrap();
    let id = popup(&first).id;
    root.reconcile(std::slice::from_ref(&first)).unwrap();
    let anchor = match popup(&first).descriptor.content {
        NodeContent::Popover { anchor, .. } => anchor,
        _ => unreachable!(),
    };
    let activate = PlatformEvent {
        node: anchor,
        payload: EventPayload::Activate,
    };
    root.accept_event(&activate).unwrap();
    projection.dispatch(&mut element, &activate).unwrap();
    assert!(open.get());
    let second = projection.snapshot(&mut element).unwrap();
    root.reconcile(std::slice::from_ref(&second)).unwrap();
    let revision = match popup(&second).descriptor.content {
        NodeContent::Popover { revision, .. } => revision,
        _ => unreachable!(),
    };
    assert_eq!(popup(&second).id, id);
    let dismiss = PlatformEvent {
        node: id,
        payload: EventPayload::Dismiss {
            backdrop: true,
            revision: revision + 1,
        },
    };
    root.accept_event(&dismiss).unwrap();
    projection.dispatch(&mut element, &dismiss).unwrap();
    assert!(!open.get());
    assert_eq!(
        projection.dispatch(&mut element, &dismiss),
        Err(ProjectionError::InvalidEvent)
    );
    let closed = projection.snapshot(&mut element).unwrap();
    root.reconcile(std::slice::from_ref(&closed)).unwrap();
    assert_eq!(popup(&closed).id, id);
    assert_eq!(root.accept_event(&dismiss), Err(BackendError::InvalidEvent));
    projection.dispose();
    assert_eq!(
        projection.dispatch(&mut element, &activate),
        Err(ProjectionError::Disposed)
    );
}

#[test]
fn cancel_callback_receives_blocked_escape_and_outside_press_without_closing() {
    let reason = Rc::new(Cell::new(None));
    let observed = reason.clone();
    let (_owner, mut element) = create_root(|| {
        ui! {
            <Popover id="cancel" defaultOpen=true onCancel={move |reason| observed.set(Some(reason))}>
                <PopoverTrigger>"Open"</PopoverTrigger>
                <PopoverContent closeOnEscape=false closeOnPressOutside=false enterDuration=120 exitDuration=160>"Panel"</PopoverContent>
            </Popover>
        }
    });
    let mut root = HeadlessRoot::new();
    let projection = ElementProjection::new(root.id());
    let tree = projection.snapshot(&mut element).unwrap();
    root.reconcile(std::slice::from_ref(&tree)).unwrap();
    let panel = popup(&tree);
    assert_eq!(panel.descriptor.style.popover_enter_ms, Some(120));
    assert_eq!(panel.descriptor.style.popover_exit_ms, Some(160));
    let NodeContent::Popover { revision, .. } = panel.descriptor.content else {
        unreachable!();
    };
    for (backdrop, expected) in [
        (false, PopoverOpenChangeReason::Cancel),
        (true, PopoverOpenChangeReason::PressOutside),
    ] {
        let attempted = PlatformEvent {
            node: panel.id,
            payload: EventPayload::PopoverCancel { backdrop },
        };
        root.accept_event(&attempted).unwrap();
        projection.dispatch(&mut element, &attempted).unwrap();
        assert_eq!(reason.take(), Some(expected));
        let dismiss = PlatformEvent {
            node: panel.id,
            payload: EventPayload::Dismiss {
                backdrop,
                revision: revision + 1,
            },
        };
        assert_eq!(root.accept_event(&dismiss), Err(BackendError::InvalidEvent));
    }
    assert!(matches!(
        popup(&projection.snapshot(&mut element).unwrap())
            .descriptor
            .content,
        NodeContent::Popover { open: true, .. }
    ));
}

#[test]
fn portable_popover_receives_only_valid_positions_while_open() {
    let observed = Rc::new(Cell::new(None));
    let received = observed.clone();
    let (_owner, (mut element, open)) = create_root(|| {
        let open = create_signal(true);
        let element = ui! {
            <Popover id="position" open={open} onPositionChange={move |position| received.set(Some(position))}>
                <PopoverTrigger>"Open"</PopoverTrigger>
                <PopoverContent>"Panel"</PopoverContent>
            </Popover>
        };
        (element, open)
    });
    let mut root = HeadlessRoot::new();
    let projection = ElementProjection::new(root.id());
    let tree = projection.snapshot(&mut element).unwrap();
    root.reconcile(std::slice::from_ref(&tree)).unwrap();
    let position = PopoverPosition {
        frame: Rect {
            x: 20.0,
            y: 40.0,
            width: 120.0,
            height: 80.0,
        },
        placement: PopoverPlacement::TopEnd,
        available_width: 180.0,
        available_height: 100.0,
    };
    let event = format!(
        r#"{{"root":"{}","node":"{}","payload":{{"kind":"popoverPosition","position":{{
            "frame":{{"x":20,"y":40,"width":120,"height":80}},
            "placement":"top-end","availableWidth":180,"availableHeight":100
        }}}}}}"#,
        root.id().token(),
        popup(&tree).id.token(),
    );
    let event = jadix_native::wire::decode_event(&event, &root).unwrap();
    root.accept_event(&event).unwrap();
    projection.dispatch(&mut element, &event).unwrap();
    assert_eq!(observed.take(), Some(Some(position)));
    let event = PlatformEvent {
        node: event.node,
        payload: EventPayload::PopoverPosition { position: None },
    };
    root.accept_event(&event).unwrap();
    projection.dispatch(&mut element, &event).unwrap();
    assert_eq!(observed.take(), Some(None));
    let invalid = PlatformEvent {
        node: event.node,
        payload: EventPayload::PopoverPosition {
            position: Some(PopoverPosition {
                available_height: -1.0,
                ..position
            }),
        },
    };
    assert_eq!(root.accept_event(&invalid), Err(BackendError::InvalidEvent));
    open.set(false);
    let closed = projection.snapshot(&mut element).unwrap();
    root.reconcile(std::slice::from_ref(&closed)).unwrap();
    assert_eq!(root.accept_event(&event), Err(BackendError::InvalidEvent));
}

#[test]
fn popover_lifecycle_follows_the_committed_open_state() {
    use jadix_native::presentation::style::PopoverAppearance;

    let observed = Rc::new(Cell::new(None));
    let opened = observed.clone();
    let closed = observed.clone();
    let (_owner, (mut element, open)) = create_root(|| {
        let open = create_signal(false);
        let element = ui! {
            <Popover id="lifecycle" open={open}
                onOpen={move || opened.set(Some(true))}
                onClose={move || closed.set(Some(false))}>
                <PopoverTrigger>"Open"</PopoverTrigger>
                <PopoverContent enterDuration={120u16} exitDuration={180u16}
                    popupStartingStyle={PopoverAppearance {
                        opacity: 0.25, scale: 0.95, offset: [0.0, 6.0],
                        background: Some([24, 32, 48, 220]),
                        corner_radius: Some(8.0),
                        ..PopoverAppearance::default()
                    }}
                    popupEndingStyle={PopoverAppearance {
                        offset: [0.0, -4.0], ..PopoverAppearance::default()
                    }}>
                    "Panel"
                </PopoverContent>
            </Popover>
        };
        (element, open)
    });
    let mut root = HeadlessRoot::new();
    let projection = ElementProjection::new(root.id());
    for visible in [true, false] {
        open.set(visible);
        let tree = projection.snapshot(&mut element).unwrap();
        let style = &popup(&tree).descriptor.style;
        assert_eq!(style.popover_enter_ms, Some(120));
        assert_eq!(style.popover_exit_ms, Some(180));
        let starting = style.popover_starting.as_ref().unwrap();
        assert_eq!(starting.scale, 0.95);
        assert_eq!(starting.background, Some([24, 32, 48, 220]));
        assert_eq!(starting.corner_radius, Some(8.0));
        assert_eq!(style.popover_ending.as_ref().unwrap().offset, [0.0, -4.0]);
        root.reconcile(std::slice::from_ref(&tree)).unwrap();
        let event = PlatformEvent {
            node: popup(&tree).id,
            payload: EventPayload::PopoverLifecycle { open: visible },
        };
        root.accept_event(&event).unwrap();
        projection.dispatch(&mut element, &event).unwrap();
        assert_eq!(observed.take(), Some(visible));
        let stale = PlatformEvent {
            node: event.node,
            payload: EventPayload::PopoverLifecycle { open: !visible },
        };
        assert_eq!(root.accept_event(&stale), Err(BackendError::InvalidEvent));
    }
}

#[test]
fn lazy_panel_mounts_once_and_retains_descendant_ids_after_reopen() {
    let builds = Rc::new(Cell::new(0));
    let count = builds.clone();
    let (_owner, (mut element, open)) = create_root(|| {
        let open = create_signal(false);
        let element = popover("lazy").open(open).child(|| {
            let trigger = PopoverTrigger(Default::default());
            let content = popover_content(Default::default(), move || {
                count.set(count.get() + 1);
                text("retained")
            });
            Element::new().child(trigger).child(content)
        });
        (element, open)
    });
    let projection = ElementProjection::new(RootId::new());
    assert!(
        popup(&projection.snapshot(&mut element).unwrap())
            .children
            .is_empty()
    );
    assert_eq!(builds.get(), 0);
    open.set(true);
    let first = projection.snapshot(&mut element).unwrap();
    let child = popup(&first).children[0].id;
    open.set(false);
    projection.snapshot(&mut element).unwrap();
    open.set(true);
    let second = projection.snapshot(&mut element).unwrap();
    assert_eq!(popup(&second).children[0].id, child);
    assert_eq!(builds.get(), 1);
}

#[test]
fn content_before_custom_trigger_resolves_the_final_anchor_identity() {
    let (_owner, mut element) = create_root(|| {
        popover("reverse").child(|| {
            let panel = popover_content(Default::default(), || text("Panel"));
            let trigger = PopoverTrigger(PopoverTriggerProps {
                id: "custom".into(),
                children: Some(button("original").child(text("Trigger")).into()),
                ..Default::default()
            });
            Element::new().child(panel).child(trigger)
        })
    });
    let tree = ElementProjection::new(RootId::new())
        .snapshot(&mut element)
        .unwrap();
    assert!(
        matches!(popup(&tree).descriptor.content, NodeContent::Popover { anchor, .. } if anchor == tree.children[1].id)
    );
}

#[test]
fn missing_and_duplicate_native_anchor_names_fail_projection() {
    for duplicate in [false, true] {
        let (_owner, mut element) = create_root(|| {
            popover("invalid").child(|| {
                let mut root = Element::new();
                if duplicate {
                    root = root
                        .child(PopoverTrigger(PopoverTriggerProps {
                            id: "same".into(),
                            ..Default::default()
                        }))
                        .child(button("same").child(text("Duplicate")));
                }
                root.child(popover_content(Default::default(), || text("Panel")))
            })
        });
        let error = ElementProjection::new(RootId::new())
            .snapshot(&mut element)
            .unwrap_err();
        assert_eq!(
            error,
            ProjectionError::Unsupported(if duplicate {
                "ambiguous Popover anchor"
            } else {
                "missing Popover anchor"
            })
        );
    }
}

#[test]
fn unsupported_rsx_attributes_and_popup_policies_are_rejected() {
    let (_owner, mut elements) = create_root(|| {
        vec![
            ui! { <Popover id="ignored" disabled=true><PopoverTrigger /> <PopoverContent>"Panel"</PopoverContent></Popover> },
            ui! { <Popover id="height"><PopoverTrigger /> <PopoverContent h={200.0}>"Panel"</PopoverContent></Popover> },
            ui! { <Popover id="lock"><PopoverTrigger /> <PopoverContent lockPageScroll=true>"Panel"</PopoverContent></Popover> },
            ui! { <Popover id="freeze"><PopoverTrigger /> <PopoverContent scrollFreeze="all">"Panel"</PopoverContent></Popover> },
            ui! { <Popover id="fixed"><PopoverTrigger /> <PopoverContent strategy="fixed">"Panel"</PopoverContent></Popover> },
            ui! { <Popover id="width"><PopoverTrigger /> <PopoverContent width="20rem">"Panel"</PopoverContent></Popover> },
            ui! { <Popover id="gap"><PopoverTrigger /> <PopoverContent sideOffset={1.5}>"Panel"</PopoverContent></Popover> },
            ui! { <Popover id="side"><PopoverTrigger /> <PopoverContent side="invalid">"Panel"</PopoverContent></Popover> },
        ]
    });
    let projection = ElementProjection::new(RootId::new());
    for element in &mut elements {
        assert_eq!(
            projection.snapshot(element).unwrap_err(),
            ProjectionError::Unsupported("native component options")
        );
    }
}

#[test]
fn builder_panel_numeric_styles_and_explicit_no_scroll_freeze_are_preserved() {
    let (_owner, mut element) = create_root(|| {
        popover("styles").child(|| {
            let trigger = PopoverTrigger(Default::default());
            let mut opts = PopoverContentOpts {
                width: Some("220px".into()),
                side: "top-end".into(),
                scroll_freeze: Some(jadix_components::app::portal_overlay::ScrollFreeze::None),
                ..Default::default()
            };
            opts.panel.base = Some(Box::new(|element| element.h(120.0).p(12.0)));
            Element::new()
                .child(trigger)
                .child(popover_content(opts, || text("Panel")))
        })
    });
    let tree = ElementProjection::new(RootId::new())
        .snapshot(&mut element)
        .unwrap();
    let panel = popup(&tree);
    assert_eq!(panel.descriptor.style.width, Some(220.0));
    assert_eq!(panel.descriptor.style.height, Some(120.0));
    assert_eq!(panel.descriptor.style.padding, [12.0; 4]);
    assert!(matches!(
        panel.descriptor.content,
        NodeContent::Popover {
            placement: PopoverPlacement::TopEnd,
            ..
        }
    ));
}

#[test]
fn nested_context_close_targets_only_its_own_controlled_signal() {
    let (_owner, (mut element, outer, inner)) = create_root(|| {
        let outer = create_signal(true);
        let inner = create_signal(true);
        let element = popover("outer").open(outer).child(|| {
            let trigger = PopoverTrigger(Default::default());
            let content = popover_content(Default::default(), move || {
                popover("inner").open(inner).child(|| {
                    let trigger = PopoverTrigger(Default::default());
                    let content = popover_content(Default::default(), || {
                        button("close-inner")
                            .on_click(popover_close)
                            .child(text("Close inner"))
                            .into()
                    });
                    Element::new().child(trigger).child(content)
                })
            });
            Element::new().child(trigger).child(content)
        });
        (element, outer, inner)
    });
    let mut root = HeadlessRoot::new();
    let projection = ElementProjection::new(root.id());
    let tree = projection.snapshot(&mut element).unwrap();
    root.reconcile(std::slice::from_ref(&tree)).unwrap();
    assert_eq!(
        nodes(&tree)
            .iter()
            .filter(|node| matches!(node.descriptor.content, NodeContent::Popover { .. }))
            .count(),
        2
    );
    let close_id = projection.node_by_template(&element, "close-inner").unwrap();
    let close = nodes(&tree).into_iter().find(|node| node.id == close_id).unwrap();
    let event = PlatformEvent {
        node: close.id,
        payload: EventPayload::Activate,
    };
    root.accept_event(&event).unwrap();
    projection.dispatch(&mut element, &event).unwrap();
    assert!(outer.get());
    assert!(!inner.get());
}
