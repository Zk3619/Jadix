#[cfg(not(windows))]
fn main() {
    eprintln!("native-field requires Windows");
    std::process::exit(1);
}

#[cfg(windows)]
fn main() {
    if let Err(error) = application::run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(windows)]
mod application {
    use jadix_native::{checkpoint::CheckpointStore, presentation::*, windows::WindowsHost};
    use jadix_native_controls::field::FieldApp;
    use std::{
        io::Write,
        path::{Path, PathBuf},
    };

    fn find(app: &FieldApp<WindowsHost>, caption: &str) -> Result<NodeId, String> {
        let mut pending = app.root.model().roots().to_vec();
        while let Some(id) = pending.pop() {
            let node = app.root.model().node(id).ok_or("missing node")?;
            if node.descriptor.accessible_name.as_deref() == Some(caption)
                || matches!(&node.descriptor.content, NodeContent::Button { label, .. } | NodeContent::Checkbox { label, .. } if label == caption)
            {
                return Ok(id);
            }
            pending.extend(&node.children);
        }
        Err(format!("Missing field control: {caption}"))
    }
    fn click(app: &mut FieldApp<WindowsHost>, caption: &str) -> Result<(), String> {
        app.root.host().activate(find(app, caption)?)?;
        app.poll_events()?;
        Ok(())
    }
    fn edit(app: &mut FieldApp<WindowsHost>, caption: &str, value: &str) -> Result<NodeId, String> {
        let id = find(app, caption)?;
        app.root
            .host()
            .select_range(id, [0, app.root.host().text(id)?.encode_utf16().count()])?;
        app.root.host().replace_selection(id, value)?;
        app.poll_events()?;
        Ok(id)
    }
    fn report(path: &Path, value: &serde_json::Value) -> Result<(), String> {
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)
            .map_err(|e| e.to_string())?;
        file.write_all(&serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())
    }
    pub fn run() -> Result<(), String> {
        let arguments: Vec<_> = std::env::args().skip(1).collect();
        let viewport = [520.0, 700.0];
        let mut app = FieldApp::new(
            WindowsHost::new(
                "Jadix Native Field",
                [viewport[0] as u32, viewport[1] as u32],
            )?,
            viewport,
            0,
            String::new(),
        )?;
        match arguments.as_slice() {
            [mode, nonce, state, output] if mode == "--recover" => {
                let store = CheckpointStore::new(state, 64 * 1024)?;
                let checkpoint = store.read()?.ok_or("Missing recovery checkpoint")?;
                app.restore(&checkpoint.payload)?;
                let expected = format!("Contact {nonce} 東京 😀");
                if app.root.host().text(find(&app, "Contact name")?)? != expected {
                    return Err("Recovered editor differs from checkpoint".into());
                }
                report(
                    Path::new(output),
                    &serde_json::json!({"nonce":nonce,"native_controls":true,"generation":checkpoint.generation,"state":app.state()}),
                )?;
                return Ok(());
            }
            [mode, nonce, output]
                if mode == "--probe" && !nonce.is_empty() && nonce.len() <= 128 =>
            {
                let output = Path::new(output);
                click(&mut app, "Edit request")?;
                click(&mut app, "Review request")?;
                let value = format!("Contact {nonce} 東京 😀");
                let input = edit(&mut app, "Contact name", &value)?;
                let window = app
                    .root
                    .host()
                    .control_identity(input)
                    .ok_or("Missing HWND")?;
                let note = "Repair door\nRetain original lock";
                edit(&mut app, "Work details", note)?;
                app.root
                    .host()
                    .select_value(find(&app, "Request priority")?, "urgent")?;
                app.poll_events()?;
                click(&mut app, "Confirm details")?;
                if app.root.host().control_identity(input) != Some(window) {
                    return Err("Unrelated editing replaced live HWND".into());
                }
                click(&mut app, "Review request")?;
                click(&mut app, "Back to editor")?;
                let remounted = find(&app, "Contact name")?;
                if remounted == input || app.root.host().text(remounted)? != value {
                    return Err("Navigation did not remount the correct draft".into());
                }
                let before = app.state();
                edit(&mut app, "Contact name", &"x".repeat(257))?;
                if app.state() != before || app.root.host().text(remounted)? != value {
                    return Err("Rejected edit did not restore native text".into());
                }
                let state_path = output.with_extension("checkpoint.json");
                let store = CheckpointStore::new(&state_path, 64 * 1024)?;
                let generation = store.save(None, &app.state())?;
                let recovered_path = output.with_extension("recovery.json");
                let completion =
                    std::process::Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
                        .arg("--recover")
                        .arg(nonce)
                        .arg(&state_path)
                        .arg(&recovered_path)
                        .output()
                        .map_err(|e| e.to_string())?;
                if !completion.status.success() {
                    return Err(format!(
                        "Recovery subprocess failed: {}",
                        String::from_utf8_lossy(&completion.stderr)
                    ));
                }
                let recovered: serde_json::Value = serde_json::from_slice(
                    &std::fs::read(&recovered_path).map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;
                if recovered["nonce"] != *nonce || recovered["state"] != app.state() {
                    return Err("Recovery receipt differs from current state".into());
                }
                click(&mut app, "Review request")?;
                click(&mut app, "Save request")?;
                app.storage_result(Some("injected storage failure".into()))?;
                if app.storage_notification().is_none() {
                    return Err("Failed storage lost its pending save".into());
                }
                let saved = app.state();
                store.save(Some(generation), &saved)?;
                app.storage_result(None)?;
                click(&mut app, "Edit request")?;
                edit(&mut app, "Contact name", "Discard this")?;
                click(&mut app, "Cancel draft")?;
                if app.state() != saved {
                    return Err("Cancellation changed the saved request".into());
                }
                let commits = app.root.host().stats().commits;
                app.render()?;
                app.render()?;
                if app.root.host().stats().commits != commits {
                    return Err("Idle form render committed".into());
                }
                report(
                    output,
                    &serde_json::json!({"schema":1,"nonce":nonce,"application":"native-field","native_controls":true,
                    "synthetic_native_messages":true,"draft_roundtrip":true,"rejected_edit_restored_native_value":true,
                    "storage_failure_preserved_pending_save":true,"independent_process_recovery":recovered,
                    "checkpoint_generation":2,"cancel_preserved_saved_request":true,"idle_native_commits":0,
                    "physical_ime_accepted":false,"assistive_technology_accepted":false,"runtime_acceptance":false,
                    "state":serde_json::from_str::<serde_json::Value>(&saved).map_err(|e| e.to_string())?}),
                )?;
                return Ok(());
            }
            [] | [_, _] => {}
            _ => return Err("Usage: native-field [--state PATH | --probe NONCE OUTPUT]".into()),
        }
        let state_path = match arguments.as_slice() {
            [] => None,
            [mode, path] if mode == "--state" => Some(PathBuf::from(path)),
            _ => return Err("Usage: native-field [--state PATH]".into()),
        };
        let store = state_path
            .map(|path| CheckpointStore::new(path, 64 * 1024))
            .transpose()?;
        let mut generation = None;
        if let Some(store) = &store {
            if let Some(checkpoint) = store.read()? {
                app.restore(&checkpoint.payload)?;
                generation = Some(checkpoint.generation);
            }
        }
        let mut saved = app.state();
        app.root.host().defer_close()?;
        app.root.host().show();
        loop {
            app.root.host().pump();
            app.poll_events()?;
            let state = app.state();
            if let Some(store) = &store {
                if state != saved {
                    match store.save(generation, &state) {
                        Ok(next) => {
                            generation = Some(next);
                            saved = state;
                            app.storage_result(None)?;
                        }
                        Err(error) => {
                            app.storage_result(Some(error))?;
                        }
                    }
                }
            }
            if app.root.host().close_requested() {
                app.root
                    .host()
                    .resolve_close(store.is_none() || saved == app.state())?;
            }
            if app.root.host().is_closed() {
                return Ok(());
            }
            app.resize(app.root.host().viewport(), app.root.host().environment())?;
            app.root.host().wait_for_events(u32::MAX)?;
        }
    }
}
