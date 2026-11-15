use std::sync::{Arc, Mutex, Weak};

use windows::core::{implement, Error, IUnknown, IUnknownImpl, Interface, Result, HRESULT};
use windows::Win32::{
    Foundation::HWND,
    System::{
        Com::SAFEARRAY,
        Ole::{SafeArrayCreateVector, SafeArrayDestroy, SafeArrayPutElement},
        Variant::{VARIANT, VT_UNKNOWN},
    },
    UI::Accessibility::*,
};
use windows_sys::Win32::{
    Foundation::{HWND as SYS_HWND, LPARAM, LRESULT, WPARAM},
    UI::{
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{WM_GETOBJECT, WM_NCDESTROY},
    },
};

use crate::presentation::{AccessibleRole, NodeDescriptor};

const SUBCLASS: usize = 0x4a445847;

fn unavailable() -> Error {
    Error::from_hresult(HRESULT(UIA_E_ELEMENTNOTAVAILABLE as i32))
}

struct State {
    window: usize,
    live: bool,
    name: String,
    description: String,
    role: AccessibleRole,
    parent: Option<Arc<Mutex<State>>>,
    items: Vec<Weak<Mutex<State>>>,
}

#[implement(
    IRawElementProviderSimple,
    IGridProvider,
    IGridItemProvider,
    ITableProvider,
    ITableItemProvider
)]
struct Provider {
    state: Arc<Mutex<State>>,
}

impl Provider {
    fn state(&self) -> Result<std::sync::MutexGuard<'_, State>> {
        let state = self.state.lock().map_err(|_| unavailable())?;
        if !state.live {
            return Err(unavailable());
        }
        Ok(state)
    }

    fn headers(&self, matching_column: Option<u32>) -> Result<*mut SAFEARRAY> {
        let state = self.state()?;
        if !matches!(state.role, AccessibleRole::Grid { .. }) {
            return Err(unavailable());
        }
        let mut headers = Vec::new();
        for item in state.items.iter().filter_map(Weak::upgrade) {
            let child = item.lock().map_err(|_| unavailable())?;
            if let AccessibleRole::GridColumnHeader { column } = child.role {
                if child.live
                    && child
                        .parent
                        .as_ref()
                        .is_some_and(|parent| Arc::ptr_eq(parent, &self.state))
                    && matching_column.is_none_or(|expected| column == expected)
                {
                    headers.push((column, item.clone()));
                }
            }
        }
        headers.sort_by_key(|(column, _)| *column);
        let providers: Vec<IRawElementProviderSimple> = headers
            .into_iter()
            .map(|(_, state)| Provider { state }.into())
            .collect();
        provider_array(&providers)
    }

    fn table_owner(&self) -> Result<Arc<Mutex<State>>> {
        let parent = {
            let state = self.state()?;
            if !matches!(state.role, AccessibleRole::GridCell { .. }) {
                return Err(unavailable());
            }
            state.parent.clone().ok_or_else(unavailable)?
        };
        let owner = parent.lock().map_err(|_| unavailable())?;
        if !owner.live || !matches!(owner.role, AccessibleRole::Grid { .. }) {
            return Err(unavailable());
        }
        drop(owner);
        Ok(parent)
    }
}

fn provider_array(providers: &[IRawElementProviderSimple]) -> Result<*mut SAFEARRAY> {
    let array = unsafe { SafeArrayCreateVector(VT_UNKNOWN, 0, providers.len() as u32) };
    if array.is_null() {
        return Err(Error::from_hresult(HRESULT(0x8007000eu32 as i32)));
    }
    for (index, provider) in providers.iter().enumerate() {
        if let Err(error) =
            unsafe { SafeArrayPutElement(array, &(index as i32), provider.as_raw()) }
        {
            unsafe {
                let _ = SafeArrayDestroy(array);
            }
            return Err(error);
        }
    }
    Ok(array)
}

impl IRawElementProviderSimple_Impl for Provider_Impl {
    fn ProviderOptions(&self) -> Result<ProviderOptions> {
        Ok(ProviderOptions_ServerSideProvider | ProviderOptions_UseComThreading)
    }

