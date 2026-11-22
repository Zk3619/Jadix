//! Apple C ABI shared by independently authored Rust applications.
//! The application supplies the retained model; the bridge owns root and callback lifetimes.
#[macro_export]
macro_rules! apple_application {
    ($application:ident) => {
type NativeApp<H> = $application<H>;
use jadix_core::menu::{MenuItem, MenuRole};
use jadix_core::reactivity::{batch, create_root, OwnerHandle};
use $crate::apple::{
    AppleCallbacks, AppleDeclaredWindowOptions, AppleFileRequestCallback, AppleHost,
    AppleQuitCleanup, AppleRequestCallback, AppleWindowContent,
};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    ffi::{c_char, c_void, CStr, CString},
    sync::mpsc::Receiver,
};

thread_local! {
    static TERMINALS: RefCell<BTreeMap<u64, jadix_core::terminal::TerminalScreen>> = RefCell::default();
    static NEXT_TERMINAL: std::cell::Cell<u64> = const { std::cell::Cell::new(1) };
    static APPS: RefCell<BTreeMap<u64, NativeApp<AppleHost>>> = const { RefCell::new(BTreeMap::new()) };
    static QUIT_CLEANUP: RefCell<AppleQuitCleanup> = RefCell::default();
}

// Shared viewport policy; the platform retains logical layout when evicting views.
#[no_mangle]
pub unsafe extern "C" fn jadix_native_viewport_plan(json: *const c_char) -> *mut c_char {
    if json.is_null() { return std::ptr::null_mut(); }
    let Ok(json) = unsafe { CStr::from_ptr(json) }.to_str() else { return std::ptr::null_mut(); };
    if json.len() > 32 * 1024 * 1024 { return std::ptr::null_mut(); }
    let Ok(nodes) = serde_json::from_str::<Vec<jadix_core::presentation::viewport::CullNode>>(json) else { return std::ptr::null_mut(); };
    jadix_core::presentation::viewport::materialization_plan(&nodes).ok()
        .and_then(|plan| CString::new(serde_json::to_string(&plan).ok()?).ok())
        .map_or(std::ptr::null_mut(), CString::into_raw)
}

#[no_mangle]
pub unsafe extern "C" fn jadix_native_rich_html(html: *const c_char) -> *mut c_char {
    if html.is_null() { return std::ptr::null_mut(); }
    let Ok(html) = unsafe { CStr::from_ptr(html) }.to_str() else { return std::ptr::null_mut(); };
    if html.len() > jadix_core::rich_editor::RichEditorSpec::MAX_HTML { return std::ptr::null_mut(); }
    $crate::apple::sanitize_rich_html(html).ok().and_then(|s| CString::new(s).ok())
        .map_or(std::ptr::null_mut(), CString::into_raw)
}

#[no_mangle]
pub unsafe extern "C" fn jadix_native_rich_html_plan(html: *const c_char) -> *mut c_char {
    if html.is_null() { return std::ptr::null_mut(); }
    let Ok(html) = unsafe { CStr::from_ptr(html) }.to_str() else { return std::ptr::null_mut(); };
    $crate::apple::rich_html_plan(html).ok().and_then(|s| CString::new(s).ok())
        .map_or(std::ptr::null_mut(), CString::into_raw)
}
// Negative dimensions denote an unspecified axis / unavailable natural size.
#[no_mangle]
pub extern "C" fn jadix_native_rich_image_size(width: f64, height: f64, natural_width: f64, natural_height: f64) -> *mut c_char {
    let size = $crate::apple::rich_image_dimensions((width >= 0.0).then_some(width), (height >= 0.0).then_some(height),
        (natural_width > 0.0 && natural_height > 0.0).then_some([natural_width, natural_height]));
    CString::new(serde_json::to_string(&size).unwrap()).map_or(std::ptr::null_mut(), CString::into_raw)
}

// -1 fails, 0 waits, 1 activates. A limit of -1 denotes an unspecified axis.
#[no_mangle]
pub extern "C" fn jadix_native_pan_decision(axis: u8, x: f64, y: f64, active_x: f64, active_y: f64, fail_x: f64, fail_y: f64) -> i8 {
    use jadix_core::presentation::interaction::{PanActivation, PanAxis, PanDecision};
    let axis = match axis { 0 => PanAxis::Both, 1 => PanAxis::Horizontal, 2 => PanAxis::Vertical, _ => return -1 };
    let optional = |value: f64| (value != -1.0).then_some(value);
    let gate = PanActivation { active_offset_x: optional(active_x), active_offset_y: optional(active_y),
        fail_offset_x: optional(fail_x), fail_offset_y: optional(fail_y) };
    match gate.decide(axis, [x, y]) { PanDecision::Fail => -1, PanDecision::Wait => 0, PanDecision::Activate => 1 }
}

// All updater ABI calls run on the application's main thread. The transport
// owns binary buffers only for this call; no root or application borrow is held.
#[no_mangle]
pub unsafe extern "C" fn jadix_native_updater_configure(json: *const c_char, platform: *const c_char, install: bool) -> bool {
    if json.is_null() || platform.is_null() { return false; }
    let (Ok(json), Ok(platform)) = (unsafe { CStr::from_ptr(json) }.to_str(), unsafe { CStr::from_ptr(platform) }.to_str()) else { return false; };
    if json.len() > 64 * 1024 || platform.len() > 64 { return false; }
    let Ok(config) = serde_json::from_str::<Option<jadix_core::updater::Config>>(json) else { return false; };
    $crate::apple::configure_application_updater(config, platform, install).is_ok()
}
#[no_mangle]
pub extern "C" fn jadix_native_updater_commands() -> *mut c_char {
    serde_json::to_string(&$crate::apple::application_updater().take_commands()).ok()
        .and_then(|s| CString::new(s).ok()).map_or(std::ptr::null_mut(), CString::into_raw)
}
#[no_mangle]
pub unsafe extern "C" fn jadix_native_updater_event(id: u32, json: *const c_char) -> bool {
    if json.is_null() { return false; }
    let Ok(json) = unsafe { CStr::from_ptr(json) }.to_str() else { return false; };
    if json.len() > 5 * 1024 * 1024 { return false; }
    let Ok(event) = serde_json::from_str(json) else { return false; };
    $crate::apple::application_updater().event(id, event)
}
#[no_mangle]
pub unsafe extern "C" fn jadix_native_updater_chunk(id: u32, bytes: *const u8, length: usize) -> bool {
    if bytes.is_null() || length > 64 * 1024 { return false; }
    $crate::apple::application_updater().chunk(id, unsafe { std::slice::from_raw_parts(bytes, length) })
}
// Extraction has no reactive/main-thread state and may run on a worker queue.
// Null means success; otherwise the caller owns the returned error string.
#[no_mangle]
pub unsafe extern "C" fn jadix_native_updater_unpack(archive: *const c_char, destination: *const c_char) -> *mut c_char {
    let result = (|| -> Result<(), String> {
        if archive.is_null() || destination.is_null() { return Err("missing update archive path".into()); }
        let archive = unsafe { CStr::from_ptr(archive) }.to_str().map_err(|_| "invalid archive path")?;
        let destination = unsafe { CStr::from_ptr(destination) }.to_str().map_err(|_| "invalid staging path")?;
        if archive.len() > 16384 || destination.len() > 16384 { return Err("update path exceeds limit".into()); }
        jadix_core::updater::archive::unpack(std::path::Path::new(archive), std::path::Path::new(destination))
    })();
    result.err().and_then(|error| CString::new(error).ok()).map_or(std::ptr::null_mut(), CString::into_raw)
}

#[no_mangle]
pub extern "C" fn jadix_native_collect_app_quit() {
    QUIT_CLEANUP.with(|cleanup| {
        if let Ok(mut cleanup) = cleanup.try_borrow_mut() {
            cleanup.collect();
        }
    });
}

#[no_mangle]
pub extern "C" fn jadix_terminal_screen_new(columns: u16, rows: u16, scrollback: usize) -> u64 {
    let Ok(screen) = jadix_core::terminal::TerminalScreen::new(jadix_core::terminal::TerminalSpec { columns, rows, scrollback }) else { return 0; };
    TERMINALS.with(|screens| { let Ok(mut screens) = screens.try_borrow_mut() else { return 0; }; if screens.len() >= 64 { return 0; }
        NEXT_TERMINAL.with(|next| { let id = next.get(); let Some(following) = id.checked_add(1) else { return 0; }; next.set(following); screens.insert(id, screen); id })
    })
}
#[no_mangle]
pub unsafe extern "C" fn jadix_terminal_screen_feed(id: u64, bytes: *const u8, length: usize) -> *mut c_char {
    if bytes.is_null() || length > 64 * 1024 { return std::ptr::null_mut(); }
    let bytes = unsafe { std::slice::from_raw_parts(bytes, length) };
    TERMINALS.with(|screens| { let Ok(mut screens) = screens.try_borrow_mut() else { return std::ptr::null_mut(); }; let Some(screen) = screens.get_mut(&id) else { return std::ptr::null_mut(); };
        if screen.feed(bytes).is_err() { return std::ptr::null_mut(); }
        serde_json::to_string(&screen.frame()).ok().and_then(|s| CString::new(s).ok()).map_or(std::ptr::null_mut(), CString::into_raw)
    })
}
#[no_mangle]
pub extern "C" fn jadix_terminal_screen_resize(id: u64, columns: u16, rows: u16) -> bool {
    TERMINALS.with(|screens| screens.try_borrow_mut().ok().is_some_and(|mut screens| screens.get_mut(&id).is_some_and(|screen| screen.resize(columns, rows).is_ok())))
}
#[no_mangle]
pub extern "C" fn jadix_terminal_screen_configure(id: u64, columns: u16, rows: u16, scrollback: usize) -> bool {
    TERMINALS.with(|screens| screens.try_borrow_mut().ok().is_some_and(|mut screens| screens.get_mut(&id).is_some_and(|screen| screen.configure(jadix_core::terminal::TerminalSpec { columns, rows, scrollback }).is_ok())))
}
#[no_mangle]
pub extern "C" fn jadix_terminal_screen_frame(id: u64) -> *mut c_char {
    TERMINALS.with(|screens| { let Ok(mut screens) = screens.try_borrow_mut() else { return std::ptr::null_mut(); }; let Some(screen) = screens.get_mut(&id) else { return std::ptr::null_mut(); };
        serde_json::to_string(&screen.frame()).ok().and_then(|s| CString::new(s).ok()).map_or(std::ptr::null_mut(), CString::into_raw)
    })
}
#[no_mangle]
pub extern "C" fn jadix_terminal_screen_free(id: u64) { TERMINALS.with(|screens| { if let Ok(mut screens) = screens.try_borrow_mut() { screens.remove(&id); } }); }

#[no_mangle]
pub extern "C" fn jadix_native_poll_app_quit() -> bool {
    QUIT_CLEANUP.with(|cleanup| {
        cleanup
            .try_borrow_mut()
            .is_ok_and(|mut cleanup| cleanup.poll())
    })
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
        AppleHost::new(
            context,
            AppleCallbacks {
                measure,
                publish,
                clear,
            },
        )
    };
    create_app(host, width, height)
}

