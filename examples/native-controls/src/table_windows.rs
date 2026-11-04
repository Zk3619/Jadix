#[cfg(not(windows))]
fn main() {
    eprintln!("native-table requires Windows");
    std::process::exit(1);
}

#[cfg(windows)]
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(windows)]
fn run() -> Result<(), String> {
    use jadix_native::windows::WindowsHost;
    use jadix_native_controls::table::DataTable;
    use std::io::Write;

    let arguments: Vec<_> = std::env::args().skip(1).collect();
    let probe = match arguments.as_slice() {
        [] => None,
        [mode, nonce, output] if mode == "--probe" && !nonce.is_empty() && nonce.len() <= 128 => {
            Some((nonce, std::path::Path::new(output)))
        }
        _ => {
            return Err(String::from(
                "Usage: native-table [--probe NONCE NEW_REPORT_JSON]",
            ));
        }
    };
    if probe.is_some_and(|(_, output)| output.exists()) {
        return Err(String::from("probe output already exists"));
    }
    let host = WindowsHost::new(
        "Jadix Native DataTable · 100,000 rows × 1,000 columns",
        [960, 760],
    )?;
    let viewport = host.viewport();
    let mut table = DataTable::new(host, viewport, 100_000, 1000)?;
    table.set_headers(true)?;
    if let Some((nonce, output)) = probe {
        table.root.host().show();
        let input = table.cell(0, 0).ok_or("missing first native cell")?;
        let identity = table
            .root
            .host()
            .control_identity(input)
            .ok_or("missing native editor")?;
        let before = table.root.host().text(input)?;
        table
            .root
            .host()
            .select_range(input, [0, before.encode_utf16().count()])?;
        let value = format!("{nonce} 表格 😀");
        table.root.host().replace_selection(input, &value)?;
        table.poll_events()?;
        table.scroll_to([80_000.0, 1_600_000.0])?;
        let distant = table
            .cell(50_000, 500)
            .ok_or("missing distant native cell")?;
        let frame = table.root.host().control_frame(distant)?;
        let viewport_frame = table.root.host().control_frame(table.scroll_node())?;
        if frame.x < viewport_frame.x - 1.0
            || frame.y < viewport_frame.y - 1.0
            || frame.x + frame.width > viewport_frame.x + viewport_frame.width + 1.0
            || frame.y + frame.height > viewport_frame.y + viewport_frame.height + 1.0
        {
            return Err(format!(
                "distant native cell is outside the table viewport: {frame:?} / {viewport_frame:?}"
            ));
        }
        table.set_frozen(1, 1)?;
        let panes = table.frozen_scroll_nodes();
        let corner = table.root.host().control_frame(input)?;
        let header = table
            .root
            .host()
            .control_frame(table.cell(0, 502).ok_or("missing frozen header")?)?;
        let leading = table
            .root
            .host()
            .control_frame(table.cell(50_001, 0).ok_or("missing frozen column")?)?;
        table.root.host().scroll_to(panes[1], [80_160.0, 0.0])?;
        table.poll_events()?;
        table.root.host().scroll_to(panes[2], [0.0, 1_600_032.0])?;
        table.poll_events()?;
        let frozen_corner = table.root.host().control_frame(input)?;
        let moved_header = table
            .root
            .host()
            .control_frame(table.cell(0, 502).ok_or("frozen header lost")?)?;
        let moved_leading = table
            .root
            .host()
            .control_frame(table.cell(50_001, 0).ok_or("frozen column lost")?)?;
        if frozen_corner != corner
            || (moved_header.x - header.x + 160.0).abs() > 1.0
            || moved_header.y != header.y
            || moved_leading.x != leading.x
            || (moved_leading.y - leading.y + 32.0).abs() > 1.0
        {
            return Err(format!(
                "frozen panes moved incorrectly: {corner:?}/{frozen_corner:?}, {header:?}/{moved_header:?}, {leading:?}/{moved_leading:?}"
            ));
        }
        for (pane, expected) in panes.into_iter().zip([
            [0.0, 0.0],
            [80_160.0, 0.0],
            [0.0, 1_600_032.0],
            [80_160.0, 1_600_032.0],
        ]) {
            if table.root.host().scroll_offset(pane)? != expected {
                return Err(String::from("frozen pane scroll synchronization failed"));
            }
        }
        let frozen_body = table
            .root
            .host()
            .control_frame(table.cell(50_002, 502).ok_or("missing frozen body cell")?)?;
        for (cell, pane) in [
            (input, panes[0]),
            (
                table.cell(0, 502).ok_or("missing visible header")?,
                panes[1],
            ),
            (
                table
                    .cell(50_002, 0)
                    .ok_or("missing visible leading cell")?,
                panes[2],
            ),
            (
                table.cell(50_002, 502).ok_or("missing visible body cell")?,
                panes[3],
            ),
        ] {
            let child = table.root.host().control_frame(cell)?;
            let parent = table.root.host().control_frame(pane)?;
            if child.x < parent.x - 1.0
                || child.y < parent.y - 1.0
                || child.x + child.width > parent.x + parent.width + 1.0
                || child.y + child.height > parent.y + parent.height + 1.0
            {
                return Err(format!("frozen cell is clipped: {child:?}/{parent:?}"));
            }
        }
        table.set_rows((0..100_000).rev().collect())?;
        table.set_columns((0..1000).rev().collect())?;
        table.set_rows((0..100_000).rev().filter(|key| key % 2 == 0).collect())?;
        table.measure_row(90_000, 64.0)?;
        table.measure_column(900, 220.0)?;
        let anchored = table
            .root
            .host()
            .control_frame(table.cell(50_002, 502).ok_or("anchor row lost")?)?;
        if (anchored.x - frozen_body.x).abs() > 1.0 || (anchored.y - frozen_body.y).abs() > 1.0 {
            return Err(format!(
                "table lost its frozen scroll anchor: {frozen_body:?} -> {anchored:?}"
            ));
        }
        if table.cell(0, 0) != Some(input)
            || table.root.host().control_identity(input) != Some(identity)
            || table.value(0, 0) != Some(value.as_str())
        {
            return Err(String::from(
                "table edit changed identity or value after reordering and filtering",
            ));
        }
        if !table.root.host().undo(input)? || table.root.host().text(input)? != before {
            return Err(String::from("table edit lost native undo history"));
        }
        table.poll_events()?;
        table.set_frozen(0, 0)?;
        table.set_frozen(1, 1)?;
        if table.cell(0, 0) != Some(input)
            || table.root.host().control_identity(input) != Some(identity)
        {
            return Err(String::from("freeze toggle replaced the retained editor"));
        }
        probe_grid(&table)?;
        let keyboard = probe_keyboard(&mut table)?;
        let state = table.state();
        let native = table.root.host().diagnostics();
        if state["mounted_cells"]
            .as_u64()
            .ok_or("missing mounted count")?
            > 256
            || native["controls"].as_u64().ok_or("missing native count")? > 650
        {
            return Err(String::from("table exceeded its native mounting budget"));
        }
        let report = serde_json::json!({"schema": 1, "nonce": nonce, "native_controls": true,
            "logical_rows_before_filter": 100000, "logical_columns": 1000, "generated_values": true,
            "state": state, "native": native, "distant_cell_frame": [frame.x, frame.y, frame.width, frame.height],
            "frozen_body_frame": [frozen_body.x, frozen_body.y, frozen_body.width, frozen_body.height],
            "frozen_corner_frame": [frozen_corner.x, frozen_corner.y, frozen_corner.width, frozen_corner.height],
            "anchored_cell_frame": [anchored.x, anchored.y, anchored.width, anchored.height],
            "frozen_rows_and_columns": true, "four_panes_synchronized": true, "freeze_toggle_retained_editor": true,
            "scrollbars_hidden_when_frozen": true,
            "retained_native_editor": true, "native_undo_retained": true, "synthetic_native_messages": true,
            "retained_editor_scope": "editing-sort-filter-freeze-before-keyboard-reveal",
            "keyboard_navigation": keyboard, "physical_keyboard_exercised": false,
            "ime_navigation_exercised": false,
            "grid_mounted_patterns_observed": true, "grid_pattern_scope": "mounted-cells",
            "grid_unmounted_item_rejected": true, "virtualized_item_implemented": false,
            "table_headers_implemented": true, "table_header_scope": "mounted-columns",
            "table_header_patterns_observed": true, "row_headers_implemented": false,
            "grid_accessibility_accepted": false, "assistive_technology_accepted": false});
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output)
            .map_err(|error| error.to_string())?;
        writeln!(
            file,
            "{}",
            serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?
        )
        .map_err(|error| error.to_string())?;
        println!("{report}");
        return Ok(());
    }
    table.root.host().show();
    let mut environment = 0;
    loop {
        table.root.host().pump();
        if table.root.host().is_closed() {
            return Ok(());
        }
        table.poll_events()?;
        let next = table.root.host().environment();
        if next != environment {
            environment = next;
            table.resize(table.root.host().viewport(), environment)?;
        }
        table.root.host().wait_for_events(u32::MAX)?;
    }
}

