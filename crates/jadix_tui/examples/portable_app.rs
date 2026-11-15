//! The existing portable authoring/state/callbacks mounted in a real terminal.
use jadix::portable::{Element, Signal};
use jadix_components::portable::*;
use jadix_tui::{
    crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers},
    ratatui::{backend::CrosstermBackend, layout::Rect, Terminal, TerminalOptions, Viewport},
    CellApplication, CellColors, FrameMeter, MenuItem, Shortcut, StdinEvents, TerminalModes,
    TerminalOutput, WireEvent,
};
use std::{cell::RefCell, io, rc::Rc};

#[derive(Clone, Copy)]
struct State {
    count: Signal<u32>,
    text: Signal<String>,
    checked: Signal<bool>,
    open: Signal<bool>,
    history: Signal<NavigationHistory>,
}
fn build(saved: Rc<RefCell<Option<State>>>) -> Element {
    let state = State {
        count: Signal::new(0),
        text: Signal::new(String::new()),
        checked: Signal::new(false),
        open: Signal::new(false),
        history: Signal::new(NavigationHistory::new(["edit", "review"], "edit").unwrap()),
    };
    *saved.borrow_mut() = Some(state);
    let title = jadix::portable::app::reactive::bind_text("title", move || {
        format!(
            "Jadix portable app | count {} | Ctrl+G count | Ctrl+Q quit | F3 dialog | F5 search",
            state.count.get()
        )
    });
    let mut editor: Element = textarea("draft").value_signal(state.text).into();
    editor.descriptor.accessible_name = Some("Draft".into());
    let mut list = scroll_area("rows");
    list.descriptor.style.grow = 1.;
    list.descriptor.style.min_height = 32.;
    list.descriptor.style.height = Some(0.);
    list.children = (1..=500)
        .map(|i| {
            Element::new()
                .id(format!("row-{i}"))
                .text(format!("{i:3} shared portable row · 中文"))
                .h(16.)
                .flex_shrink(0.)
        })
        .collect();
    let edit = Element::new().flex_col().gap(16.).children([
        editor.h(32.).flex_shrink(0.),
        button("increment")
            .text("Increment")
            .on_click(move || state.count.set(state.count.get() + 1))
            .into(),
        checkbox("check")
            .checked_signal(state.checked)
            .child(Element::new().text("Checked"))
            .into(),
        navigation_link("review", state.history, "review", "Review"),
    ]);
    let mut panel = Element::new().id("dialog").w(320.).h(96.).p(16.).child(
        button("close")
            .text("Close / Esc")
            .on_click(move || state.open.set(false)),
    );
    panel.descriptor.content = jadix_core::presentation::NodeContent::Dialog {
        open: false,
        close_on_overlay_click: true,
        revision: 0,
    };
    panel.dialog_open = Some(state.open);
    let mut page = Element::new().flex_col().gap(16.).children([
        title.h(16.).flex_shrink(0.),
        navigation_page("edit-page", state.history, "edit", edit).flex_shrink(0.),
        navigation_page(
            "review-page",
            state.history,
            "review",
            Element::new()
                .flex_col()
                .child(Element::new().text("Review page: draft Signal is retained"))
                .child(navigation_back("back", state.history, "Back")),
        )
        .flex_shrink(0.),
        list,
        panel,
    ]);
    page.descriptor.style.background = [15, 23, 42, 255];
    let mut pending = vec![&mut page];
    while let Some(node) = pending.pop() {
        node.descriptor.style.foreground = [226, 232, 240, 255];
        pending.extend(&mut node.children);
    }
    page
}
fn main() -> io::Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let stdio = args.iter().any(|s| s == "--stdio");
    let receipt = args
        .iter()
        .position(|s| s == "--receipt")
        .map(|i| {
            args.get(i + 1)
                .cloned()
                .ok_or_else(|| io::Error::other("missing receipt path"))
        })
        .transpose()?;
    let checkpoint = args
        .iter()
        .position(|s| s == "--checkpoint")
        .map(|i| {
            args.get(i + 1)
                .cloned()
                .ok_or_else(|| io::Error::other("missing checkpoint path"))
        })
        .transpose()?;
    let mut checkpoint_restored = false;
    let mut color_query = jadix_tui::DefaultColorQuery::default();
    let mut color_query_result = None;
    let colors = args
        .iter()
        .position(|s| s == "--colors")
        .map(|i| {
            args.get(i + 1)
                .ok_or_else(|| io::Error::other("missing colors"))?
                .parse::<CellColors>()
        })
        .transpose()?
        .unwrap_or_default();
    let saved = Rc::new(RefCell::new(None));
    let handles = saved.clone();
    let mut app = CellApplication::new([8., 16.], move || build(handles))?;
    let state = saved.borrow().unwrap();
    if args.iter().any(|s| s == "--system-clipboard") {
        #[cfg(feature = "native-clipboard")]
        app.surface_mut().set_clipboard(Some(Box::new(
            jadix_tui::ClipboardContext::new().map_err(|e| io::Error::other(e.to_string()))?,
        )))?;
        #[cfg(not(feature = "native-clipboard"))]
        return Err(io::Error::other(
            "--system-clipboard requires native-clipboard feature",
        ));
    }
    app.surface_mut().set_colors(colors);
    app.surface_mut().set_scrollbars(true);
    if args.iter().any(|s| s == "--menus") {
        app.set_menubar(vec![MenuItem::submenu(
            "Actions",
            vec![
                MenuItem::command("menu.increment", "Increment", move || {
                    state.count.set(state.count.get() + 1)
                }),
                MenuItem::Separator,
                MenuItem::submenu(
                    "More",
                    vec![MenuItem::command("menu.reset", "Reset", move || {
                        state.count.set(0)
                    })],
                ),
            ],
        )])?;
        app.set_context_menu(vec![MenuItem::command(
            "context.increment",
            "Increment",
            move || state.count.set(state.count.get() + 1),
        )])?;
    }
    app.set_shortcuts(vec![Shortcut {
        code: KeyCode::Char('g'),
        modifiers: KeyModifiers::CONTROL,
        target: "increment".into(),
        scope: None,
        in_editors: true,
    }])?;
    let mut modes = if stdio {
        None
    } else {
        Some(TerminalModes::enter()?)
    };
    let meter = FrameMeter::default();
    let mut terminal = Terminal::with_options(
        meter.backend(CrosstermBackend::new(
            meter.writer(jadix_tui::AnsiColorWriter::new(
                modes
                    .as_ref()
                    .map_or_else(TerminalOutput::stdio, TerminalModes::output),
                colors,
            )),
        )),
        TerminalOptions {
            viewport: if stdio {
                Viewport::Fixed(Rect::new(0, 0, 80, 24))
            } else {
                Viewport::Fullscreen
            },
        },
    )?;
    let mut input = StdinEvents::new()?;
    let mut frames = 0u64;
    let mut events = 0u64;
    let mut resizes = 0u64;
    let mut dismissals = 0u64;
    let mut scroll_events = 0u64;
    let mut last_scroll_y = 0.;
    let result = (|| -> io::Result<()> {
        let size = terminal.size()?;
        app.present(
            if stdio {
                Rect::new(0, 0, 80, 24)
            } else {
                Rect::new(0, 0, size.width, size.height)
            },
            0,
        )?;
        if let Some(path) = &checkpoint {
            checkpoint_restored = app.enable_checkpoint(path, &["draft"])?;
        }
        if args.iter().any(|s| s == "--query-colors") {
            color_query.start(
                &mut modes
                    .as_ref()
                    .map_or_else(TerminalOutput::stdio, TerminalModes::output),
                std::time::Instant::now(),
                std::time::Duration::from_millis(500),
            )?;
        }
        loop {
            if app.exit_requested() {
                break;
            }
            if let Some(colors) = color_query.poll(std::time::Instant::now()) {
                app.surface_mut().set_default_colors(colors);
                color_query_result = Some(colors);
            }
            if modes
                .as_ref()
                .and_then(TerminalModes::exit_signal)
                .is_some()
            {
                break;
            }
            if app.needs_frame() {
                meter.measure(|| terminal.draw(|f| f.render_widget(&mut app, f.area())))?;
                frames += 1;
            }
            let wait = color_query
                .deadline()
                .map_or(std::time::Duration::from_secs(60), |at| {
                    at.saturating_duration_since(std::time::Instant::now())
                });
            let Some(packet) = input.poll(wait)? else {
                continue;
            };
            if color_query.observe(&packet, std::time::Instant::now()) {
                continue;
            }
            let WireEvent::Input(event) = packet else {
                continue;
            };
            if matches!(event,Event::Key(k) if k.kind==KeyEventKind::Release) {
                continue;
            }
            let size = terminal.size()?;
            let area = if stdio {
                Rect::new(0, 0, 80, 24)
            } else {
                Rect::new(0, 0, size.width, size.height)
            };
            if matches!(event,Event::Key(k) if k.code==KeyCode::Char('q')&&k.modifiers.contains(KeyModifiers::CONTROL))
            {
                app.request_exit();
                continue;
            }
            if matches!(event, Event::Resize(..)) {
                resizes += 1;
                app.present(area, 0)?;
                continue;
            }
            if matches!(event,Event::Key(k) if k.code==KeyCode::F(3)) {
                state.open.set(!state.open.get());
                app.present(area, 0)?;
                continue;
            }
            if matches!(event,Event::Key(k) if k.code==KeyCode::F(5)) {
                app.surface_mut().find("500 shared")?;
                continue;
            }
            if matches!(event,Event::Key(k) if k.code==KeyCode::Char('o')&&k.modifiers.contains(KeyModifiers::CONTROL))
            {
                state.open.set(true);
                app.present(area, 0)?;
                continue;
            }
            if matches!(event,Event::Key(k) if k.code==KeyCode::Char('f')&&k.modifiers.contains(KeyModifiers::CONTROL))
            {
                app.surface_mut().find("500 shared")?;
                continue;
            }
            if let Some(event) = app.handle(event)? {
                if let jadix_core::presentation::EventPayload::Scroll { y, .. } = event.payload {
                    scroll_events += 1;
                    last_scroll_y = y;
                }
                if matches!(
                    event.payload,
                    jadix_core::presentation::EventPayload::Dismiss { .. }
                ) {
                    dismissals += 1;
                }
                events += 1;
            }
        }
        app.save_checkpoint()?;
        Ok(())
    })();
    let mut value = serde_json::json!({"portable":true,"auto_layout":true,"count":state.count.try_peek(),"value":state.text.try_peek(),"checked":state.checked.try_peek(),
        "route":state.history.try_peek().map(|h|h.current().to_owned()),"frames":frames,"events":events,"resizes":resizes,"error":result.as_ref().err().map(ToString::to_string),
        "kitty_required":false,"search_matches":app.surface().search_summary().matches,"dismissals":dismissals,"modal_open_on_exit":state.open.try_peek(),"scrollbars":true,"scroll_events":scroll_events,"last_scroll_y":last_scroll_y,
        "scroll_frames":app.surface().nodes().filter(|(_,d)|matches!(d.content,jadix_core::presentation::NodeContent::Scroll)).map(|(_,d)|[d.frame.x,d.frame.y,d.frame.width,d.frame.height]).collect::<Vec<_>>(),
        "layout_created":app.layout_stats().engine_created,"layout_reused":app.layout_stats().engine_reused,"layout_updated":app.layout_stats().engine_updated,"layout_children_updated":app.layout_stats().children_updated});
    value["default_colors"] = serde_json::json!(color_query_result.map(|c|
        serde_json::json!({"foreground":c.foreground,"background":c.background,"complete":c.complete()})));
    value["checkpoint_restored"] = serde_json::json!(checkpoint_restored);
    value["last_menu_command"] = serde_json::json!(app.menus().last_command());
    app.dispose();
    let cleanup = modes
        .as_ref()
        .map_or(Ok(()), TerminalModes::begin_output_cleanup);
    drop(terminal);
    let restored = modes.as_mut().map_or(Ok(()), TerminalModes::restore);
    let cleanup = cleanup.and(restored);
    value["raw_restored"] =
        serde_json::json!(!jadix_tui::crossterm::terminal::is_raw_mode_enabled()?);
    value["output_cleanup_complete"] = serde_json::json!(cleanup.is_ok());
    if let Some(path) = receipt {
        std::fs::write(path, serde_json::to_vec(&value)?)?;
    }
    result.and(cleanup)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jadix_tui::ratatui::{buffer::Buffer, widgets::Widget};
    #[test]
    fn scrollbar_remains_inside_initial_and_resized_portable_viewports() {
        let saved = Rc::new(RefCell::new(None));
        let mut app = CellApplication::new([8., 16.], || build(saved)).unwrap();
        app.surface_mut().set_scrollbars(true);
        for (width, height) in [(100, 36), (84, 31), (80, 24)] {
            let area = Rect::new(0, 0, width, height);
            app.present(area, 0).unwrap();
            let mut buffer = Buffer::empty(area);
            (&mut app).render(area, &mut buffer);
            let frames: Vec<_> = app
                .surface()
                .nodes()
                .filter(|(_, d)| matches!(d.content, jadix_core::presentation::NodeContent::Scroll))
                .map(|(_, d)| d.frame)
                .collect();
            assert!(
                buffer.content.iter().any(|c| c.symbol() == "█"),
                "{width}x{height} {frames:?}"
            );
        }
    }
}
