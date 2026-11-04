#![cfg(all(windows, feature = "windows"))]

use jadix_native::{host::NativeRoot, presentation::*, windows::WindowsHost};
use windows_sys::Win32::{
    Foundation::HWND,
    UI::{
        Input::KeyboardAndMouse::{GetFocus, SetFocus, VK_ESCAPE},
        WindowsAndMessaging::*,
    },
};

fn node(root: RootId, content: NodeContent, size: [f32; 2]) -> PresentationNode {
    PresentationNode {
        id: NodeId::new(root),
        descriptor: NodeDescriptor {
            content,
            frame: Rect::default(),
            accessible_role: None,
            accessible_selected: None,
            accessible_expanded: None,
            accessible_name: None,
            style: style::NodeStyle {
                width: Some(size[0]),
                height: Some(size[1]),
                shrink: 0.0,
                ..Default::default()
            },
        },
        children: vec![],
    }
}

fn present(root: &mut NativeRoot<WindowsHost>, nodes: &mut [PresentationNode]) {
    root.present(nodes, root.host().viewport(), root.host().environment())
        .unwrap();
}

#[test]
fn native_popover_escapes_scroll_clip_tracks_anchor_and_restores_focus_after_escape() {
    let mut root = NativeRoot::new(WindowsHost::new("Jadix native popovers", [640, 480]).unwrap());
    let identifier = root.model().id();
    let mut scroll = node(identifier, NodeContent::Scroll, [220.0, 100.0]);
    let mut body = node(identifier, NodeContent::Container, [180.0, 800.0]);
    let anchor = node(
        identifier,
        NodeContent::Button {
            label: "anchor".into(),
            enabled: true,
        },
        [140.0, 32.0],
    );
    let anchor_id = anchor.id;
    let mut panel = node(
        identifier,
        NodeContent::Popover {
            anchor: anchor_id,
            placement: PopoverPlacement::BottomStart,
            gap: 4,
            open: false,
            close_on_outside_click: false,
            revision: 0,
        },
        [320.0, 160.0],
    );
    panel.children.push(node(
        identifier,
        NodeContent::Input {
            value: "draft".into(),
            placeholder: String::new(),
            enabled: true,
            password: false,
            revision: 0,
        },
        [300.0, 40.0],
    ));
    let panel_id = panel.id;
    let input_id = panel.children[0].id;
    body.children = vec![anchor, panel];
    scroll.children.push(body);
    let mut nodes = vec![scroll];
    present(&mut root, &mut nodes);
    root.host().show();
    let panel_window = root.host().control_identity(panel_id).unwrap() as HWND;
    let input_window = root.host().control_identity(input_id).unwrap() as HWND;
    let anchor_window = root.host().control_identity(anchor_id).unwrap() as HWND;
    assert_eq!(unsafe { IsWindowVisible(panel_window) }, 0);
    if let NodeContent::Popover { open, .. } =
        &mut nodes[0].children[0].children[1].descriptor.content
    {
        *open = true;
    }
    present(&mut root, &mut nodes);
    root.poll_events().unwrap();
    assert_ne!(unsafe { IsWindowVisible(panel_window) }, 0);
    assert_eq!(
        unsafe { GetParent(panel_window) },
        root.host().native_window() as HWND
    );
    let before = root.host().control_frame(panel_id).unwrap();
    assert!(before.width > 220.0);
    root.host().scroll_to(nodes[0].id, [0.0, 20.0]).unwrap();
    root.poll_events().unwrap();
    let after = root.host().control_frame(panel_id).unwrap();
    assert!(after.y < before.y);
    root.host().dismiss(panel_id, true).unwrap();
    assert!(!root
        .poll_events()
        .unwrap()
        .iter()
        .any(|event| matches!(event.payload, EventPayload::Dismiss { .. })));
    unsafe {
        SetFocus(input_window);
        PostMessageW(input_window, WM_KEYDOWN, VK_ESCAPE as usize, 0);
    }
    root.host().pump();
    let events = root.poll_events().unwrap();
    let revision = events
        .iter()
        .find_map(|event| match event.payload {
            EventPayload::Dismiss {
                backdrop: false,
                revision,
            } if event.node == panel_id => Some(revision),
            _ => None,
        })
        .unwrap();
    if let NodeContent::Popover {
        open,
        revision: current,
        ..
    } = &mut nodes[0].children[0].children[1].descriptor.content
    {
        *open = false;
        *current = revision;
    }
    present(&mut root, &mut nodes);
    root.poll_events().unwrap();
    assert_eq!(unsafe { GetFocus() }, anchor_window);
    assert_eq!(unsafe { IsWindowVisible(input_window) }, 0);
    if let NodeContent::Popover { open, .. } =
        &mut nodes[0].children[0].children[1].descriptor.content
    {
        *open = true;
    }
    present(&mut root, &mut nodes);
    root.host().scroll_to(nodes[0].id, [0.0, 40.0]).unwrap();
    nodes[0].children[0].children.pop();
    present(&mut root, &mut nodes);
    assert!(!root.model().lifetime().is_disposed());
    assert!(root.host().control_identity(panel_id).is_none());
}
