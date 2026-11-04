use super::apple::*;
use serde_json::{json, Value};
use std::ffi::{c_char, c_void, CStr, CString};

#[derive(Default)]
struct Peer {
    snapshots: Vec<Value>,
    clears: usize,
}

unsafe extern "C" fn measure(_: *mut c_void, json: *const c_char, output: *mut f64) -> bool {
    let request: Value =
        serde_json::from_slice(unsafe { CStr::from_ptr(json) }.to_bytes()).unwrap();
    unsafe {
        output.write(request["maxWidth"].as_f64().unwrap().min(96.0));
        output
            .add(1)
            .write(request["maxHeight"].as_f64().unwrap().min(28.0));
    }
    true
}
unsafe extern "C" fn publish(context: *mut c_void, json: *const c_char) -> bool {
    unsafe { &mut *context.cast::<Peer>() }
        .snapshots
        .push(serde_json::from_slice(unsafe { CStr::from_ptr(json) }.to_bytes()).unwrap());
    true
}
unsafe extern "C" fn clear(context: *mut c_void) {
    unsafe { &mut *context.cast::<Peer>() }.clears += 1;
}
fn read(pointer: *mut c_char) -> Value {
    assert!(!pointer.is_null());
    let result = serde_json::from_slice(unsafe { CStr::from_ptr(pointer) }.to_bytes()).unwrap();
    unsafe { jadix_native_string_free(pointer) };
    result
}
fn create(peer: &mut Peer) -> u64 {
    let root = unsafe {
        jadix_native_create(
            (peer as *mut Peer).cast(),
            measure,
            publish,
            clear,
            360.0,
            640.0,
        )
    };
    assert_ne!(root, 0);
    root
}
fn state(root: u64) -> Value {
    let result = read(jadix_native_state(root));
    assert_eq!(result["ok"], true);
    serde_json::from_str(result["value"].as_str().unwrap()).unwrap()
}
fn node(peer: &Peer, enabled: bool) -> Value {
    peer.snapshots.last().unwrap()["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["kind"] == "select" && node["enabled"] == enabled)
        .unwrap()
        .clone()
}
fn event(root: u64, node: &Value, payload: Value) -> Value {
    let source = CString::new(
        json!({"root": root.to_string(), "node": node["id"], "payload": payload}).to_string(),
    )
    .unwrap();
    read(unsafe { jadix_native_event(root, source.as_ptr()) })
}

#[test]
fn apple_select_c_abi_routes_versioned_values_and_retains_identity_on_resize() {
    let mut peer = Box::<Peer>::default();
    let root = create(&mut peer);
    let choice = node(&peer, true);
    assert_eq!(choice["options"].as_array().unwrap().len(), 3);
    assert_eq!(choice["options"][2]["enabled"], false);
    assert_eq!(choice["placeholder"], "Choose fruit");
    assert_eq!(
        event(
            root,
            &choice,
            json!({"kind":"select", "value":"apple", "revision":"1"})
        )["ok"],
        true
    );
    assert_eq!(state(root)["value"], "apple");
    let current = node(&peer, true);
    assert_eq!(current["id"], choice["id"]);
    assert_eq!(current["editRevision"], "1");
    for (width, height, environment) in [(640.0, 360.0, 2), (360.0, 420.0, 3), (360.0, 640.0, 4)] {
        assert_eq!(
            read(jadix_native_resize(root, width, height, environment))["ok"],
            true
        );
        assert_eq!(node(&peer, true)["id"], choice["id"]);
        assert_eq!(node(&peer, true)["value"], "apple");
        assert_eq!(state(root)["value"], "apple");
    }
    jadix_native_dispose(root);
    assert_eq!(peer.clears, 1);
    assert_eq!(read(jadix_native_state(root))["ok"], false);
}

#[test]
fn apple_select_c_abi_rejects_disabled_unknown_stale_and_wrong_kind_without_publication() {
    let mut peer = Box::<Peer>::default();
    let root = create(&mut peer);
    let choice = node(&peer, true);
    let disabled = node(&peer, false);
    let initial = state(root);
    let publications = peer.snapshots.len();
    for (target, payload) in [
        (
            &choice,
            json!({"kind":"select", "value":"blocked", "revision":"1"}),
        ),
        (
            &choice,
            json!({"kind":"select", "value":"missing", "revision":"1"}),
        ),
        (
            &choice,
            json!({"kind":"select", "value":"apple", "revision":"01"}),
        ),
        (
            &choice,
            json!({"kind":"toggle", "checked":true, "revision":"1"}),
        ),
        (
            &disabled,
            json!({"kind":"select", "value":"banana", "revision":"1"}),
        ),
    ] {
        assert_eq!(event(root, target, payload)["ok"], false);
        assert_eq!(state(root), initial);
        assert_eq!(peer.snapshots.len(), publications);
    }
    assert_eq!(
        event(
            root,
            &choice,
            json!({"kind":"select", "value":"banana", "revision":"1"})
        )["ok"],
        true
    );
    let publications = peer.snapshots.len();
    assert_eq!(
        event(
            root,
            &choice,
            json!({"kind":"select", "value":"apple", "revision":"1"})
        )["ok"],
        false
    );
    assert_eq!(state(root)["value"], "banana");
    assert_eq!(peer.snapshots.len(), publications);
    jadix_native_dispose(root);
}

#[test]
fn apple_select_c_abi_restore_is_atomic_and_retired_callbacks_cannot_mutate_replacement_root() {
    let mut peer = Box::<Peer>::default();
    let root = create(&mut peer);
    let choice = node(&peer, true);
    assert_eq!(
        event(
            root,
            &choice,
            json!({"kind":"select", "value":"apple", "revision":"1"})
        )["ok"],
        true
    );
    let before = state(root);
    let publications = peer.snapshots.len();
    for value in ["missing", "x"] {
        let source = CString::new(json!({"count":42,"value":value}).to_string()).unwrap();
        assert_eq!(
            read(unsafe { jadix_native_restore(root, source.as_ptr()) })["ok"],
            false
        );
        assert_eq!(state(root), before);
        assert_eq!(peer.snapshots.len(), publications);
    }
    let empty = CString::new(json!({"count":7,"value":""}).to_string()).unwrap();
    assert_eq!(
        read(unsafe { jadix_native_restore(root, empty.as_ptr()) })["ok"],
        true
    );
    assert_eq!(node(&peer, true)["value"], "");
    let saved = CString::new(before.to_string()).unwrap();
    jadix_native_dispose(root);
    assert_eq!(peer.clears, 1);
    let replacement = create(&mut peer);
    assert_ne!(replacement, root);
    assert_eq!(
        read(unsafe { jadix_native_restore(replacement, saved.as_ptr()) })["ok"],
        true
    );
    assert_eq!(state(replacement), before);
    assert_eq!(
        event(
            root,
            &choice,
            json!({"kind":"select", "value":"banana", "revision":"2"})
        )["ok"],
        false
    );
    assert_eq!(
        event(
            replacement,
            &choice,
            json!({"kind":"select", "value":"banana", "revision":"2"})
        )["ok"],
        false
    );
    assert_eq!(state(replacement), before);
    jadix_native_dispose(replacement);
    assert_eq!(peer.clears, 2);
}
