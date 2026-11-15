//! Counter through the shipped SDK, VM, shared layout and character host.
use jadix_core::presentation::{component::ComponentSchema, NodeContent};
use jadix_shell::{application::ScriptApplication, wire::ComponentRegistry};
use jadix_tui::{
    crossterm::event::*,
    ratatui::{backend::TestBackend, layout::Rect, Terminal},
    ShellApplication,
};
use std::{error::Error, time::Instant};
fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().collect();
    let spec: serde_json::Value = serde_json::from_slice(&std::fs::read(&args[1])?)?;
    let columns = spec["columns"].as_u64().unwrap() as u16;
    let rows = spec["rows"].as_u64().unwrap() as u16;
    let business_rows = spec["business_rows"].as_u64().unwrap();
    let mut registry = ComponentRegistry::default();
    for schema in serde_json::from_str::<Vec<ComponentSchema>>(include_str!(
        "../../jadix_components/schema/portable-v1.json"
    ))? {
        registry.register(schema).map_err(|e| format!("{e:?}"))?;
    }
    let source = format!(
        "globalThis.businessRows={business_rows};\n{}",
        include_str!("../../jadix_shell/tests/fixtures/counter-leaves.js")
    );
    let area = Rect::new(0, 0, columns, rows);
    let mut terminal = Terminal::new(TestBackend::new(columns, rows))?;
    let start = Instant::now();
    let script = ScriptApplication::development(
        "bench.counter",
        &source,
        registry,
        jadix_shell::runtime::ResourceLimits {
            turn_milliseconds: 1000,
            memory_bytes: 128 * 1024 * 1024,
            ..Default::default()
        },
    )?;
    let mut app = ShellApplication::new(script, [8., 16.])?;
    app.publish(area, 0)?;
    app.handle(Event::FocusLost)?;
    terminal.draw(|f| f.render_widget(app.surface_mut(), area))?;
    let mount_ns = start.elapsed().as_nanos() as u64;
    let node_count = app.surface().nodes().count();
    assert_eq!(node_count as u64, business_rows + 4);
    let (mut frames, mut snapshots, mut times) = (vec![], vec![], vec![]);
    let warmup = spec["warmup"].as_u64().unwrap();
    let actions = spec["actions"].as_u64().unwrap();
    for i in 0..=warmup + actions {
        let text = terminal
            .backend()
            .buffer()
            .content
            .chunks(columns as usize)
            .map(|row| row.iter().map(|c| c.symbol()).collect::<String>())
            .collect::<Vec<_>>();
        assert!(text[0].starts_with(&format!("Counter {i}")));
        assert!(text[1].contains("Increment"));
        for n in 0..business_rows.min(21) {
            assert_eq!(
                text[n as usize + 3].trim_end(),
                format!("Row {n}: business")
            );
        }
        assert!(app
            .surface()
            .nodes()
            .any(|(_, d)| matches!(&d.content,NodeContent::Text(t) if t=="Unicode: 中文 🦀 é")));
        frames.push(text);
        if i == 0 || i == warmup + actions {
            snapshots.push(
                terminal
                    .backend()
                    .buffer()
                    .content
                    .iter()
                    .map(|c| {
                        serde_json::json!([
                            c.symbol(),
                            format!("{:?}", c.fg),
                            format!("{:?}", c.bg),
                            c.modifier.bits()
                        ])
                    })
                    .collect::<Vec<_>>(),
            );
        }
        if i == warmup + actions {
            break;
        }
        let start = Instant::now();
        app.handle(Event::FocusGained)?;
        for kind in [
            MouseEventKind::Down(MouseButton::Left),
            MouseEventKind::Up(MouseButton::Left),
        ] {
            app.handle(Event::Mouse(MouseEvent {
                kind,
                column: 4,
                row: 1,
                modifiers: KeyModifiers::NONE,
            }))?;
        }
        app.handle(Event::FocusLost)?;
        app.publish(area, 0)?;
        terminal.draw(|f| f.render_widget(app.surface_mut(), area))?;
        let first = terminal.backend().buffer().content[..columns as usize]
            .iter()
            .map(|c| c.symbol())
            .collect::<String>();
        assert_eq!(first.trim_end(), format!("Counter {}", i + 1));
        if i >= warmup {
            times.push(start.elapsed().as_nanos() as u64);
        }
    }
    let work = app.application().work();
    assert_eq!(work.full_mounts, 1);
    assert_eq!(work.leaf_patches, warmup + actions);
    std::fs::write(
        &args[2],
        serde_json::to_vec(&serde_json::json!({
            "host":"jadix-sdk","profile":if cfg!(debug_assertions){"debug"}else{"release"},
            "mount_ns":mount_ns,"samples_ns":times,"frames":frames,"snapshots":snapshots,
            "spec":spec,"nodes":node_count,"work":work,"layout_work":{"sync_nodes":app.layout_stats().sync_nodes,"geometry_reused":app.layout_stats().geometry_reused,"measurements":app.layout_stats().measurements,"nodes":app.layout_stats().nodes},
            "scope":"SDK event + QuickJS + leaf admission + shared layout + character paint + TestBackend diff; no ANSI/PTY/present"
        }))?,
    )?;
    app.dispose();
    Ok(())
}
