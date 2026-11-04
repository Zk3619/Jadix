#![cfg(all(feature = "ohos", not(target_env = "ohos")))]

pub use jadix_native::{host, layout, presentation};
#[path = "../src/ohos.rs"]
mod adapter;
#[path = "../src/popover.rs"]
#[allow(
    dead_code,
    reason = "the contract harness shares the production overlay dependency resolver"
)]
mod popover;

use adapter::OhosHost;
use host::{NativeRoot, PlatformHost};
use presentation::*;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    ffi::{CStr, c_char, c_void},
    rc::Rc,
};

#[derive(Default)]
struct Node {
    kind: i32,
    target: usize,
    label: String,
    value: String,
    options: Vec<SelectOption>,
    selected: i32,
    interactive: bool,
    select_closes: usize,
    children: Vec<usize>,
    frame: [f32; 4],
    backdrop: bool,
}

#[derive(Default)]
struct Api {
    nodes: BTreeMap<usize, Node>,
    dialogs: Vec<usize>,
    available: bool,
    focus: usize,
}

thread_local! { static API: RefCell<Api> = RefCell::new(Api { available: true, ..Default::default() }); }

unsafe extern "C" {
    fn jadix_ohos_receive(
        target: *mut c_void,
        kind: i32,
        text: *const c_char,
        preview: *const c_char,
        start: i32,
        selection_start: i32,
        selection_end: i32,
        x: f32,
        y: f32,
    );
}

#[no_mangle]
unsafe extern "C" fn jadix_ohos_content(
    _: *mut c_void,
    value: *mut c_void,
    result: *mut *mut c_void,
) -> i32 {
    unsafe {
        *result = value;
    }
    0
}

#[no_mangle]
extern "C" fn jadix_ohos_create(kind: i32, target: *mut c_void) -> *mut c_void {
    let handle = Box::into_raw(Box::new(0_u8)).cast::<c_void>();
    API.with_borrow_mut(|api| {
        api.nodes.insert(
            handle as usize,
            Node {
                kind,
                target: target as usize,
                ..Default::default()
            },
        );
    });
    handle
}

#[no_mangle]
unsafe extern "C" fn jadix_ohos_dispose(handle: *mut c_void) {
    API.with_borrow_mut(|api| {
        api.nodes.remove(&(handle as usize));
        api.dialogs.retain(|dialog| *dialog != handle as usize);
    });
    unsafe {
        drop(Box::from_raw(handle.cast::<u8>()));
    }
}

#[no_mangle]
extern "C" fn jadix_ohos_dialog_supported() -> bool {
    API.with_borrow(|api| api.available)
}

#[no_mangle]
extern "C" fn jadix_ohos_dialog(handle: *mut c_void, open: bool, backdrop: bool) -> i32 {
    let target = API.with_borrow_mut(|api| {
        let node = api
            .nodes
            .get_mut(&(handle as usize))
            .filter(|node| matches!(node.kind, 9 | 10))?;
        node.backdrop = backdrop;
        let target = node.target;
        if open {
            if !api.dialogs.contains(&(handle as usize)) {
                api.dialogs.push(handle as usize);
            }
        } else {
            api.dialogs.retain(|dialog| *dialog != handle as usize);
        }
        Some(target)
    });
    let Some(target) = target else {
        return -1;
    };
    if !open {
        unsafe {
            jadix_ohos_receive(
                target as *mut c_void,
                5,
                std::ptr::null(),
                std::ptr::null(),
                0,
                0,
                0,
                0.0,
                0.0,
            );
        }
    }
    0
}

#[no_mangle]
extern "C" fn jadix_ohos_mount(_: *mut c_void, _: *mut c_void, _: bool) -> i32 {
    0
}

#[no_mangle]
extern "C" fn jadix_ohos_child(parent: *mut c_void, child: *mut c_void, index: i32) -> i32 {
    API.with_borrow_mut(|api| {
        if api
            .nodes
            .get(&(child as usize))
            .is_some_and(|node| matches!(node.kind, 9 | 10))
        {
            return 0;
        }
        let Some(parent) = api.nodes.get_mut(&(parent as usize)) else {
            return -1;
        };
        if index < 0 {
            parent.children.retain(|node| *node != child as usize);
        } else if index as usize > parent.children.len() {
            return -2;
        } else {
            parent.children.insert(index as usize, child as usize);
        }
        0
    })
}

