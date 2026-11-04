#[cfg(not(windows))]
fn main() {
    eprintln!("native-dock requires Windows");
    std::process::exit(1);
}

#[cfg(windows)]
fn main() {
    if let Err(error) = desktop::run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(windows)]
mod desktop {
    use std::collections::{BTreeMap, BTreeSet};
    use std::io::Write;
    use std::path::{Path, PathBuf};

    use jadix_core::dock::{Bounds, DockNode, DragToken, DropLocation, Edge};
    use jadix_native::{
        presentation::{DragPhase, NodeId, Rect},
        windows::WindowsHost,
    };
    use jadix_native_controls::dock::{DockCommand, DockIntent, DockView, DockWorkspace};

    struct Desktop {
        workspace: DockWorkspace,
        views: BTreeMap<u64, DockView<WindowsHost>>,
        directory: Option<PathBuf>,
        drag: Option<DragToken>,
        pointer_source: Option<NodeId>,
        visible: bool,
    }

    impl Desktop {
        fn new(
            workspace: DockWorkspace,
            directory: Option<PathBuf>,
            visible: bool,
        ) -> Result<Self, String> {
            let mut desktop = Self {
                workspace,
                views: BTreeMap::new(),
                directory,
                drag: None,
                pointer_source: None,
                visible,
            };
            desktop.sync()?;
            Ok(desktop)
        }

        fn sync(&mut self) -> Result<(), String> {
            let wanted: BTreeSet<_> = self
                .workspace
                .layout
                .surfaces()
                .iter()
                .map(|surface| surface.key)
                .collect();
            self.views.retain(|key, _| wanted.contains(key));
            for surface in self.workspace.layout.surfaces() {
                if let std::collections::btree_map::Entry::Vacant(entry) =
                    self.views.entry(surface.key)
                {
                    let host = WindowsHost::new(
                        &format!("Jadix Native Dock · window {}", surface.key),
                        [surface.bounds.size[0] as u32, surface.bounds.size[1] as u32],
                    )?;
                    host.set_window_frame(Rect {
                        x: surface.bounds.origin[0],
                        y: surface.bounds.origin[1],
                        width: surface.bounds.size[0],
                        height: surface.bounds.size[1],
                    })?;
                    host.defer_close()?;
                    entry.insert(DockView::new(host, surface.key));
                }
            }
            for view in self.views.values_mut() {
                view.render(
                    &self.workspace,
                    view.root.host().viewport(),
                    view.root.host().environment(),
                )?;
                if self.visible {
                    view.root.host().show();
                }
            }
            Ok(())
        }

        fn main_stack(&self) -> Result<u64, String> {
            self.workspace
                .layout
                .panes()
                .iter()
                .find_map(|pane| {
                    self.workspace
                        .layout
                        .stack_of(pane.key)
                        .filter(|(surface, _)| *surface == 1)
                        .map(|(_, stack)| stack)
                })
                .ok_or_else(|| String::from("missing main dock stack"))
        }

        fn capture_bounds(&mut self) -> Result<(), String> {
            for (surface, view) in &self.views {
                if !view.root.host().is_closed() {
                    let frame = view.root.host().window_frame()?;
                    if frame.width < 160.0 || frame.height < 160.0 {
                        continue;
                    }
                    self.workspace
                        .layout
                        .set_bounds(
                            *surface,
                            Bounds {
                                origin: [frame.x, frame.y],
                                size: [frame.width, frame.height],
                            },
                        )
                        .map_err(|error| error.to_string())?;
                }
            }
            Ok(())
        }

        fn rejoin(&mut self, surface: u64, stack: u64) -> Result<(), String> {
            let current = self.workspace.current;
            let selected = self
                .workspace
                .layout
                .stack_of(current)
                .filter(|(owner, target)| *owner == surface || *target == stack)
                .map(|_| current);
            self.workspace
                .layout
                .dock_surface_with_active(surface, stack, selected)
                .map_err(|error| error.to_string())?;
            Ok(())
        }

