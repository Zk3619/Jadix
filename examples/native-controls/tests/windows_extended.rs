#![cfg(all(
    windows,
    feature = "windows",
    feature = "extended",
    not(feature = "gpu")
))]

use jadix_native::{presentation::*, windows::WindowsHost};
use jadix_native_controls::NativeApp;

fn find(app: &NativeApp<WindowsHost>, predicate: impl Fn(&NodeContent) -> bool) -> NodeId {
    let mut pending = app.root.model().roots().to_vec();
    while let Some(identifier) = pending.pop() {
        let node = app.root.model().node(identifier).unwrap();
        if predicate(&node.descriptor.content) {
            return identifier;
        }
        pending.extend(&node.children);
    }
    panic!("missing native example control");
}

fn click(app: &mut NativeApp<WindowsHost>, label: &str) {
    let button = find(
        app,
        |content| matches!(content, NodeContent::Button { label: value, .. } if value == label),
    );
    app.root.host().activate(button).unwrap();
    app.poll_events().unwrap();
}

#[test]
fn shared_builder_page_runs_native_images_booleans_and_nested_dialog_callbacks() {
    let host = WindowsHost::new("Jadix shared native components", [800, 680]).unwrap();
    let viewport = host.viewport();
    let mut app = NativeApp::new(host, viewport, 0, String::new()).unwrap();
    app.root.host().show();
    let checkbox = find(&app, |content| {
        matches!(content, NodeContent::Checkbox { .. })
    });
    let toggle = find(&app, |content| {
        matches!(content, NodeContent::Switch { .. })
    });
    app.root.host().activate(checkbox).unwrap();
    app.poll_events().unwrap();
    let state: serde_json::Value = serde_json::from_str(&app.state()).unwrap();
    assert_eq!(state["extensions"]["checked"], true);
    app.root.host().activate(toggle).unwrap();
    app.poll_events().unwrap();
    let state: serde_json::Value = serde_json::from_str(&app.state()).unwrap();
    assert_eq!(state["extensions"]["checked"], false);
    click(&mut app, "Open editor");
    let input = find(
        &app,
        |content| matches!(content, NodeContent::Input { placeholder, .. } if placeholder == "Dialog name / 中文"),
    );
    let identity = app.root.host().control_identity(input);
    app.root
        .host()
        .replace_selection(input, "原生组件 😀")
        .unwrap();
    app.poll_events().unwrap();
    let state: serde_json::Value = serde_json::from_str(&app.state()).unwrap();
    assert_eq!(state["value"], "原生组件 😀");
    click(&mut app, "Advanced");
    click(&mut app, "Return");
    assert_eq!(app.root.host().control_identity(input), identity);
    assert_eq!(app.root.host().text(input).unwrap(), "原生组件 😀");
    click(&mut app, "Done");
    click(&mut app, "Open editor");
    assert_eq!(app.root.host().control_identity(input), identity);
    assert!(app.root.host().undo(input).unwrap());
    app.poll_events().unwrap();
    let state: serde_json::Value = serde_json::from_str(&app.state()).unwrap();
    assert_eq!(state["value"], "");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        app.root.host().pump();
        app.poll_events().unwrap();
        let native = app.root.host().diagnostics();
        if native["images_ready"] == 1 && native["images_failed"] == 1 {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "{native}");
        app.root.host().wait_for_events(50).unwrap();
    }
}