#[no_mangle]
extern "C" fn jadix_ohos_geometry(
    handle: *mut c_void,
    left: f32,
    top: f32,
    width: f32,
    height: f32,
) -> i32 {
    API.with_borrow_mut(|api| {
        api.nodes.get_mut(&(handle as usize)).unwrap().frame = [left, top, width, height];
    });
    0
}

#[no_mangle]
#[allow(
    clippy::too_many_arguments,
    reason = "the stub must match the native ABI"
)]
unsafe extern "C" fn jadix_ohos_update(
    handle: *mut c_void,
    _: i32,
    value: *const c_char,
    _: *const c_char,
    _: bool,
    _: bool,
    _: bool,
    _: f32,
    _: u32,
    _: *const c_char,
    _: u32,
    _: u32,
    accessible: *const c_char,
    _: i32,
    _: i32,
    _: i32,
) -> i32 {
    let label = unsafe { CStr::from_ptr(accessible) }.to_str().unwrap();
    let label = if label.is_empty() {
        unsafe { CStr::from_ptr(value) }.to_str().unwrap()
    } else {
        label
    };
    API.with_borrow_mut(|api| {
        let node = api.nodes.get_mut(&(handle as usize)).unwrap();
        node.label = label.into();
        node.value = unsafe { CStr::from_ptr(value) }.to_str().unwrap().into();
    });
    0
}

#[no_mangle]
#[allow(
    clippy::too_many_arguments,
    reason = "the stub must match the native ABI"
)]
unsafe extern "C" fn jadix_ohos_select(
    handle: *mut c_void,
    values: *const *const c_char,
    labels: *const *const c_char,
    enabled: *const bool,
    count: usize,
    selected: i32,
    interactive: bool,
) -> i32 {
    let target = API.with_borrow_mut(|api| {
        let node = api.nodes.get_mut(&(handle as usize)).unwrap();
        assert_eq!(node.kind, 11);
        node.options = (0..count)
            .map(|index| SelectOption {
                value: unsafe { CStr::from_ptr(*values.add(index)) }
                    .to_str()
                    .unwrap()
                    .into(),
                label: unsafe { CStr::from_ptr(*labels.add(index)) }
                    .to_str()
                    .unwrap()
                    .into(),
                enabled: unsafe { *enabled.add(index) },
            })
            .collect();
        node.selected = selected;
        node.interactive = interactive;
        node.target
    });
    unsafe {
        jadix_ohos_receive(
            target as *mut c_void,
            6,
            c"update must not echo".as_ptr(),
            std::ptr::null(),
            0,
            0,
            0,
            0.0,
            0.0,
        );
    }
    0
}

#[no_mangle]
extern "C" fn jadix_ohos_select_close(handle: *mut c_void) -> i32 {
    API.with_borrow_mut(|api| {
        let node = api.nodes.get_mut(&(handle as usize)).unwrap();
        assert_eq!(node.kind, 11);
        node.select_closes += 1;
    });
    0
}

#[no_mangle]
extern "C" fn jadix_ohos_focus(handle: *mut c_void, focused: bool) -> i32 {
    API.with_borrow_mut(|api| {
        api.focus = if focused { handle as usize } else { 0 };
    });
    0
}

#[no_mangle]
extern "C" fn jadix_ohos_scroll(_: *mut c_void, _: f32, _: f32) -> i32 {
    0
}

#[no_mangle]
unsafe extern "C" fn jadix_ohos_measure(
    _: *const c_char,
    _: *const c_char,
    _: f64,
    _: u32,
    _: f64,
    result: *mut f64,
) -> i32 {
    unsafe {
        std::ptr::copy_nonoverlapping([80.0, 24.0, 18.0].as_ptr(), result, 3);
    }
    0
}

fn mounted() -> NativeRoot<OhosHost> {
    let mut token = 0_u8;
    let pointer = std::ptr::from_mut(&mut token).cast();
    let mut host = unsafe { OhosHost::from_napi(pointer, pointer, [360.0, 640.0], 1.0) }.unwrap();
    host.resize([360.0, 640.0], 1.0).unwrap();
    NativeRoot::new(host)
}

