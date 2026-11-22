use std::collections::BTreeMap;

use crate::editing::Draft;

use jadix_core::virtualization::VirtualAxis;
use jadix_native::{
    host::{NativeRoot, PlatformHost},
    presentation::*,
    ResourceLimits,
};

#[derive(Clone)]
struct Row {
    input: NodeId,
    spacer: NodeId,
}

pub struct Workbench<Host: PlatformHost> {
    pub root: NativeRoot<Host>,
    axis: VirtualAxis<u64>,
    rows: BTreeMap<u64, Row>,
    drafts: BTreeMap<u64, Draft>,
    scroll: NodeId,
    body: NodeId,
    bottom: NodeId,
    viewport: [f32; 2],
    offset: f64,
    pinned: Option<u64>,
    environment: u64,
}

fn node(identifier: NodeId, content: NodeContent, width: f32, height: f32) -> PresentationNode {
    PresentationNode {
        id: identifier,
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

impl<Host: PlatformHost> Workbench<Host> {
    pub fn new(host: Host, viewport: [f32; 2], count: u64) -> Result<Self, String> {
        let axis = VirtualAxis::new(0..count, 32.0).map_err(|error| format!("{error:?}"))?;
        let root = NativeRoot::with_limits(
            host,
            ResourceLimits {
                retired_nodes: (count as usize).saturating_mul(16).max(65536),
                ..Default::default()
            },
        );
        let identifier = root.model().id();
        let mut app = Self {
            root,
            axis,
            rows: BTreeMap::new(),
            drafts: BTreeMap::new(),
            scroll: NodeId::new(identifier),
            body: NodeId::new(identifier),
            bottom: NodeId::new(identifier),
            viewport,
            offset: 0.0,
            pinned: None,
            environment: 1,
        };
        app.render()?;
        Ok(app)
    }

    pub fn scroll_node(&self) -> NodeId {
        self.scroll
    }

    pub fn input_node(&self, key: u64) -> Option<NodeId> {
        self.rows.get(&key).map(|row| row.input)
    }

    pub fn value(&self, key: u64) -> Option<&str> {
        self.drafts.get(&key).map(|draft| draft.value.as_str())
    }

    pub fn pin(&mut self, key: Option<u64>) -> Result<(), String> {
        if key.is_some_and(|key| self.axis.index(&key).is_none()) {
            return Err(String::from("unknown workbench row"));
        }
        let previous = self.pinned;
        self.pinned = key;
        if let Err(error) = self.render() {
            self.pinned = previous;
            return Err(error);
        }
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

    pub fn render(&mut self) -> Result<(), String> {
        let pins: Vec<_> = self.pinned.into_iter().collect();
        let window = self
            .axis
            .window(self.offset, f64::from(self.viewport[1]), 64.0, &pins, 128)
            .map_err(|error| format!("{error:?}"))?;
        let offset = window.offset;
        let identifier = self.root.model().id();
        let width = (self.viewport[0] - 24.0).max(0.0);
        let mut body = node(
            self.body,
            NodeContent::Container,
            width,
            window.total as f32,
        );
        let mut position = 0.0;
        let mut desired = BTreeMap::new();
        for index in window.mounted {
            let key = *self.axis.key(index).ok_or("unknown workbench index")?;
            let row = self.rows.get(&key).cloned().unwrap_or_else(|| Row {
                input: NodeId::new(identifier),
                spacer: NodeId::new(identifier),
            });
            let top = self
                .axis
                .position(index)
                .ok_or("missing workbench row position")?;
            let height = self
                .axis
                .extent(&key)
                .ok_or("missing workbench row height")?;
            body.children.push(node(
                row.spacer,
                NodeContent::Container,
                width,
                (top - position).max(0.0) as f32,
            ));
            let draft = self
                .drafts
                .get(&key)
                .cloned()
                .unwrap_or_else(|| Draft::new(format!("Record {key:06} / 中文 😀")));
            let mut input = node(
                row.input,
                NodeContent::Input {
                    max_length: None,
                    value: draft.display().into(),
                    placeholder: String::new(),
                    enabled: true,
                    password: false,
                    revision: draft.revision,
                },
                width,
                height as f32,
            );
            input.descriptor.accessible_name = Some(format!("Record {key}"));
            body.children.push(input);
            desired.insert(key, row);
            position = top + height;
        }
        body.children.push(node(
            self.bottom,
            NodeContent::Container,
            width,
            (window.total - position).max(0.0) as f32,
        ));
        let mut scroll = node(
            self.scroll,
            NodeContent::Scroll,
            self.viewport[0],
            self.viewport[1],
        );
        scroll.children.push(body);
        self.root
            .present(&mut [scroll], self.viewport, self.environment)
            .map_err(|error| error.to_string())?;
        self.rows = desired;
        self.offset = offset;
        Ok(())
    }

    pub fn poll_events(&mut self) -> Result<usize, String> {
        let events = self.root.poll_events().map_err(|error| error.to_string())?;
        let mut changed = false;
        for event in &events {
            if let EventPayload::Scroll { y, .. } = event.payload {
                if event.node == self.scroll {
                    self.offset = y.max(0.0);
                    changed = true;
                }
                continue;
            }
            let Some(key) = self
                .rows
                .iter()
                .find(|(_, row)| row.input == event.node)
                .map(|(key, _)| *key)
            else {
                continue;
            };
            match &event.payload {
                EventPayload::Focus => {
                    self.pinned = Some(key);
                    changed = true;
                }
                EventPayload::Edit {
                    value,
                    revision,
                    composing_utf16,
                    ..
                } => {
                    self.pinned = Some(key);
                    self.drafts
                        .entry(key)
                        .or_insert_with(|| Draft::new(format!("Record {key:06} / 中文 😀")))
                        .update(value, *revision, composing_utf16.is_some());
                    changed = true;
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
        serde_json::json!({"application": "native-workbench", "rows": self.axis.len(), "mounted_rows": self.rows.len(),
            "offset": self.offset, "pinned": self.pinned, "edited_rows": self.drafts.values().filter(|draft| draft.committed).count(), "revision": self.root.model().revision()})
    }
}