#[no_mangle]
pub unsafe extern "C" fn jadix_native_create_with_file_requests(
    context: *mut c_void,
    measure: unsafe extern "C" fn(*mut c_void, *const c_char, *mut f64) -> bool,
    publish: unsafe extern "C" fn(*mut c_void, *const c_char) -> bool,
    clear: unsafe extern "C" fn(*mut c_void),
    file_request: AppleFileRequestCallback,
    width: f32,
    height: f32,
) -> u64 {
    let host = unsafe {
        AppleHost::new(
            context,
            AppleCallbacks {
                measure,
                publish,
                clear,
            },
        )
        .with_file_requests(file_request)
    };
    create_app(host, width, height)
}

#[no_mangle]
pub unsafe extern "C" fn jadix_native_create_with_requests(
    context: *mut c_void,
    measure: unsafe extern "C" fn(*mut c_void, *const c_char, *mut f64) -> bool,
    publish: unsafe extern "C" fn(*mut c_void, *const c_char) -> bool,
    clear: unsafe extern "C" fn(*mut c_void),
    file_request: AppleFileRequestCallback,
    window_request: AppleRequestCallback,
    width: f32,
    height: f32,
) -> u64 {
    let host = unsafe {
        AppleHost::new(
            context,
            AppleCallbacks {
                measure,
                publish,
                clear,
            },
        )
        .with_file_requests(file_request)
        .with_window_requests(window_request)
    };
    create_app(host, width, height)
}

