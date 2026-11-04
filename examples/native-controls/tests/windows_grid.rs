#![cfg(all(windows, feature = "windows", not(feature = "gpu")))]

use jadix_native::windows::WindowsHost;
use jadix_native_controls::table::DataTable;
use windows::Win32::{
    Foundation::HWND,
    System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
    UI::Accessibility::*,
};
use windows::core::Interface;

fn element(automation: &IUIAutomation, window: usize) -> IUIAutomationElement {
    unsafe { automation.ElementFromHandle(HWND(window as _)) }.unwrap()
}

fn cell_pattern(element: &IUIAutomationElement) -> IUIAutomationGridItemPattern {
    unsafe { element.GetCurrentPattern(UIA_GridItemPatternId) }
        .unwrap()
        .cast()
        .unwrap()
}

fn headers_align_with_cells(table: &DataTable<WindowsHost>, keys: &[(u64, u64)]) {
    for (row, column) in keys {
        let header = table
            .root
            .host()
            .control_frame(table.column_header(*column).unwrap())
            .unwrap();
        let cell = table
            .root
            .host()
            .control_frame(table.cell_semantics(*row, *column).unwrap())
            .unwrap();
        assert!(
            (header.x - cell.x).abs() <= 1.0,
            "header={header:?}, cell={cell:?}"
        );
        assert!((header.width - cell.width).abs() <= 1.0);
        assert!(header.y + header.height <= cell.y + 1.0);
    }
}

