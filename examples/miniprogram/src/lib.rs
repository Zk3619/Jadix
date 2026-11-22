#[cfg(target_arch = "wasm32")]
mod wasm {
    use std::cell::RefCell;

    #[cfg(not(feature = "consistent-ui"))]
    use jadix_native::environment::ColorScheme;
    use jadix_native::{
        environment::EnvironmentSnapshot,
        host::NativeRoot,
        miniprogram::MiniProgramHost,
        presentation::{style::NodeStyle, *},
        wire,
    };

    #[link(wasm_import_module = "jadix")]
    extern "C" {
        fn measure(pointer: *const u8, length: usize, result: *mut f64) -> i32;
        fn publish(pointer: *const u8, length: usize) -> i32;
        fn dispose();
    }

    thread_local! {
        static APP: RefCell<Option<App>> = const { RefCell::new(None) };
        static ERROR: RefCell<String> = const { RefCell::new(String::new()) };
    }

    struct App {
        root: NativeRoot<MiniProgramHost>,
        nodes: Vec<PresentationNode>,
        viewport: [f32; 2],
        count: u64,
        environment: EnvironmentSnapshot,
        environment_revision: u64,
    }

    fn node(root: RootId, content: NodeContent, height: f32) -> PresentationNode {
        PresentationNode {
            id: NodeId::new(root),
            descriptor: NodeDescriptor {
                content,
                frame: Rect::default(),
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
                style: NodeStyle {
                    height: Some(height),
                    shrink: 0.0,
                    ..Default::default()
                },
            },
            children: vec![],
        }
    }

