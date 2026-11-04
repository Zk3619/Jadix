use std::sync::{Arc, Mutex, Weak};

use windows::Win32::{
    Foundation::HWND,
    System::{
        Com::SAFEARRAY,
        Ole::{SafeArrayCreateVector, SafeArrayDestroy, SafeArrayPutElement},
        Variant::{VARIANT, VT_UNKNOWN},
    },
    UI::Accessibility::*,
};
use windows::core::{BOOL, Error, HRESULT, IUnknown, IUnknownImpl, Interface, Result, implement};
use windows_sys::Win32::{
    Foundation::{HWND as SYS_HWND, LPARAM, LRESULT, WPARAM},
    UI::{
        Input::KeyboardAndMouse::IsWindowEnabled,
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{
            IsWindowVisible, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_APP, WM_GETOBJECT,
            WM_NCDESTROY,
        },
    },
};

use super::super::{RawEvent, WindowContext};
use crate::presentation::{AccessibleRole, NodeContent, NodeDescriptor, TreeAction};

const ACTION: u32 = WM_APP + 0x43a;
const SUBCLASS: usize = 0x4a445854;

fn name(descriptor: &NodeDescriptor) -> String {
    descriptor
        .accessible_name
        .clone()
        .unwrap_or_else(|| match &descriptor.content {
            NodeContent::Button { label, .. } => label.clone(),
            _ => String::new(),
        })
}

#[derive(Clone)]
struct State {
    window: usize,
    container: usize,
    live: bool,
    name: String,
    tree_item: bool,
    tab_item: bool,
    tab_list: bool,
    parent: Option<Container>,
    items: Vec<Weak<Mutex<State>>>,
    selected: Option<bool>,
    expanded: Option<bool>,
}

#[implement(
    IRawElementProviderSimple,
    ISelectionItemProvider,
    IExpandCollapseProvider,
    IInvokeProvider,
    ISelectionProvider
)]
struct Provider {
    state: Arc<Mutex<State>>,
}

impl Provider {
    fn snapshot(&self) -> Result<State> {
        let state = self
            .state
            .lock()
            .map_err(|_| Error::from_hresult(HRESULT(UIA_E_ELEMENTNOTAVAILABLE as i32)))?;
        if !state.live {
            return Err(Error::from_hresult(HRESULT(
                UIA_E_ELEMENTNOTAVAILABLE as i32,
            )));
        }
        Ok(state.clone())
    }

    fn action(&self, action: TreeAction) -> Result<()> {
        self.action_code(match action {
            TreeAction::Select => 1,
            TreeAction::AddToSelection => 2,
            TreeAction::RemoveFromSelection => 3,
            TreeAction::Expand => 4,
            TreeAction::Collapse => 5,
        })
    }

    fn action_code(&self, code: isize) -> Result<()> {
        let state = self.snapshot()?;
        let mut response = 0;
        let sent = unsafe {
            SendMessageTimeoutW(
                state.window as SYS_HWND,
                ACTION,
                Arc::as_ptr(&self.state) as usize,
                code,
                SMTO_ABORTIFHUNG,
                1000,
                &mut response,
            )
        };
        match (sent, response as isize) {
            (0, _) => Err(Error::from_hresult(HRESULT(UIA_E_TIMEOUT as i32))),
            (_, 1) => Ok(()),
            (_, -1) => Err(Error::from_hresult(HRESULT(UIA_E_ELEMENTNOTENABLED as i32))),
            (_, -2) => Err(Error::from_hresult(HRESULT(UIA_E_INVALIDOPERATION as i32))),
            _ => Err(Error::from_hresult(HRESULT(
                UIA_E_ELEMENTNOTAVAILABLE as i32,
            ))),
        }
    }
}

impl IRawElementProviderSimple_Impl for Provider_Impl {
    fn ProviderOptions(&self) -> Result<ProviderOptions> {
        Ok(ProviderOptions_ServerSideProvider | ProviderOptions_UseComThreading)
    }

