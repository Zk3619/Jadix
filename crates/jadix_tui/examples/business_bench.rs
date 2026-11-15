//! Same shared application callback/update with retained or rebuilt Taffy.
use jadix::portable::{Element, Signal};
use jadix_core::presentation::NodeContent;
use jadix_tui::{
    crossterm::event::*,
    ratatui::{backend::TestBackend, layout::Rect, Terminal},
    CellApplication, Shortcut,
};
use std::{cell::RefCell, rc::Rc, time::Instant};
fn main() -> std::io::Result<()> {
    let mut results = vec![];
    for count in [500, 5000] {
        for retained in [false, true] {
            let saved = Rc::new(RefCell::new(None));
            let handle = saved.clone();
            let mut app = CellApplication::new([1., 1.], move || {
                let value = Signal::new(0usize);
                *handle.borrow_mut() = Some(value);
                let mut action = Element::new().id("increment").h(1.).flex_shrink(0.);
                action.descriptor.content = NodeContent::Button {
                    label: "0".into(),
                    enabled: true,
                };
                action.on_click = Some(Rc::new(RefCell::new(Box::new(move || {
                    value.set(value.get() + 1)
                }))));
                action.descriptor_binding = Some(Rc::new(move |d| {
                    d.content = NodeContent::Button {
                        label: value.get().to_string(),
                        enabled: true,
                    }
                }));
                let mut root = Element::new().flex_col();
                root.descriptor.content = NodeContent::Scroll;
                root.children.push(action);
                root.children.extend((0..count).map(|i| {
                    Element::new()
                        .text(format!("row {i}"))
                        .h(1.)
                        .flex_shrink(0.)
                }));
                root
            })?;
            app.set_shortcuts(vec![Shortcut {
                code: KeyCode::Char('g'),
                modifiers: KeyModifiers::CONTROL,
                target: "increment".into(),
                scope: None,
                in_editors: false,
            }])?;
            let area = Rect::new(0, 0, 80, 24);
            let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
            let start = Instant::now();
            app.present(area, 0)?;
            terminal
                .draw(|f| f.render_widget(&mut app, f.area()))
                .unwrap();
            let initial = start.elapsed().as_nanos();
            let value = saved.borrow().unwrap();
            let mut samples = vec![];
            let mut created = 0;
            let mut reused = 0;
            for step in 0..90 {
                let start = Instant::now();
                if !retained {
                    app.surface_mut().release_layout_cache();
                }
                assert!(app
                    .handle(Event::Key(KeyEvent::new(
                        KeyCode::Char('g'),
                        KeyModifiers::CONTROL
                    )))?
                    .is_some());
                terminal
                    .draw(|f| f.render_widget(&mut app, f.area()))
                    .unwrap();
                if step >= 10 {
                    samples.push(start.elapsed().as_nanos());
                    created += app.layout_stats().engine_created;
                    reused += app.layout_stats().engine_reused;
                }
            }
            assert_eq!(value.get(), 90);
            samples.sort_unstable();
            results.push(serde_json::json!({"rows":count,"retained":retained,"median_ns":samples[40],"p95_ns":samples[75],"initial_ns":initial,"samples":80,"engine_created_total":created,"engine_reused_total":reused,"scope":"named command + shared Signal callback + full projection/validation + Taffy + paint/TestBackend diff; no ANSI/TTY"}));
        }
    }
    println!(
        "{}",
        serde_json::json!({"profile":if cfg!(debug_assertions){"debug"}else{"release"},"results":results})
    );
    Ok(())
}
