#![cfg(all(windows, feature = "windows"))]

use jadix_native::{host::NativeRoot, presentation::*, windows::WindowsHost};

fn node(root: RootId, content: NodeContent) -> PresentationNode {
    PresentationNode {
        id: NodeId::new(root),
        descriptor: NodeDescriptor {
            content,
            frame: Rect::default(),
            style: Default::default(),
            accessible_role: None,
            accessible_selected: None,
            accessible_expanded: None,
            accessible_name: None,
        },
        children: vec![],
    }
}

fn present(root: &mut NativeRoot<WindowsHost>, nodes: &mut [PresentationNode]) {
    root.present(nodes, root.host().viewport(), root.host().environment())
        .unwrap();
}

fn reduce(nodes: &mut [PresentationNode], events: &[PlatformEvent]) {
    for node in nodes {
        for event in events.iter().filter(|event| event.node == node.id) {
            match (&mut node.descriptor.content, &event.payload) {
                (
                    NodeContent::Input {
                        value, revision, ..
                    },
                    EventPayload::Edit {
                        value: next,
                        revision: next_revision,
                        ..
                    },
                ) => {
                    *value = next.clone();
                    *revision = *next_revision;
                }
                (
                    NodeContent::Checkbox {
                        checked, revision, ..
                    },
                    EventPayload::Toggle {
                        checked: next,
                        revision: next_revision,
                    },
                ) => {
                    *checked = *next;
                    *revision = *next_revision;
                }
                _ => {}
            }
        }
        reduce(&mut node.children, events);
    }
}

#[test]
fn uia_reads_accessible_name_and_native_password_state_after_updates() {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
    use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation, UIA_EditControlTypeId};

    let mut root = NativeRoot::new(WindowsHost::new("Jadix accessibility", [640, 480]).unwrap());
    let mut nodes = vec![node(
        root.model().id(),
        NodeContent::Input {
            value: "private value".into(),
            placeholder: "Password".into(),
            enabled: true,
            password: true,
            revision: 0,
        },
    )];
    nodes[0].descriptor.accessible_name = Some("账户密码 Password".into());
    present(&mut root, &mut nodes);
    let window = HWND(root.host().control_identity(nodes[0].id).unwrap() as *mut _);
    let automation: IUIAutomation =
        unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }.unwrap();
    let element = unsafe { automation.ElementFromHandle(window) }.unwrap();
    assert_eq!(
        unsafe { element.CurrentName() }.unwrap().to_string(),
        "账户密码 Password"
    );
    assert_eq!(
        unsafe { element.CurrentControlType() }.unwrap(),
        UIA_EditControlTypeId
    );
    assert!(unsafe { element.CurrentIsEnabled() }.unwrap().as_bool());
    assert!(unsafe { element.CurrentIsPassword() }.unwrap().as_bool());

    nodes[0].descriptor.accessible_name = Some("新的名称".into());
    if let NodeContent::Input {
        enabled, password, ..
    } = &mut nodes[0].descriptor.content
    {
        *enabled = false;
        *password = false;
    }
    present(&mut root, &mut nodes);
    assert_eq!(
        unsafe { element.CurrentName() }.unwrap().to_string(),
        "新的名称"
    );
    assert!(!unsafe { element.CurrentIsEnabled() }.unwrap().as_bool());
    assert!(!unsafe { element.CurrentIsPassword() }.unwrap().as_bool());

    nodes[0].descriptor.accessible_name = None;
    present(&mut root, &mut nodes);
    assert_ne!(
        unsafe { element.CurrentName() }.unwrap().to_string(),
        "新的名称"
    );
}