#[no_mangle]
pub unsafe extern "C" fn jadix_native_create_with_baseline(
    context: *mut c_void,
    measure: unsafe extern "C" fn(*mut c_void, *const c_char, *mut f64) -> bool,
    publish: unsafe extern "C" fn(*mut c_void, *const c_char) -> bool,
    clear: unsafe extern "C" fn(*mut c_void),
    file_request: AppleFileRequestCallback,
    window_request: AppleRequestCallback,
    width: f32,
    height: f32,
) -> u64 {
    unsafe {
        jadix_native_create_with_requests(
            context,
            measure,
            publish,
            clear,
            file_request,
            window_request,
            width,
            height,
        )
    }
}

#[no_mangle]
pub unsafe extern "C" fn jadix_native_create_with_alerts(
    context: *mut c_void,
    measure: unsafe extern "C" fn(*mut c_void, *const c_char, *mut f64) -> bool,
    publish: unsafe extern "C" fn(*mut c_void, *const c_char) -> bool,
    clear: unsafe extern "C" fn(*mut c_void),
    file_request: AppleFileRequestCallback,
    window_request: AppleRequestCallback,
    alert_request: AppleRequestCallback,
    width: f32,
    height: f32,
) -> u64 {
    unsafe {
        jadix_native_create_with_services(
            context,
            measure,
            publish,
            clear,
            file_request,
            window_request,
            alert_request,
            None,
            width,
            height,
        )
    }
}