        fn command(&mut self, intent: DockIntent) -> Result<(), String> {
            let DockIntent { pane, command } = intent;
            if let DockCommand::Drag {
                node,
                phase,
                screen_position,
            } = command
            {
                return self.pointer(pane, node, phase, screen_position);
            }
            self.workspace.current = pane;
            if self.workspace.composing()
                && !matches!(
                    command,
                    DockCommand::Focus | DockCommand::Save | DockCommand::CancelDrag
                )
            {
                self.workspace.message =
                    "Finish text composition before changing the layout".into();
                return Ok(());
            }
            let pane = self.workspace.current;
            match command {
                DockCommand::Focus => return Ok(()),
                DockCommand::Drag { .. } => unreachable!(),
                DockCommand::Activate(pane) => {
                    self.workspace
                        .layout
                        .activate(pane)
                        .map_err(|error| error.to_string())?;
                    self.workspace.current = pane;
                }
                DockCommand::Split => {
                    let (_, stack) = self.workspace.layout.stack_of(pane).ok_or("unknown pane")?;
                    self.workspace
                        .layout
                        .relocate(
                            pane,
                            DropLocation::Split {
                                stack,
                                edge: Edge::Right,
                            },
                        )
                        .map_err(|error| error.to_string())?;
                }
                DockCommand::Float => {
                    self.workspace
                        .layout
                        .relocate(
                            pane,
                            DropLocation::Floating {
                                bounds: Bounds {
                                    origin: [240.0, 160.0],
                                    size: [720.0, 520.0],
                                },
                            },
                        )
                        .map_err(|error| error.to_string())?;
                }
                DockCommand::Rejoin => {
                    let stack = self.main_stack()?;
                    let floats: Vec<_> = self
                        .workspace
                        .layout
                        .surfaces()
                        .iter()
                        .filter(|surface| surface.key != 1)
                        .map(|surface| surface.key)
                        .collect();
                    for surface in floats {
                        self.rejoin(surface, stack)?;
                    }
                }
                DockCommand::Undo => {
                    self.workspace
                        .layout
                        .undo()
                        .map_err(|error| error.to_string())?;
                }
                DockCommand::Redo => {
                    self.workspace
                        .layout
                        .redo()
                        .map_err(|error| error.to_string())?;
                }
                DockCommand::Save => {
                    self.capture_bounds()?;
                    let directory = self
                        .directory
                        .as_ref()
                        .ok_or("Start with --workspace DIRECTORY to enable persistence")?;
                    self.workspace.save(directory)?;
                    self.workspace.message = "Workspace saved".into();
                    return Ok(());
                }
                DockCommand::BeginDrag => {
                    self.drag = Some(
                        self.workspace
                            .layout
                            .begin_drag(pane)
                            .map_err(|error| error.to_string())?,
                    );
                }
                DockCommand::CancelDrag => {
                    let token = self.drag.take().ok_or("no active drag")?;
                    self.workspace
                        .layout
                        .cancel_drag(&token)
                        .map_err(|error| error.to_string())?;
                }
                DockCommand::DropRight => {
                    let token = self.drag.as_ref().ok_or("no active drag")?;
                    self.workspace
                        .layout
                        .drop_drag(
                            token,
                            DropLocation::Split {
                                stack: self.main_stack()?,
                                edge: Edge::Right,
                            },
                        )
                        .map_err(|error| error.to_string())?;
                    self.drag = None;
                }
            }
            if self.workspace.layout.dragging().is_none() {
                self.drag = None;
            }
            self.workspace.message = format!("{command:?}");
            Ok(())
        }