#[test]
fn native_controls_preserve_unicode_selection_identity_and_undo_after_reorder() {
    let mut root =
        NativeRoot::new(WindowsHost::new("Jadix native regression", [640, 480]).unwrap());
    let identifier = root.model().id();
    let mut nodes = vec![node(identifier, NodeContent::Container)];
    nodes[0].children = vec![
        node(
            identifier,
            NodeContent::Input {
                value: "A😀中".into(),
                placeholder: "write".into(),
                enabled: true,
                password: false,
                revision: 0,
            },
        ),
        node(
            identifier,
            NodeContent::Button {
                label: "count".into(),
                enabled: true,
            },
        ),
        node(
            identifier,
            NodeContent::Checkbox {
                label: "checked".into(),
                checked: false,
                enabled: true,
                revision: 0,
            },
        ),
    ];
    let input = nodes[0].children[0].id;
    let button = nodes[0].children[1].id;
    let checkbox = nodes[0].children[2].id;
    present(&mut root, &mut nodes);
    let identity = root.host().control_identity(input).unwrap();
    assert_eq!(root.host().text(input).unwrap(), "A😀中");
    assert!(root.host().select_range(input, [1, 2]).is_err());
    root.host().select_range(input, [1, 3]).unwrap();
    root.host().replace_selection(input, "🙂文").unwrap();
    assert_eq!(root.host().text(input).unwrap(), "A🙂文中");
    let events = root.poll_events().unwrap();
    let edit = events
        .iter()
        .find(|event| event.node == input && matches!(event.payload, EventPayload::Edit { .. }))
        .unwrap();
    let EventPayload::Edit {
        value,
        revision,
        selection_utf16,
        ..
    } = &edit.payload
    else {
        unreachable!()
    };
    assert_eq!(value, "A🙂文中");
    assert_eq!(*selection_utf16, [4, 4]);
    let NodeContent::Input {
        value: current,
        revision: current_revision,
        ..
    } = &mut nodes[0].children[0].descriptor.content
    else {
        unreachable!()
    };
    *current = value.clone();
    *current_revision = *revision;
    nodes[0].children.swap(0, 1);
    present(&mut root, &mut nodes);
    assert_eq!(root.host().control_identity(input), Some(identity));
    assert_eq!(root.host().selection(input).unwrap(), [4, 4]);
    let revision = root.model().revision();
    present(&mut root, &mut nodes);
    assert_eq!(root.model().revision(), revision);
    assert!(root.host().undo(input).unwrap());
    assert_eq!(root.host().text(input).unwrap(), "A😀中");
    reduce(&mut nodes, &root.poll_events().unwrap());
    root.host().activate(button).unwrap();
    root.host().activate(checkbox).unwrap();
    let events = root.poll_events().unwrap();
    assert!(events
        .iter()
        .any(|event| event.node == button && event.payload == EventPayload::Activate));
    assert!(events.iter().any(|event| event.node == checkbox
        && matches!(event.payload, EventPayload::Toggle { checked: true, .. })));
    reduce(&mut nodes, &events);
    present(&mut root, &mut nodes);
    root.host().activate(button).unwrap();
    nodes[0].children.retain(|node| node.id != button);
    present(&mut root, &mut nodes);
    assert!(!root
        .poll_events()
        .unwrap()
        .iter()
        .any(|event| event.node == button));
    root.dispose();
    assert!(root.host().is_closed());
    assert!(root.host().text(input).is_err());
}

#[test]
fn native_roots_are_isolated_and_reject_missing_overlay_anchors_before_publication() {
    let mut first = NativeRoot::new(WindowsHost::new("Jadix first root", [320, 240]).unwrap());
    let mut second = NativeRoot::new(WindowsHost::new("Jadix second root", [320, 240]).unwrap());
    let mut nodes = vec![node(
        second.model().id(),
        NodeContent::Button {
            label: "alive".into(),
            enabled: true,
        },
    )];
    present(&mut second, &mut nodes);
    first.dispose();
    second.host().activate(nodes[0].id).unwrap();
    assert_eq!(
        second
            .poll_events()
            .unwrap()
            .iter()
            .filter(|event| event.payload == EventPayload::Activate)
            .count(),
        1
    );
    let mut invalid = vec![node(
        second.model().id(),
        NodeContent::Popover {
            anchor: nodes[0].id,
            placement: PopoverPlacement::Bottom,
            gap: 4,
            open: true,
            close_on_outside_click: true,
            revision: 0,
        },
    )];
    assert!(second.present(&mut invalid, [320.0, 240.0], 0).is_err());
    assert!(second.host().control_identity(invalid[0].id).is_none());
    assert!(!second.model().lifetime().is_disposed());
}