    impl App {
        fn new(viewport: [f32; 2]) -> Result<Self, String> {
            let host = MiniProgramHost::new(
                |request| {
                    let mut result = [0.0; 2];
                    let accepted =
                        unsafe { measure(request.as_ptr(), request.len(), result.as_mut_ptr()) };
                    if accepted == 0 {
                        Ok(result)
                    } else {
                        Err("Canvas text measurement failed".into())
                    }
                },
                |batch| {
                    let accepted = unsafe { publish(batch.as_ptr(), batch.len()) };
                    if accepted == 0 {
                        Ok(())
                    } else {
                        Err("mini-program commit rejected".into())
                    }
                },
                || unsafe { dispose() },
            );
            let root = NativeRoot::new(host);
            let id = root.model().id();
            let mut container = node(id, NodeContent::Container, viewport[1]);
            container.descriptor.style.padding = [16.0; 4];
            container.descriptor.style.gap = 8.0;
            container.children = vec![
                node(
                    id,
                    NodeContent::Text("Jadix · 微信小程序 · 中😀".into()),
                    30.0,
                ),
                node(
                    id,
                    NodeContent::Button {
                        label: "Count: 0".into(),
                        enabled: true,
                    },
                    40.0,
                ),
                node(
                    id,
                    NodeContent::Input {
                        value: "中😀".into(),
                        placeholder: "Unicode input".into(),
                        enabled: true,
                        password: false,
                        max_length: None,
                        revision: 0,
                    },
                    40.0,
                ),
                node(
                    id,
                    NodeContent::Checkbox {
                        label: "Enabled".into(),
                        checked: false,
                        enabled: true,
                        revision: 0,
                    },
                    36.0,
                ),
                node(
                    id,
                    NodeContent::Select {
                        options: vec![
                            SelectOption {
                                value: "a".into(),
                                label: "选项 A".into(),
                                enabled: true,
                            },
                            SelectOption {
                                value: "b".into(),
                                label: "选项 B 😀".into(),
                                enabled: true,
                            },
                        ],
                        value: "a".into(),
                        placeholder: "Select".into(),
                        enabled: true,
                        revision: 0,
                    },
                    36.0,
                ),
                node(id, NodeContent::Scroll, 100.0),
            ];
            container.children[5].children = (0..50)
                .map(|index| {
                    node(
                        id,
                        NodeContent::Text(format!("Row {index}: 共享布局 中😀")),
                        28.0,
                    )
                })
                .collect();
            container.children.push(node(
                id,
                NodeContent::Textarea {
                    value: "多行\n中😀".into(),
                    placeholder: "Textarea".into(),
                    enabled: true,
                    max_length: None,
                    revision: 0,
                },
                40.0,
            ));
            container.children.push(node(
                id,
                NodeContent::Switch {
                    label: "Switch".into(),
                    checked: false,
                    enabled: true,
                    revision: 0,
                },
                32.0,
            ));
            let mut image = node(id, NodeContent::Image {source: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aLu0AAAAASUVORK5CYII=".into(), alt: "1×1 PNG".into()}, 24.0);
            image.descriptor.style.width = Some(24.0);
            container.children.push(image);
            container.children.push(node(
                id,
                NodeContent::Button {
                    label: "Open dialog".into(),
                    enabled: true,
                },
                32.0,
            ));
            container.children.push(node(
                id,
                NodeContent::Button {
                    label: "Open popover".into(),
                    enabled: true,
                },
                32.0,
            ));
            let anchor = container.children[10].id;
            let mut dialog = node(
                id,
                NodeContent::Dialog {
                    open: false,
                    close_on_overlay_click: true,
                    revision: 0,
                },
                120.0,
            );
            dialog.descriptor.style.width = Some(240.0);
            dialog.descriptor.style.padding = [12.0; 4];
            dialog.children = vec![
                node(id, NodeContent::Text("共享 Dialog 中😀".into()), 32.0),
                node(
                    id,
                    NodeContent::Button {
                        label: "Close dialog".into(),
                        enabled: true,
                    },
                    32.0,
                ),
            ];
            container.children.push(dialog);
            let mut popover = node(
                id,
                NodeContent::Popover {
                    anchor,
                    placement: PopoverPlacement::BottomStart,
                    gap: 4,
                    open: false,
                    close_on_outside_click: true,
                    revision: 0,
                },
                104.0,
            );
            popover.descriptor.style.width = Some(220.0);
            popover.descriptor.style.padding = [12.0; 4];
            popover.children = vec![
                node(id, NodeContent::Text("共享 Popover 中😀".into()), 32.0),
                node(
                    id,
                    NodeContent::Button {
                        label: "Close popover".into(),
                        enabled: true,
                    },
                    32.0,
                ),
            ];
            container.children.push(popover);
            let mut app = Self {
                root,
                nodes: vec![container],
                viewport,
                count: 0,
                environment: EnvironmentSnapshot::default(),
                environment_revision: 0,
            };
            app.present()?;
            Ok(app)
        }

        fn present(&mut self) -> Result<(), String> {
            #[cfg(feature = "consistent-ui")]
            let mut projected = jadix_native::ConsistentTheme::default()
                .project(&self.nodes, &self.environment)
                .map_err(str::to_string)?;
            #[cfg(feature = "consistent-ui")]
            let nodes = &mut projected;
            #[cfg(not(feature = "consistent-ui"))]
            let nodes = &mut self.nodes;
            self.root
                .present(nodes, self.viewport, self.environment_revision)
                .map(|_| ())
                .map_err(|error| error.to_string())
        }

        fn event(&mut self, json: &str) -> Result<(), String> {
            let event =
                wire::decode_event(json, self.root.model()).map_err(|error| error.to_string())?;
            self.root.event(&event).map_err(|error| error.to_string())?;
            if event.payload == EventPayload::Activate {
                for (trigger, panel) in [(9, 11), (10, 12)] {
                    let open = event.node == self.nodes[0].children[trigger].id;
                    let close = event.node == self.nodes[0].children[panel].children[1].id;
                    if open || close {
                        match &mut self.nodes[0].children[panel].descriptor.content {
                            NodeContent::Dialog { open: value, .. }
                            | NodeContent::Popover { open: value, .. } => *value = open,
                            _ => unreachable!(),
                        }
                        return self.present();
                    }
                }
            }
            let Some(target) = self.nodes[0]
                .children
                .iter_mut()
                .find(|node| node.id == event.node)
            else {
                return Ok(());
            };
            match (&mut target.descriptor.content, event.payload) {
                (NodeContent::Button { label, .. }, EventPayload::Activate) => {
                    self.count = self.count.checked_add(1).ok_or("counter exhausted")?;
                    *label = format!("Count: {}", self.count);
                }
                (
                    NodeContent::Input {
                        value, revision, ..
                    }
                    | NodeContent::Textarea {
                        value, revision, ..
                    },
                    EventPayload::Submit {
                        revision: next_revision,
                    },
                ) => {
                    *revision = next_revision;
                    // Visible confirmation of the committed value in this sample.
                    self.nodes[0].children[0].descriptor.content =
                        NodeContent::Text(format!("Submitted: {value}"));
                }
                (
                    NodeContent::Input {
                        value, revision, ..
                    }
                    | NodeContent::Textarea {
                        value, revision, ..
                    },
                    EventPayload::Edit {
                        value: next,
                        revision: next_revision,
                        ..
                    },
                ) => {
                    *value = next;
                    *revision = next_revision;
                }
                (
                    NodeContent::Checkbox {
                        checked, revision, ..
                    }
                    | NodeContent::Switch {
                        checked, revision, ..
                    },
                    EventPayload::Toggle {
                        checked: next,
                        revision: next_revision,
                    },
                ) => {
                    *checked = next;
                    *revision = next_revision;
                }
                (
                    NodeContent::Select {
                        value, revision, ..
                    },
                    EventPayload::Select {
                        value: next,
                        revision: next_revision,
                    },
                ) => {
                    *value = next;
                    *revision = next_revision;
                }
                (
                    NodeContent::Dialog { open, revision, .. }
                    | NodeContent::Popover { open, revision, .. },
                    EventPayload::Dismiss {
                        revision: next_revision,
                        ..
                    },
                ) => {
                    *open = false;
                    *revision = next_revision;
                }
                _ => return Ok(()),
            }
            self.present()
        }

        fn environment(&mut self, json: &str) -> Result<(), String> {
            let next = EnvironmentSnapshot::from_json(json.as_bytes())
                .map_err(|error| error.to_string())?;
            if next == self.environment {
                return Ok(());
            }
            self.environment_revision = self
                .environment_revision
                .checked_add(1)
                .ok_or("environment revision exhausted")?;
            #[cfg(not(feature = "consistent-ui"))]
            {
                let dark = next.color_scheme == Some(ColorScheme::Dark);
                self.nodes[0].descriptor.style.background = if dark {
                    [24, 24, 27, 255]
                } else {
                    [250, 250, 250, 255]
                };
                let mut pending: Vec<_> = self.nodes.iter_mut().collect();
                while let Some(node) = pending.pop() {
                    node.descriptor.style.foreground = if dark {
                        [244, 244, 245, 255]
                    } else {
                        [24, 24, 27, 255]
                    };
                    node.descriptor.style.font_size = 16.0 * next.font_scale;
                    if matches!(
                        node.descriptor.content,
                        NodeContent::Dialog { .. } | NodeContent::Popover { .. }
                    ) {
                        node.descriptor.style.background = if dark {
                            [39, 39, 42, 255]
                        } else {
                            [255, 255, 255, 255]
                        };
                    }
                    pending.extend(node.children.iter_mut());
                }
            }
            self.environment = next;
            self.present()
        }
    }

    fn status(result: Result<(), String>) -> i32 {
        match result {
            Ok(()) => 0,
            Err(error) => {
                ERROR.with(|slot| *slot.borrow_mut() = error);
                1
            }
        }
    }

    fn with_app(work: impl FnOnce(&mut App) -> Result<(), String>) -> i32 {
        status(APP.with(|slot| {
            let mut slot = slot
                .try_borrow_mut()
                .map_err(|_| "reentrant mini-program call")?;
            work(slot.as_mut().ok_or("mini-program not mounted")?)
        }))
    }

    fn text(pointer: usize, length: usize) -> Result<String, String> {
        if pointer == 0 || length > wire::MAX_EVENT_BYTES {
            return Err("invalid message buffer".into());
        }
        let bytes = unsafe { std::slice::from_raw_parts(pointer as *const u8, length) };
        String::from_utf8(bytes.to_vec()).map_err(|error| error.to_string())
    }

    #[no_mangle]
    pub extern "C" fn jadix_alloc(length: usize) -> usize {
        if length == 0 || length > wire::MAX_EVENT_BYTES {
            return 0;
        }
        Box::into_raw(vec![0u8; length].into_boxed_slice()) as *mut u8 as usize
    }

    #[no_mangle]
    pub extern "C" fn jadix_dealloc(pointer: usize, length: usize) {
        if pointer != 0 && length != 0 {
            unsafe {
                drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(
                    pointer as *mut u8,
                    length,
                )))
            };
        }
    }