#[no_mangle]
pub unsafe extern "C" fn jadix_native_create_with_services(
    context: *mut c_void,
    measure: unsafe extern "C" fn(*mut c_void, *const c_char, *mut f64) -> bool,
    publish: unsafe extern "C" fn(*mut c_void, *const c_char) -> bool,
    clear: unsafe extern "C" fn(*mut c_void),
    file_request: AppleFileRequestCallback,
    window_request: AppleRequestCallback,
    alert_request: AppleRequestCallback,
    share_request: Option<AppleRequestCallback>,
    width: f32,
    height: f32,
) -> u64 {
    unsafe {
        jadix_native_create_example_with_content(
            context,
            measure,
            publish,
            clear,
            file_request,
            window_request,
            alert_request,
            share_request,
            width,
            height,
            std::ptr::null(),
        )
    }
}

#[no_mangle]
pub unsafe extern "C" fn jadix_native_create_example_with_content(
    context: *mut c_void,
    measure: unsafe extern "C" fn(*mut c_void, *const c_char, *mut f64) -> bool,
    publish: unsafe extern "C" fn(*mut c_void, *const c_char) -> bool,
    clear: unsafe extern "C" fn(*mut c_void),
    file_request: AppleFileRequestCallback,
    window_request: AppleRequestCallback,
    alert_request: AppleRequestCallback,
    share_request: Option<AppleRequestCallback>,
    width: f32,
    height: f32,
    content: *const c_char,
) -> u64 {
    let content = if content.is_null() {
        None
    } else {
        let Ok(content) = serde_json::from_slice::<AppleWindowContent>(
            unsafe { CStr::from_ptr(content) }.to_bytes(),
        ) else {
            return 0;
        };
        Some(content)
    };
    let host = unsafe {
        AppleHost::new(
            context,
            AppleCallbacks {
                measure,
                publish,
                clear,
            },
        )
        .with_file_requests(file_request)
        .with_window_requests(window_request)
        .with_alert_requests(alert_request)
    };
    let host = match share_request {
        Some(callback) => unsafe { host.with_share_requests(callback) },
        None => host,
    };
    let Some(content) = content else {
        return create_app(host, width, height);
    };
    register_app(NativeApp::new_with_content(host, [width, height], content))
}

fn create_app(host: AppleHost, width: f32, height: f32) -> u64 {
    register_app(NativeApp::new(host, [width, height], 0, String::new()))
}

fn register_app(app: Result<NativeApp<AppleHost>, String>) -> u64 {
    let Ok(app) = app else {
        return 0;
    };
    let id = app.root.model().id().token().parse().unwrap();
    APPS.with(|apps| {
        apps.borrow_mut().insert(id, app);
    });
    id
}

#[no_mangle]
pub unsafe extern "C" fn jadix_native_file_result(
    id: u64,
    request: u64,
    json: *const c_char,
) -> bool {
    unsafe { complete_request(id, request, json, AppleHost::complete_file_request) }
}

#[no_mangle]
pub unsafe extern "C" fn jadix_native_window_result(
    id: u64,
    request: u64,
    json: *const c_char,
) -> bool {
    unsafe { complete_request(id, request, json, AppleHost::complete_window_request) }
}

#[no_mangle]
pub unsafe extern "C" fn jadix_native_alert_result(
    id: u64,
    request: u64,
    json: *const c_char,
) -> bool {
    unsafe { complete_request(id, request, json, AppleHost::complete_alert) }
}

#[no_mangle]
pub unsafe extern "C" fn jadix_native_share_result(
    id: u64,
    request: u64,
    json: *const c_char,
) -> bool {
    unsafe { complete_request(id, request, json, AppleHost::complete_share) }
}

#[no_mangle]
pub unsafe extern "C" fn jadix_native_enable_context_menus(
    id: u64,
    callback: Option<AppleRequestCallback>,
) -> bool {
    let Some(callback) = callback else {
        return false;
    };
    APPS.with(|apps| {
        let Ok(mut apps) = apps.try_borrow_mut() else {
            return false;
        };
        let Some(app) = apps.get_mut(&id) else {
            return false;
        };
        unsafe { app.root.host_mut().set_context_menu_request(callback) };
        true
    })
}

#[no_mangle]
pub unsafe extern "C" fn jadix_native_context_menu_result(
    id: u64,
    request: u64,
    json: *const c_char,
) -> bool {
    unsafe { complete_request(id, request, json, AppleHost::complete_context_menu) }
}