#[cfg(windows)]
fn probe_keyboard(
    table: &mut jadix_native_controls::table::DataTable<jadix_native::windows::WindowsHost>,
) -> Result<serde_json::Value, String> {
    use windows_sys::Win32::UI::{
        Input::KeyboardAndMouse::{GetFocus, GetKeyboardState, SetKeyboardState},
        WindowsAndMessaging::{PostMessageW, WM_KEYDOWN},
    };

    if !table.focus_cell(99_998, 999)? {
        return Err(String::from(
            "keyboard probe could not focus its first cell",
        ));
    }
    let mut steps = Vec::new();
    for (key, shift, control, target) in [
        (0x09, false, false, Some((99_998, 998))),
        (0x0d, false, false, Some((99_996, 998))),
        (0x23, false, true, Some((0, 0))),
        (0x09, true, false, Some((0, 1))),
        (0x24, false, true, Some((99_998, 999))),
        (0x09, true, false, None),
    ] {
        let mut previous = [0u8; 256];
        if unsafe { GetKeyboardState(previous.as_mut_ptr()) } == 0 {
            return Err(String::from("keyboard probe could not read modifier state"));
        }
        let mut keyboard = previous;
        keyboard[0x10] = if shift { 0x80 } else { 0 };
        keyboard[0x11] = if control { 0x80 } else { 0 };
        keyboard[0x12] = 0;
        if unsafe { SetKeyboardState(keyboard.as_ptr()) } == 0 {
            return Err(String::from("keyboard probe could not set modifier state"));
        }
        let posted = unsafe { PostMessageW(GetFocus(), WM_KEYDOWN, key, 0) };
        table.root.host().pump();
        let restored = unsafe { SetKeyboardState(previous.as_ptr()) };
        if posted == 0 || restored == 0 {
            return Err(String::from(
                "keyboard probe message or modifier restoration failed",
            ));
        }
        table.poll_events()?;
        let focused = table
            .root
            .model()
            .focused()
            .ok_or("keyboard focus missing")?;
        if unsafe { GetFocus() } as usize
            != table
                .root
                .host()
                .control_identity(focused)
                .ok_or("focused native control missing")?
        {
            return Err(String::from("keyboard model and native focus diverged"));
        }
        let mut frame = None;
        if let Some((row, column)) = target {
            if table.cell(row, column) != Some(focused)
                || table.state()["pinned"] != serde_json::json!([row, column])
            {
                return Err(String::from("keyboard navigation lost its business key"));
            }
            let bounds = table.root.host().control_frame(focused)?;
            let viewport = table.root.host().viewport();
            if bounds.width <= 0.0
                || bounds.height <= 0.0
                || bounds.x < -1.0
                || bounds.y < 67.0
                || bounds.x + bounds.width > f64::from(viewport[0]) + 1.0
                || bounds.y + bounds.height > f64::from(viewport[1]) + 1.0
            {
                return Err(format!(
                    "keyboard target is outside the native viewport: {bounds:?}"
                ));
            }
            frame = Some([bounds.x, bounds.y, bounds.width, bounds.height]);
        } else if !matches!(
            table
                .root
                .model()
                .node(focused)
                .ok_or("focused model node missing")?
                .descriptor
                .content,
            jadix_native::presentation::NodeContent::Button { .. }
        ) {
            return Err(String::from(
                "keyboard boundary did not release focus to the toolbar",
            ));
        }
        if table.root.host().diagnostics()["queued_events"] != 0 {
            return Err(String::from(
                "keyboard navigation left unconsumed native events",
            ));
        }
        steps.push(
            serde_json::json!({"virtual_key": key, "shift": shift, "control": control,
            "business_cell": target, "native_frame": frame, "model_and_native_focus_match": true}),
        );
    }
    Ok(
        serde_json::json!({"input_source": "posted-window-messages", "steps": steps,
        "sorted_filtered_axes": true, "offscreen_reveal": true, "boundary_toolbar_focus": true}),
    )
}

