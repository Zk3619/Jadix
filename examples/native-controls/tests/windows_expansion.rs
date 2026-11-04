#![cfg(all(windows, feature = "windows", not(feature = "gpu")))]

use jadix_components::{authoring::native::ElementProjection, prelude::*};
use jadix_native::{host::NativeRoot, presentation::*, windows::WindowsHost};
use std::sync::Mutex;
use windows::core::Interface;
use windows::Win32::{
    Foundation::HWND,
    System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER},
    UI::Accessibility::*,
};

static UIA_TESTS: Mutex<()> = Mutex::new(());

fn flatten(node: &PresentationNode) -> Vec<&PresentationNode> {
    std::iter::once(node)
        .chain(node.children.iter().flat_map(flatten))
        .collect()
}

fn render(
    root: &mut NativeRoot<WindowsHost>,
    projection: &ElementProjection,
    element: &mut Element,
) -> PresentationNode {
    let viewport = root.host().viewport();
    element.descriptor.style.width = Some(viewport[0]);
    element.descriptor.style.height = Some(viewport[1]);
    let mut tree = projection
        .snapshot_with_focus(element, root.model().focused())
        .unwrap();
    root.present(
        std::slice::from_mut(&mut tree),
        viewport,
        root.host().environment(),
    )
    .unwrap();
    if let Some(target) = projection.take_focus_request(element) {
        root.focus(Some(target)).unwrap();
    }
    tree
}

fn dispatch(
    root: &mut NativeRoot<WindowsHost>,
    projection: &ElementProjection,
    element: &mut Element,
) {
    root.host().pump();
    for event in root.poll_events().unwrap() {
        if !matches!(event.payload, EventPayload::Focus) {
            projection.dispatch(element, &event).unwrap();
        }
    }
}

fn expansion(automation: &IUIAutomation, window: usize) -> IUIAutomationExpandCollapsePattern {
    let control = unsafe { automation.ElementFromHandle(HWND(window as *mut _)) }.unwrap();
    assert_eq!(
        unsafe { control.CurrentControlType() }.unwrap(),
        UIA_ButtonControlTypeId
    );
    assert!(unsafe { control.GetCurrentPattern(UIA_SelectionItemPatternId) }.is_err());
    assert!(unsafe { control.GetCurrentPattern(UIA_LegacyIAccessiblePatternId) }.is_ok());
    assert!(unsafe { control.GetCurrentPattern(UIA_InvokePatternId) }.is_ok());
    unsafe { control.GetCurrentPattern(UIA_ExpandCollapsePatternId) }
        .unwrap()
        .cast()
        .unwrap()
}