#[no_mangle]
pub unsafe extern "C" fn jadix_native_menu_command(
    id: u64,
    revision: u64,
    command: *const c_char,
) -> bool {
    if command.is_null() {
        return false;
    }
    let Ok(command) = unsafe { CStr::from_ptr(command) }.to_str() else {
        return false;
    };
    let callback = APPS.with(|apps| {
        let apps = apps.try_borrow().ok()?;
        apps.get(&id)?.root.host().menu_command(revision, command)
    });
    let Some(callback) = callback else {
        return false;
    };
    callback();
    true
}

#[no_mangle]
pub unsafe extern "C" fn jadix_native_set_example_window_menu(
    id: u64,
    locale: *const c_char,
) -> bool {
    if locale.is_null() {
        return false;
    }
    let Ok(locale) = unsafe { CStr::from_ptr(locale) }.to_str() else {
        return false;
    };
    let Some(declarations) = APPS.with(|apps| {
        let apps = apps.try_borrow().ok()?;
        Some(apps.get(&id)?.root.host().window_declarations())
    }) else {
        return false;
    };
    let scope: RefCell<Option<(OwnerHandle, Receiver<String>)>> = RefCell::new(None);
    let toggle_window = MenuItem::command("declared-window", "Toggle Declared Window", move || {
        if let Some((_, errors)) = scope.take() {
            for error in errors.try_iter() {
                eprintln!("Apple declared window: {error}");
            }
            return;
        }
        let (owner, result) = create_root(|| {
            declarations.mount(format!("rust-declared-{id}"), || {
                let mut options = AppleDeclaredWindowOptions::new(560, 420);
                options.attributes.title = Some("Declared Window".into());
                #[cfg(not(any(feature = "notes", feature = "field-workflow")))]
                {
                    options.content = Some(AppleWindowContent {
                        factory: "settings".into(),
                        argument: Some("Window Settings".into()),
                    });
                }
                Some(options)
            })
        });
        match result {
            Ok(errors) => *scope.borrow_mut() = Some((owner, errors)),
            Err(error) => eprintln!("Apple declared window: {error}"),
        }
    });
    let role = |id, role, shortcut| MenuItem::localized_role(id, role, || {}).accelerator(shortcut);
    let items = vec![
        MenuItem::submenu(
            "Jadix Native Controls",
            vec![role("quit", MenuRole::Quit, "CmdOrCtrl+Q")],
        ),
        MenuItem::submenu(
            "Edit",
            vec![
                role("undo", MenuRole::Undo, "CmdOrCtrl+Z"),
                role("redo", MenuRole::Redo, "CmdOrCtrl+Shift+Z"),
                MenuItem::Separator,
                role("cut", MenuRole::Cut, "CmdOrCtrl+X"),
                role("copy", MenuRole::Copy, "CmdOrCtrl+C"),
                role("paste", MenuRole::Paste, "CmdOrCtrl+V"),
                role("select-all", MenuRole::SelectAll, "CmdOrCtrl+A"),
            ],
        ),
        MenuItem::submenu(
            "Actions",
            vec![
                MenuItem::command("inspect", "Inspect", || {
                    eprintln!("Selected Inspect from the application menu");
                })
                .accelerator("CmdOrCtrl+I"),
                toggle_window,
            ],
        ),
        MenuItem::submenu("Window", Vec::new()),
    ];
    APPS.with(|apps| {
        let Ok(mut apps) = apps.try_borrow_mut() else {
            return false;
        };
        let Some(app) = apps.get_mut(&id) else {
            return false;
        };
        let root = app.root.model().id();
        app.root
            .host_mut()
            .set_window_menu(root, items, locale, Some(3))
            .is_ok()
    })
}

#[no_mangle]
pub unsafe extern "C" fn jadix_native_show_example_context_menu(
    id: u64,
    locale: *const c_char,
) -> bool {
    if locale.is_null() {
        return false;
    }
    let Ok(locale) = unsafe { CStr::from_ptr(locale) }.to_str() else {
        return false;
    };
    APPS.with(|apps| {
        let Ok(mut apps) = apps.try_borrow_mut() else {
            return false;
        };
        let Some(app) = apps.get_mut(&id) else {
            return false;
        };
        let Some(&anchor) = app.root.model().roots().first() else {
            return false;
        };
        let items = vec![
            MenuItem::command("enabled", "Enabled", || eprintln!("Selected Enabled")).checked(true),
            MenuItem::Separator,
            MenuItem::submenu(
                "More",
                vec![
                    MenuItem::command("inspect", "Inspect", || eprintln!("Selected Inspect"))
                        .accelerator("CmdOrCtrl+I"),
                    MenuItem::command("unavailable", "Unavailable", || {}).enabled(false),
                ],
            ),
        ];
        app.root
            .host_mut()
            .show_context_menu(anchor, items, locale)
            .is_ok()
    })
}

