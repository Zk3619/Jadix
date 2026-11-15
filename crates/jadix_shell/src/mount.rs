//! Render-independent keyed shell projection and event bindings.
use crate::handles::Handle;
use crate::{
    handles::EventDelivery,
    schema::Value,
    wire::{Command, ComponentRegistry, ElementSpec, Envelope},
    Session, ShellError,
};
use jadix_core::presentation::{capabilities::NodeKind, *};
#[cfg(all(feature = "native-vm", not(target_arch = "wasm32")))]
use std::collections::HashSet;
use std::collections::{BTreeMap, HashMap};
#[derive(Debug)]
pub enum MountError {
    Session(ShellError),
    Properties,
    Unsupported(String),
    Revision,
}
impl std::fmt::Display for MountError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for MountError {}
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

impl Binding {
    fn update_properties(&mut self, kind: NodeKind, props: &Value) -> Result<(), MountError> {
        let state = match kind {
            NodeKind::Input | NodeKind::Textarea | NodeKind::Select => property(props, "value"),
            NodeKind::Checkbox | NodeKind::Switch => property(props, "checked"),
            NodeKind::Dialog | NodeKind::Popover => property(props, "open"),
            _ => None,
        };
        let old_state = match kind {
            NodeKind::Input | NodeKind::Textarea | NodeKind::Select => {
                property(&self.properties, "value")
            }
            NodeKind::Checkbox | NodeKind::Switch => property(&self.properties, "checked"),
            NodeKind::Dialog | NodeKind::Popover => property(&self.properties, "open"),
            _ => None,
        };
        let preserve_preview = self.preview.is_some() && state == old_state;
        if !preserve_preview
            && state.is_some()
            && (self.observed.is_some() || state != old_state)
            && self.observed.as_ref() != state
        {
            self.revision = self.revision.checked_add(1).ok_or(MountError::Revision)?;
        }
        if !preserve_preview {
            self.preview = None;
        }
        self.observed = None;
        self.properties = props.clone();
        Ok(())
    }
}