fn node(
    root: RootId,
    name: &str,
    content: NodeContent,
    children: Vec<PresentationNode>,
) -> PresentationNode {
    PresentationNode {
        id: NodeId::new(root),
        descriptor: NodeDescriptor {
            content,
            frame: Rect::default(),
            accessible_name: Some(name.into()),
            accessible_role: None,
            accessible_selected: None,
            accessible_expanded: None,
            style: Default::default(),
        },
        children,
    }
}

fn dialog(open: bool, revision: u64) -> NodeContent {
    NodeContent::Dialog {
        open,
        close_on_overlay_click: true,
        revision,
    }
}

fn button(root: RootId, name: &str) -> PresentationNode {
    node(
        root,
        name,
        NodeContent::Button {
            label: name.into(),
            enabled: true,
        },
        vec![],
    )
}

fn handle(name: &str) -> usize {
    API.with_borrow(|api| {
        *api.nodes
            .iter()
            .find(|(_, node)| node.label == name)
            .unwrap()
            .0
    })
}

fn visible() -> Vec<String> {
    API.with_borrow(|api| {
        api.dialogs
            .iter()
            .map(|handle| api.nodes[handle].label.clone())
            .collect()
    })
}

#[test]
fn portals_keep_physical_indices_and_logical_modal_order_through_moves_and_removal() {
    let mut root = mounted();
    let id = root.model().id();
    let inner = node(id, "inner", dialog(true, 0), vec![button(id, "inside")]);
    let outer = node(id, "outer", dialog(false, 0), vec![inner]);
    let other = node(id, "other", dialog(false, 0), vec![]);
    let mut roots = vec![node(
        id,
        "parent",
        NodeContent::Container,
        vec![button(id, "before"), outer, button(id, "after"), other],
    )];
    root.present(&mut roots, [360.0, 640.0], 0).unwrap();
    assert!(visible().is_empty());
    let expected = [handle("before"), handle("after")];
    API.with_borrow(|api| assert_eq!(api.nodes[&handle("parent")].children, expected));
    roots[0].children[1].descriptor.content = dialog(true, 1);
    roots[0].children[3].descriptor.content = dialog(true, 1);
    root.present(&mut roots, [360.0, 640.0], 0).unwrap();
    assert_eq!(visible(), ["outer", "inner", "other"]);
    roots[0].children.swap(1, 3);
    root.present(&mut roots, [360.0, 640.0], 0).unwrap();
    assert_eq!(visible(), ["other", "outer", "inner"]);
    roots[0].children[3].descriptor.content = dialog(false, 2);
    root.present(&mut roots, [360.0, 640.0], 0).unwrap();
    assert_eq!(visible(), ["other"]);
    roots[0].children.remove(3);
    root.present(&mut roots, [360.0, 640.0], 0).unwrap();
    assert!(root.host_mut().take_events().unwrap().is_empty());
    root.dispose();
    root.dispose();
    API.with_borrow(|api| assert!(api.nodes.is_empty() && api.dialogs.is_empty()));
}

#[test]
fn dismiss_revisions_wakeup_once_and_programmatic_close_does_not_echo() {
    let mut root = mounted();
    let wakes = Rc::new(Cell::new(0));
    let observed = wakes.clone();
    root.host_mut().set_notifier(move || {
        observed.set(observed.get() + 1);
        Ok(())
    });
    let id = root.model().id();
    let mut roots = vec![node(
        id,
        "dialog",
        dialog(true, 0),
        vec![button(id, "done")],
    )];
    root.present(&mut roots, [360.0, 640.0], 0).unwrap();
    root.focus(Some(roots[0].children[0].id)).unwrap();
    API.with_borrow(|api| assert_eq!(api.focus, handle("done")));
    let target = API.with_borrow(|api| api.nodes[&handle("dialog")].target);
    for backdrop in [0, 1] {
        unsafe {
            jadix_ohos_receive(
                target as *mut c_void,
                5,
                std::ptr::null(),
                std::ptr::null(),
                backdrop,
                0,
                0,
                0.0,
                0.0,
            );
        }
    }
    assert_eq!(wakes.get(), 1);
    let events = root.poll_events().unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(
        events[0].payload,
        EventPayload::Dismiss {
            backdrop: false,
            revision: 1
        }
    );
    assert_eq!(
        events[1].payload,
        EventPayload::Dismiss {
            backdrop: true,
            revision: 2
        }
    );
    assert!(root.event(&events[0]).is_err());
    roots[0].descriptor.content = dialog(false, 2);
    root.present(&mut roots, [360.0, 640.0], 0).unwrap();
    assert!(root.host_mut().take_events().unwrap().is_empty());
    assert_eq!(wakes.get(), 1);
    root.dispose();
    assert!(root.host_mut().take_events().is_err());
    API.with_borrow(|api| assert!(api.nodes.is_empty()));
}

