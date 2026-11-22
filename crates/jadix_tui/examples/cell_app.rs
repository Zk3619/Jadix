//! CPU-only shared-protocol surface. No Kitty probe, GPU or native OS views.
use jadix_core::presentation::{
    style::NodeStyle, EventPayload, NodeContent, NodeDescriptor, NodeId, PresentationNode,
    Rect as HostRect, RootId, RootLifetime,
};
use jadix_tui::{
    crossterm::event::{Event, KeyCode, KeyModifiers},
    ratatui::{backend::CrosstermBackend, layout::Rect, Terminal, TerminalOptions, Viewport},
    CellColors, CellSurface, FrameMeter, StdinEvents, TerminalModes, TerminalOutput, WireEvent,
};
use std::{
    io::{self, Write},
    time::Duration,
};

fn node(id: NodeId, content: NodeContent, frame: [f64; 4]) -> PresentationNode {
    PresentationNode {
        id,
        descriptor: NodeDescriptor {
            content,
            frame: HostRect {
                x: frame[0],
                y: frame[1],
                width: frame[2],
                height: frame[3],
            },
            style: NodeStyle {
                foreground: [226, 232, 240, 255],
                ..Default::default()
            },
            accessible_description: None,
            accessible_name: None,
            accessible_role: None,
            accessible_selected: None,
            accessible_expanded: None,
            interaction: Default::default(),
            keyboard: Default::default(),
            navigation_keys: Vec::new(),
            accessibility: Default::default(),
            accessibility_relations: Default::default(),
        },
        children: vec![],
    }
}
fn tree(
    ids: &[NodeId],
    size: Rect,
    value: &str,
    edit_revision: u64,
    count: u32,
    modal: bool,
) -> Vec<PresentationNode> {
    let width = f64::from(size.width.max(24));
    let mut pane = node(
        ids[4],
        NodeContent::Scroll,
        [
            1.,
            7.,
            width - 2.,
            f64::from(size.height.saturating_sub(8).max(1)),
        ],
    );
    for index in 0..500 {
        pane.children.push(node(
            ids[index + 7],
            NodeContent::Text(format!("{:3}  shared presentation row · 中文", index + 1)),
            [1., 7. + index as f64, width - 2., 1.],
        ));
    }
    let mut dialog = node(
        ids[5],
        NodeContent::Dialog {
            open: modal,
            close_on_overlay_click: true,
            revision: u64::from(count),
        },
        [4., 3., width - 8., 5.],
    );
    dialog.descriptor.style.background = [30, 58, 100, 255];
    dialog.children.push(node(
        ids[6],
        NodeContent::Button {
            label: "Close / Esc".into(),
            enabled: true,
        },
        [6., 5., 18., 1.],
    ));
    vec![
        node(
            ids[0],
            NodeContent::Text(format!(
                "Jadix character surface | count {count} | Ctrl+Q quit | F3 dialog"
            )),
            [1., 0., width - 2., 1.],
        ),
        node(
            ids[1],
            NodeContent::Textarea {
                max_length: None,
                value: value.into(),
                placeholder: "中文 / emoji / selection / undo".into(),
                enabled: true,
                revision: edit_revision,
            },
            [1., 2., width - 2., 2.],
        ),
        node(
            ids[2],
            NodeContent::Button {
                label: "Increment".into(),
                enabled: true,
            },
            [1., 5., 20., 1.],
        ),
        node(
            ids[3],
            NodeContent::Button {
                label: "Dialog".into(),
                enabled: true,
            },
            [24., 5., 20., 1.],
        ),
        pane,
        dialog,
    ]
}
fn main() -> io::Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let stdio = args.iter().any(|s| s == "--stdio");
    let auto = args.iter().any(|s| s == "--auto-layout");
    let colors = args
        .iter()
        .position(|s| s == "--colors")
        .map(|index| {
            args.get(index + 1)
                .ok_or_else(|| io::Error::other("missing colors"))?
                .parse::<CellColors>()
        })
        .transpose()?
        .unwrap_or_default();
    let output_timeout = args
        .iter()
        .position(|s| s == "--output-timeout-ms")
        .map(|index| {
            args.get(index + 1)
                .ok_or_else(|| io::Error::other("missing output timeout"))?
                .parse::<u64>()
                .map_err(io::Error::other)
        })
        .transpose()?
        .unwrap_or(10_000);
    let mut modes = if stdio {
        None
    } else {
        Some(TerminalModes::enter_with_output_timeout(
            Duration::from_millis(output_timeout),
        )?)
    };
    let root = RootId::new();
    let lifetime = RootLifetime::new(root);
    let ids: Vec<_> = (0..509).map(|_| NodeId::new(root)).collect();
    let mut surface = CellSurface::new(lifetime.clone(), if auto { [8., 16.] } else { [1., 1.] })?;
    surface.set_colors(colors);
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
    let mut size = terminal.size()?;
    let mut area = Rect::new(0, 0, size.width, size.height);
    let mut count = 0u32;
    let mut value = String::new();
    let mut edit_revision = 0;
    let mut modal = false;
    publish(
        &mut surface,
        &ids,
        area,
        &value,
        edit_revision,
        count,
        modal,
        auto,
    )?;
    let mut searching = false;
    let mut query = String::new();
    let mut search_requests = 0u32;
    let mut chrome_dirty = false;
    let mut input = StdinEvents::new()?;
    let mut frames = 0;
    let mut polls = 0;
    let mut resizes = 0;
    let result = (|| -> io::Result<()> {
        loop {
            if modes
                .as_ref()
                .and_then(TerminalModes::exit_signal)
                .is_some()
            {
                break;
            }
            if surface.needs_frame() || chrome_dirty {
                meter.measure(|| {
                    terminal.draw(|f| {
                        f.render_widget(&mut surface, area);
                        if searching && area.height > 0 {
                            let status = Rect::new(area.x, area.bottom() - 1, area.width, 1);
                            f.render_widget(jadix_tui::ratatui::widgets::Clear, status);
                            f.render_widget(
                                jadix_tui::ratatui::widgets::Paragraph::new(format!(
                                    "Find: {query} | {} matches | Enter keep / Esc cancel",
                                    surface.search_summary().matches
                                )),
                                status,
                            );
                        }
                    })
                })?;
                chrome_dirty = false;
                frames += 1;
            }
            polls += 1;
            let Some(WireEvent::Input(event)) = input.poll(Duration::from_secs(60))? else {
                continue;
            };
            if matches!(event,Event::Key(k) if k.kind==jadix_tui::crossterm::event::KeyEventKind::Release)
            {
                continue;
            }
            if matches!(event,Event::Key(k) if k.modifiers.contains(KeyModifiers::CONTROL)&&matches!(k.code,KeyCode::Char('q'|'c')))
            {
                break;
            }
            let mut rebuild = false;
            if let Event::Resize(w, h) = event {
                size = jadix_tui::ratatui::layout::Size::new(w, h);
                area = Rect::new(0, 0, size.width, size.height);
                rebuild = true;
                resizes += 1;
            } else if matches!(event,Event::Key(k) if k.code==KeyCode::Char('f') && k.modifiers.contains(KeyModifiers::CONTROL))
            {
                searching = true;
                query.clear();
                surface.clear_search();
                chrome_dirty = true;
                continue;
            } else if searching {
                chrome_dirty = true;
                match event {
                    Event::Key(k)
                        if k.kind != jadix_tui::crossterm::event::KeyEventKind::Release =>
                    {
                        match k.code {
                            KeyCode::Esc => {
                                searching = false;
                                surface.clear_search();
                            }
                            KeyCode::Enter => {
                                searching = false;
                            }
                            KeyCode::Backspace => {
                                query.pop();
                                surface.find(&query)?;
                                search_requests += 1;
                            }
                            KeyCode::Char(c)
                                if (k.modifiers.is_empty()
                                    || k.modifiers == KeyModifiers::SHIFT)
                                    && query.len() + c.len_utf8() <= 512 =>
                            {
                                query.push(c);
                                surface.find(&query)?;
                                search_requests += 1;
                            }
                            _ => (),
                        }
                    }
                    Event::Paste(text)
                        if query.len() + text.len() <= 512
                            && !text.chars().any(char::is_control) =>
                    {
                        query.push_str(&text);
                        surface.find(&query)?;
                        search_requests += 1;
                    }
                    _ => (),
                }
                continue;
            } else if matches!(event,Event::Key(k) if k.code==KeyCode::F(4)) {
                surface.find_next(
                    matches!(event,Event::Key(k) if k.modifiers.contains(KeyModifiers::SHIFT)),
                )?;
                continue;
            } else if matches!(event,Event::Key(k) if k.code==KeyCode::Esc)
                && surface.search_summary().matches > 0
            {
                surface.clear_search();
                continue;
            } else if matches!(event,Event::Key(k) if k.code==KeyCode::F(3)) {
                modal = !modal;
                rebuild = true;
            } else if let Some(event) = surface.handle(event)? {
                match event.payload {
                    EventPayload::Activate if event.node == ids[2] => {
                        count += 1;
                        rebuild = true;
                    }
                    EventPayload::Activate if event.node == ids[3] => {
                        modal = true;
                        rebuild = true;
                    }
                    EventPayload::Activate if event.node == ids[6] => {
                        modal = false;
                        rebuild = true;
                    }
                    EventPayload::Dismiss { .. } => {
                        modal = false;
                        rebuild = true;
                    }
                    EventPayload::Edit {
                        value: next,
                        revision,
                        ..
                    } => {
                        value = next;
                        edit_revision = revision;
                        rebuild = true;
                    }
                    _ => (),
                }
            }
            if rebuild {
                publish(
                    &mut surface,
                    &ids,
                    area,
                    &value,
                    edit_revision,
                    count,
                    modal,
                    auto,
                )?;
            }
        }
        Ok(())
    })();
    lifetime.dispose();
    let abort = modes
        .as_ref()
        .map_or(Ok(()), TerminalModes::begin_output_cleanup);
    drop(terminal);
    let restored = modes.as_mut().map_or(Ok(()), TerminalModes::restore);
    let cleanup = abort.and(restored);
    let signal = modes.as_ref().and_then(TerminalModes::exit_signal);
    let os_written = modes.as_ref().and_then(TerminalModes::written_bytes);
    let (bytes, cells) = meter.totals();
    let search = surface.search_summary();
    let receipt = serde_json::json!({"count":count,"value":value,"frames":frames,"polls":polls,"resizes":resizes,"bytes":bytes,"os_written_bytes":os_written,"diff_cells":cells,"history":meter.samples().len(),"raw_restored":!jadix_tui::crossterm::terminal::is_raw_mode_enabled()?,"kitty_required":false,"signal":signal,"output_cleanup_complete":cleanup.is_ok(),"error":result.as_ref().err().map(ToString::to_string),"auto_layout":auto,"search_requests":search_requests,"search_matches":search.matches});
    if let Some(index) = args.iter().position(|v| v == "--receipt") {
        let path = args
            .get(index + 1)
            .ok_or_else(|| io::Error::other("missing receipt path"))?;
        std::fs::write(path, receipt.to_string())?;
    } else {
        writeln!(io::stderr(), "{receipt}")?;
    }
    if signal.is_none() {
        result?;
        cleanup?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn publish(
    view: &mut CellSurface,
    ids: &[NodeId],
    area: Rect,
    value: &str,
    revision: u64,
    count: u32,
    modal: bool,
    auto: bool,
) -> io::Result<()> {
    let mut roots = tree(ids, area, value, revision, count, modal);
    if !auto {
        view.present(view.revision(), roots)?;
        return Ok(());
    }
    // The same IDs/content/events, now driven by shared NodeStyle and terminal
    // measurement. These frames are not read by automatic layout.
    let mut dialog = roots.pop().unwrap();
    let mut scroll = roots.pop().unwrap();
    let right = roots.pop().unwrap();
    let left = roots.pop().unwrap();
    let mut field = roots.pop().unwrap();
    let mut title = roots.pop().unwrap();
    title.descriptor.style.height = Some(16.);
    title.descriptor.style.shrink = 0.;
    field.descriptor.style.height = Some(32.);
    field.descriptor.style.shrink = 0.;
    scroll.descriptor.style.grow = 1.;
    scroll.descriptor.style.min_height = 0.;
    for child in &mut scroll.children {
        child.descriptor.style.height = Some(16.);
        child.descriptor.style.shrink = 0.;
    }
    let mut buttons = node(ids[508], NodeContent::Container, [0.; 4]);
    buttons.descriptor.style.axis = jadix_core::presentation::style::Axis::Row;
    buttons.descriptor.style.gap = 24.;
    buttons.descriptor.style.height = Some(16.);
    buttons.descriptor.style.shrink = 0.;
    buttons.children = vec![left, right];
    dialog.descriptor.style.width = Some(320.);
    dialog.descriptor.style.height = Some(96.);
    dialog.descriptor.style.padding = [16.; 4];
    let mut root = node(ids[507], NodeContent::Container, [0.; 4]);
    root.descriptor.style.height = Some(f32::from(area.height) * 16.);
    root.descriptor.style.padding = [0., 8., 16., 8.];
    root.descriptor.style.gap = 16.;
    root.children = vec![title, field, buttons, scroll, dialog];
    view.present_auto(view.revision(), vec![root], area, 0)?;
    Ok(())
}
