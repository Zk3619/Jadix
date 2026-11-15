use jadix_native::{presentation::*, HeadlessRoot};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut root = HeadlessRoot::new();
    let button = NodeId::new(root.id());
    root.commit(CommitBatch {
        root: root.id(),
        previous: 0,
        revision: 1,
        operations: vec![Operation::Create {
            node: button,
            parent: None,
            index: 0,
            descriptor: NodeDescriptor {
                content: NodeContent::Button {
                    label: "Continue".into(),
                    enabled: true,
                },
                accessible_role: None,
                accessible_selected: None,
                accessible_expanded: None,
                accessibility_relations: Default::default(),
                frame: Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 120.0,
                    height: 40.0,
                },
                accessible_description: None,
                accessible_name: Some("Continue".into()),
                style: Default::default(),
            },
        }],
    })?;
    root.accept_event(&PlatformEvent {
        node: button,
        payload: EventPayload::Activate,
    })?;
    println!(
        "headless revision {}: button event accepted",
        root.revision()
    );
    root.dispose();
    Ok(())
}