#[test]
fn native_uia_fold_requests_are_idempotent_and_restore_focus_across_unmount() {
    let _guard = UIA_TESTS.lock().unwrap();
    let (_owner, (mut element, open, selected)) = create_root(|| {
        let open = create_signal(false);
        let selected = create_signal(Vec::<String>::new());
        let draft = create_signal(String::from("Draft 中文😀"));
        let element = Element::new()
            .flex_col()
            .child(
                collapsible("details")
                    .open(open)
                    .trigger(|| Element::new().text("Details"))
                    .content(move || input("draft").value_signal(draft).into()),
            )
            .child(
                accordion("faq")
                    .value_signal(selected)
                    .item("account", "Account", || {
                        Element::new().text("Account content")
                    })
                    .item("privacy", "Privacy", || {
                        Element::new().text("Privacy content")
                    })
                    .item_disabled("blocked", "Blocked", || Element::new().text("Hidden")),
            );
        (element, open, selected)
    });
    let mut root = NativeRoot::new(WindowsHost::new("Jadix UIA expansion", [640, 680]).unwrap());
    let projection = ElementProjection::with_lifetime(root.model().lifetime());
    let initial = render(&mut root, &projection, &mut element);
    root.host().show();
    let buttons = |label: &str| {
        flatten(&initial).into_iter().find(|node| matches!(&node.descriptor.content, NodeContent::Button { label: text, .. } if text == label)).unwrap().id
    };
    let details = buttons("Details");
    let identity = root.host().control_identity(details).unwrap();
    let automation: IUIAutomation =
        unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }.unwrap();
    let folder = expansion(&automation, identity);
    let account = expansion(
        &automation,
        root.host().control_identity(buttons("Account")).unwrap(),
    );
    let privacy = expansion(
        &automation,
        root.host().control_identity(buttons("Privacy")).unwrap(),
    );
    let blocked = expansion(
        &automation,
        root.host().control_identity(buttons("Blocked")).unwrap(),
    );
    assert!(unsafe { blocked.Expand() }.is_err());
    assert_eq!(
        unsafe { folder.CurrentExpandCollapseState() }.unwrap(),
        ExpandCollapseState_Collapsed
    );
    unsafe { folder.Expand() }.unwrap();
    unsafe { folder.Expand() }.unwrap();
    dispatch(&mut root, &projection, &mut element);
    assert!(open.get());
    let opened = render(&mut root, &projection, &mut element);
    let editor = flatten(&opened)
        .into_iter()
        .find(|node| matches!(node.descriptor.content, NodeContent::Input { .. }))
        .unwrap()
        .id;
    root.focus(Some(editor)).unwrap();
    dispatch(&mut root, &projection, &mut element);
    assert_eq!(
        unsafe { folder.CurrentExpandCollapseState() }.unwrap(),
        ExpandCollapseState_Expanded
    );
    unsafe { folder.Collapse() }.unwrap();
    unsafe { folder.Collapse() }.unwrap();
    dispatch(&mut root, &projection, &mut element);
    assert!(!open.get());
    render(&mut root, &projection, &mut element);
    assert_eq!(root.model().focused(), Some(details));
    assert!(root.host().control_identity(editor).is_none());
    assert_eq!(root.host().control_identity(details), Some(identity));
    unsafe { account.Expand() }.unwrap();
    unsafe { account.Expand() }.unwrap();
    dispatch(&mut root, &projection, &mut element);
    assert_eq!(selected.get(), ["account"]);
    render(&mut root, &projection, &mut element);
    unsafe { privacy.Expand() }.unwrap();
    dispatch(&mut root, &projection, &mut element);
    assert_eq!(selected.get(), ["privacy"]);
    render(&mut root, &projection, &mut element);
    assert_eq!(
        unsafe { account.CurrentExpandCollapseState() }.unwrap(),
        ExpandCollapseState_Collapsed
    );
    unsafe { privacy.Collapse() }.unwrap();
    unsafe { privacy.Collapse() }.unwrap();
    dispatch(&mut root, &projection, &mut element);
    assert!(selected.get().is_empty());
    let button = unsafe { automation.ElementFromHandle(HWND(identity as *mut _)) }.unwrap();
    let invoke: IUIAutomationInvokePattern =
        unsafe { button.GetCurrentPattern(UIA_InvokePatternId) }
            .unwrap()
            .cast()
            .unwrap();
    unsafe { invoke.Invoke() }.unwrap();
    dispatch(&mut root, &projection, &mut element);
    assert!(open.get());
    root.present(&mut [], [640.0, 680.0], 0).unwrap();
    assert!(unsafe { folder.Expand() }.is_err());
    assert!(unsafe { account.Collapse() }.is_err());
    assert!(unsafe { invoke.Invoke() }.is_err());
}

