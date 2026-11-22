#[cfg(not(windows))]
fn main() {
    eprintln!("native-tabs requires Windows");
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
        draft: Signal<String>,
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
            let (owner, (element, selected, draft)) = create_root(|| {
                let selected = create_signal("account".to_string());
                let draft = create_signal("中文😀".to_string());
                let content = ui! {
                    <Tabs id="native-settings" value={selected}>
                        <TabsList>
                            <TabsTrigger value="account">"Account"</TabsTrigger>
                            <TabsTrigger value="blocked" disabled={true}>"Disabled"</TabsTrigger>
                            <TabsTrigger value="password">"Password"</TabsTrigger>
                        </TabsList>
                        <TabsContent value="account">
                            <Input id="account-name" value={draft} placeholder="Account name" />
                        </TabsContent>
                        <TabsContent value="password">"Password settings panel"</TabsContent>
                    </Tabs>
                };
                (Element::new().p(12.0).child(content), selected, draft)
            });
            let host = WindowsHost::new("Jadix portable Native Tabs", [700, 360])?;
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
                draft,
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
                return Err("Windows Tabs navigation binding is unavailable".into());
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
                ("Account", "account"),
                ("Disabled", "blocked"),
                ("Password", "password"),
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
                    return Err("Tabs native tab stops disagree with selection".into());
                }
                if selected {
                    selected_window = window;
                }
            }
            if selected_window.is_null() {
                return Err("Tabs has no selected native tab stop".into());
            }
            let parent = unsafe { GetParent(selected_window) };
            if unsafe { GetNextDlgTabItem(parent, std::ptr::null_mut(), 0) } != selected_window
                || unsafe { GetNextDlgTabItem(parent, selected_window, 0) } != selected_window
            {
                return Err("Tabs native traversal did not retain exactly one tab stop".into());
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
            _ => return Err("Usage: native-tabs [--probe NONCE NEW_REPORT]".into()),
        };
        let mut app = App::new()?;
        if let Some((nonce, output)) = probe {
            app.verify_tab_stop()?;
            let editor = find(&app.tree, &|content| {
                matches!(content, NodeContent::Input { .. })
            })
            .ok_or("missing account editor")?;
            let account = app.button("Account")?;
            let password = app.button("Password")?;
            app.root.host().select_range(editor, [0, 4])?;
            app.root
                .host()
                .replace_selection(editor, &format!("{nonce} 中文😀"))?;
            app.tick()?;
            let committed = app.draft.get();
            app.root.host().activate(password)?;
            app.tick()?;
            app.verify_tab_stop()?;
            if app.selected.get() != "password"
                || app.root.model().node(editor).is_some()
                || app.root.host().control_identity(editor).is_some()
            {
                return Err("inactive Tabs editor remained mounted".into());
            }
            let window = app
                .root
                .host()
                .control_identity(password)
                .ok_or("missing native Password button")?;
            unsafe {
                windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW(
                    window as _,
                    windows_sys::Win32::UI::WindowsAndMessaging::WM_KEYDOWN,
                    0x24,
                    0,
                );
            }
            app.tick()?;
            let restored = find(&app.tree, &|content| {
                matches!(content, NodeContent::Input { .. })
            })
            .ok_or("account panel did not remount")?;
            if app.selected.get() != "account"
                || restored == editor
                || app.root.host().text(restored)? != committed
                || app.root.model().focused() != Some(account)
            {
                return Err("Tabs Home did not restore account data and native focus".into());
            }
            let focused_window = app.root.host().control_identity(account).unwrap();
            app.verify_tab_stop()?;
            if unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::GetFocus() } as usize
                != focused_window
            {
                return Err("Tabs focus request did not reach the OS button".into());
            }
            unsafe {
                windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW(
                    focused_window as _,
                    windows_sys::Win32::UI::WindowsAndMessaging::WM_KEYDOWN,
                    0x09,
                    0,
                );
            }
            app.tick()?;
            if unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::GetFocus() } as usize
                != app
                    .root
                    .host()
                    .control_identity(restored)
                    .ok_or("missing remounted editor")?
            {
                return Err("Tabs Tab did not leave the tab list for the active panel".into());
            }
            app.root
                .focus(Some(account))
                .map_err(|error| error.to_string())?;
            unsafe {
                windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW(
                    focused_window as _,
                    windows_sys::Win32::UI::WindowsAndMessaging::WM_KEYDOWN,
                    0x27,
                    0,
                );
            }
            app.tick()?;
            app.verify_tab_stop()?;
            if app.selected.get() != "password" || app.root.model().focused() != Some(password) {
                return Err("Tabs ArrowRight did not skip disabled trigger".into());
            }
            let before = app.root.host().stats().commits;
            app.render()?;
            app.render()?;
            if app.root.host().stats().commits != before {
                return Err("idle Tabs render committed native mutations".into());
            }
            let report = serde_json::json!({"schema": 1, "nonce": nonce, "native_controls": true, "application": "native-tabs", "rust_rsx_compound": true,
                "native_keyboard_focus": true, "single_native_tab_stop": true, "tab_enters_active_panel": true, "disabled_trigger_skipped": true, "inactive_panel_unmounted": true, "business_text_preserved": true,
                "remount_uses_fresh_identity": true, "idle_native_commits": 0, "native": app.root.host().diagnostics(), "tab_uia_provider": false, "inactive_panel_native_undo_preserved": false, "physical_keyboard_accepted": false});
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
