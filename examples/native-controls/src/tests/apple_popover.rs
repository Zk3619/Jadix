use super::apple::*;
use serde_json::{json, Value};
use std::ffi::{c_char, c_void, CStr, CString};

#[derive(Default)]
struct Peer {
    snapshots: Vec<Value>,
    clears: usize,
    fail: bool,
}

unsafe extern "C" fn measure(_: *mut c_void, source: *const c_char, output: *mut f64) -> bool {
    let request: Value =
        serde_json::from_slice(unsafe { CStr::from_ptr(source) }.to_bytes()).unwrap();
    unsafe {
        output.write(request["maxWidth"].as_f64().unwrap().min(96.0));
        output
            .add(1)
            .write(request["maxHeight"].as_f64().unwrap().min(28.0));
    }
    true
}
unsafe extern "C" fn publish(context: *mut c_void, source: *const c_char) -> bool {
    let peer = unsafe { &mut *context.cast::<Peer>() };
    peer.snapshots
        .push(serde_json::from_slice(unsafe { CStr::from_ptr(source) }.to_bytes()).unwrap());
    !peer.fail
}
unsafe extern "C" fn clear(context: *mut c_void) {
    unsafe { &mut *context.cast::<Peer>() }.clears += 1;
}
fn mount(peer: &mut Peer) -> u64 {
    let root = unsafe {
        jadix_native_create(
            (peer as *mut Peer).cast(),
            measure,
            publish,
            clear,
            420.0,
            720.0,
        )
    };
    assert_ne!(root, 0);
    root
}
fn reply(pointer: *mut c_char) -> Value {
    assert!(!pointer.is_null());
    let reply = serde_json::from_slice(unsafe { CStr::from_ptr(pointer) }.to_bytes()).unwrap();
    unsafe { jadix_native_string_free(pointer) };
    reply
}
fn state(root: u64) -> Value {
    serde_json::from_str(reply(jadix_native_state(root))["value"].as_str().unwrap()).unwrap()
}
fn node(peer: &Peer, field: &str, value: &str) -> Value {
    peer.snapshots.last().unwrap()["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node[field] == value)
        .unwrap()
        .clone()
}
fn event(root: u64, node: &Value, payload: Value) -> Value {
    let source = CString::new(
        json!({"root": root.to_string(), "node": node["id"], "payload": payload}).to_string(),
    )
    .unwrap();
    reply(unsafe { jadix_native_event(root, source.as_ptr()) })
}

#[test]
fn apple_popover_c_abi_publishes_typed_anchors_and_retains_lazy_editor_across_resize() {
    let mut peer = Peer::default();
    let root = mount(&mut peer);
    let closed = node(&peer, "name", "Profile");
    assert_eq!(closed["kind"], "popover");
    assert_eq!(closed["open"], false);
    assert_eq!(closed["placement"], "bottom-start");
    assert_eq!(closed["gap"], 4);
    let trigger = node(&peer, "text", "Edit profile");
    assert_eq!(closed["anchor"], trigger["id"]);
    assert_eq!(
        event(root, &trigger, json!({"kind": "activate"}))["ok"],
        true
    );
    let opened = node(&peer, "name", "Profile");
    let input = node(&peer, "placeholder", "Profile name");
    assert_eq!(opened["id"], closed["id"]);
    assert_eq!(opened["open"], true);
    assert!(opened["frame"][2].as_f64().unwrap() > 0.0);
    assert_eq!(
        reply(jadix_native_resize(root, 400.0, 600.0, 2))["ok"],
        true
    );
    assert_eq!(
        node(&peer, "placeholder", "Profile name")["id"],
        input["id"]
    );
    let revision = node(&peer, "name", "Profile")["editRevision"]
        .as_str()
        .unwrap()
        .parse::<u64>()
        .unwrap()
        + 1;
    let dismissal = json!({"kind": "dismiss", "backdrop": false, "revision": revision.to_string()});
    assert_eq!(event(root, &opened, dismissal.clone())["ok"], true);
    assert_eq!(node(&peer, "name", "Profile")["open"], false);
    assert_eq!(
        node(&peer, "placeholder", "Profile name")["id"],
        input["id"]
    );
    assert_eq!(event(root, &opened, dismissal)["ok"], false);
    jadix_native_dispose(root);
    assert_eq!(peer.clears, 1);
}

#[test]
fn apple_popover_c_abi_keeps_preedit_local_and_nested_context_updates_shared_state() {
    let mut peer = Peer::default();
    let root = mount(&mut peer);
    assert_eq!(
        event(
            root,
            &node(&peer, "text", "Edit profile"),
            json!({"kind": "activate"})
        )["ok"],
        true
    );
    let input = node(&peer, "placeholder", "Profile name");
    assert_eq!(
        event(
            root,
            &input,
            json!({"kind": "edit", "value": "zhong", "revision": "1", "selection": [5,5], "composing": [0,5]})
        )["ok"],
        true
    );
    assert_eq!(state(root)["value"], "");
    assert_eq!(
        reply(jadix_native_resize(root, 420.0, 500.0, 2))["ok"],
        true
    );
    assert_eq!(
        event(
            root,
            &input,
            json!({"kind": "edit", "value": "中文😀", "revision": "2", "selection": [4,4], "composing": null})
        )["ok"],
        true
    );
    assert_eq!(state(root)["value"], "中文😀");
    assert_eq!(
        event(
            root,
            &node(&peer, "text", "More options"),
            json!({"kind": "activate"})
        )["ok"],
        true
    );
    assert_eq!(node(&peer, "name", "Details")["open"], true);
    assert_eq!(
        event(
            root,
            &node(&peer, "text", "Apply option"),
            json!({"kind": "activate"})
        )["ok"],
        true
    );
    assert_eq!(state(root)["count"], 1);
    assert_eq!(node(&peer, "name", "Details")["open"], false);
    assert_eq!(node(&peer, "name", "Profile")["open"], true);
    assert_eq!(
        event(
            root,
            &node(&peer, "text", "Reset name"),
            json!({"kind": "activate"})
        )["ok"],
        true
    );
    assert_eq!(state(root)["value"], "Reset 中文 😀");
    jadix_native_dispose(root);
    assert_eq!(peer.clears, 1);
}

#[test]
fn apple_popover_c_abi_retires_failed_root_and_rejects_its_callbacks_in_replacement() {
    let mut peer = Peer::default();
    let root = mount(&mut peer);
    let trigger = node(&peer, "text", "Edit profile");
    peer.fail = true;
    assert_eq!(
        event(root, &trigger, json!({"kind": "activate"}))["ok"],
        false
    );
    assert_eq!(peer.clears, 1);
    assert_eq!(
        event(root, &trigger, json!({"kind": "activate"}))["ok"],
        false
    );
    jadix_native_dispose(root);
    assert_eq!(peer.clears, 1);
    peer.fail = false;
    let replacement = mount(&mut peer);
    assert_ne!(root, replacement);
    assert_eq!(
        event(replacement, &trigger, json!({"kind": "activate"}))["ok"],
        false
    );
    assert_eq!(state(replacement)["count"], 0);
    assert_eq!(node(&peer, "name", "Profile")["open"], false);
    let source = CString::new(json!({"count": 7, "value": "恢复😀"}).to_string()).unwrap();
    assert_eq!(
        reply(unsafe { jadix_native_restore(replacement, source.as_ptr()) })["ok"],
        true
    );
    assert_eq!(state(replacement)["value"], "恢复😀");
    jadix_native_dispose(replacement);
    assert_eq!(peer.clears, 2);
}