#[test]
fn native_scroll_offsets_and_selectors_survive_incremental_updates() {
    let mut root =
        NativeRoot::new(WindowsHost::new("Jadix scrolling and selection", [640, 480]).unwrap());
    let identifier = root.model().id();
    let mut nodes = vec![node(identifier, NodeContent::Scroll)];
    nodes[0].descriptor.style.width = Some(200.0);
    nodes[0].descriptor.style.height = Some(120.0);
    let mut content = node(identifier, NodeContent::Container);
    content.descriptor.style.width = Some(600.0);
    content.descriptor.style.height = Some(900.0);
    content.descriptor.style.shrink = 0.0;
    let selector = node(
        identifier,
        NodeContent::Select {
            options: vec![
                SelectOption {
                    value: "apple".into(),
                    label: "Apple".into(),
                    enabled: true,
                },
                SelectOption {
                    value: "disabled".into(),
                    label: "Disabled".into(),
                    enabled: false,
                },
                SelectOption {
                    value: "pear".into(),
                    label: "Pear".into(),
                    enabled: true,
                },
            ],
            value: "apple".into(),
            placeholder: "Choose".into(),
            enabled: true,
            revision: 0,
        },
    );
    let select = selector.id;
    content.children.push(selector);
    nodes[0].children.push(content);
    present(&mut root, &mut nodes);
    assert_eq!(root.host().text(select).unwrap(), "Apple");
    assert!(root.host().select_value(select, "disabled").is_err());
    root.host().select_value(select, "pear").unwrap();
    root.host().scroll_to(nodes[0].id, [80.0, 240.0]).unwrap();
    assert_eq!(
        root.host().scroll_offset(nodes[0].id).unwrap(),
        [80.0, 240.0]
    );
    let events = root.poll_events().unwrap();
    let selection = events
        .iter()
        .find(|event| event.node == select && matches!(event.payload, EventPayload::Select { .. }))
        .unwrap();
    let EventPayload::Select { value, revision } = &selection.payload else {
        unreachable!()
    };
    assert_eq!(value, "pear");
    let NodeContent::Select {
        value: current,
        revision: current_revision,
        ..
    } = &mut nodes[0].children[0].children[0].descriptor.content
    else {
        unreachable!()
    };
    *current = value.clone();
    *current_revision = *revision;
    assert!(events
        .iter()
        .any(|event| event.payload == EventPayload::Scroll { x: 80.0, y: 240.0 }));
    let identity = root.host().control_identity(select);
    present(&mut root, &mut nodes);
    assert_eq!(root.host().control_identity(select), identity);
    assert_eq!(
        root.host().scroll_offset(nodes[0].id).unwrap(),
        [80.0, 240.0]
    );
    nodes[0].children[0].descriptor.style.height = Some(60.0);
    nodes[0].children[0].descriptor.style.width = Some(80.0);
    present(&mut root, &mut nodes);
    assert_eq!(root.host().scroll_offset(nodes[0].id).unwrap(), [0.0, 0.0]);
    root.host()
        .scroll_to(nodes[0].id, [0.0, f64::NAN])
        .unwrap_err();
}

#[test]
fn native_queue_overflow_revokes_only_its_owner() {
    let mut root = NativeRoot::new(WindowsHost::new("Jadix bounded queue", [320, 240]).unwrap());
    let independent =
        NativeRoot::new(WindowsHost::new("Jadix independent queue", [320, 240]).unwrap());
    let mut nodes = vec![node(
        root.model().id(),
        NodeContent::Button {
            label: "event".into(),
            enabled: true,
        },
    )];
    present(&mut root, &mut nodes);
    for _ in 0..1025 {
        root.host().activate(nodes[0].id).unwrap();
    }
    assert!(root.poll_events().is_err());
    assert!(root.model().lifetime().is_disposed());
    assert!(root.host().is_closed());
    assert!(!independent.model().lifetime().is_disposed());
}

