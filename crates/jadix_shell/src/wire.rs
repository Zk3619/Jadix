use crate::{handles::Handle, schema::Value};
use jadix_core::presentation::{component::ComponentSchema, style::NodeStyle, PROTOCOL_VERSION};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

pub const MAX_MESSAGE_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WireError {
    InvalidJson,
    Limit,
    Version,
    Component,
    Properties,
    Events,
    Children,
    Style,
}

#[derive(Default)]
pub struct ComponentRegistry {
    components: BTreeMap<String, ComponentSchema>,
}

impl ComponentRegistry {
    /// Complete portable/base schemas shared by every native and script host.
    /// `default()` remains an empty registry for intentionally restricted hosts.
    pub fn standard() -> Self {
        let mut registry = Self::default();
        for source in [include_str!("../../jadix_components/schema/portable-v1.json"),
                       include_str!("../../jadix_components/schema/extended-v1.json")] {
            let schemas: Vec<ComponentSchema> = serde_json::from_str(source).expect("bundled component schemas");
            for schema in schemas { registry.register(schema).expect("unique valid bundled component"); }
        }
        registry
    }

    pub fn get(&self, name: &str) -> Option<&ComponentSchema> {
        self.components.get(name)
    }
    pub fn register(&mut self, schema: ComponentSchema) -> Result<(), WireError> {
        if !schema.is_valid()
            || self.components.contains_key(&schema.name)
            || self.components.len() >= 256
        {
            return Err(WireError::Component);
        }
        self.components.insert(schema.name.clone(), schema);
        Ok(())
    }

    pub fn validate(&self, root: &ElementSpec) -> Result<(), WireError> {
        self.validate_with(root, |_, _| Ok::<_, std::convert::Infallible>(()))
            .map_err(|error| match error {
                ValidationError::Wire(error) => error,
                ValidationError::Visitor(never) => match never {},
            })
    }

    pub(crate) fn validate_with<E>(
        &self,
        root: &ElementSpec,
        mut visit: impl FnMut(&ElementSpec, &ComponentSchema) -> Result<(), E>,
    ) -> Result<(), ValidationError<E>> {
        let mut pending = vec![(root, 0)];
        let mut visited = 0;
        while let Some((node, depth)) = pending.pop() {
            visited += 1;
            if visited > 4096 || depth > 32 || pending.len() > 4096 {
                return Err(WireError::Limit.into());
            }
            let schema = self
                .components
                .get(&node.component)
                .ok_or(WireError::Component)?;
            if !schema.properties.accepts(&node.props) {
                return Err(WireError::Properties.into());
            }
            if !node.style.is_valid() {
                return Err(WireError::Style.into());
            }
            if node
                .events
                .keys()
                .any(|name| !schema.events.contains_key(name))
            {
                return Err(WireError::Events.into());
            }
            if !node.children.is_empty() && schema.kind == jadix_core::presentation::capabilities::NodeKind::Hosted {
                let specification = match &node.props {
                    Value::Object(props) => match props.get("specification") { Some(Value::String(value)) => value, _ => return Err(WireError::Properties.into()) },
                    _ => return Err(WireError::Properties.into()),
                };
                if !jadix_core::hosted::HostedContent::from_json(specification).is_ok_and(|v| v.accepts_children()) {
                    return Err(WireError::Children.into());
                }
            }
            if !schema.children && !node.children.is_empty() {
                return Err(WireError::Children.into());
            }
            let mut keys = HashSet::with_capacity(node.children.len());
            if node
                .key
                .as_ref()
                .is_some_and(|key| key.is_empty() || key.len() > 256)
                || node
                    .children
                    .iter()
                    .filter_map(|child| child.key.as_ref())
                    .any(|key| !keys.insert(key))
            {
                return Err(WireError::Properties.into());
            }
            visit(node, schema).map_err(ValidationError::Visitor)?;
            pending.extend(node.children.iter().map(|child| (child, depth + 1)));
        }
        Ok(())
    }
}

pub(crate) enum ValidationError<E> {
    Wire(WireError),
    Visitor(E),
}

impl<E> From<WireError> for ValidationError<E> {
    fn from(error: WireError) -> Self {
        Self::Wire(error)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ElementSpec {
    pub component: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    pub props: Value,
    #[serde(default)]
    pub style: NodeStyle,
    #[serde(default)]
    pub events: BTreeMap<String, Handle>,
    #[serde(default)]
    pub children: Vec<ElementSpec>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Mount {
        root: String,
        tree: Box<ElementSpec>,
    },
    WriteState {
        handle: Handle,
        value: Value,
    },
    Event {
        handle: Handle,
        arguments: Value,
    },
    Call {
        request: String,
        method: String,
        arguments: Value,
    },
    Cancel {
        request: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub protocol: u32,
    pub session: String,
    pub command: Command,
}

impl Envelope {
    pub fn decode(bytes: &[u8], registry: &ComponentRegistry) -> Result<Self, WireError> {
        if bytes.len() > MAX_MESSAGE_BYTES {
            return Err(WireError::Limit);
        }
        let value: Value = serde_json::from_slice(bytes).map_err(|_| WireError::InvalidJson)?;
        if !value.is_valid() {
            return Err(WireError::Limit);
        }
        let envelope: Self = serde_json::from_slice(bytes).map_err(|_| WireError::InvalidJson)?;
        envelope.validate(registry)?;
        Ok(envelope)
    }

    pub fn encode(&self, registry: &ComponentRegistry) -> Result<Vec<u8>, WireError> {
        self.validate(registry)?;
        let bytes = serde_json::to_vec(self).map_err(|_| WireError::InvalidJson)?;
        if bytes.len() > MAX_MESSAGE_BYTES {
            return Err(WireError::Limit);
        }
        Ok(bytes)
    }

    fn validate(&self, registry: &ComponentRegistry) -> Result<(), WireError> {
        if self.protocol != PROTOCOL_VERSION {
            return Err(WireError::Version);
        }
        if !token(&self.session) {
            return Err(WireError::Properties);
        }
        match &self.command {
            Command::Mount { root, tree } => {
                if !token(root) {
                    return Err(WireError::Properties);
                }
                registry.validate(tree)?;
                let mut pending = vec![tree.as_ref()];
                while let Some(node) = pending.pop() {
                    if node
                        .events
                        .values()
                        .any(|handle| handle.session != self.session || !token(&handle.sequence))
                    {
                        return Err(WireError::Events);
                    }
                    pending.extend(node.children.iter());
                }
                Ok(())
            }
            Command::WriteState { handle, value }
            | Command::Event {
                handle,
                arguments: value,
            } => {
                if handle.session != self.session || !token(&handle.sequence) || !value.is_valid() {
                    Err(WireError::Properties)
                } else {
                    Ok(())
                }
            }
            Command::Call {
                request,
                method,
                arguments,
            } => {
                if !token(request)
                    || method.is_empty()
                    || method.len() > 128
                    || !arguments.is_valid()
                {
                    Err(WireError::Properties)
                } else {
                    Ok(())
                }
            }
            Command::Cancel { request } => {
                if token(request) {
                    Ok(())
                } else {
                    Err(WireError::Properties)
                }
            }
        }
    }
}

fn token(value: &str) -> bool {
    value
        .parse::<u64>()
        .is_ok_and(|parsed| parsed > 0 && parsed.to_string() == value)
}
