#![cfg(all(windows, feature = "windows"))]

use jadix_native::{host::NativeRoot, presentation::*, windows::WindowsHost};
use windows_sys::Win32::{
    Foundation::HWND,
    UI::{
        Input::KeyboardAndMouse::{GetCapture, ReleaseCapture, SetCapture, VK_ESCAPE},
        WindowsAndMessaging::{
            PostMessageW, SendMessageW, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
        },
    },
};

fn point(horizontal: i16, vertical: i16) -> isize {
    (i32::from(horizontal as u16) | (i32::from(vertical as u16) << 16)) as isize
}

fn send(window: HWND, message: u32, parameter: usize, horizontal: i16, vertical: i16) {
    unsafe {
        SendMessageW(window, message, parameter, point(horizontal, vertical));
    }
}

#[test]
fn native_drag_capture_orders_coalesced_moves_with_clicks_and_releases_on_escape_capture_loss_unbind_and_dispose(
) {
    let host = WindowsHost::new("Jadix Native pointer lifecycle", [400, 300]).unwrap();
    let mut root = NativeRoot::new(host);
    let owner = root.model().id();
    let outer = NodeId::new(owner);
    let drag = NodeId::new(owner);
    let other = NodeId::new(owner);
    let button = |id, label: &str| PresentationNode {
        id,
        descriptor: NodeDescriptor {
            content: NodeContent::Button {
                label: label.into(),
                enabled: true,
            },
            frame: Rect::default(),
            accessible_role: None,
            accessible_selected: None,
            accessible_expanded: None,
            accessibility_relations: Default::default(),
            accessible_description: None,
            accessible_name: None,
            style: style::NodeStyle {
                width: Some(160.0),
                height: Some(40.0),
                shrink: 0.0,
                ..Default::default()
            },
        },
        children: vec![],
    };
    let mut nodes = vec![PresentationNode {
        id: outer,
        descriptor: NodeDescriptor {
            content: NodeContent::Container,
            frame: Rect::default(),
            accessible_role: None,
            accessible_selected: None,
            accessible_expanded: None,
            accessibility_relations: Default::default(),
            accessible_description: None,
            accessible_name: None,
            style: style::NodeStyle {
                width: Some(400.0),
                height: Some(300.0),
                ..Default::default()
            },
        },
        children: vec![
            button(drag, "Drag this tab"),
            button(other, "Normal button"),
        ],
    }];
    root.present(&mut nodes, [400.0, 300.0], 0).unwrap();
    root.host_mut().bind_drag(&[drag]).unwrap();
    assert!(root
        .host_mut()
        .bind_drag(&[NodeId::new(RootId::new())])
        .is_err());
    root.host().show();
    root.focus(Some(drag)).unwrap();
    root.poll_events().unwrap();
    let window = root.host().control_identity(drag).unwrap() as HWND;
    send(window, WM_LBUTTONDOWN, 1, 2, 2);
    send(window, WM_MOUSEMOVE, 1, 3, 3);
    send(window, WM_LBUTTONUP, 0, 3, 3);
    let click = root.poll_events().unwrap();
    assert_eq!(
        click
            .iter()
            .filter(|event| event.payload == EventPayload::Activate)
            .count(),
        1
    );
    assert!(!click
        .iter()
        .any(|event| matches!(event.payload, EventPayload::Drag { .. })));

    send(window, WM_LBUTTONDOWN, 1, 2, 2);
    for horizontal in 20..2020 {
        send(window, WM_MOUSEMOVE, 1, horizontal, 12);
    }
    assert_eq!(unsafe { GetCapture() }, window);
    send(window, WM_LBUTTONUP, 0, 80, 12);
    assert_ne!(unsafe { GetCapture() }, window);
    let events = root.poll_events().unwrap();
    let ordered: Vec<_> = events
        .iter()
        .filter_map(|event| match event.payload {
            EventPayload::Drag { phase, .. } => Some(match phase {
                DragPhase::Started => "start",
                DragPhase::Moved => "move",
                DragPhase::Ended => "end",
                DragPhase::Cancelled => "cancel",
            }),
            EventPayload::Activate => Some("click"),
            _ => None,
        })
        .collect();
    assert_eq!(ordered, ["start", "move", "end"]);
    assert!(!events
        .iter()
        .any(|event| event.payload == EventPayload::Activate));
    send(window, WM_LBUTTONDOWN, 1, 2, 2);
    send(window, WM_MOUSEMOVE, 1, 40, 12);
    root.host().activate(other).unwrap();
    let events = root.poll_events().unwrap();
    let ordered: Vec<_> = events
        .iter()
        .filter_map(|event| match event.payload {
            EventPayload::Drag { phase, .. } => Some(match phase {
                DragPhase::Started => "start",
                DragPhase::Moved => "move",
                DragPhase::Ended => "end",
                DragPhase::Cancelled => "cancel",
            }),
            EventPayload::Activate => Some("click"),
            _ => None,
        })
        .collect();
    assert_eq!(ordered, ["start", "move", "cancel", "click"]);
    assert!(events
        .iter()
        .filter(|event| event.payload == EventPayload::Activate)
        .all(|event| event.node == other));

    for cancellation in 0..4 {
        root.host_mut().bind_drag(&[drag]).unwrap();
        send(window, WM_LBUTTONDOWN, 1, 2, 2);
        send(window, WM_MOUSEMOVE, 1, 40, 12);
        if cancellation == 0 {
            assert_ne!(
                unsafe { PostMessageW(window, WM_KEYDOWN, VK_ESCAPE as usize, 0) },
                0
            );
            root.host().pump();
        } else if cancellation == 1 {
            unsafe {
                SetCapture(root.host().native_window() as HWND);
            }
        } else if cancellation == 2 {
            root.host_mut().bind_drag(&[]).unwrap();
        } else {
            nodes[0].children[0].descriptor.content = NodeContent::Button {
                label: "Disabled tab".into(),
                enabled: false,
            };
            root.present(&mut nodes, [400.0, 300.0], 0).unwrap();
        }
        let events = root.poll_events().unwrap();
        let phases: Vec<_> = events
            .iter()
            .filter_map(|event| match event.payload {
                EventPayload::Drag { phase, .. } => Some(phase),
                _ => None,
            })
            .collect();
        if cancellation == 3 {
            assert_eq!(phases, [DragPhase::Cancelled]);
        } else {
            assert_eq!(
                phases,
                [DragPhase::Started, DragPhase::Moved, DragPhase::Cancelled]
            );
        }
        assert_ne!(unsafe { GetCapture() }, window);
        unsafe {
            ReleaseCapture();
        }
    }
    nodes[0].children[0].descriptor.content = NodeContent::Button {
        label: "Drag again".into(),
        enabled: true,
    };
    root.present(&mut nodes, [400.0, 300.0], 0).unwrap();
    root.host_mut().bind_drag(&[drag]).unwrap();
    send(window, WM_LBUTTONDOWN, 1, 2, 2);
    send(window, WM_MOUSEMOVE, 1, 40, 12);
    root.present(&mut [], [400.0, 300.0], 0).unwrap();
    assert_ne!(unsafe { GetCapture() }, window);
    assert!(root.poll_events().unwrap().is_empty());
    root.dispose();
    assert!(root.model().lifetime().is_disposed());
}