#[test]
fn actual_grid_coordinates_follow_frozen_panes_sorting_filtering_and_provider_lifetime() {
    let host = WindowsHost::new("Jadix grid semantics", [800, 600]).unwrap();
    let mut table = DataTable::new(host, [800.0, 600.0], 100_000, 1000).unwrap();
    table.set_headers(true).unwrap();
    table.root.host().show();
    let automation: IUIAutomation =
        unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }.unwrap();
    let grid_window = table
        .root
        .host()
        .control_identity(table.semantics_node())
        .unwrap();
    let grid_element = element(&automation, grid_window);
    assert_eq!(
        unsafe { grid_element.CurrentControlType() }.unwrap(),
        UIA_DataGridControlTypeId
    );
    let grid: IUIAutomationGridPattern =
        unsafe { grid_element.GetCurrentPattern(UIA_GridPatternId) }
            .unwrap()
            .cast()
            .unwrap();
    assert_eq!(unsafe { grid.CurrentRowCount() }.unwrap(), 100_000);
    assert_eq!(unsafe { grid.CurrentColumnCount() }.unwrap(), 1000);
    for (row, column) in [(-1, 0), (0, -1), (100_000, 0), (0, 1000), (90_000, 900)] {
        assert!(unsafe { grid.GetItem(row, column) }.is_err());
    }
    let first = unsafe { grid.GetItem(0, 0) }.unwrap();
    let table_pattern: IUIAutomationTablePattern =
        unsafe { grid_element.GetCurrentPattern(UIA_TablePatternId) }
            .unwrap()
            .cast()
            .unwrap();
    assert_eq!(
        unsafe { table_pattern.CurrentRowOrColumnMajor() }.unwrap(),
        RowOrColumnMajor_RowMajor
    );
    assert_eq!(
        unsafe { table_pattern.GetCurrentRowHeaders().unwrap().Length() }.unwrap(),
        0
    );
    let headers = unsafe { table_pattern.GetCurrentColumnHeaders() }.unwrap();
    assert!(unsafe { headers.Length() }.unwrap() > 0);
    let first_header = unsafe { headers.GetElement(0) }.unwrap();
    assert_eq!(
        unsafe { first_header.CurrentControlType() }.unwrap(),
        UIA_HeaderItemControlTypeId
    );
    assert_eq!(
        unsafe { first_header.CurrentName() }.unwrap().to_string(),
        "Column 0000"
    );
    let table_item: IUIAutomationTableItemPattern =
        unsafe { first.GetCurrentPattern(UIA_TableItemPatternId) }
            .unwrap()
            .cast()
            .unwrap();
    let cell_headers = unsafe { table_item.GetCurrentColumnHeaderItems() }.unwrap();
    assert_eq!(unsafe { cell_headers.Length() }.unwrap(), 1);
    assert!(
        unsafe { automation.CompareElements(&cell_headers.GetElement(0).unwrap(), &first_header) }
            .unwrap()
            .as_bool()
    );
    assert_eq!(
        unsafe { first.CurrentControlType() }.unwrap(),
        UIA_DataItemControlTypeId
    );
    let first_pattern = cell_pattern(&first);
    assert_eq!(unsafe { first_pattern.CurrentRow() }.unwrap(), 0);
    assert_eq!(unsafe { first_pattern.CurrentColumn() }.unwrap(), 0);
    assert_eq!(unsafe { first_pattern.CurrentRowSpan() }.unwrap(), 1);
    assert_eq!(unsafe { first_pattern.CurrentColumnSpan() }.unwrap(), 1);
    let owner = unsafe { first_pattern.CurrentContainingGrid() }.unwrap();
    assert!(
        unsafe { automation.CompareElements(&owner, &grid_element) }
            .unwrap()
            .as_bool()
    );
    let editor = table.cell(0, 0).unwrap();
    let editor_window = table.root.host().control_identity(editor).unwrap();
    table.root.focus(Some(editor)).unwrap();
    table.root.host().select_range(editor, [0, 0]).unwrap();
    table
        .root
        .host()
        .replace_selection(editor, "语义编辑😀 ")
        .unwrap();
    table.poll_events().unwrap();
    let input = element(&automation, editor_window);
    let value: IUIAutomationValuePattern = unsafe { input.GetCurrentPattern(UIA_ValuePatternId) }
        .unwrap()
        .cast()
        .unwrap();
    assert!(
        unsafe { value.CurrentValue() }
            .unwrap()
            .to_string()
            .starts_with("语义编辑😀 ")
    );
    table.set_frozen(1, 1).unwrap();
    table.scroll_to([80_160.0, 1_600_032.0]).unwrap();
    headers_align_with_cells(&table, &[(0, 0), (50_002, 502)]);
    for key in [(0, 0), (0, 502), (50_002, 0), (50_002, 502)] {
        let target = unsafe { grid.GetItem(key.0, key.1) }.unwrap();
        let pattern = cell_pattern(&target);
        assert_eq!(unsafe { pattern.CurrentRow() }.unwrap(), key.0);
        assert_eq!(unsafe { pattern.CurrentColumn() }.unwrap(), key.1);
        let expected = table
            .root
            .host()
            .control_identity(table.cell_semantics(key.0 as u64, key.1 as u64).unwrap())
            .unwrap();
        assert!(
            unsafe { automation.CompareElements(&target, &element(&automation, expected)) }
                .unwrap()
                .as_bool()
        );
    }
    let distant = unsafe { grid.GetItem(50_002, 502) }.unwrap();
    let distant_table_item: IUIAutomationTableItemPattern =
        unsafe { distant.GetCurrentPattern(UIA_TableItemPatternId) }
            .unwrap()
            .cast()
            .unwrap();
    let distant_header = unsafe { distant_table_item.GetCurrentColumnHeaderItems() }.unwrap();
    assert_eq!(unsafe { distant_header.Length() }.unwrap(), 1);
    let distant_header = unsafe { distant_header.GetElement(0) }.unwrap();
    assert_eq!(
        unsafe { distant_header.CurrentName() }.unwrap().to_string(),
        "Column 0502"
    );
    let distant_pattern = cell_pattern(&distant);
    table.set_rows((0..100_000).rev().collect()).unwrap();
    table.set_columns((0..1000).rev().collect()).unwrap();
    table
        .set_rows((0..100_000).rev().filter(|row| row % 2 == 0).collect())
        .unwrap();
    assert!(table.cell_semantics(50_002, 502).is_some());
    headers_align_with_cells(&table, &[(50_002, 502)]);
    assert_eq!(unsafe { grid.CurrentRowCount() }.unwrap(), 50_000);
    assert_eq!(unsafe { distant_pattern.CurrentRow() }.unwrap(), 24_998);
    assert_eq!(unsafe { distant_pattern.CurrentColumn() }.unwrap(), 497);
    let reordered_header = unsafe { distant_table_item.GetCurrentColumnHeaderItems() }.unwrap();
    assert!(
        unsafe {
            automation.CompareElements(&reordered_header.GetElement(0).unwrap(), &distant_header)
        }
        .unwrap()
        .as_bool()
    );
    let relocated = unsafe { grid.GetItem(24_998, 497) }.unwrap();
    assert!(
        unsafe { automation.CompareElements(&relocated, &distant) }
            .unwrap()
            .as_bool()
    );
    assert_eq!(unsafe { first_pattern.CurrentRow() }.unwrap(), 49_999);
    assert_eq!(unsafe { first_pattern.CurrentColumn() }.unwrap(), 999);
    assert_eq!(
        table.root.host().control_identity(editor),
        Some(editor_window)
    );
    assert!(table.root.host().undo(editor).unwrap());
    table.poll_events().unwrap();
    assert!(
        table
            .root
            .host()
            .text(editor)
            .unwrap()
            .starts_with("R000000")
    );
    table.root.focus(None).unwrap();
    table.poll_events().unwrap();
    table.set_rows(vec![0, 2, 4]).unwrap();
    assert!(table.cell_semantics(50_002, 502).is_none());
    assert!(unsafe { grid.GetItem(24_998, 497) }.is_err());
    assert!(unsafe { distant_pattern.CurrentContainingGrid() }.is_err());
    if let Ok(headers) = unsafe { distant_table_item.GetCurrentColumnHeaderItems() } {
        assert_eq!(unsafe { headers.Length() }.unwrap(), 0);
    }
    let previous_header = table.column_header(0).unwrap();
    table.set_headers(false).unwrap();
    assert!(table.column_header(0).is_none());
    assert_eq!(
        unsafe { table_pattern.GetCurrentColumnHeaders().unwrap().Length() }.unwrap(),
        0
    );
    assert_eq!(
        unsafe { table_item.GetCurrentColumnHeaderItems().unwrap().Length() }.unwrap(),
        0
    );
    table.set_headers(true).unwrap();
    assert_ne!(table.column_header(0), Some(previous_header));
    assert_eq!(
        unsafe { table_item.GetCurrentColumnHeaderItems().unwrap().Length() }.unwrap(),
        1
    );
    table.root.dispose();
    assert!(unsafe { grid.GetItem(0, 0) }.is_err());
    assert!(unsafe { first_pattern.CurrentContainingGrid() }.is_err());
}
