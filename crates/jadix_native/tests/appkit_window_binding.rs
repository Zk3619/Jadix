#![cfg(feature = "appkit")]

use jadix_core::{
    create_root, create_signal,
    window::{WindowLevel, WindowMode, WindowPresence, WindowWorkspaces},
};
use jadix_native::{
    appkit::{
        AppKitCallbacks, AppKitDeclaredWindow, AppKitDeclaredWindowOptions, AppKitHost,
        AppKitWindowAttributes, AppKitWindowBinding,
    },
    host::PlatformHost,
};
use serde_json::{json, Value};
use std::{
    ffi::{c_char, c_void, CStr, CString},
    ptr,
};

unsafe extern "C" fn measure(_: *mut c_void, _: *const c_char, _: *mut f64) -> bool {
    false
}

unsafe extern "C" fn publish(_: *mut c_void, _: *const c_char) -> bool {
    false
}

unsafe extern "C" fn clear(_: *mut c_void) {}

unsafe extern "C" fn request(context: *mut c_void, id: u64, json: *const c_char) -> bool {
    let requests = unsafe { &mut *context.cast::<Vec<(u64, Value)>>() };
    let Ok(value) = serde_json::from_slice(unsafe { CStr::from_ptr(json) }.to_bytes()) else {
        return false;
    };
    requests.push((id, value));
    true
}