#[test]
fn native_uia_tab_selection_changes_panels_and_enforces_single_selection() {
    let _guard = UIA_TESTS.lock().unwrap();
    let (_owner, (mut element, selected)) = create_root(|| {
        let selected = create_signal(String::from("account"));
        let draft = create_signal(String::from("Draft 中文😀"));
        let element = ui! {
            <Tabs id="settings" value={selected}>
                <TabsList>
                    <TabsTrigger value="account">"Account"</TabsTrigger>
                    <TabsTrigger value="blocked" disabled={true}>"Blocked"</TabsTrigger>
                    <TabsTrigger value="privacy">"Privacy"</TabsTrigger>
                </TabsList>
                <TabsContent value="account"><Input id="draft" value={draft} /></TabsContent>
                <TabsContent value="privacy">"Privacy panel"</TabsContent>
            </Tabs>
        };
        (element, selected)
    });
    let mut root = NativeRoot::new(WindowsHost::new("Jadix UIA tabs", [640, 480]).unwrap());
    let projection = ElementProjection::with_lifetime(root.model().lifetime());
    let initial = render(&mut root, &projection, &mut element);
    root.host().show();
    let tab = |label: &str| {
        flatten(&initial).into_iter().find(|node| matches!(&node.descriptor.content, NodeContent::Button { label: text, .. } if text == label)).unwrap().id
    };
    let account_id = tab("Account");
    let privacy_id = tab("Privacy");
    let editor = flatten(&initial)
        .into_iter()
        .find(|node| matches!(node.descriptor.content, NodeContent::Input { .. }))
        .unwrap()
        .id;
    let identity = root.host().control_identity(privacy_id).unwrap();
    let automation: IUIAutomation =
        unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }.unwrap();
    let pattern = |node| {
        let item = unsafe {
            automation
                .ElementFromHandle(HWND(root.host().control_identity(node).unwrap() as *mut _))
        }
        .unwrap_or_else(|error| {
            panic!(
                "UIA item {}: {error}; {:?}",
                node.token(),
                root.host().inspect_control(node)
            )
        });
        assert_eq!(
            unsafe { item.CurrentControlType() }.unwrap(),
            UIA_TabItemControlTypeId
        );
        assert!(unsafe { item.GetCurrentPattern(UIA_ExpandCollapsePatternId) }.is_err());
        unsafe { item.GetCurrentPattern(UIA_SelectionItemPatternId) }
            .unwrap()
            .cast::<IUIAutomationSelectionItemPattern>()
            .unwrap()
    };
    let account = pattern(account_id);
    let privacy = pattern(privacy_id);
    let blocked = pattern(tab("Blocked"));
    assert!(unsafe { account.CurrentIsSelected() }.unwrap().as_bool());
    assert!(!unsafe { privacy.CurrentIsSelected() }.unwrap().as_bool());
    let container = unsafe { privacy.CurrentSelectionContainer() }.unwrap();
    assert_eq!(
        unsafe { container.CurrentControlType() }.unwrap(),
        UIA_TabControlTypeId
    );
    let selection = unsafe { container.GetCurrentPattern(UIA_SelectionPatternId) }
        .unwrap()
        .cast::<IUIAutomationSelectionPattern>()
        .unwrap();
    assert!(!unsafe { selection.CurrentCanSelectMultiple() }
        .unwrap()
        .as_bool());
    assert!(unsafe { selection.CurrentIsSelectionRequired() }
        .unwrap()
        .as_bool());
    let selected_name = || {
        let items = unsafe { selection.GetCurrentSelection() }.unwrap();
        assert_eq!(unsafe { items.Length() }.unwrap(), 1);
        let item = unsafe { items.GetElement(0) }.unwrap();
        assert_eq!(
            unsafe { item.CurrentControlType() }.unwrap(),
            UIA_TabItemControlTypeId
        );
        unsafe { item.CurrentName() }.unwrap().to_string()
    };
    assert_eq!(selected_name(), "Account");
    assert!(unsafe { account.RemoveFromSelection() }.is_err());
    assert!(unsafe { privacy.AddToSelection() }.is_err());
    unsafe { account.AddToSelection() }.unwrap();
    unsafe { privacy.RemoveFromSelection() }.unwrap();
    assert!(unsafe { blocked.Select() }.is_err());
    unsafe { privacy.Select() }.unwrap();
    unsafe { privacy.Select() }.unwrap();
    dispatch(&mut root, &projection, &mut element);
    assert_eq!(selected.get(), "privacy");
    render(&mut root, &projection, &mut element);
    assert!(root.host().control_identity(editor).is_none());
    assert_eq!(root.host().control_identity(privacy_id), Some(identity));
    assert!(!unsafe { account.CurrentIsSelected() }.unwrap().as_bool());
    assert!(unsafe { privacy.CurrentIsSelected() }.unwrap().as_bool());
    assert_eq!(selected_name(), "Privacy");
    unsafe { account.Select() }.unwrap();
    dispatch(&mut root, &projection, &mut element);
    assert_eq!(selected.get(), "account");
    render(&mut root, &projection, &mut element);
    root.present(&mut [], [640.0, 480.0], 0).unwrap();
    assert!(unsafe { account.Select() }.is_err());
    assert!(unsafe { privacy.RemoveFromSelection() }.is_err());
    if let Ok(items) = unsafe { selection.GetCurrentSelection() } {
        assert_eq!(unsafe { items.Length() }.unwrap(), 0);
    }
}