#[test]
fn popover_uses_window_coordinates_and_retains_controlled_nonmodal_events() {
    let mut root = mounted();
    let id = root.model().id();
    let mut anchor = button(id, "anchor");
    anchor.descriptor.style.width = Some(80.0);
    anchor.descriptor.style.height = Some(40.0);
    let mut panel = node(
        id,
        "panel",
        NodeContent::Popover {
            anchor: anchor.id,
            placement: PopoverPlacement::BottomStart,
            gap: 6,
            open: true,
            close_on_outside_click: true,
            revision: 0,
        },
        vec![button(id, "save")],
    );
    panel.descriptor.style.width = Some(120.0);
    panel.descriptor.style.height = Some(80.0);
    let mut parent = node(
        id,
        "parent",
        NodeContent::Container,
        vec![panel, anchor, button(id, "background")],
    );
    parent.descriptor.style.padding = [24.0; 4];
    let mut roots = vec![parent];
    root.present(&mut roots, [360.0, 640.0], 0).unwrap();
    assert_eq!(visible(), ["panel"]);
    let frame = roots[0].children[0].descriptor.frame;
    let anchor_frame = roots[0].children[1].descriptor.frame;
    assert_eq!(frame.x, anchor_frame.x);
    assert_eq!(frame.y, anchor_frame.y + anchor_frame.height + 6.0);
    API.with_borrow(|api| {
        assert_eq!(api.nodes[&handle("panel")].kind, 10);
        assert!(api.nodes[&handle("panel")].backdrop);
        assert_eq!(
            api.nodes[&handle("panel")].frame,
            [
                frame.x as f32,
                frame.y as f32,
                frame.width as f32,
                frame.height as f32
            ]
        );
        assert_eq!(
            api.nodes[&handle("parent")].children,
            [handle("anchor"), handle("background")]
        );
    });
    root.event(&PlatformEvent {
        node: roots[0].children[2].id,
        payload: EventPayload::Activate,
    })
    .unwrap();
    root.focus(Some(roots[0].children[0].children[0].id))
        .unwrap();
    API.with_borrow(|api| assert_eq!(api.focus, handle("save")));
    let target = API.with_borrow(|api| api.nodes[&handle("panel")].target);
    unsafe {
        jadix_ohos_receive(
            target as *mut c_void,
            5,
            std::ptr::null(),
            std::ptr::null(),
            1,
            0,
            0,
            0.0,
            0.0,
        );
    }
    let events = root.poll_events().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0].payload,
        EventPayload::Dismiss {
            backdrop: true,
            revision: 1
        }
    );
    assert!(root.event(&events[0]).is_err());
    if let NodeContent::Popover {
        close_on_outside_click,
        revision,
        ..
    } = &mut roots[0].children[0].descriptor.content
    {
        *close_on_outside_click = false;
        *revision = 1;
    }
    root.present(&mut roots, [360.0, 640.0], 0).unwrap();
    API.with_borrow(|api| assert!(!api.nodes[&handle("panel")].backdrop));
    if let NodeContent::Popover { open, revision, .. } =
        &mut roots[0].children[0].descriptor.content
    {
        *open = false;
        *revision = 2;
    }
    root.present(&mut roots, [360.0, 640.0], 0).unwrap();
    assert!(visible().is_empty());
    assert!(root.host_mut().take_events().unwrap().is_empty());
    roots.clear();
    root.present(&mut roots, [360.0, 640.0], 0).unwrap();
    root.dispose();
    API.with_borrow(|api| assert!(api.nodes.is_empty()));
}

