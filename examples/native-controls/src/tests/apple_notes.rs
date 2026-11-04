use super::apple::*;
use serde_json::{json, Value};
use std::ffi::{c_char, c_void, CStr, CString};

#[derive(Default)]
struct Peer {
    snapshots: Vec<Value>,
    clears: usize,
    fail: bool,
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
    let peer = unsafe { &mut *context.cast::<Peer>() };
    peer.snapshots
        .push(serde_json::from_slice(unsafe { CStr::from_ptr(json) }.to_bytes()).unwrap());
    !peer.fail
}
unsafe extern "C" fn clear(context: *mut c_void) {
    unsafe { &mut *context.cast::<Peer>() }.clears += 1;
}
fn root(peer: &mut Peer) -> u64 {
    unsafe {
        jadix_native_create(
            (peer as *mut Peer).cast(),
            measure,
            publish,
            clear,
            360.0,
            640.0,
        )
    }
}
fn reply(pointer: *mut c_char) -> Value {
    assert!(!pointer.is_null());
    let result = serde_json::from_slice(unsafe { CStr::from_ptr(pointer) }.to_bytes()).unwrap();
    unsafe { jadix_native_string_free(pointer) };
    result
}
fn state(id: u64) -> Value {
    let result = reply(jadix_native_state(id));
    assert_eq!(result["ok"], true);
    serde_json::from_str(result["value"].as_str().unwrap()).unwrap()
}
fn event(id: u64, node: &Value, payload: Value) -> Value {
    let source =
        CString::new(json!({"root":id.to_string(),"node":node,"payload":payload}).to_string())
            .unwrap();
    reply(unsafe { jadix_native_event(id, source.as_ptr()) })
}
fn node(peer: &Peer, kind: &str, property: &str, value: &str) -> Value {
    peer.snapshots.last().unwrap()["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["kind"] == kind && node[property] == value)
        .unwrap()["id"]
        .clone()
}
fn edit(id: u64, peer: &Peer, placeholder: &str, value: &str) {
    let target = node(peer, "input", "placeholder", placeholder);
    assert_eq!(
        event(
            id,
            &target,
            json!({"kind":"edit","value":value,"revision":"1","selection":[value.encode_utf16().count(),value.encode_utf16().count()],"composing":null})
        )["ok"],
        true
    );
}
#[test]
fn notes_c_abi_restore_save_ack_and_utf8_rejections_are_atomic() {
    let mut peer = Box::<Peer>::default();
    let id = root(&mut peer);
    assert_ne!(id, 0);
    assert_eq!(state(id)["application"], "notes");
    edit(id, &peer, "Title / 标题", "C ABI 中文😀");
    let button = node(&peer, "button", "text", "Save note");
    assert_eq!(event(id, &button, json!({"kind":"activate"}))["ok"], true);
    assert_eq!(state(id)["storage_pending"], true);
    let error = CString::new("disk quota").unwrap();
    assert_eq!(
        reply(unsafe { jadix_native_storage_result(id, error.as_ptr()) })["ok"],
        true
    );
    assert!(state(id)["status"].as_str().unwrap().contains("disk quota"));
    assert_eq!(
        reply(unsafe { jadix_native_storage_result(id, std::ptr::null()) })["ok"],
        true
    );
    assert_eq!(state(id)["storage_pending"], false);
    let before = state(id);
    assert_eq!(
        reply(unsafe { jadix_native_restore(id, std::ptr::null()) })["ok"],
        false
    );
    let invalid = CString::new(vec![0xc3, 0x28]).unwrap();
    assert_eq!(
        reply(unsafe { jadix_native_restore(id, invalid.as_ptr()) })["ok"],
        false
    );
    assert_eq!(
        reply(unsafe { jadix_native_storage_result(id, invalid.as_ptr()) })["ok"],
        false
    );
    let malformed = CString::new("{}").unwrap();
    assert_eq!(
        reply(unsafe { jadix_native_restore(id, malformed.as_ptr()) })["ok"],
        false
    );
    assert_eq!(state(id), before);
    let archive = CString::new(before["archive"].to_string()).unwrap();
    assert_eq!(
        reply(unsafe { jadix_native_restore(id, archive.as_ptr()) })["ok"],
        true
    );
    assert_eq!(state(id)["archive"], before["archive"]);
    assert_eq!(state(id)["selected"], Value::Null);
    jadix_native_dispose(id);
    assert_eq!(peer.clears, 1);
    assert_eq!(
        reply(unsafe { jadix_native_restore(id, archive.as_ptr()) })["ok"],
        false
    );
    assert_eq!(
        reply(unsafe { jadix_native_storage_result(id, std::ptr::null()) })["ok"],
        false
    );
}
#[test]
fn notes_viewport_transitions_preserve_draft_and_native_node_identity() {
    let mut peer = Box::<Peer>::default();
    let id = root(&mut peer);
    assert_ne!(id, 0);
    edit(id, &peer, "Title / 标题", "视口😀אב");
    let before = state(id);
    let identities: Vec<Value> = peer.snapshots.last().unwrap()["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|node| node["id"].clone())
        .collect();
    for (width, height, environment) in [(360.0, 420.0, 2), (640.0, 360.0, 3), (360.0, 640.0, 4)] {
        assert_eq!(
            reply(jadix_native_resize(id, width, height, environment))["ok"],
            true
        );
        assert_eq!(state(id), before);
        let snapshot = peer.snapshots.last().unwrap();
        let nodes = snapshot["nodes"].as_array().unwrap();
        assert_eq!(
            nodes
                .iter()
                .map(|node| node["id"].clone())
                .collect::<Vec<_>>(),
            identities
        );
        assert_eq!(nodes[0]["frame"][2], f64::from(width));
        assert_eq!(nodes[0]["frame"][3], f64::from(height));
    }
    jadix_native_dispose(id);
    assert_eq!(peer.clears, 1);
}

#[test]
fn notes_c_abi_roots_are_isolated_and_failed_publish_revokes_callbacks() {
    let mut first = Box::<Peer>::default();
    let mut second = Box::<Peer>::default();
    let first_id = root(&mut first);
    let second_id = root(&mut second);
    assert_ne!(first_id, second_id);
    edit(first_id, &first, "Title / 标题", "first");
    assert_eq!(state(second_id)["draft"]["title"], "");
    let button = node(&first, "button", "text", "Save note");
    assert_eq!(
        event(second_id, &button, json!({"kind":"activate"}))["ok"],
        false
    );
    first.fail = true;
    assert_eq!(
        reply(unsafe { jadix_native_storage_result(first_id, std::ptr::null()) })["ok"],
        false
    );
    assert_eq!(first.clears, 1);
    assert_eq!(
        event(first_id, &button, json!({"kind":"activate"}))["ok"],
        false
    );
    jadix_native_dispose(first_id);
    jadix_native_dispose(second_id);
    assert_eq!(first.clears, 1);
    assert_eq!(second.clears, 1);
}