pub struct Mount {
    lifetime: RootLifetime,
    owner: NodeId,
    revision: u64,
    bindings: HashMap<Vec<Slot>, Binding>,
    positions: HashMap<Vec<usize>, NodeId>,
    by_node: HashMap<NodeId, Vec<Slot>>,
}
#[derive(Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LeafPatch {
    pub path: Vec<usize>,
    pub props: Value,
}
#[cfg(all(feature = "native-vm", not(target_arch = "wasm32")))]
pub(crate) struct PreparedLeafPatch {
    pub path: Vec<usize>,
    pub content: NodeContent,
    binding: Binding,
}
/// Prepared bindings become live only after the host successfully publishes.
pub struct PreparedMount {
    pub roots: Vec<PresentationNode>,
    bindings: HashMap<Vec<Slot>, Binding>,
    positions: HashMap<Vec<usize>, NodeId>,
    previous: u64,
    owner: NodeId,
}
impl Mount {
    pub fn new(lifetime: RootLifetime) -> Self {
        Self {
            owner: NodeId::new(lifetime.id()),
            lifetime,
            revision: 0,
            bindings: HashMap::new(),
            positions: HashMap::new(),
            by_node: HashMap::new(),
        }
    }
    pub fn lifetime(&self) -> &RootLifetime {
        &self.lifetime
    }
    pub fn node_at(&self, path: &[usize]) -> Option<NodeId> {
        if self.lifetime.is_disposed() {
            None
        } else {
            self.positions.get(path).copied()
        }
    }
    /// Full snapshot projection is O(N); incremental operations are a separate path.
    pub fn prepare(
        &self,
        session: &Session,
        registry: &ComponentRegistry,
        envelope: &Envelope,
    ) -> Result<PreparedMount, MountError> {
        if self.lifetime.is_disposed() {
            return Err(MountError::Session(ShellError::UnknownMount));
        }
        session
            .validate_mount(envelope, registry)
            .map_err(MountError::Session)?;
        let Command::Mount { root, tree } = &envelope.command else {
            return Err(MountError::Properties);
        };
        if *root != self.lifetime.id().token() {
            return Err(MountError::Session(ShellError::UnknownMount));
        }
        let tree = tree.as_ref();
        let mut bindings = HashMap::new();
        let mut positions = HashMap::new();
        let mut anchors = BTreeMap::<String, Vec<NodeId>>::new();
        let mut popovers = BTreeMap::<NodeId, String>::new();
        enum Work<'a> {
            Visit(&'a ElementSpec, Vec<Slot>, Vec<usize>),
            Finish,
        }
        struct Frame {
            node: PresentationNode,
            child_count: usize,
        }
        let mut pending = vec![Work::Visit(
            tree,
            vec![slot(tree, 0), Slot::Component(tree.component.clone())],
            Vec::new(),
        )];
        let mut completed = Vec::new();
        let mut frames = Vec::new();
        while let Some(work) = pending.pop() {
            match work {
                Work::Visit(element, path, position) => {
                    let previous = self
                        .bindings
                        .get(&path)
                        .filter(|binding| binding.component == element.component);
                    let mut binding = previous.cloned().unwrap_or_else(|| Binding {
                        id: NodeId::new(self.lifetime.id()),
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
                    // New bindings already hold these props with revision zero.
                    // Only retained bindings reconcile controlled/preview state.
                    if previous.is_some() {
                        binding.update_properties(kind, &element.props)?;
                    }
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
                    let node = PresentationNode {
                        id: binding.id,
                        descriptor: NodeDescriptor {
                            content,
                            frame: Rect::default(),
                            accessible_role: None,
                            accessible_selected: None,
                            accessible_expanded: None,
                            accessibility_relations: Default::default(),
                            accessible_description: None,
                            accessible_name: None,
                            style: element.style.clone(),
                        },
                        children: Vec::new(),
                    };
                    bindings.insert(path.clone(), binding);
                    frames.push(Frame {
                        node,
                        child_count: element.children.len(),
                    });
                    pending.push(Work::Finish);
                    for (index, child) in element.children.iter().enumerate().rev() {
                        let mut child_path = path.clone();
                        child_path.push(slot(child, index));
                        child_path.push(Slot::Component(child.component.clone()));
                        let mut child_position = position.clone();
                        child_position.push(index);
                        pending.push(Work::Visit(child, child_path, child_position));
                    }
                }
                Work::Finish => {
                    let mut frame = frames.pop().ok_or(MountError::Properties)?;
                    let start = completed
                        .len()
                        .checked_sub(frame.child_count)
                        .ok_or(MountError::Properties)?;
                    frame.node.children = completed.split_off(start);
                    completed.push(frame.node);
                }
            }
        }
        if completed.len() != 1 {
            return Err(MountError::Properties);
        }
        let mut tree = [completed.pop().ok_or(MountError::Properties)?];
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
        Ok(PreparedMount {
            roots: tree.into(),
            bindings,
            positions,
            previous: self.revision,
            owner: self.owner,
        })
    }

    pub fn accept(&mut self, candidate: PreparedMount) -> Result<(), MountError> {
        if self.lifetime.is_disposed()
            || candidate.owner != self.owner
            || candidate.previous != self.revision
        {
            return Err(MountError::Revision);
        }
        self.revision = self.revision.checked_add(1).ok_or(MountError::Revision)?;
        self.bindings = candidate.bindings;
        self.positions = candidate.positions;
        self.by_node = self
            .bindings
            .iter()
            .map(|(p, b)| (b.id, p.clone()))
            .collect();
        Ok(())
    }
    /// Validate only addressed, already mounted Text/Button/Input/Textarea leaves. Structure,
    /// style, key and event handles cannot change through this protocol.
    #[cfg(all(feature = "native-vm", not(target_arch = "wasm32")))]
    pub(crate) fn prepare_leaves(
        &self,
        session: &Session,
        registry: &ComponentRegistry,
        patches: &[LeafPatch],
    ) -> Result<Vec<PreparedLeafPatch>, MountError> {
        if patches.is_empty() || patches.len() > 1024 || self.lifetime.is_disposed() {
            return Err(MountError::Properties);
        }
        let mut seen = HashSet::with_capacity(patches.len());
        patches
            .iter()
            .map(|patch| {
                if patch.path.len() > 32 || !seen.insert(&patch.path) {
                    return Err(MountError::Properties);
                }
                let id = self
                    .positions
                    .get(&patch.path)
                    .ok_or(MountError::Properties)?;
                let path = self.by_node.get(id).ok_or(MountError::Properties)?;
                let mut binding = self
                    .bindings
                    .get(path)
                    .ok_or(MountError::Properties)?
                    .clone();
                let kind = registry
                    .get(&binding.component)
                    .ok_or(MountError::Properties)?
                    .kind;
                if !matches!(
                    kind,
                    NodeKind::Text
                        | NodeKind::Button
                        | NodeKind::Input
                        | NodeKind::Textarea
                        | NodeKind::Checkbox
                        | NodeKind::Switch
                        | NodeKind::Select
                ) {
                    return Err(MountError::Properties);
                }
                let element = ElementSpec {
                    component: binding.component.clone(),
                    key: None,
                    props: patch.props.clone(),
                    style: Default::default(),
                    events: binding.events.clone(),
                    children: vec![],
                };
                session
                    .validate_mount(
                        &Envelope {
                            protocol: 1,
                            session: session.token(),
                            command: Command::Mount {
                                root: self.lifetime.id().token(),
                                tree: Box::new(element.clone()),
                            },
                        },
                        registry,
                    )
                    .map_err(MountError::Session)?;
                binding.update_properties(kind, &patch.props)?;
                Ok(PreparedLeafPatch {
                    path: patch.path.clone(),
                    content: content(&element, kind, &binding)?,
                    binding,
                })
            })
            .collect()
    }
    #[cfg(all(feature = "native-vm", not(target_arch = "wasm32")))]
    pub(crate) fn accept_leaves(&mut self, patches: Vec<PreparedLeafPatch>) {
        for patch in patches {
            let path = self
                .by_node
                .get(&patch.binding.id)
                .expect("prepared live binding");
            *self.bindings.get_mut(path).expect("prepared live binding") = patch.binding;
        }
    }
    pub fn events(
        &mut self,
        session: &Session,
        events: impl IntoIterator<Item = PlatformEvent>,
    ) -> Result<Vec<EventDelivery>, MountError> {
        if self.lifetime.is_disposed() {
            return Err(MountError::Session(ShellError::UnknownMount));
        }
        let mut deliveries = Vec::new();
        for event in events {
            let Some(path) = self.by_node.get(&event.node) else {
                continue;
            };
            let binding = self.bindings.get_mut(path).ok_or(MountError::Properties)?;
            let (name, arguments) = match event.payload {
                EventPayload::Activate => ("activate", serde_json::Value::Null),
                EventPayload::Submit { revision } => {
                    if revision <= binding.revision || binding.preview.is_some() {
                        continue;
                    }
                    binding.revision = revision;
                    (
                        "submit",
                        serde_json::json!({"revision": revision.to_string()}),
                    )
                }
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
                EventPayload::Wheel { delta_x, delta_y } => (
                    "wheel",
                    serde_json::json!({"deltaX":delta_x,"deltaY":delta_y}),
                ),
                EventPayload::Focus
                | EventPayload::FocusVisible
                | EventPayload::Blur
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

    pub fn clear(&mut self) {
        self.bindings.clear();
        self.positions.clear();
        self.by_node.clear();
        self.revision = self.revision.saturating_add(1);
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
    let max_length = match property(&element.props, "maxLength") {
        Some(Value::Number(value))
            if value.is_finite()
                && value.fract() == 0.
                && *value >= 0.
                && *value <= jadix_core::presentation::input::MAX_INPUT_LENGTH as f64 =>
        {
            Some(*value as usize)
        }
        None => None,
        _ => return Err(MountError::Properties),
    };
    let revision = binding.revision;
    Ok(match kind {
        NodeKind::Container => NodeContent::Container,
        NodeKind::Text => NodeContent::Text(string("text", false)?),
        NodeKind::Button => NodeContent::Button {
            label: string("label", false)?,
            enabled,
        },
        NodeKind::Input => NodeContent::Input {
            max_length,
            value: binding.preview.clone().unwrap_or(string("value", false)?),
            placeholder: string("placeholder", true)?,
            enabled,
            password: false,
            revision,
        },
        NodeKind::Textarea => NodeContent::Textarea {
            max_length,
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
/// Translate keyed snapshots to structural operations. Snapshot projection and
/// this diff scan all nodes; only the downstream Taffy synchronization is delta.
/// Reordering siblings uses bounded Vec edits and can be quadratic in siblings.
#[cfg(all(feature = "native-vm", not(target_arch = "wasm32")))]
pub(crate) fn commits(
    root: RootId,
    previous: u64,
    before: &[PresentationNode],
    after: &[PresentationNode],
) -> Result<CommitBatch, MountError> {
    fn flatten<'a>(
        trees: &'a [PresentationNode],
        parent: Option<NodeId>,
        nodes: &mut BTreeMap<NodeId, (Option<NodeId>, &'a NodeDescriptor)>,
        lists: &mut BTreeMap<Option<NodeId>, Vec<NodeId>>,
        order: &mut Vec<NodeId>,
    ) {
        lists.insert(parent, trees.iter().map(|n| n.id).collect());
        for n in trees {
            nodes.insert(n.id, (parent, &n.descriptor));
            order.push(n.id);
            flatten(&n.children, Some(n.id), nodes, lists, order);
        }
    }
    let (mut old, mut current, mut old_order) = (BTreeMap::new(), BTreeMap::new(), vec![]);
    let (mut next, mut desired, mut order) = (BTreeMap::new(), BTreeMap::new(), vec![]);
    flatten(before, None, &mut old, &mut current, &mut old_order);
    flatten(after, None, &mut next, &mut desired, &mut order);
    let positions: BTreeMap<_, _> = desired
        .values()
        .flat_map(|children| children.iter().enumerate().map(|(i, id)| (*id, i)))
        .collect();
    let mut operations = vec![];
    for id in &order {
        if !old.contains_key(id) {
            let (parent, descriptor) = next[id];
            let list = current.entry(parent).or_default();
            let index = positions[id].min(list.len());
            list.insert(index, *id);
            current.entry(Some(*id)).or_default();
            operations.push(Operation::Create {
                node: *id,
                parent,
                index,
                descriptor: descriptor.clone(),
            });
        }
    }
    for id in &order {
        if let Some((parent, _)) = old.get(id) {
            let target = next[id].0;
            if *parent != target {
                current.get_mut(parent).unwrap().retain(|n| n != id);
                current.entry(target).or_default().insert(0, *id);
                operations.push(Operation::Move {
                    node: *id,
                    parent: target,
                    index: 0,
                });
            }
        }
    }
    for id in old_order {
        if !next.contains_key(&id) {
            let parent = old[&id].0;
            if parent.is_none_or(|p| next.contains_key(&p)) {
                current.get_mut(&parent).unwrap().retain(|n| *n != id);
                operations.push(Operation::Remove { node: id });
            }
        }
    }
    for (parent, wanted) in desired {
        let list = current.get_mut(&parent).unwrap();
        if *list == wanted {
            continue;
        }
        for (index, id) in wanted.into_iter().enumerate() {
            if list[index] != id {
                let at = list
                    .iter()
                    .position(|n| *n == id)
                    .ok_or(MountError::Properties)?;
                list.remove(at);
                list.insert(index, id);
                operations.push(Operation::Move {
                    node: id,
                    parent,
                    index,
                });
            }
        }
    }
    for id in order {
        if old.get(&id).is_some_and(|(_, d)| *d != next[&id].1) {
            operations.push(Operation::Update {
                node: id,
                descriptor: next[&id].1.clone(),
            });
        }
    }
    Ok(CommitBatch {
        root,
        previous,
        revision: previous.checked_add(1).ok_or(MountError::Revision)?,
        operations,
    })
}
