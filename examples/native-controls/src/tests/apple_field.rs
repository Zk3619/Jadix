use super::apple::*;
use serde_json::{json, Value};
use std::ffi::{c_char, c_void, CStr, CString};

#[derive(Default)]
struct Peer {
    snapshots: Vec<Value>,
    clears: usize,
}
unsafe extern "C" fn measure(_: *mut c_void, json: *const c_char, result: *mut f64) -> bool {
    let value: Value = serde_json::from_slice(unsafe { CStr::from_ptr(json) }.to_bytes()).unwrap();
    unsafe {
        result.write(value["maxWidth"].as_f64().unwrap().min(120.0));
        result
            .add(1)
            .write(value["maxHeight"].as_f64().unwrap().min(24.0));
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
    let value = serde_json::from_slice(unsafe { CStr::from_ptr(pointer) }.to_bytes()).unwrap();
    unsafe { jadix_native_string_free(pointer) };
    value
}

#[test]
fn field_c_abi_restores_draft_to_a_new_root_and_rejects_stale_roots() {
    let mut first = Box::<Peer>::default();
    let mut second = Box::<Peer>::default();
    let create = |peer: &mut Peer| unsafe {
        jadix_native_create(
            (peer as *mut Peer).cast(),
            measure,
            publish,
            clear,
            360.0,
            640.0,
        )
    };
    let root = create(&mut first);
    assert_ne!(root, 0);
    let button = first.snapshots.last().unwrap()["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["kind"] == "button" && node["text"] == "Edit request")
        .unwrap()
        .clone();
    let event = CString::new(
        json!({"root":root.to_string(),"node":button["id"],"payload":{"kind":"activate"}})
            .to_string(),
    )
    .unwrap();
    assert_eq!(
        read(unsafe { jadix_native_event(root, event.as_ptr()) })["ok"],
        true
    );
    let input = first.snapshots.last().unwrap()["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["name"] == "Contact name")
        .unwrap()
        .clone();
    let text = "Contact 東京 😀";
    let event = CString::new(json!({"root":root.to_string(),"node":input["id"],"payload":{"kind":"edit","value":text,"revision":"1","selection":[text.encode_utf16().count(),text.encode_utf16().count()],"composing":null}}).to_string()).unwrap();
    assert_eq!(
        read(unsafe { jadix_native_event(root, event.as_ptr()) })["ok"],
        true
    );
    let saved = read(jadix_native_state(root));
    assert_eq!(saved["ok"], true);
    let state = CString::new(saved["value"].as_str().unwrap()).unwrap();
    let new_root = create(&mut second);
    assert_ne!(root, new_root);
    assert_eq!(
        read(unsafe { jadix_native_restore(new_root, state.as_ptr()) })["ok"],
        true
    );
    assert_eq!(read(jadix_native_state(new_root))["value"], saved["value"]);
    assert!(second.snapshots.last().unwrap()["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|node| node["name"] == "Contact name" && node["text"] == text));
    jadix_native_dispose(root);
    assert_eq!(
        read(unsafe { jadix_native_event(root, event.as_ptr()) })["ok"],
        false
    );
    jadix_native_dispose(new_root);
    assert_eq!((first.clears, second.clears), (1, 1));
}
