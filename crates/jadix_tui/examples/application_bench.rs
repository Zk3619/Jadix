//! Same portable tree with retained scrolling versus explicit re-presentation.
use jadix::portable::Element;
use jadix_core::presentation::NodeContent;
use jadix_tui::{
    crossterm::event::*,
    ratatui::{backend::TestBackend, layout::Rect, Terminal},
    CellApplication,
};
use std::time::Instant;
fn main() -> std::io::Result<()> {
    let mut results = vec![];
    for count in [500, 5000] {
        for refresh in [false, true] {
            let mut app = CellApplication::new([1., 1.], || {
                let mut root = Element::new().flex_col();
                root.descriptor.content = NodeContent::Scroll;
                root.children = (0..count)
                    .map(|i| {
                        Element::new()
                            .text(format!("row {i} 中文"))
                            .h(1.)
                            .flex_shrink(0.)
                    })
                    .collect();
                root
            })?;
            let area = Rect::new(0, 0, 80, 24);
            let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
            let start = Instant::now();
            app.present(area, 0)?;
            terminal
                .draw(|f| f.render_widget(&mut app, f.area()))
                .unwrap();
            let initial = start.elapsed().as_nanos();
            let revision = app.surface().revision();
            let mut samples = vec![];
            for step in 0..140 {
                let start = Instant::now();
                app.handle(Event::Mouse(MouseEvent {
                    kind: if step / 70 % 2 == 0 {
                        MouseEventKind::ScrollDown
                    } else {
                        MouseEventKind::ScrollUp
                    },
                    column: 10,
                    row: 10,
                    modifiers: KeyModifiers::NONE,
                }))?;
                if refresh {
                    app.present(area, 0)?;
                }
                terminal
                    .draw(|f| f.render_widget(&mut app, f.area()))
                    .unwrap();
                if step >= 20 {
                    samples.push(start.elapsed().as_nanos());
                }
            }
            assert_eq!(revision, app.surface().revision());
            samples.sort_unstable();
            results.push(serde_json::json!({"nodes":count,"refresh_on_scroll":refresh,"initial_present_paint_ns":initial,"median_ns":samples[60],"p95_ns":samples[113],"samples":120,"scope":"portable app.handle + optional complete re-projection/layout + paint + TestBackend diff; no ANSI/TTY"}));
        }
    }
    println!(
        "{}",
        serde_json::json!({"profile":if cfg!(debug_assertions){"debug"}else{"release"},"results":results})
    );
    Ok(())
}
