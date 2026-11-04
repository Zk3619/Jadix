use super::PlatformApp as NativeApp;
use jadix_native::uikit::{UIKitCallbacks, UIKitHost};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    ffi::{c_char, c_void, CStr, CString},
};

thread_local! {
    static APPS: RefCell<BTreeMap<u64, NativeApp<UIKitHost>>> = const { RefCell::new(BTreeMap::new()) };
}

#[no_mangle]
pub unsafe extern "C" fn jadix_native_create(
    context: *mut c_void,
    measure: unsafe extern "C" fn(*mut c_void, *const c_char, *mut f64) -> bool,
    publish: unsafe extern "C" fn(*mut c_void, *const c_char) -> bool,
    clear: unsafe extern "C" fn(*mut c_void),
    width: f32,
    height: f32,
) -> u64 {
    let host = unsafe {
        UIKitHost::new(
            context,
            UIKitCallbacks {
                measure,
                publish,
                clear,
            },
        )
    };
    let Ok(app) = NativeApp::new(host, [width, height], 0, String::new()) else {
        return 0;
    };
    let id = app.root.model().id().token().parse().unwrap();
    APPS.with(|apps| {
        apps.borrow_mut().insert(id, app);
    });
    id
}

fn reply(result: Result<String, String>) -> *mut c_char {
    let value = match result {
        Ok(value) => serde_json::json!({"ok": true, "value": value}),
        Err(error) => serde_json::json!({"ok": false, "error": error}),
    };
    CString::new(value.to_string()).unwrap().into_raw()
}

fn with_app(
    id: u64,
    action: impl FnOnce(&mut NativeApp<UIKitHost>) -> Result<String, String>,
) -> *mut c_char {
    reply(APPS.with(|apps| {
        let mut apps = apps
            .try_borrow_mut()
            .map_err(|_| "reentrant UIKit event".to_owned())?;
        action(apps.get_mut(&id).ok_or("disposed UIKit root")?)
    }))
}

#[no_mangle]
pub unsafe extern "C" fn jadix_native_event(id: u64, json: *const c_char) -> *mut c_char {
    if json.is_null() {
        return reply(Err("null UIKit event".into()));
    }
    let text = match unsafe { CStr::from_ptr(json) }.to_str() {
        Ok(text) => text,
        Err(error) => return reply(Err(error.to_string())),
    };
    with_app(id, |app| {
        app.event(text)?;
        Ok(app.state())
    })
}

#[no_mangle]
pub extern "C" fn jadix_native_resize(
    id: u64,
    width: f32,
    height: f32,
    environment: u64,
) -> *mut c_char {
    with_app(id, |app| {
        app.resize([width, height], environment)?;
        Ok(app.state())
    })
}

#[no_mangle]
pub extern "C" fn jadix_native_state(id: u64) -> *mut c_char {
    with_app(id, |app| Ok(app.state()))
}

#[no_mangle]
pub unsafe extern "C" fn jadix_native_restore(id: u64, json: *const c_char) -> *mut c_char {
    if json.is_null() {
        return reply(Err("null UIKit state".into()));
    }
    let text = match unsafe { CStr::from_ptr(json) }.to_str() {
        Ok(text) => text,
        Err(error) => return reply(Err(error.to_string())),
    };
    with_app(id, |app| {
        app.restore(text)?;
        Ok(app.state())
    })
}

#[no_mangle]
pub unsafe extern "C" fn jadix_native_storage_result(id: u64, error: *const c_char) -> *mut c_char {
    let error = if error.is_null() {
        None
    } else {
        match unsafe { CStr::from_ptr(error) }.to_str() {
            Ok(error) => Some(error.to_string()),
            Err(error) => return reply(Err(error.to_string())),
        }
    };
    #[cfg(all(feature = "notes", not(feature = "gpu")))]
    {
        with_app(id, |app| {
            app.storage_result(error)?;
            Ok(app.state())
        })
    }
    #[cfg(not(all(feature = "notes", not(feature = "gpu"))))]
    {
        let _ = (id, error);
        reply(Err("storage callbacks require the notes application".into()))
    }
}

#[no_mangle]
pub extern "C" fn jadix_native_dispose(id: u64) {
    APPS.with(|apps| {
        apps.borrow_mut().remove(&id);
    });
}

#[no_mangle]
pub unsafe extern "C" fn jadix_native_string_free(value: *mut c_char) {
    if !value.is_null() {
        drop(unsafe { CString::from_raw(value) });
    }
}