#[test]
fn native_label_update_does_not_reposition_unchanged_controls_or_publish_idle() {
    let mut root = NativeRoot::new(WindowsHost::new("Jadix incremental work", [320, 240]).unwrap());
    let mut nodes = vec![node(
        root.model().id(),
        NodeContent::Button {
            label: "before".into(),
            enabled: true,
        },
    )];
    nodes[0].descriptor.style.width = Some(160.0);
    nodes[0].descriptor.style.height = Some(44.0);
    present(&mut root, &mut nodes);
    let initial = root.host().stats();
    nodes[0].descriptor.content = NodeContent::Button {
        label: "after".into(),
        enabled: true,
    };
    present(&mut root, &mut nodes);
    let changed = root.host().stats();
    assert_eq!(changed.creates, initial.creates);
    assert_eq!(changed.updates, initial.updates + 1);
    assert_eq!(changed.positions, initial.positions);
    present(&mut root, &mut nodes);
    assert_eq!(root.host().stats(), changed);
    assert_eq!(root.host().diagnostics()["controls"], 1);
}

#[test]
fn native_tab_traverses_nested_containers_and_reveals_the_focused_editor() {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetFocus, SetFocus, VK_TAB};
    use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_KEYDOWN};

    let mut root =
        NativeRoot::new(WindowsHost::new("Jadix keyboard navigation", [320, 240]).unwrap());
    let identifier = root.model().id();
    let editor = || {
        node(
            identifier,
            NodeContent::Input {
                value: String::new(),
                placeholder: String::new(),
                enabled: true,
                password: false,
                revision: 0,
            },
        )
    };
    let mut nodes = vec![node(identifier, NodeContent::Scroll)];
    nodes[0].descriptor.style.width = Some(240.0);
    nodes[0].descriptor.style.height = Some(80.0);
    let mut body = node(identifier, NodeContent::Container);
    body.descriptor.style.width = Some(220.0);
    body.descriptor.style.height = Some(600.0);
    body.descriptor.style.shrink = 0.0;
    let mut first = editor();
    first.descriptor.style.height = Some(32.0);
    first.descriptor.style.shrink = 0.0;
    let mut spacer = node(identifier, NodeContent::Container);
    spacer.descriptor.style.height = Some(400.0);
    spacer.descriptor.style.shrink = 0.0;
    let mut last = editor();
    last.descriptor.style.height = Some(32.0);
    last.descriptor.style.shrink = 0.0;
    let first = {
        let identifier = first.id;
        body.children.push(first);
        identifier
    };
    body.children.push(spacer);
    let last = {
        let identifier = last.id;
        body.children.push(last);
        identifier
    };
    nodes[0].children.push(body);
    present(&mut root, &mut nodes);
    root.host().show();
    let first_window = root.host().control_identity(first).unwrap() as HWND;
    let last_window = root.host().control_identity(last).unwrap() as HWND;
    unsafe { SetFocus(first_window) };
    root.poll_events().unwrap();
    assert_eq!(unsafe { GetFocus() }, first_window);
    assert_ne!(
        unsafe { PostMessageW(first_window, WM_KEYDOWN, VK_TAB as usize, 0) },
        0
    );
    root.host().pump();
    assert_eq!(unsafe { GetFocus() }, last_window);
    let events = root.poll_events().unwrap();
    assert!(events
        .iter()
        .any(|event| event.node == last && event.payload == EventPayload::Focus));
    assert!(root.host().scroll_offset(nodes[0].id).unwrap()[1] > 300.0);
    let frame = root.host().control_frame(last).unwrap();
    assert!(
        frame.y >= -1.0 && frame.y + frame.height <= 81.0,
        "{frame:?}"
    );
}

