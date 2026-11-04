use crate::{
    Session, ShellError,
    handles::{EventDelivery, Handle},
    schema::Value,
    wire::{Command, ComponentRegistry, ElementSpec, Envelope},
};
use jadix_native::{
    ResourceLimits,
    host::{HostError, NativeRoot, PlatformHost},
    layout::LayoutStats,
    presentation::{capabilities::NodeKind, *},
};
use std::collections::BTreeMap;

#[derive(Debug)]
pub enum MountError {
    Session(ShellError),
    Host(HostError),
    Properties,
    Unsupported(String),
    Revision,
}

impl std::fmt::Display for MountError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for MountError {}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Slot {
    Key(String),
    Index(usize),
    Component(String),
}

#[derive(Clone)]
struct Binding {
    id: NodeId,
    component: String,
    properties: Value,
    revision: u64,
    preview: Option<String>,
    observed: Option<Value>,
    events: BTreeMap<String, Handle>,
}

pub struct NativeMount<Host: PlatformHost> {
    root: NativeRoot<Host>,
    bindings: BTreeMap<Vec<Slot>, Binding>,
    positions: BTreeMap<Vec<usize>, NodeId>,
}

impl<Host: PlatformHost> NativeMount<Host> {
    pub fn new(
        session: &mut Session,
        mut host: Host,
        limits: ResourceLimits,
    ) -> Result<Self, MountError> {
        let lifetime = match session.mount_root() {
            Ok(lifetime) => lifetime,
            Err(error) => {
                host.dispose();
                return Err(MountError::Session(error));
            }
        };
        Ok(Self {
            root: NativeRoot::with_lifetime(host, lifetime, limits),
            bindings: BTreeMap::new(),
            positions: BTreeMap::new(),
        })
    }

    pub fn root(&self) -> &NativeRoot<Host> {
        &self.root
    }

    pub fn root_mut(&mut self) -> &mut NativeRoot<Host> {
        &mut self.root
    }

    pub fn node_at(&self, path: &[usize]) -> Option<NodeId> {
        self.positions
            .get(path)
            .copied()
            .filter(|node| self.root.model().node(*node).is_some())
    }