#[no_mangle]
pub unsafe extern "C" fn jadix_native_window_state(id: u64, json: *const c_char) -> bool {
    batch(|| unsafe { notify_window(id, json, AppleHost::update_window_state) })
}

#[no_mangle]
pub unsafe extern "C" fn jadix_native_window_event(id: u64, json: *const c_char) -> bool {
    batch(|| unsafe { notify_window(id, json, AppleHost::dispatch_window_event) })
}

unsafe fn notify_window(
    id: u64,
    json: *const c_char,
    notify: fn(&mut AppleHost, &CStr) -> Result<(), String>,
) -> bool {
    if json.is_null() {
        return false;
    }
    let state = unsafe { CStr::from_ptr(json) };
    APPS.with(|apps| {
        let Ok(mut apps) = apps.try_borrow_mut() else {
            return false;
        };
        apps.get_mut(&id)
            .is_some_and(|app| notify(app.root.host_mut(), state).is_ok())
    })
}

unsafe fn complete_request(
    id: u64,
    request: u64,
    json: *const c_char,
    complete: fn(&mut AppleHost, u64, &CStr) -> Result<(), String>,
) -> bool {
    if json.is_null() {
        return false;
    }
    let result = unsafe { CStr::from_ptr(json) };
    APPS.with(|apps| {
        let Ok(mut apps) = apps.try_borrow_mut() else {
            return false;
        };
        apps.get_mut(&id)
            .is_some_and(|app| complete(app.root.host_mut(), request, result).is_ok())
    })
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
    action: impl FnOnce(&mut NativeApp<AppleHost>) -> Result<String, String>,
) -> *mut c_char {
    reply(APPS.with(|apps| {
        let mut apps = apps
            .try_borrow_mut()
            .map_err(|_| "reentrant Apple event".to_owned())?;
        action(apps.get_mut(&id).ok_or("disposed Apple root")?)
    }))
}

#[no_mangle]
pub unsafe extern "C" fn jadix_native_deep_link(id: u64, json: *const c_char, configure: bool) -> *mut c_char {
    if json.is_null() { return reply(Err("null deep link".into())); }
    let bytes = unsafe { CStr::from_ptr(json) }.to_bytes();
    if bytes.len() > 128 * 1024 { return reply(Err("deep link exceeds budget".into())); }
    with_app(id, |app| {
        if configure {
            let config = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
            $crate::host::PlatformHost::configure_deep_links(app.root.host_mut(), config)?;
        } else {
            let link = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
            app.root.host_mut().receive_deep_link(link)?;
        }
        Ok(String::new())
    })
}

#[no_mangle]
pub unsafe extern "C" fn jadix_native_set_app_address(
    id: u64,
    address: *const c_char,
) -> *mut c_char {
    let address = if address.is_null() {
        None
    } else {
        match unsafe { CStr::from_ptr(address) }.to_str() {
            Ok(address) => Some(address),
            Err(error) => return reply(Err(error.to_string())),
        }
    };
    with_app(id, |app| {
        app.root
            .set_app_address(address)
            .map_err(|error| error.to_string())?;
        Ok(String::new())
    })
}

#[no_mangle]
pub unsafe extern "C" fn jadix_native_event(id: u64, json: *const c_char) -> *mut c_char {
    if json.is_null() {
        return reply(Err("null Apple event".into()));
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
pub unsafe extern "C" fn jadix_native_environment(id: u64, json: *const c_char) -> *mut c_char {
    if json.is_null() { return reply(Err("null environment".into())); }
    let bytes = unsafe { CStr::from_ptr(json) }.to_bytes();
    let snapshot = match jadix_core::environment::EnvironmentSnapshot::from_json(bytes) {
        Ok(value) => value,
        Err(error) => return reply(Err(error.to_string())),
    };
    with_app(id, |app| {
        app.root.set_environment_snapshot(snapshot).map_err(|e| e.to_string())?;
        Ok(String::new())
    })
}

#[no_mangle]
pub extern "C" fn jadix_native_state(id: u64) -> *mut c_char {
    with_app(id, |app| Ok(app.state()))
}

#[no_mangle]
pub unsafe extern "C" fn jadix_native_restore(id: u64, json: *const c_char) -> *mut c_char {
    if json.is_null() {
        return reply(Err("null Apple state".into()));
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
    #[cfg(all(
        any(feature = "notes", feature = "field-workflow"),
        not(feature = "gpu")
    ))]
    {
        with_app(id, |app| {
            app.storage_result(error)?;
            Ok(app.state())
        })
    }
    #[cfg(not(all(
        any(feature = "notes", feature = "field-workflow"),
        not(feature = "gpu")
    )))]
    {
        let _ = (id, error);
        reply(Err("storage callbacks require the notes application".into()))
    }
}

