#[cfg(not(windows))]
fn main() {
    eprintln!("native-code requires Windows");
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
    use jadix_native::{
        host::NativeRoot,
        presentation::*,
        syntax::{SyntaxResult, SyntaxWorker, TextDocument},
        windows::WindowsHost,
    };
    use std::{
        io::{Read, Write},
        path::{Path, PathBuf},
        time::{Duration, Instant},
    };

    const SOURCE: &str = "// Native code document / 中文😀\r\nfn main() {\r\n    let message = \"Hello Jadix\";\r\n    println!(\"{}\", message);\r\n}\r\n";
    const OUTER: usize = 0;
    const TOOLBAR: usize = 1;
    const SEARCH: usize = 2;
    const FIND: usize = 3;
    const SAVE: usize = 4;
    const CANCEL: usize = 5;
    const EDITOR: usize = 6;
    const SCROLL: usize = 7;
    const PREVIEW: usize = 8;
    const STATUS: usize = 9;
    const RESET: usize = 10;

    struct App {
        root: NativeRoot<WindowsHost>,
        worker: Option<SyntaxWorker>,
        ids: [NodeId; 11],
        preview_ids: std::collections::BTreeMap<(usize, usize), NodeId>,
        document: TextDocument,
        display: String,
        revision: u64,
        composing: bool,
        search: String,
        search_revision: u64,
        syntax: Option<SyntaxResult>,
        status: String,
        viewport: [f32; 2],
        environment: u64,
        directory: PathBuf,
    }

    fn node(
        id: NodeId,
        content: NodeContent,
        width: Option<f32>,
        height: Option<f32>,
    ) -> PresentationNode {
        PresentationNode {
            id,
            descriptor: NodeDescriptor {
                content,
                frame: Rect::default(),
                accessible_role: None,
                accessible_selected: None,
                accessible_expanded: None,
                accessible_description: None,
                accessible_name: None,
                style: style::NodeStyle {
                    width,
                    height,
                    shrink: 0.0,
                    ..Default::default()
                },
            },
            children: vec![],
        }
    }

    fn load(directory: &Path) -> Result<String, String> {
        let path = directory.join("source.json");
        if !path.try_exists().map_err(|error| error.to_string())? {
            return Ok(SOURCE.into());
        }
        let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
        if file.metadata().map_err(|error| error.to_string())?.len() > 512 * 1024 {
            return Err("source checkpoint exceeds budget".into());
        }
        let mut bytes = Vec::new();
        file.take(512 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Checkpoint {
            schema: u32,
            source: String,
        }
        let checkpoint: Checkpoint =
            serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        if checkpoint.schema != 1 || !valid_source(&checkpoint.source) {
            return Err("invalid source checkpoint".into());
        }
        Ok(checkpoint.source)
    }

    fn valid_source(source: &str) -> bool {
        source.len() <= 128 * 1024
            && source.lines().all(|line| line.len() <= 8192)
            && source.bytes().filter(|byte| *byte == b'\n').count() < 4096
            && !source.contains('\0')
    }

    impl App {
        fn new(directory: PathBuf) -> Result<Self, String> {
            let source = load(&directory)?;
            let host = WindowsHost::new("Jadix Native code editor", [960, 720])?;
            let viewport = host.viewport();
            let root = NativeRoot::new(host);
            let owner = root.model().id();
            let mut app = Self {
                root,
                worker: None,
                ids: std::array::from_fn(|_| NodeId::new(owner)),
                preview_ids: std::collections::BTreeMap::new(),
                document: TextDocument::new(source.clone()),
                display: source,
                revision: 0,
                composing: false,
                search: "message".into(),
                search_revision: 0,
                syntax: None,
                status: "Native EDIT with bounded background syntax preview".into(),
                viewport,
                environment: 1,
                directory,
            };
            app.render()?;
            let worker =
                SyntaxWorker::new(app.root.model(), app.ids[EDITOR], app.root.host().waker())
                    .map_err(|error| error.to_string())?;
            app.worker = Some(worker);
            app.request_syntax()?;
            app.root.host().defer_close()?;
            app.root.host().show();
            Ok(app)
        }

        fn request_syntax(&mut self) -> Result<(), String> {
            self.worker
                .as_mut()
                .ok_or("missing syntax worker")?
                .request(self.root.model(), self.document.clone(), "rs", false)
                .map_err(|error| error.to_string())?;
            Ok(())
        }

        fn render(&mut self) -> Result<(), String> {
            let width = (self.viewport[0] - 24.0).max(1.0);
            let mut outer = node(
                self.ids[OUTER],
                NodeContent::Container,
                Some(self.viewport[0]),
                Some(self.viewport[1]),
            );
            outer.descriptor.style.padding = [12.0; 4];
            outer.descriptor.style.gap = 8.0;
            let mut toolbar = node(
                self.ids[TOOLBAR],
                NodeContent::Container,
                Some(width),
                Some(36.0),
            );
            toolbar.descriptor.style.axis = style::Axis::Row;
            toolbar.descriptor.style.gap = 8.0;
            toolbar.children.push(node(
                self.ids[SEARCH],
                NodeContent::Input {
                    max_length: None,
                    value: self.search.clone(),
                    placeholder: "Find source text".into(),
                    enabled: true,
                    password: false,
                    revision: self.search_revision,
                },
                Some(160.0),
                Some(32.0),
            ));
            for (index, label) in [
                (FIND, "Find next"),
                (SAVE, "Save source"),
                (CANCEL, "Cancel syntax"),
                (RESET, "Reset example"),
            ] {
                toolbar.children.push(node(
                    self.ids[index],
                    NodeContent::Button {
                        label: label.into(),
                        enabled: !self.composing,
                    },
                    Some(120.0),
                    Some(32.0),
                ));
            }
            outer.children.push(toolbar);
            outer.children.push(node(
                self.ids[STATUS],
                NodeContent::Text(self.status.clone()),
                Some(width),
                Some(34.0),
            ));
            let mut editor = node(
                self.ids[EDITOR],
                NodeContent::Textarea {
                    max_length: None,
                    value: self.display.clone(),
                    placeholder: "Rust source".into(),
                    enabled: true,
                    revision: self.revision,
                },
                Some(width),
                Some((self.viewport[1] * 0.42).max(80.0)),
            );
            editor.descriptor.accessible_name = Some("Rust source, native multiline editor".into());
            editor.descriptor.style.font_family = "Consolas".into();
            editor.descriptor.style.font_size = 14.0;
            outer.children.push(editor);
            let mut scroll = node(
                self.ids[SCROLL],
                NodeContent::Scroll,
                Some(width),
                Some((self.viewport[1] * 0.35).max(40.0)),
            );
            let mut preview = node(self.ids[PREVIEW], NodeContent::Container, Some(width), None);
            let mut slot = 0usize;
            let mut retained = std::collections::BTreeSet::new();
            if let Some(result) = &self.syntax {
                let mut lines: Vec<Vec<(String, Option<[u8; 4]>)>> = vec![vec![]];
                for span in &result.spans {
                    for piece in span.text.split_inclusive('\n') {
                        if lines.len() > 24 {
                            break;
                        }
                        let text = piece.trim_end_matches(['\n', '\r']).replace('\t', "    ");
                        if !text.is_empty() {
                            lines.last_mut().unwrap().push((text, span.color));
                        }
                        if piece.ends_with('\n') {
                            lines.push(vec![]);
                        }
                    }
                    if lines.len() > 24 {
                        break;
                    }
                }
                for (line_index, line) in lines.into_iter().take(24).enumerate() {
                    if slot + line.len() + 1 > 160 {
                        break;
                    }
                    let key = (line_index, usize::MAX);
                    retained.insert(key);
                    let identifier = *self
                        .preview_ids
                        .entry(key)
                        .or_insert_with(|| NodeId::new(self.ids[OUTER].root));
                    let mut row = node(identifier, NodeContent::Container, Some(width), Some(21.0));
                    slot += 1;
                    row.descriptor.style.axis = style::Axis::Row;
                    for (span_index, (text, color)) in line.into_iter().enumerate() {
                        let key = (line_index, span_index);
                        retained.insert(key);
                        let identifier = *self
                            .preview_ids
                            .entry(key)
                            .or_insert_with(|| NodeId::new(self.ids[OUTER].root));
                        let mut text = node(identifier, NodeContent::Text(text), None, Some(21.0));
                        slot += 1;
                        text.descriptor.style.font_family = "Consolas".into();
                        text.descriptor.style.font_size = 14.0;
                        if let Some(color) = color {
                            text.descriptor.style.foreground = color;
                        }
                        row.children.push(text);
                    }
                    preview.children.push(row);
                }
            }
            scroll.children.push(preview);
            outer.children.push(scroll);
            let mut roots = [outer];
            self.root
                .present(&mut roots, self.viewport, self.environment)
                .map_err(|error| error.to_string())?;
            self.preview_ids.retain(|key, _| retained.contains(key));
            Ok(())
        }

        fn find(&mut self) -> Result<(), String> {
            if self.search.is_empty() {
                self.status = "Enter nonempty search text".into();
                return Ok(());
            }
            let source = self.document.text();
            let selection = self.root.host().selection(self.ids[EDITOR])?;
            let mut units = 0;
            let offset = source
                .char_indices()
                .find_map(|(offset, character)| {
                    if units >= selection[1] {
                        Some(offset)
                    } else {
                        units += character.len_utf16();
                        None
                    }
                })
                .unwrap_or(source.len());
            let found = source[offset..]
                .find(&self.search)
                .map(|index| offset + index)
                .or_else(|| source[..offset].find(&self.search));
            if let Some(found) = found {
                let start = source[..found].encode_utf16().count();
                let end = start + self.search.encode_utf16().count();
                self.root
                    .focus(Some(self.ids[EDITOR]))
                    .map_err(|error| error.to_string())?;
                self.root
                    .host()
                    .select_range(self.ids[EDITOR], [start, end])?;
                self.status = format!("Match at UTF-16 {start}..{end}");
            } else {
                self.status = "No match".into();
            }
            Ok(())
        }

        fn save(&self) -> Result<(), String> {
            if self.composing {
                return Err("Finish composition before saving".into());
            }
            std::fs::create_dir_all(&self.directory).map_err(|error| error.to_string())?;
            let temporary = self.directory.join(format!(
                "source-{}-{}.tmp",
                std::process::id(),
                RootId::new().token()
            ));
            let mut file = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temporary)
                .map_err(|error| error.to_string())?;
            let result = (|| {
                file.write_all(
                    &serde_json::to_vec(
                        &serde_json::json!({"schema": 1, "source": self.document.text()}),
                    )
                    .map_err(|error| error.to_string())?,
                )
                .map_err(|error| error.to_string())?;
                file.sync_all().map_err(|error| error.to_string())?;
                drop(file);
                std::fs::rename(&temporary, self.directory.join("source.json"))
                    .map_err(|error| error.to_string())
            })();
            if result.is_err() {
                let _ = std::fs::remove_file(temporary);
            }
            result
        }

        fn tick(&mut self) -> Result<bool, String> {
            self.root.host().pump();
            let events = self.root.poll_events().map_err(|error| error.to_string())?;
            let mut changed = false;
            let mut analysis = false;
            for event in events {
                match event.payload {
                    EventPayload::Edit {
                        value,
                        revision,
                        composing_utf16,
                        ..
                    } if event.node == self.ids[EDITOR] => {
                        self.revision = revision;
                        if !valid_source(&value) {
                            self.revision = revision.saturating_add(1);
                            self.display = self.document.text().into();
                            self.composing = false;
                            self.status = "Edit rejected: source/line budget".into();
                        } else {
                            self.display = value;
                            self.composing = composing_utf16.is_some();
                            if self.composing {
                                self.worker.as_mut().unwrap().cancel();
                                self.status = "Composing; analysis and save wait for commit".into();
                            } else {
                                self.document = TextDocument::new(self.display.clone());
                                self.syntax = None;
                                analysis = true;
                                self.status = format!(
                                    "Revision {} / {} lines",
                                    self.document.revision(),
                                    self.document.line_count()
                                );
                            }
                        }
                        changed = true;
                    }
                    EventPayload::Edit {
                        value,
                        revision,
                        composing_utf16: None,
                        ..
                    } if event.node == self.ids[SEARCH] => {
                        self.search_revision = revision;
                        if value.len() <= 256 {
                            self.search = value;
                        } else {
                            self.search_revision = revision.saturating_add(1);
                        }
                        changed = true;
                    }
                    EventPayload::Activate if !self.composing => {
                        if event.node == self.ids[FIND] {
                            self.find()?;
                        }
                        if event.node == self.ids[SAVE] {
                            self.status = match self.save() {
                                Ok(()) => "Source saved".into(),
                                Err(error) => format!("Save failed: {error}"),
                            };
                        }
                        if event.node == self.ids[CANCEL] {
                            self.worker.as_mut().unwrap().cancel();
                            self.status = "Syntax cancelled; source retained".into();
                        }
                        if event.node == self.ids[RESET] {
                            self.document = TextDocument::new(SOURCE);
                            self.display = SOURCE.into();
                            self.revision = self.revision.saturating_add(1);
                            self.syntax = None;
                            analysis = true;
                        }
                        changed = true;
                    }
                    _ => (),
                }
            }
            let environment = self.root.host().environment();
            if environment != self.environment {
                self.environment = environment;
                self.viewport = self.root.host().viewport();
                changed = true;
            }
            if changed {
                self.render()?;
            }
            if analysis {
                self.request_syntax()?;
            }
            if let Some(result) = self
                .worker
                .as_mut()
                .unwrap()
                .poll(self.root.model())
                .map_err(|error| error.to_string())?
            {
                self.status = format!(
                    "Syntax revision {} / {} spans / {} bytes",
                    result.document_revision,
                    result.spans.len(),
                    self.document.len()
                );
                self.syntax = Some(result);
                self.render()?;
            }
            if self.root.host().close_requested() {
                match self.save() {
                    Ok(()) => {
                        self.root.host().resolve_close(true)?;
                        return Ok(false);
                    }
                    Err(error) => {
                        self.root.host().resolve_close(false)?;
                        self.status = format!("Close cancelled: {error}");
                        self.render()?;
                    }
                }
            }
            Ok(true)
        }

        fn wait_syntax(&mut self) -> Result<(), String> {
            let deadline = Instant::now() + Duration::from_secs(10);
            while self
                .syntax
                .as_ref()
                .is_none_or(|result| result.document_revision != self.document.revision())
            {
                self.tick()?;
                if Instant::now() >= deadline {
                    return Err("syntax preview timed out".into());
                }
                self.root.host().wait_for_events(50)?;
            }
            Ok(())
        }
    }

    pub fn run() -> Result<(), String> {
        use std::os::windows::process::CommandExt;
        let arguments: Vec<_> = std::env::args().skip(1).collect();
        if let [mode, directory, nonce, report] = arguments.as_slice() {
            if mode == "--recover" {
                let app = App::new(PathBuf::from(directory))?;
                if app.root.host().text(app.ids[EDITOR])? != app.document.text() {
                    return Err("recovered native EDIT differs from saved document".into());
                }
                let report_value = serde_json::json!({"nonce": nonce, "process": std::process::id(), "source": app.document.text(), "native": app.root.host().diagnostics()});
                return report_json(Path::new(report), &report_value);
            }
        }
        let (directory, probe) = match arguments.as_slice() {
            [] => (PathBuf::from("target/native/code-workspace"), None),
            [mode, directory] if mode == "--workspace" => (PathBuf::from(directory), None),
            [mode, nonce, report]
                if mode == "--probe" && !nonce.is_empty() && nonce.len() <= 128 =>
            {
                let report = Path::new(report);
                if report.exists() {
                    return Err("probe output exists".into());
                }
                (
                    report
                        .parent()
                        .ok_or("probe needs parent")?
                        .join("code-workspace"),
                    Some((nonce, report)),
                )
            }
            _ => {
                return Err(
                    "Usage: native-code [--workspace DIR | --probe NONCE NEW_REPORT]".into(),
                );
            }
        };
        let mut app = App::new(directory.clone())?;
        if let Some((nonce, report)) = probe {
            app.wait_syntax()?;
            let identity = app
                .root
                .host()
                .control_identity(app.ids[EDITOR])
                .ok_or("native EDIT missing")?;
            app.search = "中文😀".into();
            app.find()?;
            let selection = app.root.host().selection(app.ids[EDITOR])?;
            if selection[1] - selection[0] != 4 {
                return Err("Unicode search used wrong UTF-16 boundaries".into());
            }
            app.root
                .host()
                .replace_selection(app.ids[EDITOR], &format!("代码 {nonce} 😀"))?;
            app.tick()?;
            app.wait_syntax()?;
            if app.root.host().control_identity(app.ids[EDITOR]) != Some(identity)
                || !app.document.text().contains(nonce)
            {
                return Err("source edit lost identity or text".into());
            }
            let parsed = app.syntax.as_ref().unwrap();
            if parsed
                .spans
                .iter()
                .map(|span| span.text.as_str())
                .collect::<String>()
                != app.document.text()
            {
                return Err("syntax changed source bytes".into());
            }
            app.root.host().undo(app.ids[EDITOR])?;
            app.tick()?;
            app.wait_syntax()?;
            if app.document.text() != SOURCE {
                return Err("native undo failed after syntax publication".into());
            }
            app.root.host().select_range(app.ids[EDITOR], [0, 0])?;
            app.root
                .host()
                .replace_selection(app.ids[EDITOR], &format!("// saved {nonce}\r\n"))?;
            app.tick()?;
            app.wait_syntax()?;
            app.save()?;
            app.save()?;
            let before = app.root.host().stats().commits;
            app.render()?;
            app.render()?;
            if app.root.host().stats().commits != before {
                return Err("idle source rendering committed changes".into());
            }
            let recovery = report.parent().unwrap().join("code-recovery.json");
            let mut child = std::process::Command::new(
                std::env::current_exe().map_err(|error| error.to_string())?,
            )
            .arg("--recover")
            .arg(&directory)
            .arg(nonce)
            .arg(&recovery)
            .creation_flags(0x08000000)
            .spawn()
            .map_err(|error| error.to_string())?;
            let deadline = Instant::now() + Duration::from_secs(15);
            loop {
                if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
                    if !status.success() {
                        return Err(format!("recovery failed: {status}"));
                    }
                    break;
                }
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("source recovery timed out".into());
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            let recovered: serde_json::Value = serde_json::from_slice(
                &std::fs::read(recovery).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
            if recovered["nonce"] != *nonce
                || recovered["source"] != app.document.text()
                || recovered["process"] == std::process::id()
            {
                return Err("source recovery reused current process or wrong checkpoint".into());
            }
            report_json(
                report,
                &serde_json::json!({"schema": 1, "nonce": nonce, "native_controls": true, "application": "native-code", "native_edit_identity_retained": true,
                "unicode_search_utf16": true, "background_syntax_exact_revision": true, "native_undo_retained": true, "idle_native_commits": 0,
                "source": app.document.text(), "syntax_spans": app.syntax.as_ref().unwrap().spans.len(), "independent_process_recovery": recovered,
                "native": app.root.host().diagnostics(), "physical_ime_accepted": false, "inline_syntax_editor": false, "lsp_or_rich_text": false}),
            )?;
            return Ok(());
        }
        while app.tick()? {
            app.root.host().wait_for_events(u32::MAX)?;
        }
        Ok(())
    }

    fn report_json(path: &Path, value: &serde_json::Value) -> Result<(), String> {
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)
            .map_err(|error| error.to_string())?;
        file.write_all(&serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())
    }
}