#[test]
fn embedded_root_disposal_preserves_the_parent_and_parent_teardown_revokes_children() {
    use jadix_native::host::PlatformHost;
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetParent, IsWindow};

    let mut parent = WindowsHost::new("Jadix embedding parent", [640, 480]).unwrap();
    let parent_window = parent.native_window();
    let embedded = unsafe { WindowsHost::embedded(parent_window, [320, 240]) }.unwrap();
    assert_eq!(
        unsafe { GetParent(embedded.native_window() as HWND) },
        parent_window as HWND
    );
    let mut root = NativeRoot::new(embedded);
    let mut nodes = vec![node(
        root.model().id(),
        NodeContent::Button {
            label: "embedded".into(),
            enabled: true,
        },
    )];
    present(&mut root, &mut nodes);
    root.host().activate(nodes[0].id).unwrap();
    assert!(root
        .poll_events()
        .unwrap()
        .iter()
        .any(|event| event.payload == EventPayload::Activate));
    root.dispose();
    assert_ne!(unsafe { IsWindow(parent_window as HWND) }, 0);
    assert!(!parent.is_closed());

    let embedded = unsafe { WindowsHost::embedded(parent_window, [320, 240]) }.unwrap();
    let mut root = NativeRoot::new(embedded);
    let mut nodes = vec![node(
        root.model().id(),
        NodeContent::Text("retained child".into()),
    )];
    present(&mut root, &mut nodes);
    parent.dispose();
    assert!(root.host().is_closed());
    assert!(root.present(&mut nodes, [320.0, 240.0], 1).is_err());
    assert!(root.model().lifetime().is_disposed());
    assert!(unsafe { WindowsHost::embedded(0, [320, 240]) }.is_err());
}

#[test]
fn native_images_decode_off_thread_recover_from_failure_and_discard_removed_results() {
    let mut root = NativeRoot::new(WindowsHost::new("Jadix native images", [320, 240]).unwrap());
    let image = |source: &str| {
        node(
            root.model().id(),
            NodeContent::Image {
                source: source.trim().into(),
                alt: "Native image / 图片".into(),
            },
        )
    };
    let valid = include_str!("../../../examples/native-controls/assets/pixel.txt");
    let broken = include_str!("../../../examples/native-controls/assets/broken-pixel.txt");
    let mut nodes = vec![image(valid), image(broken)];
    present(&mut root, &mut nodes);
    let identity = root.host().control_identity(nodes[1].id);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        root.host().pump();
        root.poll_events().unwrap();
        let state = root.host().diagnostics();
        if state["images_ready"] == 1 && state["images_failed"] == 1 {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "{state}");
        root.host().wait_for_events(50).unwrap();
    }
    nodes[1].descriptor.content = NodeContent::Image {
        source: valid.trim().into(),
        alt: "Recovered".into(),
    };
    present(&mut root, &mut nodes);
    loop {
        root.host().pump();
        root.poll_events().unwrap();
        let state = root.host().diagnostics();
        if state["images_ready"] == 2 {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "{state}");
        root.host().wait_for_events(50).unwrap();
    }
    assert_eq!(root.host().control_identity(nodes[1].id), identity);
    nodes[0].descriptor.content = NodeContent::Image {
        source: broken.trim().into(),
        alt: "Pending removal".into(),
    };
    present(&mut root, &mut nodes);
    present(&mut root, &mut []);
    root.host().pump();
    root.poll_events().unwrap();
    assert_eq!(root.host().diagnostics()["images_ready"], 0);
    assert_eq!(root.host().diagnostics()["images_loading"], 0);
    assert_eq!(root.host().diagnostics()["controls"], 0);
    root.dispose();
    assert!(root.host().is_closed());
}