    fn GetPatternProvider(&self, patternid: UIA_PATTERN_ID) -> Result<IUnknown> {
        match self.state()?.role {
            AccessibleRole::Grid { .. } if patternid == UIA_GridPatternId => {
                self.to_interface::<IGridProvider>().cast()
            }
            AccessibleRole::Grid { .. } if patternid == UIA_TablePatternId => {
                self.to_interface::<ITableProvider>().cast()
            }
            AccessibleRole::GridCell { .. } if patternid == UIA_GridItemPatternId => {
                self.to_interface::<IGridItemProvider>().cast()
            }
            AccessibleRole::GridCell { .. } if patternid == UIA_TableItemPatternId => {
                self.to_interface::<ITableItemProvider>().cast()
            }
            _ => Err(Error::empty()),
        }
    }

    fn GetPropertyValue(&self, propertyid: UIA_PROPERTY_ID) -> Result<VARIANT> {
        let state = self.state()?;
        Ok(if propertyid == UIA_ControlTypePropertyId {
            VARIANT::from(match state.role {
                AccessibleRole::Grid { .. } => UIA_DataGridControlTypeId.0,
                AccessibleRole::GridColumnHeader { .. } => UIA_HeaderItemControlTypeId.0,
                _ => UIA_DataItemControlTypeId.0,
            })
        } else if propertyid == UIA_NamePropertyId {
            VARIANT::from(state.name.as_str())
        } else if propertyid == UIA_FullDescriptionPropertyId {
            VARIANT::from(state.description.as_str())
        } else if propertyid == UIA_IsControlElementPropertyId
            || propertyid == UIA_IsContentElementPropertyId
        {
            VARIANT::from(true)
        } else {
            VARIANT::default()
        })
    }

    fn HostRawElementProvider(&self) -> Result<IRawElementProviderSimple> {
        unsafe { UiaHostProviderFromHwnd(HWND(self.state()?.window as _)) }
    }
}

impl IGridProvider_Impl for Provider_Impl {
    fn GetItem(&self, row: i32, column: i32) -> Result<IRawElementProviderSimple> {
        let state = self.state()?;
        let AccessibleRole::Grid { rows, columns } = state.role else {
            return Err(unavailable());
        };
        if row < 0 || column < 0 || row as u32 >= rows || column as u32 >= columns {
            return Err(Error::from_hresult(HRESULT(0x80070057u32 as i32)));
        }
        for item in state.items.iter().filter_map(Weak::upgrade) {
            let matches = {
                let child = item.lock().map_err(|_| unavailable())?;
                child.live
                    && child
                        .parent
                        .as_ref()
                        .is_some_and(|parent| Arc::ptr_eq(parent, &self.state))
                    && child.role
                        == AccessibleRole::GridCell {
                            row: row as u32,
                            column: column as u32,
                        }
            };
            if matches {
                return Ok(Provider { state: item }.into());
            }
        }
        Err(unavailable())
    }

    fn RowCount(&self) -> Result<i32> {
        match self.state()?.role {
            AccessibleRole::Grid { rows, .. } => Ok(rows as i32),
            _ => Err(unavailable()),
        }
    }

    fn ColumnCount(&self) -> Result<i32> {
        match self.state()?.role {
            AccessibleRole::Grid { columns, .. } => Ok(columns as i32),
            _ => Err(unavailable()),
        }
    }
}

impl IGridItemProvider_Impl for Provider_Impl {
    fn Row(&self) -> Result<i32> {
        match self.state()?.role {
            AccessibleRole::GridCell { row, .. } => Ok(row as i32),
            _ => Err(unavailable()),
        }
    }

    fn Column(&self) -> Result<i32> {
        match self.state()?.role {
            AccessibleRole::GridCell { column, .. } => Ok(column as i32),
            _ => Err(unavailable()),
        }
    }

    fn RowSpan(&self) -> Result<i32> {
        match self.state()?.role {
            AccessibleRole::GridCell { .. } => Ok(1),
            _ => Err(unavailable()),
        }
    }

    fn ColumnSpan(&self) -> Result<i32> {
        match self.state()?.role {
            AccessibleRole::GridCell { .. } => Ok(1),
            _ => Err(unavailable()),
        }
    }

    fn ContainingGrid(&self) -> Result<IRawElementProviderSimple> {
        Ok(Provider {
            state: self.table_owner()?,
        }
        .into())
    }
}

impl ITableProvider_Impl for Provider_Impl {
    fn GetRowHeaders(&self) -> Result<*mut SAFEARRAY> {
        if !matches!(self.state()?.role, AccessibleRole::Grid { .. }) {
            return Err(unavailable());
        }
        provider_array(&[])
    }

    fn GetColumnHeaders(&self) -> Result<*mut SAFEARRAY> {
        self.headers(None)
    }

