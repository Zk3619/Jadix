use std::collections::BTreeMap;

use jadix_core::{tree::*, virtualization::VirtualAxis};
use jadix_native::{
    host::{NativeRoot, PlatformHost},
    presentation::*,
};

#[derive(Clone)]
struct Row {
    button: NodeId,
    gap: NodeId,
}

pub struct TreeBrowser<Host: PlatformHost> {
    pub root: NativeRoot<Host>,
    tree: Tree<u64>,
    axis: VirtualAxis<u64>,
    axis_revision: u64,
    rows: BTreeMap<u64, Row>,
    outer: NodeId,
    toolbar: NodeId,
    actions: [NodeId; 8],
    status: NodeId,
    scroll: NodeId,
    body: NodeId,
    tail: NodeId,
    viewport: [f32; 2],
    environment: u64,
    offset: f64,
    pending: Vec<LoadRequest<u64>>,
    arrow_keys: bool,
}

fn node(id: NodeId, content: NodeContent, width: f32, height: f32) -> PresentationNode {
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
            interaction: Default::default(),
            keyboard: Default::default(),
            navigation_keys: Vec::new(),
            accessibility: Default::default(),
            accessibility_relations: Default::default(),
            style: style::NodeStyle {
                width: Some(width),
                height: Some(height),
                shrink: 0.0,
                ..Default::default()
            },
        },
        children: vec![],
    }
}

impl<Host: PlatformHost> TreeBrowser<Host> {
    pub fn new(host: Host, viewport: [f32; 2], tree: Tree<u64>) -> Result<Self, String> {
        let root = NativeRoot::new(host);
        let identifier = root.model().id();
        let axis = VirtualAxis::new(tree.visible().iter().map(|row| row.key), 32.0)
            .map_err(|error| format!("{error:?}"))?;
        let axis_revision = tree.visibility_revision();
        let mut app = Self {
            root,
            tree,
            axis,
            axis_revision,
            rows: BTreeMap::new(),
            outer: NodeId::new(identifier),
            toolbar: NodeId::new(identifier),
            actions: std::array::from_fn(|_| NodeId::new(identifier)),
            status: NodeId::new(identifier),
            scroll: NodeId::new(identifier),
            body: NodeId::new(identifier),
            tail: NodeId::new(identifier),
            viewport,
            environment: 0,
            offset: 0.0,
            pending: vec![],
            arrow_keys: false,
        };
        app.render()?;
        Ok(app)
    }

    pub fn model(&self) -> &Tree<u64> {
        &self.tree
    }
    pub fn row_node(&self, key: u64) -> Option<NodeId> {
        self.rows.get(&key).map(|row| row.button)
    }
    pub fn action_node(&self, action: usize) -> Option<NodeId> {
        self.actions.get(action).copied()
    }
    pub fn pending_loads(&mut self) -> Vec<LoadRequest<u64>> {
        std::mem::take(&mut self.pending)
            .into_iter()
            .filter(|request| self.tree.request_pending(request))
            .collect()
    }

    pub fn set_expanded(&mut self, key: u64, expanded: bool) -> Result<(), String> {
        if !expanded {
            self.pending.retain(|request| *request.key() != key);
        }
        if let Some(request) = self
            .tree
            .set_expanded(&key, expanded)
            .map_err(|error| format!("{error:?}"))?
        {
            self.pending.push(request);
        }
        self.render()
    }

    pub fn complete_load(
        &mut self,
        request: &LoadRequest<u64>,
        children: Vec<TreeNode<u64>>,
    ) -> Result<(), String> {
        self.tree
            .complete_load(request, children)
            .map_err(|error| format!("{error:?}"))?;
        self.pending.retain(|pending| pending != request);
        self.render()
    }

    pub fn cancel_load(&mut self, request: &LoadRequest<u64>) -> Result<(), String> {
        self.tree
            .cancel_load(request)
            .map_err(|error| format!("{error:?}"))?;
        self.pending.retain(|pending| pending != request);
        self.render()
    }

