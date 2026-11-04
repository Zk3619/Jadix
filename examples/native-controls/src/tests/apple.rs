use super::apple::*;
use serde_json::{json, Value};
use std::ffi::{c_char, c_void, CStr};

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

fn read(pointer: *mut c_char) -> Value {
    assert!(!pointer.is_null());
    let result = serde_json::from_slice(unsafe { CStr::from_ptr(pointer) }.to_bytes()).unwrap();
    unsafe { jadix_native_string_free(pointer) };
    result
}

#[test]
fn apple_control_restore_validates_before_mutation_and_reports_unsupported_storage() {
    let mut peer = Box::<Peer>::default();
    let id = unsafe {
        jadix_native_create(
            (&mut *peer as *mut Peer).cast(),
            measure,
            publish,
            clear,
            360.0,
            640.0,
        )
    };
    let mut saved: Value =
        serde_json::from_str(read(jadix_native_state(id))["value"].as_str().unwrap()).unwrap();
    saved["count"] = json!(42);
    saved["value"] = json!("恢复中文😀");
    let source = std::ffi::CString::new(saved.to_string()).unwrap();
    assert_eq!(
        read(unsafe { jadix_native_restore(id, source.as_ptr()) })["ok"],
        true
    );
    let before = read(jadix_native_state(id));
    let actual: Value = serde_json::from_str(before["value"].as_str().unwrap()).unwrap();
    assert_eq!(actual["count"], 42);
    assert_eq!(actual["value"], "恢复中文😀");
    let snapshots = peer.snapshots.len();
    for bytes in [b"{}".to_vec(), vec![0xc3, 0x28]] {
        let invalid = std::ffi::CString::new(bytes).unwrap();
        assert_eq!(
            read(unsafe { jadix_native_restore(id, invalid.as_ptr()) })["ok"],
            false
        );
    }
    assert_eq!(
        read(unsafe { jadix_native_restore(id, std::ptr::null()) })["ok"],
        false
    );
    assert_eq!(
        read(unsafe { jadix_native_storage_result(id, std::ptr::null()) })["ok"],
        false
    );
    assert_eq!(read(jadix_native_state(id)), before);
    assert_eq!(peer.snapshots.len(), snapshots);
    jadix_native_dispose(id);
    assert_eq!(
        read(unsafe { jadix_native_restore(id, source.as_ptr()) })["ok"],
        false
    );
    assert_eq!(peer.clears, 1);
}

#[test]
fn apple_viewport_transitions_preserve_state_and_native_node_identity() {
    let mut peer = Box::<Peer>::default();
    let id = unsafe {
        jadix_native_create(
            (&mut *peer as *mut Peer).cast(),
            measure,
            publish,
            clear,
            360.0,
            640.0,
        )
    };
    assert_ne!(id, 0);
    let mut saved: Value =
        serde_json::from_str(read(jadix_native_state(id))["value"].as_str().unwrap()).unwrap();
    saved["value"] = json!("视口😀אב");
    let source = std::ffi::CString::new(saved.to_string()).unwrap();
    assert_eq!(
        read(unsafe { jadix_native_restore(id, source.as_ptr()) })["ok"],
        true
    );
    let before = read(jadix_native_state(id));
    let identities: Vec<Value> = peer.snapshots.last().unwrap()["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|node| node["id"].clone())
        .collect();
    for (width, height, environment) in [(360.0, 420.0, 2), (640.0, 360.0, 3), (360.0, 640.0, 4)] {
        assert_eq!(
            read(jadix_native_resize(id, width, height, environment))["ok"],
            true
        );
        assert_eq!(read(jadix_native_state(id)), before);
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
fn apple_c_abi_owns_strings_isolates_roots_and_revokes_failed_publications() {
    let mut first = Box::<Peer>::default();
    let mut second = Box::<Peer>::default();
    let first_root = unsafe {
        jadix_native_create(
            (&mut *first as *mut Peer).cast(),
            measure,
            publish,
            clear,
            360.0,
            640.0,
        )
    };
    let second_root = unsafe {
        jadix_native_create(
            (&mut *second as *mut Peer).cast(),
            measure,
            publish,
            clear,
            360.0,
            640.0,
        )
    };
    assert!(first_root != 0 && second_root != 0 && first_root != second_root);
    let nodes = first.snapshots.last().unwrap()["nodes"].as_array().unwrap();
    assert!(nodes.iter().filter(|node| node["kind"] == "text").count() >= 102);
    let button = nodes
        .iter()
        .find(|node| node["kind"] == "button" && node["text"] == "Continue")
        .unwrap()["id"]
        .clone();
    let input = nodes.iter().rfind(|node| node["kind"] == "input").unwrap()["id"].clone();
    #[cfg(all(feature = "extended", not(feature = "gpu")))]
    {
        let checkbox = nodes
            .iter()
            .find(|node| node["kind"] == "checkbox")
            .unwrap()["id"]
            .clone();
        let payload = std::ffi::CString::new(json!({"root": first_root.to_string(), "node": checkbox, "payload": {"kind": "toggle", "checked": true, "revision": "1"}}).to_string()).unwrap();
        assert_eq!(
            read(unsafe { jadix_native_event(first_root, payload.as_ptr()) })["ok"],
            true
        );
        assert!(first.snapshots.last().unwrap()["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|node| node["text"]
                .as_str()
                .is_some_and(|text| text.starts_with("Checked: true"))));
        assert_eq!(
            read(unsafe { jadix_native_event(first_root, payload.as_ptr()) })["ok"],
            false
        );
    }
    let event = |node: &Value, payload: Value| {
        std::ffi::CString::new(
            json!({"root": first_root.to_string(), "node": node, "payload": payload}).to_string(),
        )
        .unwrap()
    };
    let click = event(&button, json!({"kind": "activate"}));
    assert_eq!(
        read(unsafe { jadix_native_event(first_root, click.as_ptr()) })["ok"],
        true
    );
    let edit = event(
        &input,
        json!({"kind": "edit", "value": "中文😀", "revision": "1", "selection": [4,4], "composing": null}),
    );
    assert_eq!(
        read(unsafe { jadix_native_event(first_root, edit.as_ptr()) })["ok"],
        true
    );
    let state: Value = serde_json::from_str(
        read(jadix_native_state(first_root))["value"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(state["count"], 1);
    assert_eq!(state["value"], "中文😀");
    let state: Value = serde_json::from_str(
        read(jadix_native_state(second_root))["value"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(state["count"], 0);
    assert_eq!(state["value"], "");
    assert_eq!(
        read(unsafe { jadix_native_event(second_root, click.as_ptr()) })["ok"],
        false
    );
    first.fail = true;
    assert_eq!(
        read(unsafe { jadix_native_event(first_root, click.as_ptr()) })["ok"],
        false
    );
    assert_eq!(first.clears, 1);
    assert_eq!(
        read(unsafe { jadix_native_event(first_root, click.as_ptr()) })["ok"],
        false
    );
    jadix_native_dispose(first_root);
    jadix_native_dispose(first_root);
    assert_eq!(first.clears, 1);
    assert_eq!(read(jadix_native_state(first_root))["ok"], false);
    jadix_native_dispose(second_root);
    assert_eq!(second.clears, 1);
}
