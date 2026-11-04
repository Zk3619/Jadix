#![cfg(all(windows, feature = "windows"))]

use jadix_native::{host::NativeRoot, presentation::*, windows::WindowsHost};
use windows::Win32::{
    Foundation::HWND,
    System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
    UI::Accessibility::*,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    BM_GETCHECK, BS_PUSHBUTTON, BS_RADIOBUTTON, GWL_STYLE, GetWindowLongPtrW, SendMessageW,
    WS_TABSTOP,
};

#[test]
fn radio_uses_native_checked_provider_and_updates_style_without_recreating_control() {
    let mut root = NativeRoot::new(WindowsHost::new("Native radio provider", [400, 200]).unwrap());
    let id = NodeId::new(root.model().id());
    let mut tree = [PresentationNode {
        id,
        descriptor: NodeDescriptor {
            content: NodeContent::Button {
                label: "Express".into(),
                enabled: true,
            },
            frame: Rect::default(),
            accessible_name: Some("Delivery method".into()),
            accessible_role: Some(AccessibleRole::RadioItem { tab_stop: true }),
            accessible_selected: Some(true),
            accessible_expanded: None,
            style: style::NodeStyle {
                width: Some(180.0),
                height: Some(40.0),
                ..Default::default()
            },
        },
        children: vec![],
    }];
    root.present(&mut tree, [400.0, 200.0], 0).unwrap();
    root.host().show();
    let window = root.host().control_identity(id).unwrap();
    let automation: IUIAutomation =
        unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).unwrap() };
    let provider = unsafe { automation.ElementFromHandle(HWND(window as _)).unwrap() };
    assert_eq!(
        unsafe { provider.CurrentControlType().unwrap() },
        UIA_RadioButtonControlTypeId
    );
    assert_eq!(unsafe { SendMessageW(window as _, BM_GETCHECK, 0, 0) }, 1);
    assert_ne!(
        unsafe { GetWindowLongPtrW(window as _, GWL_STYLE) } as u32 & WS_TABSTOP,
        0
    );
    tree[0].descriptor.accessible_role = Some(AccessibleRole::RadioItem { tab_stop: false });
    tree[0].descriptor.accessible_selected = Some(false);
    root.present(&mut tree, [400.0, 200.0], 0).unwrap();
    assert_eq!(root.host().control_identity(id), Some(window));
    assert_eq!(unsafe { SendMessageW(window as _, BM_GETCHECK, 0, 0) }, 0);
    assert_eq!(
        unsafe { GetWindowLongPtrW(window as _, GWL_STYLE) } as u32 & WS_TABSTOP,
        0
    );
    for (role, expected) in [
        (None, BS_PUSHBUTTON),
        (
            Some(AccessibleRole::RadioItem { tab_stop: true }),
            BS_RADIOBUTTON,
        ),
    ] {
        tree[0].descriptor.accessible_role = role;
        root.present(&mut tree, [400.0, 200.0], 0).unwrap();
        assert_eq!(root.host().control_identity(id), Some(window));
        assert_eq!(
            unsafe { GetWindowLongPtrW(window as _, GWL_STYLE) } as u32 & 0xf,
            expected as u32
        );
    }
}