    fn GetPatternProvider(&self, patternid: UIA_PATTERN_ID) -> Result<IUnknown> {
        let state = self.snapshot()?;
        if patternid == UIA_ExpandCollapsePatternId && (state.tree_item || state.expanded.is_some())
        {
            return self.to_interface::<IExpandCollapseProvider>().cast();
        }
        if patternid == UIA_SelectionItemPatternId
            && (state.tree_item || state.tab_item)
            && state.selected.is_some()
        {
            return self.to_interface::<ISelectionItemProvider>().cast();
        }
        if patternid == UIA_InvokePatternId && !state.tree_item && !state.tab_list {
            return self.to_interface::<IInvokeProvider>().cast();
        }
        if patternid == UIA_SelectionPatternId && state.tab_list {
            return self.to_interface::<ISelectionProvider>().cast();
        }
        Err(Error::empty())
    }

    fn GetPropertyValue(&self, propertyid: UIA_PROPERTY_ID) -> Result<VARIANT> {
        let state = self.snapshot()?;
        Ok(if propertyid == UIA_ControlTypePropertyId {
            VARIANT::from(if state.tree_item {
                UIA_TreeItemControlTypeId.0
            } else if state.tab_item {
                UIA_TabItemControlTypeId.0
            } else if state.tab_list {
                UIA_TabControlTypeId.0
            } else {
                UIA_ButtonControlTypeId.0
            })
        } else if propertyid == UIA_NamePropertyId {
            VARIANT::from(state.name.as_str())
        } else if propertyid == UIA_IsControlElementPropertyId
            || propertyid == UIA_IsContentElementPropertyId
        {
            VARIANT::from(true)
        } else {
            VARIANT::default()
        })
    }

    fn HostRawElementProvider(&self) -> Result<IRawElementProviderSimple> {
        let state = self.snapshot()?;
        unsafe { UiaHostProviderFromHwnd(HWND(state.window as _)) }
    }
}

impl IInvokeProvider_Impl for Provider_Impl {
    fn Invoke(&self) -> Result<()> {
        self.action_code(6)
    }
}

impl ISelectionItemProvider_Impl for Provider_Impl {
    fn Select(&self) -> Result<()> {
        self.action(TreeAction::Select)
    }
    fn AddToSelection(&self) -> Result<()> {
        self.action(TreeAction::AddToSelection)
    }
    fn RemoveFromSelection(&self) -> Result<()> {
        self.action(TreeAction::RemoveFromSelection)
    }
    fn IsSelected(&self) -> Result<BOOL> {
        Ok(self.snapshot()?.selected.unwrap_or(false).into())
    }
    fn SelectionContainer(&self) -> Result<IRawElementProviderSimple> {
        let state = self.snapshot()?;
        if state.container == 0 {
            return Err(Error::empty());
        }
        if let Some(parent) = state.parent {
            let provider = Provider { state: parent.0 };
            provider.snapshot()?;
            Ok(provider.into())
        } else {
            unsafe { UiaHostProviderFromHwnd(HWND(state.container as _)) }
        }
    }
}

impl IExpandCollapseProvider_Impl for Provider_Impl {
    fn Expand(&self) -> Result<()> {
        self.action(TreeAction::Expand)
    }
    fn Collapse(&self) -> Result<()> {
        self.action(TreeAction::Collapse)
    }
    fn ExpandCollapseState(&self) -> Result<ExpandCollapseState> {
        Ok(match self.snapshot()?.expanded {
            Some(true) => ExpandCollapseState_Expanded,
            Some(false) => ExpandCollapseState_Collapsed,
            None => ExpandCollapseState_LeafNode,
        })
    }
}

impl ISelectionProvider_Impl for Provider_Impl {
    fn GetSelection(&self) -> Result<*mut SAFEARRAY> {
        let state = self.snapshot()?;
        if !state.tab_list {
            return Err(Error::from_hresult(HRESULT(UIA_E_INVALIDOPERATION as i32)));
        }
        let mut selected = Vec::<IRawElementProviderSimple>::new();
        for item in state.items.iter().filter_map(Weak::upgrade) {
            let current = item
                .lock()
                .map_err(|_| Error::from_hresult(HRESULT(UIA_E_ELEMENTNOTAVAILABLE as i32)))?;
            if current.live
                && current.tab_item
                && current.selected == Some(true)
                && current
                    .parent
                    .as_ref()
                    .is_some_and(|parent| Arc::ptr_eq(&parent.0, &self.state))
            {
                selected.push(
                    Provider {
                        state: item.clone(),
                    }
                    .into(),
                );
            }
        }
        if selected.len() > 1 {
            return Err(Error::from_hresult(HRESULT(UIA_E_INVALIDOPERATION as i32)));
        }
        let array = unsafe { SafeArrayCreateVector(VT_UNKNOWN, 0, selected.len() as u32) };
        if array.is_null() {
            return Err(Error::from_hresult(HRESULT(0x8007000eu32 as i32)));
        }
        for (index, item) in selected.iter().enumerate() {
            if let Err(error) =
                unsafe { SafeArrayPutElement(array, &(index as i32), item.as_raw()) }
            {
                unsafe {
                    let _ = SafeArrayDestroy(array);
                }
                return Err(error);
            }
        }
        Ok(array)
    }