#[test]
fn portal_order_honors_anchor_ownership_before_logical_sibling_priority() {
    let mut root = mounted();
    let id = root.model().id();
    let anchor = button(id, "anchor");
    let owner = node(id, "owner", dialog(true, 0), vec![anchor.clone()]);
    let panel = node(
        id,
        "anchored",
        NodeContent::Popover {
            anchor: anchor.id,
            placement: PopoverPlacement::BottomStart,
            gap: 6,
            open: true,
            close_on_outside_click: true,
            revision: 0,
        },
        vec![],
    );
    let mut roots = vec![panel, owner];
    root.present(&mut roots, [360.0, 640.0], 0).unwrap();
    assert_eq!(visible(), ["owner", "anchored"]);
    root.dispose();
    API.with_borrow(|api| assert!(api.nodes.is_empty()));
}

#[test]
fn inspection_redacts_text_and_preserves_pending_native_events() {
    let mut root = mounted();
    let id = root.model().id();
    let mut roots = vec![
        node(
            id,
            "editor",
            NodeContent::Input {
                value: "private plain 中文🙂".into(),
                placeholder: "private hint".into(),
                enabled: true,
                password: false,
                revision: 0,
            },
            vec![],
        ),
        node(
            id,
            "password",
            NodeContent::Input {
                value: "password must remain hidden".into(),
                placeholder: "password hint".into(),
                enabled: true,
                password: true,
                revision: 0,
            },
            vec![],
        ),
    ];
    root.present(&mut roots, [360.0, 640.0], 0).unwrap();
    let before = root.inspection(false);
    let target = API.with_borrow(|api| api.nodes[&handle("editor")].target);
    unsafe {
        jadix_ohos_receive(
            target as *mut c_void,
            1,
            std::ptr::null(),
            std::ptr::null(),
            0,
            0,
            0,
            0.0,
            0.0,
        );
    }
    let pending = root.host().inspection().unwrap();
    assert_eq!(pending["queue"]["events"], 1);
    assert_eq!(pending["queue"]["scheduled"], true);
    assert_eq!(pending["controls"], 2);
    assert_eq!(pending["viewport"], serde_json::json!([360.0, 640.0]));
    assert_eq!(before, root.inspection(false));
    assert!(!before.to_string().contains("private plain"));
    assert!(!before.to_string().contains("private hint"));
    let explicit = root.inspection(true).to_string();
    assert!(explicit.contains("private plain 中文🙂"));
    assert!(!explicit.contains("password must remain hidden"));
    assert!(!explicit.contains("password hint"));
    assert_eq!(pending, root.host().inspection().unwrap());
    let events = root.poll_events().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].node, roots[0].id);
    assert_eq!(events[0].payload, EventPayload::Focus);
    assert_eq!(root.host().inspection().unwrap()["queue"]["events"], 0);
    root.dispose();
    let disposed = root.host().inspection().unwrap();
    assert_eq!(disposed["closed"], true);
    assert_eq!(disposed["controls"], 0);
    API.with_borrow(|api| assert!(api.nodes.is_empty()));
}

#[test]
fn unavailable_dialog_interface_rejects_before_native_publication() {
    API.with_borrow_mut(|api| api.available = false);
    let mut root = mounted();
    assert!(
        !root
            .host()
            .capabilities()
            .node_kinds
            .contains(&presentation::capabilities::NodeKind::Select)
    );
    let mut roots = vec![node(
        root.model().id(),
        "unsupported",
        dialog(true, 0),
        vec![],
    )];
    assert!(root.present(&mut roots, [360.0, 640.0], 0).is_err());
    API.with_borrow(|api| assert_eq!(api.nodes.len(), 1));
    root.dispose();
    API.with_borrow_mut(|api| {
        assert!(api.nodes.is_empty());
        api.available = true;
    });
}

fn selection(value: &str, enabled: bool, revision: u64) -> NodeContent {
    NodeContent::Select {
        options: vec![
            SelectOption {
                value: "apple".into(),
                label: "Apple 苹果🙂".into(),
                enabled: true,
            },
            SelectOption {
                value: "banana".into(),
                label: "Banana 香蕉".into(),
                enabled: true,
            },
            SelectOption {
                value: "blocked".into(),
                label: "Disabled".into(),
                enabled: false,
            },
        ],
        value: value.into(),
        placeholder: "Choose fruit".into(),
        enabled,
        revision,
    }
}

