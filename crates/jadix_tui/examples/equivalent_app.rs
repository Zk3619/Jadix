//! Canonical same-business/same-screen comparison; offline committed Cell output.
use jadix::portable::{Element, Signal};
use jadix_components::portable::button;
use jadix_core::presentation::NodeContent;
use jadix_tui::{
    crossterm::event::*,
    ratatui::{backend::TestBackend, layout::Rect, Terminal},
    CellApplication,
};
use std::time::Instant;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let spec: serde_json::Value = serde_json::from_slice(&std::fs::read(&args[1])?)?;
    let w = spec["columns"].as_u64().unwrap() as u16;
    let h = spec["rows"].as_u64().unwrap() as u16;
    let label = spec["button"].as_str().unwrap().to_owned();
    let title = spec["header"].as_str().unwrap().to_owned();
    let unicode = spec["unicode"].as_str().unwrap().to_owned();
    let area = Rect::new(0, 0, w, h);
    let started = Instant::now();
    let mut app = CellApplication::new([8., 16.], move || {
        let count = Signal::new(0u32);
        let text = jadix::portable::app::reactive::bind_text("header", move || {
            format!("{title} {}", count.get())
        });
        let mut root = Element::new()
            .w(f32::from(w) * 8.)
            .h(f32::from(h) * 16.)
            .flex_col()
            .children([
                text.h(16.).flex_shrink(0.),
                Element::from(
                    button("increment")
                        .text(label)
                        .on_click(move || count.set(count.get() + 1)),
                )
                .h(16.)
                .flex_shrink(0.),
                Element::new().text(unicode).h(16.).flex_shrink(0.),
            ]);
        let mut pending = vec![&mut root];
        while let Some(node) = pending.pop() {
            node.descriptor.style.foreground = [255, 255, 255, 255];
            node.descriptor.style.background = [0, 0, 0, 255];
            node.descriptor.style.font_size = 16.;
            pending.extend(&mut node.children);
        }
        root
    })?;
    let mut terminal = Terminal::new(TestBackend::new(w, h))?;
    app.present(area, 0)?;
    app.handle(Event::FocusLost)?;
    terminal.draw(|f| f.render_widget(&mut app, area))?;
    let mount_ns = started.elapsed().as_nanos() as u64;
    assert!(app
        .surface()
        .nodes()
        .any(|(_, d)| matches!(&d.content,NodeContent::Text(t) if t=="Counter 0")));
    let mut snapshots = vec![];
    let mut times = vec![];
    let mut capture = |terminal: &Terminal<TestBackend>| {
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
    };
    capture(&terminal);
    let warmup = spec["warmup"].as_u64().unwrap();
    let count = spec["actions"].as_u64().unwrap();
    for i in 0..warmup + count {
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
        terminal.draw(|f| f.render_widget(&mut app, area))?;
        if i >= warmup {
            times.push(start.elapsed().as_nanos() as u64);
        }
        capture(&terminal);
    }
    std::fs::write(
        &args[2],
        serde_json::to_vec(
            &serde_json::json!({"host":"jadix","mount_ns":mount_ns,"samples_ns":times,"snapshots":snapshots,"spec":spec}),
        )?,
    )?;
    Ok(())
}
