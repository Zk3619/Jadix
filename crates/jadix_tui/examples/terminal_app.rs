//! Single-input-owner interactive Element editor with live Kitty backpressure.
use jadix::{
    core::tree::{Element, LayoutStatus},
    primitives::canvas::canvas,
    style::InputCaretDraw,
};
use jadix_tui::{
    crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind},
    environment::{self, ColorScheme, EnvironmentHost, EnvironmentSnapshot, Lifecycle},
    ratatui::layout::Rect,
    Checkpoint, CheckpointStore, EditorCheckpoint, EventSendError, EventSink, HostEvent,
    JadixScene, KittyTerminal, Mailbox, RequestSlot, SceneInput, StdinEvents, TaskUpdate,
    TerminalClipboard, TerminalEditor, TerminalInput, TerminalModes, TerminalOutput,
    TerminalViewport, WireEvent,
};

type ResourceUpdate = TaskUpdate<Result<Vec<u8>, String>>;
enum AppTask {
    SystemEnvironmentChanged,
    Environment {
        revision: u64,
        result: Result<EnvironmentSnapshot, String>,
    },
    Resource(ResourceUpdate),
    Saved {
        revision: u64,
        result: Result<u64, String>,
    },
    Loaded {
        revision: u64,
        result: Result<Option<Checkpoint>, String>,
    },
}
fn reap(workers: &mut Vec<std::thread::JoinHandle<()>>) {
    let mut index = 0;
    while index < workers.len() {
        if workers[index].is_finished() {
            let _ = workers.swap_remove(index).join();
        } else {
            index += 1;
        }
    }
}
fn pause_for_io(milliseconds: u64) -> bool {
    let end = Instant::now() + Duration::from_millis(milliseconds);
    loop {
        if TerminalModes::shutdown_requested() {
            return false;
        }
        let left = end.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return true;
        }
        std::thread::sleep(left.min(Duration::from_millis(16)));
    }
}
fn load_environment(path: Option<&std::path::Path>) -> io::Result<EnvironmentSnapshot> {
    if let Some(path) = path {
        use std::io::Read;
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take(EnvironmentSnapshot::MAX_JSON_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        EnvironmentSnapshot::from_json(&bytes)
    } else {
        environment::system_snapshot()
    }
}
use std::{
    cell::Cell,
    error::Error,
    io::{self, Write},
    num::{NonZeroU32, NonZeroUsize},
    path::PathBuf,
    rc::Rc,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

fn view(
    editor: &TerminalEditor,
    count: Rc<Cell<u32>>,
    focused: Option<&str>,
    environment: &EnvironmentSnapshot,
    tick: u32,
    resource: &str,
    modal: Option<Rc<Cell<u8>>>,
) -> Element {
    let active = environment.lifecycle == Lifecycle::Active;
    let light = environment.color_scheme == Some(ColorScheme::Light);
    let selection = editor.selection();
    let mut text = Element::new()
        .focus_id("document-text")
        .w(432.0)
        .h(146.0)
        .text(editor.document().text())
        .text_size(17.0 * environment.font_scale)
        .text_color(if light { "#14243a" } else { "#e2e8f0" });
    if active && focused == Some("document") {
        text.text_style.input_caret = Some(InputCaretDraw {
            caret_byte_index: editor.caret(),
            selection: Some((selection.start, selection.end)),
            selection_bg: [40, 90, 180, 255],
            caret_color: Some(if light {
                [20, 36, 58, 255]
            } else {
                [250, 220, 40, 255]
            }),
            selection_foreground: light.then_some([255, 255, 255, 255]),
            ..Default::default()
        });
    }
    let pressed = count.clone();
    let plot = canvas().w(440.0).h(24.0).paint(move |c, _| {
        c.clear("#15243a");
        c.set_fill("#38bdf8");
        c.fill_rect(0.0, 8.0, 40.0 + (tick % 20) as f32 * 18.0, 20.0);
        LayoutStatus::Clean
    });
    let mut root = Element::new()
        .bg(if light { "#f1f5f9" } else { "#0b1220" })
        .flex_col()
        .p(16.0)
        .gap(8.0)
        .child(
            Element::new()
                .text("Jadix terminal editor")
                .text_color(if light { "#14243a" } else { "#ffffff" })
                .text_size(22.0),
        )
        .child(
            Element::new()
                .text(format!(
                    "Count {} | F2 animate | Ctrl+Q quit | Tab focus",
                    count.get()
                ))
                .text_color(if light { "#334155" } else { "#a8bad4" })
                .text_size(13.0),
        )
        .child(
            Element::new()
                .focus_id("document")
                .focusable(true)
                .focus_ring(false)
                .w(448.0)
                .h(162.0)
                .p(8.0)
                .overflow("hidden")
                .bg(if light {
                    "#dbeafe"
                } else if focused == Some("document") {
                    "#1b3557"
                } else {
                    "#14243a"
                })
                .child(text),
        )
        .child(
            Element::new()
                .focus_id("increment")
                .focusable(true)
                .focus_ring(false)
                .w(240.0)
                .h(32.0)
                .p(5.0)
                .bg(if focused == Some("increment") {
                    "#7c3aed"
                } else {
                    "#2563eb"
                })
                .text("Increment / Enter")
                .text_color("#ffffff")
                .text_size(16.0)
                .on_click(move || pressed.set(pressed.get().saturating_add(1))),
        )
        .child(
            Element::new()
                .w(440.0)
                .h(24.0)
                .flex_row()
                .gap(8.0)
                .child(
                    Element::new()
                        .w(24.0)
                        .h(24.0)
                        .image_texture("terminal-resource"),
                )
                .child(
                    Element::new()
                        .text(resource)
                        .text_size(12.0)
                        .text_color(if light { "#334155" } else { "#a8bad4" }),
                ),
        )
        .child(plot);
    if let Some(action) = modal {
        let confirm = action.clone();
        root = root.child(
            Element::new()
                .absolute()
                .left(80.0)
                .top(100.0)
                .w(320.0)
                .h(168.0)
                .focus_id("terminal-modal")
                .bg("#26364f")
                .p(16.0)
                .flex_col()
                .gap(12.0)
                .child(
                    Element::new()
                        .text("Modal focus scope")
                        .text_size(20.0)
                        .text_color("#ffffff"),
                )
                .child(
                    Element::new()
                        .w(200.0)
                        .h(32.0)
                        .p(5.0)
                        .focus_id("modal-confirm")
                        .focusable(true)
                        .bg(if focused == Some("modal-confirm") {
                            "#7c3aed"
                        } else {
                            "#2563eb"
                        })
                        .text("Confirm action")
                        .text_color("#ffffff")
                        .on_click(move || confirm.set(1)),
                )
                .child(
                    Element::new()
                        .w(200.0)
                        .h(32.0)
                        .p(5.0)
                        .focus_id("modal-cancel")
                        .focusable(true)
                        .bg(if focused == Some("modal-cancel") {
                            "#7c3aed"
                        } else {
                            "#2563eb"
                        })
                        .text("Cancel")
                        .text_color("#ffffff")
                        .on_click(move || action.set(2)),
                ),
        );
    }
    root
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut pipe = false;
    let mut tmux = false;
    let mut tmux_depth = 1;
    let mut receipt: Option<PathBuf> = None;
    let mut timeout = 1000;
    let mut compress = false;
    let mut tiled = false;
    let mut clipboard_enabled = false;
    let mut checkpoint_path: Option<PathBuf> = None;
    let mut io_delay_ms = 0u64;
    let mut environment_path: Option<PathBuf> = None;
    let mut system_environment = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--stdio" => pipe = true,
            "--tmux" => tmux = true,
            "--tmux-depth"=>{tmux_depth=args.next().ok_or("missing tmux depth")?.parse()?;tmux=true;},
            "--compress" => compress = true,
            "--tiles" => tiled = true,
            "--clipboard" => clipboard_enabled = true,
            "--environment" => environment_path = Some(args.next().ok_or("missing environment path")?.into()),
            "--system-environment" => system_environment = true,
            "--dump-environment" => { println!("{}",serde_json::to_string(&environment::system_snapshot()?)?); return Ok(()); },
            "--checkpoint" => checkpoint_path = Some(args.next().ok_or("missing checkpoint path")?.into()),
            "--io-delay-ms" => { io_delay_ms = args.next().ok_or("missing I/O delay")?.parse()?; if io_delay_ms > 2000 { return Err("I/O delay must not exceed 2000 ms".into()); } }
            "--receipt" => receipt = Some(args.next().ok_or("missing receipt path")?.into()),
            "--timeout-ms" => timeout = args.next().ok_or("missing timeout")?.parse()?,
            _ => {
                return Err(
                    "usage: terminal_app [--stdio] [--compress] [--clipboard] [--checkpoint PATH] [--environment PATH | --system-environment] [--dump-environment] [--io-delay-ms N] [--receipt PATH] [--timeout-ms N]"
                        .into(),
                )
            }
        }
    }
    if environment_path.is_some() && system_environment {
        return Err("choose injected or system environment".into());
    }
    let initial_environment = if environment_path.is_some() || system_environment {
        load_environment(environment_path.as_deref())?
    } else {
        EnvironmentSnapshot::default()
    };
    let (_environment_owner, (environment_host, hour_runs)) = jadix_core::create_root(|| {
        let host = EnvironmentHost::new(initial_environment).expect("validated environment");
        let hour = host.environment().select(|v| v.locale.hour_cycle);
        let runs = Rc::new(Cell::new(0u32));
        let count = runs.clone();
        jadix_core::create_effect(move || {
            hour.get();
            count.set(count.get() + 1);
        });
        (host, runs)
    });
    let environment = environment_host.environment();
    let mut environment_inflight = false;
    let mut environment_updates = 0u64;
    let mut environment_errors = 0u64;
    let mut environment_conflicts = 0u64;
    if !pipe && (std::env::var_os("STY").is_some() || (std::env::var_os("TMUX").is_some() && !tmux))
    {
        return Err("screen unsupported; tmux requires --tmux and allow-passthrough".into());
    }
    let mut modes = if pipe {
        None
    } else {
        Some(TerminalModes::enter_with_output_timeout(
            Duration::from_millis(timeout),
        )?)
    };
    let size = if pipe {
        (80, 28)
    } else {
        jadix_tui::crossterm::terminal::size()?
    };
    let vp = |w, h| {
        if pipe {
            TerminalViewport::new(Rect::new(0, 0, w, h), [480, 384], 1.0)
        } else {
            #[cfg(unix)]
            if tmux {
                return TerminalViewport::current_tmux(1.0);
            }
            TerminalViewport::current(1.0)
        }
    };
    let mut terminal = KittyTerminal::new(
        modes
            .as_ref()
            .map_or_else(TerminalOutput::stdio, TerminalModes::output),
        vp(size.0, size.1)?,
        NonZeroU32::new(0x4a4460).unwrap(),
        NonZeroU32::new(0x70000000).unwrap(),
        Duration::from_millis(timeout),
    )?;
    if tmux {
        terminal.request_tmux_passthrough_depth(tmux_depth)?;
    }
    if compress {
        terminal.request_compression()?;
    }
    if tiled {
        terminal.request_tiling(jadix_tui::ratatui::layout::Size::new(16, 8))?;
    }
    let mut gpu = JadixScene::new()?;
    let mut reader = StdinEvents::new()?;
    let mailbox = Mailbox::<AppTask>::new(NonZeroUsize::new(4).unwrap())?;
    let mut system_watcher = if system_environment {
        let sink = mailbox.sink();
        Some(environment::SystemEnvironmentWatcher::start(move || {
            // Full means another event already wakes the UI. The watcher keeps
            // its latest snapshot until the UI drains it before blocking.
            let _ = sink.try_send(AppTask::SystemEnvironmentChanged);
        })?)
    } else {
        None
    };
    let mut system_notifications = 0u64;
    let resource_ui = mailbox.sink();
    let resource_sink =
        EventSink::new(
            move |update| match resource_ui.try_send(AppTask::Resource(update)) {
                Ok(()) => Ok(()),
                Err(EventSendError::Full(AppTask::Resource(update))) => {
                    Err(EventSendError::Full(update))
                }
                Err(EventSendError::Closed(AppTask::Resource(update))) => {
                    Err(EventSendError::Closed(update))
                }
                _ => unreachable!("EventSink returns the same resource payload"),
            },
        );
    let resource_slot = RequestSlot::default();
    let mut workers: Vec<std::thread::JoinHandle<()>> = Vec::new();
    let stale = Arc::new(AtomicU64::new(0));
    let mut started = 0u64;
    let mut applied = 0u64;
    let mut task_errors = 0u64;
    let mut task_rejected = 0u64;
    let mut cancelled = 0u64;
    let mut task_wakes = 0u64;
    let mut resource_generation = 0u64;
    let mut resource = String::from("F3 load / F4 cancel / F5 fail");
    jadix::queue_image_rgba8(
        "terminal-resource",
        32,
        32,
        [30, 40, 55, 255].repeat(32 * 32),
    );
    let mut editor = TerminalEditor::new("Edit me: Unicode, paste, selection and undo.")?;
    let clipboard = TerminalClipboard::new(clipboard_enabled);
    let mut file_generation = None;
    let mut file_errors = 0u64;
    let mut file_saved = 0u64;
    let mut file_loaded = 0u64;
    let mut file_conflicts = 0u64;
    let mut io_inflight = false;
    let mut clipboard_writes = 0u64;
    let mut clipboard_errors = 0u64;
    if let Some(path) = &checkpoint_path {
        let mut saved = EditorCheckpoint::new(path)?;
        let revision = editor.document().revision();
        match saved.load(&mut editor, revision) {
            Ok(loaded) => {
                if loaded {
                    file_loaded += 1;
                }
                file_generation = saved.generation();
            }
            Err(_) => {
                file_errors += 1;
                resource = "Checkpoint invalid; local text retained".into();
            }
        }
    }
    let count = Rc::new(Cell::new(0));
    let mut input = SceneInput::default();
    input.focus(Some("document".into()));
    let mut dialog = false;
    let mut modal_pending = false;
    let modal_action = Rc::new(Cell::new(0u8));
    let mut modal_opens = 0u64;
    let mut modal_accepts = 0u64;
    let mut modal_cancels = 0u64;
    let mut root = view(
        &editor,
        count.clone(),
        input.focused_id(),
        &environment.snapshot(),
        0,
        &resource,
        None,
    );
    let mut active = environment.snapshot().lifecycle == Lifecycle::Active;
    let mut animation = false;
    let mut tick = 0u32;
    let mut next_tick: Option<Instant> = None;
    let mut suspended = false;
    let mut retry_at: Option<Instant> = None;
    let mut caret_at: Option<Instant> = None;
    let mut events = 0u64;
    let mut unknown = 0u64;
    let mut mouse_trace = Vec::new();
    let mut resizes = 0u64;
    let mut exit = String::from("quit");
    let result = (|| -> Result<(), Box<dyn Error>> {
        terminal.start(Instant::now())?;
        loop {
            if modes
                .as_ref()
                .and_then(TerminalModes::exit_signal)
                .is_some()
            {
                break;
            }
            if let Some(next) = system_watcher.as_ref().and_then(|w| w.take()) {
                system_notifications += 1;
                match next {
                    Ok(mut next) => {
                        next.lifecycle = environment.snapshot().lifecycle;
                        if environment_host.apply(environment.revision(), next)? {
                            environment_updates += 1;
                            next_tick = if animation
                                && active
                                && !suspended
                                && environment.snapshot().reduced_motion != Some(true)
                            {
                                Some(Instant::now() + Duration::from_millis(50))
                            } else {
                                None
                            };
                            caret_at = None;
                            gpu.restart_caret();
                            resource = format!(
                                "System locale {} | {:?}",
                                environment.snapshot().locale.locale,
                                environment.snapshot().locale.hour_cycle
                            );
                            terminal.invalidate();
                        }
                    }
                    Err(_) => {
                        environment_errors += 1;
                        resource = "System listener failed; current preferences retained".into();
                        terminal.invalidate();
                    }
                }
            }
            let now = Instant::now();
            terminal.check(now)?;
            if caret_at.is_some_and(|at| now >= at) {
                terminal.invalidate();
                caret_at = None;
            }
            if next_tick.is_some_and(|d| now >= d) {
                tick = tick.wrapping_add(1);
                terminal.invalidate();
                next_tick = Some(now + Duration::from_millis(50));
            }
            if !suspended && retry_at.is_none_or(|d| now >= d) {
                match terminal.present(now, |viewport| {
                    let next_root = view(
                        &editor,
                        count.clone(),
                        input.focused_id(),
                        &environment.snapshot(),
                        tick,
                        &resource,
                        dialog.then(|| modal_action.clone()),
                    );
                    gpu.replace_root(&mut root, next_root);
                    let frame = gpu.render(&mut root, viewport, [-1.0; 2])?;
                    input.refresh_gesture(&root);
                    Ok(frame)
                }) {
                    Ok(presented) => {
                        retry_at = None;
                        if presented && modal_pending {
                            input.push_modal(&root, "terminal-modal")?;
                            modal_pending = false;
                            modal_opens += 1;
                            caret_at = None;
                            terminal.invalidate();
                        }
                        if presented && active && input.focused_id() == Some("document") {
                            caret_at = Some(Instant::now() + gpu.caret_wakeup_after());
                        }
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                        retry_at = Some(now + Duration::from_millis(50))
                    }
                    Err(e) => return Err(e.into()),
                }
            }
            let deadline = [terminal.deadline(), next_tick, retry_at, caret_at]
                .into_iter()
                .flatten()
                .min();
            let wait = deadline.map_or(Duration::from_secs(3600), |d| {
                d.saturating_duration_since(Instant::now())
            });
            let Some(event) = reader.poll_with(&mailbox, wait)? else {
                continue;
            };
            let wire = match event {
                HostEvent::Task(AppTask::SystemEnvironmentChanged) => continue,
                HostEvent::Task(AppTask::Environment { revision, result }) => {
                    environment_inflight = false;
                    match result {
                        Ok(mut next) => {
                            if system_environment {
                                next.lifecycle = environment.snapshot().lifecycle;
                            }
                            match environment_host.apply(revision, next) {
                                Ok(changed) => {
                                    if changed {
                                        environment_updates += 1;
                                        active =
                                            environment.snapshot().lifecycle == Lifecycle::Active;
                                        if !active {
                                            input.cancel();
                                        }
                                        next_tick = if animation
                                            && active
                                            && !suspended
                                            && environment.snapshot().reduced_motion != Some(true)
                                        {
                                            Some(Instant::now() + Duration::from_millis(50))
                                        } else {
                                            None
                                        };
                                        caret_at = None;
                                        gpu.restart_caret();
                                        terminal.invalidate();
                                        resource = format!(
                                            "Locale {} | {:?}",
                                            environment.snapshot().locale.locale,
                                            environment.snapshot().locale.hour_cycle
                                        );
                                    }
                                }
                                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                                    environment_conflicts += 1;
                                }
                                Err(_) => {
                                    environment_errors += 1;
                                }
                            }
                        }
                        Err(_) => {
                            environment_errors += 1;
                            resource = "Environment invalid; current preferences retained".into();
                            terminal.invalidate();
                        }
                    }
                    continue;
                }
                HostEvent::Input(wire) => wire,
                HostEvent::Task(AppTask::Saved { revision, result }) => {
                    io_inflight = false;
                    match result {
                        Ok(generation) => {
                            file_generation = Some(generation);
                            file_saved += 1;
                            resource = if editor.document().revision() == revision {
                                "Checkpoint saved"
                            } else {
                                "Checkpoint saved; newer local edits remain"
                            }
                            .into();
                        }
                        Err(_) => {
                            file_errors += 1;
                            resource = "Save failed; original checkpoint retained".into();
                        }
                    }
                    terminal.invalidate();
                    continue;
                }
                HostEvent::Task(AppTask::Loaded { revision, result }) => {
                    io_inflight = false;
                    match result {
                        Ok(Some(value)) => {
                            match editor.restore_checkpoint(revision, value.payload.as_bytes()) {
                                Ok(()) => {
                                    file_generation = Some(value.generation);
                                    file_loaded += 1;
                                    resource = "Checkpoint restored".into();
                                    gpu.restart_caret();
                                }
                                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                                    file_conflicts += 1;
                                    resource = "Reload rejected; newer local edits retained".into();
                                }
                                Err(_) => {
                                    file_errors += 1;
                                    resource = "Invalid checkpoint; local text retained".into();
                                }
                            }
                        }
                        Ok(None) => {
                            resource = "No saved checkpoint".into();
                        }
                        Err(_) => {
                            file_errors += 1;
                            resource = "Read failed; local text retained".into();
                        }
                    }
                    terminal.invalidate();
                    continue;
                }
                HostEvent::Task(AppTask::Resource(update)) => {
                    task_wakes += 1;
                    if !resource_slot.accepts(&update.ticket) {
                        stale.fetch_add(1, Ordering::Relaxed);
                        continue;
                    }
                    match update.value {
                        Ok(pixels) => {
                            jadix::queue_image_rgba8("terminal-resource", 32, 32, pixels);
                            resource_generation = update.ticket.generation();
                            applied += 1;
                            resource = format!("Resource {} ready", resource_generation);
                        }
                        Err(error) => {
                            task_errors += 1;
                            resource = error;
                        }
                    }
                    terminal.invalidate();
                    continue;
                }
            };
            match wire {
                WireEvent::Graphics(reply) => {
                    terminal.acknowledge(&reply, Instant::now())?;
                }
                WireEvent::Unknown(_) => unknown += 1,
                WireEvent::Input(event) => {
                    events += 1;
                    if matches!(event, Event::Mouse(_)) && mouse_trace.len() < 16 {
                        mouse_trace.push(format!(
                            "{event:?} => {:?}",
                            terminal.viewport().input(event.clone())
                        ));
                    }
                    if let Event::Key(k) = event {
                        if !active
                            && k.code != KeyCode::F(10)
                            && !(k.code == KeyCode::Char('q')
                                && k.modifiers.contains(KeyModifiers::CONTROL))
                        {
                            continue;
                        }
                        if k.kind != KeyEventKind::Release && !dialog && k.code == KeyCode::F(10) {
                            if environment_path.is_none() && !system_environment {
                                resource =
                                    "Choose --environment or --system-environment for refresh"
                                        .into();
                                terminal.invalidate();
                                continue;
                            }
                            reap(&mut workers);
                            if environment_inflight || workers.len() == 2 {
                                task_rejected += 1;
                                continue;
                            }
                            let path = environment_path.clone();
                            let revision = environment.revision();
                            let sink = mailbox.sink();
                            environment_inflight = true;
                            workers.push(std::thread::spawn(move || {
                                if !pause_for_io(io_delay_ms) {
                                    return;
                                }
                                let result =
                                    load_environment(path.as_deref()).map_err(|e| e.to_string());
                                let _ = sink.try_send(AppTask::Environment { revision, result });
                            }));
                            continue;
                        }
                        if k.kind != KeyEventKind::Release
                            && !dialog
                            && matches!(k.code, KeyCode::F(7) | KeyCode::F(8))
                        {
                            let Some(path) = checkpoint_path.clone() else {
                                resource = "Choose a checkpoint path to save or reload".into();
                                terminal.invalidate();
                                continue;
                            };
                            reap(&mut workers);
                            if io_inflight || workers.len() == 2 {
                                task_rejected += 1;
                                resource = "File operation busy".into();
                                terminal.invalidate();
                                continue;
                            }
                            let save = k.code == KeyCode::F(7);
                            let bytes = if save {
                                match editor.checkpoint_bytes() {
                                    Ok(bytes) => Some(bytes),
                                    Err(_) => {
                                        file_errors += 1;
                                        resource =
                                            "Checkpoint exceeds budget; local text retained".into();
                                        terminal.invalidate();
                                        continue;
                                    }
                                }
                            } else {
                                None
                            };
                            let revision = editor.document().revision();
                            let expected = file_generation;
                            let sink = mailbox.sink();
                            io_inflight = true;
                            resource = if save {
                                "Saving checkpoint"
                            } else {
                                "Loading checkpoint"
                            }
                            .into();
                            terminal.invalidate();
                            workers.push(std::thread::spawn(move || {
                                // Controlled latency injection is for process acceptance only.
                                if !pause_for_io(io_delay_ms) {
                                    return;
                                }
                                let store = CheckpointStore::new(
                                    path,
                                    TerminalEditor::MAX_CHECKPOINT_BYTES,
                                );
                                let update = if let Some(bytes) = bytes {
                                    let result = store.and_then(|store| {
                                        store.save(
                                            expected,
                                            std::str::from_utf8(&bytes)
                                                .map_err(|e| e.to_string())?,
                                        )
                                    });
                                    AppTask::Saved { revision, result }
                                } else {
                                    AppTask::Loaded {
                                        revision,
                                        result: store.and_then(|store| store.read()),
                                    }
                                };
                                let _ = sink.try_send(update);
                            }));
                            continue;
                        }
                        if k.kind != KeyEventKind::Release && !dialog && k.code == KeyCode::F(9) {
                            let mut output = modes
                                .as_ref()
                                .map_or_else(TerminalOutput::stdio, TerminalModes::output);
                            match clipboard.copy(editor.selected_text(), &mut output) {
                                Ok(()) => {
                                    clipboard_writes += 1;
                                    resource = "Clipboard write sent".into();
                                }
                                Err(_) => {
                                    clipboard_errors += 1;
                                    resource = "Clipboard disabled, empty or unavailable".into();
                                }
                            }
                            terminal.invalidate();
                            continue;
                        }
                        if k.kind != KeyEventKind::Release && !dialog && k.code == KeyCode::F(6) {
                            dialog = true;
                            modal_pending = true;
                            caret_at = None;
                            terminal.invalidate();
                            continue;
                        }
                        if dialog
                            && modal_pending
                            && k.kind != KeyEventKind::Release
                            && k.code == KeyCode::Esc
                        {
                            dialog = false;
                            modal_pending = false;
                            modal_cancels += 1;
                            terminal.invalidate();
                            continue;
                        }
                        if k.kind != KeyEventKind::Release
                            && !dialog
                            && matches!(k.code, KeyCode::F(3) | KeyCode::F(5))
                        {
                            reap(&mut workers);
                            if workers.len() == 2 {
                                task_rejected += 1;
                                resource = "Resource busy; try again".into();
                                terminal.invalidate();
                                continue;
                            }
                            let ticket = resource_slot.replace()?;
                            let sink = resource_sink.clone();
                            let stale = stale.clone();
                            let fail = k.code == KeyCode::F(5);
                            let generation = ticket.generation();
                            started += 1;
                            resource = format!("Resource {generation} loading");
                            terminal.invalidate();
                            workers.push(std::thread::spawn(move || {
                                // Example-owned, at most two workers; no global executor.
                                let delay = if generation % 2 == 1 { 160 } else { 40 };
                                for _ in 0..delay / 8 {
                                    std::thread::sleep(Duration::from_millis(8));
                                    if ticket.cancellation().is_cancelled() {
                                        stale.fetch_add(1, Ordering::Relaxed);
                                        return;
                                    }
                                }
                                let value = if fail {
                                    Err("Resource failed; previous image retained".into())
                                } else {
                                    Ok([40 + (generation % 180) as u8, 160, 210, 255]
                                        .repeat(32 * 32))
                                };
                                if ticket.publish(&sink, value).is_err() {
                                    stale.fetch_add(1, Ordering::Relaxed);
                                }
                            }));
                            continue;
                        }
                        if k.kind != KeyEventKind::Release && !dialog && k.code == KeyCode::F(4) {
                            resource_slot.cancel();
                            cancelled += 1;
                            resource = "Resource cancelled; previous image retained".into();
                            terminal.invalidate();
                            continue;
                        }
                        if k.kind != KeyEventKind::Release
                            && (k.code == KeyCode::Char('q')
                                && k.modifiers.contains(KeyModifiers::CONTROL))
                        {
                            break;
                        }
                        if k.kind != KeyEventKind::Release && !dialog && k.code == KeyCode::F(2) {
                            animation = !animation;
                            next_tick = if animation
                                && active
                                && !suspended
                                && environment.snapshot().reduced_motion != Some(true)
                            {
                                Some(Instant::now() + Duration::from_millis(50))
                            } else {
                                None
                            };
                            continue;
                        }
                    }
                    if let Event::Resize(w, h) = event {
                        resizes += 1;
                        suspended = w == 0 || h == 0;
                        input.cancel();
                        caret_at = None;
                        retry_at = None;
                        next_tick = if animation
                            && active
                            && !suspended
                            && environment.snapshot().reduced_motion != Some(true)
                        {
                            Some(Instant::now() + Duration::from_millis(50))
                        } else {
                            None
                        };
                        if !suspended {
                            terminal.resize(vp(w, h)?)?;
                        }
                        continue;
                    }
                    if matches!(event, Event::FocusLost | Event::FocusGained) {
                        active = event == Event::FocusGained;
                        if !active {
                            input.cancel();
                        }
                        let mut next = environment.snapshot();
                        next.lifecycle = if active {
                            Lifecycle::Active
                        } else {
                            Lifecycle::Inactive
                        };
                        environment_host.apply(environment.revision(), next)?;
                        caret_at = None;
                        next_tick = if animation
                            && active
                            && !suspended
                            && environment.snapshot().reduced_motion != Some(true)
                        {
                            Some(Instant::now() + Duration::from_millis(50))
                        } else {
                            None
                        };
                        terminal.invalidate();
                    }
                    if dialog && modal_pending {
                        continue;
                    }
                    if !active {
                        continue;
                    }
                    let Some(normal) = TerminalInput::from_event(
                        event.clone(),
                        if suspended {
                            None
                        } else {
                            Some(terminal.viewport())
                        },
                    ) else {
                        continue;
                    };
                    let before = (count.get(), input.focused_id().map(str::to_owned));
                    let was_modal = dialog;
                    input.dispatch(&root, &normal);
                    let mut changed =
                        before != (count.get(), input.focused_id().map(str::to_owned));
                    if dialog && (modal_action.get() != 0 || !input.has_modal()) {
                        if modal_action.replace(0) == 1 {
                            modal_accepts += 1;
                        } else {
                            modal_cancels += 1;
                        }
                        input.pop_modal(&root);
                        dialog = false;
                        changed = true;
                    }
                    if !was_modal && input.focused_id() == Some("document") {
                        if let TerminalInput::Pointer {
                            kind:
                                MouseEventKind::Down(MouseButton::Left)
                                | MouseEventKind::Drag(MouseButton::Left),
                            logical,
                            modifiers,
                        } = normal
                        {
                            if let Some(leaf) =
                                jadix::app::pointer_events::find_by_focus_id(&root, "document-text")
                            {
                                let at = gpu.hit_text(leaf, logical.map(|v| v as f32));
                                let anchor = if matches!(event,Event::Mouse(m) if matches!(m.kind,MouseEventKind::Drag(_)))
                                    || modifiers.contains(KeyModifiers::SHIFT)
                                {
                                    editor.anchor()
                                } else {
                                    at
                                };
                                editor.set_selection(anchor, at)?;
                                changed = true;
                            }
                        } else if !matches!(event,Event::Key(k) if matches!(k.code,KeyCode::Tab|KeyCode::BackTab))
                        {
                            changed |= editor.handle(&event)?;
                        }
                    }
                    if changed {
                        gpu.restart_caret();
                        caret_at = None;
                        terminal.invalidate();
                    }
                }
            }
        }
        Ok(())
    })();
    if let Err(error) = &result {
        exit = error.to_string();
    }
    resource_slot.close();
    if let Some(watcher) = &mut system_watcher {
        watcher.close();
    }
    mailbox.close();
    for worker in workers {
        let _ = worker.join();
    }
    let abort = modes
        .as_ref()
        .map_or(Ok(()), TerminalModes::begin_output_cleanup);
    let close = terminal.close();
    let restore = modes.as_mut().map_or(Ok(()), TerminalModes::restore);
    let signal = modes.as_ref().and_then(TerminalModes::exit_signal);
    if let Some(signal) = signal {
        exit = format!("signal:{signal}");
    }
    if let Some(path) = receipt {
        let mut file = std::fs::File::create(path)?;
        writeln!(file,"exit={exit}\nframes={}\ngpu_frames={}\nevents={events}\nunknown={unknown}\ncount={}\ncaret={}\nselection={:?}\nrevision={}\nhistory_bytes={}\ntext_hex={}\nraw_restored={}",terminal.submitted_frames(),gpu.renderer().rendered_frames(),count.get(),editor.caret(),editor.selection(),editor.document().revision(),editor.history_bytes(),editor.document().text().as_bytes().iter().map(|b|format!("{b:02x}")).collect::<String>(),!jadix_tui::crossterm::terminal::is_raw_mode_enabled().unwrap_or(false))?;
        writeln!(file,"tasks_started={started}\ntasks_applied={applied}\ntasks_stale={}\ntask_errors={task_errors}\ntask_rejected={task_rejected}\ntask_cancelled={cancelled}\ntask_wakes={task_wakes}\nresource_generation={resource_generation}\ncompressed={}",stale.load(Ordering::Relaxed),terminal.compressed())?;
        writeln!(file,"file_generation={}\nfile_saved={file_saved}\nfile_loaded={file_loaded}\nfile_conflicts={file_conflicts}\nfile_errors={file_errors}\nclipboard_writes={clipboard_writes}\nclipboard_errors={clipboard_errors}\nmodal_opens={modal_opens}\nmodal_accepts={modal_accepts}\nmodal_cancels={modal_cancels}\nmodal_depth={}\nfocused={}",file_generation.unwrap_or(0),input.modal_depth(),input.focused_id().unwrap_or(""))?;
        writeln!(file,"environment_revision={}\nenvironment_updates={environment_updates}\nenvironment_errors={environment_errors}\nenvironment_conflicts={environment_conflicts}\nsystem_notifications={system_notifications}\nhour_effect_runs={}\nanimation_ticks={tick}\nenvironment_json={}",environment.revision(),hour_runs.get(),serde_json::to_string(&environment.snapshot())?)?;
        writeln!(file, "mouse_trace={}", serde_json::to_string(&mouse_trace)?)?;
        writeln!(
            file,
            "tiled={tiled}\ndamage_metrics={:?}",
            terminal.damage_metrics()
        )?;
        writeln!(
            file,
            "resizes={resizes}\nviewport_pixels={:?}\nviewport_cells={:?}",
            terminal.viewport().pixels(),
            terminal.viewport().cells()
        )?;
        writeln!(
            file,
            "signal={signal:?}\noutput_cleanup_complete={}",
            abort.is_ok() && close.is_ok() && restore.is_ok()
        )?;
        writeln!(
            file,
            "os_written_bytes={:?}",
            modes.as_ref().and_then(TerminalModes::written_bytes)
        )?;
        writeln!(file, "file_inflight_on_exit={io_inflight}")?;
    }
    if signal.is_none() {
        result?;
        abort?;
        close?;
        restore?;
    }
    Ok(())
}