    fn CanSelectMultiple(&self) -> Result<BOOL> {
        self.snapshot()?;
        Ok(false.into())
    }

    fn IsSelectionRequired(&self) -> Result<BOOL> {
        self.snapshot()?;
        Ok(true.into())
    }
}

#[derive(Clone)]
pub(in super::super) struct Container(Arc<Mutex<State>>);

pub(in super::super) struct TreeAccessibility {
    binding: Box<Binding>,
}

struct Binding {
    state: Arc<Mutex<State>>,
    provider: IRawElementProviderSimple,
    context: *const WindowContext,
}

impl TreeAccessibility {
    pub(in super::super) fn new(
        window: SYS_HWND,
        container: usize,
        context: &WindowContext,
        descriptor: &NodeDescriptor,
        parent: Option<Container>,
    ) -> std::result::Result<Self, String> {
        let state = Arc::new(Mutex::new(State {
            window: window as usize,
            container,
            live: true,
            name: name(descriptor),
            tree_item: descriptor.accessible_role == Some(AccessibleRole::TreeItem),
            tab_item: descriptor.accessible_role == Some(AccessibleRole::TabItem),
            tab_list: descriptor.accessible_role == Some(AccessibleRole::TabList),
            parent: None,
            items: Vec::new(),
            selected: descriptor.accessible_selected,
            expanded: descriptor.accessible_expanded,
        }));
        let provider: IRawElementProviderSimple = Provider {
            state: state.clone(),
        }
        .into();
        let mut binding = Box::new(Binding {
            state,
            provider,
            context,
        });
        if unsafe {
            SetWindowSubclass(
                window,
                Some(tree_proc),
                SUBCLASS,
                (&mut *binding as *mut Binding) as usize,
            )
        } == 0
        {
            return Err(super::super::native_error(
                "subclass native tree accessibility",
            ));
        }
        let result = Self { binding };
        result.link(parent);
        Ok(result)
    }

    pub(in super::super) fn container(&self) -> Container {
        Container(self.binding.state.clone())
    }

    fn link(&self, parent: Option<Container>) {
        let previous = self.binding.state.lock().unwrap().parent.clone();
        if previous
            .as_ref()
            .zip(parent.as_ref())
            .is_some_and(|(previous, next)| Arc::ptr_eq(&previous.0, &next.0))
        {
            return;
        }
        self.binding.state.lock().unwrap().parent = parent.clone();
        if let Some(parent) = parent {
            let mut state = parent.0.lock().unwrap();
            state.items.retain(|item| {
                item.upgrade().is_some_and(|item| {
                    let child = item.lock().unwrap();
                    child.live
                        && child
                            .parent
                            .as_ref()
                            .is_some_and(|owner| Arc::ptr_eq(&owner.0, &parent.0))
                        && !Arc::ptr_eq(&item, &self.binding.state)
                })
            });
            state.items.push(Arc::downgrade(&self.binding.state));
        }
    }