fn choose(name: &str, value: &str) {
    let target = API.with_borrow(|api| api.nodes[&handle(name)].target);
    let value = std::ffi::CString::new(value).unwrap();
    unsafe {
        jadix_ohos_receive(
            target as *mut c_void,
            6,
            value.as_ptr(),
            std::ptr::null(),
            0,
            0,
            0,
            0.0,
            0.0,
        );
    }
}

#[test]
fn select_preserves_trigger_and_controlled_unicode_options_without_update_echo() {
    let mut root = mounted();
    let mut roots = vec![node(
        root.model().id(),
        "fruit",
        selection("", true, 7),
        vec![],
    )];
    root.present(&mut roots, [360.0, 640.0], 0).unwrap();
    let trigger = handle("fruit");
    API.with_borrow(|api| {
        let control = &api.nodes[&trigger];
        assert_eq!(control.value, "Choose fruit");
        assert_eq!(control.selected, -1);
        assert!(!control.options[2].enabled);
    });
    assert!(root.host_mut().take_events().unwrap().is_empty());
    choose("fruit", "banana");
    let events = root.poll_events().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].node, roots[0].id);
    assert_eq!(
        events[0].payload,
        EventPayload::Select {
            value: "banana".into(),
            revision: 8
        }
    );
    roots[0].descriptor.content = selection("apple", false, 9);
    root.present(&mut roots, [360.0, 640.0], 0).unwrap();
    assert_eq!(handle("fruit"), trigger);
    choose("fruit", "banana");
    assert!(root.host_mut().take_events().unwrap().is_empty());
    API.with_borrow(|api| {
        let control = &api.nodes[&trigger];
        assert_eq!(control.value, "Apple 苹果🙂");
        assert_eq!(control.selected, 0);
        assert!(!control.interactive);
    });
    roots[0].descriptor.content = selection("", true, 10);
    root.present(&mut roots, [360.0, 640.0], 0).unwrap();
    choose("fruit", "apple");
    assert!(matches!(
        root.poll_events().unwrap()[0].payload,
        EventPayload::Select { revision: 11, .. }
    ));
    root.dispose();
    API.with_borrow(|api| assert!(api.nodes.is_empty()));
}

#[test]
fn select_rejects_unknown_disabled_and_foreign_option_events() {
    for (content, invalid) in [
        (selection("", true, 0), "unknown"),
        (selection("", true, 0), "blocked"),
        (
            NodeContent::Button {
                label: "not a Select".into(),
                enabled: true,
            },
            "apple",
        ),
    ] {
        let mut root = mounted();
        let mut roots = vec![node(root.model().id(), "target", content, vec![])];
        root.present(&mut roots, [360.0, 640.0], 0).unwrap();
        choose("target", invalid);
        assert!(root.host_mut().take_events().is_err());
        root.dispose();
        API.with_borrow(|api| assert!(api.nodes.is_empty()));
    }
}

#[test]
fn select_popup_is_closed_before_owner_overlay_stack_changes() {
    let mut root = mounted();
    let id = root.model().id();
    let mut roots = vec![node(
        id,
        "owner",
        dialog(true, 0),
        vec![node(id, "fruit", selection("apple", true, 0), vec![])],
    )];
    root.present(&mut roots, [360.0, 640.0], 0).unwrap();
    let trigger = handle("fruit");
    let closes = API.with_borrow(|api| api.nodes[&trigger].select_closes);
    assert_eq!(closes, 1);
    roots[0].children[0].descriptor.style.font_size = 19.0;
    root.present(&mut roots, [360.0, 640.0], 0).unwrap();
    API.with_borrow(|api| assert_eq!(api.nodes[&trigger].select_closes, closes));
    roots[0].descriptor.content = dialog(false, 1);
    root.present(&mut roots, [360.0, 640.0], 0).unwrap();
    API.with_borrow(|api| assert_eq!(api.nodes[&trigger].select_closes, closes + 1));
    root.dispose();
    API.with_borrow(|api| assert!(api.nodes.is_empty()));
}