    #[no_mangle]
    pub extern "C" fn jadix_mount(width: f32, height: f32) -> i32 {
        status(APP.with(|slot| {
            let mut slot = slot
                .try_borrow_mut()
                .map_err(|_| "reentrant mini-program mount")?;
            if slot.is_some() {
                return Err("mini-program already mounted".into());
            }
            *slot = Some(App::new([width, height])?);
            Ok(())
        }))
    }

    #[no_mangle]
    pub extern "C" fn jadix_event(pointer: usize, length: usize) -> i32 {
        with_app(|app| app.event(&text(pointer, length)?))
    }

    #[no_mangle]
    pub extern "C" fn jadix_environment(pointer: usize, length: usize) -> i32 {
        with_app(|app| app.environment(&text(pointer, length)?))
    }

    #[no_mangle]
    pub extern "C" fn jadix_resize(width: f32, height: f32) -> i32 {
        with_app(|app| {
            if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
                return Err("invalid mini-program viewport".into());
            }
            app.viewport = [width, height];
            app.nodes[0].descriptor.style.height = Some(height);
            app.present()
        })
    }

    #[no_mangle]
    pub extern "C" fn jadix_dispose() {
        APP.with(|slot| {
            slot.borrow_mut().take();
        });
    }

    #[no_mangle]
    pub extern "C" fn jadix_error_pointer() -> usize {
        ERROR.with(|slot| slot.borrow().as_ptr() as usize)
    }

    #[no_mangle]
    pub extern "C" fn jadix_error_length() -> usize {
        ERROR.with(|slot| slot.borrow().len())
    }
}