    pub(in super::super) fn update(
        &self,
        descriptor: &NodeDescriptor,
        container: usize,
        parent: Option<Container>,
    ) {
        self.link(parent);
        let (selected, expanded) = {
            let mut state = self.binding.state.lock().unwrap();
            let previous = (state.selected, state.expanded);
            state.selected = descriptor.accessible_selected;
            state.expanded = descriptor.accessible_expanded;
            state.name = name(descriptor);
            state.tree_item = descriptor.accessible_role == Some(AccessibleRole::TreeItem);
            state.tab_item = descriptor.accessible_role == Some(AccessibleRole::TabItem);
            state.tab_list = descriptor.accessible_role == Some(AccessibleRole::TabList);
            state.container = container;
            previous
        };
        if selected != descriptor.accessible_selected {
            unsafe {
                let _ = UiaRaiseAutomationPropertyChangedEvent(
                    &self.binding.provider,
                    UIA_SelectionItemIsSelectedPropertyId,
                    &VARIANT::from(selected.unwrap_or(false)),
                    &VARIANT::from(descriptor.accessible_selected.unwrap_or(false)),
                );
            }
        }
        if expanded != descriptor.accessible_expanded {
            let value = |expanded| match expanded {
                Some(true) => ExpandCollapseState_Expanded.0,
                Some(false) => ExpandCollapseState_Collapsed.0,
                None => ExpandCollapseState_LeafNode.0,
            };
            unsafe {
                let _ = UiaRaiseAutomationPropertyChangedEvent(
                    &self.binding.provider,
                    UIA_ExpandCollapseExpandCollapseStatePropertyId,
                    &VARIANT::from(value(expanded)),
                    &VARIANT::from(value(descriptor.accessible_expanded)),
                );
            }
        }
    }
}

impl Drop for TreeAccessibility {
    fn drop(&mut self) {
        let (window, live) = {
            let mut state = self.binding.state.lock().unwrap();
            let live = state.live;
            state.live = false;
            (state.window, live)
        };
        unsafe {
            if live {
                RemoveWindowSubclass(window as SYS_HWND, Some(tree_proc), SUBCLASS);
            }
            let _ = UiaDisconnectProvider(&self.binding.provider);
        }
    }
}

unsafe extern "system" fn tree_proc(
    window: SYS_HWND,
    message: u32,
    parameter: WPARAM,
    data: LPARAM,
    _: usize,
    reference: usize,
) -> LRESULT {
    let binding = unsafe { &*(reference as *const Binding) };
    if message == WM_GETOBJECT && data == UiaRootObjectId as isize {
        return unsafe {
            UiaReturnRawElementProvider(
                HWND(window),
                windows::Win32::Foundation::WPARAM(parameter),
                windows::Win32::Foundation::LPARAM(data),
                &binding.provider,
            )
        }
        .0;
    }
    if message == ACTION {
        if parameter != Arc::as_ptr(&binding.state) as usize {
            return -3;
        }
        let context = unsafe { &*binding.context };
        let Some(control) = context.target(window) else {
            return -3;
        };
        if unsafe { IsWindowEnabled(window) == 0 || IsWindowVisible(window) == 0 } {
            return -1;
        }
        if context.closed.get() || context.suppress.get() || context.failed.get() {
            return -3;
        }
        if data == 6 {
            let state = binding.state.lock().unwrap();
            if !state.live {
                return -3;
            }
            if state.tree_item || state.tab_list {
                return -2;
            }
            drop(state);
            context.push(RawEvent::Activate {
                control,
                checked: false,
            });
            return if context.failed.get() { -3 } else { 1 };
        }
        {
            let state = binding.state.lock().unwrap();
            if state.tab_item {
                if !state.live {
                    return -3;
                }
                let Some(selected) = state.selected else {
                    return -2;
                };
                match data {
                    1 => {
                        drop(state);
                        context.push(RawEvent::Activate {
                            control,
                            checked: false,
                        });
                        return if context.failed.get() { -3 } else { 1 };
                    }
                    2 if selected => return 1,
                    3 if !selected => return 1,
                    _ => return -2,
                }
            }
        }
        let action = match data {
            1 => TreeAction::Select,
            2 => TreeAction::AddToSelection,
            3 => TreeAction::RemoveFromSelection,
            4 => TreeAction::Expand,
            5 => TreeAction::Collapse,
            _ => return -2,
        };
        {
            let state = binding.state.lock().unwrap();
            if !state.live {
                return -3;
            }
            if matches!(action, TreeAction::Expand | TreeAction::Collapse) {
                if state.expanded.is_none() {
                    return -2;
                }
            } else if !state.tree_item || state.selected.is_none() {
                return -2;
            }
        }
        context.push(RawEvent::Tree { control, action });
        return if context.failed.get() { -3 } else { 1 };
    }
    if message == WM_NCDESTROY {
        binding.state.lock().unwrap().live = false;
        unsafe {
            RemoveWindowSubclass(window, Some(tree_proc), SUBCLASS);
        }
    }
    unsafe { DefSubclassProc(window, message, parameter, data) }
}