#[no_mangle]
pub extern "C" fn jadix_native_dispose(id: u64) {
    batch(|| {
        let app = APPS.with(|apps| apps.borrow_mut().remove(&id));
        drop(app);
    });
}

#[no_mangle]
pub unsafe extern "C" fn jadix_native_string_free(value: *mut c_char) {
    if !value.is_null() {
        drop(unsafe { CString::from_raw(value) });
    }
}

#[no_mangle]
pub extern "C" fn jadix_native_web_poll(id: u64) {
    let (wake, work) = APPS.with(|apps| apps.try_borrow_mut().ok().and_then(|mut apps| apps.get_mut(&id).map(|app| { let root = app.root.model().id(); app.root.host_mut().pump_audio(root); (app.root.host_mut().pump_web(), app.root.host_mut().take_resource_work()) }))).unwrap_or_default();
    for batch in wake { batch.wake(); }
    for work in work { work.run(); }
}

#[no_mangle]
pub unsafe extern "C" fn jadix_native_web_resource_allowed(id: u64, url: *const c_char) -> bool {
    if url.is_null() { return false; }
    let Ok(url) = (unsafe { CStr::from_ptr(url) }).to_str() else { return false; };
    if url.len() > 4096 { return false; }
    let request = jadix_core::web_resource::ResourceRequest { url: url.into(), method: "GET".into() };
    APPS.with(|apps| apps.try_borrow().ok().is_some_and(|apps| apps.get(&id).is_some_and(|app| app.root.host().allows_web_resource(&request))))
}