#[test]
fn native_nested_modals_gate_background_trap_tab_dismiss_and_restore_focus() {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetFocus, IsWindowEnabled, SetFocus, VK_ESCAPE, VK_TAB,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{IsWindowVisible, PostMessageW, WM_KEYDOWN};

    let mut root = NativeRoot::new(WindowsHost::new("Jadix native modals", [640, 480]).unwrap());
    let identifier = root.model().id();
    let mut nodes = vec![node(identifier, NodeContent::Container)];
    nodes[0].children.push(node(
        identifier,
        NodeContent::Button {
            label: "background".into(),
            enabled: true,
        },
    ));
    let editor = || {
        node(
            identifier,
            NodeContent::Input {
                value: "draft".into(),
                placeholder: "name".into(),
                enabled: true,
                password: false,
                revision: 0,
            },
        )
    };
    let mut dialog = node(
        identifier,
        NodeContent::Dialog {
            open: false,
            close_on_overlay_click: true,
            revision: 0,
        },
    );
    dialog.descriptor.style.width = Some(240.0);
    dialog.children.push(editor());
    let mut nested = node(
        identifier,
        NodeContent::Dialog {
            open: false,
            close_on_overlay_click: false,
            revision: 0,
        },
    );
    nested.children.push(editor());
    let nested_id = nested.id;
    let inner_input = nested.children[0].id;
    let dialog_id = dialog.id;
    let outer_input = dialog.children[0].id;
    dialog.children.push(nested);
    nodes[0].children.push(dialog);
    let background = nodes[0].children[0].id;
    present(&mut root, &mut nodes);
    root.host().show();
    let background_window = root.host().control_identity(background).unwrap() as HWND;
    let outer_window = root.host().control_identity(outer_input).unwrap() as HWND;
    let inner_window = root.host().control_identity(inner_input).unwrap() as HWND;
    assert_eq!(unsafe { IsWindowVisible(outer_window) }, 0);
    unsafe { SetFocus(background_window) };
    root.poll_events().unwrap();
    if let NodeContent::Dialog { open, .. } = &mut nodes[0].children[1].descriptor.content {
        *open = true;
    }
    present(&mut root, &mut nodes);
    root.poll_events().unwrap();
    assert_eq!(unsafe { IsWindowEnabled(background_window) }, 0);
    assert_eq!(unsafe { GetFocus() }, outer_window);
    let outer_identity = root.host().control_identity(outer_input);
    if let NodeContent::Dialog { open, .. } =
        &mut nodes[0].children[1].children[1].descriptor.content
    {
        *open = true;
    }
    present(&mut root, &mut nodes);
    root.poll_events().unwrap();
    assert_eq!(unsafe { GetFocus() }, inner_window);
    assert!(root.host().dismiss(dialog_id, false).is_err());
    root.host().dismiss(nested_id, true).unwrap();
    assert!(!root
        .poll_events()
        .unwrap()
        .iter()
        .any(|event| matches!(event.payload, EventPayload::Dismiss { .. })));
    unsafe { PostMessageW(inner_window, WM_KEYDOWN, VK_TAB as usize, 0) };
    root.host().pump();
    root.poll_events().unwrap();
    assert_eq!(unsafe { GetFocus() }, inner_window);
    unsafe { PostMessageW(inner_window, WM_KEYDOWN, VK_ESCAPE as usize, 0) };
    root.host().pump();
    let events = root.poll_events().unwrap();
    let revision = events
        .iter()
        .find_map(|event| match event.payload {
            EventPayload::Dismiss {
                backdrop: false,
                revision,
            } if event.node == nested_id => Some(revision),
            _ => None,
        })
        .unwrap();
    if let NodeContent::Dialog {
        open,
        revision: current,
        ..
    } = &mut nodes[0].children[1].children[1].descriptor.content
    {
        *open = false;
        *current = revision;
    }
    present(&mut root, &mut nodes);
    root.poll_events().unwrap();
    assert_eq!(unsafe { GetFocus() }, outer_window);
    assert_eq!(root.host().control_identity(outer_input), outer_identity);
    root.host().dismiss(dialog_id, true).unwrap();
    let events = root.poll_events().unwrap();
    let revision = events
        .iter()
        .find_map(|event| match event.payload {
            EventPayload::Dismiss {
                backdrop: true,
                revision,
            } if event.node == dialog_id => Some(revision),
            _ => None,
        })
        .unwrap();
    if let NodeContent::Dialog {
        open,
        revision: current,
        ..
    } = &mut nodes[0].children[1].descriptor.content
    {
        *open = false;
        *current = revision;
    }
    present(&mut root, &mut nodes);
    root.poll_events().unwrap();
    assert_eq!(unsafe { GetFocus() }, background_window);
    assert_ne!(unsafe { IsWindowEnabled(background_window) }, 0);
    assert_eq!(unsafe { IsWindowVisible(inner_window) }, 0);
}