    pub fn present(
        &mut self,
        session: &Session,
        registry: &ComponentRegistry,
        envelope: &Envelope,
        viewport: [f32; 2],
        environment: u64,
    ) -> Result<LayoutStats, MountError> {
        session
            .validate_mount(envelope, registry)
            .map_err(MountError::Session)?;
        let Command::Mount { root, tree } = &envelope.command else {
            return Err(MountError::Properties);
        };
        if *root != self.root.model().id().token() {
            return Err(MountError::Session(ShellError::UnknownMount));
        }
        let mut bindings = BTreeMap::new();
        let mut positions = BTreeMap::new();
        let mut anchors = BTreeMap::<String, Vec<NodeId>>::new();
        let mut popovers = BTreeMap::<NodeId, String>::new();
        let mut pending = vec![(
            tree,
            vec![slot(tree, 0), Slot::Component(tree.component.clone())],
            Vec::new(),
            false,
        )];
        let mut completed: BTreeMap<Vec<usize>, PresentationNode> = BTreeMap::new();
        while let Some((element, path, position, visited)) = pending.pop() {
            if !visited {
                let mut binding = self
                    .bindings
                    .get(&path)
                    .filter(|binding| binding.component == element.component)
                    .cloned()
                    .unwrap_or_else(|| Binding {
                        id: NodeId::new(self.root.model().id()),
                        component: element.component.clone(),
                        properties: element.props.clone(),
                        revision: 0,
                        preview: None,
                        observed: None,
                        events: BTreeMap::new(),
                    });
                let kind = registry
                    .get(&element.component)
                    .ok_or(MountError::Properties)?
                    .kind;
                let state = match kind {
                    NodeKind::Input | NodeKind::Textarea | NodeKind::Select => {
                        property(&element.props, "value")
                    }
                    NodeKind::Checkbox | NodeKind::Switch => property(&element.props, "checked"),
                    NodeKind::Dialog | NodeKind::Popover => property(&element.props, "open"),
                    _ => None,
                };
                let old_state = match kind {
                    NodeKind::Input | NodeKind::Textarea | NodeKind::Select => {
                        property(&binding.properties, "value")
                    }
                    NodeKind::Checkbox | NodeKind::Switch => {
                        property(&binding.properties, "checked")
                    }
                    NodeKind::Dialog | NodeKind::Popover => property(&binding.properties, "open"),
                    _ => None,
                };
                let preserve_preview = binding.preview.is_some() && state == old_state;
                if !preserve_preview
                    && state.is_some()
                    && (binding.observed.is_some() || state != old_state)
                    && binding.observed.as_ref() != state
                {
                    binding.revision = binding
                        .revision
                        .checked_add(1)
                        .ok_or(MountError::Revision)?;
                }
                if !preserve_preview {
                    binding.preview = None;
                }
                binding.observed = None;
                binding.properties = element.props.clone();
                binding.events = element.events.clone();
                let content = content(element, kind, &binding)?;
                if let Some(key) = &element.key {
                    anchors.entry(key.clone()).or_default().push(binding.id);
                }
                if kind == NodeKind::Popover {
                    let Some(Value::String(anchor)) = property(&element.props, "anchor") else {
                        return Err(MountError::Properties);
                    };
                    popovers.insert(binding.id, anchor.clone());
                }
                positions.insert(position.clone(), binding.id);
                completed.insert(
                    position.clone(),
                    PresentationNode {
                        id: binding.id,
                        descriptor: NodeDescriptor {
                            content,
                            frame: Rect::default(),
                            accessible_role: None,
                            accessible_selected: None,
                            accessible_expanded: None,
                            accessible_name: None,
                            style: element.style.clone(),
                        },
                        children: Vec::new(),
                    },
                );
                bindings.insert(path.clone(), binding);
                pending.push((element, path.clone(), position.clone(), true));
                for (index, child) in element.children.iter().enumerate().rev() {
                    let mut child_path = path.clone();
                    child_path.push(slot(child, index));
                    child_path.push(Slot::Component(child.component.clone()));
                    let mut child_position = position.clone();
                    child_position.push(index);
                    pending.push((child, child_path, child_position, false));
                }
            } else {
                let children = (0..element.children.len())
                    .map(|index| {
                        let mut child_position = position.clone();
                        child_position.push(index);
                        completed
                            .remove(&child_position)
                            .ok_or(MountError::Properties)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                completed
                    .get_mut(&position)
                    .ok_or(MountError::Properties)?
                    .children = children;
            }
        }
        let mut tree = [completed
            .remove(&Vec::new())
            .ok_or(MountError::Properties)?];
        let mut pending = vec![&mut tree[0]];
        while let Some(node) = pending.pop() {
            if let NodeContent::Popover { anchor, .. } = &mut node.descriptor.content {
                let key = popovers.get(&node.id).ok_or(MountError::Properties)?;
                let candidates = anchors.get(key).ok_or(MountError::Properties)?;
                if candidates.len() != 1 {
                    return Err(MountError::Properties);
                }
                *anchor = candidates[0];
            }
            pending.extend(node.children.iter_mut());
        }
        let stats = self
            .root
            .present(&mut tree, viewport, environment)
            .map_err(MountError::Host)?;
        self.bindings = bindings;
        self.positions = positions;
        Ok(stats)
    }

    pub fn poll(&mut self, session: &Session) -> Result<Vec<EventDelivery>, MountError> {
        let events = self.root.poll_events().map_err(MountError::Host)?;
        let by_node: BTreeMap<_, _> = self
            .bindings
            .iter()
            .map(|(path, binding)| (binding.id, path.clone()))
            .collect();
        let mut deliveries = Vec::new();
        for event in events {
            let Some(path) = by_node.get(&event.node) else {
                continue;
            };
            let binding = self.bindings.get_mut(path).ok_or(MountError::Properties)?;
            let (name, arguments) = match event.payload {
                EventPayload::Activate => ("activate", serde_json::Value::Null),
                EventPayload::Edit {
                    value,
                    revision,
                    selection_utf16,
                    composing_utf16,
                } => {
                    binding.revision = revision;
                    binding.preview = composing_utf16.map(|_| value.clone());
                    binding.observed = composing_utf16
                        .is_none()
                        .then(|| Value::String(value.clone()));
                    let mut arguments = serde_json::json!({"value":value,"revision":revision.to_string(),"selection":selection_utf16});
                    if let Some(composing) = composing_utf16 {
                        arguments["composing"] = serde_json::json!(composing);
                    }
                    ("edit", arguments)
                }
                EventPayload::Toggle { checked, revision } => {
                    binding.revision = revision;
                    binding.observed = Some(Value::Bool(checked));
                    (
                        "toggle",
                        serde_json::json!({"checked":checked,"revision":revision.to_string()}),
                    )
                }
                EventPayload::Select { value, revision } => {
                    binding.revision = revision;
                    binding.observed = Some(Value::String(value.clone()));
                    (
                        "select",
                        serde_json::json!({"value":value,"revision":revision.to_string()}),
                    )
                }
                EventPayload::Dismiss { backdrop, revision } => {
                    binding.revision = revision;
                    binding.observed = Some(Value::Bool(false));
                    (
                        "dismiss",
                        serde_json::json!({"backdrop":backdrop,"revision":revision.to_string()}),
                    )
                }
                EventPayload::Scroll { x, y } => ("scroll", serde_json::json!({"x":x,"y":y})),
                EventPayload::Focus
                | EventPayload::Navigate { .. }
                | EventPayload::Drag { .. }
                | EventPayload::Expansion { .. }
                | EventPayload::Tree { .. } => continue,
            };
            if let Some(handle) = binding.events.get(name) {
                let arguments =
                    serde_json::from_value(arguments).map_err(|_| MountError::Properties)?;
                deliveries.push(
                    session
                        .event_delivery(handle, arguments)
                        .map_err(MountError::Session)?,
                );
            }
        }
        Ok(deliveries)
    }

    pub fn dispose(&mut self) {
        self.root.dispose();
        self.bindings.clear();
        self.positions.clear();
    }
}

fn slot(element: &ElementSpec, index: usize) -> Slot {
    element
        .key
        .as_ref()
        .map_or(Slot::Index(index), |key| Slot::Key(key.clone()))
}

fn property<'a>(props: &'a Value, name: &str) -> Option<&'a Value> {
    let Value::Object(properties) = props else {
        return None;
    };
    properties.get(name)
}