#[test]
fn external_url_router_validates_before_dispatch_and_falls_back_with_owned_receipts() {
    use std::{cell::RefCell, rc::Rc};
    let mut requests = Vec::<(u64, Value)>::new();
    let mut host = unsafe {
        AppKitHost::new(
            ptr::from_mut(&mut requests).cast(),
            AppKitCallbacks {
                measure,
                publish,
                clear,
            },
        )
        .with_file_requests(request)
    };
    let routed = Rc::new(RefCell::new(Vec::new()));
    let seen = routed.clone();
    host.set_url_router(move |url| {
        seen.borrow_mut().push(url.to_string());
        if url.starts_with("denied:") {
            Err("route denied".into())
        } else {
            Ok(url.starts_with("internal:"))
        }
    });
    assert!(host.open_url("javascript:alert(1)").is_err());
    assert!(routed.borrow().is_empty());
    assert_eq!(
        host.open_url("internal:settings")
            .unwrap()
            .try_recv()
            .unwrap(),
        Ok(true)
    );
    assert!(host.open_url("denied:settings").is_err());
    assert!(requests.is_empty());
    let result = host.open_url("https://example.com/中文").unwrap();
    let (id, payload) = requests.pop().unwrap();
    assert_eq!(payload["operation"], "open_url");
    assert_eq!(payload["url"], "https://example.com/%E4%B8%AD%E6%96%87");
    host.complete_file_request(id, &CString::new(r#"{"Ok":[]}"#).unwrap())
        .unwrap();
    assert_eq!(result.try_recv().unwrap(), Ok(true));
    assert!(host
        .complete_file_request(id, &CString::new(r#"{"Ok":[]}"#).unwrap())
        .is_err());
    let invalid = host.open_url("https://example.com").unwrap();
    let id = requests.pop().unwrap().0;
    host.complete_file_request(id, &CString::new("invalid").unwrap())
        .unwrap();
    assert!(invalid.try_recv().unwrap().is_err());
    let pending = host.open_url("https://example.com/pending").unwrap();
    host.dispose();
    assert!(pending.try_recv().unwrap().is_err());
    assert!(host.open_url("internal:settings").is_err());
}

#[test]
fn scoped_declaration_applies_latest_attributes_and_closes_during_unmount() {
    let mut requests = Vec::<(u64, Value)>::new();
    let mut host = unsafe {
        AppKitHost::new(
            ptr::from_mut(&mut requests).cast(),
            AppKitCallbacks {
                measure,
                publish,
                clear,
            },
        )
        .with_window_requests(request)
    };
    let declarations = host.window_declarations();
    let (owner, (title, visible, errors)) = create_root(|| {
        let title = create_signal("First");
        let visible = create_signal(true);
        let errors = declarations
            .mount("settings", move || {
                let mut options = AppKitDeclaredWindowOptions::new(400, 300);
                options.attributes.title = Some(title.get().into());
                options.attributes.visible = Some(visible.get());
                options.attributes.mode = Some(if visible.get() {
                    WindowMode::Normal
                } else {
                    WindowMode::FullScreen
                });
                Some(options)
            })
            .unwrap();
        (title, visible, errors)
    });

    assert_eq!(requests[0].1["operation"], "open_declared");
    title.set("Second");
    title.set("Latest");
    visible.set(false);
    assert_eq!(requests.len(), 1);
    host.complete_window_request(requests[0].0, c"{\"Ok\":null}")
        .unwrap();
    assert_eq!(requests[1].1["operation"], "update_declared");
    assert_eq!(requests[1].1["attributes"]["title"], "Latest");
    assert_eq!(requests[1].1["attributes"]["visible"], false);
    assert_eq!(requests[1].1["attributes"]["mode"], "full_screen");

    drop(owner);
    assert_eq!(requests.len(), 2);
    host.complete_window_request(requests[1].0, c"{\"Err\":\"window busy\"}")
        .unwrap();
    assert_eq!(errors.try_recv().unwrap(), "window busy");
    assert_eq!(requests[2].1["operation"], "close_declared");
    host.complete_window_request(requests[2].0, c"{\"Ok\":null}")
        .unwrap();
    assert_eq!(requests.len(), 3);
}

#[test]
fn scoped_declaration_remount_waits_for_previous_window_to_close() {
    let mut requests = Vec::<(u64, Value)>::new();
    let mut host = unsafe {
        AppKitHost::new(
            ptr::from_mut(&mut requests).cast(),
            AppKitCallbacks {
                measure,
                publish,
                clear,
            },
        )
        .with_window_requests(request)
    };
    let declarations = host.window_declarations();
    let (owner, _) = create_root(|| {
        declarations
            .mount("settings", || {
                Some(AppKitDeclaredWindowOptions::new(400, 300))
            })
            .unwrap()
    });
    drop(owner);
    let (_owner, _) = create_root(|| {
        declarations
            .mount("settings", || {
                Some(AppKitDeclaredWindowOptions::new(600, 400))
            })
            .unwrap()
    });
    assert_eq!(requests.len(), 1);
    host.complete_window_request(requests[0].0, c"{\"Ok\":null}")
        .unwrap();
    assert_eq!(requests[1].1["operation"], "close_declared");
    host.dispatch_window_event(c"{\"event\":\"child_destroyed\",\"id\":\"settings\"}")
        .unwrap();
    assert_eq!(requests.len(), 2);
    host.complete_window_request(requests[1].0, c"{\"Ok\":null}")
        .unwrap();
    assert_eq!(requests[2].1["operation"], "open_declared");
    assert_eq!(requests[2].1["width"], 600);
}

#[test]
fn window_binding_tracks_acknowledged_attributes_and_retries_rejected_updates() {
    let mut requests = Vec::<(u64, Value)>::new();
    let mut host = unsafe {
        AppKitHost::new(
            ptr::from_mut(&mut requests).cast(),
            AppKitCallbacks {
                measure,
                publish,
                clear,
            },
        )
        .with_window_requests(request)
    };
    let mut binding = AppKitWindowBinding::new("settings");
    let mut desired = AppKitWindowAttributes {
        title: Some("Settings".into()),
        level: Some(WindowLevel::AboveDock),
        workspaces: Some(WindowWorkspaces::All),
        participates_in_cycle: Some(false),
        ..Default::default()
    };
    assert!(!binding.sync_attributes(&mut host, &desired).unwrap());
    let (first, payload) = &requests[0];
    assert_eq!(payload["id"], "settings");
    assert_eq!(payload["operation"], "update");
    assert_eq!(payload["attributes"]["title"], "Settings");
    assert_eq!(payload["attributes"]["level"], "above_dock");
    assert_eq!(payload["attributes"]["workspaces"], "all");
    assert_eq!(payload["attributes"]["participates_in_cycle"], false);

    desired.resizable = Some(false);
    desired.level = Some(WindowLevel::DesktopIcon);
    assert!(!binding.sync_attributes(&mut host, &desired).unwrap());
    assert_eq!(requests.len(), 1);
    host.complete_window_request(*first, c"{\"Ok\":null}")
        .unwrap();

    assert!(!binding.sync_attributes(&mut host, &desired).unwrap());
    let (second, payload) = &requests[1];
    assert_eq!(payload["attributes"]["resizable"], json!(false));
    assert_eq!(payload["attributes"]["level"], "desktop_icon");
    assert!(payload["attributes"]["title"].is_null());
    assert!(payload["attributes"]["workspaces"].is_null());
    assert!(payload["attributes"]["participates_in_cycle"].is_null());
    host.complete_window_request(*second, c"{\"Err\":\"window busy\"}")
        .unwrap();
    assert_eq!(
        binding.sync_attributes(&mut host, &desired),
        Err("window busy".into())
    );

    // Omit the title on retry. The host keeps its last applied value.
    desired.title = None;
    assert!(!binding.sync_attributes(&mut host, &desired).unwrap());
    assert_eq!(requests[2].1, requests[1].1);
    host.complete_window_request(requests[2].0, c"{\"Ok\":null}")
        .unwrap();
    assert!(binding.sync_attributes(&mut host, &desired).unwrap());
    desired.title = Some("Settings".into());
    assert!(binding.sync_attributes(&mut host, &desired).unwrap());
    assert_eq!(requests.len(), 3);
}

#[test]
fn window_binding_publishes_native_observations_separately_from_requests() {
    let mut requests = Vec::<(u64, Value)>::new();
    let mut host = unsafe {
        AppKitHost::new(
            ptr::from_mut(&mut requests).cast(),
            AppKitCallbacks {
                measure,
                publish,
                clear,
            },
        )
        .with_window_requests(request)
    };
    let (_owner, state) = create_root(|| create_signal(None));
    let binding = AppKitWindowBinding::new("settings");
    let observation = |id, mode, visible, host: &mut AppKitHost| {
        let snapshot = json!({
            "id": id, "frame": {"x": 0, "y": 0, "width": 400, "height": 300},
            "content_width": 400, "content_height": 280, "backing_scale": 2,
            "mode": mode, "key": false, "visible": visible, "occluded": false,
        });
        host.update_window_state(&CString::new(snapshot.to_string()).unwrap())
            .unwrap();
    };

    observation("settings", "normal", true, &mut host);
    binding.bind_state(&mut host, state);
    assert_eq!(state.get().unwrap().mode, WindowMode::Normal);

    let mode = binding.set_mode(&mut host, WindowMode::FullScreen).unwrap();
    let visibility = binding.set_visible(&mut host, false).unwrap();
    let restore = binding.restore(&mut host).unwrap();
    assert_eq!(
        requests[0].1,
        json!({
            "operation": "set_mode", "id": "settings", "mode": "full_screen",
        })
    );
    assert_eq!(
        requests[1].1,
        json!({
            "operation": "set_visibility", "id": "settings", "visible": false,
        })
    );
    assert_eq!(
        requests[2].1,
        json!({"operation": "restore", "id": "settings"})
    );
    for ((id, _), receipt) in requests.iter().zip([mode, visibility, restore]) {
        host.complete_window_request(*id, c"{\"Ok\":null}").unwrap();
        assert_eq!(receipt.try_recv().unwrap(), Ok(()));
    }
    let current = state.get().unwrap();
    assert_eq!(current.mode, WindowMode::Normal);
    assert!(current.visible);

    observation("settings", "full_screen", false, &mut host);
    let current = state.get().unwrap();
    assert_eq!(current.mode, WindowMode::FullScreen);
    assert!(!current.visible);

    observation("other", "maximized", true, &mut host);
    assert_eq!(state.get(), None);
    observation("settings", "maximized", true, &mut host);
    assert_eq!(state.get().unwrap().mode, WindowMode::Maximized);
    host.update_window_state(c"null").unwrap();
    assert_eq!(state.get(), None);
    observation("settings", "normal", true, &mut host);
    host.dispose();
    assert_eq!(state.get(), None);
}

#[test]
fn declaration_reconciles_latest_attributes_and_closes_on_removal() {
    let mut requests = Vec::<(u64, Value)>::new();
    let mut host = unsafe {
        AppKitHost::new(
            ptr::from_mut(&mut requests).cast(),
            AppKitCallbacks {
                measure,
                publish,
                clear,
            },
        )
        .with_window_requests(request)
    };
    let mut window = AppKitDeclaredWindow::new("settings");
    let mut desired = AppKitDeclaredWindowOptions::new(400, 300);
    desired.attributes.title = Some("Settings".into());
    assert!(!window.sync(&mut host, Some(&desired)).unwrap());
    assert_eq!(window.presence(), WindowPresence::Opening);
    assert_eq!(
        requests[0].1,
        json!({
            "operation": "open_declared", "id": "settings", "kind": "normal",
            "width": 400, "height": 300, "initial_mode": "normal",
            "initially_visible": true, "modal": false, "attributes": desired.attributes,
        })
    );
    desired.attributes.resizable = Some(false);
    host.complete_window_request(requests[0].0, c"{\"Ok\":null}")
        .unwrap();
    assert!(!window.sync(&mut host, Some(&desired)).unwrap());
    assert_eq!(window.presence(), WindowPresence::Open);
    assert_eq!(requests[1].1["operation"], "update_declared");
    assert_eq!(requests[1].1["id"], "settings");
    assert_eq!(requests[1].1["attributes"]["resizable"], json!(false));
    assert!(requests[1].1["attributes"]["title"].is_null());
    assert!(!window.sync(&mut host, None).unwrap());
    assert_eq!(requests.len(), 2);
    host.complete_window_request(requests[1].0, c"{\"Ok\":null}")
        .unwrap();
    assert!(!window.sync(&mut host, None).unwrap());
    assert_eq!(
        requests[2].1,
        json!({"operation": "close_declared", "id": "settings"})
    );
    host.dispatch_window_event(c"{\"event\":\"child_destroyed\",\"id\":\"settings\"}")
        .unwrap();
    host.complete_window_request(requests[2].0, c"{\"Ok\":null}")
        .unwrap();
    assert!(window.sync(&mut host, None).unwrap());
    assert_eq!(window.presence(), WindowPresence::Absent);
}

#[test]
fn declaration_remembers_host_closure_before_the_open_receipt() {
    let mut requests = Vec::<(u64, Value)>::new();
    let mut host = unsafe {
        AppKitHost::new(
            ptr::from_mut(&mut requests).cast(),
            AppKitCallbacks {
                measure,
                publish,
                clear,
            },
        )
        .with_window_requests(request)
    };
    let mut window = AppKitDeclaredWindow::new("settings");
    let desired = AppKitDeclaredWindowOptions::new(400, 300);
    assert!(!window.sync(&mut host, Some(&desired)).unwrap());
    host.dispatch_window_event(c"{\"event\":\"child_destroyed\",\"id\":\"settings\"}")
        .unwrap();
    host.complete_window_request(requests[0].0, c"{\"Ok\":null}")
        .unwrap();
    assert!(window.sync(&mut host, Some(&desired)).unwrap());
    assert_eq!(window.presence(), WindowPresence::ClosedByHost);
    assert!(AppKitDeclaredWindow::new("settings")
        .sync(&mut host, Some(&desired))
        .is_err());
    assert_eq!(requests.len(), 1);
    assert!(window.sync(&mut host, None).unwrap());
    assert!(!window.sync(&mut host, Some(&desired)).unwrap());
    assert_eq!(requests[0].1, requests[1].1);

    host.complete_window_request(requests[1].0, c"{\"Ok\":null}")
        .unwrap();
    assert!(window.sync(&mut host, Some(&desired)).unwrap());
    let mut changed = desired.clone();
    changed.attributes.title = Some("Renamed".into());
    assert!(!window.sync(&mut host, Some(&changed)).unwrap());
    host.dispatch_window_event(c"{\"event\":\"child_destroyed\",\"id\":\"settings\"}")
        .unwrap();
    host.complete_window_request(requests[2].0, c"{\"Err\":\"missing window\"}")
        .unwrap();
    assert!(window.sync(&mut host, Some(&changed)).unwrap());
    assert_eq!(window.presence(), WindowPresence::ClosedByHost);
    assert_eq!(requests.len(), 3);
}
