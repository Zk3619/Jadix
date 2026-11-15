#![cfg(all(not(feature = "gpu"), feature = "macros"))]
use jadix_components::{authoring::native::ElementProjection, prelude::*};
use jadix_native::{presentation::*, HeadlessRoot};

#[test]
fn bounded_history_restore_is_atomic_and_registry_bound() {
    let mut history = NavigationHistory::new(["list", "edit", "review"], "list").unwrap();
    assert!(!history.back());
    assert!(history.push("edit").unwrap());
    assert!(!history.push("edit").unwrap());
    assert_eq!(history.depth(), 2);
    let before = history.clone();
    for source in [
        "{}",
        r#"{"schema":1,"routes":["list"],"entries":["list"]}"#,
        r#"{"schema":1,"routes":["list","edit","review"],"entries":[]}"#,
        r#"{"schema":1,"routes":["list","edit","review"],"entries":["unknown"]}"#,
    ] {
        assert!(history.restore(source).is_err());
        assert_eq!(history, before);
    }
    for index in 0..MAX_NAVIGATION_DEPTH - 2 {
        history
            .push(if index % 2 == 0 { "review" } else { "edit" })
            .unwrap();
    }
    let full = history.clone();
    assert!(history.push("list").is_err());
    assert_eq!(history, full);
    history.reset("list").unwrap();
    history.restore(&before.encode()).unwrap();
    assert_eq!(history, before);
    assert!(NavigationHistory::new(["duplicate", "duplicate"], "duplicate").is_err());
    assert!(NavigationHistory::new(["bad\nroute"], "bad\nroute").is_err());
    assert!(
        serde_json::from_str::<NavigationHistory>(r#"{"schema":1,"routes":[],"entries":[]}"#)
            .is_err()
    );
}

fn labels(node: &PresentationNode, out: &mut Vec<(NodeId, String)>) {
    if let NodeContent::Button { label, .. } = &node.descriptor.content {
        out.push((node.id, label.clone()));
    }
    for child in &node.children {
        labels(child, out);
    }
}

#[test]
fn rsx_navigation_hides_old_actions_preserves_drafts_and_retires_nodes() {
    let (_owner, (mut element, history, draft)) = create_root(|| {
        let history = create_signal(NavigationHistory::new(["list", "edit"], "list").unwrap());
        let draft = create_signal("draft".to_owned());
        let element = ui! {
            <VStack>
                <NavigationPage id="list-page" history={history} route="list">
                    <NavigationLink id="open" history={history} route="edit">"Edit"</NavigationLink>
                </NavigationPage>
                <NavigationPage id="edit-page" history={history} route="edit">
                    <Input id="editor" value={draft} />
                    <NavigationBack id="back" history={history}>"Back"</NavigationBack>
                </NavigationPage>
            </VStack>
        };
        (element, history, draft)
    });
    let model = HeadlessRoot::new();
    let projection = ElementProjection::with_lifetime(model.lifetime());
    let snapshot = projection.snapshot(&mut element).unwrap();
    let mut buttons = Vec::new();
    labels(&snapshot, &mut buttons);
    let open = buttons.iter().find(|(_, label)| label == "Edit").unwrap().0;
    projection
        .dispatch(
            &mut element,
            &PlatformEvent {
                node: open,
                payload: EventPayload::Activate,
            },
        )
        .unwrap();
    assert_eq!(history.get().current(), "edit");
    let snapshot = projection.snapshot(&mut element).unwrap();
    let mut buttons = Vec::new();
    labels(&snapshot, &mut buttons);
    let back = buttons.iter().find(|(_, label)| label == "Back").unwrap().0;
    assert!(projection
        .dispatch(
            &mut element,
            &PlatformEvent {
                node: open,
                payload: EventPayload::Activate
            }
        )
        .is_err());
    draft.set("saved draft".to_owned());
    projection
        .dispatch(
            &mut element,
            &PlatformEvent {
                node: back,
                payload: EventPayload::Activate,
            },
        )
        .unwrap();
    let snapshot = projection.snapshot(&mut element).unwrap();
    let mut buttons = Vec::new();
    labels(&snapshot, &mut buttons);
    let reopened = buttons.iter().find(|(_, label)| label == "Edit").unwrap().0;
    assert_ne!(open, reopened);
    assert_eq!(draft.get(), "saved draft");
}
