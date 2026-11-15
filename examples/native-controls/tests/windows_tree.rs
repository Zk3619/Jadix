#![cfg(all(windows, feature = "windows", not(feature = "gpu")))]

use jadix_core::tree::{Children, Tree, TreeLimits, TreeNode};
use jadix_native::windows::WindowsHost;
use jadix_native_controls::tree::TreeBrowser;
use windows::core::Interface;
use windows::Win32::{
    Foundation::HWND as UIA_HWND,
    System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER},
    UI::Accessibility::*,
};
use windows_sys::Win32::{
    Foundation::HWND,
    UI::{
        Input::KeyboardAndMouse::{GetFocus, VK_DOWN, VK_END, VK_HOME, VK_LEFT, VK_RIGHT},
        WindowsAndMessaging::{
            PostMessageW, STATE_SYSTEM_COLLAPSED, STATE_SYSTEM_EXPANDED, STATE_SYSTEM_SELECTABLE,
            STATE_SYSTEM_SELECTED, WM_KEYDOWN,
        },
    },
};

fn press(app: &mut TreeBrowser<WindowsHost>, key: u16) {
    let window = unsafe { GetFocus() };
    assert!(!window.is_null());
    assert_ne!(
        unsafe { PostMessageW(window, WM_KEYDOWN, key as usize, 0) },
        0
    );
    app.root.host().pump();
    app.poll_events().unwrap();
    let focused = *app.model().focused().unwrap();
    let node = app.row_node(focused).unwrap_or_else(|| {
        panic!(
            "focused {focused} is not mounted after key {key}: {}",
            app.state()
        )
    });
    assert_eq!(
        unsafe { GetFocus() } as usize,
        app.root.host().control_identity(node).unwrap(),
        "native focus for {focused} after key {key}"
    );
}

fn legacy_state(automation: &IUIAutomation, window: usize) -> u32 {
    let item = unsafe { automation.ElementFromHandle(UIA_HWND(window as *mut _)) }.unwrap();
    let legacy = unsafe { item.GetCurrentPattern(UIA_LegacyIAccessiblePatternId) }
        .unwrap()
        .cast::<IUIAutomationLegacyIAccessiblePattern>()
        .unwrap();
    unsafe { legacy.CurrentState() }.unwrap()
}

#[test]
fn real_native_arrow_keys_drive_tree_focus_expansion_selection_and_virtual_reveal() {
    let children = (1..10_000)
        .map(|key| TreeNode::leaf(key, format!("File {key}")))
        .collect();
    let mut tree = Tree::new(
        vec![TreeNode {
            key: 0,
            label: "Folder".into(),
            enabled: true,
            children: Children::Loaded(children),
        }],
        TreeLimits::default(),
    )
    .unwrap();
    tree.set_expanded(&0, true).unwrap();
    let host = WindowsHost::new("Jadix native tree keyboard", [800, 600]).unwrap();
    let mut app = TreeBrowser::new(host, [800.0, 600.0], tree).unwrap();
    app.root.host().show();
    let row = app.row_node(0).unwrap();
    app.root.focus(Some(row)).unwrap();
    app.poll_events().unwrap();
    press(&mut app, VK_DOWN);
    assert_eq!(app.model().focused(), Some(&1));
    assert!(app.model().selected().contains(&1));
    let focused = app.row_node(1).unwrap();
    let automation: IUIAutomation =
        unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }.unwrap();
    let tree_item = unsafe {
        automation.ElementFromHandle(UIA_HWND(
            app.root.host().control_identity(focused).unwrap() as *mut _
        ))
    }
    .unwrap();
    assert_eq!(
        unsafe { tree_item.CurrentControlType() }.unwrap(),
        UIA_TreeItemControlTypeId
    );
    let state = legacy_state(
        &automation,
        app.root.host().control_identity(focused).unwrap(),
    );
    assert_ne!(state & STATE_SYSTEM_SELECTABLE, 0);
    assert_ne!(state & STATE_SYSTEM_SELECTED, 0);
    assert_eq!(
        unsafe { GetFocus() },
        app.root.host().control_identity(focused).unwrap() as HWND
    );
    press(&mut app, VK_END);
    assert_eq!(app.model().focused(), Some(&9999));
    let row = app.row_node(9999).unwrap();
    let frame = app.root.host().control_frame(row).unwrap();
    assert!(
        frame.y >= 80.0 && frame.y + frame.height <= 601.0,
        "{frame:?}"
    );
    assert!(app.state()["mounted_rows"].as_u64().unwrap() < 30);
    press(&mut app, VK_HOME);
    assert_eq!(app.model().focused(), Some(&0));
    press(&mut app, VK_LEFT);
    assert!(!app.model().expanded(&0));
    assert_eq!(app.state()["visible_rows"], 1);
    let row = app.row_node(0).unwrap();
    assert_ne!(
        legacy_state(&automation, app.root.host().control_identity(row).unwrap())
            & STATE_SYSTEM_COLLAPSED,
        0
    );
    press(&mut app, VK_RIGHT);
    assert!(app.model().expanded(&0));
    let row = app.row_node(0).unwrap();
    assert_ne!(
        legacy_state(&automation, app.root.host().control_identity(row).unwrap())
            & STATE_SYSTEM_EXPANDED,
        0
    );
    press(&mut app, VK_RIGHT);
    assert_eq!(app.model().focused(), Some(&1));
    let folder_window = app
        .root
        .host()
        .control_identity(app.row_node(0).unwrap())
        .unwrap();
    let folder =
        unsafe { automation.ElementFromHandle(UIA_HWND(folder_window as *mut _)) }.unwrap();
    let expand = unsafe { folder.GetCurrentPattern(UIA_ExpandCollapsePatternId) }
        .unwrap()
        .cast::<IUIAutomationExpandCollapsePattern>()
        .unwrap();
    let select = unsafe { folder.GetCurrentPattern(UIA_SelectionItemPatternId) }
        .unwrap()
        .cast::<IUIAutomationSelectionItemPattern>()
        .unwrap();
    unsafe { select.Select() }.unwrap();
    app.poll_events().unwrap();
    assert!(app.model().selected().contains(&0));
    assert!(app.model().expanded(&0));
    unsafe { select.RemoveFromSelection() }.unwrap();
    app.poll_events().unwrap();
    assert!(!app.model().selected().contains(&0));
    unsafe { select.AddToSelection() }.unwrap();
    unsafe { select.AddToSelection() }.unwrap();
    app.poll_events().unwrap();
    assert!(app.model().selected().contains(&0));
    assert!(unsafe { select.CurrentIsSelected() }.unwrap().as_bool());
    unsafe { expand.Collapse() }.unwrap();
    unsafe { expand.Collapse() }.unwrap();
    app.poll_events().unwrap();
    assert!(!app.model().expanded(&0));
    assert_eq!(app.state()["visible_rows"], 1);
    assert_eq!(
        unsafe { expand.CurrentExpandCollapseState() }.unwrap(),
        ExpandCollapseState_Collapsed
    );
    unsafe { expand.Expand() }.unwrap();
    app.poll_events().unwrap();
    assert!(app.model().expanded(&0));
    assert_eq!(
        unsafe { expand.CurrentExpandCollapseState() }.unwrap(),
        ExpandCollapseState_Expanded
    );
    assert_eq!(
        app.root
            .host()
            .control_identity(app.row_node(0).unwrap())
            .unwrap(),
        folder_window
    );
}