    pub fn navigate(&mut self, command: TreeCommand) -> Result<(), String> {
        if let Some(request) = self
            .tree
            .command(command)
            .map_err(|error| format!("{error:?}"))?
        {
            self.pending.push(request);
        }
        self.sync_axis()?;
        self.reveal_focused();
        self.root
            .scroll_to(self.scroll, [0.0, self.offset])
            .map_err(|error| error.to_string())?;
        self.poll_events()?;
        self.render()?;
        if let Some(node) = self.tree.focused().and_then(|key| self.row_node(*key)) {
            self.root
                .focus(Some(node))
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    pub fn scroll_to(&mut self, offset: f64) -> Result<(), String> {
        if !offset.is_finite() || offset < 0.0 {
            return Err(String::from("invalid tree offset"));
        }
        self.root
            .scroll_to(self.scroll, [0.0, offset])
            .map_err(|error| error.to_string())?;
        self.poll_events()?;
        Ok(())
    }

    pub fn resize(&mut self, viewport: [f32; 2], environment: u64) -> Result<(), String> {
        let previous = (self.viewport, self.environment);
        self.viewport = viewport;
        self.environment = environment;
        if let Err(error) = self.render() {
            (self.viewport, self.environment) = previous;
            return Err(error);
        }
        Ok(())
    }

    fn sync_axis(&mut self) -> Result<(), String> {
        if self.axis_revision != self.tree.visibility_revision() {
            let anchor = self
                .axis
                .anchor(self.offset)
                .map_err(|error| format!("{error:?}"))?;
            self.axis
                .replace(self.tree.visible().iter().map(|row| row.key), 32.0)
                .map_err(|error| format!("{error:?}"))?;
            self.axis_revision = self.tree.visibility_revision();
            if let Some(anchor) = anchor {
                self.offset = self
                    .axis
                    .restore_anchor(&anchor, f64::from((self.viewport[1] - 80.0).max(0.0)))
                    .unwrap_or(self.offset);
            }
        }
        Ok(())
    }

    fn reveal_focused(&mut self) {
        if let Some(index) = self.tree.focused().and_then(|key| self.axis.index(key)) {
            let top = self.axis.position(index).unwrap_or(0.0);
            let height = f64::from((self.viewport[1] - 80.0).max(0.0));
            if top < self.offset {
                self.offset = top;
            } else if top + 32.0 > self.offset + height {
                self.offset = (top + 32.0 - height).max(0.0);
            }
        }
    }

    pub fn render(&mut self) -> Result<(), String> {
        if self
            .viewport
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
        {
            return Err(String::from("invalid tree viewport"));
        }
        self.sync_axis()?;
        let height = (self.viewport[1] - 80.0).max(0.0);
        let width = (self.viewport[0] - 24.0).max(0.0);
        let window = self
            .axis
            .window(self.offset, f64::from(height), 64.0, &[], 128)
            .map_err(|error| format!("{error:?}"))?;
        let identifier = self.root.model().id();
        let mut body = node(
            self.body,
            NodeContent::Container,
            width,
            window.total as f32,
        );
        let mut desired = BTreeMap::new();
        let mut position = 0.0;
        for index in window.mounted {
            let key = *self.axis.key(index).ok_or("tree axis lost key")?;
            let item = &self.tree.visible()[index];
            let row = self.rows.get(&key).cloned().unwrap_or_else(|| Row {
                button: NodeId::new(identifier),
                gap: NodeId::new(identifier),
            });
            let top = self.axis.position(index).ok_or("tree axis lost position")?;
            body.children.push(node(
                row.gap,
                NodeContent::Container,
                width,
                (top - position).max(0.0) as f32,
            ));
            let marker = if self.tree.loading(&key) {
                "…"
            } else if self.tree.branch(&key) {
                if self.tree.expanded(&key) {
                    "−"
                } else {
                    "+"
                }
            } else {
                "·"
            };
            let selected = self.tree.selected().contains(&key);
            let label = format!(
                "{}{}{} {}",
                "  ".repeat(item.depth),
                if selected { "✓ " } else { "" },
                marker,
                self.tree.label(&key).unwrap_or("")
            );
            let mut button = node(
                row.button,
                NodeContent::Button {
                    label: label.clone(),
                    enabled: self.tree.enabled(&key),
                },
                width,
                32.0,
            );
            button.descriptor.accessible_name = Some(format!(
                "{label}; level {}; item {} of {}",
                item.depth + 1,
                item.position,
                item.siblings
            ));
            button.descriptor.accessible_role = Some(AccessibleRole::TreeItem);
            button.descriptor.accessible_selected = Some(selected);
            button.descriptor.accessible_expanded =
                self.tree.branch(&key).then(|| self.tree.expanded(&key));
            button.descriptor.style.background = if selected {
                [220, 235, 255, 255]
            } else {
                [255, 255, 255, 255]
            };
            body.children.push(button);
            position = top + 32.0;
            desired.insert(key, row);
        }
        body.children.push(node(
            self.tail,
            NodeContent::Container,
            width,
            (window.total - position).max(0.0) as f32,
        ));
        let mut scroll = node(self.scroll, NodeContent::Scroll, self.viewport[0], height);
        scroll.descriptor.accessible_role = Some(AccessibleRole::Tree);
        scroll.children.push(body);
        let mut toolbar = node(self.toolbar, NodeContent::Container, self.viewport[0], 40.0);
        toolbar.descriptor.style.axis = style::Axis::Row;
        for (index, label) in [
            "First", "Previous", "Next", "Last", "Parent", "Child", "Toggle", "Select",
        ]
        .iter()
        .enumerate()
        {
            toolbar.children.push(node(
                self.actions[index],
                NodeContent::Button {
                    label: (*label).into(),
                    enabled: true,
                },
                self.viewport[0] / 8.0,
                40.0,
            ));
        }
        let status = node(
            self.status,
            NodeContent::Text(format!(
                "{} items; {} visible; {} mounted; focused {:?}; {} selected",
                self.tree.len(),
                self.axis.len(),
                desired.len(),
                self.tree.focused(),
                self.tree.selected().len()
            )),
            self.viewport[0],
            40.0,
        );
        let mut outer = node(
            self.outer,
            NodeContent::Container,
            self.viewport[0],
            self.viewport[1],
        );
        outer.children = vec![toolbar, status, scroll];
        self.root
            .present(&mut [outer], self.viewport, self.environment)
            .map_err(|error| error.to_string())?;
        self.rows = desired;
        self.offset = window.offset;
        let nodes: Vec<_> = self.rows.values().map(|row| row.button).collect();
        self.arrow_keys = self.root.host_mut().bind_navigation(&nodes)?;
        Ok(())
    }

    pub fn poll_events(&mut self) -> Result<usize, String> {
        let events = self.root.poll_events().map_err(|error| error.to_string())?;
        let mut changed = false;
        for event in &events {
            match event.payload {
                EventPayload::Focus => {
                    if let Some(key) = self
                        .rows
                        .iter()
                        .find(|(_, row)| row.button == event.node)
                        .map(|(key, _)| *key)
                    {
                        self.tree
                            .focus(&key)
                            .map_err(|error| format!("{error:?}"))?;
                        changed = true;
                    }
                }
                EventPayload::Scroll { y, .. } if event.node == self.scroll => {
                    self.offset = y;
                    changed = true;
                }
                _ => {}
            }
        }
        for event in &events {
            match event.payload {
                EventPayload::Tree { action } => {
                    if let Some(key) = self
                        .rows
                        .iter()
                        .find(|(_, row)| row.button == event.node)
                        .map(|(key, _)| *key)
                    {
                        match action {
                            TreeAction::Expand | TreeAction::Collapse => {
                                self.set_expanded(key, action == TreeAction::Expand)?;
                            }
                            TreeAction::Select => {
                                self.tree
                                    .select(&key, false, false)
                                    .map_err(|error| format!("{error:?}"))?;
                            }
                            TreeAction::AddToSelection | TreeAction::RemoveFromSelection => {
                                let selected = self.tree.selected().contains(&key);
                                if selected != (action == TreeAction::AddToSelection) {
                                    self.tree
                                        .select(&key, false, true)
                                        .map_err(|error| format!("{error:?}"))?;
                                }
                            }
                        }
                        changed = true;
                    }
                }
                EventPayload::Navigate {
                    key,
                    shift,
                    control,
                } => {
                    if let Some(row) = self
                        .rows
                        .iter()
                        .find(|(_, row)| row.button == event.node)
                        .map(|(key, _)| *key)
                    {
                        self.tree
                            .focus(&row)
                            .map_err(|error| format!("{error:?}"))?;
                        let command = match key {
                            NavigationKey::Up => TreeCommand::Previous,
                            NavigationKey::Down => TreeCommand::Next,
                            NavigationKey::Left => TreeCommand::Parent,
                            NavigationKey::Right => TreeCommand::Child,
                            NavigationKey::Home => TreeCommand::First,
                            NavigationKey::End => TreeCommand::Last,
                            _ => continue,
                        };
                        self.navigate(command)?;
                        if !control {
                            if let Some(focused) = self.tree.focused().copied() {
                                self.tree
                                    .select(&focused, shift, false)
                                    .map_err(|error| format!("{error:?}"))?;
                            }
                        }
                        changed = true;
                    }
                }
                EventPayload::Activate => {
                    if let Some(index) = self.actions.iter().position(|node| *node == event.node) {
                        let commands = [
                            TreeCommand::First,
                            TreeCommand::Previous,
                            TreeCommand::Next,
                            TreeCommand::Last,
                            TreeCommand::Parent,
                            TreeCommand::Child,
                        ];
                        if index < commands.len() {
                            self.navigate(commands[index])?;
                        } else if let Some(key) = self.tree.focused().copied() {
                            if index == 6 {
                                self.set_expanded(key, !self.tree.expanded(&key))?;
                            } else {
                                self.tree
                                    .select(&key, false, false)
                                    .map_err(|error| format!("{error:?}"))?;
                                changed = true;
                            }
                        }
                    } else if let Some(key) = self
                        .rows
                        .iter()
                        .find(|(_, row)| row.button == event.node)
                        .map(|(key, _)| *key)
                    {
                        self.tree
                            .select(&key, false, false)
                            .map_err(|error| format!("{error:?}"))?;
                        if self.tree.branch(&key) {
                            self.set_expanded(key, !self.tree.expanded(&key))?;
                        }
                        changed = true;
                    }
                }
                _ => {}
            }
        }
        if changed {
            self.render()?;
        }
        Ok(events.len())
    }

    pub fn state(&self) -> serde_json::Value {
        serde_json::json!({"application": "native-tree", "nodes": self.tree.len(), "visible_rows": self.axis.len(),
            "mounted_rows": self.rows.len(), "offset": self.offset, "focused": self.tree.focused(), "selected": self.tree.selected(),
            "revision": self.root.model().revision(), "treeitem_provider": false, "arrow_key_binding": self.arrow_keys})
    }
}

pub fn demo_tree() -> Result<Tree<u64>, TreeError> {
    let children = (1..100_000)
        .map(|key| TreeNode::leaf(key, format!("File {key:06} / 中文 😀")))
        .collect();
    let mut disabled = TreeNode::leaf(100_001, "Unavailable folder");
    disabled.enabled = false;
    let mut tree = Tree::new(
        vec![
            TreeNode {
                key: 0,
                label: "Workspace".into(),
                enabled: true,
                children: Children::Loaded(children),
            },
            TreeNode {
                key: 100_000,
                label: "Lazy folder".into(),
                enabled: true,
                children: Children::Unloaded,
            },
            disabled,
        ],
        TreeLimits {
            nodes: 110_000,
            ..Default::default()
        },
    )?;
    tree.set_expanded(&0, true)?;
    Ok(tree)
}
