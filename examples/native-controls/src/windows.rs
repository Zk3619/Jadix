#[cfg(not(windows))]
fn main() {
    eprintln!("native-windows requires a Windows host");
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
    use jadix_native::{presentation::NodeContent, windows::WindowsHost};
    use jadix_native_controls::NativeApp;
    use std::io::Write;

    let arguments: Vec<_> = std::env::args().skip(1).collect();
    let probe = match arguments.as_slice() {
        [] => None,
        [mode, nonce, output] if mode == "--probe" && !nonce.is_empty() && nonce.len() <= 128 => {
            Some((nonce, std::path::Path::new(output)))
        }
        _ => return Err("Usage: native-windows [--probe NONCE NEW_REPORT_JSON]".into()),
    };
    if probe.is_some_and(|(_, output)| output.exists()) {
        return Err("Probe output already exists".into());
    }
    let host = WindowsHost::new("Jadix Native Controls", [800, 680])?;
    let viewport = host.viewport();
    let mut app = NativeApp::new(host, viewport, 0, String::new())?;
    if let Some((nonce, output)) = probe {
        let mut pending = app.root.model().roots().to_vec();
        let mut input = None;
        let mut button = None;
        let mut scroll = None;
        while let Some(identifier) = pending.pop() {
            let node = app
                .root
                .model()
                .node(identifier)
                .ok_or("Missing projected node")?;
            match &node.descriptor.content {
                NodeContent::Input { .. } => input = Some(identifier),
                NodeContent::Button { label, .. } if label == "Continue" => {
                    button = Some(identifier)
                }
                NodeContent::Scroll => scroll = Some(identifier),
                _ => {}
            }
            pending.extend(&node.children);
        }
        let input = input.ok_or("The probe requires the default input page")?;
        let value = format!("{nonce} 中文 😀");
        app.root.host().replace_selection(input, &value)?;
        app.root
            .host()
            .activate(button.ok_or("Missing Continue button")?)?;
        app.poll_events()?;
        let state: serde_json::Value =
            serde_json::from_str(&app.state()).map_err(|error| error.to_string())?;
        if state["count"] != 1 || state["value"] != value {
            return Err("Native callbacks did not update shared Rust state".into());
        }
        let scroll = scroll.ok_or("Missing shared scroll root")?;
        app.root.host().scroll_to(scroll, [0.0, 420.0])?;
        app.poll_events()?;
        let offset = app.root.host().scroll_offset(scroll)?;
        if offset[1] <= 0.0 {
            return Err("Native scroll root did not scroll".into());
        }
        let revision = app.root.model().revision();
        app.render()?;
        if revision != app.root.model().revision() {
            return Err("Idle render published a new native commit".into());
        }
        let report = serde_json::json!({
            "schema": 1, "scenario": "shared-rust-native-controls", "platform": "windows",
            "nonce": nonce, "state": state, "scroll_offset": offset,
            "root": app.root.model().id().token(), "revision": revision,
            "native_controls": true, "synthetic_native_messages": true,
            "physical_ime_accepted": false, "assistive_technology_accepted": false,
            "shared_rust_callbacks": true, "idle_commit_suppressed": true,
            "inspection": app.root.inspection(false),
        });
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
        println!("{}", report);
        return Ok(());
    }
    app.root.host().show();
    let mut environment = 0;
    let mut size = [0.0, 0.0];
    loop {
        app.root.host().pump();
        if app.root.host().is_closed() {
            return Ok(());
        }
        app.poll_events()?;
        let next_size = app.root.host().viewport();
        let next_environment = app.root.host().environment();
        if next_size != size || next_environment != environment {
            size = next_size;
            environment = next_environment;
            app.resize(size, environment)?;
        }
        app.root.host().wait_for_events(u32::MAX)?;
    }
}