#[cfg(windows)]
fn probe_grid(
    table: &jadix_native_controls::table::DataTable<jadix_native::windows::WindowsHost>,
) -> Result<(), String> {
    use windows::Win32::{
        Foundation::HWND,
        System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
        UI::Accessibility::*,
    };
    use windows::core::Interface;

    let inspect = || -> windows::core::Result<()> {
        let automation: IUIAutomation =
            unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }?;
        let window = table
            .root
            .host()
            .control_identity(table.semantics_node())
            .unwrap();
        let element = unsafe { automation.ElementFromHandle(HWND(window as _)) }?;
        let grid: IUIAutomationGridPattern =
            unsafe { element.GetCurrentPattern(UIA_GridPatternId) }?.cast()?;
        let headers: IUIAutomationTablePattern =
            unsafe { element.GetCurrentPattern(UIA_TablePatternId) }?.cast()?;
        if unsafe { headers.GetCurrentColumnHeaders()?.Length() }? == 0
            || unsafe { headers.GetCurrentRowHeaders()?.Length() }? != 0
            || unsafe { headers.CurrentRowOrColumnMajor() }? != RowOrColumnMajor_RowMajor
        {
            return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
                0x80004005u32 as i32,
            )));
        }
        if unsafe { grid.CurrentRowCount() }? != 50_000
            || unsafe { grid.CurrentColumnCount() }? != 1000
            || unsafe { grid.GetItem(123, 123) }.is_ok()
        {
            return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
                0x80004005u32 as i32,
            )));
        }
        for (key, coordinate) in [((0, 0), (49_999, 999)), ((50_002, 502), (24_998, 497))] {
            let item = unsafe { grid.GetItem(coordinate.0, coordinate.1) }?;
            let pattern: IUIAutomationGridItemPattern =
                unsafe { item.GetCurrentPattern(UIA_GridItemPatternId) }?.cast()?;
            let owner = unsafe { pattern.CurrentContainingGrid() }?;
            let node = table.cell_semantics(key.0, key.1).unwrap();
            let window = table.root.host().control_identity(node).unwrap();
            let expected = unsafe { automation.ElementFromHandle(HWND(window as _)) }?;
            let header_pattern: IUIAutomationTableItemPattern =
                unsafe { item.GetCurrentPattern(UIA_TableItemPatternId) }?.cast()?;
            let column_headers = unsafe { header_pattern.GetCurrentColumnHeaderItems() }?;
            let header = unsafe { column_headers.GetElement(0) }?;
            let header_window = table
                .root
                .host()
                .control_identity(table.column_header(key.1).unwrap())
                .unwrap();
            let expected_header =
                unsafe { automation.ElementFromHandle(HWND(header_window as _)) }?;
            if unsafe { pattern.CurrentRow() }? != coordinate.0
                || unsafe { pattern.CurrentColumn() }? != coordinate.1
                || !unsafe { automation.CompareElements(&item, &expected) }?.as_bool()
                || !unsafe { automation.CompareElements(&owner, &element) }?.as_bool()
                || unsafe { column_headers.Length() }? != 1
                || unsafe { header.CurrentControlType() }? != UIA_HeaderItemControlTypeId
                || unsafe { header.CurrentName() }? != format!("Column {:04}", key.1)
                || !unsafe { automation.CompareElements(&header, &expected_header) }?.as_bool()
            {
                return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
                    0x80004005u32 as i32,
                )));
            }
        }
        Ok(())
    };
    inspect().map_err(|error| format!("native table mounted Grid probe: {error}"))
}
