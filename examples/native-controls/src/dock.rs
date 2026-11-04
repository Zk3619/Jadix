use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::path::Path;

use jadix_core::dock::{Axis, Bounds, Dock, DockLimits, DockNode, Pane};
use jadix_native::{
    host::{NativeRoot, PlatformHost},
    presentation::*,
};
use serde::{Deserialize, Serialize};

use crate::editing::Draft;

const DOCUMENT_BYTES: usize = 128 * 1024;
const WORKSPACE_BYTES: usize = 2 * 1024 * 1024;

pub struct DockWorkspace {
    pub layout: Dock,
    pub current: u64,
    pub message: String,
    documents: BTreeMap<u64, Draft>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Checkpoint {
    version: u32,
    layout: serde_json::Value,
    documents: BTreeMap<u64, String>,
    current: u64,
}

impl DockWorkspace {
    pub fn demo() -> Result<Self, String> {
        let panes = [
            (1, "Welcome"),
            (2, "main.rs"),
            (3, "Notes / 中文"),
            (4, "Tasks"),
        ]
        .into_iter()
        .map(|(key, title)| Pane {
            key,
            title: title.into(),
            closable: key != 1,
        })
        .collect();
        let layout = Dock::new(
            panes,
            Bounds {
                origin: [80.0, 80.0],
                size: [1000.0, 720.0],
            },
            DockLimits::default(),
        )
        .map_err(|error| error.to_string())?;
        let documents = [
            (1, "Select a tab, split a pane, float it into another native window, or save this workspace."),
            (2, "fn main() {\n    println!(\"Jadix Native\");\n}\n"),
            (3, "中文 / 😀\nBusiness text survives docking and process recovery.\n"),
            (4, "[ ] Search\n[ ] Edit\n[ ] Save\n"),
        ].into_iter().map(|(key, value)| (key, Draft::new(value.into()))).collect();
        Ok(Self {
            layout,
            current: 1,
            message: "Ready".into(),
            documents,
        })
    }

    pub fn value(&self, pane: u64) -> Option<&str> {
        self.documents.get(&pane).map(|draft| draft.value.as_str())
    }

    pub fn composing(&self) -> bool {
        self.documents.values().any(Draft::is_composing)
    }

    fn edit(
        &mut self,
        pane: u64,
        value: &str,
        revision: u64,
        composing: bool,
    ) -> Result<(), String> {
        if value.len() > DOCUMENT_BYTES {
            return Err(String::from("Document exceeds 128 KiB"));
        }
        let bytes: usize = self
            .documents
            .iter()
            .filter(|(key, _)| **key != pane)
            .map(|(_, draft)| draft.display().len())
            .sum();
        if bytes + value.len() > 1024 * 1024 {
            return Err(String::from("Workspace text exceeds 1 MiB"));
        }
        self.documents
            .get_mut(&pane)
            .ok_or("unknown document")?
            .update(value, revision, composing);
        self.current = pane;
        Ok(())
    }