#[no_mangle]
pub unsafe extern "C" fn jadix_native_web_resource_request(id: u64, node: *const c_char, url: *const c_char, method: *const c_char) -> u64 {
    if node.is_null() || url.is_null() || method.is_null() { return 0; }
    let (Ok(node), Ok(url), Ok(method)) = (unsafe { CStr::from_ptr(node) }.to_str(), unsafe { CStr::from_ptr(url) }.to_str(), unsafe { CStr::from_ptr(method) }.to_str()) else { return 0; };
    if node.len() > 256 || url.len() > 4096 || method.len() > 8 { return 0; }
    APPS.with(|apps| apps.try_borrow_mut().ok().and_then(|mut apps| {
        let app = apps.get_mut(&id)?;
        let node = $crate::presentation::NodeId::from_token(app.root.model().id(), node)?;
        app.root.host_mut().request_web_resource(node, jadix_core::web_resource::ResourceRequest { url: url.into(), method: method.into() })
    })).unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn jadix_native_web_resource_cancel(id: u64, request: u64) {
    APPS.with(|apps| { if let Ok(mut apps) = apps.try_borrow_mut() { if let Some(app) = apps.get_mut(&id) { app.root.host_mut().cancel_web_resource(request); } } });
}

#[no_mangle]
pub unsafe extern "C" fn jadix_native_audio_event(id: u64, object: u32, json: *const c_char) -> bool {
    if json.is_null() { return false; }
    let bytes = unsafe { CStr::from_ptr(json) }.to_bytes();
    if bytes.len() > 16 * 1024 { return false; }
    let Ok(event) = serde_json::from_slice(bytes) else { return false; };
    APPS.with(|apps| apps.try_borrow_mut().ok().is_some_and(|mut apps| apps.get_mut(&id).is_some_and(|app| app.root.host_mut().audio_event(object, event))))
}

#[no_mangle]
pub unsafe extern "C" fn jadix_native_fetch_event(id: u64, request: u32, json: *const c_char) -> bool {
    if json.is_null() { return false; }
    let bytes = unsafe { CStr::from_ptr(json) }.to_bytes();
    if bytes.len() > 400 * 1024 { return false; }
    let Ok(event) = serde_json::from_slice::<$crate::fetch::FetchEvent>(bytes) else { return false; };
    APPS.with(|apps| {
        let Ok(apps) = apps.try_borrow() else { return false; };
        apps.get(&id).is_some_and(|app| app.root.host().fetch().event(request, event))
    })
}

#[no_mangle]
pub unsafe extern "C" fn jadix_native_speech_event(id: u64, session: u32, json: *const c_char) -> bool {
    if json.is_null() { return false; }
    let bytes = unsafe { CStr::from_ptr(json) }.to_bytes(); if bytes.len() > 70 * 1024 { return false; }
    let Ok(event) = serde_json::from_slice(bytes) else { return false; };
    APPS.with(|apps| apps.try_borrow_mut().ok().is_some_and(|mut apps| apps.get_mut(&id).is_some_and(|app| app.root.host_mut().speech_event(session, event))))
}

#[no_mangle]
pub unsafe extern "C" fn jadix_native_desktop_service_event(id: u64, object: u32, json: *const c_char) -> bool {
    if json.is_null() { return false; }
    let bytes = unsafe { CStr::from_ptr(json) }.to_bytes();
    if bytes.len() > 128 * 1024 { return false; }
    #[derive(serde::Deserialize)]
    #[serde(tag = "kind", content = "value", rename_all = "snake_case", deny_unknown_fields)]
    enum Message {
        Reply(jadix_core::desktop_services::ServiceReply),
        Event(jadix_core::desktop_services::ServiceEvent),
        Activation(jadix_core::desktop_services::NotificationActivation),
    }
    let Ok(message) = serde_json::from_slice::<Message>(bytes) else { return false; };
    APPS.with(|apps| apps.try_borrow().ok().is_some_and(|apps| apps.get(&id).is_some_and(|app| {
        let service = app.root.host().desktop_services();
        match message { Message::Reply(reply) => service.reply(object, reply), Message::Event(event) => service.event(object, event), Message::Activation(activation) => object == 0 && service.activate(activation) }
    })))
}

/// Display-link callbacks execute on the main run loop, outside publication.
#[no_mangle]
pub extern "C" fn jadix_native_display_tick(id: u64, refresh_rate: f64, work_seconds: f64, changed: bool) {
    if !refresh_rate.is_finite() || !work_seconds.is_finite() || !(0.0..=60.0).contains(&work_seconds) { return; }
    APPS.with(|apps| {
        if let Ok(mut apps) = apps.try_borrow_mut() {
            if let Some(app) = apps.get_mut(&id) {
                app.root.display_tick(Some(refresh_rate), changed.then(|| std::time::Duration::from_secs_f64(work_seconds)));
            }
        }
    });
}

#[no_mangle]
pub extern "C" fn jadix_native_performance(id: u64) -> *mut c_char {
    APPS.with(|apps| {
        let Ok(mut apps) = apps.try_borrow_mut() else { return std::ptr::null_mut(); };
        let Some(app) = apps.get_mut(&id) else { return std::ptr::null_mut(); };
        let sample = app.root.performance();
        if $crate::__appkit_log::log_enabled!(target: "jadix::fps", $crate::__appkit_log::Level::Debug) {
            $crate::__appkit_log::debug!(target: "jadix::fps", "{}", serde_json::to_string(&sample).unwrap_or_default());
        }
        serde_json::to_string(&sample).ok().and_then(|json| CString::new(json).ok()).map_or(std::ptr::null_mut(), CString::into_raw)
    })
}

#[no_mangle]
pub unsafe extern "C" fn jadix_native_web_ready(id: u64, node: *const c_char, ready: bool) -> bool {
    if node.is_null() { return false; }
    let Ok(token) = (unsafe { CStr::from_ptr(node) }).to_str() else { return false; };
    APPS.with(|apps| {
        let Ok(mut apps) = apps.try_borrow_mut() else { return false; };
        let Some(app) = apps.get_mut(&id) else { return false; };
        let Some(node) = $crate::presentation::NodeId::from_token(app.root.model().id(), token) else { return false; };
        app.root.host_mut().web_document_ready(node, ready); true
    })
}

#[no_mangle]
pub unsafe extern "C" fn jadix_native_web_complete(id: u64, request: u64, success: bool, json: *const c_char) -> bool {
    if json.is_null() { return false; }
    let bytes = unsafe { CStr::from_ptr(json) }.to_bytes();
    if bytes.len() > jadix_core::web_evaluation::MAX_RESULT_BYTES + 4096 { return false; }
    let Ok(text) = std::str::from_utf8(bytes) else { return false; };
    APPS.with(|apps| {
        let Ok(mut apps) = apps.try_borrow_mut() else { return false; };
        apps.get_mut(&id).is_some_and(|app| app.root.host_mut().complete_web_evaluation(request, if success { Ok(text.into()) } else { Err(text.into()) }))
    })
}

#[no_mangle]
pub unsafe extern "C" fn jadix_native_tray_command(id: u64, revision: u64, command: *const c_char) -> bool {
    if command.is_null() { return false; }
    let Ok(command) = (unsafe { CStr::from_ptr(command) }).to_str() else { return false; };
    if command.len() > 4096 { return false; }
    APPS.with(|apps| apps.try_borrow_mut().ok().is_some_and(|mut apps| apps.get_mut(&id).is_some_and(|app| app.root.host_mut().tray_command(revision, command))))
}

    };
}

/// AppKit compatibility entry point.
#[macro_export]
macro_rules! appkit_application { ($app:ident) => { $crate::apple_application!($app); }; }
/// UIKit application bridge, sharing the Apple ABI and lifetime rules.
#[macro_export]
macro_rules! uikit_application { ($app:ident) => { $crate::apple_application!($app); }; }
