#[cfg(not(windows))]
fn main() {
    eprintln!("native-tree requires Windows");
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
    use jadix_core::tree::{TreeCommand, TreeNode};
    use jadix_native::windows::WindowsHost;
    use jadix_native_controls::tree::{demo_tree, TreeBrowser};
    use std::io::Write;

    let arguments: Vec<_> = std::env::args().skip(1).collect();
    let probe = match arguments.as_slice() {
        [] => None,
        [mode, nonce, output] if mode == "--probe" && !nonce.is_empty() && nonce.len() <= 128 => {
            Some((nonce, std::path::Path::new(output)))
        }
        _ => {
            return Err(String::from(
                "Usage: native-tree [--probe NONCE NEW_REPORT_JSON]",
            ))
        }
    };
    if probe.is_some_and(|(_, output)| output.exists()) {
        return Err(String::from("probe output already exists"));
    }
    let host = WindowsHost::new("Jadix Native Tree · 100,002 items", [960, 720])?;
    let viewport = host.viewport();
    let mut app = TreeBrowser::new(
        host,
        viewport,
        demo_tree().map_err(|error| format!("{error:?}"))?,
    )?;
    if let Some((nonce, output)) = probe {
        let row = app.row_node(1).ok_or("missing mounted tree row")?;
        let identity = app
            .root
            .host()
            .control_identity(row)
            .ok_or("missing native tree button")?;
        app.root.host().activate(row)?;
        app.poll_events()?;
        if !app.model().selected().contains(&1)
            || app.root.host().control_identity(row) != Some(identity)
        {
            return Err(String::from("native selection did not retain its row"));
        }
        if !app.state()["arrow_key_binding"].as_bool().unwrap_or(false) {
            return Err(String::from("native tree keyboard binding is unavailable"));
        }
        app.scroll_to(1_600_000.0)?;
        let distant = app
            .row_node(50_000)
            .ok_or("distant tree row was not mounted")?;
        let frame = app.root.host().control_frame(distant)?;
        if frame.y < 79.0 || frame.y + frame.height > f64::from(viewport[1]) {
            return Err(format!("tree virtual row escaped its viewport: {frame:?}"));
        }
        let last = app.action_node(3).ok_or("missing Last command")?;
        app.root.host().activate(last)?;
        app.poll_events()?;
        if app.model().focused() != Some(&100_000) {
            return Err(String::from("tree navigation did not skip disabled item"));
        }
        app.navigate(TreeCommand::Child)?;
        let cancelled = app.pending_loads().pop().ok_or("missing lazy request")?;
        app.cancel_load(&cancelled)?;
        if app
            .complete_load(&cancelled, vec![TreeNode::leaf(200_000, "late")])
            .is_ok()
        {
            return Err(String::from("cancelled tree request was accepted"));
        }
        app.set_expanded(100_000, true)?;
        let request = app
            .pending_loads()
            .pop()
            .ok_or("missing replacement lazy request")?;
        app.complete_load(
            &request,
            vec![TreeNode::leaf(200_000, format!("{nonce} / 中文 😀"))],
        )?;
        app.navigate(TreeCommand::Child)?;
        let child = app.row_node(200_000).ok_or("lazy child was not revealed")?;
        app.root.host().activate(child)?;
        app.poll_events()?;
        if !app.model().selected().contains(&200_000) {
            return Err(String::from("loaded native row could not be selected"));
        }
        app.set_expanded(100_000, false)?;
        if app.model().focused() != Some(&100_000) || app.row_node(200_000).is_some() {
            return Err(String::from(
                "collapse did not return focus to visible parent",
            ));
        }
        let native = app.root.host().diagnostics();
        if native["controls"].as_u64().unwrap_or(u64::MAX) > 300 {
            return Err(String::from("tree exceeded its native node budget"));
        }
        let report = serde_json::json!({"schema": 1, "nonce": nonce, "native_controls": true,
            "state": app.state(), "native": native, "inspection": app.root.inspection(false),
            "distant_row_frame": [frame.x, frame.y, frame.width, frame.height],
            "retained_selected_native_button": true, "cancelled_load_rejected": true,
            "lazy_business_value": app.model().label(&200_000), "synthetic_native_messages": true,
            "treeitem_accessibility_accepted": false, "standard_arrow_key_binding": app.state()["arrow_key_binding"]});
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
        println!(
            "{}",
            serde_json::json!({"output": output, "nodes": app.model().len(), "native_controls": native["controls"]})
        );
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
        for request in app.pending_loads() {
            app.complete_load(
                &request,
                (200_000..201_000)
                    .map(|key| TreeNode::leaf(key, format!("Loaded file {key}")))
                    .collect(),
            )?;
        }
        let next = app.root.host().environment();
        if next != environment {
            environment = next;
            app.resize(app.root.host().viewport(), environment)?;
        }
        app.root.host().wait_for_events(u32::MAX)?;
    }
}
