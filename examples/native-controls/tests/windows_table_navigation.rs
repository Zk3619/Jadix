#![cfg(all(windows, feature = "windows", not(feature = "gpu")))]

use jadix_native::windows::WindowsHost;
use jadix_native_controls::table::DataTable;
use windows_sys::Win32::UI::{
    Input::KeyboardAndMouse::{GetFocus, GetKeyboardState, SetKeyboardState},
    WindowsAndMessaging::{PostMessageW, WM_KEYDOWN},
};

fn press(table: &mut DataTable<WindowsHost>, key: usize, shift: bool, control: bool) {
    let mut previous = [0u8; 256];
    assert_ne!(unsafe { GetKeyboardState(previous.as_mut_ptr()) }, 0);
    let mut keyboard = previous;
    keyboard[0x10] = if shift { 0x80 } else { 0 };
    keyboard[0x11] = if control { 0x80 } else { 0 };
    keyboard[0x12] = 0;
    assert_ne!(unsafe { SetKeyboardState(keyboard.as_ptr()) }, 0);
    let posted = unsafe { PostMessageW(GetFocus(), WM_KEYDOWN, key, 0) };
    table.root.host().pump();
    assert_ne!(unsafe { SetKeyboardState(previous.as_ptr()) }, 0);
    assert_ne!(posted, 0);
    table.poll_events().unwrap();
}

fn focused(table: &DataTable<WindowsHost>, row: u64, column: u64) {
    let input = table.cell(row, column).unwrap();
    assert_eq!(table.root.model().focused(), Some(input));
    assert_eq!(
        unsafe { GetFocus() } as usize,
        table.root.host().control_identity(input).unwrap()
    );
    assert_eq!(table.state()["pinned"], serde_json::json!([row, column]));
    let frame = table.root.host().control_frame(input).unwrap();
    assert!(frame.width > 0.0 && frame.height > 0.0, "{frame:?}");
    assert!(frame.x >= -1.0 && frame.y >= 67.0, "{frame:?}");
    assert!(
        frame.x + frame.width <= 801.0 && frame.y + frame.height <= 601.0,
        "{frame:?}"
    );
}

#[test]
fn native_grid_keys_reveal_offscreen_cells_preserve_editing_and_follow_reordered_keys() {
    let host = WindowsHost::new("Jadix table keyboard", [800, 600]).unwrap();
    let mut table = DataTable::new(host, [800.0, 600.0], 100_000, 1000).unwrap();
    table.set_headers(true).unwrap();
    table.set_frozen(1, 1).unwrap();
    table.root.host().show();
    assert_eq!(table.state()["keyboard_navigation"], true);
    table.focus_cell(0, 0).unwrap();
    table.poll_events().unwrap();
    let input = table.cell(0, 0).unwrap();
    let identity = table.root.host().control_identity(input).unwrap();
    let original = table.root.host().text(input).unwrap();
    table
        .root
        .host()
        .select_range(input, [0, original.encode_utf16().count()])
        .unwrap();
    table
        .root
        .host()
        .replace_selection(input, "原生表格😀")
        .unwrap();
    table.poll_events().unwrap();
    table.root.host().select_range(input, [0, 0]).unwrap();
    press(&mut table, 0x27, false, false);
    focused(&table, 0, 0);
    assert_eq!(table.root.host().selection(input).unwrap(), [1, 1]);
    press(&mut table, 0x09, false, false);
    focused(&table, 0, 1);
    press(&mut table, 0x09, true, false);
    focused(&table, 0, 0);
    assert_eq!(table.root.host().control_identity(input), Some(identity));
    assert!(table.root.host().undo(input).unwrap());
    assert_eq!(table.root.host().text(input).unwrap(), original);
    table.poll_events().unwrap();
    press(&mut table, 0x0d, false, false);
    focused(&table, 1, 0);
    press(&mut table, 0x0d, true, false);
    focused(&table, 0, 0);
    press(&mut table, 0x23, false, true);
    focused(&table, 99_999, 999);
    assert!(table.state()["mounted_cells"].as_u64().unwrap() <= 256);
    press(&mut table, 0x09, false, false);
    let action = table.root.model().focused().unwrap();
    assert!(matches!(
        table.root.model().node(action).unwrap().descriptor.content,
        jadix_native::presentation::NodeContent::Button { .. }
    ));
    table
        .set_rows((0..100_000).rev().filter(|key| key % 2 == 0).collect())
        .unwrap();
    table.set_columns((0..1000).rev().collect()).unwrap();
    table.focus_cell(99_998, 999).unwrap();
    table.poll_events().unwrap();
    press(&mut table, 0x09, false, false);
    focused(&table, 99_998, 998);
    press(&mut table, 0x28, false, true);
    focused(&table, 99_996, 998);
    press(&mut table, 0x22, false, false);
    let paged = table.state()["pinned"][0].as_u64().unwrap();
    assert!(paged < 99_994);
    focused(&table, paged, 998);
    press(&mut table, 0x21, false, false);
    assert!(table.state()["pinned"][0].as_u64().unwrap() > paged);
    press(&mut table, 0x24, false, true);
    focused(&table, 99_998, 999);
    assert_eq!(table.root.host().diagnostics()["queued_events"], 0);
}
