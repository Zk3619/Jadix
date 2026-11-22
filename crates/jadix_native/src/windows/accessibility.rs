use windows::Win32::Foundation::{HWND, RPC_E_CHANGED_MODE};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    CoUninitialize,
};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::{
    ANNO_THIS, CAccPropServices, CreateStdAccessibleObject, IAccPropServer, IAccPropServer_Impl,
    IAccPropServices, IAccessible, PROPID_ACC_DESCRIPTION, PROPID_ACC_NAME, PROPID_ACC_ROLE,
    PROPID_ACC_STATE,
};
use windows::core::{BOOL, Error, GUID, Interface, PCWSTR, implement};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CHILDID_SELF, OBJID_CLIENT, STATE_SYSTEM_COLLAPSED, STATE_SYSTEM_EXPANDED,
    STATE_SYSTEM_SELECTABLE, STATE_SYSTEM_SELECTED,
};

mod grid;
mod tree;
pub(super) use grid::GridAccessibility;
pub(super) use tree::TreeAccessibility;

pub(super) struct Accessibility {
    properties: Option<IAccPropServices>,
    initialized: bool,
}

impl Accessibility {
    pub(super) fn role(
        &self,
        window: windows_sys::Win32::Foundation::HWND,
        role: Option<i32>,
    ) -> Result<(), String> {
        let properties = self.properties.as_ref().ok_or("accessibility is closed")?;
        unsafe {
            match role {
                Some(role) => properties.SetHwndProp(
                    HWND(window),
                    OBJID_CLIENT as u32,
                    CHILDID_SELF,
                    PROPID_ACC_ROLE,
                    &VARIANT::from(role),
                ),
                None => properties.ClearHwndProps(
                    HWND(window),
                    OBJID_CLIENT as u32,
                    CHILDID_SELF,
                    &[PROPID_ACC_ROLE],
                ),
            }
        }
        .map_err(|error| format!("set native accessible role: {error}"))
    }

    pub(super) fn state(
        &self,
        window: windows_sys::Win32::Foundation::HWND,
        selected: Option<bool>,
        expanded: Option<bool>,
    ) -> Result<(), String> {
        let properties = self.properties.as_ref().ok_or("accessibility is closed")?;
        unsafe {
            match (selected, expanded) {
                (None, None) => properties.ClearHwndProps(
                    HWND(window),
                    OBJID_CLIENT as u32,
                    CHILDID_SELF,
                    &[PROPID_ACC_STATE],
                ),
                (selected, expanded) => {
                    let selectable = selected.map_or(0, |selected| {
                        STATE_SYSTEM_SELECTABLE
                            | if selected { STATE_SYSTEM_SELECTED } else { 0 }
                    });
                    let expansion = expanded.map_or(0, |expanded| {
                        if expanded {
                            STATE_SYSTEM_EXPANDED
                        } else {
                            STATE_SYSTEM_COLLAPSED
                        }
                    });
                    let server: IAccPropServer = SemanticState {
                        window: window as usize,
                        state: selectable | expansion,
                    }.into();
                    properties.SetHwndPropServer(
                        HWND(window),
                        OBJID_CLIENT as u32,
                        CHILDID_SELF,
                        &[PROPID_ACC_STATE],
                        &server,
                        ANNO_THIS,
                    )
                }
            }
        }
        .map_err(|error| {
            format!(
                "set native accessible state (selected={selected:?}, expanded={expanded:?}): {error}"
            )
        })
    }

    pub(super) fn new() -> Result<Self, String> {
        let status = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        if status.is_err() && status != RPC_E_CHANGED_MODE {
            return Err(format!("initialize native accessibility: {status}"));
        }
        let mut instance = Self {
            properties: None,
            initialized: status.is_ok(),
        };
        instance.properties = Some(
            unsafe { CoCreateInstance(&CAccPropServices, None, CLSCTX_INPROC_SERVER) }
                .map_err(|error| format!("create native accessibility properties: {error}"))?,
        );
        Ok(instance)
    }

    pub(super) fn name(
        &self,
        window: windows_sys::Win32::Foundation::HWND,
        name: Option<&str>,
    ) -> Result<(), String> {
        self.text_property(window, PROPID_ACC_NAME, "name", name)
    }

    pub(super) fn description(
        &self,
        window: windows_sys::Win32::Foundation::HWND,
        description: Option<&str>,
    ) -> Result<(), String> {
        self.text_property(window, PROPID_ACC_DESCRIPTION, "description", description)
    }

    fn text_property(
        &self,
        window: windows_sys::Win32::Foundation::HWND,
        property: GUID,
        label: &str,
        value: Option<&str>,
    ) -> Result<(), String> {
        let properties = self.properties.as_ref().ok_or("accessibility is closed")?;
        let window = HWND(window);
        unsafe {
            match value {
                Some(name) => {
                    if name.contains('\0') {
                        return Err(format!("accessible {label} cannot contain NUL"));
                    }
                    let name = super::wide(name);
                    properties.SetHwndPropStr(
                        window,
                        OBJID_CLIENT as u32,
                        CHILDID_SELF,
                        property,
                        PCWSTR(name.as_ptr()),
                    )
                }
                None => properties.ClearHwndProps(
                    window,
                    OBJID_CLIENT as u32,
                    CHILDID_SELF,
                    &[property],
                ),
            }
        }
        .map_err(|error| format!("set native accessible {label}: {error}"))
    }
}

// Query the standard proxy on every request so semantic selection/expansion never
// replaces live focus, pressed, visibility, or disabled states. Unlike
// AccessibleObjectFromWindow, CreateStdAccessibleObject bypasses our annotations.
#[implement(IAccPropServer)]
struct SemanticState {
    window: usize,
    state: u32,
}

impl IAccPropServer_Impl for SemanticState_Impl {
    fn GetPropValue(
        &self,
        _: *const u8,
        _: u32,
        property: &GUID,
        value: *mut VARIANT,
        has_property: *mut BOOL,
    ) -> windows::core::Result<()> {
        if value.is_null() || has_property.is_null() {
            return Err(Error::from_hresult(windows::Win32::Foundation::E_POINTER));
        }
        unsafe {
            has_property.write(BOOL(0));
        }
        if *property != PROPID_ACC_STATE {
            return Ok(());
        }
        let mut standard: Option<IAccessible> = None;
        unsafe {
            CreateStdAccessibleObject(
                HWND(self.window as *mut _),
                OBJID_CLIENT,
                &IAccessible::IID,
                &mut standard as *mut _ as *mut _,
            )?;
            let standard =
                standard.ok_or_else(|| Error::from_hresult(windows::Win32::Foundation::E_FAIL))?;
            let state = standard.get_accState(&VARIANT::from(CHILDID_SELF as i32))?;
            value.write(VARIANT::from(
                state.Anonymous.Anonymous.Anonymous.lVal | self.state as i32,
            ));
            has_property.write(BOOL(1));
        }
        Ok(())
    }
}

impl Drop for Accessibility {
    fn drop(&mut self) {
        self.properties.take();
        if self.initialized {
            unsafe { CoUninitialize() };
        }
    }
}
