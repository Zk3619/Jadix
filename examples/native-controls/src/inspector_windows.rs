#[cfg(not(windows))]
fn main() {
    eprintln!("native-inspector requires Windows");
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
        presentation::{style::NodeStyle, *},
        windows::WindowsHost,
    };
    use jadix_native_controls::NativeApp;
    use serde_json::{json, Value};
    use std::io::Write;

    struct Inspector {
        root: NativeRoot<WindowsHost>,
        container: NodeId,
        selector: NodeId,
        privacy: NodeId,
        refresh: NodeId,
        scroll: NodeId,
        detail: NodeId,
        selected: Option<NodeId>,
        include_text: bool,
        requested: Option<String>,
        receipt: Value,
        revision: u64,
        environment: u64,
    }

    fn nodes(root: &NativeRoot<WindowsHost>) -> Vec<NodeId> {
        let mut pending: Vec<_> = root.model().roots().iter().rev().copied().collect();
        let mut result = Vec::new();
        while let Some(identifier) = pending.pop() {
            result.push(identifier);
            if let Some(node) = root.model().node(identifier) {
                pending.extend(node.children.iter().rev().copied());
            }
        }
        result
    }

    fn node(identifier: NodeId, content: NodeContent, height: f32) -> PresentationNode {
        PresentationNode {
            id: identifier,
            descriptor: NodeDescriptor {
                content,
                frame: Rect::default(),
                accessible_name: None,
                accessible_role: None,
                accessible_selected: None,
                accessible_expanded: None,
                style: NodeStyle {
                    height: Some(height),
                    shrink: 0.0,
                    ..Default::default()
                },
            },
            children: vec![],
        }
    }

    impl Inspector {
        fn new() -> Result<Self, String> {
            let root = NativeRoot::new(WindowsHost::new("Jadix Native Inspector", [720, 720])?);
            let identifier = root.model().id();
            Ok(Self {
                root,
                container: NodeId::new(identifier),
                selector: NodeId::new(identifier),
                privacy: NodeId::new(identifier),
                refresh: NodeId::new(identifier),
                scroll: NodeId::new(identifier),
                detail: NodeId::new(identifier),
                selected: None,
                include_text: false,
                requested: None,
                receipt: Value::Null,
                revision: 0,
                environment: 0,
            })
        }

        fn poll(&mut self) -> Result<bool, String> {
            let mut changed = false;
            for event in self.root.host_mut().take_events()? {
                self.root.event(&event).map_err(|error| error.to_string())?;
                match event.payload {
                    EventPayload::Select { value, .. } if event.node == self.selector => {
                        self.requested = Some(value);
                        changed = true;
                    }
                    EventPayload::Toggle { checked, .. } if event.node == self.privacy => {
                        self.include_text = checked;
                        changed = true;
                    }
                    EventPayload::Activate if event.node == self.refresh => changed = true,
                    _ => {}
                }
            }
            Ok(changed)
        }

        fn update(
            &mut self,
            source: &NativeRoot<WindowsHost>,
            force: bool,
        ) -> Result<bool, String> {
            let inspection = source.inspection(self.include_text);
            let available = nodes(source);
            let requested = self.requested.take();
            self.selected = available
                .iter()
                .take(256)
                .copied()
                .find(|identifier| {
                    requested
                        .as_ref()
                        .is_some_and(|token| identifier.token() == *token)
                })
                .or_else(|| {
                    self.selected
                        .filter(|selected| available.iter().take(256).any(|node| node == selected))
                })
                .or_else(|| {
                    available.iter().take(256).copied().find(|identifier| {
                        source.model().node(*identifier).is_some_and(|node| {
                            matches!(
                                node.descriptor.content,
                                NodeContent::Input {
                                    password: false,
                                    ..
                                }
                            )
                        })
                    })
                })
                .or_else(|| available.first().copied());
            let selected = self.selected.and_then(|identifier| {
                inspection["nodes"]
                    .as_array()?
                    .iter()
                    .find(|entry| entry["id"] == identifier.token())
                    .cloned()
            });
            let native = self
                .selected
                .map(|identifier| source.host().inspect_control(identifier))
                .transpose()?;
            let receipt = json!({"protocol": 1, "root": inspection["root"], "revision": inspection["revision"],
                "selected": selected, "native_control": native, "layout": inspection["layout"],
                "last_commit": inspection["last_commit"], "resources": inspection["resources"],
                "limits": inspection["limits"], "host": source.host().diagnostics(),
                "node_count": available.len(), "selectable_nodes": available.len().min(256),
                "omitted_from_selector": available.len().saturating_sub(256),
                "include_text": self.include_text, "assistive_technology_accepted": false,
                "authority": "presentation metadata plus same-thread Win32 HWND observation"});
            let environment = self.root.host().environment();
            if !force && self.receipt == receipt && environment == self.environment {
                return Ok(false);
            }
            self.receipt = receipt;
            self.environment = environment;
            self.revision = self
                .revision
                .checked_add(1)
                .ok_or("inspector revision exhausted")?;
            let options = available
                .iter()
                .take(256)
                .map(|identifier| SelectOption {
                    value: identifier.token(),
                    label: format!(
                        "{} · {:?}",
                        identifier.token(),
                        source
                            .model()
                            .node(*identifier)
                            .map(|node| node.descriptor.content.kind())
                    ),
                    enabled: true,
                })
                .collect();
            let viewport = self.root.host().viewport();
            let mut tree = node(self.container, NodeContent::Container, viewport[1]);
            tree.descriptor.style.padding = [12.0; 4];
            tree.descriptor.style.gap = 8.0;
            tree.children.push(node(
                self.selector,
                NodeContent::Select {
                    options,
                    value: self.selected.map(NodeId::token).unwrap_or_default(),
                    placeholder: "Choose a mounted node".into(),
                    enabled: !available.is_empty(),
                    revision: self.revision,
                },
                40.0,
            ));
            tree.children.push(node(
                self.privacy,
                NodeContent::Checkbox {
                    label: "Include body text (password values remain hidden)".into(),
                    checked: self.include_text,
                    enabled: true,
                    revision: self.revision,
                },
                36.0,
            ));
            tree.children.push(node(
                self.refresh,
                NodeContent::Button {
                    label: "Refresh native observation".into(),
                    enabled: true,
                },
                36.0,
            ));
            let mut scroll = node(
                self.scroll,
                NodeContent::Scroll,
                (viewport[1] - 160.0).max(40.0),
            );
            let text =
                serde_json::to_string_pretty(&self.receipt).map_err(|error| error.to_string())?;
            let height = (text.lines().count() as f32 * 22.0).max(40.0);
            scroll
                .children
                .push(node(self.detail, NodeContent::Text(text), height));
            tree.children.push(scroll);
            self.root
                .present(&mut [tree], viewport, self.environment)
                .map_err(|error| error.to_string())?;
            Ok(true)
        }
    }

    pub fn run() -> Result<(), String> {
        let arguments: Vec<_> = std::env::args().skip(1).collect();
        let probe = match arguments.as_slice() {
            [] => None,
            [mode, nonce, output]
                if mode == "--probe" && !nonce.is_empty() && nonce.len() <= 128 =>
            {
                Some((nonce, std::path::Path::new(output)))
            }
            _ => return Err("Usage: native-inspector [--probe NONCE NEW_REPORT_JSON]".into()),
        };
        if probe.is_some_and(|(_, output)| output.exists()) {
            return Err("probe output already exists".into());
        }
        let host = WindowsHost::new("Native application · inspect this window", [620, 680])?;
        let viewport = host.viewport();
        let mut app = NativeApp::new(host, viewport, 0, "Inspector draft 中文😀".into())?;
        let mut inspector = Inspector::new()?;
        inspector.update(&app.root, true)?;
        if let Some((nonce, output)) = probe {
            let editor = inspector.selected.ok_or("source editor missing")?;
            let identity = app
                .root
                .host()
                .control_identity(editor)
                .ok_or("source HWND missing")?;
            if !inspector.receipt["selected"]["descriptor"]["text"].is_null() {
                return Err("body text was exposed before opt-in".into());
            }
            let initial_revision = inspector.receipt["revision"].clone();
            let value = format!("{nonce} inspector 中文😀");
            let before = app.root.host().text(editor)?;
            app.root
                .host()
                .select_range(editor, [0, before.encode_utf16().count()])?;
            app.root.host().replace_selection(editor, &value)?;
            app.poll_events()?;
            if !inspector.update(&app.root, false)?
                || inspector.receipt["revision"] == initial_revision
            {
                return Err("inspector did not follow the source edit".into());
            }
            app.root.host().activate(
                nodes(&app.root)
                    .into_iter()
                    .find(|identifier| {
                        app.root.model().node(*identifier).is_some_and(|node| {
                            matches!(node.descriptor.content, NodeContent::Button { .. })
                        })
                    })
                    .ok_or("source button missing")?,
            )?;
            app.poll_events()?;
            inspector.update(&app.root, false)?;
            inspector.root.host().activate(inspector.privacy)?;
            if !inspector.poll()? {
                return Err("inspector privacy control did not dispatch".into());
            }
            inspector.update(&app.root, true)?;
            if inspector.receipt["selected"]["descriptor"]["text"] != value
                || app.root.host().control_identity(editor) != Some(identity)
                || inspector.receipt["native_control"]["class_name"] != "Edit"
                || inspector.update(&app.root, false)?
            {
                return Err("live detail, HWND identity or idle suppression failed".into());
            }
            let button = nodes(&app.root)
                .into_iter()
                .find(|identifier| {
                    app.root.model().node(*identifier).is_some_and(|node| {
                        matches!(node.descriptor.content, NodeContent::Button { .. })
                    })
                })
                .ok_or("source button missing")?;
            inspector
                .root
                .host()
                .select_value(inspector.selector, &button.token())?;
            if !inspector.poll()? {
                return Err("inspector selector did not dispatch".into());
            }
            inspector.update(&app.root, true)?;
            if inspector.selected != Some(button)
                || inspector.receipt["native_control"]["class_name"] != "Button"
            {
                return Err("selected node did not correlate with its actual HWND".into());
            }
            let report = json!({"schema": 1, "nonce": nonce, "native_controls": true,
                "live_edit_observed": true, "body_text_opt_in": true, "retained_source_hwnd": true,
                "native_selector_observed": true, "idle_commit_suppressed": true,
                "inspection": inspector.receipt, "synthetic_native_messages": true,
                "assistive_technology_accepted": false});
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(output)
                .map_err(|error| error.to_string())?;
            writeln!(
                file,
                "{}",
                serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?
            )
            .map_err(|error| error.to_string())?;
            println!("{report}");
            return Ok(());
        }
        app.root.host().show();
        inspector.root.host().show();
        let mut environment = app.root.host().environment();
        loop {
            app.root.host().pump();
            if app.root.host().is_closed() || inspector.root.host().is_closed() {
                return Ok(());
            }
            app.poll_events()?;
            let next = app.root.host().environment();
            if environment != next {
                environment = next;
                app.resize(app.root.host().viewport(), environment)?;
            }
            let changed = inspector.poll()?;
            inspector.update(&app.root, changed)?;
            app.root.host().wait_for_events(u32::MAX)?;
        }
    }
}
