use super::*;
use jadix_native::layout::*;

struct Host;
impl PlatformHost for Host {
    fn measure(
        &mut self,
        request: MeasureRequest,
        _: &NodeDescriptor,
    ) -> Result<MeasureResult, LayoutError> {
        Ok(MeasureResult {
            key: request.key,
            width: request.key.max_width.min(160.0),
            height: 24.0,
            baseline: Some(18.0),
        })
    }
    fn publish(&mut self, _: RootId, _: u64, _: &[PresentationNode]) -> Result<(), String> {
        Ok(())
    }
    fn dispose(&mut self) {}
}

fn find(app: &FieldApp<Host>, caption: &str) -> NodeId {
    let mut pending = app.root.model().roots().to_vec();
    while let Some(id) = pending.pop() {
        let node = app.root.model().node(id).unwrap();
        if node.descriptor.accessible_name.as_deref() == Some(caption)
            || matches!(&node.descriptor.content, NodeContent::Button { label, .. } | NodeContent::Checkbox { label, .. } if label == caption)
        {
            return id;
        }
        pending.extend(&node.children);
    }
    panic!("Missing field control: {caption}");
}
fn send(app: &mut FieldApp<Host>, node: NodeId, payload: EventPayload) -> Result<(), String> {
    let event = PlatformEvent { node, payload };
    app.root.event(&event).map_err(|e| e.to_string())?;
    app.dispatch(&event)
}
fn click(app: &mut FieldApp<Host>, caption: &str) {
    let node = find(app, caption);
    send(app, node, EventPayload::Activate).unwrap();
}
fn edit(app: &mut FieldApp<Host>, caption: &str, text: &str) -> Result<(), String> {
    let node = find(app, caption);
    let revision = match app.root.model().node(node).unwrap().descriptor.content {
        NodeContent::Input { revision, .. } | NodeContent::Textarea { revision, .. } => {
            revision + 1
        }
        _ => unreachable!(),
    };
    send(
        app,
        node,
        EventPayload::Edit {
            value: text.into(),
            revision,
            selection_utf16: [text.encode_utf16().count(); 2],
            composing_utf16: None,
        },
    )
}

#[test]
fn form_validation_review_back_cancel_storage_and_restore_are_one_workflow() {
    let mut app = FieldApp::new(Host, [360.0, 640.0], 0, String::new()).unwrap();
    click(&mut app, "Edit request");
    click(&mut app, "Review request");
    assert_eq!(app.fields.history.get().current(), "edit");
    assert!(app.fields.status.get().contains("name"));
    edit(&mut app, "Contact name", "東京 😀").unwrap();
    edit(
        &mut app,
        "Work details",
        "Repair the door\nKeep the original lock",
    )
    .unwrap();
    let original = find(&app, "Contact name");
    let priority = find(&app, "Request priority");
    send(
        &mut app,
        priority,
        EventPayload::Select {
            value: "urgent".into(),
            revision: 1,
        },
    )
    .unwrap();
    let consent = find(&app, "Confirm details");
    send(
        &mut app,
        consent,
        EventPayload::Toggle {
            checked: true,
            revision: 1,
        },
    )
    .unwrap();
    click(&mut app, "Review request");
    assert_eq!(app.fields.history.get().current(), "review");
    click(&mut app, "Back to editor");
    assert_ne!(original, find(&app, "Contact name"));
    assert_eq!(app.fields.name.get(), "東京 😀");
    assert!(send(&mut app, original, EventPayload::Activate).is_err());
    click(&mut app, "Review request");
    click(&mut app, "Save request");
    assert_eq!(app.fields.history.get().current(), "overview");
    assert_eq!(app.fields.revision.get(), 1);
    let saved = app.state();
    app.storage_result(Some("disk full".into())).unwrap();
    assert!(app.storage_notification().is_some());
    assert_eq!(app.state(), saved);
    app.storage_result(None).unwrap();
    assert!(app.storage_notification().is_none());
    click(&mut app, "Edit request");
    edit(&mut app, "Contact name", "Cancel this change").unwrap();
    let draft = app.state();
    let mut recovered = FieldApp::new(Host, [480.0, 720.0], 0, String::new()).unwrap();
    recovered.restore(&draft).unwrap();
    assert_eq!(recovered.state(), draft);
    assert_ne!(app.root.model().id(), recovered.root.model().id());
    click(&mut recovered, "Cancel draft");
    assert_eq!(recovered.state(), saved);
    assert_eq!(recovered.fields.name.get(), "東京 😀");
}

#[test]
fn malformed_checkpoint_and_over_limit_edits_are_atomic() {
    let mut app = FieldApp::new(Host, [360.0, 640.0], 0, String::new()).unwrap();
    click(&mut app, "Edit request");
    let before = app.state();
    edit(&mut app, "Contact name", &"界".repeat(86)).unwrap();
    assert_eq!(app.state(), before);
    edit(&mut app, "Contact name", "invalid\u{1}").unwrap();
    assert_eq!(app.state(), before);
    let original: serde_json::Value = serde_json::from_str(&before).unwrap();
    for field in ["schema", "application", "navigation", "draft", "revision"] {
        let mut altered = original.clone();
        altered[field] = serde_json::json!("invalid");
        assert!(app.restore(&altered.to_string()).is_err());
        assert_eq!(app.state(), before);
    }
    let mut altered = original.clone();
    altered["navigation"]["entries"] = serde_json::json!(["review"]);
    assert!(app.restore(&altered.to_string()).is_err());
    assert!(app.restore(&"x".repeat(MAX_STATE_BYTES + 1)).is_err());
    assert_eq!(app.state(), before);
    let maximum = FieldDraft {
        name: "\"".repeat(256),
        details: "\\".repeat(8192),
        priority: "urgent".into(),
        consent: true,
    };
    maximum.validate().unwrap();
    app.fields.set_draft(maximum.clone());
    app.fields.saved.set(Some(maximum));
    app.fields.revision.set(1);
    let escaped = app.state();
    assert!(escaped.len() <= MAX_STATE_BYTES);
    app.restore(&escaped).unwrap();
    assert_eq!(app.state(), escaped);
    app.root.dispose();
    assert!(app.restore(&before).is_err());
}