        fn drop_location(&self, position: [f64; 2]) -> Result<DropLocation, String> {
            let contains = |frame: Rect| {
                position[0] >= frame.x
                    && position[1] >= frame.y
                    && position[0] < frame.x + frame.width
                    && position[1] < frame.y + frame.height
            };
            for (key, view) in &self.views {
                if !view.root.host().owns_screen_point(position) {
                    continue;
                }
                let surface = self
                    .workspace
                    .layout
                    .surfaces()
                    .iter()
                    .find(|surface| surface.key == *key)
                    .ok_or("missing surface")?;
                let mut pending = vec![&surface.root];
                while let Some(node) = pending.pop() {
                    match node {
                        DockNode::Split { first, second, .. } => {
                            pending.push(second);
                            pending.push(first);
                        }
                        DockNode::Stack { key, panes, .. } => {
                            let container = view
                                .layout_container(*key)
                                .ok_or("missing stack container")?;
                            let frame = view.root.host().control_screen_frame(container)?;
                            if !contains(frame) {
                                continue;
                            }
                            for (index, pane) in panes.iter().enumerate() {
                                let tab = view.tab(*pane).ok_or("missing tab")?;
                                let tab_frame = view.root.host().control_screen_frame(tab)?;
                                if contains(tab_frame) {
                                    return Ok(DropLocation::Tab {
                                        stack: *key,
                                        index: index
                                            + usize::from(
                                                position[0] > tab_frame.x + tab_frame.width * 0.5,
                                            ),
                                    });
                                }
                            }
                            let horizontal = (position[0] - frame.x) / frame.width;
                            let vertical = (position[1] - frame.y) / frame.height;
                            if (0.25..=0.75).contains(&horizontal)
                                && (0.25..=0.75).contains(&vertical)
                            {
                                return Ok(DropLocation::Tab {
                                    stack: *key,
                                    index: panes.len(),
                                });
                            }
                            let edge = [
                                (horizontal, Edge::Left),
                                (1.0 - horizontal, Edge::Right),
                                (vertical, Edge::Top),
                                (1.0 - vertical, Edge::Bottom),
                            ]
                            .into_iter()
                            .min_by(|first, second| first.0.total_cmp(&second.0))
                            .ok_or("missing drop edge")?
                            .1;
                            return Ok(DropLocation::Split { stack: *key, edge });
                        }
                    }
                }
            }
            Ok(DropLocation::Floating {
                bounds: Bounds {
                    origin: [position[0] - 80.0, position[1] - 24.0],
                    size: [720.0, 520.0],
                },
            })
        }

        fn pointer(
            &mut self,
            pane: u64,
            node: NodeId,
            phase: DragPhase,
            position: [f64; 2],
        ) -> Result<(), String> {
            if phase == DragPhase::Started {
                if self.workspace.composing() {
                    for view in self.views.values() {
                        view.root.host().cancel_pointer_drag();
                    }
                    return Err(String::from("Finish composition before dragging a tab"));
                }
                if let Some(token) = self.drag.take() {
                    let _ = self.workspace.layout.cancel_drag(&token);
                }
                self.workspace.current = pane;
                self.workspace
                    .layout
                    .activate(pane)
                    .map_err(|error| error.to_string())?;
                self.drag = Some(
                    self.workspace
                        .layout
                        .begin_drag(pane)
                        .map_err(|error| error.to_string())?,
                );
                self.pointer_source = Some(node);
                self.workspace.message = format!("Dragging pane {pane}; Escape cancels");
                return Ok(());
            }
            if self.pointer_source != Some(node) {
                return Ok(());
            }
            match phase {
                DragPhase::Moved => {
                    self.workspace.message = format!("Drop: {:?}", self.drop_location(position)?);
                }
                DragPhase::Ended => {
                    let token = self.drag.take().ok_or("missing drag transaction")?;
                    self.pointer_source = None;
                    let destination = self.drop_location(position);
                    let result = destination.and_then(|location| {
                        self.workspace
                            .layout
                            .drop_drag(&token, location)
                            .map_err(|error| error.to_string())
                    });
                    if result.is_err() {
                        let _ = self.workspace.layout.cancel_drag(&token);
                    }
                    result?;
                    self.workspace.message = format!("Dropped pane {pane}");
                }
                DragPhase::Cancelled => {
                    if let Some(token) = self.drag.take() {
                        let _ = self.workspace.layout.cancel_drag(&token);
                    }
                    self.pointer_source = None;
                    self.workspace.message = "Pointer drag cancelled".into();
                }
                DragPhase::Started => {}
            }
            Ok(())
        }

