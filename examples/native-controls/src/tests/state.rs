use super::NativeApp;
use jadix_native::{host::PlatformHost, layout::*, presentation::*};
use serde_json::json;
#[cfg(not(feature = "selection"))]
use serde_json::Value;

struct Host;
impl PlatformHost for Host {
    fn measure(
        &mut self,
        request: MeasureRequest,
        _: &NodeDescriptor,
    ) -> Result<MeasureResult, LayoutError> {
        Ok(MeasureResult {
            key: request.key,
            width: request.key.max_width.min(80.0),
            height: request.key.max_height.min(24.0),
            baseline: None,
        })
    }
    fn publish(&mut self, _: RootId, _: u64, _: &[PresentationNode]) -> Result<(), String> {
        Ok(())
    }
    fn dispose(&mut self) {}
}

#[cfg(all(feature = "extended", not(feature = "selection")))]
#[test]
fn shared_editor_dialog_preserves_unicode_and_nested_controlled_ownership() {
    fn find(app: &NativeApp<Host>, name: &str) -> NodeId {
        let mut pending = app.root.model().roots().to_vec();
        while let Some(id) = pending.pop() {
            let node = app.root.model().node(id).unwrap();
            if node.descriptor.accessible_name.as_deref() == Some(name)
                || matches!(&node.descriptor.content, NodeContent::Button { label, .. } if label == name)
                || matches!(&node.descriptor.content, NodeContent::Input { placeholder, .. } if placeholder == name)
            {
                return id;
            }
            pending.extend(&node.children);
        }
        panic!("missing shared control: {name}");
    }
    fn send(app: &mut NativeApp<Host>, node: NodeId, payload: Value) -> Result<(), String> {
        app.event(&json!({"root": app.root.model().id().token(), "node": node.token(), "payload": payload}).to_string())
    }
    fn dismiss(app: &mut NativeApp<Host>, node: NodeId) -> Result<(), String> {
        let NodeContent::Dialog { revision, .. } =
            app.root.model().node(node).unwrap().descriptor.content
        else {
            panic!("not a dialog");
        };
        send(
            app,
            node,
            json!({"kind": "dismiss", "backdrop": false, "revision": (revision + 1).to_string()}),
        )
    }
    fn open(app: &NativeApp<Host>, node: NodeId) -> bool {
        matches!(
            app.root.model().node(node).unwrap().descriptor.content,
            NodeContent::Dialog { open: true, .. }
        )
    }
    let mut app = NativeApp::new(Host, [360.0, 640.0], 7, String::new()).unwrap();
    let editor = find(&app, "Edit details");
    assert!(!open(&app, editor));
    let trigger = find(&app, "Open editor");
    send(&mut app, trigger, json!({"kind": "activate"})).unwrap();
    assert!(open(&app, editor));
    let confirmation = find(&app, "Confirm changes");
    let advanced = find(&app, "Advanced");
    assert!(!open(&app, confirmation));
    let field = find(&app, "Dialog name / 中文");
    let value = "编辑 中文😀";
    let length = value.encode_utf16().count();
    send(&mut app, field, json!({"kind": "edit", "value": value, "revision": "1", "selection": [length, length], "composing": null})).unwrap();
    send(&mut app, advanced, json!({"kind": "activate"})).unwrap();
    assert!(open(&app, editor) && open(&app, confirmation));
    assert!(dismiss(&mut app, editor).is_err());
    dismiss(&mut app, confirmation).unwrap();
    assert!(open(&app, editor) && !open(&app, confirmation));
    let background = find(&app, "Continue");
    assert!(send(&mut app, background, json!({"kind": "activate"})).is_err());
    dismiss(&mut app, editor).unwrap();
    assert!(!open(&app, editor));
    assert_eq!(app.value.get(), value);
    assert_eq!(app.count.get(), 7);
    assert!(send(&mut app, field, json!({"kind": "activate"})).is_err());
    send(&mut app, background, json!({"kind": "activate"})).unwrap();
    assert_eq!(app.count.get(), 8);
    send(&mut app, trigger, json!({"kind": "activate"})).unwrap();
    let reopened = find(&app, "Dialog name / 中文");
    assert!(matches!(
        &app.root.model().node(reopened).unwrap().descriptor.content,
        NodeContent::Input { value: restored, .. } if restored == value
    ));
    assert!(!open(&app, find(&app, "Confirm changes")));
    dismiss(&mut app, editor).unwrap();
}

#[cfg(not(feature = "selection"))]
fn edit(app: &mut NativeApp<Host>, multiline: bool, value: &str) {
    let mut pending = app.root.model().roots().to_vec();
    let mut target = None;
    while let Some(id) = pending.pop() {
        let node = app.root.model().node(id).unwrap();
        if matches!(node.descriptor.content, NodeContent::Textarea { .. }) && multiline
            || matches!(node.descriptor.content, NodeContent::Input { .. }) && !multiline
        {
            target = Some(id);
            break;
        }
        pending.extend(&node.children);
    }
    let node = target.unwrap();
    let length = value.encode_utf16().count();
    let event = json!({"root": app.root.model().id().token(), "node": node.token(),
        "payload": {"kind": "edit", "value": value, "revision": "1", "selection": [length, length], "composing": null}}).to_string();
    assert!(event.len() < 1024 * 1024);
    app.event(&event).unwrap();
}