fn content(
    element: &ElementSpec,
    kind: NodeKind,
    binding: &Binding,
) -> Result<NodeContent, MountError> {
    let string = |name, optional| match property(&element.props, name) {
        Some(Value::String(value)) => Ok(value.clone()),
        None if optional => Ok(String::new()),
        _ => Err(MountError::Properties),
    };
    let boolean = |name, default| match property(&element.props, name) {
        Some(Value::Bool(value)) => Ok(*value),
        None => Ok(default),
        _ => Err(MountError::Properties),
    };
    let enabled = !boolean("disabled", false)?;
    let revision = binding.revision;
    Ok(match kind {
        NodeKind::Container => NodeContent::Container,
        NodeKind::Text => NodeContent::Text(string("text", false)?),
        NodeKind::Button => NodeContent::Button {
            label: string("label", false)?,
            enabled,
        },
        NodeKind::Input => NodeContent::Input {
            value: binding.preview.clone().unwrap_or(string("value", false)?),
            placeholder: string("placeholder", true)?,
            enabled,
            password: false,
            revision,
        },
        NodeKind::Textarea => NodeContent::Textarea {
            value: binding.preview.clone().unwrap_or(string("value", false)?),
            placeholder: string("placeholder", true)?,
            enabled,
            revision,
        },
        NodeKind::Checkbox => NodeContent::Checkbox {
            label: string("label", false)?,
            checked: boolean("checked", false)?,
            enabled,
            revision,
        },
        NodeKind::Switch => NodeContent::Switch {
            label: string("label", false)?,
            checked: boolean("checked", false)?,
            enabled,
            revision,
        },
        NodeKind::Select => {
            let Some(Value::Array(options)) = property(&element.props, "options") else {
                return Err(MountError::Properties);
            };
            let options = options
                .iter()
                .map(|option| {
                    let Some(Value::String(value)) = property(option, "value") else {
                        return Err(MountError::Properties);
                    };
                    let Some(Value::String(label)) = property(option, "label") else {
                        return Err(MountError::Properties);
                    };
                    let Some(Value::Bool(enabled)) = property(option, "enabled") else {
                        return Err(MountError::Properties);
                    };
                    Ok(SelectOption {
                        value: value.clone(),
                        label: label.clone(),
                        enabled: *enabled,
                    })
                })
                .collect::<Result<_, _>>()?;
            NodeContent::Select {
                options,
                value: string("value", false)?,
                placeholder: string("placeholder", true)?,
                enabled,
                revision,
            }
        }
        NodeKind::Scroll => NodeContent::Scroll,
        NodeKind::Image => NodeContent::Image {
            source: string("src", false)?,
            alt: string("alt", true)?,
        },
        NodeKind::Dialog => NodeContent::Dialog {
            open: boolean("open", false)?,
            close_on_overlay_click: boolean("close_on_overlay_click", false)?,
            revision,
        },
        NodeKind::Popover => {
            let placement = match property(&element.props, "placement") {
                None => PopoverPlacement::default(),
                Some(Value::String(value)) => {
                    serde_json::from_value(serde_json::Value::String(value.clone()))
                        .map_err(|_| MountError::Properties)?
                }
                _ => return Err(MountError::Properties),
            };
            let gap = match property(&element.props, "gap") {
                None => 4,
                Some(Value::Number(value))
                    if value.is_finite()
                        && value.fract() == 0.0
                        && (0.0..=1024.0).contains(value) =>
                {
                    *value as u16
                }
                _ => return Err(MountError::Properties),
            };
            NodeContent::Popover {
                anchor: binding.id,
                placement,
                gap,
                open: boolean("open", false)?,
                close_on_outside_click: boolean("close_on_outside_click", true)?,
                revision,
            }
        }
    })
}