        fn tick(&mut self) -> Result<bool, String> {
            for view in self.views.values() {
                view.root.host().pump();
            }
            if self
                .views
                .get(&1)
                .is_some_and(|view| view.root.host().is_closed())
            {
                return Ok(false);
            }
            let closed: Vec<_> = self
                .views
                .iter()
                .filter(|(_, view)| view.root.host().is_closed())
                .map(|(key, _)| *key)
                .collect();
            let mut commands = Vec::new();
            for view in self
                .views
                .values_mut()
                .filter(|view| !view.root.host().is_closed())
            {
                commands.extend(view.poll(&mut self.workspace)?);
            }
            let requested: Vec<_> = self
                .views
                .iter()
                .filter(|(_, view)| view.root.host().close_requested())
                .map(|(key, _)| *key)
                .collect();
            if !requested.is_empty() && self.workspace.composing() {
                for key in &requested {
                    self.views[key].root.host().resolve_close(false)?;
                }
                self.workspace.message = "Finish text composition before closing a window".into();
            } else if requested.contains(&1) {
                let saved = self.capture_bounds().and_then(|_| {
                    self.directory
                        .as_ref()
                        .map(|directory| self.workspace.save(directory))
                        .transpose()
                });
                match saved {
                    Ok(_) => {
                        self.views[&1].root.host().resolve_close(true)?;
                        return Ok(false);
                    }
                    Err(error) => {
                        for key in &requested {
                            self.views[key].root.host().resolve_close(false)?;
                        }
                        self.workspace.message = format!("Close cancelled: {error}");
                    }
                }
            } else {
                for key in requested {
                    self.rejoin(key, self.main_stack()?)?;
                    self.views[&key].root.host().resolve_close(true)?;
                }
            }
            for surface in closed {
                self.rejoin(surface, self.main_stack()?)?;
            }
            for command in commands {
                if let Err(error) = self.command(command) {
                    self.workspace.message = error;
                }
            }
            self.sync()?;
            Ok(true)
        }

        fn activate_action(&mut self, surface: u64, index: usize) -> Result<(), String> {
            let view = self.views.get(&surface).ok_or("missing native window")?;
            view.root
                .host()
                .activate(view.action(index).ok_or("missing action")?)?;
            if !self.tick()? {
                return Err(String::from("unexpected root closure"));
            }
            Ok(())
        }

        fn active_editor(
            &self,
            pane: u64,
        ) -> Result<(&DockView<WindowsHost>, jadix_native::presentation::NodeId), String> {
            let surface = self
                .workspace
                .layout
                .stack_of(pane)
                .ok_or("unknown pane")?
                .0;
            let view = self.views.get(&surface).ok_or("missing native window")?;
            Ok((view, view.editor(pane).ok_or("missing native editor")?))
        }
    }