    pub fn checkpoint(&self) -> Result<Vec<u8>, String> {
        if self.layout.stack_of(self.current).is_none() {
            return Err(String::from("current pane is not in the workspace"));
        }
        let documents = self
            .layout
            .panes()
            .iter()
            .map(|pane| {
                self.value(pane.key)
                    .map(|value| (pane.key, value.to_owned()))
                    .ok_or_else(|| String::from("missing document"))
            })
            .collect::<Result<_, _>>()?;
        let checkpoint = Checkpoint {
            version: 1,
            layout: serde_json::from_slice(
                &self.layout.to_json().map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?,
            documents,
            current: self.current,
        };
        let bytes = serde_json::to_vec(&checkpoint).map_err(|error| error.to_string())?;
        if bytes.len() > WORKSPACE_BYTES {
            return Err(String::from("Checkpoint exceeds 2 MiB"));
        }
        Ok(bytes)
    }

    pub fn from_checkpoint(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > WORKSPACE_BYTES {
            return Err(String::from("Checkpoint exceeds 2 MiB"));
        }
        let checkpoint: Checkpoint =
            serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        if checkpoint.version != 1 {
            return Err(String::from("unsupported workspace version"));
        }
        let layout = Dock::from_json(
            &serde_json::to_vec(&checkpoint.layout).map_err(|error| error.to_string())?,
            DockLimits::default(),
        )
        .map_err(|error| error.to_string())?;
        let keys: BTreeSet<_> = layout.panes().iter().map(|pane| pane.key).collect();
        if checkpoint
            .documents
            .keys()
            .copied()
            .collect::<BTreeSet<_>>()
            != keys
            || !keys.contains(&checkpoint.current)
            || checkpoint
                .documents
                .values()
                .any(|text| text.len() > DOCUMENT_BYTES)
            || checkpoint
                .documents
                .values()
                .map(String::len)
                .sum::<usize>()
                > 1024 * 1024
        {
            return Err(String::from("invalid workspace document inventory"));
        }
        Ok(Self {
            layout,
            current: checkpoint.current,
            message: "Recovered workspace".into(),
            documents: checkpoint
                .documents
                .into_iter()
                .map(|(key, text)| (key, Draft::new(text)))
                .collect(),
        })
    }

    pub fn load(directory: &Path) -> Result<Self, String> {
        let file = std::fs::File::open(directory.join("workspace.json"))
            .map_err(|error| error.to_string())?;
        if file.metadata().map_err(|error| error.to_string())?.len() > WORKSPACE_BYTES as u64 {
            return Err(String::from("Checkpoint exceeds 2 MiB"));
        }
        let mut bytes = Vec::new();
        file.take(WORKSPACE_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        Self::from_checkpoint(&bytes)
    }

    pub fn save(&self, directory: &Path) -> Result<(), String> {
        let bytes = self.checkpoint()?;
        std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let sequence = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let temporary = directory.join(format!(".workspace-{}-{sequence}.tmp", std::process::id()));
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|error| error.to_string())?;
        let result = (|| -> Result<(), std::io::Error> {
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            std::fs::rename(&temporary, directory.join("workspace.json"))?;
            #[cfg(unix)]
            std::fs::File::open(directory)?.sync_all()?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        result.map_err(|error| error.to_string())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DockCommand {
    Focus,
    Drag {
        node: NodeId,
        phase: DragPhase,
        screen_position: [f64; 2],
    },
    Activate(u64),
    Split,
    Float,
    Rejoin,
    Undo,
    Redo,
    Save,
    BeginDrag,
    CancelDrag,
    DropRight,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DockIntent {
    pub pane: u64,
    pub command: DockCommand,
}

#[derive(Clone)]
struct PaneNodes {
    tab: NodeId,
    editor: NodeId,
}

#[derive(Clone)]
struct LayoutNodes {
    container: NodeId,
    tabs: NodeId,
}

struct RenderCache<'a> {
    panes: &'a mut BTreeMap<u64, PaneNodes>,
    nodes: &'a mut BTreeMap<u64, LayoutNodes>,
    seen_panes: &'a mut BTreeSet<u64>,
    seen_nodes: &'a mut BTreeSet<u64>,
}

pub struct DockView<Host: PlatformHost> {
    pub root: NativeRoot<Host>,
    pub surface: u64,
    outer: NodeId,
    actions: [NodeId; 2],
    buttons: [NodeId; 9],
    status: NodeId,
    panes: BTreeMap<u64, PaneNodes>,
    nodes: BTreeMap<u64, LayoutNodes>,
    pointer_drag: bool,
}

fn node(id: NodeId, content: NodeContent, size: [f32; 2]) -> PresentationNode {
    PresentationNode {
        id,
        descriptor: NodeDescriptor {
            content,
            frame: Rect::default(),
            accessible_role: None,
            accessible_selected: None,
            accessible_expanded: None,
            accessible_name: None,
            style: style::NodeStyle {
                width: Some(size[0].max(0.0)),
                height: Some(size[1].max(0.0)),
                shrink: 0.0,
                ..Default::default()
            },
        },
        children: vec![],
    }
}

impl<Host: PlatformHost> DockView<Host> {
    pub fn new(host: Host, surface: u64) -> Self {
        let root = NativeRoot::new(host);
        let owner = root.model().id();
        Self {
            root,
            surface,
            outer: NodeId::new(owner),
            actions: std::array::from_fn(|_| NodeId::new(owner)),
            buttons: std::array::from_fn(|_| NodeId::new(owner)),
            status: NodeId::new(owner),
            panes: BTreeMap::new(),
            nodes: BTreeMap::new(),
            pointer_drag: false,
        }
    }

    pub fn editor(&self, pane: u64) -> Option<NodeId> {
        self.panes.get(&pane).map(|nodes| nodes.editor)
    }
    pub fn tab(&self, pane: u64) -> Option<NodeId> {
        self.panes.get(&pane).map(|nodes| nodes.tab)
    }
    pub fn action(&self, index: usize) -> Option<NodeId> {
        self.buttons.get(index).copied()
    }

    pub fn layout_container(&self, key: u64) -> Option<NodeId> {
        self.nodes.get(&key).map(|nodes| nodes.container)
    }

    pub fn pointer_drag_available(&self) -> bool {
        self.pointer_drag
    }

    pub fn render(
        &mut self,
        workspace: &DockWorkspace,
        viewport: [f32; 2],
        environment: u64,
    ) -> Result<(), String> {
        let surface = workspace
            .layout
            .surfaces()
            .iter()
            .find(|surface| surface.key == self.surface)
            .ok_or("unknown dock surface")?;
        let owner = self.root.model().id();
        let mut panes = self.panes.clone();
        let mut nodes = self.nodes.clone();
        let mut seen_panes = BTreeSet::new();
        let mut seen_nodes = BTreeSet::new();
        fn render_node(
            layout: &DockNode,
            workspace: &DockWorkspace,
            owner: RootId,
            size: [f32; 2],
            cache: &mut RenderCache<'_>,
        ) -> Result<PresentationNode, String> {
            let retained = cache
                .nodes
                .entry(layout.key())
                .or_insert_with(|| LayoutNodes {
                    container: NodeId::new(owner),
                    tabs: NodeId::new(owner),
                })
                .clone();
            cache.seen_nodes.insert(layout.key());
            let mut container = node(retained.container, NodeContent::Container, size);
            match layout {
                DockNode::Stack {
                    panes: keys,
                    active,
                    ..
                } => {
                    let mut tabs = node(retained.tabs, NodeContent::Container, [size[0], 32.0]);
                    tabs.descriptor.style.axis = style::Axis::Row;
                    let mut editors = Vec::new();
                    for key in keys {
                        let ids = cache
                            .panes
                            .entry(*key)
                            .or_insert_with(|| PaneNodes {
                                tab: NodeId::new(owner),
                                editor: NodeId::new(owner),
                            })
                            .clone();
                        cache.seen_panes.insert(*key);
                        let title = &workspace
                            .layout
                            .panes()
                            .iter()
                            .find(|pane| pane.key == *key)
                            .ok_or("missing pane")?
                            .title;
                        let selected = key == active;
                        tabs.children.push(node(
                            ids.tab,
                            NodeContent::Button {
                                label: format!("{}{title}", if selected { "● " } else { "" }),
                                enabled: true,
                            },
                            [size[0] / keys.len() as f32, 32.0],
                        ));
                        let draft = workspace.documents.get(key).ok_or("missing document")?;
                        let editor_size = if selected {
                            [size[0], (size[1] - 32.0).max(0.0)]
                        } else {
                            [0.0, 0.0]
                        };
                        let mut editor = node(
                            ids.editor,
                            NodeContent::Textarea {
                                value: draft.display().into(),
                                placeholder: String::new(),
                                enabled: selected,
                                revision: draft.revision,
                            },
                            editor_size,
                        );
                        editor.descriptor.accessible_name = Some(title.clone());
                        editor.descriptor.style.font_family = "Consolas".into();
                        editors.push(editor);
                    }
                    container.children.push(tabs);
                    container.children.extend(editors);
                }
                DockNode::Split {
                    axis,
                    ratio,
                    first,
                    second,
                    ..
                } => {
                    let horizontal = *axis == Axis::Horizontal;
                    container.descriptor.style.axis = if horizontal {
                        style::Axis::Row
                    } else {
                        style::Axis::Column
                    };
                    container.descriptor.style.gap = 4.0;
                    let index = usize::from(!horizontal);
                    let available = (size[index] - 4.0).max(0.0);
                    let mut first_size = size;
                    let mut second_size = size;
                    first_size[index] = available * *ratio as f32;
                    second_size[index] = available - first_size[index];
                    container
                        .children
                        .push(render_node(first, workspace, owner, first_size, cache)?);
                    container.children.push(render_node(
                        second,
                        workspace,
                        owner,
                        second_size,
                        cache,
                    )?);
                }
            }
            Ok(container)
        }
        let mut outer = node(self.outer, NodeContent::Container, viewport);
        let labels = [
            "Split right",
            "Float pane",
            "Dock windows",
            "Undo layout",
            "Redo layout",
            "Save workspace",
            "Begin drag",
            "Cancel drag",
            "Drop right",
        ];
        for row in 0..2 {
            let mut bar = node(
                self.actions[row],
                NodeContent::Container,
                [viewport[0], 32.0],
            );
            bar.descriptor.style.axis = style::Axis::Row;
            for (index, label) in labels.iter().enumerate().skip(row * 5).take(5) {
                bar.children.push(node(
                    self.buttons[index],
                    NodeContent::Button {
                        label: (*label).into(),
                        enabled: true,
                    },
                    [viewport[0] / 5.0, 32.0],
                ));
            }
            outer.children.push(bar);
        }
        outer.children.push(render_node(
            &surface.root,
            workspace,
            owner,
            [viewport[0], (viewport[1] - 88.0).max(0.0)],
            &mut RenderCache {
                panes: &mut panes,
                nodes: &mut nodes,
                seen_panes: &mut seen_panes,
                seen_nodes: &mut seen_nodes,
            },
        )?);
        outer.children.push(node(
            self.status,
            NodeContent::Text(format!(
                "{} · revision {} · pane {} · {} windows",
                workspace.message,
                workspace.layout.revision(),
                workspace.current,
                workspace.layout.surfaces().len()
            )),
            [viewport[0], 24.0],
        ));
        self.root
            .present(&mut [outer], viewport, environment)
            .map_err(|error| error.to_string())?;
        panes.retain(|key, _| seen_panes.contains(key));
        nodes.retain(|key, _| seen_nodes.contains(key));
        match self
            .root
            .host_mut()
            .bind_drag(&panes.values().map(|nodes| nodes.tab).collect::<Vec<_>>())
        {
            Ok(available) => self.pointer_drag = available,
            Err(error) => {
                self.root.dispose();
                return Err(error);
            }
        }
        self.panes = panes;
        self.nodes = nodes;
        Ok(())
    }

    pub fn poll(&mut self, workspace: &mut DockWorkspace) -> Result<Vec<DockIntent>, String> {
        let events = self.root.poll_events().map_err(|error| error.to_string())?;
        let mut commands = Vec::new();
        for event in events {
            if let Some((pane, _)) = self
                .panes
                .iter()
                .find(|(_, nodes)| nodes.editor == event.node)
            {
                match &event.payload {
                    EventPayload::Edit {
                        value,
                        revision,
                        composing_utf16,
                        ..
                    } => {
                        if let Err(error) =
                            workspace.edit(*pane, value, *revision, composing_utf16.is_some())
                        {
                            let draft = workspace
                                .documents
                                .get_mut(pane)
                                .ok_or("missing document")?;
                            draft.revision =
                                revision.checked_add(1).ok_or("edit revision exhausted")?;
                            workspace.message = error;
                        }
                        commands.push(DockIntent {
                            pane: *pane,
                            command: DockCommand::Focus,
                        });
                    }
                    EventPayload::Focus => {
                        workspace.current = *pane;
                        commands.push(DockIntent {
                            pane: *pane,
                            command: DockCommand::Focus,
                        });
                    }
                    _ => {}
                }
            }
            if event.payload == EventPayload::Activate {
                if let Some((pane, _)) =
                    self.panes.iter().find(|(_, nodes)| nodes.tab == event.node)
                {
                    workspace.current = *pane;
                    commands.push(DockIntent {
                        pane: *pane,
                        command: DockCommand::Activate(*pane),
                    });
                }
                if let Some(index) = self.buttons.iter().position(|id| *id == event.node) {
                    commands.push(DockIntent {
                        pane: workspace.current,
                        command: [
                            DockCommand::Split,
                            DockCommand::Float,
                            DockCommand::Rejoin,
                            DockCommand::Undo,
                            DockCommand::Redo,
                            DockCommand::Save,
                            DockCommand::BeginDrag,
                            DockCommand::CancelDrag,
                            DockCommand::DropRight,
                        ][index],
                    });
                }
            }
            if let EventPayload::Drag {
                phase,
                screen_position,
            } = event.payload
            {
                if let Some((pane, _)) =
                    self.panes.iter().find(|(_, nodes)| nodes.tab == event.node)
                {
                    commands.push(DockIntent {
                        pane: *pane,
                        command: DockCommand::Drag {
                            node: event.node,
                            phase,
                            screen_position,
                        },
                    });
                }
            }
        }
        Ok(commands)
    }
}
