#[cfg(not(windows))]
fn main() {
    eprintln!("native-workbench requires Windows");
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
    use jadix_native_controls::workbench::Workbench;
    use std::io::Write;

    let arguments: Vec<_> = std::env::args().skip(1).collect();
    let probe = match arguments.as_slice() {
        [] => None,
        [mode, nonce, output] if mode == "--probe" && !nonce.is_empty() && nonce.len() <= 128 => {
            Some((nonce, std::path::Path::new(output)))
        }
        _ => {
            return Err(String::from(
                "Usage: native-workbench [--probe NONCE NEW_REPORT_JSON]",
            ))
        }
    };
    if probe.is_some_and(|(_, output)| output.exists()) {
        return Err(String::from("probe output already exists"));
    }
    let host = WindowsHost::new("Jadix Native Workbench · 100,000 records", [800, 680])?;
    let viewport = host.viewport();
    let mut app = Workbench::new(host, viewport, 100_000)?;
    if let Some((nonce, output)) = probe {
        let input = app.input_node(0).ok_or("missing first row")?;
        let identity = app
            .root
            .host()
            .control_identity(input)
            .ok_or("missing native editor")?;
        let before = app.root.host().text(input)?;
        app.root
            .host()
            .select_range(input, [0, before.encode_utf16().count()])?;
        let value = format!("{nonce} 编辑 😀");
        app.root.host().replace_selection(input, &value)?;
        app.poll_events()?;
        app.root
            .host()
            .scroll_to(app.scroll_node(), [0.0, 1_600_000.0])?;
        app.poll_events()?;
        let distant = app
            .root
            .host()
            .control_frame(app.input_node(50_000).ok_or("missing distant row")?)?;
        if distant.y < 0.0 || distant.y + distant.height > f64::from(viewport[1]) {
            return Err(format!(
                "distant native row is not inside the actual viewport: {distant:?}"
            ));
        }
        if app.value(0) != Some(value.as_str())
            || app.input_node(0) != Some(input)
            || app.root.host().control_identity(input) != Some(identity)
            || app.input_node(50_000).is_none()
        {
            return Err(String::from(
                "virtual window did not preserve the pinned native editor or mount the distant row",
            ));
        }
        if !app.root.host().undo(input)? || app.root.host().text(input)? != before {
            return Err(String::from("pinned native editor lost its undo history"));
        }
        app.poll_events()?;
        let state = app.state();
        let diagnostics = app.root.host().diagnostics();
        if state["mounted_rows"]
            .as_u64()
            .ok_or("missing mounted count")?
            > 32
            || diagnostics["controls"]
                .as_u64()
                .ok_or("missing native count")?
                > 80
        {
            return Err(String::from(
                "virtual window mounted too many native controls",
            ));
        }
        let report = serde_json::json!({"schema": 1, "nonce": nonce, "state": state, "native": diagnostics, "distant_row_native_frame": [distant.x, distant.y, distant.width, distant.height],
            "pinned_edit_retained": true, "native_undo_retained": true, "native_controls": true,
            "synthetic_native_messages": true, "assistive_technology_accepted": false});
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
    app.root.host().show();
    let mut environment = 0;
    loop {
        app.root.host().pump();
        if app.root.host().is_closed() {
            return Ok(());
        }
        app.poll_events()?;
        let next = app.root.host().environment();
        if next != environment {
            environment = next;
            app.resize(app.root.host().viewport(), environment)?;
        }
        app.root.host().wait_for_events(u32::MAX)?;
    }
}