    fn write_new(path: &Path, value: &serde_json::Value) -> Result<(), String> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|error| error.to_string())?;
        writeln!(
            file,
            "{}",
            serde_json::to_string_pretty(value).map_err(|error| error.to_string())?
        )
        .map_err(|error| error.to_string())
    }

    fn recovery(directory: &Path, nonce: &str, report: &Path) -> Result<(), String> {
        let desktop = Desktop::new(
            DockWorkspace::load(directory)?,
            Some(directory.to_owned()),
            false,
        )?;
        let expected = format!("{nonce} / 恢复 😀");
        let (view, editor) = desktop.active_editor(2)?;
        if desktop.views.len() != 2
            || desktop.workspace.value(2) != Some(expected.as_str())
            || view.root.host().text(editor)? != expected
        {
            return Err(String::from(
                "independent process did not recover the floating native editor",
            ));
        }
        let windows: Vec<_> = desktop
            .views
            .values()
            .map(|view| view.root.host().diagnostics())
            .collect();
        write_new(
            report,
            &serde_json::json!({"nonce": nonce, "process": std::process::id(), "windows": windows, "native_controls": true, "recovered_text": expected, "checkpoint_bytes": std::fs::metadata(directory.join("workspace.json")).map_err(|error| error.to_string())?.len()}),
        )
    }

    fn probe(nonce: &str, output: &Path) -> Result<(), String> {
        if output.exists() {
            return Err(String::from("probe output already exists"));
        }
        let directory = output.with_extension("workspace");
        std::fs::create_dir(&directory).map_err(|error| error.to_string())?;
        let mut desktop = Desktop::new(DockWorkspace::demo()?, Some(directory.clone()), false)?;
        let initial = desktop.views.get(&1).ok_or("missing main window")?;
        if !initial.pointer_drag_available() {
            return Err(String::from("native pointer drag binding is unavailable"));
        }
        initial.root.host().show();
        let tab = initial.tab(4).ok_or("missing pointer source tab")?;
        let tab_window = initial
            .root
            .host()
            .control_identity(tab)
            .ok_or("missing native pointer source")?
            as windows_sys::Win32::Foundation::HWND;
        use windows_sys::Win32::UI::{
            Input::KeyboardAndMouse::GetCapture,
            WindowsAndMessaging::{SendMessageW, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE},
        };
        let point = |horizontal: u16, vertical: u16| {
            (u32::from(horizontal) | u32::from(vertical) << 16) as isize
        };
        unsafe {
            SendMessageW(tab_window, WM_LBUTTONDOWN, 1, point(2, 2));
            SendMessageW(tab_window, WM_MOUSEMOVE, 1, point(48, 12));
        }
        if unsafe { GetCapture() } != tab_window {
            return Err(String::from("native pointer capture was not established"));
        }
        unsafe {
            SendMessageW(tab_window, WM_LBUTTONUP, 0, point(2000, 1200));
        }
        if unsafe { GetCapture() } == tab_window {
            return Err(String::from("native pointer release retained capture"));
        }
        desktop.tick()?;
        let pointer_surface = desktop
            .workspace
            .layout
            .stack_of(4)
            .ok_or("pointer-moved pane missing")?
            .0;
        if pointer_surface == 1 || desktop.views.len() != 2 {
            return Err(String::from(
                "native pointer drop did not create its floating window",
            ));
        }
        desktop.activate_action(pointer_surface, 2)?;
        let main = desktop.views.get(&1).ok_or("missing main window")?;
        main.root
            .host()
            .activate(main.tab(2).ok_or("missing native tab")?)?;
        desktop.tick()?;
        let (view, editor) = desktop.active_editor(2)?;
        let identity = view
            .root
            .host()
            .control_identity(editor)
            .ok_or("missing native edit identity")?;
        let original = view.root.host().text(editor)?;
        let text = format!("{nonce} / 恢复 😀");
        view.root
            .host()
            .select_range(editor, [0, original.encode_utf16().count()])?;
        view.root.host().replace_selection(editor, &text)?;
        desktop.tick()?;
        desktop.activate_action(1, 0)?;
        let (view, after) = desktop.active_editor(2)?;
        if editor != after
            || view.root.host().control_identity(after) != Some(identity)
            || !view.root.host().undo(after)?
            || view.root.host().text(after)? != original
        {
            return Err(String::from(
                "split lost the editor HWND or native undo history",
            ));
        }
        desktop.tick()?;
        let (view, editor) = desktop.active_editor(2)?;
        view.root
            .host()
            .select_range(editor, [0, original.encode_utf16().count()])?;
        view.root.host().replace_selection(editor, &text)?;
        desktop.tick()?;
        desktop.activate_action(1, 1)?;
        let (surface, _) = desktop
            .workspace
            .layout
            .stack_of(2)
            .ok_or("floating pane missing")?;
        let (view, editor) = desktop.active_editor(2)?;
        if surface == 1 || desktop.views.len() != 2 || view.root.host().text(editor)? != text {
            return Err(String::from(
                "floating window did not receive the business document",
            ));
        }
        let lifetime = view.root.model().lifetime().clone();
        let bounds = view.root.host().window_frame()?;
        if bounds.x != 240.0 || bounds.y != 160.0 || bounds.width != 720.0 || bounds.height != 520.0
        {
            return Err(format!("native window placement mismatch: {bounds:?}"));
        }
        let before_cancel = desktop
            .workspace
            .layout
            .to_json()
            .map_err(|error| error.to_string())?;
        desktop.activate_action(surface, 6)?;
        desktop.activate_action(surface, 7)?;
        if desktop.workspace.layout.dragging().is_some()
            || desktop
                .workspace
                .layout
                .to_json()
                .map_err(|error| error.to_string())?
                != before_cancel
        {
            return Err(String::from(
                "cancelled docking transaction changed the layout",
            ));
        }
        desktop.activate_action(surface, 5)?;
        desktop.workspace.save(&directory)?;
        let child_report = directory.join("recovered.json");
        let mut command =
            std::process::Command::new(std::env::current_exe().map_err(|error| error.to_string())?);
        command
            .args(["--recover-check"])
            .arg(&directory)
            .arg(nonce)
            .arg(&child_report)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::inherit());
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
        let mut child = command.spawn().map_err(|error| error.to_string())?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        let status = loop {
            if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
                break status;
            }
            if std::time::Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err(String::from("workspace recovery child timed out"));
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        if !status.success() {
            return Err(format!("workspace recovery child failed: {status}"));
        }
        let recovery: serde_json::Value = serde_json::from_slice(
            &std::fs::read(&child_report).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        if recovery["nonce"] != nonce || recovery["process"] == std::process::id() {
            return Err(String::from(
                "recovery evidence was not produced by the new process",
            ));
        }
        desktop.activate_action(surface, 2)?;
        let (view, editor) = desktop.active_editor(2)?;
        if desktop.views.len() != 1
            || !lifetime.is_disposed()
            || view.root.host().text(editor)? != text
        {
            return Err(String::from(
                "redocking lost the document or retained the old floating root",
            ));
        }
        desktop.activate_action(1, 3)?;
        if desktop.views.len() != 2 {
            return Err(String::from(
                "layout undo did not restore the floating window",
            ));
        }
        desktop.activate_action(1, 4)?;
        if desktop.views.len() != 1 {
            return Err(String::from("layout redo did not redock the window"));
        }
        let before_idle: Vec<_> = desktop
            .views
            .values()
            .map(|view| view.root.host().stats().commits)
            .collect();
        desktop.sync()?;
        let after_idle: Vec<_> = desktop
            .views
            .values()
            .map(|view| view.root.host().stats().commits)
            .collect();
        if before_idle != after_idle {
            return Err(String::from("idle dock emitted another native commit"));
        }
        let (view, editor) = desktop.active_editor(2)?;
        let final_text = format!("{nonce} / 最后一次编辑 😀");
        view.root
            .host()
            .select_range(editor, [0, text.encode_utf16().count()])?;
        view.root.host().replace_selection(editor, &final_text)?;
        view.root.host().request_close()?;
        if desktop.tick()? || DockWorkspace::load(&directory)?.value(2) != Some(final_text.as_str())
        {
            return Err(String::from(
                "close did not drain and save the last native edit",
            ));
        }
        let report = serde_json::json!({"schema": 1, "nonce": nonce, "native_controls": true, "process": std::process::id(), "retained_editor_on_split": true, "native_undo_retained": true,
            "floating_window_and_rejoin": true, "floating_root_disposed": lifetime.is_disposed(), "layout_undo_redo": true, "cancelled_drag_unchanged": true,
            "independent_process_recovery": recovery, "native_window_bounds": [bounds.x, bounds.y, bounds.width, bounds.height], "idle_native_commits": 0,
            "native": desktop.views.values().map(|view| view.root.host().diagnostics()).collect::<Vec<_>>(), "workspace": directory,
            "close_drained_and_saved_last_edit": true, "programmatic_pointer_capture_and_drop": true, "cross_window_editor_remounted": true, "cross_window_native_undo_preserved": false, "pointer_drag_accepted": false, "synthetic_native_messages": true, "screen_reader_accepted": false});
        write_new(output, &report)?;
        println!(
            "{}",
            serde_json::json!({"output": output, "native_controls": true, "independent_process_recovery": true})
        );
        Ok(())
    }

    pub fn run() -> Result<(), String> {
        let arguments: Vec<_> = std::env::args().skip(1).collect();
        let directory =
            match arguments.as_slice() {
                [mode, nonce, output]
                    if mode == "--probe" && !nonce.is_empty() && nonce.len() <= 128 =>
                {
                    return probe(nonce, Path::new(output))
                }
                [mode, directory, nonce, output]
                    if mode == "--recover-check" && !nonce.is_empty() && nonce.len() <= 128 =>
                {
                    return recovery(Path::new(directory), nonce, Path::new(output))
                }
                [mode, directory] if mode == "--workspace" => Some(PathBuf::from(directory)),
                [] => None,
                _ => return Err(String::from(
                    "Usage: native-dock [--workspace DIRECTORY | --probe NONCE NEW_REPORT_JSON]",
                )),
            };
        let workspace = if let Some(directory) = directory
            .as_ref()
            .filter(|directory| directory.join("workspace.json").exists())
        {
            DockWorkspace::load(directory)?
        } else {
            DockWorkspace::demo()?
        };
        let mut desktop = Desktop::new(workspace, directory, true)?;
        while desktop.tick()? {
            desktop
                .views
                .get(&1)
                .ok_or("missing main window")?
                .root
                .host()
                .wait_for_events(u32::MAX)?;
        }
        Ok(())
    }
}
