//! One existing shell QuickJS/SDK application presented by Ratatui.
use jadix_shell::{application::ScriptApplication, wire::ComponentRegistry};
use jadix_tui::{
    crossterm::event::{Event, KeyCode, KeyModifiers},
    ratatui::{backend::CrosstermBackend, layout::Rect, Terminal},
    ShellApplication, StdinEvents, TerminalModes, WireEvent,
};
use std::{
    io::{Read, Write},
    time::Duration,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut receipt = None;
    let mut bundle = None;
    let mut search_query = None;
    let mut receipt_cells = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--bundle" => {
                bundle = Some(std::path::PathBuf::from(
                    args.next().ok_or("bundle required")?,
                ))
            }
            "--receipt" => {
                receipt = Some(std::path::PathBuf::from(
                    args.next().ok_or("receipt required")?,
                ))
            }
            "--search-query" => search_query = Some(args.next().ok_or("search query required")?),
            "--receipt-cells" => receipt_cells = true,
            _ => {
                return Err(
                    "use --receipt PATH [--receipt-cells], --bundle PATH and/or --search-query TEXT (Ctrl-F)".into(),
                )
            }
        }
    }
    let schemas: Vec<jadix_core::presentation::component::ComponentSchema> = serde_json::from_str(
        include_str!("../../jadix_components/schema/portable-v1.json"),
    )?;
    let mut registry = ComponentRegistry::default();
    for schema in schemas.into_iter().chain(serde_json::from_str::<
        Vec<jadix_core::presentation::component::ComponentSchema>,
    >(include_str!(
        "../../jadix_components/schema/extended-v1.json"
    ))?) {
        registry.register(schema).map_err(|e| format!("{e:?}"))?;
    }
    let source = if let Some(path) = bundle {
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 1024 * 1024 {
            return Err("bundle too large".into());
        }
        String::from_utf8(bytes)?
    } else {
        include_str!("../../jadix_shell/tests/fixtures/application.js").to_owned()
    };
    let limits = jadix_shell::runtime::ResourceLimits::default();
    let vm_limits = serde_json::json!({
        "load_milliseconds":limits.load_milliseconds, "turn_milliseconds":limits.turn_milliseconds,
        "memory_bytes":limits.memory_bytes, "stack_bytes":limits.stack_bytes,
        "jobs_per_turn":limits.jobs_per_turn
    });
    let load_started = std::time::Instant::now();
    let app = match ScriptApplication::development("jadix.terminal.sdk", &source, registry, limits)
    {
        Ok(app) => app,
        Err(error) => {
            if let Some(path) = &receipt {
                std::fs::write(path, serde_json::json!({
                    "phase":"failed", "stage":"application_load", "terminal_entered":false,
                    "error":error.to_string(), "vm_limits":vm_limits, "application_load_ns":load_started.elapsed().as_nanos() as u64
                }).to_string())?;
            }
            return Err(error.into());
        }
    };
    let mut app = ShellApplication::new(app, [8., 16.])?;
    let application_load_ns = load_started.elapsed().as_nanos() as u64;
    let mut modes = TerminalModes::enter_with_output_timeout(Duration::from_millis(1000))?;
    let mut output = modes.output();
    output.write_all(b"\x1b]2;Jadix JS TSX Shell\x07")?;
    output.flush()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(output))?;
    let mut input = StdinEvents::new()?;
    let mut frames = 0;
    let mut first_publish_ns = None;
    let mut first_draw_ns = None;
    let mut cells: Option<Vec<Vec<String>>> = None;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        loop {
            if modes.exit_signal().is_some() || app.exit_requested() {
                break;
            }
            let size = terminal.size()?;
            let area = Rect::new(0, 0, size.width, size.height);
            let publish_started = std::time::Instant::now();
            app.publish(area, 0)?;
            first_publish_ns.get_or_insert(publish_started.elapsed().as_nanos() as u64);
            if app.surface().needs_frame() {
                let draw_started = std::time::Instant::now();
                let frame = terminal.draw(|f| f.render_widget(app.surface_mut(), area))?;
                first_draw_ns.get_or_insert(draw_started.elapsed().as_nanos() as u64);
                if receipt_cells {
                    cells = Some(
                        (area.y..area.bottom())
                            .map(|y| {
                                (area.x..area.right())
                                    .map(|x| frame.buffer[(x, y)].symbol().to_owned())
                                    .collect()
                            })
                            .collect(),
                    );
                }
                frames += 1;
            }
            if let Some(path) = &receipt {
                let content: Vec<_> = app
                    .application()
                    .roots()
                    .iter()
                    .flat_map(|r| r.children.iter())
                    .map(|n| format!("{:?}", n.descriptor.content))
                    .collect();
                std::fs::write(path,serde_json::json!({"frames":frames,"vm_limits":vm_limits,"startup_work":{"application_load_ns":application_load_ns,"first_publish_ns":first_publish_ns,"first_draw_ns":first_draw_ns},"viewport":[area.width,area.height],"revision":app.surface().revision(),"cells":cells,"content":content,"search_matches":app.surface().search_summary().matches,"phase":"active","application_work":app.application().work(),"layout_work":{"nodes":app.layout_stats().nodes,"sync_nodes":app.layout_stats().sync_nodes,"incremental_sync":app.layout_stats().incremental_sync},"wezterm_pane":std::env::var("WEZTERM_PANE").ok(),"wezterm_socket":std::env::var("WEZTERM_UNIX_SOCKET").ok()}).to_string())?;
            }
            if let Some(event) = input.poll(Duration::from_millis(250))? {
                match event {
                    WireEvent::Input(Event::Key(key))
                        if key.code == KeyCode::Char('q')
                            && key.modifiers.contains(KeyModifiers::CONTROL) =>
                    {
                        app.request_exit();
                    }
                    WireEvent::Input(Event::Key(key))
                        if key.code == KeyCode::Char('f')
                            && key.modifiers.contains(KeyModifiers::CONTROL)
                            && search_query.is_some() =>
                    {
                        app.surface_mut().find(search_query.as_deref().unwrap())?;
                    }
                    WireEvent::Input(event) => app.handle(event)?,
                    _ => {}
                }
            }
        }
        Ok(())
    })();
    app.dispose();
    drop(terminal);
    let _ = modes.begin_output_cleanup();
    let restored = modes.restore();
    if let Some(path) = receipt {
        let mut value: serde_json::Value = serde_json::from_slice(&std::fs::read(&path)?)?;
        value["phase"] = "closed".into();
        value["restored"] = restored.is_ok().into();
        value["error"] = result.as_ref().err().map(ToString::to_string).into();
        std::fs::write(path, serde_json::to_vec(&value)?)?;
    }
    result?;
    restored?;
    Ok(())
}