    fn RowOrColumnMajor(&self) -> Result<RowOrColumnMajor> {
        if !matches!(self.state()?.role, AccessibleRole::Grid { .. }) {
            return Err(unavailable());
        }
        Ok(RowOrColumnMajor_RowMajor)
    }
}

impl ITableItemProvider_Impl for Provider_Impl {
    fn GetRowHeaderItems(&self) -> Result<*mut SAFEARRAY> {
        self.table_owner()?;
        provider_array(&[])
    }

    fn GetColumnHeaderItems(&self) -> Result<*mut SAFEARRAY> {
        let column = match self.state()?.role {
            AccessibleRole::GridCell { column, .. } => column,
            _ => return Err(unavailable()),
        };
        Provider {
            state: self.table_owner()?,
        }
        .headers(Some(column))
    }
}

pub(in super::super) struct GridAccessibility {
    binding: Box<Binding>,
}

struct Binding {
    state: Arc<Mutex<State>>,
    provider: IRawElementProviderSimple,
}

impl GridAccessibility {
    pub(in super::super) fn new(
        window: SYS_HWND,
        descriptor: &NodeDescriptor,
    ) -> std::result::Result<Self, String> {
        let state = Arc::new(Mutex::new(State {
            window: window as usize,
            live: true,
            role: descriptor.accessible_role.ok_or("missing grid role")?,
            name: descriptor.accessible_name.clone().unwrap_or_default(),
            description: descriptor
                .accessible_description
                .clone()
                .unwrap_or_default(),
            parent: None,
            items: Vec::new(),
        }));
        let provider: IRawElementProviderSimple = Provider {
            state: state.clone(),
        }
        .into();
        let mut binding = Box::new(Binding { state, provider });
        if unsafe {
            SetWindowSubclass(
                window,
                Some(grid_proc),
                SUBCLASS,
                (&mut *binding as *mut Binding) as usize,
            )
        } == 0
        {
            return Err(super::super::native_error(
                "subclass native grid accessibility",
            ));
        }
        Ok(Self { binding })
    }

    pub(in super::super) fn update(&self, descriptor: &NodeDescriptor) {
        let role = descriptor.accessible_role.unwrap();
        let name = descriptor.accessible_name.clone().unwrap_or_default();
        let description = descriptor
            .accessible_description
            .clone()
            .unwrap_or_default();
        let (previous, previous_name, previous_description) = {
            let mut state = self.binding.state.lock().unwrap();
            let previous = (state.role, state.name.clone(), state.description.clone());
            state.role = role;
            state.name = name.clone();
            state.description = description.clone();
            if matches!(role, AccessibleRole::Grid { .. }) {
                state.parent = None;
            }
            previous
        };
        let changes = match (previous, role) {
            (
                AccessibleRole::Grid {
                    rows: old_rows,
                    columns: old_columns,
                },
                AccessibleRole::Grid { rows, columns },
            ) => vec![
                (UIA_GridRowCountPropertyId, old_rows, rows),
                (UIA_GridColumnCountPropertyId, old_columns, columns),
            ],
            (
                AccessibleRole::GridCell {
                    row: old_row,
                    column: old_column,
                },
                AccessibleRole::GridCell { row, column },
            ) => vec![
                (UIA_GridItemRowPropertyId, old_row, row),
                (UIA_GridItemColumnPropertyId, old_column, column),
            ],
            _ => Vec::new(),
        };
        for (property, previous, current) in changes {
            if previous != current {
                unsafe {
                    let _ = UiaRaiseAutomationPropertyChangedEvent(
                        &self.binding.provider,
                        property,
                        &VARIANT::from(previous as i32),
                        &VARIANT::from(current as i32),
                    );
                }
            }
        }
        if previous_name != name {
            unsafe {
                let _ = UiaRaiseAutomationPropertyChangedEvent(
                    &self.binding.provider,
                    UIA_NamePropertyId,
                    &VARIANT::from(previous_name.as_str()),
                    &VARIANT::from(name.as_str()),
                );
            }
        }
        if previous_description != description {
            unsafe {
                let _ = UiaRaiseAutomationPropertyChangedEvent(
                    &self.binding.provider,
                    UIA_FullDescriptionPropertyId,
                    &VARIANT::from(previous_description.as_str()),
                    &VARIANT::from(description.as_str()),
                );
            }
        }
    }

