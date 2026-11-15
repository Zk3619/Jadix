#![cfg(all(windows, feature = "windows"))]

use jadix_native::{host::NativeRoot, presentation::*, windows::WindowsHost};
use windows::core::Interface;
use windows::Win32::{
    Foundation::HWND,
    System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER},
    UI::Accessibility::*,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetWindowLongPtrW, GWL_STYLE, STATE_SYSTEM_COLLAPSED, STATE_SYSTEM_EXPANDED, WS_TABSTOP,
};

fn node(root: RootId, content: NodeContent, name: &str) -> PresentationNode {
    PresentationNode {
        id: NodeId::new(root),
        descriptor: NodeDescriptor {
            content,
            frame: Rect::default(),
            accessible_role: None,
            accessible_selected: None,
            accessible_expanded: None,
            accessibility_relations: Default::default(),
            accessible_description: None,
            accessible_name: Some(name.into()),
            style: style::NodeStyle {
                width: Some(160.0),
                height: Some(80.0),
                ..Default::default()
            },
        },
        children: vec![],
    }
}

#[test]
fn descriptions_update_and_clear_in_msaa_and_custom_tree_grid_providers() {
    use windows::Win32::System::Variant::VARIANT;
    let mut root = NativeRoot::new(WindowsHost::new("Jadix descriptions", [640, 480]).unwrap());
    let button = || NodeContent::Button {
        label: "Visible".into(),
        enabled: true,
    };
    let mut roots = vec![
        node(root.model().id(), button(), "Button name"),
        node(root.model().id(), button(), "Tree name"),
        node(root.model().id(), NodeContent::Container, "Grid name"),
    ];
    roots[1].descriptor.accessible_role = Some(AccessibleRole::TreeItem);
    roots[1].descriptor.accessible_expanded = Some(false);
    roots[2].descriptor.accessible_role = Some(AccessibleRole::Grid {
        rows: 1,
        columns: 1,
    });
    root.present(&mut roots, [640.0, 480.0], 0).unwrap();
    root.host().show();
    let identities: Vec<_> = roots
        .iter()
        .map(|node| root.host().control_identity(node.id).unwrap())
        .collect();
    let automation: IUIAutomation =
        unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }.unwrap();
    for description in [Some("Help 中文 😀"), Some("Changed"), None] {
        for node in &mut roots {
            node.descriptor.accessible_description = description.map(String::from);
        }
        root.present(&mut roots, [640.0, 480.0], 1).unwrap();
        let mut accessible: Option<IAccessible> = None;
        unsafe {
            AccessibleObjectFromWindow(
                HWND(identities[0] as _),
                0xfffffffcu32,
                &IAccessible::IID,
                &mut accessible as *mut _ as *mut _,
            )
        }
        .unwrap();
        let actual = unsafe { accessible.unwrap().get_accDescription(&VARIANT::from(0)) };
        assert_eq!(
            actual.map(|value| value.to_string()).unwrap_or_default(),
            description.unwrap_or("")
        );
        for (index, node) in roots.iter().enumerate().skip(1) {
            assert_eq!(
                root.host().control_identity(node.id).unwrap(),
                identities[index]
            );
            let element =
                unsafe { automation.ElementFromHandle(HWND(identities[index] as _)) }.unwrap();
            assert_eq!(
                unsafe { element.CurrentName() }.unwrap().to_string(),
                node.descriptor.accessible_name.as_deref().unwrap()
            );
            let value =
                unsafe { element.GetCurrentPropertyValue(UIA_FullDescriptionPropertyId) }.unwrap();
            let actual = unsafe {
                assert_eq!(
                    value.Anonymous.Anonymous.vt,
                    windows::Win32::System::Variant::VT_BSTR
                );
                value.Anonymous.Anonymous.Anonymous.bstrVal.to_string()
            };
            assert_eq!(actual, description.unwrap_or(""));
        }
    }
    root.dispose();
}

