#[cfg(not(windows))]
fn main() {
    eprintln!("native-radio requires Windows");
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
    use jadix_components::{authoring::native::ElementProjection, prelude::*};
    use jadix_core::OwnerHandle;
    use jadix_native::{
        host::{NativeRoot, PlatformHost},
        presentation::*,
        windows::WindowsHost,
    };
    use std::io::Write;

    struct App {
        _owner: OwnerHandle,
        element: Element,
        selected: Signal<String>,
        projection: ElementProjection,
        root: NativeRoot<WindowsHost>,
        tree: PresentationNode,
        viewport: [f32; 2],
        environment: u64,
    }

    fn find(node: &PresentationNode, predicate: &impl Fn(&NodeContent) -> bool) -> Option<NodeId> {
        if predicate(&node.descriptor.content) {
            return Some(node.id);
        }
        node.children
            .iter()
            .find_map(|child| find(child, predicate))
    }

    impl App {
        fn new() -> Result<Self, String> {
            let (owner, (element, selected)) = create_root(|| {
                let selected = create_signal("standard".to_string());
                let content = ui! {
                    <RadioGroup id="native-delivery" value={selected}>
                        <RadioGroupItem value="standard">"Standard"</RadioGroupItem>
                        <RadioGroupItem value="blocked" disabled={true}>"Disabled"</RadioGroupItem>
                        <RadioGroupItem value="express">"Express"</RadioGroupItem>
                    </RadioGroup>
                };
                (Element::new().p(12.0).child(content), selected)
            });
            let host = WindowsHost::new("Jadix portable Native RadioGroup", [700, 360])?;
            let viewport = host.viewport();
            let root = NativeRoot::new(host);
            let projection = ElementProjection::with_lifetime(root.model().lifetime());
            let tree = PresentationNode {
                id: NodeId::new(root.model().id()),
                descriptor: NodeDescriptor {
                interaction: Default::default(),
                keyboard: Default::default(),
                navigation_keys: vec![],
                accessibility: Default::default(),
                accessibility_relations: Default::default(),
                    content: NodeContent::Container,
                    frame: Rect::default(),
                    accessible_role: None,
                    accessible_selected: None,
                    accessible_expanded: None,
                    accessible_description: None,
                    accessible_name: None,
                    style: Default::default(),
                },
                children: vec![],
            };
            let mut app = Self {
                _owner: owner,
                element,
                selected,
                projection,
                root,
                tree,
                viewport,
                environment: 1,
            };
            app.render()?;
            app.root.host().show();
            Ok(app)
        }

        fn render(&mut self) -> Result<(), String> {
            self.element.descriptor.style.width = Some(self.viewport[0]);
            self.element.descriptor.style.height = Some(self.viewport[1]);
            self.tree = self
                .projection
                .snapshot(&mut self.element)
                .map_err(|error| format!("{error:?}"))?;
            self.root
                .present(
                    std::slice::from_mut(&mut self.tree),
                    self.viewport,
                    self.environment,
                )
                .map_err(|error| error.to_string())?;
            let bindings = self.projection.navigation_nodes(&self.element);
            if !self.root.host_mut().bind_navigation(&bindings)? {
                return Err("Windows RadioGroup navigation binding is unavailable".into());
            }
            if let Some(node) = self.projection.take_focus_request(&self.element) {
                self.root
                    .focus(Some(node))
                    .map_err(|error| error.to_string())?;
            }
            Ok(())
        }

        fn tick(&mut self) -> Result<(), String> {
            self.root.host().pump();
            let events = self.root.poll_events().map_err(|error| error.to_string())?;
            let mut changed = false;
            for event in events {
                if matches!(event.payload, EventPayload::Focus) {
                    continue;
                }
                self.projection
                    .dispatch(&mut self.element, &event)
                    .map_err(|error| format!("{error:?}"))?;
                changed = true;
            }
            if self.root.host().environment() != self.environment {
                self.environment = self.root.host().environment();
                self.viewport = self.root.host().viewport();
                changed = true;
            }
            if changed {
                self.render()?;
            }
            Ok(())
        }

        fn button(&self, label: &str) -> Result<NodeId, String> {
            find(&self.tree, &|content| matches!(content, NodeContent::Button { label: current, .. } if current == label)).ok_or_else(|| format!("missing button {label}"))
        }

        fn verify_tab_stop(&self) -> Result<(), String> {
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                GetNextDlgTabItem, GetParent, GetWindowLongPtrW, GWL_STYLE, WS_TABSTOP,
            };
            let mut selected_window = std::ptr::null_mut();
            for (label, value) in [
                ("Standard", "standard"),
                ("Disabled", "blocked"),
                ("Express", "express"),
            ] {
                let window = self
                    .root
                    .host()
                    .control_identity(self.button(label)?)
                    .ok_or("missing native tab trigger")? as _;
                let selected = self.selected.get() == value;
                if (unsafe { GetWindowLongPtrW(window, GWL_STYLE) } as u32 & WS_TABSTOP != 0)
                    != selected
                {
                    return Err("RadioGroup native tab stops disagree with selection".into());
                }
                if selected {
                    selected_window = window;
                }
            }
            if selected_window.is_null() {
                return Err("RadioGroup has no selected native tab stop".into());
            }
            let parent = unsafe { GetParent(selected_window) };
            if unsafe { GetNextDlgTabItem(parent, std::ptr::null_mut(), 0) } != selected_window
                || unsafe { GetNextDlgTabItem(parent, selected_window, 0) } != selected_window
            {
                return Err(
                    "RadioGroup native traversal did not retain exactly one tab stop".into(),
                );
            }
            Ok(())
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
            _ => return Err("Usage: native-radio [--probe NONCE NEW_REPORT]".into()),
        };
        let mut app = App::new()?;
        if let Some((nonce, output)) = probe {
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                PostMessageW, SendMessageW, BM_GETCHECK, WM_KEYDOWN,
            };
            app.verify_tab_stop()?;
            let standard = app.button("Standard")?;
            let express = app.button("Express")?;
            let identities =
                [standard, express].map(|id| app.root.host().control_identity(id).unwrap());
            app.root.host().activate(express)?;
            app.tick()?;
            app.verify_tab_stop()?;
            if app.selected.get() != "express" {
                return Err("native radio activation did not update controlled state".into());
            }
            app.root
                .focus(Some(express))
                .map_err(|error| error.to_string())?;
            unsafe {
                PostMessageW(identities[1] as _, WM_KEYDOWN, 0x27, 0);
            }
            app.tick()?;
            app.verify_tab_stop()?;
            if app.selected.get() != "standard" || app.root.model().focused() != Some(standard) {
                return Err("native radio navigation did not wrap and return focus".into());
            }
            if unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::GetFocus() } as usize
                != identities[0]
            {
                return Err("radio focus request did not reach the OS control".into());
            }
            for (index, id) in [standard, express].into_iter().enumerate() {
                let window = app
                    .root
                    .host()
                    .control_identity(id)
                    .ok_or("radio disappeared")?;
                if window != identities[index]
                    || unsafe { SendMessageW(window as _, BM_GETCHECK, 0, 0) }
                        != isize::from(index == 0)
                {
                    return Err(
                        "radio native identity or checked state disagrees with projection".into(),
                    );
                }
            }
            let before = app.root.host().stats().commits;
            app.render()?;
            app.render()?;
            if app.root.host().stats().commits != before {
                return Err("idle radio render committed native mutations".into());
            }
            let report = serde_json::json!({"schema": 1, "nonce": nonce, "native_controls": true,
                "application": "native-radio", "rust_rsx_compound": true, "native_keyboard_focus": true,
                "single_native_tab_stop": true, "native_checked_state": true, "stable_control_identity": true,
                "disabled_item_skipped": true, "idle_native_commits": 0, "native": app.root.host().diagnostics(),
                "physical_keyboard_accepted": false, "screen_reader_accepted": false});
            let mut file = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(output)
                .map_err(|error| error.to_string())?;
            file.write_all(&serde_json::to_vec_pretty(&report).map_err(|error| error.to_string())?)
                .map_err(|error| error.to_string())?;
            return Ok(());
        }
        loop {
            app.root.host().pump();
            if app.root.host().is_closed() {
                return Ok(());
            }
            app.tick()?;
            app.root.host().wait_for_events(u32::MAX)?;
        }
    }
}