    pub(in super::super) fn link(&self, parent: Option<&Self>) {
        let parent = parent.map(|parent| parent.binding.state.clone());
        let previous = self.binding.state.lock().unwrap().parent.clone();
        if previous
            .as_ref()
            .zip(parent.as_ref())
            .is_some_and(|(previous, next)| Arc::ptr_eq(previous, next))
        {
            return;
        }
        self.binding.state.lock().unwrap().parent = parent.clone();
        if let Some(parent) = parent {
            let mut state = parent.lock().unwrap();
            state.items.retain(|item| {
                item.upgrade().is_some_and(|item| {
                    let child = item.lock().unwrap();
                    child.live
                        && child
                            .parent
                            .as_ref()
                            .is_some_and(|owner| Arc::ptr_eq(owner, &parent))
                        && !Arc::ptr_eq(&item, &self.binding.state)
                })
            });
            state.items.push(Arc::downgrade(&self.binding.state));
        }
    }
}

impl Drop for GridAccessibility {
    fn drop(&mut self) {
        let (window, live) = {
            let mut state = self.binding.state.lock().unwrap();
            let live = state.live;
            state.live = false;
            (state.window, live)
        };
        unsafe {
            if live {
                RemoveWindowSubclass(window as SYS_HWND, Some(grid_proc), SUBCLASS);
            }
            let _ = UiaDisconnectProvider(&self.binding.provider);
        }
    }
}

unsafe extern "system" fn grid_proc(
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
    if message == WM_NCDESTROY {
        binding.state.lock().unwrap().live = false;
        unsafe {
            RemoveWindowSubclass(window, Some(grid_proc), SUBCLASS);
        }
    }
    unsafe { DefSubclassProc(window, message, parameter, data) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(role: AccessibleRole) -> Arc<Mutex<State>> {
        Arc::new(Mutex::new(State {
            window: 0,
            live: true,
            name: String::new(),
            description: String::new(),
            role,
            parent: None,
            items: Vec::new(),
        }))
    }

    #[test]
    fn direct_grid_provider_rejects_stale_cells_owners_and_invalid_coordinates() {
        let owner = state(AccessibleRole::Grid {
            rows: 3,
            columns: 4,
        });
        let cell = state(AccessibleRole::GridCell { row: 1, column: 2 });
        cell.lock().unwrap().parent = Some(owner.clone());
        owner.lock().unwrap().items.push(Arc::downgrade(&cell));
        let raw: IRawElementProviderSimple = Provider {
            state: owner.clone(),
        }
        .into();
        let grid: IGridProvider = raw.cast().unwrap();
        let table: ITableProvider = raw.cast().unwrap();
        let item: IGridItemProvider = unsafe { grid.GetItem(1, 2) }.unwrap().cast().unwrap();
        let table_item: ITableItemProvider = item.cast().unwrap();
        assert_eq!(unsafe { item.Row() }.unwrap(), 1);
        assert_eq!(unsafe { item.Column() }.unwrap(), 2);
        assert_eq!(
            unsafe { grid.GetItem(-1, 2) }.unwrap_err().code(),
            HRESULT(0x80070057u32 as i32)
        );
        assert_eq!(
            unsafe { grid.GetItem(2, 3) }.unwrap_err().code(),
            unavailable().code()
        );
        cell.lock().unwrap().live = false;
        assert_eq!(
            unsafe { table_item.GetColumnHeaderItems() }
                .unwrap_err()
                .code(),
            unavailable().code()
        );
        assert_eq!(
            unsafe { item.Row() }.unwrap_err().code(),
            unavailable().code()
        );
        assert_eq!(
            unsafe { item.RowSpan() }.unwrap_err().code(),
            unavailable().code()
        );
        assert_eq!(
            unsafe { item.ContainingGrid() }.unwrap_err().code(),
            unavailable().code()
        );
        assert_eq!(
            unsafe { grid.GetItem(1, 2) }.unwrap_err().code(),
            unavailable().code()
        );
        cell.lock().unwrap().live = true;
        owner.lock().unwrap().live = false;
        assert_eq!(
            unsafe { table_item.GetColumnHeaderItems() }
                .unwrap_err()
                .code(),
            unavailable().code()
        );
        assert_eq!(
            unsafe { table.GetColumnHeaders() }.unwrap_err().code(),
            unavailable().code()
        );
        assert_eq!(
            unsafe { item.ContainingGrid() }.unwrap_err().code(),
            unavailable().code()
        );
        assert_eq!(
            unsafe { grid.RowCount() }.unwrap_err().code(),
            unavailable().code()
        );
    }
}