#[test]
fn uia_maps_native_image_dialog_and_popover_roles_to_real_provider_control_types() {
    let mut root =
        NativeRoot::new(WindowsHost::new("Jadix native accessibility roles", [640, 480]).unwrap());
    let identifier = root.model().id();
    let mut dialog = node(
        identifier,
        NodeContent::Dialog {
            open: true,
            close_on_overlay_click: true,
            revision: 0,
        },
        "Settings",
    );
    let anchor = node(
        identifier,
        NodeContent::Button {
            label: "More".into(),
            enabled: true,
        },
        "More",
    );
    let popover = node(
        identifier,
        NodeContent::Popover {
            anchor: anchor.id,
            open: true,
            placement: PopoverPlacement::Bottom,
            gap: 4,
            close_on_outside_click: true,
            revision: 0,
        },
        "Details",
    );
    let image = node(
        identifier,
        NodeContent::Image {
            source: include_str!("../../../examples/native-controls/assets/pixel.txt")
                .trim()
                .into(),
            alt: "Pixel".into(),
        },
        "Picture",
    );
    let image_id = image.id;
    let dialog_id = dialog.id;
    let popover_id = popover.id;
    dialog.children = vec![anchor, image, popover];
    let mut tab_list = node(identifier, NodeContent::Container, "Sections");
    tab_list.descriptor.accessible_role = Some(AccessibleRole::TabList);
    let mut tab_item = node(
        identifier,
        NodeContent::Button {
            label: "Overview".into(),
            enabled: true,
        },
        "Overview",
    );
    tab_item.descriptor.accessible_role = Some(AccessibleRole::TabItem);
    tab_item.descriptor.accessible_selected = Some(true);
    let tab_list_id = tab_list.id;
    let tab_item_id = tab_item.id;
    tab_list.children.push(tab_item);
    let mut tree = node(identifier, NodeContent::Scroll, "Files");
    tree.descriptor.accessible_role = Some(AccessibleRole::Tree);
    let mut tree_item = node(
        identifier,
        NodeContent::Button {
            label: "Folder".into(),
            enabled: true,
        },
        "Folder",
    );
    tree_item.descriptor.accessible_role = Some(AccessibleRole::TreeItem);
    tree_item.descriptor.accessible_selected = Some(false);
    tree_item.descriptor.accessible_expanded = Some(false);
    let tree_id = tree.id;
    let tree_item_id = tree_item.id;
    tree.children.push(tree_item);
    let mut roots = vec![dialog, tab_list, tree];
    root.present(&mut roots, [640.0, 480.0], 0).unwrap();
    root.host().show();
    let automation: IUIAutomation =
        unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }.unwrap();
    for (node, expected, name) in [
        (image_id, UIA_ImageControlTypeId, "Picture"),
        (dialog_id, UIA_PaneControlTypeId, "Settings"),
        (popover_id, UIA_GroupControlTypeId, "Details"),
    ] {
        let element = unsafe {
            automation
                .ElementFromHandle(HWND(root.host().control_identity(node).unwrap() as *mut _))
        }
        .unwrap();
        assert_eq!(
            unsafe { element.CurrentControlType() }.unwrap(),
            expected,
            "{name}"
        );
        assert_eq!(unsafe { element.CurrentName() }.unwrap().to_string(), name);
    }
    let tab_list = unsafe {
        automation.ElementFromHandle(HWND(
            root.host().control_identity(tab_list_id).unwrap() as *mut _
        ))
    }
    .unwrap();
    assert_eq!(
        unsafe { tab_list.CurrentControlType() }.unwrap(),
        UIA_TabControlTypeId
    );
    let tab_item = unsafe {
        automation.ElementFromHandle(HWND(
            root.host().control_identity(tab_item_id).unwrap() as *mut _
        ))
    }
    .unwrap();
    assert_eq!(
        unsafe { tab_item.CurrentControlType() }.unwrap(),
        UIA_TabItemControlTypeId
    );
    let selected = unsafe { tab_item.GetCurrentPattern(UIA_SelectionItemPatternId) }
        .unwrap()
        .cast::<IUIAutomationSelectionItemPattern>()
        .unwrap();
    assert!(unsafe { selected.CurrentIsSelected() }.unwrap().as_bool());
    let tab_window = root.host().control_identity(tab_item_id).unwrap() as _;
    assert_ne!(
        unsafe { GetWindowLongPtrW(tab_window, GWL_STYLE) } as u32 & WS_TABSTOP,
        0
    );
    roots[1].children[0].descriptor.accessible_selected = Some(false);
    root.present(&mut roots, [640.0, 480.0], 1).unwrap();
    assert_eq!(
        unsafe { GetWindowLongPtrW(tab_window, GWL_STYLE) } as u32 & WS_TABSTOP,
        0
    );
    let tab_item = unsafe {
        automation.ElementFromHandle(HWND(
            root.host().control_identity(tab_item_id).unwrap() as *mut _
        ))
    }
    .unwrap();
    let selected = unsafe { tab_item.GetCurrentPattern(UIA_SelectionItemPatternId) }
        .unwrap()
        .cast::<IUIAutomationSelectionItemPattern>()
        .unwrap();
    assert!(!unsafe { selected.CurrentIsSelected() }.unwrap().as_bool());
    roots[1].children[0].descriptor.accessible_role = None;
    roots[1].children[0].descriptor.accessible_selected = None;
    root.present(&mut roots, [640.0, 480.0], 1).unwrap();
    assert_ne!(
        unsafe { GetWindowLongPtrW(tab_window, GWL_STYLE) } as u32 & WS_TABSTOP,
        0
    );
    let tree = unsafe {
        automation.ElementFromHandle(HWND(
            root.host().control_identity(tree_id).unwrap() as *mut _
        ))
    }
    .unwrap();
    assert_eq!(
        unsafe { tree.CurrentControlType() }.unwrap(),
        UIA_TreeControlTypeId
    );
    let tree_item = unsafe {
        automation.ElementFromHandle(HWND(
            root.host().control_identity(tree_item_id).unwrap() as *mut _
        ))
    }
    .unwrap();
    assert_eq!(
        unsafe { tree_item.CurrentControlType() }.unwrap(),
        UIA_TreeItemControlTypeId
    );
    let expansion = unsafe { tree_item.GetCurrentPattern(UIA_ExpandCollapsePatternId) }
        .unwrap()
        .cast::<IUIAutomationExpandCollapsePattern>()
        .unwrap();
    let selection = unsafe { tree_item.GetCurrentPattern(UIA_SelectionItemPatternId) }
        .unwrap()
        .cast::<IUIAutomationSelectionItemPattern>()
        .unwrap();
    assert_eq!(
        unsafe { expansion.CurrentExpandCollapseState() }.unwrap(),
        ExpandCollapseState_Collapsed
    );
    assert!(!unsafe { selection.CurrentIsSelected() }.unwrap().as_bool());
    assert!(unsafe { expansion.Expand() }.is_err());
    assert!(unsafe { selection.Select() }.is_err());
    roots[0].descriptor.content = NodeContent::Dialog {
        open: false,
        close_on_overlay_click: true,
        revision: 1,
    };
    root.present(&mut roots, [640.0, 480.0], 1).unwrap();
    unsafe { expansion.Expand() }.unwrap();
    unsafe { selection.Select() }.unwrap();
    let actions: Vec<_> = root
        .poll_events()
        .unwrap()
        .into_iter()
        .filter_map(|event| {
            if event.node == tree_item_id {
                if let EventPayload::Tree { action } = event.payload {
                    return Some(action);
                }
            }
            None
        })
        .collect();
    assert_eq!(actions, [TreeAction::Expand, TreeAction::Select]);
    let legacy = unsafe { tree_item.GetCurrentPattern(UIA_LegacyIAccessiblePatternId) }
        .unwrap()
        .cast::<IUIAutomationLegacyIAccessiblePattern>()
        .unwrap();
    assert_ne!(
        unsafe { legacy.CurrentState() }.unwrap() & STATE_SYSTEM_COLLAPSED,
        0
    );
    roots[2].children[0].descriptor.accessible_selected = Some(true);
    roots[2].children[0].descriptor.accessible_expanded = Some(true);
    root.present(&mut roots, [640.0, 480.0], 2).unwrap();
    let tree_item = unsafe {
        automation.ElementFromHandle(HWND(
            root.host().control_identity(tree_item_id).unwrap() as *mut _
        ))
    }
    .unwrap();
    let legacy = unsafe { tree_item.GetCurrentPattern(UIA_LegacyIAccessiblePatternId) }
        .unwrap()
        .cast::<IUIAutomationLegacyIAccessiblePattern>()
        .unwrap();
    assert_ne!(
        unsafe { legacy.CurrentState() }.unwrap() & STATE_SYSTEM_EXPANDED,
        0
    );
    let selected = unsafe { tree_item.GetCurrentPattern(UIA_SelectionItemPatternId) }
        .unwrap()
        .cast::<IUIAutomationSelectionItemPattern>()
        .unwrap();
    assert!(unsafe { selected.CurrentIsSelected() }.unwrap().as_bool());
    assert_eq!(
        unsafe { expansion.CurrentExpandCollapseState() }.unwrap(),
        ExpandCollapseState_Expanded
    );
    roots[2].children[0].descriptor.content = NodeContent::Button {
        label: "Folder".into(),
        enabled: false,
    };
    root.present(&mut roots, [640.0, 480.0], 3).unwrap();
    assert!(unsafe { selection.Select() }.is_err());
    assert!(unsafe { expansion.Collapse() }.is_err());
    roots[2].children[0].descriptor.content = NodeContent::Button {
        label: "Folder".into(),
        enabled: true,
    };
    roots[2].children[0].descriptor.accessible_expanded = None;
    root.present(&mut roots, [640.0, 480.0], 4).unwrap();
    assert_eq!(
        unsafe { expansion.CurrentExpandCollapseState() }.unwrap(),
        ExpandCollapseState_LeafNode
    );
    assert!(unsafe { expansion.Expand() }.is_err());
    assert!(unsafe { expansion.Collapse() }.is_err());
    root.present(&mut [], [640.0, 480.0], 0).unwrap();
    assert_eq!(root.host().diagnostics()["controls"], 0);
    assert!(unsafe { selection.Select() }.is_err());
    assert!(unsafe { expansion.Expand() }.is_err());
}