#[cfg(not(feature = "selection"))]
#[test]
fn accepted_large_unicode_input_round_trips_between_roots() {
    let mut original = NativeApp::new(Host, [360.0, 640.0], 7, String::new()).unwrap();
    edit(&mut original, false, &"中😀".repeat(20000));
    let saved = original.state();
    assert!(saved.len() > 65536);
    let mut restored = NativeApp::new(Host, [360.0, 640.0], 0, String::new()).unwrap();
    restored.restore(&saved).unwrap();
    assert_eq!(restored.state(), saved);
    assert_ne!(restored.root.model().id(), original.root.model().id());
}

#[cfg(not(feature = "selection"))]
#[test]
fn restored_maximum_counter_can_still_handle_activation() {
    let mut app = NativeApp::new(Host, [360.0, 640.0], usize::MAX, String::new()).unwrap();
    let saved = app.state();
    app.restore(&saved).unwrap();
    let mut pending = app.root.model().roots().to_vec();
    let mut target = None;
    while let Some(id) = pending.pop() {
        let node = app.root.model().node(id).unwrap();
        if matches!(&node.descriptor.content, NodeContent::Button { label, .. } if label == "Continue")
        {
            target = Some(id);
            break;
        }
        pending.extend(&node.children);
    }
    let event = json!({"root": app.root.model().id().token(), "node": target.unwrap().token(),
        "payload": {"kind": "activate"}})
    .to_string();
    app.event(&event).unwrap();
    assert_eq!(app.state(), saved);
}

#[cfg(feature = "extended")]
#[cfg(not(feature = "selection"))]
#[test]
fn two_escaped_fields_near_the_wire_budget_restore_together() {
    let mut original = NativeApp::new(Host, [360.0, 640.0], 3, String::new()).unwrap();
    edit(&mut original, false, &"\0".repeat(160000));
    edit(&mut original, true, &"\0".repeat(160000));
    let saved = original.state();
    assert!(saved.len() > 1024 * 1024 && saved.len() < 2 * 1024 * 1024);
    let mut restored = NativeApp::new(Host, [360.0, 640.0], 0, String::new()).unwrap();
    restored.restore(&saved).unwrap();
    assert_eq!(restored.state(), saved);
}

#[cfg(not(feature = "selection"))]
#[test]
fn rejected_restore_keeps_all_business_values_and_root_revision() {
    let mut app = NativeApp::new(Host, [360.0, 640.0], 7, "keep 中文😀".into()).unwrap();
    let saved = app.state();
    let revision = app.root.model().revision();
    let mut oversized: Value = serde_json::from_str(&saved).unwrap();
    oversized["count"] = json!(99);
    oversized["value"] = json!("x".repeat(2 * 1024 * 1024));
    assert!(app.restore(&oversized.to_string()).is_err());
    let mut malformed: Value = serde_json::from_str(&saved).unwrap();
    malformed["count"] = json!(99);
    malformed["value"] = json!(false);
    assert!(app.restore(&malformed.to_string()).is_err());
    assert_eq!(app.state(), saved);
    assert_eq!(app.root.model().revision(), revision);
    app.restore(&saved).unwrap();
    app.root.dispose();
    assert_eq!(app.restore(&saved).unwrap_err(), "disposed native root");
}

#[cfg(feature = "selection")]
#[test]
fn select_routes_to_shared_state_and_restores_atomically() {
    let mut app = NativeApp::new(Host, [360.0, 640.0], usize::MAX, String::new()).unwrap();
    let mut pending = app.root.model().roots().to_vec();
    let mut select = None;
    let mut reset = None;
    let mut increment = None;
    while let Some(id) = pending.pop() {
        let node = app.root.model().node(id).unwrap();
        match &node.descriptor.content {
            NodeContent::Select { enabled: true, .. } => select = Some(id),
            NodeContent::Button { label, .. } if label == "Reset selection" => reset = Some(id),
            NodeContent::Button { label, .. } if label == "Continue" => increment = Some(id),
            _ => {}
        }
        pending.extend(&node.children);
    }
    let selected = json!({"root": app.root.model().id().token(), "node": select.unwrap().token(),
        "payload": {"kind": "select", "value": "apple", "revision": "1"}})
    .to_string();
    app.event(&selected).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&app.state()).unwrap()["value"],
        "apple"
    );
    let saved = app.state();
    let revision = app.root.model().revision();
    let mut invalid: serde_json::Value = serde_json::from_str(&saved).unwrap();
    invalid["count"] = json!(7);
    invalid["value"] = json!("missing");
    assert!(app.restore(&invalid.to_string()).is_err());
    assert_eq!(app.state(), saved);
    assert_eq!(app.root.model().revision(), revision);
    for node in [increment.unwrap(), reset.unwrap()] {
        app.event(
            &json!({"root": app.root.model().id().token(), "node": node.token(),
            "payload": {"kind": "activate"}})
            .to_string(),
        )
        .unwrap();
    }
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&app.state()).unwrap()["value"],
        ""
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&app.state()).unwrap()["count"],
        json!(usize::MAX)
    );
    app.restore(&saved).unwrap();
    assert_eq!(app.state(), saved);
    assert!(app.event(&selected).is_err());
}
