#![cfg(all(windows, feature = "windows"))]

use jadix_native::{host::NativeRoot, presentation::*, windows::WindowsHost};
use windows_sys::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{
        PostMessageW, SendMessageW, SetWindowTextW, DLGC_WANTMESSAGE, WM_CHAR, WM_GETDLGCODE,
        WM_IME_ENDCOMPOSITION, WM_IME_STARTCOMPOSITION, WM_KEYDOWN,
    },
};

fn editor(root: RootId, multiline: bool) -> PresentationNode {
    PresentationNode {
        id: NodeId::new(root),
        descriptor: NodeDescriptor {
            content: if multiline {
                NodeContent::Textarea {
                    value: String::new(),
                    placeholder: String::new(),
                    enabled: true,
                    revision: 0,
                    max_length: None,
                }
            } else {
                NodeContent::Input {
                    value: String::new(),
                    placeholder: String::new(),
                    enabled: true,
                    password: false,
                    revision: 0,
                    max_length: None,
                }
            },
            frame: Rect::default(),
            style: Default::default(),
            accessible_name: None,
            accessible_description: None,
            accessible_role: None,
            accessible_selected: None,
            accessible_expanded: None,
            interaction: Default::default(),
            keyboard: Default::default(),
            navigation_keys: Vec::new(),
            accessibility: Default::default(),
            accessibility_relations: Default::default(),
        },
        children: vec![],
    }
}

#[test]
fn native_enter_submits_committed_single_line_edits_and_preserves_multiline_newlines() {
    let mut root =
        NativeRoot::new(WindowsHost::new("Jadix submit acceptance", [360, 220]).unwrap());
    let mut nodes = vec![
        editor(root.model().id(), false),
        editor(root.model().id(), true),
    ];
    for node in &mut nodes {
        node.descriptor.style.height = Some(60.0);
    }
    root.present(
        &mut nodes,
        root.host().viewport(),
        root.host().environment(),
    )
    .unwrap();
    let input = root.host().control_identity(nodes[0].id).unwrap() as HWND;
    let textarea = root.host().control_identity(nodes[1].id).unwrap() as HWND;
    // Real keyboard input sends supplementary characters as two WM_CHARs,
    // with an EN_CHANGE notification after each UTF-16 code unit.
    unsafe { SendMessageW(input, WM_CHAR, 0xd83d, 1); }
    assert!(root.poll_events().unwrap().iter().all(|event|
        !matches!(&event.payload, EventPayload::Edit { value, .. } if !value.is_empty())
    ));
    unsafe { SendMessageW(input, WM_CHAR, 0xde00, 1); }
    assert!(root.poll_events().unwrap().iter().any(|event|
        matches!(&event.payload, EventPayload::Edit { value, .. } if value == "😀")
    ));
    unsafe { SendMessageW(input, WM_CHAR, 1, 1); }
    assert_eq!(root.host().selection(nodes[0].id).unwrap(), [0, 2]);
    unsafe { SendMessageW(input, WM_CHAR, b'x' as usize, 1); }
    assert_eq!(root.host().text(nodes[0].id).unwrap(), "x");
    let multiline: Vec<u16> = "多行😀".encode_utf16().chain(Some(0)).collect();
    unsafe {
        SetWindowTextW(textarea, multiline.as_ptr());
        SendMessageW(textarea, WM_CHAR, 1, 1);
    }
    assert_eq!(root.host().selection(nodes[1].id).unwrap(), [0, 4]);
    unsafe { SetWindowTextW(textarea, [0u16].as_ptr()); }
    assert_ne!(
        unsafe { SendMessageW(input, WM_GETDLGCODE, 13, 0) } & DLGC_WANTMESSAGE as isize,
        0
    );
    let text: Vec<u16> = "中😀".encode_utf16().chain(Some(0)).collect();
    assert_ne!(unsafe { SetWindowTextW(input, text.as_ptr()) }, 0);
    assert_ne!(unsafe { PostMessageW(input, WM_KEYDOWN, 13, 1) }, 0);
    root.host().pump();
    let events = root.poll_events().unwrap();
    let edited = events.iter().position(|event| event.node == nodes[0].id && matches!(
        &event.payload, EventPayload::Edit { value, composing_utf16: None, .. } if value == "中😀"
    )).expect("native edit must precede submission");
    let submitted = events
        .iter()
        .position(|event| {
            event.node == nodes[0].id && matches!(event.payload, EventPayload::Submit { .. })
        })
        .expect("dialog dispatch must deliver Enter to the edit subclass");
    assert!(edited < submitted);
    let EventPayload::Submit { revision } = events[submitted].payload else {
        unreachable!()
    };
    assert!(revision > 0);
    assert!(!events
        .iter()
        .any(|event| matches!(event.payload, EventPayload::Activate)));

    unsafe {
        SendMessageW(input, WM_KEYDOWN, 13, (1isize << 30) | 1);
        SendMessageW(input, WM_IME_STARTCOMPOSITION, 0, 0);
        SendMessageW(input, WM_KEYDOWN, 13, 1);
        SendMessageW(input, WM_IME_ENDCOMPOSITION, 0, 0);
        SendMessageW(textarea, WM_CHAR, 13, 1);
    }
    let events = root.poll_events().unwrap();
    assert!(!events
        .iter()
        .any(|event| matches!(event.payload, EventPayload::Submit { .. })));
    assert!(events.iter().any(|event| event.node == nodes[1].id
        && matches!(
            &event.payload, EventPayload::Edit { value, .. } if value.contains('\n')
        )));
    unsafe {
        SendMessageW(input, WM_KEYDOWN, 13, 1);
    }
    assert!(root.poll_events().unwrap().iter().any(|event| matches!(
        event.payload, EventPayload::Submit { revision: next } if next > revision
    )));

    unsafe {
        windows_sys::Win32::UI::Input::KeyboardAndMouse::EnableWindow(input, 0);
    }
    unsafe {
        SendMessageW(input, WM_KEYDOWN, 13, 1);
    }
    assert!(!root
        .poll_events()
        .unwrap()
        .iter()
        .any(|event| matches!(event.payload, EventPayload::Submit { .. })));
    root.dispose();
    assert_eq!(unsafe { PostMessageW(input, WM_KEYDOWN, 13, 1) }, 0);
}
